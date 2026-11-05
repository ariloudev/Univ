-- The D1 schema of Univ releases, version 3 (univ_model::release::SCHEMA_VERSION).
--
-- Version 3 adds source_details and structure_details and changes no table of version 2.
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
-- Compact by design, every fact kept (since version 2):
--
-- - Terms. A column whose comment says "term" holds the id of a row of `terms`: the name
--   univ-model serializes for that value, such as 'parsec', 'estimated' or 'credible_interval'.
--   Each release numbers its own terms; a reader loads a release's terms once.
-- - Sources. A column named `source` holds the source's `sources.n` in that release.
-- - Entries. An entry is numbered by its place among its object's entries, from 0: `entry`.
-- - Uncertainties. Four columns `{prefix}form` (term), `minus`, `plus` and `level` hold one
--   univ-model Uncertainty: none_reported, upper_limit and lower_limit set none of the three;
--   symmetric sets minus = plus = sigma; asymmetric minus and plus; credible_region level (its
--   probability); credible_interval minus, plus and level. NULL in all four, where a comment
--   says so, means "the same as the native value's".
-- - Normalized beside native. A measurement keeps its native value, unit and uncertainty; its
--   normalized ones are NULL when they are the native ones unchanged (the same unit).
--
-- Columns holding JSON hold what univ-model serializes for that value, as the API returns it.
-- Positions are ICRS Cartesian about the Solar System barycentre unless named otherwise.

CREATE TABLE IF NOT EXISTS schema_version (
  version INTEGER PRIMARY KEY
);

-- Every release ever loaded, pruned ones included. A release is published once its
-- published_at is set, after latest moved to it; until then its rows may be part loaded, so
-- readers list only the releases whose published_at is set. d1_estimated_bytes is the
-- release's CON-3 estimate, which publishing sums over the releases not pruned. Every derived
-- position of the release is placed under its cosmology (JSON) by univ-model model_version.
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
  tiles INTEGER NOT NULL,
  d1_estimated_bytes INTEGER NOT NULL
);

-- The release that reads naming no release answer from: one row, moved in a single write.
CREATE TABLE IF NOT EXISTS latest (
  one INTEGER PRIMARY KEY CHECK (one = 1),
  release INTEGER NOT NULL REFERENCES releases (number),
  moved_at TEXT NOT NULL
);

-- The names that term columns hold by id, per release.
CREATE TABLE IF NOT EXISTS terms (
  release INTEGER NOT NULL,
  id INTEGER NOT NULL,
  term TEXT NOT NULL,
  PRIMARY KEY (release, id)
) WITHOUT ROWID;

