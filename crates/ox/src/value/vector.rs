use crate::heap::{Heap, HeapFull};
use crate::value::Value;

const BITS: u32 = 5;
const WIDTH: usize = 1 << BITS;
const MASK: usize = WIDTH - 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vector {
    pub count: usize,
    start: usize,
    shift: u32,
    root: Value,
    tail: Value,
}

fn new_path(heap: &mut Heap, level: u32, node: Value) -> Result<Value, HeapFull> {
    let mut node = node;
    let mut level = level;
    while level > 0 {
        node = heap.node(Box::new([node]))?;
        level -= BITS;
    }
    Ok(node)
}

fn assoc_in(
    heap: &mut Heap,
    level: u32,
    node: Value,
    index: usize,
    item: Value,
) -> Result<Value, HeapFull> {
    let mut items = heap.node_items(node).to_vec();
    let slot = (index >> level) & MASK;
    let replacement = if level == 0 {
        item
    } else {
        let child = items.get(slot).copied().unwrap_or(Value::Nil);
        assoc_in(heap, level - BITS, child, index, item)?
    };
    if let Some(place) = items.get_mut(slot) {
        *place = replacement;
    }
    heap.node(items.into())
}

impl Vector {
    pub const EMPTY: Self = Self {
        count: 0,
        start: 0,
        shift: BITS,
        root: Value::Nil,
        tail: Value::Nil,
    };

