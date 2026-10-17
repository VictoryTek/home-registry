pub mod auth;
pub mod backup;
pub mod totp;

use crate::db::DatabaseService;
use crate::models::{
    ApiResponse, CreateInventoryRequest, CreateItemRequest, CreateOrganizerOptionRequest,
    CreateOrganizerTypeRequest, EffectivePermissions, ErrorResponse, ImageUploadResponse,
    InventoryReportData, InventoryReportRequest, Item, ItemExportRow,
    SetItemOrganizerValuesRequest, UpdateInventoryRequest, UpdateItemRequest,
    UpdateOrganizerOptionRequest, UpdateOrganizerTypeRequest,
};
use actix_multipart::form::{tempfile::TempFile, MultipartForm};
use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse, Responder, Result, Scope};
use deadpool_postgres::Pool;
use log::{error, info, warn};
use validator::Validate;

/// Validates that data URIs in `image_url` start with `data:image/` to prevent arbitrary data storage.
fn validate_image_url(image_url: Option<&str>) -> std::result::Result<(), String> {
    if let Some(url) = image_url {
        if url.starts_with("data:") && !url.starts_with("data:image/") {
            return Err("Invalid image data URI: must start with 'data:image/'".to_string());
        }
    }
    Ok(())
}

#[get("/")]
pub async fn index() -> impl Responder {
    // Serve the static HTML file instead of embedded HTML
    match std::fs::read_to_string("static/index.html") {
        Ok(content) => HttpResponse::Ok().content_type("text/html").body(content),
        Err(_) => HttpResponse::InternalServerError().body("Could not load index page"),
    }
}

#[get("/health")]
pub async fn api_health() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "message": "Home Inventory Manager is running",
        "timestamp": chrono::Utc::now()
    }))
}

/// Permission an endpoint requires on the target inventory.
#[derive(Clone, Copy, Debug)]
enum Perm {
    View,
    EditInventory,
    DeleteInventory,
    AddItems,
    EditItems,
    RemoveItems,
    ManageOrganizers,
    ManageSharing,
}

impl Perm {
    fn granted_by(self, p: &EffectivePermissions) -> bool {
        match self {
            Perm::View => p.can_view,
            Perm::EditInventory => p.can_edit_inventory,
            Perm::DeleteInventory => p.can_delete_inventory,
            Perm::AddItems => p.can_add_items,
            Perm::EditItems => p.can_edit_items,
            Perm::RemoveItems => p.can_remove_items,
            Perm::ManageOrganizers => p.can_manage_organizers,
            Perm::ManageSharing => p.can_manage_sharing,
        }
    }
}

fn internal_error_response(message: &str) -> HttpResponse {
    HttpResponse::InternalServerError().json(ErrorResponse {
        success: false,
        error: "An internal error occurred".to_string(),
        message: Some(message.to_string()),
    })
}

fn not_found_response(what: &str) -> HttpResponse {
    HttpResponse::NotFound().json(ErrorResponse {
        success: false,
        error: format!("{what} not found"),
        message: Some(format!("{what} not found")),
    })
}

/// Authenticate the caller and check they hold `perm` on `inventory_id`.
/// Unknown inventories and inventories the caller cannot view both yield 404 so ids do not leak.
async fn authorize_inventory(
    pool: &Pool,
    req: &HttpRequest,
    inventory_id: i32,
    perm: Perm,
) -> std::result::Result<crate::auth::AuthContext, HttpResponse> {
    let ctx = auth::get_auth_context_from_request(req, pool).await?;
    let db = DatabaseService::new(pool.clone());
    let perms = db
        .get_effective_permissions(ctx.user_id, inventory_id)
        .await
        .map_err(|e| {
            error!("Error checking permissions: {}", e);
            internal_error_response("Failed to check permissions")
        })?;
    if !perms.can_view {
        return Err(not_found_response("Resource"));
    }
    if !perm.granted_by(&perms) {
        return Err(HttpResponse::Forbidden().json(ErrorResponse {
            success: false,
            error: "Insufficient permissions".to_string(),
            message: Some("You don't have permission to perform this action".to_string()),
        }));
    }
    Ok(ctx)
}

/// IDs of every inventory the user can view (owned, shared, or via an All Access grant).
async fn accessible_inventory_ids(
    db: &DatabaseService,
    user_id: uuid::Uuid,
) -> std::result::Result<Vec<i32>, HttpResponse> {
    match db.get_accessible_inventories(user_id).await {
        Ok(inventories) => Ok(inventories.iter().filter_map(|i| i.id).collect()),
        Err(e) => {
            error!("Error listing accessible inventories: {}", e);
            Err(internal_error_response("Failed to check permissions"))
        },
    }
}

/// Authorize against the inventory resolved by `lookup`. Authentication happens first so
/// unauthenticated callers cannot probe which ids exist.
async fn authorize_resolved(
    pool: &Pool,
    req: &HttpRequest,
    lookup: std::result::Result<Option<i32>, Box<dyn std::error::Error>>,
    what: &str,
    perm: Perm,
) -> std::result::Result<crate::auth::AuthContext, HttpResponse> {
    let lookup = lookup.map_err(|e| {
        error!("Error resolving inventory for {}: {}", what, e);
        internal_error_response("Failed to check permissions")
    });
    match lookup {
        Ok(Some(inventory_id)) => authorize_inventory(pool, req, inventory_id, perm).await,
        Ok(None) => {
            auth::get_auth_context_from_request(req, pool).await?;
            Err(not_found_response("Resource"))
        },
        Err(resp) => {
            auth::get_auth_context_from_request(req, pool).await?;
            Err(resp)
        },
    }
}

// Inventories API endpoints
#[get("/inventories")]
pub async fn get_inventories(pool: web::Data<Pool>, req: HttpRequest) -> Result<impl Responder> {
    let auth = match auth::get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_accessible_inventories(auth.user_id).await {
        Ok(inventories) => {
            info!(
                "Successfully retrieved {} inventories for user {}",
                inventories.len(),
                auth.username
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(inventories.clone()),
                message: Some(format!(
                    "Retrieved {count} inventories",
                    count = inventories.len()
                )),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error retrieving inventories: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to retrieve inventories".to_string()),
            }))
        },
    }
}

