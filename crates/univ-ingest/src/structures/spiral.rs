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
use super::geometry::is_length;

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

impl TryFrom<RawSpiral> for LogSpiral {
    type Error = String;
    fn try_from(r: RawSpiral) -> Result<Self, String> {
        let s = Self {
            extent: r.extent,
            beta_kink: r.beta_kink,
            r_kink: r.r_kink,
            pitch_inside: r.pitch_inside,
            pitch_outside: r.pitch_outside,
            width: r.width,
            fitted: r.fitted,
            r0: r.r0,
            sun_height: r.sun_height,
        };
        for (name, c) in s.pairs() {
            let angle = matches!(name, "beta_kink" | "pitch_inside" | "pitch_outside")
                || name.starts_with("fitted");
            let fits = if angle {
                c.unit().factor_to(Unit::Degree).is_ok()
            } else {
                is_length(c.unit()) && c.unit() != Unit::Megaparsec
            };
            if !fits {
                let what = if angle {
                    "an angle"
                } else {
                    "a length within the Galaxy"
                };
                return Err(format!("a log spiral's {name} is {what}"));
            }
        }
        let [low, high] = s.fitted_degrees()?;
        if low >= high {
            return Err(format!(
                "a log spiral's fitted range runs from its lower azimuth to its higher, not {low}° \
                 to {high}°"
            ));
        }
        if s.r_kink_kpc()? <= 0.0 || s.r0_kpc()? <= 0.0 {
            return Err("a log spiral's radius at the kink and R0 are positive".to_owned());
        }
        Ok(s)
    }
}

/// `c` in `unit`.
fn in_unit(c: &Cited, unit: Unit) -> Result<f64, String> {
    c.unit()
        .factor_to(unit)
        .map(|f| c.value() * f)
        .map_err(|e| e.to_string())
}

impl LogSpiral {
    /// Each cited parameter with its name, `fitted[0]` and `fitted[1]` for the range.
    #[must_use]
    pub fn pairs(&self) -> Vec<(&'static str, &Cited)> {
        vec![
            ("beta_kink", &self.beta_kink),
            ("r_kink", &self.r_kink),
            ("pitch_inside", &self.pitch_inside),
            ("pitch_outside", &self.pitch_outside),
            ("width", &self.width),
            ("fitted[0]", &self.fitted[0]),
            ("fitted[1]", &self.fitted[1]),
            ("r0", &self.r0),
            ("sun_height", &self.sun_height),
        ]
    }

    fn fitted_degrees(&self) -> Result<[f64; 2], String> {
        Ok([
            in_unit(&self.fitted[0], Unit::Degree)?,
            in_unit(&self.fitted[1], Unit::Degree)?,
        ])
    }

    fn r_kink_kpc(&self) -> Result<f64, String> {
        in_unit(&self.r_kink, Unit::Kiloparsec)
    }

    fn r0_kpc(&self) -> Result<f64, String> {
        in_unit(&self.r0, Unit::Kiloparsec)
    }

    /// The Galactocentric radius at azimuth `beta` degrees, in kpc.
    ///
    /// # Errors
    ///
    /// Fails on a parameter in a unit that does not convert.
    pub fn radius_kpc(&self, beta: f64) -> Result<f64, String> {
        let kink = in_unit(&self.beta_kink, Unit::Degree)?;
        let pitch = if beta <= kink {
            &self.pitch_inside
        } else {
            &self.pitch_outside
        };
        let psi = in_unit(pitch, Unit::Degree)?.to_radians();
        Ok(self.r_kink_kpc()? * (-(beta - kink).to_radians() * psi.tan()).exp())
    }

    /// The azimuths the path samples, in degrees, in increasing order: each whole degree within
    /// its range and each end.
    ///
    /// # Errors
    ///
    /// As [`LogSpiral::radius_kpc`].
    pub fn azimuths(&self) -> Result<Vec<f64>, String> {
        let [low, high] = self.fitted_degrees()?;
        let mut out = vec![low];
        let mut beta = low.floor() + 1.0;
        while beta < high {
            out.push(beta);
            beta += 1.0;
        }
        out.push(high);
        if self.extent == Extent::Fitted {
            return Ok(out);
        }
        let inside = |beta: f64| -> Result<bool, String> {
            let r = self.radius_kpc(beta)?;
            Ok((RADIUS_BAND_KPC.0..=RADIUS_BAND_KPC.1).contains(&r))
        };
        let mut before = Vec::new();
        let mut beta = low.ceil() - 1.0;
        while low - beta <= EXTRAPOLATION_DEGREES && inside(beta)? {
            before.push(beta);
            beta -= 1.0;
        }
        let mut after = Vec::new();
        let mut beta = high.floor() + 1.0;
        while beta - high <= EXTRAPOLATION_DEGREES && inside(beta)? {
            after.push(beta);
            beta += 1.0;
        }
        before.reverse();
        before.extend(out);
        before.extend(after);
        Ok(before)
    }

    /// The path in heliocentric galactic Cartesian kpc, one point at each of
    /// [`LogSpiral::azimuths`].
    ///
    /// # Errors
    ///
    /// As [`LogSpiral::radius_kpc`].
    pub fn path_galactic_kpc(&self) -> Result<Vec<Vec3>, String> {
        let r0 = self.r0_kpc()?;
        let z = -in_unit(&self.sun_height, Unit::Kiloparsec)?;
        self.azimuths()?
            .into_iter()
            .map(|beta| {
                let r = self.radius_kpc(beta)?;
                let b = beta.to_radians();
                Ok([r0 - r * b.cos(), r * b.sin(), z])
            })
            .collect()
    }
}
