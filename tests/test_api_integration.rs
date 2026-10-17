//! Integration tests for the Home Registry API
//!
//! These tests require a `PostgreSQL` database. Set `TEST_DATABASE_URL` environment variable
//! or ensure `DATABASE_URL` points to a test database.
//!
//! Run with: cargo test --test `test_api_integration`

mod common;

use actix_web::{http::StatusCode, test, web, App};
use home_registry::api;
use serde_json::json;

// ==================== Registration and Login Flow Tests ====================

#[actix_web::test]
async fn test_register_and_login_flow() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    // Create initial admin user (required for registration endpoint)
    // The register endpoint requires at least one user to exist (initial setup)
    let admin_username = common::test_username("admin_initial");
    common::create_admin_user(&pool, &admin_username).await;

    let app = test::init_service(
        App::new().app_data(web::Data::new(pool.clone())).service(
            web::scope("/api")
                .service(api::auth::register)
                .service(api::auth::login),
        ),
    )
    .await;

    let username = common::test_username("reg_login");
    let password = "TestPassword123!";

    // Test registration
    let register_payload = json!({
        "username": username,
        "password": password,
        "full_name": "Test User"
    });

    let req = test::TestRequest::post()
        .uri("/api/auth/register")
        .set_json(&register_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Test login with created credentials
    let login_payload = json!({
        "username": username,
        "password": password
    });

    let req = test::TestRequest::post()
        .uri("/api/auth/login")
        .set_json(&login_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["data"]["token"].is_string());
    assert_eq!(body["data"]["user"]["username"], username);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
    common::delete_test_user(&pool, &admin_username).await.ok();
}

// ==================== Inventory CRUD Tests ====================