#[post("/inventories")]
pub async fn create_inventory(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    req: web::Json<CreateInventoryRequest>,
) -> Result<impl Responder> {
    let auth = match auth::get_auth_context_from_request(&http_req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    // Validate input before processing
    if let Err(validation_errors) = req.validate() {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Validation failed".to_string(),
            message: Some(validation_errors.to_string()),
        }));
    }

    // Reject data URIs that are not images
    if let Err(msg) = validate_image_url(req.image_url.as_deref()) {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Validation failed".to_string(),
            message: Some(msg),
        }));
    }

    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service
        .create_inventory(req.into_inner(), auth.user_id)
        .await
    {
        Ok(inventory) => {
            info!("Successfully created inventory: {}", inventory.name);
            Ok(HttpResponse::Created().json(ApiResponse {
                success: true,
                data: Some(inventory),
                message: Some("Inventory created successfully".to_string()),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error creating inventory: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to create inventory".to_string()),
            }))
        },
    }
}

#[get("/inventories/{id}")]
pub async fn get_inventory(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
) -> Result<impl Responder> {
    let inventory_id = path.into_inner();
    if let Err(resp) =
        authorize_inventory(pool.get_ref(), &http_req, inventory_id, Perm::View).await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_inventory_by_id(inventory_id).await {
        Ok(Some(inventory)) => {
            info!("Successfully retrieved inventory with id: {}", inventory_id);
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(inventory),
                message: Some("Inventory retrieved successfully".to_string()),
                error: None,
            }))
        },
        Ok(None) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: format!("Inventory with id {inventory_id} not found"),
            message: Some("Inventory not found".to_string()),
        })),
        Err(e) => {
            error!("Error retrieving inventory: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to retrieve inventory".to_string()),
            }))
        },
    }
}

#[get("/inventories/{id}/items")]
pub async fn get_inventory_items(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
) -> Result<impl Responder> {
    let inventory_id = path.into_inner();
    if let Err(resp) =
        authorize_inventory(pool.get_ref(), &http_req, inventory_id, Perm::View).await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_items_by_inventory(inventory_id).await {
        Ok(items) => {
            info!(
                "Successfully retrieved {} items for inventory {}",
                items.len(),
                inventory_id
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(items.clone()),
                message: Some(format!("Retrieved {count} items", count = items.len())),
                error: None,
            }))
        },
        Err(e) => {
            error!(
                "Error retrieving items for inventory {}: {}",
                inventory_id, e
            );
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to retrieve inventory items".to_string()),
            }))
        },
    }
}

#[put("/inventories/{id}")]
pub async fn update_inventory(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
    req: web::Json<UpdateInventoryRequest>,
) -> Result<impl Responder> {
    // Validate input before processing
    if let Err(validation_errors) = req.validate() {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Validation failed".to_string(),
            message: Some(validation_errors.to_string()),
        }));
    }

    // Reject data URIs that are not images
    if let Err(msg) = validate_image_url(req.image_url.as_deref()) {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Validation failed".to_string(),
            message: Some(msg),
        }));
    }

    let inventory_id = path.into_inner();

    if let Err(resp) =
        authorize_inventory(pool.get_ref(), &http_req, inventory_id, Perm::EditInventory).await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service
        .update_inventory(inventory_id, req.into_inner())
        .await
    {
        Ok(Some(inventory)) => {
            info!("Successfully updated inventory with id: {}", inventory_id);
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(inventory),
                message: Some("Inventory updated successfully".to_string()),
                error: None,
            }))
        },
        Ok(None) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: format!("Inventory with id {inventory_id} not found"),
            message: Some("Inventory not found".to_string()),
        })),
        Err(e) => {
            error!("Error updating inventory: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to update inventory".to_string()),
            }))
        },
    }
}

#[delete("/inventories/{id}")]
pub async fn delete_inventory(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
) -> Result<impl Responder> {
    let inventory_id = path.into_inner();
    if let Err(resp) = authorize_inventory(
        pool.get_ref(),
        &http_req,
        inventory_id,
        Perm::DeleteInventory,
    )
    .await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.delete_inventory(inventory_id).await {
        Ok(true) => {
            info!("Successfully deleted inventory with id: {}", inventory_id);
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(()),
                message: Some("Inventory deleted successfully".to_string()),
                error: None,
            }))
        },
        Ok(false) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: format!("Inventory with id {inventory_id} not found"),
            message: Some("Inventory not found".to_string()),
        })),
        Err(e) => {
            error!("Error deleting inventory: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to delete inventory".to_string()),
            }))
        },
    }
}

// Items API endpoints
#[get("/items")]
pub async fn get_items(pool: web::Data<Pool>, http_req: HttpRequest) -> Result<impl Responder> {
    let auth = match auth::get_auth_context_from_request(&http_req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };
    let db_service = DatabaseService::new(pool.get_ref().clone());

    let inventory_ids = match accessible_inventory_ids(&db_service, auth.user_id).await {
        Ok(ids) => ids,
        Err(resp) => return Ok(resp),
    };

    match db_service.get_items_in_inventories(&inventory_ids).await {
        Ok(items) => {
            info!("Successfully retrieved {} items from database", items.len());
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(items.clone()),
                message: Some(format!("Retrieved {count} items", count = items.len())),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error retrieving items: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to retrieve items".to_string()),
            }))
        },
    }
}

#[get("/items/{id}")]
pub async fn get_item(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
) -> Result<impl Responder> {
    let item_id = path.into_inner();
    if let Err(resp) = authorize_resolved(
        pool.get_ref(),
        &http_req,
        DatabaseService::new(pool.get_ref().clone())
            .get_item_inventory_id(item_id)
            .await,
        "Item",
        Perm::View,
    )
    .await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_item_by_id(item_id).await {
        Ok(Some(item)) => {
            info!("Successfully retrieved item with id: {}", item_id);
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(item),
                message: Some("Item retrieved successfully".to_string()),
                error: None,
            }))
        },
        Ok(None) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: format!("Item with id {item_id} not found"),
            message: Some("Item not found".to_string()),
        })),
        Err(e) => {
            error!("Error retrieving item: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to retrieve item".to_string()),
            }))
        },
    }
}

