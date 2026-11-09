#[doc(no_inline)]
pub use alloc::collections::*;

pub mod hash_map {
    use alloc::vec::{self, Vec};
    use core::borrow::Borrow;
    use core::fmt;
    use core::hash::{BuildHasher, Hash, Hasher};
    use core::iter::{FromIterator, FusedIterator};
    use core::ops::Index;

    #[derive(Clone, Debug)]
    pub struct RandomState {
        k0: u64,
        k1: u64,
    }

    impl RandomState {
        #[inline]
        pub fn new() -> Self {
            let r = arceos_api::modules::axhal::misc::random();
            Self {
                k0: r as u64,
                k1: (r >> 64) as u64,
            }
        }
    }

    impl Default for RandomState {
        #[inline]
        fn default() -> Self {
            Self::new()
        }
    }

    impl BuildHasher for RandomState {
        type Hasher = DefaultHasher;

        #[inline]
        fn build_hasher(&self) -> Self::Hasher {
            DefaultHasher::new_with_keys(self.k0, self.k1)
        }
    }

    #[derive(Clone, Debug)]
    pub struct DefaultHasher {
        state: u64,
        k1: u64,
    }

    impl DefaultHasher {
        const K: u64 = 0x517cc1b727220a95;

        #[inline]
        pub fn new() -> Self {
            Self::new_with_keys(0, 0)
        }

        #[inline]
        pub const fn new_with_keys(k0: u64, k1: u64) -> Self {
            Self {
                state: k0 ^ 0x243f6a8885a308d3,
                k1: k1 ^ 0x13198a2e03707344,
            }
        }

        #[inline]
        fn mix(&mut self, v: u64) {
            self.state = (self.state.rotate_left(5) ^ v).wrapping_mul(Self::K);
        }
    }

    impl Default for DefaultHasher {
        #[inline]
        fn default() -> Self {
            Self::new()
        }
    }

    impl Hasher for DefaultHasher {
        #[inline]
        fn write(&mut self, mut bytes: &[u8]) {
            while bytes.len() >= 8 {
                let chunk = u64::from_ne_bytes(bytes[..8].try_into().unwrap());
                self.mix(chunk);
                bytes = &bytes[8..];
            }
            if bytes.len() >= 4 {
                let chunk = u32::from_ne_bytes(bytes[..4].try_into().unwrap()) as u64;
                self.mix(chunk);
                bytes = &bytes[4..];
            }
            if bytes.len() >= 2 {
                let chunk = u16::from_ne_bytes(bytes[..2].try_into().unwrap()) as u64;
                self.mix(chunk);
                bytes = &bytes[2..];
            }
            if let Some(&b) = bytes.first() {
                self.mix(b as u64);
            }
        }

        #[inline]
        fn write_u8(&mut self, i: u8) {
            self.mix(i as u64);
        }

        #[inline]
        fn write_u16(&mut self, i: u16) {
            self.mix(i as u64);
        }

        #[inline]
        fn write_u32(&mut self, i: u32) {
            self.mix(i as u64);
        }

        #[inline]
        fn write_u64(&mut self, i: u64) {
            self.mix(i);
        }

        #[inline]
        fn write_u128(&mut self, i: u128) {
            self.mix(i as u64);
            self.mix((i >> 64) as u64);
        }

        #[inline]
        fn write_usize(&mut self, i: usize) {
            self.mix(i as u64);
        }

        #[inline]
        fn finish(&self) -> u64 {
            let mut h = self.state ^ self.k1;
            h ^= h >> 33;
            h = h.wrapping_mul(0xff51afd7ed558ccd);
            h ^= h >> 33;
            h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
            h ^ (h >> 33)
        }
    }

    struct Slot<K, V> {
        hash: u64,
        key: K,
        val: V,
    }

    impl<K: Clone, V: Clone> Clone for Slot<K, V> {
        fn clone(&self) -> Self {
            Self {
                hash: self.hash,
                key: self.key.clone(),
                val: self.val.clone(),
            }
        }
    }

    pub struct HashMap<K, V, S = RandomState> {
        slots: Vec<Option<Slot<K, V>>>,
        len: usize,
        hash_builder: S,
    }

    impl<K, V> HashMap<K, V, RandomState> {
        #[inline]
        pub fn new() -> Self {
            Self::with_hasher(RandomState::new())
        }