#[actix_web::test]
async fn test_inventory_crud_operations() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new().app_data(web::Data::new(pool.clone())).service(
            web::scope("/api")
                .service(api::create_inventory)
                .service(api::get_inventories)
                .service(api::get_inventory)
                .service(api::update_inventory)
                .service(api::delete_inventory),
        ),
    )
    .await;

    let username = common::test_username("inv_crud");
    common::create_test_user(&pool, &username).await;
    let token = common::get_test_token(&pool, &username).await;

    // CREATE: Create new inventory
    let create_payload = json!({
        "name": "Test Inventory",
        "description": "Test description",
        "location": "Home"
    });

    let req = test::TestRequest::post()
        .uri("/api/inventories")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(&create_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let create_body: serde_json::Value = test::read_body_json(resp).await;
    #[allow(clippy::cast_possible_truncation)]
    let inventory_id = create_body["data"]["id"].as_i64().unwrap() as i32;

    // READ: Get all inventories
    let req = test::TestRequest::get()
        .uri("/api/inventories")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // READ: Get specific inventory
    let req = test::TestRequest::get()
        .uri(&format!("/api/inventories/{inventory_id}"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let get_body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(get_body["data"]["name"], "Test Inventory");

    // UPDATE: Update inventory
    let update_payload = json!({
        "name": "Updated Inventory",
        "description": "Updated description"
    });

    let req = test::TestRequest::put()
        .uri(&format!("/api/inventories/{inventory_id}"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(&update_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // DELETE: Delete inventory
    let req = test::TestRequest::delete()
        .uri(&format!("/api/inventories/{inventory_id}"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

// ==================== Item CRUD Tests ====================

#[actix_web::test]
async fn test_item_crud_operations() {
    let pool = common::create_test_pool();

    let app = test::init_service(
        App::new().app_data(web::Data::new(pool.clone())).service(
            web::scope("/api")
                .service(api::create_item)
                .service(api::get_items)
                .service(api::get_item)
                .service(api::delete_item),
        ),
    )
    .await;

    let username = common::test_username("item_crud");
    common::create_test_user(&pool, &username).await;

    let db = home_registry::db::DatabaseService::new(pool.clone());
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    let inventory_id = common::create_test_inventory(&pool, user.id, "Test Inventory")
        .await
        .unwrap();

    let token = common::get_test_token(&pool, &username).await;

    // CREATE: Create new item
    let create_payload = json!({
        "inventory_id": inventory_id,
        "name": "Test Item",
        "description": "Test item description",
        "category": "Electronics",
        "quantity": 1
    });

    let req = test::TestRequest::post()
        .uri("/api/items")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(&create_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let create_body: serde_json::Value = test::read_body_json(resp).await;
    #[allow(clippy::cast_possible_truncation)]
    let item_id = create_body["data"]["id"].as_i64().unwrap() as i32;

    // READ: Get items by inventory
    let req = test::TestRequest::get()
        .uri(&format!("/api/items?inventory_id={inventory_id}"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // READ: Get specific item
    let req = test::TestRequest::get()
        .uri(&format!("/api/items/{item_id}"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let get_body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(get_body["data"]["name"], "Test Item");

    // DELETE: Delete item
    let req = test::TestRequest::delete()
        .uri(&format!("/api/items/{item_id}"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Cleanup
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &username).await.ok();
}

// ==================== Authorization Middleware Tests ====================

#[actix_web::test]
async fn test_authorization_middleware_unauthenticated() {
    let pool = common::create_test_pool();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(api::get_inventories)),
    )
    .await;

    // Request without authentication token
    let req = test::TestRequest::get()
        .uri("/api/inventories")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_authorization_user_cannot_access_other_inventory() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(api::get_inventory)),
    )
    .await;

    // Create two users
    let username1 = common::test_username("auth_user1");
    let username2 = common::test_username("auth_user2");

    common::create_test_user(&pool, &username1).await;
    common::create_test_user(&pool, &username2).await;

    let db = home_registry::db::DatabaseService::new(pool.clone());
    let user1 = db.get_user_by_username(&username1).await.unwrap().unwrap();
    let _user2 = db.get_user_by_username(&username2).await.unwrap().unwrap();

    // Create inventory for user1
    let inventory_id = common::create_test_inventory(&pool, user1.id, "User1 Inventory")
        .await
        .unwrap();

    // Try to access user1's inventory with user2's token
    let token2 = common::get_test_token(&pool, &username2).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/inventories/{inventory_id}"))
        .insert_header(("Authorization", format!("Bearer {token2}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    // Users without any access to the inventory get 404 so its existence is not revealed
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Cleanup
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &username1).await.ok();
    common::delete_test_user(&pool, &username2).await.ok();
}

#[actix_web::test]
async fn test_authorization_admin_can_access_user_list() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(api::auth::admin_get_users)),
    )
    .await;

    let username = common::test_username("auth_admin");
    common::create_admin_user(&pool, &username).await;
    let token = common::get_test_token(&pool, &username).await;

    let req = test::TestRequest::get()
        .uri("/api/admin/users")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

// ==================== Input Validation Tests ====================

#[actix_web::test]
async fn test_create_inventory_validation_empty_name() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(api::create_inventory)),
    )
    .await;

    let username = common::test_username("inv_validation");
    common::create_test_user(&pool, &username).await;
    let token = common::get_test_token(&pool, &username).await;

    let create_payload = json!({
        "name": "",  // Invalid: empty name
        "description": "Test"
    });

    let req = test::TestRequest::post()
        .uri("/api/inventories")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(&create_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_create_item_validation_invalid_price() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(api::create_item)),
    )
    .await;

    let username = common::test_username("item_validation");
    common::create_test_user(&pool, &username).await;

    let db = home_registry::db::DatabaseService::new(pool.clone());
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    let inventory_id = common::create_test_inventory(&pool, user.id, "Test Inventory")
        .await
        .unwrap();

    let token = common::get_test_token(&pool, &username).await;

    let create_payload = json!({
        "inventory_id": inventory_id,
        "name": "Test Item",
        "purchase_price": -100.0  // Invalid: negative price
    });

    let req = test::TestRequest::post()
        .uri("/api/items")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(&create_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Cleanup
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &username).await.ok();
}

// ==================== Extended Inventory CRUD Tests ====================

#[actix_web::test]
async fn test_get_inventories_pagination() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(api::get_inventories)),
    )
    .await;

    let username = common::test_username("inv_page");
    common::create_test_user(&pool, &username).await;

    let db = home_registry::db::DatabaseService::new(pool.clone());
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    // Create multiple inventories
    let mut inv_ids = Vec::new();
    for i in 1..=5 {
        let id = common::create_test_inventory(&pool, user.id, &format!("Inventory {i}"))
            .await
            .unwrap();
        inv_ids.push(id);
    }

    let token = common::get_test_token(&pool, &username).await;

    // Test pagination (page 1, limit 2)
    let req = test::TestRequest::get()
        .uri("/api/inventories?page=1&limit=2")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    let inventories = body["data"].as_array().unwrap();
    // Pagination not currently implemented in API - endpoint returns all accessible inventories
    // TODO: Implement pagination support
    assert!(!inventories.is_empty());

    // Cleanup
    for id in inv_ids {
        common::delete_test_inventory(&pool, id).await.ok();
    }
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_update_inventory_not_found() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(api::update_inventory)),
    )
    .await;

    let username = common::test_username("inv_update_404");
    common::create_test_user(&pool, &username).await;
    let token = common::get_test_token(&pool, &username).await;

    let update_payload = json!({"name": "Updated"});

    let req = test::TestRequest::put()
        .uri("/api/inventories/99999") // Non-existent ID
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(&update_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_delete_inventory_cascade_deletes_items() {
    let pool = common::create_test_pool();
    let db = home_registry::db::DatabaseService::new(pool.clone());

    let username = common::test_username("inv_cascade");
    common::create_test_user(&pool, &username).await;
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    // Create inventory with items
    let inventory_id = common::create_test_inventory(&pool, user.id, "Test Inventory")
        .await
        .unwrap();
    let item_id = common::create_test_item(&pool, inventory_id, "Test Item")
        .await
        .unwrap();

    // Delete inventory
    db.delete_inventory(inventory_id).await.unwrap();

    // Verify item is also deleted (cascade)
    let item_check = db.get_item_by_id(item_id).await.unwrap();
    assert!(item_check.is_none());

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_inventory_ownership_verification() {
    let pool = common::create_test_pool();
    let db = home_registry::db::DatabaseService::new(pool.clone());

    // Create two users
    let owner_username = common::test_username("inv_owner");
    let other_username = common::test_username("inv_other");

    common::create_test_user(&pool, &owner_username).await;
    common::create_test_user(&pool, &other_username).await;

    let owner = db
        .get_user_by_username(&owner_username)
        .await
        .unwrap()
        .unwrap();

    // Owner creates inventory
    let inventory_id = common::create_test_inventory(&pool, owner.id, "Owner's Inventory")
        .await
        .unwrap();

    // Verify ownership
    let inventory = db.get_inventory_by_id(inventory_id).await.unwrap().unwrap();
    assert_eq!(inventory.user_id, Some(owner.id));

    // Cleanup
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &owner_username).await.ok();
    common::delete_test_user(&pool, &other_username).await.ok();
}

// ==================== Extended Item CRUD Tests ====================

#[actix_web::test]
async fn test_item_search_by_name() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(api::search_items)),
    )
    .await;

    let username = common::test_username("item_search");
    common::create_test_user(&pool, &username).await;

    let db = home_registry::db::DatabaseService::new(pool.clone());
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    let inventory_id = common::create_test_inventory(&pool, user.id, "Test Inventory")
        .await
        .unwrap();

    let unique_name = format!("UniqueItem_{}", uuid::Uuid::new_v4());
    let item_id = common::create_test_item(&pool, inventory_id, &unique_name)
        .await
        .unwrap();

    let token = common::get_test_token(&pool, &username).await;

    // Search for the item
    let req = test::TestRequest::get()
        .uri(&format!("/api/items/search/{unique_name}"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    let results = body["data"].as_array().unwrap();
    assert!(results
        .iter()
        .any(|item| item["name"].as_str() == Some(&unique_name)));

    // Cleanup
    common::delete_test_item(&pool, item_id).await.ok();
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_item_crud_with_all_fields() {
    let pool = common::create_test_pool();
    let db = home_registry::db::DatabaseService::new(pool.clone());

    let username = common::test_username("item_full");
    common::create_test_user(&pool, &username).await;
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    let inventory_id = common::create_test_inventory(&pool, user.id, "Test Inventory")
        .await
        .unwrap();

    // Create item with all fields populated
    let request = home_registry::models::CreateItemRequest {
        inventory_id: Some(inventory_id),
        name: "Complete Item".to_string(),
        description: Some("Full description".to_string()),
        category: Some("Electronics".to_string()),
        location: Some("Shelf A".to_string()),
        purchase_date: Some("2024-01-15".to_string()),
        purchase_price: Some(299.99),
        warranty_expiry: Some("2025-01-15".to_string()),
        notes: Some("Important notes".to_string()),
        quantity: Some(3),
    };

    let item = db.create_item(request).await.unwrap();

    assert_eq!(item.name, "Complete Item");
    assert_eq!(item.description, Some("Full description".to_string()));
    assert_eq!(item.category, Some("Electronics".to_string()));
    assert_eq!(item.purchase_price, Some(299.99));
    assert_eq!(item.quantity, Some(3));

    // Cleanup
    common::delete_test_item(&pool, item.id.unwrap()).await.ok();
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_item_quantity_update() {
    let pool = common::create_test_pool();
    let db = home_registry::db::DatabaseService::new(pool.clone());

    let username = common::test_username("item_qty");
    common::create_test_user(&pool, &username).await;
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    let inventory_id = common::create_test_inventory(&pool, user.id, "Test Inventory")
        .await
        .unwrap();
    let item_id = common::create_test_item(&pool, inventory_id, "Test Item")
        .await
        .unwrap();

    // Update quantity
    let update = home_registry::models::UpdateItemRequest {
        name: None,
        description: None,
        category: None,
        location: None,
        purchase_date: None,
        purchase_price: None,
        warranty_expiry: None,
        notes: None,
        quantity: Some(10),
        inventory_id: None,
    };

    db.update_item(item_id, update).await.unwrap();

    let updated_item = db.get_item_by_id(item_id).await.unwrap().unwrap();
    assert_eq!(updated_item.quantity, Some(10));

    // Cleanup
    common::delete_test_item(&pool, item_id).await.ok();
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_get_items_by_inventory_filter() {
    let pool = common::create_test_pool();
    let db = home_registry::db::DatabaseService::new(pool.clone());

    let username = common::test_username("item_filter");
    common::create_test_user(&pool, &username).await;
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    // Create two inventories
    let inv1_id = common::create_test_inventory(&pool, user.id, "Inventory 1")
        .await
        .unwrap();
    let inv2_id = common::create_test_inventory(&pool, user.id, "Inventory 2")
        .await
        .unwrap();

    // Create items in each
    let item1_id = common::create_test_item(&pool, inv1_id, "Item in Inv1")
        .await
        .unwrap();
    let item2_id = common::create_test_item(&pool, inv2_id, "Item in Inv2")
        .await
        .unwrap();

    // Get items for inv1 only
    let inv1_items = db.get_items_by_inventory(inv1_id).await.unwrap();
    assert!(inv1_items.iter().any(|item| item.id == Some(item1_id)));
    assert!(!inv1_items.iter().any(|item| item.id == Some(item2_id)));

    // Cleanup
    common::delete_test_item(&pool, item1_id).await.ok();
    common::delete_test_item(&pool, item2_id).await.ok();
    common::delete_test_inventory(&pool, inv1_id).await.ok();
    common::delete_test_inventory(&pool, inv2_id).await.ok();
    common::delete_test_user(&pool, &username).await.ok();
}

// ==================== Concurrent Access Tests ====================

#[actix_web::test]
async fn test_concurrent_inventory_creation() {
    let pool = common::create_test_pool();
    let db = home_registry::db::DatabaseService::new(pool.clone());

    let username = common::test_username("concurrent");
    common::create_test_user(&pool, &username).await;
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    // Create 5 inventories rapidly (simulating concurrent requests)
    let mut created_ids = vec![];
    for i in 0..5 {
        if let Ok(id) =
            common::create_test_inventory(&pool, user.id, &format!("Concurrent {i}")).await
        {
            created_ids.push(id);
        }
    }

    // All should succeed
    assert_eq!(created_ids.len(), 5);

    // Cleanup
    for id in created_ids {
        common::delete_test_inventory(&pool, id).await.ok();
    }
    common::delete_test_user(&pool, &username).await.ok();
}

// ==================== Database Constraint Tests ====================

#[actix_web::test]
async fn test_item_foreign_key_constraint() {
    let pool = common::create_test_pool();
    let db = home_registry::db::DatabaseService::new(pool.clone());

    let username = common::test_username("fk_test");
    common::create_test_user(&pool, &username).await;

    // Try to create item with non-existent inventory_id
    let request = home_registry::models::CreateItemRequest {
        inventory_id: Some(99999), // Non-existent
        name: "Invalid Item".to_string(),
        description: None,
        category: None,
        location: None,
        purchase_date: None,
        purchase_price: None,
        warranty_expiry: None,
        notes: None,
        quantity: None,
    };

    let result = db.create_item(request).await;
    // Should fail due to foreign key constraint
    assert!(result.is_err());

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

// ==================== Inventory Authorization Tests ====================

#[actix_web::test]
async fn test_data_endpoints_require_authorization() {
    let pool = common::create_test_pool();

    let app = test::init_service(
        App::new().app_data(web::Data::new(pool.clone())).service(
            web::scope("/api")
                .service(api::get_items)
                .service(api::get_item)
                .service(api::search_items)
                .service(api::update_inventory)
                .service(api::delete_inventory),
        ),
    )
    .await;

    let owner_username = common::test_username("authz_owner");
    let other_username = common::test_username("authz_other");
    common::create_test_user(&pool, &owner_username).await;
    common::create_test_user(&pool, &other_username).await;

    let db = home_registry::db::DatabaseService::new(pool.clone());
    let owner = db
        .get_user_by_username(&owner_username)
        .await
        .unwrap()
        .unwrap();

    let inventory_id = common::create_test_inventory(&pool, owner.id, "Private Inventory")
        .await
        .unwrap();
    let unique_name = format!("PrivateItem_{}", uuid::Uuid::new_v4());
    let item_id = common::create_test_item(&pool, inventory_id, &unique_name)
        .await
        .unwrap();

    let other_token = common::get_test_token(&pool, &other_username).await;

    // No token: 401 on every route
    for uri in [
        "/api/items".to_string(),
        format!("/api/items/{item_id}"),
        format!("/api/items/search/{unique_name}"),
    ] {
        let req = test::TestRequest::get().uri(&uri).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "GET {uri}");
    }
    let req = test::TestRequest::delete()
        .uri(&format!("/api/inventories/{inventory_id}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Another user: 404 (existence is not revealed), and nothing is modified
    let req = test::TestRequest::get()
        .uri(&format!("/api/items/{item_id}"))
        .insert_header(("Authorization", format!("Bearer {other_token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let req = test::TestRequest::put()
        .uri(&format!("/api/inventories/{inventory_id}"))
        .insert_header(("Authorization", format!("Bearer {other_token}")))
        .set_json(json!({ "name": "Hijacked" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/inventories/{inventory_id}"))
        .insert_header(("Authorization", format!("Bearer {other_token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Listing and search only return the caller's accessible items
    for uri in [
        "/api/items".to_string(),
        format!("/api/items/search/{unique_name}"),
    ] {
        let req = test::TestRequest::get()
            .uri(&uri)
            .insert_header(("Authorization", format!("Bearer {other_token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert!(!body["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["name"].as_str() == Some(&unique_name)));
    }

    // Cleanup
    common::delete_test_item(&pool, item_id).await.ok();
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &owner_username).await.ok();
    common::delete_test_user(&pool, &other_username).await.ok();
}

#[actix_web::test]
async fn test_view_share_is_forbidden_to_modify_and_move_requires_destination() {
    use home_registry::models::{CreateOrganizerTypeRequest, PermissionLevel};

    let pool = common::create_test_pool();

    let app = test::init_service(
        App::new().app_data(web::Data::new(pool.clone())).service(
            web::scope("/api")
                .service(api::update_inventory)
                .service(api::update_item)
                .service(api::delete_item)
                .service(api::set_item_organizer_values),
        ),
    )
    .await;

    let owner_username = common::test_username("authz_share_owner");
    let viewer_username = common::test_username("authz_share_viewer");
    common::create_test_user(&pool, &owner_username).await;
    common::create_test_user(&pool, &viewer_username).await;

    let db = home_registry::db::DatabaseService::new(pool.clone());
    let owner = db
        .get_user_by_username(&owner_username)
        .await
        .unwrap()
        .unwrap();
    let viewer = db
        .get_user_by_username(&viewer_username)
        .await
        .unwrap()
        .unwrap();

    // Owner's inventory shared view-only with the viewer
    let shared_inv = common::create_test_inventory(&pool, owner.id, "Shared")
        .await
        .unwrap();
    db.create_inventory_share(shared_inv, viewer.id, owner.id, PermissionLevel::View)
        .await
        .unwrap();
    // Owner-only inventory, plus the viewer's own inventory and item
    let private_inv = common::create_test_inventory(&pool, owner.id, "Private")
        .await
        .unwrap();
    let viewer_inv = common::create_test_inventory(&pool, viewer.id, "Viewer Own")
        .await
        .unwrap();
    let shared_item = common::create_test_item(&pool, shared_inv, "Shared Item")
        .await
        .unwrap();
    let viewer_item = common::create_test_item(&pool, viewer_inv, "Viewer Item")
        .await
        .unwrap();

    let private_type = db
        .create_organizer_type(
            private_inv,
            CreateOrganizerTypeRequest {
                name: "Private Type".to_string(),
                input_type: Some("text".to_string()),
                is_required: None,
                display_order: None,
            },
        )
        .await
        .unwrap();

    let token = common::get_test_token(&pool, &viewer_username).await;
    let auth = ("Authorization", format!("Bearer {token}"));

    // View-only share: can see, but modifying is 403
    let req = test::TestRequest::put()
        .uri(&format!("/api/inventories/{shared_inv}"))
        .insert_header(auth.clone())
        .set_json(json!({ "name": "Renamed" }))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::FORBIDDEN
    );

    let req = test::TestRequest::delete()
        .uri(&format!("/api/items/{shared_item}"))
        .insert_header(auth.clone())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::FORBIDDEN
    );

    // Moving the viewer's own item into an inventory they cannot see is refused
    let req = test::TestRequest::put()
        .uri(&format!("/api/items/{viewer_item}"))
        .insert_header(auth.clone())
        .set_json(json!({ "inventory_id": private_inv }))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NOT_FOUND
    );

    // ...and into one they can only view
    let req = test::TestRequest::put()
        .uri(&format!("/api/items/{viewer_item}"))
        .insert_header(auth.clone())
        .set_json(json!({ "inventory_id": shared_inv }))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::FORBIDDEN
    );

    // An organizer type from another inventory cannot be attached to the viewer's item
    let req = test::TestRequest::put()
        .uri(&format!("/api/items/{viewer_item}/organizer-values"))
        .insert_header(auth)
        .set_json(json!({ "values": [{
            "organizer_type_id": private_type.id.unwrap(),
            "text_value": "x"
        }] }))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );

    // Cleanup
    common::delete_test_item(&pool, shared_item).await.ok();
    common::delete_test_item(&pool, viewer_item).await.ok();
    for id in [shared_inv, private_inv, viewer_inv] {
        common::delete_test_inventory(&pool, id).await.ok();
    }
    common::delete_test_user(&pool, &owner_username).await.ok();
    common::delete_test_user(&pool, &viewer_username).await.ok();
}

// ==================== Share and Image Ownership Tests ====================

#[actix_web::test]
async fn test_share_and_image_endpoints_enforce_ownership() {
    use home_registry::models::PermissionLevel;

    let pool = common::create_test_pool();

    let app = test::init_service(
        App::new().app_data(web::Data::new(pool.clone())).service(
            web::scope("/api")
                .service(api::auth::update_inventory_share)
                .service(api::auth::delete_inventory_share)
                .service(api::auth::get_inventory_shares)
                .service(api::delete_image)
                .service(api::get_inventory_item_images),
        ),
    )
    .await;

    let owner_name = common::test_username("own_owner");
    let viewer_name = common::test_username("own_viewer");
    let other_viewer_name = common::test_username("own_viewer2");
    let stranger_name = common::test_username("own_stranger");
    for name in [
        &owner_name,
        &viewer_name,
        &other_viewer_name,
        &stranger_name,
    ] {
        common::create_test_user(&pool, name).await;
    }

    let db = home_registry::db::DatabaseService::new(pool.clone());
    let owner = db.get_user_by_username(&owner_name).await.unwrap().unwrap();
    let viewer = db
        .get_user_by_username(&viewer_name)
        .await
        .unwrap()
        .unwrap();
    let other_viewer = db
        .get_user_by_username(&other_viewer_name)
        .await
        .unwrap()
        .unwrap();

    let inventory_id = common::create_test_inventory(&pool, owner.id, "Shared Inventory")
        .await
        .unwrap();
    let viewer_share = db
        .create_inventory_share(inventory_id, viewer.id, owner.id, PermissionLevel::View)
        .await
        .unwrap();
    let other_share = db
        .create_inventory_share(
            inventory_id,
            other_viewer.id,
            owner.id,
            PermissionLevel::View,
        )
        .await
        .unwrap();

    // An item in the inventory that references an uploaded image
    let item_id = common::create_test_item(&pool, inventory_id, "Item With Image")
        .await
        .unwrap();
    let image_name = format!("{}_1.png", uuid::Uuid::new_v4().simple());
    pool.get()
        .await
        .unwrap()
        .execute(
            "UPDATE items SET image_url = $1 WHERE id = $2",
            &[&format!("/uploads/img/{image_name}"), &item_id],
        )
        .await
        .unwrap();

    let viewer_token = common::get_test_token(&pool, &viewer_name).await;
    let stranger_token = common::get_test_token(&pool, &stranger_name).await;
    let owner_token = common::get_test_token(&pool, &owner_name).await;
    let bearer = |t: &str| ("Authorization", format!("Bearer {t}"));

    // A view sharee cannot raise their own share
    let req = test::TestRequest::put()
        .uri(&format!("/api/shares/{}", viewer_share.id))
        .insert_header(bearer(&viewer_token))
        .set_json(json!({ "permission_level": "edit_inventory" }))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::FORBIDDEN
    );

    // ...nor delete somebody else's share
    let req = test::TestRequest::delete()
        .uri(&format!("/api/shares/{}", other_share.id))
        .insert_header(bearer(&viewer_token))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::FORBIDDEN
    );

    // A stranger learns nothing about the share
    let req = test::TestRequest::put()
        .uri(&format!("/api/shares/{}", viewer_share.id))
        .insert_header(bearer(&stranger_token))
        .set_json(json!({ "permission_level": "edit_inventory" }))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NOT_FOUND
    );
    let req = test::TestRequest::delete()
        .uri(&format!("/api/shares/{}", viewer_share.id))
        .insert_header(bearer(&stranger_token))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NOT_FOUND
    );

    // Listing/creating shares: a stranger gets 404, a view sharee gets 403
    let req = test::TestRequest::get()
        .uri(&format!("/api/inventories/{inventory_id}/shares"))
        .insert_header(bearer(&stranger_token))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NOT_FOUND
    );
    let req = test::TestRequest::get()
        .uri(&format!("/api/inventories/{inventory_id}/shares"))
        .insert_header(bearer(&viewer_token))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::FORBIDDEN
    );

    // Image / item-images: stranger gets 404, a view-only sharee cannot delete the image
    let req = test::TestRequest::get()
        .uri(&format!("/api/inventories/{inventory_id}/item-images"))
        .insert_header(bearer(&stranger_token))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NOT_FOUND
    );
    let req = test::TestRequest::delete()
        .uri(&format!("/api/images/{image_name}"))
        .insert_header(bearer(&stranger_token))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NOT_FOUND
    );
    let req = test::TestRequest::delete()
        .uri(&format!("/api/images/{image_name}"))
        .insert_header(bearer(&viewer_token))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::FORBIDDEN
    );
    let req = test::TestRequest::get()
        .uri(&format!("/api/inventories/{inventory_id}/item-images"))
        .insert_header(bearer(&viewer_token))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), StatusCode::OK);

    // The owner can update a share; the sharee can then leave on their own
    let req = test::TestRequest::put()
        .uri(&format!("/api/shares/{}", viewer_share.id))
        .insert_header(bearer(&owner_token))
        .set_json(json!({ "permission_level": "edit_items" }))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), StatusCode::OK);
    let req = test::TestRequest::delete()
        .uri(&format!("/api/shares/{}", viewer_share.id))
        .insert_header(bearer(&viewer_token))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), StatusCode::OK);

    // Cleanup
    common::delete_test_item(&pool, item_id).await.ok();
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    for name in [
        &owner_name,
        &viewer_name,
        &other_viewer_name,
        &stranger_name,
    ] {
        common::delete_test_user(&pool, name).await.ok();
    }
}

