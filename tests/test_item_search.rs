// Item search: `GET /api/items/search?q=` (and the deprecated `GET /api/items/search/{query}`
// on the same engine) and `GET /api/inventories/{id}/organizer-values`, the batch call the
// detail page filters with.
//
// The cases in frontend/src/test/fixtures/item_search_cases.json are shared with the detail page's own
// filter (frontend/src/utils/itemFilter.test.ts), so both must find the same items.
//
// Each test works in its own scratch database (tests/common::ScratchDb).

mod common;

use actix_web::{http::StatusCode, test, web, App};
use common::ScratchDb;
use home_registry::access_log::access_logger;
use home_registry::api;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, Once};
use uuid::Uuid;

const FIXTURE: &str = include_str!("../frontend/src/test/fixtures/item_search_cases.json");
const SENTINEL: &str = "SENTINEL-7be03a";

// ---------------------------------------------------------------- log capture

struct CaptureLogger;
static LOGS: Mutex<Vec<(log::Level, String)>> = Mutex::new(Vec::new());

impl log::Log for CaptureLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        LOGS.lock()
            .unwrap()
            .push((record.level(), record.args().to_string()));
    }
    fn flush(&self) {}
}

fn install_logger() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        log::set_logger(&CaptureLogger).expect("no other logger in this test binary");
        log::set_max_level(log::LevelFilter::Info);
    });
}

// ------------------------------------------------------------------- helpers

macro_rules! app {
    ($pool:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($pool.clone()))
                .wrap(access_logger())
                .service(api::init_routes()),
        )
        .await
    };
}

/// GET with an optional bearer token; yields (status, parsed JSON or Null, raw body).
macro_rules! get {
    ($app:expr, $uri:expr, $token:expr) => {{
        let mut request = test::TestRequest::get().uri($uri);
        if let Some(token) = $token {
            request = request.insert_header(("Authorization", format!("Bearer {}", token)));
        }
        let resp = test::call_service($app, request.to_request()).await;
        let status = resp.status();
        let raw = String::from_utf8_lossy(&test::read_body(resp).await).to_string();
        let json = serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
        (status, json, raw)
    }};
}

