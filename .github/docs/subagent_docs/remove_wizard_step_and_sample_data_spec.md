# Remove Wizard Step 3 and Sample Data - Comprehensive Specification

**Created:** February 15, 2026  
**Project:** Home Registry  
**Status:** Ready for Implementation

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Current State Analysis](#current-state-analysis)
3. [Proposed Solution](#proposed-solution)
4. [Research & Best Practices](#research--best-practices)
5. [Implementation Plan](#implementation-plan)
6. [Files to Modify](#files-to-modify)
7. [Database Migration Strategy](#database-migration-strategy)
8. [Dependencies & Requirements](#dependencies--requirements)
9. [Risk Analysis & Mitigations](#risk-analysis--mitigations)
10. [Testing Recommendations](#testing-recommendations)

---

## Executive Summary

This specification outlines the removal of:
1. **Step 3** ("Create First Inventory") from the setup wizard
2. **All sample data** functionality (migrations, backend code, scripts)

**Rationale:**
- Users can create inventories after setup through the main UI
- Sample data creates confusion about data ownership
- Production releases should not include development/testing data
- Simplifies onboarding flow from 4 steps to 3 steps

**Impact Areas:**
- Frontend: Setup wizard component (TypeScript/React)
- Backend: Auth endpoint and database service (Rust)
- Database: Sample data migrations (SQL)
- Scripts: Sample data assignment utilities (PowerShell)
- Documentation: README and testing guides (Markdown)

---

## Current State Analysis

### 1. Setup Wizard Structure

**File:** `frontend/src/pages/SetupPage.tsx` (461 lines)

**Current Flow:**

```
Step 1: Account Details
├─ Full Name
└─ Username

Step 2: Password
├─ Password
└─ Confirm Password

Step 3: Create First Inventory ← TO BE REMOVED
└─ Inventory Name (Optional)

Step 4: Recovery Codes ← TO BECOME STEP 3
├─ Display 8 recovery codes
├─ Download/Copy/Print options
└─ Confirmation checkbox
```

**Key Implementation Details:**

1. **State Management** (Lines 9-22):
   ```typescript
   const [step, setStep] = useState(1);
   const [formData, setFormData] = useState({
     username: '',
     full_name: '',
     password: '',
     confirmPassword: '',
     inventory_name: '',  // ← TO BE REMOVED
   });
   const [recoveryCodes, setRecoveryCodes] = useState<string[] | null>(null);
   ```

2. **Progress Indicator** (Lines 243-270):
   - Uses CSS classes: `progress-step`, `step-number`
   - Shows 4 steps: Account, Security, Inventory, Recovery
   - Hard-coded step numbers (1, 2, 3, 4)

3. **Navigation Logic** (Lines 66-70):
   ```typescript
   const handleNext = () => {
     if (step === 1 && validateStep1()) {
       setStep(2);
     } else if (step === 2 && validateStep2()) {
       setStep(3);  // ← WILL SKIP TO RECOVERY CODES
     }
   };
   ```

4. **Step 3 Submission** (Lines 75-79):
   ```typescript
   const handleStep3Submit = async () => {
     setIsLoading(true);
     setError(null);
     await completeSetup();  // Sends inventory_name to backend
   };
   ```

5. **Backend API Call** (Lines 92-97):
   ```typescript
   const result = await authApi.setup({
     username: formData.username,
     full_name: formData.full_name,
     password: formData.password,
     inventory_name: formData.inventory_name || undefined,  // ← TO BE REMOVED
   });
   ```

6. **Conditional Rendering** (Lines 347-369):
   - Step 3 renders form with `inventory_name` input
   - Input is marked as "(Optional)"
   - Includes hint: "You can skip this and create inventories later."

7. **Button Logic** (Lines 427-445):
   - Back button: Shows for steps 2-3 (not step 1 or 4)
   - Next button: Shows for steps 1-2
   - Complete Setup button: Shows for step 3
   - Done button: Shows for step 4

**CSS Styling:**

**File:** `frontend/src/styles/auth.css`

- **Lines 243-270**: `.setup-progress` grid with 4 steps
- **Lines 400-424**: `.auth-step` content styling
- **Lines 425-431**: `.auth-actions` button layout
- **Lines 522-633**: Recovery codes display (`.recovery-codes-display`, `.codes-grid`)

**Progress Indicator Structure:**
```css
.setup-progress {
  display: grid;
  grid-template-columns: repeat(7, 1fr); /* 4 steps + 3 progress lines */
}
```

### 2. Backend Sample Data Implementation

#### A. Database Service Method

**File:** `src/db/mod.rs` (Lines 106-122)

```rust
/// Assign all inventories with NULL `user_id` to the specified user
/// Used during initial setup to assign sample data to first admin
/// Returns the number of inventories assigned
pub async fn assign_sample_inventories_to_user(
    &self,
    user_id: Uuid,
) -> Result<u64, Box<dyn std::error::Error>> {
    let client = self.pool.get().await?;

    let result = client
        .execute(
            "UPDATE inventories SET user_id = $1, updated_at = NOW() WHERE user_id IS NULL",
            &[&user_id],
        )
        .await?;

    Ok(result)
}
```

**Purpose:** Assigns sample inventories (IDs 100-104) to first admin user during setup

#### B. Auth Setup Endpoint

**File:** `src/api/auth.rs` (Lines 223-236)

```rust
// Auto-assign sample inventories (with NULL user_id) to this first admin
match db_service.assign_sample_inventories_to_user(user.id).await {
    Ok(assigned_count) => {
        if assigned_count > 0 {
            info!(
                "Assigned {} sample inventories to first admin user: {}",
                assigned_count, user.username
            );
        }
    },
    Err(e) => {
        // Non-fatal: log warning but don't fail setup
        warn!("Failed to assign sample inventories: {}", e);
    },
}
```

**Purpose:** Called during `/auth/setup` to automatically assign sample data

#### C. Optional First Inventory Creation

**File:** `src/api/auth.rs` (Lines 238-257)

```rust
// Optionally create first inventory
if let Some(inventory_name) = &req.inventory_name {
    if !inventory_name.is_empty() {
        let inventory_request = crate::models::CreateInventoryRequest {
            name: inventory_name.clone(),
            description: Some("Initial inventory created during setup".to_string()),
            location: None,
            image_url: None,
        };

        match db_service
            .create_inventory(inventory_request, user.id)
            .await
        {
            Ok(inventory) => {
                info!(
                    "Created initial inventory: {} (ID: {:?}) for user {}",
                    inventory.name, inventory.id, user.username
                );
            },
            Err(e) => {
                warn!("Failed to create initial inventory: {}", e);
            },
        }
    }
}
```

**Purpose:** Creates user-specified inventory from Step 3 (if provided)

#### D. Data Models

**File:** `src/models/mod.rs` (Lines 772-779)

```rust
/// Request for initial admin setup (first run)
#[derive(Deserialize, Debug)]
pub struct InitialSetupRequest {
    pub username: String,
    pub full_name: String,
    pub password: String,
    pub inventory_name: Option<String>, // ← TO BE REMOVED
}
```

**File:** `frontend/src/types/index.ts` (Lines 260-265)

```typescript
export interface InitialSetupRequest {
  username: string;
  full_name: string;
  password: string;
  inventory_name?: string; // ← TO BE REMOVED
}
```

### 3. Sample Data Migrations

#### Migration 019: Create Sample Data

**File:** `migrations/019_add_sample_inventory_data.sql` (91 lines)

**Contents:**
- 5 sample inventories (IDs 100-104) with `user_id = NULL`
  - Home Office (8 items)
  - Living Room (7 items)
  - Kitchen (8 items)
  - Garage (9 items)
  - Master Bedroom (8 items)
- 40 sample items with detailed data
  - Categories: Electronics, Furniture, Appliances, Tools, Kitchenware, Sports
  - Price range: $45.99 - $2,199.99
  - Total value: ~$19,228.59
  - Date range: 2021-05-20 to 2024-06-20

**Key Features:**
- Uses fixed IDs (100-104) to avoid conflicts with user data
- Creates realistic test data for development/testing
- NULL `user_id` allows assignment to first admin

#### Migration 020: Auto-Assign Sample Data

**File:** `migrations/020_assign_sample_data_to_first_admin.sql` (30 lines)

**Purpose:** Defensive backup to application-level assignment

**Logic:**
```sql
UPDATE inventories 
SET user_id = (
    SELECT id 
    FROM users 
    WHERE is_admin = true 
    ORDER BY created_at 
    LIMIT 1
),
updated_at = NOW()
WHERE user_id IS NULL 
  AND EXISTS (SELECT 1 FROM users WHERE is_admin = true);
```

**Features:**
- Idempotent (safe to run multiple times)
- Only assigns if admin exists
- Only assigns inventories with NULL user_id
- Logs result for audit trail

### 4. Sample Data Scripts

#### PowerShell Script

**File:** `assign_sample_data.ps1` (42 lines)

**Purpose:** Manual assignment of sample data to specific user

**Usage:**
```powershell
.\assign_sample_data.ps1 -Username "admin"
```

**Functionality:**
- Updates inventories with NULL user_id to specified username
- Runs SQL via `docker compose exec db psql`
- Displays success message with inventory list
- Referenced in `TESTING_REPORTS.md`

### 5. Documentation References

#### README.md

**Current Content:**
- Line 47: "Complete the setup wizard" (mentions 4-step wizard)
- No specific mention of sample data in Quick Start section
- Sample data is mentioned in other docs but not here

#### TESTING_REPORTS.md

**Purpose:** Instructions for testing with sample data

**Content:**
- Step 1: Create user account via setup wizard
- Step 2: Run `assign_sample_data.ps1` to get sample inventories
- Step 3: Test reporting features
- Lists all 40 sample items with details

**Status:** This document becomes obsolete after sample data removal

---

## Proposed Solution

### Overview

1. **Remove Step 3** from setup wizard (inventory creation)
2. **Remove all sample data** infrastructure
3. **Renumber Step 4 to Step 3** (recovery codes)
4. **Clean up** related code, migrations, scripts, and docs

### Goals

- ✅ Simplify setup wizard to 3 steps (Account → Security → Recovery)
- ✅ Remove development/testing data from production migrations
- ✅ Eliminate confusion about sample data ownership
- ✅ Maintain clean onboarding experience
- ✅ Users create inventories naturally after setup

### Non-Goals

- ❌ Adding sample data back later as opt-in feature
- ❌ Preserving sample data for existing deployments
- ❌ Creating migration rollback to restore sample data

---

## Research & Best Practices

### Source 1: React Multi-Step Form Best Practices

**Source:** React Documentation + Kent C. Dodds Blog  
**URL:** https://kentcdodds.com/blog/application-state-management-with-react

**Key Findings:**
- **State Management:** Use `useState` for simple wizards (< 5 steps)
- **Step Validation:** Validate each step before proceeding
- **Navigation:** Allow back navigation but validate forward movement
- **Progress Indicators:** Update visual feedback when steps change
- **Conditional Rendering:** Use `step === X` pattern for clarity

**Applied to Home Registry:**
- ✅ Current implementation uses `useState(1)` for step tracking
- ✅ Validation functions (`validateStep1`, `validateStep2`) exist
- ✅ Progress indicator with 4 visual steps needs update to 3
- ⚠️ Step numbers are hard-coded (1, 2, 3, 4) - need manual updates

**Recommendation:** When removing step 3:
- Update `handleNext()` to jump from step 2 → step 3 (skipping old step 3)
- Update button logic to show "Complete Setup" on new step 3
- Update progress indicator to show 3 steps instead of 4
- Remove `inventory_name` from form state

### Source 2: Database Migration Strategies (Flyway/Liquibase Best Practices)

**Source:** Flyway Documentation, Liquibase Best Practices  
**URL:** https://flywaydb.org/documentation/  
**URL:** https://www.liquibase.org/get-started/best-practices

**Key Findings:**

**Never Delete Migrations:**
- Migrations create an audit trail of database schema evolution
- Deleting migrations breaks deployments that haven't run them yet
- Existing production databases may have data created by those migrations

**For Development Data:**
- Sample data should be separate from schema migrations
- Use `V` prefix for schema changes, `R` for repeatable scripts
- Consider environment-specific migration paths

**For Production Cleanup:**
- Create a **new migration** that removes data from old migrations
- Document why data is being removed
- Make it idempotent (safe to run multiple times)

**Applied to Home Registry:**

**Current Migration Files:**
- `019_add_sample_inventory_data.sql` - Creates sample inventories/items
- `020_assign_sample_data_to_first_admin.sql` - Auto-assigns to first admin

**Strategy Decision:**

**❌ DO NOT DELETE these migration files** because:
- Existing deployments have already run migrations 019 and 020
- Deleting them would break fresh deployments (migration numbering gap)
- Historical record of why sample data existed is valuable

**✅ DO CREATE new migration:** `021_remove_sample_data.sql`
```sql
-- Remove sample data for production release
-- Sample inventories have fixed IDs 100-104
-- This is safe to run multiple times (idempotent)

DELETE FROM items WHERE inventory_id BETWEEN 100 AND 104;
DELETE FROM inventories WHERE id BETWEEN 100 AND 104;

-- Reset sequences if needed
SELECT setval('inventories_id_seq', COALESCE((SELECT MAX(id) FROM inventories), 1), true);
SELECT setval('items_id_seq', COALESCE((SELECT MAX(id) FROM items), 1), true);
```

**Benefits:**
- Preserves migration history
- Idempotent (safe for existing deployments)
- Clear intent in filename
- Cleans up sample data from all deployments

### Source 3: Clean Database State for Production Releases

**Source:** 12 Factor App Methodology, Martin Fowler's Refactoring Databases  
**URL:** https://12factor.net/  
**URL:** https://martinfowler.com/articles/evodb.html

**Key Findings:**

**Separation of Concerns:**
- Test/sample data should not exist in production schema migrations
- Use seed scripts for development environments
- Production migrations should only contain schema and reference data

**Data Ownership:**
- Every record should have a clear owner
- NULL ownership creates security vulnerabilities
- Sample data with NULL user_id can leak across users

**Applied to Home Registry:**

**Current Issues:**
- Sample inventories use `user_id = NULL` (ownership ambiguity)
- Migration 020 automatically assigns to "first admin" (assumes single-user deployment)
- Multi-user deployments would see sample data assigned to wrong user

**Recommendation:**
- Remove sample data creation from migrations entirely
- Document how to create test data for development
- Consider separate `docker-compose.dev.yml` with seed scripts

### Source 4: React TypeScript Form Refactoring

**Source:** TypeScript Deep Dive, Total TypeScript  
**URL:** https://www.typescriptlang.org/docs/handbook/2/everyday-types.html  
**URL:** https://www.totaltypescript.com/

**Key Findings:**

**Form State Management:**
- Remove unused properties immediately (dead code)
- Update TypeScript interfaces to match backend models
- Use optional properties (`?`) only when truly optional

**Type Safety:**
- Backend and frontend types should match exactly
- Use discriminated unions for step-specific state
- Validate types at boundaries (API calls)

**Applied to Home Registry:**

**Changes Required:**

1. **Frontend Type** (`frontend/src/types/index.ts`):
   ```typescript
   export interface InitialSetupRequest {
     username: string;
     full_name: string;
     password: string;
     // inventory_name?: string; ← REMOVE THIS LINE
   }
   ```

2. **Backend Model** (`src/models/mod.rs`):
   ```rust
   #[derive(Deserialize, Debug)]
   pub struct InitialSetupRequest {
       pub username: String,
       pub full_name: String,
       pub password: String,
       // pub inventory_name: Option<String>, ← REMOVE THIS LINE
   }
   ```

3. **Form State** (`frontend/src/pages/SetupPage.tsx`):
   ```typescript
   const [formData, setFormData] = useState({
     username: '',
     full_name: '',
     password: '',
     confirmPassword: '',
     // inventory_name: '', ← REMOVE THIS LINE
   });
   ```

### Source 5: Multi-Step Wizard UX Patterns

**Source:** Nielsen Norman Group, Baymard Institute  
**URL:** https://www.nngroup.com/articles/wizard-design/  
**URL:** https://baymard.com/blog/checkout-flow-design

**Key Findings:**

**Optimal Step Count:**
- 3-5 steps is ideal for most wizards
- Too many steps increase abandonment
- Each step should have clear purpose

**Progress Indicators:**
- Show total steps and current position
- Use visual progress bar
- Label each step clearly

**Navigation:**
- Allow backward navigation
- Disable forward until validation passes
- Final step should clearly complete the process

**Applied to Home Registry:**

**Current Wizard:**
- 4 steps total
- Step 3 ("Create First Inventory") is optional
- Optional steps slow down onboarding

**After Removal:**
- 3 steps total (optimal range)
- All steps are required
- Clearer progression: Account → Security → Recovery

**Benefits:**
- Faster onboarding (one less screen)
- No optional/skippable steps (clearer purpose)
- Recovery codes are the final critical step

### Source 6: Rust API Design Best Practices

**Source:** Rust API Guidelines, Actix-Web Documentation  
**URL:** https://rust-lang.github.io/api-guidelines/  
**URL:** https://actix.rs/docs/

**Key Findings:**

**API Simplicity:**
- Remove unused parameters immediately
- `Option<T>` should only be used when value is truly optional
- Simplify request models as requirements change

**Backward Compatibility:**
- Accept optional fields for backward compatibility
- Reject required fields in breaking changes
- Version APIs when changing contracts

**Applied to Home Registry:**

**Breaking Change Strategy:**

Since Home Registry is "Work in Progress" (README line 7):
- ✅ Safe to make breaking changes to API
- ✅ No versioning needed (pre-1.0 release)
- ✅ Remove `inventory_name` from `InitialSetupRequest`

**Backend Code to Remove:**

1. **Auth endpoint logic** (`src/api/auth.rs` lines 238-257):
   - Remove optional inventory creation code
   - Keep sample data assignment removal
   - Simplify setup flow

2. **Database service** (`src/db/mod.rs` lines 106-122):
   - Remove `assign_sample_inventories_to_user()` method
   - No longer needed after sample data removal

### Source 7: PostgreSQL Migration Patterns

**Source:** PostgreSQL Wiki, Database Reliability Engineering  
**URL:** https://wiki.postgresql.org/wiki/Don%27t_Do_This  
**URL:** https://www.oreilly.com/library/view/database-reliability-engineering/9781491925935/

**Key Findings:**

**Idempotent Migrations:**
- Always use `IF EXISTS` / `IF NOT EXISTS`
- Design migrations to be run multiple times safely
- Check for data existence before deletion

**Sequence Management:**
- Reset sequences after bulk deletes
- Use `setval()` with `COALESCE()` for safety
- Avoid gaps in sequences when possible

**Applied to Home Registry:**

**Migration 021 Design:**

```sql
-- Remove sample data (idempotent)
DELETE FROM items WHERE inventory_id IN (100, 101, 102, 103, 104);
DELETE FROM inventories WHERE id IN (100, 101, 102, 103, 104);

-- Reset sequences (safe even if no records exist)
SELECT setval('inventories_id_seq', 
              COALESCE((SELECT MAX(id) FROM inventories WHERE id < 100), 1), 
              true);
SELECT setval('items_id_seq', 
              COALESCE((SELECT MAX(id) FROM items WHERE inventory_id < 100), 1), 
              true);
```

**Safety Features:**
- Uses `IN` clause with specific IDs (no wildcards)
- `COALESCE()` prevents errors on empty tables
- Excludes sample data IDs from sequence reset
- Can run multiple times without errors

---

## Implementation Plan

### Phase 1: Remove Backend Sample Data Infrastructure

**Priority:** HIGH  
**Order:** Do first (backend changes enable frontend changes)

#### Step 1.1: Create New Migration to Remove Sample Data

**File:** `migrations/021_remove_sample_data.sql`

**Action:** CREATE new file

**Content:**
```sql
-- Remove sample inventories and items for production release
-- Sample data was useful for initial development but should not exist in production
-- Inventories 100-104 and their associated items are removed
-- This migration is idempotent and safe to run multiple times

-- Remove items belonging to sample inventories
DELETE FROM items WHERE inventory_id BETWEEN 100 AND 104;

-- Remove sample inventories
DELETE FROM inventories WHERE id BETWEEN 100 AND 104;

-- Reset sequences to highest real user data ID (excluding sample range)
-- COALESCE ensures we don't error on empty tables
SELECT setval(
    'inventories_id_seq', 
    GREATEST(
        COALESCE((SELECT MAX(id) FROM inventories WHERE id < 100), 1),
        COALESCE(currval('inventories_id_seq'), 1)
    ), 
    true
);

SELECT setval(
    'items_id_seq', 
    GREATEST(
        COALESCE((SELECT MAX(id) FROM items WHERE EXISTS (
            SELECT 1 FROM inventories WHERE items.inventory_id = inventories.id AND inventories.id < 100
        )), 1),
        COALESCE(currval('items_id_seq'), 1)
    ), 
    true
);

-- Log completion
DO $$ 
BEGIN
    RAISE NOTICE 'Migration 021: Sample data removed successfully';
END $$;
```

**Why:**
- Preserves migration history (don't delete 019/020)
- Idempotent (safe to run multiple times)
- Cleans existing deployments that have sample data
- New deployments won't have sample data after running all migrations

#### Step 1.2: Remove Sample Data Assignment Method

**File:** `src/db/mod.rs`

**Action:** DELETE lines 106-122

**Removed Code:**
```rust
/// Assign all inventories with NULL `user_id` to the specified user
/// Used during initial setup to assign sample data to first admin
/// Returns the number of inventories assigned
pub async fn assign_sample_inventories_to_user(
    &self,
    user_id: Uuid,
) -> Result<u64, Box<dyn std::error::Error>> {
    let client = self.pool.get().await?;

    let result = client
        .execute(
            "UPDATE inventories SET user_id = $1, updated_at = NOW() WHERE user_id IS NULL",
            &[&user_id],
        )
        .await?;

    Ok(result)
}
```

**Impact:** Method no longer called after Step 1.3

#### Step 1.3: Remove Sample Data Assignment from Auth Endpoint

**File:** `src/api/auth.rs`

**Action:** DELETE lines 223-236

**Removed Code:**
```rust
// Auto-assign sample inventories (with NULL user_id) to this first admin
match db_service.assign_sample_inventories_to_user(user.id).await {
    Ok(assigned_count) => {
        if assigned_count > 0 {
            info!(
                "Assigned {} sample inventories to first admin user: {}",
                assigned_count, user.username
            );
        }
    },
    Err(e) => {
        // Non-fatal: log warning but don't fail setup
        warn!("Failed to assign sample inventories: {}", e);
    },
}
```

**Impact:** No longer assigns sample data during setup

#### Step 1.4: Remove Optional Inventory Creation Logic

**File:** `src/api/auth.rs`

**Action:** DELETE lines 238-257

**Removed Code:**
```rust
// Optionally create first inventory
if let Some(inventory_name) = &req.inventory_name {
    if !inventory_name.is_empty() {
        let inventory_request = crate::models::CreateInventoryRequest {
            name: inventory_name.clone(),
            description: Some("Initial inventory created during setup".to_string()),
            location: None,
            image_url: None,
        };

        match db_service
            .create_inventory(inventory_request, user.id)
            .await
        {
            Ok(inventory) => {
                info!(
                    "Created initial inventory: {} (ID: {:?}) for user {}",
                    inventory.name, inventory.id, user.username
                );
            },
            Err(e) => {
                warn!("Failed to create initial inventory: {}", e);
            },
        }
    }
}
```

**Impact:** Setup endpoint no longer creates inventory

#### Step 1.5: Update Backend Data Model

**File:** `src/models/mod.rs`

**Action:** MODIFY lines 772-779

**Before:**
```rust
/// Request for initial admin setup (first run)
#[derive(Deserialize, Debug)]
pub struct InitialSetupRequest {
    pub username: String,
    pub full_name: String,
    pub password: String,
    pub inventory_name: Option<String>, // Optional first inventory name
}
```

**After:**
```rust
/// Request for initial admin setup (first run)
#[derive(Deserialize, Debug)]
pub struct InitialSetupRequest {
    pub username: String,
    pub full_name: String,
    pub password: String,
}
```

**Impact:** API model no longer accepts `inventory_name`

### Phase 2: Update Frontend Setup Wizard

**Priority:** HIGH  
**Order:** After Phase 1 (depends on backend API changes)

#### Step 2.1: Update TypeScript Data Model

**File:** `frontend/src/types/index.ts`

**Action:** MODIFY lines 260-265

**Before:**
```typescript
export interface InitialSetupRequest {
  username: string;
  full_name: string;
  password: string;
  inventory_name?: string;
}
```

**After:**
```typescript
export interface InitialSetupRequest {
  username: string;
  full_name: string;
  password: string;
}
```

**Impact:** Frontend type matches backend model

#### Step 2.2: Remove Step 3 and Renumber Step 4 to Step 3

**File:** `frontend/src/pages/SetupPage.tsx`

**Major Changes:**

**A. Update Form State (Lines 16-22):**

**Before:**
```typescript
const [formData, setFormData] = useState({
  username: '',
  full_name: '',
  password: '',
  confirmPassword: '',
  inventory_name: '',
});
```

**After:**
```typescript
const [formData, setFormData] = useState({
  username: '',
  full_name: '',
  password: '',
  confirmPassword: '',
});
```

**B. Update Navigation Logic (Lines 66-70):**

**Before:**
```typescript
const handleNext = () => {
  if (step === 1 && validateStep1()) {
    setStep(2);
  } else if (step === 2 && validateStep2()) {
    setStep(3);
  }
};
```

**After:**
```typescript
const handleNext = () => {
  if (step === 1 && validateStep1()) {
    setStep(2);
  } else if (step === 2 && validateStep2()) {
    // Step 2 validation passes, now call setup and move to recovery codes
    void completeSetup();
  }
};
```

**C. Remove Step 3 Submit Handler (Lines 75-79):**

**Before:**
```typescript
const handleStep3Submit = async () => {
  setIsLoading(true);
  setError(null);
  await completeSetup();
};
```

**After:**
```typescript
// REMOVE THIS FUNCTION - no longer needed
```

**D. Update API Call (Lines 92-97):**

**Before:**
```typescript
const result = await authApi.setup({
  username: formData.username,
  full_name: formData.full_name,
  password: formData.password,
  inventory_name: formData.inventory_name || undefined,
});
```

**After:**
```typescript
const result = await authApi.setup({
  username: formData.username,
  full_name: formData.full_name,
  password: formData.password,
});
```

**E. Update Complete Setup to Move to Step 3 (Recovery Codes):**

**Before:**
```typescript
if (codesResponse.success && codesResponse.data) {
  setRecoveryCodes(codesResponse.data.codes);
  setStep(4);  // Move to recovery codes
}
```

**After:**
```typescript
if (codesResponse.success && codesResponse.data) {
  setRecoveryCodes(codesResponse.data.codes);
  setStep(3);  // Move to recovery codes (was step 4)
}
```

**F. Update Progress Indicator (Lines 243-270):**

**Before:**
```tsx
<div className="setup-progress">
  <div className={`progress-step ${step >= 1 ? 'active' : ''} ${step > 1 ? 'completed' : ''}`}>
    <div className="step-number">1</div>
    <span>Account</span>
  </div>
  <div className="progress-line"></div>
  <div className={`progress-step ${step >= 2 ? 'active' : ''} ${step > 2 ? 'completed' : ''}`}>
    <div className="step-number">2</div>
    <span>Security</span>
  </div>
  <div className="progress-line"></div>
  <div className={`progress-step ${step >= 3 ? 'active' : ''} ${step > 3 ? 'completed' : ''}`}>
    <div className="step-number">3</div>
    <span>Inventory</span>
  </div>
  <div className="progress-line"></div>
  <div className={`progress-step ${step >= 4 ? 'active' : ''}`}>
    <div className="step-number">4</div>
    <span>Recovery</span>
  </div>
</div>
```

**After:**
```tsx
<div className="setup-progress">
  <div className={`progress-step ${step >= 1 ? 'active' : ''} ${step > 1 ? 'completed' : ''}`}>
    <div className="step-number">1</div>
    <span>Account</span>
  </div>
  <div className="progress-line"></div>
  <div className={`progress-step ${step >= 2 ? 'active' : ''} ${step > 2 ? 'completed' : ''}`}>
    <div className="step-number">2</div>
    <span>Security</span>
  </div>
  <div className="progress-line"></div>
  <div className={`progress-step ${step >= 3 ? 'active' : ''}`}>
    <div className="step-number">3</div>
    <span>Recovery</span>
  </div>
</div>
```

**G. Remove Step 3 Form Rendering (Lines 347-369):**

**Before:**
```tsx
{/* Step 3: First Inventory */}
{step === 3 && (
  <div className="auth-step">
    <h2>Create First Inventory</h2>
    <p className="step-description">
      Optionally create your first inventory to get started.
    </p>

    <div className="form-group">
      <label htmlFor="inventory_name">Inventory Name (Optional)</label>
      <input
        type="text"
        id="inventory_name"
        name="inventory_name"
        value={formData.inventory_name}
        onChange={handleInputChange}
        placeholder="e.g., Home, Office, Garage"
        autoFocus
      />
      <p className="form-hint">You can skip this and create inventories later.</p>
    </div>
  </div>
)}
```

**After:**
```tsx
// REMOVE THIS ENTIRE BLOCK
```

**H. Update Recovery Codes Condition (Line 372):**

**Before:**
```tsx
{/* Step 4: Recovery Codes */}
{step === 4 && recoveryCodes && (
```

**After:**
```tsx
{/* Step 3: Recovery Codes */}
{step === 3 && recoveryCodes && (
```

**I. Update Button Logic (Lines 427-445):**

**Before:**
```tsx
<div className="auth-actions">
  {step > 1 && step < 4 && (
    <button type="button" className="btn-secondary" onClick={handleBack}>
      Back
    </button>
  )}

  {step < 3 ? (
    <button type="button" className="btn-primary" onClick={handleNext}>
      Next
    </button>
  ) : step === 3 ? (
    <button
      type="button"
      className="btn-primary"
      onClick={handleStep3Submit}
      disabled={isLoading}
    >
      {isLoading ? 'Creating Account...' : 'Complete Setup'}
    </button>
  ) : (
    <button
      type="button"
      className="btn-primary"
      onClick={handleCompleteSetup}
      disabled={isLoading || !codesConfirmed}
    >
      {isLoading ? 'Completing...' : 'Done'}
    </button>
  )}
</div>
```

**After:**
```tsx
<div className="auth-actions">
  {step > 1 && step < 3 && (
    <button type="button" className="btn-secondary" onClick={handleBack}>
      Back
    </button>
  )}

  {step < 2 ? (
    <button type="button" className="btn-primary" onClick={handleNext}>
      Next
    </button>
  ) : step === 2 ? (
    <button
      type="button"
      className="btn-primary"
      onClick={handleNext}
      disabled={isLoading}
    >
      {isLoading ? 'Creating Account...' : 'Complete Setup'}
    </button>
  ) : (
    <button
      type="button"
      className="btn-primary"
      onClick={handleCompleteSetup}
      disabled={isLoading || !codesConfirmed}
    >
      {isLoading ? 'Completing...' : 'Done'}
    </button>
  )}
</div>
```

**Key Changes:**
- Back button: Shows for step 2 only (not step 3)
- Next button: Shows for step 1 only
- Complete Setup button: Shows for step 2
- Done button: Shows for step 3 (recovery codes)

#### Step 2.3: Update CSS for 3-Step Progress Indicator

**File:** `frontend/src/styles/auth.css`

**Action:** MODIFY progress indicator grid

**Before:**
```css
.setup-progress {
  display: grid;
  grid-template-columns: repeat(7, 1fr); /* 4 steps + 3 progress lines */
  gap: 0.75rem;
  align-items: center;
  margin-bottom: 2rem;
}
```

**After:**
```css
.setup-progress {
  display: grid;
  grid-template-columns: repeat(5, 1fr); /* 3 steps + 2 progress lines */
  gap: 0.75rem;
  align-items: center;
  margin-bottom: 2rem;
}
```

**Impact:** Grid now accommodates 3 steps instead of 4

### Phase 3: Remove Scripts and Update Documentation

**Priority:** MEDIUM  
**Order:** After Phases 1-2 (cleanup)

#### Step 3.1: Remove Sample Data Assignment Script

**File:** `assign_sample_data.ps1`

**Action:** DELETE entire file (42 lines)

**Rationale:**
- No longer needed (sample data doesn't exist)
- Referenced in TESTING_REPORTS.md (will be updated)

#### Step 3.2: Archive or Remove TESTING_REPORTS.md

**File:** `TESTING_REPORTS.md`

**Action:** DELETE entire file

**Rationale:**
- Document is specific to sample data testing
- Users can create their own test data
- No longer relevant after sample data removal

**Alternative:** Move to `docs/archive/` if historical reference needed

#### Step 3.3: Update README.md

**File:** `README.md`

**Action:** UPDATE Quick Start section

**Before (Lines 44-51):**
```markdown
**First Run:**
- Complete the setup wizard
- Create your admin account
- Start adding your inventory
```

**After:**
```markdown
**First Run:**
- Complete the 3-step setup wizard
  1. Create your admin account
  2. Set a secure password
  3. Save your recovery codes
- Create your first inventory from the main page
- Start adding items to track
```

**Additional Changes:**

Add new section after "Features" (around line 28):

```markdown
## Getting Started

### First-Time Setup

When you first access Home Registry, you'll complete a simple 3-step wizard:

1. **Create Admin Account** - Choose your username and full name
2. **Set Password** - Create a secure password (min 8 characters)
3. **Save Recovery Codes** - Download 8 recovery codes for account recovery

After setup, you'll land on the empty dashboard. Click "Create Inventory" to add your first collection (e.g., "Home", "Office", "Garage").

### Creating Your First Inventory

1. Click **"Create Inventory"** on the main page
2. Enter a name (e.g., "Main House", "Garage")
3. Optionally add a description and location
4. Click **"Create Inventory"**
5. Start adding items to your inventory
```

**Impact:** Clear guidance for new users without sample data references

### Phase 4: Testing and Validation

**Priority:** HIGH  
**Order:** After Phases 1-3 (verification)

#### Step 4.1: Local Testing

**Test 1: Fresh Database Setup**
```powershell
# Stop and remove all data
docker compose down -v

# Start fresh
docker compose up -d

# Wait for startup
Start-Sleep -Seconds 10

# Open browser
Start-Process "http://localhost:8210"
```

**Expected Behavior:**
- Setup wizard shows 3 steps (not 4)
- Step 1: Account details
- Step 2: Password
- Step 3: Recovery codes (after clicking "Complete Setup" on step 2)
- No "Create First Inventory" step
- After completion, redirects to empty dashboard
- No sample inventories visible

**Test 2: Migration Verification**
```powershell
# Check that migration 021 ran
docker compose exec db psql -U postgres -d home_inventory -c "SELECT * FROM inventories WHERE id BETWEEN 100 AND 104;"
# Expected: 0 rows

# Check user can create inventory
# Use browser to create "Test Inventory"

# Verify ID is not in sample range
docker compose exec db psql -U postgres -d home_inventory -c "SELECT id, name, user_id FROM inventories;"
# Expected: User inventory with proper user_id, ID not 100-104
```

**Test 3: Existing Database Upgrade**
```powershell
# Simulate existing deployment with sample data
docker compose down -v
docker compose up -d
# Setup first admin (creates sample data via migrations 019/020)

# Now apply migration 021 (automatically via migration system)
docker compose restart app

# Verify sample data removed
docker compose exec db psql -U postgres -d home_inventory -c "SELECT COUNT(*) FROM inventories WHERE id BETWEEN 100 AND 104;"
# Expected: 0
```

#### Step 4.2: Frontend Build Testing

```powershell
cd frontend
npm run build
```

**Expected:** No TypeScript errors related to:
- `inventory_name` property
- `InitialSetupRequest` interface
- SetupPage component

#### Step 4.3: Backend Build Testing

```powershell
cargo build --release
cargo clippy -- -D warnings
cargo test
```

**Expected:**
- No compiler warnings about unused `inventory_name` field
- No clippy warnings
- All existing tests pass

---

## Files to Modify

### Backend (Rust)

| File | Action | Lines | Description |
|------|--------|-------|-------------|
| `migrations/021_remove_sample_data.sql` | **CREATE** | N/A | New migration to delete sample inventories/items |
| `src/db/mod.rs` | **DELETE** | 106-122 | Remove `assign_sample_inventories_to_user()` method |
| `src/api/auth.rs` | **DELETE** | 223-236 | Remove sample data assignment call |
| `src/api/auth.rs` | **DELETE** | 238-257 | Remove optional inventory creation logic |
| `src/models/mod.rs` | **MODIFY** | 772-779 | Remove `inventory_name` field from `InitialSetupRequest` |

**Total Backend Changes:** 1 new file, 4 modifications

### Frontend (TypeScript/React)

| File | Action | Lines | Description |
|------|--------|-------|-------------|
| `frontend/src/types/index.ts` | **MODIFY** | 260-265 | Remove `inventory_name` from `InitialSetupRequest` |
| `frontend/src/pages/SetupPage.tsx` | **MODIFY** | Multiple | Remove step 3, renumber step 4 → 3, update all logic |
| `frontend/src/styles/auth.css` | **MODIFY** | ~245 | Update grid columns from 7 to 5 (3 steps + 2 lines) |

**Total Frontend Changes:** 3 modifications

**Detailed SetupPage.tsx Changes:**
- Line 18: Remove `inventory_name` from form state
- Lines 66-70: Update `handleNext()` to call `completeSetup()` after step 2
- Lines 75-79: Delete `handleStep3Submit()` handler
- Lines 92-97: Remove `inventory_name` from API call
- Line 108: Change `setStep(4)` to `setStep(3)`
- Lines 243-270: Update progress indicator (remove step 3, keep 3 steps)
- Lines 347-369: Delete step 3 form rendering
- Line 372: Change `step === 4` to `step === 3`
- Lines 427-445: Update button logic for 3 steps

### Scripts

| File | Action | Description |
|------|--------|-------------|
| `assign_sample_data.ps1` | **DELETE** | Sample data assignment script (no longer needed) |

**Total Script Changes:** 1 deletion

### Documentation

| File | Action | Lines | Description |
|------|--------|-------|-------------|
| `TESTING_REPORTS.md` | **DELETE** | All | Document specific to sample data testing |
| `README.md` | **MODIFY** | 44-51 + new section | Update Quick Start, add First-Time Setup section |

**Total Documentation Changes:** 1 deletion, 1 modification

---

## Database Migration Strategy

### Migration Files Status

#### ✅ Keep Existing Migrations (DO NOT DELETE)

**File:** `migrations/019_add_sample_inventory_data.sql`  
**Reason:** Historical record, needed for deployments that haven't run it yet

**File:** `migrations/020_assign_sample_data_to_first_admin.sql`  
**Reason:** Historical record, pairs with migration 019

#### ✅ Create New Migration

**File:** `migrations/021_remove_sample_data.sql`  
**Purpose:** Clean up sample data for production release

**Content:**
```sql
-- Remove sample inventories and items for production release
-- Sample data was useful for initial development but should not exist in production
-- Inventories 100-104 and their associated items are removed
-- This migration is idempotent and safe to run multiple times

BEGIN;

-- Remove items belonging to sample inventories
DELETE FROM items WHERE inventory_id BETWEEN 100 AND 104;

-- Remove sample inventories
DELETE FROM inventories WHERE id BETWEEN 100 AND 104;

-- Reset sequences to highest real user data ID (excluding sample range)
-- GREATEST ensures we never go backwards, COALESCE handles empty tables
SELECT setval(
    'inventories_id_seq', 
    GREATEST(
        COALESCE((SELECT MAX(id) FROM inventories WHERE id < 100 OR id > 104), 1),
        COALESCE(currval('inventories_id_seq'), 1)
    ), 
    true
);

SELECT setval(
    'items_id_seq', 
    GREATEST(
        COALESCE((SELECT MAX(id) FROM items WHERE inventory_id NOT BETWEEN 100 AND 104), 1),
        COALESCE(currval('items_id_seq'), 1)
    ), 
    true
);

COMMIT;

-- Log completion
DO $$ 
BEGIN
    RAISE NOTICE 'Migration 021: Sample data removed successfully';
END $$;
```

**Features:**
- ✅ Transaction wrapper for atomicity
- ✅ Idempotent (safe to run multiple times)
- ✅ Specific ID ranges (no wildcards)
- ✅ Sequence reset with safety checks
- ✅ Logs completion for audit trail

### Deployment Scenarios

#### Scenario 1: Fresh Deployment (No Existing Data)

**Migration Sequence:**
1. Run migrations 001-018 (schema setup)
2. Run migration 019 (creates sample data)
3. Run migration 020 (assigns sample data to NULL)
4. ✅ **Run migration 021 (immediately removes sample data)**

**Result:** Clean database, no sample data, ready for production

#### Scenario 2: Existing Deployment (Has Sample Data)

**Migration Sequence:**
1. Already ran migrations 001-020
2. Sample data exists (inventories 100-104 assigned to first admin)
3. ✅ **Run migration 021 (removes sample data)**

**Result:** Existing deployment cleaned, sample data removed

#### Scenario 3: Existing Deployment (Sample Data Already Deleted)

**Migration Sequence:**
1. Already ran migrations 001-020
2. Admin manually deleted sample inventories
3. ✅ **Run migration 021 (no-op, idempotent)**

**Result:** No errors, migration completes successfully

### Rollback Strategy

**Question:** Should we create a rollback migration to restore sample data?

**Answer:** ❌ **NO**

**Rationale:**
- Sample data is development/testing data, not production data
- If needed, developers can re-run migrations 019-020 locally
- Production systems should never restore sample data
- Complexity not worth the maintenance burden

**Alternative:**
```powershell
# For local testing, developers can reset entire database:
docker compose down -v
docker compose up -d
# This re-runs all migrations including 019, 020, and 021
```

---

## Dependencies & Requirements

### Build Dependencies

**Backend:**
- Rust 1.75.0+ (current MSRV)
- Actix-Web 4.x
- tokio for async runtime
- PostgreSQL client libraries

**Frontend:**
- Node.js 18+ / npm 9+
- TypeScript 5.x
- React 18.x
- Vite 5.x

**Database:**
- PostgreSQL 16+

### Runtime Dependencies

**None** - This change only removes code, no new dependencies

### Breaking Changes

**API Contract:**
- ✅ Breaking change: `InitialSetupRequest` removes `inventory_name` field
- ✅ Acceptable: Project is "Work in Progress" (no versioning commitment)

**Database:**
- ✅ Migration 021 deletes data
- ✅ Safe: Only affects known sample data IDs (100-104)
- ✅ Idempotent: Can run multiple times

**User Impact:**
- ✅ Setup wizard changes from 4 steps to 3 steps
- ✅ Users must create inventory manually after setup
- ✅ No sample inventories for new admins

---

## Risk Analysis & Mitigations

### Risk 1: Breaking Existing Deployments

**Risk Level:** 🟡 MEDIUM

**Description:**
Existing deployments in mid-setup may fail if wizard step changes while user is on step 3.

**Likelihood:** LOW (rare to be mid-setup during deployment)

**Impact:** MEDIUM (user must start setup over)

**Mitigation:**
- Deploy during low-usage hours
- Clear browser cache after deployment
- Update cache headers to prevent old JavaScript from running

**Code:**
```rust
// In main.rs or static file handler
HttpResponse::Ok()
    .insert_header(("Cache-Control", "no-cache, no-store, must-revalidate"))
    .insert_header(("Pragma", "no-cache"))
    .insert_header(("Expires", "0"))
    .body(content)
```

### Risk 2: Migration 021 Fails on Read-Only Databases

**Risk Level:** 🟢 LOW

**Description:**
Some deployments may use read-only replicas or have insufficient permissions.

**Likelihood:** VERY LOW (migrations require write access)

**Impact:** LOW (migration fails, app doesn't start)

**Mitigation:**
- Migration system already handles failures gracefully
- Use transaction wrapper (BEGIN/COMMIT)
- Log errors clearly

**Code:**
```sql
-- Already implemented in migration 021
BEGIN;
-- ... operations ...
COMMIT;
```

### Risk 3: Users Confused by Missing Sample Data

**Risk Level:** 🟢 LOW

**Description:**
New users may expect sample inventories to demonstrate features.

**Likelihood:** MEDIUM (users may not know how to start)

**Impact:** LOW (user creates inventory manually)

**Mitigation:**
- Update README.md with clear first-time setup instructions
- Add "Getting Started" section
- Consider empty state message: "No inventories yet. Create your first one!"

**Code:**
```tsx
// In InventoriesPage.tsx (already exists)
{inventories.length === 0 && (
  <EmptyState
    icon="fas fa-warehouse"
    title="No Inventories"
    text="Create your first inventory to start tracking your items."
    action={
      <button className="btn btn-primary" onClick={() => setShowCreateModal(true)}>
        <i className="fas fa-plus"></i>
        Create Inventory
      </button>
    }
  />
)}
```

### Risk 4: TypeScript Build Errors

**Risk Level:** 🟡 MEDIUM

**Description:**
Removing `inventory_name` from types may break existing code that references it.

**Likelihood:** LOW (only used in SetupPage)

**Impact:** HIGH (frontend won't build)

**Mitigation:**
- Run `npm run build` before committing
- Search codebase for all references to `inventory_name`:
  ```powershell
  cd frontend
  grep -r "inventory_name" src/
  ```
- Ensure all references are in SetupPage.tsx (and removed)

**Verification:**
```powershell
cd frontend
npm run build
# Expected: No errors
```

### Risk 5: Rust Compile Errors

**Risk Level:** 🟡 MEDIUM

**Description:**
Removing code may leave orphaned imports or unused dependencies.

**Likelihood:** LOW (Rust compiler catches this)

**Impact:** HIGH (backend won't build)

**Mitigation:**
- Run `cargo build` after each change
- Run `cargo clippy` to catch warnings
- Run `cargo test` to ensure tests pass

**Verification:**
```powershell
cargo build --release
cargo clippy -- -D warnings
cargo test
```

### Risk 6: Sequence Gaps After Sample Data Removal

**Risk Level:** 🟢 LOW

**Description:**
Removing inventories 100-104 may leave a gap in ID sequence.

**Likelihood:** CERTAIN (by design)

**Impact:** NONE (gaps in sequences are normal and acceptable)

**Mitigation:**
- No action needed; gaps are fine
- Migration 021 resets sequence to highest real ID
- New inventories will use next available ID

**PostgreSQL Behavior:**
- Sequences can have gaps (e.g., 1, 2, 50, 51...)
- Gaps occur from rollbacks, deletions, or manual resets
- Applications should never rely on sequential IDs

### Risk 7: Lost Historical Data

**Risk Level:** 🟡 MEDIUM

**Description:**
Deleting migrations 019/020 would lose historical record of why sample data existed.

**Likelihood:** NONE (we're NOT deleting migrations)

**Impact:** N/A (migrations preserved)

**Mitigation:**
- ✅ Keep migrations 019 and 020 in repository
- ✅ Add comments explaining they are superseded by 021
- ✅ Use migration 021 to remove data, not delete old migrations

---

## Testing Recommendations

### Unit Tests

**Backend:**

**File:** `tests/test_auth.rs`

**New Test:**
```rust
#[actix_web::test]
async fn test_initial_setup_without_inventory_name() {
    let pool = setup_test_db().await;
    let db_service = DatabaseService::new(pool.clone());

    // Create initial admin
    let setup_request = InitialSetupRequest {
        username: "admin".to_string(),
        full_name: "Test Admin".to_string(),
        password: "SecurePass123!".to_string(),
        // NO inventory_name field
    };

    let result = initial_setup(
        web::Data::new(pool),
        web::Json(setup_request)
    ).await;

    assert!(result.is_ok());
    
    // Verify no inventories created
    let inventories = db_service.get_all_inventories_for_user(user_id).await.unwrap();
    assert_eq!(inventories.len(), 0);
}
```

**File:** `tests/test_db.rs`

**Remove Test:**
```rust
// DELETE THIS TEST (method no longer exists)
#[tokio::test]
async fn test_assign_sample_inventories() {
    // ...
}
```

**Frontend:**

**File:** `frontend/src/pages/SetupPage.test.tsx` (if exists)

**Update Tests:**
```typescript
describe('SetupPage', () => {
  it('should have 3 steps', () => {
    render(<SetupPage />);
    const steps = screen.getAllByRole('presentation'); // progress steps
    expect(steps).toHaveLength(3);
  });

  it('should not show inventory creation step', () => {
    render(<SetupPage />);
    expect(screen.queryByLabelText(/inventory name/i)).not.toBeInTheDocument();
  });

  it('should submit setup without inventory_name', async () => {
    const mockSetup = jest.fn().mockResolvedValue({ success: true });
    jest.spyOn(authApi, 'setup').mockImplementation(mockSetup);

    render(<SetupPage />);
    
    // Fill step 1
    fireEvent.change(screen.getByLabelText(/full name/i), { target: { value: 'Test User' } });
    fireEvent.change(screen.getByLabelText(/username/i), { target: { value: 'testuser' } });
    fireEvent.click(screen.getByText(/next/i));

    // Fill step 2
    fireEvent.change(screen.getByLabelText(/^password/i), { target: { value: 'SecurePass123!' } });
    fireEvent.change(screen.getByLabelText(/confirm password/i), { target: { value: 'SecurePass123!' } });
    fireEvent.click(screen.getByText(/complete setup/i));

    await waitFor(() => {
      expect(mockSetup).toHaveBeenCalledWith({
        username: 'testuser',
        full_name: 'Test User',
        password: 'SecurePass123!',
        // NO inventory_name
      });
    });
  });
});
```

### Integration Tests

**Test 1: Fresh Setup Flow**

```powershell
# Prerequisites
docker compose down -v
docker compose up -d
Start-Sleep -Seconds 10

# Test Steps
1. Open http://localhost:8210
2. Verify setup wizard shows 3 steps (not 4)
3. Complete step 1 (account details)
4. Complete step 2 (password)
5. Verify step 3 shows recovery codes (not inventory creation)
6. Download recovery codes
7. Check confirmation checkbox
8. Click "Done"
9. Verify redirect to main page
10. Verify no inventories exist (empty state shown)
11. Click "Create Inventory"
12. Create inventory named "Test Inventory"
13. Verify inventory appears on dashboard

# Expected Results
✅ 3-step wizard (not 4)
✅ No sample inventories
✅ User can create inventory after setup
✅ Inventory has proper user_id (not NULL)
```

**Test 2: Migration 021 Cleanup**

```powershell
# Prerequisites
docker compose down -v
docker compose up -d
Start-Sleep -Seconds 10

# Create first admin (triggers migrations 019-020)
curl -X POST http://localhost:8210/api/auth/setup `
  -H "Content-Type: application/json" `
  -d '{"username":"admin","full_name":"Admin User","password":"SecurePass123!"}' 

# Check sample data exists
docker compose exec db psql -U postgres -d home_inventory -c "SELECT COUNT(*) FROM inventories WHERE id BETWEEN 100 AND 104;"
# Expected: 5 rows (before migration 021)

# Restart app (triggers migration 021)
docker compose restart app
Start-Sleep -Seconds 5

# Check sample data removed
docker compose exec db psql -U postgres -d home_inventory -c "SELECT COUNT(*) FROM inventories WHERE id BETWEEN 100 AND 104;"
# Expected: 0 rows (after migration 021)

# Verify sequences reset
docker compose exec db psql -U postgres -d home_inventory -c "SELECT currval('inventories_id_seq');"
# Expected: Should not be in 100-104 range
```

**Test 3: Idempotency Verification**

```powershell
# Run migration 021 multiple times
docker compose exec db psql -U postgres -d home_inventory < migrations/021_remove_sample_data.sql
docker compose exec db psql -U postgres -d home_inventory < migrations/021_remove_sample_data.sql
docker compose exec db psql -U postgres -d home_inventory < migrations/021_remove_sample_data.sql

# Expected: No errors, completes successfully each time
```

### Manual Testing Checklist

- [ ] Fresh database setup shows 3-step wizard
- [ ] Step 1: Account details form works
- [ ] Step 2: Password form works
- [ ] Step 3: Recovery codes display correctly
- [ ] No "Create First Inventory" step appears
- [ ] Progress indicator shows 3 steps (not 4)
- [ ] Back button works (step 2 → step 1)
- [ ] Complete Setup button on step 2 creates account
- [ ] Recovery codes can be downloaded
- [ ] Recovery codes can be copied
- [ ] Recovery codes can be printed
- [ ] Confirmation checkbox required before "Done"
- [ ] Done button redirects to main page
- [ ] Main page shows empty state (no inventories)
- [ ] User can create inventory from empty state
- [ ] Created inventory has proper user_id (not NULL)
- [ ] No sample inventories appear (100-104)
- [ ] Database has no inventories with IDs 100-104
- [ ] Frontend builds without TypeScript errors
- [ ] Backend builds without Rust warnings
- [ ] All tests pass

### Performance Testing

**Not Applicable** - This change removes code and data, no performance impact

### Security Testing

**Verification:**
- [ ] New users cannot access other users' data
- [ ] No exposed data from deleted sample inventories
- [ ] Recovery codes still work correctly
- [ ] Authentication flow unchanged

---

## Summary

This specification provides a comprehensive plan to:

1. **Remove Step 3** ("Create First Inventory") from the setup wizard
2. **Remove all sample data** infrastructure (migrations, backend, scripts)
3. **Simplify onboarding** to 3 clear steps (Account → Security → Recovery)
4. **Clean production databases** by removing development/testing data

**Implementation is safe and straightforward:**
- ✅ Preserves migration history (no deletions of 019/020)
- ✅ Creates new migration (021) to remove sample data
- ✅ All changes are backward-compatible with existing deployments
- ✅ Clear testing plan ensures reliability
- ✅ Documentation updated for new users

**Next Steps:**
1. Review this specification
2. Create implementation plan with task breakdown
3. Implement Phase 1 (backend changes)
4. Implement Phase 2 (frontend changes)
5. Implement Phase 3 (cleanup)
6. Execute Phase 4 (testing)
7. Deploy to production

---

**Document Version:** 1.0  
**Prepared By:** Research Subagent  
**Review Status:** Ready for Implementation

