use crate::block::{BlockPtr, ItemContent, ItemPosition, Prelim};
use crate::types::{Branch, BranchRef, TypePtr, Value, TYPE_REFS_ARRAY};
use crate::Transaction;
use lib0::any::Any;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt::Formatter;

/// A collection used to store data in an indexed sequence structure. This type is internally
/// implemented as a double linked list, which may squash values inserted directly one after another
/// into single list node upon transaction commit.
///
/// Reading a root-level type as an YArray means treating its sequence components as a list, where
/// every countable element becomes an individual entity:
///
/// - JSON-like primitives (booleans, numbers, strings, JSON maps, arrays etc.) are counted
///   individually.
/// - Text chunks inserted by [Text] data structure: each character becomes an element of an
///   array.
/// - Embedded and binary values: they count as a single element even though they correspond of
///   multiple bytes.
///
/// Like all Yrs shared data types, YArray is resistant to the problem of interleaving (situation
/// when elements inserted one after another may interleave with other peers concurrent inserts
/// after merging all updates together). In case of Yrs conflict resolution is solved by using
/// unique document id to determine correct and consistent ordering.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Array(BranchRef);

impl Array {
    /// Returns a number of elements stored in current array.
    pub fn len(&self) -> u32 {
        let inner = self.0.borrow();
        inner.len()
    }

    /// Inserts a `value` at the given `index`. Inserting at index `0` is equivalent to prepending
    /// current array with given `value`, while inserting at array length is equivalent to appending
    /// that value at the end of it.
    ///
    /// Using `index` value that's higher than current array length results in panic.
    pub fn insert<V: Prelim>(&self, txn: &mut Transaction, index: u32, value: V) {
        let (start, parent) = {
            let parent = self.0.borrow();
            if index <= parent.len() {
                (parent.start, parent.ptr.clone())
            } else {
                panic!("Cannot insert item at index over the length of an array")
            }
        };
        let (left, right) = if index == 0 {
            (None, None)
        } else {
            Branch::index_to_ptr(txn, start, index)
        };
        let pos = ItemPosition {
            parent,
            left,
            right,
            index: 0,
        };

        txn.create_item(&pos, value, None);
    }

    /// Inserts multiple `values` at the given `index`. Inserting at index `0` is equivalent to
    /// prepending current array with given `values`, while inserting at array length is equivalent
    /// to appending that value at the end of it.
    ///
    /// Using `index` value that's higher than current array length results in panic.
    pub fn insert_range<T, V>(&self, txn: &mut Transaction, index: u32, values: T)
    where
        T: IntoIterator<Item = V>,
        V: Into<Any>,
    {
        self.insert(txn, index, PrelimRange(values))
    }

    /// Inserts given `value` at the end of the current array.
    pub fn push_back<V: Prelim>(&self, txn: &mut Transaction, value: V) {
        let len = self.len();
        self.insert(txn, len, value)
    }

    /// Inserts given `value` at the beginning of the current array.
    pub fn push_front<V: Prelim>(&self, txn: &mut Transaction, content: V) {
        self.insert(txn, 0, content)
    }

    /// Removes a single element at provided `index`.
    pub fn remove(&self, txn: &mut Transaction, index: u32) {
        self.remove_range(txn, index, 1)
    }

    /// Removes a range of elements from current array, starting at given `index` up until
    /// a particular number described by `len` has been deleted. This method panics in case when
    /// not all expected elements were removed (due to insufficient number of elements in an array)
    /// or `index` is outside of the bounds of an array.
    pub fn remove_range(&self, txn: &mut Transaction, index: u32, len: u32) {
        let removed = self.0.remove_at(txn, index, len);
        if removed != len {
            panic!("Couldn't remove {} elements from an array. Only {} of them were successfully removed.", len, removed);
        }
    }

    /// Retrieves a value stored at a given `index`. Returns `None` when provided index was out
    /// of the range of a current array.
    pub fn get(&self, txn: &Transaction, index: u32) -> Option<Value> {
        let inner = self.0.borrow();
        let (content, idx) = inner.get_at(txn, index)?;
        Some(content.get_content(txn).remove(idx))
    }

    /// Returns an iterator, that can be used to lazely traverse over all values stored in a current
    /// array.
    pub fn iter<'a, 'b, 'txn>(&'a self, txn: &'b Transaction<'txn>) -> ArrayIter<'b, 'txn> {
        ArrayIter::new(self, txn)
    }

    /// Converts all contents of current array into a JSON-like representation.
    pub fn to_json(&self, txn: &Transaction) -> Any {
        let res = self.iter(txn).map(|v| v.to_json(txn)).collect();
        Any::Array(res)
    }
}

