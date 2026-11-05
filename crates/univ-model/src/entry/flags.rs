//! An entry's quality columns, by column name, as its source publishes them.
//!
//! An entry is held for each of tens of millions of rows, most with one or two flags, such as
//! DESI's `spectype`. A map would allocate a node of several hundred bytes and a copy of each
//! name for every entry. Here the flags are one sorted slice, and each name is shared by every
//! entry that carries it, through one table of the names seen ([`name`]).

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Flags(Box<[(Arc<str>, String)]>);

impl Flags {
    /// The value of the flag `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&String> {
        let at = self.0.binary_search_by(|(n, _)| (**n).cmp(name)).ok()?;
        Some(&self.0[at].1)
    }

    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Takes the flag `name` out, giving its value.
    pub fn remove(&mut self, name: &str) -> Option<String> {
        let at = self.0.binary_search_by(|(n, _)| (**n).cmp(name)).ok()?;
        let mut kept = std::mem::take(&mut self.0).into_vec();
        let (_, value) = kept.remove(at);
        self.0 = kept.into_boxed_slice();
        Some(value)
    }

    /// Each flag's name and value, by name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &String)> {
        self.0.iter().map(|(n, v)| (&**n, v))
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
    /// The flags `pairs` give; of two of one name, the later, as a map's `insert` keeps it.
    fn from_iter<I: IntoIterator<Item = (K, V)>>(pairs: I) -> Self {
        let mut flags: Vec<(Arc<str>, String)> = pairs
            .into_iter()
            .map(|(k, v)| (name(k.as_ref()), v.into()))
            .collect();
        // A stable sort keeps pairs of one name in order, so the last is the later.
        flags.sort_by(|a, b| a.0.cmp(&b.0));
        let mut kept: Vec<(Arc<str>, String)> = Vec::with_capacity(flags.len());
        for pair in flags {
            match kept.last_mut() {
                Some(last) if last.0 == pair.0 => *last = pair,
                _ => kept.push(pair),
            }
        }
        Self(kept.into_boxed_slice())
    }
}

impl From<BTreeMap<String, String>> for Flags {
    fn from(map: BTreeMap<String, String>) -> Self {
        map.into_iter().collect()
    }
}

impl PartialEq<Flags> for BTreeMap<String, String> {
    fn eq(&self, flags: &Flags) -> bool {
        self.len() == flags.len()
            && self
                .iter()
                .zip(flags.iter())
                .all(|(a, b)| (&**a.0, a.1) == b)
    }
}

impl Serialize for Flags {
    /// As a map, by name: the JSON object a release's `flags` column holds.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.iter())
    }
}

impl<'de> Deserialize<'de> for Flags {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        BTreeMap::<String, String>::deserialize(deserializer).map(Self::from)
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
        assert_eq!(a.get("q_z").unwrap(), "5", "the later of two");
        assert_eq!(a.get("spectype").map(String::as_str), Some("GALAXY"));
        assert_eq!(a.get("absent"), None);
        let names: Vec<&str> = a.iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["q_z", "spectype"]);
        // Two entries' flags share one copy of a name.
        let b: Flags = [("spectype", "QSO")].into_iter().collect();
        assert!(Arc::ptr_eq(&a.0[1].0, &b.0[0].0));
        // The JSON is a map's, by name.
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            r#"{"q_z":"5","spectype":"GALAXY"}"#
        );
        let back: Flags = serde_json::from_str(r#"{"spectype":"GALAXY","q_z":"5"}"#).unwrap();
        assert_eq!(back, a);
        let map = BTreeMap::from([("q_z".to_owned(), "5".to_owned())]);
        let mut c = a.clone();
        assert_eq!(c.remove("spectype").as_deref(), Some("GALAXY"));
        assert_eq!(map, c);
        assert!(!c.contains_key("spectype"));
    }
}
