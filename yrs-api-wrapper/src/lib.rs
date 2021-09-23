use lib0::any::Any;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::mem::{forget, MaybeUninit, ManuallyDrop};
use std::os::raw::{c_char, c_float, c_int, c_long, c_uchar, c_ulong};
use yrs::block::{ItemContent, Prelim};
use yrs::types::{
    Branch, BranchRef, TypePtr, Value, TYPE_REFS_ARRAY, TYPE_REFS_MAP, TYPE_REFS_XML_ELEMENT,
    TYPE_REFS_XML_TEXT,
};
use yrs::updates::decoder::{Decode, DecoderV1};
use yrs::updates::encoder::{Encode, Encoder, EncoderV1};
use yrs::StateVector;
use yrs::Update;
use yrs::{DeleteSet, Xml};

/// Flag used by `YInput` and `YOutput` to tag boolean values.
#[no_mangle]
#[export_name = "Y_JSON_BOOL"]
pub static Y_JSON_BOOL: c_char = -8;

/// Flag used by `YInput` and `YOutput` to tag floating point numbers.
#[no_mangle]
#[export_name = "Y_JSON_NUM"]
pub static Y_JSON_NUM: c_char = -7;

/// Flag used by `YInput` and `YOutput` to tag 64-bit integer numbers.
#[no_mangle]
#[export_name = "Y_JSON_INT"]
pub static Y_JSON_INT: c_char = -6;

/// Flag used by `YInput` and `YOutput` to tag strings.
#[no_mangle]
#[export_name = "Y_JSON_STR"]
pub static Y_JSON_STR: c_char = -5;

/// Flag used by `YInput` and `YOutput` to tag binary content.
#[no_mangle]
#[export_name = "Y_JSON_BUF"]
pub static Y_JSON_BUF: c_char = -4;

/// Flag used by `YInput` and `YOutput` to tag embedded JSON-like arrays of values,
/// which themselves are `YInput` and `YOutput` instances respectively.
#[no_mangle]
#[export_name = "Y_JSON_ARRAY"]
pub static Y_JSON_ARR: c_char = -3;

/// Flag used by `YInput` and `YOutput` to tag embedded JSON-like maps of key-value pairs,
/// where keys are strings and values are `YInput` and `YOutput` instances respectively.
#[no_mangle]
#[export_name = "Y_JSON_MAP"]
pub static Y_JSON_MAP: c_char = -2;

/// Flag used by `YInput` and `YOutput` to tag JSON-like null values.
#[no_mangle]
#[export_name = "Y_JSON_NULL"]
pub static Y_JSON_NULL: c_char = -1;

/// Flag used by `YInput` and `YOutput` to tag JSON-like undefined values.
#[no_mangle]
#[export_name = "Y_JSON_UNDEF"]
pub static Y_JSON_UNDEF: c_char = 0;

/// Flag used by `YInput` and `YOutput` to tag content, which is an `YArray` shared type.
#[no_mangle]
#[export_name = "Y_ARRAY"]
pub static Y_ARRAY: c_char = 1;

/// Flag used by `YInput` and `YOutput` to tag content, which is an `YMap` shared type.
#[no_mangle]
#[export_name = "Y_MAP"]
pub static Y_MAP: c_char = 2;

/// Flag used by `YInput` and `YOutput` to tag content, which is an `YText` shared type.
#[no_mangle]
#[export_name = "Y_TEXT"]
pub static Y_TEXT: c_char = 3;

/// Flag used by `YInput` and `YOutput` to tag content, which is an `YXmlElement` shared type.
#[no_mangle]
#[export_name = "Y_XML_ELEM"]
pub static Y_XML_ELEM: c_char = 4;

/// Flag used by `YInput` and `YOutput` to tag content, which is an `YXmlText` shared type.
#[no_mangle]
#[export_name = "Y_XML_TEXT"]
pub static Y_XML_TEXT: c_char = 5;

/// Flag used to mark a truthy boolean numbers.
#[no_mangle]
#[export_name = "Y_TRUE"]
pub static Y_TRUE: c_char = 1;

/// Flag used to mark a falsy boolean numbers.
#[no_mangle]
#[export_name = "Y_FALSE"]
pub static Y_FALSE: c_char = 0;

/* pub types below are used by cbindgen for c header generation */

/// A Yrs document type. Documents are most important units of collaborative resources management.
/// All shared collections live within a scope of their corresponding documents. All updates are
/// generated on per document basis (rather than individual shared type). All operations on shared
/// collections happen via `YTransaction`, which lifetime is also bound to a document.
///
/// Document manages so called root types, which are top-level shared types definitions (as opposed
/// to recursively nested types).
pub type Doc = yrs::Doc;

/// Transaction is one of the core types in Yrs. All operations that need to touch a document's
/// contents (a.k.a. block store), need to be executed in scope of a transaction.
pub type Transaction = yrs::Transaction<'static>;

/// Collection used to store key-value entries in an unordered manner. Keys are always represented
/// as UTF-8 strings. Values can be any value type supported by Yrs: JSON-like primitives as well as
/// shared data types.
///
/// In terms of conflict resolution, `YMap` uses logical last-write-wins principle, meaning the past
/// updates are automatically overridden and discarded by newer ones, while concurrent updates made
/// by different peers are resolved into a single value using document id seniority to establish
/// order.
pub type Map = yrs::Map;

/// A shared data type used for collaborative text editing. It enables multiple users to add and
/// remove chunks of text in efficient manner. This type is internally represented as a mutable
/// double-linked list of text chunks - an optimization occurs during [ytransaction_commit], which
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
pub type Text = yrs::Text;

/// A collection used to store data in an indexed sequence structure. This type is internally
/// implemented as a double linked list, which may squash values inserted directly one after another
/// into single list node upon transaction commit.
///
/// Reading a root-level type as an `YArray` means treating its sequence components as a list, where
/// every countable element becomes an individual entity:
///
/// - JSON-like primitives (booleans, numbers, strings, JSON maps, arrays etc.) are counted
///   individually.
/// - Text chunks inserted by `YText` data structure: each character becomes an element of an
///   array.
/// - Embedded and binary values: they count as a single element even though they correspond of
///   multiple bytes.
///
/// Like all Yrs shared data types, `YArray` is resistant to the problem of interleaving (situation
/// when elements inserted one after another may interleave with other peers concurrent inserts
/// after merging all updates together). In case of Yrs conflict resolution is solved by using
/// unique document id to determine correct and consistent ordering.
pub type Array = yrs::Array;

/// XML element data type. It represents an XML node, which can contain key-value attributes
/// (interpreted as strings) as well as other nested XML elements or rich text (represented by
/// `YXmlText` type).
///
/// In terms of conflict resolution, `YXmlElement` uses following rules:
///
/// - Attribute updates use logical last-write-wins principle, meaning the past updates are
///   automatically overridden and discarded by newer ones, while concurrent updates made by
///   different peers are resolved into a single value using document id seniority to establish
///   an order.
/// - Child node insertion uses sequencing rules from other Yrs collections - elements are inserted
///   using interleave-resistant algorithm, where order of concurrent inserts at the same index
///   is established using peer's document id seniority.
pub type XmlElement = yrs::XmlElement;

/// A shared data type used for collaborative text editing, that can be used in a context of
/// `YXmlElement` nodee. It enables multiple users to add and remove chunks of text in efficient
/// manner. This type is internally represented as a mutable double-linked list of text chunks
/// - an optimization occurs during [ytransaction_commit], which allows to squash multiple
/// consecutively inserted characters together as a single chunk of text even between transaction
/// boundaries in order to preserve more efficient memory model.
///
/// Just like `YXmlElement`, `YXmlText` can be marked with extra metadata in form of attributes.
///
/// `YXmlText` structure internally uses UTF-8 encoding and its length is described in a number of
/// bytes rather than individual characters (a single UTF-8 code point can consist of many bytes).
///
/// Like all Yrs shared data types, `YXmlText` is resistant to the problem of interleaving (situation
/// when characters inserted one after another may interleave with other peers concurrent inserts
/// after merging all updates together). In case of Yrs conflict resolution is solved by using
/// unique document id to determine correct and consistent ordering.
pub type XmlText = yrs::XmlText;

/// Iterator structure used by shared array data type.
pub type ArrayIter = yrs::types::array::ArrayIter<'static, 'static>;

/// Iterator structure used by shared map data type. Map iterators are unordered - there's no
/// specific order in which map entries will be returned during consecutive iterator calls.
pub type MapIter = yrs::types::map::MapIter<'static, 'static>;

/// Iterator structure used by XML nodes (elements and text) to iterate over node's attributes.
/// Attribute iterators are unordered - there's no specific order in which map entries will be
/// returned during consecutive iterator calls.
pub type Attributes = yrs::types::xml::Attributes<'static, 'static>;

/// Iterator used to traverse over the complex nested tree structure of a XML node. XML node
/// iterator walks only over `YXmlElement` and `YXmlText` nodes. It does so in ordered manner (using
/// the order in which children are ordered within their parent nodes) and using **depth-first**
/// traverse.
pub type TreeWalker = yrs::types::xml::TreeWalker<'static, 'static>;

/// A structure representing single key-value entry of a map output (used by either
/// embedded JSON-like maps or YMaps).
#[repr(C)]
pub struct YMapEntry {
    /// Null-terminated string representing an entry's key component. Encoded as UTF-8.
    pub key: *const c_char,
    /// A `YOutput` value representing containing variadic content that can be stored withing map's
    /// entry.
    pub value: YOutput,
}

impl YMapEntry {
    fn new(key: &str, value: Value) -> Self {
        let value = YOutput::from(value);
        YMapEntry {
            key: CString::new(key).unwrap().into_raw(),
            value,
        }
    }
}

impl Drop for YMapEntry {
    fn drop(&mut self) {
        unsafe {
            drop(CString::from_raw(self.key as *mut c_char));
            //self.value.drop();
        }
    }
}

/// A structure representing single attribute of an either `YXmlElement` or `YXmlText` instance.
/// It consists of attribute name and string, both of which are null-terminated UTF-8 strings.
#[repr(C)]
pub struct YXmlAttr {
    pub name: *const c_char,
    pub value: *const c_char,
}

impl Drop for YXmlAttr {
    fn drop(&mut self) {
        unsafe {
            drop(CString::from_raw(self.name as *mut _));
            drop(CString::from_raw(self.value as *mut _));
        }
    }
}

/// Releases all memory-allocated resources bound to given document.
#[no_mangle]
pub unsafe extern "C" fn ydoc_destroy(value: *mut Doc) {
    if !value.is_null() {
        drop(Box::from_raw(value));
    }
}

