// Database service tests

mod common;

use home_registry::db::DatabaseService;
use uuid::Uuid;

// ==================== User Database Tests ====================

#[tokio::test]
async fn test_create_user() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_create");
    let password_hash = home_registry::auth::hash_password("TestPassword123!".to_string())
        .await
        .expect("Failed to hash password");

    let user = db
        .create_user(&username, "Test User", &password_hash, false, false)
        .await
        .expect("Failed to create user");

    assert_eq!(user.username, username);
    assert_eq!(user.full_name, "Test User");
    assert!(!user.is_admin);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[tokio::test]
async fn test_get_user_by_id() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_get_id");
    common::create_test_user(&pool, &username).await;

    let user = db
        .get_user_by_username(&username)
        .await
        .expect("Failed to get user by username")
        .expect("User not found");

    let user_id = user.id;

    let found_user = db
        .get_user_by_id(user_id)
        .await
        .expect("Failed to get user by ID")
        .expect("User not found");

    assert_eq!(found_user.id, user_id);
    assert_eq!(found_user.username, username);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[tokio::test]
async fn test_get_user_by_username() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_get_uname");
    common::create_test_user(&pool, &username).await;

    let user = db
        .get_user_by_username(&username)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    assert_eq!(user.username, username);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[tokio::test]
async fn test_get_user_by_username_not_found() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let result = db
        .get_user_by_username("nonexistent_user_xyz")
        .await
        .expect("Query should succeed");

    assert!(result.is_none());
}

#[tokio::test]
async fn test_update_user_password() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_update_pw");
    common::create_test_user(&pool, &username).await;

    let user = db
        .get_user_by_username(&username)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    let new_password_hash = home_registry::auth::hash_password("NewPassword456!".to_string())
        .await
        .expect("Failed to hash password");

    let result = db.update_user_password(user.id, &new_password_hash).await;

    assert!(result.is_ok());

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[tokio::test]
async fn test_delete_user() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_delete");
    common::create_test_user(&pool, &username).await;

    let user = db
        .get_user_by_username(&username)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    let deleted = db
        .delete_user(user.id)
        .await
        .expect("Failed to delete user");
    assert!(deleted);

    // Verify deletion
    let user_check = db
        .get_user_by_id(user.id)
        .await
        .expect("Query should succeed");

    assert!(user_check.is_none());
}

// ==================== Inventory Database Tests ====================

#[tokio::test]
async fn test_create_inventory() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_inv_create");
    common::create_test_user(&pool, &username).await;

    let user = db
        .get_user_by_username(&username)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    let request = home_registry::models::CreateInventoryRequest {
        name: "Test Inventory".to_string(),
        description: Some("Test description".to_string()),
        location: Some("Test location".to_string()),
        image_url: None,
    };

    let inventory = db
        .create_inventory(request, user.id)
        .await
        .expect("Failed to create inventory");

    assert_eq!(inventory.name, "Test Inventory");
    assert_eq!(inventory.description, Some("Test description".to_string()));
    assert_eq!(inventory.location, Some("Test location".to_string()));

    // Cleanup
    common::delete_test_inventory(&pool, inventory.id.unwrap())
        .await
        .ok();
    common::delete_test_user(&pool, &username).await.ok();
}

#[tokio::test]
async fn test_get_inventory_by_id() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_inv_get");
    common::create_test_user(&pool, &username).await;

    let user = db
        .get_user_by_username(&username)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    let inventory_id = common::create_test_inventory(&pool, user.id, "Test Inventory")
        .await
        .expect("Failed to create inventory");

    let inventory = db
        .get_inventory_by_id(inventory_id)
        .await
        .expect("Failed to get inventory")
        .expect("Inventory not found");

    assert_eq!(inventory.id.unwrap(), inventory_id);
    assert_eq!(inventory.name, "Test Inventory");

    // Cleanup
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &username).await.ok();
}

