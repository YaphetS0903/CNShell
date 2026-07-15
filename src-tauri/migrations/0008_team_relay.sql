CREATE TABLE IF NOT EXISTS team_relay_profiles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    account_id TEXT,
    account_email TEXT,
    account_session_expires_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS team_relay_bindings (
    workspace_id TEXT PRIMARY KEY NOT NULL REFERENCES team_workspaces(id) ON DELETE CASCADE,
    profile_id TEXT NOT NULL REFERENCES team_relay_profiles(id) ON DELETE RESTRICT,
    account_id TEXT NOT NULL,
    device_session_expires_at TEXT,
    last_synced_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_team_relay_bindings_profile
ON team_relay_bindings(profile_id, workspace_id);

CREATE TABLE IF NOT EXISTS team_relay_pending_acceptances (
    token_hash TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL REFERENCES team_relay_profiles(id) ON DELETE CASCADE,
    device_id TEXT NOT NULL UNIQUE,
    device_name TEXT NOT NULL,
    encryption_public_key TEXT NOT NULL,
    signing_public_key TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    created_at TEXT NOT NULL
);