pub struct ArrayIter<'b, 'txn> {
    content: VecDeque<Value>,
    ptr: Option<BlockPtr>,
    txn: &'b Transaction<'txn>,
}

impl<'b, 'txn> ArrayIter<'b, 'txn> {
    fn new(array: &Array, txn: &'b Transaction<'txn>) -> Self {
        let inner = array.0.borrow();
        ArrayIter {
            ptr: inner.start,
            txn,
            content: VecDeque::default(),
        }
    }
}

impl<'b, 'txn> Iterator for ArrayIter<'b, 'txn> {
    type Item = Value;

    fn next(&mut self) -> Option<Self::Item> {
        match self.content.pop_front() {
            None => {
                if let Some(ptr) = self.ptr.take() {
                    let item = self.txn.store.blocks.get_item(&ptr)?;
                    self.ptr = item.right.clone();
                    if !item.is_deleted() && item.is_countable() {
                        self.content = item.content.get_content(self.txn).into();
                    }
                    self.next()
                } else {
                    None // end of iterator
                }
            }
            value => value,
        }
    }
}

impl From<BranchRef> for Array {
    fn from(inner: BranchRef) -> Self {
        Array(inner)
    }
}

/// A preliminary array. It's can be used to initialize an YArray, when it's about to be nested
/// into another Yrs data collection, such as [Map] or another YArray.
pub struct PrelimArray<T, V>(T)
where
    T: IntoIterator<Item = V>;

impl<T, V> From<T> for PrelimArray<T, V>
where
    T: IntoIterator<Item = V>,
{
    fn from(iter: T) -> Self {
        PrelimArray(iter)
    }
}

/// Prelim range defines a way to insert multiple elements effectively at once one after another
/// in an efficient way, provided that these elements correspond to a primitive JSON-like types.
struct PrelimRange<T, V>(T)
where
    T: IntoIterator<Item = V>,
    V: Into<Any>;

impl<T, V> Prelim for PrelimRange<T, V>
where
    T: IntoIterator<Item = V>,
    V: Into<Any>,
{
    fn into_content(self, txn: &mut Transaction, ptr: TypePtr) -> (ItemContent, Option<Self>) {
        let vec: Vec<Any> = self.0.into_iter().map(|v| v.into()).collect();
        (ItemContent::Any(vec), None)
    }

    fn integrate(self, txn: &mut Transaction, inner_ref: BranchRef) {}
}

impl<T, V> Prelim for PrelimArray<T, V>
where
    V: Prelim,
    T: IntoIterator<Item = V>,
{
    fn into_content(self, txn: &mut Transaction, ptr: TypePtr) -> (ItemContent, Option<Self>) {
        let inner = BranchRef::new(Branch::new(ptr, TYPE_REFS_ARRAY, None));
        (ItemContent::Type(inner), Some(self))
    }

    fn integrate(self, txn: &mut Transaction, inner_ref: BranchRef) {
        let array = Array::from(inner_ref);
        for value in self.0 {
            array.push_back(txn, value);
        }
    }
}

#[cfg(test)]
mod test {
    use crate::test_utils::exchange_updates;
    use crate::types::map::PrelimMap;
    use crate::types::Value;
    use crate::Doc;
    use lib0::any::Any;
    use std::collections::HashMap;

    #[test]
    fn push_back() {
        let doc = Doc::with_client_id(1);
        let mut txn = doc.transact();
        let a = txn.get_array("array");

        a.push_back(&mut txn, "a");
        a.push_back(&mut txn, "b");
        a.push_back(&mut txn, "c");

        let actual: Vec<_> = a.iter(&txn).collect();
        assert_eq!(actual, vec!["a".into(), "b".into(), "c".into()]);
    }

    #[test]
    fn push_front() {
        let doc = Doc::with_client_id(1);
        let mut txn = doc.transact();
        let a = txn.get_array("array");

        a.push_front(&mut txn, "c");
        a.push_front(&mut txn, "b");
        a.push_front(&mut txn, "a");

        let actual: Vec<_> = a.iter(&txn).collect();
        assert_eq!(actual, vec!["a".into(), "b".into(), "c".into()]);
    }

    #[test]
    fn insert() {
        let doc = Doc::with_client_id(1);
        let mut txn = doc.transact();
        let a = txn.get_array("array");

        a.insert(&mut txn, 0, "a");
        a.insert(&mut txn, 1, "c");
        a.insert(&mut txn, 1, "b");

        let actual: Vec<_> = a.iter(&txn).collect();
        assert_eq!(actual, vec!["a".into(), "b".into(), "c".into()]);
    }

