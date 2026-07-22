CREATE TABLE IF NOT EXISTS mcp_clients (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('active','revoked')),
  executable_path TEXT,
  executable_sha256 TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  last_used_at TEXT,
  revoked_at TEXT
);

CREATE TABLE IF NOT EXISTS mcp_grants (
  id TEXT PRIMARY KEY NOT NULL,
  client_id TEXT NOT NULL REFERENCES mcp_clients(id) ON DELETE CASCADE,
  connection_id TEXT NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
  tool TEXT NOT NULL,
  remote_root TEXT NOT NULL DEFAULT '/',
  created_at TEXT NOT NULL,
  expires_at TEXT,
  UNIQUE(client_id, connection_id, tool, remote_root)
);

CREATE INDEX IF NOT EXISTS idx_mcp_grants_client
ON mcp_grants(client_id, connection_id, tool);

CREATE TABLE IF NOT EXISTS mcp_local_grants (
  id TEXT PRIMARY KEY NOT NULL,
  client_id TEXT NOT NULL REFERENCES mcp_clients(id) ON DELETE CASCADE,
  direction TEXT NOT NULL CHECK (direction IN ('upload','download')),
  display_name TEXT NOT NULL,
  path_hint TEXT NOT NULL DEFAULT '',
  persistent INTEGER NOT NULL DEFAULT 0 CHECK (persistent IN (0,1)),
  created_at TEXT NOT NULL,
  expires_at TEXT,
  revoked_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_mcp_local_grants_client
ON mcp_local_grants(client_id, direction, revoked_at);

CREATE TABLE IF NOT EXISTS mcp_audit_events (
  id TEXT PRIMARY KEY NOT NULL,
  request_id TEXT,
  client_id TEXT,
  connection_id TEXT,
  tool TEXT NOT NULL,
  target_summary TEXT NOT NULL DEFAULT '',
  risk TEXT NOT NULL CHECK (risk IN ('info','low','medium','high')),
  outcome TEXT NOT NULL,
  duration_ms INTEGER,
  transferred_bytes INTEGER,
  truncated INTEGER NOT NULL DEFAULT 0 CHECK (truncated IN (0,1)),
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mcp_audit_created
ON mcp_audit_events(created_at DESC);

INSERT INTO settings(key,value)
VALUES('mcp', '{"enabled":false,"showHostnames":false}')
ON CONFLICT(key) DO NOTHING;
