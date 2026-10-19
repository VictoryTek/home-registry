import type {
  ApiResponse,
  Inventory,
  Item,
  CreateInventoryRequest,
  UpdateInventoryRequest,
  CreateItemRequest,
  UpdateItemRequest,
  OrganizerTypeWithOptions,
  OrganizerType,
  OrganizerOption,
  CreateOrganizerTypeRequest,
  UpdateOrganizerTypeRequest,
  CreateOrganizerOptionRequest,
  UpdateOrganizerOptionRequest,
  ItemOrganizerValueWithDetails,
  ItemOrganizerValue,
  SetItemOrganizerValuesRequest,
  InventoryOrganizerValue,
  ItemSearchResults,
  // Auth types
  User,
  UserSettings,
  SetupStatusResponse,
  RegistrationSettings,
  RegistrationMode,
  IssuedToken,
  InviteInfo,
  LoginRequest,
  LoginResponse,
  LoginApiResponse,
  InitialSetupRequest,
  UpdateProfileRequest,
  ChangePasswordRequest,
  UpdateUserSettingsRequest,
  CreateUserRequest,
  UpdateUserRequest,
  // Sharing types
  InventoryShare,
  CreateInventoryShareRequest,
  UpdateInventoryShareRequest,
  UserAccessGrant,
  UserAccessGrantWithUsers,
  CreateUserAccessGrantRequest,
  EffectivePermissions,
  // Transfer ownership types
  TransferOwnershipRequest,
  TransferOwnershipResponse,
  // Recovery codes types
  RecoveryCodesResponse,
  RecoveryCodesStatus,
  RecoveryCodeUsedResponse,
  // TOTP types
  TotpStatusResponse,
  TotpSetupResponse,
  TotpVerifySetupResponse,
  TotpMode,
  SessionTokenResponse,
  // Report types
  InventoryReportParams,
  InventoryReportData,
  InventoryStatistics,
  CategorySummary,
  // Backup types
  BackupInfo,
  RestoreReport,
  // Image types
  ImageUploadResponse,
} from '@/types';

const API_BASE = '/api';
const TOKEN_KEY = 'home_registry_token';
// Set (in sessionStorage) when the server reports a revoked session, so the login screen can say why
export const SESSION_NOTICE_KEY = 'home_registry_session_notice';
// Fired in this tab after a fresh token was stored, so AuthContext can pick it up
export const TOKEN_CHANGED_EVENT = 'home_registry_token_changed';

// Store a fresh token issued after this device's own session generation changed
// (password change, "sign out other devices", TOTP enable/disable)
function adoptSessionToken(token: string | undefined): void {
  if (!token) {
    return;
  }
  localStorage.setItem(TOKEN_KEY, token);
  window.dispatchEvent(new Event(TOKEN_CHANGED_EVENT));
}

// The bearer token a request was sent with, if any
function sentBearerToken(options: RequestInit): string | null {
  const headers = options.headers;
  if (headers && typeof headers === 'object' && !Array.isArray(headers)) {
    const auth = (headers as Record<string, string>).Authorization;
    if (typeof auth === 'string' && auth.startsWith('Bearer ')) {
      return auth.slice('Bearer '.length);
    }
  }
  return null;
}

// Get auth token from localStorage
function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

// Get headers with optional auth token
function getHeaders(includeAuth = true): Record<string, string> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  };

  if (includeAuth) {
    const token = getToken();
    if (token) {
      headers.Authorization = `Bearer ${token}`;
    }
  }

  return headers;
}

async function handleResponse<T>(response: Response): Promise<ApiResponse<T>> {
  // Handle 401 Unauthorized - redirect to login
  if (response.status === 401) {
    // Remember why when the server says the session was revoked, so the login page can explain
    try {
      const body = (await response.clone().json()) as { error?: string };
      if (body.error === 'session_revoked') {
        sessionStorage.setItem(SESSION_NOTICE_KEY, 'revoked');
      }
    } catch {
      // Not a JSON body; nothing to record
    }
    localStorage.removeItem(TOKEN_KEY);
    localStorage.removeItem('home_registry_user');
    // Only redirect if not already on login/setup page
    if (
      !window.location.pathname.includes('/login') &&
      !window.location.pathname.includes('/setup')
    ) {
      window.location.href = '/login';
    }
  }

  // Check if response is JSON
  const contentType = response.headers.get('content-type');
  if (!contentType?.includes('application/json')) {
    // Not JSON - probably an HTML error page
    const text = await response.text();
    console.error('Received non-JSON response:', text.substring(0, 200));
    return {
      success: false,
      error: `Server error (${response.status}): Expected JSON but received ${contentType ?? 'unknown content type'}`,
      data: undefined,
    };
  }

  try {
    const data = (await response.json()) as ApiResponse<T>;
    return data;
  } catch {
    console.error('Failed to parse JSON response');
    return {
      success: false,
      error: 'Invalid JSON response from server',
      data: undefined,
    };
  }
}

