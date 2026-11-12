//! Object shards (schema version 5): a release's objects divided among several D1 databases by
//! the cell of the cell index that holds each, beside one catalogue database.
//!
//! - **Which rows.** An object's rows in [`SHARD_TABLES`] are in one object shard; every other
//!   table's rows, and the index of the shards (`object_shards`, `shard_cells`,
//!   `release_shards`), are in the catalogue database ([`Table::in_shard`]).
//! - **Which shard.** The level-0 cells of each scale domain are taken in the order of their
//!   [`morton`] codes, domain by domain, and packed into consecutive shards up to a budget, so
//!   all the objects of one cell are in one shard and each shard holds one run of cells in each
//!   domain it reaches. An object that is not placed is given a shard by its id; a read by id
//!   finds every object's shard in `object_shards`.
//! - **Where.** Shard `i` is the database bound as `OBJECTS_{i}`; the catalogue is `DB`.

use std::fmt;

use super::cells::Cell;
use super::sql::Table;

/// The tables whose rows of one object are in that object's shard.
pub const SHARD_TABLES: [&str; 11] = [
    "objects",
    "entries",
    "measurements",
    "measurement_details",
    "entry_flags",
    "entry_references",
    "object_links",
    "object_relations",
    "memberships",
    "designations",
    "object_cells",
];

/// A D1 database of the releases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Database {
    /// The catalogue database: sources, terms, structures, grids and the shards' index.
    Catalogue,
    /// Object shard `i`, from 0.
    Shard(u32),
}

impl Database {
    /// Its directory among a release's D1 files, below `d1/`: `catalogue` or `shard-{i:02}`.
    #[must_use]
    pub fn dir(self) -> String {
        match self {
            Self::Catalogue => "catalogue".to_owned(),
            Self::Shard(i) => format!("shard-{i:02}"),
        }
    }

    /// The database a directory of [`Database::dir`] names.
    #[must_use]
    pub fn from_dir(dir: &str) -> Option<Self> {
        if dir == "catalogue" {
            return Some(Self::Catalogue);
        }
        let i = dir.strip_prefix("shard-")?;
        (i.len() >= 2 && i.bytes().all(|b| b.is_ascii_digit()))
            .then(|| i.parse().ok().map(Self::Shard))
            .flatten()
    }

    /// The name `wrangler.toml` gives it: [`CATALOGUE_NAME`], or `univ-objects-{i}` for object
    /// shard `i`.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Catalogue => CATALOGUE_NAME.to_owned(),
            Self::Shard(i) => format!("univ-objects-{i}"),
        }
    }

    /// The binding Univ Server reads it through: `DB` or `OBJECTS_{i}`.
    #[must_use]
    pub fn binding(self) -> String {
        match self {
            Self::Catalogue => "DB".to_owned(),
            Self::Shard(i) => format!("OBJECTS_{i}"),
        }
    }
}

/// The name `wrangler.toml` gives the catalogue database.
pub const CATALOGUE_NAME: &str = "univ";

/// How many object shards the configuration `toml` binds: `OBJECTS_0` onwards, each named as
/// [`Database::name`] names it, until the first missing.
#[must_use]
pub fn bound_shards(toml: &str) -> u32 {
    let blocks: Vec<&str> = toml
        .split("[[d1_databases]]")
        .skip(1)
        .map(|block| block.split("[[").next().unwrap_or(block))
        .collect();
    let declared = |shard: Database| {
        let binding = format!("binding = \"{}\"", shard.binding());
        let name = format!("database_name = \"{}\"", shard.name());
        blocks
            .iter()
            .any(|b| b.contains(&binding) && b.contains(&name))
    };
    let bound = (0..u32::MAX)
        .take_while(|&i| declared(Database::Shard(i)))
        .count();
    u32::try_from(bound).unwrap_or(u32::MAX)
}

impl fmt::Display for Database {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalogue => f.write_str("the catalogue database"),
            Self::Shard(i) => write!(f, "object shard {i}"),
        }
    }
}

impl Table {
    /// Whether an object shard holds this table's rows, as it does for [`SHARD_TABLES`]; the
    /// catalogue holds every other's.
    #[must_use]
    pub fn in_shard(&self) -> bool {
        SHARD_TABLES.contains(&self.name)
    }

