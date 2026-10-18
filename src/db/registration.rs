//! Registration mode and the single-use invitation / password-reset tokens.
//!
//! A token is redeemed by one statement that sets `used_at` where the token is still unused,
//! unrevoked and unexpired. It runs in the same transaction as the work it authorises (creating
//! the user, changing the password), so a failure after it rolls the token back to unused, and two
//! simultaneous redemptions cannot both succeed: the second waits for the first, then finds
//! `used_at` set.

use chrono::{DateTime, Utc};
use tokio_postgres::error::SqlState;
use uuid::Uuid;

use super::DatabaseService;
use crate::models::{InviteInfo, RegistrationMode, User};

type BoxError = Box<dyn std::error::Error>;

/// The setting that records who may register
const MODE_KEY: &str = "registration_mode";
/// Present (as `show`) after an upgrade made registration invite-only, until an admin has seen it
const NOTICE_KEY: &str = "registration_upgrade_notice";

/// A token is usable while it has not been used, revoked or expired.
const USABLE: &str = "used_at IS NULL AND revoked_at IS NULL AND expires_at > NOW()";

/// What happened when someone tried to register with an invitation
#[derive(Debug)]
pub enum InviteRegistration {
    /// The account exists and the invitation is spent
    Registered { user: User, token_id: Uuid },
    /// Unknown, used, revoked, expired or not an invitation
    InviteInvalid,
    /// The username belongs to someone else; the invitation is still unused
    UsernameTaken,
}

/// What happened when someone tried to set a password with a reset link
#[derive(Debug)]
pub enum PasswordReset {
    Done {
        user_id: Uuid,
        token_id: Uuid,
        /// The new session generation: every earlier session is signed out
        token_version: i32,
    },
    /// Unknown, used, revoked, expired or not a reset link
    LinkInvalid,
}

fn is_violation(error: &tokio_postgres::Error, state: &SqlState) -> bool {
    error.as_db_error().is_some_and(|db| db.code() == state)
}

impl DatabaseService {
    // ---------------------------------------------------------------- settings

    /// Who may register. A missing or unreadable value means invite-only, never open.
    pub async fn get_registration_mode(&self) -> Result<RegistrationMode, BoxError> {
        let client = self.pool.get().await?;
        let row = client
            .query_opt(
                "SELECT value FROM app_settings WHERE key = $1",
                &[&MODE_KEY],
            )
            .await?;
        Ok(row
            .and_then(|r| RegistrationMode::parse(&r.get::<_, String>(0)))
            .unwrap_or(RegistrationMode::Invite))
    }

    /// Choose who may register. Choosing is an answer to the upgrade notice, so it clears it.
    pub async fn set_registration_mode(&self, mode: RegistrationMode) -> Result<(), BoxError> {
        let mut client = self.pool.get().await?;
        let tx = client.transaction().await?;
        tx.execute(
            "INSERT INTO app_settings (key, value) VALUES ($1, $2)
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
            &[&MODE_KEY, &mode.as_str()],
        )
        .await?;
        tx.execute(
            "UPDATE app_settings SET value = 'dismissed', updated_at = NOW()
             WHERE key = $1 AND value = 'show'",
            &[&NOTICE_KEY],
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Whether admins should still be told that registration became invite-only.
    pub async fn registration_upgrade_notice_pending(&self) -> Result<bool, BoxError> {
        let client = self.pool.get().await?;
        let row = client
            .query_opt(
                "SELECT value = 'show' FROM app_settings WHERE key = $1",
                &[&NOTICE_KEY],
            )
            .await?;
        Ok(row.is_some_and(|r| r.get(0)))
    }

    pub async fn dismiss_registration_upgrade_notice(&self) -> Result<(), BoxError> {
        let client = self.pool.get().await?;
        client
            .execute(
                "UPDATE app_settings SET value = 'dismissed', updated_at = NOW()
                 WHERE key = $1 AND value = 'show'",
                &[&NOTICE_KEY],
            )
            .await?;
        Ok(())
    }

    // ---------------------------------------------------------------- invitations

    /// Store a new invitation (by hash) valid for `hours` hours. Returns its id and expiry.
    pub async fn create_invite(
        &self,
        created_by: Uuid,
        token_hash: &str,
        hours: i32,
    ) -> Result<(Uuid, DateTime<Utc>), BoxError> {
        let client = self.pool.get().await?;
        let row = client
            .query_one(
                "INSERT INTO auth_tokens (purpose, token_hash, created_by, expires_at)
                 VALUES ('invite', $1, $2, NOW() + $3::int * INTERVAL '1 hour')
                 RETURNING id, expires_at",
                &[&token_hash, &created_by, &hours],
            )
            .await?;
        Ok((row.get(0), row.get(1)))
    }

    /// Invitations that can still be used, newest first.
    pub async fn list_open_invites(&self) -> Result<Vec<InviteInfo>, BoxError> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT t.id, t.created_at, t.expires_at, u.username
                 FROM auth_tokens t LEFT JOIN users u ON u.id = t.created_by
                 WHERE t.purpose = 'invite'
                   AND t.used_at IS NULL AND t.revoked_at IS NULL AND t.expires_at > NOW()
                 ORDER BY t.created_at DESC",
                &[],
            )
            .await?;
        Ok(rows
            .iter()
            .map(|r| InviteInfo {
                id: r.get(0),
                created_at: r.get(1),
                expires_at: r.get(2),
                created_by_username: r.get(3),
            })
            .collect())
    }

