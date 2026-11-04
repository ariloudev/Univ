//! The objects of a volume query: those whose derived position (ICRS Cartesian about the
//! barycentre, in their scale domain's unit, under the release cosmology) lies in the volume.
//! The cell index gives the candidates, the cells of each domain that the volume's box meets,
//! and the volume's own test, a polynomial in the position, decides in SQL: no square root, so
//! that the database tests each exactly as [`Region::contains`] would.

use serde::Deserialize;
use serde_json::json;
use univ_model::api::query::{HitPosition, ObjectHit, QueryResult};
use univ_model::release::cells::CellSpace;
use univ_model::units::{Unit, scale_domain_unit};
use univ_model::volume::Region;

use super::Query;
use super::cursor::After;
use crate::backend::{Backend, rows_as};
use crate::problem::Problem;

/// The scale domains objects are placed in, with their cell spaces.
const DOMAINS: [(&str, CellSpace); 3] = [
    ("stars", CellSpace::Stars),
    ("nearby_galaxies", CellSpace::NearbyGalaxies),
    ("cosmic_web", CellSpace::CosmicWeb),
];

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
    for (name, space) in DOMAINS {
        let wanted = query
            .filters
            .scale_domains
            .as_ref()
            .is_none_or(|d| d.iter().any(|n| n == name));
        let Some(domain) = vocabulary.term_id(name).filter(|_| wanted) else {
            continue;
        };
        let unit = scale_domain_unit(name);
        let Ok(factor) = Unit::Megaparsec.factor_to(unit) else {
            continue;
        };
        let region = query.region.clone().scaled(factor);
        let Some(cells) = space.range(&region.bounds()) else {
            continue;
        };
        units.push((domain, unit));
        selects.push(format!(
            "SELECT o.id AS id, o.kind AS kind, o.scale_domain AS domain, o.x AS x, o.y AS y, \
             o.z AS z FROM object_cells c JOIN objects o ON o.release = c.release \
             AND o.id = c.object_id WHERE c.release = ?1 AND c.domain = {domain} \
             AND c.ix BETWEEN {} AND {} AND c.iy BETWEEN {} AND {} AND c.iz BETWEEN {} AND {} \
             AND c.object_id > ?2 AND {} \
             AND (?4 IS NULL OR o.kind IN (SELECT value FROM json_each(?4))) \
             AND (?5 IS NULL OR EXISTS (SELECT 1 FROM entries e WHERE e.release = o.release \
             AND e.object_id = o.id AND e.source IN (SELECT value FROM json_each(?5))))",
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
    let rows = backend
        .releases(
            &sql,
            &[
                json!(query.release),
                json!(after.unwrap_or(0)),
                json!(want),
                json!(query.kind_ids),
                json!(query.source_ns),
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
                    frame: "icrs".to_owned(),
                    unit: unit_name(unit),
                    at: [row.x, row.y, row.z],
                },
            };
            (After::Object { id: row.id }, QueryResult::Object(hit))
        })
        .collect())
}

/// The unit's name as the API writes it.
fn unit_name(unit: Unit) -> String {
    serde_json::to_value(unit)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// The region's test of a position `(o.x, o.y, o.z)` in SQL, with its numbers written out in
/// full: a sphere's squared distance, a box's three slabs, a cone's shell and angle squared.
fn predicate(region: &Region) -> String {
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
