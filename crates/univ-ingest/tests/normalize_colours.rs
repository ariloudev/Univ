//! The star sources' colour indices and spectral types (normalize-records): each read from its
//! recorded answer, every expected value copied from the fixture's text or worked from it as
//! the comment beside it says.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;

use common::{definition_of, entry, fixture_of, near, normalize_body, normalized_fixture};
use univ_model::entry::{CatalogueEntry, ColourIndex};
use univ_model::value::{KindOfValue, Uncertainty};

/// The colour of `e` between `bands`.
fn colour<'e>(e: &'e CatalogueEntry, bands: &str) -> &'e ColourIndex {
    e.colours.iter().find(|c| c.band_pair() == bands).unwrap()
}

#[test]
fn hipparcos_2_keeps_its_b_v_with_its_error_and_its_v_i_with_none_reported() {
    let n = normalized_fixture("hipparcos-2007");
    // HIP 1: B-V 0.482, e_B-V 0.025, V-I 0.550.
    let e = entry(&n, "1");
    let bv = colour(e, "B-V");
    assert_eq!(bv.bands, ["B".to_owned(), "V".to_owned()]);
    assert_eq!(bv.value.native.value, 0.482);
    assert_eq!(
        bv.value.native.uncertainty,
        Uncertainty::Symmetric { sigma: 0.025 }
    );
    assert_eq!(bv.value.kind, KindOfValue::Measured);
    assert_eq!(bv.derived_from, None);
    let vi = colour(e, "V-I");
    assert_eq!(vi.value.native.value, 0.55);
    assert_eq!(vi.value.native.uncertainty, Uncertainty::NoneReported);
    // HIP 3: B-V -0.019, a negative colour kept as published.
    assert_eq!(colour(entry(&n, "3"), "B-V").value.native.value, -0.019);
}

#[test]
fn gaia_keeps_its_bp_rp_and_its_esp_hs_type_as_estimated() {
    let n = normalized_fixture("gaia-dr3-bright");
    // Row 1: bp_rp 1.2496433, spectraltype_esphs K.
    let e = entry(&n, "139396191545402112");
    let bp_rp = colour(e, "BP-RP");
    assert_eq!(bp_rp.value.native.value, 1.249_643_3);
    assert_eq!(bp_rp.value.native.uncertainty, Uncertainty::NoneReported);
    assert_eq!(bp_rp.value.kind, KindOfValue::Measured);
    let t = e.spectral_type.as_ref().unwrap();
    assert_eq!((t.text.as_str(), t.kind), ("K", KindOfValue::Estimated));
    // Row 4: M.
    let m = entry(&n, "196955487820819968")
        .spectral_type
        .as_ref()
        .unwrap();
    assert_eq!(m.text, "M");
}

#[test]
fn gcns_derives_bp_rp_from_its_two_magnitudes_and_gives_none_where_they_are_blank() {
    let n = normalized_fixture("gcns");
    // Row 1: BPmag 11.3022, RPmag 10.2396, so BP-RP is 1.0626.
    let e = entry(&n, "530861741656374272");
    let c = colour(e, "BP-RP");
    assert!(
        near(c.value.native.value, 1.0626, 1e-12),
        "{}",
        c.value.native.value
    );
    assert_eq!(
        c.derived_from,
        Some(["BPmag".to_owned(), "RPmag".to_owned()])
    );
    assert_eq!(c.value.native.uncertainty, Uncertainty::NoneReported);
    // Row 3: BPmag and RPmag blank.
    assert_eq!(entry(&n, "530685166956474752").colours, Vec::new());
}

#[test]
fn the_hipparcos_catalogue_keeps_its_spectral_type_as_measured_with_its_source_flag() {
    let n = normalized_fixture("hipparcos-1997");
    assert_eq!((n.report.published, n.report.left_out_total()), (5, 0));
    // HIP 2: SpType K3V, r_SpType 4 (Michigan catalogue, vol. 4).
    let e = entry(&n, "2");
    let t = e.spectral_type.as_ref().unwrap();
    assert_eq!((t.text.as_str(), t.kind), ("K3V", KindOfValue::Measured));
    assert_eq!(e.flags.get("r_SpType").unwrap(), "4");
    assert!(e.position.is_none());
    // HIP 5: G8III.
    assert_eq!(entry(&n, "5").spectral_type.as_ref().unwrap().text, "G8III");
}

#[test]
fn an_esp_hs_tag_of_unknown_is_read_as_no_spectral_type() {
    let definition = definition_of("gaia-dr3-bright");
    let body = std::fs::read_to_string(fixture_of(&definition)).unwrap();
    // Row 2 as recorded, its tag K written as ESP-HS writes a star it gave no type.
    let row = ",9.485988,1.4611044,K";
    assert!(body.contains(row));
    let n = normalize_body(
        &definition,
        body.replace(row, ",9.485988,1.4611044,unknown").as_bytes(),
    );
    let e = entry(&n, "140437257257667584");
    assert_eq!(e.spectral_type, None);
    assert_eq!(
        colour(e, "BP-RP").value.native.value,
        1.461_104_4,
        "the rest of the row is kept"
    );
}
