pub mod array;
pub mod map;
pub mod text;
pub mod xml;

use crate::*;
pub use map::Map;
pub use text::Text;

use crate::block::{BlockPtr, Item, ItemContent, ItemPosition, Prelim};
use crate::types::array::Array;
use crate::types::xml::{XmlElement, XmlText};
use lib0::any::Any;
use std::cell::{BorrowMutError, Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::fmt::Formatter;
use std::hash::Hasher;
use std::rc::Rc;

pub type TypeRefs = u8;

/// Type ref identifier for an [Array] type.
pub const TYPE_REFS_ARRAY: TypeRefs = 0;

/// Type ref identifier for a [Map] type.
pub const TYPE_REFS_MAP: TypeRefs = 1;

/// Type ref identifier for a [Text] type.
pub const TYPE_REFS_TEXT: TypeRefs = 2;

/// Type ref identifier for a [XmlElement] type.
pub const TYPE_REFS_XML_ELEMENT: TypeRefs = 3;

/// Type ref identifier for a [XmlFragment] type. Used for compatibility.
pub const TYPE_REFS_XML_FRAGMENT: TypeRefs = 4;

/// Type ref identifier for a [XmlHook] type. Used for compatibility.
pub const TYPE_REFS_XML_HOOK: TypeRefs = 5;

/// Type ref identifier for a [XmlText] type.
pub const TYPE_REFS_XML_TEXT: TypeRefs = 6;

/// Placeholder type ref identifier for non-specialized AbstractType. Used only for root-level types
/// which have been integrated from remote peers before they were defined locally.
pub const TYPE_REFS_UNDEFINED: TypeRefs = 7;

/// A wrapper around [Branch] cell, supplied with a bunch of convenience methods to operate on both
/// map-like and array-like contents of a [Branch].
#[derive(Debug, Clone)]
pub struct BranchRef(Rc<RefCell<Branch>>);

impl BranchRef {
    pub fn new(inner: Branch) -> Self {
        BranchRef(Rc::new(RefCell::new(inner)))
    }

    /// Returns an immutable ref wrapper to an underlying [Branch].
    /// This method will panic, if current branch was already mutably borrowed.
    pub fn borrow(&self) -> Ref<Branch> {
        self.0.borrow()
    }

    /// Returns a mutable ref wrapper to an underlying [Branch].
    /// This method will panic, if current branch was already borrowed (either mutably or immutably)
    /// somewhere else.
    pub fn borrow_mut(&self) -> RefMut<Branch> {
        self.0.borrow_mut()
    }

    /// Returns a result, which may either be a mutable ref wrapper to an underlying [Branch],
    /// or an error in case when this branch was already borrowed (either mutably or immutably)
    /// somewhere else.
    pub fn try_borrow_mut(&self) -> Result<RefMut<Branch>, BorrowMutError> {
        self.0.try_borrow_mut()
    }

    /// Converts current branch data into a [Value]. It uses a type ref information to resolve,
    /// which value variant is a correct one for this branch. Since branch represent only complex
    /// types [Value::Any] will never be returned from this method.  
    pub fn into_value(self, txn: &Transaction) -> Value {
        let type_ref = { self.as_ref().type_ref() };
        match type_ref {
            TYPE_REFS_ARRAY => Value::YArray(Array::from(self)),
            TYPE_REFS_MAP => Value::YMap(Map::from(self)),
            TYPE_REFS_TEXT => Value::YText(Text::from(self)),
            TYPE_REFS_XML_ELEMENT => Value::YXmlElement(XmlElement::from(self)),
            TYPE_REFS_XML_FRAGMENT => Value::YXmlElement(XmlElement::from(self)),
            TYPE_REFS_XML_TEXT => Value::YXmlText(XmlText::from(self)),
            //TYPE_REFS_XML_HOOK => Value::YXmlElement(XmlElement::from(self)),
            other => panic!("Cannot convert to value - unsupported type ref: {}", other),
        }
    }

    /// Removes up to a `len` of countable elements from current branch sequence, starting at the
    /// given `index`. Returns number of removed elements.
    pub(crate) fn remove_at(&self, txn: &mut Transaction, index: u32, len: u32) -> u32 {
        let mut remaining = len;
        let start = {
            let parent = self.borrow();
            parent.start
        };
        let (_, mut ptr) = if index == 0 {
            (None, start)
        } else {
            Branch::index_to_ptr(txn, start, index)
        };
        while remaining > 0 {
            if let Some(mut p) = ptr {
                if let Some(item) = txn.store.blocks.get_item(&p) {
                    if !item.is_deleted() {
                        let item_len = item.len();
                        let (l, r) = if remaining < item_len {
                            p.id.clock += remaining;
                            remaining = 0;
                            txn.store.blocks.split_block(&p)
                        } else {
                            remaining -= item_len;
                            (ptr, item.right.clone())
                        };
                        txn.delete(&l.unwrap());
                        ptr = r;
                    } else {
                        ptr = item.right.clone();
                    }
                }
            } else {
                break;
            }
        }

        len - remaining
    }

    /// Inserts a preliminary `value` into a current branch indexed sequence component at the given
    /// `index`. Returns an item reference created as a result of this operation.
    pub(crate) fn insert_at<'t, V: Prelim>(
        &self,
        txn: &'t mut Transaction,
        index: u32,
        value: V,
    ) -> &'t Item {
        let (start, parent) = {
            let parent = self.borrow();
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

        txn.create_item(&pos, value, None)
    }
}