// RETRY LOGIC: Utility functions for exponential backoff with jitter
// Used to handle rate limiting (429) and transient network errors gracefully

/**
 * Sleep for the specified number of milliseconds
 */
async function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Calculate exponential backoff delay: 2^attempt * 1000ms
 * Max delay is capped at 32 seconds to avoid excessive waits
 */
function calculateBackoff(attempt: number): number {
  const baseDelay = 1000; // 1 second
  const maxDelay = 32000; // 32 seconds
  return Math.min(Math.pow(2, attempt) * baseDelay, maxDelay);
}

/**
 * Add random jitter to delay to prevent thundering herd
 * Jitter is ±25% of the delay (multiplies by 0.75 to 1.25)
 */
function addJitter(delay: number): number {
  return delay * (0.75 + Math.random() * 0.5);
}

/**
 * Parse Retry-After header from rate limit response
 * Returns delay in milliseconds, or null if header is missing/invalid
 */
function parseRetryAfter(response: Response): number | null {
  const retryAfter = response.headers.get('Retry-After');
  if (!retryAfter) {
    return null;
  }

  // Retry-After can be either:
  // 1. Number of seconds: "120"
  // 2. HTTP date: "Wed, 21 Oct 2015 07:28:00 GMT"
  const seconds = parseInt(retryAfter, 10);
  if (!isNaN(seconds)) {
    return seconds * 1000; // Convert to milliseconds
  }

  // Try parsing as HTTP date
  try {
    const date = new Date(retryAfter);
    const now = new Date();
    const diff = date.getTime() - now.getTime();
    return diff > 0 ? diff : null;
  } catch {
    return null;
  }
}

/**
 * Fetch with automatic retry logic for rate limiting and transient errors
 *
 * Implements:
 * - Exponential backoff (1s, 2s, 4s, 8s, 16s)
 * - Retry-After header parsing for rate limits
 * - Jitter to prevent thundering herd
 * - Max 5 retries
 *
 * @param url - URL to fetch
 * @param options - Fetch options (method, headers, body, etc.)
 * @param maxRetries - Maximum number of retries (default: 5)
 * @returns Response object
 */
async function fetchWithRetry(
  url: string,
  options: RequestInit,
  maxRetries = 5
): Promise<Response> {
  let lastError: Error | null = null;
  let init = options;
  let retriedWithNewerToken = false;

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      const response = await fetch(url, init);

      // Another tab of this browser may have replaced the token (password change, sign out of
      // other devices) after this request was built. Retry once with the newer token before
      // the caller treats the 401 as a signed-out session.
      if (response.status === 401 && !retriedWithNewerToken) {
        const sent = sentBearerToken(init);
        const current = getToken();
        if (sent && current && current !== sent) {
          retriedWithNewerToken = true;
          init = {
            ...init,
            headers: {
              ...(init.headers as Record<string, string>),
              Authorization: `Bearer ${current}`,
            },
          };
          continue;
        }
      }

      // Handle rate limiting (429 Too Many Requests)
      if (response.status === 429) {
        if (attempt === maxRetries) {
          // Last attempt - return the 429 response to be handled by caller
          console.error(`Rate limit exceeded after ${maxRetries} retries`);
          return response;
        }

        // Parse Retry-After header or use exponential backoff
        const retryAfterMs = parseRetryAfter(response);
        let waitMs: number;

        if (retryAfterMs !== null) {
          // Use server-provided retry delay
          waitMs = retryAfterMs;
          console.warn(`Rate limited. Retrying after ${waitMs}ms (from Retry-After header)`);
        } else {
          // Use exponential backoff with jitter
          waitMs = addJitter(calculateBackoff(attempt));
          console.warn(
            `Rate limited. Retrying after ${Math.round(waitMs)}ms (attempt ${attempt + 1}/${maxRetries + 1})`
          );
        }

        await sleep(waitMs);
        continue; // Retry
      }

      // Success or non-retriable error - return response
      return response;
    } catch (error) {
      // Network error or fetch failed
      lastError = error as Error;

      if (attempt === maxRetries) {
        // Last attempt - throw the error
        console.error(`Request failed after ${maxRetries} retries:`, lastError);
        throw lastError;
      }

      // Retry with exponential backoff
      const waitMs = addJitter(calculateBackoff(attempt));
      console.warn(
        `Network error. Retrying after ${Math.round(waitMs)}ms (attempt ${attempt + 1}/${maxRetries + 1}):`,
        error
      );
      await sleep(waitMs);
    }
  }

  // Should never reach here, but TypeScript requires it
  throw lastError ?? new Error('Request failed after retries');
}

