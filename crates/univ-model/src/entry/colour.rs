//! What a source publishes of a star's colour: its colour indices, and its spectral type as the
//! text the source gives.

use serde::{Deserialize, Serialize};

use crate::value::{KindOfValue, Value};

/// A colour index: the magnitude in the first band minus that in the second, such as B−V or
/// BP−RP, normalized to magnitudes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColourIndex {
    /// The two bands, as the source definition names them, the one subtracted from second.
    pub bands: [String; 2],
    pub value: Value,
    /// The two magnitude columns it was computed from, where its source publishes the
    /// magnitudes and not the colour; none for a colour index the source publishes.
    pub derived_from: Option<[String; 2]>,
}

impl ColourIndex {
    /// The bands as one name, such as `B-V`.
    #[must_use]
    pub fn band_pair(&self) -> String {
        format!("{}-{}", self.bands[0], self.bands[1])
    }
}

/// A spectral type, character for character as its source publishes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpectralType {
    pub text: String,
    /// Measured where the source classified the star's spectrum, estimated where it fitted a
    /// model to it.
    pub kind: KindOfValue,
}