impl AsRef<Branch> for BranchRef {
    fn as_ref<'a>(&'a self) -> &'a Branch {
        unsafe { &*self.0.as_ptr() as &'a Branch }
    }
}

impl Eq for BranchRef {}

#[cfg(not(test))]
impl PartialEq for BranchRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[cfg(test)]
impl PartialEq for BranchRef {
    fn eq(&self, other: &Self) -> bool {
        if Rc::ptr_eq(&self.0, &other.0) {
            true
        } else {
            self.0.borrow().eq(&other.0.borrow())
        }
    }
}

/// Branch describes a content of a complex Yrs data structures, such as arrays or maps.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Branch {
    /// A pointer to a first block of a indexed sequence component of this branch node. If `None`,
    /// it means that sequence is empty or a branch doesn't act as an indexed sequence. Indexed
    /// sequences include:
    ///
    /// - [Array]: all elements are stored as a double linked list, while the head of the list is
    ///   kept in this field.
    /// - [XmlElement]: this field acts as a head to a first child element stored within current XML
    ///   node.
    /// - [Text] and [XmlText]: this field point to a first chunk of text appended to collaborative
    ///   text data structure.
    pub start: Option<BlockPtr>,

    /// A map component of this branch node, used by some of the specialized complex types
    /// including:
    ///
    /// - [Map]: all of the map elements are based on this field. The value of each entry points
    ///   to the last modified value.
    /// - [XmlElement]: this field stores attributes assigned to a given XML node.
    pub map: HashMap<String, BlockPtr>,

    /// Unique identifier of a current branch node. It can be contain either a named string - which
    /// means, this branch is a root-level complex data structure - or a block identifier. In latter
    /// case it means, that this branch is a complex type (eg. Map or Array) nested inside of
    /// another complex type.
    pub ptr: TypePtr,

    /// A tag name identifier, used only by [XmlElement].
    pub name: Option<String>,

    pub item: Option<BlockPtr>, //TODO: isn't this equivalent to `ptr` field?

    /// A length of an indexed sequence component of a current branch node. Map component elements
    /// are computed on demand.
    pub len: u32,

    /// An identifier of an underlying complex data type (eg. is it an Array or a Map).
    type_ref: TypeRefs,
}

impl Branch {
    pub fn new(ptr: TypePtr, type_ref: TypeRefs, name: Option<String>) -> Self {
        Self {
            start: None,
            map: HashMap::default(),
            len: 0,
            item: None,
            ptr,
            name,
            type_ref,
        }
    }

