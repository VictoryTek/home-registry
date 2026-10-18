// Inventory reports: one filter definition for the item list, the statistics, the breakdown and the
// CSV; grouping and filtering by organizer; strict parsing of the organizer parameters.
//
// Each test works in its own scratch database (tests/common::ScratchDb).

mod common;

use actix_web::{http::StatusCode, test, web, App};
use common::ScratchDb;
use home_registry::api;
use serde_json::Value;
use std::collections::BTreeSet;
use uuid::Uuid;

macro_rules! app {
    ($pool:expr) => {
        test::init_service(
            App::new().app_data(web::Data::new($pool.clone())).service(
                web::scope("/api")
                    .service(api::get_inventory_report)
                    .service(api::get_inventory_statistics_endpoint)
                    .service(api::get_category_breakdown_endpoint),
            ),
        )
        .await
    };
}

/// GET with a bearer token; yields (status, parsed JSON or Null, raw body).
macro_rules! get {
    ($app:expr, $uri:expr, $token:expr) => {{
        let resp = test::call_service(
            $app,
            test::TestRequest::get()
                .uri($uri)
                .insert_header(("Authorization", format!("Bearer {}", $token)))
                .to_request(),
        )
        .await;
        let status = resp.status();
        let raw = String::from_utf8_lossy(&test::read_body(resp).await).to_string();
        let json = serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
        (status, json, raw)
    }};
}

/// Everything the tests need to know about the seeded data.
struct Data {
    scratch: ScratchDb,
    jwt: String,
    user: Uuid,
    inv: i32,
    category: i32,
    tools: i32,
    books: i32,
    toys: i32,
    room: i32,
    garage: i32,
    kitchen: i32,
    serial: i32,
    photo: i32,
    items: Vec<i32>,
}

