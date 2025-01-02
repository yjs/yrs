/**
 * The MIT License (MIT)
 *
 *  Copyright (c) 2020
 *    - Bartosz Sypytkowski <b.sypytkowski@gmail.com>
 *    - Kevin Jahns <kevin.jahns@pm.me>.
 *
 *  Permission is hereby granted, free of charge, to any person obtaining a copy
 *  of this software and associated documentation files (the "Software"), to deal
 *  in the Software without restriction, including without limitation the rights
 *  to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 *  copies of the Software, and to permit persons to whom the Software is
 *  furnished to do so, subject to the following conditions:
 *
 *  The above copyright notice and this permission notice shall be included in all
 *  copies or substantial portions of the Software.
 *
 *  THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 *  IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 *  FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 *  AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 *  LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 *  OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 *  SOFTWARE.
 */

#ifndef YRS_FFI_H
#define YRS_FFI_H

/**
 * A Yrs document type. Documents are most important units of collaborative resources management.
 * All shared collections live within a scope of their corresponding documents. All updates are
 * generated on per document basis (rather than individual shared type). All operations on shared
 * collections happen via `YTransaction`, which lifetime is also bound to a document.
 *
 * Document manages so called root types, which are top-level shared types definitions (as opposed
 * to recursively nested types).
 */
typedef struct YDoc {} YDoc;

/**
 * A common shared data type. All Yrs instances can be refered to using this data type (use
 * `ytype_kind` function if a specific type needs to be determined). Branch pointers are passed
 * over type-specific functions like `ytext_insert`, `yarray_insert` or `ymap_insert` to perform
 * a specific shared type operations.
 *
 * Using write methods of different shared types (eg. `ytext_insert` and `yarray_insert`) over
 * the same branch may result in undefined behavior.
 */
typedef struct Branch {} Branch;

typedef struct Transaction {} Transaction;
typedef struct TransactionMut {} TransactionMut;

/**
 * Iterator structure used by weak link unquote.
 */
typedef struct YWeakIter {} YWeakIter;

/**
 * Iterator structure used by shared array data type.
 */
typedef struct YArrayIter {} YArrayIter;

/**
 * Iterator structure used by shared map data type. Map iterators are unordered - there's no
 * specific order in which map entries will be returned during consecutive iterator calls.
 */
typedef struct YMapIter {} YMapIter;

/**
 * Iterator structure used by XML nodes (elements and text) to iterate over node's attributes.
 * Attribute iterators are unordered - there's no specific order in which map entries will be
 * returned during consecutive iterator calls.
 */
typedef struct YXmlAttrIter {} YXmlAttrIter;

/**
 * Iterator used to traverse over the complex nested tree structure of a XML node. XML node
 * iterator walks only over `YXmlElement` and `YXmlText` nodes. It does so in ordered manner (using
 * the order in which children are ordered within their parent nodes) and using **depth-first**
 * traverse.
 */
typedef struct YXmlTreeWalker {} YXmlTreeWalker;

typedef struct YUndoManager {} YUndoManager;
typedef struct LinkSource {} LinkSource;
typedef struct Unquote {} Unquote;
typedef struct StickyIndex {} StickyIndex;
typedef struct YSubscription {} YSubscription;


#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Flag used by `YInput` to pass JSON string for an object that should be deserialized and
 * stored internally as fully fledged scalar type.
 */
#define Y_JSON -9

/**
 * Flag used by `YInput` and `YOutput` to tag boolean values.
 */
#define Y_JSON_BOOL -8

/**
 * Flag used by `YInput` and `YOutput` to tag floating point numbers.
 */
#define Y_JSON_NUM -7

/**
 * Flag used by `YInput` and `YOutput` to tag 64-bit integer numbers.
 */
#define Y_JSON_INT -6

/**
 * Flag used by `YInput` and `YOutput` to tag strings.
 */
#define Y_JSON_STR -5

/**
 * Flag used by `YInput` and `YOutput` to tag binary content.
 */
#define Y_JSON_BUF -4

/**
 * Flag used by `YInput` and `YOutput` to tag embedded JSON-like arrays of values,
 * which themselves are `YInput` and `YOutput` instances respectively.
 */
#define Y_JSON_ARR -3

/**
 * Flag used by `YInput` and `YOutput` to tag embedded JSON-like maps of key-value pairs,
 * where keys are strings and v
 */
#define Y_JSON_MAP -2

/**
 * Flag used by `YInput` and `YOutput` to tag JSON-like null values.
 */
#define Y_JSON_NULL -1

/**
 * Flag used by `YInput` and `YOutput` to tag JSON-like undefined values.
 */
#define Y_JSON_UNDEF 0

/**
 * Flag used by `YInput` and `YOutput` to tag content, which is an `YArray` shared type.
 */
#define Y_ARRAY 1

/**
 * Flag used by `YInput` and `YOutput` to tag content, which is an `YMap` shared type.
 */
#define Y_MAP 2

/**
 * Flag used by `YInput` and `YOutput` to tag content, which is an `YText` shared type.
 */
#define Y_TEXT 3

/**
 * Flag used by `YInput` and `YOutput` to tag content, which is an `YXmlElement` shared type.
 */
#define Y_XML_ELEM 4

/**
 * Flag used by `YInput` and `YOutput` to tag content, which is an `YXmlText` shared type.
 */
#define Y_XML_TEXT 5

/**
 * Flag used by `YInput` and `YOutput` to tag content, which is an `YXmlFragment` shared type.
 */
#define Y_XML_FRAG 6

/**
 * Flag used by `YInput` and `YOutput` to tag content, which is an `YDoc` shared type.
 */
#define Y_DOC 7

/**
 * Flag used by `YInput` and `YOutput` to tag content, which is an `YWeakLink` shared type.
 */
#define Y_WEAK_LINK 8

/**
 * Flag used by `YOutput` to tag content, which is an undefined shared type. This usually happens
 * when it's referencing a root type that has not been initalized localy.
 */
#define Y_UNDEFINED 9

/**
 * Flag used to mark a truthy boolean numbers.
 */
#define Y_TRUE 1

/**
 * Flag used to mark a falsy boolean numbers.
 */
#define Y_FALSE 0

/**
 * Flag used by `YOptions` to determine, that text operations offsets and length will be counted by
 * the byte number of UTF8-encoded string.
 */
#define Y_OFFSET_BYTES 0

/**
 * Flag used by `YOptions` to determine, that text operations offsets and length will be counted by
 * UTF-16 chars of encoded string.
 */
#define Y_OFFSET_UTF16 1

/**
 * Error code: couldn't read data from input stream.
 */
#define ERR_CODE_IO 1

/**
 * Error code: decoded variable integer outside of the expected integer size bounds.
 */
#define ERR_CODE_VAR_INT 2

/**
 * Error code: end of stream found when more data was expected.
 */
#define ERR_CODE_EOS 3

/**
 * Error code: decoded enum tag value was not among known cases.
 */
#define ERR_CODE_UNEXPECTED_VALUE 4

/**
 * Error code: failure when trying to decode JSON content.
 */
#define ERR_CODE_INVALID_JSON 5

/**
 * Error code: other error type than the one specified.
 */
#define ERR_CODE_OTHER 6

/**
 * Error code: not enough memory to perform an operation.
 */
#define ERR_NOT_ENOUGH_MEMORY 7

/**
 * Error code: conversion attempt to specific Rust type was not possible.
 */
#define ERR_TYPE_MISMATCH 8

/**
 * Error code: miscellaneous error coming from serde, not covered by other error codes.
 */
#define ERR_CUSTOM 9

/**
 * Error code: update block assigned to parent that is not a valid shared ref of deleted block.
 */
#define ERR_INVALID_PARENT 9

#define YCHANGE_ADD 1

#define YCHANGE_RETAIN 0

#define YCHANGE_REMOVE -1

#define Y_KIND_UNDO 0

#define Y_KIND_REDO 1

/**
 * Tag used to identify `YPathSegment` storing a *char parameter.
 */
#define Y_EVENT_PATH_KEY 1

/**
 * Tag used to identify `YPathSegment` storing an int parameter.
 */
#define Y_EVENT_PATH_INDEX 2

/**
 * Tag used to identify `YEventChange` (see: `yevent_delta` function) case, when a new element
 * has been added to an observed collection.
 */
#define Y_EVENT_CHANGE_ADD 1

/**
 * Tag used to identify `YEventChange` (see: `yevent_delta` function) case, when an existing
 * element has been removed from an observed collection.
 */
#define Y_EVENT_CHANGE_DELETE 2

/**
 * Tag used to identify `YEventChange` (see: `yevent_delta` function) case, when no changes have
 * been detected for a particular range of observed collection.
 */
#define Y_EVENT_CHANGE_RETAIN 3

/**
 * Tag used to identify `YEventKeyChange` (see: `yevent_keys` function) case, when a new entry has
 * been inserted into a map component of shared collection.
 */
#define Y_EVENT_KEY_CHANGE_ADD 4

/**
 * Tag used to identify `YEventKeyChange` (see: `yevent_keys` function) case, when an existing
 * entry has been removed from a map component of shared collection.
 */
#define Y_EVENT_KEY_CHANGE_DELETE 5

/**
 * Tag used to identify `YEventKeyChange` (see: `yevent_keys` function) case, when an existing
 * entry has been overridden with a new value within a map component of shared collection.
 */
#define Y_EVENT_KEY_CHANGE_UPDATE 6

typedef struct TransactionInner TransactionInner;

/**
 * Configuration object used by `YDoc`.
 */
typedef struct YOptions {
  /**
   * Globally unique 53-bit integer assigned to corresponding document replica as its identifier.
   *
   * If two clients share the same `id` and will perform any updates, it will result in
   * unrecoverable document state corruption. The same thing may happen if the client restored
   * document state from snapshot, that didn't contain all of that clients updates that were sent
   * to other peers.
   */
  uint64_t id;
  /**
   * A NULL-able globally unique Uuid v4 compatible null-terminated string identifier
   * of this document. If passed as NULL, a random Uuid will be generated instead.
   */
  const char *guid;
  /**
   * A NULL-able, UTF-8 encoded, null-terminated string of a collection that this document
   * belongs to. It's used only by providers.
   */
  const char *collection_id;
  /**
   * Encoding used by text editing operations on this document. It's used to compute
   * `YText`/`YXmlText` insertion offsets and text lengths. Either:
   *
   * - `Y_OFFSET_BYTES`
   * - `Y_OFFSET_UTF16`
   */
  uint8_t encoding;
  /**
   * Boolean flag used to determine if deleted blocks should be garbage collected or not
   * during the transaction commits. Setting this value to 0 means GC will be performed.
   */
  uint8_t skip_gc;
  /**
   * Boolean flag used to determine if subdocument should be loaded automatically.
   * If this is a subdocument, remote peers will load the document as well automatically.
   */
  uint8_t auto_load;
  /**
   * Boolean flag used to determine whether the document should be synced by the provider now.
   */
  uint8_t should_load;
} YOptions;

/**
 * A Yrs document type. Documents are the most important units of collaborative resources management.
 * All shared collections live within a scope of their corresponding documents. All updates are
 * generated on per-document basis (rather than individual shared type). All operations on shared
 * collections happen via `YTransaction`, which lifetime is also bound to a document.
 *
 * Document manages so-called root types, which are top-level shared types definitions (as opposed
 * to recursively nested types).
 */
typedef YDoc YDoc;

/**
 * A common shared data type. All Yrs instances can be refered to using this data type (use
 * `ytype_kind` function if a specific type needs to be determined). Branch pointers are passed
 * over type-specific functions like `ytext_insert`, `yarray_insert` or `ymap_insert` to perform
 * a specific shared type operations.
 *
 * Using write methods of different shared types (eg. `ytext_insert` and `yarray_insert`) over
 * the same branch may result in undefined behavior.
 */
typedef Branch Branch;

typedef union YOutputContent {
  uint8_t flag;
  double num;
  int64_t integer;
  char *str;
  const char *buf;
  struct YOutput *array;
  struct YMapEntry *map;
  Branch *y_type;
  YDoc *y_doc;
} YOutputContent;

/**
 * An output value cell returned from yrs API methods. It describes a various types of data
 * supported by yrs shared data types.
 *
 * Since `YOutput` instances are always created by calling the corresponding yrs API functions,
 * they eventually should be deallocated using [youtput_destroy] function.
 */