    /// Returns an identifier of an underlying complex data type (eg. is it an Array or a Map).
    pub fn type_ref(&self) -> TypeRefs {
        self.type_ref & 0b1111
    }

    /// Returns a length of an indexed sequence component of a current branch node.
    /// Map component elements are computed on demand.
    pub fn len(&self) -> u32 {
        self.len
    }

    /// Get iterator over (String, Block) entries of a map component of a current root type.
    /// Deleted blocks are skipped by this iterator.
    pub(crate) fn entries<'a, 'b, 'txn>(&'a self, txn: &'b Transaction<'txn>) -> Entries<'b, 'txn> {
        Entries::new(&self.ptr, txn)
    }

    /// Get iterator over Block entries of an array component of a current root type.
    /// Deleted blocks are skipped by this iterator.
    pub(crate) fn iter<'a, 'b, 'txn>(&'a self, txn: &'b Transaction<'txn>) -> Iter<'b, 'txn> {
        Iter::new(self.start, txn)
    }

    /// Returns a materialized value of non-deleted entry under a given `key` of a map component
    /// of a current root type.
    pub(crate) fn get(&self, txn: &Transaction<'_>, key: &str) -> Option<Value> {
        let ptr = self.map.get(key)?;
        let item = txn.store.blocks.get_item(ptr)?;
        if item.is_deleted() {
            None
        } else {
            item.content.get_content_last(txn)
        }
    }

    /// Given an `index` parameter, returns an item content reference which contains that index
    /// together with an offset inside of this content, which points precisely to an `index`
    /// location within wrapping item content.
    /// If `index` was outside of the array component boundary of current branch node, `None` will
    /// be returned.
    pub(crate) fn get_at<'a, 'b>(
        &'a self,
        txn: &'b Transaction,
        mut index: u32,
    ) -> Option<(&'b ItemContent, usize)> {
        let mut ptr = self.start;
        while let Some(p) = ptr {
            let item = txn.store.blocks.get_item(&p)?;
            let len = item.len();
            if !item.is_deleted() && item.is_countable() {
                if index < len {
                    return Some((&item.content, index as usize));
                }
            }
            index -= len;
            ptr = item.right.clone();
        }

        None
    }

    /// Removes an entry under given `key` of a map component of a current root type, returning
    /// a materialized representation of value stored underneath if entry existed prior deletion.
    pub(crate) fn remove(&self, txn: &mut Transaction, key: &str) -> Option<Value> {
        let ptr = self.map.get(key)?;
        let prev = {
            let item = txn.store.blocks.get_item(ptr)?;
            if item.is_deleted() {
                None
            } else {
                item.content.get_content_last(txn)
            }
        };
        txn.delete(ptr);
        prev
    }

    /// Returns a first non-deleted item from an array component of a current root type.
    pub(crate) fn first<'a, 'b>(&'a self, txn: &'b Transaction) -> Option<&'b Item> {
        let mut ptr = self.start;
        while let Some(p) = ptr {
            let item = txn.store.blocks.get_item(&p)?;
            if item.is_deleted() {
                ptr = item.right.clone();
            } else {
                return Some(item);
            }
        }

        None
    }

    /// Given an `index` and start block `ptr`, returns a pair of block pointers.
    ///
    /// If `index` happens to point inside of an existing block content, such block will be split at
    /// position of an `index`. In such case left tuple value contains start of a block pointer on
    /// a left side of an `index` and a pointer to a block directly on the right side of an `index`.
    ///
    /// If `index` point to the end of a block and no splitting is necessary, tuple will return only
    /// left side (beginning of a block), while right side will be `None`.
    ///
    /// If `index` is outside of the range of an array component of current branch node, both tuple
    /// values will be `None`.
    fn index_to_ptr(
        txn: &mut Transaction,
        mut ptr: Option<BlockPtr>,
        mut index: u32,
    ) -> (Option<BlockPtr>, Option<BlockPtr>) {
        while let Some(p) = ptr {
            let item = txn
                .store
                .blocks
                .get_item(&p)
                .expect("No item for a given pointer was found.");
            let len = item.len();
            if !item.is_deleted() && item.is_countable() {
                if index == len {
                    let left = Some(p.clone());
                    let right = item.right.clone();
                    return (left, right);
                } else if index < len {
                    let split_point = ID::new(item.id.client, item.id.clock + index);
                    let ptr = BlockPtr::new(split_point, p.pivot() as u32);
                    let (left, mut right) = txn.store.blocks.split_block(&ptr);
                    if right.is_none() {
                        if let Some(left_ptr) = left.as_ref() {
                            if let Some(left) = txn.store.blocks.get_item(left_ptr) {
                                right = left.right.clone();
                            }
                        }
                    }
                    return (left, right);
                }
                index -= len;
            }
            ptr = item.right.clone();
        }
        (None, None)
    }
}

