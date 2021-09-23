use crate::*;

use crate::block::{Block, BlockPtr, Item, ItemContent, Prelim, ID};
use crate::block_store::StateVector;
use crate::event::UpdateEvent;
use crate::id_set::{DeleteSet, IdSet};
use crate::store::Store;
use crate::types::array::Array;
use crate::types::xml::{XmlElement, XmlText};
use crate::types::{
    Branch, Map, Text, TypePtr, TYPE_REFS_ARRAY, TYPE_REFS_MAP, TYPE_REFS_TEXT,
    TYPE_REFS_XML_ELEMENT, TYPE_REFS_XML_TEXT,
};
use crate::update::Update;
use std::cell::RefMut;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use updates::encoder::*;

/// Transaction is one of the core types in Yrs. All operations that need to touch a document's
/// contents (a.k.a. block store), need to be executed in scope of a transaction.
pub struct Transaction<'a> {
    /// Store containing the state of the document.
    pub(crate) store: RefMut<'a, Store>,
    /// State vector of a current transaction at the moment of its creation.
    pub before_state: StateVector,
    /// Current state vector of a transaction, which includes all performed updates.
    pub after_state: StateVector,
    /// ID's of the blocks to be merged.
    pub merge_blocks: Vec<ID>,
    /// Describes the set of deleted items by ids.
    pub delete_set: DeleteSet,
    /// All types that were directly modified (property added or child inserted/deleted).
    /// New types are not included in this Set.
    changed: HashMap<TypePtr, HashSet<Option<String>>>,
}

impl<'a> Transaction<'a> {
    pub(crate) fn new(store: RefMut<'a, Store>) -> Transaction {
        let begin_timestamp = store.blocks.get_state_vector();
        Transaction {
            store,
            before_state: begin_timestamp,
            merge_blocks: Vec::new(),
            delete_set: DeleteSet::new(),
            changed: HashMap::new(),
            after_state: StateVector::default(),
        }
    }

    /// Returns state vector describing current state of the updates.
    pub fn state_vector(&self) -> StateVector {
        self.store.blocks.get_state_vector()
    }

    /// Encodes the difference between remove peer state given its `state_vector` and the state
    /// of a current local peer  
    pub fn encode_diff<E: Encoder>(&self, state_vector: &StateVector, encoder: &mut E) {
        self.store.encode_diff(state_vector, encoder)
    }

    /// Returns a [Text] data structure stored under a given `name`. Text structures are used for
    /// collaborative text editing: they expose operations to append and remove chunks of text,
    /// which are free to execute concurrently by multiple peers over remote boundaries.
    ///
    /// If not structure under defined `name` existed before, it will be created and returned
    /// instead.
    ///
    /// If a structure under defined `name` already existed, but its type was different it will be
    /// reinterpreted as a text (in such case a sequence component of complex data type will be
    /// interpreted as a list of text chunks).
    pub fn get_text(&mut self, name: &str) -> Text {
        let c = self.store.create_type(name, None, TYPE_REFS_TEXT);
        Text::from(c)
    }

    /// Returns a [Map] data structure stored under a given `name`. Maps are used to store key-value
    /// pairs associated together. These values can be primitive data (similar but not limited to
    /// a JavaScript Object Notation) as well as other shared types (Yrs maps, arrays, text
    /// structures etc.), enabling to construct a complex recursive tree structures.
    ///
    /// If not structure under defined `name` existed before, it will be created and returned
    /// instead.
    ///
    /// If a structure under defined `name` already existed, but its type was different it will be
    /// reinterpreted as a map (in such case a map component of complex data type will be
    /// interpreted as native map).
    pub fn get_map(&mut self, name: &str) -> Map {
        let c = self.store.create_type(name, None, TYPE_REFS_MAP);
        Map::from(c)
    }

