//! The repository holds a valid definition for each source (RQ-5).

mod common;

use common::{sources_dir, workspace};
use univ_ingest::source::{RedistributionStatus, ScaleDomain, check_structures, read_all};
use univ_ingest::structures::read_all_named;

/// The sources by id, with their scale domains: those of the first release, the Cosmicflows-4
/// galaxies added for the second, the Hipparcos spectral types added for the eighth, and the
/// galaxy surveys added for the tenth, the Galaxy tracers of the thirteenth and the luminous hot
/// stars of the fifteenth.
const SOURCES: [(&str, ScaleDomain); 37] = [
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
    ("edenhofer-2024-dust", ScaleDomain::Stars),
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
    // Release 13's tracers of the Galaxy, each naming its kind of object.
    ("hunt-reffert-2023", ScaleDomain::GalaxyTracers),
    ("skowron-2025-cepheids", ScaleDomain::GalaxyTracers),
    ("li-2023-rr-lyrae", ScaleDomain::GalaxyTracers),
    ("reid-2019-masers", ScaleDomain::GalaxyTracers),
    ("baumgardt-vasiliev-2021", ScaleDomain::GalaxyTracers),
    // Release 15's luminous hot stars, to show the Galaxy's spiral arms.
    ("zari-2021-luminous-hot-stars", ScaleDomain::GalaxyTracers),
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

#[test]
fn every_membership_names_a_structure_of_the_repository() {
    let definitions = read_all(&sources_dir()).unwrap();
    let structures = read_all_named(&workspace().join("structures")).unwrap();
    check_structures(&definitions, structures.iter().map(|(id, _)| id.as_str()))
        .unwrap_or_else(|e| panic!("{e}"));
    let masers = definitions
        .iter()
        .find(|d| d.id.as_str() == "reid-2019-masers")
        .unwrap();
    // The eleven codes of Reid et al. 2019's Table 1 on the seven arms of their Table 2.
    let named: std::collections::BTreeSet<&str> = masers.memberships[0]
        .structures
        .values()
        .map(String::as_str)
        .collect();
    assert_eq!(
        (masers.memberships[0].structures.len(), named.len()),
        (11, 7)
    );
}
