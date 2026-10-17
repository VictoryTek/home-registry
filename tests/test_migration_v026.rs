// Migration V026 removes six unused tables and items.category_id. Each test builds its own
// scratch database at the state just before it (V025), seeds the old objects, and applies it.

mod common;

use common::ScratchDb;
use uuid::Uuid;

/// Rows in the old tables and a category on an item, plus ordinary data that must survive.
struct Seeded {
    item_id: i32,
}

async fn seed_legacy(pool: &deadpool_postgres::Pool) -> Seeded {
    let username = common::test_username("legacy");
    common::create_test_user(pool, &username).await;
    let user_id: Uuid = pool
        .get()
        .await
        .unwrap()
        .query_one("SELECT id FROM users WHERE username = $1", &[&username])
        .await
        .unwrap()
        .get(0);
    let inventory = common::create_test_inventory(pool, user_id, "Legacy")
        .await
        .unwrap();
    let item_id = common::create_test_item(pool, inventory, "Hammer")
        .await
        .unwrap();

    let client = pool.get().await.unwrap();
    let category: i32 = client
        .query_one(
            "INSERT INTO categories (name) VALUES ('Tools') RETURNING id",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    client
        .execute(
            "UPDATE items SET category_id = $1, category = 'Tools (text)' WHERE id = $2",
            &[&category, &item_id],
        )
        .await
        .unwrap();
    let tag: i32 = client
        .query_one("INSERT INTO tags (name) VALUES ('heavy') RETURNING id", &[])
        .await
        .unwrap()
        .get(0);
    client
        .execute(
            "INSERT INTO item_tags (item_id, tag_id) VALUES ($1, $2)",
            &[&item_id, &tag],
        )
        .await
        .unwrap();
    let field: i32 = client
        .query_one(
            "INSERT INTO custom_fields (category_id, name) VALUES ($1, 'Weight') RETURNING id",
            &[&category],
        )
        .await
        .unwrap()
        .get(0);
    client
        .execute(
            "INSERT INTO item_custom_values (item_id, custom_field_id, value) VALUES ($1, $2, '2kg')",
            &[&item_id, &field],
        )
        .await
        .unwrap();
    client
        .execute(
            "INSERT INTO password_reset_tokens (user_id, token) VALUES ($1, 'tok')",
            &[&user_id],
        )
        .await
        .unwrap();
    Seeded { item_id }
}

async fn exists(pool: &deadpool_postgres::Pool, table: &str) -> bool {
    pool.get()
        .await
        .unwrap()
        .query_one(
            "SELECT to_regclass($1::text) IS NOT NULL",
            &[&format!("public.{table}")],
        )
        .await
        .unwrap()
        .get(0)
}

async fn items_has_column(pool: &deadpool_postgres::Pool, column: &str) -> bool {
    pool.get()
        .await
        .unwrap()
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM information_schema.columns
                            WHERE table_schema = 'public' AND table_name = 'items'
                              AND column_name = $1)",
            &[&column],
        )
        .await
        .unwrap()
        .get(0)
}

const DROPPED: [&str; 6] = [
    "categories",
    "tags",
    "item_tags",
    "custom_fields",
    "item_custom_values",
    "password_reset_tokens",
];

#[actix_web::test]
async fn the_migration_removes_the_unused_objects_and_keeps_everything_else() {
    let mut scratch = ScratchDb::create_up_to(25).await;
    let seeded = seed_legacy(&scratch.pool).await;
    for table in DROPPED {
        assert!(
            exists(&scratch.pool, table).await,
            "{table} exists before V026"
        );
    }
    assert!(items_has_column(&scratch.pool, "category_id").await);

    scratch.apply_migration(26).await;

    for table in DROPPED {
        assert!(!exists(&scratch.pool, table).await, "{table} is gone");
    }
    assert!(!items_has_column(&scratch.pool, "category_id").await);
    let client = scratch.pool.get().await.unwrap();
    let leftovers: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM pg_constraint WHERE conname = 'fk_items_category'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(leftovers, 0, "the foreign key is gone");
    let index: bool = client
        .query_one(
            "SELECT to_regclass('public.idx_items_category_id') IS NOT NULL",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!index);

    // The live text column and the item itself are untouched
    let row = client
        .query_one(
            "SELECT name::text, category::text FROM items WHERE id = $1",
            &[&seeded.item_id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>(0), "Hammer");
    assert_eq!(row.get::<_, String>(1), "Tools (text)");
    assert!(items_has_column(&scratch.pool, "category").await);
    // The access view (V025) does not depend on anything that was dropped
    client
        .query_one("SELECT COUNT(*) FROM inventory_access", &[])
        .await
        .expect("inventory_access still works");
}

#[actix_web::test]
async fn applying_the_migration_twice_is_harmless() {
    let mut scratch = ScratchDb::create_up_to(25).await;
    scratch.apply_migration(26).await;

    let sql = std::fs::read_to_string("migrations/V026__drop_unused_legacy_tables.sql").unwrap();
    scratch
        .pool
        .get()
        .await
        .unwrap()
        .batch_execute(&sql)
        .await
        .expect("re-applying V026 does nothing");
}

#[actix_web::test]
async fn an_unexpected_dependency_makes_the_migration_fail_without_changing_anything() {
    let scratch = ScratchDb::create_up_to(25).await;
    seed_legacy(&scratch.pool).await;
    let client = scratch.pool.get().await.unwrap();
    // Something outside the migration's knowledge depends on one of the tables
    client
        .batch_execute("CREATE VIEW someone_elses_report AS SELECT id, name FROM tags")
        .await
        .unwrap();

    let sql = std::fs::read_to_string("migrations/V026__drop_unused_legacy_tables.sql").unwrap();
    let err = client
        .batch_execute(&sql)
        .await
        .expect_err("no CASCADE, so this must fail");
    let db_error = err.as_db_error().expect("a database error");
    let detail = format!("{db_error} {:?}", db_error.detail());
    assert!(detail.contains("someone_elses_report"), "{detail}");

    // All or nothing: even the statements before the failing DROP were rolled back
    for table in DROPPED {
        assert!(
            exists(&scratch.pool, table).await,
            "{table} must still exist"
        );
    }
    assert!(items_has_column(&scratch.pool, "category_id").await);
}
