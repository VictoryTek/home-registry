// Tests for model validation

use home_registry::models::{
    CreateInventoryRequest, CreateItemRequest, UpdateInventoryRequest, UpdateItemRequest,
};
use validator::Validate;

#[test]
fn test_create_inventory_validation() {
    // Valid inventory
    let valid = CreateInventoryRequest {
        name: "Test Inventory".to_string(),
        description: Some("A test inventory".to_string()),
        location: Some("Home".to_string()),
        image_url: None,
    };
    assert!(valid.validate().is_ok());

    // Invalid: empty name
    let invalid = CreateInventoryRequest {
        name: String::new(),
        description: None,
        location: None,
        image_url: None,
    };
    assert!(invalid.validate().is_err());

    // Invalid: name too long
    let invalid = CreateInventoryRequest {
        name: "a".repeat(256),
        description: None,
        location: None,
        image_url: None,
    };
    assert!(invalid.validate().is_err());

    // Invalid: description too long
    let invalid = CreateInventoryRequest {
        name: "Valid Name".to_string(),
        description: Some("x".repeat(5001)),
        location: None,
        image_url: None,
    };
    assert!(invalid.validate().is_err());
}

#[test]
fn test_create_item_validation() {
    // Valid item
    let valid = CreateItemRequest {
        inventory_id: 1,
        name: "Test Item".to_string(),
        description: Some("Test description".to_string()),
        category: Some("Electronics".to_string()),
        location: Some("Shelf A".to_string()),
        purchase_date: Some("2024-01-01".to_string()),
        purchase_price: Some(99.99),
        warranty_expiry: Some("2025-01-01".to_string()),
        notes: Some("Test notes".to_string()),
        quantity: Some(1),
    };
    assert!(valid.validate().is_ok());

    // Invalid: empty name
    let invalid = CreateItemRequest {
        inventory_id: 1,
        name: String::new(),
        description: None,
        category: None,
        location: None,
        purchase_date: None,
        purchase_price: None,
        warranty_expiry: None,
        notes: None,
        quantity: None,
    };
    assert!(invalid.validate().is_err());

    // Invalid: price negative
    let invalid = CreateItemRequest {
        inventory_id: 1,
        name: "Valid Name".to_string(),
        description: None,
        category: None,
        location: None,
        purchase_date: None,
        purchase_price: Some(-10.0),
        warranty_expiry: None,
        notes: None,
        quantity: None,
    };
    assert!(invalid.validate().is_err());

    // Invalid: price too high
    let invalid = CreateItemRequest {
        inventory_id: 1,
        name: "Valid Name".to_string(),
        description: None,
        category: None,
        location: None,
        purchase_date: None,
        purchase_price: Some(2_000_000_000.0),
        warranty_expiry: None,
        notes: None,
        quantity: None,
    };
    assert!(invalid.validate().is_err());

    // Invalid: quantity negative
    let invalid = CreateItemRequest {
        inventory_id: 1,
        name: "Valid Name".to_string(),
        description: None,
        category: None,
        location: None,
        purchase_date: None,
        purchase_price: None,
        warranty_expiry: None,
        notes: None,
        quantity: Some(-1),
    };
    assert!(invalid.validate().is_err());

    // Invalid: quantity too high
    let invalid = CreateItemRequest {
        inventory_id: 1,
        name: "Valid Name".to_string(),
        description: None,
        category: None,
        location: None,
        purchase_date: None,
        purchase_price: None,
        warranty_expiry: None,
        notes: None,
        quantity: Some(2_000_000),
    };
    assert!(invalid.validate().is_err());
}

#[test]
fn test_update_inventory_validation() {
    // Valid update
    let valid = UpdateInventoryRequest {
        name: Some("Updated Name".to_string()),
        description: Some("Updated description".to_string()),
        location: Some("New location".to_string()),
        image_url: Some("https://example.com/image.jpg".to_string()),
    };
    assert!(valid.validate().is_ok());

    // Valid: all None (no changes)
    let valid = UpdateInventoryRequest {
        name: None,
        description: None,
        location: None,
        image_url: None,
    };
    assert!(valid.validate().is_ok());

    // Invalid: empty name
    let invalid = UpdateInventoryRequest {
        name: Some(String::new()),
        description: None,
        location: None,
        image_url: None,
    };
    assert!(invalid.validate().is_err());
}