typedef struct YOutput {
  /**
   * Tag describing, which `value` type is being stored by this input cell. Can be one of:
   *
   * - [Y_JSON_BOOL] for boolean flags.
   * - [Y_JSON_NUM] for 64-bit floating point numbers.
   * - [Y_JSON_INT] for 64-bit signed integers.
   * - [Y_JSON_STR] for null-terminated UTF-8 encoded strings.
   * - [Y_JSON_BUF] for embedded binary data.
   * - [Y_JSON_ARR] for arrays of JSON-like values.
   * - [Y_JSON_MAP] for JSON-like objects build from key-value pairs.
   * - [Y_JSON_NULL] for JSON-like null values.
   * - [Y_JSON_UNDEF] for JSON-like undefined values.
   * - [Y_TEXT] for pointers to `YText` data types.
   * - [Y_ARRAY] for pointers to `YArray` data types.
   * - [Y_MAP] for pointers to `YMap` data types.
   * - [Y_XML_ELEM] for pointers to `YXmlElement` data types.
   * - [Y_XML_TEXT] for pointers to `YXmlText` data types.
   * - [Y_DOC] for pointers to nested `YDocRef` data types.
   */
  int8_t tag;
  /**
   * Length of the contents stored by a current `YOutput` cell.
   *
   * For [Y_JSON_NULL] and [Y_JSON_UNDEF] its equal to `0`.
   *
   * For [Y_JSON_ARR], [Y_JSON_MAP] it describes a number of passed elements.
   *
   * For other types it's always equal to `1`.
   */
  uint32_t len;
  /**
   * Union struct which contains a content corresponding to a provided `tag` field.
   */
  union YOutputContent value;
} YOutput;

/**
 * A structure representing single key-value entry of a map output (used by either
 * embedded JSON-like maps or YMaps).
 */
typedef struct YMapEntry {
  /**
   * Null-terminated string representing an entry's key component. Encoded as UTF-8.
   */
  const char *key;
  /**
   * A `YOutput` value representing containing variadic content that can be stored withing map's
   * entry.
   */
  const struct YOutput *value;
} YMapEntry;

/**
 * A structure representing single attribute of an either `YXmlElement` or `YXmlText` instance.
 * It consists of attribute name and string, both of which are null-terminated UTF-8 strings.
 */
typedef struct YXmlAttr {
  const char *name;
  const char *value;
} YXmlAttr;

/**
 * Subscription to any kind of observable events, like `ymap_observe`, `ydoc_observe_updates_v1` etc.
 * This subscription can be destroyed by calling `yunobserve` function, which will cause to unsubscribe
 * correlated callback.
 */
typedef YSubscription YSubscription;

/**
 * Struct representing a state of a document. It contains the last seen clocks for blocks submitted
 * per any of the clients collaborating on document updates.
 */
typedef struct YStateVector {
  /**
   * Number of clients. It describes a length of both `client_ids` and `clocks` arrays.
   */
  uint32_t entries_count;
  /**
   * Array of unique client identifiers (length is given in `entries_count` field). Each client
   * ID has corresponding clock attached, which can be found in `clocks` field under the same
   * index.
   */
  uint64_t *client_ids;
  /**
   * Array of clocks (length is given in `entries_count` field) known for each client. Each clock
   * has a corresponding client identifier attached, which can be found in `client_ids` field
   * under the same index.
   */
  uint32_t *clocks;
} YStateVector;

typedef struct YIdRange {
  uint32_t start;
  uint32_t end;
} YIdRange;

/**
 * Fixed-length sequence of ID ranges. Each range is a pair of [start, end) values, describing the
 * range of items identified by clock values, that this range refers to.
 */
typedef struct YIdRangeSeq {
  /**
   * Number of ranges stored in this sequence.
   */
  uint32_t len;
  /**
   * Array (length is stored in `len` field) or ranges. Each range is a pair of [start, end)
   * values, describing continuous collection of items produced by the same client, identified
   * by clock values, that this range refers to.
   */
  struct YIdRange *seq;
} YIdRangeSeq;

/**
 * Delete set is a map of `(ClientID, Range[])` entries. Length of a map is stored in
 * `entries_count` field. ClientIDs reside under `client_ids` and their corresponding range
 * sequences can be found under the same index of `ranges` field.
 */
typedef struct YDeleteSet {
  /**
   * Number of client identifier entries.
   */
  uint32_t entries_count;
  /**
   * Array of unique client identifiers (length is given in `entries_count` field). Each client
   * ID has corresponding sequence of ranges attached, which can be found in `ranges` field under
   * the same index.
   */
  uint64_t *client_ids;
  /**
   * Array of range sequences (length is given in `entries_count` field). Each sequence has
   * a corresponding client ID attached, which can be found in `client_ids` field under
   * the same index.
   */
  struct YIdRangeSeq *ranges;
} YDeleteSet;

/**
 * Event generated for callbacks subscribed using `ydoc_observe_after_transaction`. It contains
 * snapshot of changes made within any committed transaction.
 */
typedef struct YAfterTransactionEvent {
  /**
   * Descriptor of a document state at the moment of creating the transaction.
   */
  struct YStateVector before_state;
  /**
   * Descriptor of a document state at the moment of committing the transaction.
   */
  struct YStateVector after_state;
  /**
   * Information about all items deleted within the scope of a transaction.
   */
  struct YDeleteSet delete_set;
} YAfterTransactionEvent;

typedef struct YSubdocsEvent {
  uint32_t added_len;
  uint32_t removed_len;
  uint32_t loaded_len;
  YDoc **added;
  YDoc **removed;
  YDoc **loaded;
} YSubdocsEvent;

/**
 * Transaction is one of the core types in Yrs. All operations that need to touch or
 * modify a document's contents (a.k.a. block store), need to be executed in scope of a
 * transaction.
 */
typedef struct TransactionInner YTransaction;

/**
 * Structure containing unapplied update data.
 * Created via `ytransaction_pending_update`.
 * Released via `ypending_update_destroy`.
 */
typedef struct YPendingUpdate {
  /**
   * A state vector that informs about minimal client clock values that need to be satisfied
   * in order to successfully apply current update.
   */
  struct YStateVector missing;
  /**
   * Update data stored in lib0 v1 format.
   */
  char *update_v1;
  /**
   * Length of `update_v1` payload.
   */
  uint32_t update_len;
} YPendingUpdate;

typedef struct YMapInputData {
  char **keys;
  struct YInput *values;
} YMapInputData;

typedef LinkSource Weak;

typedef union YInputContent {
  uint8_t flag;
  double num;
  int64_t integer;
  char *str;
  char *buf;
  struct YInput *values;
  struct YMapInputData map;
  YDoc *doc;
  const Weak *weak;
} YInputContent;

/**
 * A data structure that is used to pass input values of various types supported by Yrs into a
 * shared document store.
 *
 * `YInput` constructor function don't allocate any resources on their own, neither they take
 * ownership by pointers to memory blocks allocated by user - for this reason once an input cell
 * has been used, its content should be freed by the caller.
 */
typedef struct YInput {
  /**
   * Tag describing, which `value` type is being stored by this input cell. Can be one of:
   *
   * - [Y_JSON] for a UTF-8 encoded, NULL-terminated JSON string.
   * - [Y_JSON_BOOL] for boolean flags.
   * - [Y_JSON_NUM] for 64-bit floating point numbers.
   * - [Y_JSON_INT] for 64-bit signed integers.
   * - [Y_JSON_STR] for null-terminated UTF-8 encoded strings.
   * - [Y_JSON_BUF] for embedded binary data.
   * - [Y_JSON_ARR] for arrays of JSON-like values.
   * - [Y_JSON_MAP] for JSON-like objects build from key-value pairs.
   * - [Y_JSON_NULL] for JSON-like null values.
   * - [Y_JSON_UNDEF] for JSON-like undefined values.
   * - [Y_ARRAY] for cells which contents should be used to initialize a `YArray` shared type.
   * - [Y_MAP] for cells which contents should be used to initialize a `YMap` shared type.
   * - [Y_DOC] for cells which contents should be used to nest a `YDoc` sub-document.
   * - [Y_WEAK_LINK] for cells which contents should be used to nest a `YWeakLink` sub-document.
   */
  int8_t tag;
  /**
   * Length of the contents stored by current `YInput` cell.
   *
   * For [Y_JSON_NULL] and [Y_JSON_UNDEF] its equal to `0`.
   *
   * For [Y_JSON_ARR], [Y_JSON_MAP], [Y_ARRAY] and [Y_MAP] it describes a number of passed
   * elements.
   *
   * For other types it's always equal to `1`.
   */
  uint32_t len;
  /**
   * Union struct which contains a content corresponding to a provided `tag` field.
   */
  union YInputContent value;
} YInput;

/**
 * A data type representing a single change to be performed in sequence of changes defined
 * as parameter to a `ytext_insert_delta` function. A type of change can be detected using
 * a `tag` field:
 *
 * 1. `Y_EVENT_CHANGE_ADD` marks a new characters added to a collection. In this case `insert`
 * field contains a pointer to a list of newly inserted values, while `len` field informs about
 * their count. Additionally `attributes_len` nad `attributes` carry information about optional
 * formatting attributes applied to edited blocks.
 * 2. `Y_EVENT_CHANGE_DELETE` marks an existing elements removed from the collection. In this case
 * `len` field informs about number of removed elements.
 * 3. `Y_EVENT_CHANGE_RETAIN` marks a number of characters that have not been changed, counted from
 * the previous element. `len` field informs about number of retained elements. Additionally
 * `attributes_len` nad `attributes` carry information about optional formatting attributes applied
 * to edited blocks.
 */
typedef struct YDeltaIn {
  /**
   * Tag field used to identify particular type of change made:
   *
   * 1. `Y_EVENT_CHANGE_ADD` marks a new elements added to a collection. In this case `values`
   * field contains a pointer to a list of newly inserted values, while `len` field informs about
   * their count.
   * 2. `Y_EVENT_CHANGE_DELETE` marks an existing elements removed from the collection. In this
   * case `len` field informs about number of removed elements.
   * 3. `Y_EVENT_CHANGE_RETAIN` marks a number of elements that have not been changed, counted
   * from the previous element. `len` field informs about number of retained elements.
   */
  uint8_t tag;
  /**
   * Number of element affected by current type of change. It can refer to a number of
   * inserted `values`, number of deleted element or a number of retained (unchanged) values.
   */
  uint32_t len;
  /**
   * A nullable pointer to a list of formatting attributes assigned to an edited area represented
   * by this delta.
   */
  const struct YInput *attributes;
  /**
   * Used in case when current change is of `Y_EVENT_CHANGE_ADD` type. Contains a list (of
   * length stored in `len` field) of newly inserted values.
   */
  const struct YInput *insert;
} YDeltaIn;

/**
 * A chunk of text contents formatted with the same set of attributes.
 */
typedef struct YChunk {
  /**
   * Piece of YText formatted using the same `fmt` rules. It can be a string, embedded object
   * or another y-type.
   */
  struct YOutput data;
  /**
   * Number of formatting attributes attached to current chunk of text.
   */
  uint32_t fmt_len;
  /**
   * The formatting attributes attached to the current chunk of text.
   */
  struct YMapEntry *fmt;
} YChunk;

/**
 * Event pushed into callbacks registered with `ytext_observe` function. It contains delta of all
 * text changes made within a scope of corresponding transaction (see: `ytext_event_delta`) as
 * well as navigation data used to identify a `YText` instance which triggered this event.
 */
typedef struct YTextEvent {
  const void *inner;
  const TransactionMut *txn;
} YTextEvent;

/**
 * Event pushed into callbacks registered with `ymap_observe` function. It contains all
 * key-value changes made within a scope of corresponding transaction (see: `ymap_event_keys`) as
 * well as navigation data used to identify a `YMap` instance which triggered this event.
 */
typedef struct YMapEvent {
  const void *inner;
  const TransactionMut *txn;
} YMapEvent;

/**
 * Event pushed into callbacks registered with `yarray_observe` function. It contains delta of all
 * content changes made within a scope of corresponding transaction (see: `yarray_event_delta`) as
 * well as navigation data used to identify a `YArray` instance which triggered this event.
 */
typedef struct YArrayEvent {
  const void *inner;
  const TransactionMut *txn;
} YArrayEvent;

/**
 * Event pushed into callbacks registered with `yxmlelem_observe` function. It contains
 * all attribute changes made within a scope of corresponding transaction
 * (see: `yxmlelem_event_keys`) as well as child XML nodes changes (see: `yxmlelem_event_delta`)
 * and navigation data used to identify a `YXmlElement` instance which triggered this event.
 */
typedef struct YXmlEvent {
  const void *inner;
  const TransactionMut *txn;
} YXmlEvent;

/**
 * Event pushed into callbacks registered with `yxmltext_observe` function. It contains
 * all attribute changes made within a scope of corresponding transaction
 * (see: `yxmltext_event_keys`) as well as text edits (see: `yxmltext_event_delta`)
 * and navigation data used to identify a `YXmlText` instance which triggered this event.
 */
typedef struct YXmlTextEvent {
  const void *inner;
  const TransactionMut *txn;
} YXmlTextEvent;

/**
 * Event pushed into callbacks registered with `yweak_observe` function. It contains
 * all an event changes of the underlying transaction.
 */