-- Each source of the release, n its index, which the `source` columns of the release hold.
CREATE TABLE IF NOT EXISTS sources (
  release INTEGER NOT NULL,
  n INTEGER NOT NULL,
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
  PRIMARY KEY (release, n)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS sources_by_id ON sources (release, id);

-- Each payload a source's provenance records, and what normalization made of the source.
CREATE TABLE IF NOT EXISTS payloads (
  release INTEGER NOT NULL,
  source INTEGER NOT NULL,
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
  source INTEGER NOT NULL,
  published INTEGER NOT NULL,
  left_out TEXT NOT NULL,
  fixes TEXT NOT NULL,
  PRIMARY KEY (release, source)
) WITHOUT ROWID;

-- Each distinct transformation chain of the release's derived positions, held once. `steps` is
-- the JSON array of univ-model steps with each per-object number replaced by the string "$1"
-- to "$4", filled from the object's chain_p1 to chain_p4, and each copy of the position's
-- native value by "$native" (see objects). `params` is how many of chain_p1..4 it takes,
-- `objects` how many objects of the release it derived.
CREATE TABLE IF NOT EXISTS chains (
  release INTEGER NOT NULL,
  id INTEGER NOT NULL,
  steps TEXT NOT NULL,
  params INTEGER NOT NULL,
  objects INTEGER NOT NULL,
  PRIMARY KEY (release, id)
) WITHOUT ROWID;

-- An object with its preferred measurements and, when placed, its derived position.
--
-- - Sky position: ra, dec (ICRS degrees) at epoch (Julian year), from its preferred position,
--   the entry position_entry, chosen as position_class (term) with position_uncertainty_mas.
-- - Preferred distance: the measurement (preferred_entry, preferred_quantity (term),
--   preferred_n) of the measurements table, ranked preferred_rank (term) at preferred_mpc and
--   decided by preferred_decided (term); its fractional uncertainty is preferred_total (NULL:
--   none reported) with its measurement and peculiar-velocity parts. runner_up_* is the
--   measurement it was preferred over, likewise.
-- - Derived position, when placed: x, y, z (ICRS Cartesian, Mpc) at distance_mpc, a measure
--   (term) distance, with its uncertainty (distance_form, ...), in scale_domain (term). Its
--   direction is its sky position, published in its preferred position entry's frame. Its
--   native value is that of its preferred measurement: a distance's or a parallax's native
--   value, a redshift's CMB-frame value (cmb). Its assumed cosmology is the preferred
--   distance's (measurements.extra). Its cosmology and model version are the release's.
--   Galactic and supergalactic Cartesian positions are x, y, z rotated as univ-model rotates
--   them (ESA 1997; Lahav et al. 2000). Its transformation chain is chains.steps of chain,
--   filled with chain_p1 to chain_p4 and the native value.
-- - links and relations: JSON arrays, NULL when empty.
CREATE TABLE IF NOT EXISTS objects (
  release INTEGER NOT NULL,
  id INTEGER NOT NULL,
  kind INTEGER NOT NULL,
  ra REAL,
  dec REAL,
  epoch REAL,
  position_entry INTEGER,
  position_class INTEGER,
  position_uncertainty_mas REAL,
  placed INTEGER NOT NULL,
  not_placed INTEGER,
  scale_domain INTEGER,
  x REAL,
  y REAL,
  z REAL,
  distance_mpc REAL,
  distance_form INTEGER,
  distance_minus REAL,
  distance_plus REAL,
  distance_level REAL,
  measure INTEGER,
  chain INTEGER,
  chain_p1 REAL,
  chain_p2 REAL,
  chain_p3 REAL,
  chain_p4 REAL,
  preferred_entry INTEGER,
  preferred_quantity INTEGER,
  preferred_n INTEGER,
  preferred_rank INTEGER,
  preferred_mpc REAL,
  preferred_decided INTEGER,
  preferred_total REAL,
  preferred_measurement REAL,
  preferred_velocity REAL,
  runner_up_entry INTEGER,
  runner_up_quantity INTEGER,
  runner_up_n INTEGER,
  runner_up_total REAL,
  runner_up_measurement REAL,
  runner_up_velocity REAL,
  links TEXT,
  relations TEXT,
  PRIMARY KEY (release, id)
) WITHOUT ROWID;

-- Each catalogue entry resolved to an object, with how it was resolved.
--
-- - resolution (term): anchor, cross_identification or match. A cross-identification gives
--   its scheme (term), the identifier it shares (shared, NULL when the entry's own) and its
--   confidence; a match gives its method as scheme (term), its separation_arcsec, the
--   line-of-sight difference (term velocity, delta in km/s, or distance, delta as a fraction)
--   and its confidence. Each was made through the object's entry via_entry.
-- - Sky position as published: lon, lat in position_unit (term) in frame (term) at epoch; ra,
--   dec its ICRS degrees, NULL when the native position is ICRS degrees already. Its published
--   errors error_ra and error_dec are in error_unit (term), of kind error_kind (term), each with
--   no uncertainty of its own and normalized to degrees.
-- - flags: JSON object, NULL when empty. extra: JSON object of what no column holds, NULL when
--   nothing: "via" (the designation it was resolved through, when not one of the object's
--   entries), "position_uncertainty" (when not held by the error columns), "spectral_type"
--   (its text as published and its kind of value, where its source gives one) and "references"
--   (the papers a researched entry's values come from: citation, doi, bibcode).
CREATE TABLE IF NOT EXISTS entries (
  release INTEGER NOT NULL,
  object_id INTEGER NOT NULL,
  entry INTEGER NOT NULL,
  source INTEGER NOT NULL,
  identifier TEXT NOT NULL,
  resolution INTEGER NOT NULL,
  scheme INTEGER,
  shared TEXT,
  via_entry INTEGER,
  confidence REAL,
  separation_arcsec REAL,
  difference INTEGER,
  delta REAL,
  name TEXT,
  frame INTEGER,
  lon REAL,
  lat REAL,
  position_unit INTEGER,
  epoch REAL,
  ra REAL,
  dec REAL,
  error_ra REAL,
  error_dec REAL,
  error_unit INTEGER,
  error_kind INTEGER,
  group_id TEXT,
  flags TEXT,
  extra TEXT,
  PRIMARY KEY (release, object_id, entry)
) WITHOUT ROWID;

-- Every measurement of every entry, its native value beside its normalized one. quantity
-- (term) is distance, redshift, parallax, velocity, proper_motion_ra, proper_motion_dec,
-- magnitude, supergalactic_velocity_x, _y or _z; n its place among the entry's values of it.
--
-- - native_value, native_unit (term) and native_form, ... as published; value, unit (term) and
--   form, ... normalized, NULL when the native ones unchanged.
-- - A distance: kind (term) and method (term) of the distance, its measure (term); extra holds
--   also_published and assumed_cosmology where it has them.
-- - A redshift: native is the published velocity cz where the source gives one, else the
--   heliocentric z; normalized is the heliocentric z (NULL when that is the native value);
--   cmb and cmb_form, ... its CMB-frame value; kind (term) spectroscopic or photometric;
--   method (term) the dipole model.
-- - A magnitude: kind (term) its band.
-- - A colour index: kind (term) its two bands, such as B-V, numbered within the entry as its
--   distances are; extra holds derived_from, the two magnitude columns it was computed from,
--   where its source publishes the magnitudes and not the colour.
CREATE TABLE IF NOT EXISTS measurements (
  release INTEGER NOT NULL,
  object_id INTEGER NOT NULL,
  entry INTEGER NOT NULL,
  quantity INTEGER NOT NULL,
  n INTEGER NOT NULL,
  kind_of_value INTEGER NOT NULL,
  native_value REAL NOT NULL,
  native_unit INTEGER NOT NULL,
  native_form INTEGER NOT NULL,
  native_minus REAL,
  native_plus REAL,
  native_level REAL,
  value REAL,
  unit INTEGER,
  form INTEGER,
  minus REAL,
  plus REAL,
  level REAL,
  kind INTEGER,
  method INTEGER,
  measure INTEGER,
  cmb REAL,
  cmb_form INTEGER,
  cmb_minus REAL,
  cmb_plus REAL,
  cmb_level REAL,
  extra TEXT,
  PRIMARY KEY (release, object_id, entry, quantity, n)
) WITHOUT ROWID;

-- Every designation an object carries, an entry's own or one its source gives beside it.
CREATE TABLE IF NOT EXISTS designations (
  release INTEGER NOT NULL,
  source INTEGER NOT NULL,
  identifier TEXT NOT NULL,
  object_id INTEGER NOT NULL,
  PRIMARY KEY (release, source, identifier, object_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS designations_by_object ON designations (release, object_id);

-- Common names by their key: lower case with white space removed. The name is part of the
-- primary key, so two spellings one source gives an object ("M 31", "M31") are both kept.
CREATE TABLE IF NOT EXISTS names (
  release INTEGER NOT NULL,
  key TEXT NOT NULL,
  object_id INTEGER NOT NULL,
  source INTEGER NOT NULL,
  name TEXT NOT NULL,
  PRIMARY KEY (release, key, object_id, source, name)
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
-- the level-0 cell of its scale domain (term) that holds its position; a structure is in each
-- cell of the structures space, at its level, that its bounding box meets.
CREATE TABLE IF NOT EXISTS object_cells (
  release INTEGER NOT NULL,
  domain INTEGER NOT NULL,
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

-- Each researched structure, listed by kind and scale domain. Its centre is ICRS Cartesian
-- about the barycentre in its scale domain's unit (pc for stars, Mpc otherwise), NULL when it
-- cannot be placed. members is JSON of its member list placed whole: "shape", each member an
-- object at its derived position or a tracer, and "placed_from_file", the redshift, its kind,
-- where it was cited, its CMB-frame value and the comoving distance of each tracer placed from
-- the structure file's own sky position and redshift; NULL when the list is not placed whole.
CREATE TABLE IF NOT EXISTS structures (
  release INTEGER NOT NULL,
  id TEXT NOT NULL,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  scale_domain TEXT NOT NULL,
  centre_x REAL,
  centre_y REAL,
  centre_z REAL,
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
  source INTEGER NOT NULL,
  unit TEXT NOT NULL,
  shape TEXT NOT NULL,
  fields TEXT NOT NULL,
  header_key TEXT NOT NULL,
  PRIMARY KEY (release, id)
) WITHOUT ROWID;

-- Each source's primary reference and coverage (version 3), source its sources.n.
--
-- - Reference: bibcode, the ADS bibcode of the paper the source's citation names first, and
--   doi, its DOI without the "doi:" prefix, each NULL when it has none; reference_note says why
--   a source has no single primary reference (the researched structures cite their papers per
--   structure), NULL when it has one.
-- - sky: JSON {"order": 5, "cells": [...]}, the NESTED HEALPix cells of order 5 (Gorski et al.
--   2005) in the ICRS, ascending, that hold the ICRS position of at least one of the source's
--   entries published in the release; NULL when no entry has a sky position.
-- - radial: JSON array, one item per measure its entries hold: {"measure": "redshift",
--   "distance" or "parallax", "min", "max", "unit" (univ-model unit name), "basis" (what the
--   values are), "entries" (how many contributed)}, over the entries' normalized values; NULL
--   when none holds one. A grid source, or the researched structures, holds no entry: both NULL.
CREATE TABLE IF NOT EXISTS source_details (
  release INTEGER NOT NULL,
  source INTEGER NOT NULL,
  bibcode TEXT,
  doi TEXT,
  reference_note TEXT,
  sky TEXT,
  radial TEXT,
  PRIMARY KEY (release, source)
) WITHOUT ROWID;

-- Each researched structure's placed geometry, outline test and rescaled sizes (version 3), id
-- its structures.id.
--
-- - shape: JSON univ-model shape of the structure as placed (point, sphere, member list or sky
--   region), ICRS Cartesian about the barycentre in its scale domain's unit, beside the
--   geometry its structure file gives (structures.record); NULL when it cannot be placed.
-- - derivation: JSON {"from": JSON pointers into structures.record of each cited value used,
--   "frame_chain": the frames converted through, "distance": {"from", "method"}, "cosmology":
--   "release"}: how shape, the centre and the bounds were derived; NULL when unplaced.
-- - outline: JSON result of the test that admitted or refused the structure's outline on a
--   volume grid: {"grid", "field", "threshold", "test", "verdict": "admitted" or "refused",
--   "reason", "cells", "touches_edge", "checks", "dense"}; NULL for a structure not outlined
--   from a grid.
-- - rescaled: JSON array of each length published under another cosmology: {"value": JSON
--   pointer into the record, "native": {"value", "unit", "hubble_constant", "assumed"},
--   "rescaled": {"value", "unit"}, "factor", "method": "hubble_ratio" or "comoving_ratio",
--   "redshift": that of a comoving ratio, else null}; NULL when none.
CREATE TABLE IF NOT EXISTS structure_details (
  release INTEGER NOT NULL,
  id TEXT NOT NULL,
  shape TEXT,
  derivation TEXT,
  outline TEXT,
  rescaled TEXT,
  PRIMARY KEY (release, id)
) WITHOUT ROWID;