impl Data {
    fn pool(&self) -> &deadpool_postgres::Pool {
        &self.scratch.pool
    }
    fn report(&self, query: &str) -> String {
        format!("/api/reports/inventory?inventory_id={}{query}", self.inv)
    }
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

async fn organizer(
    pool: &deadpool_postgres::Pool,
    inv: i32,
    name: &str,
    kind: &str,
    order: i32,
) -> i32 {
    one_i32(
        pool,
        "INSERT INTO organizer_types (inventory_id, name, input_type, display_order)
         VALUES ($1, $2, $3, $4) RETURNING id",
        &[&inv, &name, &kind, &order],
    )
    .await
}

async fn option(pool: &deadpool_postgres::Pool, type_id: i32, name: &str) -> i32 {
    one_i32(
        pool,
        "INSERT INTO organizer_options (organizer_type_id, name) VALUES ($1, $2) RETURNING id",
        &[&type_id, &name],
    )
    .await
}

async fn item(
    pool: &deadpool_postgres::Pool,
    inv: i32,
    name: &str,
    price: Option<f64>,
    quantity: Option<i32>,
    date: Option<&str>,
) -> i32 {
    one_i32(
        pool,
        "INSERT INTO items (inventory_id, name, purchase_price, quantity, purchase_date)
         VALUES ($1, $2, $3, $4, $5::text::date) RETURNING id",
        &[&inv, &name, &price, &quantity, &date],
    )
    .await
}

async fn value(
    pool: &deadpool_postgres::Pool,
    item: i32,
    type_id: i32,
    option: Option<i32>,
    text: Option<&str>,
) {
    exec(
        pool,
        "INSERT INTO item_organizer_values (item_id, organizer_type_id, organizer_option_id, text_value)
         VALUES ($1, $2, $3, $4)",
        &[&item, &type_id, &option, &text],
    )
    .await;
}

/// Seven items with known prices, quantities and organizer values:
///
/// | # | item    | price | qty  | purchase date | Category  | Room    | Serial |
/// |---|---------|-------|------|---------------|-----------|---------|--------|
/// | 1 | Drill   | 100   | 2    | 2023-03-01    | Tools     | Garage  | SN-1   |
/// | 2 | Saw     | 50    | 1    | 2023-06-01    | Tools     | Garage  |        |
/// | 3 | Novel   | 20    | 3    | 2024-01-10    | Books     |         |        |
/// | 4 | Atlas   | 40    | NULL | 2024-02-02    | Books     | Kitchen |        |
/// | 5 | Blocks  | NULL  | 4    | NULL          | Toys      |         |        |
/// | 6 | Mystery | 10    | 1    | 2024-05-05    | (no row)  | Kitchen |        |
/// | 7 | Ghost   | 30    | 1    | 2022-01-01    | deleted   | Garage  |        |
///
/// "deleted" is a value whose option was deleted afterwards (the row stays, the option is NULL).
async fn seed() -> Data {
    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let pool = scratch.pool.clone();
    let name = common::test_username("owner");
    common::create_admin_user(&pool, &name).await;
    let jwt = common::get_test_token(&pool, &name).await;
    let user: Uuid = pool
        .get()
        .await
        .unwrap()
        .query_one("SELECT id FROM users WHERE username = $1", &[&name])
        .await
        .unwrap()
        .get(0);
    let inv = common::create_test_inventory(&pool, user, "Home")
        .await
        .unwrap();

    let category = organizer(&pool, inv, "Category", "select", 1).await;
    let room = organizer(&pool, inv, "Room", "select", 2).await;
    let serial = organizer(&pool, inv, "Serial", "text", 3).await;
    let photo = organizer(&pool, inv, "Photo", "image", 4).await;
    let tools = option(&pool, category, "Tools").await;
    let books = option(&pool, category, "Books").await;
    let toys = option(&pool, category, "Toys").await;
    let obsolete = option(&pool, category, "Obsolete").await;
    let garage = option(&pool, room, "Garage").await;
    let kitchen = option(&pool, room, "Kitchen").await;

    let mut items = Vec::new();
    items.push(
        item(
            &pool,
            inv,
            "Drill",
            Some(100.0),
            Some(2),
            Some("2023-03-01"),
        )
        .await,
    );
    items.push(item(&pool, inv, "Saw", Some(50.0), Some(1), Some("2023-06-01")).await);
    items.push(item(&pool, inv, "Novel", Some(20.0), Some(3), Some("2024-01-10")).await);
    items.push(item(&pool, inv, "Atlas", Some(40.0), None, Some("2024-02-02")).await);
    items.push(item(&pool, inv, "Blocks", None, Some(4), None).await);
    items.push(
        item(
            &pool,
            inv,
            "Mystery",
            Some(10.0),
            Some(1),
            Some("2024-05-05"),
        )
        .await,
    );
    items.push(item(&pool, inv, "Ghost", Some(30.0), Some(1), Some("2022-01-01")).await);

    for (i, t) in [
        (0, tools),
        (1, tools),
        (2, books),
        (3, books),
        (4, toys),
        (6, obsolete),
    ] {
        value(&pool, items[i], category, Some(t), None).await;
    }
    for (i, r) in [
        (0, garage),
        (1, garage),
        (3, kitchen),
        (5, kitchen),
        (6, garage),
    ] {
        value(&pool, items[i], room, Some(r), None).await;
    }
    value(&pool, items[0], serial, None, Some("SN-1")).await;
    // The option behind the Ghost's category is deleted; the value row stays with a NULL option
    exec(
        &pool,
        "DELETE FROM organizer_options WHERE id = $1",
        &[&obsolete],
    )
    .await;

    Data {
        scratch,
        jwt,
        user,
        inv,
        category,
        tools,
        books,
        toys,
        room,
        garage,
        kitchen,
        serial,
        photo,
        items,
    }
}

fn num(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {value}"))
}

fn ids(report: &Value) -> BTreeSet<i64> {
    report["data"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["id"].as_i64().unwrap())
        .collect()
}

fn csv_rows(raw: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let mut reader = csv::ReaderBuilder::new().from_reader(raw.as_bytes());
    let header = reader.headers().unwrap().iter().map(String::from).collect();
    let rows = reader
        .records()
        .map(|r| r.unwrap().iter().map(String::from).collect())
        .collect();
    (header, rows)
}

// ---------------------------------------------------------------- one set of items everywhere

#[actix_web::test]
async fn the_list_the_statistics_the_breakdown_and_the_csv_describe_the_same_items() {
    let d = seed().await;
    let app = app!(d.pool());
    let (cat, room) = (d.category, d.room);
    let (tools, books, garage) = (d.tools, d.books, d.garage);

    let filters = [
        String::new(),
        "&min_price=40".to_string(),
        "&max_price=40".to_string(),
        "&from_date=2023-01-01&to_date=2023-12-31".to_string(),
        format!("&organizers={cat}:{tools}"),
        format!("&organizers={cat}:{tools},{cat}:{books}"),
        format!("&organizers={cat}:{tools},{room}:{garage}"),
        format!("&organizers={cat}:none"),
        format!("&organizers={cat}:{books},{cat}:none"),
        format!("&organizers={cat}:{tools}&min_price=60"),
        format!("&organizers={room}:{garage}&from_date=2022-06-01"),
        // Matches nothing
        "&min_price=1000".to_string(),
    ];

    let grouped = format!("&group_by={cat}");
    for filter in &filters {
        for group in ["", grouped.as_str()] {
            let query = format!("{filter}{group}");
            let (status, report, _) = get!(&app, &d.report(&query), &d.jwt);
            assert_eq!(status, StatusCode::OK, "{query}: {report}");
            let listed = i64::try_from(report["data"]["items"].as_array().unwrap().len()).unwrap();
            let stats = &report["data"]["statistics"];
            assert_eq!(
                stats["total_items"], listed,
                "{query}: statistics.total_items"
            );

            let breakdown = report["data"]["category_breakdown"].as_array().unwrap();
            let in_groups: i64 = breakdown
                .iter()
                .map(|g| g["item_count"].as_i64().unwrap())
                .sum();
            assert_eq!(in_groups, listed, "{query}: breakdown item count");
            let value_in_groups: f64 = breakdown.iter().map(|g| num(&g["total_value"])).sum();
            assert!(
                (value_in_groups - num(&stats["total_value"])).abs() < 1e-6,
                "{query}: breakdown value"
            );
            assert_eq!(
                stats["category_count"],
                i64::try_from(breakdown.len()).unwrap(),
                "{query}: group count"
            );

            // The two single-purpose endpoints answer for the same set
            let (_, only_stats, _) = get!(
                &app,
                &format!(
                    "/api/reports/inventory/statistics?inventory_id={}{query}",
                    d.inv
                ),
                &d.jwt
            );
            assert_eq!(only_stats["data"], *stats, "{query}: statistics endpoint");
            let (_, only_groups, _) = get!(
                &app,
                &format!(
                    "/api/reports/inventory/categories?inventory_id={}{query}",
                    d.inv
                ),
                &d.jwt
            );
            assert_eq!(
                only_groups["data"], report["data"]["category_breakdown"],
                "{query}: categories endpoint"
            );

            // And so does the CSV
            let (status, _, raw) = get!(&app, &d.report(&format!("{query}&format=csv")), &d.jwt);
            assert_eq!(status, StatusCode::OK, "{query}");
            let (_, rows) = csv_rows(&raw);
            assert_eq!(
                i64::try_from(rows.len()).unwrap(),
                listed,
                "{query}: csv rows"
            );
        }
    }
}

#[actix_web::test]
async fn organizer_and_other_filters_select_the_expected_items() {
    let d = seed().await;
    let app = app!(d.pool());
    let (cat, room) = (d.category, d.room);
    let i =
        |ns: &[usize]| -> BTreeSet<i64> { ns.iter().map(|n| i64::from(d.items[n - 1])).collect() };

    let cases: Vec<(String, BTreeSet<i64>)> = vec![
        (String::new(), i(&[1, 2, 3, 4, 5, 6, 7])),
        (format!("&organizers={cat}:{}", d.tools), i(&[1, 2])),
        // Or within one organizer
        (
            format!("&organizers={cat}:{},{cat}:{}", d.tools, d.books),
            i(&[1, 2, 3, 4]),
        ),
        // And across organizers
        (
            format!("&organizers={cat}:{},{room}:{}", d.tools, d.garage),
            i(&[1, 2]),
        ),
        (
            format!("&organizers={cat}:{},{room}:{}", d.books, d.kitchen),
            i(&[4]),
        ),
        (format!("&organizers={room}:{}", d.garage), i(&[1, 2, 7])),
        // "none": no row, or a row whose option was deleted
        (format!("&organizers={cat}:none"), i(&[6, 7])),
        (
            format!("&organizers={cat}:{},{cat}:none", d.books),
            i(&[3, 4, 6, 7]),
        ),
        (format!("&organizers={room}:none"), i(&[3, 5])),
        // Repeated pairs and the order of pairs change nothing
        (
            format!("&organizers={cat}:{0},{cat}:{0}", d.tools),
            i(&[1, 2]),
        ),
        (
            format!("&organizers={room}:{},{cat}:{}", d.garage, d.tools),
            i(&[1, 2]),
        ),
        // Together with the other filters
        (
            format!("&organizers={cat}:{}&min_price=60", d.tools),
            i(&[1]),
        ),
        ("&min_price=40".to_string(), i(&[1, 2, 4])),
        (
            "&from_date=2023-01-01&to_date=2023-12-31".to_string(),
            i(&[1, 2]),
        ),
        ("&min_price=1000".to_string(), BTreeSet::new()),
    ];
    for (query, expected) in cases {
        let (status, report, _) = get!(&app, &d.report(&query), &d.jwt);
        assert_eq!(status, StatusCode::OK, "{query}: {report}");
        assert_eq!(ids(&report), expected, "{query}");
    }
}

// ---------------------------------------------------------------- grouping

#[actix_web::test]
async fn grouping_by_an_organizer_counts_every_item_once_with_a_group_for_no_value() {
    let d = seed().await;
    let app = app!(d.pool());
    let (status, report, _) = get!(
        &app,
        &d.report(&format!("&group_by={}", d.category)),
        &d.jwt
    );
    assert_eq!(status, StatusCode::OK, "{report}");

    assert_eq!(report["data"]["group_by"]["organizer_type_id"], d.category);
    assert_eq!(report["data"]["group_by"]["name"], "Category");

    let groups = report["data"]["category_breakdown"].as_array().unwrap();
    let rows: Vec<(String, Option<i64>, i64, i64, f64)> = groups
        .iter()
        .map(|g| {
            (
                g["category"].as_str().unwrap().to_string(),
                g["organizer_option_id"].as_i64(),
                g["item_count"].as_i64().unwrap(),
                g["total_quantity"].as_i64().unwrap(),
                num(&g["total_value"]),
            )
        })
        .collect();
    // Value, largest first. A NULL quantity counts as 1 (Atlas: 40 x 1), a NULL price as no value
    // (Blocks), and the Mystery and the Ghost (deleted option) fall into "No value".
    assert_eq!(
        rows,
        vec![
            ("Tools".to_string(), Some(i64::from(d.tools)), 2, 3, 250.0),
            ("Books".to_string(), Some(i64::from(d.books)), 2, 4, 100.0),
            ("No value".to_string(), None, 2, 2, 40.0),
            ("Toys".to_string(), Some(i64::from(d.toys)), 1, 4, 0.0),
        ]
    );

    // Shares of the total value: exact, summing to 100 (no rounding in the API)
    let shares: Vec<f64> = groups
        .iter()
        .map(|g| num(&g["percentage_of_total"]))
        .collect();
    assert!(
        (shares.iter().sum::<f64>() - 100.0).abs() < 1e-9,
        "{shares:?}"
    );
    assert!((shares[0] - 250.0 / 390.0 * 100.0).abs() < 1e-9);
    assert_eq!(shares[3], 0.0);

    // The statistics use the same rules: NULL quantity is 1, the average is over priced items
    let stats = &report["data"]["statistics"];
    assert_eq!(stats["total_items"], 7);
    assert_eq!(stats["total_quantity"], 13);
    assert!((num(&stats["total_value"]) - 390.0).abs() < 1e-9);
    assert!((num(&stats["average_item_value"]) - 250.0 / 6.0).abs() < 1e-9);
    assert_eq!(stats["category_count"], 4);
}

#[actix_web::test]
async fn a_renamed_option_shows_its_new_name_and_a_deleted_one_means_no_value() {
    let d = seed().await;
    let app = app!(d.pool());
    exec(
        d.pool(),
        "UPDATE organizer_options SET name = 'Hardware' WHERE id = $1",
        &[&d.tools],
    )
    .await;

    let (_, report, _) = get!(
        &app,
        &d.report(&format!("&group_by={}", d.category)),
        &d.jwt
    );
    let labels: Vec<&str> = report["data"]["category_breakdown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["category"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["Hardware", "Books", "No value", "Toys"]);

    // Deleting an option leaves its items without a value
    exec(
        d.pool(),
        "DELETE FROM organizer_options WHERE id = $1",
        &[&d.books],
    )
    .await;
    let (_, report, _) = get!(
        &app,
        &d.report(&format!("&group_by={}", d.category)),
        &d.jwt
    );
    let no_value = report["data"]["category_breakdown"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["category"] == "No value")
        .unwrap()
        .clone();
    assert_eq!(no_value["item_count"], 4);
    let (_, report, _) = get!(
        &app,
        &d.report(&format!("&organizers={}:none", d.category)),
        &d.jwt
    );
    assert_eq!(report["data"]["items"].as_array().unwrap().len(), 4);
}

#[actix_web::test]
async fn an_item_has_at_most_one_value_per_organizer() {
    let d = seed().await;
    // The grouping relies on this: a second value for the same item and organizer is refused
    let again = d
        .pool()
        .get()
        .await
        .unwrap()
        .execute(
            "INSERT INTO item_organizer_values (item_id, organizer_type_id, organizer_option_id)
             VALUES ($1, $2, $3)",
            &[&d.items[0], &d.category, &d.books],
        )
        .await;
    assert!(again.is_err());
}

#[actix_web::test]
async fn all_zero_values_give_zero_shares_and_no_items_give_no_groups() {
    let d = seed().await;
    let app = app!(d.pool());
    let (_, report, _) = get!(
        &app,
        &d.report(&format!(
            "&group_by={}&organizers={}:{}",
            d.category, d.category, d.toys
        )),
        &d.jwt
    );
    let groups = report["data"]["category_breakdown"].as_array().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["category"], "Toys");
    assert_eq!(num(&groups[0]["percentage_of_total"]), 0.0);

    let (_, report, _) = get!(
        &app,
        &d.report(&format!("&group_by={}&min_price=1000", d.category)),
        &d.jwt
    );
    assert_eq!(
        report["data"]["category_breakdown"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(report["data"]["statistics"]["total_items"], 0);
    assert_eq!(
        num(&report["data"]["statistics"]["average_item_value"]),
        0.0
    );
}

#[actix_web::test]
async fn items_carry_their_organizer_values_but_not_images() {
    let d = seed().await;
    let app = app!(d.pool());
    let (_, report, _) = get!(&app, &d.report("&sort_by=name&sort_order=asc"), &d.jwt);
    let drill = report["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "Drill")
        .unwrap();
    let values = &drill["organizer_values"];
    assert_eq!(values[d.category.to_string()], "Tools");
    assert_eq!(values[d.room.to_string()], "Garage");
    assert_eq!(values[d.serial.to_string()], "SN-1");
    assert!(values.get(d.photo.to_string()).is_none());
    // An item without a value has no entry for that organizer
    let blocks = report["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "Blocks")
        .unwrap();
    assert!(blocks["organizer_values"].get(d.room.to_string()).is_none());
}

#[actix_web::test]
async fn without_group_by_the_legacy_category_breakdown_stays_but_follows_the_filters() {
    let d = seed().await;
    let app = app!(d.pool());
    let other = common::create_test_inventory(d.pool(), d.user, "Legacy")
        .await
        .unwrap();
    for (name, category, price) in [
        ("A", Some("Tools"), 10.0),
        ("B", Some(""), 20.0),
        ("C", None, 30.0),
        ("D", Some("Tools"), 40.0),
    ] {
        let id = item(d.pool(), other, name, Some(price), Some(1), None).await;
        exec(
            d.pool(),
            "UPDATE items SET category = $1 WHERE id = $2",
            &[&category, &id],
        )
        .await;
    }

    let (_, report, _) = get!(
        &app,
        &format!("/api/reports/inventory?inventory_id={other}"),
        &d.jwt
    );
    let groups: Vec<(String, i64)> = report["data"]["category_breakdown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            (
                g["category"].as_str().unwrap().to_string(),
                g["item_count"].as_i64().unwrap(),
            )
        })
        .collect();
    // Tools 50, Uncategorized 50 (the empty string and NULL are one group); ties by name
    assert_eq!(
        groups,
        vec![("Tools".to_string(), 2), ("Uncategorized".to_string(), 2)]
    );
    assert!(report["data"]["group_by"].is_null());

    let (_, report, _) = get!(
        &app,
        &format!("/api/reports/inventory?inventory_id={other}&min_price=25"),
        &d.jwt
    );
    let total: i64 = report["data"]["category_breakdown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["item_count"].as_i64().unwrap())
        .sum();
    assert_eq!(total, 2, "the breakdown follows the filters");
}

// ---------------------------------------------------------------- strict parameters

#[actix_web::test]
async fn bad_organizer_parameters_are_refused_without_echoing_them() {
    let d = seed().await;
    let app = app!(d.pool());
    let sentinel = "SENTINEL-8e51";
    let other = common::create_test_inventory(d.pool(), d.user, "Other")
        .await
        .unwrap();
    let foreign_type = organizer(d.pool(), other, "Foreign", "select", 1).await;
    let foreign_option = option(d.pool(), foreign_type, "F").await;
    let (cat, room, tools) = (d.category, d.room, d.tools);

    let many: String = (1..=51)
        .map(|n| format!("{cat}:{n}"))
        .collect::<Vec<_>>()
        .join(",");
    let bad: Vec<(String, &str)> = vec![
        // Not the syntax
        (format!("&organizers={sentinel}"), "malformed"),
        (format!("&organizers={cat}"), "malformed"),
        (format!("&organizers={cat}:"), "malformed"),
        (format!("&organizers=:{tools}"), "malformed"),
        (format!("&organizers={cat}:{tools}:{tools}"), "malformed"),
        (format!("&organizers={cat}:{tools},"), "malformed"),
        (
            format!("&organizers={cat}:{tools},,{room}:{}", d.garage),
            "malformed",
        ),
        (format!("&organizers={cat}:-1"), "malformed"),
        (format!("&organizers={cat}:0"), "malformed"),
        (format!("&organizers={cat}:NONE"), "malformed"),
        (format!("&organizers={cat}:99999999999"), "malformed"),
        (format!("&organizers={cat}:{tools}%20"), "malformed"),
        (format!("&organizers={cat}:1.5"), "malformed"),
        ("&organizers=%00".to_string(), "malformed"),
        (format!("&organizers={many}"), "too many"),
        // Valid syntax, wrong target
        ("&organizers=99999:1".to_string(), "unknown"),
        (format!("&organizers={cat}:99999"), "unknown"),
        (
            format!("&organizers={foreign_type}:{foreign_option}"),
            "other inventory",
        ),
        (
            format!("&organizers={cat}:{foreign_option}"),
            "option of another organizer",
        ),
        (
            format!("&organizers={room}:{tools}"),
            "option of another organizer",
        ),
        (format!("&organizers={}:none", d.serial), "text organizer"),
        (format!("&organizers={}:none", d.photo), "image organizer"),
        (format!("&group_by={}", d.serial), "text organizer"),
        (format!("&group_by={}", d.photo), "image organizer"),
        (format!("&group_by={foreign_type}"), "other inventory"),
        ("&group_by=99999".to_string(), "unknown"),
        ("&group_by=0".to_string(), "unknown"),
        ("&group_by=-3".to_string(), "unknown"),
    ];
    for (query, why) in &bad {
        for path in [
            "/api/reports/inventory",
            "/api/reports/inventory/statistics",
            "/api/reports/inventory/categories",
        ] {
            let (status, body, raw) = get!(
                &app,
                &format!("{path}?inventory_id={}{query}", d.inv),
                &d.jwt
            );
            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "{path}{query} ({why}): {raw}"
            );
            assert_eq!(body["success"], false);
            assert!(body["error"].is_string(), "{raw}");
            assert!(
                !raw.contains(sentinel),
                "received value echoed for {query}: {raw}"
            );
        }
    }

    // Organizer parameters need an inventory to mean anything
    for query in [
        format!("organizers={cat}:{tools}"),
        format!("group_by={cat}"),
    ] {
        let (status, _, raw) = get!(&app, &format!("/api/reports/inventory?{query}"), &d.jwt);
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {raw}");
    }

    // The messages are fixed text
    let (_, a, _) = get!(&app, &d.report(&format!("&organizers={sentinel}")), &d.jwt);
    let (_, b, _) = get!(&app, &d.report("&organizers=other-garbage"), &d.jwt);
    assert_eq!(a, b);
}

#[actix_web::test]
async fn invalid_dates_and_ranges_are_refused_on_every_report_endpoint() {
    let d = seed().await;
    let app = app!(d.pool());
    for query in [
        "&from_date=not-a-date",
        "&to_date=2024-13-40",
        "&from_date=2024-02-01&to_date=2024-01-01",
        "&min_price=9&max_price=1",
    ] {
        for path in [
            "/api/reports/inventory",
            "/api/reports/inventory/statistics",
            "/api/reports/inventory/categories",
        ] {
            let (status, _, raw) = get!(
                &app,
                &format!("{path}?inventory_id={}{query}", d.inv),
                &d.jwt
            );
            assert_eq!(status, StatusCode::BAD_REQUEST, "{path}{query}: {raw}");
        }
    }
}

// ---------------------------------------------------------------- access

#[actix_web::test]
async fn a_stranger_is_refused_and_a_view_only_share_can_report() {
    let d = seed().await;
    let app = app!(d.pool());
    let stranger = common::test_username("stranger");
    common::create_test_user(d.pool(), &stranger).await;
    let stranger_jwt = common::get_test_token(d.pool(), &stranger).await;
    let stranger_id: Uuid = d
        .pool()
        .get()
        .await
        .unwrap()
        .query_one("SELECT id FROM users WHERE username = $1", &[&stranger])
        .await
        .unwrap()
        .get(0);

    let cat = d.category;
    for path in [
        "/api/reports/inventory",
        "/api/reports/inventory/statistics",
        "/api/reports/inventory/categories",
    ] {
        let queries = [
            String::new(),
            format!("&group_by={cat}"),
            format!("&organizers={cat}:none"),
        ];
        for query in &queries {
            let (status, _, raw) = get!(
                &app,
                &format!("{path}?inventory_id={}{query}", d.inv),
                &stranger_jwt
            );
            assert_eq!(status, StatusCode::FORBIDDEN, "{path}{query}: {raw}");
            // Nothing about the inventory's organizers leaks in the refusal
            assert!(!raw.contains("Category"), "{raw}");
        }
    }

    // Without an inventory the report covers only what the user can see: nothing here
    let (status, body, _) = get!(&app, "/api/reports/inventory", &stranger_jwt);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 0);

    exec(
        d.pool(),
        "INSERT INTO inventory_shares (inventory_id, shared_with_user_id, shared_by_user_id, permission_level)
         VALUES ($1, $2, $3, 'view')",
        &[&d.inv, &stranger_id, &d.user],
    )
    .await;
    let (status, body, raw) = get!(&app, &d.report(&format!("&group_by={cat}")), &stranger_jwt);
    assert_eq!(status, StatusCode::OK, "{raw}");
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 7);
    assert_eq!(
        body["data"]["category_breakdown"].as_array().unwrap().len(),
        4
    );
}

// ---------------------------------------------------------------- CSV

#[actix_web::test]
async fn the_csv_has_a_column_per_organizer_and_no_legacy_columns_without_legacy_data() {
    let d = seed().await;
    let app = app!(d.pool());
    let (status, _, raw) = get!(
        &app,
        &d.report("&format=csv&sort_by=name&sort_order=asc"),
        &d.jwt
    );
    assert_eq!(status, StatusCode::OK);
    let (header, rows) = csv_rows(&raw);
    assert_eq!(
        header,
        [
            "ID",
            "Inventory",
            "Name",
            "Description",
            "Category",
            "Room",
            "Serial",
            "Quantity",
            "Purchase Price",
            "Total Value",
            "Purchase Date",
            "Warranty Expiry",
            "Created At"
        ]
    );
    let by_name = |name: &str| rows.iter().find(|r| r[2] == name).unwrap().clone();
    let drill = by_name("Drill");
    assert_eq!(&drill[4..7], ["Tools", "Garage", "SN-1"]);
    assert_eq!(drill[7], "2");
    assert_eq!(drill[9], "200.00");
    // NULL quantity counts as 1; NULL price leaves the total empty (and the price)
    assert_eq!(by_name("Atlas")[9], "40.00");
    let blocks = by_name("Blocks");
    assert_eq!((blocks[8].as_str(), blocks[9].as_str()), ("", ""));
    // The Ghost's deleted option and the Mystery's missing value are empty cells
    assert_eq!(by_name("Ghost")[4], "");
    assert_eq!(by_name("Mystery")[4], "");
}

#[actix_web::test]
async fn legacy_columns_appear_only_when_items_still_hold_legacy_values_and_never_clash() {
    let d = seed().await;
    let app = app!(d.pool());
    exec(
        d.pool(),
        "UPDATE items SET category = 'OldCat' WHERE id = $1",
        &[&d.items[0]],
    )
    .await;
    let (_, _, raw) = get!(
        &app,
        &d.report("&format=csv&sort_by=name&sort_order=asc"),
        &d.jwt
    );
    let (header, rows) = csv_rows(&raw);
    assert!(header.contains(&"Legacy category".to_string()));
    assert!(!header.contains(&"Legacy location".to_string()));
    let at = header.iter().position(|h| h == "Legacy category").unwrap();
    assert_eq!(rows.iter().find(|r| r[2] == "Drill").unwrap()[at], "OldCat");
    assert_eq!(rows.iter().find(|r| r[2] == "Saw").unwrap()[at], "");

    // An organizer that already has that name does not produce two equal headers
    organizer(d.pool(), d.inv, "Legacy category", "text", 9).await;
    let (_, _, raw) = get!(&app, &d.report("&format=csv"), &d.jwt);
    let (header, _) = csv_rows(&raw);
    let unique: BTreeSet<String> = header.iter().map(|h| h.to_lowercase()).collect();
    assert_eq!(unique.len(), header.len(), "{header:?}");
}

#[actix_web::test]
async fn csv_quotes_special_characters_and_neutralises_formulas_in_text_cells_only() {
    let d = seed().await;
    let app = app!(d.pool());
    let tricky = organizer(d.pool(), d.inv, "=Formula, \"quoted\"", "text", 5).await;

    let mut ids = Vec::new();
    for (name, description) in [
        ("=SUM(A1)", "+1"),
        ("-2", "@cmd"),
        ("\tTab", "\rReturn"),
        ("\nLine", "a\nb, \"c\""),
        ("＝fullwidth", "＋＠－"),
        ("Plain", "safe = text - with @ signs"),
    ] {
        let id = item(d.pool(), d.inv, name, Some(5.0), Some(1), None).await;
        exec(
            d.pool(),
            "UPDATE items SET description = $1 WHERE id = $2",
            &[&description, &id],
        )
        .await;
        value(d.pool(), id, tricky, None, Some("=HYPERLINK(\"x\")")).await;
        ids.push(id);
    }
    let (_, _, raw) = get!(&app, &d.report("&format=csv"), &d.jwt);
    let (header, rows) = csv_rows(&raw);
    let name_at = header.iter().position(|h| h == "Name").unwrap();
    let desc_at = header.iter().position(|h| h == "Description").unwrap();
    let organizer_at = header.iter().position(|h| h.contains("Formula")).unwrap();
    // The header of an organizer is text too
    assert!(
        header[organizer_at].starts_with('\''),
        "{:?}",
        header[organizer_at]
    );

    let row = |id: i32| {
        rows.iter()
            .find(|r| r[0] == id.to_string())
            .unwrap()
            .clone()
    };
    assert_eq!(row(ids[0])[name_at], "'=SUM(A1)");
    assert_eq!(row(ids[0])[desc_at], "'+1");
    assert_eq!(row(ids[1])[name_at], "'-2");
    assert_eq!(row(ids[1])[desc_at], "'@cmd");
    assert_eq!(row(ids[2])[name_at], "'\tTab");
    assert_eq!(row(ids[2])[desc_at], "'\rReturn");
    assert_eq!(row(ids[3])[name_at], "'\nLine");
    assert_eq!(
        row(ids[3])[desc_at],
        "a\nb, \"c\"",
        "embedded characters are quoted, not changed"
    );
    assert_eq!(row(ids[4])[name_at], "'＝fullwidth");
    assert_eq!(row(ids[4])[desc_at], "'＋＠－");
    assert_eq!(row(ids[5])[name_at], "Plain");
    assert_eq!(row(ids[5])[desc_at], "safe = text - with @ signs");
    assert_eq!(row(ids[0])[organizer_at], "'=HYPERLINK(\"x\")");

    // Numbers are untouched
    let price_at = header.iter().position(|h| h == "Purchase Price").unwrap();
    assert_eq!(row(ids[0])[price_at], "5.00");
}