typedef struct YWeakLinkEvent {
  const void *inner;
  const TransactionMut *txn;
} YWeakLinkEvent;

typedef union YEventContent {
  struct YTextEvent text;
  struct YMapEvent map;
  struct YArrayEvent array;
  struct YXmlEvent xml_elem;
  struct YXmlTextEvent xml_text;
  struct YWeakLinkEvent weak;
} YEventContent;

typedef struct YEvent {
  /**
   * Tag describing, which shared type emitted this event.
   *
   * - [Y_TEXT] for pointers to `YText` data types.
   * - [Y_ARRAY] for pointers to `YArray` data types.
   * - [Y_MAP] for pointers to `YMap` data types.
   * - [Y_XML_ELEM] for pointers to `YXmlElement` data types.
   * - [Y_XML_TEXT] for pointers to `YXmlText` data types.
   */
  int8_t tag;
  /**
   * A nested event type, specific for a shared data type that triggered it. Type of an
   * event can be verified using `tag` field.
   */
  union YEventContent content;
} YEvent;

typedef union YPathSegmentCase {
  const char *key;
  uint32_t index;
} YPathSegmentCase;

/**
 * A single segment of a path returned from `yevent_path` function. It can be one of two cases,
 * recognized by it's `tag` field:
 *
 * 1. `Y_EVENT_PATH_KEY` means that segment value can be accessed by `segment.value.key` and is
 * referring to a string key used by map component (eg. `YMap` entry).
 * 2. `Y_EVENT_PATH_INDEX` means that segment value can be accessed by `segment.value.index` and is
 * referring to an int index used by sequence component (eg. `YArray` item or `YXmlElement` child).
 */
typedef struct YPathSegment {
  /**
   * Tag used to identify which case current segment is referring to:
   *
   * 1. `Y_EVENT_PATH_KEY` means that segment value can be accessed by `segment.value.key` and is
   * referring to a string key used by map component (eg. `YMap` entry).
   * 2. `Y_EVENT_PATH_INDEX` means that segment value can be accessed by `segment.value.index`
   * and is referring to an int index used by sequence component (eg. `YArray` item or
   * `YXmlElement` child).
   */
  char tag;
  /**
   * Union field containing either `key` or `index`. A particular case can be recognized by using
   * segment's `tag` field.
   */
  union YPathSegmentCase value;
} YPathSegment;

/**
 * A single instance of formatting attribute stored as part of `YDelta` instance.
 */
typedef struct YDeltaAttr {
  /**
   * A null-terminated UTF-8 encoded string containing a unique formatting attribute name.
   */
  const char *key;
  /**
   * A value assigned to a formatting attribute.
   */
  struct YOutput value;
} YDeltaAttr;

/**
 * A data type representing a single change detected over an observed `YText`/`YXmlText`. A type
 * of change can be detected using a `tag` field:
 *
 * 1. `Y_EVENT_CHANGE_ADD` marks a new characters added to a collection. In this case `insert`
 * field contains a pointer to a list of newly inserted values, while `len` field informs about
 * their count. Additionally `attributes_len` nad `attributes` carry information about optional
 * formatting attributes applied to edited blocks.
 * 2. `Y_EVENT_CHANGE_DELETE` marks an existing elements removed from the collection. In this case
 * `len` field informs about number of removed elements.
 * 3. `Y_EVENT_CHANGE_RETAIN` marks a number of characters that have not been changed, counted from
 * the previous element. `len` field informs about number of retained elements. Additionally
 * `attributes_len` nad `attributes` carry information about optional formatting attributes applied
 * to edited blocks.
 *
 * A list of changes returned by `ytext_event_delta`/`yxmltext_event_delta` enables to locate
 * a position of all changes within an observed collection by using a combination of added/deleted
 * change structs separated by retained changes (marking eg. number of elements that can be safely
 * skipped, since they remained unchanged).
 */
typedef struct YDeltaOut {
  /**
   * Tag field used to identify particular type of change made:
   *
   * 1. `Y_EVENT_CHANGE_ADD` marks a new elements added to a collection. In this case `values`
   * field contains a pointer to a list of newly inserted values, while `len` field informs about
   * their count.
   * 2. `Y_EVENT_CHANGE_DELETE` marks an existing elements removed from the collection. In this
   * case `len` field informs about number of removed elements.
   * 3. `Y_EVENT_CHANGE_RETAIN` marks a number of elements that have not been changed, counted
   * from the previous element. `len` field informs about number of retained elements.
   */
  uint8_t tag;
  /**
   * Number of element affected by current type of change. It can refer to a number of
   * inserted `values`, number of deleted element or a number of retained (unchanged) values.
   */
  uint32_t len;
  /**
   * A number of formatting attributes assigned to an edited area represented by this delta.
   */
  uint32_t attributes_len;
  /**
   * A nullable pointer to a list of formatting attributes assigned to an edited area represented
   * by this delta.
   */
  struct YDeltaAttr *attributes;
  /**
   * Used in case when current change is of `Y_EVENT_CHANGE_ADD` type. Contains a list (of
   * length stored in `len` field) of newly inserted values.
   */
  struct YOutput *insert;
} YDeltaOut;

/**
 * A data type representing a single change detected over an observed shared collection. A type
 * of change can be detected using a `tag` field:
 *
 * 1. `Y_EVENT_CHANGE_ADD` marks a new elements added to a collection. In this case `values` field
 * contains a pointer to a list of newly inserted values, while `len` field informs about their
 * count.
 * 2. `Y_EVENT_CHANGE_DELETE` marks an existing elements removed from the collection. In this case
 * `len` field informs about number of removed elements.
 * 3. `Y_EVENT_CHANGE_RETAIN` marks a number of elements that have not been changed, counted from
 * the previous element. `len` field informs about number of retained elements.
 *
 * A list of changes returned by `yarray_event_delta`/`yxml_event_delta` enables to locate a
 * position of all changes within an observed collection by using a combination of added/deleted
 * change structs separated by retained changes (marking eg. number of elements that can be safely
 * skipped, since they remained unchanged).
 */
typedef struct YEventChange {
  /**
   * Tag field used to identify particular type of change made:
   *
   * 1. `Y_EVENT_CHANGE_ADD` marks a new elements added to a collection. In this case `values`
   * field contains a pointer to a list of newly inserted values, while `len` field informs about
   * their count.
   * 2. `Y_EVENT_CHANGE_DELETE` marks an existing elements removed from the collection. In this
   * case `len` field informs about number of removed elements.
   * 3. `Y_EVENT_CHANGE_RETAIN` marks a number of elements that have not been changed, counted
   * from the previous element. `len` field informs about number of retained elements.
   */
  uint8_t tag;
  /**
   * Number of element affected by current type of a change. It can refer to a number of
   * inserted `values`, number of deleted element or a number of retained (unchanged) values.
   */
  uint32_t len;
  /**
   * Used in case when current change is of `Y_EVENT_CHANGE_ADD` type. Contains a list (of
   * length stored in `len` field) of newly inserted values.
   */
  const struct YOutput *values;
} YEventChange;

/**
 * A data type representing a single change made over a map component of shared collection types,
 * such as `YMap` entries or `YXmlText`/`YXmlElement` attributes. A `key` field provides a
 * corresponding unique key string of a changed entry, while `tag` field informs about specific
 * type of change being done:
 *
 * 1. `Y_EVENT_KEY_CHANGE_ADD` used to identify a newly added entry. In this case an `old_value`
 * field is NULL, while `new_value` field contains an inserted value.
 * 1. `Y_EVENT_KEY_CHANGE_DELETE` used to identify an existing entry being removed. In this case
 * an `old_value` field contains the removed value.
 * 1. `Y_EVENT_KEY_CHANGE_UPDATE` used to identify an existing entry, which value has been changed.
 * In this case `old_value` field contains replaced value, while `new_value` contains a newly
 * inserted one.
 */
typedef struct YEventKeyChange {
  /**
   * A UTF8-encoded null-terminated string containing a key of a changed entry.
   */
  const char *key;
  /**
   * Tag field informing about type of change current struct refers to:
   *
   * 1. `Y_EVENT_KEY_CHANGE_ADD` used to identify a newly added entry. In this case an
   * `old_value` field is NULL, while `new_value` field contains an inserted value.
   * 1. `Y_EVENT_KEY_CHANGE_DELETE` used to identify an existing entry being removed. In this
   * case an `old_value` field contains the removed value.
   * 1. `Y_EVENT_KEY_CHANGE_UPDATE` used to identify an existing entry, which value has been
   * changed. In this case `old_value` field contains replaced value, while `new_value` contains
   * a newly inserted one.
   */
  char tag;
  /**
   * Contains a removed entry's value or replaced value of an updated entry.
   */
  const struct YOutput *old_value;
  /**
   * Contains a value of newly inserted entry or an updated entry's new value.
   */
  const struct YOutput *new_value;
} YEventKeyChange;

typedef struct YUndoManagerOptions {
  int32_t capture_timeout_millis;
} YUndoManagerOptions;

/**
 * Event type related to `UndoManager` observer operations, such as `yundo_manager_observe_popped`
 * and `yundo_manager_observe_added`. It contains various informations about the context in which
 * undo/redo operations are executed.
 */
typedef struct YUndoEvent {
  /**
   * Informs if current event is related to executed undo (`Y_KIND_UNDO`) or redo (`Y_KIND_REDO`)
   * operation.
   */
  char kind;
  /**
   * Origin assigned to a transaction, in context of which this event is being executed.
   * Transaction origin is specified via `ydoc_write_transaction(doc, origin_len, origin)`.
   */
  const char *origin;
  /**
   * Length of an `origin` field assigned to a transaction, in context of which this event is
   * being executed.
   * Transaction origin is specified via `ydoc_write_transaction(doc, origin_len, origin)`.
   */
  uint32_t origin_len;
  /**
   * Pointer to a custom metadata object that can be passed between
   * `yundo_manager_observe_popped` and `yundo_manager_observe_added`. It's useful for passing
   * around custom user data ie. cursor position, that needs to be remembered and restored as
   * part of undo/redo operations.
   *
   * This field always starts with no value (`NULL`) assigned to it and can be set/unset in
   * corresponding callback calls. In such cases it's up to a programmer to handle allocation
   * and deallocation of memory that this pointer will point to. Not releasing it properly may
   * lead to memory leaks.
   */
  void *meta;
} YUndoEvent;

/**
 * A sticky index is based on the Yjs model and is not affected by document changes.
 * E.g. If you place a sticky index before a certain character, it will always point to this character.
 * If you place a sticky index at the end of a type, it will always point to the end of the type.
 *
 * A numeric position is often unsuited for user selections, because it does not change when content is inserted
 * before or after.
 *
 * ```Insert(0, 'x')('a.bc') = 'xa.bc'``` Where `.` is the sticky index position.
 *
 * Instances of `YStickyIndex` can be freed using `ysticky_index_destroy`.
 */
typedef StickyIndex YStickyIndex;

typedef union YBranchIdVariant {
  /**
   * Clock number timestamp when the creator of a nested shared type created it.
   */
  uint32_t clock;
  /**
   * Pointer to UTF-8 encoded string representing root-level type name. This pointer is valid
   * as long as document - in which scope it was created in - was not destroyed. As usually
   * root-level type names are statically allocated strings, it can also be supplied manually
   * from the outside.
   */
  const uint8_t *name;
} YBranchIdVariant;

/**
 * A structure representing logical identifier of a specific shared collection.
 * Can be obtained by `ybranch_id` executed over alive `Branch`.
 *
 * Use `ybranch_get` to resolve a `Branch` pointer from this branch ID.
 *
 * This structure doesn't need to be destroyed. It's internal pointer reference is valid through
 * a lifetime of a document, which collection this branch ID has been created from.
 */
typedef struct YBranchId {
  /**
   * If positive: Client ID of a creator of a nested shared type, this identifier points to.
   * If negative: a negated Length of a root-level shared collection name.
   */
  int64_t client_or_len;
  union YBranchIdVariant variant;
} YBranchId;

/**
 * Returns default ceonfiguration for `YOptions`.
 */
struct YOptions yoptions(void);

/**
 * Releases all memory-allocated resources bound to given document.
 */
void ydoc_destroy(YDoc *value);

/**
 * Frees all memory-allocated resources bound to a given [YMapEntry].
 */
void ymap_entry_destroy(struct YMapEntry *value);

/**
 * Frees all memory-allocated resources bound to a given [YXmlAttr].
 */
void yxmlattr_destroy(struct YXmlAttr *attr);

/**
 * Frees all memory-allocated resources bound to a given UTF-8 null-terminated string returned from
 * Yrs document API. Yrs strings don't use libc malloc, so calling `free()` on them will fault.
 */
void ystring_destroy(char *str);

