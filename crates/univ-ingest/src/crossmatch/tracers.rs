//! A member that no matched object carries, placed from the structure file itself: the sky
//! position and redshift its paper cites for it, as a tracer of the member table's kind.
//!
//! The members of the researched structures are quasars, Mg II absorbers, gamma-ray bursts and
//! galaxy clusters that the release's sources do not hold, so most match no object. Each is
//! placed where its own cited values put it, under the release cosmology:
//!
//! - its direction from its row's longitude and latitude, in the table's frame, moved to the
//!   ICRS;
//! - its redshift from its row (a `redshift` column, or a `velocity` column as cz), of the
//!   column's kind; or, for a table that gives none, such as the absorbers read from a figure
//!   of Lopez et al. 2022 and 2024, the structure's own cited redshift (`redshift` or
//!   `central_redshift`), the kind then estimated, since one redshift stands for every member;
//! - the redshift taken as heliocentric, as each paper's are (from SDSS spectra and the SDSS
//!   DR7 quasar catalogue for Clowes et al. 2012 and 2013 and Lopez et al. 2022 and 2024, from
//!   spectroscopy of the bursts for Balázs et al. 2015, and from the WHL clusters' SDSS
//!   redshifts for Bagchi et al. 2017), moved to the CMB frame toward the member with the
//!   Planck 2018 dipole, and made a comoving distance under the release cosmology.
//!
//! A distance a table gives under its paper's own cosmology stays in the file; the release
//! cosmology places the member. A member matched to an object is placed by its object, as
//! before; one that matches several, lies too far from its object, or names an unplaced object
//! is not placed from the file.

use serde::Serialize;
use univ_model::cosmology::SPEED_OF_LIGHT;
use univ_model::derive::Release;
use univ_model::position::{Vec3, scaled};
use univ_model::redshift::{CmbCorrection, RedshiftFrame};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty};

use crate::structures::{ColumnName, Members, Row, Structure};

/// The structure values that give its redshift, in the order they are looked for.
pub const STRUCTURE_REDSHIFTS: [&str; 2] = ["redshift", "central_redshift"];

/// A member placed from the structure file, recorded with the values that placed it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FromFile {
    pub member: String,
    /// The heliocentric redshift as cited.
    pub redshift: f64,
    /// Where it was cited: the member's row (its column's header) or a structure value.
    pub redshift_from: String,
    /// The redshift's kind of value: its column's, or estimated for the structure's.
    pub kind: KindOfValue,
    /// The redshift's uncertainty as cited: its column's, or the structure value's, such as
    /// the ±0.060 of the redshift slice the Giant Arc and the Big Ring were found in.
    pub redshift_uncertainty: Uncertainty,
    /// The comoving distances, Mpc, of the two ends of the distance's interval as the release
    /// derives it from the redshift: one sigma either side, the redshift's error and the
    /// peculiar-velocity term in quadrature, or a cited interval's bounds as bounds.
    pub distance_range_mpc: Option<[f64; 2]>,
    /// The redshift in the CMB frame, toward the member.
    pub cmb_redshift: f64,
    /// Its comoving distance under the release cosmology, Mpc.
    pub distance_mpc: f64,
}

/// What places members from their structure file: the release cosmology and the CMB dipole.
pub struct FilePlacer<'a> {
    pub release: &'a Release,
    pub correction: CmbCorrection,
}

impl<'a> FilePlacer<'a> {
    /// A placer under `release`, with `correction`, the run's dipole, the one the normalization
    /// uses.
    #[must_use]
    pub fn new(release: &'a Release, correction: &CmbCorrection) -> Self {
        Self {
            release,
            correction: *correction,
        }
    }

    /// The heliocentric redshift of `row` with where it was cited and its kind: its own, else
    /// the structure's.
    fn redshift(
        structure: &Structure,
        members: &Members,
        row: &Row,
    ) -> Option<(Quantity, String, KindOfValue)> {
        for (name, per) in [
            (ColumnName::Redshift, 1.0),
            (ColumnName::Velocity, SPEED_OF_LIGHT),
        ] {
            let Some(i) = members.column(name) else {
                continue;
            };
            let column = &members.columns[i];
            let value = row.values.get(i)?;
            let value = match name {
                ColumnName::Velocity => {
                    let kms = column.unit.factor_to(Unit::KilometrePerSecond).ok()?;
                    value * kms / per
                }
                _ => *value,
            };
            let z = Quantity::new(
                value,
                Unit::Dimensionless,
                column.uncertainty.scaled(1.0 / per),
            )
            .ok()?;
            return Some((
                z,
                format!("its row, column {:?}", column.header),
                column.kind,
            ));
        }
        STRUCTURE_REDSHIFTS.iter().find_map(|key| {
            let cited = structure.values.get(*key)?;
            (cited.quantity.unit == Unit::Dimensionless).then(|| {
                (
                    cited.quantity,
                    format!("the structure's value {key}"),
                    KindOfValue::Estimated,
                )
            })
        })
    }

    /// Where `row`, toward `direction` (an ICRS unit vector), lies under the release cosmology,
    /// with its record; none when no redshift is cited for it or it gives no distance.
    #[must_use]
    pub fn place(
        &self,
        structure: &Structure,
        members: &Members,
        row: &Row,
        direction: Vec3,
    ) -> Option<(Vec3, FromFile)> {
        let (z, redshift_from, kind) = Self::redshift(structure, members, row)?;
        let cmb = self
            .correction
            .apply(z, RedshiftFrame::Heliocentric, direction)
            .ok()?;
        let radial = self.release.radial_from_redshift(&cmb).ok()?;
        let d = radial.distance();
        let distance = d.value;
        let distance_range_mpc = match d.uncertainty {
            Uncertainty::Asymmetric { minus, plus }
            | Uncertainty::Interval { minus, plus }
            | Uncertainty::CredibleInterval { minus, plus, .. } => {
                Some([distance - minus, distance + plus])
            }
            _ => None,
        };
        let record = FromFile {
            member: row.name.clone(),
            redshift: z.value,
            redshift_from,
            kind,
            redshift_uncertainty: z.uncertainty,
            distance_range_mpc,
            cmb_redshift: cmb.value.value,
            distance_mpc: distance,
        };
        Some((scaled(direction, distance), record))
    }
}
