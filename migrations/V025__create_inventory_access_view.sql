-- ============================================================================
-- inventory_access: THE SINGLE DEFINITION OF "WHO CAN ACCESS WHICH INVENTORY".
--
-- One row per way a user can reach an inventory:
--   owner       the user owns the inventory (inventories.user_id)
--   all_access  the owner granted the user All Access (user_access_grants)
--   share       the inventory was shared with the user (inventory_shares), with a level
-- `precedence` orders them (lower wins): owner, then all_access, then share. A user can have
-- several rows for one inventory (e.g. All Access plus a share, or a self-grant), so consumers
-- must use IN / EXISTS, or ORDER BY precedence LIMIT 1 when they need the winning row.
--
-- Rows with an unknown share level are not emitted, so every consumer denies them identically.
--
-- MAINTENANCE: any later migration that changes inventories, inventory_shares or
-- user_access_grants (renames, retypes, drops) must check whether this view needs DROP + CREATE;
-- CREATE OR REPLACE VIEW cannot drop or retype columns. Whoever changes who may access an
-- inventory (for example how deactivated owners are treated) should change it HERE only.
-- ============================================================================
CREATE OR REPLACE VIEW inventory_access AS
SELECT i.id AS inventory_id,
       i.user_id AS user_id,
       'owner'::text AS source,
       NULL::text AS permission_level,
       0::smallint AS precedence
FROM inventories i
WHERE i.user_id IS NOT NULL
UNION ALL
SELECT i.id,
       g.grantee_user_id,
       'all_access'::text,
       NULL::text,
       1::smallint
FROM inventories i
JOIN user_access_grants g ON g.grantor_user_id = i.user_id
UNION ALL
SELECT s.inventory_id,
       s.shared_with_user_id,
       'share'::text,
       s.permission_level::text,
       2::smallint
FROM inventory_shares s
WHERE s.permission_level IN ('view', 'edit_items', 'edit_inventory');