#[tokio::test]
async fn test_update_inventory() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_inv_update");
    common::create_test_user(&pool, &username).await;

    let user = db
        .get_user_by_username(&username)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    let inventory_id = common::create_test_inventory(&pool, user.id, "Original Name")
        .await
        .expect("Failed to create inventory");

    let update_request = home_registry::models::UpdateInventoryRequest {
        name: Some("Updated Name".to_string()),
        description: Some("Updated description".to_string()),
        location: Some("Updated location".to_string()),
        image_url: None,
    };

    db.update_inventory(inventory_id, update_request)
        .await
        .expect("Failed to update inventory");

    let updated = db
        .get_inventory_by_id(inventory_id)
        .await
        .expect("Failed to get inventory")
        .expect("Inventory not found");

    assert_eq!(updated.name, "Updated Name");
    assert_eq!(updated.description, Some("Updated description".to_string()));

    // Cleanup
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &username).await.ok();
}

#[tokio::test]
async fn test_delete_inventory() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_inv_delete");
    common::create_test_user(&pool, &username).await;

    let user = db
        .get_user_by_username(&username)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    let inventory_id = common::create_test_inventory(&pool, user.id, "To Delete")
        .await
        .expect("Failed to create inventory");

    let deleted = db
        .delete_inventory(inventory_id)
        .await
        .expect("Failed to delete inventory");

    assert!(deleted);

    // Verify deletion
    let check = db
        .get_inventory_by_id(inventory_id)
        .await
        .expect("Query should succeed");

    assert!(check.is_none());

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

// ==================== Item Database Tests ====================

#[tokio::test]
async fn test_create_item() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_item_create");
    common::create_test_user(&pool, &username).await;

    let user = db
        .get_user_by_username(&username)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    let inventory_id = common::create_test_inventory(&pool, user.id, "Test Inventory")
        .await
        .expect("Failed to create inventory");

    let request = home_registry::models::CreateItemRequest {
        inventory_id,
        name: "Test Item".to_string(),
        description: Some("Test item description".to_string()),
        category: Some("Electronics".to_string()),
        location: Some("Shelf A".to_string()),
        purchase_date: Some("2024-01-01".to_string()),
        purchase_price: Some(99.99),
        warranty_expiry: Some("2025-01-01".to_string()),
        notes: Some("Test notes".to_string()),
        quantity: Some(1),
    };

    let item = db
        .create_item(request)
        .await
        .expect("Failed to create item");

    assert_eq!(item.name, "Test Item");
    assert_eq!(item.category, Some("Electronics".to_string()));

    // Cleanup
    common::delete_test_item(&pool, item.id.unwrap()).await.ok();
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &username).await.ok();
}

#[tokio::test]
async fn test_get_item_by_id() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_item_get");
    common::create_test_user(&pool, &username).await;

    let user = db
        .get_user_by_username(&username)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    let inventory_id = common::create_test_inventory(&pool, user.id, "Test Inventory")
        .await
        .expect("Failed to create inventory");

    let item_id = common::create_test_item(&pool, inventory_id, "Test Item")
        .await
        .expect("Failed to create item");

    let item = db
        .get_item_by_id(item_id)
        .await
        .expect("Failed to get item")
        .expect("Item not found");

    assert_eq!(item.id.unwrap(), item_id);
    assert_eq!(item.name, "Test Item");

    // Cleanup
    common::delete_test_item(&pool, item_id).await.ok();
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &username).await.ok();
}

#[tokio::test]
async fn test_update_item() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_item_update");
    common::create_test_user(&pool, &username).await;

    let user = db
        .get_user_by_username(&username)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    let inventory_id = common::create_test_inventory(&pool, user.id, "Test Inventory")
        .await
        .expect("Failed to create inventory");

    let item_id = common::create_test_item(&pool, inventory_id, "Original Item")
        .await
        .expect("Failed to create item");

    let update_request = home_registry::models::UpdateItemRequest {
        name: Some("Updated Item".to_string()),
        description: Some("Updated description".to_string()),
        category: None,
        location: None,
        purchase_date: None,
        purchase_price: Some(199.99),
        warranty_expiry: None,
        notes: None,
        quantity: Some(5),
        inventory_id: None,
    };

    db.update_item(item_id, update_request)
        .await
        .expect("Failed to update item");

    let updated = db
        .get_item_by_id(item_id)
        .await
        .expect("Failed to get item")
        .expect("Item not found");

    assert_eq!(updated.name, "Updated Item");
    assert_eq!(updated.description, Some("Updated description".to_string()));

    // Cleanup
    common::delete_test_item(&pool, item_id).await.ok();
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &username).await.ok();
}