    /// Revoke an unused invitation. `false` if there is no such outstanding invitation.
    pub async fn revoke_invite(&self, id: Uuid) -> Result<bool, BoxError> {
        let client = self.pool.get().await?;
        let rows = client
            .execute(
                "UPDATE auth_tokens SET revoked_at = NOW()
                 WHERE id = $1 AND purpose = 'invite' AND used_at IS NULL AND revoked_at IS NULL",
                &[&id],
            )
            .await?;
        Ok(rows > 0)
    }

    /// Whether an invitation could be redeemed right now (not the mode; the caller checks that).
    pub async fn invite_is_redeemable(&self, token_hash: &str) -> Result<bool, BoxError> {
        self.token_is_redeemable("invite", token_hash).await
    }

    /// Redeem an invitation by creating an ordinary, active, non-admin user. All or nothing: if
    /// the user cannot be created, the invitation stays unused.
    pub async fn register_with_invite(
        &self,
        token_hash: &str,
        username: &str,
        full_name: &str,
        password_hash: &str,
    ) -> Result<InviteRegistration, BoxError> {
        let mut client = self.pool.get().await?;
        let tx = client.transaction().await?;

        let token = tx
            .query_opt(
                &format!(
                    "UPDATE auth_tokens SET used_at = NOW()
                     WHERE token_hash = $1 AND purpose = 'invite' AND {USABLE}
                     RETURNING id"
                ),
                &[&token_hash],
            )
            .await?;
        let Some(token) = token else {
            return Ok(InviteRegistration::InviteInvalid);
        };
        let token_id: Uuid = token.get(0);

        // The role and state are fixed here: nothing in an invitation can make an admin
        let inserted = tx
            .query_one(
                "INSERT INTO users (username, full_name, password_hash, is_admin, is_active)
                 VALUES ($1, $2, $3, FALSE, TRUE)
                 RETURNING id, username, full_name, password_hash, is_admin, is_active,
                           created_at, updated_at, token_version",
                &[&username, &full_name, &password_hash],
            )
            .await;
        let row = match inserted {
            Ok(row) => row,
            Err(e) if is_violation(&e, &SqlState::UNIQUE_VIOLATION) => {
                return Ok(InviteRegistration::UsernameTaken);
            },
            Err(e) => return Err(e.into()),
        };
        let user = User {
            id: row.get(0),
            username: row.get(1),
            full_name: row.get(2),
            password_hash: row.get(3),
            is_admin: row.get(4),
            is_active: row.get(5),
            created_at: row.get(6),
            updated_at: row.get(7),
            recovery_codes_generated_at: None,
            recovery_codes_confirmed: false,
            token_version: row.get(8),
        };

        tx.execute(
            "INSERT INTO user_settings (user_id) VALUES ($1)",
            &[&user.id],
        )
        .await?;
        tx.execute(
            "UPDATE auth_tokens SET used_by = $1 WHERE id = $2",
            &[&user.id, &token_id],
        )
        .await?;
        tx.commit().await?;
        Ok(InviteRegistration::Registered { user, token_id })
    }