/// Value that can be returned by Yrs data types. This includes [Any] which is an extension
/// representation of JSON, but also nested complex collaborative structures specific to Yrs.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Primitive value.
    Any(Any),
    YText(Text),
    YArray(Array),
    YMap(Map),
    YXmlElement(XmlElement),
    YXmlText(XmlText),
}

impl Value {
    /// Converts current value into [Any] object equivalent that resembles enhanced JSON payload.
    /// Rules are:
    ///
    /// - Primitive types ([Value::Any]) are passed right away, as no transformation is needed.
    /// - [Value::YArray] is converted into JSON-like array.
    /// - [Value::YMap] is converted into JSON-like object map.
    /// - [Value::YText], [Value::YXmlText] and [Value::YXmlElement] are converted into strings
    ///   (XML types are stringified XML representation).
    pub fn to_json(self, txn: &Transaction) -> Any {
        match self {
            Value::Any(a) => a,
            Value::YText(v) => Any::String(v.to_string(txn)),
            Value::YArray(v) => v.to_json(txn),
            Value::YMap(v) => v.to_json(txn),
            Value::YXmlElement(v) => Any::String(v.to_string(txn)),
            Value::YXmlText(v) => Any::String(v.to_string(txn)),
        }
    }

    /// Converts current value into stringified representation.
    pub fn to_string(self, txn: &Transaction) -> String {
        match self {
            Value::Any(a) => a.to_string(),
            Value::YText(v) => v.to_string(txn),
            Value::YArray(v) => v.to_json(txn).to_string(),
            Value::YMap(v) => v.to_json(txn).to_string(),
            Value::YXmlElement(v) => v.to_string(txn),
            Value::YXmlText(v) => v.to_string(txn),
        }
    }
}

impl<T> From<T> for Value
where
    T: Into<Any>,
{
    fn from(v: T) -> Self {
        let any: Any = v.into();
        Value::Any(any)
    }
}