#[test]
fn test_update_item_validation() {
    // Valid update
    let valid = UpdateItemRequest {
        name: Some("Updated Item".to_string()),
        description: Some("Updated".to_string()),
        category: Some("New Category".to_string()),
        location: Some("New Location".to_string()),
        purchase_date: Some("2024-06-01".to_string()),
        purchase_price: Some(199.99),
        warranty_expiry: Some("2026-06-01".to_string()),
        notes: Some("Updated notes".to_string()),
        quantity: Some(5),
        inventory_id: Some(2),
    };
    assert!(valid.validate().is_ok());

    // Valid: all None (no changes)
    let valid = UpdateItemRequest {
        name: None,
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
    assert!(valid.validate().is_ok());

    // Invalid: empty name
    let invalid = UpdateItemRequest {
        name: Some(String::new()),
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
    assert!(invalid.validate().is_err());
}

// ==================== Additional Model Tests ====================

#[test]
fn test_user_response_serialization() {
    use chrono::Utc;
    use home_registry::models::UserResponse;
    use uuid::Uuid;

    let user = UserResponse {
        id: Uuid::new_v4(),
        username: "testuser".to_string(),
        full_name: "Test User".to_string(),
        is_admin: false,
        is_active: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    // Test serialization
    let json = serde_json::to_string(&user).expect("Failed to serialize");
    assert!(json.contains("testuser"));
    assert!(json.contains("Test User"));

    // Test deserialization
    let deserialized: UserResponse = serde_json::from_str(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.username, "testuser");
}

#[test]
fn test_api_response_structure() {
    use home_registry::models::ApiResponse;

    let response: ApiResponse<String> = ApiResponse {
        success: true,
        data: Some("test data".to_string()),
        message: Some("Operation successful".to_string()),
        error: None,
    };

    let json = serde_json::to_string(&response).expect("Failed to serialize");
    assert!(json.contains("test data"));
    assert!(json.contains("Operation successful"));

    let deserialized: ApiResponse<String> =
        serde_json::from_str(&json).expect("Failed to deserialize");
    assert!(deserialized.success);
    assert_eq!(deserialized.data, Some("test data".to_string()));
}

#[test]
fn test_error_response_structure() {
    use home_registry::models::ErrorResponse;

    let response = ErrorResponse {
        success: false,
        error: "An error occurred".to_string(),
        message: Some("Please try again".to_string()),
    };

    let json = serde_json::to_string(&response).expect("Failed to serialize");
    assert!(json.contains("An error occurred"));
    assert!(json.contains("Please try again"));

    let deserialized: ErrorResponse = serde_json::from_str(&json).expect("Failed to deserialize");
    assert!(!deserialized.success);
    assert_eq!(deserialized.error, "An error occurred");
}

#[test]
fn test_login_request_validation() {
    use home_registry::models::LoginRequest;

    // Valid login request - test deserialization only (LoginRequest has Deserialize)
    let json = r#"{"username":"testuser","password":"password123"}"#;
    let deserialized: LoginRequest = serde_json::from_str(json).expect("Failed to deserialize");
    assert_eq!(deserialized.username, "testuser");
    assert_eq!(deserialized.password, "password123");
}

#[test]
fn test_item_model_with_optional_fields() {
    // Test that item creation works with minimal fields
    let minimal = CreateItemRequest {
        inventory_id: 1,
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
    assert!(minimal.validate().is_ok());

    // Test with all optional fields populated
    let complete = CreateItemRequest {
        inventory_id: 1,
        name: "Complete Item".to_string(),
        description: Some("Full description".to_string()),
        category: Some("Electronics".to_string()),
        location: Some("Shelf A".to_string()),
        purchase_date: Some("2024-01-01".to_string()),
        purchase_price: Some(99.99),
        warranty_expiry: Some("2025-01-01".to_string()),
        notes: Some("Important notes".to_string()),
        quantity: Some(5),
    };
    assert!(complete.validate().is_ok());
}

// ==================== TOTP login requirement ====================

fn totp_settings(
    mode: &str,
    is_enabled: bool,
    is_verified: bool,
) -> home_registry::models::TotpSettings {
    home_registry::models::TotpSettings {
        id: uuid::Uuid::new_v4(),
        user_id: uuid::Uuid::new_v4(),
        totp_secret_encrypted: String::new(),
        totp_mode: mode.to_string(),
        is_enabled,
        is_verified,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        last_used_at: None,
        failed_attempts: 0,
        last_failed_at: None,
    }
}

#[test]
fn test_login_requires_totp_only_when_enabled_and_verified() {
    assert!(!totp_settings("both", false, false).login_requires_totp());
    assert!(!totp_settings("both", true, false).login_requires_totp());
    assert!(!totp_settings("both", false, true).login_requires_totp());
    assert!(totp_settings("both", true, true).login_requires_totp());
}

#[test]
fn test_login_requires_totp_by_mode() {
    assert!(totp_settings("2fa_only", true, true).login_requires_totp());
    assert!(totp_settings("both", true, true).login_requires_totp());
    assert!(!totp_settings("recovery_only", true, true).login_requires_totp());
}

#[test]
fn test_login_requires_totp_fails_closed_on_unknown_mode() {
    assert!(totp_settings("not_a_mode", true, true).login_requires_totp());
    assert!(totp_settings("", true, true).login_requires_totp());
    // An unknown mode on a non-enabled record still does not demand TOTP
    assert!(!totp_settings("not_a_mode", false, false).login_requires_totp());
}

// ==================== Effective permissions from an inventory_access row ====================

mod effective_permissions {
    use home_registry::models::{EffectivePermissions, PermissionSource};

    #[test]
    fn owner_and_all_access_have_full_control() {
        for (source, expect) in [
            ("owner", PermissionSource::Owner),
            ("all_access", PermissionSource::AllAccess),
        ] {
            let p = EffectivePermissions::from_access(source, None).expect(source);
            assert!(p.can_view && p.can_edit_items && p.can_add_items && p.can_remove_items);
            assert!(p.can_edit_inventory && p.can_delete_inventory);
            assert!(p.can_manage_sharing && p.can_manage_organizers);
            assert_eq!(p.permission_source, expect);
            assert_eq!(p.is_owner, source == "owner");
            assert_eq!(p.has_all_access, source == "all_access");
        }
    }

    #[test]
    fn share_levels_map_to_exactly_their_capabilities() {
        let view = EffectivePermissions::from_access("share", Some("view")).unwrap();
        assert!(view.can_view);
        assert!(!view.can_edit_items && !view.can_add_items && !view.can_remove_items);
        assert!(!view.can_edit_inventory && !view.can_manage_organizers);

        let edit_items = EffectivePermissions::from_access("share", Some("edit_items")).unwrap();
        assert!(edit_items.can_view && edit_items.can_edit_items);
        assert!(!edit_items.can_add_items && !edit_items.can_remove_items);
        assert!(!edit_items.can_edit_inventory && !edit_items.can_manage_organizers);

        let manager = EffectivePermissions::from_access("share", Some("edit_inventory")).unwrap();
        assert!(manager.can_view && manager.can_edit_items && manager.can_add_items);
        assert!(manager.can_remove_items && manager.can_edit_inventory);
        assert!(manager.can_manage_organizers);

        // A share never confers deleting the inventory or managing its sharing
        for level in ["view", "edit_items", "edit_inventory"] {
            let p = EffectivePermissions::from_access("share", Some(level)).unwrap();
            assert!(!p.can_delete_inventory && !p.can_manage_sharing, "{level}");
            assert_eq!(p.permission_source, PermissionSource::InventoryShare);
        }
    }

    #[test]
    fn unknown_input_fails_closed() {
        assert!(EffectivePermissions::from_access("share", Some("superuser")).is_none());
        assert!(EffectivePermissions::from_access("share", Some("")).is_none());
        assert!(EffectivePermissions::from_access("share", None).is_none());
        assert!(EffectivePermissions::from_access("admin", None).is_none());
        assert!(EffectivePermissions::from_access("", Some("view")).is_none());

        let none = EffectivePermissions::none();
        assert!(!none.can_view && !none.can_edit_items && !none.can_delete_inventory);
        assert_eq!(none.permission_source, PermissionSource::None);
    }
}

// ==================== CreateItemRequest.inventory_id is required ====================

#[test]
fn test_create_item_requires_a_positive_inventory_id() {
    // Missing from the JSON: no default inventory
    let missing: Result<CreateItemRequest, _> = serde_json::from_str(r#"{"name": "Drill"}"#);
    let err = missing.expect_err("inventory_id is required");
    assert!(err.to_string().contains("inventory_id"), "{err}");

    // null is not a value either
    assert!(serde_json::from_str::<CreateItemRequest>(
        r#"{"inventory_id": null, "name": "Drill"}"#
    )
    .is_err());

    // Present but not a valid id
    for bad in [0, -1, -42] {
        let request: CreateItemRequest =
            serde_json::from_str(&format!(r#"{{"inventory_id": {bad}, "name": "Drill"}}"#))
                .unwrap();
        let errors = request.validate().expect_err("must be rejected");
        assert!(errors.to_string().contains("inventory_id"), "{errors}");
    }

    let ok: CreateItemRequest =
        serde_json::from_str(r#"{"inventory_id": 7, "name": "Drill"}"#).unwrap();
    assert!(ok.validate().is_ok());
}
