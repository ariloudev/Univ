//! A release's structures (read-a-structure): voids, walls, filaments, superclusters, groups,
//! clusters and basins of attraction, each number cited.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::derive::Step;
use crate::geometry::{Aabb, Shape};
use crate::position::{BARYCENTRE, Frame, ICRS_CARTESIAN_NAME, Vec3};
use crate::units::{Unit, scale_domain_unit};

/// A page of a release's structures, by id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StructurePage {
    /// The release the page was read from; every page of one listing reads the same release.
    pub release: u32,
    /// The structures of this page, ordered by id.
    pub structures: Vec<StructureSummary>,
    /// The cursor of the next page, while structures remain.
    pub next: Option<String>,
}

/// A structure as a listing gives it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StructureSummary {
    pub id: String,
    pub name: String,
    /// Its kind, such as `void`, `wall` or `supercluster`.
    pub kind: String,
    pub scale_domain: String,
    /// The centre of its geometry; absent when it cannot be placed.
    pub centre: Option<Centre>,
}

/// A point placed by the release: ICRS equatorial Cartesian about the Solar System barycentre,
/// in its scale domain's unit, under the release cosmology, at epoch [`STRUCTURE_EPOCH`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Centre {
    /// "ICRS equatorial Cartesian", as the tile manifests name it.
    pub frame: String,
    /// "Solar System barycentre".
    pub origin: String,
    /// The scale domain's unit ([`scale_domain_unit`]): `parsec` for stars, `megaparsec`
    /// otherwise, named as a shape names its unit.
    pub unit: Unit,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// The epoch of the position, a Julian year: always [`STRUCTURE_EPOCH`], J2000.0.
    pub epoch: f64,
}

/// The epoch every structure's centre is given at, a Julian year: J2000.0, a convention.
///
/// No structure file gives an epoch. Its papers publish extragalactic positions, whose proper
/// motions no epoch could change measurably, in FK4 B1950, FK5 J2000, supergalactic or
/// equatorial coordinates of an unstated equinox (placed as J2000), and each is moved to the
/// ICRS, whose axes match FK5 J2000's. The centre is therefore stated at J2000.0; any other
/// epoch would move it by nothing measurable.
pub const STRUCTURE_EPOCH: f64 = 2000.0;

impl Centre {
    /// The centre `at` of a structure in the scale domain named `scale_domain`: ICRS Cartesian
    /// about the barycentre, in that domain's unit, at [`STRUCTURE_EPOCH`].
    #[must_use]
    pub fn placed(scale_domain: &str, at: Vec3) -> Self {
        Self {
            frame: ICRS_CARTESIAN_NAME.to_owned(),
            origin: BARYCENTRE.to_owned(),
            unit: scale_domain_unit(scale_domain),
            x: at[0],
            y: at[1],
            z: at[2],
            epoch: STRUCTURE_EPOCH,
        }
    }
}

/// One structure in full.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StructureDetail {
    pub release: u32,
    pub id: String,
    pub name: String,
    pub kind: String,
    /// `observed`, `reconstructed`, `statistical_claim` or `model`: how the structure is known.
    pub status: String,
    pub scale_domain: String,
    /// The structure as its researched file records it, verbatim: each published number with
    /// its kind of value, its citation (paper and table, figure or sentence), and the geometry
    /// in the frames the papers used.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub record: Value,
    pub centre: Option<Centre>,
    /// The placed geometry's axis-aligned bounds, in the centre's frame and unit.
    pub bounds: Option<Aabb>,
    /// The placed geometry, one of univ-model's geometry kinds, in a named frame: an admitted
    /// outline keeps its grid's frame; every other structure is in ICRS.
    pub shape: Option<Shape>,
    /// How the placed numbers were derived from the published ones.
    pub derivation: Option<Derivation>,
    /// Its members placed whole: objects of the release, or tracers with their own positions
    /// and kinds.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub members: Option<Value>,
    /// For a structure outlined from a volume grid: the grid, the threshold, the test and its
    /// verdict.
    pub outline: Option<OutlineRecord>,
    /// Each size published under another cosmology, as published and rescaled to the release
    /// cosmology.
    pub rescaled: Vec<RescaledLength>,
}

/// The test of a structure outlined from a volume grid, as `structure_details.outline` records
/// it: the grid, field, threshold and test its structure file states, the verdict and its
/// reason, the region's size and each check the test ran. An admitted outline is the
/// structure's placed shape; a refused one, or one not drawn, leaves it at its published
/// position.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct OutlineRecord {
    /// The volume grid's source id, such as `cf4pp-density`.
    pub grid: String,
    /// The grid field the threshold applies to, such as `density`.
    pub field: String,
    /// The threshold as the structure file records it: a cited value, or a choice of the
    /// pipeline with the reason for it.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub threshold: Value,
    /// The test as the structure file records it: its catalogues, rule, ratio and reasons.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub test: Value,
    pub verdict: OutlineVerdict,
    /// Why the outline was refused or not drawn; none when admitted.
    pub reason: Option<String>,
    /// The cells of the region on the grid; 0 when none was drawn.
    pub cells: u64,
    /// Whether the region reaches a face of the grid, which may cut it.
    pub touches_edge: bool,
    /// Each catalogue's comparison with the region: its coverage, galaxies held and expected,
    /// and whether it passes.
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<Object>))]
    pub checks: Vec<Value>,
    /// What the check of the dense places found, when the test states them.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub dense: Option<Value>,
}

/// Whether an outline's test admitted it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum OutlineVerdict {
    Admitted,
    Refused,
}

