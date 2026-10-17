// Creating an item must name its inventory: there is no default inventory (migration V027).

mod common;

use actix_web::{http::StatusCode, test, web, App};
use common::ScratchDb;
use home_registry::api;
use serde_json::{json, Value};
use uuid::Uuid;

/// A user who owns inventory number 1. Before H-10, an item created without an `inventory_id`
/// was filed into exactly that inventory.
struct Setup {
    scratch: ScratchDb,
    token: String,
}

async fn setup_with_inventory_1() -> Setup {
    let scratch = ScratchDb::create().await;
    let username = common::test_username("owner");
    common::create_test_user(&scratch.pool, &username).await;
    let client = scratch.pool.get().await.unwrap();
    let user_id: Uuid = client
        .query_one("SELECT id FROM users WHERE username = $1", &[&username])
        .await
        .unwrap()
        .get(0);
    client
        .execute(
            "INSERT INTO inventories (id, name, user_id) VALUES (1, 'First', $1)",
            &[&user_id],
        )
        .await
        .unwrap();
    drop(client);
    let token = common::get_test_token(&scratch.pool, &username).await;
    Setup { scratch, token }
}

async fn item_count(setup: &Setup) -> i64 {
    setup
        .scratch
        .pool
        .get()
        .await
        .unwrap()
        .query_one("SELECT COUNT(*) FROM items", &[])
        .await
        .unwrap()
        .get(0)
}

macro_rules! app {
    ($setup:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($setup.scratch.pool.clone()))
                .service(web::scope("/api").service(api::create_item)),
        )
        .await
    };
}

/// POST /api/items; yields (status, parsed JSON body or Null).
macro_rules! post_item {
    ($app:expr, $token:expr, $body:expr) => {{
        let mut req = test::TestRequest::post().uri("/api/items").set_json($body);
        let token: Option<&str> = $token;
        if let Some(token) = token {
            req = req.insert_header(("Authorization", format!("Bearer {token}")));
        }
        let resp = test::call_service($app, req.to_request()).await;
        let status = resp.status();
        let bytes = test::read_body(resp).await;
        (
            status,
            serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
        )
    }};
}

#[actix_web::test]
async fn an_item_without_an_inventory_is_rejected_and_not_filed_into_inventory_1() {
    let setup = setup_with_inventory_1().await;
    let app = app!(setup);

    let (status, _) = post_item!(&app, Some(&setup.token), json!({ "name": "Drill" }));
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(item_count(&setup).await, 0, "nothing was created anywhere");
}

#[actix_web::test]
async fn zero_and_negative_inventory_ids_are_a_clear_400() {
    let setup = setup_with_inventory_1().await;
    let app = app!(setup);

    for bad in [0, -1, -5, i32::MIN] {
        let (status, body) = post_item!(
            &app,
            Some(&setup.token),
            json!({ "inventory_id": bad, "name": "Drill" })
        );
        assert_eq!(status, StatusCode::BAD_REQUEST, "inventory_id {bad}");
        assert_eq!(body["error"], "Validation failed", "inventory_id {bad}");
        assert!(
            body["message"].as_str().unwrap().contains("inventory_id"),
            "{body}"
        );
    }
    assert_eq!(item_count(&setup).await, 0);
}

#[actix_web::test]
async fn a_valid_inventory_still_creates_the_item_there() {
    let setup = setup_with_inventory_1().await;
    let app = app!(setup);

    let (status, body) = post_item!(
        &app,
        Some(&setup.token),
        json!({ "inventory_id": 1, "name": "Drill" })
    );
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["inventory_id"], 1);
    assert_eq!(item_count(&setup).await, 1);
}

#[actix_web::test]
async fn validation_runs_before_authorization_and_authentication_still_applies() {
    let setup = setup_with_inventory_1().await;
    let app = app!(setup);

    // A well-formed request without a token is unauthorised ...
    let (status, _) = post_item!(&app, None, json!({ "inventory_id": 1, "name": "x" }));
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // ... a malformed one is rejected as such, whoever sends it (validation comes first)
    let (status, _) = post_item!(&app, None, json!({ "inventory_id": 0, "name": "x" }));
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Someone else's inventory is still not found for a user with no access to it
    let other = common::test_username("other");
    common::create_test_user(&setup.scratch.pool, &other).await;
    let other_token = common::get_test_token(&setup.scratch.pool, &other).await;
    let (status, _) = post_item!(
        &app,
        Some(&other_token),
        json!({ "inventory_id": 1, "name": "x" })
    );
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(item_count(&setup).await, 0);
}

// ---------------------------------------------------------------- the migration

async fn column_default(scratch: &ScratchDb) -> Option<String> {
    scratch
        .pool
        .get()
        .await
        .unwrap()
        .query_one(
            "SELECT column_default FROM information_schema.columns
             WHERE table_schema = 'public' AND table_name = 'items'
               AND column_name = 'inventory_id'",
            &[],
        )
        .await
        .unwrap()
        .get(0)
}

#[actix_web::test]
async fn migration_v027_removes_the_default_so_an_omitted_inventory_fails() {
    let mut scratch = ScratchDb::create_up_to(26).await;
    let username = common::test_username("mig");
    common::create_test_user(&scratch.pool, &username).await;
    let client = scratch.pool.get().await.unwrap();
    let user_id: Uuid = client
        .query_one("SELECT id FROM users WHERE username = $1", &[&username])
        .await
        .unwrap()
        .get(0);
    client
        .execute(
            "INSERT INTO inventories (id, name, user_id) VALUES (1, 'First', $1)",
            &[&user_id],
        )
        .await
        .unwrap();

    // Before: the default is 1, and an insert that omits the inventory silently lands in it
    assert_eq!(column_default(&scratch).await.as_deref(), Some("1"));
    client
        .execute("INSERT INTO items (name) VALUES ('before')", &[])
        .await
        .expect("the old default accepts it");
    let filed_into: i32 = client
        .query_one("SELECT inventory_id FROM items WHERE name = 'before'", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(filed_into, 1);

    scratch.apply_migration(27).await;

    // After: no default, the NOT NULL constraint rejects an omitted inventory, and the row
    // inserted earlier is untouched
    assert_eq!(column_default(&scratch).await, None);
    let err = client
        .execute("INSERT INTO items (name) VALUES ('after')", &[])
        .await
        .expect_err("no default any more");
    let db = err.as_db_error().expect("a database error");
    assert_eq!(db.column(), Some("inventory_id"));
    assert!(db.message().contains("null value"), "{}", db.message());
    let still: i32 = client
        .query_one("SELECT inventory_id FROM items WHERE name = 'before'", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(still, 1);

    // Re-applying is harmless
    let sql = std::fs::read_to_string("migrations/V027__drop_items_inventory_default.sql").unwrap();
    client.batch_execute(&sql).await.expect("idempotent");
    assert_eq!(column_default(&scratch).await, None);
}
