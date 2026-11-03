//! An object's candidate links, `objects.links`.
//!
//! A link's reason is a [`crate::object::Resolution`], tagged by `by`. Releases built before
//! that tag (up to release 6 of the local state) tagged it by `method`, so a match's reason
//! held `method` twice, the tag and then its match method: `{"method":"match",
//! "method":"position",...}`. A JSON reader keeps the last, the match method, so such a reason
//! reads as a match when it carries a match's `separation_arcsec`, and as the resolution its
//! `method` names otherwise.
//!
//! Only releases built before the `by` tag, by univ-ingest before commit 99f2ac7 ("tag a
//! resolution by \"by\""), wrote the old shape; every release built since writes the new one.
//! [`retag`] is a shim for those releases alone: it can go once no published release built
//! before that tag remains, every one of them pruned or rebuilt.

use serde::Deserialize;
use serde_json::Value;

use super::ReadError;
use super::columns::Columns;
use crate::object::CandidateLink;

/// A reason in the old shape, tagged by `method`, in the new one, tagged by `by`.
fn retag(reason: &mut Value) {
    let Value::Object(map) = reason else {
        return;
    };
    if map.contains_key("by") {
        return;
    }
    if map.contains_key("separation_arcsec") {
        map.insert("by".into(), Value::String("match".into()));
    } else if let Some(tag) = map.remove("method") {
        map.insert("by".into(), tag);
    }
}

/// The object's candidate links, none when the column is NULL.
pub(super) fn links(c: &Columns<'_>) -> Result<Vec<CandidateLink>, ReadError> {
    let Some(mut links) = c.json_opt::<Value>("links")? else {
        return Ok(Vec::new());
    };
    if let Value::Array(items) = &mut links {
        for link in items {
            if let Some(reason) = link.get_mut("reason") {
                retag(reason);
            }
        }
    }
    Vec::<CandidateLink>::deserialize(&links)
        .map_err(|e| c.error("links", format!("{links} does not read: {e}")))
}