/// Releases all memory-allocated resources bound to given `YText` instance. It doesn't remove the
/// `YText` stored inside of a document itself, but rather only parts of it related to a specific
/// pointer that's a subject of being destroyed.
#[no_mangle]
pub unsafe extern "C" fn ytext_destroy(value: *mut Text) {
    if !value.is_null() {
        drop(Box::from_raw(value));
    }
}

/// Releases all memory-allocated resources bound to given `YArray` instance. It doesn't remove the
/// `YArray` stored inside of a document itself, but rather only parts of it related to a specific
/// pointer that's a subject of being destroyed.
#[no_mangle]
pub unsafe extern "C" fn yarray_destroy(value: *mut Array) {
    if !value.is_null() {
        drop(Box::from_raw(value));
    }
}

/// Releases all memory-allocated resources bound to given `YMap` instance. It doesn't remove the
/// `YMap` stored inside of a document itself, but rather only parts of it related to a specific
/// pointer that's a subject of being destroyed.
#[no_mangle]
pub unsafe extern "C" fn ymap_destroy(value: *mut Map) {
    if !value.is_null() {
        drop(Box::from_raw(value));
    }
}

/// Releases all memory-allocated resources bound to given `YXmlElement` instance. It doesn't remove
/// the `YXmlElement` stored inside of a document itself, but rather only parts of it related to
/// a specific pointer that's a subject of being destroyed.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_destroy(value: *mut XmlElement) {
    if !value.is_null() {
        drop(Box::from_raw(value));
    }
}

/// Releases all memory-allocated resources bound to given `YXmlText` instance. It doesn't remove
/// the `YXmlText` stored inside of a document itself, but rather only parts of it related to
/// a specific pointer that's a subject of being destroyed.
#[no_mangle]
pub unsafe extern "C" fn yxmltext_destroy(value: *mut XmlText) {
    if !value.is_null() {
        drop(Box::from_raw(value));
    }
}

/// Frees all memory-allocated resources bound to a given [YMapEntry].
#[no_mangle]
pub unsafe extern "C" fn ymap_entry_destroy(value: *mut YMapEntry) {
    if !value.is_null() {
        drop(Box::from_raw(value));
    }
}

/// Frees all memory-allocated resources bound to a given [YXmlAttr].
#[no_mangle]
pub unsafe extern "C" fn yxmlattr_destroy(attr: *mut YXmlAttr) {
    if !attr.is_null() {
        drop(Box::from_raw(attr));
    }
}

/// Frees all memory-allocated resources bound to a given UTF-8 null-terminated string returned from
/// Yrs document API. Yrs strings don't use libc malloc, so calling `free()` on them will fault.
#[no_mangle]
pub unsafe extern "C" fn ystring_destroy(str: *mut c_char) {
    if !str.is_null() {
        drop(CString::from_raw(str));
    }
}

/// Frees all memory-allocated resources bound to a given binary returned from Yrs document API.
/// Unlike strings binaries are not null-terminated and can contain null characters inside,
/// therefore a size of memory to be released must be explicitly provided.
/// Yrs binaries don't use libc malloc, so calling `free()` on them will fault.
#[no_mangle]
pub unsafe extern "C" fn ybinary_destroy(ptr: *mut c_uchar, len: c_int) {
    if !ptr.is_null() {
        drop(Vec::from_raw_parts(ptr, len as usize, len as usize));
    }
}

/// Creates a new [Doc] instance with a randomized unique client identifier.
///
/// Use [ydoc_destroy] in order to release created [Doc] resources.
#[no_mangle]
pub extern "C" fn ydoc_new() -> *mut Doc {
    Box::into_raw(Box::new(Doc::new()))
}

/// Creates a new [Doc] instance with a specified client `id`. Provided `id` must be unique across
/// all collaborating clients.
///
/// If two clients share the same `id` and will perform any updates, it will result in unrecoverable
/// document state corruption. The same thing may happen if the client restored document state from
/// snapshot, that didn't contain all of that clients updates that were sent to other peers.
///
/// Use [ydoc_destroy] in order to release created [Doc] resources.
#[no_mangle]
pub extern "C" fn ydoc_new_with_id(id: c_ulong) -> *mut Doc {
    Box::into_raw(Box::new(Doc::with_client_id(id as u64)))
}

/// Returns a unique client identifier of this [Doc] instance.
#[no_mangle]
pub unsafe extern "C" fn ydoc_id(doc: *mut Doc) -> c_ulong {
    let doc = doc.as_ref().unwrap();
    doc.client_id as c_ulong
}

/// Starts a new read-write transaction on a given document. All other operations happen in context
/// of a transaction. Yrs transactions do not follow ACID rules. Once a set of operations is
/// complete, a transaction can be finished using [ytransaction_commit] function.
#[no_mangle]
pub unsafe extern "C" fn ytransaction_new(doc: *mut Doc) -> *mut Transaction {
    assert!(!doc.is_null());

    let doc = doc.as_mut().unwrap();
    Box::into_raw(Box::new(doc.transact()))
}

/// Commit and dispose provided transaction. This operation releases allocated resources, triggers
/// update events and performs a storage compression over all operations executed in scope of
/// current transaction.
#[no_mangle]
pub unsafe extern "C" fn ytransaction_commit(txn: *mut Transaction) {
    assert!(!txn.is_null());
    drop(Box::from_raw(txn)); // transaction is auto-committed when dropped
}

/// Gets or creates a new shared `YText` data type instance as a root-level type of a given document.
/// This structure can later be accessed using its `name`, which must be a null-terminated UTF-8
/// compatible string.
///
/// Use [ytext_destroy] in order to release pointer returned that way - keep in mind that this will
/// not remove `YText` instance from the document itself (once created it'll last for the entire
/// lifecycle of a document).
#[no_mangle]
pub unsafe extern "C" fn ytext(txn: *mut Transaction, name: *const c_char) -> *mut Text {
    assert!(!txn.is_null());
    assert!(!name.is_null());

    let name = CStr::from_ptr(name).to_str().unwrap();
    let value = txn.as_mut().unwrap().get_text(name);
    Box::into_raw(Box::new(value))
}

/// Gets or creates a new shared `YArray` data type instance as a root-level type of a given document.
/// This structure can later be accessed using its `name`, which must be a null-terminated UTF-8
/// compatible string.
///
/// Use [yarray_destroy] in order to release pointer returned that way - keep in mind that this will
/// not remove `YArray` instance from the document itself (once created it'll last for the entire
/// lifecycle of a document).
#[no_mangle]
pub unsafe extern "C" fn yarray(txn: *mut Transaction, name: *const c_char) -> *mut Array {
    assert!(!txn.is_null());
    assert!(!name.is_null());

    let name = CStr::from_ptr(name).to_str().unwrap();
    let value = txn.as_mut().unwrap().get_array(name);
    Box::into_raw(Box::new(value))
}

/// Gets or creates a new shared `YMap` data type instance as a root-level type of a given document.
/// This structure can later be accessed using its `name`, which must be a null-terminated UTF-8
/// compatible string.
///
/// Use [ymap_destroy] in order to release pointer returned that way - keep in mind that this will
/// not remove `YMap` instance from the document itself (once created it'll last for the entire
/// lifecycle of a document).
#[no_mangle]
pub unsafe extern "C" fn ymap(txn: *mut Transaction, name: *const c_char) -> *mut Map {
    assert!(!txn.is_null());
    assert!(!name.is_null());

    let name = CStr::from_ptr(name).to_str().unwrap();
    let value = txn.as_mut().unwrap().get_map(name);
    Box::into_raw(Box::new(value))
}

/// Gets or creates a new shared `YXmlElement` data type instance as a root-level type of a given
/// document. This structure can later be accessed using its `name`, which must be a null-terminated
/// UTF-8 compatible string.
///
/// Use [yxmlelem_destroy] in order to release pointer returned that way - keep in mind that this
/// will not remove `YXmlElement` instance from the document itself (once created it'll last for
/// the entire lifecycle of a document).
#[no_mangle]
pub unsafe extern "C" fn yxmlelem(
    txn: *mut Transaction,
    name: *const c_char,
) -> *mut XmlElement {
    assert!(!txn.is_null());
    assert!(!name.is_null());

    let name = CStr::from_ptr(name).to_str().unwrap();
    let value = txn.as_mut().unwrap().get_xml_element(name);
    Box::into_raw(Box::new(value))
}

/// Gets or creates a new shared `YXmlText` data type instance as a root-level type of a given
/// document. This structure can later be accessed using its `name`, which must be a null-terminated
/// UTF-8 compatible string.
///
/// Use [yxmltext_destroy] in order to release pointer returned that way - keep in mind that this
/// will not remove `YXmlText` instance from the document itself (once created it'll last for
/// the entire lifecycle of a document).
#[no_mangle]
pub unsafe extern "C" fn yxmltext(txn: *mut Transaction, name: *const c_char) -> *mut XmlText {
    assert!(!txn.is_null());
    assert!(!name.is_null());

    let name = CStr::from_ptr(name).to_str().unwrap();
    let value = txn.as_mut().unwrap().get_xml_text(name);
    Box::into_raw(Box::new(value))
}

/// Returns a state vector of a current transaction's document, serialized using lib0 version 1
/// encoding. Payload created by this function can then be send over the network to a remote peer,
/// where it can be used as a parameter of [ytransaction_state_diff_v1] in order to produce a delta
/// update payload, that can be send back and applied locally in order to efficiently propagate
/// updates from one peer to another.
///
/// The length of a generated binary will be passed within a `len` out parameter.
///
/// Once no longer needed, a returned binary can be disposed using [ybinary_destroy] function.
#[no_mangle]
pub unsafe extern "C" fn ytransaction_state_vector_v1(
    txn: *const Transaction,
    len: *mut c_int,
) -> *mut c_uchar {
    assert!(!txn.is_null());

    let state_vector = txn.as_ref().unwrap().state_vector();
    let binary = state_vector.encode_v1().into_boxed_slice();

    *len = binary.len() as c_int;
    Box::into_raw(binary) as *mut c_uchar
}