    #[test]
    fn basic() {
        let d1 = Doc::with_client_id(1);
        let d2 = Doc::with_client_id(2);

        let mut t1 = d1.transact();
        let a1 = t1.get_array("array");

        a1.insert(&mut t1, 0, "Hi");
        let update = d1.encode_state_as_update_v1(&t1);

        let mut t2 = d2.transact();
        d2.apply_update_v1(&mut t2, update.as_slice());
        let a2 = t2.get_array("array");
        let actual: Vec<_> = a2.iter(&t2).collect();

        assert_eq!(actual, vec!["Hi".into()]);
    }

    #[test]
    fn len() {
        let d = Doc::with_client_id(1);

        {
            let mut txn = d.transact();
            let a = txn.get_array("array");

            a.push_back(&mut txn, 0); // len: 1
            a.push_back(&mut txn, 1); // len: 2
            a.push_back(&mut txn, 2); // len: 3
            a.push_back(&mut txn, 3); // len: 4

            a.remove_range(&mut txn, 0, 1); // len: 3
            a.insert(&mut txn, 0, 0); // len: 4

            assert_eq!(a.len(), 4);
        }
        {
            let mut txn = d.transact();
            let a = txn.get_array("array");
            a.remove_range(&mut txn, 1, 1); // len: 3
            assert_eq!(a.len(), 3);

            a.insert(&mut txn, 1, 1); // len: 4
            assert_eq!(a.len(), 4);

            a.remove_range(&mut txn, 2, 1); // len: 3
            assert_eq!(a.len(), 3);

            a.insert(&mut txn, 2, 2); // len: 4
            assert_eq!(a.len(), 4);
        }

        let mut txn = d.transact();
        let a = txn.get_array("array");
        assert_eq!(a.len(), 4);

        a.remove_range(&mut txn, 1, 1);
        assert_eq!(a.len(), 3);

        a.insert(&mut txn, 1, 1);
        assert_eq!(a.len(), 4);
    }

    #[test]
    fn remove_insert() {
        let d1 = Doc::with_client_id(1);

        let mut t1 = d1.transact();
        let a1 = t1.get_array("array");
        a1.insert(&mut t1, 0, "A");
        a1.remove_range(&mut t1, 1, 0);
    }

    #[test]
    fn insert_3_elements_try_re_get() {
        let d1 = Doc::with_client_id(1);
        let d2 = Doc::with_client_id(2);
        {
            let mut t1 = d1.transact();
            let a1 = t1.get_array("array");

            a1.push_back(&mut t1, 1);
            a1.push_back(&mut t1, true);
            a1.push_back(&mut t1, false);
            let actual: Vec<_> = a1.iter(&t1).collect();
            assert_eq!(
                actual,
                vec![Value::from(1.0), Value::from(true), Value::from(false)]
            );
        }

        exchange_updates(&[&d1, &d2]);

        let mut t2 = d2.transact();
        let a2 = t2.get_array("array");
        let actual: Vec<_> = a2.iter(&t2).collect();
        assert_eq!(
            actual,
            vec![Value::from(1.0), Value::from(true), Value::from(false)]
        );
    }

    #[test]
    fn concurrent_insert_with_3_conflicts() {
        let d1 = Doc::with_client_id(1);
        {
            let mut txn = d1.transact();
            let a = txn.get_array("array");
            a.insert(&mut txn, 0, 0);
        }

        let d2 = Doc::with_client_id(2);
        {
            let mut txn = d1.transact();
            let a = txn.get_array("array");
            a.insert(&mut txn, 0, 1);
        }

        let d3 = Doc::with_client_id(3);
        {
            let mut txn = d1.transact();
            let a = txn.get_array("array");
            a.insert(&mut txn, 0, 2);
        }

        exchange_updates(&[&d1, &d2, &d3]);

        let a1 = to_array(&d1);
        let a2 = to_array(&d2);
        let a3 = to_array(&d3);

        assert_eq!(a1, a2, "Peer 1 and peer 2 states are different");
        assert_eq!(a2, a3, "Peer 2 and peer 3 states are different");
    }

    fn to_array(d: &Doc) -> Vec<Value> {
        let mut txn = d.transact();
        let a = txn.get_array("array");
        a.iter(&txn).collect()
    }

