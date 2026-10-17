//! In-memory failed-attempt throttling for password-style checks.
//!
//! Two independent counters are kept, both with bounded memory:
//! - per (username, client IP): [`PAIR_MAX_FAILURES`] failures inside [`PAIR_WINDOW`] lock that
//!   pair for [`PAIR_LOCKOUT`]. An attacker therefore cannot lock the real owner out from the
//!   owner's own address.
//! - per username across all addresses: after [`USER_FREE_FAILURES`] failures the account is
//!   not locked but gets an exponentially growing delay, capped at [`USER_BACKOFF_CAP`], which
//!   slows distributed guessing without turning into a denial of service for the owner.
//!
//! State is process-local and lost on restart, which is acceptable for a single-instance
//! self-hosted deployment. Unknown usernames are counted exactly like real ones.
//!
//! The mutex is only held inside the synchronous methods below, never across an `.await`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Failures per (username, IP) pair that trigger a lockout.
pub const PAIR_MAX_FAILURES: u32 = 5;
/// Window in which pair failures are counted.
pub const PAIR_WINDOW: Duration = Duration::from_secs(15 * 60);
/// How long a locked pair is rejected.
pub const PAIR_LOCKOUT: Duration = Duration::from_secs(15 * 60);
/// Failures per username (any IP) tolerated before backoff starts.
pub const USER_FREE_FAILURES: u32 = 50;
/// Window in which per-username failures are counted.
pub const USER_WINDOW: Duration = Duration::from_secs(15 * 60);
/// Longest delay the per-username backoff can impose.
pub const USER_BACKOFF_CAP: Duration = Duration::from_secs(60);
/// Maximum entries kept per table; see [`Table::make_room`] for the eviction rules.
pub const MAX_TRACKED_KEYS: usize = 10_000;
/// Usernames are truncated to this many characters when used as a key.
const MAX_USERNAME_KEY_CHARS: usize = 100;

/// Normalise a username for use as a throttle key (trim + lowercase, bounded length).
#[must_use]
pub fn normalize_username(username: &str) -> String {
    username
        .trim()
        .to_lowercase()
        .chars()
        .take(MAX_USERNAME_KEY_CHARS)
        .collect()
}

/// What is being throttled. Usernames and user ids live in separate key namespaces, so input
/// typed into a login form can never collide with the key of an authenticated user.
#[derive(Debug, Clone, Copy)]
pub enum Subject<'a> {
    /// An account name typed by a client (may not exist).
    Username(&'a str),
    /// An authenticated user, identified by id.
    UserId(Uuid),
}

impl Subject<'_> {
    fn key(&self) -> String {
        match self {
            Subject::Username(name) => format!("n\u{1f}{}", normalize_username(name)),
            Subject::UserId(id) => format!("i\u{1f}{id}"),
        }
    }
}

struct Entry {
    failures: u32,
    window_start: Instant,
    locked_until: Option<Instant>,
    last_seen: Instant,
}

impl Entry {
    fn is_locked(&self, now: Instant) -> bool {
        self.locked_until.is_some_and(|until| until > now)
    }
}

struct Table {
    entries: HashMap<String, Entry>,
    cap: usize,
    window: Duration,
}

