ALTER TABLE mcp_clients
ADD COLUMN show_hostnames INTEGER NOT NULL DEFAULT 0 CHECK (show_hostnames IN (0,1));