/// Percent-encode everything but unreserved characters, so a term is one query value.
fn enc(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

async fn exec(
    pool: &deadpool_postgres::Pool,
    sql: &str,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) {
    pool.get()
        .await
        .unwrap()
        .execute(sql, params)
        .await
        .unwrap();
}

async fn one_i32(
    pool: &deadpool_postgres::Pool,
    sql: &str,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> i32 {
    pool.get()
        .await
        .unwrap()
        .query_one(sql, params)
        .await
        .unwrap()
        .get(0)
}

async fn user(pool: &deadpool_postgres::Pool, prefix: &str) -> (Uuid, String) {
    let name = common::test_username(prefix);
    common::create_test_user(pool, &name).await;
    let token = common::get_test_token(pool, &name).await;
    let id: Uuid = pool
        .get()
        .await
        .unwrap()
        .query_one("SELECT id FROM users WHERE username = $1", &[&name])
        .await
        .unwrap()
        .get(0);
    (id, token)
}

async fn item(pool: &deadpool_postgres::Pool, inv: i32, name: &str, description: &str) -> i32 {
    one_i32(
        pool,
        "INSERT INTO items (inventory_id, name, description) VALUES ($1, $2, $3) RETURNING id",
        &[&inv, &name, &description],
    )
    .await
}

fn nullable(value: &Value) -> Option<&str> {
    value.as_str()
}

/// The fixture inventory, owned by `owner`: its items by key.
struct Seeded {
    inv: i32,
    key_of: BTreeMap<i32, String>,
    ids: BTreeMap<String, i32>,
}

async fn seed_fixture(pool: &deadpool_postgres::Pool, owner: Uuid, name: &str) -> Seeded {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let inv = common::create_test_inventory(pool, owner, name)
        .await
        .unwrap();

    // organizer name -> (type id, kind, option name -> option id)
    let mut types: BTreeMap<String, (i32, String, BTreeMap<String, i32>)> = BTreeMap::new();
    for (order, organizer) in fixture["organizers"].as_array().unwrap().iter().enumerate() {
        let organizer_name = organizer["name"].as_str().unwrap();
        let kind = organizer["input_type"].as_str().unwrap();
        let order = i32::try_from(order).unwrap();
        let type_id = one_i32(
            pool,
            "INSERT INTO organizer_types (inventory_id, name, input_type, display_order)
             VALUES ($1, $2, $3, $4) RETURNING id",
            &[&inv, &organizer_name, &kind, &order],
        )
        .await;
        let mut options = BTreeMap::new();
        for option in organizer["options"].as_array().into_iter().flatten() {
            let option = option.as_str().unwrap();
            let id = one_i32(
                pool,
                "INSERT INTO organizer_options (organizer_type_id, name) VALUES ($1, $2)
                 RETURNING id",
                &[&type_id, &option],
            )
            .await;
            options.insert(option.to_string(), id);
        }
        types.insert(
            organizer_name.to_string(),
            (type_id, kind.to_string(), options),
        );
    }

    let mut key_of = BTreeMap::new();
    let mut ids = BTreeMap::new();
    for entry in fixture["items"].as_array().unwrap() {
        let key = entry["key"].as_str().unwrap();
        let item_name = entry["name"].as_str().unwrap();
        let (description, notes, category, location) = (
            nullable(&entry["description"]),
            nullable(&entry["notes"]),
            nullable(&entry["category"]),
            nullable(&entry["location"]),
        );
        let id = one_i32(
            pool,
            "INSERT INTO items (inventory_id, name, description, notes, category, location)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
            &[&inv, &item_name, &description, &notes, &category, &location],
        )
        .await;
        for (organizer_name, value) in entry["values"].as_object().unwrap() {
            let value = value.as_str().unwrap();
            let (type_id, kind, options) = &types[organizer_name];
            if kind == "select" {
                let option_id = options[value];
                exec(
                    pool,
                    "INSERT INTO item_organizer_values (item_id, organizer_type_id, organizer_option_id)
                     VALUES ($1, $2, $3)",
                    &[&id, type_id, &option_id],
                )
                .await;
            } else {
                exec(
                    pool,
                    "INSERT INTO item_organizer_values (item_id, organizer_type_id, text_value)
                     VALUES ($1, $2, $3)",
                    &[&id, type_id, &value],
                )
                .await;
            }
        }
        key_of.insert(id, key.to_string());
        ids.insert(key.to_string(), id);
    }
    Seeded { inv, key_of, ids }
}

/// The item ids of a search response's results, in order.
fn result_ids(json: &Value) -> Vec<i32> {
    json["data"]["results"]
        .as_array()
        .unwrap_or_else(|| panic!("no results in {json}"))
        .iter()
        .map(|hit| i32::try_from(hit["id"].as_i64().unwrap()).unwrap())
        .collect()
}

async fn share_view(pool: &deadpool_postgres::Pool, inv: i32, with: Uuid, by: Uuid) {
    exec(
        pool,
        "INSERT INTO inventory_shares (inventory_id, shared_with_user_id, shared_by_user_id, permission_level)
         VALUES ($1, $2, $3, 'view')",
        &[&inv, &with, &by],
    )
    .await;
}

// --------------------------------------------------------------------- tests

#[actix_web::test]
async fn shared_cases_find_the_same_items_through_both_routes() {
    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let pool = scratch.pool.clone();
    let (owner, token) = user(&pool, "owner").await;
    let seeded = seed_fixture(&pool, owner, "Workshop").await;
    let app = app!(pool);

    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let term = case["term"].as_str().unwrap();
        let label = case["label"].as_str().unwrap();
        let expected: BTreeSet<String> = case["expect"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap().to_string())
            .collect();

        let (status, json, _) = get!(
            &app,
            &format!("/api/items/search?q={}", enc(term)),
            Some(&token)
        );
        assert_eq!(status, StatusCode::OK, "{label}: {json}");
        let found: BTreeSet<String> = result_ids(&json)
            .iter()
            .map(|id| seeded.key_of[id].clone())
            .collect();
        assert_eq!(found, expected, "{label} (q={term:?})");
        assert_eq!(
            json["data"]["total"].as_u64().unwrap(),
            expected.len() as u64,
            "{label}: total"
        );

        // The deprecated route is the same search with the term in the path
        let (status, legacy, _) = get!(
            &app,
            &format!("/api/items/search/{}", enc(term)),
            Some(&token)
        );
        assert_eq!(status, StatusCode::OK, "{label} (path route): {legacy}");
        let found: BTreeSet<String> = legacy["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| {
                seeded.key_of[&i32::try_from(item["id"].as_i64().unwrap()).unwrap()].clone()
            })
            .collect();
        assert_eq!(found, expected, "{label} (path route)");
    }
}

