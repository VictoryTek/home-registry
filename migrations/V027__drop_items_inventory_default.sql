-- items.inventory_id had DEFAULT 1 since V001/V002/V003, so an INSERT that left it out silently
-- filed the item into inventory #1, which belongs to whoever created the first inventory.
-- The column is already NOT NULL (V002); without a default, an omitted value now fails.
-- Only future INSERTs are affected; existing rows are not changed.
ALTER TABLE items ALTER COLUMN inventory_id DROP DEFAULT;