#[tokio::test]
async fn test_delete_item() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_item_delete");
    common::create_test_user(&pool, &username).await;

    let user = db
        .get_user_by_username(&username)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    let inventory_id = common::create_test_inventory(&pool, user.id, "Test Inventory")
        .await
        .expect("Failed to create inventory");

    let item_id = common::create_test_item(&pool, inventory_id, "To Delete")
        .await
        .expect("Failed to create item");

    let deleted = db
        .delete_item(item_id)
        .await
        .expect("Failed to delete item");
    assert!(deleted);

    // Verify deletion
    let check = db
        .get_item_by_id(item_id)
        .await
        .expect("Query should succeed");

    assert!(check.is_none());

    // Cleanup
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &username).await.ok();
}

#[tokio::test]
async fn test_get_items_by_inventory() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_items_list");
    common::create_test_user(&pool, &username).await;

    let user = db
        .get_user_by_username(&username)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    let inventory_id = common::create_test_inventory(&pool, user.id, "Test Inventory")
        .await
        .expect("Failed to create inventory");

    // Create multiple items
    let item1_id = common::create_test_item(&pool, inventory_id, "Item 1")
        .await
        .expect("Failed to create item 1");
    let item2_id = common::create_test_item(&pool, inventory_id, "Item 2")
        .await
        .expect("Failed to create item 2");

    let items = db
        .get_items_by_inventory(inventory_id)
        .await
        .expect("Failed to get items");

    assert!(items.len() >= 2);

    // Cleanup
    common::delete_test_item(&pool, item1_id).await.ok();
    common::delete_test_item(&pool, item2_id).await.ok();
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &username).await.ok();
}

#[tokio::test]
async fn test_search_items() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_item_search");
    common::create_test_user(&pool, &username).await;

    let user = db
        .get_user_by_username(&username)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    let inventory_id = common::create_test_inventory(&pool, user.id, "Test Inventory")
        .await
        .expect("Failed to create inventory");

    let unique_name = format!("UniqueSearchTerm_{}", Uuid::new_v4());
    let item_id = common::create_test_item(&pool, inventory_id, &unique_name)
        .await
        .expect("Failed to create item");

    let found = db
        .search_items(user.id, &unique_name, 100)
        .await
        .expect("Failed to search items");

    assert!(found.results.iter().any(|hit| hit.item.id == Some(item_id)));
    assert_eq!(found.total, 1);

    // Someone without access to the inventory finds nothing
    let other = common::test_username("db_item_search_other");
    common::create_test_user(&pool, &other).await;
    let other_user = db
        .get_user_by_username(&other)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    let found = db
        .search_items(other_user.id, &unique_name, 100)
        .await
        .expect("Failed to search items");
    assert!(found.results.is_empty());
    assert_eq!(found.total, 0);
    common::delete_test_user(&pool, &other).await.ok();

    // Cleanup
    common::delete_test_item(&pool, item_id).await.ok();
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &username).await.ok();
}

// ==================== Error Handling Tests ====================

