//! A release's sources, provenance, structures and scale domains as D1 rows.

use std::collections::BTreeMap;
use std::path::Path;

use univ_model::api::objects::MeasuredQuantity;
use univ_model::geometry::Aabb;
use univ_model::ids::SourceId;
use univ_model::release::keys;
use univ_model::release::rows::{
    CatalogueUncertainty, NormalizationRow, PayloadRow, ScaleDomainRow, SourceRow, SourceTermRow,
    StructureRow,
};

use super::objects::{json, snake};
use super::placement::Placement;
use super::terms::SourceIndex;
use crate::provenance::Provenance;
use crate::source::SourceDefinition;
use crate::tiles::{Built, name_of, unit_symbol};

/// The rows of a release's sources and their provenance.
#[derive(Debug, Default)]
pub struct SourceRows {
    pub sources: Vec<SourceRow>,
    pub payloads: Vec<PayloadRow>,
    pub normalizations: Vec<NormalizationRow>,
    /// One for each source whose terms limit it to non-commercial use, whose permission is
    /// pending, or that states an uncertainty once for its whole catalogue.
    pub terms: Vec<SourceTermRow>,
}

/// The uncertainties `d` states once for its whole catalogue, as `source_terms` holds them.
fn catalogue_uncertainties(d: &SourceDefinition) -> Vec<CatalogueUncertainty> {
    d.normalization
        .redshifts
        .iter()
        .filter_map(|r| r.catalogue_error.as_ref())
        .map(|c| CatalogueUncertainty {
            quantity: MeasuredQuantity::Redshift,
            error: c.clone(),
        })
        .collect()
}

/// The rows of `definitions` in release `release`, each numbered by `index` and
/// with the entries the release holds from it and the provenance kept in `cache`.
///
/// # Errors
///
/// Fails when a provenance cannot be read or a value cannot be written as JSON.
pub fn source_rows(
    release: u32,
    definitions: &[SourceDefinition],
    index: &mut SourceIndex,
    entries: &BTreeMap<SourceId, u64>,
    cache: &Path,
) -> Result<SourceRows, String> {
    let mut rows = SourceRows::default();
    for d in definitions {
        let n = index.n(&d.id);
        let id = d.id.to_string();
        rows.sources.push(SourceRow {
            release,
            n,
            id: id.clone(),
            title: d.title.clone(),
            provider: d.provider.clone(),
            scale_domain: name_of(d.scale_domain).to_owned(),
            citation: d.citation.clone(),
            terms_url: d.terms.url.clone(),
            terms_quote: d.terms.quote.clone(),
            credit: d.terms.credit.clone(),
            redistribution: d.redistribution.status.as_str().to_owned(),
            review: d.redistribution.review.as_str().to_owned(),
            entries: entries.get(&d.id).copied().unwrap_or(0),
            access: json(&d.access)?,
            cuts: json(&d.cuts)?,
            fixes: json(&d.fixes)?,
        });
        let permission = d.redistribution.permission.as_ref();
        let uncertainties = catalogue_uncertainties(d);
        if d.terms.non_commercial.is_some() || permission.is_some() || !uncertainties.is_empty() {
            rows.terms.push(SourceTermRow {
                release,
                source: n,
                non_commercial: d.terms.non_commercial.clone(),
                permission: permission.map(json).transpose()?,
                catalogue_uncertainties: (!uncertainties.is_empty())
                    .then(|| json(&uncertainties))
                    .transpose()?,
            });
        }
        let provenance = Provenance::read(&cache.join(d.id.as_str()))
            .map_err(|e| format!("{id}: its provenance cannot be read: {e}"))?;
        for p in &provenance.payloads {
            rows.payloads.push(PayloadRow {
                release,
                source: n,
                item: p.item.clone(),
                url: p.url.clone(),
                sha256: p.sha256.clone(),
                bytes: p.bytes,
                retrieved: p.retrieved.clone(),
                rows: p.rows,
            });
        }
        if let Some(report) = &provenance.normalization {
            rows.normalizations.push(NormalizationRow {
                release,
                source: n,
                published: report.published,
                left_out: json(&report.left_out)?,
                fixes: json(&report.fixes)?,
            });
        }
    }
    Ok(rows)
}

/// A structure's id and its ICRS bounding box in Mpc, or why it has none.
pub type StructureBounds = (String, Result<Aabb, String>);

/// The rows of the release's `structures`, each by its id, with its scale domain, its member
/// list as resolved, and its centre and bounds as `placed` ([`super::placement`]), both in its
/// scale domain's unit; and each structure's ICRS bounding box in Mpc, the box the cell index
/// holds, or why it has none.
///
/// # Errors
///
/// Fails when a value cannot be written as JSON.
pub fn structure_rows(
    release_number: u32,
    placed: &[Placement<'_>],
) -> Result<(Vec<StructureRow>, Vec<StructureBounds>), String> {
    let mut rows = Vec::with_capacity(placed.len());
    let mut bounds = Vec::with_capacity(placed.len());
    for p in placed {
        let s = p.structure;
        let members = p
            .members
            .and_then(|m| m.shape.as_ref().map(|shape| (m, shape)));
        rows.push(StructureRow {
            release: release_number,
            id: p.id.to_owned(),
            name: s.name.clone(),
            kind: snake(&s.kind),
            status: snake(&s.status),
            scale_domain: name_of(s.scale_domain).to_owned(),
            centre_x: p.centre.map(|c| c[0]),
            centre_y: p.centre.map(|c| c[1]),
            centre_z: p.centre.map(|c| c[2]),
            geometry_kind: s.geometry_kind().map(str::to_owned),
            record: json(s)?,
            members: members
                .map(|(m, shape)| {
                    json(&serde_json::json!({
                        "shape": shape,
                        "placed_from_file": m.from_file,
                    }))
                })
                .transpose()?,
            bounds: p.bounds.as_ref().map(json).transpose()?,
        });
        bounds.push((p.id.to_owned(), p.index.clone()));
    }
    Ok((rows, bounds))
}

/// Each scale domain's row, from its built tiles.
#[must_use]
pub fn domain_rows(release: u32, tiles: &Built) -> Vec<ScaleDomainRow> {
    tiles
        .domains
        .iter()
        .map(|d| {
            let name = name_of(d.domain);
            ScaleDomainRow {
                release,
                name: name.to_owned(),
                unit: unit_symbol(d.domain).to_owned(),
                radius: d.manifest.radius,
                points: d.points() as u64,
                nodes: d.nodes.len() as u64,
                manifest_key: keys::key(release, &keys::tile_manifest(name)),
            }
        })
        .collect()
}