    /// Whether this table's rows grow with a release's objects: rows of each object or each
    /// catalogue entry, shared between releases, each keyed by its object's id.
    #[must_use]
    pub fn grows_with_objects(&self) -> bool {
        self.scope == super::Scope::PerRelease && !self.scale.allows_json()
    }
}

/// How far a cell index may lie from 0 along an axis for a Morton code: 2^16 cells, 655,360 Mpc
/// in the 10 Mpc cells of the cosmic web, 65,536 Mpc in the 1 Mpc ones of nearby galaxies and
/// 655 kpc in the 10 pc ones of stars, beyond any placed object. Each code then holds 51 bits,
/// below 2^53: a double holds it exactly, as D1, `SQLite`'s `total()` and a JavaScript reader
/// read it.
pub const MORTON_REACH: i64 = 1 << 16;

/// The bits of each axis in a Morton code.
const MORTON_BITS: u32 = MORTON_REACH.trailing_zeros() + 1;

/// The Morton (Z-order) code of a cell: the bits of its three indexes, each biased by
/// [`MORTON_REACH`] to 17 bits, interleaved x, y, z from the lowest. Cells near each other in
/// space are mostly near each other in this order. None for a cell beyond the reach.
#[must_use]
pub fn morton(cell: Cell) -> Option<u64> {
    let bits = |i: i64| -> Option<u64> {
        let b = i.checked_add(MORTON_REACH)?;
        (0..2 * MORTON_REACH).contains(&b).then(|| b.unsigned_abs())
    };
    let (x, y, z) = (bits(cell.ix)?, bits(cell.iy)?, bits(cell.iz)?);
    Some(spread(x) | spread(y) << 1 | spread(z) << 2)
}

/// The ranges of Morton codes, sorted and apart, that hold every cell within reach of the box
/// from `min` to `max`, at most about `budget` of them.
///
/// A cube of side 2^l aligned on 2^l holds the codes from its least corner's on, 8^l of them,
/// and nothing else. The box is cut into such cubes from the whole reach down, level by level:
/// a cube inside the box is one range, a cube outside it none, and a cube across its edge is cut
/// into its eight until another level would pass `budget`, then taken whole. The ranges hold
/// more codes than the box only by those last cubes. One range from the codes of the box's
/// corners would not do: a box across an index of 0 would take nearly every code, since the
/// bias sets the top bit of each index from 0 up.
#[must_use]
pub fn morton_ranges(min: Cell, max: Cell, budget: usize) -> Vec<(u64, u64)> {
    let biased = |i: i64| (i.clamp(-MORTON_REACH, MORTON_REACH - 1) + MORTON_REACH).unsigned_abs();
    let lo = [biased(min.ix), biased(min.iy), biased(min.iz)];
    let hi = [biased(max.ix), biased(max.iy), biased(max.iz)];
    if (0..3).any(|a| lo[a] > hi[a]) {
        return Vec::new();
    }
    let code = |c: [u64; 3]| spread(c[0]) | spread(c[1]) << 1 | spread(c[2]) << 2;
    let range = |c: [u64; 3], level: u32| (code(c), code(c) + (1_u64 << (3 * level)) - 1);
    let mut ranges = Vec::new();
    // The cubes across the box's edge at `level`, by their least corners.
    let mut across = vec![[0_u64; 3]];
    let mut level = MORTON_BITS;
    while !across.is_empty() {
        if level == 0 || ranges.len() + 8 * across.len() > budget {
            ranges.extend(across.iter().map(|&c| range(c, level)));
            break;
        }
        level -= 1;
        let side = 1_u64 << level;
        let mut next = Vec::new();
        for corner in across {
            for octant in 0..8_u64 {
                let c = [0, 1, 2].map(|a| corner[a] + ((octant >> a) & 1) * side);
                let last = c.map(|v| v + side - 1);
                if (0..3).any(|a| last[a] < lo[a] || c[a] > hi[a]) {
                    continue;
                }
                if (0..3).all(|a| c[a] >= lo[a] && last[a] <= hi[a]) {
                    ranges.push(range(c, level));
                } else {
                    next.push(c);
                }
            }
        }
        across = next;
    }
    ranges.sort_unstable();
    // Ranges that meet are one.
    let mut merged: Vec<(u64, u64)> = Vec::with_capacity(ranges.len());
    for (first, last) in ranges {
        match merged.last_mut() {
            Some(prev) if prev.1 + 1 >= first => prev.1 = prev.1.max(last),
            _ => merged.push((first, last)),
        }
    }
    merged
}