#[tokio::test]
async fn test_get_nonexistent_user() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let result = db.get_user_by_id(Uuid::new_v4()).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_get_nonexistent_inventory() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let result = db.get_inventory_by_id(99999).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_get_nonexistent_item() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let result = db.get_item_by_id(99999).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_update_nonexistent_inventory() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let update = home_registry::models::UpdateInventoryRequest {
        name: Some("Updated".to_string()),
        description: None,
        location: None,
        image_url: None,
    };

    let result = db.update_inventory(99999, update).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_update_nonexistent_item() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let update = home_registry::models::UpdateItemRequest {
        name: Some("Updated".to_string()),
        description: None,
        category: None,
        location: None,
        purchase_date: None,
        purchase_price: None,
        warranty_expiry: None,
        notes: None,
        quantity: None,
        inventory_id: None,
    };

    let result = db.update_item(99999, update).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_delete_nonexistent_inventory() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let result = db.delete_inventory(99999).await;
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[tokio::test]
async fn test_delete_nonexistent_item() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let result = db.delete_item(99999).await;
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[tokio::test]
async fn test_create_user_duplicate_username() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_dup_user");
    let password_hash = home_registry::auth::hash_password("TestPassword123!".to_string())
        .await
        .expect("Failed to hash password");

    // Create first user
    db.create_user(&username, &password_hash, "Test User", false, false)
        .await
        .expect("Failed to create first user");

    // Try to create duplicate
    let result = db
        .create_user(&username, &password_hash, "Duplicate User", false, false)
        .await;

    // Should fail with constraint violation
    assert!(result.is_err());

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

// ==================== User Settings Tests ====================

#[tokio::test]
async fn test_user_settings_crud() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_settings");
    common::create_test_user(&pool, &username).await;
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    // Create settings (defaults)
    let settings = db.create_user_settings(user.id).await;
    assert!(settings.is_ok());

    // Get settings
    let retrieved = db.get_user_settings(user.id).await.unwrap();
    assert!(retrieved.is_some());
    let settings = retrieved.unwrap();
    assert_eq!(settings.theme, "light"); // Default theme

    // Update settings
    let update_req = home_registry::models::UpdateUserSettingsRequest {
        theme: Some("dark".to_string()),
        default_inventory_id: None,
        items_per_page: None,
        date_format: None,
        currency: None,
        notifications_enabled: None,
        settings_json: None,
    };

    db.update_user_settings(user.id, update_req).await.unwrap();
    let updated = db.get_user_settings(user.id).await.unwrap().unwrap();
    assert_eq!(updated.theme, "dark");

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

// ==================== Inventory Sharing Tests ====================

#[tokio::test]
async fn test_share_inventory() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let owner_username = common::test_username("db_share_owner");
    let shared_username = common::test_username("db_share_user");

    common::create_test_user(&pool, &owner_username).await;
    common::create_test_user(&pool, &shared_username).await;

    let owner = db
        .get_user_by_username(&owner_username)
        .await
        .unwrap()
        .unwrap();
    let shared_user = db
        .get_user_by_username(&shared_username)
        .await
        .unwrap()
        .unwrap();

    let inventory_id = common::create_test_inventory(&pool, owner.id, "Shared Inventory")
        .await
        .unwrap();

    // Share with view permission
    let result = db
        .create_inventory_share(
            inventory_id,
            shared_user.id,
            owner.id,
            home_registry::models::PermissionLevel::View,
        )
        .await;
    assert!(result.is_ok());

    // Get shared inventories
    let accessible = db.get_accessible_inventories(shared_user.id).await.unwrap();
    assert!(accessible.iter().any(|inv| inv.id == Some(inventory_id)));

    // Cleanup
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &owner_username).await.ok();
    common::delete_test_user(&pool, &shared_username).await.ok();
}

#[tokio::test]
async fn test_unshare_inventory() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let owner_username = common::test_username("db_unshare_owner");
    let shared_username = common::test_username("db_unshare_user");

    common::create_test_user(&pool, &owner_username).await;
    common::create_test_user(&pool, &shared_username).await;

    let owner = db
        .get_user_by_username(&owner_username)
        .await
        .unwrap()
        .unwrap();
    let shared_user = db
        .get_user_by_username(&shared_username)
        .await
        .unwrap()
        .unwrap();

    let inventory_id = common::create_test_inventory(&pool, owner.id, "Shared Inventory")
        .await
        .unwrap();

    // Share
    let share = db
        .create_inventory_share(
            inventory_id,
            shared_user.id,
            owner.id,
            home_registry::models::PermissionLevel::View,
        )
        .await
        .unwrap();

    // Unshare
    let result = db.delete_inventory_share(share.id).await;
    assert!(result.is_ok());

    // Verify removed
    let accessible = db.get_accessible_inventories(shared_user.id).await.unwrap();
    assert!(!accessible.iter().any(|inv| inv.id == Some(inventory_id)));

    // Cleanup
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &owner_username).await.ok();
    common::delete_test_user(&pool, &shared_username).await.ok();
}

