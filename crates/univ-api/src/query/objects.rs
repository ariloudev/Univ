//! The objects of a volume query: those whose derived position (ICRS Cartesian about the
//! barycentre, in their scale domain's unit, under the release cosmology) lies in the volume.
//! The cell index gives the candidates, the cells of each domain that the volume's box meets,
//! and the volume's own test, a polynomial in the position, decides in SQL: no square root, so
//! that the database tests each as [`Region::contains`] would, which a test holds it to.
//!
//! The cells drive the join (`CROSS JOIN` keeps them the outer loop): ordered by id alone, the
//! database would walk every object of the release and look each up in the cells.

use serde::Deserialize;
use serde_json::json;
use univ_model::api::query::{HitPosition, ObjectHit, QueryResult};
use univ_model::position::Frame;
use univ_model::release::cells::{CellSpace, SPACES};
use univ_model::units::{Unit, scale_domain_unit};
use univ_model::volume::Region;

use super::Query;
use super::cursor::After;
use crate::backend::{Backend, rows_as};
use crate::problem::Problem;

#[derive(Deserialize)]
struct Row {
    id: u64,
    kind: i64,
    domain: i64,
    x: f64,
    y: f64,
    z: f64,
}

/// Up to `want` objects in the volume after object `after`, by id.
pub(super) async fn page<B: Backend>(
    backend: &B,
    query: &Query,
    after: Option<u64>,
    want: usize,
) -> Result<Vec<(After, QueryResult)>, Problem> {
    let vocabulary = &query.vocabulary;
    let mut selects = Vec::new();
    let mut units = Vec::new();
    for space in SPACES.into_iter().filter(|s| *s != CellSpace::Structures) {
        let name = space.name();
        let (Some(domain), Some(region)) = (vocabulary.term_id(name), query.region_in_domain(name))
        else {
            continue;
        };
        let Some(cells) = space
            .range(&region.bounds())
            .filter(|_| query.wants_domain(name))
        else {
            continue;
        };
        units.push((domain, scale_domain_unit(name)));
        selects.push(format!(
            "SELECT * FROM (SELECT o.id AS id, o.kind AS kind, o.scale_domain AS domain, \
             o.x AS x, o.y AS y, o.z AS z FROM object_cells c CROSS JOIN objects o \
             ON o.release = c.release AND o.id = c.object_id WHERE c.release = ?1 \
             AND c.domain = {domain} AND c.ix BETWEEN {} AND {} AND c.iy BETWEEN {} AND {} \
             AND c.iz BETWEEN {} AND {} AND c.object_id > ?2 AND {} \
             AND (?4 IS NULL OR o.kind IN (SELECT value FROM json_each(?4))) \
             AND (?5 IS NULL OR EXISTS (SELECT 1 FROM entries e WHERE e.release = o.release \
             AND e.object_id = o.id AND e.source IN (SELECT value FROM json_each(?5)))) \
             ORDER BY c.object_id LIMIT ?3)",
            cells.min.ix,
            cells.max.ix,
            cells.min.iy,
            cells.max.iy,
            cells.min.iz,
            cells.max.iz,
            predicate(&region),
        ));
    }
    if selects.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT * FROM ({}) ORDER BY id LIMIT ?3",
        selects.join(" UNION ALL ")
    );
    let ids = |names: &Option<Vec<String>>, lookup: &dyn Fn(&str) -> Option<i64>| {
        names
            .as_ref()
            .map(|n| n.iter().filter_map(|v| lookup(v)).collect::<Vec<i64>>())
    };
    let kinds = ids(&query.filters.kinds, &|k| vocabulary.term_id(k));
    let sources = ids(&query.filters.sources, &|s| vocabulary.source_n(s));
    let rows = backend
        .releases(
            &sql,
            &[
                json!(query.release),
                json!(after.unwrap_or(0)),
                json!(want),
                json!(kinds),
                json!(sources),
            ],
        )
        .await?;
    Ok(rows_as::<Row>(rows)?
        .into_iter()
        .map(|row| {
            let unit = units
                .iter()
                .find_map(|(domain, unit)| (*domain == row.domain).then_some(*unit))
                .unwrap_or(Unit::Megaparsec);
            let hit = ObjectHit {
                id: row.id,
                kind: vocabulary.term(row.kind).unwrap_or_default().to_owned(),
                scale_domain: vocabulary.term(row.domain).unwrap_or_default().to_owned(),
                position: HitPosition {
                    frame: Frame::Icrs,
                    unit,
                    at: [row.x, row.y, row.z],
                },
            };
            (After::Object { id: row.id }, QueryResult::Object(hit))
        })
        .collect())
}

