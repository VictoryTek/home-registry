# Remove Email from Login Form - Specification

**Created:** February 13, 2026  
**Status:** Research Complete  
**Category:** Bug Fix / UI Cleanup

---

## Executive Summary

The login form currently displays "Username or Email" text, but the application does not support email-based authentication. User accounts only have usernames, not email addresses. This specification documents the required changes to remove email references from the login UI.

---

## Current State Analysis

### Frontend Files with Email References

#### 1. `frontend/src/pages/LoginPage.tsx`

**Location:** Lines 28, 80, 88

**Occurrences:**

1. **Line 28** - Error message:
   ```typescript
   setError('Please enter your username or email');
   ```

2. **Line 80** - Label text:
   ```tsx
   <label htmlFor="username">
     <i className="fas fa-user"></i>
     Username or Email
   </label>
   ```

3. **Line 88** - Placeholder text:
   ```tsx
   <input
     type="text"
     id="username"
     name="username"
     value={formData.username}
     onChange={handleInputChange}
     placeholder="Enter your username or email"
     autoFocus
     autoComplete="username"
   />
   ```

**Current Form State:**
- Field name: `username`
- Field type: `text`
- Autocomplete: `username`
- Validation: Checks if `formData.username.trim()` is empty

### Backend Implementation Review

#### 1. `src/api/auth.rs` (Lines 280-350)

**Login Endpoint:**
```rust
pub async fn login(pool: web::Data<Pool>, req: web::Json<LoginRequest>) -> Result<impl Responder> {
    // Calls get_user_by_username_or_email but only searches by username
    let user = match db_service
        .get_user_by_username_or_email(&req.username)
        .await
```

**Error Messages:**
- Line 294: `"Username or password is incorrect"`
- Line 336: `"Username or password is incorrect"`

#### 2. `src/db/mod.rs` (Lines 1208-1240)

**Database Function:**
```rust
pub async fn get_user_by_username_or_email(
    &self,
    identifier: &str,
) -> Result<Option<User>, Box<dyn std::error::Error>> {
    // SQL query only checks username, NOT email
    "SELECT ... FROM users WHERE LOWER(username) = LOWER($1)"
```

**Finding:** Despite the function name suggesting email support, it **only queries by username**.

#### 3. `src/models/mod.rs` (Lines 331-345)

**User Model:**
```rust
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub full_name: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub is_admin: bool,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // ... recovery code fields
}
```

**Finding:** No `email` field exists in the User model.

#### 4. `src/models/mod.rs` (Lines 408-411)

**LoginRequest Model:**
```rust
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}
```

**Finding:** LoginRequest accepts `username` field, not email.

### Cosmetic Issues (Optional to Fix)

#### 1. `frontend/src/components/UserMenu.tsx` (Line 89)

```tsx
<span className="menu-user-email">@{user.username}</span>
```

**Issue:** CSS class name `.menu-user-email` is misleading - it displays username, not email.  
**Suggested Fix:** Rename to `.menu-user-handle` or `.menu-user-username`.

#### 2. `frontend/src/styles/layout.css` (Line 310)

**CSS Class:**
```css
.menu-user-email {
  /* styling */
}
```

**Issue:** Same as above - naming artifact from earlier design.

---

## Root Cause

The login UI was likely copied from a template or previous project that supported email authentication. However, the backend implementation was simplified to username-only authentication, but the frontend UI text was not updated to match.

---

## Proposed Solution

### Required Changes (Must Fix)

#### Change 1: Update Error Message
**File:** `frontend/src/pages/LoginPage.tsx`  
**Line:** 28  
**Current:**
```typescript
setError('Please enter your username or email');
```
**New:**
```typescript
setError('Please enter your username');
```

#### Change 2: Update Label Text
**File:** `frontend/src/pages/LoginPage.tsx`  
**Line:** 80  
**Current:**
```tsx
Username or Email
```
**New:**
```tsx
Username
```

#### Change 3: Update Placeholder Text
**File:** `frontend/src/pages/LoginPage.tsx`  
**Line:** 88  
**Current:**
```tsx
placeholder="Enter your username or email"
```
**New:**
```tsx
placeholder="Enter your username"
```

### Optional Backend Refactoring (Recommended for Clarity)

#### Optional Change 1: Rename Database Function
**File:** `src/db/mod.rs`  
**Line:** 1208  
**Current:** `pub async fn get_user_by_username_or_email(`  
**Suggested:** `pub async fn get_user_by_username(`

**Impact:** Would require updating the call site in `src/api/auth.rs` line 285.

**Justification:** Function name should reflect actual behavior (username-only lookup).

#### Optional Change 2: Update Backend Error Message
**File:** `src/api/auth.rs`  
**Lines:** 294, 336  
**Current:** `"Username or password is incorrect"`  
**Suggested:** Keep as-is (security best practice to not reveal which field is wrong)