    #[test]
    fn concurrent_insert_remove_with_3_conflicts() {
        let d1 = Doc::with_client_id(1);
        {
            let mut txn = d1.transact();
            let a = txn.get_array("array");
            a.insert_range(&mut txn, 0, ["x", "y", "z"]);
        }
        let d2 = Doc::with_client_id(2);
        let d3 = Doc::with_client_id(3);

        exchange_updates(&[&d1, &d2, &d3]);

        {
            // start state: [x,y,z]
            let mut t1 = d1.transact();
            let mut t2 = d2.transact();
            let mut t3 = d3.transact();
            let a1 = t1.get_array("array");
            let a2 = t2.get_array("array");
            let a3 = t3.get_array("array");

            a1.insert(&mut t1, 1, 0); // [x,0,y,z]
            a2.remove_range(&mut t2, 0, 1); // [y,z]
            a2.remove_range(&mut t2, 1, 1); // [y]
            a3.insert(&mut t3, 1, 2); // [x,2,y,z]
        }

        exchange_updates(&[&d1, &d2, &d3]);
        // after exchange expected: [0,2,y]

        let a1 = to_array(&d1);
        let a2 = to_array(&d2);
        let a3 = to_array(&d3);

        assert_eq!(a1, a2, "Peer 1 and peer 2 states are different");
        assert_eq!(a2, a3, "Peer 2 and peer 3 states are different");
    }

    #[test]
    fn insertions_in_late_sync() {
        let d1 = Doc::with_client_id(1);
        {
            let mut txn = d1.transact();
            let a = txn.get_array("array");
            a.push_back(&mut txn, "x");
            a.push_back(&mut txn, "y");
        }
        let d2 = Doc::with_client_id(2);
        let d3 = Doc::with_client_id(3);

        exchange_updates(&[&d1, &d2, &d3]);

        {
            let mut t1 = d1.transact();
            let mut t2 = d2.transact();
            let mut t3 = d3.transact();
            let a1 = t1.get_array("array");
            let a2 = t2.get_array("array");
            let a3 = t3.get_array("array");

            a1.insert(&mut t1, 1, "user0");
            a2.insert(&mut t2, 1, "user1");
            a3.insert(&mut t3, 1, "user2");
        }

        exchange_updates(&[&d1, &d2, &d3]);

        let a1 = to_array(&d1);
        let a2 = to_array(&d2);
        let a3 = to_array(&d3);

        assert_eq!(a1, a2, "Peer 1 and peer 2 states are different");
        assert_eq!(a2, a3, "Peer 2 and peer 3 states are different");
    }

    #[test]
    fn removals_in_late_sync() {
        let d1 = Doc::with_client_id(1);
        {
            let mut txn = d1.transact();
            let a = txn.get_array("array");
            a.push_back(&mut txn, "x");
            a.push_back(&mut txn, "y");
        }
        let d2 = Doc::with_client_id(2);

        exchange_updates(&[&d1, &d2]);

        {
            let mut t1 = d1.transact();
            let mut t2 = d2.transact();
            let a1 = t1.get_array("array");
            let a2 = t2.get_array("array");

            a2.remove_range(&mut t2, 1, 1);
            a1.remove_range(&mut t1, 0, 2);
        }

        exchange_updates(&[&d1, &d2]);

        let a1 = to_array(&d1);
        let a2 = to_array(&d2);

        assert_eq!(a1, a2, "Peer 1 and peer 2 states are different");
    }

    #[test]
    fn insert_then_merge_delete_on_sync() {
        let d1 = Doc::with_client_id(1);
        {
            let mut txn = d1.transact();
            let a = txn.get_array("array");
            a.push_back(&mut txn, "x");
            a.push_back(&mut txn, "y");
            a.push_back(&mut txn, "z");
        }
        let d2 = Doc::with_client_id(2);

        exchange_updates(&[&d1, &d2]);

        {
            let mut t2 = d2.transact();
            let a2 = t2.get_array("array");

            a2.remove_range(&mut t2, 0, 3);
        }

        exchange_updates(&[&d1, &d2]);

        let a1 = to_array(&d1);
        let a2 = to_array(&d2);

        assert_eq!(a1, a2, "Peer 1 and peer 2 states are different");
    }

    #[test]
    fn iter_array_containing_types() {
        let d = Doc::with_client_id(1);
        let mut txn = d.transact();
        let a = txn.get_array("arr");
        for i in 0..10 {
            let mut m = HashMap::new();
            m.insert("value".to_owned(), i);
            a.push_back(&mut txn, PrelimMap::from(m));
        }

        for (i, value) in a.iter(&txn).enumerate() {
            let mut expected = HashMap::new();
            expected.insert("value".to_owned(), Any::Number(i as f64));
            match value {
                Value::YMap(ref map) => {
                    assert_eq!(value.to_json(&txn), Any::Map(expected))
                }
                other => panic!("Value of array at index {} was no YMap", i),
            }
        }
    }
}
