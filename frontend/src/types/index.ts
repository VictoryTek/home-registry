// API Response types
export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  message?: string;
  error?: string;
}

export interface ErrorResponse {
  success: false;
  error: string;
  message?: string;
}

// Core domain types
export interface Inventory {
  id?: number;
  name: string;
  description?: string;
  location?: string;
  image_url?: string;
  user_id?: string;
  created_at?: string;
  updated_at?: string;
}

export interface Item {
  id?: number;
  inventory_id: number;
  name: string;
  description?: string;
  category?: string;
  location?: string;
  purchase_date?: string;
  purchase_price?: number;
  warranty_expiry?: string;
  notes?: string;
  quantity?: number;
  created_at?: string;
  updated_at?: string;
}

export interface Category {
  id?: number;
  name: string;
  description?: string;
  color?: string;
  icon?: string;
  customFields?: CustomField[];
  created_at?: string;
  updated_at?: string;
}

export interface Tag {
  id?: number;
  name: string;
  description?: string;
  color?: string;
  categoryId?: number;
  created_at?: string;
  updated_at?: string;
}

export interface CustomField {
  id: string;
  name: string;
  type: 'text' | 'number' | 'date' | 'textarea' | 'select' | 'checkbox';
  required?: boolean;
  options?: string[];
}

export interface CustomFieldValue {
  id?: number;
  item_id: number;
  custom_field_id: number;
  value?: string;
  created_at?: string;
  updated_at?: string;
}

// Request types
export interface CreateInventoryRequest {
  name: string;
  description?: string;
  location?: string;
  image_url?: string;
}

export interface UpdateInventoryRequest {
  name?: string;
  description?: string;
  location?: string;
  image_url?: string;
}

export interface CreateItemRequest {
  inventory_id?: number;
  name: string;
  description?: string;
  category?: string;
  location?: string;
  purchase_date?: string;
  purchase_price?: number;
  warranty_expiry?: string;
  notes?: string;
  quantity?: number;
}

export interface UpdateItemRequest {
  name?: string;
  description?: string;
  category?: string;
  location?: string;
  purchase_date?: string;
  purchase_price?: number;
  warranty_expiry?: string;
  notes?: string;
  quantity?: number;
  inventory_id?: number;
}

// UI state types
export type Theme = 'light' | 'dark';

export type Page =
  | 'inventories'
  | 'categories'
  | 'tags'
  | 'settings'
  | 'inventory-detail'
  | 'organizers';

export interface ToastMessage {
  id: string;
  message: string;
  type: 'success' | 'error' | 'warning' | 'info';
}

// Organizer types
export interface OrganizerType {
  id?: number;
  inventory_id: number;
  name: string;
  input_type: 'select' | 'text' | 'image';
  is_required: boolean;
  display_order: number;
  created_at?: string;
  updated_at?: string;
}

export interface OrganizerOption {
  id?: number;
  organizer_type_id: number;
  name: string;
  display_order: number;
  created_at?: string;
  updated_at?: string;
}

export interface OrganizerTypeWithOptions extends OrganizerType {
  options: OrganizerOption[];
}

export interface ItemOrganizerValue {
  id?: number;
  item_id: number;
  organizer_type_id: number;
  organizer_option_id?: number;
  text_value?: string;
  created_at?: string;
  updated_at?: string;
}

export interface ItemOrganizerValueWithDetails {
  organizer_type_id: number;
  organizer_type_name: string;
  input_type: 'select' | 'text' | 'image';
  is_required: boolean;
  value?: string;
  organizer_option_id?: number;
  text_value?: string;
}

// Organizer request types
export interface CreateOrganizerTypeRequest {
  name: string;
  input_type?: 'select' | 'text' | 'image';
  is_required?: boolean;
  display_order?: number;
}

export interface UpdateOrganizerTypeRequest {
  name?: string;
  input_type?: 'select' | 'text' | 'image';
  is_required?: boolean;
  display_order?: number;
}

export interface CreateOrganizerOptionRequest {
  name: string;
  display_order?: number;
}

export interface UpdateOrganizerOptionRequest {
  name?: string;
  display_order?: number;
}

export interface SetItemOrganizerValueRequest {
  organizer_type_id: number;
  organizer_option_id?: number;
  text_value?: string;
}

export interface SetItemOrganizerValuesRequest {
  values: SetItemOrganizerValueRequest[];
}

// ==================== Image Upload Types ====================

export interface ImageUploadResponse {
  url: string;
  filename: string;
}

// ==================== Authentication Types ====================

export interface User {
  id: string;
  username: string;
  full_name: string;
  is_admin: boolean;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

export interface UserSettings {
  id: string;
  user_id: string;
  theme: string;
  default_inventory_id?: number | null; // Allow null to represent "no default"
  items_per_page: number;
  date_format: string;
  currency: string;
  notifications_enabled: boolean;
  settings_json: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface SetupStatusResponse {
  needs_setup: boolean;
  user_count: number;
}

export interface LoginRequest {
  username: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  user: User;
}

export interface InitialSetupRequest {
  username: string;
  full_name: string;
  password: string;
}

export interface RegisterRequest {
  username: string;

  full_name: string;
  password: string;
}

export interface UpdateProfileRequest {
  full_name?: string;
}

export interface ChangePasswordRequest {
  current_password: string;
  new_password: string;
}

export interface UpdateUserSettingsRequest {
  theme?: string;
  default_inventory_id?: number | null; // Allow null to explicitly clear the setting
  items_per_page?: number;
  date_format?: string;
  currency?: string;
  notifications_enabled?: boolean;
  settings_json?: Record<string, unknown>;
}

// Admin user management types
export interface CreateUserRequest {
  username: string;