    pub fn from_items(heap: &mut Heap, items: &[Value]) -> Result<Self, HeapFull> {
        if items.is_empty() {
            return Ok(Self::EMPTY);
        }
        let (body, rest) = items.split_at(((items.len() - 1) >> BITS) << BITS);
        let tail = heap.node(rest.into())?;
        let mut level = body
            .chunks(WIDTH)
            .map(|leaf| heap.node(leaf.into()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut shift = BITS;
        let root = loop {
            if level.is_empty() {
                break Value::Nil;
            }
            let parents = level
                .chunks(WIDTH)
                .map(|children| heap.node(children.into()))
                .collect::<Result<Vec<_>, _>>()?;
            if let [root] = parents.as_slice() {
                break *root;
            }
            level = parents;
            shift += BITS;
        };
        Ok(Self {
            count: items.len(),
            start: 0,
            shift,
            root,
            tail,
        })
    }

    pub fn rest(self) -> Self {
        if self.count <= 1 {
            return Self::EMPTY;
        }
        Self {
            count: self.count - 1,
            start: self.start + 1,
            ..self
        }
    }

    fn end(self) -> usize {
        self.start + self.count
    }

    pub fn children(self) -> [Value; 2] {
        [self.root, self.tail]
    }

    fn tail_offset(self) -> usize {
        if self.end() < WIDTH {
            0
        } else {
            ((self.end() - 1) >> BITS) << BITS
        }
    }

    fn leaf_node(self, heap: &Heap, index: usize) -> Value {
        if index >= self.tail_offset() {
            return self.tail;
        }
        let mut node = self.root;
        let mut level = self.shift;
        while level > 0 {
            node = heap
                .node_items(node)
                .get((index >> level) & MASK)
                .copied()
                .unwrap_or(Value::Nil);
            level -= BITS;
        }
        node
    }

    pub fn get(self, heap: &Heap, index: usize) -> Option<Value> {
        if index >= self.count {
            return None;
        }
        let at = self.start + index;
        heap.node_items(self.leaf_node(heap, at))
            .get(at & MASK)
            .copied()
    }

    pub fn items(self, heap: &Heap) -> Vec<Value> {
        let mut items = Vec::with_capacity(self.count);
        let mut at = self.start;
        while at < self.end() {
            let leaf = heap.node_items(self.leaf_node(heap, at));
            let Some(slice) = leaf.get(at & MASK..) else {
                break;
            };
            let taken = slice.len().min(self.end() - at);
            if taken == 0 {
                break;
            }
            items.extend_from_slice(&slice[..taken]);
            at += taken;
        }
        items
    }

    pub fn conj(self, heap: &mut Heap, item: Value) -> Result<Self, HeapFull> {
        let tail = heap.node_items(self.tail);
        if tail.len() < WIDTH {
            let mut items = tail.to_vec();
            items.push(item);
            let tail = heap.node(items.into())?;
            return Ok(Self {
                count: self.count + 1,
                tail,
                ..self
            });
        }
        let (root, shift) = if (self.end() >> BITS) > (1 << self.shift) {
            let path = new_path(heap, self.shift, self.tail)?;
            (heap.node(Box::new([self.root, path]))?, self.shift + BITS)
        } else {
            (self.push_tail(heap, self.shift, self.root)?, self.shift)
        };
        let tail = heap.node(Box::new([item]))?;
        Ok(Self {
            count: self.count + 1,
            start: self.start,
            shift,
            root,
            tail,
        })
    }

    fn push_tail(self, heap: &mut Heap, level: u32, parent: Value) -> Result<Value, HeapFull> {
        let slot = ((self.end() - 1) >> level) & MASK;
        let mut children = heap.node_items(parent).to_vec();
        let child = if level == BITS {
            self.tail
        } else if let Some(existing) = children.get(slot).copied() {
            self.push_tail(heap, level - BITS, existing)?
        } else {
            new_path(heap, level - BITS, self.tail)?
        };
        if let Some(place) = children.get_mut(slot) {
            *place = child;
        } else {
            children.push(child);
        }
        heap.node(children.into())
    }

    pub fn assoc(
        self,
        heap: &mut Heap,
        index: usize,
        item: Value,
    ) -> Result<Option<Self>, HeapFull> {
        if index > self.count {
            return Ok(None);
        }
        if index == self.count {
            return self.conj(heap, item).map(Some);
        }
        let index = self.start + index;
        if index >= self.tail_offset() {
            let mut items = heap.node_items(self.tail).to_vec();
            if let Some(place) = items.get_mut(index & MASK) {
                *place = item;
            }
            let tail = heap.node(items.into())?;
            return Ok(Some(Self { tail, ..self }));
        }
        let root = assoc_in(heap, self.shift, self.root, index, item)?;
        Ok(Some(Self { root, ..self }))
    }

    pub fn pop(self, heap: &mut Heap) -> Result<Option<Self>, HeapFull> {
        match self.count {
            0 => return Ok(None),
            1 => return Ok(Some(Self::EMPTY)),
            _ => {}
        }
        let tail = heap.node_items(self.tail);
        if tail.len() > 1 {
            let tail = heap.node(tail[..tail.len() - 1].into())?;
            return Ok(Some(Self {
                count: self.count - 1,
                tail,
                ..self
            }));
        }
        let tail = self.leaf_node(heap, self.end() - 2);
        let mut root = self.pop_tail(heap, self.shift, self.root)?;
        let mut shift = self.shift;
        if shift > BITS
            && let [only] = heap.node_items(root)
        {
            root = *only;
            shift -= BITS;
        }
        Ok(Some(Self {
            count: self.count - 1,
            start: self.start,
            shift,
            root,
            tail,
        }))
    }

    fn pop_tail(self, heap: &mut Heap, level: u32, node: Value) -> Result<Value, HeapFull> {
        let slot = ((self.end() - 2) >> level) & MASK;
        let mut children = heap.node_items(node).to_vec();
        if level > BITS {
            let child = children.get(slot).copied().unwrap_or(Value::Nil);
            let child = self.pop_tail(heap, level - BITS, child)?;
            if child == Value::Nil && slot == 0 {
                return Ok(Value::Nil);
            }
            children.truncate(slot);
            if child != Value::Nil {
                children.push(child);
            }
        } else {
            if slot == 0 {
                return Ok(Value::Nil);
            }
            children.truncate(slot);
        }
        heap.node(children.into())
    }
}
#[cfg(test)]
mod tests {
    use super::Vector;
    use crate::heap::Heap;
    use crate::value::Value;

    const SIZES: [usize; 10] = [0, 1, 31, 32, 33, 64, 1024, 1056, 1057, 33_900];

    fn numbers(count: usize) -> Vec<Value> {
        (0..count)
            .map(|number| Value::Int(i64::try_from(number).unwrap()))
            .collect()
    }

    fn conjoined(heap: &mut Heap, items: &[Value]) -> Vector {
        items
            .iter()
            .try_fold(Vector::EMPTY, |vector, item| vector.conj(heap, *item))
            .unwrap()
    }

    #[test]
    fn vectors_built_at_once_or_by_conj_hold_their_items_in_order() {
        let mut heap = Heap::default();
        for size in SIZES {
            let items = numbers(size);
            let built = Vector::from_items(&mut heap, &items).unwrap();
            let grown = conjoined(&mut heap, &items);
            for vector in [built, grown] {
                assert_eq!(vector.count, size);
                assert_eq!(vector.items(&heap), items, "{size}");
                for (index, item) in items.iter().enumerate() {
                    assert_eq!(vector.get(&heap, index), Some(*item), "{size} {index}");
                }
                assert_eq!(vector.get(&heap, size), None);
            }
        }
    }

    #[test]
    fn assoc_replaces_one_item_and_leaves_the_original_alone() {
        let mut heap = Heap::default();
        for size in SIZES.into_iter().filter(|size| *size > 0) {
            let items = numbers(size);
            let vector = Vector::from_items(&mut heap, &items).unwrap();
            for index in [0, size / 2, size - 1] {
                let changed = vector.assoc(&mut heap, index, Value::Nil).unwrap().unwrap();
                let mut expected = items.clone();
                expected[index] = Value::Nil;
                assert_eq!(changed.items(&heap), expected, "{size} {index}");
                assert_eq!(vector.items(&heap), items);
            }
            let appended = vector.assoc(&mut heap, size, Value::Nil).unwrap().unwrap();
            assert_eq!(appended.count, size + 1);
            assert_eq!(appended.get(&heap, size), Some(Value::Nil));
            assert!(
                vector
                    .assoc(&mut heap, size + 1, Value::Nil)
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[test]
    fn the_rest_of_a_vector_holds_the_items_after_the_first_and_grows_and_shrinks_like_one() {
        let mut heap = Heap::default();
        for size in [1, 2, 31, 32, 33, 64, 65, 1057] {
            let items = numbers(size);
            let mut view = Vector::from_items(&mut heap, &items).unwrap();
            for dropped in 1..=size.min(70) {
                view = view.rest();
                let expected = &items[dropped..];
                assert_eq!(view.count, expected.len(), "{size} {dropped}");
                assert_eq!(view.items(&heap), expected, "{size} {dropped}");
                if let Some(last) = expected.len().checked_sub(1) {
                    assert_eq!(view.get(&heap, last), Some(expected[last]));
                    let changed = view.assoc(&mut heap, 0, Value::Nil).unwrap().unwrap();
                    assert_eq!(changed.get(&heap, 0), Some(Value::Nil));
                    assert_eq!(changed.items(&heap)[1..], expected[1..]);
                    let popped = view.pop(&mut heap).unwrap().unwrap();
                    assert_eq!(popped.items(&heap), expected[..last], "{size} {dropped}");
                }
                assert_eq!(view.get(&heap, expected.len()), None);
                let grown = view.conj(&mut heap, Value::Nil).unwrap();
                let mut longer = expected.to_vec();
                longer.push(Value::Nil);
                assert_eq!(grown.items(&heap), longer, "{size} {dropped}");
            }
        }
        assert_eq!(Vector::EMPTY.rest().count, 0);
    }

    #[test]
    fn pop_removes_the_last_item_down_to_empty() {
        let mut heap = Heap::default();
        for size in [1100, 33_900] {
            let items = numbers(size);
            let mut vector = Vector::from_items(&mut heap, &items).unwrap();
            for remaining in (0..size).rev() {
                vector = vector.pop(&mut heap).unwrap().unwrap();
                assert_eq!(vector.count, remaining);
                if remaining > 0 {
                    assert_eq!(vector.get(&heap, remaining - 1), Some(items[remaining - 1]));
                }
                if remaining % 1000 == 0 || SIZES.contains(&remaining) {
                    assert_eq!(vector.items(&heap), items[..remaining], "{remaining}");
                    let regrown = vector.conj(&mut heap, Value::Nil).unwrap();
                    let mut expected = items[..remaining].to_vec();
                    expected.push(Value::Nil);
                    assert_eq!(regrown.items(&heap), expected, "{remaining}");
                }
            }
            assert!(vector.pop(&mut heap).unwrap().is_none());
        }
    }
}
