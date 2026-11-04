//! The layer items of a volume query: the current version of each item, not retracted, of a
//! layer not retired that the caller may read, whose shape meets the volume, and, under a time
//! filter, with a time record of its kind whose uncertainty interval meets the interval on TT
//! (filter-by-time). The box each item keeps gives the candidates; the shape decides.

use serde::Deserialize;
use serde_json::json;
use univ_model::api::layers::VersionBody;
use univ_model::api::query::{ItemHit, QueryResult};
use univ_model::geometry::Shape;
use univ_model::units::Unit;

use super::Query;
use super::cursor::After;
use crate::backend::{Backend, rows_as};
use crate::layers::broken;
use crate::problem::Problem;

/// The candidates read at once.
const CHUNK: usize = 1_000;

#[derive(Deserialize)]
struct Row {
    layer_id: String,
    id: String,
    version: u64,
    content: String,
}

/// What a volume query reads of an item's content.
#[derive(Deserialize)]
struct Placed {
    title: String,
    geometry: Shape,
}

/// Up to `want` items that meet the volume after item `after`, by layer and item.
pub(super) async fn page<B: Backend>(
    backend: &B,
    query: &Query,
    after: Option<(&str, &str)>,
    want: usize,
) -> Result<Vec<(After, QueryResult)>, Problem> {
    let bounds = query.region.bounds();
    let sql = format!(
        "SELECT i.layer_id, i.id, i.version, v.content FROM items i \
         JOIN layers l ON l.id = i.layer_id \
         JOIN item_versions v ON v.layer_id = i.layer_id AND v.item_id = i.id \
         AND v.version = i.version \
         WHERE i.retracted = 0 AND l.retired_at IS NULL \
         AND (l.visibility = 'public' OR l.key_id = ?1) \
         AND i.max_x >= {:?} AND i.min_x <= {:?} AND i.max_y >= {:?} AND i.min_y <= {:?} \
         AND i.max_z >= {:?} AND i.min_z <= {:?} \
         AND (i.layer_id > ?2 OR (i.layer_id = ?2 AND i.id > ?3)) \
         AND (?4 IS NULL OR i.layer_id IN (SELECT value FROM json_each(?4))) \
         AND (?5 IS NULL OR EXISTS (SELECT 1 FROM item_times t WHERE t.layer_id = i.layer_id \
         AND t.item_id = i.id AND t.kind = ?5 AND t.latest >= ?6 AND t.earliest <= ?7)) \
         ORDER BY i.layer_id, i.id LIMIT {CHUNK}",
        bounds.min[0], bounds.max[0], bounds.min[1], bounds.max[1], bounds.min[2], bounds.max[2],
    );
    let (kind, from, to) = match query.time {
        Some((kind, from, to)) => (json!(kind), json!(from), json!(to)),
        None => (json!(null), json!(null), json!(null)),
    };
    let (mut layer, mut item) = after.map_or((String::new(), String::new()), |(l, i)| {
        (l.to_owned(), i.to_owned())
    });
    let mut found = Vec::new();
    loop {
        let rows = backend
            .layers(
                &sql,
                &[
                    json!(query.key),
                    json!(layer),
                    json!(item),
                    json!(query.filters.layers),
                    kind.clone(),
                    from.clone(),
                    to.clone(),
                ],
            )
            .await?;
        let read = rows.len();
        for row in rows_as::<Row>(rows)? {
            (layer, item) = (row.layer_id.clone(), row.id.clone());
            let body: VersionBody = serde_json::from_str(&row.content).map_err(|e| {
                broken(&format!("Item {:?} of layer {:?}", row.id, row.layer_id), e)
            })?;
            let Some(placed) = body
                .content
                .and_then(|c| serde_json::from_value::<Placed>(c).ok())
            else {
                continue;
            };
            if query.region.meets(Unit::Megaparsec, &placed.geometry) == Some(true) {
                let hit = ItemHit {
                    layer: row.layer_id.clone(),
                    item: row.id.clone(),
                    version: row.version,
                    title: placed.title,
                };
                found.push((
                    After::Item {
                        layer: row.layer_id,
                        item: row.id,
                    },
                    QueryResult::Item(hit),
                ));
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
