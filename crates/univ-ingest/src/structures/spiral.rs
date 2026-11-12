//! A spiral arm as a published log-spiral fit, and the path computed from it
//! (record-a-researched-structure).
//!
//! Reid et al. 2019 (`ApJ` 885, 131, section 3) fit each arm with
//! `ln(R / R_kink) = −(β − β_kink) tan ψ`, `R` the Galactocentric radius at the Galactocentric
//! azimuth `β` in radians, "defined as 0 toward the Sun and increasing in the direction of
//! Galactic rotation", with the pitch angle `ψ_<` for azimuths at or below the kink and `ψ_>`
//! above it. A structure file gives the parameters of its arm's row of their Table 2, each
//! cited, beside the Galactic Centre's distance the fits assume and the Sun's height above the
//! plane; the path is computed here, so that no number of it is uncited.
//!
//! The path samples each whole degree of azimuth, and each end of its range. With the Galactic
//! Centre at `R0` from the Sun toward `l = 0°`, a point at `(R, β)` lies at `x = R0 − R cos β`
//! toward `l = 0°`, `y = R sin β` toward `l = 90°` (the direction of rotation at the Sun) and
//! `z = −Z☉` toward the north Galactic pole, the plane lying `Z☉` below the Sun: heliocentric
//! galactic Cartesian kiloparsecs.
//!
//! A spiral arm spans the range of azimuth its fit covers, the range of its masers. Its
//! extrapolation carries the same formula on from each end for up to
//! [`EXTRAPOLATION_DEGREES`], stopping at the last degree whose radius lies within
//! [`RADIUS_BAND_KPC`], and passes through the fitted range between: a choice of this project,
//! labelled as such, not the paper's (its Figure 1 extends some arms by hand).

use serde::{Deserialize, Serialize};
use univ_model::position::Vec3;
use univ_model::units::Unit;

use super::cited::Cited;
use super::geometry::{is_angle, is_length};

/// How far an extrapolation runs on beyond each end of its arm's fitted range, in degrees of
/// azimuth.
pub const EXTRAPOLATION_DEGREES: f64 = 90.0;

/// The Galactocentric radii an extrapolation stays within, in kpc: inside the bar's reach and
/// out to the disc's edge, where the model says nothing.
pub const RADIUS_BAND_KPC: (f64, f64) = (3.0, 16.0);

/// Whether a structure is the arm over its fitted range or its extrapolation beyond it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Extent {
    /// The range of azimuth the fit covers, and no more.
    Fitted,
    /// The fitted range and up to [`EXTRAPOLATION_DEGREES`] beyond each end.
    Extrapolated,
}

/// A log-spiral arm as its paper's table gives it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawSpiral")]
pub struct LogSpiral {
    pub extent: Extent,
    /// The azimuth of the kink, an angle.
    pub beta_kink: Cited,
    /// The Galactocentric radius at the kink, a length.
    pub r_kink: Cited,
    /// The pitch angle at azimuths at or below the kink, `ψ_<`.
    pub pitch_inside: Cited,
    /// The pitch angle at azimuths above the kink, `ψ_>`.
    pub pitch_outside: Cited,
    /// The intrinsic (Gaussian 1σ) width of the arm at the kink, a length; recorded, not drawn.
    pub width: Cited,
    /// The range of azimuth the fit covers, from the lower end to the upper.
    pub fitted: [Cited; 2],
    /// The Galactic Centre's distance from the Sun that the fit assumes, a length.
    pub r0: Cited,
    /// The Sun's height above the Galactic plane, a length.
    pub sun_height: Cited,
    /// The parameters in degrees and kiloparsecs, converted once when read.
    #[serde(skip)]
    numbers: Numbers,
}

/// A log spiral's parameters as plain numbers: azimuths in degrees, lengths in kpc.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Numbers {
    kink: f64,
    r_kink: f64,
    tan_inside: f64,
    tan_outside: f64,
    fitted: [f64; 2],
    r0: f64,
    sun_height: f64,
}

/// A log spiral as written, checked into [`LogSpiral`].
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSpiral {
    extent: Extent,
    beta_kink: Cited,
    r_kink: Cited,
    pitch_inside: Cited,
    pitch_outside: Cited,
    width: Cited,
    fitted: [Cited; 2],
    r0: Cited,
    sun_height: Cited,
}

/// `c` in `unit`.
fn in_unit(c: &Cited, unit: Unit) -> Result<f64, String> {
    c.quantity
        .to(unit)
        .map(|q| q.value)
        .map_err(|e| e.to_string())
}