#[post("/items")]
pub async fn create_item(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    req: web::Json<CreateItemRequest>,
) -> Result<impl Responder> {
    // Items default to inventory 1 when none is given (see H-10), so authorize against that
    let target_inventory = req.inventory_id.unwrap_or(1);
    if let Err(resp) =
        authorize_inventory(pool.get_ref(), &http_req, target_inventory, Perm::AddItems).await
    {
        return Ok(resp);
    }

    // Validate input before processing
    if let Err(validation_errors) = req.validate() {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Validation failed".to_string(),
            message: Some(validation_errors.to_string()),
        }));
    }

    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.create_item(req.into_inner()).await {
        Ok(item) => {
            info!("Successfully created item: {}", item.name);
            Ok(HttpResponse::Created().json(ApiResponse {
                success: true,
                data: Some(item),
                message: Some("Item created successfully".to_string()),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error creating item: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to create item".to_string()),
            }))
        },
    }
}

#[put("/items/{id}")]
pub async fn update_item(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
    req: web::Json<UpdateItemRequest>,
) -> Result<impl Responder> {
    // Validate input before processing
    if let Err(validation_errors) = req.validate() {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Validation failed".to_string(),
            message: Some(validation_errors.to_string()),
        }));
    }

    let item_id = path.into_inner();

    if let Err(resp) = authorize_resolved(
        pool.get_ref(),
        &http_req,
        DatabaseService::new(pool.get_ref().clone())
            .get_item_inventory_id(item_id)
            .await,
        "Item",
        Perm::EditItems,
    )
    .await
    {
        return Ok(resp);
    }
    if let Some(destination) = req.inventory_id {
        if let Err(resp) =
            authorize_inventory(pool.get_ref(), &http_req, destination, Perm::AddItems).await
        {
            return Ok(resp);
        }
    }

    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.update_item(item_id, req.into_inner()).await {
        Ok(Some(item)) => {
            info!("Successfully updated item with id: {}", item_id);
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(item),
                message: Some("Item updated successfully".to_string()),
                error: None,
            }))
        },
        Ok(None) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: format!("Item with id {item_id} not found"),
            message: Some("Item not found".to_string()),
        })),
        Err(e) => {
            error!("Error updating item: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to update item".to_string()),
            }))
        },
    }
}

#[delete("/items/{id}")]
pub async fn delete_item(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
) -> Result<impl Responder> {
    let item_id = path.into_inner();
    if let Err(resp) = authorize_resolved(
        pool.get_ref(),
        &http_req,
        DatabaseService::new(pool.get_ref().clone())
            .get_item_inventory_id(item_id)
            .await,
        "Item",
        Perm::RemoveItems,
    )
    .await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.delete_item(item_id).await {
        Ok(true) => {
            info!("Successfully deleted item with id: {}", item_id);
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(()),
                message: Some("Item deleted successfully".to_string()),
                error: None,
            }))
        },
        Ok(false) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: format!("Item with id {item_id} not found"),
            message: Some("Item not found".to_string()),
        })),
        Err(e) => {
            error!("Error deleting item: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to delete item".to_string()),
            }))
        },
    }
}

#[get("/items/search/{query}")]
pub async fn search_items(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<String>,
) -> Result<impl Responder> {
    let auth = match auth::get_auth_context_from_request(&http_req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };
    let query = path.into_inner();
    let db_service = DatabaseService::new(pool.get_ref().clone());

    let inventory_ids = match accessible_inventory_ids(&db_service, auth.user_id).await {
        Ok(ids) => ids,
        Err(resp) => return Ok(resp),
    };

    match db_service
        .search_items_in_inventories(&inventory_ids, &query)
        .await
    {
        Ok(items) => {
            info!(
                "Successfully searched items with query '{}', found {} results",
                query,
                items.len()
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(items.clone()),
                message: Some(format!(
                    "Found {count} items matching '{query}'",
                    count = items.len()
                )),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error searching items: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to search items".to_string()),
            }))
        },
    }
}

// ==================== Organizer Type Endpoints ====================

#[get("/inventories/{id}/organizers")]
pub async fn get_inventory_organizers(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
) -> Result<impl Responder> {
    let inventory_id = path.into_inner();
    if let Err(resp) =
        authorize_inventory(pool.get_ref(), &http_req, inventory_id, Perm::View).await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service
        .get_organizer_types_with_options_by_inventory(inventory_id)
        .await
    {
        Ok(organizers) => {
            info!(
                "Successfully retrieved {} organizers for inventory {}",
                organizers.len(),
                inventory_id
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(organizers.clone()),
                message: Some(format!(
                    "Retrieved {count} organizers",
                    count = organizers.len()
                )),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error retrieving organizers: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to retrieve organizers".to_string()),
            }))
        },
    }
}

#[post("/inventories/{id}/organizers")]
pub async fn create_organizer_type(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
    req: web::Json<CreateOrganizerTypeRequest>,
) -> Result<impl Responder> {
    let inventory_id = path.into_inner();
    if let Err(resp) = authorize_inventory(
        pool.get_ref(),
        &http_req,
        inventory_id,
        Perm::ManageOrganizers,
    )
    .await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service
        .create_organizer_type(inventory_id, req.into_inner())
        .await
    {
        Ok(organizer) => {
            info!("Successfully created organizer type: {}", organizer.name);
            Ok(HttpResponse::Created().json(ApiResponse {
                success: true,
                data: Some(organizer),
                message: Some("Organizer type created successfully".to_string()),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error creating organizer type: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to create organizer type".to_string()),
            }))
        },
    }
}

#[get("/organizers/{id}")]
pub async fn get_organizer_type(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
) -> Result<impl Responder> {
    let organizer_id = path.into_inner();
    if let Err(resp) = authorize_resolved(
        pool.get_ref(),
        &http_req,
        DatabaseService::new(pool.get_ref().clone())
            .get_organizer_type_inventory_id(organizer_id)
            .await,
        "Organizer type",
        Perm::View,
    )
    .await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_organizer_type_by_id(organizer_id).await {
        Ok(Some(organizer)) => {
            info!(
                "Successfully retrieved organizer type with id: {}",
                organizer_id
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(organizer),
                message: Some("Organizer type retrieved successfully".to_string()),
                error: None,
            }))
        },
        Ok(None) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: format!("Organizer type with id {organizer_id} not found"),
            message: Some("Organizer type not found".to_string()),
        })),
        Err(e) => {
            error!("Error retrieving organizer type: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to retrieve organizer type".to_string()),
            }))
        },
    }
}

