CREATE TABLE IF NOT EXISTS automation_runs (
  id TEXT PRIMARY KEY NOT NULL,
  plan_id TEXT NOT NULL,
  plan_name TEXT NOT NULL,
  connection_id TEXT NOT NULL,
  source TEXT NOT NULL,
  schedule_id TEXT,
  started_at TEXT NOT NULL,
  finished_at TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('completed', 'failed', 'cancelled')),
  results TEXT NOT NULL DEFAULT '[]',
  error TEXT
);

CREATE INDEX IF NOT EXISTS idx_automation_runs_started_at
ON automation_runs(started_at DESC);