#[actix_web::test]
async fn a_slash_in_the_term_works_in_the_query_string() {
    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let pool = scratch.pool.clone();
    let (owner, token) = user(&pool, "owner").await;
    let inv = common::create_test_inventory(&pool, owner, "Home")
        .await
        .unwrap();
    let id = item(&pool, inv, "Left/Right switch", "an A/B box").await;
    let app = app!(pool);

    for term in ["left/right", "A/B", "/"] {
        let (status, json, _) = get!(
            &app,
            &format!("/api/items/search?q={}", enc(term)),
            Some(&token)
        );
        assert_eq!(status, StatusCode::OK, "{term}: {json}");
        assert_eq!(result_ids(&json), vec![id], "{term}");
    }
}

#[actix_web::test]
async fn results_stay_inside_what_the_caller_can_see() {
    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let pool = scratch.pool.clone();
    let (owner, owner_token) = user(&pool, "owner").await;
    let (stranger, stranger_token) = user(&pool, "stranger").await;
    let (viewer, viewer_token) = user(&pool, "viewer").await;
    let seeded = seed_fixture(&pool, owner, "Workshop").await;
    let own_inv = common::create_test_inventory(&pool, stranger, "Strangers")
        .await
        .unwrap();
    let strangers_item = item(&pool, own_inv, "Bosch shelf", "").await;
    share_view(&pool, seeded.inv, viewer, owner).await;
    let app = app!(pool);

    // Nothing leaks: not by name, not by organizer value ("bosch" is an option of the owner)
    let (status, json, _) = get!(&app, "/api/items/search?q=drill", Some(&stranger_token));
    assert_eq!(status, StatusCode::OK);
    assert!(result_ids(&json).is_empty(), "{json}");
    assert_eq!(json["data"]["total"], 0);
    let (_, json, _) = get!(&app, "/api/items/search?q=bosch", Some(&stranger_token));
    assert_eq!(result_ids(&json), vec![strangers_item], "only their own");
    let (_, json, _) = get!(&app, "/api/items/search/bosch", Some(&stranger_token));
    let legacy: Vec<i64> = json["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_i64().unwrap())
        .collect();
    assert_eq!(legacy, vec![i64::from(strangers_item)]);
    let (_, json, _) = get!(&app, "/api/items/search?q=sn-0042", Some(&stranger_token));
    assert!(result_ids(&json).is_empty(), "text values do not leak");

    // A view-only share finds the shared inventory's items, organizer matches included
    let (status, json, _) = get!(&app, "/api/items/search?q=bosch", Some(&viewer_token));
    assert_eq!(status, StatusCode::OK);
    let found: BTreeSet<_> = result_ids(&json).into_iter().collect();
    assert_eq!(
        found,
        BTreeSet::from([seeded.ids["drill"], seeded.ids["sander"]])
    );

    // The owner sees the shared inventory's items and nothing of the stranger's
    let (_, json, _) = get!(&app, "/api/items/search?q=bosch", Some(&owner_token));
    assert!(!result_ids(&json).contains(&strangers_item));

    // Not signed in
    let (status, _, _) = get!(&app, "/api/items/search?q=drill", None::<&str>);
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = get!(&app, "/api/items/search/drill", None::<&str>);
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn name_matches_come_first_then_newest_then_the_higher_id() {
    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let pool = scratch.pool.clone();
    let (owner, token) = user(&pool, "owner").await;
    let inv = common::create_test_inventory(&pool, owner, "Home")
        .await
        .unwrap();

    let old_name = item(&pool, inv, "Old tool shed", "").await;
    let described = item(&pool, inv, "Hammer", "a tool for nails").await;
    let twin_low = item(&pool, inv, "Toolbox A", "").await;
    let twin_high = item(&pool, inv, "Toolbox B", "").await;
    let newest_described = item(&pool, inv, "Pliers", "tool").await;
    for (id, when) in [
        (old_name, "2020-01-01T00:00:00Z"),
        (described, "2023-06-01T00:00:00Z"),
        (twin_low, "2024-01-01T00:00:00Z"),
        (twin_high, "2024-01-01T00:00:00Z"),
        (newest_described, "2025-01-01T00:00:00Z"),
    ] {
        exec(
            &pool,
            "UPDATE items SET created_at = $2::text::timestamptz WHERE id = $1",
            &[&id, &when],
        )
        .await;
    }
    let app = app!(pool);

    let (status, json, _) = get!(&app, "/api/items/search?q=TOOL", Some(&token));
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(
        result_ids(&json),
        vec![twin_high, twin_low, old_name, newest_described, described],
        "name matches (newest first, the higher id first on a tie), then the others"
    );
    // Same order every time
    let (_, again, _) = get!(&app, "/api/items/search?q=TOOL", Some(&token));
    assert_eq!(result_ids(&again), result_ids(&json));
}

#[actix_web::test]
async fn an_item_without_a_creation_time_comes_after_the_others() {
    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let pool = scratch.pool.clone();
    let (owner, token) = user(&pool, "owner").await;
    let inv = common::create_test_inventory(&pool, owner, "Home")
        .await
        .unwrap();
    // items.created_at is nullable. The client sorts a missing time last; so must the search.
    let undated = item(&pool, inv, "Lamp undated", "").await;
    let old = item(&pool, inv, "Lamp old", "").await;
    let new = item(&pool, inv, "Lamp new", "").await;
    let described = item(&pool, inv, "Shade", "a lamp shade").await;
    exec(
        &pool,
        "UPDATE items SET created_at = NULL WHERE id = $1",
        &[&undated],
    )
    .await;
    exec(
        &pool,
        "UPDATE items SET created_at = '2020-01-01T00:00:00Z' WHERE id = $1",
        &[&old],
    )
    .await;
    exec(
        &pool,
        "UPDATE items SET created_at = '2024-01-01T00:00:00Z' WHERE id = $1",
        &[&new],
    )
    .await;
    exec(
        &pool,
        "UPDATE items SET created_at = '2023-01-01T00:00:00Z' WHERE id = $1",
        &[&described],
    )
    .await;
    let app = app!(pool);

    let (status, json, _) = get!(&app, "/api/items/search?q=lamp", Some(&token));
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(
        result_ids(&json),
        vec![new, old, undated, described],
        "name matches newest first with the undated one last, then the other matches"
    );
}

#[actix_web::test]
async fn the_limit_caps_the_list_but_total_counts_everything() {
    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let pool = scratch.pool.clone();
    let (owner, token) = user(&pool, "owner").await;
    let inv = common::create_test_inventory(&pool, owner, "Home")
        .await
        .unwrap();
    for n in 0..7 {
        item(&pool, inv, &format!("Cable {n}"), "").await;
    }
    item(&pool, inv, "Lamp", "").await;
    let app = app!(pool);

    let (status, json, _) = get!(&app, "/api/items/search?q=cable&limit=3", Some(&token));
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(result_ids(&json).len(), 3);
    assert_eq!(json["data"]["total"], 7);
    assert_eq!(json["data"]["limit"], 3);

    let (_, json, _) = get!(&app, "/api/items/search?q=cable", Some(&token));
    assert_eq!(result_ids(&json).len(), 7, "the default is far above 7");
    assert_eq!(json["data"]["limit"], 100);

    let (_, json, _) = get!(&app, "/api/items/search?q=cable&limit=100000", Some(&token));
    assert_eq!(json["data"]["limit"], 500, "capped, and the answer says so");
    assert_eq!(result_ids(&json).len(), 7);

    for bad in ["0", "-1", "abc", "1.5", ""] {
        let (status, json, raw) = get!(
            &app,
            &format!("/api/items/search?q=cable&limit={bad}"),
            Some(&token)
        );
        assert_eq!(status, StatusCode::BAD_REQUEST, "limit={bad:?}: {raw}");
        assert_eq!(json["success"], false);
        assert!(json["error"].is_string() && json["message"].is_string());
    }
}

#[actix_web::test]
async fn the_deprecated_route_returns_up_to_500_items_not_the_default_100() {
    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let pool = scratch.pool.clone();
    let (owner, token) = user(&pool, "owner").await;
    let inv = common::create_test_inventory(&pool, owner, "Home")
        .await
        .unwrap();
    exec(
        &pool,
        "INSERT INTO items (inventory_id, name) SELECT $1, 'Cable ' || g FROM generate_series(1, 520) g",
        &[&inv],
    )
    .await;
    let app = app!(pool);

    let (status, json, _) = get!(&app, "/api/items/search/cable", Some(&token));
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["data"].as_array().unwrap().len(), 500);
    // The query form keeps its own default
    let (_, json, _) = get!(&app, "/api/items/search?q=cable", Some(&token));
    assert_eq!(result_ids(&json).len(), 100);
    assert_eq!(json["data"]["total"], 520);
}

#[actix_web::test]
async fn bad_terms_get_a_fixed_400_that_repeats_nothing() {
    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let pool = scratch.pool.clone();
    let (_, token) = user(&pool, "owner").await;
    let app = app!(pool);

    let too_long = format!("{SENTINEL}{}", "x".repeat(101 - SENTINEL.len()));
    assert_eq!(too_long.chars().count(), 101);
    let with_nul = format!("{SENTINEL}\0");
    let with_newline = format!("{SENTINEL}\nb");
    let bad: [(&str, String, &str); 6] = [
        ("missing q", "/api/items/search".to_string(), "required"),
        ("empty q", "/api/items/search?q=".to_string(), "required"),
        (
            "blank q",
            "/api/items/search?q=%20%09".to_string(),
            "required",
        ),
        (
            "101 characters",
            format!("/api/items/search?q={}", enc(&too_long)),
            "100 characters",
        ),
        (
            "NUL",
            format!("/api/items/search?q={}", enc(&with_nul)),
            "control characters",
        ),
        (
            "newline inside",
            format!("/api/items/search?q={}", enc(&with_newline)),
            "control characters",
        ),
    ];
    for (label, uri, fragment) in &bad {
        let (status, json, raw) = get!(&app, uri, Some(&token));
        assert_eq!(status, StatusCode::BAD_REQUEST, "{label}: {raw}");
        assert_eq!(json["success"], false, "{label}");
        let message = json["message"].as_str().unwrap();
        assert!(message.contains(fragment), "{label}: {message}");
        assert!(!raw.contains(SENTINEL), "{label} echoed the term: {raw}");
    }

    // The deprecated route applies the same rules
    let (status, json, raw) = get!(
        &app,
        &format!("/api/items/search/{}", enc(&too_long)),
        Some(&token)
    );
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["success"], false);
    assert!(!raw.contains(SENTINEL));
    let (status, _, _) = get!(&app, "/api/items/search/%20", Some(&token));
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // The limits themselves are allowed: 100 characters, counted as characters not bytes
    for ok in ["x".repeat(100), "é".repeat(100), "  a  ".to_string()] {
        let (status, _, raw) = get!(
            &app,
            &format!("/api/items/search?q={}", enc(&ok)),
            Some(&token)
        );
        assert_eq!(status, StatusCode::OK, "{raw}");
    }
}

