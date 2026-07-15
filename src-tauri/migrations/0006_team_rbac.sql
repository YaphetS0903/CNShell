CREATE TABLE IF NOT EXISTS team_workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    local_member_id TEXT NOT NULL,
    key_epoch INTEGER NOT NULL DEFAULT 1 CHECK (key_epoch >= 1),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS team_members (
    id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES team_workspaces(id) ON DELETE CASCADE,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('owner','admin','operator','viewer')),
    status TEXT NOT NULL CHECK (status IN ('active','removed')),
    joined_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    removed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_team_members_workspace
ON team_members(workspace_id, status, role);

CREATE TABLE IF NOT EXISTS team_audit_events (
    id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES team_workspaces(id) ON DELETE CASCADE,
    actor_member_id TEXT NOT NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_team_audit_workspace_time
ON team_audit_events(workspace_id, created_at DESC);
