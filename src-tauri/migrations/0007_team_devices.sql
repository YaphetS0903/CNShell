ALTER TABLE team_workspaces ADD COLUMN local_device_id TEXT;

CREATE TABLE IF NOT EXISTS team_devices (
    id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES team_workspaces(id) ON DELETE CASCADE,
    member_id TEXT NOT NULL REFERENCES team_members(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    encryption_public_key TEXT NOT NULL,
    signing_public_key TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    is_local INTEGER NOT NULL DEFAULT 0 CHECK (is_local IN (0,1)),
    status TEXT NOT NULL CHECK (status IN ('active','revoked')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    revoked_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_team_devices_workspace_member
ON team_devices(workspace_id, member_id, status);

CREATE UNIQUE INDEX IF NOT EXISTS idx_team_devices_one_active_local
ON team_devices(workspace_id)
WHERE is_local = 1 AND status = 'active';