    /// Returns an [Array] data structure stored under a given `name`. Array structures are used for
    /// storing a sequences of elements in ordered manner, positioning given element accordingly
    /// to its index.
    ///
    /// If not structure under defined `name` existed before, it will be created and returned
    /// instead.
    ///
    /// If a structure under defined `name` already existed, but its type was different it will be
    /// reinterpreted as an array (in such case a sequence component of complex data type will be
    /// interpreted as a list of inserted values).
    pub fn get_array(&mut self, name: &str) -> Array {
        let c = self.store.create_type(name, None, TYPE_REFS_ARRAY);
        Array::from(c)
    }

    /// Returns a [XmlElement] data structure stored under a given `name`. XML elements represent
    /// nodes of XML document. They can contain attributes (key-value pairs, both of string type)
    /// as well as other nested XML elements or text values, which are stored in their insertion
    /// order.
    ///
    /// If not structure under defined `name` existed before, it will be created and returned
    /// instead.
    ///
    /// If a structure under defined `name` already existed, but its type was different it will be
    /// reinterpreted as a XML element (in such case a map component of complex data type will be
    /// interpreted as map of its attributes, while a sequence component - as a list of its child
    /// XML nodes).
    pub fn get_xml_element(&mut self, name: &str) -> XmlElement {
        let c = self
            .store
            .create_type(name, Some("UNDEFINED".to_string()), TYPE_REFS_XML_ELEMENT);
        XmlElement::from(c)
    }

    /// Returns a [XmlText] data structure stored under a given `name`. Text structures are used for
    /// collaborative text editing: they expose operations to append and remove chunks of text,
    /// which are free to execute concurrently by multiple peers over remote boundaries.
    ///
    /// If not structure under defined `name` existed before, it will be created and returned
    /// instead.
    ///
    /// If a structure under defined `name` already existed, but its type was different it will be
    /// reinterpreted as a text (in such case a sequence component of complex data type will be
    /// interpreted as a list of text chunks).
    pub fn get_xml_text(&mut self, name: &str) -> XmlText {
        let c = self.store.create_type(name, None, TYPE_REFS_XML_TEXT);
        XmlText::from(c)
    }

    /// Encodes the document state to a binary format.
    ///
    /// Document updates are idempotent and commutative. Caveats:
    /// * It doesn't matter in which order document updates are applied.
    /// * As long as all clients receive the same document updates, all clients
    ///   end up with the same content.
    /// * Even if an update contains known information, the unknown information
    ///   is extracted and integrated into the document structure.
    pub fn encode_update_v1(&self) -> Vec<u8> {
        let mut update_encoder = updates::encoder::EncoderV1::new();
        self.store
            .encode_diff(&self.before_state, &mut update_encoder);
        update_encoder.to_vec()
    }

    pub(crate) fn iterate_structs<F>(&mut self, client: &u64, range: &Range<u32>, f: &F)
    where
        F: Fn(&Block) -> (),
    {
        let clock_start = range.start;
        let clock_end = range.end;

        if clock_start == clock_end {
            return;
        }

        if let Some(mut index) = self.find_index_clean_start(client, clock_start) {
            let mut blocks = self.store.blocks.get(client).unwrap();
            let mut block = &blocks[index];

            while index < blocks.len() && block.id().clock < clock_end {
                if clock_end < block.clock_end() {
                    self.find_index_clean_start(client, clock_start);
                    blocks = self.store.blocks.get(client).unwrap();
                    block = &blocks[index];
                }

                f(block);
                index += 1;

                block = &blocks[index];
            }
        }
    }

    pub(crate) fn find_index_clean_start(&mut self, client: &u64, clock: u32) -> Option<usize> {
        let blocks = self.store.blocks.get_mut(client)?;
        let index = blocks.find_pivot(clock)?;
        let block = &mut blocks[index];
        if let Some(item) = block.as_item_mut() {
            if item.id.clock < clock {
                // if we run over the clock, we need to the split item
                let id = ID::new(*client, clock - item.id.clock);
                self.store
                    .blocks
                    .split_block(&BlockPtr::new(id, index as u32));
                return Some(index + 1);
            }
        }

        Some(index)
    }