impl TryFrom<RawSpiral> for LogSpiral {
    type Error = String;
    fn try_from(r: RawSpiral) -> Result<Self, String> {
        let [low, high] = &r.fitted;
        for (name, c) in [
            ("beta_kink", &r.beta_kink),
            ("pitch_inside", &r.pitch_inside),
            ("pitch_outside", &r.pitch_outside),
            ("fitted[0]", low),
            ("fitted[1]", high),
        ] {
            if !is_angle(c) {
                return Err(format!("a log spiral's {name} is an angle"));
            }
        }
        for (name, c) in [
            ("r_kink", &r.r_kink),
            ("width", &r.width),
            ("r0", &r.r0),
            ("sun_height", &r.sun_height),
        ] {
            if !is_length(c.unit()) || c.unit() == Unit::Megaparsec {
                return Err(format!(
                    "a log spiral's {name} is a length within the Galaxy"
                ));
            }
        }
        let tan = |c: &Cited| in_unit(c, Unit::Degree).map(|d| d.to_radians().tan());
        let numbers = Numbers {
            kink: in_unit(&r.beta_kink, Unit::Degree)?,
            r_kink: in_unit(&r.r_kink, Unit::Kiloparsec)?,
            tan_inside: tan(&r.pitch_inside)?,
            tan_outside: tan(&r.pitch_outside)?,
            fitted: [in_unit(low, Unit::Degree)?, in_unit(high, Unit::Degree)?],
            r0: in_unit(&r.r0, Unit::Kiloparsec)?,
            sun_height: in_unit(&r.sun_height, Unit::Kiloparsec)?,
        };
        let [low, high] = numbers.fitted;
        if low >= high {
            return Err(format!(
                "a log spiral's fitted range runs from its lower azimuth to its higher, not {low}° \
                 to {high}°"
            ));
        }
        if numbers.r_kink <= 0.0 || numbers.r0 <= 0.0 {
            return Err("a log spiral's radius at the kink and R0 are positive".to_owned());
        }
        Ok(Self {
            extent: r.extent,
            beta_kink: r.beta_kink,
            r_kink: r.r_kink,
            pitch_inside: r.pitch_inside,
            pitch_outside: r.pitch_outside,
            width: r.width,
            fitted: r.fitted,
            r0: r.r0,
            sun_height: r.sun_height,
            numbers,
        })
    }
}

impl LogSpiral {
    /// Each cited parameter but the fitted range, with its name.
    #[must_use]
    pub fn scalars(&self) -> [(&'static str, &Cited); 7] {
        [
            ("beta_kink", &self.beta_kink),
            ("r_kink", &self.r_kink),
            ("pitch_inside", &self.pitch_inside),
            ("pitch_outside", &self.pitch_outside),
            ("width", &self.width),
            ("r0", &self.r0),
            ("sun_height", &self.sun_height),
        ]
    }

    /// The fitted range, the one pair of bounds, with its name.
    #[must_use]
    pub fn pairs(&self) -> [(&'static str, &[Cited; 2]); 1] {
        [("fitted", &self.fitted)]
    }

    /// The Galactocentric radius at azimuth `beta` degrees, in kpc.
    #[must_use]
    pub fn radius_kpc(&self, beta: f64) -> f64 {
        let n = &self.numbers;
        let tan = if beta <= n.kink {
            n.tan_inside
        } else {
            n.tan_outside
        };
        n.r_kink * (-(beta - n.kink).to_radians() * tan).exp()
    }

    /// The azimuths the path samples, in degrees, in increasing order: each whole degree within
    /// its range and each end.
    #[must_use]
    pub fn azimuths(&self) -> Vec<f64> {
        let [low, high] = self.numbers.fitted;
        let mut out = vec![low];
        let mut beta = low.floor() + 1.0;
        while beta < high {
            out.push(beta);
            beta += 1.0;
        }
        out.push(high);
        if self.extent == Extent::Fitted {
            return out;
        }
        let inside =
            |beta: f64| (RADIUS_BAND_KPC.0..=RADIUS_BAND_KPC.1).contains(&self.radius_kpc(beta));
        let mut before = Vec::new();
        let mut beta = low.ceil() - 1.0;
        while low - beta <= EXTRAPOLATION_DEGREES && inside(beta) {
            before.push(beta);
            beta -= 1.0;
        }
        let mut beta = high.floor() + 1.0;
        while beta - high <= EXTRAPOLATION_DEGREES && inside(beta) {
            out.push(beta);
            beta += 1.0;
        }
        before.reverse();
        before.extend(out);
        before
    }

    /// The path in heliocentric galactic Cartesian kpc, one point at each of
    /// [`LogSpiral::azimuths`].
    #[must_use]
    pub fn path_galactic_kpc(&self) -> Vec<Vec3> {
        let (r0, z) = (self.numbers.r0, -self.numbers.sun_height);
        self.azimuths()
            .into_iter()
            .map(|beta| {
                let r = self.radius_kpc(beta);
                let b = beta.to_radians();
                [r0 - r * b.cos(), r * b.sin(), z]
            })
            .collect()
    }
}