  full_name: string;
  password: string;
  is_admin?: boolean;
  is_active?: boolean;
}

export interface UpdateUserRequest {
  full_name?: string;
  password?: string;
  is_admin?: boolean;
  is_active?: boolean;
}

// Permission types - 4-tier system
// view: Can view inventory and items
// edit_items: Can view and edit item details (not add/remove)
// edit_inventory: Can view, edit items, add/remove items, edit inventory details
// all_access: User-to-user grant - full access to ALL grantor's inventories (via UserAccessGrant)
export type PermissionLevel = 'view' | 'edit_items' | 'edit_inventory';

// Permission source - where the user's access comes from
export type PermissionSource = 'owner' | 'all_access' | 'inventory_share' | 'none';

export interface InventoryShare {
  id: string;
  inventory_id: number;
  shared_with_user: User;
  shared_by_user: User;
  permission_level: PermissionLevel;
  created_at: string;
  updated_at: string;
}

export interface CreateInventoryShareRequest {
  shared_with_username: string;
  permission_level: PermissionLevel;
}

export interface UpdateInventoryShareRequest {
  permission_level: PermissionLevel;
}

// User Access Grant types (All Access tier)
export interface UserAccessGrant {
  id: string;
  grantor_user_id: string;
  grantee_user_id: string;
  created_at: string;
  updated_at: string;
}

export interface UserAccessGrantWithUsers {
  id: string;
  grantor: User;
  grantee: User;
  created_at: string;
  updated_at: string;
}

export interface CreateUserAccessGrantRequest {
  grantee_username: string;
}

// Ownership Transfer types
export interface TransferOwnershipRequest {
  new_owner_username: string;
}

export interface TransferOwnershipResponse {
  inventory_id: number;
  inventory_name: string;
  previous_owner: User;
  new_owner: User;
  items_transferred: number;
  shares_removed: number;
}

// Effective permissions for a user on an inventory
export interface EffectivePermissions {
  can_view: boolean;
  can_edit_items: boolean;
  can_add_items: boolean;
  can_remove_items: boolean;
  can_edit_inventory: boolean;
  can_delete_inventory: boolean;
  can_manage_sharing: boolean;
  can_manage_organizers: boolean;
  is_owner: boolean;
  has_all_access: boolean;
  permission_source: PermissionSource;
}

// Recovery Codes types
export interface RecoveryCodesResponse {
  codes: string[];
  generated_at: string;
  message: string;
}

export interface RecoveryCodesStatus {
  has_codes: boolean;
  codes_confirmed: boolean;
  unused_count: number;
  generated_at: string | null;
}

export interface ConfirmRecoveryCodesRequest {
  confirmed: boolean;
}

export interface UseRecoveryCodeRequest {
  username: string;
  recovery_code: string;
  new_password: string;
}

export interface RecoveryCodeUsedResponse {
  success: boolean;
  message: string;
  remaining_codes: number;
}

// Inventory Report types
export interface InventoryReportParams {
  inventory_id?: number;
  from_date?: string;
  to_date?: string;
  min_price?: number;
  max_price?: number;
  category?: string;
  format?: string;
}

export interface InventoryStatistics {
  total_items: number;
  total_value: number;
  category_count: number;
  average_price: number;
}

export interface CategorySummary {
  category: string;
  item_count: number;
  total_value: number;
}

export interface InventoryReportData {
  statistics: InventoryStatistics;
  category_breakdown: CategorySummary[];
  items: Item[];
  generated_at: string;
  filters_applied: InventoryReportParams;
}

// Dismissed warranty notifications (stored in UserSettings.settings_json)
export type DismissedWarranties = Record<
  string,
  {
    dismissedAt: string; // ISO timestamp
    warrantyExpiry: string; // Date at dismissal (to detect changes)
  }
>;

// ==================== TOTP Authenticator Types ====================

export interface TotpStatusResponse {
  is_enabled: boolean;
  mode?: TotpMode | null;
  last_used_at?: string | null;
  created_at?: string | null;
}

export type TotpMode = '2fa_only' | 'recovery_only' | 'both';

export interface TotpSetupResponse {
  secret: string;
  otpauth_uri: string;
  qr_code_data_uri: string;
  issuer: string;
  algorithm: string;
  digits: number;
  period: number;
}

export interface TotpVerifySetupRequest {
  code: string;
  mode: TotpMode;
}

export interface TotpVerifySetupResponse {
  enabled: boolean;
  mode: TotpMode;
}

export interface TotpVerifyRequest {
  code: string;
}

export interface TotpRecoveryRequest {
  username: string;
  totp_code: string;
  new_password: string;
}

export interface TotpModeRequest {
  mode: TotpMode;
}

export interface TotpDisableRequest {
  password: string;
}

export interface LoginTotpRequiredResponse {
  requires_totp: boolean;
  partial_token: string;
  user: User;
}

// Discriminated union for login API response (normal login vs TOTP-required)
export type LoginApiResponse = LoginResponse | LoginTotpRequiredResponse;

// ==================== Backup & Restore Types ====================

export interface BackupInfo {
  name: string;
  date: string;
  size: string;
}

export interface BackupMetadata {
  version: string;
  app_version: string;
  created_at: string;
  database_type: string;
  description?: string;
}