    pub(crate) fn apply_ranges<F>(&mut self, set: &IdSet, f: &F)
    where
        F: Fn(&Block) -> (),
    {
        // equivalent of JS: Y.iterateDeletedStructs
        for (client, ranges) in set.iter() {
            if self.store.blocks.contains_client(client) {
                for range in ranges.iter() {
                    self.iterate_structs(client, range, f);
                }
            }
        }
    }

    /// Applies given `id_set` onto current transaction to run multi-range deletion.
    /// Returns a remaining of original ID set, that couldn't be applied.
    pub(crate) fn apply_delete(&mut self, ds: &DeleteSet) -> Option<DeleteSet> {
        let mut unapplied = DeleteSet::new();
        for (client, ranges) in ds.iter() {
            let mut blocks = self.store.blocks.get_mut(client).unwrap();
            let state = blocks.get_state();

            for range in ranges.iter() {
                let clock = range.start;
                let clock_end = range.end;

                if clock < state {
                    if state < clock_end {
                        unapplied.insert(ID::new(*client, clock), clock_end - state);
                    }
                    // We can ignore the case of GC and Delete structs, because we are going to skip them
                    if let Some(mut index) = blocks.find_pivot(clock) {
                        // We can ignore the case of GC and Delete structs, because we are going to skip them
                        if let Some(item) = blocks[index].as_item_mut() {
                            // split the first item if necessary
                            if !item.is_deleted() && item.id.clock < clock {
                                let split_ptr = BlockPtr::new(
                                    ID::new(*client, clock - item.id.clock),
                                    index as u32,
                                );
                                let (_, right) = self.store.blocks.split_block(&split_ptr);
                                if let Some(right) = right {
                                    index += 1;
                                    self.merge_blocks.push(right.id);
                                }
                                blocks = self.store.blocks.get_mut(client).unwrap();
                            }

                            while index < blocks.len() {
                                let block = &mut blocks[index];
                                if let Some(item) = block.as_item_mut() {
                                    if item.id.clock < clock_end {
                                        if !item.is_deleted() {
                                            let delete_ptr =
                                                BlockPtr::new(item.id.clone(), index as u32);
                                            if item.id.clock + item.len() > clock_end {
                                                let diff = clock_end - item.id.clock;
                                                let mut split_ptr = delete_ptr.clone();
                                                split_ptr.id.clock += diff;
                                                let (_, right) =
                                                    self.store.blocks.split_block(&split_ptr);
                                                if let Some(right) = right {
                                                    self.merge_blocks.push(right.id);
                                                    index += 1;
                                                }
                                            }
                                            self.delete(&delete_ptr);
                                            blocks = self.store.blocks.get_mut(client).unwrap();
                                            // just to make the borrow checker happy
                                        }
                                    } else {
                                        break;
                                    }
                                }
                                index += 1;
                            }
                        }
                    }
                } else {
                    unapplied.insert(ID::new(*client, clock), clock_end - clock);
                }
            }
        }

        if unapplied.is_empty() {
            None
        } else {
            Some(unapplied)
        }
    }

    /// Delete item under given pointer.
    /// Returns true if block was successfully deleted, false if it was already deleted in the past.
    pub(crate) fn delete(&mut self, ptr: &BlockPtr) -> bool {
        let mut recurse = Vec::new();
        let mut result = false;

        if let Some(item) = self.store.blocks.get_item(&ptr) {
            if !item.is_deleted() {
                if item.parent_sub.is_none() && item.is_countable() {
                    if let Some(parent) = self.store.get_type(&item.parent) {
                        let mut inner = parent.borrow_mut();
                        inner.len -= item.len();
                    }
                }

                item.mark_as_deleted();
                self.delete_set.insert(item.id.clone(), item.len());
                // addChangedTypeToTransaction(transaction, item.type, item.parentSub)
                if item.id.clock < self.before_state.get(&item.id.client) {
                    let set = self.changed.entry(item.parent.clone()).or_default();
                    set.insert(item.parent_sub.clone());
                }

                match &item.content {
                    ItemContent::Doc(_, _) => {
                        //if (transaction.subdocsAdded.has(this.doc)) {
                        //    transaction.subdocsAdded.delete(this.doc)
                        //} else {
                        //    transaction.subdocsRemoved.add(this.doc)
                        //}
                        todo!()
                    }
                    ItemContent::Type(t) => {
                        let inner = t.borrow_mut();
                        let mut ptr = inner.start;
                        self.changed.remove(&item.parent);

                        while let Some(item) = ptr.and_then(|ptr| self.store.blocks.get_item(&ptr))
                        {
                            if !item.is_deleted() {
                                recurse.push(ptr.unwrap());
                            }

                            ptr = item.right.clone();
                        }

                        for ptr in inner.map.values() {
                            recurse.push(ptr.clone());
                        }
                    }
                    _ => {
                        // nothing to do for other content types
                    }
                }
                result = true;
            }
        }

        for ptr in recurse.iter() {
            if !self.delete(ptr) {
                self.merge_blocks.push(ptr.id);
            }
        }

        result
    }