#[put("/organizers/{id}")]
pub async fn update_organizer_type(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
    req: web::Json<UpdateOrganizerTypeRequest>,
) -> Result<impl Responder> {
    let organizer_id = path.into_inner();
    if let Err(resp) = authorize_resolved(
        pool.get_ref(),
        &http_req,
        DatabaseService::new(pool.get_ref().clone())
            .get_organizer_type_inventory_id(organizer_id)
            .await,
        "Organizer type",
        Perm::ManageOrganizers,
    )
    .await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service
        .update_organizer_type(organizer_id, req.into_inner())
        .await
    {
        Ok(Some(organizer)) => {
            info!(
                "Successfully updated organizer type with id: {}",
                organizer_id
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(organizer),
                message: Some("Organizer type updated successfully".to_string()),
                error: None,
            }))
        },
        Ok(None) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: format!("Organizer type with id {organizer_id} not found"),
            message: Some("Organizer type not found".to_string()),
        })),
        Err(e) => {
            error!("Error updating organizer type: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to update organizer type".to_string()),
            }))
        },
    }
}

#[delete("/organizers/{id}")]
pub async fn delete_organizer_type(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
) -> Result<impl Responder> {
    let organizer_id = path.into_inner();
    if let Err(resp) = authorize_resolved(
        pool.get_ref(),
        &http_req,
        DatabaseService::new(pool.get_ref().clone())
            .get_organizer_type_inventory_id(organizer_id)
            .await,
        "Organizer type",
        Perm::ManageOrganizers,
    )
    .await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.delete_organizer_type(organizer_id).await {
        Ok(true) => {
            info!(
                "Successfully deleted organizer type with id: {}",
                organizer_id
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(()),
                message: Some("Organizer type deleted successfully".to_string()),
                error: None,
            }))
        },
        Ok(false) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: format!("Organizer type with id {organizer_id} not found"),
            message: Some("Organizer type not found".to_string()),
        })),
        Err(e) => {
            error!("Error deleting organizer type: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to delete organizer type".to_string()),
            }))
        },
    }
}

// ==================== Organizer Option Endpoints ====================

#[get("/organizers/{id}/options")]
pub async fn get_organizer_options(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
) -> Result<impl Responder> {
    let organizer_id = path.into_inner();
    if let Err(resp) = authorize_resolved(
        pool.get_ref(),
        &http_req,
        DatabaseService::new(pool.get_ref().clone())
            .get_organizer_type_inventory_id(organizer_id)
            .await,
        "Organizer type",
        Perm::View,
    )
    .await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_organizer_options(organizer_id).await {
        Ok(options) => {
            info!(
                "Successfully retrieved {} options for organizer {}",
                options.len(),
                organizer_id
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(options.clone()),
                message: Some(format!("Retrieved {count} options", count = options.len())),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error retrieving organizer options: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to retrieve organizer options".to_string()),
            }))
        },
    }
}

#[post("/organizers/{id}/options")]
pub async fn create_organizer_option(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
    req: web::Json<CreateOrganizerOptionRequest>,
) -> Result<impl Responder> {
    let organizer_id = path.into_inner();
    if let Err(resp) = authorize_resolved(
        pool.get_ref(),
        &http_req,
        DatabaseService::new(pool.get_ref().clone())
            .get_organizer_type_inventory_id(organizer_id)
            .await,
        "Organizer type",
        Perm::ManageOrganizers,
    )
    .await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service
        .create_organizer_option(organizer_id, req.into_inner())
        .await
    {
        Ok(option) => {
            info!("Successfully created organizer option: {}", option.name);
            Ok(HttpResponse::Created().json(ApiResponse {
                success: true,
                data: Some(option),
                message: Some("Organizer option created successfully".to_string()),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error creating organizer option: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to create organizer option".to_string()),
            }))
        },
    }
}

#[put("/organizer-options/{id}")]
pub async fn update_organizer_option(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
    req: web::Json<UpdateOrganizerOptionRequest>,
) -> Result<impl Responder> {
    let option_id = path.into_inner();
    if let Err(resp) = authorize_resolved(
        pool.get_ref(),
        &http_req,
        DatabaseService::new(pool.get_ref().clone())
            .get_organizer_option_inventory_id(option_id)
            .await,
        "Organizer option",
        Perm::ManageOrganizers,
    )
    .await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service
        .update_organizer_option(option_id, req.into_inner())
        .await
    {
        Ok(Some(option)) => {
            info!(
                "Successfully updated organizer option with id: {}",
                option_id
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(option),
                message: Some("Organizer option updated successfully".to_string()),
                error: None,
            }))
        },
        Ok(None) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: format!("Organizer option with id {option_id} not found"),
            message: Some("Organizer option not found".to_string()),
        })),
        Err(e) => {
            error!("Error updating organizer option: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to update organizer option".to_string()),
            }))
        },
    }
}

#[delete("/organizer-options/{id}")]
pub async fn delete_organizer_option(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
) -> Result<impl Responder> {
    let option_id = path.into_inner();
    if let Err(resp) = authorize_resolved(
        pool.get_ref(),
        &http_req,
        DatabaseService::new(pool.get_ref().clone())
            .get_organizer_option_inventory_id(option_id)
            .await,
        "Organizer option",
        Perm::ManageOrganizers,
    )
    .await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.delete_organizer_option(option_id).await {
        Ok(true) => {
            info!(
                "Successfully deleted organizer option with id: {}",
                option_id
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(()),
                message: Some("Organizer option deleted successfully".to_string()),
                error: None,
            }))
        },
        Ok(false) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: format!("Organizer option with id {option_id} not found"),
            message: Some("Organizer option not found".to_string()),
        })),
        Err(e) => {
            error!("Error deleting organizer option: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to delete organizer option".to_string()),
            }))
        },
    }
}

// ==================== Item Organizer Value Endpoints ====================

