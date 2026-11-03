-- API keys (issue-and-revoke-keys): each with its label, the SHA-256 of its secret (never the
-- secret itself), its issuance and revocation times (RFC 3339 UTC), its quotas and its
-- rate-limit tier. A key is refused from its revocation on; its row stays, for its layers.
CREATE TABLE IF NOT EXISTS keys (
  id TEXT PRIMARY KEY,
  label TEXT NOT NULL,
  secret_sha256 TEXT NOT NULL UNIQUE,
  issued_at TEXT NOT NULL,
  revoked_at TEXT,
  layers_per_key INTEGER NOT NULL,
  items_per_layer INTEGER NOT NULL,
  bytes_per_item_version INTEGER NOT NULL,
  tier TEXT NOT NULL CHECK (tier IN ('basic', 'standard', 'extended'))
);
INSERT OR IGNORE INTO schema_version (version) VALUES (2);