    pub fn apply_update(&mut self, mut update: Update, mut ds: DeleteSet) {
        if self.store.update_events.has_subscribers() {
            let event = UpdateEvent::new(update, ds);
            self.store.update_events.publish(&event);
            update = event.update;
            ds = event.delete_set;
        }
        let remaining = update.integrate(self);

        let mut retry = false;
        if let Some(mut pending) = self.store.pending.take() {
            // check if we can apply something
            for (client, &clock) in pending.missing.iter() {
                if clock < self.store.blocks.get_state(client) {
                    retry = true;
                    break;
                }
            }

            if let Some(remaining) = remaining {
                // merge restStructs into store.pending
                for (&client, &clock) in remaining.missing.iter() {
                    pending.missing.set_min(client, clock);
                }
                pending.update.merge(remaining.update);
                self.store.pending = Some(pending);
            }
        } else {
            self.store.pending = remaining;
        }

        let ds = self.apply_delete(&ds);
        if let Some(pending) = self.store.pending_ds.take() {
            let ds2 = self.apply_delete(&pending);
            let ds = match (ds, ds2) {
                (Some(mut a), Some(b)) => {
                    a.merge(b);
                    Some(a)
                }
                (Some(x), _) | (_, Some(x)) => Some(x),
                _ => None,
            };
            self.store.pending_ds = ds;
        } else {
            self.store.pending_ds = ds;
        }

        if retry {
            if let Some(pending) = self.store.pending.take() {
                let ds = self.store.pending_ds.take().unwrap_or_default();
                self.apply_update(pending.update, ds);
            }
        }
    }

    pub(crate) fn create_item<T: Prelim>(
        &mut self,
        pos: &block::ItemPosition,
        value: T,
        parent_sub: Option<String>,
    ) -> &Item {
        let left = pos.left;
        let right = pos.right;
        let origin = if let Some(ptr) = pos.left.as_ref() {
            if let Some(item) = self.store.blocks.get_item(ptr) {
                Some(item.last_id())
            } else {
                None
            }
        } else {
            None
        };
        let client_id = self.store.client_id;
        let id = block::ID {
            client: client_id,
            clock: self.store.get_local_state(),
        };
        let pivot = self
            .store
            .blocks
            .get_client_blocks_mut(client_id)
            .integrated_len() as u32;

        let ptr = BlockPtr::new(id, pivot);
        let (content, remainder) = value.into_content(self, TypePtr::Id(ptr));
        let inner_ref = if let ItemContent::Type(inner_ref) = &content {
            Some(inner_ref.clone())
        } else {
            None
        };
        let mut item = Item::new(
            id,
            left,
            origin,
            right,
            right.map(|r| r.id),
            pos.parent.clone(),
            parent_sub,
            content,
        );
        item.integrate(self, pivot, 0);
        let local_block_list = self.store.blocks.get_client_blocks_mut(client_id);
        local_block_list.push(block::Block::Item(item));
        let idx = local_block_list.len() - 1;

        if let Some(remainder) = remainder {
            remainder.integrate(self, inner_ref.unwrap())
        }

        self.store.blocks.get_client_blocks_mut(client_id)[idx]
            .as_item()
            .unwrap()
    }

