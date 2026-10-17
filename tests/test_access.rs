// The inventory_access view is the single definition of inventory access. These tests build
// every relationship a user can have to an inventory and check that all consumers agree:
// effective permissions, the accessible-inventory list, the "can see" check and the report
// queries. Requires the Postgres test database (see tests/common).

mod common;

use actix_web::{http::StatusCode, test, web, App};
use home_registry::api;
use home_registry::db::DatabaseService;
use home_registry::models::{InventoryReportRequest, PermissionLevel, PermissionSource};
use std::collections::BTreeSet;
use uuid::Uuid;

/// What a user is expected to hold on one inventory:
/// (source, edit items, add items, edit inventory, manage organizers, delete, manage sharing)
type Expect = (PermissionSource, bool, bool, bool, bool, bool, bool);

const FULL_OWNER: Expect = (PermissionSource::Owner, true, true, true, true, true, true);
const FULL_ALL_ACCESS: Expect = (
    PermissionSource::AllAccess,
    true,
    true,
    true,
    true,
    true,
    true,
);
const SHARE_VIEW: Expect = (
    PermissionSource::InventoryShare,
    false,
    false,
    false,
    false,
    false,
    false,
);
const SHARE_EDIT_ITEMS: Expect = (
    PermissionSource::InventoryShare,
    true,
    false,
    false,
    false,
    false,
    false,
);
const SHARE_MANAGER: Expect = (
    PermissionSource::InventoryShare,
    true,
    true,
    true,
    true,
    false,
    false,
);

/// Serialises the tests that run DDL. Each takes ACCESS EXCLUSIVE on one base table of the
/// view and then queries the view (which needs the others), so two of them overlapping can
/// deadlock. Taking this advisory lock first, inside the transaction, lets only one hold
/// table locks at a time; ordinary readers elsewhere only wait and cannot form a cycle.
const DDL_TEST_LOCK: i64 = 0x01D5_0025;

async fn lock_for_ddl(tx: &deadpool_postgres::Transaction<'_>) {
    tx.execute("SELECT pg_advisory_xact_lock($1)", &[&DDL_TEST_LOCK])
        .await
        .unwrap();
    tx.batch_execute("SET LOCAL lock_timeout = '30s'")
        .await
        .unwrap();
}

