-- The univ-layers database: API keys, layers, items, item versions and change sequences, which
-- only Univ Server writes. Each migration records the schema version it leaves.
CREATE TABLE IF NOT EXISTS schema_version (
  version INTEGER PRIMARY KEY
);
INSERT OR IGNORE INTO schema_version (version) VALUES (1);
