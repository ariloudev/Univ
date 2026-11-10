//! The repository holds a valid definition for each source (RQ-5).

mod common;

use common::sources_dir;
use univ_ingest::source::{RedistributionStatus, ScaleDomain, read_all};

/// The sources by id, with their scale domains: those of the first release, the Cosmicflows-4
/// galaxies added for the second, the Hipparcos spectral types added for the eighth, and the
/// galaxy surveys added for the tenth.
const SOURCES: [(&str, ScaleDomain); 30] = [
    ("sdss-dr17-galaxies", ScaleDomain::CosmicWeb),
    ("boss-dr17-great-wall", ScaleDomain::CosmicWeb),
    ("sixdfgs-dr3", ScaleDomain::CosmicWeb),
    ("twomrs", ScaleDomain::CosmicWeb),
    ("twompp-catalogue", ScaleDomain::CosmicWeb),
    ("cosmicflows-4", ScaleDomain::CosmicWeb),
    ("cosmicflows-4-galaxies", ScaleDomain::CosmicWeb),
    ("mcconnachie-2012", ScaleDomain::NearbyGalaxies),
    ("updated-nearby-galaxies", ScaleDomain::NearbyGalaxies),
    ("gcns", ScaleDomain::Stars),
    ("gaia-dr3-bright", ScaleDomain::Stars),
    ("bailer-jones-2021", ScaleDomain::Stars),
    ("gaia-dr3-hipparcos2", ScaleDomain::Stars),
    ("hipparcos-2007", ScaleDomain::Stars),
    ("hipparcos-1997", ScaleDomain::Stars),
    ("researched-star-distances", ScaleDomain::Stars),
    ("cf4pp-density", ScaleDomain::CosmicWeb),
    ("cf4-basins", ScaleDomain::CosmicWeb),
    ("twompp-reconstruction", ScaleDomain::CosmicWeb),
    ("erass1-clusters", ScaleDomain::CosmicWeb),
    ("researched-structures", ScaleDomain::CosmicWeb),
    // Release 10.
    ("desi-dr1", ScaleDomain::CosmicWeb),
    ("sdss-dr17-distant", ScaleDomain::CosmicWeb),
    ("twodfgrs", ScaleDomain::CosmicWeb),
    ("alfalfa-a100", ScaleDomain::CosmicWeb),
    // Release 10, permission pending.
    ("twompz", ScaleDomain::CosmicWeb),
    ("gama-dr4", ScaleDomain::CosmicWeb),
    ("wise-supercosmos", ScaleDomain::CosmicWeb),
    ("iau-star-names", ScaleDomain::Stars),
    ("openngc", ScaleDomain::NearbyGalaxies),
];

#[test]
fn every_source_has_a_valid_definition() {
    let definitions = read_all(&sources_dir()).unwrap_or_else(|e| panic!("{e}"));
    let ids: Vec<&str> = definitions.iter().map(|d| d.id.as_str()).collect();
    for (id, domain) in SOURCES {
        let d = definitions
            .iter()
            .find(|d| d.id.as_str() == id)
            .unwrap_or_else(|| panic!("no definition for {id}; found {ids:?}"));
        assert_eq!(d.scale_domain, domain, "{id}");
    }
    assert_eq!(definitions.len(), SOURCES.len(), "{ids:?}");
}

#[test]
fn no_source_is_forbidden() {
    for d in common::read_sources() {
        assert_ne!(
            d.redistribution.status,
            RedistributionStatus::Forbidden,
            "{}",
            d.id
        );
    }
}