        #[inline]
        pub fn with_capacity(capacity: usize) -> Self {
            Self::with_capacity_and_hasher(capacity, RandomState::new())
        }
    }

    impl<K, V, S> HashMap<K, V, S> {
        #[inline]
        pub const fn with_hasher(hash_builder: S) -> Self {
            Self {
                slots: Vec::new(),
                len: 0,
                hash_builder,
            }
        }

        pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
            let cap = Self::raw_capacity(capacity);
            let mut slots = Vec::with_capacity(cap);
            slots.resize_with(cap, || None);
            Self {
                slots,
                len: 0,
                hash_builder,
            }
        }

        #[inline]
        fn raw_capacity(capacity: usize) -> usize {
            if capacity == 0 {
                0
            } else {
                (capacity.saturating_mul(4) / 3 + 1)
                    .next_power_of_two()
                    .max(8)
            }
        }

        #[inline]
        pub fn capacity(&self) -> usize {
            self.slots.len() * 3 / 4
        }

        #[inline]
        pub fn len(&self) -> usize {
            self.len
        }

        #[inline]
        pub fn is_empty(&self) -> bool {
            self.len == 0
        }

        #[inline]
        pub fn hasher(&self) -> &S {
            &self.hash_builder
        }

        pub fn clear(&mut self) {
            for slot in &mut self.slots {
                *slot = None;
            }
            self.len = 0;
        }

