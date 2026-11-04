-- Each item's bounding box for bounded volume queries (query-a-bounded-volume): its current
-- shape's box in the ICRS about the barycentre, in megaparsecs, the cell index's candidate
-- filter before the shape itself is tested. Written by every publish and revision and kept by
-- a retraction; NULL for an item stored before this migration until its next revision.
ALTER TABLE items ADD COLUMN min_x REAL;
ALTER TABLE items ADD COLUMN min_y REAL;
ALTER TABLE items ADD COLUMN min_z REAL;
ALTER TABLE items ADD COLUMN max_x REAL;
ALTER TABLE items ADD COLUMN max_y REAL;
ALTER TABLE items ADD COLUMN max_z REAL;