/**
 * Frees all memory-allocated resources bound to a given binary returned from Yrs document API.
 * Unlike strings binaries are not null-terminated and can contain null characters inside,
 * therefore a size of memory to be released must be explicitly provided.
 * Yrs binaries don't use libc malloc, so calling `free()` on them will fault.
 */
void ybinary_destroy(char *ptr, uint32_t len);

/**
 * Creates a new [Doc] instance with a randomized unique client identifier.
 *
 * Use [ydoc_destroy] in order to release created [Doc] resources.
 */
YDoc *ydoc_new(void);

/**
 * Creates a shallow clone of a provided `doc` - it's realized by increasing the ref-count
 * value of the document. In result both input and output documents point to the same instance.
 *
 * Documents created this way can be destroyed via [ydoc_destroy] - keep in mind, that the memory
 * will still be persisted until all strong references are dropped.
 */
YDoc *ydoc_clone(YDoc *doc);

/**
 * Creates a new [Doc] instance with a specified `options`.
 *
 * Use [ydoc_destroy] in order to release created [Doc] resources.
 */
YDoc *ydoc_new_with_options(struct YOptions options);

/**
 * Returns a unique client identifier of this [Doc] instance.
 */
uint64_t ydoc_id(YDoc *doc);

/**
 * Returns a unique document identifier of this [Doc] instance.
 *
 * Generated string resources should be released using [ystring_destroy] function.
 */
char *ydoc_guid(YDoc *doc);

/**
 * Returns a collection identifier of this [Doc] instance.
 * If none was defined, a `NULL` will be returned.
 *
 * Generated string resources should be released using [ystring_destroy] function.
 */
char *ydoc_collection_id(YDoc *doc);

/**
 * Returns status of should_load flag of this [Doc] instance, informing parent [Doc] if this
 * document instance requested a data load.
 */
uint8_t ydoc_should_load(YDoc *doc);

/**
 * Returns status of auto_load flag of this [Doc] instance. Auto loaded sub-documents automatically
 * send a load request to their parent documents.
 */
uint8_t ydoc_auto_load(YDoc *doc);

YSubscription *ydoc_observe_updates_v1(YDoc *doc, void *state, void (*cb)(void*,
                                                                          uint32_t,
                                                                          const char*,
                                                                          uint32_t,
                                                                          const char*));

YSubscription *ydoc_observe_updates_v2(YDoc *doc, void *state, void (*cb)(void*,
                                                                          uint32_t,
                                                                          const char*,
                                                                          uint32_t,
                                                                          const char*));

YSubscription *ydoc_observe_after_transaction(YDoc *doc,
                                              void *state,
                                              void (*cb)(void*, struct YAfterTransactionEvent*));

YSubscription *ydoc_observe_subdocs(YDoc *doc,
                                    void *state,
                                    void (*cb)(void*, struct YSubdocsEvent*));

YSubscription *ydoc_observe_clear(YDoc *doc, void *state, void (*cb)(void*, YDoc*));

/**
 * Manually send a load request to a parent document of this subdoc.
 */
void ydoc_load(YDoc *doc, YTransaction *parent_txn);

/**
 * Destroys current document, sending a 'destroy' event and clearing up all the event callbacks
 * registered.
 */
void ydoc_clear(YDoc *doc, YTransaction *parent_txn);

/**
 * Starts a new read-only transaction on a given document. All other operations happen in context
 * of a transaction. Yrs transactions do not follow ACID rules. Once a set of operations is
 * complete, a transaction can be finished using `ytransaction_commit` function.
 *
 * Returns `NULL` if read-only transaction couldn't be created, i.e. when another read-write
 * transaction is already opened.
 */
YTransaction *ydoc_read_transaction(YDoc *doc);

/**
 * Starts a new read-write transaction on a given document. All other operations happen in context
 * of a transaction. Yrs transactions do not follow ACID rules. Once a set of operations is
 * complete, a transaction can be finished using `ytransaction_commit` function.
 *
 * `origin_len` and `origin` are optional parameters to specify a byte sequence used to mark
 * the origin of this transaction (eg. you may decide to give different origins for transaction
 * applying remote updates). These can be used by event handlers or `YUndoManager` to perform
 * specific actions. If origin should not be set, call `ydoc_write_transaction(doc, 0, NULL)`.
 *
 * Returns `NULL` if read-write transaction couldn't be created, i.e. when another transaction is
 * already opened.
 */
YTransaction *ydoc_write_transaction(YDoc *doc, uint32_t origin_len, const char *origin);

/**
 * Returns a list of subdocs existing within current document.
 */
YDoc **ytransaction_subdocs(YTransaction *txn, uint32_t *len);

/**
 * Commit and dispose provided read-write transaction. This operation releases allocated resources,
 * triggers update events and performs a storage compression over all operations executed in scope
 * of a current transaction.
 */
void ytransaction_commit(YTransaction *txn);

/**
 * Perform garbage collection of deleted blocks, even if a document was created with `skip_gc`
 * option. This operation will scan over ALL deleted elements, NOT ONLY the ones that have been
 * changed as part of this transaction scope.
 */
void ytransaction_force_gc(YTransaction *txn);

/**
 * Returns `1` if current transaction is of read-write type.
 * Returns `0` if transaction is read-only.
 */
uint8_t ytransaction_writeable(YTransaction *txn);

/**
 * Gets a reference to shared data type instance at the document root-level,
 * identified by its `name`, which must be a null-terminated UTF-8 compatible string.
 *
 * Returns `NULL` if no such structure was defined in the document before.
 */
Branch *ytype_get(YTransaction *txn, const char *name);

/**
 * Gets or creates a new shared `YText` data type instance as a root-level type of a given document.
 * This structure can later be accessed using its `name`, which must be a null-terminated UTF-8
 * compatible string.
 */
Branch *ytext(YDoc *doc, const char *name);

/**
 * Gets or creates a new shared `YArray` data type instance as a root-level type of a given document.
 * This structure can later be accessed using its `name`, which must be a null-terminated UTF-8
 * compatible string.
 *
 * Use [yarray_destroy] in order to release pointer returned that way - keep in mind that this will
 * not remove `YArray` instance from the document itself (once created it'll last for the entire
 * lifecycle of a document).
 */
Branch *yarray(YDoc *doc,
               const char *name);

/**
 * Gets or creates a new shared `YMap` data type instance as a root-level type of a given document.
 * This structure can later be accessed using its `name`, which must be a null-terminated UTF-8
 * compatible string.
 *
 * Use [ymap_destroy] in order to release pointer returned that way - keep in mind that this will
 * not remove `YMap` instance from the document itself (once created it'll last for the entire
 * lifecycle of a document).
 */
Branch *ymap(YDoc *doc, const char *name);

/**
 * Gets or creates a new shared `YXmlElement` data type instance as a root-level type of a given
 * document. This structure can later be accessed using its `name`, which must be a null-terminated
 * UTF-8 compatible string.
 */
Branch *yxmlfragment(YDoc *doc, const char *name);

/**
 * Returns a state vector of a current transaction's document, serialized using lib0 version 1
 * encoding. Payload created by this function can then be send over the network to a remote peer,
 * where it can be used as a parameter of [ytransaction_state_diff_v1] in order to produce a delta
 * update payload, that can be send back and applied locally in order to efficiently propagate
 * updates from one peer to another.
 *
 * The length of a generated binary will be passed within a `len` out parameter.
 *
 * Once no longer needed, a returned binary can be disposed using [ybinary_destroy] function.
 */
char *ytransaction_state_vector_v1(const YTransaction *txn, uint32_t *len);

/**
 * Returns a delta difference between current state of a transaction's document and a state vector
 * `sv` encoded as a binary payload using lib0 version 1 encoding (which could be generated using
 * [ytransaction_state_vector_v1]). Such delta can be send back to the state vector's sender in
 * order to propagate and apply (using [ytransaction_apply]) all updates known to a current
 * document, which remote peer was not aware of.
 *
 * If passed `sv` pointer is null, the generated diff will be a snapshot containing entire state of
 * the document.
 *
 * A length of an encoded state vector payload must be passed as `sv_len` parameter.
 *
 * A length of generated delta diff binary will be passed within a `len` out parameter.
 *
 * Once no longer needed, a returned binary can be disposed using [ybinary_destroy] function.
 */
char *ytransaction_state_diff_v1(const YTransaction *txn,
                                 const char *sv,
                                 uint32_t sv_len,
                                 uint32_t *len);

/**
 * Returns a delta difference between current state of a transaction's document and a state vector
 * `sv` encoded as a binary payload using lib0 version 1 encoding (which could be generated using
 * [ytransaction_state_vector_v1]). Such delta can be send back to the state vector's sender in
 * order to propagate and apply (using [ytransaction_apply_v2]) all updates known to a current
 * document, which remote peer was not aware of.
 *
 * If passed `sv` pointer is null, the generated diff will be a snapshot containing entire state of
 * the document.
 *
 * A length of an encoded state vector payload must be passed as `sv_len` parameter.
 *
 * A length of generated delta diff binary will be passed within a `len` out parameter.
 *
 * Once no longer needed, a returned binary can be disposed using [ybinary_destroy] function.
 */
char *ytransaction_state_diff_v2(const YTransaction *txn,
                                 const char *sv,
                                 uint32_t sv_len,
                                 uint32_t *len);

/**
 * Returns a snapshot descriptor of a current state of the document. This snapshot information
 * can be then used to encode document data at a particular point in time
 * (see: `ytransaction_encode_state_from_snapshot`).
 */
char *ytransaction_snapshot(const YTransaction *txn, uint32_t *len);

/**
 * Encodes a state of the document at a point in time specified by the provided `snapshot`
 * (generated by: `ytransaction_snapshot`). This is useful to generate a past view of the document.
 *
 * The returned update is binary compatible with Yrs update lib0 v1 encoding, and can be processed
 * with functions dedicated to work on it, like `ytransaction_apply`.
 *
 * This function requires document with a GC option flag turned off (otherwise "time travel" would
 * not be a safe operation). If this is not a case, the NULL pointer will be returned.
 */
char *ytransaction_encode_state_from_snapshot_v1(const YTransaction *txn,
                                                 const char *snapshot,
                                                 uint32_t snapshot_len,
                                                 uint32_t *len);

/**
 * Encodes a state of the document at a point in time specified by the provided `snapshot`
 * (generated by: `ytransaction_snapshot`). This is useful to generate a past view of the document.
 *
 * The returned update is binary compatible with Yrs update lib0 v2 encoding, and can be processed
 * with functions dedicated to work on it, like `ytransaction_apply_v2`.
 *
 * This function requires document with a GC option flag turned off (otherwise "time travel" would
 * not be a safe operation). If this is not a case, the NULL pointer will be returned.
 */
char *ytransaction_encode_state_from_snapshot_v2(const YTransaction *txn,
                                                 const char *snapshot,
                                                 uint32_t snapshot_len,
                                                 uint32_t *len);

/**
 * Returns an unapplied Delete Set for the current document, waiting for missing updates in order
 * to be integrated into document store.
 *
 * Return `NULL` if there's no missing delete set and all deletions have been applied.
 * See also: `ytransaction_pending_update`
 */
struct YDeleteSet *ytransaction_pending_ds(const YTransaction *txn);

void ydelete_set_destroy(struct YDeleteSet *ds);

/**
 * Returns a pending update associated with an underlying `YDoc`. Pending update contains update
 * data waiting for being integrated into main document store. Usually reason for that is that
 * there were missing updates required for integration. In such cases they need to arrive and be
 * integrated first.
 *
 * Returns `NULL` if there is not update pending. Returned value can be released by calling
 * `ypending_update_destroy`.
 * See also: `ytransaction_pending_ds`
 */
struct YPendingUpdate *ytransaction_pending_update(const YTransaction *txn);

void ypending_update_destroy(struct YPendingUpdate *update);

/**
 * Returns a null-terminated UTF-8 encoded string representation of an `update` binary payload,
 * encoded using lib0 v1 encoding.
 * Returns null if update couldn't be parsed into a lib0 v1 formatting.
 */
char *yupdate_debug_v1(const char *update, uint32_t update_len);

/**
 * Returns a null-terminated UTF-8 encoded string representation of an `update` binary payload,
 * encoded using lib0 v2 encoding.
 * Returns null if update couldn't be parsed into a lib0 v2 formatting.
 */
char *yupdate_debug_v2(const char *update, uint32_t update_len);

/**
 * Applies an diff update (generated by `ytransaction_state_diff_v1`) to a local transaction's
 * document.
 *
 * A length of generated `diff` binary must be passed within a `diff_len` out parameter.
 *
 * Returns an error code in case if transaction succeeded failed:
 * - **0**: success
 * - `ERR_CODE_IO` (**1**): couldn't read data from input stream.
 * - `ERR_CODE_VAR_INT` (**2**): decoded variable integer outside of the expected integer size bounds.
 * - `ERR_CODE_EOS` (**3**): end of stream found when more data was expected.
 * - `ERR_CODE_UNEXPECTED_VALUE` (**4**): decoded enum tag value was not among known cases.
 * - `ERR_CODE_INVALID_JSON` (**5**): failure when trying to decode JSON content.
 * - `ERR_CODE_OTHER` (**6**): other error type than the one specified.
 */
