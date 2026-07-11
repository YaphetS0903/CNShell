CREATE TABLE IF NOT EXISTS proxies (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  type TEXT NOT NULL,
  host TEXT NOT NULL,
  port INTEGER NOT NULL,
  username TEXT,
  credential_ref TEXT,
  jump_connection_id TEXT REFERENCES connections(id),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

ALTER TABLE connections ADD COLUMN proxy_id TEXT REFERENCES proxies(id);

CREATE TABLE IF NOT EXISTS port_forwards (
  id TEXT PRIMARY KEY NOT NULL,
  connection_id TEXT NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
  type TEXT NOT NULL,
  bind_host TEXT NOT NULL,
  bind_port INTEGER NOT NULL,
  destination_host TEXT,
  destination_port INTEGER,
  auto_start INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS command_snippets (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  command TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  tags TEXT NOT NULL DEFAULT '[]',
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS workspace_state (
  key TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_connections_folder ON connections(folder_id);
CREATE INDEX IF NOT EXISTS idx_history_connection ON command_history(connection_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_forward_connection ON port_forwards(connection_id);
