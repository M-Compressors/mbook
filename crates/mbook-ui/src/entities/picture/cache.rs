//! A cache of decoded pictures bounded by their size in bytes.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

/// One value of the cache with what the cache needs to know about it.
struct Entry<V> {
    value: Arc<V>,
    bytes: usize,
    /// The tick of the last use; the smallest is the least recently used.
    used: u64,
}

/// A cache bounded by the sum of the bytes of its values. The least recently
/// used value leaves first.
///
/// Not thread-safe by itself: the one who owns it serialises the calls. With a
/// few dozen pictures the least recently used one is found by a scan, which
/// costs less than the structure that would avoid it.
pub(crate) struct PictureCache<K, V> {
    entries: HashMap<K, Entry<V>>,
    max_bytes: usize,
    bytes: usize,
    tick: u64,
}

impl<K: Eq + Hash + Clone, V> PictureCache<K, V> {
    /// An empty cache that holds at most `max_bytes` bytes.
    pub(crate) fn new(max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_bytes,
            bytes: 0,
            tick: 0,
        }
    }

    /// The value of `key`, marked as used now.
    pub(crate) fn get(&mut self, key: &K) -> Option<Arc<V>> {
        let used = self.next_tick();
        let entry = self.entries.get_mut(key)?;
        entry.used = used;
        Some(Arc::clone(&entry.value))
    }

    /// Keeps `value`, which takes `bytes`, under `key`.
    ///
    /// A value kept under the key before is dropped first. Values that were
    /// used least recently leave until the new one fits. A value larger than
    /// the bound of the cache is not kept.
    pub(crate) fn insert(&mut self, key: K, value: Arc<V>, bytes: usize) {
        self.remove(&key);
        if bytes > self.max_bytes {
            return;
        }
        while self.bytes + bytes > self.max_bytes && self.evict_oldest() {}
        let used = self.next_tick();
        self.bytes += bytes;
        self.entries.insert(key, Entry { value, bytes, used });
    }

    /// The keys of the values kept, in no order.
    #[cfg(test)]
    pub(crate) fn keys(&self) -> impl Iterator<Item = &K> {
        self.entries.keys()
    }

    /// How many values are kept.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is kept.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The sum of the bytes of the values kept.
    #[cfg(test)]
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    /// Drops everything.
    #[cfg(test)]
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }

    fn next_tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }

    fn remove(&mut self, key: &K) {
        if let Some(entry) = self.entries.remove(key) {
            self.bytes -= entry.bytes;
        }
    }

    /// Drops the least recently used value; `false` when there is none.
    fn evict_oldest(&mut self) -> bool {
        let oldest = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.used)
            .map(|(key, _)| key.clone());
        match oldest {
            Some(key) => {
                self.remove(&key);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PictureCache;
    use std::sync::Arc;

    fn cache(max_bytes: usize) -> PictureCache<&'static str, u32> {
        PictureCache::new(max_bytes)
    }

    #[test]
    fn new_cache_is_empty() {
        let cache = cache(10);
        assert!(cache.is_empty());
        assert_eq!((cache.len(), cache.bytes()), (0, 0));
    }

    #[test]
    fn inserted_value_is_found_and_counted() {
        let mut cache = cache(10);
        cache.insert("a", Arc::new(1), 4);
        assert_eq!(cache.get(&"a").as_deref(), Some(&1));
        assert_eq!((cache.len(), cache.bytes()), (1, 4));
    }

    #[test]
    fn missing_key_is_a_miss() {
        let mut cache = cache(10);
        cache.insert("a", Arc::new(1), 4);
        assert!(cache.get(&"b").is_none());
    }

    #[test]
    fn least_recently_inserted_leaves_first() {
        let mut cache = cache(10);
        cache.insert("a", Arc::new(1), 4);
        cache.insert("b", Arc::new(2), 4);
        cache.insert("c", Arc::new(3), 4);
        assert!(cache.get(&"a").is_none());
        assert!(cache.get(&"b").is_some());
        assert!(cache.get(&"c").is_some());
        assert_eq!(cache.bytes(), 8);
    }

    #[test]
    fn a_hit_protects_the_value_from_eviction() {
        let mut cache = cache(10);
        cache.insert("a", Arc::new(1), 4);
        cache.insert("b", Arc::new(2), 4);
        assert!(cache.get(&"a").is_some());
        cache.insert("c", Arc::new(3), 4);
        assert!(cache.get(&"a").is_some());
        assert!(cache.get(&"b").is_none());
    }

    #[test]
    fn several_values_leave_for_one_large_value() {
        let mut cache = cache(10);
        cache.insert("a", Arc::new(1), 3);
        cache.insert("b", Arc::new(2), 3);
        cache.insert("c", Arc::new(3), 3);
        cache.insert("d", Arc::new(4), 8);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.bytes(), 8);
        assert!(cache.get(&"d").is_some());
    }

    #[test]
    fn value_of_exactly_the_bound_is_kept() {
        let mut cache = cache(10);
        cache.insert("a", Arc::new(1), 10);
        assert_eq!(cache.bytes(), 10);
        assert!(cache.get(&"a").is_some());
    }

    #[test]
    fn value_larger_than_the_bound_is_not_kept_and_evicts_nothing() {
        let mut cache = cache(10);
        cache.insert("a", Arc::new(1), 4);
        cache.insert("big", Arc::new(2), 11);
        assert!(cache.get(&"big").is_none());
        assert!(cache.get(&"a").is_some());
        assert_eq!(cache.bytes(), 4);
    }

    #[test]
    fn replacing_a_key_swaps_the_value_and_the_bytes() {
        let mut cache = cache(10);
        cache.insert("a", Arc::new(1), 4);
        cache.insert("a", Arc::new(2), 6);
        assert_eq!(cache.get(&"a").as_deref(), Some(&2));
        assert_eq!((cache.len(), cache.bytes()), (1, 6));
    }

    #[test]
    fn replacing_a_key_with_a_value_that_is_too_large_drops_the_old_one() {
        let mut cache = cache(10);
        cache.insert("a", Arc::new(1), 4);
        cache.insert("a", Arc::new(2), 20);
        assert!(cache.is_empty());
        assert_eq!(cache.bytes(), 0);
    }

    #[test]
    fn cache_without_room_keeps_nothing() {
        let mut cache = cache(0);
        cache.insert("a", Arc::new(1), 1);
        assert!(cache.is_empty());
    }

    #[test]
    fn empty_value_costs_nothing_and_is_kept() {
        let mut cache = cache(0);
        cache.insert("a", Arc::new(1), 0);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn clear_drops_everything() {
        let mut cache = cache(10);
        cache.insert("a", Arc::new(1), 4);
        cache.insert("b", Arc::new(2), 4);
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.bytes(), 0);
        assert!(cache.get(&"a").is_none());
    }

    #[test]
    fn value_handed_out_outlives_its_eviction() {
        let mut cache = cache(4);
        cache.insert("a", Arc::new(7), 4);
        let held = cache.get(&"a");
        cache.insert("b", Arc::new(8), 4);
        assert!(cache.get(&"a").is_none());
        assert_eq!(held.as_deref(), Some(&7));
    }
}
