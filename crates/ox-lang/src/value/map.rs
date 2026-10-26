use crate::heap::{Heap, HeapFull};
use crate::value::Value;
use crate::value::equal::{equal, hash};
use crate::value::vector::Vector;

const FLAT: usize = 8;
const BITS: u32 = 5;
const MASK: u64 = 31;

pub type Hasher = fn(&Heap, Value) -> u64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leaf {
    key: Value,
    hash: u64,
    position: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Slot {
    Entry(Leaf),
    Child(Value),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Branch { bitmap: u32, slots: Box<[Slot]> },
    Collision(Box<[Leaf]>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Map {
    pub count: usize,
    entries: Vector,
    index: Value,
    holes: usize,
}

fn bit(hash: u64, shift: u32) -> u32 {
    1 << ((hash >> shift) & MASK)
}

fn slot(bitmap: u32, bit: u32) -> usize {
    (bitmap & (bit - 1)).count_ones() as usize
}

fn find(heap: &Heap, root: Value, hash: u64, key: Value) -> Option<usize> {
    let mut node = root;
    let mut shift = 0;
    loop {
        match heap.map_node_of(node)? {
            Node::Branch { bitmap, slots } => {
                let bit = bit(hash, shift);
                if bitmap & bit == 0 {
                    return None;
                }
                match slots.get(slot(*bitmap, bit))? {
                    Slot::Entry(leaf) => {
                        return (leaf.hash == hash && equal(heap, leaf.key, key))
                            .then_some(leaf.position);
                    }
                    Slot::Child(child) => {
                        node = *child;
                        shift += BITS;
                    }
                }
            }
            Node::Collision(leaves) => {
                return leaves
                    .iter()
                    .find(|leaf| leaf.hash == hash && equal(heap, leaf.key, key))
                    .map(|leaf| leaf.position);
            }
        }
    }
}

fn branch(heap: &mut Heap, bitmap: u32, slots: Vec<Slot>) -> Result<Value, HeapFull> {
    heap.map_node(Node::Branch {
        bitmap,
        slots: slots.into(),
    })
}

fn merge(heap: &mut Heap, shift: u32, first: Leaf, second: Leaf) -> Result<Value, HeapFull> {
    if first.hash == second.hash {
        return heap.map_node(Node::Collision(Box::new([first, second])));
    }
    let (one, two) = (bit(first.hash, shift), bit(second.hash, shift));
    if one == two {
        let child = merge(heap, shift + BITS, first, second)?;
        return branch(heap, one, vec![Slot::Child(child)]);
    }
    let slots = if one < two {
        vec![Slot::Entry(first), Slot::Entry(second)]
    } else {
        vec![Slot::Entry(second), Slot::Entry(first)]
    };
    branch(heap, one | two, slots)
}

fn insert(heap: &mut Heap, node: Value, shift: u32, leaf: Leaf) -> Result<Value, HeapFull> {
    match heap.map_node_of(node).cloned() {
        None => branch(heap, bit(leaf.hash, shift), vec![Slot::Entry(leaf)]),
        Some(Node::Branch { bitmap, slots }) => {
            let bit = bit(leaf.hash, shift);
            let at = slot(bitmap, bit);
            let mut slots = slots.into_vec();
            if bitmap & bit == 0 {
                slots.insert(at, Slot::Entry(leaf));
                return branch(heap, bitmap | bit, slots);
            }
            let replacement = match slots.get(at).copied() {
                Some(Slot::Child(child)) => insert(heap, child, shift + BITS, leaf)?,
                Some(Slot::Entry(existing)) => merge(heap, shift + BITS, existing, leaf)?,
                None => return branch(heap, bitmap, slots),
            };
            if let Some(place) = slots.get_mut(at) {
                *place = Slot::Child(replacement);
            }
            branch(heap, bitmap, slots)
        }
        Some(Node::Collision(leaves)) => {
            let shared = leaves.first().map_or(leaf.hash, |first| first.hash);
            if shared == leaf.hash {
                let mut leaves = leaves.into_vec();
                leaves.push(leaf);
                return heap.map_node(Node::Collision(leaves.into()));
            }
            let wrapper = branch(heap, bit(shared, shift), vec![Slot::Child(node)])?;
            insert(heap, wrapper, shift, leaf)
        }
    }
}

fn remove(
    heap: &mut Heap,
    node: Value,
    shift: u32,
    hash: u64,
    key: Value,
) -> Result<Value, HeapFull> {
    match heap.map_node_of(node).cloned() {
        None => Ok(node),
        Some(Node::Branch { bitmap, slots }) => {
            let bit = bit(hash, shift);
            if bitmap & bit == 0 {
                return Ok(node);
            }
            let at = slot(bitmap, bit);
            let mut slots = slots.into_vec();
            let gone = match slots.get(at).copied() {
                Some(Slot::Entry(leaf)) => {
                    if leaf.hash != hash || !equal(heap, leaf.key, key) {
                        return Ok(node);
                    }
                    true
                }
                Some(Slot::Child(child)) => {
                    let replacement = remove(heap, child, shift + BITS, hash, key)?;
                    if let Some(place) = slots.get_mut(at) {
                        *place = Slot::Child(replacement);
                    }
                    replacement == Value::Nil
                }
                None => return Ok(node),
            };
            let bitmap = if gone {
                slots.remove(at);
                bitmap & !bit
            } else {
                bitmap
            };
            if slots.is_empty() {
                return Ok(Value::Nil);
            }
            branch(heap, bitmap, slots)
        }
        Some(Node::Collision(leaves)) => {
            let mut leaves = leaves.into_vec();
            leaves.retain(|leaf| leaf.hash != hash || !equal(heap, leaf.key, key));
            if leaves.is_empty() {
                return Ok(Value::Nil);
            }
            heap.map_node(Node::Collision(leaves.into()))
        }
    }
}

fn leaves(heap: &Heap, root: Value) -> Vec<Leaf> {
    let mut found = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        match heap.map_node_of(node) {
            Some(Node::Branch { slots, .. }) => {
                for slot in slots {
                    match slot {
                        Slot::Entry(leaf) => found.push(*leaf),
                        Slot::Child(child) => pending.push(*child),
                    }
                }
            }
            Some(Node::Collision(collided)) => found.extend_from_slice(collided),
            None => {}
        }
    }
    found
}

impl Node {
    pub fn children(&self, out: &mut Vec<Value>) {
        match self {
            Self::Branch { slots, .. } => {
                for slot in slots {
                    match slot {
                        Slot::Entry(leaf) => out.push(leaf.key),
                        Slot::Child(child) => out.push(*child),
                    }
                }
            }
            Self::Collision(leaves) => out.extend(leaves.iter().map(|leaf| leaf.key)),
        }
    }
}

impl Map {
    pub const EMPTY: Self = Self {
        count: 0,
        entries: Vector::EMPTY,
        index: Value::Nil,
        holes: 0,
    };

    fn position(self, heap: &Heap, key: Value, hasher: Hasher) -> Option<usize> {
        if self.index != Value::Nil {
            return find(heap, self.index, hasher(heap, key), key);
        }
        self.entries.items(heap).into_iter().position(|entry| {
            heap.pair_of(entry)
                .is_some_and(|pair| equal(heap, pair.head, key))
        })
    }

    pub fn children(self) -> [Value; 3] {
        let [root, tail] = self.entries.children();
        [root, tail, self.index]
    }

    pub fn entries(self, heap: &Heap) -> Vec<(Value, Value)> {
        self.entries
            .items(heap)
            .into_iter()
            .filter_map(|entry| heap.pair_of(entry))
            .map(|pair| (pair.head, pair.tail))
            .collect()
    }

    pub fn get(self, heap: &Heap, key: Value) -> Option<Value> {
        self.get_by(heap, key, hash)
    }

    pub fn assoc(self, heap: &mut Heap, key: Value, value: Value) -> Result<Self, HeapFull> {
        self.assoc_by(heap, key, value, hash)
    }

    pub fn dissoc(self, heap: &mut Heap, key: Value) -> Result<Self, HeapFull> {
        self.dissoc_by(heap, key, hash)
    }

    pub fn entry(self, heap: &Heap, key: Value) -> Option<(Value, Value)> {
        let position = self.position(heap, key, hash)?;
        let entry = self.entries.get(heap, position)?;
        heap.pair_of(entry).map(|pair| (pair.head, pair.tail))
    }

    fn get_by(self, heap: &Heap, key: Value, hasher: Hasher) -> Option<Value> {
        let position = self.position(heap, key, hasher)?;
        let entry = self.entries.get(heap, position)?;
        heap.pair_of(entry).map(|pair| pair.tail)
    }

    fn assoc_by(
        self,
        heap: &mut Heap,
        key: Value,
        value: Value,
        hasher: Hasher,
    ) -> Result<Self, HeapFull> {
        if let Some(position) = self.position(heap, key, hasher) {
            let original = self
                .entries
                .get(heap, position)
                .and_then(|entry| heap.pair_of(entry))
                .map_or(key, |pair| pair.head);
            let entry = heap.pair(original, value)?;
            let entries = self
                .entries
                .assoc(heap, position, entry)?
                .unwrap_or(self.entries);
            return Ok(Self { entries, ..self });
        }
        let position = self.entries.count;
        let entry = heap.pair(key, value)?;
        let entries = self.entries.conj(heap, entry)?;
        let index = if self.index != Value::Nil {
            let leaf = Leaf {
                key,
                hash: hasher(heap, key),
                position,
            };
            insert(heap, self.index, 0, leaf)?
        } else if self.count >= FLAT {
            let mut index = Value::Nil;
            for (position, entry) in entries.items(heap).into_iter().enumerate() {
                let Some(pair) = heap.pair_of(entry) else {
                    continue;
                };
                let leaf = Leaf {
                    key: pair.head,
                    hash: hasher(heap, pair.head),
                    position,
                };
                index = insert(heap, index, 0, leaf)?;
            }
            index
        } else {
            Value::Nil
        };
        Ok(Self {
            count: self.count + 1,
            entries,
            index,
            holes: self.holes,
        })
    }

    fn dissoc_by(self, heap: &mut Heap, key: Value, hasher: Hasher) -> Result<Self, HeapFull> {
        let Some(position) = self.position(heap, key, hasher) else {
            return Ok(self);
        };
        let index = if self.index == Value::Nil {
            Value::Nil
        } else {
            remove(heap, self.index, 0, hasher(heap, key), key)?
        };
        let mut entries = self
            .entries
            .assoc(heap, position, Value::Nil)?
            .unwrap_or(self.entries);
        let mut holes = self.holes + 1;
        while holes > 0
            && let Some(last) = entries.count.checked_sub(1)
            && entries.get(heap, last) == Some(Value::Nil)
        {
            entries = entries.pop(heap)?.unwrap_or(Vector::EMPTY);
            holes -= 1;
        }
        let map = Self {
            count: self.count - 1,
            entries,
            index,
            holes,
        };
        if map.holes > map.count {
            map.compact(heap)
        } else {
            Ok(map)
        }
    }

    fn compact(self, heap: &mut Heap) -> Result<Self, HeapFull> {
        let mut moved = vec![0; self.entries.count];
        let mut live = Vec::with_capacity(self.count);
        for (old, entry) in self.entries.items(heap).into_iter().enumerate() {
            if entry != Value::Nil {
                if let Some(place) = moved.get_mut(old) {
                    *place = live.len();
                }
                live.push(entry);
            }
        }
        let entries = Vector::from_items(heap, &live)?;
        let mut index = Value::Nil;
        for leaf in leaves(heap, self.index) {
            let position = moved.get(leaf.position).copied().unwrap_or_default();
            index = insert(heap, index, 0, Leaf { position, ..leaf })?;
        }
        Ok(Self {
            count: self.count,
            entries,
            index,
            holes: 0,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::{Hasher, Map};
    use crate::heap::Heap;
    use crate::value::Value;
    use crate::value::equal::{equal, hash, identical};

    fn colliding(heap: &Heap, key: Value) -> u64 {
        hash(heap, key) % 4
    }

    fn clustered(heap: &Heap, key: Value) -> u64 {
        let full = hash(heap, key);
        (full % 4) | ((full & 4) << 50)
    }

    struct Random(u64);

    impl Random {
        fn below(&mut self, limit: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            usize::try_from(self.0 % u64::try_from(limit).unwrap()).unwrap()
        }
    }

    fn keys(heap: &mut Heap) -> Vec<Value> {
        let mut keys: Vec<Value> = (0..40).map(Value::Int).collect();
        for index in 0..24 {
            keys.push(heap.string(&format!("key {index}")).unwrap());
        }
        keys
    }

    fn check(heap: &Heap, map: Map, model: &[(Value, Value)], keys: &[Value], hasher: Hasher) {
        assert_eq!(map.count, model.len());
        let entries = map.entries(heap);
        assert_eq!(entries.len(), model.len());
        for ((key, value), (expected_key, expected_value)) in entries.iter().zip(model) {
            assert!(equal(heap, *key, *expected_key));
            assert_eq!(value, expected_value);
        }
        for key in keys {
            let expected = model
                .iter()
                .find(|(known, _)| equal(heap, *known, *key))
                .map(|(_, value)| *value);
            assert_eq!(map.get_by(heap, *key, hasher), expected, "{key:?}");
        }
    }

    #[test]
    fn maps_match_an_ordered_model_under_random_changes() {
        for hasher in [hash as Hasher, colliding, clustered] {
            let mut heap = Heap::default();
            let keys = keys(&mut heap);
            let mut random = Random(0x2545_f491_4f6c_dd1d);
            let mut map = Map::EMPTY;
            let mut model: Vec<(Value, Value)> = Vec::new();
            for step in 0..4000 {
                let key = keys[random.below(keys.len())];
                let found = model
                    .iter()
                    .position(|(known, _)| equal(&heap, *known, key));
                if random.below(3) == 0 {
                    map = map.dissoc_by(&mut heap, key, hasher).unwrap();
                    if let Some(position) = found {
                        model.remove(position);
                    }
                } else {
                    let value = Value::Int(step);
                    map = map.assoc_by(&mut heap, key, value, hasher).unwrap();
                    match found {
                        Some(position) => model[position].1 = value,
                        None => model.push((key, value)),
                    }
                }
                if step % 50 == 0 {
                    check(&heap, map, &model, &keys, hasher);
                } else {
                    check(&heap, map, &model, &[key], hasher);
                }
            }
        }
    }

    #[test]
    fn large_maps_keep_order_through_removals() {
        let mut heap = Heap::default();
        let mut map = Map::EMPTY;
        for number in 0..20_000 {
            map = map
                .assoc(&mut heap, Value::Int(number), Value::Int(-number))
                .unwrap();
        }
        for number in (0..20_000).step_by(2) {
            map = map.dissoc(&mut heap, Value::Int(number)).unwrap();
        }

        let expected: Vec<(Value, Value)> = (0..20_000)
            .filter(|number| number % 2 == 1)
            .map(|number| (Value::Int(number), Value::Int(-number)))
            .collect();
        assert_eq!(map.entries(&heap), expected);
        assert_eq!(map.get(&heap, Value::Int(4)), None);
        assert_eq!(
            map.get(&heap, Value::Int(19_999)),
            Some(Value::Int(-19_999))
        );
    }

    #[test]
    fn replacing_a_value_keeps_the_original_key_and_position() {
        let mut heap = Heap::default();
        let first = heap.string("a").unwrap();
        let again = heap.string("a").unwrap();
        let other = heap.string("b").unwrap();

        let map = Map::EMPTY
            .assoc(&mut heap, first, Value::Int(1))
            .unwrap()
            .assoc(&mut heap, other, Value::Int(2))
            .unwrap()
            .assoc(&mut heap, again, Value::Int(3))
            .unwrap();

        let entries = map.entries(&heap);
        assert_eq!(map.count, 2);
        assert!(identical(entries[0].0, first));
        assert_eq!(entries[0].1, Value::Int(3));
        assert_eq!(entries[1], (other, Value::Int(2)));
    }

    #[test]
    fn changes_leave_the_original_map_alone() {
        let mut heap = Heap::default();
        let small = (0..5).try_fold(Map::EMPTY, |map, number| {
            map.assoc(&mut heap, Value::Int(number), Value::Nil)
        });
        let small = small.unwrap();
        let large = (0..50).try_fold(small, |map, number| {
            map.assoc(&mut heap, Value::Int(number), Value::Bool(true))
        });
        let large = large.unwrap();

        let fewer = large.dissoc(&mut heap, Value::Int(3)).unwrap();

        assert_eq!(small.count, 5);
        assert_eq!(small.get(&heap, Value::Int(3)), Some(Value::Nil));
        assert_eq!(large.count, 50);
        assert_eq!(large.get(&heap, Value::Int(3)), Some(Value::Bool(true)));
        assert_eq!(fewer.count, 49);
        assert_eq!(fewer.get(&heap, Value::Int(3)), None);
    }
}