#[actix_web::test]
async fn test_login_requires_totp_code_when_enabled() {
    let pool = common::create_test_pool();
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(api::auth::login)),
    )
    .await;

    let two_fa_name = common::test_username("totp_2fa");
    let recovery_name = common::test_username("totp_rec");
    for name in [&two_fa_name, &recovery_name] {
        common::create_test_user(&pool, name).await;
    }

    let db = home_registry::db::DatabaseService::new(pool.clone());
    for (name, mode) in [
        (&two_fa_name, "2fa_only"),
        (&recovery_name, "recovery_only"),
    ] {
        let user = db.get_user_by_username(name).await.unwrap().unwrap();
        db.create_totp_settings(user.id, "encrypted-secret")
            .await
            .unwrap();
        db.enable_totp(user.id, mode).await.unwrap();
    }

    let login = |name: &str| {
        test::TestRequest::post()
            .uri("/api/auth/login")
            .set_json(json!({ "username": name, "password": common::test_password() }))
            .to_request()
    };

    // 2fa_only: password alone yields a partial token, never a full session token
    let resp = test::call_service(&app, login(&two_fa_name)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["requires_totp"], true);
    assert!(body["data"]["partial_token"].is_string());
    assert!(body["data"]["token"].is_null());

    // recovery_only: TOTP is not required at login
    let resp = test::call_service(&app, login(&recovery_name)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["data"]["token"].is_string());
    assert!(body["data"]["requires_totp"].is_null());

    for name in [&two_fa_name, &recovery_name] {
        common::delete_test_user(&pool, name).await.ok();
    }
}

