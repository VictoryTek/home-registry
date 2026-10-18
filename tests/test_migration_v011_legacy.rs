// What migration V011 did with the legacy `items.category` / `items.location` values, which is what
// decides whether the CSV export still needs "Legacy category" / "Legacy location" columns.
//
// V011 turned the values of each inventory into a "Category" / "Location" select organizer, but
// only where no organizer of that name existed yet; in an inventory that already had one it
// skipped the values. Nothing stops an API client from writing the legacy columns afterwards.
// So a database can still hold legacy values that no organizer carries; the report keeps them
// visible in the CSV instead of dropping them.

mod common;

use common::ScratchDb;

#[actix_web::test]
async fn v011_migrates_the_usual_case_and_skips_inventories_that_already_had_the_organizer() {
    let mut scratch = ScratchDb::create_up_to(10).await;
    let client = scratch.pool.get().await.unwrap();

    let inventory = |name: &'static str| {
        let client = &client;
        async move {
            let id: i32 = client
                .query_one(
                    "INSERT INTO inventories (name) VALUES ($1) RETURNING id",
                    &[&name],
                )
                .await
                .unwrap()
                .get(0);
            id
        }
    };
    let add_item = |inventory: i32,
                    name: &'static str,
                    category: Option<&'static str>,
                    location: Option<&'static str>| {
        let client = &client;
        async move {
            client
                .execute(
                    "INSERT INTO items (inventory_id, name, category, location) VALUES ($1, $2, $3, $4)",
                    &[&inventory, &name, &category, &location],
                )
                .await
                .unwrap();
        }
    };

    // A: no organizers yet. Its legacy values are migrated.
    let a = inventory("A").await;
    add_item(a, "Drill", Some("Tools"), Some("Garage")).await;
    add_item(a, "Saw", Some("Tools"), None).await;
    add_item(a, "Blank", Some(""), None).await;

    // B: already had an organizer called "Category" before V011 ran. Its values are skipped.
    let b = inventory("B").await;
    client
        .execute(
            "INSERT INTO organizer_types (inventory_id, name, input_type) VALUES ($1, 'Category', 'select')",
            &[&b],
        )
        .await
        .unwrap();
    add_item(b, "Novel", Some("Books"), None).await;
    add_item(b, "Atlas", Some("Books"), Some("Shelf")).await;
    drop(client);

    scratch.apply_migration(11).await;
    let client = scratch.pool.get().await.unwrap();

    // Items with a non-empty legacy value that no organizer value carries
    let uncovered = |column: &'static str, organizer: &'static str| {
        let client = &client;
        async move {
            let sql = format!(
                "SELECT COUNT(*) FROM items i
                 WHERE NULLIF(i.{column}, '') IS NOT NULL
                   AND NOT EXISTS (
                       SELECT 1 FROM item_organizer_values v
                       JOIN organizer_types t ON t.id = v.organizer_type_id
                       WHERE v.item_id = i.id AND t.name = '{organizer}'
                         AND v.organizer_option_id IS NOT NULL)"
            );
            let n: i64 = client.query_one(sql.as_str(), &[]).await.unwrap().get(0);
            n
        }
    };
    let with_value = |column: &'static str| {
        let client = &client;
        async move {
            let n: i64 = client
                .query_one(
                    format!("SELECT COUNT(*) FROM items WHERE NULLIF({column}, '') IS NOT NULL")
                        .as_str(),
                    &[],
                )
                .await
                .unwrap()
                .get(0);
            n
        }
    };

    assert_eq!(
        with_value("category").await,
        4,
        "legacy categories before and after: untouched"
    );
    assert_eq!(with_value("location").await, 2);
    // A's two categories and one location were migrated; B's two categories were not, and B's
    // Atlas location was migrated into a new "Location" organizer
    assert_eq!(uncovered("category", "Category").await, 2);
    assert_eq!(uncovered("location", "Location").await, 0);
}