// Inventory API
export const inventoryApi = {
  async getAll(): Promise<ApiResponse<Inventory[]>> {
    const response = await fetchWithRetry(`${API_BASE}/inventories`, {
      headers: getHeaders(),
    });
    return handleResponse<Inventory[]>(response);
  },

  async getById(id: number): Promise<ApiResponse<Inventory>> {
    const response = await fetchWithRetry(`${API_BASE}/inventories/${id}`, {
      headers: getHeaders(),
    });
    return handleResponse<Inventory>(response);
  },

  async create(data: CreateInventoryRequest): Promise<ApiResponse<Inventory>> {
    const response = await fetchWithRetry(`${API_BASE}/inventories`, {
      method: 'POST',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<Inventory>(response);
  },

  async update(id: number, data: UpdateInventoryRequest): Promise<ApiResponse<Inventory>> {
    const response = await fetchWithRetry(`${API_BASE}/inventories/${id}`, {
      method: 'PUT',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<Inventory>(response);
  },

  async getItems(inventoryId: number): Promise<ApiResponse<Item[]>> {
    const response = await fetchWithRetry(`${API_BASE}/inventories/${inventoryId}/items`, {
      headers: getHeaders(),
    });
    return handleResponse<Item[]>(response);
  },

  /** The select and text organizer values of every item in the inventory, in one call. */
  async getOrganizerValues(inventoryId: number): Promise<ApiResponse<InventoryOrganizerValue[]>> {
    const response = await fetchWithRetry(
      `${API_BASE}/inventories/${inventoryId}/organizer-values`,
      { headers: getHeaders() }
    );
    return handleResponse<InventoryOrganizerValue[]>(response);
  },

  async delete(id: number): Promise<ApiResponse<Record<string, never>>> {
    const response = await fetchWithRetry(`${API_BASE}/inventories/${id}`, {
      method: 'DELETE',
      headers: getHeaders(),
    });
    return handleResponse<Record<string, never>>(response);
  },
};

// Items API
export const itemApi = {
  async getAll(): Promise<ApiResponse<Item[]>> {
    const response = await fetchWithRetry(`${API_BASE}/items`, {
      headers: getHeaders(),
    });
    return handleResponse<Item[]>(response);
  },

  async getById(id: number): Promise<ApiResponse<Item>> {
    const response = await fetchWithRetry(`${API_BASE}/items/${id}`, {
      headers: getHeaders(),
    });
    return handleResponse<Item>(response);
  },

  async create(data: CreateItemRequest): Promise<ApiResponse<Item>> {
    const response = await fetchWithRetry(`${API_BASE}/items`, {
      method: 'POST',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<Item>(response);
  },

  async update(id: number, data: UpdateItemRequest): Promise<ApiResponse<Item>> {
    const response = await fetchWithRetry(`${API_BASE}/items/${id}`, {
      method: 'PUT',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<Item>(response);
  },

  async delete(id: number): Promise<ApiResponse<boolean>> {
    const response = await fetchWithRetry(`${API_BASE}/items/${id}`, {
      method: 'DELETE',
      headers: getHeaders(),
    });
    return handleResponse<boolean>(response);
  },

  /** Search the items the user can see. The term goes in `?q=`, so it can hold any character. */
  async search(query: string, limit?: number): Promise<ApiResponse<ItemSearchResults>> {
    const params = new URLSearchParams({ q: query });
    if (limit !== undefined) {
      params.set('limit', String(limit));
    }
    const response = await fetchWithRetry(`${API_BASE}/items/search?${params.toString()}`, {
      headers: getHeaders(),
    });
    return handleResponse<ItemSearchResults>(response);
  },

  async getOrganizerValues(itemId: number): Promise<ApiResponse<ItemOrganizerValueWithDetails[]>> {
    const response = await fetchWithRetry(`${API_BASE}/items/${itemId}/organizer-values`, {
      headers: getHeaders(),
    });
    return handleResponse<ItemOrganizerValueWithDetails[]>(response);
  },

  async setOrganizerValues(
    itemId: number,
    data: SetItemOrganizerValuesRequest
  ): Promise<ApiResponse<ItemOrganizerValue[]>> {
    const response = await fetchWithRetry(`${API_BASE}/items/${itemId}/organizer-values`, {
      method: 'PUT',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<ItemOrganizerValue[]>(response);
  },
};

// Organizer API
export const organizerApi = {
  // Organizer Types
  async getByInventory(inventoryId: number): Promise<ApiResponse<OrganizerTypeWithOptions[]>> {
    const response = await fetchWithRetry(`${API_BASE}/inventories/${inventoryId}/organizers`, {
      headers: getHeaders(),
    });
    return handleResponse<OrganizerTypeWithOptions[]>(response);
  },

  async createType(
    inventoryId: number,
    data: CreateOrganizerTypeRequest
  ): Promise<ApiResponse<OrganizerType>> {
    const response = await fetchWithRetry(`${API_BASE}/inventories/${inventoryId}/organizers`, {
      method: 'POST',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<OrganizerType>(response);
  },

  async getType(id: number): Promise<ApiResponse<OrganizerType>> {
    const response = await fetchWithRetry(`${API_BASE}/organizers/${id}`, {
      headers: getHeaders(),
    });
    return handleResponse<OrganizerType>(response);
  },

  async updateType(
    id: number,
    data: UpdateOrganizerTypeRequest
  ): Promise<ApiResponse<OrganizerType>> {
    const response = await fetchWithRetry(`${API_BASE}/organizers/${id}`, {
      method: 'PUT',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<OrganizerType>(response);
  },

  async deleteType(id: number): Promise<ApiResponse<Record<string, never>>> {
    const response = await fetchWithRetry(`${API_BASE}/organizers/${id}`, {
      method: 'DELETE',
      headers: getHeaders(),
    });
    return handleResponse<Record<string, never>>(response);
  },

  // Organizer Options
  async getOptions(organizerTypeId: number): Promise<ApiResponse<OrganizerOption[]>> {
    const response = await fetchWithRetry(`${API_BASE}/organizers/${organizerTypeId}/options`, {
      headers: getHeaders(),
    });
    return handleResponse<OrganizerOption[]>(response);
  },

  async createOption(
    organizerTypeId: number,
    data: CreateOrganizerOptionRequest
  ): Promise<ApiResponse<OrganizerOption>> {
    const response = await fetchWithRetry(`${API_BASE}/organizers/${organizerTypeId}/options`, {
      method: 'POST',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<OrganizerOption>(response);
  },

  async updateOption(
    optionId: number,
    data: UpdateOrganizerOptionRequest
  ): Promise<ApiResponse<OrganizerOption>> {
    const response = await fetchWithRetry(`${API_BASE}/organizer-options/${optionId}`, {
      method: 'PUT',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<OrganizerOption>(response);
  },

  async deleteOption(optionId: number): Promise<ApiResponse<Record<string, never>>> {
    const response = await fetchWithRetry(`${API_BASE}/organizer-options/${optionId}`, {
      method: 'DELETE',
      headers: getHeaders(),
    });
    return handleResponse<Record<string, never>>(response);
  },
};

// ==================== Image API ====================

export const imageApi = {
  async upload(file: File): Promise<ApiResponse<ImageUploadResponse>> {
    const token = getToken();
    const formData = new FormData();
    formData.append('image', file);

    const response = await fetchWithRetry(`${API_BASE}/images/upload`, {
      method: 'POST',
      headers: {
        // Do NOT set Content-Type for multipart - browser sets it with boundary
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
      body: formData,
    });
    return handleResponse<ImageUploadResponse>(response);
  },

  async delete(filename: string): Promise<ApiResponse<void>> {
    const response = await fetchWithRetry(`${API_BASE}/images/${encodeURIComponent(filename)}`, {
      method: 'DELETE',
      headers: getHeaders(),
    });
    return handleResponse<undefined>(response);
  },

  async getInventoryItemImages(inventoryId: number): Promise<ApiResponse<Record<string, string>>> {
    const response = await fetchWithRetry(`${API_BASE}/inventories/${inventoryId}/item-images`, {
      headers: getHeaders(),
    });
    return handleResponse<Record<string, string>>(response);
  },
};

// Health check
export async function checkHealth(): Promise<ApiResponse<{ status: string; message: string }>> {
  const response = await fetchWithRetry(`${API_BASE}/health`, {
    headers: getHeaders(false), // Public endpoint - no auth required
  });
  return handleResponse<{ status: string; message: string }>(response);
}

// ==================== Authentication API ====================

export const authApi = {
  // Check if initial setup is needed
  async checkSetupStatus(): Promise<ApiResponse<SetupStatusResponse>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/setup/status`, {
      headers: { 'Content-Type': 'application/json' }, // Public endpoint - no auth required
    });
    return handleResponse<SetupStatusResponse>(response);
  },

  // Initial setup - create first admin user
  async setup(data: InitialSetupRequest): Promise<ApiResponse<LoginResponse>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/setup`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    });
    return handleResponse<LoginResponse>(response);
  },

  // Login
  async login(data: LoginRequest): Promise<ApiResponse<LoginApiResponse>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    });
    return handleResponse<LoginApiResponse>(response);
  },

  // Register new user (after initial setup)
  async register(data: {
    username: string;
    full_name: string;
    password: string;
    /** The invitation from the link, needed while registration is invite-only */
    invite_token?: string;
  }): Promise<ApiResponse<LoginResponse>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/register`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    });
    return handleResponse<LoginResponse>(response);
  },

  // Whether an invitation can be used. Yes or no only; the token goes in the body, not the URL.
  async checkInvite(token: string): Promise<ApiResponse<{ valid: boolean }>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/invite/check`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ token }),
    });
    return handleResponse<{ valid: boolean }>(response);
  },

  // Set a new password with a reset link from an administrator
  async resetPassword(token: string, newPassword: string): Promise<ApiResponse<null>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/reset-password`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ token, new_password: newPassword }),
    });
    return handleResponse<null>(response);
  },

  // Get current user profile
  async getProfile(token?: string): Promise<ApiResponse<User>> {
    const headers: Record<string, string> = { 'Content-Type': 'application/json' };
    if (token) {
      headers.Authorization = `Bearer ${token}`;
    } else {
      const storedToken = getToken();
      if (storedToken) {
        headers.Authorization = `Bearer ${storedToken}`;
      }
    }

    const response = await fetchWithRetry(`${API_BASE}/auth/me`, {
      headers,
    });
    return handleResponse<User>(response);
  },

  // Update current user profile
  async updateProfile(data: UpdateProfileRequest): Promise<ApiResponse<User>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/me`, {
      method: 'PUT',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<User>(response);
  },

  // Change password
  // The server signs out the account's other devices and returns a fresh token for this one
  async changePassword(data: ChangePasswordRequest): Promise<ApiResponse<SessionTokenResponse>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/password`, {
      method: 'PUT',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    const result = await handleResponse<SessionTokenResponse>(response);
    adoptSessionToken(result.data?.token);
    return result;
  },

  // Sign out every other device; this device stays signed in with a fresh token
  async logoutAllDevices(): Promise<ApiResponse<SessionTokenResponse>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/logout-all`, {
      method: 'POST',
      headers: getHeaders(),
    });
    const result = await handleResponse<SessionTokenResponse>(response);
    adoptSessionToken(result.data?.token);
    return result;
  },

  // Get user settings
  async getSettings(token?: string): Promise<ApiResponse<UserSettings>> {
    const headers: Record<string, string> = { 'Content-Type': 'application/json' };
    if (token) {
      headers.Authorization = `Bearer ${token}`;
    } else {
      const storedToken = getToken();
      if (storedToken) {
        headers.Authorization = `Bearer ${storedToken}`;
      }
    }

    const response = await fetchWithRetry(`${API_BASE}/auth/settings`, {
      headers,
    });
    return handleResponse<UserSettings>(response);
  },

  // Update user settings
  async updateSettings(
    token: string,
    data: Partial<UpdateUserSettingsRequest>
  ): Promise<ApiResponse<UserSettings>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/settings`, {
      method: 'PUT',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify(data),
    });
    return handleResponse<UserSettings>(response);
  },

  // Admin: Get all users
  async getAllUsers(): Promise<ApiResponse<User[]>> {
    const response = await fetchWithRetry(`${API_BASE}/admin/users`, {
      headers: getHeaders(),
    });
    return handleResponse<User[]>(response);
  },

  // Admin: Get specific user
  async getUser(userId: string): Promise<ApiResponse<User>> {
    const response = await fetchWithRetry(`${API_BASE}/admin/users/${userId}`, {
      headers: getHeaders(),
    });
    return handleResponse<User>(response);
  },

  // Admin: Create new user
  async createUser(data: CreateUserRequest): Promise<ApiResponse<User>> {
    const response = await fetchWithRetry(`${API_BASE}/admin/users`, {
      method: 'POST',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<User>(response);
  },

  // Admin: Update user
  async updateUser(userId: string, data: UpdateUserRequest): Promise<ApiResponse<User>> {
    const response = await fetchWithRetry(`${API_BASE}/admin/users/${userId}`, {
      method: 'PUT',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<User>(response);
  },

  // Admin: Delete user
  async deleteUser(userId: string): Promise<ApiResponse<{ message: string }>> {
    const response = await fetchWithRetry(`${API_BASE}/admin/users/${userId}`, {
      method: 'DELETE',
      headers: getHeaders(),
    });
    return handleResponse<{ message: string }>(response);
  },

  // Admin: who may register
  async getRegistrationSettings(): Promise<ApiResponse<RegistrationSettings>> {
    const response = await fetchWithRetry(`${API_BASE}/admin/settings/registration`, {
      headers: getHeaders(),
    });
    return handleResponse<RegistrationSettings>(response);
  },

  // Admin: choose who may register and/or dismiss the upgrade notice
  async updateRegistrationSettings(data: {
    registration_mode?: RegistrationMode;
    dismiss_upgrade_notice?: boolean;
  }): Promise<ApiResponse<RegistrationSettings>> {
    const response = await fetchWithRetry(`${API_BASE}/admin/settings/registration`, {
      method: 'PUT',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<RegistrationSettings>(response);
  },

  // Admin: create an invitation (the token in the answer is shown once)
  async createInvite(expiresInHours?: number): Promise<ApiResponse<IssuedToken>> {
    const response = await fetchWithRetry(`${API_BASE}/admin/invites`, {
      method: 'POST',
      headers: getHeaders(),
      body: JSON.stringify(
        expiresInHours === undefined ? {} : { expires_in_hours: expiresInHours }
      ),
    });
    return handleResponse<IssuedToken>(response);
  },

  // Admin: invitations that can still be used
  async listInvites(): Promise<ApiResponse<InviteInfo[]>> {
    const response = await fetchWithRetry(`${API_BASE}/admin/invites`, {
      headers: getHeaders(),
    });
    return handleResponse<InviteInfo[]>(response);
  },

  // Admin: revoke an unused invitation
  async revokeInvite(inviteId: string): Promise<ApiResponse<null>> {
    const response = await fetchWithRetry(`${API_BASE}/admin/invites/${inviteId}`, {
      method: 'DELETE',
      headers: getHeaders(),
    });
    return handleResponse<null>(response);
  },

  // Admin: create a password reset link for a user (the token in the answer is shown once)
  async issueResetLink(userId: string, expiresInHours?: number): Promise<ApiResponse<IssuedToken>> {
    const response = await fetchWithRetry(`${API_BASE}/admin/users/${userId}/reset-link`, {
      method: 'POST',
      headers: getHeaders(),
      body: JSON.stringify(
        expiresInHours === undefined ? {} : { expires_in_hours: expiresInHours }
      ),
    });
    return handleResponse<IssuedToken>(response);
  },

  // ==================== Inventory Sharing ====================

  // Get shares for an inventory
  async getInventoryShares(inventoryId: number): Promise<ApiResponse<InventoryShare[]>> {
    const response = await fetchWithRetry(`${API_BASE}/inventories/${inventoryId}/shares`, {
      headers: getHeaders(),
    });
    return handleResponse<InventoryShare[]>(response);
  },

  // Share an inventory with another user
  async shareInventory(
    inventoryId: number,
    data: CreateInventoryShareRequest
  ): Promise<ApiResponse<InventoryShare>> {
    const response = await fetchWithRetry(`${API_BASE}/inventories/${inventoryId}/shares`, {
      method: 'POST',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<InventoryShare>(response);
  },

  // Update share permission
  async updateInventoryShare(
    shareId: string,
    data: UpdateInventoryShareRequest
  ): Promise<ApiResponse<InventoryShare>> {
    const response = await fetchWithRetry(`${API_BASE}/shares/${shareId}`, {
      method: 'PUT',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<InventoryShare>(response);
  },

  // Remove a share
  async removeInventoryShare(shareId: string): Promise<ApiResponse<Record<string, never>>> {
    const response = await fetchWithRetry(`${API_BASE}/shares/${shareId}`, {
      method: 'DELETE',
      headers: getHeaders(),
    });
    return handleResponse<Record<string, never>>(response);
  },

  // Get effective permissions for current user on an inventory
  async getInventoryPermissions(inventoryId: number): Promise<ApiResponse<EffectivePermissions>> {
    const response = await fetchWithRetry(`${API_BASE}/inventories/${inventoryId}/permissions`, {
      headers: getHeaders(),
    });
    return handleResponse<EffectivePermissions>(response);
  },

  // ==================== Ownership Transfer ====================

  // Transfer ownership of an inventory to another user
  async transferOwnership(
    inventoryId: number,
    data: TransferOwnershipRequest
  ): Promise<ApiResponse<TransferOwnershipResponse>> {
    const response = await fetchWithRetry(
      `${API_BASE}/inventories/${inventoryId}/transfer-ownership`,
      {
        method: 'POST',
        headers: getHeaders(),
        body: JSON.stringify(data),
      }
    );
    return handleResponse<TransferOwnershipResponse>(response);
  },

  // ==================== User Access Grants (All Access) ====================

  // Get users who have All Access to my inventories
  async getMyAccessGrants(): Promise<ApiResponse<UserAccessGrantWithUsers[]>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/access-grants`, {
      headers: getHeaders(),
    });
    return handleResponse<UserAccessGrantWithUsers[]>(response);
  },

  // Get users who have granted me All Access
  async getReceivedAccessGrants(): Promise<ApiResponse<UserAccessGrantWithUsers[]>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/access-grants/received`, {
      headers: getHeaders(),
    });
    return handleResponse<UserAccessGrantWithUsers[]>(response);
  },

  // Grant All Access to another user
  async createAccessGrant(
    data: CreateUserAccessGrantRequest
  ): Promise<ApiResponse<UserAccessGrant>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/access-grants`, {
      method: 'POST',
      headers: getHeaders(),
      body: JSON.stringify(data),
    });
    return handleResponse<UserAccessGrant>(response);
  },

  // Revoke All Access from a user
  async revokeAccessGrant(grantId: string): Promise<ApiResponse<Record<string, never>>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/access-grants/${grantId}`, {
      method: 'DELETE',
      headers: getHeaders(),
    });
    return handleResponse<Record<string, never>>(response);
  },

  // ==================== Recovery Codes ====================

  // Generate new recovery codes (replaces any existing codes); requires the current password
  async generateRecoveryCodes(
    currentPassword: string
  ): Promise<ApiResponse<RecoveryCodesResponse>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/recovery-codes/generate`, {
      method: 'POST',
      headers: getHeaders(),
      body: JSON.stringify({ current_password: currentPassword }),
    });
    return handleResponse<RecoveryCodesResponse>(response);
  },

  // Get recovery codes status (not the codes themselves)
  async getRecoveryCodesStatus(): Promise<ApiResponse<RecoveryCodesStatus>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/recovery-codes/status`, {
      headers: getHeaders(),
    });
    return handleResponse<RecoveryCodesStatus>(response);
  },

  // Confirm that user has saved recovery codes
  async confirmRecoveryCodes(): Promise<ApiResponse<Record<string, never>>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/recovery-codes/confirm`, {
      method: 'POST',
      headers: getHeaders(),
      body: JSON.stringify({ confirmed: true }),
    });
    return handleResponse<Record<string, never>>(response);
  },

  // Use a recovery code to reset password (no auth required)
  async useRecoveryCode(
    username: string,
    recoveryCode: string,
    newPassword: string
  ): Promise<ApiResponse<RecoveryCodeUsedResponse>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/recovery-codes/use`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        username,
        recovery_code: recoveryCode,
        new_password: newPassword,
      }),
    });
    return handleResponse<RecoveryCodeUsedResponse>(response);
  },

  // ==================== TOTP Authenticator ====================

  // Get TOTP status for current user
  async getTotpStatus(): Promise<ApiResponse<TotpStatusResponse>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/totp/status`, {
      headers: getHeaders(),
    });
    return handleResponse<TotpStatusResponse>(response);
  },

  // Initiate TOTP setup (generate secret + QR code)
  async setupTotp(): Promise<ApiResponse<TotpSetupResponse>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/totp/setup`, {
      method: 'POST',
      headers: getHeaders(),
    });
    return handleResponse<TotpSetupResponse>(response);
  },

  // Verify first TOTP code to enable TOTP
  async verifyTotpSetup(
    code: string,
    mode: TotpMode
  ): Promise<ApiResponse<TotpVerifySetupResponse>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/totp/verify-setup`, {
      method: 'POST',
      headers: getHeaders(),
      body: JSON.stringify({ code, mode }),
    });
    const result = await handleResponse<TotpVerifySetupResponse>(response);
    adoptSessionToken(result.data?.token);
    return result;
  },

  // Verify TOTP during login (uses partial_token)
  async verifyTotp(partialToken: string, code: string): Promise<ApiResponse<LoginResponse>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/totp/verify`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${partialToken}`,
      },
      body: JSON.stringify({ code }),
    });
    return handleResponse<LoginResponse>(response);
  },

  // Recover account using TOTP code (no auth required)
  async recoverWithTotp(
    username: string,
    totpCode: string,
    newPassword: string
  ): Promise<ApiResponse<{ message: string }>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/totp/recover`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        username,
        totp_code: totpCode,
        new_password: newPassword,
      }),
    });
    return handleResponse<{ message: string }>(response);
  },

  // Change TOTP mode
  async changeTotpMode(mode: TotpMode): Promise<ApiResponse<{ mode: string }>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/totp/mode`, {
      method: 'PUT',
      headers: getHeaders(),
      body: JSON.stringify({ mode }),
    });
    return handleResponse<{ mode: string }>(response);
  },

  // Disable TOTP (requires password confirmation)
  async disableTotp(password: string): Promise<ApiResponse<{ token?: string }>> {
    const response = await fetchWithRetry(`${API_BASE}/auth/totp`, {
      method: 'DELETE',
      headers: getHeaders(),
      body: JSON.stringify({ password }),
    });
    const result = await handleResponse<{ token?: string }>(response);
    adoptSessionToken(result.data?.token);
    return result;
  },
};