#[actix_web::test]
async fn each_result_says_where_it_lives_and_why_it_matched() {
    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let pool = scratch.pool.clone();
    let (owner, token) = user(&pool, "owner").await;
    let seeded = seed_fixture(&pool, owner, "Workshop").await;
    // A second organizer on the drill that also matches "bosch": listed in organizer order
    let extra = one_i32(
        &pool,
        "INSERT INTO organizer_types (inventory_id, name, input_type, display_order)
         VALUES ($1, 'Supplier', 'text', 9) RETURNING id",
        &[&seeded.inv],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO item_organizer_values (item_id, organizer_type_id, text_value) VALUES ($1, $2, 'Bosch Ltd')",
        &[&seeded.ids["drill"], &extra],
    )
    .await;
    let app = app!(pool);

    let (_, json, _) = get!(&app, "/api/items/search?q=bosch", Some(&token));
    let hits = json["data"]["results"].as_array().unwrap();
    let drill = hits
        .iter()
        .find(|h| h["id"].as_i64() == Some(i64::from(seeded.ids["drill"])))
        .unwrap();
    assert_eq!(drill["inventory_name"], "Workshop");
    assert_eq!(drill["inventory_id"], seeded.inv);
    assert_eq!(drill["name"], "Cordless Drill");
    assert_eq!(
        drill["matches"],
        serde_json::json!([
            { "organizer": "Brand", "value": "Bosch" },
            { "organizer": "Supplier", "value": "Bosch Ltd" }
        ])
    );

    // A match in the item itself lists no organizer
    let (_, json, _) = get!(&app, "/api/items/search?q=hammer", Some(&token));
    assert_eq!(json["data"]["results"][0]["matches"], serde_json::json!([]));

    // The image organizer never shows up as a reason
    let (_, json, _) = get!(&app, "/api/items/search?q=hidden", Some(&token));
    assert!(result_ids(&json).is_empty(), "{json}");
}