impl std::fmt::Display for Branch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.type_ref() {
            TYPE_REFS_ARRAY => write!(f, "YArray(start: {})", self.start.unwrap()),
            TYPE_REFS_MAP => {
                write!(f, "YMap(")?;
                let mut iter = self.map.iter();
                if let Some((k, v)) = iter.next() {
                    write!(f, "'{}': {}", k, v)?;
                }
                while let Some((k, v)) = iter.next() {
                    write!(f, ", '{}': {}", k, v)?;
                }
                write!(f, ")")
            }
            TYPE_REFS_TEXT => write!(f, "YText(start: {})", self.start.unwrap()),
            TYPE_REFS_XML_ELEMENT => {
                write!(f, "YXmlElement")?;
                if let Some(start) = self.start.as_ref() {
                    write!(f, "(start: {})", start)?;
                }
                if !self.map.is_empty() {
                    write!(f, " {{")?;
                    let mut iter = self.map.iter();
                    if let Some((k, v)) = iter.next() {
                        write!(f, "'{}': {}", k, v)?;
                    }
                    while let Some((k, v)) = iter.next() {
                        write!(f, ", '{}': {}", k, v)?;
                    }
                    write!(f, "}}")?;
                }
                Ok(())
            }
            TYPE_REFS_XML_HOOK => {
                write!(f, "YXmlHook(")?;
                let mut iter = self.map.iter();
                if let Some((k, v)) = iter.next() {
                    write!(f, "'{}': {}", k, v)?;
                }
                while let Some((k, v)) = iter.next() {
                    write!(f, ", '{}': {}", k, v)?;
                }
                write!(f, ")")
            }
            TYPE_REFS_XML_TEXT => write!(f, "YXmlText(start: {})", self.start.unwrap()),
            other => {
                write!(f, "UnknownRef")?;
                if let Some(start) = self.start.as_ref() {
                    write!(f, "(start: {})", start)?;
                }
                if !self.map.is_empty() {
                    write!(f, " {{")?;
                    let mut iter = self.map.iter();
                    if let Some((k, v)) = iter.next() {
                        write!(f, "'{}': {}", k, v)?;
                    }
                    while let Some((k, v)) = iter.next() {
                        write!(f, ", '{}': {}", k, v)?;
                    }
                    write!(f, "}}")?;
                }
                Ok(())
            }
        }
    }
}

pub(crate) struct Entries<'a, 'txn> {
    pub txn: &'a Transaction<'txn>,
    iter: std::collections::hash_map::Iter<'a, String, BlockPtr>,
}

impl<'a, 'txn> Entries<'a, 'txn> {
    pub(crate) fn new<'b>(ptr: &'b TypePtr, txn: &'a Transaction<'txn>) -> Self {
        let inner = txn.store.get_type(ptr).unwrap();
        let iter = inner.as_ref().map.iter();
        Entries { txn, iter }
    }
}

impl<'a, 'txn> Iterator for Entries<'a, 'txn> {
    type Item = (&'a String, &'a Item);

    fn next(&mut self) -> Option<Self::Item> {
        let (mut key, ptr) = self.iter.next()?;
        let mut block = self.txn.store.blocks.get_item(ptr);
        loop {
            match block {
                Some(item) if !item.is_deleted() => {
                    break;
                }
                _ => {
                    let (k, ptr) = self.iter.next()?;
                    key = k;
                    block = self.txn.store.blocks.get_item(ptr);
                }
            }
        }
        let item = block.unwrap();
        Some((key, item))
    }
}

pub(crate) struct Iter<'a, 'txn> {
    ptr: Option<BlockPtr>,
    txn: &'a Transaction<'txn>,
}

impl<'a, 'txn> Iter<'a, 'txn> {
    fn new(start: Option<BlockPtr>, txn: &'a Transaction<'txn>) -> Self {
        Iter { ptr: start, txn }
    }
}

impl<'a, 'txn> Iterator for Iter<'a, 'txn> {
    type Item = &'a Item;

    fn next(&mut self) -> Option<Self::Item> {
        let ptr = self.ptr.take()?;
        let item = self.txn.store.blocks.get_item(&ptr)?;
        self.ptr = item.right;
        Some(item)
    }
}

/// Type pointer - used to localize a complex [Branch] node within a scope of a document store.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypePtr {
    /// Temporary value - used only when block is deserialized right away, but had not been
    /// integrated into block store yet. As part of block integration process, items are
    /// repaired and their fields (including parent) are being rewired.
    Unknown,

    /// Pointer to another block. Used in nested data types ie. YMap containing another YMap.
    Id(block::BlockPtr),

    /// Pointer to a root-level type.
    Named(Rc<String>),
}

impl std::fmt::Display for TypePtr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TypePtr::Unknown => write!(f, "unknown"),
            TypePtr::Id(ptr) => write!(f, "{}", ptr),
            TypePtr::Named(name) => write!(f, "'{}'", name),
        }
    }
}
