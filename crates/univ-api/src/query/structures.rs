//! The structures of a volume query: those whose placed geometry meets the volume, even with
//! their centre outside it. The structure cells give the candidates, the cells of each level
//! that the volume's box meets; each is then tested by the first the release holds of its
//! placed shape, its placed member list, its bounding box (approximate, and said to be) or its
//! centre.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::json;
use univ_model::api::query::{MatchedBy, QueryResult, StructureHit};
use univ_model::geometry::{Aabb, Shape};
use univ_model::release::cells::structure_ranges;
use univ_model::units::Unit;
use univ_model::valid_in;
use univ_model::volume::Region;

use super::Query;
use super::cursor::After;
use crate::backend::{Backend, rows_as};
use crate::layers::broken;
use crate::problem::Problem;

/// The candidates read at once.
const CHUNK: usize = 100;

#[derive(Deserialize)]
struct Row {
    id: String,
    name: String,
    kind: String,
    scale_domain: String,
    centre_x: Option<f64>,
    centre_y: Option<f64>,
    centre_z: Option<f64>,
    /// The placed member list, JSON with its shape under `shape`.
    members: Option<String>,
    bounds: Option<String>,
    /// The placed shape, JSON.
    shape: Option<String>,
}

/// What the members column holds that a volume query reads.
#[derive(Deserialize)]
struct Members {
    shape: Option<Shape>,
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
        concat!(
            "SELECT s.id, s.name, s.kind, s.scale_domain, s.centre_x, s.centre_y, s.centre_z, \
             s.members, s.bounds, sd.shape FROM structures s \
             LEFT JOIN structure_details sd ON sd.id = s.id AND ",
            valid_in!("sd.", "?1"),
            " WHERE s.id > ?2 AND ",
            valid_in!("s.", "?1"),
            " AND s.id IN (SELECT structure FROM structure_cells WHERE ({cells}) AND ",
            valid_in!("", "?1"),
            ") AND (?3 IS NULL OR s.kind IN (SELECT value FROM json_each(?3))) \
             AND (?4 IS NULL OR s.scale_domain IN (SELECT value FROM json_each(?4))) \
             ORDER BY s.id LIMIT {chunk}"
        ),
        cells = cells.join(" OR "),
        chunk = CHUNK,
    );
    let mut regions: HashMap<String, Option<Region>> = HashMap::new();
    let mut from = after.unwrap_or_default().to_owned();
    let mut found = Vec::new();
    loop {
        let params = [
            json!(query.release),
            json!(from),
            json!(query.filters.kinds),
            json!(query.filters.scale_domains),
        ];
        let rows = rows_as::<Row>(backend.releases(&sql, &params).await?)?;
        let read = rows.len();
        for row in rows {
            from.clone_from(&row.id);
            let region = *regions
                .entry(row.scale_domain.clone())
                .or_insert_with(|| query.region_in_domain(&row.scale_domain));
            if let Some(matched_by) = meets(&query.region, region, &row)? {
                let hit = StructureHit {
                    id: row.id.clone(),
                    name: row.name,
                    kind: row.kind,
                    scale_domain: row.scale_domain,
                    matched_by,
                };
                found.push((After::Structure { id: row.id }, QueryResult::Structure(hit)));
                if found.len() >= want {
                    return Ok(found);
                }
            }
        }
        if read < CHUNK {
            return Ok(found);
        }
    }
}

/// How the structure of `row` meets the volume, by the first of its representations the
/// release holds; none when that one does not meet it, or it has none. `in_mpc` is the volume
/// in megaparsecs, `in_domain` in the structure's scale domain's unit, the unit of its box and
/// centre.
///
/// # Errors
///
/// A 500 for a representation the release holds that does not read.
fn meets(
    in_mpc: &Region,
    in_domain: Option<Region>,
    row: &Row,
) -> Result<Option<MatchedBy>, Problem> {
    let shape_meets = |shape: &Shape| in_mpc.meets(Unit::Megaparsec, shape) == Some(true);
    if let Some(text) = &row.shape {
        let shape: Shape = parse(&row.id, "placed shape", text)?;
        return Ok(shape_meets(&shape).then_some(MatchedBy::Shape));
    }
    if let Some(text) = &row.members
        && let Some(shape) = parse::<Members>(&row.id, "member list", text)?.shape
    {
        return Ok(shape_meets(&shape).then_some(MatchedBy::Members));
    }
    let Some(region) = in_domain else {
        return Ok(None);
    };
    if let Some(text) = &row.bounds {
        let bounds: Aabb = parse(&row.id, "bounding box", text)?;
        return Ok(region.meets_box(&bounds).then_some(MatchedBy::BoundingBox));
    }
    Ok(match (row.centre_x, row.centre_y, row.centre_z) {
        (Some(x), Some(y), Some(z)) => region.contains([x, y, z]).then_some(MatchedBy::Centre),
        _ => None,
    })
}

/// Column `what` of structure `id`, read from its JSON.
fn parse<T: serde::de::DeserializeOwned>(id: &str, what: &str, text: &str) -> Result<T, Problem> {
    serde_json::from_str(text).map_err(|e| broken(&format!("The {what} of structure {id:?}"), e))
}