    // ---------------------------------------------------------------- reset links

    /// Store a reset link for `user_id` valid for `hours` hours, replacing any earlier unused
    /// link for that user. `None` if the user does not exist.
    pub async fn issue_reset_token(
        &self,
        user_id: Uuid,
        created_by: Uuid,
        token_hash: &str,
        hours: i32,
    ) -> Result<Option<(Uuid, DateTime<Utc>)>, BoxError> {
        let mut client = self.pool.get().await?;
        let tx = client.transaction().await?;
        tx.execute(
            "UPDATE auth_tokens SET revoked_at = NOW()
             WHERE user_id = $1 AND purpose = 'password_reset'
               AND used_at IS NULL AND revoked_at IS NULL",
            &[&user_id],
        )
        .await?;
        let inserted = tx
            .query_one(
                "INSERT INTO auth_tokens (purpose, token_hash, user_id, created_by, expires_at)
                 VALUES ('password_reset', $1, $2, $3, NOW() + $4::int * INTERVAL '1 hour')
                 RETURNING id, expires_at",
                &[&token_hash, &user_id, &created_by, &hours],
            )
            .await;
        match inserted {
            Ok(row) => {
                tx.commit().await?;
                Ok(Some((row.get(0), row.get(1))))
            },
            // The user was deleted between the caller's check and this insert
            Err(e) if is_violation(&e, &SqlState::FOREIGN_KEY_VIOLATION) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Whether a reset link could be used right now.
    pub async fn reset_token_is_redeemable(&self, token_hash: &str) -> Result<bool, BoxError> {
        self.token_is_redeemable("password_reset", token_hash).await
    }

    /// Redeem a reset link: set the new password, end every session of that user, and void any
    /// other unused link for them. All or nothing.
    pub async fn reset_password_with_token(
        &self,
        token_hash: &str,
        password_hash: &str,
    ) -> Result<PasswordReset, BoxError> {
        let mut client = self.pool.get().await?;
        let tx = client.transaction().await?;

        let token = tx
            .query_opt(
                &format!(
                    "UPDATE auth_tokens SET used_at = NOW(), used_by = user_id
                     WHERE token_hash = $1 AND purpose = 'password_reset' AND {USABLE}
                     RETURNING id, user_id"
                ),
                &[&token_hash],
            )
            .await?;
        let Some(token) = token else {
            return Ok(PasswordReset::LinkInvalid);
        };
        let token_id: Uuid = token.get(0);
        let user_id: Uuid = token.get(1);

        let updated = tx
            .query_opt(
                "UPDATE users
                 SET password_hash = $1, token_version = token_version + 1, updated_at = NOW()
                 WHERE id = $2
                 RETURNING token_version",
                &[&password_hash, &user_id],
            )
            .await?;
        let Some(updated) = updated else {
            return Ok(PasswordReset::LinkInvalid);
        };

        tx.execute(
            "UPDATE auth_tokens SET revoked_at = NOW()
             WHERE user_id = $1 AND purpose = 'password_reset'
               AND used_at IS NULL AND revoked_at IS NULL",
            &[&user_id],
        )
        .await?;
        tx.commit().await?;
        Ok(PasswordReset::Done {
            user_id,
            token_id,
            token_version: updated.get(0),
        })
    }

    async fn token_is_redeemable(&self, purpose: &str, token_hash: &str) -> Result<bool, BoxError> {
        let client = self.pool.get().await?;
        let row = client
            .query_one(
                &format!(
                    "SELECT EXISTS (SELECT 1 FROM auth_tokens
                                    WHERE token_hash = $1 AND purpose = $2 AND {USABLE})"
                ),
                &[&token_hash, &purpose],
            )
            .await?;
        Ok(row.get(0))
    }
}