uint8_t ytransaction_apply(YTransaction *txn,
                           const char *diff,
                           uint32_t diff_len);

/**
 * Applies an diff update (generated by [ytransaction_state_diff_v2]) to a local transaction's
 * document.
 *
 * A length of generated `diff` binary must be passed within a `diff_len` out parameter.
 *
 * Returns an error code in case if transaction succeeded failed:
 * - **0**: success
 * - `ERR_CODE_IO` (**1**): couldn't read data from input stream.
 * - `ERR_CODE_VAR_INT` (**2**): decoded variable integer outside of the expected integer size bounds.
 * - `ERR_CODE_EOS` (**3**): end of stream found when more data was expected.
 * - `ERR_CODE_UNEXPECTED_VALUE` (**4**): decoded enum tag value was not among known cases.
 * - `ERR_CODE_INVALID_JSON` (**5**): failure when trying to decode JSON content.
 * - `ERR_CODE_OTHER` (**6**): other error type than the one specified.
 */
uint8_t ytransaction_apply_v2(YTransaction *txn,
                              const char *diff,
                              uint32_t diff_len);

/**
 * Returns the length of the `YText` string content in bytes (without the null terminator character)
 */
uint32_t ytext_len(const Branch *txt, const YTransaction *txn);

/**
 * Returns a null-terminated UTF-8 encoded string content of a current `YText` shared data type.
 *
 * Generated string resources should be released using [ystring_destroy] function.
 */
char *ytext_string(const Branch *txt, const YTransaction *txn);

/**
 * Inserts a null-terminated UTF-8 encoded string a given `index`. `index` value must be between
 * 0 and a length of a `YText` (inclusive, accordingly to [ytext_len] return value), otherwise this
 * function will panic.
 *
 * A `str` parameter must be a null-terminated UTF-8 encoded string. This function doesn't take
 * ownership over a passed value - it will be copied and therefore a string parameter must be
 * released by the caller.
 *
 * A nullable pointer with defined `attrs` will be used to wrap provided text with
 * a formatting blocks. `attrs` must be a map-like type.
 */
void ytext_insert(const Branch *txt,
                  YTransaction *txn,
                  uint32_t index,
                  const char *value,
                  const struct YInput *attrs);

/**
 * Wraps an existing piece of text within a range described by `index`-`len` parameters with
 * formatting blocks containing provided `attrs` metadata. `attrs` must be a map-like type.
 */
void ytext_format(const Branch *txt,
                  YTransaction *txn,
                  uint32_t index,
                  uint32_t len,
                  const struct YInput *attrs);

/**
 * Inserts an embed content given `index`. `index` value must be between 0 and a length of a
 * `YText` (inclusive, accordingly to [ytext_len] return value), otherwise this
 * function will panic.
 *
 * A `str` parameter must be a null-terminated UTF-8 encoded string. This function doesn't take
 * ownership over a passed value - it will be copied and therefore a string parameter must be
 * released by the caller.
 *
 * A nullable pointer with defined `attrs` will be used to wrap provided text with
 * a formatting blocks. `attrs` must be a map-like type.
 */
void ytext_insert_embed(const Branch *txt,
                        YTransaction *txn,
                        uint32_t index,
                        const struct YInput *content,
                        const struct YInput *attrs);

/**
 * Performs a series of changes over the given `YText` shared ref type, described by the `delta`
 * parameter:
 *
 * - Deltas constructed with `ydelta_input_retain` will move cursor position by the given number
 *   of elements. If formatting attributes were defined, all elements skipped over this way will be
 *   wrapped by given formatting attributes.
 * - Deltas constructed with `ydelta_input_delete` will tell cursor to remove a corresponding
 *   number of elements.
 * - Deltas constructed with `ydelta_input_insert` will tell cursor to insert given elements into
 *   current cursor position. While these elements can be of any type (used for embedding ie.
 *   shared types or binary payload like images), for the text insertion a `yinput_string`
 *   is expected. If formatting attributes were specified, inserted elements will be wrapped by
 *   given formatting attributes.
 */
void ytext_insert_delta(const Branch *txt,
                        YTransaction *txn,
                        struct YDeltaIn *delta,
                        uint32_t delta_len);

/**
 * Creates a parameter for `ytext_insert_delta` function. This parameter will move cursor position
 * by the `len` of elements. If formatting `attrs` were defined, all elements skipped over this
 * way will be wrapped by given formatting attributes.
 */
struct YDeltaIn ydelta_input_retain(uint32_t len, const struct YInput *attrs);

/**
 * Creates a parameter for `ytext_insert_delta` function. This parameter will tell cursor to remove
 * a corresponding number of elements, starting from current cursor position.
 */
struct YDeltaIn ydelta_input_delete(uint32_t len);

/**
 * Creates a parameter for `ytext_insert_delta` function. This parameter will tell cursor to insert
 * given elements into current cursor position. While these elements can be of any type (used for
 * embedding ie. shared types or binary payload like images), for the text insertion a `yinput_string`
 * is expected. If formatting attributes were specified, inserted elements will be wrapped by
 * given formatting attributes.
 */
struct YDeltaIn ydelta_input_insert(const struct YInput *data,
                                    const struct YInput *attrs);

/**
 * Removes a range of characters, starting a a given `index`. This range must fit within the bounds
 * of a current `YText`, otherwise this function call will fail.
 *
 * An `index` value must be between 0 and the length of a `YText` (exclusive, accordingly to
 * [ytext_len] return value).
 *
 * A `length` must be lower or equal number of characters (counted as UTF chars depending on the
 * encoding configured by `YDoc`) from `index` position to the end of of the string.
 */
void ytext_remove_range(const Branch *txt, YTransaction *txn, uint32_t index, uint32_t length);

/**
 * Returns a number of elements stored within current instance of `YArray`.
 */
uint32_t yarray_len(const Branch *array);

/**
 * Returns a pointer to a `YOutput` value stored at a given `index` of a current `YArray`.
 * If `index` is outside the bounds of an array, a null pointer will be returned.
 *
 * A value returned should be eventually released using [youtput_destroy] function.
 */
struct YOutput *yarray_get(const Branch *array, const YTransaction *txn, uint32_t index);

/**
 * Returns a UTF-8 encoded, NULL-terminated JSON string representing a value stored in a current
 * YArray under a given index.
 *
 * This method will return `NULL` pointer if value was outside the bound of an array or couldn't be
 * serialized into JSON string.
 *
 * This method will also try to serialize complex types that don't have native JSON representation
 * like YMap, YArray, YText etc. in such cases their contents will be materialized into JSON values.
 *
 * A string returned should be eventually released using [ystring_destroy] function.
 */
char *yarray_get_json(const Branch *array, const YTransaction *txn, uint32_t index);

/**
 * Inserts a range of `items` into current `YArray`, starting at given `index`. An `items_len`
 * parameter is used to determine the size of `items` array - it can also be used to insert
 * a single element given its pointer.
 *
 * An `index` value must be between 0 and (inclusive) length of a current array (use [yarray_len]
 * to determine its length), otherwise it will panic at runtime.
 *
 * `YArray` doesn't take ownership over the inserted `items` data - their contents are being copied
 * into array structure - therefore caller is responsible for freeing all memory associated with
 * input params.
 */
void yarray_insert_range(const Branch *array,
                         YTransaction *txn,
                         uint32_t index,
                         const struct YInput *items,
                         uint32_t items_len);

/**
 * Removes a `len` of consecutive range of elements from current `array` instance, starting at
 * a given `index`. Range determined by `index` and `len` must fit into boundaries of an array,
 * otherwise it will panic at runtime.
 */
void yarray_remove_range(const Branch *array, YTransaction *txn, uint32_t index, uint32_t len);

void yarray_move(const Branch *array, YTransaction *txn, uint32_t source, uint32_t target);

/**
 * Returns an iterator, which can be used to traverse over all elements of an `array` (`array`'s
 * length can be determined using [yarray_len] function).
 *
 * Use [yarray_iter_next] function in order to retrieve a consecutive array elements.
 * Use [yarray_iter_destroy] function in order to close the iterator and release its resources.
 */
YArrayIter *yarray_iter(const Branch *array, YTransaction *txn);

/**
 * Releases all of an `YArray` iterator resources created by calling [yarray_iter].
 */
void yarray_iter_destroy(YArrayIter *iter);

/**
 * Moves current `YArray` iterator over to a next element, returning a pointer to it. If an iterator
 * comes to an end of an array, a null pointer will be returned.
 *
 * Returned values should be eventually released using [youtput_destroy] function.
 */
struct YOutput *yarray_iter_next(YArrayIter *iterator);

/**
 * Returns an iterator, which can be used to traverse over all key-value pairs of a `map`.
 *
 * Use [ymap_iter_next] function in order to retrieve a consecutive (**unordered**) map entries.
 * Use [ymap_iter_destroy] function in order to close the iterator and release its resources.
 */
YMapIter *ymap_iter(const Branch *map, const YTransaction *txn);

/**
 * Releases all of an `YMap` iterator resources created by calling [ymap_iter].
 */
void ymap_iter_destroy(YMapIter *iter);

/**
 * Moves current `YMap` iterator over to a next entry, returning a pointer to it. If an iterator
 * comes to an end of a map, a null pointer will be returned. Yrs maps are unordered and so are
 * their iterators.
 *
 * Returned values should be eventually released using [ymap_entry_destroy] function.
 */
struct YMapEntry *ymap_iter_next(YMapIter *iter);

/**
 * Returns a number of entries stored within a `map`.
 */
uint32_t ymap_len(const Branch *map, const YTransaction *txn);

/**
 * Inserts a new entry (specified as `key`-`value` pair) into a current `map`. If entry under such
 * given `key` already existed, its corresponding value will be replaced.
 *
 * A `key` must be a null-terminated UTF-8 encoded string, which contents will be copied into
 * a `map` (therefore it must be freed by the function caller).
 *
 * A `value` content is being copied into a `map`, therefore any of its content must be freed by
 * the function caller.
 */
void ymap_insert(const Branch *map, YTransaction *txn, const char *key, const struct YInput *value);

/**
 * Removes a `map` entry, given its `key`. Returns `1` if the corresponding entry was successfully
 * removed or `0` if no entry with a provided `key` has been found inside of a `map`.
 *
 * A `key` must be a null-terminated UTF-8 encoded string.
 */
uint8_t ymap_remove(const Branch *map, YTransaction *txn, const char *key);

/**
 * Returns a value stored under the provided `key`, or a null pointer if no entry with such `key`
 * has been found in a current `map`. A returned value is allocated by this function and therefore
 * should be eventually released using [youtput_destroy] function.
 *
 * A `key` must be a null-terminated UTF-8 encoded string.
 */
struct YOutput *ymap_get(const Branch *map, const YTransaction *txn, const char *key);

/**
 * Returns a value stored under the provided `key` as UTF-8 encoded, NULL-terminated JSON string.
 * Once not needed that string should be deallocated using `ystring_destroy`.
 *
 * This method will return `NULL` pointer if value was not found or value couldn't be serialized
 * into JSON string.
 *
 * This method will also try to serialize complex types that don't have native JSON representation
 * like YMap, YArray, YText etc. in such cases their contents will be materialized into JSON values.
 */
char *ymap_get_json(const Branch *map, const YTransaction *txn, const char *key);

/**
 * Removes all entries from a current `map`.
 */
void ymap_remove_all(const Branch *map, YTransaction *txn);

/**
 * Return a name (or an XML tag) of a current `YXmlElement`. Root-level XML nodes use "UNDEFINED" as
 * their tag names.
 *
 * Returned value is a null-terminated UTF-8 string, which must be released using [ystring_destroy]
 * function.
 */
char *yxmlelem_tag(const Branch *xml);

/**
 * Converts current `YXmlElement` together with its children and attributes into a flat string
 * representation (no padding) eg. `<UNDEFINED><title key="value">sample text</title></UNDEFINED>`.
 *
 * Returned value is a null-terminated UTF-8 string, which must be released using [ystring_destroy]
 * function.
 */
char *yxmlelem_string(const Branch *xml, const YTransaction *txn);

/**
 * Inserts an XML attribute described using `attr_name` and `attr_value`. If another attribute with
 * the same name already existed, its value will be replaced with a provided one.
 *
 * Both `attr_name` and `attr_value` must be a null-terminated UTF-8 encoded strings. Their
 * contents are being copied, therefore it's up to a function caller to properly release them.
 */