struct Fixture {
    db: DatabaseService,
    /// (label, user id)
    users: Vec<(&'static str, Uuid)>,
    inv_main: i32,
    inv_self_granted: i32,
    inv_ownerless: i32,
    names: Vec<String>,
}

async fn user_id(db: &DatabaseService, name: &str) -> Uuid {
    db.get_user_by_username(name).await.unwrap().unwrap().id
}

async fn build_fixture(pool: &deadpool_postgres::Pool) -> Fixture {
    let db = DatabaseService::new(pool.clone());
    let labels = [
        "owner",
        "allaccess",
        "viewer",
        "editor",
        "manager",
        "both",
        "stranger",
        "otherowner",
        "inactive",
    ];
    let mut names = Vec::new();
    let mut users = Vec::new();
    for label in labels {
        let name = common::test_username(label);
        common::create_test_user(pool, &name).await;
        users.push((label, user_id(&db, &name).await));
        names.push(name);
    }
    let id = |label: &str| users.iter().find(|(l, _)| *l == label).unwrap().1;

    // inv_main: owned by `owner`
    let inv_main = common::create_test_inventory(pool, id("owner"), "Main")
        .await
        .unwrap();
    // inv_other: owned by a second owner who has no sharing at all
    let inv_self_granted = common::create_test_inventory(pool, id("otherowner"), "Other Owner")
        .await
        .unwrap();
    // inv_ownerless: no owner at all
    let client = pool.get().await.unwrap();
    let inv_ownerless: i32 = client
        .query_one(
            "INSERT INTO inventories (name, user_id) VALUES ('Ownerless', NULL) RETURNING id",
            &[],
        )
        .await
        .unwrap()
        .get(0);

    // All Access from `owner`
    db.create_user_access_grant(id("owner"), id("allaccess"))
        .await
        .unwrap();
    db.create_user_access_grant(id("owner"), id("both"))
        .await
        .unwrap();
    // Per-inventory shares on inv_main
    for (label, level) in [
        ("viewer", PermissionLevel::View),
        ("editor", PermissionLevel::EditItems),
        ("manager", PermissionLevel::EditInventory),
        ("both", PermissionLevel::View), // also All Access: All Access must win
        ("inactive", PermissionLevel::View),
    ] {
        db.create_inventory_share(inv_main, id(label), id("owner"), level)
            .await
            .unwrap();
    }
    // A share on the ownerless inventory
    db.create_inventory_share(
        inv_ownerless,
        id("viewer"),
        id("owner"),
        PermissionLevel::View,
    )
    .await
    .unwrap();

    // One item per inventory for the report queries
    for (inv, label) in [
        (inv_main, "main"),
        (inv_self_granted, "selfgranted"),
        (inv_ownerless, "ownerless"),
    ] {
        common::create_test_item(pool, inv, &format!("access-matrix-{label}"))
            .await
            .unwrap();
    }
    // Deactivate this user after the data is in place
    client
        .execute(
            "UPDATE users SET is_active = false WHERE id = $1",
            &[&id("inactive")],
        )
        .await
        .unwrap();

    Fixture {
        db,
        users,
        inv_main,
        inv_self_granted,
        inv_ownerless,
        names,
    }
}

async fn cleanup(pool: &deadpool_postgres::Pool, f: &Fixture) {
    for inv in [f.inv_main, f.inv_self_granted, f.inv_ownerless] {
        // Items cascade with the inventory
        common::delete_test_inventory(pool, inv).await.ok();
    }
    for name in &f.names {
        common::delete_test_user(pool, name).await.ok();
    }
}

fn expectations(f: &Fixture) -> Vec<(&'static str, Vec<(i32, Expect)>)> {
    let (m, s, n) = (f.inv_main, f.inv_self_granted, f.inv_ownerless);
    vec![
        ("owner", vec![(m, FULL_OWNER)]),
        ("allaccess", vec![(m, FULL_ALL_ACCESS)]),
        ("viewer", vec![(m, SHARE_VIEW), (n, SHARE_VIEW)]),
        ("editor", vec![(m, SHARE_EDIT_ITEMS)]),
        ("manager", vec![(m, SHARE_MANAGER)]),
        // All Access + a share: All Access wins, with its full capabilities
        ("both", vec![(m, FULL_ALL_ACCESS)]),
        ("stranger", vec![]),
        ("otherowner", vec![(s, FULL_OWNER)]),
        // Deactivation is enforced at authentication, not by the access rule
        ("inactive", vec![(m, SHARE_VIEW)]),
    ]
}

fn flags(p: &home_registry::models::EffectivePermissions) -> Expect {
    (
        p.permission_source.clone(),
        p.can_edit_items,
        p.can_add_items,
        p.can_edit_inventory,
        p.can_manage_organizers,
        p.can_delete_inventory,
        p.can_manage_sharing,
    )
}

#[actix_web::test]
async fn every_access_path_agrees_for_every_relationship() {
    let pool = common::create_test_pool();
    let f = build_fixture(&pool).await;
    let all_inventories = [f.inv_main, f.inv_self_granted, f.inv_ownerless];

    for (label, expected) in expectations(&f) {
        let uid = f.users.iter().find(|(l, _)| *l == label).unwrap().1;
        let expected_ids: BTreeSet<i32> = expected.iter().map(|(inv, _)| *inv).collect();

        // 1. Effective permissions per inventory: exact capabilities, and no access elsewhere
        for inv in all_inventories {
            let p = f.db.get_effective_permissions(uid, inv).await.unwrap();
            if let Some((_, want)) = expected.iter().find(|(i, _)| *i == inv) {
                assert!(p.can_view, "{label} should see inventory {inv}");
                assert_eq!(&flags(&p), want, "{label} on inventory {inv}");
                assert_eq!(p.is_owner, want.0 == PermissionSource::Owner, "{label}");
                assert_eq!(p.has_all_access, want.0 == PermissionSource::AllAccess);
            } else {
                assert!(!p.can_view, "{label} must not see inventory {inv}");
                assert_eq!(p.permission_source, PermissionSource::None, "{label}");
                assert!(!p.can_edit_items && !p.can_delete_inventory, "{label}");
            }
        }

        // 2. The accessible list: exactly the expected inventories, each listed once
        let listed: Vec<i32> =
            f.db.get_accessible_inventories(uid)
                .await
                .unwrap()
                .iter()
                .filter_map(|i| i.id)
                .filter(|id| all_inventories.contains(id))
                .collect();
        assert_eq!(
            listed.len(),
            expected_ids.len(),
            "{label}: no duplicate listing"
        );
        assert_eq!(
            listed.iter().copied().collect::<BTreeSet<_>>(),
            expected_ids,
            "{label}: accessible inventories"
        );

        // 3. The "can see" check agrees with the list for every inventory
        for inv in all_inventories {
            assert_eq!(
                f.db.check_inventory_access(uid, inv).await.unwrap(),
                expected_ids.contains(&inv),
                "{label}: check_inventory_access({inv})"
            );
        }

        // 4. Report scoping returns items from exactly the same inventories
        let request: InventoryReportRequest =
            serde_json::from_value(serde_json::json!({})).expect("empty report request");
        let reported: BTreeSet<i32> =
            f.db.get_inventory_report_data(request, uid)
                .await
                .unwrap()
                .iter()
                .map(|item| item.inventory_id)
                .filter(|id| all_inventories.contains(id))
                .collect();
        assert_eq!(reported, expected_ids, "{label}: report inventories");

        // 5. Statistics and category breakdown count the same items (one per inventory here)
        let stats = f.db.get_inventory_statistics(None, uid).await.unwrap();
        let breakdown = f.db.get_category_breakdown(None, uid).await.unwrap();
        let breakdown_items: i64 = breakdown.iter().map(|c| c.item_count).sum();
        assert_eq!(
            stats.total_items,
            i64::try_from(expected_ids.len()).unwrap(),
            "{label}: statistics item count"
        );
        assert_eq!(breakdown_items, stats.total_items, "{label}: breakdown");
    }

    cleanup(&pool, &f).await;
}

#[actix_web::test]
async fn a_deactivated_user_with_access_is_still_rejected_at_authentication() {
    let pool = common::create_test_pool();
    home_registry::auth::get_or_init_jwt_secret();
    let f = build_fixture(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(api::get_inventory)),
    )
    .await;

