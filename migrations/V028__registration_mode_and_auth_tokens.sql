-- V028: registration mode and single-use invitation / password-reset tokens
--
-- app_settings holds server-wide settings that an admin changes at run time.
-- registration_mode is 'open' (anyone can register), 'invite' (a valid invitation is needed) or
-- 'closed' (nobody can register; admins can still create users). Every install, new or upgraded,
-- starts as 'invite': open registration with no off switch let strangers create accounts on an
-- internet-exposed instance. An upgrade that has users also leaves a notice for the admins, which
-- the application shows until it is dismissed and logs once at start-up.
--
-- auth_tokens holds the invitation and reset links an admin hands out. Only a SHA-256 hash of the
-- token is stored (the token is shown once, when it is created). A token is redeemed once: the
-- redeeming statement sets used_at in the same transaction that creates the user or changes the
-- password. revoked_at is set by an admin, by a newer reset link for the same user, and for every
-- unused token when a backup is restored.

CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT app_settings_registration_mode_valid
        CHECK (key <> 'registration_mode' OR value IN ('open', 'invite', 'closed'))
);

INSERT INTO app_settings (key, value)
VALUES ('registration_mode', 'invite')
ON CONFLICT (key) DO NOTHING;

-- An upgrade (users already exist) tells the admins that registration is no longer open
INSERT INTO app_settings (key, value)
SELECT 'registration_upgrade_notice', 'show'
WHERE EXISTS (SELECT 1 FROM users)
ON CONFLICT (key) DO NOTHING;

CREATE TABLE IF NOT EXISTS auth_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    purpose TEXT NOT NULL CHECK (purpose IN ('invite', 'password_reset')),
    token_hash TEXT NOT NULL UNIQUE,
    -- The user a reset link is for; an invitation is not for anyone yet
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    used_by UUID REFERENCES users(id) ON DELETE SET NULL,
    revoked_at TIMESTAMPTZ,
    CONSTRAINT auth_tokens_user_matches_purpose
        CHECK ((purpose = 'password_reset') = (user_id IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS idx_auth_tokens_purpose_expires ON auth_tokens (purpose, expires_at);
CREATE INDEX IF NOT EXISTS idx_auth_tokens_user ON auth_tokens (user_id) WHERE user_id IS NOT NULL;