void yxmlelem_insert_attr(const Branch *xml,
                          YTransaction *txn,
                          const char *attr_name,
                          const char *attr_value);

/**
 * Removes an attribute from a current `YXmlElement`, given its name.
 *
 * An `attr_name`must be a null-terminated UTF-8 encoded string.
 */
void yxmlelem_remove_attr(const Branch *xml, YTransaction *txn, const char *attr_name);

/**
 * Returns the value of a current `YXmlElement`, given its name, or a null pointer if not attribute
 * with such name has been found. Returned pointer is a null-terminated UTF-8 encoded string, which
 * should be released using [ystring_destroy] function.
 *
 * An `attr_name` must be a null-terminated UTF-8 encoded string.
 */
char *yxmlelem_get_attr(const Branch *xml, const YTransaction *txn, const char *attr_name);

/**
 * Returns an iterator over the `YXmlElement` attributes.
 *
 * Use [yxmlattr_iter_next] function in order to retrieve a consecutive (**unordered**) attributes.
 * Use [yxmlattr_iter_destroy] function in order to close the iterator and release its resources.
 */
YXmlAttrIter *yxmlelem_attr_iter(const Branch *xml, const YTransaction *txn);

/**
 * Returns an iterator over the `YXmlText` attributes.
 *
 * Use [yxmlattr_iter_next] function in order to retrieve a consecutive (**unordered**) attributes.
 * Use [yxmlattr_iter_destroy] function in order to close the iterator and release its resources.
 */
YXmlAttrIter *yxmltext_attr_iter(const Branch *xml, const YTransaction *txn);

/**
 * Releases all of attributes iterator resources created by calling [yxmlelem_attr_iter]
 * or [yxmltext_attr_iter].
 */
void yxmlattr_iter_destroy(YXmlAttrIter *iterator);

/**
 * Returns a next XML attribute from an `iterator`. Attributes are returned in an unordered
 * manner. Once `iterator` reaches the end of attributes collection, a null pointer will be
 * returned.
 *
 * Returned value should be eventually released using [yxmlattr_destroy].
 */
struct YXmlAttr *yxmlattr_iter_next(YXmlAttrIter *iterator);

/**
 * Returns a next sibling of a current XML node, which can be either another `YXmlElement`
 * or a `YXmlText`. Together with [yxmlelem_first_child] it may be used to iterate over the direct
 * children of an XML node (in order to iterate over the nested XML structure use
 * [yxmlelem_tree_walker]).
 *
 * If current `YXmlElement` is the last child, this function returns a null pointer.
 * A returned value should be eventually released using [youtput_destroy] function.
 */
struct YOutput *yxml_next_sibling(const Branch *xml, const YTransaction *txn);

/**
 * Returns a previous sibling of a current XML node, which can be either another `YXmlElement`
 * or a `YXmlText`.
 *
 * If current `YXmlElement` is the first child, this function returns a null pointer.
 * A returned value should be eventually released using [youtput_destroy] function.
 */
struct YOutput *yxml_prev_sibling(const Branch *xml, const YTransaction *txn);

/**
 * Returns a parent `YXmlElement` of a current node, or null pointer when current `YXmlElement` is
 * a root-level shared data type.
 */
Branch *yxmlelem_parent(const Branch *xml);

/**
 * Returns a number of child nodes (both `YXmlElement` and `YXmlText`) living under a current XML
 * element. This function doesn't count a recursive nodes, only direct children of a current node.
 */
uint32_t yxmlelem_child_len(const Branch *xml, const YTransaction *txn);

/**
 * Returns a first child node of a current `YXmlElement`, or null pointer if current XML node is
 * empty. Returned value could be either another `YXmlElement` or `YXmlText`.
 *
 * A returned value should be eventually released using [youtput_destroy] function.
 */
struct YOutput *yxmlelem_first_child(const Branch *xml);

/**
 * Returns an iterator over a nested recursive structure of a current `YXmlElement`, starting from
 * first of its children. Returned values can be either `YXmlElement` or `YXmlText` nodes.
 *
 * Use [yxmlelem_tree_walker_next] function in order to iterate over to a next node.
 * Use [yxmlelem_tree_walker_destroy] function to release resources used by the iterator.
 */
YXmlTreeWalker *yxmlelem_tree_walker(const Branch *xml, const YTransaction *txn);

/**
 * Releases resources associated with a current XML tree walker iterator.
 */
void yxmlelem_tree_walker_destroy(YXmlTreeWalker *iter);

/**
 * Moves current `iterator` to a next value (either `YXmlElement` or `YXmlText`), returning its
 * pointer or a null, if an `iterator` already reached the last successor node.
 *
 * Values returned by this function should be eventually released using [youtput_destroy].
 */
struct YOutput *yxmlelem_tree_walker_next(YXmlTreeWalker *iterator);

/**
 * Inserts an `YXmlElement` as a child of a current node at the given `index` and returns its
 * pointer. Node created this way will have a given `name` as its tag (eg. `p` for `<p></p>` node).
 *
 * An `index` value must be between 0 and (inclusive) length of a current XML element (use
 * [yxmlelem_child_len] function to determine its length).
 *
 * A `name` must be a null-terminated UTF-8 encoded string, which will be copied into current
 * document. Therefore `name` should be freed by the function caller.
 */
Branch *yxmlelem_insert_elem(const Branch *xml,
                             YTransaction *txn,
                             uint32_t index,
                             const char *name);

/**
 * Inserts an `YXmlText` as a child of a current node at the given `index` and returns its
 * pointer.
 *
 * An `index` value must be between 0 and (inclusive) length of a current XML element (use
 * [yxmlelem_child_len] function to determine its length).
 */
Branch *yxmlelem_insert_text(const Branch *xml, YTransaction *txn, uint32_t index);

/**
 * Removes a consecutive range of child elements (of specified length) from the current
 * `YXmlElement`, starting at the given `index`. Specified range must fit into boundaries of current
 * XML node children, otherwise this function will panic at runtime.
 */
void yxmlelem_remove_range(const Branch *xml, YTransaction *txn, uint32_t index, uint32_t len);

/**
 * Returns an XML child node (either a `YXmlElement` or `YXmlText`) stored at a given `index` of
 * a current `YXmlElement`. Returns null pointer if `index` was outside of the bound of current XML
 * node children.
 *
 * Returned value should be eventually released using [youtput_destroy].
 */
const struct YOutput *yxmlelem_get(const Branch *xml, const YTransaction *txn, uint32_t index);

/**
 * Returns the length of the `YXmlText` string content in bytes (without the null terminator
 * character)
 */
uint32_t yxmltext_len(const Branch *txt, const YTransaction *txn);

/**
 * Returns a null-terminated UTF-8 encoded string content of a current `YXmlText` shared data type.
 *
 * Generated string resources should be released using [ystring_destroy] function.
 */
char *yxmltext_string(const Branch *txt, const YTransaction *txn);

/**
 * Inserts a null-terminated UTF-8 encoded string a a given `index`. `index` value must be between
 * 0 and a length of a `YXmlText` (inclusive, accordingly to [yxmltext_len] return value), otherwise
 * this function will panic.
 *
 * A `str` parameter must be a null-terminated UTF-8 encoded string. This function doesn't take
 * ownership over a passed value - it will be copied and therefore a string parameter must be
 * released by the caller.
 *
 * A nullable pointer with defined `attrs` will be used to wrap provided text with
 * a formatting blocks. `attrs` must be a map-like type.
 */
void yxmltext_insert(const Branch *txt,
                     YTransaction *txn,
                     uint32_t index,
                     const char *str,
                     const struct YInput *attrs);

/**
 * Inserts an embed content given `index`. `index` value must be between 0 and a length of a
 * `YXmlText` (inclusive, accordingly to [ytext_len] return value), otherwise this
 * function will panic.
 *
 * A `str` parameter must be a null-terminated UTF-8 encoded string. This function doesn't take
 * ownership over a passed value - it will be copied and therefore a string parameter must be
 * released by the caller.
 *
 * A nullable pointer with defined `attrs` will be used to wrap provided text with
 * a formatting blocks. `attrs` must be a map-like type.
 */
void yxmltext_insert_embed(const Branch *txt,
                           YTransaction *txn,
                           uint32_t index,
                           const struct YInput *content,
                           const struct YInput *attrs);

/**
 * Wraps an existing piece of text within a range described by `index`-`len` parameters with
 * formatting blocks containing provided `attrs` metadata. `attrs` must be a map-like type.
 */
void yxmltext_format(const Branch *txt,
                     YTransaction *txn,
                     uint32_t index,
                     uint32_t len,
                     const struct YInput *attrs);

/**
 * Removes a range of characters, starting a a given `index`. This range must fit within the bounds
 * of a current `YXmlText`, otherwise this function call will fail.
 *
 * An `index` value must be between 0 and the length of a `YXmlText` (exclusive, accordingly to
 * [yxmltext_len] return value).
 *
 * A `length` must be lower or equal number of characters (counted as UTF chars depending on the
 * encoding configured by `YDoc`) from `index` position to the end of of the string.
 */
void yxmltext_remove_range(const Branch *txt, YTransaction *txn, uint32_t idx, uint32_t len);

/**
 * Inserts an XML attribute described using `attr_name` and `attr_value`. If another attribute with
 * the same name already existed, its value will be replaced with a provided one.
 *
 * Both `attr_name` and `attr_value` must be a null-terminated UTF-8 encoded strings. Their
 * contents are being copied, therefore it's up to a function caller to properly release them.
 */
void yxmltext_insert_attr(const Branch *txt,
                          YTransaction *txn,
                          const char *attr_name,
                          const char *attr_value);

/**
 * Removes an attribute from a current `YXmlText`, given its name.
 *
 * An `attr_name`must be a null-terminated UTF-8 encoded string.
 */
void yxmltext_remove_attr(const Branch *txt, YTransaction *txn, const char *attr_name);

/**
 * Returns the value of a current `YXmlText`, given its name, or a null pointer if not attribute
 * with such name has been found. Returned pointer is a null-terminated UTF-8 encoded string, which
 * should be released using [ystring_destroy] function.
 *
 * An `attr_name` must be a null-terminated UTF-8 encoded string.
 */
char *yxmltext_get_attr(const Branch *txt, const YTransaction *txn, const char *attr_name);

/**
 * Returns a collection of chunks representing pieces of `YText` rich text string grouped together
 * by the same formatting rules and type. `chunks_len` is used to inform about a number of chunks
 * generated this way.
 *
 * Returned array needs to be eventually deallocated using `ychunks_destroy`.
 */
struct YChunk *ytext_chunks(const Branch *txt, const YTransaction *txn, uint32_t *chunks_len);

/**
 * Deallocates result of `ytext_chunks` method.
 */
void ychunks_destroy(struct YChunk *chunks, uint32_t len);

/**
 * Releases all resources related to a corresponding `YOutput` cell.
 */
void youtput_destroy(struct YOutput *val);

/**
 * Function constructor used to create JSON-like NULL `YInput` cell.
 * This function doesn't allocate any heap resources.
 */
struct YInput yinput_null(void);

/**
 * Function constructor used to create JSON-like undefined `YInput` cell.
 * This function doesn't allocate any heap resources.
 */
struct YInput yinput_undefined(void);

/**
 * Function constructor used to create JSON-like boolean `YInput` cell.
 * This function doesn't allocate any heap resources.
 */
struct YInput yinput_bool(uint8_t flag);

/**
 * Function constructor used to create JSON-like 64-bit floating point number `YInput` cell.
 * This function doesn't allocate any heap resources.
 */
struct YInput yinput_float(double num);

/**
 * Function constructor used to create JSON-like 64-bit signed integer `YInput` cell.
 * This function doesn't allocate any heap resources.
 */
struct YInput yinput_long(int64_t integer);

/**
 * Function constructor used to create a string `YInput` cell. Provided parameter must be
 * a null-terminated UTF-8 encoded string. This function doesn't allocate any heap resources,
 * and doesn't release any on its own, therefore its up to a caller to free resources once
 * a structure is no longer needed.
 */
struct YInput yinput_string(const char *str);

/**
 * Function constructor used to create aa `YInput` cell representing any JSON-like object.
 * Provided parameter must be a null-terminated UTF-8 encoded JSON string.
 *
 * This function doesn't allocate any heap resources and doesn't release any on its own, therefore
 * its up to a caller to free resources once a structure is no longer needed.
 */
struct YInput yinput_json(const char *str);

/**
 * Function constructor used to create a binary `YInput` cell of a specified length.
 * This function doesn't allocate any heap resources and doesn't release any on its own, therefore
 * its up to a caller to free resources once a structure is no longer needed.
 */
struct YInput yinput_binary(const char *buf, uint32_t len);