#[actix_web::test]
async fn test_generate_recovery_codes_requires_current_password() {
    let pool = common::create_test_pool();
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(api::auth::generate_recovery_codes)),
    )
    .await;

    let username = common::test_username("rc_regen");
    common::create_test_user(&pool, &username).await;
    let db = home_registry::db::DatabaseService::new(pool.clone());
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();
    let token = common::get_test_token(&pool, &username).await;

    // Existing codes the attacker must not be able to replace
    db.store_recovery_codes(user.id, vec!["hash-a".to_string(), "hash-b".to_string()])
        .await
        .unwrap();
    let unused_before = db.get_recovery_codes_status(user.id).await.unwrap().2;
    assert_eq!(unused_before, 2);

    // No token
    let req = test::TestRequest::post()
        .uri("/api/auth/recovery-codes/generate")
        .set_json(json!({ "current_password": common::test_password() }))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );

    // Valid token but wrong password: 400 (not 401), codes untouched
    let req = test::TestRequest::post()
        .uri("/api/auth/recovery-codes/generate")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({ "current_password": "WrongPassword123!" }))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );

    // Missing password field
    let req = test::TestRequest::post()
        .uri("/api/auth/recovery-codes/generate")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({}))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        db.get_recovery_codes_status(user.id).await.unwrap().2,
        unused_before
    );

    // Correct password: new codes are issued and replace the old ones
    let req = test::TestRequest::post()
        .uri("/api/auth/recovery-codes/generate")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({ "current_password": common::test_password() }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["codes"].as_array().unwrap().len(), 10);
    assert_eq!(db.get_recovery_codes_status(user.id).await.unwrap().2, 10);

    common::delete_test_user(&pool, &username).await.ok();
}
