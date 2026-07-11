PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS folders (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  parent_id TEXT REFERENCES folders(id),
  sort_order INTEGER NOT NULL DEFAULT 0,
  deleted_at TEXT
);

CREATE TABLE IF NOT EXISTS connections (
  id TEXT PRIMARY KEY NOT NULL,
  folder_id TEXT REFERENCES folders(id),
  protocol TEXT NOT NULL DEFAULT 'ssh',
  name TEXT NOT NULL,
  host TEXT NOT NULL,
  port INTEGER NOT NULL DEFAULT 22,
  username TEXT NOT NULL,
  auth_type TEXT NOT NULL DEFAULT 'password',
  credential_ref TEXT,
  private_key_path TEXT,
  host_key_policy TEXT NOT NULL DEFAULT 'strict',
  note TEXT NOT NULL DEFAULT '',
  tags TEXT NOT NULL DEFAULT '[]',
  encoding TEXT NOT NULL DEFAULT 'UTF-8',
  startup_command TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT
);

CREATE TABLE IF NOT EXISTS known_hosts (
  host TEXT NOT NULL,
  port INTEGER NOT NULL,
  algorithm TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY(host, port)
);

CREATE TABLE IF NOT EXISTS command_history (
  id TEXT PRIMARY KEY NOT NULL,
  connection_id TEXT REFERENCES connections(id),
  command TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS transfer_tasks (
  id TEXT PRIMARY KEY NOT NULL,
  session_id TEXT NOT NULL,
  direction TEXT NOT NULL,
  source TEXT NOT NULL,
  destination TEXT NOT NULL,
  total_bytes INTEGER NOT NULL DEFAULT 0,
  transferred_bytes INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL,
  conflict_policy TEXT NOT NULL DEFAULT 'ask',
  error TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL
);