#[get("/items/{id}/organizer-values")]
pub async fn get_item_organizer_values(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
) -> Result<impl Responder> {
    let item_id = path.into_inner();
    if let Err(resp) = authorize_resolved(
        pool.get_ref(),
        &http_req,
        DatabaseService::new(pool.get_ref().clone())
            .get_item_inventory_id(item_id)
            .await,
        "Item",
        Perm::View,
    )
    .await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_item_organizer_values(item_id).await {
        Ok(values) => {
            info!(
                "Successfully retrieved {} organizer values for item {}",
                values.len(),
                item_id
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(values.clone()),
                message: Some(format!(
                    "Retrieved {count} organizer values",
                    count = values.len()
                )),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error retrieving item organizer values: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to retrieve item organizer values".to_string()),
            }))
        },
    }
}

#[put("/items/{id}/organizer-values")]
pub async fn set_item_organizer_values(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<i32>,
    req: web::Json<SetItemOrganizerValuesRequest>,
) -> Result<impl Responder> {
    let item_id = path.into_inner();
    if let Err(resp) = authorize_resolved(
        pool.get_ref(),
        &http_req,
        DatabaseService::new(pool.get_ref().clone())
            .get_item_inventory_id(item_id)
            .await,
        "Item",
        Perm::EditItems,
    )
    .await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Every organizer type/option must belong to the item's own inventory
    let item_inventory = match db_service.get_item_inventory_id(item_id).await {
        Ok(Some(id)) => id,
        Ok(None) => return Ok(not_found_response("Item")),
        Err(e) => {
            error!("Error resolving item inventory: {}", e);
            return Ok(internal_error_response("Failed to check permissions"));
        },
    };
    for value in &req.values {
        let type_inventory = db_service
            .get_organizer_type_inventory_id(value.organizer_type_id)
            .await;
        let option_inventory = match value.organizer_option_id {
            Some(option_id) => Some(
                db_service
                    .get_organizer_option_inventory_id(option_id)
                    .await,
            ),
            None => None,
        };
        let lookups_ok =
            type_inventory.is_ok() && option_inventory.as_ref().is_none_or(Result::is_ok);
        if !lookups_ok {
            error!("Error validating organizer values for item {}", item_id);
            return Ok(internal_error_response(
                "Failed to validate organizer values",
            ));
        }
        let type_matches = matches!(type_inventory, Ok(Some(id)) if id == item_inventory);
        let option_matches = match option_inventory {
            Some(Ok(found)) => found == Some(item_inventory),
            _ => true,
        };
        if !type_matches || !option_matches {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: "Validation failed".to_string(),
                message: Some(
                    "Organizer type or option does not belong to the item's inventory".to_string(),
                ),
            }));
        }
    }

    match db_service
        .set_item_organizer_values(item_id, req.into_inner().values)
        .await
    {
        Ok(values) => {
            info!(
                "Successfully set {} organizer values for item {}",
                values.len(),
                item_id
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(values),
                message: Some("Item organizer values updated successfully".to_string()),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error setting item organizer values: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to set item organizer values".to_string()),
            }))
        },
    }
}

#[delete("/items/{item_id}/organizer-values/{organizer_type_id}")]
pub async fn delete_item_organizer_value(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    path: web::Path<(i32, i32)>,
) -> Result<impl Responder> {
    let (item_id, organizer_type_id) = path.into_inner();
    if let Err(resp) = authorize_resolved(
        pool.get_ref(),
        &http_req,
        DatabaseService::new(pool.get_ref().clone())
            .get_item_inventory_id(item_id)
            .await,
        "Item",
        Perm::EditItems,
    )
    .await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service
        .delete_item_organizer_value(item_id, organizer_type_id)
        .await
    {
        Ok(true) => {
            info!(
                "Successfully deleted organizer value for item {} type {}",
                item_id, organizer_type_id
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(()),
                message: Some("Item organizer value deleted successfully".to_string()),
                error: None,
            }))
        },
        Ok(false) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: format!("Organizer value not found for item {item_id} type {organizer_type_id}"),
            message: Some("Item organizer value not found".to_string()),
        })),
        Err(e) => {
            error!("Error deleting item organizer value: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to delete item organizer value".to_string()),
            }))
        },
    }
}

// ==================== Image Upload/Delete Endpoints ====================

/// Validate file magic bytes to determine actual image type.
/// Returns the file extension if valid, or None if not a recognized image.
fn detect_image_type(data: &[u8]) -> Option<&'static str> {
    if data.len() < 4 {
        return None;
    }
    // JPEG: starts with FF D8 FF
    if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        return Some("jpg");
    }
    // PNG: starts with 89 50 4E 47 0D 0A 1A 0A
    if data.len() >= 8 && data[0] == 0x89 && data[1] == 0x50 && data[2] == 0x4E && data[3] == 0x47 {
        return Some("png");
    }
    // GIF: starts with GIF87a or GIF89a
    if data.len() >= 6 && data[0] == 0x47 && data[1] == 0x49 && data[2] == 0x46 {
        return Some("gif");
    }
    // WebP: starts with RIFF....WEBP
    if data.len() >= 12
        && data[0] == 0x52
        && data[1] == 0x49
        && data[2] == 0x46
        && data[3] == 0x46
        && data[8] == 0x57
        && data[9] == 0x45
        && data[10] == 0x42
        && data[11] == 0x50
    {
        return Some("webp");
    }
    None
}

/// Validate that a filename is safe (no path traversal).
/// Only allows alphanumeric chars, underscores, hyphens, a single dot, and an extension.
fn is_safe_filename(filename: &str) -> bool {
    if filename.contains("..")
        || filename.contains('/')
        || filename.contains('\\')
        || filename.is_empty()
    {
        return false;
    }
    // Must be: one or more [a-zA-Z0-9_-], then a dot, then one or more [a-zA-Z0-9]
    filename
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
        && filename.chars().filter(|c| *c == '.').count() == 1
        && !filename.starts_with('.')
        && !filename.ends_with('.')
}

const MAX_IMAGE_SIZE: usize = 5 * 1024 * 1024; // 5 MB

/// Multipart form for image upload
#[derive(MultipartForm)]
struct ImageUploadForm {
    #[multipart(limit = "5MB")]
    image: TempFile,
}

