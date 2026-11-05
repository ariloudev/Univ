//! How an entry came to belong to its object, as an object holds it for each of its entries.
//!
//! An entry is held for each of tens of millions of rows, and nearly every one is its object's
//! anchor. A [`Resolution`] is 96 bytes, most of them for a match or a cross-identification, so
//! an anchor is held as nothing and any other resolution boxed: 8 bytes an entry.

use std::ops::Deref;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::Resolution;

/// The anchor every [`Resolved`] that holds nothing reads as.
const ANCHOR: &Resolution = &Resolution::Anchor;

/// A [`Resolution`], read through [`Deref`] and serialized as one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resolved(Option<Box<Resolution>>);

impl Deref for Resolved {
    type Target = Resolution;

    fn deref(&self) -> &Resolution {
        self.0.as_deref().unwrap_or(ANCHOR)
    }
}

impl From<Resolution> for Resolved {
    fn from(resolution: Resolution) -> Self {
        match resolution {
            Resolution::Anchor => Self(None),
            other => Self(Some(Box::new(other))),
        }
    }
}

impl PartialEq<Resolution> for Resolved {
    fn eq(&self, resolution: &Resolution) -> bool {
        **self == *resolution
    }
}

impl Serialize for Resolved {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (**self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Resolved {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Resolution::deserialize(deserializer).map(Self::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{Designation, SourceId};
    use crate::object::Confidence;

    #[test]
    fn an_anchor_is_held_as_nothing_and_reads_and_serializes_as_a_resolution() {
        assert_eq!(std::mem::size_of::<Resolved>(), 8);
        let anchor = Resolved::from(Resolution::Anchor);
        assert_eq!(anchor, Resolved::default());
        assert_eq!(anchor, Resolution::Anchor);
        let via = Designation::new(SourceId::new("hipparcos-2007").unwrap(), "1").unwrap();
        let cross = Resolution::CrossIdentification {
            scheme: "hip".to_owned(),
            identifier: "1".to_owned(),
            via,
            confidence: Confidence::new(0.9).unwrap(),
        };
        let held = Resolved::from(cross.clone());
        assert_eq!(*held, cross);
        for (resolved, resolution) in [(anchor, Resolution::Anchor), (held, cross)] {
            let json = serde_json::to_string(&resolved).unwrap();
            assert_eq!(json, serde_json::to_string(&resolution).unwrap());
            let back: Resolved = serde_json::from_str(&json).unwrap();
            assert_eq!(back, resolved);
        }
    }
}
