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
pub const SHARD_TABLES: [&str; 5] = [
    "objects",
    "entries",
    "measurements",
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

/// The Morton code of the cell within reach nearest `cell`, each index clamped to it. A code
/// grows with each index, so every cell of a box within reach has a code from that of its
/// clamped least corner to that of its clamped greatest: the range of codes a box can meet.
#[must_use]
pub fn morton_clamped(cell: Cell) -> u64 {
    let clamp = |i: i64| i.clamp(-MORTON_REACH, MORTON_REACH - 1);
    let within = Cell {
        ix: clamp(cell.ix),
        iy: clamp(cell.iy),
        iz: clamp(cell.iz),
    };
    morton(within).unwrap_or(0)
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
    fn every_cell_of_a_box_has_a_code_between_its_clamped_corners() {
        let (min, max) = (cell(-3, 2, -1), cell(4, 5, 2));
        let (lo, hi) = (morton_clamped(min), morton_clamped(max));
        for ix in min.ix..=max.ix {
            for iy in min.iy..=max.iy {
                for iz in min.iz..=max.iz {
                    let code = morton(cell(ix, iy, iz)).unwrap();
                    assert!((lo..=hi).contains(&code), "({ix}, {iy}, {iz})");
                }
            }
        }
        // A box reaching beyond takes the codes of the cells within reach.
        let wide = morton_clamped(cell(MORTON_REACH * 4, 0, 0));
        assert_eq!(wide, morton(cell(MORTON_REACH - 1, 0, 0)).unwrap());
        assert_eq!(morton_clamped(cell(i64::MIN, i64::MIN, i64::MIN)), 0);
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