#[post("/images/upload")]
pub async fn upload_image(
    req: HttpRequest,
    pool: web::Data<Pool>,
    MultipartForm(form): MultipartForm<ImageUploadForm>,
) -> Result<impl Responder> {
    // Auth check
    let _auth = match auth::get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    // Read the temp file contents
    let file_data = match tokio::fs::read(form.image.file.path()).await {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to read uploaded temp file: {}", e);
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: "Upload read error".to_string(),
                message: Some(format!("Failed to read upload data: {e}")),
            }));
        },
    };

    // Check size limit
    if file_data.len() > MAX_IMAGE_SIZE {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "File too large".to_string(),
            message: Some(format!(
                "Image must be under {} MB",
                MAX_IMAGE_SIZE / 1024 / 1024
            )),
        }));
    }

    if file_data.is_empty() {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "No image provided".to_string(),
            message: Some("Please include an 'image' field with file data".to_string()),
        }));
    }

    // Validate magic bytes
    let Some(ext) = detect_image_type(&file_data) else {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Invalid image type".to_string(),
            message: Some("Only JPEG, PNG, GIF, and WebP images are allowed".to_string()),
        }));
    };

    // Generate unique filename: {uuid}_{timestamp}.{ext}
    let timestamp = chrono::Utc::now().timestamp();
    let unique_id = uuid::Uuid::new_v4().to_string().replace('-', "");
    let filename = format!("{unique_id}_{timestamp}.{ext}");

    // Ensure uploads/img directory exists
    let upload_dir = std::path::Path::new("uploads/img");
    if let Err(e) = tokio::fs::create_dir_all(upload_dir).await {
        error!("Failed to create upload directory: {}", e);
        return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
            success: false,
            error: "Upload directory error".to_string(),
            message: Some("Failed to prepare upload directory".to_string()),
        }));
    }

    // Write file
    let file_path = upload_dir.join(&filename);
    if let Err(e) = tokio::fs::write(&file_path, &file_data).await {
        error!("Failed to write uploaded image: {}", e);
        return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
            success: false,
            error: "File write error".to_string(),
            message: Some("Failed to save uploaded image".to_string()),
        }));
    }

    let url = format!("/uploads/img/{filename}");
    info!(
        "Image uploaded successfully: {} ({} bytes)",
        filename,
        file_data.len()
    );

    Ok(HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(ImageUploadResponse {
            url: url.clone(),
            filename,
        }),
        message: Some("Image uploaded successfully".to_string()),
        error: None,
    }))
}

#[delete("/images/{filename}")]
pub async fn delete_image(
    req: HttpRequest,
    pool: web::Data<Pool>,
    path: web::Path<String>,
) -> Result<impl Responder> {
    // Auth check
    if let Err(e) = auth::get_auth_context_from_request(&req, pool.get_ref()).await {
        return Ok(e);
    }

    let filename = path.into_inner();

    // Validate filename to prevent path traversal
    if !is_safe_filename(&filename) {
        warn!(
            "Path traversal attempt detected in image delete: {}",
            filename
        );
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Invalid filename".to_string(),
            message: Some("Filename contains invalid characters".to_string()),
        }));
    }

    // A referenced image may only be deleted by someone who can edit what references it.
    // Unreferenced files (e.g. just uploaded, not yet saved) have no recorded owner and stay
    // deletable; see M-19.
    let db_service = DatabaseService::new(pool.get_ref().clone());
    let references = match db_service.get_image_references(&filename).await {
        Ok(refs) => refs,
        Err(e) => {
            error!("Error looking up image references for {}: {}", filename, e);
            return Ok(internal_error_response("Failed to check permissions"));
        },
    };
    for (inventory_id, is_cover) in references {
        let perm = if is_cover {
            Perm::EditInventory
        } else {
            Perm::EditItems
        };
        if let Err(resp) = authorize_inventory(pool.get_ref(), &req, inventory_id, perm).await {
            return Ok(resp);
        }
    }

    let file_path = std::path::Path::new("uploads/img").join(&filename);

    if !file_path.exists() {
        return Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: format!("Image not found: {filename}"),
            message: Some("The specified image does not exist".to_string()),
        }));
    }

    if let Err(e) = tokio::fs::remove_file(&file_path).await {
        error!("Failed to delete image {}: {}", filename, e);
        return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
            success: false,
            error: "File delete error".to_string(),
            message: Some("Failed to delete image file".to_string()),
        }));
    }

    info!("Image deleted successfully: {}", filename);

    Ok(HttpResponse::Ok().json(ApiResponse::<()> {
        success: true,
        data: None,
        message: Some("Image deleted successfully".to_string()),
        error: None,
    }))
}

#[get("/inventories/{id}/item-images")]
pub async fn get_inventory_item_images(
    req: HttpRequest,
    pool: web::Data<Pool>,
    path: web::Path<i32>,
) -> Result<impl Responder> {
    let inventory_id = path.into_inner();
    if let Err(resp) = authorize_inventory(pool.get_ref(), &req, inventory_id, Perm::View).await {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service
        .get_item_image_urls_by_inventory(inventory_id)
        .await
    {
        Ok(image_map) => {
            let count = image_map.len();
            info!(
                "Retrieved {} item images for inventory {}",
                count, inventory_id
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(image_map),
                message: Some(format!("Retrieved {count} item images")),
                error: None,
            }))
        },
        Err(e) => {
            error!(
                "Error retrieving item images for inventory {}: {}",
                inventory_id, e
            );
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to retrieve item images".to_string()),
            }))
        },
    }
}

// ==================== Inventory Reporting Endpoints ====================

