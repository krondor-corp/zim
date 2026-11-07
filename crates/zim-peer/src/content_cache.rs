//! Blob pin policy and the on-demand content cache.
//!
//! A [`PinPolicy`] decides which of a manifest's pins a pull downloads.
//! [`PinPolicy::Full`] (the daemon default) mirrors every pin.
//! [`PinPolicy::OnDemand`] keeps only the structural ones (the ops-log
//! blob and the previous manifest; dir bodies already ship inline in
//! the manifest) and fetches file content on first read.
//!
//! [`ContentCache`] is the LRU over content blobs fetched that way. It
//! only ever tracks blobs it fetched from a peer, so locally-authored
//! content (put straight into the store by a write) is never a
//! candidate for eviction: a blob that came off the network still
//! exists on the peer that served it, a locally-written one may not.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use zim_core::fs::{Manifest, Pins};
use zim_core::linked_data::Hash;

/// Which pinned blobs a peer keeps locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PinPolicy {
    /// Download every pin on pull; the reconcile sweep heals any that
    /// are missing. The default, and what `zim` daemons run.
    #[default]
    Full,
    /// Download only structural pins on pull. File content is fetched
    /// on first read and evicted least-recently-used once fetched
    /// content exceeds `budget_bytes`.
    OnDemand { budget_bytes: u64 },
}

impl PinPolicy {
    /// The pins of `manifest` a pull should download under this policy.
    pub fn pins_to_fetch(&self, manifest: &Manifest) -> Pins {
        match self {
            PinPolicy::Full => manifest.pins().clone(),
            PinPolicy::OnDemand { .. } => {
                let content = manifest.file_pins();
                let mut structural = Pins::new();
                structural.extend(
                    manifest
                        .pins()
                        .iter()
                        .filter(|h| !content.contains(h))
                        .copied(),
                );
                structural
            }
        }
    }
}

/// LRU bookkeeping for on-demand content. Pure accounting: it says
/// what to evict, the coordinator does the deleting.
#[derive(Debug)]
pub struct ContentCache {
    budget_bytes: u64,
    inner: Mutex<Lru>,
}

#[derive(Debug, Default)]
struct Lru {
    clock: u64,
    total_bytes: u64,
    /// hash → (size, last-used tick)
    entries: HashMap<Hash, (u64, u64)>,
    /// last-used tick → hash, oldest first
    order: BTreeMap<u64, Hash>,
}

impl Lru {
    fn bump(&mut self, hash: Hash) -> bool {
        let Some((_, tick)) = self.entries.get_mut(&hash) else {
            return false;
        };
        self.order.remove(tick);
        self.clock += 1;
        *tick = self.clock;
        self.order.insert(self.clock, hash);
        true
    }
}

impl ContentCache {
    pub fn new(budget_bytes: u64) -> Self {
        Self {
            budget_bytes,
            inner: Mutex::new(Lru::default()),
        }
    }

    /// Bytes of fetched content currently tracked.
    pub fn total_bytes(&self) -> u64 {
        self.inner.lock().unwrap().total_bytes
    }

    /// Whether `hash` is fetched content this cache owns.
    pub fn contains(&self, hash: &Hash) -> bool {
        self.inner.lock().unwrap().entries.contains_key(hash)
    }

    /// Mark a tracked blob as just read. No-op for blobs the cache
    /// doesn't own.
    pub fn touch(&self, hash: &Hash) {
        self.inner.lock().unwrap().bump(*hash);
    }

    /// Record a freshly fetched blob of `size` bytes as most recently
    /// used, and return the blobs to evict (oldest first) to get back
    /// under budget. The blob just admitted is never a victim, even if
    /// it alone exceeds the budget: the caller is about to read it.
    /// Victims are dropped from the bookkeeping here.
    pub fn admit(&self, hash: Hash, size: u64) -> Vec<Hash> {
        let mut lru = self.inner.lock().unwrap();
        if !lru.bump(hash) {
            lru.clock += 1;
            let tick = lru.clock;
            lru.entries.insert(hash, (size, tick));
            lru.order.insert(tick, hash);
            lru.total_bytes += size;
        }
        let mut victims = Vec::new();
        while lru.total_bytes > self.budget_bytes {
            let Some((&tick, &oldest)) = lru.order.iter().next() else {
                break;
            };
            if oldest == hash {
                break;
            }
            lru.order.remove(&tick);
            if let Some((size, _)) = lru.entries.remove(&oldest) {
                lru.total_bytes -= size;
            }
            victims.push(oldest);
        }
        victims
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(n: u8) -> Hash {
        Hash::from_bytes([n; 32])
    }

    #[test]
    fn the_least_recently_read_blob_goes_first() {
        // Bob's phone has room for 100 bytes of fetched content. He
        // reads a, then b, then re-reads a. Fetching c overflows the
        // budget: b is the stalest, so b goes and a stays.
        let cache = ContentCache::new(100);
        assert!(cache.admit(h(1), 40).is_empty());
        assert!(cache.admit(h(2), 40).is_empty());
        cache.touch(&h(1));

        let victims = cache.admit(h(3), 40);

        assert_eq!(victims, vec![h(2)]);
        assert!(cache.contains(&h(1)));
        assert!(!cache.contains(&h(2)));
        assert_eq!(cache.total_bytes(), 80);
    }

    #[test]
    fn an_oversized_blob_evicts_everything_else_but_itself() {
        // A single file bigger than the whole budget still gets read:
        // everything older is cleared to make room, the new blob stays.
        let cache = ContentCache::new(100);
        cache.admit(h(1), 50);
        cache.admit(h(2), 30);

        let victims = cache.admit(h(3), 500);

        assert_eq!(victims, vec![h(1), h(2)]);
        assert!(cache.contains(&h(3)));
    }

    #[test]
    fn touching_an_unknown_blob_is_a_no_op() {
        // Locally-authored content never entered the cache; a read of
        // it must not start tracking (and later evicting) it.
        let cache = ContentCache::new(100);
        cache.touch(&h(9));
        assert!(!cache.contains(&h(9)));
        assert_eq!(cache.total_bytes(), 0);
    }
}