/**
 * Function constructor used to create a JSON-like array `YInput` cell of other JSON-like values of
 * a given length. This function doesn't allocate any heap resources and doesn't release any on its
 * own, therefore its up to a caller to free resources once a structure is no longer needed.
 */
struct YInput yinput_json_array(struct YInput *values, uint32_t len);

/**
 * Function constructor used to create a JSON-like map `YInput` cell of other JSON-like key-value
 * pairs. These pairs are build from corresponding indexes of `keys` and `values`, which must have
 * the same specified length.
 *
 * This function doesn't allocate any heap resources and doesn't release any on its own, therefore
 * its up to a caller to free resources once a structure is no longer needed.
 */
struct YInput yinput_json_map(char **keys, struct YInput *values, uint32_t len);

/**
 * Function constructor used to create a nested `YArray` `YInput` cell prefilled with other
 * values of a given length. This function doesn't allocate any heap resources and doesn't release
 * any on its own, therefore its up to a caller to free resources once a structure is no longer
 * needed.
 */
struct YInput yinput_yarray(struct YInput *values, uint32_t len);

/**
 * Function constructor used to create a nested `YMap` `YInput` cell prefilled with other key-value
 * pairs. These pairs are build from corresponding indexes of `keys` and `values`, which must have
 * the same specified length.
 *
 * This function doesn't allocate any heap resources and doesn't release any on its own, therefore
 * its up to a caller to free resources once a structure is no longer needed.
 */
struct YInput yinput_ymap(char **keys, struct YInput *values, uint32_t len);

/**
 * Function constructor used to create a nested `YText` `YInput` cell prefilled with a specified
 * string, which must be a null-terminated UTF-8 character pointer.
 *
 * This function doesn't allocate any heap resources and doesn't release any on its own, therefore
 * its up to a caller to free resources once a structure is no longer needed.
 */
struct YInput yinput_ytext(char *str);

/**
 * Function constructor used to create a nested `YXmlElement` `YInput` cell with a specified
 * tag name, which must be a null-terminated UTF-8 character pointer.
 *
 * This function doesn't allocate any heap resources and doesn't release any on its own, therefore
 * its up to a caller to free resources once a structure is no longer needed.
 */
struct YInput yinput_yxmlelem(char *name);

/**
 * Function constructor used to create a nested `YXmlText` `YInput` cell prefilled with a specified
 * string, which must be a null-terminated UTF-8 character pointer.
 *
 * This function doesn't allocate any heap resources and doesn't release any on its own, therefore
 * its up to a caller to free resources once a structure is no longer needed.
 */
struct YInput yinput_yxmltext(char *str);

/**
 * Function constructor used to create a nested `YDoc` `YInput` cell.
 *
 * This function doesn't allocate any heap resources and doesn't release any on its own, therefore
 * its up to a caller to free resources once a structure is no longer needed.
 */
struct YInput yinput_ydoc(YDoc *doc);

/**
 * Function constructor used to create a string `YInput` cell with weak reference to another
 * element(s) living inside of the same document.
 */
struct YInput yinput_weak(const Weak *weak);

/**
 * Attempts to read the value for a given `YOutput` pointer as a `YDocRef` reference to a nested
 * document.
 */
YDoc *youtput_read_ydoc(const struct YOutput *val);

/**
 * Attempts to read the value for a given `YOutput` pointer as a boolean flag, which can be either
 * `1` for truthy case and `0` otherwise. Returns a null pointer in case when a value stored under
 * current `YOutput` cell is not of a boolean type.
 */
const uint8_t *youtput_read_bool(const struct YOutput *val);

/**
 * Attempts to read the value for a given `YOutput` pointer as a 64-bit floating point number.
 *
 * Returns a null pointer in case when a value stored under current `YOutput` cell
 * is not a floating point number.
 */
const double *youtput_read_float(const struct YOutput *val);

/**
 * Attempts to read the value for a given `YOutput` pointer as a 64-bit signed integer.
 *
 * Returns a null pointer in case when a value stored under current `YOutput` cell
 * is not a signed integer.
 */
const int64_t *youtput_read_long(const struct YOutput *val);

/**
 * Attempts to read the value for a given `YOutput` pointer as a null-terminated UTF-8 encoded
 * string.
 *
 * Returns a null pointer in case when a value stored under current `YOutput` cell
 * is not a string. Underlying string is released automatically as part of [youtput_destroy]
 * destructor.
 */
char *youtput_read_string(const struct YOutput *val);

/**
 * Attempts to read the value for a given `YOutput` pointer as a binary payload (which length is
 * stored within `len` filed of a cell itself).
 *
 * Returns a null pointer in case when a value stored under current `YOutput` cell
 * is not a binary type. Underlying binary is released automatically as part of [youtput_destroy]
 * destructor.
 */
const char *youtput_read_binary(const struct YOutput *val);

/**
 * Attempts to read the value for a given `YOutput` pointer as a JSON-like array of `YOutput`
 * values (which length is stored within `len` filed of a cell itself).
 *
 * Returns a null pointer in case when a value stored under current `YOutput` cell
 * is not a JSON-like array. Underlying heap resources are released automatically as part of
 * [youtput_destroy] destructor.
 */
struct YOutput *youtput_read_json_array(const struct YOutput *val);

/**
 * Attempts to read the value for a given `YOutput` pointer as a JSON-like map of key-value entries
 * (which length is stored within `len` filed of a cell itself).
 *
 * Returns a null pointer in case when a value stored under current `YOutput` cell
 * is not a JSON-like map. Underlying heap resources are released automatically as part of
 * [youtput_destroy] destructor.
 */
struct YMapEntry *youtput_read_json_map(const struct YOutput *val);

/**
 * Attempts to read the value for a given `YOutput` pointer as an `YArray`.
 *
 * Returns a null pointer in case when a value stored under current `YOutput` cell
 * is not an `YArray`. Underlying heap resources are released automatically as part of
 * [youtput_destroy] destructor.
 */
Branch *youtput_read_yarray(const struct YOutput *val);

/**
 * Attempts to read the value for a given `YOutput` pointer as an `YXmlElement`.
 *
 * Returns a null pointer in case when a value stored under current `YOutput` cell
 * is not an `YXmlElement`. Underlying heap resources are released automatically as part of
 * [youtput_destroy] destructor.
 */
Branch *youtput_read_yxmlelem(const struct YOutput *val);

/**
 * Attempts to read the value for a given `YOutput` pointer as an `YMap`.
 *
 * Returns a null pointer in case when a value stored under current `YOutput` cell
 * is not an `YMap`. Underlying heap resources are released automatically as part of
 * [youtput_destroy] destructor.
 */
Branch *youtput_read_ymap(const struct YOutput *val);

/**
 * Attempts to read the value for a given `YOutput` pointer as an `YText`.
 *
 * Returns a null pointer in case when a value stored under current `YOutput` cell
 * is not an `YText`. Underlying heap resources are released automatically as part of
 * [youtput_destroy] destructor.
 */
Branch *youtput_read_ytext(const struct YOutput *val);

/**
 * Attempts to read the value for a given `YOutput` pointer as an `YXmlText`.
 *
 * Returns a null pointer in case when a value stored under current `YOutput` cell
 * is not an `YXmlText`. Underlying heap resources are released automatically as part of
 * [youtput_destroy] destructor.
 */
Branch *youtput_read_yxmltext(const struct YOutput *val);

/**
 * Attempts to read the value for a given `YOutput` pointer as an `YWeakRef`.
 *
 * Returns a null pointer in case when a value stored under current `YOutput` cell
 * is not an `YWeakRef`. Underlying heap resources are released automatically as part of
 * [youtput_destroy] destructor.
 */
Branch *youtput_read_yweak(const struct YOutput *val);

/**
 * Unsubscribe callback from the oberver event it was previously subscribed to.
 */
void yunobserve(YSubscription *subscription);

/**
 * Subscribes a given callback function `cb` to changes made by this `YText` instance. Callbacks
 * are triggered whenever a `ytransaction_commit` is called.
 * Returns a subscription ID which can be then used to unsubscribe this callback by using
 * `yunobserve` function.
 */
YSubscription *ytext_observe(const Branch *txt, void *state, void (*cb)(void*,
                                                                        const struct YTextEvent*,
                                                                        uint32_t,
                                                                        const char*));

/**
 * Subscribes a given callback function `cb` to changes made by this `YMap` instance. Callbacks
 * are triggered whenever a `ytransaction_commit` is called.
 * Returns a subscription ID which can be then used to unsubscribe this callback by using
 * `yunobserve` function.
 */
YSubscription *ymap_observe(const Branch *map, void *state, void (*cb)(void*,
                                                                       const struct YMapEvent*,
                                                                       uint32_t,
                                                                       const char*));

/**
 * Subscribes a given callback function `cb` to changes made by this `YArray` instance. Callbacks
 * are triggered whenever a `ytransaction_commit` is called.
 * Returns a subscription ID which can be then used to unsubscribe this callback by using
 * `yunobserve` function.
 */
YSubscription *yarray_observe(const Branch *array,
                              void *state,
                              void (*cb)(void*, const struct YArrayEvent*, uint32_t, const char*));

/**
 * Subscribes a given callback function `cb` to changes made by this `YXmlElement` instance.
 * Callbacks are triggered whenever a `ytransaction_commit` is called.
 * Returns a subscription ID which can be then used to unsubscribe this callback by using
 * `yunobserve` function.
 */
YSubscription *yxmlelem_observe(const Branch *xml, void *state, void (*cb)(void*,
                                                                           const struct YXmlEvent*,
                                                                           uint32_t,
                                                                           const char*));

/**
 * Subscribes a given callback function `cb` to changes made by this `YXmlText` instance. Callbacks
 * are triggered whenever a `ytransaction_commit` is called.
 * Returns a subscription ID which can be then used to unsubscribe this callback by using
 * `yunobserve` function.
 */
YSubscription *yxmltext_observe(const Branch *xml,
                                void *state,
                                void (*cb)(void*, const struct YXmlTextEvent*, uint32_t, const char*));

/**
 * Subscribes a given callback function `cb` to changes made by this shared type instance as well
 * as all nested shared types living within it. Callbacks are triggered whenever a
 * `ytransaction_commit` is called.
 *
 * Returns a subscription ID which can be then used to unsubscribe this callback by using
 * `yunobserve` function.
 */
YSubscription *yobserve_deep(Branch *ytype, void *state, void (*cb)(void*,
                                                                    uint32_t,
                                                                    const struct YEvent*,
                                                                    uint32_t,
                                                                    const char*));

/**
 * Returns a pointer to a shared collection, which triggered passed event `e`.
 */
Branch *ytext_event_target(const struct YTextEvent *e);

/**
 * Returns a pointer to a shared collection, which triggered passed event `e`.
 */
Branch *yarray_event_target(const struct YArrayEvent *e);

/**
 * Returns a pointer to a shared collection, which triggered passed event `e`.
 */
Branch *ymap_event_target(const struct YMapEvent *e);

/**
 * Returns a pointer to a shared collection, which triggered passed event `e`.
 */
Branch *yxmlelem_event_target(const struct YXmlEvent *e);

/**
 * Returns a pointer to a shared collection, which triggered passed event `e`.
 */
Branch *yxmltext_event_target(const struct YXmlTextEvent *e);

/**
 * Returns a path from a root type down to a current shared collection (which can be obtained using
 * `ytext_event_target` function). It can consist of either integer indexes (used by sequence
 * components) of *char keys (used by map components). `len` output parameter is used to provide
 * information about length of the path.
 *
 * Path returned this way should be eventually released using `ypath_destroy`.
 */
struct YPathSegment *ytext_event_path(const struct YTextEvent *e, uint32_t *len);

/**
 * Returns a path from a root type down to a current shared collection (which can be obtained using
 * `ymap_event_target` function). It can consist of either integer indexes (used by sequence
 * components) of *char keys (used by map components). `len` output parameter is used to provide
 * information about length of the path.
 *
 * Path returned this way should be eventually released using `ypath_destroy`.
 */
struct YPathSegment *ymap_event_path(const struct YMapEvent *e, uint32_t *len);

/**
 * Returns a path from a root type down to a current shared collection (which can be obtained using
 * `yxmlelem_event_path` function). It can consist of either integer indexes (used by sequence
 * components) of *char keys (used by map components). `len` output parameter is used to provide
 * information about length of the path.
 *
 * Path returned this way should be eventually released using `ypath_destroy`.
 */
struct YPathSegment *yxmlelem_event_path(const struct YXmlEvent *e, uint32_t *len);

/**
 * Returns a path from a root type down to a current shared collection (which can be obtained using
 * `yxmltext_event_path` function). It can consist of either integer indexes (used by sequence
 * components) of *char keys (used by map components). `len` output parameter is used to provide
 * information about length of the path.
 *
 * Path returned this way should be eventually released using `ypath_destroy`.
 */