/// A length a structure file publishes under another cosmology, as published beside the length
/// rescaled to the release cosmology: an item of `structure_details.rescaled`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RescaledLength {
    /// A JSON pointer (RFC 6901) to the length in the structure's record.
    pub value: String,
    pub native: NativeLength,
    pub rescaled: Length,
    /// The rescaled length over the native one.
    pub factor: f64,
    pub method: RescalingMethod,
    /// The redshift of the comoving ratio; none for the ratio of Hubble constants.
    pub redshift: Option<f64>,
}

/// A length as published, with the cosmology it was published for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NativeLength {
    pub value: f64,
    pub unit: Unit,
    /// The Hubble constant it assumed, km/s/Mpc: 100 for a length in h⁻¹ Mpc.
    pub hubble_constant: f64,
    /// The cosmology as the structure file states it, its words and citation; none for a length
    /// in h⁻¹ Mpc whose file states none.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub assumed: Option<Value>,
}

/// A length under the release cosmology.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Length {
    pub value: f64,
    pub unit: Unit,
}

/// How a length was rescaled to the release cosmology.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RescalingMethod {
    /// By the ratio of the Hubble constants, as a length inferred from a velocity scales.
    HubbleRatio,
    /// By the ratio of the comoving distances of the structure's redshift in the two
    /// cosmologies.
    ComovingRatio,
}

/// How a structure's placed shape was derived from its published numbers:
/// `structure_details.derivation`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Derivation {
    /// JSON pointers (RFC 6901) to the cited values in the structure's record that the shape
    /// comes from.
    pub from: Vec<String>,
    /// Each move of frame, cited as an object's derivation cites it.
    pub frame_chain: Vec<Step>,
    /// How its distance was found.
    pub distance: DistanceDerivation,
    /// The cosmology it is placed under: always `release`.
    pub cosmology: String,
    /// For a structure placed as its admitted grid outline: the grid, the factor and the frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<GridPlacement>,
    /// Where its papers give the shape as a lower limit on the structure's extent, such as
    /// "at least 25° × 20°": a JSON pointer to that citation in the record. Absent when they do
    /// not, so the structure is known to reach as far as its shape and no further.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_least: Option<String>,
    /// For a structure placed at a point whose papers give its extent along each axis of a frame
    /// but not where it ends: those extents about the point. Its bounds stay the point's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extents: Option<Extents>,
}

/// A structure's published extents along the axes of the frame they were published in, about
/// its point, where its papers do not say where it ends. The point need not be its middle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Extents {
    /// The frame whose axes the extents were published along.
    pub frame: Frame,
    /// Each axis's whole extent as a vector in the ICRS, in the shape's unit, in the order of
    /// the frame's axes: a line through the point runs half of it each way.
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<Vec<f64>>, min_items = 3, max_items = 3))]
    pub axes: [Vec3; 3],
    /// JSON pointers to the cited values each axis comes from, in the same order.
    pub from: Vec<String>,
    /// How each published extent became a length.
    pub method: String,
}

/// Where a placed shape's distance comes from, and how it was found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DistanceDerivation {
    pub from: String,
    pub method: String,
}

/// How a grid outline becomes a placed shape: its cells kept in the grid's frame, its origin and
/// cell edge turned from the grid's unit into the shape's by one factor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GridPlacement {
    /// The volume grid the outline was drawn on.
    pub grid: String,
    /// The grid's frame, which the shape keeps.
    pub frame: Frame,
    /// The grid's unit, such as h⁻¹ Mpc.
    pub native_unit: Unit,
    /// The shape's unit.
    pub unit: Unit,
    /// The factor from the grid's unit to the shape's: 100 / H0 of the release for h⁻¹ Mpc.
    pub factor: f64,
    /// How the structure's centre is found from the shape.
    pub centre: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derive::Transformation;

    #[test]
    fn a_derivation_with_its_frame_steps_reads_back_as_written() {
        let derivation = Derivation {
            from: vec!["/position/lon".into()],
            frame_chain: vec![
                Transformation::EquinoxAssumedJ2000 {
                    systematic_degrees: 0.7,
                }
                .into(),
                Transformation::ToIcrs {
                    from: Frame::EquatorialEquinoxUnstated,
                }
                .into(),
            ],
            distance: DistanceDerivation {
                from: "the published distance".into(),
                method: "the length as published".into(),
            },
            cosmology: "release".into(),
            grid: None,
            at_least: None,
            extents: None,
        };
        let text = serde_json::to_string(&derivation).unwrap();
        assert!(!text.contains("\"grid\""), "{text}");
        assert!(!text.contains("\"at_least\""), "{text}");
        assert!(!text.contains("\"extents\""), "{text}");
        let step = &serde_json::from_str::<Value>(&text).unwrap()["frame_chain"][1];
        assert_eq!(step["step"], "to_icrs");
        assert_eq!(step["from"], "equatorial_equinox_unstated");
        let read: Derivation = serde_json::from_str(&text).unwrap();
        assert_eq!(read, derivation);
    }

    #[test]
    fn a_centre_names_its_frame_unit_and_epoch() {
        let star = serde_json::to_value(Centre::placed("stars", [1.0, 2.0, 3.0])).unwrap();
        assert_eq!(star["frame"], "ICRS equatorial Cartesian");
        assert_eq!(star["origin"], "Solar System barycentre");
        assert_eq!(star["unit"], "parsec");
        assert_eq!(star["epoch"], 2000.0);
        let void = Centre::placed("cosmic_web", [0.0; 3]);
        assert_eq!(serde_json::to_value(&void).unwrap()["unit"], "megaparsec");
        assert_eq!(void.unit, Unit::Megaparsec);
    }
}
