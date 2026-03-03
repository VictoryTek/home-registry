use crate::models::{
    AdminUpdateUserRequest,
    // Backup & Restore models
    BackupDatabaseContent,
    CreateInventoryRequest,
    CreateItemRequest,
    CreateOrganizerOptionRequest,
    CreateOrganizerTypeRequest,
    EffectivePermissions,
    Inventory,
    InventoryShare,
    InventoryShareWithUser,
    Item,
    ItemOrganizerValue,
    ItemOrganizerValueWithDetails,
    OrganizerOption,
    OrganizerType,
    OrganizerTypeWithOptions,
    PermissionLevel,
    PermissionSource,
    SetItemOrganizerValueRequest,
    // TOTP models
    TotpSettings,
    UpdateItemRequest,
    UpdateOrganizerOptionRequest,
    UpdateOrganizerTypeRequest,
    UpdateUserSettingsRequest,
    // User-related models
    User,
    // User Access Grant models (All Access tier)
    UserAccessGrant,
    UserAccessGrantWithUsers,
    UserResponse,
    UserSettings,
};
use chrono::{DateTime, Utc};
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod};
use log::{error, info};
use std::env;
use tokio_postgres::NoTls;
use uuid::Uuid;

/// Escape special characters in SQL LIKE patterns to prevent injection
fn escape_like_pattern(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub fn get_pool() -> Result<Pool, Box<dyn std::error::Error + Send + Sync>> {
    let db_url =
        env::var("DATABASE_URL").map_err(|_| "DATABASE_URL environment variable must be set")?;

    // Parse DATABASE_URL: postgres://user:password@host:port/database
    let url = db_url
        .strip_prefix("postgres://")
        .ok_or("Invalid DATABASE_URL format: must start with postgres://")?;

    let parts: Vec<&str> = url.split('@').collect();
    if parts.len() != 2 {
        return Err(
            "Invalid DATABASE_URL format: expected postgres://user:password@host/database".into(),
        );
    }

    let auth_parts: Vec<&str> = parts[0].split(':').collect();
    let host_parts: Vec<&str> = parts[1].split('/').collect();
    let host_port: Vec<&str> = host_parts[0].split(':').collect();

    let user = (*auth_parts.first().unwrap_or(&"postgres")).to_string();
    let password = (*auth_parts.get(1).unwrap_or(&"password")).to_string();
    let host = (*host_port.first().unwrap_or(&"localhost")).to_string();
    let port = host_port
        .get(1)
        .unwrap_or(&"5432")
        .parse::<u16>()
        .unwrap_or(5432);
    let dbname = (*host_parts.get(1).unwrap_or(&"home_inventory")).to_string();

    let mut cfg = Config::new();
    cfg.user = Some(user);
    cfg.password = Some(password);
    cfg.host = Some(host);
    cfg.port = Some(port);
    cfg.dbname = Some(dbname);
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });

    cfg.create_pool(None, NoTls)
        .map_err(|e| format!("Failed to create database pool: {e}").into())
}

pub struct DatabaseService {
    pool: Pool,
}