    // The access rule still lists the share for the deactivated user (unchanged behaviour) ...
    let inactive = f.users.iter().find(|(l, _)| *l == "inactive").unwrap().1;
    assert!(f
        .db
        .check_inventory_access(inactive, f.inv_main)
        .await
        .unwrap());

    // ... but every request is refused before the rule is consulted
    let name = &f.names[f.users.iter().position(|(l, _)| *l == "inactive").unwrap()];
    let token = common::get_test_token(&pool, name).await;
    let req = test::TestRequest::get()
        .uri(&format!("/api/inventories/{}", f.inv_main))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::FORBIDDEN
    );

    cleanup(&pool, &f).await;
}

#[actix_web::test]
async fn rows_with_an_unknown_share_level_grant_nothing_anywhere() {
    let pool = common::create_test_pool();
    let f = build_fixture(&pool).await;
    let stranger = f.users.iter().find(|(l, _)| *l == "stranger").unwrap().1;
    let owner = f.users.iter().find(|(l, _)| *l == "owner").unwrap().1;

    // The CHECK constraint is the first line of defence and must exist ...
    let mut client = pool.get().await.unwrap();
    let checks = client
        .query(
            "SELECT pg_get_constraintdef(oid) FROM pg_constraint
             WHERE conrelid = 'inventory_shares'::regclass AND contype = 'c'
               AND pg_get_constraintdef(oid) ILIKE '%permission_level%'",
            &[],
        )
        .await
        .unwrap();
    assert!(
        !checks.is_empty(),
        "permission_level CHECK constraint is missing"
    );

    // ... and if a bad row got in anyway (constraint dropped inside this rolled-back
    // transaction), the view must not emit it, so every consumer denies it.
    let tx = client.transaction().await.unwrap();
    lock_for_ddl(&tx).await;
    tx.batch_execute(
        "ALTER TABLE inventory_shares DROP CONSTRAINT IF EXISTS inventory_shares_permission_level_check",
    )
    .await
    .unwrap();
    tx.execute(
        "INSERT INTO inventory_shares (inventory_id, shared_with_user_id, shared_by_user_id, permission_level)
         VALUES ($1, $2, $3, 'superuser')",
        &[&f.inv_main, &stranger, &owner],
    )
    .await
    .unwrap();
    let rows_for_stranger: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM inventory_access WHERE user_id = $1 AND inventory_id = $2",
            &[&stranger, &f.inv_main],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        rows_for_stranger, 0,
        "an unknown level must not appear in the view"
    );
    // The same predicate every consumer uses (list, can-see check, report scoping)
    let listed: bool = tx
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM inventories i WHERE i.id IN
                (SELECT inventory_id FROM inventory_access WHERE user_id = $1))",
            &[&stranger],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!listed);
    tx.rollback().await.unwrap();

    // The pure mapping is the second layer: it refuses the same input
    assert!(
        home_registry::models::EffectivePermissions::from_access("share", Some("superuser"))
            .is_none()
    );

    // Rolled back: the constraint is back and the stranger still has nothing
    assert!(!f
        .db
        .check_inventory_access(stranger, f.inv_main)
        .await
        .unwrap());

    cleanup(&pool, &f).await;
}

