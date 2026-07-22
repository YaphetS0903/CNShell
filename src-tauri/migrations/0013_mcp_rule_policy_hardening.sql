-- Exact command rules created by early MCP previews did not have the current
-- conservative low-risk policy. Invalidate them on upgrade rather than
-- retaining an opaque hash that cannot be safely reclassified.
DELETE FROM mcp_approval_rules;