    /// Commits current transaction. This step involves cleaning up and optimizing changes performed
    /// during lifetime of a transaction. Such changes include squashing delete sets data
    /// or squashing blocks that have been appended one after another to preserve memory.
    ///
    /// This step is performed automatically when a transaction is about to be dropped (its life
    /// scope comes to an end).
    pub fn commit(&mut self) {
        // 1. sort and merge delete set
        self.delete_set.squash();
        self.after_state = self.store.blocks.get_state_vector();

        // 2. emit 'beforeObserverCalls'
        // 3. for each change observed by the transaction call 'afterTransaction'
        // 4. try GC delete set
        self.try_gc(); //TODO: eventually this is a configurable variant: if (doc.gc)

        // 5. try merge delete set
        self.delete_set.try_compact(&self.store.blocks);

        // 6. get transaction after state and try to merge to left
        for (client, &clock) in self.after_state.iter() {
            let before_clock = self.before_state.get(client);
            if before_clock != clock {
                let mut blocks = self.store.blocks.get_mut(client).unwrap();
                let first_change = blocks.find_pivot(before_clock).unwrap().max(1);
                let mut i = blocks.len() - 1;
                while i >= first_change {
                    if let Some(compaction) = blocks.squash_left(i) {
                        self.store.gc_cleanup(compaction);
                        blocks = self.store.blocks.get_mut(client).unwrap();
                    }
                    i -= 1;
                }
            }
        }
        // 7. get merge_structs and try to merge to left
        for id in self.merge_blocks.iter() {
            let client = id.client;
            let clock = id.clock;
            let blocks = self.store.blocks.get_mut(&client).unwrap();
            let replaced_pos = blocks.find_pivot(clock).unwrap();
            if replaced_pos + 1 < blocks.len() {
                if let Some(compaction) = blocks.squash_left(replaced_pos + 1) {
                    self.store.gc_cleanup(compaction);
                }
            } else if replaced_pos > 0 {
                if let Some(compaction) = blocks.squash_left(replaced_pos) {
                    self.store.gc_cleanup(compaction);
                }
            }
        }
        // 8. emit 'afterTransactionCleanup'
        // 9. emit 'update'
        // 10. emit 'updateV2'
        // 11. add and remove subdocs
        // 12. emit 'subdocs'
    }

    fn try_gc(&mut self) {
        for (client, range) in self.delete_set.iter() {
            if let Some(blocks) = self.store.blocks.get_mut(client) {
                for delete_item in range.iter().rev() {
                    let mut start = delete_item.start;
                    if let Some(mut i) = blocks.find_pivot(start) {
                        while i < blocks.len() {
                            let block = &mut blocks[i];
                            let len = block.len();
                            start += len;
                            if start > delete_item.end {
                                break;
                            } else {
                                if let Block::Item(item) = block {
                                    if item.is_deleted() {
                                        if let ItemContent::Type(t) = &item.content {
                                            /*
                                            let item = this.type._start
                                            while (item !== null) {
                                              item.gc(store, true)
                                              item = item.right
                                            }
                                            this.type._start = null
                                            this.type._map.forEach(/** @param {Item | null} item */ (item) => {
                                              while (item !== null) {
                                                item.gc(store, true)
                                                item = item.left
                                              }
                                            })
                                            this.type._map = new Map()
                                            */
                                            todo!()
                                        }

                                        item.content = ItemContent::Deleted(len);
                                    }
                                }
                                i += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn add_changed_type(&mut self, parent: &mut Branch, parent_sub: Option<&String>) {
        // TODO:
        /*
              const item = type._item
              if (item === null || (item.id.clock < (transaction.beforeState.get(item.id.client) || 0) && !item.deleted)) {
                map.setIfUndefined(transaction.changed, type, set.create).add(parentSub)
              }
        */
    }
}

impl<'a> Drop for Transaction<'a> {
    fn drop(&mut self) {
        self.commit()
    }
}