/// Formats a collection of items as CSV data.
///
/// Generates a CSV file with columns for all relevant item fields including
/// inventory name, purchase information, and calculated total values.
///
/// # Arguments
/// * `items` - Vector of items to export
/// * `inventories` - Map of inventory IDs to names for lookup
///
/// # Returns
/// * `Ok(Vec<u8>)` - UTF-8 encoded CSV data ready for HTTP response
/// * `Err` - CSV serialization or I/O errors
fn format_items_as_csv(
    items: Vec<Item>,
    inventories: &std::collections::HashMap<i32, String>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut writer = csv::Writer::from_writer(vec![]);

    // Write header
    writer.write_record([
        "ID",
        "Inventory",
        "Name",
        "Description",
        "Category",
        "Location",
        "Quantity",
        "Purchase Price",
        "Total Value",
        "Purchase Date",
        "Warranty Expiry",
        "Created At",
    ])?;

    // Write data rows
    for item in items {
        let inventory_name = inventories
            .get(&item.inventory_id)
            .map_or("Unknown", std::string::String::as_str);

        let total_value = item
            .purchase_price
            .and_then(|price| item.quantity.map(|qty| price * f64::from(qty)))
            .map(|v| format!("{v:.2}"))
            .unwrap_or_default();

        writer.serialize(ItemExportRow {
            id: item.id.unwrap_or(0),
            inventory_name: inventory_name.to_string(),
            item_name: item.name,
            description: item.description.unwrap_or_default(),
            category: item.category.unwrap_or_default(),
            location: item.location.unwrap_or_default(),
            quantity: item.quantity.unwrap_or(0),
            purchase_price: item
                .purchase_price
                .map(|p| format!("{p:.2}"))
                .unwrap_or_default(),
            total_value,
            purchase_date: item.purchase_date.unwrap_or_default(),
            warranty_expiry: item.warranty_expiry.unwrap_or_default(),
            created_at: item
                .created_at
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default(),
        })?;
    }

    writer.flush()?;
    Ok(writer.into_inner()?)
}

#[get("/reports/inventory")]
pub async fn get_inventory_report(
    pool: web::Data<Pool>,
    req: HttpRequest,
    query: web::Query<InventoryReportRequest>,
) -> Result<impl Responder> {
    // Get authenticated user context
    let auth = match auth::get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let request = query.into_inner();

    // Validate input
    if let Err(validation_errors) = request.validate() {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Validation failed".to_string(),
            message: Some(validation_errors.to_string()),
        }));
    }

    let db_service = DatabaseService::new(pool.get_ref().clone());
    let format = request.format.as_deref().unwrap_or("json");

    // Validate date formats if provided
    if let Some(ref date_str) = request.from_date {
        if chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").is_err() {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: "Invalid from_date format".to_string(),
                message: Some("Date must be in ISO 8601 format (YYYY-MM-DD)".to_string()),
            }));
        }
    }

    if let Some(ref date_str) = request.to_date {
        if chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").is_err() {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: "Invalid to_date format".to_string(),
                message: Some("Date must be in ISO 8601 format (YYYY-MM-DD)".to_string()),
            }));
        }
    }

    // Validate date range (from_date must not be after to_date)
    if let (Some(ref from), Some(ref to)) = (&request.from_date, &request.to_date) {
        if let (Ok(from_parsed), Ok(to_parsed)) = (
            chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d"),
            chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d"),
        ) {
            if to_parsed < from_parsed {
                return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                    success: false,
                    error: "Invalid date range".to_string(),
                    message: Some("to_date cannot be before from_date".to_string()),
                }));
            }
        }
    }

    // Validate price range
    if let (Some(min), Some(max)) = (request.min_price, request.max_price) {
        if min > max {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: "Invalid price range".to_string(),
                message: Some("min_price cannot exceed max_price".to_string()),
            }));
        }
    }

    // Check inventory access if specific inventory requested
    if let Some(inv_id) = request.inventory_id {
        match db_service
            .check_inventory_access(auth.user_id, inv_id)
            .await
        {
            Ok(false) => {
                return Ok(HttpResponse::Forbidden().json(ErrorResponse {
                    success: false,
                    error: "Access denied to this inventory".to_string(),
                    message: None,
                }))
            },
            Err(e) => {
                error!("Error checking inventory access: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: "An internal error occurred".to_string(),
                    message: None,
                }));
            },
            _ => {},
        }
    }

    // Fetch report data
    let items = match db_service
        .get_inventory_report_data(request.clone(), auth.user_id)
        .await
    {
        Ok(items) => items,
        Err(e) => {
            error!("Error generating report: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to generate report".to_string(),
                message: Some(e.to_string()),
            }));
        },
    };

    // Handle format selection - CSV vs JSON export
    if format == "csv" {
        // Fetch inventory names for CSV export
        let inventory_names = match db_service.get_accessible_inventories(auth.user_id).await {
            Ok(inventories) => inventories
                .into_iter()
                .filter_map(|inv| inv.id.map(|id| (id, inv.name)))
                .collect(),
            Err(e) => {
                error!("Error fetching inventories for CSV: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: "Failed to fetch inventory names".to_string(),
                    message: Some(e.to_string()),
                }));
            },
        };

        match format_items_as_csv(items, &inventory_names) {
            Ok(csv_data) => {
                let filename = format!(
                    "inventory-report-{}.csv",
                    chrono::Utc::now().format("%Y%m%d-%H%M%S")
                );

                info!(
                    "Generated CSV report for user {}: {} bytes",
                    auth.username,
                    csv_data.len()
                );

                Ok(HttpResponse::Ok()
                    .content_type("text/csv; charset=utf-8")
                    .insert_header((
                        "Content-Disposition",
                        format!("attachment; filename=\"{filename}\""),
                    ))
                    .body(csv_data))
            },
            Err(e) => {
                error!("Error formatting CSV for user {}: {}", auth.username, e);
                let error_msg = if e.to_string().contains("CSV") {
                    "CSV serialization error"
                } else {
                    "Failed to format CSV"
                };
                Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: error_msg.to_string(),
                    message: Some(format!("Could not generate CSV export: {e}")),
                }))
            },
        }
    } else {
        // Fetch additional data for complete report
        let statistics = match db_service
            .get_inventory_statistics(request.inventory_id, auth.user_id)
            .await
        {
            Ok(stats) => stats,
            Err(e) => {
                error!("Error fetching statistics: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: "Failed to fetch statistics".to_string(),
                    message: Some(e.to_string()),
                }));
            },
        };

        let category_breakdown = match db_service
            .get_category_breakdown(request.inventory_id, auth.user_id)
            .await
        {
            Ok(breakdown) => breakdown,
            Err(e) => {
                error!("Error fetching category breakdown: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: "Failed to fetch category breakdown".to_string(),
                    message: Some(e.to_string()),
                }));
            },
        };

        let report_data = InventoryReportData {
            statistics,
            category_breakdown,
            items,
            generated_at: chrono::Utc::now(),
            filters_applied: request,
        };

        info!("Generated JSON report for user {}", auth.username);

        Ok(HttpResponse::Ok().json(ApiResponse {
            success: true,
            data: Some(report_data),
            message: Some("Report generated successfully".to_string()),
            error: None,
        }))
    }
}