**Rationale:** Error message intentionally vague for security reasons. No change needed.

### Optional Frontend Refactoring (Cosmetic)

#### Optional Change 3: Rename CSS Class
**File:** `frontend/src/components/UserMenu.tsx`  
**Line:** 89  
**Current:** `className="menu-user-email"`  
**Suggested:** `className="menu-user-handle"`

#### Optional Change 4: Update CSS Definition
**File:** `frontend/src/styles/layout.css`  
**Line:** 310  
**Current:** `.menu-user-email {`  
**Suggested:** `.menu-user-handle {`

---

## Implementation Plan

### Phase 1: Required Frontend Changes (5 minutes)

1. Open `frontend/src/pages/LoginPage.tsx`
2. Update line 28: Change error message to "Please enter your username"
3. Update line 80: Change label to "Username"
4. Update line 88: Change placeholder to "Enter your username"
5. Save file

### Phase 2: Verify Changes (5 minutes)

1. Build frontend: `cd frontend && npm run build`
2. Test login form displays correct text
3. Test error validation shows correct message
4. Verify login functionality still works

### Phase 3: Optional Refactoring (15 minutes)

**If desired:**

1. Rename `get_user_by_username_or_email` to `get_user_by_username`
   - Update `src/db/mod.rs` line 1208
   - Update call site in `src/api/auth.rs` line 285

2. Rename `.menu-user-email` CSS class
   - Update `frontend/src/components/UserMenu.tsx` line 89
   - Update `frontend/src/styles/layout.css` line 310

3. Rebuild and test entire application

---

## Risk Assessment

### Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Breaking login functionality | Medium | No logic changes, only text updates |
| CSS styling issues | Low | Class usage is localized |
| Backend function rename breaks other code | Medium | Search codebase for all usages first |
| Translation/i18n issues | Low | No i18n system currently in use |

### Testing Requirements

**Manual Testing:**
1. Open login page and verify text displays "Username" not "Username or Email"
2. Submit empty username → should show "Please enter your username"
3. Submit valid credentials → login should work normally
4. Submit invalid credentials → should show "Username or password is incorrect"
5. Check user menu displays `@username` correctly

**Automated Testing:**
- No existing frontend tests to update
- Backend tests don't reference the function name

---

## Dependencies

### Build Dependencies
- TypeScript compiler
- Vite build system (frontend)
- Cargo build (if backend changes made)

### Runtime Dependencies
- No new dependencies required
- No database migrations needed

---

## Validation Criteria

### Success Criteria

✅ Login form label shows "Username" (not "Username or Email")  
✅ Login form placeholder shows "Enter your username"  
✅ Error message shows "Please enter your username"  
✅ Login functionality continues to work correctly  
✅ No console errors in browser  
✅ No build errors  

### Regression Testing

Test these scenarios to ensure no breakage:
- ✅ Login with valid username/password
- ✅ Login with invalid username
- ✅ Login with invalid password
- ✅ Login with empty fields
- ✅ Password visibility toggle works
- ✅ "Remember me" autocomplete behavior
- ✅ Navigation to register page
- ✅ Navigation to password recovery

---

## Files Summary

### Files Requiring Changes (Required)

1. ✅ `frontend/src/pages/LoginPage.tsx` - 3 text changes

### Files to Consider (Optional)

2. ⚪ `src/db/mod.rs` - Function rename
3. ⚪ `src/api/auth.rs` - Update function call
4. ⚪ `frontend/src/components/UserMenu.tsx` - CSS class rename
5. ⚪ `frontend/src/styles/layout.css` - CSS class rename

### Files Referenced (No Changes Needed)

- `src/models/mod.rs` - Verified User model has no email field
- `frontend/src/pages/RegisterPage.tsx` - Correctly uses username only
- `frontend/src/context/AuthContext.tsx` - Login function is backend-agnostic

---

## Notes

- Migration #017 removed email column from database (see `migrations/017_remove_email_column.sql`)
- RegisterPage correctly uses "Username" labels (no changes needed there)
- Backend error messages intentionally use "Username or password" for security (recommended practice)
- No database changes required for this fix
- No API contract changes (request/response formats unchanged)

---

## Estimated Effort

- **Required changes only:** 5-10 minutes
- **With optional refactoring:** 20-30 minutes
- **Testing:** 10 minutes
- **Total:** 15-40 minutes depending on scope

---

## Conclusion

This is a straightforward text update to align the frontend UI with the backend implementation. The system already uses username-only authentication; the login form just needs its display text corrected. No logic changes or database migrations are required.

**Recommendation:** Implement required changes immediately. Consider optional refactoring as a separate code cleanup task.
