-- The D1 schema of Univ releases, version 1 (univ_model::release::SCHEMA_VERSION).
--
-- One database holds every published release. A table that belongs to one release carries a
-- `release` column that leads its primary key: publishing release n + 1 only inserts rows whose
-- release is n + 1, which no read of release n selects, and `latest` moves in one write once
-- they are all in. Pruning release n deletes its rows from those tables and sets its
-- `releases.pruned_at`; the id ledger and the list of releases are kept.
--
-- Every statement can run again: tables and indexes are created only when missing, and rows are
-- written with INSERT OR IGNORE on their primary keys, so a row once written stays as it was
-- (a release's rows never change, and publishing and pruning set their own columns later).
--
-- Columns holding JSON hold what univ-model serializes for that value, as the API returns it.
-- Positions are ICRS Cartesian about the Solar System barycentre unless named otherwise.

CREATE TABLE IF NOT EXISTS schema_version (
  version INTEGER PRIMARY KEY
);

-- Every release ever built and published, pruned ones included.
CREATE TABLE IF NOT EXISTS releases (
  number INTEGER PRIMARY KEY,
  built_at TEXT NOT NULL,
  published_at TEXT,
  pruned_at TEXT,
  cosmology TEXT NOT NULL,
  peculiar_velocity_km_s REAL NOT NULL,
  model_version TEXT NOT NULL,
  ingest_version TEXT NOT NULL,
  schema_version INTEGER NOT NULL,
  matching_rules TEXT NOT NULL,
  ranking_rule TEXT NOT NULL,
  ranking_rule_name TEXT NOT NULL,
  sources TEXT NOT NULL,
  objects INTEGER NOT NULL,
  structures INTEGER NOT NULL,
  grids INTEGER NOT NULL,
  tiles INTEGER NOT NULL
);

-- The release that reads naming no release answer from: one row, moved in a single write.
CREATE TABLE IF NOT EXISTS latest (
  one INTEGER PRIMARY KEY CHECK (one = 1),
  release INTEGER NOT NULL REFERENCES releases (number),
  moved_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS objects (
  release INTEGER NOT NULL,
  id INTEGER NOT NULL,
  kind TEXT NOT NULL,
  ra REAL,
  dec REAL,
  epoch REAL,
  placed INTEGER NOT NULL,
  not_placed TEXT,
  scale_domain TEXT,
  x REAL,
  y REAL,
  z REAL,
  distance_mpc REAL,
  preferred_distance TEXT,
  preferred_position TEXT,
  position TEXT,
  links TEXT NOT NULL,
  relations TEXT NOT NULL,
  PRIMARY KEY (release, id)
) WITHOUT ROWID;

-- Each catalogue entry resolved to an object, with how it was resolved.
CREATE TABLE IF NOT EXISTS entries (
  release INTEGER NOT NULL,
  source TEXT NOT NULL,
  identifier TEXT NOT NULL,
  object_id INTEGER NOT NULL,
  anchor INTEGER NOT NULL,
  resolution TEXT NOT NULL,
  name TEXT,
  position TEXT,
  group_id TEXT,
  flags TEXT NOT NULL,
  PRIMARY KEY (release, source, identifier)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS entries_by_object ON entries (release, object_id);

-- Every measurement of every entry, its native value beside its normalized one.
CREATE TABLE IF NOT EXISTS measurements (
  release INTEGER NOT NULL,
  object_id INTEGER NOT NULL,
  source TEXT NOT NULL,
  identifier TEXT NOT NULL,
  quantity TEXT NOT NULL,
  n INTEGER NOT NULL,
  kind_of_value TEXT NOT NULL,
  native_value REAL NOT NULL,
  native_unit TEXT NOT NULL,
  value REAL NOT NULL,
  unit TEXT NOT NULL,
  uncertainty TEXT NOT NULL,
  detail TEXT NOT NULL,
  PRIMARY KEY (release, object_id, source, identifier, quantity, n)
) WITHOUT ROWID;

-- Every designation an object carries, an entry's own or one its source gives beside it.
CREATE TABLE IF NOT EXISTS designations (
  release INTEGER NOT NULL,
  source TEXT NOT NULL,
  identifier TEXT NOT NULL,
  object_id INTEGER NOT NULL,
  PRIMARY KEY (release, source, identifier, object_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS designations_by_object ON designations (release, object_id);

-- Common names by their key: lower case with white space removed.
CREATE TABLE IF NOT EXISTS names (
  release INTEGER NOT NULL,
  key TEXT NOT NULL,
  object_id INTEGER NOT NULL,
  source TEXT NOT NULL,
  name TEXT NOT NULL,
  PRIMARY KEY (release, key, object_id, source)
) WITHOUT ROWID;

-- The id ledger, kept across releases and never pruned: the ids each release issued, and
-- what became of every id that left the releases. A read of release n takes the rows whose
-- release is at most n.
CREATE TABLE IF NOT EXISTS ids_issued (
  release INTEGER PRIMARY KEY,
  first INTEGER NOT NULL,
  last INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS ids_replaced (
  id INTEGER PRIMARY KEY,
  release INTEGER NOT NULL,
  change TEXT NOT NULL,
  replaced_by TEXT NOT NULL
);

-- The cell index of bounded volume queries (univ_model::release::cells). A placed object is in
-- the level-0 cell of its scale domain that holds its position; a structure is in each cell of
-- the structures space, at its level, that its bounding box meets.
CREATE TABLE IF NOT EXISTS object_cells (
  release INTEGER NOT NULL,
  domain TEXT NOT NULL,
  ix INTEGER NOT NULL,
  iy INTEGER NOT NULL,
  iz INTEGER NOT NULL,
  object_id INTEGER NOT NULL,
  PRIMARY KEY (release, domain, ix, iy, iz, object_id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS structure_cells (
  release INTEGER NOT NULL,
  level INTEGER NOT NULL,
  ix INTEGER NOT NULL,
  iy INTEGER NOT NULL,
  iz INTEGER NOT NULL,
  structure TEXT NOT NULL,
  PRIMARY KEY (release, level, ix, iy, iz, structure)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS sources (
  release INTEGER NOT NULL,
  id TEXT NOT NULL,
  title TEXT NOT NULL,
  provider TEXT NOT NULL,
  scale_domain TEXT NOT NULL,
  citation TEXT NOT NULL,
  terms_url TEXT NOT NULL,
  terms_quote TEXT NOT NULL,
  credit TEXT NOT NULL,
  redistribution TEXT NOT NULL,
  review TEXT NOT NULL,
  entries INTEGER NOT NULL,
  access TEXT NOT NULL,
  cuts TEXT NOT NULL,
  fixes TEXT NOT NULL,
  PRIMARY KEY (release, id)
) WITHOUT ROWID;

-- Each payload a source's provenance records, and what normalization made of the source.
CREATE TABLE IF NOT EXISTS payloads (
  release INTEGER NOT NULL,
  source TEXT NOT NULL,
  item TEXT NOT NULL,
  url TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  retrieved TEXT NOT NULL,
  rows INTEGER,
  PRIMARY KEY (release, source, item)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS normalizations (
  release INTEGER NOT NULL,
  source TEXT NOT NULL,
  published INTEGER NOT NULL,
  left_out TEXT NOT NULL,
  fixes TEXT NOT NULL,
  PRIMARY KEY (release, source)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS structures (
  release INTEGER NOT NULL,
  id TEXT NOT NULL,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  geometry_kind TEXT,
  record TEXT NOT NULL,
  members TEXT,
  bounds TEXT,
  PRIMARY KEY (release, id)
) WITHOUT ROWID;

-- Each scale domain's tiles, whose manifest and tiles are in R2.
CREATE TABLE IF NOT EXISTS scale_domains (
  release INTEGER NOT NULL,
  name TEXT NOT NULL,
  unit TEXT NOT NULL,
  radius REAL NOT NULL,
  points INTEGER NOT NULL,
  nodes INTEGER NOT NULL,
  manifest_key TEXT NOT NULL,
  PRIMARY KEY (release, name)
) WITHOUT ROWID;

-- Each volume grid, whose header and samples are in R2.
CREATE TABLE IF NOT EXISTS grids (
  release INTEGER NOT NULL,
  id TEXT NOT NULL,
  source TEXT NOT NULL,
  unit TEXT NOT NULL,
  shape TEXT NOT NULL,
  fields TEXT NOT NULL,
  header_key TEXT NOT NULL,
  PRIMARY KEY (release, id)
) WITHOUT ROWID;
