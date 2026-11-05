//! An entry's quality columns, by column name, as its source publishes them.
//!
//! An entry is held for each of tens of millions of rows, most with one or two flags, such as
//! DESI's `spectype`. A map would allocate a node of several hundred bytes and a copy of each
//! name for every entry. Here the flags are one sorted slice, and each name is shared by every
//! entry that carries it: normalization names its columns once per table and passes the names
//! in ([`Flags::named`]); any other name goes through one table of the names seen ([`name`]).

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use serde::{Deserialize, Serialize, Serializer};

/// Every flag name seen, each held once.
static NAMES: LazyLock<Mutex<HashSet<Arc<str>>>> = LazyLock::new(Mutex::default);

/// The shared copy of the flag name `text`.
#[must_use]
pub fn name(text: &str) -> Arc<str> {
    let mut names = NAMES.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(held) = names.get(text) {
        return Arc::clone(held);
    }
    let held: Arc<str> = Arc::from(text);
    names.insert(Arc::clone(&held));
    held
}

/// An entry's flags, sorted by name, each name at most once.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(from = "BTreeMap<String, String>")]
pub struct Flags(Box<[(Arc<str>, String)]>);

impl Flags {
    /// The flags `pairs` give, their names already shared; of two of one name, the later, as a
    /// map's `insert` keeps it.
    #[must_use]
    pub fn named(pairs: impl IntoIterator<Item = (Arc<str>, String)>) -> Self {
        let mut flags: Vec<(Arc<str>, String)> = pairs.into_iter().collect();
        // Reversed, then sorted stably, the later of two of one name comes first and is kept.
        flags.reverse();
        flags.sort_by(|a, b| a.0.cmp(&b.0));
        flags.dedup_by(|later, kept| later.0 == kept.0);
        Self(flags.into_boxed_slice())
    }

    /// The value of the flag `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        let at = self.0.binary_search_by(|(n, _)| (**n).cmp(name)).ok()?;
        Some(&self.0[at].1)
    }

    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Each flag's name and value, by name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(n, v)| (&**n, v.as_str()))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl<K: AsRef<str>, V: Into<String>> FromIterator<(K, V)> for Flags {
    /// The flags `pairs` give, each name shared through [`name`]; see [`Flags::named`].
    fn from_iter<I: IntoIterator<Item = (K, V)>>(pairs: I) -> Self {
        Self::named(pairs.into_iter().map(|(k, v)| (name(k.as_ref()), v.into())))
    }
}

impl From<BTreeMap<String, String>> for Flags {
    fn from(map: BTreeMap<String, String>) -> Self {
        map.into_iter().collect()
    }
}

impl Serialize for Flags {
    /// As a map, by name: the JSON object a release's `flags` column holds.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_read_as_a_map_and_share_their_names() {
        let a: Flags = [("spectype", "GALAXY"), ("q_z", "4"), ("q_z", "5")]
            .into_iter()
            .collect();
        assert_eq!(a.len(), 2);
        assert_eq!(a.get("q_z"), Some("5"), "the later of two");
        assert_eq!(a.get("spectype"), Some("GALAXY"));
        assert_eq!(a.get("absent"), None);
        assert!(a.contains_key("spectype") && !a.contains_key("absent"));
        let names: Vec<&str> = a.iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["q_z", "spectype"]);
        // Two entries' flags share one copy of a name.
        let b: Flags = [("spectype", "QSO")].into_iter().collect();
        assert!(Arc::ptr_eq(&a.0[1].0, &b.0[0].0));
        // Names shared beforehand are kept as they are.
        let column: Arc<str> = Arc::from("spectype");
        let c = Flags::named([(Arc::clone(&column), "STAR".to_owned())]);
        assert!(Arc::ptr_eq(&c.0[0].0, &column));
        // The JSON is a map's, by name.
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            r#"{"q_z":"5","spectype":"GALAXY"}"#
        );
        let back: Flags = serde_json::from_str(r#"{"spectype":"GALAXY","q_z":"5"}"#).unwrap();
        assert_eq!(back, a);
    }
}