/// The [`MORTON_BITS`] low bits of `v`, each moved to every third bit.
fn spread(v: u64) -> u64 {
    (0..MORTON_BITS).fold(0, |code, bit| code | ((v >> bit) & 1) << (3 * bit))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(ix: i64, iy: i64, iz: i64) -> Cell {
        Cell { ix, iy, iz }
    }

    #[test]
    fn morton_codes_interleave_the_biased_indexes() {
        let origin = morton(cell(0, 0, 0)).unwrap();
        // The bias sets bit 16 of each axis: bits 48, 49 and 50.
        assert_eq!(origin, 0b111 << 48);
        assert_eq!(morton(cell(1, 0, 0)).unwrap(), origin | 0b001);
        assert_eq!(morton(cell(0, 1, 0)).unwrap(), origin | 0b010);
        assert_eq!(morton(cell(0, 0, 1)).unwrap(), origin | 0b100);
        assert_eq!(morton(cell(2, 0, 0)).unwrap(), origin | 0b001_000);
        assert_eq!(
            morton(cell(-MORTON_REACH, -MORTON_REACH, -MORTON_REACH)),
            Some(0)
        );
        assert!(morton(cell(MORTON_REACH, 0, 0)).is_none());
        assert!(morton(cell(0, -MORTON_REACH - 1, 0)).is_none());
        let far = morton(cell(MORTON_REACH - 1, MORTON_REACH - 1, MORTON_REACH - 1)).unwrap();
        assert!(far < 1 << 53, "a double holds every code exactly");
    }

    #[test]
    fn a_boxs_ranges_hold_every_cell_of_it_and_few_others() {
        let held =
            |ranges: &[(u64, u64)], code: u64| ranges.iter().any(|r| (r.0..=r.1).contains(&code));
        // Across 0 on every axis, as a sphere about the Sun is.
        let (min, max) = (cell(-3, -2, -1), cell(4, 5, 2));
        let ranges = morton_ranges(min, max, 64);
        assert!(ranges.len() <= 64 + 8, "{}", ranges.len());
        let mut inside = 0_u64;
        for ix in min.ix..=max.ix {
            for iy in min.iy..=max.iy {
                for iz in min.iz..=max.iz {
                    assert!(
                        held(&ranges, morton(cell(ix, iy, iz)).unwrap()),
                        "({ix}, {iy}, {iz})"
                    );
                    inside += 1;
                }
            }
        }
        // The ranges hold a few times the box's cells, not the half of all codes one range
        // from the corners' codes would.
        let codes: u64 = ranges.iter().map(|r| r.1 - r.0 + 1).sum();
        assert!(codes < 8 * inside, "{codes} codes for {inside} cells");
        let (a, b) = (morton(min).unwrap(), morton(max).unwrap());
        assert!(b - a > 1 << 40, "one range would take {}", b - a);
        // A cell outside the box, between the corners' codes, is in no range.
        assert!(!held(&ranges, morton(cell(40, 40, 40)).unwrap()));
        // With room for every cell, the ranges are exact.
        let exact = morton_ranges(min, max, 10_000);
        let codes: u64 = exact.iter().map(|r| r.1 - r.0 + 1).sum();
        assert_eq!(codes, inside);
        // A box beyond the reach takes the cells within it; an empty box none.
        let wide = morton_ranges(
            cell(MORTON_REACH * 4, 0, 0),
            cell(MORTON_REACH * 5, 0, 0),
            64,
        );
        assert!(held(&wide, morton(cell(MORTON_REACH - 1, 0, 0)).unwrap()));
        assert_eq!(morton_ranges(cell(1, 0, 0), cell(0, 0, 0), 64), Vec::new());
    }

    #[test]
    fn a_database_names_its_directory_and_binding() {
        assert_eq!(Database::Shard(3).dir(), "shard-03");
        assert_eq!(Database::from_dir("shard-03"), Some(Database::Shard(3)));
        assert_eq!(Database::from_dir("catalogue"), Some(Database::Catalogue));
        assert_eq!(Database::from_dir("shard-x"), None);
        assert_eq!(Database::Shard(7).binding(), "OBJECTS_7");
    }
}
