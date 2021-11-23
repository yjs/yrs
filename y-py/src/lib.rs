#![feature()]

use lib0::any::Any;
use pyo3::exceptions::PyIndexError;
use pyo3::prelude::*;
use pyo3::types as pytypes;
use pyo3::types::PyTuple;
use pyo3::types::{PyAny, PyByteArray, PyDict};
use pyo3::wrap_pyfunction;
use std::borrow::Borrow;
use std::cell::Ref;
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use yrs;
use yrs::block::{ItemContent, Prelim};
use yrs::types::array::ArrayIter;
use yrs::types::map::MapIter;
use yrs::types::xml::{Attributes, TreeWalker};
use yrs::types::{
    Branch, BranchRef, TypePtr, TypeRefs, Value, TYPE_REFS_ARRAY, TYPE_REFS_MAP, TYPE_REFS_TEXT,
    TYPE_REFS_XML_ELEMENT, TYPE_REFS_XML_TEXT,
};
use yrs::updates::decoder::{Decode, DecoderV1};
use yrs::updates::encoder::{Encode, Encoder, EncoderV1};
use yrs::{
    Array, DeleteSet, Doc, Map, StateVector, Text, Transaction, Update, Xml, XmlElement, XmlText,
};

/// A ywasm document type. Documents are most important units of collaborative resources management.
/// All shared collections live within a scope of their corresponding documents. All updates are
/// generated on per document basis (rather than individual shared type). All operations on shared
/// collections happen via [YTransaction], which lifetime is also bound to a document.
///
/// Document manages so called root types, which are top-level shared types definitions (as opposed
/// to recursively nested types).
///
/// A basic workflow sample:
///
/// ```javascript
/// import YDoc from 'ywasm'
///
/// const doc = new YDoc()
/// const txn = doc.beginTransaction()
/// try {
///     const text = txn.getText('name')
///     text.push(txn, 'hello world')
///     const output = text.toString(txn)
///     console.log(output)
/// } finally {
///     txn.free()
/// }
/// ```
#[pyclass(unsendable)]
pub struct YDoc {
    inner: Doc,
}

#[pymethods]
impl YDoc {
    /// Creates a new ywasm document. If `id` parameter was passed it will be used as this document
    /// globally unique identifier (it's up to caller to ensure that requirement). Otherwise it will
    /// be assigned a randomly generated number.
    #[new]
    pub fn new(id: Option<f64>) -> Self {
        if let Some(id) = id {
            YDoc {
                inner: Doc::with_client_id(id as u64),
            }
        } else {
            YDoc { inner: Doc::new() }
        }
    }

    /// Gets globally unique identifier of this `YDoc` instance.
    #[getter]
    pub fn id(&self) -> f64 {
        self.inner.client_id as f64
    }

    /// Returns a new transaction for this document. Ywasm shared data types execute their
    /// operations in a context of a given transaction. Each document can have only one active
    /// transaction at the time - subsequent attempts will cause exception to be thrown.
    ///
    /// Transactions started with `doc.beginTransaction` can be released using `transaction.free`
    /// method.
    ///
    /// Example:
    ///
    /// ```javascript
    /// import YDoc from 'ywasm'
    ///
    /// // helper function used to simplify transaction
    /// // create/release cycle
    /// YDoc.prototype.transact = callback => {
    ///     const txn = this.beginTransaction()
    ///     try {
    ///         return callback(txn)
    ///     } finally {
    ///         txn.free()
    ///     }
    /// }
    ///
    /// const doc = new YDoc()
    /// const text = doc.getText('name')
    /// doc.transact(txn => text.insert(txn, 0, 'hello world'))
    /// ```
    pub fn begin_transaction(&mut self) -> YTransaction {
        unsafe {
            let doc: *mut Doc = &mut self.inner;
            let static_txn: ManuallyDrop<Transaction<'static>> =
                ManuallyDrop::new((*doc).transact());
            YTransaction(static_txn)
        }
    }

    pub fn transact(&mut self, callback: PyObject) -> PyResult<PyObject> {
        let txn = self.begin_transaction();
        Python::with_gil(|py| {
            let args = PyTuple::new(py, std::iter::once(txn.into_py(py)));
            callback.call(py, args, None)
        })
    }

    /// Returns a `YMap` shared data type, that's accessible for subsequent accesses using given
    /// `name`.
    ///
    /// If there was no instance with this name before, it will be created and then returned.
    ///
    /// If there was an instance with this name, but it was of different type, it will be projected
    /// onto `YMap` instance.
    // pub fn get_map(&mut self, name: &str) -> YMap {
    //     self.begin_transaction().get_map(name)
    // }

    /// Returns a `YXmlElement` shared data type, that's accessible for subsequent accesses using
    /// given `name`.
    ///
    /// If there was no instance with this name before, it will be created and then returned.
    ///
    /// If there was an instance with this name, but it was of different type, it will be projected
    /// onto `YXmlElement` instance.
    // pub fn get_xml_element(&mut self, name: &str) -> YXmlElement {
    //     self.begin_transaction().get_xml_element(name)
    // }

    /// Returns a `YXmlText` shared data type, that's accessible for subsequent accesses using given
    /// `name`.
    ///
    /// If there was no instance with this name before, it will be created and then returned.
    ///
    /// If there was an instance with this name, but it was of different type, it will be projected
    /// onto `YXmlText` instance.
    // pub fn get_xml_text(&mut self, name: &str) -> YXmlText {
    //     self.begin_transaction().get_xml_text(name)
    // }

    /// Returns a `YArray` shared data type, that's accessible for subsequent accesses using given
    /// `name`.
    ///
    /// If there was no instance with this name before, it will be created and then returned.
    ///
    /// If there was an instance with this name, but it was of different type, it will be projected
    /// onto `YArray` instance.
    pub fn get_array(&mut self, name: &str) -> YArray {
        self.begin_transaction().get_array(name)
    }

    /// Returns a `YText` shared data type, that's accessible for subsequent accesses using given
    /// `name`.
    ///
    /// If there was no instance with this name before, it will be created and then returned.
    ///
    /// If there was an instance with this name, but it was of different type, it will be projected
    /// onto `YText` instance.
    pub fn get_text(&mut self, name: &str) -> YText {
        self.begin_transaction().get_text(name)
    }
}

/// Encodes a state vector of a given ywasm document into its binary representation using lib0 v1
/// encoding. State vector is a compact representation of updates performed on a given document and
/// can be used by `encode_state_as_update` on remote peer to generate a delta update payload to
/// synchronize changes between peers.
///
/// Example:
///
/// ```javascript
/// import {YDoc, encodeStateVector, encodeStateAsUpdate, applyUpdate} from 'ywasm'
///
/// /// document on machine A
/// const localDoc = new YDoc()
/// const localSV = encodeStateVector(localDoc)
///
/// // document on machine B
/// const remoteDoc = new YDoc()
/// const remoteDelta = encodeStateAsUpdate(remoteDoc, localSV)
///
/// applyUpdate(localDoc, remoteDelta)
/// ```
#[pyfunction]
pub fn encode_state_vector(doc: &mut YDoc) -> Vec<u8> {
    doc.begin_transaction().state_vector_v1()
}

