-- ============================================================================
-- Remove tables and a column that no release has ever written to:
--   categories, tags, item_tags, custom_fields, item_custom_values   (V005-V007)
--   password_reset_tokens                                            (V013)
--   items.category_id and its foreign key fk_items_category          (V005)
-- Organizers (V010/V011) replaced the first five; recovery codes and TOTP (V018/V023)
-- replaced password reset. items.category (a live text column) is NOT touched.
--
-- Safety net: at startup, before this migration runs, the application checks these tables and
-- items.category_id. If any holds data it writes a backup named
-- backups/home_registry_pre_migration_v026_<timestamp>_<id>.json and refuses to migrate if that
-- backup cannot be written. The notice below is an extra record in the database logs.
--
-- No CASCADE on purpose: if anything else depends on these objects the migration fails
-- loudly (the whole migration is one transaction, so nothing is half-applied).
-- ============================================================================
DO $$
DECLARE
    legacy_table text;
    row_count bigint;
BEGIN
    FOREACH legacy_table IN ARRAY ARRAY[
        'categories', 'tags', 'item_tags', 'custom_fields', 'item_custom_values',
        'password_reset_tokens'
    ] LOOP
        IF to_regclass('public.' || legacy_table) IS NOT NULL THEN
            EXECUTE format('SELECT count(*) FROM public.%I', legacy_table) INTO row_count;
            IF row_count > 0 THEN
                RAISE NOTICE 'V026: dropping % rows from %', row_count, legacy_table;
            END IF;
        END IF;
    END LOOP;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'items' AND column_name = 'category_id'
    ) THEN
        EXECUTE 'SELECT count(*) FROM public.items WHERE category_id IS NOT NULL' INTO row_count;
        IF row_count > 0 THEN
            RAISE NOTICE 'V026: dropping % non-null values of items.category_id', row_count;
        END IF;
    END IF;
END $$;

-- The foreign key and index on items must go before the table they point at
ALTER TABLE items DROP CONSTRAINT IF EXISTS fk_items_category;
DROP INDEX IF EXISTS idx_items_category_id;
ALTER TABLE items DROP COLUMN IF EXISTS category_id;

-- Dependent tables first, then the tables they reference
DROP TABLE IF EXISTS item_tags;
DROP TABLE IF EXISTS item_custom_values;
DROP TABLE IF EXISTS tags;
DROP TABLE IF EXISTS custom_fields;
DROP TABLE IF EXISTS categories;
DROP TABLE IF EXISTS password_reset_tokens;