// ==================== Inventory Reports ====================
export const reportApi = {
  // Get comprehensive inventory report
  async getInventoryReport(
    params: InventoryReportParams
  ): Promise<ApiResponse<InventoryReportData>> {
    const queryParams = new URLSearchParams();
    Object.entries(params).forEach(([key, value]) => {
      if (value !== undefined && value !== null && value !== '') {
        queryParams.append(key, String(value));
      }
    });

    const response = await fetchWithRetry(
      `${API_BASE}/reports/inventory?${queryParams.toString()}`,
      {
        headers: getHeaders(),
      }
    );
    return handleResponse<InventoryReportData>(response);
  },

  // Download report as CSV
  async downloadReportCSV(params: InventoryReportParams): Promise<Blob> {
    const queryParams = new URLSearchParams();
    Object.entries({ ...params, format: 'csv' }).forEach(([key, value]) => {
      if (value !== '') {
        queryParams.append(key, String(value));
      }
    });

    const response = await fetchWithRetry(
      `${API_BASE}/reports/inventory?${queryParams.toString()}`,
      {
        headers: {
          Authorization: `Bearer ${getToken()}`,
        },
      }
    );

    if (!response.ok) {
      throw new Error('Failed to download CSV');
    }

    return response.blob();
  },

  // Get inventory statistics
  async getStatistics(inventoryId?: number): Promise<ApiResponse<InventoryStatistics>> {
    const query = inventoryId ? `?inventory_id=${inventoryId}` : '';
    const response = await fetchWithRetry(`${API_BASE}/reports/inventory/statistics${query}`, {
      headers: getHeaders(),
    });
    return handleResponse<InventoryStatistics>(response);
  },

  // Get category breakdown
  async getCategoryBreakdown(inventoryId?: number): Promise<ApiResponse<CategorySummary[]>> {
    const query = inventoryId ? `?inventory_id=${inventoryId}` : '';
    const response = await fetchWithRetry(`${API_BASE}/reports/inventory/categories${query}`, {
      headers: getHeaders(),
    });
    return handleResponse<CategorySummary[]>(response);
  },
};