#[actix_web::test]
async fn the_view_migration_is_idempotent() {
    let pool = common::create_test_pool();
    let mut client = pool.get().await.unwrap();
    let sql = include_str!("../migrations/V025__create_inventory_access_view.sql");
    // CREATE OR REPLACE VIEW locks the view and its base tables, so serialise it like the
    // other DDL tests and keep it in one transaction
    let tx = client.transaction().await.unwrap();
    lock_for_ddl(&tx).await;
    tx.batch_execute(sql).await.unwrap();
    tx.batch_execute(sql).await.unwrap();
    tx.commit().await.unwrap();

    let columns: Vec<String> = client
        .query(
            "SELECT column_name FROM information_schema.columns
             WHERE table_name = 'inventory_access' ORDER BY ordinal_position",
            &[],
        )
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(
        columns,
        [
            "inventory_id",
            "user_id",
            "source",
            "permission_level",
            "precedence"
        ]
    );
}

#[actix_web::test]
async fn a_self_grant_would_resolve_to_owner_and_list_the_inventory_once() {
    let pool = common::create_test_pool();
    let f = build_fixture(&pool).await;
    let owner = f.users.iter().find(|(l, _)| *l == "otherowner").unwrap().1;

    let mut client = pool.get().await.unwrap();
    // The database refuses self-grants ...
    let guard = client
        .query(
            "SELECT 1 FROM pg_constraint
             WHERE conrelid = 'user_access_grants'::regclass AND conname = 'no_self_grant'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(guard.len(), 1, "no_self_grant CHECK constraint is missing");

    // ... but if one existed (constraint dropped in this rolled-back transaction), the owner
    // would hold two rows for the inventory and every consumer must still see one owner.
    let tx = client.transaction().await.unwrap();
    lock_for_ddl(&tx).await;
    tx.batch_execute("ALTER TABLE user_access_grants DROP CONSTRAINT no_self_grant")
        .await
        .unwrap();
    tx.execute(
        "INSERT INTO user_access_grants (grantor_user_id, grantee_user_id) VALUES ($1, $1)",
        &[&owner],
    )
    .await
    .unwrap();

    let sources: BTreeSet<String> = tx
        .query(
            "SELECT source FROM inventory_access WHERE user_id = $1 AND inventory_id = $2",
            &[&owner, &f.inv_self_granted],
        )
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(
        sources,
        BTreeSet::from(["owner".to_string(), "all_access".to_string()])
    );
    // The winning row (same ORDER BY get_effective_permissions uses) is the owner row
    let winner: String = tx
        .query_one(
            "SELECT source FROM inventory_access WHERE user_id = $1 AND inventory_id = $2
             ORDER BY precedence LIMIT 1",
            &[&owner, &f.inv_self_granted],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(winner, "owner");
    // The listing predicate counts the inventory once
    let owned_and_listed: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM inventories i
             WHERE i.id IN (SELECT inventory_id FROM inventory_access WHERE user_id = $1)",
            &[&owner],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(owned_and_listed, 1);
    tx.rollback().await.unwrap();

    cleanup(&pool, &f).await;
}
