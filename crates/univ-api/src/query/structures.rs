//! The structures of a volume query: those whose placed geometry meets the volume, even with
//! their centre outside it. The structure cells give the candidates, the cells of each level
//! that the volume's box meets; each is then tested by the first the release holds of its
//! placed shape, its placed member list, its bounding box (approximate, and said to be) or its
//! centre.

use serde::Deserialize;
use serde_json::{Value, json};
use univ_model::api::query::{MatchedBy, QueryResult, StructureHit};
use univ_model::geometry::{Aabb, Shape};
use univ_model::release::cells::structure_ranges;
use univ_model::units::{Unit, scale_domain_unit};

use super::Query;
use super::cursor::After;
use crate::backend::{Backend, json_column, rows_as};
use crate::problem::Problem;

#[derive(Deserialize)]
struct Row {
    id: String,
    name: String,
    kind: String,
    scale_domain: String,
    centre_x: Option<f64>,
    centre_y: Option<f64>,
    centre_z: Option<f64>,
}

/// Up to `want` structures that meet the volume after structure `after`, by id.
pub(super) async fn page<B: Backend>(
    backend: &B,
    query: &Query,
    after: Option<&str>,
    want: usize,
) -> Result<Vec<(After, QueryResult)>, Problem> {
    let cells: Vec<String> = structure_ranges(&query.region.bounds())
        .into_iter()
        .map(|(level, r)| {
            format!(
                "(level = {level} AND ix BETWEEN {} AND {} AND iy BETWEEN {} AND {} \
                 AND iz BETWEEN {} AND {})",
                r.min.ix, r.max.ix, r.min.iy, r.max.iy, r.min.iz, r.max.iz
            )
        })
        .collect();
    if cells.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT s.id, s.name, s.kind, s.scale_domain, s.centre_x, s.centre_y, s.centre_z, \
         s.members, s.bounds, sd.shape FROM structures s \
         LEFT JOIN structure_details sd ON sd.release = s.release AND sd.id = s.id \
         WHERE s.release = ?1 AND s.id > ?2 AND s.id IN (SELECT structure FROM structure_cells \
         WHERE release = ?1 AND ({})) \
         AND (?3 IS NULL OR s.kind IN (SELECT value FROM json_each(?3))) \
         AND (?4 IS NULL OR s.scale_domain IN (SELECT value FROM json_each(?4))) \
         ORDER BY s.id",
        cells.join(" OR ")
    );
    let rows = backend
        .releases(
            &sql,
            &[
                json!(query.release),
                json!(after.unwrap_or_default()),
                json!(query.filters.kinds),
                json!(query.filters.scale_domains),
            ],
        )
        .await?;
    let mut found = Vec::new();
    for row in rows {
        if found.len() >= want {
            break;
        }
        let shape: Option<Shape> = json_column::<Option<Value>>(&row, "shape")?
            .and_then(|v| serde_json::from_value(v).ok());
        let members: Option<Shape> = json_column::<Option<Value>>(&row, "members")?
            .and_then(|v| serde_json::from_value(v.get("shape")?.clone()).ok());
        let bounds: Option<Aabb> = json_column::<Option<Value>>(&row, "bounds")?
            .and_then(|v| serde_json::from_value(v).ok());
        let Some(structure) = rows_as::<Row>(vec![row])?.pop() else {
            continue;
        };
        if let Some(matched_by) = meets(query, &structure, shape.as_ref(), members.as_ref(), bounds)
        {
            let hit = StructureHit {
                id: structure.id.clone(),
                name: structure.name,
                kind: structure.kind,
                scale_domain: structure.scale_domain,
                matched_by,
            };
            found.push((
                After::Structure { id: structure.id },
                QueryResult::Structure(hit),
            ));
        }
    }
    Ok(found)
}

/// How `structure` meets the volume, by the first of its representations the release holds;
/// none when that one does not meet it, or it has none.
fn meets(
    query: &Query,
    structure: &Row,
    shape: Option<&Shape>,
    members: Option<&Shape>,
    bounds: Option<Aabb>,
) -> Option<MatchedBy> {
    let in_mpc = |shape: &Shape| query.region.meets(Unit::Megaparsec, shape) == Some(true);
    // The box and centre are in the scale domain's unit.
    let factor = Unit::Megaparsec
        .factor_to(scale_domain_unit(&structure.scale_domain))
        .ok()?;
    let region = query.region.clone().scaled(factor);
    let centre = match (structure.centre_x, structure.centre_y, structure.centre_z) {
        (Some(x), Some(y), Some(z)) => Some([x, y, z]),
        _ => None,
    };
    let (matched_by, meets) = if let Some(shape) = shape {
        (MatchedBy::Shape, in_mpc(shape))
    } else if let Some(members) = members {
        (MatchedBy::Members, in_mpc(members))
    } else if let Some(bounds) = &bounds {
        (MatchedBy::BoundingBox, region.meets_box(bounds))
    } else {
        (MatchedBy::Centre, region.contains(centre?))
    };
    meets.then_some(matched_by)
}
