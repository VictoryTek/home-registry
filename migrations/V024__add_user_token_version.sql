-- Per-user session generation counter. Every issued JWT carries the user's current value
-- in its `tv` claim; bumping it (password change/reset, role or active-flag change, TOTP
-- enable/disable, "sign out all other devices") invalidates all previously issued tokens.
-- Existing users start at 0, which is also what tokens issued before this migration
-- (without a `tv` claim) deserialize to, so existing sessions keep working.
ALTER TABLE users ADD COLUMN IF NOT EXISTS token_version INTEGER NOT NULL DEFAULT 0;
