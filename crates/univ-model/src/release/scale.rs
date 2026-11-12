//! What each table of the release schema grows with, and so whether it may hold JSON.

/// What a table's rows grow with (keep-json-out-of-the-tables-that-grow-with-a-releases-objects):
/// the release itself, its sources, its structures, its transformation chains, or its objects
/// and catalogue entries, which run to tens of millions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Release,
    Source,
    Structure,
    Chain,
    Object,
    Entry,
}

impl Scale {
    /// The words the schema's `-- Scale:` line gives it.
    #[must_use]
    pub fn words(self) -> &'static str {
        match self {
            Self::Release => "per release",
            Self::Source => "per source",
            Self::Structure => "per structure",
            Self::Chain => "per chain",
            Self::Object => "per object",
            Self::Entry => "per catalogue entry",
        }
    }

    /// Whether a table of this scale grows with the release's objects or catalogue entries,
    /// which run to tens of millions: read only through an index.
    #[must_use]
    pub fn grows_with_objects(self) -> bool {
        matches!(self, Self::Object | Self::Entry)
    }

    /// Whether a table of this scale may hold a JSON column: only those bounded by the
    /// release's sources, structures and chains, never one that grows with its objects.
    #[must_use]
    pub fn allows_json(self) -> bool {
        !self.grows_with_objects()
    }
}