/// Encodes all updates that have happened since a given version `vector` into a compact delta
/// representation using lib0 v1 encoding. If `vector` parameter has not been provided, generated
/// delta payload will contain all changes of a current ywasm document, working effectivelly as its
/// state snapshot.
///
/// Example:
///
/// ```javascript
/// import {YDoc, encodeStateVector, encodeStateAsUpdate, applyUpdate} from 'ywasm'
///
/// /// document on machine A
/// const localDoc = new YDoc()
/// const localSV = encodeStateVector(localDoc)
///
/// // document on machine B
/// const remoteDoc = new YDoc()
/// const remoteDelta = encodeStateAsUpdate(remoteDoc, localSV)
///
/// applyUpdate(localDoc, remoteDelta)
/// ```
#[pyfunction]
pub fn encode_state_as_update(doc: &mut YDoc, vector: Option<Vec<u8>>) -> Vec<u8> {
    doc.begin_transaction().diff_v1(vector)
}

/// Applies delta update generated by the remote document replica to a current document. This
/// method assumes that a payload maintains lib0 v1 encoding format.
///
/// Example:
///
/// ```javascript
/// import {YDoc, encodeStateVector, encodeStateAsUpdate, applyUpdate} from 'ywasm'
///
/// /// document on machine A
/// const localDoc = new YDoc()
/// const localSV = encodeStateVector(localDoc)
///
/// // document on machine B
/// const remoteDoc = new YDoc()
/// const remoteDelta = encodeStateAsUpdate(remoteDoc, localSV)
///
/// applyUpdate(localDoc, remoteDelta)
/// ```
#[pyfunction]
pub fn apply_update(doc: &mut YDoc, diff: Vec<u8>) {
    doc.begin_transaction().apply_v1(diff);
}