/// Returns a delta difference between current state of a transaction's document and a state vector
/// `sv` encoded as a binary payload using lib0 version 1 encoding (which could be generated using
/// [ytransaction_state_vector_v1]). Such delta can be send back to the state vector's sender in
/// order to propagate and apply (using [ytransaction_apply]) all updates known to a current
/// document, which remote peer was not aware of.
///
/// If passed `sv` pointer is null, the generated diff will be a snapshot containing entire state of
/// the document.
///
/// A length of an encoded state vector payload must be passed as `sv_len` parameter.
///
/// A length of generated delta diff binary will be passed within a `len` out parameter.
///
/// Once no longer needed, a returned binary can be disposed using [ybinary_destroy] function.
#[no_mangle]
pub unsafe extern "C" fn ytransaction_state_diff_v1(
    txn: *const Transaction,
    sv: *const c_uchar,
    sv_len: c_int,
    len: *mut c_int,
) -> *mut c_uchar {
    assert!(!txn.is_null());

    let sv = {
        if sv.is_null() {
            StateVector::default()
        } else {
            let sv_slice = std::slice::from_raw_parts(sv as *const u8, sv_len as usize);
            StateVector::decode_v1(sv_slice)
        }
    };

    let mut encoder = EncoderV1::new();
    txn.as_ref().unwrap().encode_diff(&sv, &mut encoder);
    let binary = encoder.to_vec().into_boxed_slice();
    *len = binary.len() as c_int;
    Box::into_raw(binary) as *mut c_uchar
}

/// Applies an diff update (generated by [ytransaction_state_diff_v1]) to a local transaction's
/// document.
///
/// A length of generated `diff` binary must be passed within a `diff_len` out parameter.
#[no_mangle]
pub unsafe extern "C" fn ytransaction_apply(txn: *mut Transaction, diff: *const c_uchar, diff_len: c_int) {
    assert!(!txn.is_null());
    assert!(!diff.is_null());

    let update = std::slice::from_raw_parts(diff as *const u8, diff_len as usize);
    let mut decoder = DecoderV1::from(update);
    let update = Update::decode(&mut decoder);
    let ds = DeleteSet::decode(&mut decoder);
    txn.as_mut().unwrap().apply_update(update, ds)
}

/// Returns the length of the `YText` string content in bytes (without the null terminator character)
#[no_mangle]
pub unsafe extern "C" fn ytext_len(txt: *const Text) -> c_int {
    assert!(!txt.is_null());
    txt.as_ref().unwrap().len() as c_int
}

/// Returns a null-terminated UTF-8 encoded string content of a current `YText` shared data type.
///
/// Generated string resources should be released using [ystring_destroy] function.
#[no_mangle]
pub unsafe extern "C" fn ytext_string(txt: *const Text, txn: *const Transaction) -> *mut c_char {
    assert!(!txt.is_null());
    assert!(!txn.is_null());

    let txn = txn.as_ref().unwrap();
    let str = txt.as_ref().unwrap().to_string(txn);
    CString::new(str).unwrap().into_raw()
}

/// Inserts a null-terminated UTF-8 encoded string a a given `index`. `index` value must be between
/// 0 and a length of a `YText` (inclusive, accordingly to [ytext_len] return value), otherwise this
/// function will panic.
///
/// A `str` parameter must be a null-terminated UTF-8 encoded string. This function doesn't take
/// ownership over a passed value - it will be copied and therefore a string parameter must be
/// released by the caller.
#[no_mangle]
pub unsafe extern "C" fn ytext_insert(
    txt: *const Text,
    txn: *mut Transaction,
    index: c_int,
    value: *const c_char,
) {
    assert!(!txt.is_null());
    assert!(!txn.is_null());
    assert!(!value.is_null());

    let chunk = CStr::from_ptr(value).to_str().unwrap();
    let txn = txn.as_mut().unwrap();
    let txt = txt.as_ref().unwrap();
    txt.insert(txn, index as u32, chunk)
}

/// Removes a range of characters, starting a a given `index`. This range must fit within the bounds
/// of a current `YText`, otherwise this function call will fail.
///
/// An `index` value must be between 0 and the length of a `YText` (exclusive, accordingly to
/// [ytext_len] return value).
///
/// A `length` must be lower or equal number of bytes (internally `YText` uses UTF-8 encoding) from
/// `index` position to the end of of the string.
#[no_mangle]
pub unsafe extern "C" fn ytext_remove_range(
    txt: *const Text,
    txn: *mut Transaction,
    index: c_int,
    length: c_int,
) {
    assert!(!txt.is_null());
    assert!(!txn.is_null());

    let txn = txn.as_mut().unwrap();
    let txt = txt.as_ref().unwrap();
    txt.remove_range(txn, index as u32, length as u32)
}

/// Returns a number of elements stored within current instance of `YArray`.
#[no_mangle]
pub unsafe extern "C" fn yarray_len(array: *const Array) -> c_int {
    assert!(!array.is_null());

    let array = array.as_ref().unwrap();
    array.len() as c_int
}

/// Returns a pointer to a `YOutput` value stored at a given `index` of a current `YArray`.
/// If `index` is outside of the bounds of an array, a null pointer will be returned.
///
/// A value returned should be eventually released using [youtput_destroy] function.
#[no_mangle]
pub unsafe extern "C" fn yarray_get(
    array: *const Array,
    txn: *mut Transaction,
    index: c_int,
) -> *mut YOutput {
    assert!(!array.is_null());
    assert!(!txn.is_null());

    let array = array.as_ref().unwrap();
    let txn = txn.as_mut().unwrap();

    if let Some(val) = array.get(txn, index as u32) {
        Box::into_raw(Box::new(YOutput::from(val)))
    } else {
        std::ptr::null_mut()
    }
}

/// Inserts a range of `items` into current `YArray`, starting at given `index`. An `items_len`
/// parameter is used to determine the size of `items` array - it can also be used to insert
/// a single element given its pointer.
///
/// An `index` value must be between 0 and (inclusive) length of a current array (use [yarray_len]
/// to determine its length), otherwise it will panic at runtime.
///
/// `YArray` doesn't take ownership over the inserted `items` data - their contents are being copied
/// into array structure - therefore caller is responsible for freeing all memory associated with
/// input params.
#[no_mangle]
pub unsafe extern "C" fn yarray_insert_range(
    array: *const Array,
    txn: *mut Transaction,
    index: c_int,
    items: *const YInput,
    items_len: c_int,
) {
    assert!(!array.is_null());
    assert!(!txn.is_null());
    assert!(!items.is_null());

    let arr = array.as_ref().unwrap();
    let txn = txn.as_mut().unwrap();

    let ptr = items;
    let mut i = 0;
    let len = items_len as isize;
    let mut vec: Vec<Any> = Vec::with_capacity(len as usize);

    // try read as many values a JSON-like primitives and insert them at once
    while i < len {
        let val = ptr.offset(i).read();
        if val.tag <= 0 {
            let any = val.into();
            vec.push(any);
        } else {
            break;
        }
        i += 1;
    }

    if !vec.is_empty() {
        arr.insert_range(txn, index as u32, vec);
    }

    // insert remaining values one by one
    while i < len {
        let val = ptr.offset(i).read();
        arr.push_back(txn, val);
        i += 1;
    }
}

/// Removes a `len` of consecutive range of elements from current `array` instance, starting at
/// a given `index`. Range determined by `index` and `len` must fit into boundaries of an array,
/// otherwise it will panic at runtime.
#[no_mangle]
pub unsafe extern "C" fn yarray_remove_range(
    array: *const Array,
    txn: *mut Transaction,
    index: c_int,
    len: c_int,
) {
    assert!(!array.is_null());
    assert!(!txn.is_null());

    let array = array.as_ref().unwrap();
    let txn = txn.as_mut().unwrap();

    array.remove_range(txn, index as u32, len as u32)
}

/// Returns an iterator, which can be used to traverse over all elements of an `array` (`array`'s
/// length can be determined using [yarray_len] function).
///
/// Use [yarray_iter_next] function in order to retrieve a consecutive array elements.
/// Use [yarray_iter_destroy] function in order to close the iterator and release its resources.
#[no_mangle]
pub unsafe extern "C" fn yarray_iter(
    array: *const Array,
    txn: *const Transaction,
) -> *mut ArrayIter {
    assert!(!array.is_null());
    assert!(!txn.is_null());

    let array = array.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();
    Box::into_raw(Box::new(array.iter(txn)))
}

/// Releases all of an `YArray` iterator resources created by calling [yarray_iter].
#[no_mangle]
pub unsafe extern "C" fn yarray_iter_destroy(iter: *mut ArrayIter) {
    if !iter.is_null() {
        drop(Box::from_raw(iter))
    }
}

/// Moves current `YArray` iterator over to a next element, returning a pointer to it. If an iterator
/// comes to an end of an array, a null pointer will be returned.
///
/// Returned values should be eventually released using [youtput_destroy] function.
#[no_mangle]
pub unsafe extern "C" fn yarray_iter_next(iterator: *mut ArrayIter) -> *mut YOutput {
    assert!(!iterator.is_null());

    let iter = iterator.as_mut().unwrap();
    if let Some(v) = iter.next() {
        Box::into_raw(Box::new(YOutput::from(v)))
    } else {
        std::ptr::null_mut()
    }
}

/// Returns an iterator, which can be used to traverse over all key-value pairs of a `map`.
///
/// Use [ymap_iter_next] function in order to retrieve a consecutive (**unordered**) map entries.
/// Use [ymap_iter_destroy] function in order to close the iterator and release its resources.
#[no_mangle]
pub unsafe extern "C" fn ymap_iter(map: *const Map, txn: *const Transaction) -> *mut MapIter {
    assert!(!map.is_null());
    assert!(!txn.is_null());

    let map = map.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();
    Box::into_raw(Box::new(map.iter(txn)))
}

/// Releases all of an `YMap` iterator resources created by calling [ymap_iter].
#[no_mangle]
pub unsafe extern "C" fn ymap_iter_destroy(iter: *mut MapIter) {
    if !iter.is_null() {
        drop(Box::from_raw(iter))
    }
}

/// Moves current `YMap` iterator over to a next entry, returning a pointer to it. If an iterator
/// comes to an end of a map, a null pointer will be returned. Yrs maps are unordered and so are
/// their iterators.
///
/// Returned values should be eventually released using [ymap_entry_destroy] function.
#[no_mangle]
pub unsafe extern "C" fn ymap_iter_next(iter: *mut MapIter) -> *mut YMapEntry {
    assert!(!iter.is_null());

    let iter = iter.as_mut().unwrap();
    if let Some((key, value)) = iter.next() {
        Box::into_raw(Box::new(YMapEntry::new(key.as_str(), value)))
    } else {
        std::ptr::null_mut()
    }
}