#[get("/reports/inventory/statistics")]
pub async fn get_inventory_statistics_endpoint(
    pool: web::Data<Pool>,
    req: HttpRequest,
    query: web::Query<InventoryReportRequest>,
) -> Result<impl Responder> {
    let auth = match auth::get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());
    let request = query.into_inner();

    // Check inventory access if specific inventory requested
    if let Some(inv_id) = request.inventory_id {
        match db_service
            .check_inventory_access(auth.user_id, inv_id)
            .await
        {
            Ok(false) => {
                return Ok(HttpResponse::Forbidden().json(ErrorResponse {
                    success: false,
                    error: "Access denied to this inventory".to_string(),
                    message: None,
                }))
            },
            Err(e) => {
                error!("Error checking inventory access: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: "An internal error occurred".to_string(),
                    message: None,
                }));
            },
            _ => {},
        }
    }

    match db_service
        .get_inventory_statistics(request.inventory_id, auth.user_id)
        .await
    {
        Ok(stats) => {
            info!("Retrieved statistics for user {}", auth.username);
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(stats),
                message: Some("Statistics retrieved successfully".to_string()),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error retrieving statistics: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to retrieve statistics".to_string(),
                message: Some(e.to_string()),
            }))
        },
    }
}

#[get("/reports/inventory/categories")]
pub async fn get_category_breakdown_endpoint(
    pool: web::Data<Pool>,
    req: HttpRequest,
    query: web::Query<InventoryReportRequest>,
) -> Result<impl Responder> {
    let auth = match auth::get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());
    let request = query.into_inner();

    // Check inventory access if specific inventory requested
    if let Some(inv_id) = request.inventory_id {
        match db_service
            .check_inventory_access(auth.user_id, inv_id)
            .await
        {
            Ok(false) => {
                return Ok(HttpResponse::Forbidden().json(ErrorResponse {
                    success: false,
                    error: "Access denied to this inventory".to_string(),
                    message: None,
                }))
            },
            Err(e) => {
                error!("Error checking inventory access: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: "An internal error occurred".to_string(),
                    message: None,
                }));
            },
            _ => {},
        }
    }

    match db_service
        .get_category_breakdown(request.inventory_id, auth.user_id)
        .await
    {
        Ok(breakdown) => {
            info!("Retrieved category breakdown for user {}", auth.username);
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(breakdown),
                message: Some("Category breakdown retrieved successfully".to_string()),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error retrieving category breakdown: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to retrieve category breakdown".to_string(),
                message: Some(e.to_string()),
            }))
        },
    }
}

// API 404 handler - returns JSON instead of HTML
async fn api_not_found(req: HttpRequest) -> impl Responder {
    log::warn!("API 404: {}", req.uri());
    HttpResponse::NotFound().json(ErrorResponse {
        success: false,
        error: "Endpoint not found".to_string(),
        message: Some(format!(
            "The API endpoint {uri} does not exist",
            uri = req.uri()
        )),
    })
}

// Create scope with all API routes
pub fn api_scope() -> Scope {
    web::scope("/api")
        .app_data(web::JsonConfig::default().limit(15_728_640))
        .app_data(web::PayloadConfig::new(20 * 1024 * 1024))
        .service(api_health)
        // Auth routes (setup, login, profile, admin user management) - imported directly to avoid nested scope
        .service(auth::get_setup_status)
        .service(auth::initial_setup)
        .service(auth::login)
        .service(auth::register)
        .service(auth::get_current_user)
        .service(auth::update_current_user)
        .service(auth::change_password)
        // Recovery codes endpoints
        .service(auth::generate_recovery_codes)
        .service(auth::get_recovery_codes_status)
        .service(auth::confirm_recovery_codes)
        .service(auth::use_recovery_code)
        .service(auth::get_user_settings)
        .service(auth::update_user_settings)
        .service(auth::get_my_inventories)
        .service(auth::get_inventory_shares)
        .service(auth::create_inventory_share)
        .service(auth::update_inventory_share)
        .service(auth::delete_inventory_share)
        .service(auth::transfer_inventory_ownership)
        .service(auth::get_inventory_permissions)
        .service(auth::get_my_access_grants)
        .service(auth::get_received_access_grants)
        .service(auth::create_access_grant)
        .service(auth::delete_access_grant)
        .service(auth::admin_get_users)
        .service(auth::admin_get_user)
        .service(auth::admin_create_user)
        .service(auth::admin_update_user)
        .service(auth::admin_delete_user)
        // TOTP authenticator routes
        .service(totp::totp_setup)
        .service(totp::totp_verify_setup)
        .service(totp::totp_verify)
        .service(totp::totp_recover)
        .service(totp::totp_update_mode)
        .service(totp::totp_disable)
        .service(totp::totp_status)
        // Inventory routes
        .service(get_inventories)
        .service(create_inventory)
        .service(get_inventory)
        .service(get_inventory_items)
        .service(get_inventory_organizers)
        .service(create_organizer_type)
        .service(update_inventory)
        .service(delete_inventory)
        // Item routes
        .service(get_items)
        .service(get_item)
        .service(create_item)
        .service(update_item)
        .service(delete_item)
        .service(search_items)
        .service(get_item_organizer_values)
        .service(set_item_organizer_values)
        .service(delete_item_organizer_value)
        // Organizer routes
        .service(get_organizer_type)
        .service(update_organizer_type)
        .service(delete_organizer_type)
        .service(get_organizer_options)
        .service(create_organizer_option)
        .service(update_organizer_option)
        .service(delete_organizer_option)
        // Image upload/delete routes
        .service(upload_image)
        .service(delete_image)
        .service(get_inventory_item_images)
        // Inventory reporting routes
        .service(get_inventory_report)
        .service(get_inventory_statistics_endpoint)
        .service(get_category_breakdown_endpoint)
        // Backup & Restore routes
        .service(backup::create_backup)
        .service(backup::list_backups)
        .service(backup::download_backup)
        .service(backup::upload_backup)
        .service(backup::restore_backup)
        .service(backup::delete_backup)
        // Catch-all for non-existent API endpoints
        .default_service(web::to(api_not_found))
}

// Alias for backward compatibility
#[must_use]
pub fn init_routes() -> Scope {
    api_scope()
}