impl DatabaseService {
    #[must_use]
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn get_all_items(&self) -> Result<Vec<Item>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT id, inventory_id, name, description, category, location, purchase_date::text, purchase_price::float8, warranty_expiry::text, notes, quantity, created_at, updated_at 
             FROM items ORDER BY created_at DESC",
                &[],
            )
            .await?;

        let mut items = Vec::new();
        for row in rows {
            let item = Item {
                id: Some(row.get(0)),
                inventory_id: row.get(1),
                name: row.get(2),
                description: row.get(3),
                category: row.get(4),
                location: row.get(5),
                purchase_date: row.get::<_, Option<String>>(6),
                purchase_price: row.get(7),
                warranty_expiry: row.get::<_, Option<String>>(8),
                notes: row.get(9),
                quantity: row.get(10),
                created_at: row.get::<_, Option<DateTime<Utc>>>(11),
                updated_at: row.get::<_, Option<DateTime<Utc>>>(12),
            };
            items.push(item);
        }

        info!("Retrieved {} items from database", items.len());
        Ok(items)
    }

    pub async fn get_item_by_id(
        &self,
        id: i32,
    ) -> Result<Option<Item>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT id, inventory_id, name, description, category, location, purchase_date::text, purchase_price::float8, warranty_expiry::text, notes, quantity, created_at, updated_at 
             FROM items WHERE id = $1",
                &[&id],
            )
            .await?;

        if let Some(row) = rows.first() {
            let item = Item {
                id: Some(row.get(0)),
                inventory_id: row.get(1),
                name: row.get(2),
                description: row.get(3),
                category: row.get(4),
                location: row.get(5),
                purchase_date: row.get::<_, Option<String>>(6),
                purchase_price: row.get(7),
                warranty_expiry: row.get::<_, Option<String>>(8),
                notes: row.get(9),
                quantity: row.get(10),
                created_at: row.get::<_, Option<DateTime<Utc>>>(11),
                updated_at: row.get::<_, Option<DateTime<Utc>>>(12),
            };
            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    pub async fn create_item(
        &self,
        request: CreateItemRequest,
    ) -> Result<Item, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        // Convert date strings to proper format or None
        let purchase_date: Option<chrono::NaiveDate> = request
            .purchase_date
            .as_ref()
            .filter(|s| !s.is_empty())
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        let warranty_expiry: Option<chrono::NaiveDate> = request
            .warranty_expiry
            .as_ref()
            .filter(|s| !s.is_empty())
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        // Handle price properly - convert to None if not provided
        let purchase_price_param: Option<f64> = request.purchase_price;

        let row = client
            .query_one(
                "INSERT INTO items (inventory_id, name, description, category, location, purchase_date, purchase_price, warranty_expiry, notes, quantity) 
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) 
             RETURNING id, inventory_id, name, description, category, location, purchase_date::text, purchase_price::float8, warranty_expiry::text, notes, quantity, created_at, updated_at",
                &[
                    &request.inventory_id.unwrap_or(1),
                    &request.name,
                    &request.description,
                    &request.category,
                    &request.location,
                    &purchase_date,
                    &purchase_price_param,
                    &warranty_expiry,
                    &request.notes,
                    &request.quantity,
                ],
            )
            .await?;

        let item = Item {
            id: Some(row.get(0)),
            inventory_id: row.get(1),
            name: row.get(2),
            description: row.get(3),
            category: row.get(4),
            location: row.get(5),
            purchase_date: row.get::<_, Option<String>>(6),
            purchase_price: row.get(7),
            warranty_expiry: row.get::<_, Option<String>>(8),
            notes: row.get(9),
            quantity: row.get(10),
            created_at: row.get::<_, Option<DateTime<Utc>>>(11),
            updated_at: row.get::<_, Option<DateTime<Utc>>>(12),
        };

        info!("Created new item: {} (ID: {:?})", item.name, item.id);
        Ok(item)
    }

    pub async fn update_item(
        &self,
        id: i32,
        request: UpdateItemRequest,
    ) -> Result<Option<Item>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        // Build dynamic update query
        let mut fields = Vec::new();
        let mut values: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::new();
        let mut param_count = 1;

        if let Some(ref name) = request.name {
            fields.push(format!("name = ${param_count}"));
            values.push(name);
            param_count += 1;
        }
        if let Some(ref description) = request.description {
            fields.push(format!("description = ${param_count}"));
            values.push(description);
            param_count += 1;
        }
        if let Some(ref category) = request.category {
            fields.push(format!("category = ${param_count}"));
            values.push(category);
            param_count += 1;
        }
        if let Some(ref location) = request.location {
            fields.push(format!("location = ${param_count}"));
            values.push(location);
            param_count += 1;
        }
        if let Some(ref purchase_price) = request.purchase_price {
            fields.push(format!("purchase_price = ${param_count}"));
            values.push(purchase_price);
            param_count += 1;
        }
        if let Some(ref quantity) = request.quantity {
            fields.push(format!("quantity = ${param_count}"));
            values.push(quantity);
            param_count += 1;
        }
        if let Some(ref notes) = request.notes {
            fields.push(format!("notes = ${param_count}"));
            values.push(notes);
            param_count += 1;
        }
        if let Some(ref inventory_id) = request.inventory_id {
            fields.push(format!("inventory_id = ${param_count}"));
            values.push(inventory_id);
            param_count += 1;
        }

        // Handle date fields
        let purchase_date_val: Option<chrono::NaiveDate>;
        if let Some(ref pd) = request.purchase_date {
            let date_str = pd.trim();
            purchase_date_val = if date_str.is_empty() {
                None
            } else {
                chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()
            };
            fields.push(format!("purchase_date = ${param_count}"));
            values.push(&purchase_date_val);
            param_count += 1;
        }

        let warranty_expiry_val: Option<chrono::NaiveDate>;
        if let Some(ref we) = request.warranty_expiry {
            let date_str = we.trim();
            warranty_expiry_val = if date_str.is_empty() {
                None
            } else {
                chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()
            };
            fields.push(format!("warranty_expiry = ${param_count}"));
            values.push(&warranty_expiry_val);
            param_count += 1;
        }

        if fields.is_empty() {
            return self.get_item_by_id(id).await;
        }

        fields.push("updated_at = NOW()".to_string());
        values.push(&id);

        let query = format!(
            "UPDATE items SET {} WHERE id = ${} RETURNING id, inventory_id, name, description, category, location, purchase_date::text, purchase_price::float8, warranty_expiry::text, notes, quantity, created_at, updated_at",
            fields.join(", "),
            param_count
        );

        let rows = client.query(&query, &values).await?;

        if let Some(row) = rows.first() {
            let item = Item {
                id: Some(row.get(0)),
                inventory_id: row.get(1),
                name: row.get(2),
                description: row.get(3),
                category: row.get(4),
                location: row.get(5),
                purchase_date: row.get::<_, Option<String>>(6),
                purchase_price: row.get(7),
                warranty_expiry: row.get::<_, Option<String>>(8),
                notes: row.get(9),
                quantity: row.get(10),
                created_at: row.get::<_, Option<DateTime<Utc>>>(11),
                updated_at: row.get::<_, Option<DateTime<Utc>>>(12),
            };
            info!("Updated item ID: {}", id);
            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    pub async fn delete_item(&self, id: i32) -> Result<bool, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows_affected = client
            .execute("DELETE FROM items WHERE id = $1", &[&id])
            .await?;

        let deleted = rows_affected > 0;
        if deleted {
            info!("Deleted item ID: {}", id);
        }
        Ok(deleted)
    }

    pub async fn search_items(&self, query: &str) -> Result<Vec<Item>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        // Escape SQL LIKE wildcards to prevent pattern injection
        let escaped_query = escape_like_pattern(&query.to_lowercase());
        let search_pattern = format!("%{escaped_query}%");
        let rows = client
            .query(
                "SELECT id, inventory_id, name, description, category, location, purchase_date::text, purchase_price::float8, warranty_expiry::text, notes, quantity, created_at, updated_at 
             FROM items 
             WHERE LOWER(name) LIKE $1 ESCAPE '\\' 
                OR LOWER(description) LIKE $1 ESCAPE '\\' 
                OR LOWER(category) LIKE $1 ESCAPE '\\' 
                OR LOWER(location) LIKE $1 ESCAPE '\\'
             ORDER BY created_at DESC",
                &[&search_pattern],
            )
            .await?;

        let mut items = Vec::new();
        for row in rows {
            let item = Item {
                id: Some(row.get(0)),
                inventory_id: row.get(1),
                name: row.get(2),
                description: row.get(3),
                category: row.get(4),
                location: row.get(5),
                purchase_date: row.get::<_, Option<String>>(6),
                purchase_price: row.get(7),
                warranty_expiry: row.get::<_, Option<String>>(8),
                notes: row.get(9),
                quantity: row.get(10),
                created_at: row.get::<_, Option<DateTime<Utc>>>(11),
                updated_at: row.get::<_, Option<DateTime<Utc>>>(12),
            };
            items.push(item);
        }

        info!(
            "Found {} items matching search query: '{}'",
            items.len(),
            query
        );
        Ok(items)
    }

    // Inventory operations
    pub async fn get_inventory_by_id(
        &self,
        id: i32,
    ) -> Result<Option<Inventory>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT id, name, description, location, image_url, user_id, created_at, updated_at 
                 FROM inventories WHERE id = $1",
                &[&id],
            )
            .await?;

        if let Some(row) = rows.first() {
            let inventory = Inventory {
                id: Some(row.get(0)),
                name: row.get(1),
                description: row.get(2),
                location: row.get(3),
                image_url: row.get(4),
                user_id: row.get(5),
                created_at: row.get::<_, Option<DateTime<Utc>>>(6),
                updated_at: row.get::<_, Option<DateTime<Utc>>>(7),
            };
            Ok(Some(inventory))
        } else {
            Ok(None)
        }
    }

    pub async fn create_inventory(
        &self,
        request: CreateInventoryRequest,
        user_id: uuid::Uuid,
    ) -> Result<Inventory, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let row = client
            .query_one(
                "INSERT INTO inventories (name, description, location, image_url, user_id) 
                 VALUES ($1, $2, $3, $4, $5) 
                 RETURNING id, name, description, location, image_url, user_id, created_at, updated_at",
                &[&request.name, &request.description, &request.location, &request.image_url, &user_id],
            )
            .await?;

        let inventory = Inventory {
            id: Some(row.get(0)),
            name: row.get(1),
            description: row.get(2),
            location: row.get(3),
            image_url: row.get(4),
            user_id: row.get(5),
            created_at: row.get::<_, Option<DateTime<Utc>>>(6),
            updated_at: row.get::<_, Option<DateTime<Utc>>>(7),
        };

        info!(
            "Created new inventory: {} (ID: {:?})",
            inventory.name, inventory.id
        );
        Ok(inventory)
    }

    pub async fn update_inventory(
        &self,
        id: i32,
        request: crate::models::UpdateInventoryRequest,
    ) -> Result<Option<Inventory>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        // Build dynamic update query
        let mut fields = Vec::new();
        let mut values: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::new();
        let mut param_count = 1;

        if let Some(ref name) = request.name {
            fields.push(format!("name = ${param_count}"));
            values.push(name);
            param_count += 1;
        }
        if let Some(ref description) = request.description {
            fields.push(format!("description = ${param_count}"));
            values.push(description);
            param_count += 1;
        }
        if let Some(ref location) = request.location {
            fields.push(format!("location = ${param_count}"));
            values.push(location);
            param_count += 1;
        }
        if let Some(ref image_url) = request.image_url {
            fields.push(format!("image_url = ${param_count}"));
            values.push(image_url);
            param_count += 1;
        }

        if fields.is_empty() {
            return self.get_inventory_by_id(id).await;
        }

        fields.push("updated_at = NOW()".to_string());
        values.push(&id);

        let query = format!(
            "UPDATE inventories SET {} WHERE id = ${} RETURNING id, name, description, location, image_url, user_id, created_at, updated_at",
            fields.join(", "),
            param_count
        );

        let rows = client.query(&query, &values).await?;

        if let Some(row) = rows.first() {
            let inventory = Inventory {
                id: Some(row.get(0)),
                name: row.get(1),
                description: row.get(2),
                location: row.get(3),
                image_url: row.get(4),
                user_id: row.get(5),
                created_at: row.get::<_, Option<DateTime<Utc>>>(6),
                updated_at: row.get::<_, Option<DateTime<Utc>>>(7),
            };
            info!("Updated inventory ID: {}", id);
            Ok(Some(inventory))
        } else {
            Ok(None)
        }
    }

    pub async fn delete_inventory(&self, id: i32) -> Result<bool, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows_affected = client
            .execute("DELETE FROM inventories WHERE id = $1", &[&id])
            .await?;

        let deleted = rows_affected > 0;
        if deleted {
            info!(
                "Deleted inventory ID: {} (CASCADE: organizers and items)",
                id
            );
        }
        Ok(deleted)
    }

    pub async fn get_items_by_inventory(
        &self,
        inventory_id: i32,
    ) -> Result<Vec<Item>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT id, inventory_id, name, description, category, location, purchase_date::text, purchase_price::float8, warranty_expiry::text, notes, quantity, created_at, updated_at 
                 FROM items WHERE inventory_id = $1 ORDER BY created_at DESC",
                &[&inventory_id],
            )
            .await?;

        let mut items = Vec::new();
        for row in rows {
            let item = Item {
                id: Some(row.get(0)),
                inventory_id: row.get(1),
                name: row.get(2),
                description: row.get(3),
                category: row.get(4),
                location: row.get(5),
                purchase_date: row.get::<_, Option<String>>(6),
                purchase_price: row.get(7),
                warranty_expiry: row.get::<_, Option<String>>(8),
                notes: row.get(9),
                quantity: row.get(10),
                created_at: row.get::<_, Option<DateTime<Utc>>>(11),
                updated_at: row.get::<_, Option<DateTime<Utc>>>(12),
            };
            items.push(item);
        }

        info!(
            "Retrieved {} items for inventory {}",
            items.len(),
            inventory_id
        );
        Ok(items)
    }

    // ==================== Organizer Type Operations ====================

    pub async fn get_organizer_types_by_inventory(
        &self,
        inventory_id: i32,
    ) -> Result<Vec<OrganizerType>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT id, inventory_id, name, input_type, is_required, display_order, created_at, updated_at 
                 FROM organizer_types WHERE inventory_id = $1 ORDER BY display_order ASC, name ASC",
                &[&inventory_id],
            )
            .await?;

        let mut organizers = Vec::new();
        for row in rows {
            let organizer = OrganizerType {
                id: Some(row.get(0)),
                inventory_id: row.get(1),
                name: row.get(2),
                input_type: row.get(3),
                is_required: row.get(4),
                display_order: row.get(5),
                created_at: row.get::<_, Option<DateTime<Utc>>>(6),
                updated_at: row.get::<_, Option<DateTime<Utc>>>(7),
            };
            organizers.push(organizer);
        }

        info!(
            "Retrieved {} organizer types for inventory {}",
            organizers.len(),
            inventory_id
        );
        Ok(organizers)
    }

    pub async fn get_organizer_types_with_options_by_inventory(
        &self,
        inventory_id: i32,
    ) -> Result<Vec<OrganizerTypeWithOptions>, Box<dyn std::error::Error>> {
        let organizer_types = self.get_organizer_types_by_inventory(inventory_id).await?;

        let mut result = Vec::new();
        for organizer_type in organizer_types {
            let options = if organizer_type.input_type == "select" {
                if let Some(id) = organizer_type.id {
                    self.get_organizer_options(id).await?
                } else {
                    error!("Organizer type missing ID for inventory {}", inventory_id);
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            result.push(OrganizerTypeWithOptions {
                organizer_type,
                options,
            });
        }

        Ok(result)
    }

    pub async fn get_organizer_type_by_id(
        &self,
        id: i32,
    ) -> Result<Option<OrganizerType>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT id, inventory_id, name, input_type, is_required, display_order, created_at, updated_at 
                 FROM organizer_types WHERE id = $1",
                &[&id],
            )
            .await?;

        if let Some(row) = rows.first() {
            Ok(Some(OrganizerType {
                id: Some(row.get(0)),
                inventory_id: row.get(1),
                name: row.get(2),
                input_type: row.get(3),
                is_required: row.get(4),
                display_order: row.get(5),
                created_at: row.get::<_, Option<DateTime<Utc>>>(6),
                updated_at: row.get::<_, Option<DateTime<Utc>>>(7),
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn create_organizer_type(
        &self,
        inventory_id: i32,
        request: CreateOrganizerTypeRequest,
    ) -> Result<OrganizerType, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let input_type = request.input_type.unwrap_or_else(|| "select".to_string());
        let is_required = request.is_required.unwrap_or(false);
        let display_order = request.display_order.unwrap_or(0);

        let row = client
            .query_one(
                "INSERT INTO organizer_types (inventory_id, name, input_type, is_required, display_order) 
                 VALUES ($1, $2, $3, $4, $5) 
                 RETURNING id, inventory_id, name, input_type, is_required, display_order, created_at, updated_at",
                &[&inventory_id, &request.name, &input_type, &is_required, &display_order],
            )
            .await?;

        let organizer = OrganizerType {
            id: Some(row.get(0)),
            inventory_id: row.get(1),
            name: row.get(2),
            input_type: row.get(3),
            is_required: row.get(4),
            display_order: row.get(5),
            created_at: row.get::<_, Option<DateTime<Utc>>>(6),
            updated_at: row.get::<_, Option<DateTime<Utc>>>(7),
        };

        info!(
            "Created organizer type: {} (ID: {:?})",
            organizer.name, organizer.id
        );
        Ok(organizer)
    }

    pub async fn update_organizer_type(
        &self,
        id: i32,
        request: UpdateOrganizerTypeRequest,
    ) -> Result<Option<OrganizerType>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let mut fields = Vec::new();
        let mut values: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::new();
        let mut param_count = 1;

        if let Some(ref name) = request.name {
            fields.push(format!("name = ${param_count}"));
            values.push(name);
            param_count += 1;
        }
        if let Some(ref input_type) = request.input_type {
            fields.push(format!("input_type = ${param_count}"));
            values.push(input_type);
            param_count += 1;
        }
        if let Some(ref is_required) = request.is_required {
            fields.push(format!("is_required = ${param_count}"));
            values.push(is_required);
            param_count += 1;
        }
        if let Some(ref display_order) = request.display_order {
            fields.push(format!("display_order = ${param_count}"));
            values.push(display_order);
            param_count += 1;
        }

        if fields.is_empty() {
            return self.get_organizer_type_by_id(id).await;
        }

        fields.push("updated_at = NOW()".to_string());
        values.push(&id);

        let query = format!(
            "UPDATE organizer_types SET {} WHERE id = ${} 
             RETURNING id, inventory_id, name, input_type, is_required, display_order, created_at, updated_at",
            fields.join(", "),
            param_count
        );

        let rows = client.query(&query, &values).await?;

        if let Some(row) = rows.first() {
            let organizer = OrganizerType {
                id: Some(row.get(0)),
                inventory_id: row.get(1),
                name: row.get(2),
                input_type: row.get(3),
                is_required: row.get(4),
                display_order: row.get(5),
                created_at: row.get::<_, Option<DateTime<Utc>>>(6),
                updated_at: row.get::<_, Option<DateTime<Utc>>>(7),
            };
            info!("Updated organizer type ID: {}", id);
            Ok(Some(organizer))
        } else {
            Ok(None)
        }
    }

    pub async fn delete_organizer_type(&self, id: i32) -> Result<bool, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows_affected = client
            .execute("DELETE FROM organizer_types WHERE id = $1", &[&id])
            .await?;

        let deleted = rows_affected > 0;
        if deleted {
            info!("Deleted organizer type ID: {}", id);
        }
        Ok(deleted)
    }

    // ==================== Organizer Option Operations ====================

    pub async fn get_organizer_options(
        &self,
        organizer_type_id: i32,
    ) -> Result<Vec<OrganizerOption>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT id, organizer_type_id, name, display_order, created_at, updated_at 
                 FROM organizer_options WHERE organizer_type_id = $1 ORDER BY display_order ASC, name ASC",
                &[&organizer_type_id],
            )
            .await?;

        let mut options = Vec::new();
        for row in rows {
            let option = OrganizerOption {
                id: Some(row.get(0)),
                organizer_type_id: row.get(1),
                name: row.get(2),
                display_order: row.get(3),
                created_at: row.get::<_, Option<DateTime<Utc>>>(4),
                updated_at: row.get::<_, Option<DateTime<Utc>>>(5),
            };
            options.push(option);
        }

        info!(
            "Retrieved {} options for organizer type {}",
            options.len(),
            organizer_type_id
        );
        Ok(options)
    }

    pub async fn get_organizer_option_by_id(
        &self,
        id: i32,
    ) -> Result<Option<OrganizerOption>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT id, organizer_type_id, name, display_order, created_at, updated_at 
                 FROM organizer_options WHERE id = $1",
                &[&id],
            )
            .await?;

        if let Some(row) = rows.first() {
            Ok(Some(OrganizerOption {
                id: Some(row.get(0)),
                organizer_type_id: row.get(1),
                name: row.get(2),
                display_order: row.get(3),
                created_at: row.get::<_, Option<DateTime<Utc>>>(4),
                updated_at: row.get::<_, Option<DateTime<Utc>>>(5),
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn create_organizer_option(
        &self,
        organizer_type_id: i32,
        request: CreateOrganizerOptionRequest,
    ) -> Result<OrganizerOption, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let display_order = request.display_order.unwrap_or(0);

        let row = client
            .query_one(
                "INSERT INTO organizer_options (organizer_type_id, name, display_order) 
                 VALUES ($1, $2, $3) 
                 RETURNING id, organizer_type_id, name, display_order, created_at, updated_at",
                &[&organizer_type_id, &request.name, &display_order],
            )
            .await?;

        let option = OrganizerOption {
            id: Some(row.get(0)),
            organizer_type_id: row.get(1),
            name: row.get(2),
            display_order: row.get(3),
            created_at: row.get::<_, Option<DateTime<Utc>>>(4),
            updated_at: row.get::<_, Option<DateTime<Utc>>>(5),
        };

        info!(
            "Created organizer option: {} (ID: {:?})",
            option.name, option.id
        );
        Ok(option)
    }

    pub async fn update_organizer_option(
        &self,
        id: i32,
        request: UpdateOrganizerOptionRequest,
    ) -> Result<Option<OrganizerOption>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let mut fields = Vec::new();
        let mut values: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::new();
        let mut param_count = 1;

        if let Some(ref name) = request.name {
            fields.push(format!("name = ${param_count}"));
            values.push(name);
            param_count += 1;
        }
        if let Some(ref display_order) = request.display_order {
            fields.push(format!("display_order = ${param_count}"));
            values.push(display_order);
            param_count += 1;
        }

        if fields.is_empty() {
            return self.get_organizer_option_by_id(id).await;
        }

        fields.push("updated_at = NOW()".to_string());
        values.push(&id);

        let query = format!(
            "UPDATE organizer_options SET {} WHERE id = ${} 
             RETURNING id, organizer_type_id, name, display_order, created_at, updated_at",
            fields.join(", "),
            param_count
        );

        let rows = client.query(&query, &values).await?;

        if let Some(row) = rows.first() {
            let option = OrganizerOption {
                id: Some(row.get(0)),
                organizer_type_id: row.get(1),
                name: row.get(2),
                display_order: row.get(3),
                created_at: row.get::<_, Option<DateTime<Utc>>>(4),
                updated_at: row.get::<_, Option<DateTime<Utc>>>(5),
            };
            info!("Updated organizer option ID: {}", id);
            Ok(Some(option))
        } else {
            Ok(None)
        }
    }

    pub async fn delete_organizer_option(
        &self,
        id: i32,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows_affected = client
            .execute("DELETE FROM organizer_options WHERE id = $1", &[&id])
            .await?;

        let deleted = rows_affected > 0;
        if deleted {
            info!("Deleted organizer option ID: {}", id);
        }
        Ok(deleted)
    }

    // ==================== Item Organizer Value Operations ====================

    pub async fn get_item_organizer_values(
        &self,
        item_id: i32,
    ) -> Result<Vec<ItemOrganizerValueWithDetails>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT 
                    iov.organizer_type_id,
                    ot.name as organizer_type_name,
                    ot.input_type,
                    ot.is_required,
                    COALESCE(oo.name, iov.text_value) as display_value,
                    iov.organizer_option_id,
                    iov.text_value
                 FROM item_organizer_values iov
                 JOIN organizer_types ot ON iov.organizer_type_id = ot.id
                 LEFT JOIN organizer_options oo ON iov.organizer_option_id = oo.id
                 WHERE iov.item_id = $1
                 ORDER BY ot.display_order ASC, ot.name ASC",
                &[&item_id],
            )
            .await?;

        let mut values = Vec::new();
        for row in rows {
            let value = ItemOrganizerValueWithDetails {
                organizer_type_id: row.get(0),
                organizer_type_name: row.get(1),
                input_type: row.get(2),
                is_required: row.get(3),
                value: row.get(4),
                organizer_option_id: row.get(5),
                text_value: row.get(6),
            };
            values.push(value);
        }

        info!(
            "Retrieved {} organizer values for item {}",
            values.len(),
            item_id
        );
        Ok(values)
    }

    pub async fn set_item_organizer_value(
        &self,
        item_id: i32,
        request: SetItemOrganizerValueRequest,
    ) -> Result<ItemOrganizerValue, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        // Use UPSERT to insert or update the value
        let row = client
            .query_one(
                "INSERT INTO item_organizer_values (item_id, organizer_type_id, organizer_option_id, text_value) 
                 VALUES ($1, $2, $3, $4) 
                 ON CONFLICT (item_id, organizer_type_id) 
                 DO UPDATE SET organizer_option_id = $3, text_value = $4, updated_at = NOW()
                 RETURNING id, item_id, organizer_type_id, organizer_option_id, text_value, created_at, updated_at",
                &[&item_id, &request.organizer_type_id, &request.organizer_option_id, &request.text_value],
            )
            .await?;

        let value = ItemOrganizerValue {
            id: Some(row.get(0)),
            item_id: row.get(1),
            organizer_type_id: row.get(2),
            organizer_option_id: row.get(3),
            text_value: row.get(4),
            created_at: row.get::<_, Option<DateTime<Utc>>>(5),
            updated_at: row.get::<_, Option<DateTime<Utc>>>(6),
        };

        info!(
            "Set organizer value for item {} type {}",
            item_id, request.organizer_type_id
        );
        Ok(value)
    }

    pub async fn set_item_organizer_values(
        &self,
        item_id: i32,
        values: Vec<SetItemOrganizerValueRequest>,
    ) -> Result<Vec<ItemOrganizerValue>, Box<dyn std::error::Error>> {
        let mut results = Vec::new();
        for request in values {
            let result = self.set_item_organizer_value(item_id, request).await?;
            results.push(result);
        }
        Ok(results)
    }

    pub async fn delete_item_organizer_value(
        &self,
        item_id: i32,
        organizer_type_id: i32,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows_affected = client
            .execute(
                "DELETE FROM item_organizer_values WHERE item_id = $1 AND organizer_type_id = $2",
                &[&item_id, &organizer_type_id],
            )
            .await?;

        let deleted = rows_affected > 0;
        if deleted {
            info!(
                "Deleted organizer value for item {} type {}",
                item_id, organizer_type_id
            );
        }
        Ok(deleted)
    }

    #[allow(dead_code)]
    pub async fn clear_item_organizer_values(
        &self,
        item_id: i32,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows_affected = client
            .execute(
                "DELETE FROM item_organizer_values WHERE item_id = $1",
                &[&item_id],
            )
            .await?;

        info!(
            "Cleared {} organizer values for item {}",
            rows_affected, item_id
        );
        Ok(rows_affected)
    }

    // ==================== Item Image Operations ====================

    /// Bulk-fetch item images for an inventory.
    /// Returns a map of `item_id` → `image_url` for all items that have an image organizer value.
    /// This avoids N+1 queries when rendering item card thumbnails.
    pub async fn get_item_image_urls_by_inventory(
        &self,
        inventory_id: i32,
    ) -> Result<std::collections::HashMap<i32, String>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT iov.item_id, iov.text_value
                 FROM item_organizer_values iov
                 JOIN organizer_types ot ON iov.organizer_type_id = ot.id
                 JOIN items i ON iov.item_id = i.id
                 WHERE ot.inventory_id = $1
                   AND ot.input_type = 'image'
                   AND iov.text_value IS NOT NULL
                   AND iov.text_value != ''
                 ORDER BY ot.display_order ASC",
                &[&inventory_id],
            )
            .await?;

        let mut image_map = std::collections::HashMap::new();
        for row in rows {
            let item_id: i32 = row.get(0);
            let image_url: String = row.get(1);
            // First image organizer wins (by display_order)
            image_map.entry(item_id).or_insert(image_url);
        }

        info!(
            "Retrieved {} item image URLs for inventory {}",
            image_map.len(),
            inventory_id
        );
        Ok(image_map)
    }

    // ==================== User Operations ====================

    /// Get user count for setup status check
    pub async fn get_user_count(&self) -> Result<i64, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;
        let row = client.query_one("SELECT COUNT(*) FROM users", &[]).await?;
        Ok(row.get(0))
    }

    /// Get a user by ID
    pub async fn get_user_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<User>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT id, username, full_name, password_hash, is_admin, is_active, created_at, updated_at,
                        recovery_codes_generated_at, COALESCE(recovery_codes_confirmed, false)
                 FROM users WHERE id = $1",
                &[&id],
            )
            .await?;

        if let Some(row) = rows.first() {
            Ok(Some(User {
                id: row.get(0),
                username: row.get(1),
                full_name: row.get(2),
                password_hash: row.get(3),
                is_admin: row.get(4),
                is_active: row.get(5),
                created_at: row.get(6),
                updated_at: row.get(7),
                recovery_codes_generated_at: row.get(8),
                recovery_codes_confirmed: row.get(9),
            }))
        } else {
            Ok(None)
        }
    }

    /// Get a user by username
    pub async fn get_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<User>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT id, username, full_name, password_hash, is_admin, is_active, created_at, updated_at,
                        recovery_codes_generated_at, COALESCE(recovery_codes_confirmed, false)
                 FROM users WHERE LOWER(username) = LOWER($1)",
                &[&username],
            )
            .await?;

        if let Some(row) = rows.first() {
            Ok(Some(User {
                id: row.get(0),
                username: row.get(1),
                full_name: row.get(2),
                password_hash: row.get(3),
                is_admin: row.get(4),
                is_active: row.get(5),
                created_at: row.get(6),
                updated_at: row.get(7),
                recovery_codes_generated_at: row.get(8),
                recovery_codes_confirmed: row.get(9),
            }))
        } else {
            Ok(None)
        }
    }

    /// Get all users (admin only)
    pub async fn get_all_users(&self) -> Result<Vec<UserResponse>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT id, username, full_name, is_admin, is_active, created_at, updated_at 
                 FROM users ORDER BY created_at DESC",
                &[],
            )
            .await?;

        let users = rows
            .iter()
            .map(|row| UserResponse {
                id: row.get(0),
                username: row.get(1),
                full_name: row.get(2),
                is_admin: row.get(3),
                is_active: row.get(4),
                created_at: row.get(5),
                updated_at: row.get(6),
            })
            .collect();

        Ok(users)
    }

    /// Create a new user
    pub async fn create_user(
        &self,
        username: &str,
        full_name: &str,
        password_hash: &str,
        is_admin: bool,
        is_active: bool,
    ) -> Result<User, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let row = client
            .query_one(
                "INSERT INTO users (username, full_name, password_hash, is_admin, is_active) 
                 VALUES ($1, $2, $3, $4, $5) 
                 RETURNING id, username, full_name, password_hash, is_admin, is_active, created_at, updated_at",
                &[&username, &full_name, &password_hash, &is_admin, &is_active],
            )
            .await?;

        let user = User {
            id: row.get(0),
            username: row.get(1),
            full_name: row.get(2),
            password_hash: row.get(3),
            is_admin: row.get(4),
            is_active: row.get(5),
            created_at: row.get(6),
            updated_at: row.get(7),
            recovery_codes_generated_at: None,
            recovery_codes_confirmed: false,
        };

        info!("Created new user: {} (ID: {})", user.username, user.id);
        Ok(user)
    }

    /// Update a user's profile
    pub async fn update_user_profile(
        &self,
        id: Uuid,
        full_name: Option<&str>,
    ) -> Result<Option<User>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let mut fields = Vec::new();
        let mut values: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::new();
        let mut param_count = 1;

        if let Some(ref n) = full_name {
            fields.push(format!("full_name = ${param_count}"));
            values.push(n);
            param_count += 1;
        }

        if fields.is_empty() {
            return self.get_user_by_id(id).await;
        }

        fields.push("updated_at = NOW()".to_string());
        values.push(&id);

        let query = format!(
            "UPDATE users SET {} WHERE id = ${} 
             RETURNING id, username, full_name, password_hash, is_admin, is_active, created_at, updated_at,
                       recovery_codes_generated_at, COALESCE(recovery_codes_confirmed, false)",
            fields.join(", "),
            param_count
        );

        let rows = client.query(&query, &values).await?;

        if let Some(row) = rows.first() {
            Ok(Some(User {
                id: row.get(0),
                username: row.get(1),
                full_name: row.get(2),
                password_hash: row.get(3),
                is_admin: row.get(4),
                is_active: row.get(5),
                created_at: row.get(6),
                updated_at: row.get(7),
                recovery_codes_generated_at: row.get(8),
                recovery_codes_confirmed: row.get(9),
            }))
        } else {
            Ok(None)
        }
    }

    /// Admin update user
    pub async fn admin_update_user(
        &self,
        id: Uuid,
        request: AdminUpdateUserRequest,
    ) -> Result<Option<User>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let mut fields = Vec::new();
        let mut values: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::new();
        let mut param_count = 1;

        if let Some(ref username) = request.username {
            fields.push(format!("username = ${param_count}"));
            values.push(username);
            param_count += 1;
        }
        if let Some(ref full_name) = request.full_name {
            fields.push(format!("full_name = ${param_count}"));
            values.push(full_name);
            param_count += 1;
        }
        if let Some(ref is_admin) = request.is_admin {
            fields.push(format!("is_admin = ${param_count}"));
            values.push(is_admin);
            param_count += 1;
        }
        if let Some(ref is_active) = request.is_active {
            fields.push(format!("is_active = ${param_count}"));
            values.push(is_active);
            param_count += 1;
        }

        if fields.is_empty() {
            return self.get_user_by_id(id).await;
        }

        fields.push("updated_at = NOW()".to_string());
        values.push(&id);

        let query = format!(
            "UPDATE users SET {} WHERE id = ${} 
             RETURNING id, username, full_name, password_hash, is_admin, is_active, created_at, updated_at,
                       recovery_codes_generated_at, COALESCE(recovery_codes_confirmed, false)",
            fields.join(", "),
            param_count
        );

        let rows = client.query(&query, &values).await?;

        if let Some(row) = rows.first() {
            Ok(Some(User {
                id: row.get(0),
                username: row.get(1),
                full_name: row.get(2),
                password_hash: row.get(3),
                is_admin: row.get(4),
                is_active: row.get(5),
                created_at: row.get(6),
                updated_at: row.get(7),
                recovery_codes_generated_at: row.get(8),
                recovery_codes_confirmed: row.get(9),
            }))
        } else {
            Ok(None)
        }
    }

    /// Update user password
    pub async fn update_user_password(
        &self,
        id: Uuid,
        password_hash: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows_affected = client
            .execute(
                "UPDATE users SET password_hash = $1, updated_at = NOW() WHERE id = $2",
                &[&password_hash, &id],
            )
            .await?;

        Ok(rows_affected > 0)
    }

    /// Delete a user
    pub async fn delete_user(&self, id: Uuid) -> Result<bool, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows_affected = client
            .execute("DELETE FROM users WHERE id = $1", &[&id])
            .await?;

        let deleted = rows_affected > 0;
        if deleted {
            info!("Deleted user ID: {}", id);
        }
        Ok(deleted)
    }

    /// Count admin users
    pub async fn count_admin_users(&self) -> Result<i64, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;
        let row = client
            .query_one("SELECT COUNT(*) FROM users WHERE is_admin = true", &[])
            .await?;
        Ok(row.get(0))
    }

    // ==================== User Settings Operations ====================

    /// Get user settings
    pub async fn get_user_settings(
        &self,
        user_id: Uuid,
    ) -> Result<Option<UserSettings>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT id, user_id, theme, default_inventory_id, items_per_page, date_format, 
                        currency, notifications_enabled, settings_json, created_at, updated_at 
                 FROM user_settings WHERE user_id = $1",
                &[&user_id],
            )
            .await?;

        if let Some(row) = rows.first() {
            Ok(Some(UserSettings {
                id: row.get(0),
                user_id: row.get(1),
                theme: row.get(2),
                default_inventory_id: row.get(3),
                items_per_page: row.get(4),
                date_format: row.get(5),
                currency: row.get(6),
                notifications_enabled: row.get(7),
                settings_json: row.get(8),
                created_at: row.get(9),
                updated_at: row.get(10),
            }))
        } else {
            Ok(None)
        }
    }

    /// Create default user settings
    pub async fn create_user_settings(
        &self,
        user_id: Uuid,
    ) -> Result<UserSettings, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let row = client
            .query_one(
                "INSERT INTO user_settings (user_id) VALUES ($1) 
                 RETURNING id, user_id, theme, default_inventory_id, items_per_page, date_format, 
                           currency, notifications_enabled, settings_json, created_at, updated_at",
                &[&user_id],
            )
            .await?;

        Ok(UserSettings {
            id: row.get(0),
            user_id: row.get(1),
            theme: row.get(2),
            default_inventory_id: row.get(3),
            items_per_page: row.get(4),
            date_format: row.get(5),
            currency: row.get(6),
            notifications_enabled: row.get(7),
            settings_json: row.get(8),
            created_at: row.get(9),
            updated_at: row.get(10),
        })
    }

    /// Update user settings
    pub async fn update_user_settings(
        &self,
        user_id: Uuid,
        request: UpdateUserSettingsRequest,
    ) -> Result<Option<UserSettings>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let mut fields = Vec::new();
        let mut values: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::new();
        let mut param_count = 1;

        if let Some(ref theme) = request.theme {
            fields.push(format!("theme = ${param_count}"));
            values.push(theme);
            param_count += 1;
        }
        // BUG FIX: Handle special sentinel value 0 as "set to NULL" to allow clearing the setting.
        // Frontend sends 0 when user selects "None" to avoid JSON serialization issues with undefined.
        if let Some(ref default_inventory_id_value) = request.default_inventory_id {
            if *default_inventory_id_value == 0 {
                // Special case: 0 means clear the default inventory setting
                fields.push("default_inventory_id = NULL".to_string());
                // No parameter needed - NULL is inline in SQL
            } else {
                fields.push(format!("default_inventory_id = ${param_count}"));
                values.push(default_inventory_id_value);
                param_count += 1;
            }
        }
        if let Some(ref items_per_page) = request.items_per_page {
            fields.push(format!("items_per_page = ${param_count}"));
            values.push(items_per_page);
            param_count += 1;
        }
        if let Some(ref date_format) = request.date_format {
            fields.push(format!("date_format = ${param_count}"));
            values.push(date_format);
            param_count += 1;
        }
        if let Some(ref currency) = request.currency {
            fields.push(format!("currency = ${param_count}"));
            values.push(currency);
            param_count += 1;
        }
        if let Some(ref notifications_enabled) = request.notifications_enabled {
            fields.push(format!("notifications_enabled = ${param_count}"));
            values.push(notifications_enabled);
            param_count += 1;
        }
        if let Some(ref settings_json) = request.settings_json {
            fields.push(format!("settings_json = ${param_count}"));
            values.push(settings_json);
            param_count += 1;
        }

        if fields.is_empty() {
            return self.get_user_settings(user_id).await;
        }

        fields.push("updated_at = NOW()".to_string());
        values.push(&user_id);

        let query = format!(
            "UPDATE user_settings SET {} WHERE user_id = ${} 
             RETURNING id, user_id, theme, default_inventory_id, items_per_page, date_format, 
                       currency, notifications_enabled, settings_json, created_at, updated_at",
            fields.join(", "),
            param_count
        );

        let rows = client.query(&query, &values).await?;

        if let Some(row) = rows.first() {
            Ok(Some(UserSettings {
                id: row.get(0),
                user_id: row.get(1),
                theme: row.get(2),
                default_inventory_id: row.get(3),
                items_per_page: row.get(4),
                date_format: row.get(5),
                currency: row.get(6),
                notifications_enabled: row.get(7),
                settings_json: row.get(8),
                created_at: row.get(9),
                updated_at: row.get(10),
            }))
        } else {
            Ok(None)
        }
    }

    /// Get or create user settings
    pub async fn get_or_create_user_settings(
        &self,
        user_id: Uuid,
    ) -> Result<UserSettings, Box<dyn std::error::Error>> {
        if let Some(settings) = self.get_user_settings(user_id).await? {
            Ok(settings)
        } else {
            self.create_user_settings(user_id).await
        }
    }

    // ==================== Inventory Sharing Operations ====================

    /// Share an inventory with a user
    pub async fn create_inventory_share(
        &self,
        inventory_id: i32,
        shared_with_user_id: Uuid,
        shared_by_user_id: Uuid,
        permission_level: PermissionLevel,
    ) -> Result<InventoryShare, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let permission_str = permission_level.to_string();
        let row = client
            .query_one(
                "INSERT INTO inventory_shares (inventory_id, shared_with_user_id, shared_by_user_id, permission_level) 
                 VALUES ($1, $2, $3, $4) 
                 RETURNING id, inventory_id, shared_with_user_id, shared_by_user_id, permission_level, created_at, updated_at",
                &[&inventory_id, &shared_with_user_id, &shared_by_user_id, &permission_str],
            )
            .await?;

        let perm_str: String = row.get(4);
        Ok(InventoryShare {
            id: row.get(0),
            inventory_id: row.get(1),
            shared_with_user_id: row.get(2),
            shared_by_user_id: row.get(3),
            permission_level: perm_str.parse().unwrap_or(PermissionLevel::View),
            created_at: row.get(5),
            updated_at: row.get(6),
        })
    }

    /// Get shares for an inventory
    pub async fn get_inventory_shares(
        &self,
        inventory_id: i32,
    ) -> Result<Vec<InventoryShareWithUser>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT 
                    s.id, s.inventory_id, s.permission_level, s.created_at, s.updated_at,
                    sw.id, sw.username, sw.full_name, sw.is_admin, sw.is_active, sw.created_at, sw.updated_at,
                    sb.id, sb.username, sb.full_name, sb.is_admin, sb.is_active, sb.created_at, sb.updated_at
                 FROM inventory_shares s
                 JOIN users sw ON s.shared_with_user_id = sw.id
                 JOIN users sb ON s.shared_by_user_id = sb.id
                 WHERE s.inventory_id = $1
                 ORDER BY s.created_at DESC",
                &[&inventory_id],
            )
            .await?;

        let shares = rows
            .iter()
            .map(|row| {
                let perm_str: String = row.get(2);
                InventoryShareWithUser {
                    id: row.get(0),
                    inventory_id: row.get(1),
                    permission_level: perm_str.parse().unwrap_or(PermissionLevel::View),
                    created_at: row.get(3),
                    updated_at: row.get(4),
                    shared_with_user: UserResponse {
                        id: row.get(5),
                        username: row.get(6),
                        full_name: row.get(7),
                        is_admin: row.get(8),
                        is_active: row.get(9),
                        created_at: row.get(10),
                        updated_at: row.get(11),
                    },
                    shared_by_user: UserResponse {
                        id: row.get(12),
                        username: row.get(13),
                        full_name: row.get(14),
                        is_admin: row.get(15),
                        is_active: row.get(16),
                        created_at: row.get(17),
                        updated_at: row.get(18),
                    },
                }
            })
            .collect();

        Ok(shares)
    }

    /// Get comprehensive effective permissions for a user on an inventory
    pub async fn get_effective_permissions(
        &self,
        user_id: Uuid,
        inventory_id: i32,
    ) -> Result<EffectivePermissions, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        // Check if user is the owner
        let owner_rows = client
            .query(
                "SELECT user_id FROM inventories WHERE id = $1",
                &[&inventory_id],
            )
            .await?;

        if let Some(row) = owner_rows.first() {
            let owner_id: Option<Uuid> = row.get(0);
            if owner_id == Some(user_id) {
                return Ok(EffectivePermissions {
                    can_view: true,
                    can_edit_items: true,
                    can_add_items: true,
                    can_remove_items: true,
                    can_edit_inventory: true,
                    can_delete_inventory: true,
                    can_manage_sharing: true,
                    can_manage_organizers: true,
                    is_owner: true,
                    has_all_access: false,
                    permission_source: PermissionSource::Owner,
                });
            }

            // Check for All Access grant from the owner
            if let Some(owner_uuid) = owner_id {
                let all_access_rows = client
                    .query(
                        "SELECT id FROM user_access_grants 
                         WHERE grantor_user_id = $1 AND grantee_user_id = $2",
                        &[&owner_uuid, &user_id],
                    )
                    .await?;

                if !all_access_rows.is_empty() {
                    return Ok(EffectivePermissions {
                        can_view: true,
                        can_edit_items: true,
                        can_add_items: true,
                        can_remove_items: true,
                        can_edit_inventory: true,
                        can_delete_inventory: true,
                        can_manage_sharing: true,
                        can_manage_organizers: true,
                        is_owner: false,
                        has_all_access: true,
                        permission_source: PermissionSource::AllAccess,
                    });
                }
            }
        }

        // Check for per-inventory share
        let share_rows = client
            .query(
                "SELECT permission_level FROM inventory_shares 
                 WHERE inventory_id = $1 AND shared_with_user_id = $2",
                &[&inventory_id, &user_id],
            )
            .await?;

        if let Some(row) = share_rows.first() {
            let perm_str: String = row.get(0);
            let permission = perm_str.parse().unwrap_or(PermissionLevel::View);

            return Ok(EffectivePermissions {
                can_view: permission.can_view(),
                can_edit_items: permission.can_edit_items(),
                can_add_items: permission.can_add_items(),
                can_remove_items: permission.can_remove_items(),
                can_edit_inventory: permission.can_edit_inventory(),
                can_delete_inventory: false, // Only owner or AllAccess can delete
                can_manage_sharing: false,   // Only owner or AllAccess can manage sharing
                can_manage_organizers: permission.can_manage_organizers(),
                is_owner: false,
                has_all_access: false,
                permission_source: PermissionSource::InventoryShare,
            });
        }

        // No access
        Ok(EffectivePermissions {
            can_view: false,
            can_edit_items: false,
            can_add_items: false,
            can_remove_items: false,
            can_edit_inventory: false,
            can_delete_inventory: false,
            can_manage_sharing: false,
            can_manage_organizers: false,
            is_owner: false,
            has_all_access: false,
            permission_source: PermissionSource::None,
        })
    }

    /// Update inventory share permission
    pub async fn update_inventory_share(
        &self,
        share_id: Uuid,
        permission_level: PermissionLevel,
    ) -> Result<Option<InventoryShare>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let permission_str = permission_level.to_string();
        let rows = client
            .query(
                "UPDATE inventory_shares SET permission_level = $1, updated_at = NOW() 
                 WHERE id = $2 
                 RETURNING id, inventory_id, shared_with_user_id, shared_by_user_id, permission_level, created_at, updated_at",
                &[&permission_str, &share_id],
            )
            .await?;

        if let Some(row) = rows.first() {
            let perm_str: String = row.get(4);
            Ok(Some(InventoryShare {
                id: row.get(0),
                inventory_id: row.get(1),
                shared_with_user_id: row.get(2),
                shared_by_user_id: row.get(3),
                permission_level: perm_str.parse().unwrap_or(PermissionLevel::View),
                created_at: row.get(5),
                updated_at: row.get(6),
            }))
        } else {
            Ok(None)
        }
    }

    /// Delete inventory share
    pub async fn delete_inventory_share(
        &self,
        share_id: Uuid,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows_affected = client
            .execute("DELETE FROM inventory_shares WHERE id = $1", &[&share_id])
            .await?;

        Ok(rows_affected > 0)
    }

    /// Get inventories accessible to a user (owned, shared via `inventory_shares`, or via All Access grants)
    pub async fn get_accessible_inventories(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<Inventory>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        // Query includes:
        // 1. Inventories owned by the user (i.user_id = $1)
        // 2. Inventories shared directly with the user (inventory_shares)
        // 3. Inventories owned by users who granted All Access to this user (user_access_grants)
        let rows = client
            .query(
                "SELECT DISTINCT i.id, i.name, i.description, i.location, i.image_url, i.user_id, i.created_at, i.updated_at 
                 FROM inventories i
                 LEFT JOIN inventory_shares s ON i.id = s.inventory_id AND s.shared_with_user_id = $1
                 LEFT JOIN user_access_grants g ON i.user_id = g.grantor_user_id AND g.grantee_user_id = $1
                 WHERE i.user_id = $1 
                    OR s.shared_with_user_id = $1
                    OR g.grantee_user_id = $1
                 ORDER BY i.name ASC",
                &[&user_id],
            )
            .await?;

        let inventories = rows
            .iter()
            .map(|row| Inventory {
                id: Some(row.get(0)),
                name: row.get(1),
                description: row.get(2),
                location: row.get(3),
                image_url: row.get(4),
                user_id: row.get(5),
                created_at: row.get::<_, Option<DateTime<Utc>>>(6),
                updated_at: row.get::<_, Option<DateTime<Utc>>>(7),
            })
            .collect();

        Ok(inventories)
    }

    // ==================== User Access Grant Operations (All Access Tier) ====================

    /// Create a user access grant (All Access tier)
    pub async fn create_user_access_grant(
        &self,
        grantor_user_id: Uuid,
        grantee_user_id: Uuid,
    ) -> Result<UserAccessGrant, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let row = client
            .query_one(
                "INSERT INTO user_access_grants (grantor_user_id, grantee_user_id) 
                 VALUES ($1, $2) 
                 RETURNING id, grantor_user_id, grantee_user_id, created_at, updated_at",
                &[&grantor_user_id, &grantee_user_id],
            )
            .await?;

        Ok(UserAccessGrant {
            id: row.get(0),
            grantor_user_id: row.get(1),
            grantee_user_id: row.get(2),
            created_at: row.get(3),
            updated_at: row.get(4),
        })
    }

    /// Get all access grants where the user is the grantor (people who can access my inventories)
    pub async fn get_user_access_grants_by_grantor(
        &self,
        grantor_user_id: Uuid,
    ) -> Result<Vec<UserAccessGrantWithUsers>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT 
                    g.id, g.created_at, g.updated_at,
                    gr.id, gr.username, gr.full_name, gr.is_admin, gr.is_active, gr.created_at, gr.updated_at,
                    ge.id, ge.username, ge.full_name, ge.is_admin, ge.is_active, ge.created_at, ge.updated_at
                 FROM user_access_grants g
                 JOIN users gr ON g.grantor_user_id = gr.id
                 JOIN users ge ON g.grantee_user_id = ge.id
                 WHERE g.grantor_user_id = $1
                 ORDER BY g.created_at DESC",
                &[&grantor_user_id],
            )
            .await?;

        let grants = rows
            .iter()
            .map(|row| UserAccessGrantWithUsers {
                id: row.get(0),
                created_at: row.get(1),
                updated_at: row.get(2),
                grantor: UserResponse {
                    id: row.get(3),
                    username: row.get(4),
                    full_name: row.get(5),
                    is_admin: row.get(6),
                    is_active: row.get(7),
                    created_at: row.get(8),
                    updated_at: row.get(9),
                },
                grantee: UserResponse {
                    id: row.get(10),
                    username: row.get(11),
                    full_name: row.get(12),
                    is_admin: row.get(13),
                    is_active: row.get(14),
                    created_at: row.get(15),
                    updated_at: row.get(16),
                },
            })
            .collect();

        Ok(grants)
    }

    /// Get all access grants where the user is the grantee (users who gave me access)
    pub async fn get_user_access_grants_by_grantee(
        &self,
        grantee_user_id: Uuid,
    ) -> Result<Vec<UserAccessGrantWithUsers>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT 
                    g.id, g.created_at, g.updated_at,
                    gr.id, gr.username, gr.full_name, gr.is_admin, gr.is_active, gr.created_at, gr.updated_at,
                    ge.id, ge.username, ge.full_name, ge.is_admin, ge.is_active, ge.created_at, ge.updated_at
                 FROM user_access_grants g
                 JOIN users gr ON g.grantor_user_id = gr.id
                 JOIN users ge ON g.grantee_user_id = ge.id
                 WHERE g.grantee_user_id = $1
                 ORDER BY g.created_at DESC",
                &[&grantee_user_id],
            )
            .await?;

        let grants = rows
            .iter()
            .map(|row| UserAccessGrantWithUsers {
                id: row.get(0),
                created_at: row.get(1),
                updated_at: row.get(2),
                grantor: UserResponse {
                    id: row.get(3),
                    username: row.get(4),
                    full_name: row.get(5),
                    is_admin: row.get(6),
                    is_active: row.get(7),
                    created_at: row.get(8),
                    updated_at: row.get(9),
                },
                grantee: UserResponse {
                    id: row.get(10),
                    username: row.get(11),
                    full_name: row.get(12),
                    is_admin: row.get(13),
                    is_active: row.get(14),
                    created_at: row.get(15),
                    updated_at: row.get(16),
                },
            })
            .collect();

        Ok(grants)
    }

    /// Delete a user access grant
    pub async fn delete_user_access_grant(
        &self,
        grant_id: Uuid,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows_affected = client
            .execute("DELETE FROM user_access_grants WHERE id = $1", &[&grant_id])
            .await?;

        Ok(rows_affected > 0)
    }

    /// Get a user access grant by ID
    pub async fn get_user_access_grant_by_id(
        &self,
        grant_id: Uuid,
    ) -> Result<Option<UserAccessGrant>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT id, grantor_user_id, grantee_user_id, created_at, updated_at 
                 FROM user_access_grants WHERE id = $1",
                &[&grant_id],
            )
            .await?;

        if let Some(row) = rows.first() {
            Ok(Some(UserAccessGrant {
                id: row.get(0),
                grantor_user_id: row.get(1),
                grantee_user_id: row.get(2),
                created_at: row.get(3),
                updated_at: row.get(4),
            }))
        } else {
            Ok(None)
        }
    }

    // ==================== Ownership Transfer Operations ====================

    /// Transfer ownership of an inventory from one user to another
    /// This operation:
    /// 1. Updates the inventory's `user_id` to the new owner
    /// 2. Removes all existing shares for the inventory (new owner controls sharing)
    /// 3. The previous owner loses all access
    pub async fn transfer_inventory_ownership(
        &self,
        inventory_id: i32,
        from_user_id: Uuid,
        to_user_id: Uuid,
    ) -> Result<(i64, i64), Box<dyn std::error::Error>> {
        let mut client = self.pool.get().await?;

        // Start a transaction for atomic operation
        let transaction = client.transaction().await?;

        // Verify the inventory exists and is owned by from_user_id
        let verify_result = transaction
            .query_opt(
                "SELECT id FROM inventories WHERE id = $1 AND user_id = $2",
                &[&inventory_id, &from_user_id],
            )
            .await?;

        if verify_result.is_none() {
            return Err("Inventory not found or you are not the owner".into());
        }

        // Verify the target user exists
        let target_user = transaction
            .query_opt(
                "SELECT id FROM users WHERE id = $1 AND is_active = true",
                &[&to_user_id],
            )
            .await?;

        if target_user.is_none() {
            return Err("Target user not found or is inactive".into());
        }

        // Count items that will be transferred (for reporting)
        let items_count: i64 = transaction
            .query_one(
                "SELECT COUNT(*) FROM items WHERE inventory_id = $1",
                &[&inventory_id],
            )
            .await?
            .get(0);

        // Transfer ownership by updating user_id
        transaction
            .execute(
                "UPDATE inventories SET user_id = $1, updated_at = NOW() WHERE id = $2",
                &[&to_user_id, &inventory_id],
            )
            .await?;

        // Remove all existing shares for this inventory (new owner will manage sharing)
        let shares_removed = transaction
            .execute(
                "DELETE FROM inventory_shares WHERE inventory_id = $1",
                &[&inventory_id],
            )
            .await?;

        // Commit the transaction
        transaction.commit().await?;

        info!(
            "Transferred ownership of inventory {} from {:?} to {:?}. Items: {}, Shares removed: {}",
            inventory_id, from_user_id, to_user_id, items_count, shares_removed
        );

        // Safe cast: shares_removed is clamped to i64::MAX, preventing wrap
        #[allow(
            clippy::cast_possible_wrap,
            reason = "Value is clamped to i64::MAX preventing wrap"
        )]
        let shares_removed_i64 = shares_removed.min(i64::MAX as u64) as i64;
        Ok((items_count, shares_removed_i64))
    }

    // ==================== Recovery Codes Methods ====================

    /// Store recovery codes for a user (deletes any existing codes first)
    pub async fn store_recovery_codes(
        &self,
        user_id: Uuid,
        code_hashes: Vec<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        // Delete any existing recovery codes for this user
        client
            .execute("DELETE FROM recovery_codes WHERE user_id = $1", &[&user_id])
            .await?;

        // Insert new codes
        for code_hash in code_hashes {
            client
                .execute(
                    "INSERT INTO recovery_codes (user_id, code_hash) VALUES ($1, $2)",
                    &[&user_id, &code_hash],
                )
                .await?;
        }

        // Update user's recovery codes timestamp and reset confirmation
        client
            .execute(
                "UPDATE users SET recovery_codes_generated_at = NOW(), recovery_codes_confirmed = false WHERE id = $1",
                &[&user_id],
            )
            .await?;

        info!("Stored {} recovery codes for user {}", 10, user_id);
        Ok(())
    }

    /// Confirm that user has saved their recovery codes
    pub async fn confirm_recovery_codes(
        &self,
        user_id: Uuid,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        client
            .execute(
                "UPDATE users SET recovery_codes_confirmed = true WHERE id = $1",
                &[&user_id],
            )
            .await?;

        info!("User {} confirmed saving recovery codes", user_id);
        Ok(())
    }

    /// Get all unused recovery code hashes for a user (for verification)
    pub async fn get_unused_recovery_codes(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(Uuid, String)>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT id, code_hash FROM recovery_codes WHERE user_id = $1 AND is_used = false",
                &[&user_id],
            )
            .await?;

        let codes: Vec<(Uuid, String)> = rows.iter().map(|row| (row.get(0), row.get(1))).collect();

        Ok(codes)
    }

    /// Mark a recovery code as used
    pub async fn mark_recovery_code_used(
        &self,
        code_id: Uuid,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        client
            .execute(
                "UPDATE recovery_codes SET is_used = true, used_at = NOW() WHERE id = $1",
                &[&code_id],
            )
            .await?;

        info!("Marked recovery code {} as used", code_id);
        Ok(())
    }

    /// Get count of unused recovery codes for a user
    pub async fn get_unused_recovery_codes_count(
        &self,
        user_id: Uuid,
    ) -> Result<i32, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let row = client
            .query_one(
                "SELECT COUNT(*)::int4 FROM recovery_codes WHERE user_id = $1 AND is_used = false",
                &[&user_id],
            )
            .await?;

        Ok(row.get(0))
    }

    /// Get recovery codes status for a user
    pub async fn get_recovery_codes_status(
        &self,
        user_id: Uuid,
    ) -> Result<(bool, bool, i32, Option<DateTime<Utc>>), Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        // Get user info
        let user_row = client
            .query_one(
                "SELECT recovery_codes_generated_at, COALESCE(recovery_codes_confirmed, false) FROM users WHERE id = $1",
                &[&user_id],
            )
            .await?;

        let generated_at: Option<DateTime<Utc>> = user_row.get(0);
        let confirmed: bool = user_row.get(1);

        // Get unused count
        let count_row = client
            .query_one(
                "SELECT COUNT(*)::int4 FROM recovery_codes WHERE user_id = $1 AND is_used = false",
                &[&user_id],
            )
            .await?;

        let unused_count: i32 = count_row.get(0);
        let has_codes = unused_count > 0;

        Ok((has_codes, confirmed, unused_count, generated_at))
    }

    // ==================== Inventory Reporting Operations ====================

    /// Check if user has access to a specific inventory
    pub async fn check_inventory_access(
        &self,
        user_id: Uuid,
        inventory_id: i32,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let row = client
            .query_one(
                "SELECT COUNT(*)::int8 as count FROM inventories 
                 WHERE id = $1 AND (
                     user_id = $2
                     OR id IN (SELECT inventory_id FROM inventory_shares WHERE shared_with_user_id = $2)
                     OR user_id IN (SELECT grantor_user_id FROM user_access_grants WHERE grantee_user_id = $2)
                 )",
                &[&inventory_id, &user_id],
            )
            .await?;

        let count: i64 = row.get(0);
        Ok(count > 0)
    }

    /// Retrieves filtered inventory items for report generation.
    ///
    /// This method enforces row-level security by only returning items from inventories
    /// that the user owns or has been granted access to via shares or access grants.
    ///
    /// # Arguments
    /// * `request` - Filter parameters (`inventory_id`, `category`, dates, prices, etc.)
    /// * `user_id` - UUID of the authenticated user making the request
    ///
    /// # Returns
    /// * `Ok(Vec<Item>)` - Filtered and sorted items accessible to the user
    /// * `Err(Box<dyn Error>)` - Database connection or query execution errors
    pub async fn get_inventory_report_data(
        &self,
        request: crate::models::InventoryReportRequest,
        user_id: Uuid,
    ) -> Result<Vec<crate::models::Item>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        // Build dynamic WHERE clause based on filters
        let mut conditions = vec![
            "i.inventory_id IN (
                SELECT id FROM inventories 
                WHERE user_id = $1
                   OR id IN (SELECT inventory_id FROM inventory_shares WHERE shared_with_user_id = $1)
                   OR user_id IN (SELECT grantor_user_id FROM user_access_grants WHERE grantee_user_id = $1)
            )".to_string()
        ];
        let mut param_index = 2;

        // Build parameters vector
        let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync>> = vec![Box::new(user_id)];

        // Add optional filters
        if let Some(inv_id) = request.inventory_id {
            conditions.push(format!("i.inventory_id = ${param_index}"));
            params.push(Box::new(inv_id));
            param_index += 1;
        }

        if let Some(ref category) = request.category {
            conditions.push(format!("i.category = ${param_index}"));
            params.push(Box::new(category.clone()));
            param_index += 1;
        }

        if let Some(ref location) = request.location {
            let pattern = format!("%{}%", escape_like_pattern(location));
            conditions.push(format!("i.location ILIKE ${param_index}"));
            params.push(Box::new(pattern));
            param_index += 1;
        }

        if let Some(ref from_date) = request.from_date {
            conditions.push(format!("i.purchase_date >= ${param_index}::date"));
            params.push(Box::new(from_date.clone()));
            param_index += 1;
        }

        if let Some(ref to_date) = request.to_date {
            conditions.push(format!("i.purchase_date <= ${param_index}::date"));
            params.push(Box::new(to_date.clone()));
            param_index += 1;
        }

        if let Some(min_price) = request.min_price {
            conditions.push(format!("i.purchase_price >= ${param_index}::float8"));
            params.push(Box::new(min_price));
            param_index += 1;
        }

        if let Some(max_price) = request.max_price {
            conditions.push(format!("i.purchase_price <= ${param_index}::float8"));
            params.push(Box::new(max_price));
            #[allow(unused_assignments)]
            {
                param_index += 1;
            }
        }

        // Build ORDER BY clause
        let order_by = build_order_by(&request);

        let query = format!(
            "SELECT i.id, i.inventory_id, i.name, i.description, i.category, i.location,
                    i.purchase_date::text, i.purchase_price::float8, i.warranty_expiry::text,
                    i.notes, i.quantity, i.created_at, i.updated_at
             FROM items i
             WHERE {}
             ORDER BY {}",
            conditions.join(" AND "),
            order_by
        );

        // Convert params to references for query
        let params_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params.iter().map(std::convert::AsRef::as_ref).collect();

        let rows = client.query(&query, &params_refs).await?;

        let items: Vec<crate::models::Item> = rows
            .iter()
            .map(|row| crate::models::Item {
                id: Some(row.get(0)),
                inventory_id: row.get(1),
                name: row.get(2),
                description: row.get(3),
                category: row.get(4),
                location: row.get(5),
                purchase_date: row.get::<_, Option<String>>(6),
                purchase_price: row.get(7),
                warranty_expiry: row.get::<_, Option<String>>(8),
                notes: row.get(9),
                quantity: row.get(10),
                created_at: row.get::<_, Option<DateTime<Utc>>>(11),
                updated_at: row.get::<_, Option<DateTime<Utc>>>(12),
            })
            .collect();

        info!(
            "Generated report with {} items for user {}",
            items.len(),
            user_id
        );
        Ok(items)
    }

    /// Calculates aggregated statistics across inventory items.
    ///
    /// Computes total item count, total value (price × quantity), average values,
    /// and date ranges for items. When `inventory_id` is None, aggregates across
    /// all inventories the user has access to.
    ///
    /// # Arguments
    /// * `inventory_id` - Optional inventory ID to limit statistics to one inventory
    /// * `user_id` - UUID of the authenticated user making the request
    ///
    /// # Returns
    /// * `Ok(InventoryStatistics)` - Aggregated statistics
    /// * `Err(Box<dyn Error>)` - Database connection or query execution errors
    pub async fn get_inventory_statistics(
        &self,
        inventory_id: Option<i32>,
        user_id: Uuid,
    ) -> Result<crate::models::InventoryStatistics, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let (query, params): (&str, Vec<Box<dyn tokio_postgres::types::ToSql + Sync>>) =
            if let Some(inv_id) = inventory_id {
                (
                    "SELECT 
                    COUNT(*)::int8 as total_items,
                    COALESCE(SUM(purchase_price::float8 * quantity), 0.0)::float8 as total_value,
                    COALESCE(SUM(quantity), 0)::int8 as total_quantity,
                    COUNT(DISTINCT category)::int8 as category_count,
                    1::int8 as inventories_count,
                    MIN(purchase_date)::text as oldest_item_date,
                    MAX(purchase_date)::text as newest_item_date,
                    COALESCE(AVG(purchase_price::float8), 0.0)::float8 as average_item_value
                 FROM items
                 WHERE inventory_id = $1",
                    vec![Box::new(inv_id)],
                )
            } else {
                (
                "SELECT 
                    COUNT(*)::int8 as total_items,
                    COALESCE(SUM(purchase_price::float8 * quantity), 0.0)::float8 as total_value,
                    COALESCE(SUM(quantity), 0)::int8 as total_quantity,
                    COUNT(DISTINCT category)::int8 as category_count,
                    COUNT(DISTINCT inventory_id)::int8 as inventories_count,
                    MIN(purchase_date)::text as oldest_item_date,
                    MAX(purchase_date)::text as newest_item_date,
                    COALESCE(AVG(purchase_price::float8), 0.0)::float8 as average_item_value
                 FROM items
                 WHERE inventory_id IN (
                     SELECT id FROM inventories 
                     WHERE user_id = $1
                        OR id IN (SELECT inventory_id FROM inventory_shares WHERE shared_with_user_id = $1)
                        OR user_id IN (SELECT grantor_user_id FROM user_access_grants WHERE grantee_user_id = $1)
                 )",
                vec![Box::new(user_id)],
            )
            };

        let params_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params.iter().map(std::convert::AsRef::as_ref).collect();

        let row = client.query_one(query, &params_refs).await?;

        let statistics = crate::models::InventoryStatistics {
            total_items: row.get(0),
            total_value: row.get(1),
            total_quantity: row.get(2),
            category_count: row.get(3),
            inventories_count: row.get(4),
            oldest_item_date: row.get(5),
            newest_item_date: row.get(6),
            average_item_value: row.get(7),
        };

        info!("Generated statistics for user {}", user_id);
        Ok(statistics)
    }

    /// Generates category breakdown with item counts and value percentages.
    ///
    /// Groups items by category and calculates total values, quantities, and
    /// percentage of total inventory value for each category. Uncategorized
    /// items are grouped under "Uncategorized".
    ///
    /// # Arguments
    /// * `inventory_id` - Optional inventory ID to limit breakdown to one inventory
    /// * `user_id` - UUID of the authenticated user making the request
    ///
    /// # Returns
    /// * `Ok(Vec<CategoryBreakdown>)` - Breakdown sorted by total value descending
    /// * `Err(Box<dyn Error>)` - Database connection or query execution errors
    pub async fn get_category_breakdown(
        &self,
        inventory_id: Option<i32>,
        user_id: Uuid,
    ) -> Result<Vec<crate::models::CategoryBreakdown>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let (query, params): (&str, Vec<Box<dyn tokio_postgres::types::ToSql + Sync>>) =
            if let Some(inv_id) = inventory_id {
                (
                "WITH totals AS (
                     SELECT COALESCE(SUM(purchase_price::float8 * quantity), 0.0)::float8 as grand_total
                     FROM items
                     WHERE inventory_id = $1
                 )
                 SELECT 
                     COALESCE(i.category, 'Uncategorized') as category,
                     COUNT(*)::int8 as item_count,
                     COALESCE(SUM(i.quantity), 0)::int8 as total_quantity,
                     COALESCE(SUM(i.purchase_price::float8 * i.quantity), 0.0)::float8 as total_value,
                     CASE 
                         WHEN t.grand_total > 0 THEN 
                             (COALESCE(SUM(i.purchase_price::float8 * i.quantity), 0.0) / t.grand_total * 100.0)::float8
                         ELSE 0.0::float8
                     END as percentage
                 FROM items i
                 CROSS JOIN totals t
                 WHERE i.inventory_id = $1
                 GROUP BY i.category, t.grand_total
                 ORDER BY total_value DESC",
                vec![Box::new(inv_id)],
            )
            } else {
                (
                "WITH totals AS (
                     SELECT COALESCE(SUM(purchase_price::float8 * quantity), 0.0)::float8 as grand_total
                     FROM items
                     WHERE inventory_id IN (
                         SELECT id FROM inventories 
                         WHERE user_id = $1
                            OR id IN (SELECT inventory_id FROM inventory_shares WHERE shared_with_user_id = $1)
                            OR user_id IN (SELECT grantor_user_id FROM user_access_grants WHERE grantee_user_id = $1)
                     )
                 )
                 SELECT 
                     COALESCE(i.category, 'Uncategorized') as category,
                     COUNT(*)::int8 as item_count,
                     COALESCE(SUM(i.quantity), 0)::int8 as total_quantity,
                     COALESCE(SUM(i.purchase_price::float8 * i.quantity), 0.0)::float8 as total_value,
                     CASE 
                         WHEN t.grand_total > 0 THEN 
                             (COALESCE(SUM(i.purchase_price::float8 * i.quantity), 0.0) / t.grand_total * 100.0)::float8
                         ELSE 0.0::float8
                     END as percentage
                 FROM items i
                 CROSS JOIN totals t
                 WHERE i.inventory_id IN (
                     SELECT id FROM inventories 
                     WHERE user_id = $1
                        OR id IN (SELECT inventory_id FROM inventory_shares WHERE shared_with_user_id = $1)
                        OR user_id IN (SELECT grantor_user_id FROM user_access_grants WHERE grantee_user_id = $1)
                 )
                 GROUP BY i.category, t.grand_total
                 ORDER BY total_value DESC",
                vec![Box::new(user_id)],
            )
            };

        let params_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params.iter().map(std::convert::AsRef::as_ref).collect();

        let rows = client.query(query, &params_refs).await?;

        let breakdown = rows
            .iter()
            .map(|row| crate::models::CategoryBreakdown {
                category: row.get(0),
                item_count: row.get(1),
                total_quantity: row.get(2),
                total_value: row.get(3),
                percentage_of_total: row.get(4),
            })
            .collect();

        info!("Generated category breakdown for user {}", user_id);
        Ok(breakdown)
    }

    // ==================== Backup & Restore Methods ====================

    /// Export all database tables as JSON values for backup
    pub async fn export_all_data(
        &self,
    ) -> Result<BackupDatabaseContent, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        // Helper closure to build the JSON export query for a table
        let build_export_query = |table: &str| {
            format!("SELECT COALESCE(jsonb_agg(to_jsonb(t)), '[]'::jsonb) FROM {table} t")
        };

        let users: serde_json::Value = client
            .query_one(&build_export_query("users"), &[])
            .await?
            .get(0);
        let inventories: serde_json::Value = client
            .query_one(&build_export_query("inventories"), &[])
            .await?
            .get(0);
        let items: serde_json::Value = client
            .query_one(&build_export_query("items"), &[])
            .await?
            .get(0);
        let categories: serde_json::Value = client
            .query_one(&build_export_query("categories"), &[])
            .await?
            .get(0);
        let tags: serde_json::Value = client
            .query_one(&build_export_query("tags"), &[])
            .await?
            .get(0);
        let item_tags: serde_json::Value = client
            .query_one(&build_export_query("item_tags"), &[])
            .await?
            .get(0);
        let custom_fields: serde_json::Value = client
            .query_one(&build_export_query("custom_fields"), &[])
            .await?
            .get(0);
        let item_custom_values: serde_json::Value = client
            .query_one(&build_export_query("item_custom_values"), &[])
            .await?
            .get(0);
        let organizer_types: serde_json::Value = client
            .query_one(&build_export_query("organizer_types"), &[])
            .await?
            .get(0);
        let organizer_options: serde_json::Value = client
            .query_one(&build_export_query("organizer_options"), &[])
            .await?
            .get(0);
        let item_organizer_values: serde_json::Value = client
            .query_one(&build_export_query("item_organizer_values"), &[])
            .await?
            .get(0);
        let user_settings: serde_json::Value = client
            .query_one(&build_export_query("user_settings"), &[])
            .await?
            .get(0);
        let inventory_shares: serde_json::Value = client
            .query_one(&build_export_query("inventory_shares"), &[])
            .await?
            .get(0);
        let user_access_grants: serde_json::Value = client
            .query_one(&build_export_query("user_access_grants"), &[])
            .await?
            .get(0);
        let recovery_codes: serde_json::Value = client
            .query_one(&build_export_query("recovery_codes"), &[])
            .await?
            .get(0);
        let password_reset_tokens: serde_json::Value = client
            .query_one(&build_export_query("password_reset_tokens"), &[])
            .await?
            .get(0);

        info!("Successfully exported all database tables for backup");

        Ok(BackupDatabaseContent {
            users,
            inventories,
            items,
            categories,
            tags,
            item_tags,
            custom_fields,
            item_custom_values,
            organizer_types,
            organizer_options,
            item_organizer_values,
            user_settings,
            inventory_shares,
            user_access_grants,
            recovery_codes,
            password_reset_tokens,
        })
    }

    /// Import all database tables from backup data (within a transaction)
    pub async fn import_all_data(
        &self,
        data: &BackupDatabaseContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut client = self.pool.get().await?;
        let transaction = client.transaction().await?;

        // Defer foreign key constraint checks until commit
        transaction
            .execute("SET CONSTRAINTS ALL DEFERRED", &[])
            .await?;

        // Truncate all tables in reverse dependency order
        let truncate_order = [
            "password_reset_tokens",
            "recovery_codes",
            "user_access_grants",
            "inventory_shares",
            "user_settings",
            "item_organizer_values",
            "organizer_options",
            "organizer_types",
            "item_custom_values",
            "custom_fields",
            "item_tags",
            "tags",
            "categories",
            "items",
            "inventories",
            "users",
        ];

        for table in &truncate_order {
            let query = format!("TRUNCATE TABLE {table} RESTART IDENTITY CASCADE");
            transaction.execute(query.as_str(), &[]).await?;
        }

        // Import tables in dependency order
        let import_order: Vec<(&str, &serde_json::Value)> = vec![
            ("users", &data.users),
            ("inventories", &data.inventories),
            ("items", &data.items),
            ("categories", &data.categories),
            ("tags", &data.tags),
            ("item_tags", &data.item_tags),
            ("custom_fields", &data.custom_fields),
            ("item_custom_values", &data.item_custom_values),
            ("organizer_types", &data.organizer_types),
            ("organizer_options", &data.organizer_options),
            ("item_organizer_values", &data.item_organizer_values),
            ("user_settings", &data.user_settings),
            ("inventory_shares", &data.inventory_shares),
            ("user_access_grants", &data.user_access_grants),
            ("recovery_codes", &data.recovery_codes),
            ("password_reset_tokens", &data.password_reset_tokens),
        ];

        for (table, rows_json) in &import_order {
            if let Some(rows) = rows_json.as_array() {
                for row in rows {
                    let query = format!(
                        "INSERT INTO {table} SELECT * FROM jsonb_populate_record(NULL::{table}, $1)"
                    );
                    transaction.execute(query.as_str(), &[row]).await?;
                }
            }
        }

        // Reset sequences for tables with serial/identity columns
        let sequence_tables = [
            "items",
            "inventories",
            "categories",
            "tags",
            "custom_fields",
            "item_custom_values",
            "item_tags",
            "organizer_types",
            "organizer_options",
            "item_organizer_values",
        ];

        for table in &sequence_tables {
            let query = format!(
                "SELECT setval(pg_get_serial_sequence('{table}', 'id'), \
                 COALESCE(MAX(id), 0) + 1, false) FROM {table}"
            );
            // Ignore errors for tables that may not have sequences
            if let Err(e) = transaction.execute(query.as_str(), &[]).await {
                info!(
                    "Note: Could not reset sequence for table {}: {} (this may be expected)",
                    table, e
                );
            }
        }

        transaction.commit().await?;
        info!("Successfully imported all database tables from backup");
        Ok(())
    }

    // ==================== TOTP Settings Operations ====================

    /// Create TOTP settings for a user (during setup, before verification)
    pub async fn create_totp_settings(
        &self,
        user_id: Uuid,
        encrypted_secret: &str,
    ) -> Result<TotpSettings, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let row = client
            .query_one(
                "INSERT INTO user_totp_settings (user_id, totp_secret_encrypted)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id) DO UPDATE SET
                     totp_secret_encrypted = $2,
                     is_enabled = false,
                     is_verified = false,
                     failed_attempts = 0,
                     last_failed_at = NULL,
                     updated_at = NOW()
                 RETURNING id, user_id, totp_secret_encrypted, totp_mode, is_enabled,
                           is_verified, created_at, updated_at, last_used_at,
                           failed_attempts, last_failed_at",
                &[&user_id, &encrypted_secret],
            )
            .await?;

        Ok(TotpSettings {
            id: row.get(0),
            user_id: row.get(1),
            totp_secret_encrypted: row.get(2),
            totp_mode: row.get(3),
            is_enabled: row.get(4),
            is_verified: row.get(5),
            created_at: row.get(6),
            updated_at: row.get(7),
            last_used_at: row.get(8),
            failed_attempts: row.get(9),
            last_failed_at: row.get(10),
        })
    }

    /// Get TOTP settings for a user
    pub async fn get_totp_settings(
        &self,
        user_id: Uuid,
    ) -> Result<Option<TotpSettings>, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                "SELECT id, user_id, totp_secret_encrypted, totp_mode, is_enabled,
                        is_verified, created_at, updated_at, last_used_at,
                        failed_attempts, last_failed_at
                 FROM user_totp_settings WHERE user_id = $1",
                &[&user_id],
            )
            .await?;

        if let Some(row) = rows.first() {
            Ok(Some(TotpSettings {
                id: row.get(0),
                user_id: row.get(1),
                totp_secret_encrypted: row.get(2),
                totp_mode: row.get(3),
                is_enabled: row.get(4),
                is_verified: row.get(5),
                created_at: row.get(6),
                updated_at: row.get(7),
                last_used_at: row.get(8),
                failed_attempts: row.get(9),
                last_failed_at: row.get(10),
            }))
        } else {
            Ok(None)
        }
    }

    /// Enable TOTP after successful verification (set enabled, verified, and mode)
    pub async fn enable_totp(
        &self,
        user_id: Uuid,
        mode: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        client
            .execute(
                "UPDATE user_totp_settings
                 SET is_enabled = true, is_verified = true, totp_mode = $2,
                     failed_attempts = 0, last_failed_at = NULL, updated_at = NOW()
                 WHERE user_id = $1",
                &[&user_id, &mode],
            )
            .await?;

        Ok(())
    }

    /// Update TOTP mode
    pub async fn update_totp_mode(
        &self,
        user_id: Uuid,
        mode: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        client
            .execute(
                "UPDATE user_totp_settings SET totp_mode = $2, updated_at = NOW()
                 WHERE user_id = $1 AND is_enabled = true",
                &[&user_id, &mode],
            )
            .await?;

        Ok(())
    }

    /// Update TOTP last used timestamp
    pub async fn update_totp_last_used(
        &self,
        user_id: Uuid,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        client
            .execute(
                "UPDATE user_totp_settings SET last_used_at = NOW(), updated_at = NOW()
                 WHERE user_id = $1",
                &[&user_id],
            )
            .await?;

        Ok(())
    }

    /// Delete TOTP settings (disable TOTP)
    pub async fn delete_totp_settings(
        &self,
        user_id: Uuid,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let rows_affected = client
            .execute(
                "DELETE FROM user_totp_settings WHERE user_id = $1",
                &[&user_id],
            )
            .await?;

        Ok(rows_affected > 0)
    }

    /// Increment failed TOTP attempts and record timestamp
    pub async fn increment_totp_failed_attempts(
        &self,
        user_id: Uuid,
    ) -> Result<i32, Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        let row = client
            .query_one(
                "UPDATE user_totp_settings
                 SET failed_attempts = failed_attempts + 1, last_failed_at = NOW()
                 WHERE user_id = $1
                 RETURNING failed_attempts",
                &[&user_id],
            )
            .await?;

        Ok(row.get(0))
    }

    /// Reset failed TOTP attempts after successful verification
    pub async fn reset_totp_failed_attempts(
        &self,
        user_id: Uuid,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        client
            .execute(
                "UPDATE user_totp_settings
                 SET failed_attempts = 0, last_failed_at = NULL
                 WHERE user_id = $1",
                &[&user_id],
            )
            .await?;

        Ok(())
    }
}

/// Helper function to build ORDER BY clause
fn build_order_by(request: &crate::models::InventoryReportRequest) -> String {
    let sort_by = request.sort_by.as_deref().unwrap_or("created_at");
    let sort_order = request.sort_order.as_deref().unwrap_or("desc");

    let column = match sort_by {
        "name" => "i.name",
        "price" => "i.purchase_price",
        "date" => "i.purchase_date",
        "category" => "i.category",
        _ => "i.created_at",
    };

    let order = if sort_order.eq_ignore_ascii_case("asc") {
        "ASC"
    } else {
        "DESC"
    };

    format!("{column} {order}")
}