/// Returns a number of entries stored within a `map`.
#[no_mangle]
pub unsafe extern "C" fn ymap_len(map: *const Map, txn: *const Transaction) -> c_int {
    assert!(!map.is_null());
    assert!(!txn.is_null());

    let map = map.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    map.len(txn) as c_int
}

/// Inserts a new entry (specified as `key`-`value` pair) into a current `map`. If entry under such
/// given `key` already existed, its corresponding value will be replaced.
///
/// A `key` must be a null-terminated UTF-8 encoded string, which contents will be copied into
/// a `map` (therefore it must be freed by the function caller).
///
/// A `value` content is being copied into a `map`, therefore any of its content must be freed by
/// the function caller.
#[no_mangle]
pub unsafe extern "C" fn ymap_insert(
    map: *const Map,
    txn: *mut Transaction,
    key: *const c_char,
    value: *const YInput,
) {
    assert!(!map.is_null());
    assert!(!txn.is_null());
    assert!(!key.is_null());
    assert!(!value.is_null());

    let cstr = CStr::from_ptr(key);
    let key = cstr.to_str().unwrap().to_string();

    let map = map.as_ref().unwrap();
    let txn = txn.as_mut().unwrap();

    map.insert(txn, key, value.read());
}

/// Removes a `map` entry, given its `key`. Returns `1` if the corresponding entry was successfully
/// removed or `0` if no entry with a provided `key` has been found inside of a `map`.
///
/// A `key` must be a null-terminated UTF-8 encoded string.
#[no_mangle]
pub unsafe extern "C" fn ymap_remove(
    map: *const Map,
    txn: *mut Transaction,
    key: *const c_char,
) -> c_char {
    assert!(!map.is_null());
    assert!(!txn.is_null());
    assert!(!key.is_null());

    let key = CStr::from_ptr(key).to_str().unwrap();

    let map = map.as_ref().unwrap();
    let txn = txn.as_mut().unwrap();

    if let Some(_) = map.remove(txn, key) {
        Y_TRUE
    } else {
        Y_FALSE
    }
}

/// Returns a value stored under the provided `key`, or a null pointer if no entry with such `key`
/// has been found in a current `map`. A returned value is allocated by this function and therefore
/// should be eventually released using [youtput_destroy] function.
///
/// A `key` must be a null-terminated UTF-8 encoded string.
#[no_mangle]
pub unsafe extern "C" fn ymap_get(
    map: *const Map,
    txn: *const Transaction,
    key: *const c_char,
) -> *mut YOutput {
    assert!(!map.is_null());
    assert!(!txn.is_null());
    assert!(!key.is_null());

    let key = CStr::from_ptr(key).to_str().unwrap();

    let map = map.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    if let Some(value) = map.get(txn, key) {
        Box::into_raw(Box::new(YOutput::from(value)))
    } else {
        std::ptr::null_mut()
    }
}

/// Removes all entries from a current `map`.
#[no_mangle]
pub unsafe extern "C" fn ymap_remove_all(map: *const Map, txn: *mut Transaction) {
    assert!(!map.is_null());
    assert!(!txn.is_null());

    let map = map.as_ref().unwrap();
    let txn = txn.as_mut().unwrap();

    map.clear(txn);
}

/// Return a name (or an XML tag) of a current `YXmlElement`. Root-level XML nodes use "UNDEFINED" as
/// their tag names.
///
/// Returned value is a null-terminated UTF-8 string, which must be released using [ystring_destroy]
/// function.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_tag(xml: *const XmlElement) -> *mut c_char {
    assert!(!xml.is_null());
    let xml = xml.as_ref().unwrap();
    let tag = xml.tag();
    CString::new(tag).unwrap().into_raw()
}