impl Table {
    fn new(cap: usize, window: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            cap,
            window,
        }
    }

    /// Remaining lock time for `key`, if it is currently locked.
    fn retry_after(&self, key: &str, now: Instant) -> Option<Duration> {
        let until = self.entries.get(key)?.locked_until?;
        until.checked_duration_since(now).filter(|d| !d.is_zero())
    }

    /// Record one failure and let `policy(failures)` decide whether it locks the key.
    fn record_failure(
        &mut self,
        key: &str,
        now: Instant,
        policy: impl Fn(u32) -> Option<Duration>,
    ) {
        if !self.entries.contains_key(key) {
            self.make_room(now);
        }
        let window = self.window;
        let entry = self.entries.entry(key.to_string()).or_insert(Entry {
            failures: 0,
            window_start: now,
            locked_until: None,
            last_seen: now,
        });
        if !entry.is_locked(now) && now.duration_since(entry.window_start) >= window {
            entry.failures = 0;
            entry.window_start = now;
            entry.locked_until = None;
        }
        entry.failures = entry.failures.saturating_add(1);
        entry.last_seen = now;
        if let Some(lock) = policy(entry.failures) {
            entry.locked_until = Some(now + lock);
        }
    }

    fn clear(&mut self, key: &str) {
        self.entries.remove(key);
    }

    /// Keep the table below `cap` before a new key is inserted.
    ///
    /// Expired entries go first. If the table is still full, the oldest ~10% (by last use) are
    /// dropped, unlocked entries before locked ones, so active lockouts survive a flood of new
    /// keys. Evicting an unlocked entry forgets its failure count; that fails open for that one
    /// key, which is the accepted price for bounded memory.
    fn make_room(&mut self, now: Instant) {
        if self.entries.len() < self.cap {
            return;
        }
        let window = self.window;
        self.entries
            .retain(|_, e| e.is_locked(now) || now.duration_since(e.window_start) < window);
        if self.entries.len() < self.cap {
            return;
        }
        let mut order: Vec<(bool, Instant, String)> = self
            .entries
            .iter()
            .map(|(k, e)| (e.is_locked(now), e.last_seen, k.clone()))
            .collect();
        order.sort();
        let evict = (self.cap / 10).max(1);
        for (_, _, key) in order.into_iter().take(evict) {
            self.entries.remove(&key);
        }
    }
}

struct Inner {
    pairs: Table,
    users: Table,
}

/// Failed-attempt throttle keyed by (username, client IP) and by username alone.
pub struct AuthThrottle {
    inner: Mutex<Inner>,
}

impl Default for AuthThrottle {
    fn default() -> Self {
        Self::with_capacity(MAX_TRACKED_KEYS)
    }
}

impl AuthThrottle {
    /// A throttle that tracks at most `cap` keys per table.
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                pairs: Table::new(cap, PAIR_WINDOW),
                users: Table::new(cap, USER_WINDOW),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn pair_key(subject: Subject<'_>, ip: &str) -> String {
        format!("{}\u{1f}{ip}", subject.key())
    }

    /// Reject the attempt up front if this username/IP is currently locked out.
    /// `Err` carries how long the caller should wait.
    pub fn check(&self, subject: Subject<'_>, ip: &str) -> Result<(), Duration> {
        self.check_at(subject, ip, Instant::now())
    }

    /// Count a failed attempt (wrong password, unknown user, bad code).
    pub fn record_failure(&self, subject: Subject<'_>, ip: &str) {
        self.record_failure_at(subject, ip, Instant::now());
    }

    /// Forget the counters after a successful attempt.
    pub fn record_success(&self, subject: Subject<'_>, ip: &str) {
        let mut inner = self.lock();
        inner.pairs.clear(&Self::pair_key(subject, ip));
        inner.users.clear(&subject.key());
    }

    fn check_at(&self, subject: Subject<'_>, ip: &str, now: Instant) -> Result<(), Duration> {
        let inner = self.lock();
        let pair = inner.pairs.retry_after(&Self::pair_key(subject, ip), now);
        let user = inner.users.retry_after(&subject.key(), now);
        match pair.max(user) {
            Some(wait) => Err(wait),
            None => Ok(()),
        }
    }

    fn record_failure_at(&self, subject: Subject<'_>, ip: &str, now: Instant) {
        let mut inner = self.lock();
        inner
            .pairs
            .record_failure(&Self::pair_key(subject, ip), now, |failures| {
                (failures >= PAIR_MAX_FAILURES).then_some(PAIR_LOCKOUT)
            });
        inner
            .users
            .record_failure(&subject.key(), now, user_backoff);
    }
}

/// Delay applied to a username after `failures` failures in the current window.
fn user_backoff(failures: u32) -> Option<Duration> {
    let over = failures.checked_sub(USER_FREE_FAILURES)?;
    if over == 0 {
        return None;
    }
    // 1s, 2s, 4s, ... up to the cap; the shift is bounded so it cannot overflow.
    let secs = 1u64 << (over - 1).min(16);
    Some(Duration::from_secs(secs).min(USER_BACKOFF_CAP))
}

