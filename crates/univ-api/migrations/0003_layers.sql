-- Layers (create-and-configure-a-layer), their items and every version of each item
-- (publish-a-layer-item, revise-or-retract-an-item), and each layer's change sequence
-- (follow-a-layers-changes). Times are RFC 3339 UTC.
CREATE TABLE IF NOT EXISTS layers (
  id TEXT PRIMARY KEY,
  key_id TEXT NOT NULL REFERENCES keys (id),
  title TEXT NOT NULL,
  description TEXT NOT NULL,
  visibility TEXT NOT NULL CHECK (visibility IN ('public', 'private')),
  created_at TEXT NOT NULL,
  retired_at TEXT,
  -- The number of the layer's last change; 0 until its first.
  sequence INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS layers_by_key ON layers (key_id, retired_at);

-- Each item's current version; the item's id is the client's, unique within its layer.
CREATE TABLE IF NOT EXISTS items (
  layer_id TEXT NOT NULL REFERENCES layers (id),
  id TEXT NOT NULL,
  version INTEGER NOT NULL,
  retracted INTEGER NOT NULL DEFAULT 0 CHECK (retracted IN (0, 1)),
  PRIMARY KEY (layer_id, id)
) WITHOUT ROWID;

-- Every version of every item, never changed once stored. content is the version's JSON: the
-- item as published or revised, or the retraction with its reason.
CREATE TABLE IF NOT EXISTS item_versions (
  layer_id TEXT NOT NULL,
  item_id TEXT NOT NULL,
  version INTEGER NOT NULL,
  stored_at TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('published', 'update', 'correction', 'retraction')),
  content TEXT NOT NULL,
  PRIMARY KEY (layer_id, item_id, version),
  FOREIGN KEY (layer_id, item_id) REFERENCES items (layer_id, id)
) WITHOUT ROWID;

-- The time records of each item's current version on TT, in days from J2000.0, with their
-- uncertainty interval: what the time filter compares (filter-by-time). A retracted item has
-- none.
CREATE TABLE IF NOT EXISTS item_times (
  layer_id TEXT NOT NULL,
  item_id TEXT NOT NULL,
  record INTEGER NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('source_event', 'arrival', 'discovery')),
  at REAL NOT NULL,
  earliest REAL NOT NULL,
  latest REAL NOT NULL,
  PRIMARY KEY (layer_id, item_id, record),
  FOREIGN KEY (layer_id, item_id) REFERENCES items (layer_id, id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS item_times_by_kind ON item_times (kind, earliest, latest);

-- Each layer's changes, numbered from 1 in the order they were made.
CREATE TABLE IF NOT EXISTS changes (
  layer_id TEXT NOT NULL REFERENCES layers (id),
  sequence INTEGER NOT NULL,
  at TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN (
    'layer_configured', 'item_published', 'item_revised', 'item_retracted', 'layer_retired'
  )),
  item_id TEXT,
  version INTEGER,
  PRIMARY KEY (layer_id, sequence)
) WITHOUT ROWID;
INSERT OR IGNORE INTO schema_version (version) VALUES (3);
