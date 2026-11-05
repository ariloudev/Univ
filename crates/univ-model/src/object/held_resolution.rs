//! How an entry came to belong to its object, as an object holds it for each of its entries.
//!
//! An entry is held for each of tens of millions of rows, and nearly every one is its object's
//! anchor. A [`Resolution`] is 96 bytes, most of them for a match or a cross-identification, so
//! an anchor is held as nothing and any other resolution boxed: 8 bytes an entry.

use std::ops::Deref;

use serde::{Deserialize, Serialize, Serializer};

use super::Resolution;

/// The anchor every [`HeldResolution`] that holds nothing reads as.
const ANCHOR: &Resolution = &Resolution::Anchor;

/// A [`Resolution`], read through [`Deref`] and serialized as one.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(from = "Resolution")]
pub struct HeldResolution(Option<Box<Resolution>>);

impl Deref for HeldResolution {
    type Target = Resolution;

    fn deref(&self) -> &Resolution {
        self.0.as_deref().unwrap_or(ANCHOR)
    }
}

impl From<Resolution> for HeldResolution {
    fn from(resolution: Resolution) -> Self {
        match resolution {
            Resolution::Anchor => Self(None),
            other => Self(Some(Box::new(other))),
        }
    }
}

impl PartialEq<Resolution> for HeldResolution {
    fn eq(&self, resolution: &Resolution) -> bool {
        **self == *resolution
    }
}

impl Serialize for HeldResolution {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (**self).serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{Designation, SourceId};
    use crate::object::Confidence;

    #[test]
    fn an_anchor_is_held_as_nothing_and_reads_and_serializes_as_a_resolution() {
        assert_eq!(std::mem::size_of::<HeldResolution>(), 8);
        let anchor = HeldResolution::from(Resolution::Anchor);
        assert_eq!(anchor, HeldResolution::default());
        assert_eq!(anchor, Resolution::Anchor);
        let via = Designation::new(SourceId::new("hipparcos-2007").unwrap(), "1").unwrap();
        let cross = Resolution::CrossIdentification {
            scheme: "hip".to_owned(),
            identifier: "1".to_owned(),
            via,
            confidence: Confidence::new(0.9).unwrap(),
        };
        let held = HeldResolution::from(cross.clone());
        assert_eq!(*held, cross);
        for (resolved, resolution) in [(anchor, Resolution::Anchor), (held, cross)] {
            let json = serde_json::to_string(&resolved).unwrap();
            assert_eq!(json, serde_json::to_string(&resolution).unwrap());
            let back: HeldResolution = serde_json::from_str(&json).unwrap();
            assert_eq!(back, resolved);
        }
    }
}
