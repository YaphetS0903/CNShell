CREATE TABLE IF NOT EXISTS mcp_approval_rules (
  id TEXT PRIMARY KEY NOT NULL,
  client_id TEXT NOT NULL REFERENCES mcp_clients(id) ON DELETE CASCADE,
  connection_id TEXT NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
  tool TEXT NOT NULL,
  target_key TEXT NOT NULL,
  created_at TEXT NOT NULL,
  last_used_at TEXT,
  UNIQUE(client_id, connection_id, tool, target_key)
);

CREATE INDEX IF NOT EXISTS idx_mcp_approval_rules_lookup
ON mcp_approval_rules(client_id, connection_id, tool, target_key);
