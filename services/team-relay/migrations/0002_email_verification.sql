ALTER TABLE accounts ADD COLUMN email_verified_at TEXT;
ALTER TABLE accounts ADD COLUMN email_verification_sent_at TEXT;

UPDATE accounts
SET email_verified_at = created_at
WHERE email_verified_at IS NULL;

CREATE TABLE account_email_verifications (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    last_sent_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_account_email_verifications_account
ON account_email_verifications(account_id, created_at DESC);