// Export types for convenience
export type {
  InventoryReportParams,
  InventoryStatistics,
  CategorySummary,
  InventoryReportData,
} from '@/types';

// ==================== Backup & Restore API ====================

export const backupApi = {
  // Create a new backup
  async create(): Promise<ApiResponse<BackupInfo>> {
    const response = await fetchWithRetry(`${API_BASE}/backup/create`, {
      method: 'POST',
      headers: getHeaders(),
    });
    return handleResponse<BackupInfo>(response);
  },

  // List all available backups
  async list(): Promise<ApiResponse<BackupInfo[]>> {
    const response = await fetchWithRetry(`${API_BASE}/backup/list`, {
      headers: getHeaders(),
    });
    return handleResponse<BackupInfo[]>(response);
  },

  // Download a backup file
  async download(filename: string): Promise<void> {
    const token = getToken();
    const response = await fetch(`${API_BASE}/backup/download/${encodeURIComponent(filename)}`, {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    if (!response.ok) {
      throw new Error('Download failed');
    }

    const blob = await response.blob();
    const link = document.createElement('a');
    link.href = URL.createObjectURL(blob);
    link.download = filename;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(link.href);
  },

  // Upload a backup file
  async upload(file: File): Promise<ApiResponse<BackupInfo>> {
    const token = getToken();
    const formData = new FormData();
    formData.append('file', file);

    const response = await fetch(`${API_BASE}/backup/upload`, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${token}`,
      },
      body: formData,
    });
    return handleResponse<BackupInfo>(response);
  },

  // Restore from a server-side backup
  // `restoreWithout2fa` restores everything except two-factor enrolment; the server only needs
  // it when the backup's TOTP key differs from this server's (error code `totp_key_mismatch`)
  async restore(filename: string, restoreWithout2fa = false): Promise<ApiResponse<RestoreReport>> {
    const response = await fetchWithRetry(
      `${API_BASE}/backup/restore/${encodeURIComponent(filename)}`,
      {
        method: 'POST',
        headers: getHeaders(),
        body: JSON.stringify({ restore_without_2fa: restoreWithout2fa }),
      }
    );
    return handleResponse<RestoreReport>(response);
  },

  // Delete a backup
  async delete(filename: string): Promise<ApiResponse<null>> {
    const response = await fetchWithRetry(`${API_BASE}/backup/${encodeURIComponent(filename)}`, {
      method: 'DELETE',
      headers: getHeaders(),
    });
    return handleResponse<null>(response);
  },
};
