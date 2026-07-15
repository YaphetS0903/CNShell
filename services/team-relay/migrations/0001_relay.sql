PRAGMA foreign_keys = ON;

CREATE TABLE accounts (
    id TEXT PRIMARY KEY NOT NULL,
    email TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active','disabled')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE account_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    revoked_at TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    key_epoch INTEGER NOT NULL DEFAULT 1 CHECK (key_epoch >= 1),
    status TEXT NOT NULL CHECK (status IN ('active','deleted')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE members (
    id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('owner','admin','operator','viewer')),
    status TEXT NOT NULL CHECK (status IN ('active','removed')),
    joined_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    removed_at TEXT,
    UNIQUE(workspace_id, account_id)
);

CREATE TABLE devices (
    id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    member_id TEXT NOT NULL REFERENCES members(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    encryption_public_key TEXT NOT NULL,
    signing_public_key TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active','revoked')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    revoked_at TEXT
);

CREATE INDEX idx_devices_workspace_member ON devices(workspace_id, member_id, status);

CREATE TABLE device_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    device_id TEXT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    revoked_at TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE device_challenges (
    id TEXT PRIMARY KEY NOT NULL,
    device_id TEXT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    challenge_hash TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE workspace_invitations (
    id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    invited_by_member_id TEXT NOT NULL REFERENCES members(id),
    member_id TEXT NOT NULL,
    email TEXT NOT NULL COLLATE NOCASE,
    role TEXT NOT NULL CHECK (role IN ('admin','operator','viewer')),
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    accepted_at TEXT,
    revoked_at TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE terminal_rooms (
    id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    host_member_id TEXT NOT NULL REFERENCES members(id),
    host_device_id TEXT NOT NULL REFERENCES devices(id),
    key_epoch INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active','closed')),
    next_output_sequence INTEGER NOT NULL DEFAULT 1,
    lease_generation INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    closed_at TEXT
);

CREATE TABLE room_invitations (
    room_id TEXT NOT NULL REFERENCES terminal_rooms(id) ON DELETE CASCADE,
    recipient_device_id TEXT NOT NULL REFERENCES devices(id),
    envelope_json TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    accepted_at TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY(room_id, recipient_device_id)
);

CREATE TABLE room_participants (
    room_id TEXT NOT NULL REFERENCES terminal_rooms(id) ON DELETE CASCADE,
    member_id TEXT NOT NULL REFERENCES members(id),
    device_id TEXT NOT NULL REFERENCES devices(id),
    next_input_sequence INTEGER NOT NULL DEFAULT 1,
    joined_at TEXT NOT NULL,
    removed_at TEXT,
    PRIMARY KEY(room_id, device_id)
);

CREATE TABLE room_control_leases (
    room_id TEXT PRIMARY KEY NOT NULL REFERENCES terminal_rooms(id) ON DELETE CASCADE,
    lease_id TEXT NOT NULL UNIQUE,
    member_id TEXT NOT NULL REFERENCES members(id),
    device_id TEXT NOT NULL REFERENCES devices(id),
    generation INTEGER NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE relay_frames (
    room_id TEXT NOT NULL REFERENCES terminal_rooms(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL,
    envelope_json TEXT NOT NULL,
    encoded_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY(room_id, sequence)
);

CREATE INDEX idx_relay_frames_room_time ON relay_frames(room_id, created_at, sequence);

CREATE TABLE relay_audit_events (
    id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    actor_member_id TEXT NOT NULL REFERENCES members(id),
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_relay_audit_workspace_time
ON relay_audit_events(workspace_id, created_at DESC);