/// A transaction that serves as a proxy to document block store. Ywasm shared data types execute
/// their operations in a context of a given transaction. Each document can have only one active
/// transaction at the time - subsequent attempts will cause exception to be thrown.
///
/// Transactions started with `doc.beginTransaction` can be released using `transaction.free`
/// method.
///
/// Example:
///
/// ```javascript
/// import YDoc from 'ywasm'
///
/// // helper function used to simplify transaction
/// // create/release cycle
/// YDoc.prototype.transact = callback => {
///     const txn = this.beginTransaction()
///     try {
///         return callback(txn)
///     } finally {
///         txn.free()
///     }
/// }
///
/// const doc = new YDoc()
/// const text = doc.getText('name')
/// doc.transact(txn => text.insert(txn, 0, 'hello world'))
/// ```
#[pyclass(unsendable)]
pub struct YTransaction(ManuallyDrop<Transaction<'static>>);

impl Deref for YTransaction {
    type Target = Transaction<'static>;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl DerefMut for YTransaction {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}

impl Drop for YTransaction {
    fn drop(&mut self) {
        unsafe { ManuallyDrop::drop(&mut self.0) }
    }
}

#[pymethods]
impl YTransaction {
    /// Returns a `YText` shared data type, that's accessible for subsequent accesses using given
    /// `name`.
    ///
    /// If there was no instance with this name before, it will be created and then returned.
    ///
    /// If there was an instance with this name, but it was of different type, it will be projected
    /// onto `YText` instance.
    pub fn get_text(&mut self, name: &str) -> YText {
        self.0.get_text(name).into()
    }

    /// Returns a `YArray` shared data type, that's accessible for subsequent accesses using given
    /// `name`.
    ///
    /// If there was no instance with this name before, it will be created and then returned.
    ///
    /// If there was an instance with this name, but it was of different type, it will be projected
    /// onto `YArray` instance.
    pub fn get_array(&mut self, name: &str) -> YArray {
        self.0.get_array(name).into()
    }

    /// Returns a `YMap` shared data type, that's accessible for subsequent accesses using given
    /// `name`.
    ///
    /// If there was no instance with this name before, it will be created and then returned.
    ///
    /// If there was an instance with this name, but it was of different type, it will be projected
    /// onto `YMap` instance.
    // pub fn get_map(&mut self, name: &str) -> YMap {
    //     self.inner.get_map(name).into()
    // }

    /// Returns a `YXmlElement` shared data type, that's accessible for subsequent accesses using
    /// given `name`.
    ///
    /// If there was no instance with this name before, it will be created and then returned.
    ///
    /// If there was an instance with this name, but it was of different type, it will be projected
    /// onto `YXmlElement` instance.
    // pub fn get_xml_element(&mut self, name: &str) -> YXmlElement {
    //     YXmlElement(self.inner.get_xml_element(name))
    // }

    /// Returns a `YXmlText` shared data type, that's accessible for subsequent accesses using given
    /// `name`.
    ///
    /// If there was no instance with this name before, it will be created and then returned.
    ///
    /// If there was an instance with this name, but it was of different type, it will be projected
    /// onto `YXmlText` instance.
    // pub fn get_xml_text(&mut self, name: &str) -> YXmlText {
    //     YXmlText(self.inner.get_xml_text(name))
    // }

    /// Triggers a post-update series of operations without `free`ing the transaction. This includes
    /// compaction and optimization of internal representation of updates, triggering events etc.
    /// ywasm transactions are auto-committed when they are `free`d.
    pub fn commit(&mut self) {
        self.0.commit()
    }

    /// Encodes a state vector of a given transaction document into its binary representation using
    /// lib0 v1 encoding. State vector is a compact representation of updates performed on a given
    /// document and can be used by `encode_state_as_update` on remote peer to generate a delta
    /// update payload to synchronize changes between peers.
    ///
    /// Example:
    ///
    /// ```javascript
    /// import YDoc from 'ywasm'
    ///
    /// /// document on machine A
    /// const localDoc = new YDoc()
    /// const localTxn = localDoc.beginTransaction()
    ///
    /// // document on machine B
    /// const remoteDoc = new YDoc()
    /// const remoteTxn = localDoc.beginTransaction()
    ///
    /// try {
    ///     const localSV = localTxn.stateVectorV1()
    ///     const remoteDelta = remoteTxn.diffV1(localSv)
    ///     localTxn.applyV1(remoteDelta)
    /// } finally {
    ///     localTxn.free()
    ///     remoteTxn.free()
    /// }
    /// ```
    pub fn state_vector_v1(&self) -> Vec<u8> {
        let sv = self.0.state_vector();
        let payload = sv.encode_v1();
        payload
    }

    /// Encodes all updates that have happened since a given version `vector` into a compact delta
    /// representation using lib0 v1 encoding. If `vector` parameter has not been provided, generated
    /// delta payload will contain all changes of a current ywasm document, working effectively as
    /// its state snapshot.
    ///
    /// Example:
    ///
    /// ```javascript
    /// import YDoc from 'ywasm'
    ///
    /// /// document on machine A
    /// const localDoc = new YDoc()
    /// const localTxn = localDoc.beginTransaction()
    ///
    /// // document on machine B
    /// const remoteDoc = new YDoc()
    /// const remoteTxn = localDoc.beginTransaction()
    ///
    /// try {
    ///     const localSV = localTxn.stateVectorV1()
    ///     const remoteDelta = remoteTxn.diffV1(localSv)
    ///     localTxn.applyV1(remoteDelta)
    /// } finally {
    ///     localTxn.free()
    ///     remoteTxn.free()
    /// }
    /// ```
    pub fn diff_v1(&self, vector: Option<Vec<u8>>) -> Vec<u8> {
        let mut encoder = EncoderV1::new();
        let sv = if let Some(vector) = vector {
            StateVector::decode_v1(vector.to_vec().as_slice())
        } else {
            StateVector::default()
        };
        self.0.encode_diff(&sv, &mut encoder);
        encoder.to_vec()
    }

    /// Applies delta update generated by the remote document replica to a current transaction's
    /// document. This method assumes that a payload maintains lib0 v1 encoding format.
    ///
    /// Example:
    ///
    /// ```javascript
    /// import YDoc from 'ywasm'
    ///
    /// /// document on machine A
    /// const localDoc = new YDoc()
    /// const localTxn = localDoc.beginTransaction()
    ///
    /// // document on machine B
    /// const remoteDoc = new YDoc()
    /// const remoteTxn = localDoc.beginTransaction()
    ///
    /// try {
    ///     const localSV = localTxn.stateVectorV1()
    ///     const remoteDelta = remoteTxn.diffV1(localSv)
    ///     localTxn.applyV1(remoteDelta)
    /// } finally {
    ///     localTxn.free()
    ///     remoteTxn.free()
    /// }
    /// ```
    pub fn apply_v1(&mut self, diff: Vec<u8>) {
        let diff: Vec<u8> = diff.to_vec();
        let mut decoder = DecoderV1::from(diff.as_slice());
        let update = Update::decode(&mut decoder);
        self.0.apply_update(update)
    }

    fn __enter__<'p>(slf: PyRef<'p, Self>, _py: Python<'p>) -> PyResult<PyRef<'p, Self>> {
        Ok(slf)
    }

    fn __exit__<'p>(
        &'p mut self,
        _exc_type: Option<&'p PyAny>,
        _exc_value: Option<&'p PyAny>,
        _traceback: Option<&'p PyAny>,
    ) -> PyResult<bool> {
        self.commit();
        drop(self);
        return Ok(true);
    }
}

enum SharedType<T, P> {
    Integrated(T),
    Prelim(P),
}

impl<T, P> SharedType<T, P> {
    #[inline(always)]
    fn new(value: T) -> RefCell<Self> {
        RefCell::new(SharedType::Integrated(value))
    }

    #[inline(always)]
    fn prelim(prelim: P) -> RefCell<Self> {
        RefCell::new(SharedType::Prelim(prelim))
    }
}

/// A shared data type used for collaborative text editing. It enables multiple users to add and
/// remove chunks of text in efficient manner. This type is internally represented as a mutable
/// double-linked list of text chunks - an optimization occurs during `YTransaction.commit`, which
/// allows to squash multiple consecutively inserted characters together as a single chunk of text
/// even between transaction boundaries in order to preserve more efficient memory model.
///
/// `YText` structure internally uses UTF-8 encoding and its length is described in a number of
/// bytes rather than individual characters (a single UTF-8 code point can consist of many bytes).
///
/// Like all Yrs shared data types, `YText` is resistant to the problem of interleaving (situation
/// when characters inserted one after another may interleave with other peers concurrent inserts
/// after merging all updates together). In case of Yrs conflict resolution is solved by using
/// unique document id to determine correct and consistent ordering.
#[pyclass(unsendable)]
#[derive(Clone)]
pub struct YText(Rc<RefCell<SharedType<Text, String>>>);

impl From<Text> for YText {
    fn from(v: Text) -> Self {
        YText(Rc::new(SharedType::new(v)))
    }
}

#[pymethods]
impl YText {
    /// Creates a new preliminary instance of a `YText` shared data type, with its state initialized
    /// to provided parameter.
    ///
    /// Preliminary instances can be nested into other shared data types such as `YArray` and `YMap`.
    /// Once a preliminary instance has been inserted this way, it becomes integrated into ywasm
    /// document store and cannot be nested again: attempt to do so will result in an exception.
    #[new]
    pub fn new(init: Option<String>) -> Self {
        YText(Rc::new(SharedType::prelim(init.unwrap_or_default())))
    }

    /// Returns true if this is a preliminary instance of `YText`.
    ///
    /// Preliminary instances can be nested into other shared data types such as `YArray` and `YMap`.
    /// Once a preliminary instance has been inserted this way, it becomes integrated into ywasm
    /// document store and cannot be nested again: attempt to do so will result in an exception.
    #[getter]
    pub fn prelim(&self) -> bool {
        let s = &*self.0.deref().borrow();
        match s {
            SharedType::Prelim(_) => true,
            _ => false,
        }
    }

    /// Returns length of an underlying string stored in this `YText` instance,
    /// understood as a number of UTF-8 encoded bytes.
    #[getter]
    pub fn length(&self) -> u32 {
        match &*self.0.deref().borrow() {
            SharedType::Integrated(v) => v.len(),
            SharedType::Prelim(v) => v.len() as u32,
        }
    }

    /// Returns an underlying shared string stored in this data type.
    // TODO: Make this a native __str__ dunder function
    pub fn to_string(&self, txn: &YTransaction) -> String {
        match &*self.0.deref().borrow() {
            SharedType::Integrated(v) => v.to_string(txn),
            SharedType::Prelim(v) => v.clone(),
        }
    }

    /// Returns an underlying shared string stored in this data type.
    pub fn to_json(&self, txn: &YTransaction) -> String {
        match &*self.0.deref().borrow() {
            SharedType::Integrated(v) => v.to_string(txn),
            SharedType::Prelim(v) => v.clone(),
        }
    }

    /// Inserts a given `chunk` of text into this `YText` instance, starting at a given `index`.
    pub fn insert(&self, txn: &mut YTransaction, index: u32, chunk: &str) {
        match &mut *self.0.deref().borrow_mut() {
            SharedType::Integrated(v) => v.insert(txn, index, chunk),
            SharedType::Prelim(v) => v.insert_str(index as usize, chunk),
        }
    }

    /// Appends a given `chunk` of text at the end of current `YText` instance.
    pub fn push(&self, txn: &mut YTransaction, chunk: &str) {
        match &mut *self.0.deref().borrow_mut() {
            SharedType::Integrated(v) => v.push(txn, chunk),
            SharedType::Prelim(v) => v.push_str(chunk),
        }
    }

    /// Deletes a specified range of of characters, starting at a given `index`.
    /// Both `index` and `length` are counted in terms of a number of UTF-8 character bytes.
    pub fn delete(&mut self, txn: &mut YTransaction, index: u32, length: u32) {
        match &mut *self.0.deref().borrow_mut() {
            SharedType::Integrated(v) => v.remove_range(txn, index, length),
            SharedType::Prelim(v) => {
                v.drain((index as usize)..(index + length) as usize);
            }
        }
    }
}

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
#[pyclass(unsendable)]
pub struct YArray(RefCell<SharedType<Array, Vec<PyObject>>>);

impl From<Array> for YArray {
    fn from(v: Array) -> Self {
        YArray(SharedType::new(v))
    }
}

#[pymethods]
impl YArray {
    /// Creates a new preliminary instance of a `YArray` shared data type, with its state
    /// initialized to provided parameter.
    ///
    /// Preliminary instances can be nested into other shared data types such as `YArray` and `YMap`.
    /// Once a preliminary instance has been inserted this way, it becomes integrated into ywasm
    /// document store and cannot be nested again: attempt to do so will result in an exception.
    #[new]
    pub fn new(init: Option<Vec<PyObject>>) -> Self {
        YArray(SharedType::prelim(init.unwrap_or_default()))
    }

    /// Returns true if this is a preliminary instance of `YArray`.
    ///
    /// Preliminary instances can be nested into other shared data types such as `YArray` and `YMap`.
    /// Once a preliminary instance has been inserted this way, it becomes integrated into ywasm
    /// document store and cannot be nested again: attempt to do so will result in an exception.
    #[getter]
    pub fn prelim(&self) -> bool {
        if let SharedType::Prelim(_) = &*self.0.borrow() {
            true
        } else {
            false
        }
    }

    /// Returns a number of elements stored within this instance of `YArray`.
    #[getter]
    pub fn length(&self) -> u32 {
        match &*self.0.borrow() {
            SharedType::Integrated(v) => v.len(),
            SharedType::Prelim(v) => v.len() as u32,
        }
    }

    /// Converts an underlying contents of this `YArray` instance into their JSON representation.
    pub fn to_json(&self, txn: &YTransaction) -> PyObject {
        Python::with_gil(|py| match &*self.0.borrow() {
            SharedType::Integrated(v) => AnyWrapper(v.to_json(txn)).into_py(py),
            SharedType::Prelim(v) => {
                let py_ptrs: Vec<PyObject> = v.iter().map(|ptr| ptr.clone()).collect();
                py_ptrs.into_py(py)
            }
        })
    }

    /// Inserts a given range of `items` into this `YArray` instance, starting at given `index`.
    pub fn insert(&self, txn: &mut YTransaction, index: u32, items: Vec<PyObject>) {
        let mut j = index;
        match &mut *self.0.borrow_mut() {
            SharedType::Integrated(array) => {
                insert_at(array, txn, index, items);
            }
            SharedType::Prelim(vec) => {
                for el in items {
                    vec.insert(j as usize, el);
                    j += 1;
                }
            }
        }
    }

    /// Appends a range of `items` at the end of this `YArray` instance.
    pub fn push(&self, txn: &mut YTransaction, items: Vec<PyObject>) {
        let index = self.length();
        self.insert(txn, index, items);
    }

    /// Deletes a range of items of given `length` from current `YArray` instance,
    /// starting from given `index`.
    pub fn delete(&self, txn: &mut YTransaction, index: u32, length: u32) {
        match &mut *self.0.borrow_mut() {
            SharedType::Integrated(v) => v.remove_range(txn, index, length),
            SharedType::Prelim(v) => {
                v.drain((index as usize)..(index + length) as usize);
            }
        }
    }

    /// Returns an element stored under given `index`.
    pub fn get(&self, txn: &YTransaction, index: u32) -> PyResult<PyObject> {
        match &*self.0.borrow() {
            SharedType::Integrated(v) => {
                if let Some(value) = v.get(txn, index) {
                    Ok(Python::with_gil(|py| ValueWrapper(value).into_py(py)))
                } else {
                    Err(PyIndexError::new_err(
                        "Index outside the bounds of an YArray",
                    ))
                }
            }
            SharedType::Prelim(v) => {
                if let Some(value) = v.get(index as usize) {
                    Ok(value.clone())
                } else {
                    Err(PyIndexError::new_err(
                        "Index outside the bounds of an YArray",
                    ))
                }
            }
        }
    }

    /// Returns an iterator that can be used to traverse over the values stored withing this
    /// instance of `YArray`.
    ///
    /// Example:
    ///
    /// ```javascript
    /// import YDoc from 'ywasm'
    ///
    /// /// document on machine A
    /// const doc = new YDoc()
    /// const array = doc.getArray('name')
    /// const txn = doc.beginTransaction()
    /// try {
    ///     array.push(txn, ['hello', 'world'])
    ///     for (let item of array.values(txn)) {
    ///         console.log(item)
    ///     }
    /// } finally {
    ///     txn.free()
    /// }
    /// ```
    pub fn values(&self, txn: &YTransaction) -> PyObject {
        Python::with_gil(|py| match &*self.0.borrow() {
            SharedType::Integrated(v) => unsafe {
                let this: *const Array = v;
                let tx: *const Transaction<'static> = txn.0.deref();
                let static_iter: ManuallyDrop<ArrayIter<'static, 'static>> =
                    ManuallyDrop::new((*this).iter(tx.as_ref().unwrap()));
                YArrayIterator(static_iter).into_py(py)
            },
            SharedType::Prelim(v) => unsafe {
                let this: *const Vec<PyObject> = v;
                let static_iter: ManuallyDrop<std::slice::Iter<'static, PyObject>> =
                    ManuallyDrop::new((*this).iter());
                PrelimArrayIterator(static_iter).into_py(py)
            },
        })
    }
}

#[pyclass]
pub struct IteratorNext {
    value: PyObject,
    done: bool,
}

#[pymethods]
impl IteratorNext {
    #[new]
    fn new(value: PyObject) -> Self {
        IteratorNext { done: false, value }
    }

    #[staticmethod] // TODO: Check if this is really static
    fn finished() -> Self {
        Python::with_gil(|py| -> IteratorNext {
            IteratorNext {
                done: true,
                value: py.None(),
            }
        })
    }

    #[getter]
    pub fn value(&self) -> PyObject {
        // TODO: should we clone?
        self.value.clone()
    }

    #[getter]
    pub fn done(&self) -> bool {
        self.done
    }
}

impl From<Option<Value>> for IteratorNext {
    fn from(v: Option<Value>) -> Self {
        match v {
            None => IteratorNext::finished(),
            Some(v) => Python::with_gil(|py| IteratorNext::new(ValueWrapper(v).into_py(py))),
        }
    }
}

#[pyclass(unsendable)]
pub struct YArrayIterator(ManuallyDrop<ArrayIter<'static, 'static>>);

impl Drop for YArrayIterator {
    fn drop(&mut self) {
        unsafe { ManuallyDrop::drop(&mut self.0) }
    }
}

#[pymethods]
impl YArrayIterator {
    pub fn __iter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    pub fn __next__(mut slf: PyRefMut<Self>) -> IteratorNext {
        slf.0.next().into()
    }
}

#[pyclass(unsendable)]
pub struct PrelimArrayIterator(ManuallyDrop<std::slice::Iter<'static, PyObject>>);

impl Drop for PrelimArrayIterator {
    fn drop(&mut self) {
        unsafe { ManuallyDrop::drop(&mut self.0) }
    }
}

#[pymethods]
impl PrelimArrayIterator {
    pub fn next(&mut self) -> IteratorNext {
        if let Some(py) = self.0.next() {
            let py = py.clone();
            IteratorNext::new(py)
        } else {
            IteratorNext::finished()
        }
    }
}

/// Collection used to store key-value entries in an unordered manner. Keys are always represented
/// as UTF-8 strings. Values can be any value type supported by Yrs: JSON-like primitives as well as
/// shared data types.
///
/// In terms of conflict resolution, [Map] uses logical last-write-wins principle, meaning the past
/// updates are automatically overridden and discarded by newer ones, while concurrent updates made
/// by different peers are resolved into a single value using document id seniority to establish
/// order.
// #[pyclass]
// pub struct YMap(RefCell<SharedType<Map, HashMap<String, PyAny>>>);

// impl From<Map> for YMap {
//     fn from(v: Map) -> Self {
//         YMap(SharedType::new(v))
//     }
// }

// #[pymethods]
// impl YMap {
//     /// Creates a new preliminary instance of a `YMap` shared data type, with its state
//     /// initialized to provided parameter.
//     ///
//     /// Preliminary instances can be nested into other shared data types such as `YArray` and `YMap`.
//     /// Once a preliminary instance has been inserted this way, it becomes integrated into ywasm
//     /// document store and cannot be nested again: attempt to do so will result in an exception.
//     #[new]
//     pub fn new(init: Option<js_sys::Object>) -> Self {
//         let map = if let Some(object) = init {
//             let mut map = HashMap::new();
//             let entries = js_sys::Object::entries(&object);
//             for tuple in entries.iter() {
//                 let tuple = js_sys::Array::from(&tuple);
//                 let key = tuple.get(0).as_string().unwrap();
//                 let value = tuple.get(1);
//                 map.insert(key, value);
//             }
//             map
//         } else {
//             HashMap::new()
//         };
//         YMap(SharedType::prelim(map))
//     }

//     /// Returns true if this is a preliminary instance of `YMap`.
//     ///
//     /// Preliminary instances can be nested into other shared data types such as `YArray` and `YMap`.
//     /// Once a preliminary instance has been inserted this way, it becomes integrated into ywasm
//     /// document store and cannot be nested again: attempt to do so will result in an exception.
//     #[getter]
//     pub fn prelim(&self) -> bool {
//         if let SharedType::Prelim(_) = &*self.inner.borrow() {
//             true
//         } else {
//             false
//         }
//     }

//     /// Returns a number of entries stored within this instance of `YMap`.
//     pub fn length(&self, txn: &YTransaction) -> u32 {
//         match &*self.inner.borrow() {
//             SharedType::Integrated(v) => v.len(txn),
//             SharedType::Prelim(v) => v.len() as u32,
//         }
//     }

//     /// Converts contents of this `YMap` instance into a JSON representation.
//     pub fn to_json(&self, txn: &YTransaction) -> PyAny {
//         match &*self.inner.borrow() {
//             SharedType::Integrated(v) => any_into_py(v.to_json(txn)),
//             SharedType::Prelim(v) => {
//                 let map = js_sys::Object::new();
//                 for (k, v) in v.iter() {
//                     js_sys::Reflect::set(&map, &k.into(), v).unwrap();
//                 }
//                 map.into()
//             }
//         }
//     }

//     /// Sets a given `key`-`value` entry within this instance of `YMap`. If another entry was
//     /// already stored under given `key`, it will be overridden with new `value`.
//     pub fn set(&self, txn: &mut YTransaction, key: &str, value: PyAny) {
//         match &mut *self.inner.borrow_mut() {
//             SharedType::Integrated(v) => {
//                 v.insert(txn, key.to_string(), PyAnyWrapper(value));
//             }
//             SharedType::Prelim(v) => {
//                 v.insert(key.to_string(), value);
//             }
//         }
//     }

//     /// Removes an entry identified by a given `key` from this instance of `YMap`, if such exists.
//     pub fn delete(&mut self, txn: &mut YTransaction, key: &str) {
//         match &mut *self.inner.borrow_mut() {
//             SharedType::Integrated(v) => {
//                 v.remove(txn, key);
//             }
//             SharedType::Prelim(v) => {
//                 v.remove(key);
//             }
//         }
//     }

//     /// Returns value of an entry stored under given `key` within this instance of `YMap`,
//     /// or `undefined` if no such entry existed.
//     pub fn get(&self, txn: &mut YTransaction, key: &str) -> PyAny {
//         match &*self.inner.borrow() {
//             SharedType::Integrated(v) => {
//                 if let Some(value) = v.get(txn, key) {
//                     value_into_py(value)
//                 } else {
//                     PyAny::undefined()
//                 }
//             }
//             SharedType::Prelim(v) => {
//                 if let Some(value) = v.get(key) {
//                     value.clone()
//                 } else {
//                     PyAny::undefined()
//                 }
//             }
//         }
//     }

//     /// Returns an iterator that can be used to traverse over all entries stored within this
//     /// instance of `YMap`. Order of entry is not specified.
//     ///
//     /// Example:
//     ///
//     /// ```javascript
//     /// import YDoc from 'ywasm'
//     ///
//     /// /// document on machine A
//     /// const doc = new YDoc()
//     /// const map = doc.getMap('name')
//     /// const txn = doc.beginTransaction()
//     /// try {
//     ///     map.set(txn, 'key1', 'value1')
//     ///     map.set(txn, 'key2', true)
//     ///
//     ///     for (let [key, value] of map.entries(txn)) {
//     ///         console.log(key, value)
//     ///     }
//     /// } finally {
//     ///     txn.free()
//     /// }
//     /// ```
//     pub fn entries(&self, txn: &mut YTransaction) -> PyAny {
//         to_iter(match &*self.inner.borrow() {
//             SharedType::Integrated(v) => unsafe {
//                 let this: *const Map = v;
//                 let tx: *const Transaction<'static> = txn.0.deref();
//                 let static_iter: ManuallyDrop<MapIter<'static, 'static>> =
//                     ManuallyDrop::new((*this).iter(tx.as_ref().unwrap()));
//                 YMapIterator(static_iter).into()
//             },
//             SharedType::Prelim(v) => unsafe {
//                 let this: *const HashMap<String, PyAny> = v;
//                 let static_iter: ManuallyDrop<
//                     std::collections::hash_map::Iter<'static, String, PyAny>,
//                 > = ManuallyDrop::new((*this).iter());
//                 PrelimMapIterator(static_iter).into()
//             },
//         })
//     }
// }

// #[pyclass(unsendable)]
// pub struct YMapIterator {
//     inner: ManuallyDrop<MapIter<'static, 'static>>,
// }

// impl Deref for YMapIterator {
//     fn deref(self) {
//         self.inner.deref();
//     }
// }

// impl Drop for YMapIterator {
//     fn drop(&mut self) {
//         unsafe { ManuallyDrop::drop(&mut self.inner) }
//     }
// }

// impl<'a> From<Option<(&'a String, Value)>> for IteratorNext {
//     fn from(entry: Option<(&'a String, Value)>) -> Self {
//         match entry {
//             None => IteratorNext::finished(),
//             Some((k, v)) => {
//                 let tuple = js_sys::Array::new_with_length(2);
//                 tuple.set(0, PyAny::from(k));
//                 tuple.set(1, value_into_py(v));
//                 IteratorNext::new(tuple.into())
//             }
//         }
//     }
// }

// #[pymethods]
// impl YMapIterator {
//     pub fn next(&mut self) -> IteratorNext {
//         self.inner.next().into()
//     }
// }

// #[pyclass]
// pub struct PrelimMapIterator(
//     ManuallyDrop<std::collections::hash_map::Iter<'static, String, PyAny>>,
// );

// impl Drop for PrelimMapIterator {
//     fn drop(&mut self) {
//         unsafe { ManuallyDrop::drop(&mut self.inner) }
//     }
// }

// #[pymethods]
// impl PrelimMapIterator {
//     pub fn next(&mut self) -> IteratorNext {
//         if let Some((key, value)) = self.inner.next() {
//             let array = js_sys::Array::new_with_length(2);
//             array.push(&PyAny::from(key));
//             array.push(value);
//             IteratorNext::new(array.into())
//         } else {
//             IteratorNext::finished()
//         }
//     }
// }

// /// XML element data type. It represents an XML node, which can contain key-value attributes
// /// (interpreted as strings) as well as other nested XML elements or rich text (represented by
// /// `YXmlText` type).
// ///
// /// In terms of conflict resolution, `YXmlElement` uses following rules:
// ///
// /// - Attribute updates use logical last-write-wins principle, meaning the past updates are
// ///   automatically overridden and discarded by newer ones, while concurrent updates made by
// ///   different peers are resolved into a single value using document id seniority to establish
// ///   an order.
// /// - Child node insertion uses sequencing rules from other Yrs collections - elements are inserted
// ///   using interleave-resistant algorithm, where order of concurrent inserts at the same index
// ///   is established using peer's document id seniority.
// #[pyclass]
// pub struct YXmlElement(XmlElement);

// #[pymethods]
// impl YXmlElement {
//     /// Returns a tag name of this XML node.
//     #[getter]
//     pub fn name(&self) -> String {
//         self.inner.tag().to_string()
//     }

//     /// Returns a number of child XML nodes stored within this `YXMlElement` instance.
//     pub fn length(&self, txn: &YTransaction) -> u32 {
//         self.inner.len(txn)
//     }

//     /// Inserts a new instance of `YXmlElement` as a child of this XML node and returns it.
//     pub fn insert_xml_element(
//         &self,
//         txn: &mut YTransaction,
//         index: u32,
//         name: &str,
//     ) -> YXmlElement {
//         YXmlElement(self.inner.insert_elem(txn, index, name))
//     }

//     /// Inserts a new instance of `YXmlText` as a child of this XML node and returns it.
//     pub fn insert_xml_text(&self, txn: &mut YTransaction, index: u32) -> YXmlText {
//         YXmlText(self.inner.insert_text(txn, index))
//     }

//     /// Removes a range of children XML nodes from this `YXmlElement` instance,
//     /// starting at given `index`.

//     pub fn delete(&self, txn: &mut YTransaction, index: u32, length: u32) {
//         self.inner.remove_range(txn, index, length)
//     }

//     /// Appends a new instance of `YXmlElement` as the last child of this XML node and returns it.
//     pub fn push_xml_element(&self, txn: &mut YTransaction, name: &str) -> YXmlElement {
//         YXmlElement(self.inner.push_elem_back(txn, name))
//     }

//     /// Appends a new instance of `YXmlText` as the last child of this XML node and returns it.
//     pub fn push_xml_text(&self, txn: &mut YTransaction) -> YXmlText {
//         YXmlText(self.inner.push_text_back(txn))
//     }

//     /// Returns a first child of this XML node.
//     /// It can be either `YXmlElement`, `YXmlText` or `undefined` if current node has not children.
//     pub fn first_child(&self, txn: &YTransaction) -> PyAny {
//         if let Some(xml) = self.inner.first_child(txn) {
//             xml_into_js(xml)
//         } else {
//             PyAny::undefined()
//         }
//     }

//     /// Returns a next XML sibling node of this XMl node.
//     /// It can be either `YXmlElement`, `YXmlText` or `undefined` if current node is a last child of
//     /// parent XML node.
//     pub fn next_sibling(&self, txn: &YTransaction) -> PyAny {
//         if let Some(xml) = self.inner.next_sibling(txn) {
//             xml_into_js(xml)
//         } else {
//             PyAny::undefined()
//         }
//     }

//     /// Returns a previous XML sibling node of this XMl node.
//     /// It can be either `YXmlElement`, `YXmlText` or `undefined` if current node is a first child
//     /// of parent XML node.
//     pub fn prev_sibling(&self, txn: &YTransaction) -> PyAny {
//         if let Some(xml) = self.inner.prev_sibling(txn) {
//             xml_into_js(xml)
//         } else {
//             PyAny::undefined()
//         }
//     }

//     /// Returns a parent `YXmlElement` node or `undefined` if current node has no parent assigned.
//     pub fn parent(&self, txn: &YTransaction) -> PyAny {
//         if let Some(xml) = self.inner.parent(txn) {
//             xml_into_js(Xml::Element(xml))
//         } else {
//             PyAny::undefined()
//         }
//     }

//     /// Returns a string representation of this XML node.
//     pub fn to_string(&self, txn: &YTransaction) -> String {
//         self.inner.to_string(txn)
//     }

//     /// Sets a `name` and `value` as new attribute for this XML node. If an attribute with the same
//     /// `name` already existed on that node, its value with be overridden with a provided one.
//     pub fn set_attribute(&self, txn: &mut YTransaction, name: &str, value: &str) {
//         self.inner.insert_attribute(txn, name, value)
//     }

//     /// Returns a value of an attribute given its `name`. If no attribute with such name existed,
//     /// `null` will be returned.
//     pub fn get_attribute(&self, txn: &YTransaction, name: &str) -> Option<String> {
//         self.inner.get_attribute(txn, name)
//     }

//     /// Removes an attribute from this XML node, given its `name`.

//     pub fn remove_attribute(&self, txn: &mut YTransaction, name: &str) {
//         self.inner.remove_attribute(txn, name);
//     }

//     /// Returns an iterator that enables to traverse over all attributes of this XML node in
//     /// unspecified order.

//     pub fn attributes(&self, txn: &YTransaction) -> PyAny {
//         to_iter(unsafe {
//             let this: *const XmlElement = &self.inner;
//             let tx: *const Transaction<'static> = txn.0.deref();
//             let static_iter: ManuallyDrop<Attributes<'static, 'static>> =
//                 ManuallyDrop::new((*this).attributes(tx.as_ref().unwrap()));
//             YXmlAttributes(static_iter).into()
//         })
//     }

//     /// Returns an iterator that enables a deep traversal of this XML node - starting from first
//     /// child over this XML node successors using depth-first strategy.

//     pub fn tree_walker(&self, txn: &YTransaction) -> PyAny {
//         to_iter(unsafe {
//             let this: *const XmlElement = &self.inner;
//             let tx: *const Transaction<'static> = txn.0.deref();
//             let static_iter: ManuallyDrop<TreeWalker<'static, 'static>> =
//                 ManuallyDrop::new((*this).successors(tx.as_ref().unwrap()));
//             YXmlTreeWalker(static_iter).into()
//         })
//     }
// }

// #[pyclass]
// pub struct YXmlAttributes(ManuallyDrop<Attributes<'static, 'static>>);

// impl Drop for YXmlAttributes {
//     fn drop(&mut self) {
//         unsafe { ManuallyDrop::drop(&mut self.inner) }
//     }
// }

// impl<'a> From<Option<(&'a str, String)>> for IteratorNext {
//     fn from(o: Option<(&'a str, String)>) -> Self {
//         match o {
//             None => IteratorNext::finished(),
//             Some((name, value)) => {
//                 let tuple = js_sys::Array::new_with_length(2);
//                 tuple.set(0, PyAny::from_str(name));
//                 tuple.set(1, PyAny::from(&value));
//                 IteratorNext::new(tuple.into())
//             }
//         }
//     }
// }

// #[pymethods]
// impl YXmlAttributes {
//     pub fn next(&mut self) -> IteratorNext {
//         self.inner.next().into()
//     }
// }

// #[pyclass]
// pub struct YXmlTreeWalker(ManuallyDrop<TreeWalker<'static, 'static>>);

// impl Drop for YXmlTreeWalker {
//     fn drop(&mut self) {
//         unsafe { ManuallyDrop::drop(&mut self.inner) }
//     }
// }

// #[pymethods]
// impl YXmlTreeWalker {
//     pub fn next(&mut self) -> IteratorNext {
//         if let Some(xml) = self.inner.next() {
//             let js_val = xml_into_js(xml);
//             IteratorNext::new(js_val)
//         } else {
//             IteratorNext::finished()
//         }
//     }
// }

// /// A shared data type used for collaborative text editing, that can be used in a context of
// /// `YXmlElement` nodee. It enables multiple users to add and remove chunks of text in efficient
// /// manner. This type is internally represented as a mutable double-linked list of text chunks
// /// - an optimization occurs during `YTransaction.commit`, which allows to squash multiple
// /// consecutively inserted characters together as a single chunk of text even between transaction
// /// boundaries in order to preserve more efficient memory model.
// ///
// /// Just like `YXmlElement`, `YXmlText` can be marked with extra metadata in form of attributes.
// ///
// /// `YXmlText` structure internally uses UTF-8 encoding and its length is described in a number of
// /// bytes rather than individual characters (a single UTF-8 code point can consist of many bytes).
// ///
// /// Like all Yrs shared data types, `YXmlText` is resistant to the problem of interleaving (situation
// /// when characters inserted one after another may interleave with other peers concurrent inserts
// /// after merging all updates together). In case of Yrs conflict resolution is solved by using
// /// unique document id to determine correct and consistent ordering.
// #[pyclass]
// pub struct YXmlText {
//     inner: XmlText,
// }

// #[pymethods]
// impl YXmlText {
//     /// Returns length of an underlying string stored in this `YXmlText` instance,
//     /// understood as a number of UTF-8 encoded bytes.
//     #[getter]
//     pub fn length(&self) -> u32 {
//         self.inner.len()
//     }

//     /// Inserts a given `chunk` of text into this `YXmlText` instance, starting at a given `index`.

//     pub fn insert(&self, txn: &mut YTransaction, index: i32, chunk: &str) {
//         self.inner.insert(txn, index as u32, chunk)
//     }

//     /// Appends a given `chunk` of text at the end of `YXmlText` instance.

//     pub fn push(&self, txn: &mut YTransaction, chunk: &str) {
//         self.inner.push(txn, chunk)
//     }

//     /// Deletes a specified range of of characters, starting at a given `index`.
//     /// Both `index` and `length` are counted in terms of a number of UTF-8 character bytes.

//     pub fn delete(&self, txn: &mut YTransaction, index: u32, length: u32) {
//         self.inner.remove_range(txn, index, length)
//     }

//     /// Returns a next XML sibling node of this XMl node.
//     /// It can be either `YXmlElement`, `YXmlText` or `undefined` if current node is a last child of
//     /// parent XML node.

//     pub fn next_sibling(&self, txn: &YTransaction) -> PyAny {
//         if let Some(xml) = self.inner.next_sibling(txn) {
//             xml_into_js(xml)
//         } else {
//             PyAny::undefined()
//         }
//     }

//     /// Returns a previous XML sibling node of this XMl node.
//     /// It can be either `YXmlElement`, `YXmlText` or `undefined` if current node is a first child
//     /// of parent XML node.

//     pub fn prev_sibling(&self, txn: &YTransaction) -> PyAny {
//         if let Some(xml) = self.inner.prev_sibling(txn) {
//             xml_into_js(xml)
//         } else {
//             PyAny::undefined()
//         }
//     }

//     /// Returns a parent `YXmlElement` node or `undefined` if current node has no parent assigned.

//     pub fn parent(&self, txn: &YTransaction) -> PyAny {
//         if let Some(xml) = self.inner.parent(txn) {
//             xml_into_js(Xml::Element(xml))
//         } else {
//             PyAny::undefined()
//         }
//     }

//     /// Returns an underlying string stored in this `YXmlText` instance.

//     pub fn to_string(&self, txn: &YTransaction) -> String {
//         self.inner.to_string(txn)
//     }

//     /// Sets a `name` and `value` as new attribute for this XML node. If an attribute with the same
//     /// `name` already existed on that node, its value with be overridden with a provided one.

//     pub fn set_attribute(&self, txn: &mut YTransaction, name: &str, value: &str) {
//         self.inner.insert_attribute(txn, name, value);
//     }

//     /// Returns a value of an attribute given its `name`. If no attribute with such name existed,
//     /// `null` will be returned.

//     pub fn get_attribute(&self, txn: &YTransaction, name: &str) -> Option<String> {
//         self.inner.get_attribute(txn, name)
//     }

//     /// Removes an attribute from this XML node, given its `name`.

//     pub fn remove_attribute(&self, txn: &mut YTransaction, name: &str) {
//         self.inner.remove_attribute(txn, name);
//     }

//     /// Returns an iterator that enables to traverse over all attributes of this XML node in
//     /// unspecified order.

//     pub fn attributes(&self, txn: &YTransaction) -> YXmlAttributes {
//         unsafe {
//             let this: *const XmlText = &self.inner;
//             let tx: *const Transaction<'static> = txn.0.deref();
//             let static_iter: ManuallyDrop<Attributes<'static, 'static>> =
//                 ManuallyDrop::new((*this).attributes(tx.as_ref().unwrap()));
//             YXmlAttributes(static_iter)
//         }
//     }
// }

struct PyObjectWrapper(PyObject);

impl Prelim for PyObjectWrapper {
    fn into_content(self, _txn: &mut Transaction, ptr: TypePtr) -> (ItemContent, Option<Self>) {
        let guard = Python::acquire_gil();
        let py = guard.python();
        let content = if let Some(any) = py_into_any(self.0.clone()) {
            ItemContent::Any(vec![any])
        } else if let Ok(shared) = Shared::extract(self.0.as_ref(py)) {
            if shared.is_prelim() {
                let branch = BranchRef::new(Branch::new(ptr, shared.type_ref(), None));
                ItemContent::Type(branch)
            } else {
                panic!("Cannot integrate this type")
            }
        } else {
            panic!("Cannot integrate this type")
        };

        let this = if let ItemContent::Type(_) = &content {
            Some(self)
        } else {
            None
        };

        (content, this)
    }

    fn integrate(self, txn: &mut Transaction, inner_ref: BranchRef) {
        let guard = Python::acquire_gil();
        let py = guard.python();
        let obj_ref = self.0.as_ref(py);
        if let Ok(shared) = Shared::extract(obj_ref) {
            if shared.is_prelim() {
                match shared {
                    Shared::Text(v) => {
                        let text = Text::from(inner_ref);
                        if let SharedType::Prelim(v) =
                            v.0.replace(SharedType::Integrated(text.clone()))
                        {
                            text.push(txn, v.as_str());
                        }
                    }
                    Shared::Array(v) => {
                        let array = Array::from(inner_ref);
                        if let SharedType::Prelim(items) = Python::with_gil(|py| {
                            let arr = v.borrow(py);
                            arr.0.replace(SharedType::Integrated(array.clone()))
                        }) {
                            let len = array.len();
                            insert_at(&array, txn, len, items);
                        }
                    }
                    // Shared::Map(v) => {
                    //     let map = Map::from(inner_ref);
                    //     if let SharedType::Prelim(entries) =
                    //         v.0.replace(SharedType::Integrated(map.clone()))
                    //     {
                    //         for (k, v) in entries {
                    //             map.insert(txn, k, PyAnyWrapper { inner: v });
                    //         }
                    //     }
                    // }
                    _ => panic!("Cannot integrate this type"),
                }
            }
        }
    }
}

fn insert_at(dst: &Array, txn: &mut Transaction, index: u32, src: Vec<PyObject>) {
    let mut j = index;
    let mut i = 0;
    while i < src.len() {
        let mut anys = Vec::default();
        while i < src.len() {
            if let Some(any) = py_into_any(src[i].clone()) {
                anys.push(any);
                i += 1;
            } else {
                break;
            }
        }

        if !anys.is_empty() {
            let len = anys.len() as u32;
            dst.insert_range(txn, j, anys);
            j += len;
        } else {
            let wrapper = PyObjectWrapper(src[i].clone());
            dst.insert(txn, j, wrapper);
            i += 1;
            j += 1;
        }
    }
}

fn py_into_any(v: PyObject) -> Option<Any> {
    Python::with_gil(|py| -> Option<Any> {
        let v = v.as_ref(py);

        if let Ok(s) = v.downcast::<pytypes::PyString>() {
            Some(Any::String(s.extract().unwrap()))
        } else if let Ok(l) = v.downcast::<pytypes::PyLong>() {
            let i: f64 = l.extract().unwrap();
            Some(Any::BigInt(i as i64))
        }
        // TODO: Handle Null vals
        // else if let Ok(s) = v.downcast::<pytypes::Null>() {
        //     Some(Any::Null)
        // }
        // else if v.is_undefined() {
        //     Some(Any::Undefined)
        // }
        else if let Ok(f) = v.downcast::<pytypes::PyFloat>() {
            Some(Any::Number(f.extract().unwrap()))
        } else if let Ok(b) = v.downcast::<pytypes::PyBool>() {
            Some(Any::Bool(b.extract().unwrap()))
        } else if let Ok(list) = v.downcast::<pytypes::PyList>() {
            let mut result = Vec::with_capacity(list.len());
            for value in list.iter() {
                result.push(py_into_any(value.into())?);
            }
            Some(Any::Array(result))
        } else if let Ok(dict) = v.downcast::<pytypes::PyDict>() {
            if let Ok(_) = Shared::extract(v) {
                None
            } else {
                let mut result = HashMap::new();
                for (k, v) in dict.iter() {
                    // TODO: Handle non string keys
                    let key = k
                        .downcast::<pytypes::PyString>()
                        .unwrap()
                        .extract()
                        .unwrap();
                    let value = py_into_any(v.into())?;
                    result.insert(key, value);
                }
                Some(Any::Map(result))
            }
        } else {
            None
        }
    })
}

pub struct AnyWrapper(Any);

impl IntoPy<pyo3::PyObject> for AnyWrapper {
    fn into_py(self, py: Python) -> pyo3::PyObject {
        match self.0 {
            Any::Null | Any::Undefined => py.None(),
            Any::Bool(v) => v.into_py(py),
            Any::Number(v) => v.into_py(py),
            Any::BigInt(v) => v.into_py(py),
            Any::String(v) => v.into_py(py),
            Any::Buffer(v) => {
                unreachable!();
                // pytypes::PyByteArray::new(v)
                // pytypes::PyByteArray::from(v)
                // let v = Vec::<u8>::from(v.as_ref());
                // v.into_py(py)
            }
            Any::Array(v) => {
                let mut a = Vec::new();
                for value in v {
                    let value = AnyWrapper(value);
                    a.push(value);
                }
                a.into_py(py)
            }
            Any::Map(v) => {
                let mut m = HashMap::new();
                for (k, v) in v {
                    let value = AnyWrapper(v);
                    m.insert(k, value);
                }
                m.into_py(py)
            }
        }
    }
}

impl Deref for AnyWrapper {
    type Target = Any;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct ValueWrapper(Value);

impl IntoPy<pyo3::PyObject> for ValueWrapper {
    fn into_py(self, py: Python) -> pyo3::PyObject {
        match self.0 {
            Value::Any(v) => AnyWrapper(v).into_py(py),
            Value::YText(v) => YText::from(v).into_py(py),
            //YText::from(v).into(),
            Value::YArray(v) => unreachable!(),
            // YArray::from(v).into(),
            Value::YMap(v) => unreachable!(),
            //YMap::from(v).into(),
            Value::YXmlElement(v) => unreachable!(),
            //YXmlElement(v).into(),
            Value::YXmlText(v) => unreachable!(),
            // YXmlText(v).into(),
        }
    }
}

// fn xml_into_js(v: Xml) -> PyAny {
//     match v {
//         Xml::Element(v) => YXmlElement(v).into(),
//         Xml::Text(v) => YXmlText(v).into(),
//     }
// }

#[derive(FromPyObject)]
enum Shared {
    Text(YText),
    Array(Py<YArray>),
    // Map(Ref<'a, YMap>),
    // XmlElement(Ref<'a, YXmlElement>),
    // XmlText(Ref<'a, YXmlText>),
}
// TODO: pointer deref?
// fn as_ref<'a, T>(py: u32) -> Ref<'a, T> {
//     unsafe {
//         let py = py as *mut wasm_bindgen::__rt::WasmRefCell<T>;
//         (*py).borrow()
//     }
// }

// impl<'a> TryFrom<&'a PyAny> for Shared<'a> {
//     type Error = PyAny;

//     // TODO
//     fn try_from(py: &'a PyAny) -> Result<Self, Self::Error> {
//         let ctor_name = Object::get_prototype_of(py).constructor().name();
//         let ptr = Reflect::get(py, &PyAny::from_str("ptr"))?;
//         let ptr_u32: u32 = ptr.as_f64().ok_or(PyAny::NULL)? as u32;
//         if ctor_name == "YText" {
//             Ok(Shared::Text(as_ref(ptr_u32)))
//         }
//         else if ctor_name == "YArray" {
//             Ok(Shared::Array(as_ref(ptr_u32)))
//         } else if ctor_name == "YMap" {
//             Ok(Shared::Map(as_ref(ptr_u32)))
//         } else if ctor_name == "YXmlElement" {
//             Ok(Shared::XmlElement(as_ref(ptr_u32)))
//         } else if ctor_name == "YXmlText" {
//             Ok(Shared::XmlText(as_ref(ptr_u32)))
//         }
//         else {
//             Err(PyAny::NULL)
//         }
//     }
// }

impl Shared {
    fn is_prelim(&self) -> bool {
        match self {
            Shared::Text(v) => v.prelim(),
            Shared::Array(v) => Python::with_gil(|py| v.borrow(py).prelim()),
            // Shared::Map(v) => v.prelim(),
            // Shared::XmlElement(_) | Shared::XmlText(_) => false,
        }
    }

    fn type_ref(&self) -> TypeRefs {
        match self {
            Shared::Text(_) => TYPE_REFS_TEXT,
            Shared::Array(_) => TYPE_REFS_ARRAY,
            // Shared::Map(_) => TYPE_REFS_MAP,
            // Shared::XmlElement(_) => TYPE_REFS_XML_ELEMENT,
            // Shared::XmlText(_) => TYPE_REFS_XML_TEXT,
        }
    }
}

#[pymodule]
pub fn y_py(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<YDoc>()?;
    m.add_class::<YText>()?;
    m.add_class::<YArray>()?;
    m.add_class::<YArrayIterator>()?;
    m.add_wrapped(wrap_pyfunction!(encode_state_vector))?;
    m.add_wrapped(wrap_pyfunction!(encode_state_as_update))?;
    m.add_wrapped(wrap_pyfunction!(apply_update))?;
    Ok(())
}