/// The region's test of a position `(o.x, o.y, o.z)` in SQL, with its numbers written out in
/// full: a sphere's squared distance, a box's three slabs, a cone's shell and angle squared.
pub(crate) fn predicate(region: &Region) -> String {
    let dot = |v: [f64; 3]| format!("({:?} * o.x + {:?} * o.y + {:?} * o.z)", v[0], v[1], v[2]);
    let squared = "(o.x * o.x + o.y * o.y + o.z * o.z)";
    match region {
        Region::Sphere { centre, radius } => format!(
            "((o.x - {:?}) * (o.x - {:?}) + (o.y - {:?}) * (o.y - {:?}) + (o.z - {:?}) * (o.z - {:?})) <= {:?}",
            centre[0],
            centre[0],
            centre[1],
            centre[1],
            centre[2],
            centre[2],
            radius * radius
        ),
        Region::Box {
            to_frame,
            centre,
            half,
        } => (0..3)
            .map(|i| {
                format!(
                    "abs({} - {:?}) <= {:?}",
                    dot(to_frame.0[i]),
                    centre[i],
                    half[i]
                )
            })
            .collect::<Vec<_>>()
            .join(" AND "),
        Region::Sector {
            axis,
            half_angle,
            near,
            far,
        } => {
            let along = dot(*axis);
            let cos = half_angle.cos();
            format!(
                "{squared} BETWEEN {:?} AND {:?} AND {along} >= 0.0 AND {along} * {along} >= {:?} * {squared}",
                near * near,
                far * far,
                cos * cos
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use univ_model::position::Frame;
    use univ_model::volume::{Volume, VolumeForm};

    use super::*;

    fn region(frame: Frame, form: VolumeForm) -> Region {
        Volume {
            frame,
            unit: Unit::Megaparsec,
            form,
        }
        .region_in(Unit::Megaparsec)
        .unwrap()
    }

    #[test]
    fn the_sql_test_of_a_position_is_the_regions_own() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE o (x REAL, y REAL, z REAL);")
            .unwrap();
        // A grid of points over [-12, 12]³ Mpc, off every round number.
        let points: Vec<[f64; 3]> = (0..25 * 25 * 25)
            .map(|i| {
                let c = |n: i32| f64::from(n) - 12.0 + 0.037;
                [c(i % 25), c(i / 25 % 25), c(i / 625)]
            })
            .collect();
        let mut insert = db.prepare("INSERT INTO o VALUES (?1, ?2, ?3)").unwrap();
        for p in &points {
            insert.execute(rusqlite::params![p[0], p[1], p[2]]).unwrap();
        }
        let regions = [
            region(
                Frame::Icrs,
                VolumeForm::Sphere {
                    centre: [1.0, -2.0, 3.0],
                    radius: 7.5,
                },
            ),
            region(
                Frame::Galactic,
                VolumeForm::Box {
                    min: [-4.0, -9.0, -2.0],
                    max: [6.0, 3.0, 5.0],
                },
            ),
            region(
                Frame::Supergalactic,
                VolumeForm::Cone {
                    lon: 40.0,
                    lat: 20.0,
                    opening_angle: 70.0,
                    near: 3.0,
                    far: 11.0,
                },
            ),
            region(
                Frame::Icrs,
                VolumeForm::Cone {
                    lon: 300.0,
                    lat: -50.0,
                    opening_angle: 180.0,
                    near: 0.0,
                    far: 9.0,
                },
            ),
        ];
        for region in regions {
            let sql = format!("SELECT x, y, z FROM o WHERE {}", predicate(&region));
            let mut selected: Vec<[f64; 3]> = db
                .prepare(&sql)
                .unwrap()
                .query_map([], |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?]))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            let mut contained: Vec<[f64; 3]> = points
                .iter()
                .copied()
                .filter(|p| region.contains(*p))
                .collect();
            let key = |p: &[f64; 3]| p.map(f64::to_bits);
            selected.sort_by_key(key);
            contained.sort_by_key(key);
            assert!(!contained.is_empty(), "{region:?} holds points of the grid");
            assert_eq!(selected, contained, "{region:?}");
        }
    }
}
