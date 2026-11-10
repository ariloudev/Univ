//! The chain of transformations behind a derived value: an ordered list of typed steps, each
//! with its parameters and the citation for what it does.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::cosmology::AssumedCosmology;
use crate::distance::Measure;
use crate::entry::RedshiftKind;
use crate::position::Frame;
use crate::redshift::DipoleModel;
use crate::value::Quantity;

/// One transformation and its parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", tag = "step")]
pub enum Transformation {
    /// The distance as its source published it, and which distance that is.
    Published { native: Quantity, measure: Measure },
    /// A distance modulus to a luminosity distance, `d = 10^(μ/5 + 1)` pc, its error carried
    /// to an asymmetric interval.
    FromModulus { modulus: Quantity },
    /// A length published for the Hubble constant of `assumed`, rescaled to the release's by
    /// `factor = H0(assumed) / H0(release)`, as a length inferred from a velocity scales as
    /// 1 / H0.
    HubbleRescaled {
        assumed: AssumedCosmology,
        release_hubble_constant: f64,
        factor: f64,
    },
    /// A length rescaled by the ratio of the comoving distances of `redshift` in the release
    /// cosmology and in the flat ΛCDM of `assumed`, which keeps a size in proportion to the
    /// distance of its structure at any redshift.
    ComovingRescaled {
        assumed: AssumedCosmology,
        redshift: f64,
        factor: f64,
    },
    /// A luminosity distance to a comoving one, `D_C = D_L / (1 + z)`, with the CMB-frame
    /// redshift of the same entry, spectroscopic or photometric.
    LuminosityToComoving {
        cmb_redshift: f64,
        redshift_kind: RedshiftKind,
    },
    /// The comoving distance of a CMB-frame redshift in the release cosmology.
    ComovingFromRedshift {
        cmb_redshift: f64,
        heliocentric_redshift: f64,
        dipole: DipoleModel,
    },
    /// The comoving distance of a CMB-frame redshift in the release cosmology, moved from a
    /// redshift its source published relative to the Solar System barycentre.
    ComovingFromBarycentricRedshift {
        cmb_redshift: f64,
        barycentric_redshift: f64,
        dipole: DipoleModel,
    },
    /// The comoving distance in the release cosmology of a redshift its source published in the
    /// CMB frame, used as published, with no dipole step.
    ComovingFromPublishedCmbRedshift { cmb_redshift: f64 },
    /// A parallax as its source published it.
    PublishedParallax { native: Quantity },
    /// A parallax zero point subtracted from a parallax: `ϖ − Z`, in mas.
    ParallaxZeroPoint {
        zero_point_mas: f64,
        corrected_mas: f64,
    },
    /// A parallax inverted to a distance, `d = 1 / ϖ`, its interval exactly
    /// `[1 / (ϖ + σ), 1 / (ϖ − σ)]`; an inferred distance.
    InvertedParallax { parallax_mas: f64 },
    /// A distance kept as published, with the reason; it is not a comoving distance.
    KeptAsPublished {
        measure: Measure,
        #[cfg_attr(feature = "openapi", schema(value_type = String))]
        reason: Cow<'static, str>,
    },
    /// A Euclidean distance taken as it is: the cosmological distances differ from it by about
    /// `H0 d / c`, the fraction given.
    Euclidean { largest_difference: f64 },
    /// A direction moved from its published frame to the ICRS.
    ToIcrs { from: Frame },
    /// A direction whose equinox is not stated, placed as J2000 with this systematic error.
    EquinoxAssumedJ2000 { systematic_degrees: f64 },
    /// Spherical to Cartesian in the ICRS, with the Solar System barycentre as origin.
    Cartesian,
    /// The ICRS Cartesian position rotated into another frame.
    Rotated { to: Frame },
}

/// A step of a chain: the transformation and its citation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Step {
    #[serde(flatten)]
    pub transformation: Transformation,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub citation: Cow<'static, str>,
}

/// A citation of Hogg 1999 at `$at`, a static string: a chain is held for millions of objects.
macro_rules! hogg {
    ($at:literal) => {
        concat!("Hogg 1999, arXiv:astro-ph/9905116", $at)
    };
}

impl From<Transformation> for Step {
    fn from(transformation: Transformation) -> Self {
        let citation: Cow<'static, str> = match &transformation {
            Transformation::Published { .. } | Transformation::KeptAsPublished { .. } => {
                "the source's own value".into()
            }
            Transformation::PublishedParallax { .. } => "the source's own parallax".into(),
            Transformation::ParallaxZeroPoint { .. } => {
                "Lindegren et al. 2021, A&A 649, A4, §1: \"In Gaia EDR3 quasars have a median \
                 parallax of about −17 µas\"; the zero point Z is subtracted, ϖ − Z"
                    .into()
            }
            Transformation::InvertedParallax { .. } => {
                "d = 1 / ϖ, a fair estimate where σ_ϖ / ϖ is small (Bailer-Jones 2015, PASP 127, \
                 994)"
                    .into()
            }
            Transformation::FromModulus { .. } => {
                "the distance modulus, μ = 5 log10(d / 10 pc)".into()
            }
            Transformation::HubbleRescaled { .. } => {
                hogg!(", §3: distances scale with the Hubble distance c / H0").into()
            }
            Transformation::ComovingRescaled { .. }
            | Transformation::ComovingFromRedshift { .. }
            | Transformation::ComovingFromBarycentricRedshift { .. }
            | Transformation::ComovingFromPublishedCmbRedshift { .. } => {
                hogg!(", eq. 15, flat ΛCDM").into()
            }
            Transformation::LuminosityToComoving { .. } => {
                hogg!(", eq. 16 and 21: D_L = (1 + z) D_M, D_M = D_C when flat").into()
            }
            Transformation::Euclidean { .. } => {
                hogg!(", eq. 21: D_L / D_C − 1 = z ≈ H0 d / c").into()
            }
            Transformation::ToIcrs { from } => frame_citation(*from).into(),
            Transformation::EquinoxAssumedJ2000 { .. } => {
                "the B1950 to J2000 rotation angle of Standish 1982 (A&A 115, 20)".into()
            }
            Transformation::Cartesian => {
                "x toward α = 0, z toward the north pole, origin at the barycentre".into()
            }
            Transformation::Rotated { to } => frame_citation(*to).into(),
        };
        Self {
            transformation,
            citation,
        }
    }
}

/// The definition each frame's transformation follows.
fn frame_citation(frame: Frame) -> &'static str {
    match frame {
        Frame::Icrs => "IAU 1997 Resolution B2 (the ICRS)",
        Frame::Galactic => "ESA 1997, The Hipparcos Catalogue, vol. 1, §1.5.3",
        Frame::Supergalactic => "Lahav et al. 2000, MNRAS 312, 166",
        Frame::Fk5J2000 => "Mignard & Froeschlé 2000, A&A 354, 732",
        Frame::Fk4B1950 => {
            "Standish 1982 (A&A 115, 20) and Aoki et al. 1983 (A&A 128, 263) as ERFA fk45z, zero \
             FK5 proper motion; then Mignard & Froeschlé 2000 (A&A 354, 732)"
        }
        Frame::EquatorialEquinoxUnstated => {
            "placed as FK5 J2000, then Mignard & Froeschlé 2000 (A&A 354, 732)"
        }
    }
}
