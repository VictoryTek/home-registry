// Migration V028 adds the registration mode setting and the invite / password-reset token table.
// Each test builds its own scratch database at the state just before it (V027).

mod common;

use common::ScratchDb;
use uuid::Uuid;

const V028: &str = include_str!("../migrations/V028__registration_mode_and_auth_tokens.sql");

async fn setting(pool: &deadpool_postgres::Pool, key: &str) -> Option<String> {
    pool.get()
        .await
        .unwrap()
        .query_opt("SELECT value FROM app_settings WHERE key = $1", &[&key])
        .await
        .unwrap()
        .map(|row| row.get(0))
}

async fn user(pool: &deadpool_postgres::Pool, name: &str) -> Uuid {
    pool.get()
        .await
        .unwrap()
        .query_one(
            "INSERT INTO users (username, full_name, password_hash, is_admin, is_active)
             VALUES ($1, 'Test', 'hash', FALSE, TRUE) RETURNING id",
            &[&name],
        )
        .await
        .unwrap()
        .get(0)
}

#[actix_web::test]
async fn an_upgrade_becomes_invite_only_and_leaves_a_notice() {
    let mut scratch = ScratchDb::create_up_to(27).await;
    user(&scratch.pool, "existing").await;

    scratch.apply_migration(28).await;

    assert_eq!(
        setting(&scratch.pool, "registration_mode").await.as_deref(),
        Some("invite")
    );
    assert_eq!(
        setting(&scratch.pool, "registration_upgrade_notice")
            .await
            .as_deref(),
        Some("show")
    );
}

#[actix_web::test]
async fn a_fresh_install_is_invite_only_without_a_notice() {
    let mut scratch = ScratchDb::create_up_to(27).await;
    scratch.apply_migration(28).await;

    assert_eq!(
        setting(&scratch.pool, "registration_mode").await.as_deref(),
        Some("invite")
    );
    assert_eq!(
        setting(&scratch.pool, "registration_upgrade_notice").await,
        None
    );
}

#[actix_web::test]
async fn applying_it_twice_changes_nothing() {
    let mut scratch = ScratchDb::create_up_to(27).await;
    user(&scratch.pool, "existing").await;
    scratch.apply_migration(28).await;
    scratch
        .pool
        .get()
        .await
        .unwrap()
        .execute(
            "UPDATE app_settings SET value = 'open' WHERE key = 'registration_mode'",
            &[],
        )
        .await
        .unwrap();

    // Re-running the file (as an idempotent migration may be) must not reset a chosen mode
    scratch
        .pool
        .get()
        .await
        .unwrap()
        .batch_execute(V028)
        .await
        .unwrap();
    assert_eq!(
        setting(&scratch.pool, "registration_mode").await.as_deref(),
        Some("open")
    );
}

#[actix_web::test]
async fn the_tables_enforce_their_rules() {
    let mut scratch = ScratchDb::create_up_to(27).await;
    scratch.apply_migration(28).await;
    let client = scratch.pool.get().await.unwrap();
    let admin = user(&scratch.pool, "admin").await;
    let member = user(&scratch.pool, "member").await;

    // Only the three modes can be stored
    assert!(client
        .execute(
            "UPDATE app_settings SET value = 'anyone' WHERE key = 'registration_mode'",
            &[]
        )
        .await
        .is_err());

    // A reset link needs a user; an invitation must not name one; unknown purposes are refused
    let insert = |purpose: &'static str, user_id: Option<Uuid>, hash: &'static str| {
        let client = &client;
        async move {
            client
                .execute(
                    "INSERT INTO auth_tokens (purpose, token_hash, user_id, created_by, expires_at)
                     VALUES ($1, $2, $3, $4, NOW() + INTERVAL '1 day')",
                    &[&purpose, &hash, &user_id, &admin],
                )
                .await
        }
    };
    assert!(insert("password_reset", None, "h1").await.is_err());
    assert!(insert("invite", Some(member), "h2").await.is_err());
    assert!(insert("something_else", None, "h3").await.is_err());
    insert("invite", None, "h4").await.unwrap();
    insert("password_reset", Some(member), "h5").await.unwrap();
    // The hash is unique
    assert!(insert("invite", None, "h4").await.is_err());

    // Deleting the user kills their reset link; deleting the creator keeps everything else
    client
        .execute("DELETE FROM users WHERE id = $1", &[&member])
        .await
        .unwrap();
    let left: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM auth_tokens WHERE token_hash = 'h5'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(left, 0);
    client
        .execute("DELETE FROM users WHERE id = $1", &[&admin])
        .await
        .unwrap();
    let orphaned: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM auth_tokens WHERE token_hash = 'h4' AND created_by IS NULL",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(orphaned, 1);
}