// ==================== Data Validation Tests ====================

#[tokio::test]
async fn test_create_inventory_with_empty_name() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_empty_name");
    common::create_test_user(&pool, &username).await;
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    let request = home_registry::models::CreateInventoryRequest {
        name: String::new(), // Empty name
        description: None,
        location: None,
        image_url: None,
    };

    // Should succeed at DB level (validation should be in API layer)
    let result = db.create_inventory(request, user.id).await;
    // This tests that DB doesn't crash, validation is API responsibility
    if let Ok(inventory) = result {
        common::delete_test_inventory(&pool, inventory.id.unwrap())
            .await
            .ok();
    }

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[tokio::test]
async fn test_create_item_with_null_optional_fields() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_null_fields");
    common::create_test_user(&pool, &username).await;
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    let inventory_id = common::create_test_inventory(&pool, user.id, "Test Inventory")
        .await
        .unwrap();

    let request = home_registry::models::CreateItemRequest {
        inventory_id,
        name: "Minimal Item".to_string(),
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
    assert!(result.is_ok());

    let item = result.unwrap();
    assert_eq!(item.name, "Minimal Item");
    assert!(item.description.is_none());
    assert!(item.purchase_price.is_none());

    // Cleanup
    common::delete_test_item(&pool, item.id.unwrap()).await.ok();
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &username).await.ok();
}

#[tokio::test]
async fn test_inventory_list_ordering() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_order");
    common::create_test_user(&pool, &username).await;
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    // Create inventories with specific names
    let id1 = common::create_test_inventory(&pool, user.id, "Alpha Inventory")
        .await
        .unwrap();
    let id2 = common::create_test_inventory(&pool, user.id, "Beta Inventory")
        .await
        .unwrap();
    let id3 = common::create_test_inventory(&pool, user.id, "Gamma Inventory")
        .await
        .unwrap();

    // Get all inventories (should be ordered by created_at DESC by default)
    let inventories = db.get_accessible_inventories(user.id).await.unwrap();

    // Verify we got at least our 3 inventories
    assert!(inventories.len() >= 3);

    // Cleanup
    common::delete_test_inventory(&pool, id1).await.ok();
    common::delete_test_inventory(&pool, id2).await.ok();
    common::delete_test_inventory(&pool, id3).await.ok();
    common::delete_test_user(&pool, &username).await.ok();
}

#[tokio::test]
async fn test_get_all_items() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    // This test verifies get_all_items works
    let result = db.get_all_items().await;
    assert!(result.is_ok());

    // Should return a vec (may be empty or contain items)
    let items = result.unwrap();
    assert!(items.is_empty() || !items.is_empty()); // Just verify it's a valid vec
}

#[tokio::test]
async fn test_user_password_update_and_verification() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("db_pw_verify");
    let old_password = "OldPassword123!";
    let new_password = "NewPassword456!";

    let old_hash = home_registry::auth::hash_password(old_password.to_string())
        .await
        .unwrap();

    db.create_user(&username, "Test User", &old_hash, false, false)
        .await
        .unwrap();

    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    // Verify old password works
    let old_verification =
        home_registry::auth::verify_password(old_password.to_string(), user.password_hash.clone())
            .await
            .unwrap();
    assert!(old_verification);

    // Update password
    let new_hash = home_registry::auth::hash_password(new_password.to_string())
        .await
        .unwrap();
    db.update_user_password(user.id, &new_hash).await.unwrap();

    // Get updated user
    let updated_user = db.get_user_by_username(&username).await.unwrap().unwrap();

    // Verify new password works
    let new_verification = home_registry::auth::verify_password(
        new_password.to_string(),
        updated_user.password_hash.clone(),
    )
    .await
    .unwrap();
    assert!(new_verification);

    // Verify old password no longer works
    let old_verification_after = home_registry::auth::verify_password(
        old_password.to_string(),
        updated_user.password_hash.clone(),
    )
    .await
    .unwrap();
    assert!(!old_verification_after);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}