        #[inline]
        pub fn iter(&self) -> Iter<'_, K, V> {
            Iter {
                inner: self.slots.iter(),
                rem: self.len,
            }
        }

        #[inline]
        pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
            IterMut {
                inner: self.slots.iter_mut(),
                rem: self.len,
            }
        }

        #[inline]
        pub fn keys(&self) -> Keys<'_, K, V> {
            Keys { inner: self.iter() }
        }

        #[inline]
        pub fn values(&self) -> Values<'_, K, V> {
            Values { inner: self.iter() }
        }

        #[inline]
        pub fn values_mut(&mut self) -> ValuesMut<'_, K, V> {
            ValuesMut {
                inner: self.iter_mut(),
            }
        }

        #[inline]
        pub fn drain(&mut self) -> Drain<'_, K, V> {
            let rem = self.len;
            self.len = 0;
            Drain {
                inner: self.slots.iter_mut(),
                rem,
            }
        }

        pub fn retain<F>(&mut self, mut f: F)
        where
            F: FnMut(&K, &mut V) -> bool,
        {
            let n = self.slots.len();
            let mut i = 0;
            while i < n {
                let keep = match &mut self.slots[i] {
                    Some(slot) => f(&slot.key, &mut slot.val),
                    None => true,
                };
                if !keep {
                    self.remove_slot(i);
                } else {
                    i += 1;
                }
            }
        }

        fn remove_slot(&mut self, mut idx: usize) -> (K, V) {
            let mask = self.slots.len() - 1;
            let removed = self.slots[idx].take().unwrap();
            self.len -= 1;
            let mut next_idx = (idx + 1) & mask;
            while let Some(slot) = &self.slots[next_idx] {
                if Self::probe_distance(mask, slot.hash, next_idx) == 0 {
                    break;
                }
                self.slots[idx] = self.slots[next_idx].take();
                idx = next_idx;
                next_idx = (idx + 1) & mask;
            }
            (removed.key, removed.val)
        }

        #[inline]
        fn probe_distance(mask: usize, hash: u64, idx: usize) -> usize {
            idx.wrapping_sub(hash as usize) & mask
        }
    }

    impl<K, V, S> HashMap<K, V, S>
    where
        K: Eq + Hash,
        S: BuildHasher,
    {
        #[inline]
        fn make_hash<Q: Hash + ?Sized>(&self, k: &Q) -> u64 {
            self.hash_builder.hash_one(k)
        }

        pub fn reserve(&mut self, additional: usize) {
            let needed = self.len.saturating_add(additional);
            let min_slots = Self::raw_capacity(needed);
            if min_slots > self.slots.len() {
                self.rehash(min_slots);
            }
        }

        pub fn shrink_to_fit(&mut self) {
            let min_slots = Self::raw_capacity(self.len);
            if min_slots < self.slots.len() {
                self.rehash(min_slots);
            }
        }

        fn rehash(&mut self, new_cap: usize) {
            if new_cap == 0 {
                self.slots = Vec::new();
                return;
            }
            let mut new_slots = Vec::with_capacity(new_cap);
            new_slots.resize_with(new_cap, || None);
            let mask = new_cap - 1;
            for slot in self.slots.drain(..).flatten() {
                let mut idx = (slot.hash as usize) & mask;
                let mut dist = 0;
                let mut cur = slot;
                loop {
                    match &mut new_slots[idx] {
                        None => {
                            new_slots[idx] = Some(cur);
                            break;
                        }
                        Some(existing) => {
                            let existing_dist = Self::probe_distance(mask, existing.hash, idx);
                            if existing_dist < dist {
                                core::mem::swap(existing, &mut cur);
                                dist = existing_dist;
                            }
                            idx = (idx + 1) & mask;
                            dist += 1;
                        }
                    }
                }
            }
            self.slots = new_slots;
        }

        fn find_slot<Q>(&self, hash: u64, k: &Q) -> Option<usize>
        where
            K: Borrow<Q>,
            Q: Eq + ?Sized,
        {
            if self.slots.is_empty() {
                return None;
            }
            let mask = self.slots.len() - 1;
            let mut idx = (hash as usize) & mask;
            let mut dist = 0;
            loop {
                match &self.slots[idx] {
                    None => return None,
                    Some(slot) => {
                        if slot.hash == hash && slot.key.borrow() == k {
                            return Some(idx);
                        }
                        if Self::probe_distance(mask, slot.hash, idx) < dist {
                            return None;
                        }
                        idx = (idx + 1) & mask;
                        dist += 1;
                    }
                }
            }
        }

        fn insert_slot(&mut self, hash: u64, k: K, v: V) -> (usize, Option<V>) {
            if (self.len + 1) * 4 > self.slots.len() * 3 || self.slots.is_empty() {
                let next_cap = (self.slots.len() * 2).max(8);
                self.rehash(next_cap);
            }
            let mask = self.slots.len() - 1;
            let mut idx = (hash as usize) & mask;
            let mut dist = 0;
            let mut cur = Slot {
                hash,
                key: k,
                val: v,
            };
            let mut inserted_idx = None;
            loop {
                match &mut self.slots[idx] {
                    None => {
                        self.slots[idx] = Some(cur);
                        self.len += 1;
                        return (inserted_idx.unwrap_or(idx), None);
                    }
                    Some(existing) => {
                        if inserted_idx.is_none()
                            && existing.hash == cur.hash
                            && existing.key == cur.key
                        {
                            let old = core::mem::replace(&mut existing.val, cur.val);
                            return (idx, Some(old));
                        }
                        let existing_dist = Self::probe_distance(mask, existing.hash, idx);
                        if existing_dist < dist {
                            if inserted_idx.is_none() {
                                inserted_idx = Some(idx);
                            }
                            core::mem::swap(existing, &mut cur);
                            dist = existing_dist;
                        }
                        idx = (idx + 1) & mask;
                        dist += 1;
                    }
                }
            }
        }

        pub fn entry(&mut self, key: K) -> Entry<'_, K, V, S> {
            let hash = self.make_hash(&key);
            if let Some(idx) = self.find_slot(hash, &key) {
                Entry::Occupied(OccupiedEntry { map: self, idx })
            } else {
                Entry::Vacant(VacantEntry {
                    map: self,
                    hash,
                    key,
                })
            }
        }

        pub fn insert(&mut self, k: K, v: V) -> Option<V> {
            let hash = self.make_hash(&k);
            self.insert_slot(hash, k, v).1
        }

        pub fn get<Q>(&self, k: &Q) -> Option<&V>
        where
            K: Borrow<Q>,
            Q: Hash + Eq + ?Sized,
        {
            let hash = self.make_hash(k);
            let idx = self.find_slot(hash, k)?;
            self.slots[idx].as_ref().map(|s| &s.val)
        }

        pub fn get_key_value<Q>(&self, k: &Q) -> Option<(&K, &V)>
        where
            K: Borrow<Q>,
            Q: Hash + Eq + ?Sized,
        {
            let hash = self.make_hash(k);
            let idx = self.find_slot(hash, k)?;
            self.slots[idx].as_ref().map(|s| (&s.key, &s.val))
        }

        pub fn get_mut<Q>(&mut self, k: &Q) -> Option<&mut V>
        where
            K: Borrow<Q>,
            Q: Hash + Eq + ?Sized,
        {
            let hash = self.make_hash(k);
            let idx = self.find_slot(hash, k)?;
            self.slots[idx].as_mut().map(|s| &mut s.val)
        }

        pub fn contains_key<Q>(&self, k: &Q) -> bool
        where
            K: Borrow<Q>,
            Q: Hash + Eq + ?Sized,
        {
            self.get(k).is_some()
        }

        pub fn remove<Q>(&mut self, k: &Q) -> Option<V>
        where
            K: Borrow<Q>,
            Q: Hash + Eq + ?Sized,
        {
            self.remove_entry(k).map(|(_, v)| v)
        }

        pub fn remove_entry<Q>(&mut self, k: &Q) -> Option<(K, V)>
        where
            K: Borrow<Q>,
            Q: Hash + Eq + ?Sized,
        {
            let hash = self.make_hash(k);
            let idx = self.find_slot(hash, k)?;
            Some(self.remove_slot(idx))
        }
    }

    impl<K, V, S: Default> Default for HashMap<K, V, S> {
        #[inline]
        fn default() -> Self {
            Self::with_hasher(S::default())
        }
    }

    impl<K: Clone, V: Clone, S: Clone> Clone for HashMap<K, V, S> {
        fn clone(&self) -> Self {
            Self {
                slots: self.slots.clone(),
                len: self.len,
                hash_builder: self.hash_builder.clone(),
            }
        }
    }

    impl<K: fmt::Debug, V: fmt::Debug, S> fmt::Debug for HashMap<K, V, S> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_map().entries(self.iter()).finish()
        }
    }

    impl<K, V, S> PartialEq for HashMap<K, V, S>
    where
        K: Eq + Hash,
        V: PartialEq,
        S: BuildHasher,
    {
        fn eq(&self, other: &Self) -> bool {
            self.len() == other.len() && self.iter().all(|(k, v)| other.get(k) == Some(v))
        }
    }

    impl<K, V, S> Eq for HashMap<K, V, S>
    where
        K: Eq + Hash,
        V: Eq,
        S: BuildHasher,
    {
    }

    impl<K, Q: ?Sized, V, S> Index<&Q> for HashMap<K, V, S>
    where
        K: Eq + Hash + Borrow<Q>,
        Q: Eq + Hash,
        S: BuildHasher,
    {
        type Output = V;

        #[inline]
        fn index(&self, key: &Q) -> &V {
            self.get(key).expect("no entry found for key")
        }
    }

    impl<K, V, S> Extend<(K, V)> for HashMap<K, V, S>
    where
        K: Eq + Hash,
        S: BuildHasher,
    {
        fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
            let iter = iter.into_iter();
            self.reserve(iter.size_hint().0);
            for (k, v) in iter {
                self.insert(k, v);
            }
        }
    }

    impl<'a, K, V, S> Extend<(&'a K, &'a V)> for HashMap<K, V, S>
    where
        K: Eq + Hash + Copy,
        V: Copy,
        S: BuildHasher,
    {
        fn extend<T: IntoIterator<Item = (&'a K, &'a V)>>(&mut self, iter: T) {
            self.extend(iter.into_iter().map(|(&k, &v)| (k, v)));
        }
    }

    impl<K, V, S> FromIterator<(K, V)> for HashMap<K, V, S>
    where
        K: Eq + Hash,
        S: BuildHasher + Default,
    {
        fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
            let mut map = Self::with_hasher(S::default());
            map.extend(iter);
            map
        }
    }

    impl<K, V, const N: usize> From<[(K, V); N]> for HashMap<K, V, RandomState>
    where
        K: Eq + Hash,
    {
        fn from(arr: [(K, V); N]) -> Self {
            Self::from_iter(arr)
        }
    }

    pub enum Entry<'a, K, V, S = RandomState> {
        Occupied(OccupiedEntry<'a, K, V, S>),
        Vacant(VacantEntry<'a, K, V, S>),
    }

    pub struct OccupiedEntry<'a, K, V, S = RandomState> {
        map: &'a mut HashMap<K, V, S>,
        idx: usize,
    }

    pub struct VacantEntry<'a, K, V, S = RandomState> {
        map: &'a mut HashMap<K, V, S>,
        hash: u64,
        key: K,
    }

    impl<'a, K: Eq + Hash, V, S: BuildHasher> Entry<'a, K, V, S> {
        pub fn or_insert(self, default: V) -> &'a mut V {
            match self {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => entry.insert(default),
            }
        }

        pub fn or_insert_with<F: FnOnce() -> V>(self, default: F) -> &'a mut V {
            match self {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => entry.insert(default()),
            }
        }

        pub fn or_insert_with_key<F: FnOnce(&K) -> V>(self, default: F) -> &'a mut V {
            match self {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => {
                    let val = default(entry.key());
                    entry.insert(val)
                }
            }
        }

        pub fn key(&self) -> &K {
            match self {
                Entry::Occupied(entry) => entry.key(),
                Entry::Vacant(entry) => entry.key(),
            }
        }

        pub fn and_modify<F: FnOnce(&mut V)>(self, f: F) -> Self {
            match self {
                Entry::Occupied(mut entry) => {
                    f(entry.get_mut());
                    Entry::Occupied(entry)
                }
                Entry::Vacant(entry) => Entry::Vacant(entry),
            }
        }
    }

    impl<'a, K: Eq + Hash, V: Default, S: BuildHasher> Entry<'a, K, V, S> {
        pub fn or_default(self) -> &'a mut V {
            self.or_insert_with(V::default)
        }
    }

    impl<'a, K, V, S> OccupiedEntry<'a, K, V, S> {
        #[inline]
        pub fn key(&self) -> &K {
            &self.map.slots[self.idx].as_ref().unwrap().key
        }

        #[inline]
        pub fn remove_entry(self) -> (K, V) {
            self.map.remove_slot(self.idx)
        }

        #[inline]
        pub fn get(&self) -> &V {
            &self.map.slots[self.idx].as_ref().unwrap().val
        }

        #[inline]
        pub fn get_mut(&mut self) -> &mut V {
            &mut self.map.slots[self.idx].as_mut().unwrap().val
        }

        #[inline]
        pub fn into_mut(self) -> &'a mut V {
            &mut self.map.slots[self.idx].as_mut().unwrap().val
        }

        #[inline]
        pub fn insert(&mut self, value: V) -> V {
            core::mem::replace(self.get_mut(), value)
        }

        #[inline]
        pub fn remove(self) -> V {
            self.remove_entry().1
        }
    }

    impl<'a, K: Eq + Hash, V, S: BuildHasher> VacantEntry<'a, K, V, S> {
        #[inline]
        pub fn key(&self) -> &K {
            &self.key
        }

        #[inline]
        pub fn into_key(self) -> K {
            self.key
        }

        #[inline]
        pub fn insert(self, value: V) -> &'a mut V {
            let (idx, _) = self.map.insert_slot(self.hash, self.key, value);
            &mut self.map.slots[idx].as_mut().unwrap().val
        }
    }

    #[derive(Clone)]
    pub struct Iter<'a, K, V> {
        inner: core::slice::Iter<'a, Option<Slot<K, V>>>,
        rem: usize,
    }

    impl<'a, K, V> Iterator for Iter<'a, K, V> {
        type Item = (&'a K, &'a V);

        #[inline]
        fn next(&mut self) -> Option<Self::Item> {
            let s = self.inner.by_ref().flatten().next()?;
            self.rem -= 1;
            Some((&s.key, &s.val))
        }

        #[inline]
        fn size_hint(&self) -> (usize, Option<usize>) {
            (self.rem, Some(self.rem))
        }
    }

    impl<'a, K, V> ExactSizeIterator for Iter<'a, K, V> {}
    impl<'a, K, V> FusedIterator for Iter<'a, K, V> {}

    pub struct IterMut<'a, K, V> {
        inner: core::slice::IterMut<'a, Option<Slot<K, V>>>,
        rem: usize,
    }

    impl<'a, K, V> Iterator for IterMut<'a, K, V> {
        type Item = (&'a K, &'a mut V);

        #[inline]
        fn next(&mut self) -> Option<Self::Item> {
            let s = self.inner.by_ref().flatten().next()?;
            self.rem -= 1;
            Some((&s.key, &mut s.val))
        }

        #[inline]
        fn size_hint(&self) -> (usize, Option<usize>) {
            (self.rem, Some(self.rem))
        }
    }

    impl<'a, K, V> ExactSizeIterator for IterMut<'a, K, V> {}
    impl<'a, K, V> FusedIterator for IterMut<'a, K, V> {}

    pub struct IntoIter<K, V> {
        inner: vec::IntoIter<Option<Slot<K, V>>>,
        rem: usize,
    }

    impl<K, V> Iterator for IntoIter<K, V> {
        type Item = (K, V);

        #[inline]
        fn next(&mut self) -> Option<Self::Item> {
            let s = self.inner.by_ref().flatten().next()?;
            self.rem -= 1;
            Some((s.key, s.val))
        }

        #[inline]
        fn size_hint(&self) -> (usize, Option<usize>) {
            (self.rem, Some(self.rem))
        }
    }

    impl<K, V> ExactSizeIterator for IntoIter<K, V> {}
    impl<K, V> FusedIterator for IntoIter<K, V> {}

    #[derive(Clone)]
    pub struct Keys<'a, K, V> {
        inner: Iter<'a, K, V>,
    }

    impl<'a, K, V> Iterator for Keys<'a, K, V> {
        type Item = &'a K;

        #[inline]
        fn next(&mut self) -> Option<Self::Item> {
            self.inner.next().map(|(k, _)| k)
        }

        #[inline]
        fn size_hint(&self) -> (usize, Option<usize>) {
            self.inner.size_hint()
        }
    }

    impl<'a, K, V> ExactSizeIterator for Keys<'a, K, V> {}
    impl<'a, K, V> FusedIterator for Keys<'a, K, V> {}

    #[derive(Clone)]
    pub struct Values<'a, K, V> {
        inner: Iter<'a, K, V>,
    }

    impl<'a, K, V> Iterator for Values<'a, K, V> {
        type Item = &'a V;

        #[inline]
        fn next(&mut self) -> Option<Self::Item> {
            self.inner.next().map(|(_, v)| v)
        }

        #[inline]
        fn size_hint(&self) -> (usize, Option<usize>) {
            self.inner.size_hint()
        }
    }

    impl<'a, K, V> ExactSizeIterator for Values<'a, K, V> {}
    impl<'a, K, V> FusedIterator for Values<'a, K, V> {}

    pub struct ValuesMut<'a, K, V> {
        inner: IterMut<'a, K, V>,
    }

    impl<'a, K, V> Iterator for ValuesMut<'a, K, V> {
        type Item = &'a mut V;

        #[inline]
        fn next(&mut self) -> Option<Self::Item> {
            self.inner.next().map(|(_, v)| v)
        }

        #[inline]
        fn size_hint(&self) -> (usize, Option<usize>) {
            self.inner.size_hint()
        }
    }

    impl<'a, K, V> ExactSizeIterator for ValuesMut<'a, K, V> {}
    impl<'a, K, V> FusedIterator for ValuesMut<'a, K, V> {}

    pub struct Drain<'a, K, V> {
        inner: core::slice::IterMut<'a, Option<Slot<K, V>>>,
        rem: usize,
    }

    impl<'a, K, V> Iterator for Drain<'a, K, V> {
        type Item = (K, V);

        #[inline]
        fn next(&mut self) -> Option<Self::Item> {
            let s = self.inner.by_ref().find_map(Option::take)?;
            self.rem -= 1;
            Some((s.key, s.val))
        }

        #[inline]
        fn size_hint(&self) -> (usize, Option<usize>) {
            (self.rem, Some(self.rem))
        }
    }

    impl<'a, K, V> Drop for Drain<'a, K, V> {
        fn drop(&mut self) {
            for slot in &mut self.inner {
                *slot = None;
            }
        }
    }

    impl<'a, K, V> ExactSizeIterator for Drain<'a, K, V> {}
    impl<'a, K, V> FusedIterator for Drain<'a, K, V> {}

    impl<'a, K, V, S> IntoIterator for &'a HashMap<K, V, S> {
        type Item = (&'a K, &'a V);
        type IntoIter = Iter<'a, K, V>;

        #[inline]
        fn into_iter(self) -> Self::IntoIter {
            self.iter()
        }
    }

    impl<'a, K, V, S> IntoIterator for &'a mut HashMap<K, V, S> {
        type Item = (&'a K, &'a mut V);
        type IntoIter = IterMut<'a, K, V>;

        #[inline]
        fn into_iter(self) -> Self::IntoIter {
            self.iter_mut()
        }
    }

    impl<K, V, S> IntoIterator for HashMap<K, V, S> {
        type Item = (K, V);
        type IntoIter = IntoIter<K, V>;

        #[inline]
        fn into_iter(self) -> Self::IntoIter {
            IntoIter {
                inner: self.slots.into_iter(),
                rem: self.len,
            }
        }
    }
}

#[doc(no_inline)]
pub use self::hash_map::HashMap;
