//! A release's sources, provenance, structures and scale domains as D1 rows.

use std::collections::BTreeMap;
use std::path::Path;

use univ_model::derive::Release;
use univ_model::geometry::Aabb;
use univ_model::ids::SourceId;
use univ_model::release::keys;
use univ_model::release::rows::{
    NormalizationRow, PayloadRow, ScaleDomainRow, SourceRow, StructureRow,
};

use super::objects::{json, snake};
use super::place::{centre, index_bounds};
use crate::crossmatch::MemberPlacement;
use crate::provenance::Provenance;
use crate::source::{ScaleDomain, SourceDefinition};
use crate::structures::Structure;
use crate::tiles::{Built, name_of, unit_symbol};

/// The rows of a release's sources and their provenance.
#[derive(Debug, Default)]
pub struct SourceRows {
    pub sources: Vec<SourceRow>,
    pub payloads: Vec<PayloadRow>,
    pub normalizations: Vec<NormalizationRow>,
}

/// The rows of `definitions` in release `release`, each with the entries the release holds
/// from it and the provenance kept in `cache`.
///
/// # Errors
///
/// Fails when a provenance cannot be read or a value cannot be written as JSON.
pub fn source_rows(
    release: u32,
    definitions: &[SourceDefinition],
    entries: &BTreeMap<SourceId, u64>,
    cache: &Path,
) -> Result<SourceRows, String> {
    let mut rows = SourceRows::default();
    for d in definitions {
        let id = d.id.to_string();
        rows.sources.push(SourceRow {
            release,
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
        let provenance = Provenance::read(&cache.join(d.id.as_str()))
            .map_err(|e| format!("{id}: its provenance cannot be read: {e}"))?;
        for p in &provenance.payloads {
            rows.payloads.push(PayloadRow {
                release,
                source: id.clone(),
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
                source: id.clone(),
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

/// The rows of the release's `structures`, each by its id, with its scale domain and centre and
/// the member list `placements` resolved for it; and each structure's ICRS bounding box in Mpc under `release`, the box its
/// row records and the cell index holds (see [`super::place`]), or why it has none.
///
/// # Errors
///
/// Fails when a value cannot be written as JSON.
pub fn structure_rows(
    release_number: u32,
    structures: &[(String, Structure)],
    placements: &[MemberPlacement],
    release: &Release,
) -> Result<(Vec<StructureRow>, Vec<StructureBounds>), String> {
    let mut rows = Vec::with_capacity(structures.len());
    let mut bounds = Vec::with_capacity(structures.len());
    for (id, s) in structures {
        let placed = placements.iter().find(|p| p.structure == s.name);
        let shape = placed.and_then(|p| p.shape.as_ref());
        let boxed = index_bounds(s, placed, release);
        // In the domain's unit: pc for stars, Mpc otherwise.
        let per_mpc = if s.scale_domain == ScaleDomain::Stars {
            1e6
        } else {
            1.0
        };
        let at = centre(s, placed, release)
            .ok()
            .map(|c| c.map(|v| v * per_mpc));
        rows.push(StructureRow {
            release: release_number,
            id: id.clone(),
            name: s.name.clone(),
            kind: snake(&s.kind),
            status: snake(&s.status),
            scale_domain: name_of(s.scale_domain).to_owned(),
            centre_x: at.map(|c| c[0]),
            centre_y: at.map(|c| c[1]),
            centre_z: at.map(|c| c[2]),
            geometry_kind: s.geometry_kind().map(str::to_owned),
            record: json(s)?,
            members: shape.map(json).transpose()?,
            bounds: boxed.as_ref().ok().map(json).transpose()?,
        });
        bounds.push((id.clone(), boxed));
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
