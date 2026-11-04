//! The layer items of a volume query: the current version of each item, not retracted, of the
//! layers the query reads (those the caller may read, not retired, holding current items, as
//! its cursor records them), whose shape meets the volume, and, under a time filter, with a
//! time record of its kind whose uncertainty interval meets the interval on TT
//! (filter-by-time). The box each item keeps gives the candidates, and an item with no box yet
//! (stored before boxes were kept) is a candidate everywhere; the shape decides.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};
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

/// What a volume query reads of a version: its content's title and shape.
#[derive(Deserialize)]
struct Version {
    content: Option<Placed>,
}

#[derive(Deserialize)]
struct Placed {
    title: String,
    geometry: Shape,
}

/// Up to `want` items of `layers` that meet the volume after item `after`, by layer and item.
pub(super) async fn page<B: Backend>(
    backend: &B,
    query: &Query,
    layers: &BTreeMap<String, u64>,
    after: Option<(&String, &String)>,
    want: usize,
) -> Result<Vec<(After, QueryResult)>, Problem> {
    if layers.is_empty() {
        return Ok(Vec::new());
    }
    let b = query.region.bounds();
    let sql = format!(
        "SELECT i.layer_id, i.id, i.version, v.content FROM items i \
         JOIN item_versions v ON v.layer_id = i.layer_id AND v.item_id = i.id \
         AND v.version = i.version \
         WHERE i.layer_id IN (SELECT value FROM json_each(?1)) AND i.retracted = 0 \
         AND (i.min_x IS NULL OR (i.max_x >= {:?} AND i.min_x <= {:?} AND i.max_y >= {:?} \
         AND i.min_y <= {:?} AND i.max_z >= {:?} AND i.min_z <= {:?})) \
         AND (i.layer_id > ?2 OR (i.layer_id = ?2 AND i.id > ?3)) \
         AND (?4 IS NULL OR EXISTS (SELECT 1 FROM item_times t WHERE t.layer_id = i.layer_id \
         AND t.item_id = i.id AND t.kind = ?4 AND t.latest >= ?5 AND t.earliest <= ?6)) \
         ORDER BY i.layer_id, i.id LIMIT {CHUNK}",
        b.min[0], b.max[0], b.min[1], b.max[1], b.min[2], b.max[2],
    );
    let mut params: Vec<Value> = vec![
        json!(layers.keys().collect::<Vec<_>>()),
        json!(after.map_or("", |(layer, _)| layer)),
        json!(after.map_or("", |(_, item)| item)),
        json!(query.time.map(|t| t.0)),
        json!(query.time.map(|t| t.1)),
        json!(query.time.map(|t| t.2)),
    ];
    let mut found = Vec::new();
    loop {
        let rows = rows_as::<Row>(backend.layers(&sql, &params).await?)?;
        let read = rows.len();
        for row in rows {
            (params[1], params[2]) = (json!(row.layer_id), json!(row.id));
            let version: Version = serde_json::from_str(&row.content).map_err(|e| {
                broken(&format!("Item {:?} of layer {:?}", row.id, row.layer_id), e)
            })?;
            let Some(placed) = version.content else {
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
