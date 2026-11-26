//! The numbers of text and the values written as published.
use super::*;

#[test]
fn a_sexagesimal_value_has_the_same_figures_whatever_parts_them() {
    assert_eq!(figures("18h12m02.6822s"), figures("18 12 02.6822"));
    assert_eq!(figures("\u{2212}18°40′25.002″"), ["-18", "40", "25.002"]);
    assert_ne!(figures("18h12m02.6822s"), figures("18 12 02.682"));
    // A full stop after the last figure ends the sentence, not the figure.
    assert_eq!(figures("at 18 12 02.6822."), figures("18h12m02.6822s"));
}

#[test]
fn numbers_are_whole_tokens_with_their_separators_and_signs() {
    assert_eq!(numbers("about 1650 km/s"), [1650.0]);
    assert!(!has_number("about 1650 km/s", 165.0));
    assert_eq!(numbers("16,000±4,500 km s-1"), [16000.0, 4500.0]);
    assert_eq!(
        numbers("cz ~ 18 000 km s-1 in 22 2dF fields"),
        [18000.0, 22.0]
    );
    assert_eq!(
        numbers("[11,000, \u{2212}6,000, 10,000]"),
        [11000.0, -6000.0, 10000.0]
    );
    assert_eq!(
        numbers("z = 0.068 (+0.003, -0.007)"),
        [0.068, 0.003, -0.007]
    );
    assert_eq!(
        numbers("Lacerta-2.4, U1.11, DR17, 2dF, h^-1 Mpc, 10^5"),
        [] as [f64; 0]
    );
    assert_eq!(numbers("G069.39+68.05 (z = 0.763)"), [0.763]);
    assert_eq!(
        numbers("a distance of $168^{+27}_{-15}$ pc"),
        [168.0, 27.0, -15.0]
    );
    assert!(!numbers("a contrast of 10^{-3} or 10^{+5}").contains(&10.0));
    assert_eq!(numbers("A2147/2152 cover 1/8 of it"), [1.0, 8.0]);
    assert_eq!(
        numbers("~300 - 400 Mpc, 12°, 5200,3000,4500"),
        [300.0, 400.0, 12.0, 5200.0, 3000.0, 4500.0]
    );
    assert_eq!(numbers("z = 0.14, 0.26 and 0.30."), [0.14, 0.26, 0.3]);
    assert_eq!(
        stated_numbers("Table 1 of the paper lists 38 quasars (Fig. 2)."),
        [38.0]
    );
    assert_eq!(
        stated_numbers("HIP 9640 is HD 12533 and Gaia source 4933453324590295000: 8.30 mas"),
        [8.3]
    );
}

#[test]
fn a_published_form_is_read_with_the_rounding_of_its_last_digit() {
    let p = published("16.7h").unwrap();
    assert!((p.value - 250.5).abs() < 1e-12 && (p.half_step - 0.75).abs() < 1e-12);
    let p = published("α = 03h19m").unwrap();
    assert!((p.value - 49.75).abs() < 1e-12);
    let p = published("δ = -50°02′").unwrap();
    assert!((p.value + 50.0 + 2.0 / 60.0).abs() < 1e-12);
    assert!((published("38◦45′").unwrap().value - 38.75).abs() < 1e-12);
    let p = published("~1 Gpc").unwrap();
    assert_eq!(
        (p.value, p.half_step, p.unit),
        (1000.0, 500.0, Some(Unit::Megaparsec))
    );
    assert_eq!(published("celestial South Pole").unwrap().value, -90.0);
    assert!(consistent(
        0.06,
        Unit::Dimensionless,
        &published("0.060").unwrap()
    ));
    assert!(!consistent(
        9999.0,
        Unit::Dimensionless,
        &published("1650").unwrap()
    ));
    assert!(consistent(135.0, Unit::Degree, &published("9h").unwrap()));
    assert!(contains_whole("R.A. = 13h25m.", "13h25m"));
    assert!(!contains_whole("about 1650 km/s", "165"));
    assert!(!contains_whole("about 2165 km/s", "165"));
    assert_eq!(published("2 x 10^5"), None);
}

#[test]
fn a_number_written_with_a_power_of_ten_is_read_with_its_uncertainty() {
    let thin =
        "\\entry{$\\Mthin$}{$3.5{\\small\\pm}1\\times 10^{10}\\Msun$, thin disk stellar mass}";
    assert_eq!(scaled_numbers(thin), [3.5e10, 1e10]);
    assert!(has_number(thin, 3.5e10) && has_magnitude(thin, 1e10));
    assert_eq!(scaled_numbers("a mass of 6 × 10^9 solar masses"), [6e9]);
    assert_eq!(
        scaled_numbers("$0.91\\pm0.07\\times10^{10}$"),
        [0.91e10, 0.07e10]
    );
    // LaTeX's plus-minus glued to its error.
    assert!(has_number("$2.0\\pm0.2$ kpc", 2.0) && has_magnitude("$46\\pm3$", 3.0));
    assert!(has_number("$M/L_V\\narreq1.4\\pm0.5$", 1.4));
    // A power alone states nothing.
    assert_eq!(scaled_numbers("a contrast of 10^{-3}"), Vec::<f64>::new());
}

#[test]
fn a_bound_on_a_magnitude_stands_for_both_ends_of_its_range() {
    for text in ["|b| < 10°", "|b| ≤ 10°"] {
        let p = published(text).unwrap();
        assert!(consistent(10.0, Unit::Degree, &p), "{text}");
        assert!(consistent(-10.0, Unit::Degree, &p), "{text}");
        assert!(!consistent(-9.0, Unit::Degree, &p), "{text}");
    }
    // A plain number keeps its sign.
    let ten = published("10°").unwrap();
    assert!(!consistent(-10.0, Unit::Degree, &ten));
}