struct YPathSegment *yxmltext_event_path(const struct YXmlTextEvent *e, uint32_t *len);

/**
 * Returns a path from a root type down to a current shared collection (which can be obtained using
 * `yarray_event_target` function). It can consist of either integer indexes (used by sequence
 * components) of *char keys (used by map components). `len` output parameter is used to provide
 * information about length of the path.
 *
 * Path returned this way should be eventually released using `ypath_destroy`.
 */
struct YPathSegment *yarray_event_path(const struct YArrayEvent *e, uint32_t *len);

/**
 * Releases allocated memory used by objects returned from path accessor functions of shared type
 * events.
 */
void ypath_destroy(struct YPathSegment *path, uint32_t len);

/**
 * Returns a sequence of changes produced by sequence component of shared collections (such as
 * `YText`, `YXmlText` and XML nodes added to `YXmlElement`). `len` output parameter is used to
 * provide information about number of changes produced.
 *
 * Delta returned from this function should eventually be released using `yevent_delta_destroy`
 * function.
 */
struct YDeltaOut *ytext_event_delta(const struct YTextEvent *e, uint32_t *len);

/**
 * Returns a sequence of changes produced by sequence component of shared collections (such as
 * `YText`, `YXmlText` and XML nodes added to `YXmlElement`). `len` output parameter is used to
 * provide information about number of changes produced.
 *
 * Delta returned from this function should eventually be released using `yevent_delta_destroy`
 * function.
 */
struct YDeltaOut *yxmltext_event_delta(const struct YXmlTextEvent *e, uint32_t *len);

/**
 * Returns a sequence of changes produced by sequence component of shared collections (such as
 * `YText`, `YXmlText` and XML nodes added to `YXmlElement`). `len` output parameter is used to
 * provide information about number of changes produced.
 *
 * Delta returned from this function should eventually be released using `yevent_delta_destroy`
 * function.
 */
struct YEventChange *yarray_event_delta(const struct YArrayEvent *e, uint32_t *len);

/**
 * Returns a sequence of changes produced by sequence component of shared collections (such as
 * `YText`, `YXmlText` and XML nodes added to `YXmlElement`). `len` output parameter is used to
 * provide information about number of changes produced.
 *
 * Delta returned from this function should eventually be released using `yevent_delta_destroy`
 * function.
 */
struct YEventChange *yxmlelem_event_delta(const struct YXmlEvent *e, uint32_t *len);

/**
 * Releases memory allocated by the object returned from `yevent_delta` function.
 */
void ytext_delta_destroy(struct YDeltaOut *delta, uint32_t len);

/**
 * Releases memory allocated by the object returned from `yevent_delta` function.
 */
void yevent_delta_destroy(struct YEventChange *delta, uint32_t len);

/**
 * Returns a sequence of changes produced by map component of shared collections (such as
 * `YMap` and `YXmlText`/`YXmlElement` attribute changes). `len` output parameter is used to
 * provide information about number of changes produced.
 *
 * Delta returned from this function should eventually be released using `yevent_keys_destroy`
 * function.
 */
struct YEventKeyChange *ymap_event_keys(const struct YMapEvent *e, uint32_t *len);

/**
 * Returns a sequence of changes produced by map component of shared collections.
 * `len` output parameter is used to provide information about number of changes produced.
 *
 * Delta returned from this function should eventually be released using `yevent_keys_destroy`
 * function.
 */
struct YEventKeyChange *yxmlelem_event_keys(const struct YXmlEvent *e, uint32_t *len);

/**
 * Returns a sequence of changes produced by map component of shared collections.
 * `len` output parameter is used to provide information about number of changes produced.
 *
 * Delta returned from this function should eventually be released using `yevent_keys_destroy`
 * function.
 */
struct YEventKeyChange *yxmltext_event_keys(const struct YXmlTextEvent *e, uint32_t *len);

/**
 * Releases memory allocated by the object returned from `yxml_event_keys` and `ymap_event_keys`
 * functions.
 */
void yevent_keys_destroy(struct YEventKeyChange *keys, uint32_t len);

/**
 * Creates a new instance of undo manager bound to a current `doc`. It can be used to track
 * specific shared refs via `yundo_manager_add_scope` and updates coming from specific origin
 * - like ability to undo/redo operations originating only at the local peer - by using
 * `yundo_manager_add_origin`.
 *
 * This object can be deallocated via `yundo_manager_destroy`.
 */
YUndoManager *yundo_manager(const YDoc *doc, const struct YUndoManagerOptions *options);

/**
 * Deallocated undo manager instance created via `yundo_manager`.
 */
void yundo_manager_destroy(YUndoManager *mgr);

/**
 * Adds an origin to be tracked by current undo manager. This way only changes made within context
 * of transactions created with specific origin will be subjects of undo/redo operations. This is
 * useful when you want to be able to revert changed done by specific user without reverting
 * changes made by other users that were applied in the meantime.
 */
void yundo_manager_add_origin(YUndoManager *mgr, uint32_t origin_len, const char *origin);

/**
 * Removes an origin previously added to undo manager via `yundo_manager_add_origin`.
 */
void yundo_manager_remove_origin(YUndoManager *mgr, uint32_t origin_len, const char *origin);

/**
 * Add specific shared type to be tracked by this instance of an undo manager.
 */
void yundo_manager_add_scope(YUndoManager *mgr, const Branch *ytype);

/**
 * Removes all the undo/redo stack changes tracked by current undo manager. This also cleans up
 * all the items that couldn't be deallocated / garbage collected for the sake of possible
 * undo/redo operations.
 *
 * Keep in mind that this function call requires that underlying document store is not concurrently
 * modified by other read-write transaction. This is done by acquiring the read-only transaction
 * itself. If such transaction could be acquired (because of another read-write transaction is in
 * progress, this function will hold current thread until acquisition is possible.
 */
void yundo_manager_clear(YUndoManager *mgr);

/**
 * Cuts off tracked changes, producing a new stack item on undo stack.
 *
 * By default, undo manager gathers undergoing changes together into undo stack items on periodic
 * basis (defined by `YUndoManagerOptions.capture_timeout_millis`). By calling this function, we're
 * explicitly creating a new stack item will all the changes registered since last stack item was
 * created.
 */
void yundo_manager_stop(YUndoManager *mgr);

/**
 * Performs an undo operations, reverting all the changes defined by the last undo stack item.
 * These changes can be then reapplied again by calling `yundo_manager_redo` function.
 *
 * Returns `Y_TRUE` if successfully managed to do an undo operation.
 * Returns `Y_FALSE` if undo stack was empty or if undo couldn't be performed (because another
 * transaction is in progress).
 */
uint8_t yundo_manager_undo(YUndoManager *mgr);

/**
 * Performs a redo operations, reapplying changes undone by `yundo_manager_undo` operation.
 *
 * Returns `Y_TRUE` if successfully managed to do a redo operation.
 * Returns `Y_FALSE` if redo stack was empty or if redo couldn't be performed (because another
 * transaction is in progress).
 */
uint8_t yundo_manager_redo(YUndoManager *mgr);

/**
 * Returns number of elements stored on undo stack.
 */
uint32_t yundo_manager_undo_stack_len(YUndoManager *mgr);

/**
 * Returns number of elements stored on redo stack.
 */
uint32_t yundo_manager_redo_stack_len(YUndoManager *mgr);

/**
 * Subscribes a `callback` function pointer to a given undo manager event. This event will be
 * triggered every time a new undo/redo stack item is added.
 *
 * Returns a subscription pointer that can be used to cancel current callback registration via
 * `yunobserve`.
 */
YSubscription *yundo_manager_observe_added(YUndoManager *mgr,
                                           void *state,
                                           void (*callback)(void*, const struct YUndoEvent*));

/**
 * Subscribes a `callback` function pointer to a given undo manager event. This event will be
 * triggered every time a undo/redo operation was called.
 *
 * Returns a subscription pointer that can be used to cancel current callback registration via
 * `yunobserve`.
 */
YSubscription *yundo_manager_observe_popped(YUndoManager *mgr,
                                            void *state,
                                            void (*callback)(void*, const struct YUndoEvent*));

/**
 * Returns a value informing what kind of Yrs shared collection given `branch` represents.
 * Returns either 0 when `branch` is null or one of values: `Y_ARRAY`, `Y_TEXT`, `Y_MAP`,
 * `Y_XML_ELEM`, `Y_XML_TEXT`.
 */
int8_t ytype_kind(const Branch *branch);

/**
 * Releases resources allocated by `YStickyIndex` pointers.
 */
void ysticky_index_destroy(YStickyIndex *pos);

/**
 * Returns association of current `YStickyIndex`.
 * If association is **after** the referenced inserted character, returned number will be >= 0.
 * If association is **before** the referenced inserted character, returned number will be < 0.
 */
int8_t ysticky_index_assoc(const YStickyIndex *pos);

/**
 * Retrieves a `YStickyIndex` corresponding to a given human-readable `index` pointing into
 * the shared y-type `branch`. Unlike standard indexes sticky one enables to track
 * the location inside of a shared y-types, even in the face of concurrent updates.
 *
 * If association is >= 0, the resulting position will point to location **after** the referenced index.
 * If association is < 0, the resulting position will point to location **before** the referenced index.
 */
YStickyIndex *ysticky_index_from_index(const Branch *branch,
                                       YTransaction *txn,
                                       uint32_t index,
                                       int8_t assoc);

/**
 * Serializes `YStickyIndex` into binary representation. `len` parameter is updated with byte
 * length of the generated binary. Returned binary can be free'd using `ybinary_destroy`.
 */
char *ysticky_index_encode(const YStickyIndex *pos, uint32_t *len);

/**
 * Deserializes `YStickyIndex` from the payload previously serialized using `ysticky_index_encode`.
 */
YStickyIndex *ysticky_index_decode(const char *binary, uint32_t len);

/**
 * Given `YStickyIndex` and transaction reference, if computes a human-readable index in a
 * context of the referenced shared y-type.
 *
 * `out_branch` is getting assigned with a corresponding shared y-type reference.
 * `out_index` will be used to store computed human-readable index.
 */
void ysticky_index_read(const YStickyIndex *pos,
                        const YTransaction *txn,
                        Branch **out_branch,
                        uint32_t *out_index);

void yweak_destroy(const Weak *weak);

struct YOutput *yweak_deref(const Branch *map_link, const YTransaction *txn);

YWeakIter *yweak_iter(const Branch *array_link, const YTransaction *txn);

void yweak_iter_destroy(YWeakIter *iter);

struct YOutput *yweak_iter_next(YWeakIter *iter);

char *yweak_string(const Branch *text_link, const YTransaction *txn);

char *yweak_xml_string(const Branch *xml_text_link, const YTransaction *txn);

/**
 * Subscribes a given callback function `cb` to changes made by this `YText` instance. Callbacks
 * are triggered whenever a `ytransaction_commit` is called.
 * Returns a subscription ID which can be then used to unsubscribe this callback by using
 * `yunobserve` function.
 */
YSubscription *yweak_observe(const Branch *weak,
                             void *state,
                             void (*cb)(void*, const struct YWeakLinkEvent*));

const Weak *ymap_link(const Branch *map, const YTransaction *txn, const char *key);

const Weak *ytext_quote(const Branch *text,
                        YTransaction *txn,
                        uint32_t start_index,
                        uint32_t end_index,
                        int8_t start_exclusive,
                        int8_t end_exclusive);

const Weak *yarray_quote(const Branch *array,
                         YTransaction *txn,
                         uint32_t start_index,
                         uint32_t end_index,
                         int8_t start_exclusive,
                         int8_t end_exclusive);

/**
 * Returns a logical identifier for a given shared collection. That collection must be alive at
 * the moment of function call.
 */
struct YBranchId ybranch_id(const Branch *branch);

/**
 * Given a logical identifier, returns a physical pointer to a shared collection.
 * Returns null if collection was not found - either because it was not defined or not synchronized
 * yet.
 * Returned pointer may still point to deleted collection. In such case a subsequent `ybranch_alive`
 * function call is required.
 */
Branch *ybranch_get(const struct YBranchId *branch_id, YTransaction *txn);

/**
 * Check if current branch is still alive (returns `Y_TRUE`, otherwise `Y_FALSE`).
 * If it was deleted, this branch pointer is no longer a valid pointer and cannot be used to
 * execute any functions using it.
 */
uint8_t ybranch_alive(Branch *branch);

/**
 * Returns a UTF-8 encoded, NULL-terminated JSON string representation of the current branch
 * contents. Once no longer needed, this string must be explicitly deallocated by user using
 * `ystring_destroy`.
 *
 * If branch type couldn't be resolved (which usually happens for root-level types that were not
 * initialized locally) or doesn't have JSON representation a NULL pointer can be returned.
 */
char *ybranch_json(Branch *branch, YTransaction *txn);

#endif