/// Converts current `YXmlElement` together with its children and attributes into a flat string
/// representation (no padding) eg. `<UNDEFINED><title key="value">sample text</title></UNDEFINED>`.
///
/// Returned value is a null-terminated UTF-8 string, which must be released using [ystring_destroy]
/// function.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_string(
    xml: *const XmlElement,
    txn: *const Transaction,
) -> *mut c_char {
    assert!(!xml.is_null());
    assert!(!txn.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    let str = xml.to_string(txn);
    CString::new(str).unwrap().into_raw()
}

/// Inserts an XML attribute described using `attr_name` and `attr_value`. If another attribute with
/// the same name already existed, its value will be replaced with a provided one.
///
/// Both `attr_name` and `attr_value` must be a null-terminated UTF-8 encoded strings. Their
/// contents are being copied, therefore it's up to a function caller to properly release them.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_insert_attr(
    xml: *const XmlElement,
    txn: *mut Transaction,
    attr_name: *const c_char,
    attr_value: *const c_char,
) {
    assert!(!xml.is_null());
    assert!(!txn.is_null());
    assert!(!attr_name.is_null());
    assert!(!attr_value.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_mut().unwrap();

    let key = CStr::from_ptr(attr_name).to_str().unwrap();
    let value = CStr::from_ptr(attr_value).to_str().unwrap();

    xml.insert_attribute(txn, key, value);
}

/// Removes an attribute from a current `YXmlElement`, given its name.
///
/// An `attr_name`must be a null-terminated UTF-8 encoded string.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_remove_attr(
    xml: *const XmlElement,
    txn: *mut Transaction,
    attr_name: *const c_char,
) {
    assert!(!xml.is_null());
    assert!(!txn.is_null());
    assert!(!attr_name.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_mut().unwrap();

    let key = CStr::from_ptr(attr_name).to_str().unwrap();
    xml.remove_attribute(txn, key);
}

/// Returns the value of a current `YXmlElement`, given its name, or a null pointer if not attribute
/// with such name has been found. Returned pointer is a null-terminated UTF-8 encoded string, which
/// should be released using [ystring_destroy] function.
///
/// An `attr_name` must be a null-terminated UTF-8 encoded string.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_get_attr(
    xml: *const XmlElement,
    txn: *const Transaction,
    attr_name: *const c_char,
) -> *mut c_char {
    assert!(!xml.is_null());
    assert!(!txn.is_null());
    assert!(!attr_name.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    let key = CStr::from_ptr(attr_name).to_str().unwrap();
    if let Some(value) = xml.get_attribute(txn, key) {
        CString::new(value).unwrap().into_raw()
    } else {
        std::ptr::null_mut()
    }
}

/// Returns an iterator over the `YXmlElement` attributes.
///
/// Use [yxmlattr_iter_next] function in order to retrieve a consecutive (**unordered**) attributes.
/// Use [yxmlattr_iter_destroy] function in order to close the iterator and release its resources.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_attr_iter(
    xml: *const XmlElement,
    txn: *const Transaction,
) -> *mut Attributes {
    assert!(!xml.is_null());
    assert!(!txn.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    Box::into_raw(Box::new(xml.attributes(txn)))
}

/// Returns an iterator over the `YXmlText` attributes.
///
/// Use [yxmlattr_iter_next] function in order to retrieve a consecutive (**unordered**) attributes.
/// Use [yxmlattr_iter_destroy] function in order to close the iterator and release its resources.
#[no_mangle]
pub unsafe extern "C" fn yxmltext_attr_iter(
    xml: *const XmlText,
    txn: *const Transaction,
) -> *mut Attributes {
    assert!(!xml.is_null());
    assert!(!txn.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    Box::into_raw(Box::new(xml.attributes(txn)))
}

/// Releases all of attributes iterator resources created by calling [yxmlelem_attr_iter]
/// or [yxmltext_attr_iter].
#[no_mangle]
pub unsafe extern "C" fn yxmlattr_iter_destroy(iterator: *mut Attributes) {
    if !iterator.is_null() {
        drop(Box::from_raw(iterator))
    }
}

/// Returns a next XML attribute from an `iterator`. Attributes are returned in an unordered
/// manner. Once `iterator` reaches the end of attributes collection, a null pointer will be
/// returned.
///
/// Returned value should be eventually released using [yxmlattr_destroy].
#[no_mangle]
pub unsafe extern "C" fn yxmlattr_iter_next(iterator: *mut Attributes) -> *mut YXmlAttr {
    assert!(!iterator.is_null());

    let iter = iterator.as_mut().unwrap();

    if let Some((name, value)) = iter.next() {
        Box::into_raw(Box::new(YXmlAttr {
            name: CString::new(name).unwrap().into_raw(),
            value: CString::new(value).unwrap().into_raw(),
        }))
    } else {
        std::ptr::null_mut()
    }
}

/// Returns a next sibling of a current `YXmlElement`, which can be either another `YXmlElement`
/// or a `YXmlText`. Together with [yxmlelem_first_child] it may be used to iterate over the direct
/// children of an XML node (in order to iterate over the nested XML structure use
/// [yxmlelem_tree_walker]).
///
/// If current `YXmlElement` is the last child, this function returns a null pointer.
/// A returned value should be eventually released using [youtput_destroy] function.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_next_sibling(
    xml: *const XmlElement,
    txn: *const Transaction,
) -> *mut YOutput {
    assert!(!xml.is_null());
    assert!(!txn.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    if let Some(next) = xml.next_sibling(txn) {
        match next {
            Xml::Element(v) =>
                Box::into_raw(Box::new(YOutput::from(Value::YXmlElement(v)))),
            Xml::Text(v) => Box::into_raw(Box::new(YOutput::from(Value::YXmlText(v)))),
        }
    } else {
        std::ptr::null_mut()
    }
}

/// Returns a previous sibling of a current `YXmlElement`, which can be either another `YXmlElement`
/// or a `YXmlText`.
///
/// If current `YXmlElement` is the first child, this function returns a null pointer.
/// A returned value should be eventually released using [youtput_destroy] function.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_prev_sibling(
    xml: *const XmlElement,
    txn: *const Transaction,
) -> *mut YOutput {
    assert!(!xml.is_null());
    assert!(!txn.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    if let Some(next) = xml.prev_sibling(txn) {
        match next {
            Xml::Element(v) => Box::into_raw(Box::new(YOutput::from(Value::YXmlElement(v)))),
            Xml::Text(v) => Box::into_raw(Box::new(YOutput::from(Value::YXmlText(v)))),
        }
    } else {
        std::ptr::null_mut()
    }
}

/// Returns a next sibling of a current `YXmlText`, which can be either another `YXmlText` or
/// an `YXmlElement`. Together with [yxmlelem_first_child] it may be used to iterate over the direct
/// children of an XML node (in order to iterate over the nested XML structure use
/// [yxmlelem_tree_walker]).
///
/// If current `YXmlText` is the last child, this function returns a null pointer.
/// A returned value should be eventually released using [youtput_destroy] function.
#[no_mangle]
pub unsafe extern "C" fn yxmltext_next_sibling(
    xml: *const XmlText,
    txn: *const Transaction,
) -> *mut YOutput {
    assert!(!xml.is_null());
    assert!(!txn.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    if let Some(next) = xml.next_sibling(txn) {
        match next {
            Xml::Element(v) => Box::into_raw(Box::new(YOutput::from(Value::YXmlElement(v)))),
            Xml::Text(v) => Box::into_raw(Box::new(YOutput::from(Value::YXmlText(v)))),
        }
    } else {
        std::ptr::null_mut()
    }
}

/// Returns a previous sibling of a current `YXmlText`, which can be either another `YXmlText` or
/// an `YXmlElement`.
///
/// If current `YXmlText` is the first child, this function returns a null pointer.
/// A returned value should be eventually released using [youtput_destroy] function.
#[no_mangle]
pub unsafe extern "C" fn yxmltext_prev_sibling(
    xml: *const XmlText,
    txn: *const Transaction,
) -> *mut YOutput {
    assert!(!xml.is_null());
    assert!(!txn.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    if let Some(next) = xml.prev_sibling(txn) {
        match next {
            Xml::Element(v) => Box::into_raw(Box::new(YOutput::from(Value::YXmlElement(v)))),
            Xml::Text(v) => Box::into_raw(Box::new(YOutput::from(Value::YXmlText(v)))),
        }
    } else {
        std::ptr::null_mut()
    }
}

/// Returns a parent `YXmlElement` of a current node, or null pointer when current `YXmlElement` is
/// a root-level shared data type.
///
/// A returned value should be eventually released using [youtput_destroy] function.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_parent(
    xml: *const XmlElement,
    txn: *const Transaction,
) -> *mut XmlElement {
    assert!(!xml.is_null());
    assert!(!txn.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    if let Some(parent) = xml.parent(txn) {
        Box::into_raw(Box::new(parent))
    } else {
        std::ptr::null_mut()
    }
}

/// Returns a number of child nodes (both `YXmlElement` and `YXmlText`) living under a current XML
/// element. This function doesn't count a recursive nodes, only direct children of a current node.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_child_len(xml: *const XmlElement, txn: *const Transaction) -> c_int {
    assert!(!xml.is_null());
    assert!(!txn.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    xml.len(txn) as c_int
}

/// Returns a first child node of a current `YXmlElement`, or null pointer if current XML node is
/// empty. Returned value could be either another `YXmlElement` or `YXmlText`.
///
/// A returned value should be eventually released using [youtput_destroy] function.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_first_child(xml: *const XmlElement, txn: *const Transaction) -> *mut YOutput {
    assert!(!xml.is_null());
    assert!(!txn.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    if let Some(value) = xml.first_child(txn) {
        match value {
            Xml::Element(v) => Box::into_raw(Box::new(YOutput::from(Value::YXmlElement(v)))),
            Xml::Text(v) => Box::into_raw(Box::new(YOutput::from(Value::YXmlText(v)))),
        }
    } else {
        std::ptr::null_mut()
    }
}

/// Returns an iterator over a nested recursive structure of a current `YXmlElement`, starting from
/// first of its children. Returned values can be either `YXmlElement` or `YXmlText` nodes.
///
/// Use [yxmlelem_tree_walker_next] function in order to iterate over to a next node.
/// Use [yxmlelem_tree_walker_destroy] function to release resources used by the iterator.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_tree_walker(
    xml: *const XmlElement,
    txn: *const Transaction,
) -> *mut TreeWalker {
    assert!(!xml.is_null());
    assert!(!txn.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    Box::into_raw(Box::new(xml.successors(txn)))
}

/// Releases resources associated with a current XML tree walker iterator.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_tree_walker_destroy(iter: *mut TreeWalker) {
    if !iter.is_null() {
        drop(Box::from_raw(iter))
    }
}

/// Moves current `iterator` to a next value (either `YXmlElement` or `YXmlText`), returning its
/// pointer or a null, if an `iterator` already reached the last successor node.
///
/// Values returned by this function should be eventually released using [youtput_destroy].
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_tree_walker_next(iterator: *mut TreeWalker) -> *mut YOutput {
    assert!(!iterator.is_null());

    let iter = iterator.as_mut().unwrap();

    if let Some(next) = iter.next() {
        match next {
            Xml::Element(v) => Box::into_raw(Box::new(YOutput::from(Value::YXmlElement(v)))),
            Xml::Text(v) => Box::into_raw(Box::new(YOutput::from(Value::YXmlText(v)))),
        }
    } else {
        std::ptr::null_mut()
    }
}

/// Inserts an `YXmlElement` as a child of a current node at the given `index` and returns its
/// pointer. Node created this way will have a given `name` as its tag (eg. `p` for `<p></p>` node).
///
/// An `index` value must be between 0 and (inclusive) length of a current XML element (use
/// [yxmlelem_child_len] function to determine its length).
///
/// A `name` must be a null-terminated UTF-8 encoded string, which will be copied into current
/// document. Therefore `name` should be freed by the function caller.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_insert_elem(
    xml: *const XmlElement,
    txn: *mut Transaction,
    index: c_int,
    name: *const c_char,
) -> *mut XmlElement {
    assert!(!xml.is_null());
    assert!(!txn.is_null());
    assert!(!name.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_mut().unwrap();

    let name = CStr::from_ptr(name).to_str().unwrap();
    let child = xml.insert_elem(txn, index as u32, name);

    Box::into_raw(Box::new(child))
}

/// Inserts an `YXmlText` as a child of a current node at the given `index` and returns its
/// pointer.
///
/// An `index` value must be between 0 and (inclusive) length of a current XML element (use
/// [yxmlelem_child_len] function to determine its length).
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_insert_text(
    xml: *const XmlElement,
    txn: *mut Transaction,
    index: c_int,
) -> *mut XmlText {
    assert!(!xml.is_null());
    assert!(!txn.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_mut().unwrap();
    let child = xml.insert_text(txn, index as u32);

    Box::into_raw(Box::new(child))
}

/// Removes a consecutive range of child elements (of specified length) from the current
/// `YXmlElement`, starting at the given `index`. Specified range must fit into boundaries of current
/// XML node children, otherwise this function will panic at runtime.
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_remove_range(
    xml: *const XmlElement,
    txn: *mut Transaction,
    index: c_int,
    len: c_int,
) {
    assert!(!xml.is_null());
    assert!(!txn.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_mut().unwrap();

    xml.remove_range(txn, index as u32, len as u32)
}

/// Returns an XML child node (either a `YXmlElement` or `YXmlText`) stored at a given `index` of
/// a current `YXmlElement`. Returns null pointer if `index` was outside of the bound of current XML
/// node children.
///
/// Returned value should be eventually released using [youtput_destroy].
#[no_mangle]
pub unsafe extern "C" fn yxmlelem_get(
    xml: *const XmlElement,
    txn: *const Transaction,
    index: c_int,
) -> *const YOutput {
    assert!(!xml.is_null());
    assert!(!txn.is_null());

    let xml = xml.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    if let Some(child) = xml.get(txn, index as u32) {
        match child {
            Xml::Element(v) => Box::into_raw(Box::new(YOutput::from(Value::YXmlElement(v)))),
            Xml::Text(v) => Box::into_raw(Box::new(YOutput::from(Value::YXmlText(v)))),
        }
    } else {
        std::ptr::null()
    }
}

/// Returns the length of the `YXmlText` string content in bytes (without the null terminator
/// character)
#[no_mangle]
pub unsafe extern "C" fn yxmltext_len(txt: *const XmlText, txn: *const Transaction) -> c_int {
    assert!(!txt.is_null());
    assert!(!txn.is_null());

    let txt = txt.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    txt.len(txn) as c_int
}

/// Returns a null-terminated UTF-8 encoded string content of a current `YXmlText` shared data type.
///
/// Generated string resources should be released using [ystring_destroy] function.
#[no_mangle]
pub unsafe extern "C" fn yxmltext_string(
    txt: *const XmlText,
    txn: *const Transaction,
) -> *mut c_char {
    assert!(!txt.is_null());
    assert!(!txn.is_null());

    let txt = txt.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();

    let str = txt.to_string(txn);
    CString::new(str).unwrap().into_raw()
}

/// Inserts a null-terminated UTF-8 encoded string a a given `index`. `index` value must be between
/// 0 and a length of a `YXmlText` (inclusive, accordingly to [yxmltext_len] return value), otherwise
/// this function will panic.
///
/// A `str` parameter must be a null-terminated UTF-8 encoded string. This function doesn't take
/// ownership over a passed value - it will be copied and therefore a string parameter must be
/// released by the caller.
#[no_mangle]
pub unsafe extern "C" fn yxmltext_insert(
    txt: *const XmlText,
    txn: *mut Transaction,
    index: c_int,
    str: *const c_char,
) {
    assert!(!txt.is_null());
    assert!(!txn.is_null());
    assert!(!str.is_null());

    let txt = txt.as_ref().unwrap();
    let txn = txn.as_mut().unwrap();

    let chunk = CStr::from_ptr(str).to_str().unwrap();
    txt.insert(txn, index as u32, chunk)
}

/// Removes a range of characters, starting a a given `index`. This range must fit within the bounds
/// of a current `YXmlText`, otherwise this function call will fail.
///
/// An `index` value must be between 0 and the length of a `YXmlText` (exclusive, accordingly to
/// [yxmltext_len] return value).
///
/// A `length` must be lower or equal number of bytes (internally `YXmlText` uses UTF-8 encoding)
/// from `index` position to the end of of the string.
#[no_mangle]
pub unsafe extern "C" fn yxmltext_remove_range(
    txt: *const XmlText,
    txn: *mut Transaction,
    idx: c_int,
    len: c_int,
) {
    assert!(!txt.is_null());
    assert!(!txn.is_null());

    let txt = txt.as_ref().unwrap();
    let txn = txn.as_mut().unwrap();
    txt.remove_range(txn, idx as u32, len as u32)
}

/// Inserts an XML attribute described using `attr_name` and `attr_value`. If another attribute with
/// the same name already existed, its value will be replaced with a provided one.
///
/// Both `attr_name` and `attr_value` must be a null-terminated UTF-8 encoded strings. Their
/// contents are being copied, therefore it's up to a function caller to properly release them.
#[no_mangle]
pub unsafe extern "C" fn yxmltext_insert_attr(
    txt: *const XmlText,
    txn: *mut Transaction,
    attr_name: *const c_char,
    attr_value: *const c_char,
) {
    assert!(!txt.is_null());
    assert!(!txn.is_null());
    assert!(!attr_name.is_null());
    assert!(!attr_value.is_null());

    let txt = txt.as_ref().unwrap();
    let txn = txn.as_mut().unwrap();

    let name = CStr::from_ptr(attr_name).to_str().unwrap();
    let value = CStr::from_ptr(attr_value).to_str().unwrap();

    txt.insert_attribute(txn, name, value)
}

/// Removes an attribute from a current `YXmlText`, given its name.
///
/// An `attr_name`must be a null-terminated UTF-8 encoded string.
#[no_mangle]
pub unsafe extern "C" fn yxmltext_remove_attr(
    txt: *const XmlText,
    txn: *mut Transaction,
    attr_name: *const c_char,
) {
    assert!(!txt.is_null());
    assert!(!txn.is_null());
    assert!(!attr_name.is_null());

    let txt = txt.as_ref().unwrap();
    let txn = txn.as_mut().unwrap();
    let name = CStr::from_ptr(attr_name).to_str().unwrap();

    txt.remove_attribute(txn, name)
}

/// Returns the value of a current `YXmlText`, given its name, or a null pointer if not attribute
/// with such name has been found. Returned pointer is a null-terminated UTF-8 encoded string, which
/// should be released using [ystring_destroy] function.
///
/// An `attr_name` must be a null-terminated UTF-8 encoded string.
#[no_mangle]
pub unsafe extern "C" fn yxmltext_get_attr(
    txt: *const XmlText,
    txn: *const Transaction,
    attr_name: *const c_char,
) -> *mut c_char {
    assert!(!txt.is_null());
    assert!(!txn.is_null());
    assert!(!attr_name.is_null());

    let txt = txt.as_ref().unwrap();
    let txn = txn.as_ref().unwrap();
    let name = CStr::from_ptr(attr_name).to_str().unwrap();

    if let Some(value) = txt.get_attribute(txn, name) {
        CString::new(value).unwrap().into_raw()
    } else {
        std::ptr::null_mut()
    }
}

/// A data structure that is used to pass input values of various types supported by Yrs into a
/// shared document store.
///
/// `YInput` constructor function don't allocate any resources on their own, neither they take
/// ownership by pointers to memory blocks allocated by user - for this reason once an input cell
/// has been used, its content should be freed by the caller.
#[repr(C)]
pub struct YInput {
    /// Tag describing, which `value` type is being stored by this input cell. Can be one of:
    ///
    /// - [Y_JSON_BOOL] for boolean flags.
    /// - [Y_JSON_NUM] for 64-bit floating point numbers.
    /// - [Y_JSON_INT] for 64-bit signed integers.
    /// - [Y_JSON_STR] for null-terminated UTF-8 encoded strings.
    /// - [Y_JSON_BUF] for embedded binary data.
    /// - [Y_JSON_ARR] for arrays of JSON-like values.
    /// - [Y_JSON_MAP] for JSON-like objects build from key-value pairs.
    /// - [Y_JSON_NULL] for JSON-like null values.
    /// - [Y_JSON_UNDEF] for JSON-like undefined values.
    /// - [Y_ARRAY] for cells which contents should be used to initialize a `YArray` shared type.
    /// - [Y_MAP] for cells which contents should be used to initialize a `YMap` shared type.
    pub tag: c_char,

    /// Length of the contents stored by current `YInput` cell.
    ///
    /// For [Y_JSON_NULL] and [Y_JSON_UNDEF] its equal to `0`.
    ///
    /// For [Y_JSON_ARR], [Y_JSON_MAP], [Y_ARRAY] and [Y_MAP] it describes a number of passed
    /// elements.
    ///
    /// For other types it's always equal to `1`.
    pub len: c_int,

    /// Union struct which contains a content corresponding to a provided `tag` field.
    value: YInputContent,
}

impl YInput {
    fn into(self) -> Any {
        let tag = self.tag;
        unsafe {
            if tag == Y_JSON_STR {
                let str = CStr::from_ptr(self.value.str).to_str().unwrap().to_owned();
                Any::String(str)
            } else if tag == Y_JSON_ARR {
                let ptr = self.value.values;
                let mut dst: Vec<Any> = Vec::with_capacity(self.len as usize);
                let mut i = 0;
                while i < self.len as isize {
                    let value = ptr.offset(i).read();
                    let any = value.into();
                    dst.push(any);
                    i += 1;
                }
                Any::Array(dst)
            } else if tag == Y_JSON_MAP {
                let mut dst = HashMap::with_capacity(self.len as usize);
                let mut keys = self.value.map.keys;
                let mut values = self.value.map.values;
                let mut i = 0;
                while i < self.len as isize {
                    let key = CStr::from_ptr(keys.offset(i).read()).to_str().unwrap().to_owned();
                    let value = values.offset(i).read().into();
                    dst.insert(key, value);
                    i += 1;
                }
                Any::Map(dst)
            } else if tag == Y_JSON_NULL {
                Any::Null
            } else if tag == Y_JSON_UNDEF {
                Any::Undefined
            } else if tag == Y_JSON_INT {
                Any::BigInt(self.value.integer as i64)
            } else if tag == Y_JSON_NUM {
                Any::Number(self.value.num as f64)
            } else if tag == Y_JSON_BOOL {
                Any::Bool(if self.value.flag == 0 { false } else { true })
            } else if tag == Y_JSON_BUF {
                let slice = std::slice::from_raw_parts(self.value.buf as *mut u8, self.len as usize);
                let buf = Box::from(slice);
                Any::Buffer(buf)
            } else {
                panic!("Unrecognized YVal value tag.")
            }
        }
    }
}

#[repr(C)]
union YInputContent {
    flag: c_char,
    num: c_float,
    integer: c_long,
    str: *mut c_char,
    buf: *mut c_uchar,
    values: *mut YInput,
    map: ManuallyDrop<YMapInputData>,
}

#[repr(C)]
struct YMapInputData {
    keys: *mut *mut c_char,
    values: *mut YInput,
}

impl Drop for YInput {
    fn drop(&mut self) {
    }
}

impl Prelim for YInput {
    fn into_content(
        self,
        _txn: &mut yrs::Transaction,
        ptr: TypePtr,
    ) -> (ItemContent, Option<Self>) {
        unsafe {
            if self.tag <= 0 {
                let value = self.into();
                (ItemContent::Any(vec![value]), None)
            } else {
                let type_ref = if self.tag == Y_MAP {
                    TYPE_REFS_MAP
                } else if self.tag == Y_ARRAY {
                    TYPE_REFS_ARRAY
                } else if self.tag == Y_XML_ELEM {
                    TYPE_REFS_XML_ELEMENT
                } else if self.tag == Y_XML_TEXT {
                    TYPE_REFS_XML_TEXT
                } else {
                    panic!("Unrecognized YVal value tag.")
                };
                let name = if type_ref == TYPE_REFS_XML_ELEMENT {
                    let name = CStr::from_ptr(self.value.str).to_str().unwrap().to_owned();
                    Some(name)
                } else {
                    None
                };
                let inner = BranchRef::new(Branch::new(ptr, type_ref, name));
                (ItemContent::Type(inner), Some(self))
            }
        }
    }

    fn integrate(self, txn: &mut yrs::Transaction, inner_ref: BranchRef) {
        unsafe {
            if self.tag == Y_MAP {
                let map = Map::from(inner_ref);
                let mut keys = self.value.map.keys;
                let mut values = self.value.map.values;
                let mut i = 0;
                while i < self.len as isize {
                    let key = CStr::from_ptr(keys.offset(i).read()).to_str().unwrap().to_owned();
                    let value = values.offset(i).read().into();
                    map.insert(txn, key, value);
                }
            } else if self.tag == Y_ARRAY {
                let array = Array::from(inner_ref);
                let ptr = self.value.values;
                let len = self.len as isize;
                let mut i = 0;
                while i < len {
                    let value = ptr.offset(i).read();
                    array.push_back(txn, value);
                    i += 1;
                }
            } else if self.tag == Y_TEXT {
                let text = Text::from(inner_ref);
                let init = CStr::from_ptr(self.value.str).to_str().unwrap();
                text.push(txn, init);
            } else if self.tag == Y_XML_TEXT {
                let text = XmlText::from(inner_ref);
                let init = CStr::from_ptr(self.value.str).to_str().unwrap();
                text.push(txn, init);
            };
        }
    }
}

/// An output value cell returned from yrs API methods. It describes a various types of data
/// supported by yrs shared data types.
///
/// Since `YOutput` instances are always created by calling the corresponding yrs API functions,
/// they eventually should be deallocated using [youtput_destroy] function.
#[repr(C)]
pub struct YOutput {
    /// Tag describing, which `value` type is being stored by this input cell. Can be one of:
    ///
    /// - [Y_JSON_BOOL] for boolean flags.
    /// - [Y_JSON_NUM] for 64-bit floating point numbers.
    /// - [Y_JSON_INT] for 64-bit signed integers.
    /// - [Y_JSON_STR] for null-terminated UTF-8 encoded strings.
    /// - [Y_JSON_BUF] for embedded binary data.
    /// - [Y_JSON_ARR] for arrays of JSON-like values.
    /// - [Y_JSON_MAP] for JSON-like objects build from key-value pairs.
    /// - [Y_JSON_NULL] for JSON-like null values.
    /// - [Y_JSON_UNDEF] for JSON-like undefined values.
    /// - [Y_TEXT] for pointers to `YText` data types.
    /// - [Y_ARRAY] for pointers to `YArray` data types.
    /// - [Y_MAP] for pointers to `YMap` data types.
    /// - [Y_XML_ELEM] for pointers to `YXmlElement` data types.
    /// - [Y_XML_TEXT] for pointers to `YXmlText` data types.
    pub tag: c_char,

    /// Length of the contents stored by a current `YOutput` cell.
    ///
    /// For [Y_JSON_NULL] and [Y_JSON_UNDEF] its equal to `0`.
    ///
    /// For [Y_JSON_ARR], [Y_JSON_MAP] it describes a number of passed elements.
    ///
    /// For other types it's always equal to `1`.
    pub len: c_int,

    /// Union struct which contains a content corresponding to a provided `tag` field.
    value: YOutputContent,
}

impl std::fmt::Display for YOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let tag = self.tag;
        unsafe {
            if tag == Y_JSON_INT {
                write!(f, "{}", self.value.integer)
            } else if tag == Y_JSON_NUM {
                write!(f, "{}", self.value.num)
            } else if tag == Y_JSON_BOOL {
                write!(f, "{}", if self.value.flag == 0 { "false" } else { "true" })
            } else if tag == Y_JSON_UNDEF {
                write!(f, "undefined")
            } else if tag == Y_JSON_NULL {
                write!(f, "null")
            } else if tag == Y_JSON_STR {
                write!(f,"{}", CString::from_raw(self.value.str).to_str().unwrap())
            } else if tag == Y_MAP {
                write!(f,"YMap")
            } else if tag == Y_ARRAY {
                write!(f,"YArray")
            } else if tag == Y_JSON_ARR {
                write!(f,"[")?;
                let slice = std::slice::from_raw_parts(self.value.array, self.len as usize);
                for o in slice {
                    write!(f, ", {}", o)?;
                }
                write!(f,"]")
            } else if tag == Y_JSON_MAP {
                write!(f,"{{")?;
                let slice = std::slice::from_raw_parts(self.value.map, self.len as usize);
                for e in slice {
                    write!(f, ", '{}' => {}", CStr::from_ptr(e.key).to_str().unwrap(), e.value)?;
                }
                write!(f,"}}")
            } else if tag == Y_TEXT {
                write!(f,"YText")
            } else if tag == Y_XML_TEXT {
                write!(f,"YXmlText")
            } else if tag == Y_XML_ELEM {
                write!(f,"YXmlElement", )
            } else if tag == Y_JSON_BUF {
                write!(f,"YBinary(len: {})", self.len)
            } else {
                Ok(())
            }
        }
    }
}

impl Drop for YOutput {
    fn drop(&mut self) {
        let tag = self.tag;
        unsafe {
            if tag == Y_JSON_STR {
                drop(CString::from_raw(self.value.str));
            } else if tag == Y_MAP {
                drop(Box::from_raw(self.value.y_map));
            } else if tag == Y_ARRAY {
                drop(Box::from_raw(self.value.y_array));
            } else if tag == Y_JSON_ARR {
                drop(Vec::from_raw_parts(self.value.array, self.len as usize, self.len as usize));
            } else if tag == Y_JSON_MAP {
                drop(Vec::from_raw_parts(self.value.map, self.len as usize, self.len as usize));
            } else if tag == Y_TEXT {
                drop(Box::from_raw(self.value.y_text));
            } else if tag == Y_XML_TEXT {
                drop(Box::from_raw(self.value.y_xmltext));
            } else if tag == Y_XML_ELEM {
                drop(Box::from_raw(self.value.y_xmlelem));
            } else if tag == Y_JSON_BUF {
                drop(Vec::from_raw_parts(self.value.buf, self.len as usize, self.len as usize));
            }
        }
    }
}

impl From<Value> for YOutput {
    fn from(v: Value) -> Self {
        match v {
            Value::Any(v) => Self::from(v),
            Value::YText(v) => Self::from(v),
            Value::YArray(v) => Self::from(v),
            Value::YMap(v) => Self::from(v),
            Value::YXmlElement(v) => Self::from(v),
            Value::YXmlText(v) => Self::from(v),
        }
    }
}

impl From<Any> for YOutput {
    fn from(v: Any) -> Self {
        unsafe {
            match v {
                Any::Null => YOutput {
                    tag: Y_JSON_NULL,
                    len: 0,
                    value: MaybeUninit::uninit().assume_init()
                },
                Any::Undefined => YOutput {
                    tag: Y_JSON_UNDEF,
                    len: 0,
                    value: MaybeUninit::uninit().assume_init()
                },
                Any::Bool(v) => YOutput {
                    tag: Y_JSON_BOOL,
                    len: 1,
                    value: YOutputContent { flag: if v { Y_TRUE } else { Y_FALSE } }
                },
                Any::Number(v) => YOutput {
                    tag: Y_JSON_NUM,
                    len: 1,
                    value: YOutputContent { num: v as _ }
                },
                Any::BigInt(v) => YOutput {
                    tag: Y_JSON_INT,
                    len: 1,
                    value: YOutputContent { integer: v as _ }
                },
                Any::String(v) => YOutput {
                    tag: Y_JSON_STR,
                    len: v.len() as c_int,
                    value: YOutputContent { str: CString::new(v).unwrap().into_raw() }
                },
                Any::Buffer(v) => YOutput {
                    tag: Y_JSON_BUF,
                    len: v.len() as c_int,
                    value: YOutputContent { buf: Box::into_raw(v) as *mut _ }
                },
                Any::Array(v) => {
                    let len = v.len() as c_int;
                    let mut array: Vec<_> = v.into_iter().map(|v| YOutput::from(v)).collect();
                    array.shrink_to_fit();
                    let ptr = array.as_mut_ptr();
                    forget(array);
                    YOutput {
                        tag: Y_JSON_ARR,
                        len,
                        value: YOutputContent { array: ptr }
                    }
                }
                Any::Map(v) => {
                    let len = v.len() as c_int;
                    let mut array: Vec<_> = v.into_iter()
                        .map(|(k,v)| YMapEntry::new(k.as_str(), Value::Any(v)))
                        .collect();
                    array.shrink_to_fit();
                    let ptr = array.as_mut_ptr();
                    forget(array);
                    YOutput {
                        tag: Y_JSON_MAP,
                        len,
                        value: YOutputContent { map: ptr }
                    }
                }
            }
        }
    }
}

impl From<Text> for YOutput {
    fn from(v: Text) -> Self {
        YOutput {
            tag: Y_TEXT,
            len: 1,
            value: YOutputContent { y_text: Box::into_raw(Box::new(v)) }
        }
    }
}

impl From<Array> for YOutput {
    fn from(v: Array) -> Self {
        YOutput {
            tag: Y_ARRAY,
            len: 1,
            value: YOutputContent { y_array: Box::into_raw(Box::new(v)) }
        }
    }
}

impl From<Map> for YOutput {
    fn from(v: Map) -> Self {
        YOutput {
            tag: Y_MAP,
            len: 1,
            value: YOutputContent { y_map: Box::into_raw(Box::new(v)) }
        }
    }
}

impl From<XmlElement> for YOutput {
    fn from(v: XmlElement) -> Self {
        YOutput {
            tag: Y_XML_ELEM,
            len: 1,
            value: YOutputContent { y_xmlelem: Box::into_raw(Box::new(v)) }
        }
    }
}

impl From<XmlText> for YOutput {
    fn from(v: XmlText) -> Self {
        YOutput {
            tag: Y_XML_TEXT,
            len: 1,
            value: YOutputContent { y_xmltext: Box::into_raw(Box::new(v)) }
        }
    }
}

#[repr(C)]
union YOutputContent {
    flag: c_char,
    num: c_float,
    integer: c_long,
    str: *mut c_char,
    buf: *mut c_uchar,
    array: *mut YOutput,
    map: *mut YMapEntry,
    y_array: *mut Array,
    y_map: *mut Map,
    y_text: *mut Text,
    y_xmlelem: *mut XmlElement,
    y_xmltext: *mut XmlText,
}

/// Releases all resources related to a corresponding `YOutput` cell.
#[no_mangle]
pub unsafe extern "C" fn youtput_destroy(val: *mut YOutput) {
    if !val.is_null() {
        drop(Box::from_raw(val))
    }
}

/// Function constructor used to create JSON-like NULL `YInput` cell.
/// This function doesn't allocate any heap resources.
#[no_mangle]
pub unsafe extern "C" fn yinput_null() -> YInput {
    YInput {
        tag: Y_JSON_NULL,
        len: 0,
        value: MaybeUninit::uninit().assume_init(),
    }
}

/// Function constructor used to create JSON-like undefined `YInput` cell.
/// This function doesn't allocate any heap resources.
#[no_mangle]
pub unsafe extern "C" fn yinput_undefined() -> YInput {
    YInput {
        tag: Y_JSON_UNDEF,
        len: 0,
        value: MaybeUninit::uninit().assume_init(),
    }
}

/// Function constructor used to create JSON-like boolean `YInput` cell.
/// This function doesn't allocate any heap resources.
#[no_mangle]
pub unsafe extern "C" fn yinput_bool(flag: c_char) -> YInput {
    YInput {
        tag: Y_JSON_BOOL,
        len: 1,
        value: YInputContent { flag },
    }
}

/// Function constructor used to create JSON-like 64-bit floating point number `YInput` cell.
/// This function doesn't allocate any heap resources.
#[no_mangle]
pub unsafe extern "C" fn yinput_float(num: c_float) -> YInput {
    YInput {
        tag: Y_JSON_NUM,
        len: 1,
        value: YInputContent { num },
    }
}

/// Function constructor used to create JSON-like 64-bit signed integer `YInput` cell.
/// This function doesn't allocate any heap resources.
#[no_mangle]
pub unsafe extern "C" fn yinput_long(integer: c_long) -> YInput {
    YInput {
        tag: Y_JSON_INT,
        len: 1,
        value: YInputContent { integer },
    }
}

/// Function constructor used to create a string `YInput` cell. Provided parameter must be
/// a null-terminated UTF-8 encoded string. This function doesn't allocate any heap resources,
/// and doesn't release any on its own, therefore its up to a caller to free resources once
/// a structure is no longer needed.
#[no_mangle]
pub unsafe extern "C" fn yinput_string(str: *const c_char) -> YInput {
    YInput {
        tag: Y_JSON_STR,
        len: 1,
        value: YInputContent { str: str as *mut c_char },
    }
}

/// Function constructor used to create a binary `YInput` cell of a specified length.
/// This function doesn't allocate any heap resources and doesn't release any on its own, therefore
/// its up to a caller to free resources once a structure is no longer needed.
#[no_mangle]
pub unsafe extern "C" fn yinput_binary(buf: *const u8, len: c_int) -> YInput {
    YInput {
        tag: Y_JSON_BUF,
        len,
        value: YInputContent { buf: buf as *mut u8 },
    }
}

/// Function constructor used to create a JSON-like array `YInput` cell of other JSON-like values of
/// a given length. This function doesn't allocate any heap resources and doesn't release any on its
/// own, therefore its up to a caller to free resources once a structure is no longer needed.
#[no_mangle]
pub unsafe extern "C" fn yinput_json_array(values: *mut YInput, len: c_int) -> YInput {
    YInput {
        tag: Y_JSON_ARR,
        len,
        value: YInputContent { values },
    }
}

/// Function constructor used to create a JSON-like map `YInput` cell of other JSON-like key-value
/// pairs. These pairs are build from corresponding indexes of `keys` and `values`, which must have
/// the same specified length.
///
/// This function doesn't allocate any heap resources and doesn't release any on its own, therefore
/// its up to a caller to free resources once a structure is no longer needed.
#[no_mangle]
pub unsafe extern "C" fn yinput_json_map(keys: *mut *mut c_char, values: *mut YInput, len: c_int) -> YInput {
    YInput {
        tag: Y_JSON_ARR,
        len,
        value: YInputContent { map: ManuallyDrop::new(YMapInputData { keys, values }) },
    }
}

/// Function constructor used to create a nested `YArray` `YInput` cell prefilled with other
/// values of a given length. This function doesn't allocate any heap resources and doesn't release
/// any on its own, therefore its up to a caller to free resources once a structure is no longer
/// needed.
#[no_mangle]
pub unsafe extern "C" fn yinput_yarray(values: *mut YInput, len: c_int) -> YInput {
    YInput {
        tag: Y_ARRAY,
        len,
        value: YInputContent { values },
    }
}

/// Function constructor used to create a nested `YMap` `YInput` cell prefilled with other key-value
/// pairs. These pairs are build from corresponding indexes of `keys` and `values`, which must have
/// the same specified length.
///
/// This function doesn't allocate any heap resources and doesn't release any on its own, therefore
/// its up to a caller to free resources once a structure is no longer needed.
#[no_mangle]
pub unsafe extern "C" fn yinput_ymap(keys: *mut *mut c_char, values: *mut YInput, len: c_int) -> YInput {
    YInput {
        tag: Y_MAP,
        len,
        value: YInputContent { map: ManuallyDrop::new(YMapInputData { keys, values }) },
    }
}

/// Function constructor used to create a nested `YText` `YInput` cell prefilled with a specified
/// string, which must be a null-terminated UTF-8 character pointer.
///
/// This function doesn't allocate any heap resources and doesn't release any on its own, therefore
/// its up to a caller to free resources once a structure is no longer needed.
#[no_mangle]
pub unsafe extern "C" fn yinput_ytext(str: *mut c_char) -> YInput {
    YInput {
        tag: Y_TEXT,
        len: 1,
        value: YInputContent { str },
    }
}

/// Function constructor used to create a nested `YXmlElement` `YInput` cell with a specified
/// tag name, which must be a null-terminated UTF-8 character pointer.
///
/// This function doesn't allocate any heap resources and doesn't release any on its own, therefore
/// its up to a caller to free resources once a structure is no longer needed.
#[no_mangle]
pub unsafe extern "C" fn yinput_yxmlelem(name: *mut c_char) -> YInput {
    YInput {
        tag: Y_XML_ELEM,
        len: 1,
        value: YInputContent { str: name },
    }
}

/// Function constructor used to create a nested `YXmlText` `YInput` cell prefilled with a specified
/// string, which must be a null-terminated UTF-8 character pointer.
///
/// This function doesn't allocate any heap resources and doesn't release any on its own, therefore
/// its up to a caller to free resources once a structure is no longer needed.
#[no_mangle]
pub unsafe extern "C" fn yinput_yxmltext(str: *mut c_char) -> YInput {
    YInput {
        tag: Y_XML_TEXT,
        len: 1,
        value: YInputContent { str },
    }
}

/// Attempts to read the value for a given `YOutput` pointer as a boolean flag, which can be either
/// `1` for truthy case and `0` otherwise. Returns a null pointer in case when a value stored under
/// current `YOutput` cell is not of a boolean type.
#[no_mangle]
pub unsafe extern "C" fn youtput_read_bool(val: *const YOutput) -> *const c_char {
    let v = val.as_ref().unwrap();
    if v.tag == Y_JSON_BOOL {
        &v.value.flag
    } else {
        std::ptr::null()
    }
}

/// Attempts to read the value for a given `YOutput` pointer as a 64-bit floating point number.
///
/// Returns a null pointer in case when a value stored under current `YOutput` cell
/// is not a floating point number.
#[no_mangle]
pub unsafe extern "C" fn youtput_read_float(val: *const YOutput) -> *const c_float {
    let v = val.as_ref().unwrap();
    if v.tag == Y_JSON_NUM {
        &v.value.num
    } else {
        std::ptr::null()
    }
}

/// Attempts to read the value for a given `YOutput` pointer as a 64-bit signed integer.
///
/// Returns a null pointer in case when a value stored under current `YOutput` cell
/// is not a signed integer.
#[no_mangle]
pub unsafe extern "C" fn youtput_read_long(val: *const YOutput) -> *const c_long {
    let v = val.as_ref().unwrap();
    if v.tag == Y_JSON_INT {
        &v.value.integer
    } else {
        std::ptr::null()
    }
}

/// Attempts to read the value for a given `YOutput` pointer as a null-terminated UTF-8 encoded
/// string.
///
/// Returns a null pointer in case when a value stored under current `YOutput` cell
/// is not a string. Underlying string is released automatically as part of [youtput_destroy]
/// destructor.
#[no_mangle]
pub unsafe extern "C" fn youtput_read_string(val: *const YOutput) -> *mut c_char {
    let v = val.as_ref().unwrap();
    if v.tag == Y_JSON_STR {
        v.value.str
    } else {
        std::ptr::null_mut()
    }
}

/// Attempts to read the value for a given `YOutput` pointer as a binary payload (which length is
/// stored within `len` filed of a cell itself).
///
/// Returns a null pointer in case when a value stored under current `YOutput` cell
/// is not a binary type. Underlying binary is released automatically as part of [youtput_destroy]
/// destructor.
#[no_mangle]
pub unsafe extern "C" fn youtput_read_binary(val: *const YOutput) -> *const c_uchar {
    let v = val.as_ref().unwrap();
    if v.tag == Y_JSON_BUF {
        v.value.buf
    } else {
        std::ptr::null()
    }
}

/// Attempts to read the value for a given `YOutput` pointer as a JSON-like array of `YOutput`
/// values (which length is stored within `len` filed of a cell itself).
///
/// Returns a null pointer in case when a value stored under current `YOutput` cell
/// is not a JSON-like array. Underlying heap resources are released automatically as part of
/// [youtput_destroy] destructor.
#[no_mangle]
pub unsafe extern "C" fn youtput_read_json_array(val: *const YOutput) -> *mut YOutput {
    let v = val.as_ref().unwrap();
    if v.tag == Y_JSON_ARR {
        v.value.array
    } else {
        std::ptr::null_mut()
    }
}

/// Attempts to read the value for a given `YOutput` pointer as a JSON-like map of key-value entries
/// (which length is stored within `len` filed of a cell itself).
///
/// Returns a null pointer in case when a value stored under current `YOutput` cell
/// is not a JSON-like map. Underlying heap resources are released automatically as part of
/// [youtput_destroy] destructor.
#[no_mangle]
pub unsafe extern "C" fn youtput_read_json_map(val: *const YOutput) -> *mut YMapEntry {
    let v = val.as_ref().unwrap();
    if v.tag == Y_JSON_MAP {
        v.value.map
    } else {
        std::ptr::null_mut()
    }
}

/// Attempts to read the value for a given `YOutput` pointer as an `YArray`.
///
/// Returns a null pointer in case when a value stored under current `YOutput` cell
/// is not an `YArray`. Underlying heap resources are released automatically as part of
/// [youtput_destroy] destructor.
#[no_mangle]
pub unsafe extern "C" fn youtput_read_yarray(val: *const YOutput) -> *mut Array {
    let v = val.as_ref().unwrap();
    if v.tag == Y_ARRAY {
        v.value.y_array
    } else {
        std::ptr::null_mut()
    }
}

/// Attempts to read the value for a given `YOutput` pointer as an `YXmlElement`.
///
/// Returns a null pointer in case when a value stored under current `YOutput` cell
/// is not an `YXmlElement`. Underlying heap resources are released automatically as part of
/// [youtput_destroy] destructor.
#[no_mangle]
pub unsafe extern "C" fn youtput_read_yxmlelem(val: *const YOutput) -> *mut XmlElement {
    let v = val.as_ref().unwrap();
    if v.tag == Y_XML_ELEM {
        v.value.y_xmlelem
    } else {
        std::ptr::null_mut()
    }
}

/// Attempts to read the value for a given `YOutput` pointer as an `YMap`.
///
/// Returns a null pointer in case when a value stored under current `YOutput` cell
/// is not an `YMap`. Underlying heap resources are released automatically as part of
/// [youtput_destroy] destructor.
#[no_mangle]
pub unsafe extern "C" fn youtput_read_ymap(val: *const YOutput) -> *mut Map {
    let v = val.as_ref().unwrap();
    if v.tag == Y_MAP {
        v.value.y_map
    } else {
        std::ptr::null_mut()
    }
}

/// Attempts to read the value for a given `YOutput` pointer as an `YText`.
///
/// Returns a null pointer in case when a value stored under current `YOutput` cell
/// is not an `YText`. Underlying heap resources are released automatically as part of
/// [youtput_destroy] destructor.
#[no_mangle]
pub unsafe extern "C" fn youtput_read_ytext(val: *const YOutput) -> *mut Text {
    let v = val.as_ref().unwrap();
    if v.tag == Y_TEXT {
        v.value.y_text
    } else {
        std::ptr::null_mut()
    }
}

/// Attempts to read the value for a given `YOutput` pointer as an `YXmlText`.
///
/// Returns a null pointer in case when a value stored under current `YOutput` cell
/// is not an `YXmlText`. Underlying heap resources are released automatically as part of
/// [youtput_destroy] destructor.
#[no_mangle]
pub unsafe extern "C" fn youtput_read_yxmltext(val: *const YOutput) -> *mut XmlText {
    let v = val.as_ref().unwrap();
    if v.tag == Y_XML_TEXT {
        v.value.y_xmltext
    } else {
        std::ptr::null_mut()
    }
}

#[cfg(test)]
mod test {
    use crate::*;

    #[test]
    fn yval_preliminary_types() {
        unsafe {
            let doc = ydoc_new();
            let txn = ytransaction_new(doc);
            let array = yarray(txn, CString::new("test").unwrap().as_ptr());

            let y_true = yinput_bool(Y_TRUE);
            let y_false = yinput_bool(Y_FALSE);
            let y_float = yinput_float(0.5);
            let y_int = yinput_long(11);
            let y_str = yinput_string(CString::new("hello").unwrap().as_ptr());

            let values = &[
                y_true,
                y_false,
                y_float,
                y_int,
                y_str
            ];

            yarray_insert_range(array, txn, 0, values.as_ptr(), 5);

            ytransaction_commit(txn);
            ydoc_destroy(doc);
        }
    }

}
