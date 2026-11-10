//! What a backend keeps between requests: a release never changes once published, so what is
//! read of it once stays true. The Worker keeps one per isolate; each test database its own.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, PoisonError};

use univ_model::release::read::Vocabulary;

use crate::bulk::{DomainRow, TileNodes};
use crate::grids::GridKeys;
use crate::shards::ShardMap;

/// Everything a backend keeps, by release.
#[derive(Default)]
pub struct Kept {
    pub(crate) vocabularies: Store<u32, Vocabulary>,
    pub(crate) tile_nodes: Store<(u32, String), TileNodes>,
    pub(crate) domains: Store<u32, Vec<DomainRow>>,
    pub(crate) grids: Store<(u32, String), GridKeys>,
    pub(crate) shard_maps: Store<u32, ShardMap>,
}

/// Values read once, by key.
pub(crate) struct Store<K, V>(Mutex<HashMap<K, Arc<V>>>);

impl<K, V> Default for Store<K, V> {
    fn default() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

impl<K: Eq + Hash, V> Store<K, V> {
    /// The value kept for `key`.
    pub(crate) fn get(&self, key: &K) -> Option<Arc<V>> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .cloned()
    }

    /// Keeps `value` for `key`, and gives it back.
    pub(crate) fn keep(&self, key: K, value: V) -> Arc<V> {
        let value = Arc::new(value);
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, Arc::clone(&value));
        value
    }
}