#[actix_web::test]
async fn a_deleted_option_is_not_searchable_and_does_not_break_the_search() {
    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let pool = scratch.pool.clone();
    let (owner, token) = user(&pool, "owner").await;
    let seeded = seed_fixture(&pool, owner, "Workshop").await;
    exec(
        &pool,
        "DELETE FROM organizer_options WHERE name = 'Makita'",
        &[],
    )
    .await;
    let app = app!(pool);

    let (status, json, _) = get!(&app, "/api/items/search?q=makita", Some(&token));
    assert_eq!(status, StatusCode::OK, "{json}");
    assert!(result_ids(&json).is_empty(), "{json}");
    let (_, json, _) = get!(&app, "/api/items/search?q=saw", Some(&token));
    assert_eq!(result_ids(&json), vec![seeded.ids["saw"]]);
}

#[actix_web::test]
async fn the_deprecated_route_keeps_its_shape_and_repeats_nothing() {
    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let pool = scratch.pool.clone();
    let (owner, token) = user(&pool, "owner").await;
    let seeded = seed_fixture(&pool, owner, "Workshop").await;
    let app = app!(pool);

    let (status, json, raw) = get!(&app, "/api/items/search/drill", Some(&token));
    assert_eq!(status, StatusCode::OK);
    let items = json["data"]
        .as_array()
        .expect("still a plain list of items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], seeded.ids["drill"]);
    assert_eq!(items[0]["name"], "Cordless Drill");
    assert!(items[0].get("matches").is_none());
    assert!(
        !raw.contains("drill'") && !json["message"].as_str().unwrap().contains("drill"),
        "the message no longer echoes the term: {raw}"
    );
}

#[actix_web::test]
async fn the_term_never_reaches_a_log_line() {
    install_logger();
    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let pool = scratch.pool.clone();
    let (owner, token) = user(&pool, "owner").await;
    let inv = common::create_test_inventory(&pool, owner, "Home")
        .await
        .unwrap();
    item(&pool, inv, &format!("{SENTINEL} thing"), "").await;
    let app = app!(pool);
    LOGS.lock().unwrap().clear();

    let too_long = format!("{SENTINEL}{}", "x".repeat(101));
    let uris = [
        format!("/api/items/search?q={SENTINEL}"),
        format!("/api/items/search?q={}", enc(&too_long)),
        format!("/api/items/search?q=%20&sentinel={SENTINEL}"),
        format!("/api/items/search/{SENTINEL}"),
        format!("/api/items/search/{}", enc(&too_long)),
    ];
    for uri in &uris {
        get!(&app, uri, Some(&token));
    }
    // A hit and a miss
    let (_, json, _) = get!(
        &app,
        &format!("/api/items/search?q={SENTINEL}"),
        Some(&token)
    );
    assert_eq!(result_ids(&json).len(), 1, "the search itself worked");

    let logs = LOGS.lock().unwrap();
    assert!(!logs.is_empty(), "something was logged at all");
    for (level, message) in logs.iter() {
        assert!(
            !message.contains(SENTINEL),
            "the term was logged at {level}: {message}"
        );
    }
}

#[actix_web::test]
async fn organizer_values_for_a_whole_inventory_in_one_call() {
    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let pool = scratch.pool.clone();
    let (owner, owner_token) = user(&pool, "owner").await;
    let (_, stranger_token) = user(&pool, "stranger").await;
    let (viewer, viewer_token) = user(&pool, "viewer").await;
    let seeded = seed_fixture(&pool, owner, "Workshop").await;
    // Another inventory whose values must not appear
    let other = common::create_test_inventory(&pool, owner, "Other")
        .await
        .unwrap();
    let other_item = item(&pool, other, "Elsewhere", "").await;
    let other_type = one_i32(
        &pool,
        "INSERT INTO organizer_types (inventory_id, name, input_type) VALUES ($1, 'Note', 'text') RETURNING id",
        &[&other],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO item_organizer_values (item_id, organizer_type_id, text_value) VALUES ($1, $2, 'elsewhere-value')",
        &[&other_item, &other_type],
    )
    .await;
    // A value whose option was deleted stays as a row with no option and no text
    exec(
        &pool,
        "DELETE FROM organizer_options WHERE name = 'Makita'",
        &[],
    )
    .await;
    share_view(&pool, seeded.inv, viewer, owner).await;
    let app = app!(pool);

    let uri = format!("/api/inventories/{}/organizer-values", seeded.inv);
    let (status, json, raw) = get!(&app, &uri, Some(&owner_token));
    assert_eq!(status, StatusCode::OK, "{raw}");
    assert_eq!(json["success"], true);
    let rows = json["data"].as_array().unwrap();

    // (item key, value) pairs: select option names and text, no image values, no deleted option
    let pairs: BTreeSet<(String, String)> = rows
        .iter()
        .map(|r| {
            (
                seeded.key_of[&i32::try_from(r["item_id"].as_i64().unwrap()).unwrap()].clone(),
                r["value"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        BTreeSet::from([
            ("drill".to_string(), "Bosch".to_string()),
            ("drill".to_string(), "SN-0042".to_string()),
            ("sander".to_string(), "Bosch".to_string()),
            ("plain".to_string(), "50% Off".to_string()),
        ])
    );
    assert!(
        !raw.contains("elsewhere-value"),
        "other inventories stay out"
    );
    assert!(!raw.contains("hidden-photo"), "image values stay out");

    // Select rows carry the option id the page filters on, text rows have none
    let bosch = rows.iter().find(|r| r["value"] == "Bosch").unwrap();
    assert!(bosch["organizer_option_id"].is_i64());
    assert!(bosch["organizer_type_id"].is_i64());
    let serial = rows.iter().find(|r| r["value"] == "SN-0042").unwrap();
    assert!(serial["organizer_option_id"].is_null());

    // View access is enough; no access, no data
    let (status, json, _) = get!(&app, &uri, Some(&viewer_token));
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["data"].as_array().unwrap().len(), 4);
    let (status, _, raw) = get!(&app, &uri, Some(&stranger_token));
    assert_eq!(status, StatusCode::NOT_FOUND, "{raw}");
    assert!(!raw.contains("Bosch"));
    let (status, _, _) = get!(&app, &uri, None::<&str>);
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = get!(
        &app,
        "/api/inventories/999999/organizer-values",
        Some(&owner_token)
    );
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// Performance check, not part of the normal run (`cargo test --test test_item_search -- --ignored
/// --nocapture`). "Slow" means a p95 above 200 ms for any of the terms below on 60,000 items in
/// six inventories, each with a select and a text organizer value, after ANALYZE.
#[actix_web::test]
#[ignore = "builds 60,000 items; run by hand"]
async fn search_stays_under_200_ms_at_p95_on_60k_items() {
    use std::time::{Duration, Instant};

    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let pool = scratch.pool.clone();
    let (owner, _) = user(&pool, "owner").await;

    for n in 0..6 {
        let inv = common::create_test_inventory(&pool, owner, &format!("Inventory {n}"))
            .await
            .unwrap();
        let brand = one_i32(
            &pool,
            "INSERT INTO organizer_types (inventory_id, name, input_type, display_order)
             VALUES ($1, 'Brand', 'select', 1) RETURNING id",
            &[&inv],
        )
        .await;
        let serial = one_i32(
            &pool,
            "INSERT INTO organizer_types (inventory_id, name, input_type, display_order)
             VALUES ($1, 'Serial', 'text', 2) RETURNING id",
            &[&inv],
        )
        .await;
        let mut options = Vec::new();
        for brand_name in [
            "Bosch", "Makita", "Dewalt", "Ikea", "Philips", "Sony", "Miele", "Stanley", "Hilti",
            "Festool",
        ] {
            options.push(
                one_i32(
                    &pool,
                    "INSERT INTO organizer_options (organizer_type_id, name) VALUES ($1, $2) RETURNING id",
                    &[&brand, &brand_name],
                )
                .await,
            );
        }
        exec(
            &pool,
            "INSERT INTO items (inventory_id, name, description, notes, created_at)
             SELECT $1, 'Item ' || g || ' ' || (ARRAY['drill','saw','lamp','cable','shelf','chair'])[1 + g % 6],
                    md5(g::text) || ' ' || (ARRAY['red','blue','old','new','spare','boxed'])[1 + (g / 7) % 6],
                    CASE WHEN g % 5 = 0 THEN md5((g * 3)::text) END,
                    NOW() - (g || ' seconds')::interval
             FROM generate_series(1, 10000) g",
            &[&inv],
        )
        .await;
        exec(
            &pool,
            "INSERT INTO item_organizer_values (item_id, organizer_type_id, organizer_option_id)
             SELECT i.id, $2, ($3::int[])[1 + i.id % 10] FROM items i WHERE i.inventory_id = $1",
            &[&inv, &brand, &options],
        )
        .await;
        exec(
            &pool,
            "INSERT INTO item_organizer_values (item_id, organizer_type_id, text_value)
             SELECT i.id, $2, 'SN-' || substr(md5(i.id::text), 1, 8)
             FROM items i WHERE i.inventory_id = $1 AND i.id % 2 = 0",
            &[&inv, &serial],
        )
        .await;
    }
    exec(&pool, "ANALYZE", &[]).await;

    let db = home_registry::db::DatabaseService::new(pool.clone());
    let mut worst = Duration::ZERO;
    for (label, term) in [
        ("matches everything", "item"),
        ("a name word (1 in 6)", "drill"),
        ("one item", "item 31337 "),
        ("an organizer option (1 in 10)", "bosch"),
        ("a text value", "sn-3a"),
        ("hex in descriptions", "ab"),
        ("nothing", "zzzzqqq"),
        ("two characters", "dr"),
    ] {
        let mut times = Vec::new();
        let mut total = 0;
        for _ in 0..50 {
            let started = Instant::now();
            let found = db.search_items(owner, term, 100).await.unwrap();
            times.push(started.elapsed());
            total = found.total;
        }
        times.sort();
        let p95 = times[times.len() * 95 / 100];
        worst = worst.max(p95);
        println!(
            "{label:<32} term {term:<12} matches {total:>6}  p50 {:>6.1} ms  p95 {:>6.1} ms  max {:>6.1} ms",
            times[times.len() / 2].as_secs_f64() * 1000.0,
            p95.as_secs_f64() * 1000.0,
            times[times.len() - 1].as_secs_f64() * 1000.0,
        );
    }
    assert!(
        worst < Duration::from_millis(200),
        "slowest p95 was {worst:?}"
    );
}