/// Process-wide throttle shared by every worker.
#[must_use]
pub fn global() -> &'static AuthThrottle {
    static THROTTLE: OnceLock<AuthThrottle> = OnceLock::new();
    THROTTLE.get_or_init(AuthThrottle::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_is_trimmed_and_lowercased() {
        assert_eq!(normalize_username("  Alice "), "alice");
        assert_eq!(normalize_username("ALICE"), normalize_username("alice"));
        assert_eq!(normalize_username(&"x".repeat(500)).chars().count(), 100);
    }

    #[test]
    fn pair_locks_after_five_failures_and_reports_retry_after() {
        let t = AuthThrottle::default();
        let now = Instant::now();
        for _ in 0..PAIR_MAX_FAILURES - 1 {
            t.record_failure_at(Subject::Username("alice"), "1.2.3.4", now);
            assert!(t
                .check_at(Subject::Username("alice"), "1.2.3.4", now)
                .is_ok());
        }
        t.record_failure_at(Subject::Username("alice"), "1.2.3.4", now);
        let wait = t
            .check_at(Subject::Username("alice"), "1.2.3.4", now)
            .unwrap_err();
        assert_eq!(wait, PAIR_LOCKOUT);
        // Still locked a bit later, unlocked after the lockout
        assert!(t
            .check_at(
                Subject::Username("alice"),
                "1.2.3.4",
                now + Duration::from_secs(60)
            )
            .is_err());
        assert!(t
            .check_at(Subject::Username("alice"), "1.2.3.4", now + PAIR_LOCKOUT)
            .is_ok());
    }

    #[test]
    fn lockout_is_per_ip_so_the_owner_is_not_locked_out_by_an_attacker() {
        let t = AuthThrottle::default();
        let now = Instant::now();
        for _ in 0..PAIR_MAX_FAILURES {
            t.record_failure_at(Subject::Username("alice"), "6.6.6.6", now);
        }
        assert!(t
            .check_at(Subject::Username("alice"), "6.6.6.6", now)
            .is_err());
        assert!(t
            .check_at(Subject::Username("alice"), "10.0.0.1", now)
            .is_ok());
    }

    #[test]
    fn keys_are_normalised_so_case_and_whitespace_do_not_evade_the_limit() {
        let t = AuthThrottle::default();
        let now = Instant::now();
        for name in ["alice", "ALICE", " Alice ", "aLiCe", "alice"] {
            t.record_failure_at(Subject::Username(name), "1.2.3.4", now);
        }
        assert!(t
            .check_at(Subject::Username("Alice"), "1.2.3.4", now)
            .is_err());
    }

    #[test]
    fn success_clears_counters() {
        let t = AuthThrottle::default();
        let now = Instant::now();
        for _ in 0..PAIR_MAX_FAILURES - 1 {
            t.record_failure_at(Subject::Username("alice"), "1.2.3.4", now);
        }
        t.record_success(Subject::Username("alice"), "1.2.3.4");
        // A fresh run of failures starts from zero again
        for _ in 0..PAIR_MAX_FAILURES - 1 {
            t.record_failure_at(Subject::Username("alice"), "1.2.3.4", now);
        }
        assert!(t
            .check_at(Subject::Username("alice"), "1.2.3.4", now)
            .is_ok());
    }

    #[test]
    fn failures_outside_the_window_do_not_accumulate() {
        let t = AuthThrottle::default();
        let start = Instant::now();
        for _ in 0..PAIR_MAX_FAILURES - 1 {
            t.record_failure_at(Subject::Username("alice"), "1.2.3.4", start);
        }
        // One more failure, but after the window has passed: counted as the first
        let later = start + PAIR_WINDOW + Duration::from_secs(1);
        t.record_failure_at(Subject::Username("alice"), "1.2.3.4", later);
        assert!(t
            .check_at(Subject::Username("alice"), "1.2.3.4", later)
            .is_ok());
    }

    #[test]
    fn user_backoff_starts_after_threshold_doubles_and_is_capped() {
        assert_eq!(user_backoff(1), None);
        assert_eq!(user_backoff(USER_FREE_FAILURES), None);
        assert_eq!(
            user_backoff(USER_FREE_FAILURES + 1),
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            user_backoff(USER_FREE_FAILURES + 2),
            Some(Duration::from_secs(2))
        );
        assert_eq!(
            user_backoff(USER_FREE_FAILURES + 4),
            Some(Duration::from_secs(8))
        );
        assert_eq!(
            user_backoff(USER_FREE_FAILURES + 100),
            Some(USER_BACKOFF_CAP)
        );
        assert_eq!(user_backoff(u32::MAX), Some(USER_BACKOFF_CAP));
    }

    #[test]
    fn distributed_guessing_gets_a_delay_but_never_a_hard_lock() {
        let t = AuthThrottle::default();
        let now = Instant::now();
        // 60 failures for one username from 60 different addresses: no pair ever locks
        for i in 0..60 {
            t.record_failure_at(Subject::Username("victim"), &format!("10.0.0.{i}"), now);
        }
        // Every address is now delayed (the per-username backoff) ...
        let wait = t
            .check_at(Subject::Username("victim"), "192.168.1.1", now)
            .unwrap_err();
        assert!(wait <= USER_BACKOFF_CAP);
        // ... but the delay is short and ends by itself
        assert!(t
            .check_at(
                Subject::Username("victim"),
                "192.168.1.1",
                now + USER_BACKOFF_CAP
            )
            .is_ok());
    }

    #[test]
    fn table_is_capped_and_prefers_evicting_unlocked_oldest_entries() {
        let t = AuthThrottle::with_capacity(20);
        let now = Instant::now();
        // An active lockout that must survive the flood
        for _ in 0..PAIR_MAX_FAILURES {
            t.record_failure_at(Subject::Username("locked-user"), "6.6.6.6", now);
        }
        // Spray many distinct usernames
        for i in 0..500u64 {
            t.record_failure_at(
                Subject::Username(&format!("random-{i}")),
                "7.7.7.7",
                now + Duration::from_millis(i + 1),
            );
        }
        let inner = t.lock();
        assert!(inner.pairs.entries.len() <= 20, "pairs table exceeded cap");
        assert!(inner.users.entries.len() <= 20, "users table exceeded cap");
        drop(inner);
        assert!(
            t.check_at(
                Subject::Username("locked-user"),
                "6.6.6.6",
                now + Duration::from_secs(1)
            )
            .is_err(),
            "an active lockout must not be evicted by spraying new keys"
        );
    }

    #[test]
    fn expired_entries_are_pruned_before_evicting_live_ones() {
        let t = AuthThrottle::with_capacity(10);
        let start = Instant::now();
        for i in 0..10 {
            t.record_failure_at(Subject::Username(&format!("old-{i}")), "1.1.1.1", start);
        }
        let later = start + PAIR_WINDOW + Duration::from_secs(1);
        t.record_failure_at(Subject::Username("fresh"), "1.1.1.1", later);
        let inner = t.lock();
        assert_eq!(inner.pairs.entries.len(), 1);
    }

    #[test]
    fn user_ids_and_typed_usernames_never_share_a_counter() {
        let t = AuthThrottle::default();
        let now = Instant::now();
        let id = Uuid::new_v4();
        // Somebody types the victim's id (or a lookalike of the internal key) as a username
        for name in [id.to_string(), format!("i\u{1f}{id}")] {
            for _ in 0..60 {
                t.record_failure_at(Subject::Username(&name), "6.6.6.6", now);
            }
        }
        assert!(t.check_at(Subject::UserId(id), "6.6.6.6", now).is_ok());
        assert!(t.check_at(Subject::UserId(id), "10.0.0.1", now).is_ok());
    }

    #[test]
    fn user_id_subject_locks_like_a_username() {
        let t = AuthThrottle::default();
        let now = Instant::now();
        let id = Uuid::new_v4();
        for _ in 0..PAIR_MAX_FAILURES {
            t.record_failure_at(Subject::UserId(id), "6.6.6.6", now);
        }
        assert!(t.check_at(Subject::UserId(id), "6.6.6.6", now).is_err());
        assert!(t.check_at(Subject::UserId(id), "10.0.0.1", now).is_ok());
    }
}
