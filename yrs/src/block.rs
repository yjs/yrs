use crate::branch::{Branch, BranchPtr};
use crate::doc::{DocAddr, OffsetKind};
use crate::encoding::read::Error;
use crate::error::UpdateError;
use crate::gc::GCCollector;
use crate::moving::Move;
use crate::slice::{BlockSlice, GCSlice, ItemSlice};
use crate::store::Store;
use crate::transaction::TransactionMut;
use crate::types::text::update_current_attributes;
use crate::types::{Attrs, TypePtr, TypeRef};
use crate::undo::UndoStack;
use crate::updates::decoder::{Decode, Decoder};
use crate::updates::encoder::{Encode, Encoder};
use crate::utils::OptionExt;
use crate::{Any, DeleteSet, Doc, Options, Out, Transact};
use serde::{Deserialize, Serialize};
use smallstr::SmallString;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::fmt::Formatter;
use std::hash::Hash;
use std::ops::{Deref, DerefMut};
use std::panic;
use std::ptr::NonNull;
use std::sync::Arc;

/// Bit flag used to identify [Item::GC].
pub const BLOCK_GC_REF_NUMBER: u8 = 0;

/// Bit flag used to identify items with content of type [ItemContent::Deleted].
pub const BLOCK_ITEM_DELETED_REF_NUMBER: u8 = 1;

/// Bit flag used to identify items with content of type [ItemContent::JSON].
pub const BLOCK_ITEM_JSON_REF_NUMBER: u8 = 2;

/// Bit flag used to identify items with content of type [ItemContent::Binary].
pub const BLOCK_ITEM_BINARY_REF_NUMBER: u8 = 3;

/// Bit flag used to identify items with content of type [ItemContent::String].
pub const BLOCK_ITEM_STRING_REF_NUMBER: u8 = 4;

/// Bit flag used to identify items with content of type [ItemContent::Embed].
pub const BLOCK_ITEM_EMBED_REF_NUMBER: u8 = 5;

/// Bit flag used to identify items with content of type [ItemContent::Format].
pub const BLOCK_ITEM_FORMAT_REF_NUMBER: u8 = 6;

/// Bit flag used to identify items with content of type [ItemContent::Number].
pub const BLOCK_ITEM_TYPE_REF_NUMBER: u8 = 7;

/// Bit flag used to identify items with content of type [ItemContent::Any].
pub const BLOCK_ITEM_ANY_REF_NUMBER: u8 = 8;

/// Bit flag used to identify items with content of type [ItemContent::Doc].
pub const BLOCK_ITEM_DOC_REF_NUMBER: u8 = 9;

/// Bit flag used to identify [Item::Skip].
pub const BLOCK_SKIP_REF_NUMBER: u8 = 10;

/// Bit flag used to identify items with content of type [ItemContent::Move].
pub const BLOCK_ITEM_MOVE_REF_NUMBER: u8 = 11;

/// Bit flag used to tell if encoded item has right origin defined.
pub const HAS_RIGHT_ORIGIN: u8 = 0b01000000;

/// Bit flag used to tell if encoded item has left origin defined.
pub const HAS_ORIGIN: u8 = 0b10000000;

/// Bit flag used to tell if encoded item has a parent subtitle defined. Subtitles are used only
/// for blocks which act as map-like types entries.
pub const HAS_PARENT_SUB: u8 = 0b00100000;

/// Globally unique client identifier. No two active peers are allowed to share the same [ClientID].
/// If that happens, following updates may cause document store to be corrupted and desync in a result.
pub type ClientID = u64;

/// Block identifier, which allows to uniquely identify any element insertion in a global scope
/// (across different replicas of the same document). It consists of client ID (which is a unique
/// document replica identifier) and monotonically incrementing clock value.
///
/// [ID] corresponds to a [Lamport timestamp](https://en.wikipedia.org/wiki/Lamport_timestamp) in
/// terms of its properties and guarantees.
#[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ID {
    /// Unique identifier of a client.
    pub client: ClientID,

    /// Monotonically incrementing sequence number, which informs about order of inserted item
    /// operation in a scope of a given `client`. This value doesn't have to increase by 1, but
    /// instead is increased by number of countable elements which make a content of an inserted
    /// block.
    pub clock: u32,
}

impl ID {
    pub fn new(client: ClientID, clock: u32) -> Self {
        ID { client, clock }
    }
}

pub(crate) enum BlockCell {
    GC(GC),
    Block(Box<Item>),
}

impl PartialEq for BlockCell {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (BlockCell::GC(a), BlockCell::GC(b)) => a == b,
            (BlockCell::Block(a), BlockCell::Block(b)) => a.id == b.id,
            _ => false,
        }
    }
}

impl std::fmt::Debug for BlockCell {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockCell::GC(gc) => write!(f, "gc({}..={})", gc.start, gc.end),
            BlockCell::Block(item) => item.fmt(f),
        }
    }
}

impl BlockCell {
    pub fn clock_start(&self) -> u32 {
        match self {
            BlockCell::GC(gc) => gc.start,
            BlockCell::Block(item) => item.id.clock,
        }
    }

    pub fn clock_end(&self) -> u32 {
        match self {
            BlockCell::GC(gc) => gc.end,
            BlockCell::Block(item) => item.id.clock + item.len - 1,
        }
    }

    /// Returns a range of first and the last clock sequence numbers that belong to a current block.
    #[inline]
    pub fn clock_range(&self) -> (u32, u32) {
        match self {
            BlockCell::GC(gc) => (gc.start, gc.end),
            BlockCell::Block(block) => block.clock_range(),
        }
    }

    pub fn is_deleted(&self) -> bool {
        match self {
            BlockCell::GC(_) => true,
            BlockCell::Block(item) => item.is_deleted(),
        }
    }

    pub fn len(&self) -> u32 {
        match self {
            BlockCell::GC(gc) => gc.end - gc.start + 1,
            BlockCell::Block(block) => block.len(),
        }
    }

    pub fn as_slice(&self) -> BlockSlice {
        match self {
            BlockCell::GC(gc) => BlockSlice::GC(GCSlice::from(gc.clone())),
            BlockCell::Block(item) => {
                let ptr = ItemPtr::from(item);
                BlockSlice::Item(ItemSlice::from(ptr))
            }
        }
    }

    pub fn as_item(&self) -> Option<ItemPtr> {
        if let BlockCell::Block(item) = self {
            Some(ItemPtr::from(item))
        } else {
            None
        }
    }
}

impl From<Box<Item>> for BlockCell {
    fn from(value: Box<Item>) -> Self {
        BlockCell::Block(value)
    }
}

impl From<GC> for BlockCell {
    fn from(gc: GC) -> Self {
        BlockCell::GC(gc)
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) struct GC {
    pub start: u32,
    pub end: u32,
}

impl GC {
    #[inline]
    pub fn new(start: u32, end: u32) -> Self {
        GC { start, end }
    }

    pub fn len(&self) -> u32 {
        self.end - self.start + 1
    }
}

impl From<BlockRange> for GC {
    fn from(value: BlockRange) -> Self {
        let start = value.id.clock;
        let end = start + value.len - 1;
        GC { start, end }
    }
}

impl Encode for GC {
    fn encode<E: Encoder>(&self, encoder: &mut E) {
        encoder.write_info(BLOCK_GC_REF_NUMBER);
        encoder.write_len(self.len());
    }
}

/// A raw [Item] pointer. As the underlying block doesn't move it's in-memory location, [ItemPtr]
/// can be considered a pinned object.
#[repr(transparent)]
#[derive(Clone, Copy, Hash)]
pub struct ItemPtr(NonNull<Item>);

unsafe impl Send for ItemPtr {}
unsafe impl Sync for ItemPtr {}

impl ItemPtr {
    pub(crate) fn redo<M>(
        &mut self,
        txn: &mut TransactionMut,
        redo_items: &HashSet<ItemPtr>,
        items_to_delete: &DeleteSet,
        s1: &UndoStack<M>,
        s2: &UndoStack<M>,
    ) -> Option<ItemPtr> {
        let self_ptr = self.clone();
        let item = self.deref_mut();
        if let Some(redone) = item.redone.as_ref() {
            let slice = txn.store.blocks.get_item_clean_start(redone)?;
            return Some(txn.store.materialize(slice));
        }

        let mut parent_block = item.parent.as_branch().and_then(|b| b.item);
        // make sure that parent is redone
        if let Some(mut parent) = parent_block.clone() {
            if parent.is_deleted() {
                // try to undo parent if it will be undone anyway
                if parent.redone.is_none()
                    && (!redo_items.contains(&parent)
                        || parent
                            .redo(txn, redo_items, items_to_delete, s1, s2)
                            .is_none())
                {
                    return None;
                }
                let mut redone = parent.redone;
                while let Some(id) = redone.as_ref() {
                    parent_block = txn
                        .store
                        .blocks
                        .get_item_clean_start(id)
                        .map(|slice| txn.store.materialize(slice));
                    redone = parent_block.and_then(|ptr| ptr.redone);
                }
            }
        }
        let parent_branch = BranchPtr::from(if let Some(item) = parent_block.as_deref() {
            if let ItemContent::Type(b) = &item.content {
                b.as_ref()
            } else {
                item.parent.as_branch().unwrap()
            }
        } else {
            item.parent.as_branch().unwrap()
        });

        let mut left = None;
        let mut right = None;
        if let Some(sub) = item.parent_sub.as_ref() {
            if item.right.is_some() {
                left = Some(self_ptr);
                // Iterate right while is in itemsToDelete
                // If it is intended to delete right while item is redone,
                // we can expect that item should replace right.
                while let Some(left_item) = left.as_deref() {
                    if let Some(left_right) = left_item.right {
                        let id = left_right.id();
                        if left_right.redone.is_some()
                            || items_to_delete.is_deleted(id)
                            || s1.is_deleted(id)
                            || s2.is_deleted(id)
                        {
                            // follow redone
                            left = Some(left_right);
                            while let Some(item) = left.as_deref() {
                                if let Some(id) = item.redone.as_ref() {
                                    left = match txn.store.blocks.get_item_clean_start(id) {
                                        None => break,
                                        Some(slice) => {
                                            let ptr = txn.store.materialize(slice);
                                            txn.merge_blocks.push(ptr.id().clone());
                                            Some(ptr)
                                        }
                                    };
                                } else {
                                    break;
                                }
                            }
                            continue;
                        }
                    }
                    break;
                }

                if let Some(left_item) = left.as_deref() {
                    if left_item.right.is_some() {
                        // It is not possible to redo this item because it conflicts with a
                        // change from another client
                        return None;
                    }
                }
            } else {
                left = parent_branch.map.get(sub).cloned();
            }
        } else {
            // Is an array item. Insert at the old position
            left = item.left;
            right = Some(self_ptr);
            // find next cloned_redo items
            while let Some(left_item) = left.clone().as_deref() {
                let mut left_trace = left;
                while let Some(trace) = left_trace.as_deref() {
                    let p = trace.parent.as_branch().and_then(|p| p.item);
                    if parent_block != p {
                        left_trace = if let Some(redone) = trace.redone.as_ref() {
                            let slice = txn.store.blocks.get_item_clean_start(redone);
                            slice.map(|s| txn.store.materialize(s))
                        } else {
                            None
                        };
                    } else {
                        break;
                    }
                }
                if let Some(trace) = left_trace.as_deref() {
                    let p = trace.parent.as_branch().and_then(|p| p.item);
                    if parent_block == p {
                        left = left_trace;
                        break;
                    }
                }
                left = left_item.left.clone();
            }

            while let Some(right_item) = right.clone().as_deref() {
                let mut right_trace = right;
                // trace redone until parent matches
                while let Some(trace) = right_trace.as_deref() {
                    let p = trace.parent.as_branch().and_then(|p| p.item);
                    if parent_block != p {
                        right_trace = if let Some(redone) = trace.redone.as_ref() {
                            let slice = txn.store.blocks.get_item_clean_start(redone);
                            slice.map(|s| txn.store.materialize(s))
                        } else {
                            None
                        };
                    } else {
                        break;
                    }
                }
                if let Some(trace) = right_trace.as_deref() {
                    let p = trace.parent.as_branch().and_then(|p| p.item);
                    if parent_block == p {
                        right = right_trace;
                        break;
                    }
                }
                right = right_item.right.clone();
            }
        }

        let next_clock = txn.store.get_local_state();
        let next_id = ID::new(txn.store.client_id, next_clock);
        let mut redone_item = Item::new(
            next_id,
            left,
            left.map(|p| p.last_id()),
            right,
            right.map(|p| *p.id()),
            TypePtr::Branch(parent_branch),
            item.parent_sub.clone(),
            item.content.clone(),
        )?;
        item.redone = Some(*redone_item.id());
        redone_item.info.set_keep();
        let mut block_ptr = ItemPtr::from(&mut redone_item);

        block_ptr.integrate(txn, 0);

        txn.store_mut().blocks.push_block(redone_item);
        Some(block_ptr)
    }

    pub(crate) fn keep(&self, keep: bool) {
        let mut curr = Some(*self);
        while let Some(item) = curr.as_deref_mut() {
            if item.info.is_keep() == keep {
                break;
            } else {
                if keep {
                    item.info.set_keep();
                } else {
                    item.info.clear_keep();
                }
                curr = item.parent.as_branch().and_then(|b| b.item);
            }
        }
    }

    pub(crate) fn delete_as_cleanup(&self, txn: &mut TransactionMut, is_local: bool) {
        txn.delete(*self);
        if is_local {
            txn.delete_set.insert(*self.id(), self.len());
        }
    }

    pub(crate) fn splice(&mut self, offset: u32, encoding: OffsetKind) -> Option<Box<Item>> {
        let self_ptr = self.clone();
        if offset == 0 {
            None
        } else {
            let item = self.deref_mut();
            let client = item.id.client;
            let clock = item.id.clock;
            let content = item.content.splice(offset as usize, encoding).unwrap();
            item.len = offset;
            let mut new = Box::new(Item {
                id: ID::new(client, clock + offset),
                len: content.len(OffsetKind::Utf16),
                left: Some(self_ptr),
                right: item.right.clone(),
                origin: Some(ID::new(client, clock + offset - 1)),
                right_origin: item.right_origin.clone(),
                content,
                parent: item.parent.clone(),
                moved: item.moved.clone(),
                parent_sub: item.parent_sub.clone(),
                info: item.info.clone(),
                redone: item.redone.map(|id| ID::new(id.client, id.clock + offset)),
            });
            let new_ptr = ItemPtr::from(&mut new);

            if let Some(right) = item.right.as_deref_mut() {
                right.left = Some(new_ptr);
            }

            if let Some(parent_sub) = item.parent_sub.as_ref() {
                if item.right.is_none() {
                    // update parent.map
                    if let TypePtr::Branch(mut branch) = item.parent {
                        branch.map.insert(parent_sub.clone(), new_ptr);
                    }
                }
            }

            item.right = Some(new_ptr);

            Some(new)
        }
    }

    /// Integrates current block into block store.
    /// If it returns true, it means that the block should be deleted after being added to a block store.
    pub(crate) fn integrate(&mut self, txn: &mut TransactionMut, offset: u32) -> bool {
        let self_ptr = self.clone();
        let this = self.deref_mut();
        let store = txn.store_mut();
        let encoding = store.offset_kind;
        if offset > 0 {
            // offset could be > 0 only in context of Update::integrate,
            // is such case offset kind in use always means Yjs-compatible offset (utf-16)
            this.id.clock += offset;
            this.left = store
                .blocks
                .get_item_clean_end(&ID::new(this.id.client, this.id.clock - 1))
                .map(|slice| store.materialize(slice));
            this.origin = this.left.as_deref().map(|b: &Item| b.last_id());
            this.content = this
                .content
                .splice(offset as usize, OffsetKind::Utf16)
                .unwrap();
            this.len -= offset;
        }

        let parent = match &this.parent {
            TypePtr::Branch(branch) => Some(*branch),
            TypePtr::Named(name) => {
                let branch = store.get_or_create_type(name.clone(), TypeRef::Undefined);
                this.parent = TypePtr::Branch(branch);
                Some(branch)
            }
            TypePtr::ID(id) => {
                if let Some(item) = store.blocks.get_item(id) {
                    if let Some(branch) = item.as_branch() {
                        this.parent = TypePtr::Branch(branch);
                        Some(branch)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            TypePtr::Unknown => return true,
        };

        let left: Option<&Item> = this.left.as_deref();
        let right: Option<&Item> = this.right.as_deref();

        let right_is_null_or_has_left = match right {
            None => true,
            Some(i) => i.left.is_some(),
        };
        let left_has_other_right_than_self = match left {
            Some(i) => i.right != this.right,
            _ => false,
        };

        if let Some(mut parent_ref) = parent {
            if (left.is_none() && right_is_null_or_has_left) || left_has_other_right_than_self {
                // set the first conflicting item
                let mut o = if let Some(left) = left {
                    left.right
                } else if let Some(sub) = &this.parent_sub {
                    let mut o = parent_ref.map.get(sub).cloned();
                    while let Some(item) = o.as_deref() {
                        if item.left.is_some() {
                            o = item.left.clone();
                            continue;
                        }
                        break;
                    }
                    o.clone()
                } else {
                    parent_ref.start
                };

                let mut left = this.left.clone();
                let mut conflicting_items = HashSet::new();
                let mut items_before_origin = HashSet::new();

                // Let c in conflicting_items, b in items_before_origin
                // ***{origin}bbbb{this}{c,b}{c,b}{o}***
                // Note that conflicting_items is a subset of items_before_origin
                while let Some(item) = o {
                    if Some(item) == this.right {
                        break;
                    }

                    items_before_origin.insert(item);
                    conflicting_items.insert(item);
                    if this.origin == item.origin {
                        // case 1
                        if item.id.client < this.id.client {
                            left = Some(item.clone());
                            conflicting_items.clear();
                        } else if this.right_origin == item.right_origin {
                            // `self` and `item` are conflicting and point to the same integration
                            // points. The id decides which item comes first. Since `self` is to
                            // the left of `item`, we can break here.
                            break;
                        }
                    } else {
                        if let Some(origin_ptr) = item
                            .origin
                            .as_ref()
                            .and_then(|id| store.blocks.get_item(id))
                        {
                            if items_before_origin.contains(&origin_ptr) {
                                if !conflicting_items.contains(&origin_ptr) {
                                    left = Some(item.clone());
                                    conflicting_items.clear();
                                }
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    o = item.right.clone();
                }
                this.left = left;
            }

            if this.parent_sub.is_none() {
                if let Some(item) = this.left.as_deref() {
                    if item.parent_sub.is_some() {
                        this.parent_sub = item.parent_sub.clone();
                    } else if let Some(item) = this.right.as_deref() {
                        this.parent_sub = item.parent_sub.clone();
                    }
                }
            }

            // reconnect left/right
            if let Some(left) = this.left.as_deref_mut() {
                this.right = left.right.replace(self_ptr);
            } else {
                let r = if let Some(parent_sub) = &this.parent_sub {
                    // update parent map/start if necessary
                    let mut r = parent_ref.map.get(parent_sub).cloned();
                    while let Some(item) = r {
                        if item.left.is_some() {
                            r = item.left;
                        } else {
                            break;
                        }
                    }
                    r
                } else {
                    let start = parent_ref.start.replace(self_ptr);
                    start
                };
                this.right = r;
            }

            if let Some(right) = this.right.as_deref_mut() {
                right.left = Some(self_ptr);
            } else if let Some(parent_sub) = &this.parent_sub {
                // set as current parent value if right === null and this is parentSub
                parent_ref.map.insert(parent_sub.clone(), self_ptr);
                if let Some(mut left) = this.left {
                    #[cfg(feature = "weak")]
                    {
                        if left.info.is_linked() {
                            // inherit links from the block we're overriding
                            left.info.clear_linked();
                            this.info.set_linked();
                            let all_links = &mut txn.store.linked_by;
                            if let Some(linked_by) = all_links.remove(&left) {
                                all_links.insert(self_ptr, linked_by);
                                // since left is being deleted, it will remove
                                // its links from store.linkedBy anyway
                            }
                        }
                    }
                    // this is the current attribute value of parent. delete right
                    txn.delete(left);
                }
            }

            // adjust length of parent
            if this.parent_sub.is_none() && !this.is_deleted() {
                if this.is_countable() {
                    // adjust length of parent
                    parent_ref.block_len += this.len;
                    parent_ref.content_len += this.content_len(encoding);
                }
                #[cfg(feature = "weak")]
                match (this.left, this.right) {
                    (Some(l), Some(r)) if l.info.is_linked() || r.info.is_linked() => {
                        crate::types::weak::join_linked_range(self_ptr, txn)
                    }
                    _ => {}
                }
            }

            // check if this item is in a moved range
            let left_moved = this.left.and_then(|i| i.moved);
            let right_moved = this.right.and_then(|i| i.moved);
            if left_moved.is_some() || right_moved.is_some() {
                if left_moved == right_moved {
                    this.moved = left_moved;
                } else {
                    #[inline]
                    fn try_integrate(mut item: ItemPtr, txn: &mut TransactionMut) {
                        let ptr = item.clone();
                        if let ItemContent::Move(m) = &mut item.content {
                            if !m.is_collapsed() {
                                m.integrate_block(txn, ptr);
                            }
                        }
                    }

                    if let Some(ptr) = left_moved {
                        try_integrate(ptr, txn);
                    }

                    if let Some(ptr) = right_moved {
                        try_integrate(ptr, txn);
                    }
                }
            }

            match &mut this.content {
                ItemContent::Deleted(len) => {
                    txn.delete_set.insert(this.id, *len);
                    this.mark_as_deleted();
                }
                ItemContent::Move(m) => m.integrate_block(txn, self_ptr),
                ItemContent::Doc(parent_doc, doc) => {
                    *parent_doc = Some(txn.doc().clone());
                    {
                        let mut child_txn = doc.transact_mut();
                        child_txn.store.parent = Some(self_ptr);
                    }
                    let subdocs = txn.subdocs.get_or_init();
                    subdocs.added.insert(DocAddr::new(doc), doc.clone());
                    if doc.should_load() {
                        subdocs.loaded.insert(doc.addr(), doc.clone());
                    }
                }
                ItemContent::Format(_, _) => {
                    // @todo searchmarker are currently unsupported for rich text documents
                    // /** @type {AbstractType<any>} */ (item.parent)._searchMarker = null
                }
                ItemContent::Type(branch) => {
                    let ptr = if this.info.is_deleted() {
                        BranchPtr::from(branch)
                    } else {
                        // if current node is alive register is as such
                        txn.store.register(branch)
                    };
                    #[cfg(feature = "weak")]
                    if let TypeRef::WeakLink(source) = &ptr.type_ref {
                        source.materialize(txn, ptr);
                    }
                }
                _ => {
                    // other types don't define integration-specific actions
                }
            }
            txn.add_changed_type(parent_ref, this.parent_sub.clone());
            if this.info.is_linked() {
                if let Some(links) = txn.store.linked_by.get(&self_ptr).cloned() {
                    // notify links about changes
                    for link in links.iter() {
                        txn.add_changed_type(*link, this.parent_sub.clone());
                    }
                }
            }
            let parent_deleted = if let TypePtr::Branch(ptr) = &this.parent {
                if let Some(block) = ptr.item {
                    block.is_deleted()
                } else {
                    false
                }
            } else {
                false
            };
            if parent_deleted || (this.parent_sub.is_some() && this.right.is_some()) {
                // delete if parent is deleted or if this is not the current attribute value of parent
                true
            } else {
                false
            }
        } else {
            true
        }
    }

    /// Squashes two blocks together. Returns true if it succeeded. Squashing is possible only if
    /// blocks are of the same type, their contents are of the same type, they belong to the same
    /// parent data structure, their IDs are sequenced directly one after another and they point to
    /// each other as their left/right neighbors respectively.
    pub(crate) fn try_squash(&mut self, other: ItemPtr) -> bool {
        if self.id.client == other.id.client
            && self.id.clock + self.len() == other.id.clock
            && other.origin == Some(self.last_id())
            && self.right_origin == other.right_origin
            && self.right == Some(other)
            && self.is_deleted() == other.is_deleted()
            && (self.redone.is_none() && other.redone.is_none())
            && (!self.info.is_linked() && !other.info.is_linked()) // linked items cannot be merged
            && self.moved == other.moved
            && self.content.try_squash(&other.content)
        {
            self.len = self.content.len(OffsetKind::Utf16);
            if let Some(mut right_right) = other.right {
                right_right.left = Some(*self);
            }
            if other.info.is_keep() {
                self.info.set_keep();
            }
            self.right = other.right;
            true
        } else {
            false
        }
    }

    pub(crate) fn as_branch(self) -> Option<BranchPtr> {
        if let ItemContent::Type(branch) = &self.content {
            Some(BranchPtr::from(branch))
        } else {
            None
        }
    }
}

impl Deref for ItemPtr {
    type Target = Item;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.as_ref() }
    }
}

impl DerefMut for ItemPtr {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.0.as_mut() }
    }
}

impl<'a> From<&'a mut Box<Item>> for ItemPtr {
    fn from(block: &'a mut Box<Item>) -> Self {
        ItemPtr(NonNull::from(block.as_mut()))
    }
}

impl<'a> From<&'a Box<Item>> for ItemPtr {
    fn from(block: &'a Box<Item>) -> Self {
        ItemPtr(unsafe { NonNull::new_unchecked(block.as_ref() as *const Item as *mut Item) })
    }
}

impl Eq for ItemPtr {}

impl PartialEq for ItemPtr {
    fn eq(&self, other: &Self) -> bool {
        // BlockPtr.pivot may differ, but logically it doesn't affect block equality
        self.id() == other.id()
    }
}

impl TryFrom<ItemPtr> for Any {
    type Error = ItemPtr;

    fn try_from(item: ItemPtr) -> Result<Self, Self::Error> {
        match &item.content {
            ItemContent::Any(v) => Ok(v[0].clone()),
            ItemContent::Embed(v) => Ok(v.clone()),
            ItemContent::Binary(v) => Ok(v.clone().into()),
            ItemContent::JSON(v) => Ok(v[0].clone().into()),
            ItemContent::String(v) => Ok(v.to_string().into()),
            _ => Err(item),
        }
    }
}

impl Item {
    #[inline]
    pub(crate) fn clock_range(&self) -> (u32, u32) {
        let start = self.id.clock;
        let end = start + self.len - 1;
        (start, end)
    }

    pub fn encode<E: Encoder>(&self, encoder: &mut E) {
        let info = self.info();
        let cant_copy_parent_info = info & (HAS_ORIGIN | HAS_RIGHT_ORIGIN) == 0;
        encoder.write_info(info);
        if let Some(origin_id) = self.origin.as_ref() {
            encoder.write_left_id(origin_id);
        }
        if let Some(right_origin_id) = self.right_origin.as_ref() {
            encoder.write_right_id(right_origin_id);
        }
        if cant_copy_parent_info {
            match &self.parent {
                TypePtr::Branch(branch) => {
                    if let Some(block) = branch.item {
                        encoder.write_parent_info(false);
                        encoder.write_left_id(block.id());
                    } else if let Some(name) = branch.name.as_deref() {
                        encoder.write_parent_info(true);
                        encoder.write_string(name);
                    } else {
                        unreachable!()
                    }
                }
                TypePtr::Named(name) => {
                    encoder.write_parent_info(true);
                    encoder.write_string(name);
                }
                TypePtr::ID(id) => {
                    encoder.write_parent_info(false);
                    encoder.write_left_id(id);
                }
                TypePtr::Unknown => {
                    panic!("Couldn't get item's parent")
                }
            }
            if let Some(parent_sub) = self.parent_sub.as_ref() {
                encoder.write_string(parent_sub.as_ref());
            }
        }
        self.content.encode(encoder);
    }

    /// Returns a unique identifier of a first update contained by a current [Item].
    pub fn id(&self) -> &ID {
        &self.id
    }
}

/// A helper structure that's used to precisely describe a location of an [Item] to be inserted in
/// relation to its neighbors and parent.
#[derive(Debug)]
pub(crate) struct ItemPosition {
    pub parent: TypePtr,
    pub left: Option<ItemPtr>,
    pub right: Option<ItemPtr>,
    pub index: u32,
    pub current_attrs: Option<Box<Attrs>>,
}

impl ItemPosition {
    pub fn forward(&mut self) -> bool {
        if let Some(right) = self.right.as_deref() {
            if !right.is_deleted() {
                match &right.content {
                    ItemContent::String(_) | ItemContent::Embed(_) => {
                        self.index += right.len();
                    }
                    ItemContent::Format(key, value) => {
                        let attrs = self.current_attrs.get_or_init();
                        update_current_attributes(attrs, key, value.as_ref());
                    }
                    _ => {}
                }
            }

            let new = right.right.clone();
            self.left = self.right.take();
            self.right = new;

            return true;
        }
        false
    }

    /// If current `attributes` don't confirm the same keys as the formatting wrapping
    /// current insert position, they should be unset.
    pub fn unset_missing(&self, attributes: &mut Attrs) {
        if let Some(attrs) = self.current_attrs.as_ref() {
            // if current `attributes` don't confirm the same keys as the formatting wrapping
            // current insert position, they should be unset
            for (k, _) in attrs.iter() {
                if !attributes.contains_key(k) {
                    attributes.insert(k.clone(), Any::Null);
                }
            }
        }
    }
}

/// Bit flag (9st bit) for item that is linked by Weak Link references
const ITEM_FLAG_LINKED: u16 = 0b0001_0000_0000;

/// Bit flag (4th bit) for a marked item - not used atm.
const ITEM_FLAG_MARKED: u16 = 0b0000_1000;

/// Bit flag (3rd bit) for a tombstoned (deleted) item.
const ITEM_FLAG_DELETED: u16 = 0b0000_0100;

/// Bit flag (2nd bit) for an item, which contents are considered countable.
const ITEM_FLAG_COUNTABLE: u16 = 0b0000_0010;

/// Bit flag (1st bit) used for an item which should be kept - not used atm.
const ITEM_FLAG_KEEP: u16 = 0b0000_0001;

/// Collection of flags attached to an [Item] - most of them are serializable and define specific
/// properties of an associated [Item], like:
///
/// - Has item been deleted?
/// - Is item countable (should its content add to the length/offset calculation of containing collection)?
/// - Should item be kept untouched eg. because it's being tracked by [UndoManager].
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ItemFlags(u16);

impl ItemFlags {
    pub fn new(source: u16) -> Self {
        ItemFlags(source)
    }

    #[inline]
    fn set(&mut self, value: u16) {
        self.0 |= value
    }

    #[inline]
    fn clear(&mut self, value: u16) {
        self.0 &= !value
    }

    #[inline]
    fn check(&self, value: u16) -> bool {
        self.0 & value == value
    }

    #[inline]
    pub fn is_keep(&self) -> bool {
        self.check(ITEM_FLAG_KEEP)
    }

    #[inline]
    pub fn set_keep(&mut self) {
        self.set(ITEM_FLAG_KEEP)
    }

    #[inline]
    pub fn clear_keep(&mut self) {
        self.clear(ITEM_FLAG_KEEP)
    }

    #[inline]
    pub fn set_countable(&mut self) {
        self.set(ITEM_FLAG_COUNTABLE)
    }

    #[inline]
    pub fn clear_countable(&mut self) {
        self.clear(ITEM_FLAG_COUNTABLE)
    }

    #[inline]
    pub fn is_countable(&self) -> bool {
        self.check(ITEM_FLAG_COUNTABLE)
    }

    #[inline]
    pub fn set_deleted(&mut self) {
        self.set(ITEM_FLAG_DELETED)
    }

    #[inline]
    pub fn is_deleted(&self) -> bool {
        self.check(ITEM_FLAG_DELETED)
    }

    #[inline]
    pub fn is_marked(&self) -> bool {
        self.check(ITEM_FLAG_MARKED)
    }

    #[inline]
    pub fn is_linked(&self) -> bool {
        self.check(ITEM_FLAG_LINKED)
    }

    #[inline]
    pub fn set_linked(&mut self) {
        self.set(ITEM_FLAG_LINKED)
    }

    #[inline]
    pub fn clear_linked(&mut self) {
        self.clear(ITEM_FLAG_LINKED)
    }
}

impl Into<u16> for ItemFlags {
    #[inline]
    fn into(self) -> u16 {
        self.0
    }
}

/// Block defines a range of consecutive updates performed by the same peer. While individual
/// updates are always uniquely defined by their corresponding [ID]s, they may contain a lot of
/// additional metadata. Block representation here is crucial, since it optimizes memory usage,
/// available when multiple updates have been performed one after another (eg. *when user is writing
/// a sentence, individual key strokes are independent updates but they can be compresses into a
/// single block containing an entire sentence for as long as another piece of data is not being
/// inserted in the middle it*).
#[derive(PartialEq)]
pub struct Item {
    /// Unique identifier of the first update described by the current [Item].
    pub(crate) id: ID,

    /// A number of splittable updates within a current [Item].
    pub(crate) len: u32,

    /// Pointer to left neighbor of this item. Used in sequenced collections.
    /// If `None`, then current item is the first one on its [parent](Item::parent) collection.
    pub(crate) left: Option<ItemPtr>,

    /// Pointer to right neighbor of this item. Used in sequenced collections.
    /// If `None`, then current item is the last one on its [parent](Item::parent) collection.
    ///
    /// For map-like CRDTs if this field is `None`, it means that a current item also contains
    /// the most recent update for an individual key-value entry.
    pub(crate) right: Option<ItemPtr>,

    /// Used for concurrent insert conflict resolution. An ID of a left-side neighbor at the moment
    /// of insertion of current block.
    pub(crate) origin: Option<ID>,

    /// Used for concurrent insert conflict resolution. An ID of a right-side neighbor at the moment
    /// of insertion of current block.
    pub(crate) right_origin: Option<ID>,

    /// A user data stored inside of a current item.
    pub(crate) content: ItemContent,

    /// Pointer to a parent collection containing current item.
    pub(crate) parent: TypePtr,

    /// Used by [UndoManager] to track another block that reverts the effects of deletion of current
    /// item.
    pub(crate) redone: Option<ID>,

    /// Used only when current item is used by map-like types. In such case this item works as a
    /// key-value entry of a map, and this field contains a key used by map.
    pub(crate) parent_sub: Option<Arc<str>>,

    /// This property is reused by the moved prop. In this case this property refers to an Item.
    pub(crate) moved: Option<ItemPtr>,

    /// Bit flag field which contains information about specifics of this item.
    pub(crate) info: ItemFlags,
}

/// Describes a consecutive range of updates (identified by their [ID]s).
#[derive(PartialEq, Eq, Clone)]
pub struct BlockRange {
    /// [ID] of the first update stored within current [BlockRange] bounds.
    pub id: ID,
    /// Number of splittable updates stored within this [BlockRange].
    pub len: u32,
}

impl BlockRange {
    pub fn new(id: ID, len: u32) -> Self {
        BlockRange { id, len }
    }

    /// Returns an [ID] of the last update fitting into the bounds of current [BlockRange]
    pub fn last_id(&self) -> ID {
        ID::new(self.id.client, self.id.clock + self.len)
    }

    /// Returns a slice of a current [BlockRange], which starts at a given offset (relative to
    /// current range).
    ///
    /// # Example:
    ///
    /// ```rust
    /// use yrs::block::BlockRange;
    /// use yrs::ID;
    /// let a = BlockRange::new(ID::new(1, 2), 8); // range of clocks [2..10)
    /// let b = a.slice(3); // range of clocks [5..10)
    ///
    /// assert_eq!(b.id, ID::new(1, 5));
    /// assert_eq!(b.last_id(), ID::new(1, 10));
    /// ```
    pub fn slice(&self, offset: u32) -> Self {
        let mut next = self.clone();
        next.id.clock += offset;
        next.len -= offset;
        next
    }

    pub(crate) fn integrate(&mut self, pivot: u32) -> bool {
        if pivot > 0 {
            self.id.clock += pivot;
            self.len -= pivot;
        }

        false
    }

    #[inline]
    pub(crate) fn merge(&mut self, other: &Self) {
        self.len += other.len;
    }

    /// Checks if provided `id` fits inside of boundaries defined by current [BlockRange].
    pub fn contains(&self, id: &ID) -> bool {
        self.id.client == id.client
            && id.clock >= self.id.clock
            && id.clock < self.id.clock + self.len
    }
}

impl std::fmt::Debug for BlockRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::fmt::Display for BlockRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}-{})", self.id, self.len)
    }
}

impl Item {
    pub(crate) fn new(
        id: ID,
        left: Option<ItemPtr>,
        origin: Option<ID>,
        right: Option<ItemPtr>,
        right_origin: Option<ID>,
        parent: TypePtr,
        parent_sub: Option<Arc<str>>,
        content: ItemContent,
    ) -> Option<Box<Item>> {
        let info = ItemFlags::new(if content.is_countable() {
            ITEM_FLAG_COUNTABLE
        } else {
            0
        });
        let len = content.len(OffsetKind::Utf16);
        if len == 0 {
            return None;
        }
        let root_name = if let TypePtr::Named(root) = &parent {
            Some(root.clone())
        } else {
            None
        };
        let mut item = Box::new(Item {
            id,
            len,
            left,
            right,
            origin,
            right_origin,
            content,
            parent,
            parent_sub,
            info,
            moved: None,
            redone: None,
        });
        let item_ptr = ItemPtr::from(&mut item);
        if let ItemContent::Type(branch) = &mut item.content {
            let b = Arc::get_mut(branch).unwrap();
            b.item = Some(item_ptr);
            if b.name.is_none() {
                b.name = root_name;
            }
        }
        Some(item)
    }

    /// Checks if provided `id` fits inside of updates defined within bounds of current [Item].
    pub fn contains(&self, id: &ID) -> bool {
        self.id.client == id.client
            && id.clock >= self.id.clock
            && id.clock < self.id.clock + self.len()
    }

    /// Checks if current item is marked as deleted (tombstoned).
    /// Yrs uses soft item deletion mechanism, which means that deleted values are not physically
    /// erased from memory, but just marked as deleted.
    pub fn is_deleted(&self) -> bool {
        self.info.is_deleted()
    }

    /// Checks if item content can be considered countable. Countable elements can be split
    /// and joined together.
    pub fn is_countable(&self) -> bool {
        self.info.is_countable()
    }

    pub(crate) fn mark_as_deleted(&mut self) {
        self.info.set_deleted()
    }

    /// Assign left/right neighbors of the block. This may require for origin/right_origin
    /// blocks to be already present in block store - which may not be the case during block
    /// decoding. We decode entire update first, and apply individual blocks second, hence
    /// repair function is called before applying the block rather than on decode.
    pub(crate) fn repair(&mut self, store: &mut Store) -> Result<(), UpdateError> {
        if let Some(origin) = self.origin.as_ref() {
            self.left = store
                .blocks
                .get_item_clean_end(origin)
                .map(|slice| store.materialize(slice));
        }

        if let Some(origin) = self.right_origin.as_ref() {
            self.right = store
                .blocks
                .get_item_clean_start(origin)
                .map(|slice| store.materialize(slice));
        }

        // We have all missing ids, now find the items

        // In the original Y.js algorithm we decoded items as we go and attached them to client
        // block list. During that process if we had right origin but no left, we made a lookup for
        // right origin's parent and attach it as a parent of current block.
        //
        // Here since we decode all blocks first, then apply them, we might not find them in
        // the block store during decoding. Therefore we retroactively reattach it here.

        self.parent = match &self.parent {
            TypePtr::Branch(branch_ptr) => TypePtr::Branch(*branch_ptr),
            TypePtr::Unknown => match (self.left, self.right) {
                (Some(item), _) if item.parent != TypePtr::Unknown => {
                    self.parent_sub = item.parent_sub.clone();
                    item.parent.clone()
                }
                (_, Some(item)) if item.parent != TypePtr::Unknown => {
                    self.parent_sub = item.parent_sub.clone();
                    item.parent.clone()
                }
                _ => TypePtr::Unknown,
            },
            TypePtr::Named(name) => {
                let branch = store.get_or_create_type(name.clone(), TypeRef::Undefined);
                TypePtr::Branch(branch)
            }
            TypePtr::ID(id) => {
                let ptr = store.blocks.get_item(id);
                if let Some(item) = ptr {
                    match &item.content {
                        ItemContent::Type(branch) => {
                            TypePtr::Branch(BranchPtr::from(branch.as_ref()))
                        }
                        ItemContent::Deleted(_) => TypePtr::Unknown,
                        other => {
                            return Err(UpdateError::InvalidParent(
                                id.clone(),
                                other.get_ref_number(),
                            ))
                        }
                    }
                } else {
                    TypePtr::Unknown
                }
            }
        };

        Ok(())
    }

    /// Returns a length of a block. For most situation it works like [Item::content_len] with a
    /// difference to a [Text]/[XmlText] contents - in order to achieve compatibility with
    /// Yjs we need to calculate string length in terms of UTF-16 character encoding.
    /// However depending on used [Encoding] scheme we may calculate string length/offsets
    /// differently.
    pub fn len(&self) -> u32 {
        self.len
    }

    pub fn content_len(&self, kind: OffsetKind) -> u32 {
        self.content.len(kind)
    }

    /// Returns an ID of the last element that can be considered a part of this item.
    pub fn last_id(&self) -> ID {
        ID::new(self.id.client, self.id.clock + self.len() - 1)
    }

    pub fn info(&self) -> u8 {
        let info = if self.origin.is_some() { HAS_ORIGIN } else { 0 } // is left null
            | if self.right_origin.is_some() { HAS_RIGHT_ORIGIN } else { 0 } // is right null
            | if self.parent_sub.is_some() { HAS_PARENT_SUB } else { 0 }
            | (self.content.get_ref_number() & 0b1111);
        info
    }

    pub(crate) fn gc(&mut self, collector: &mut GCCollector, parent_gc: bool) {
        if self.is_deleted() && !self.info.is_keep() {
            self.content.gc(collector);
            let len = self.len();
            if parent_gc {
                collector.mark(&self.id);
            } else {
                self.content = ItemContent::Deleted(len);
                self.info.clear_countable();
            }
        }
    }
}

#[derive(PartialEq, PartialOrd, Eq, Ord, Debug, Clone)]
pub struct SplittableString {
    content: SmallString<[u8; 8]>,
}

impl SplittableString {
    pub fn len(&self, kind: OffsetKind) -> usize {
        let len = self.content.len();
        if len == 1 {
            len // quite often strings are single-letter, so we don't care about OffsetKind
        } else {
            match kind {
                OffsetKind::Bytes => len,
                OffsetKind::Utf16 => self.utf16_len(),
            }
        }
    }

    #[inline(always)]
    pub fn as_str(&self) -> &str {
        self.content.as_str()
    }

    #[inline(always)]
    pub fn utf16_len(&self) -> usize {
        self.encode_utf16().count()
    }

    /// Maps given offset onto block offset. This means, that given an `offset` provided
    /// in given `encoding` we want the output as a UTF-16 compatible offset (required
    /// by Yjs for compatibility reasons).
    pub(crate) fn block_offset(&self, offset: u32, kind: OffsetKind) -> u32 {
        match kind {
            OffsetKind::Utf16 => offset,
            OffsetKind::Bytes => {
                let mut remaining = offset;
                let mut i = 0;
                // since this offset is used to splitting later on - and we can only split entire
                // characters - we're computing by characters
                for c in self.content.chars() {
                    if remaining == 0 {
                        break;
                    }
                    remaining -= c.len_utf8() as u32;
                    i += c.len_utf16() as u32;
                }
                i
            }
        }
    }

    pub fn push_str(&mut self, str: &str) {
        self.content.push_str(str);
    }
}

impl std::fmt::Display for SplittableString {
    #[inline(always)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.content.fmt(f)
    }
}

impl Into<SmallString<[u8; 8]>> for SplittableString {
    #[inline(always)]
    fn into(self) -> SmallString<[u8; 8]> {
        self.content
    }
}

impl Into<Box<str>> for SplittableString {
    #[inline(always)]
    fn into(self) -> Box<str> {
        self.content.into_string().into_boxed_str()
    }
}

impl From<SmallString<[u8; 8]>> for SplittableString {
    fn from(content: SmallString<[u8; 8]>) -> Self {
        SplittableString { content }
    }
}

impl<'a> From<&'a str> for SplittableString {
    fn from(str: &'a str) -> Self {
        Self::from(SmallString::from_str(str))
    }
}

impl Deref for SplittableString {
    type Target = str;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.content
    }
}

pub(crate) fn split_str(str: &str, offset: usize, kind: OffsetKind) -> Option<(&str, &str)> {
    fn map_utf16_offset(str: &str, offset: u32) -> (usize, bool) {
        let mut off = 0;
        let mut i = 0;
        for c in str.chars() {
            if i >= offset {
                break;
            }
            off += c.len_utf8() as u32;
            i += c.len_utf16() as u32;
        }
        let exceeded = i > offset;
        (off as usize ,exceeded)
    }

    let (off, exceeded) = match kind {
        OffsetKind::Bytes => (offset, false),
        OffsetKind::Utf16 => map_utf16_offset(str, offset as u32),
        }

    if exceeded{
            None 
        }
    else{
            Some(str.split_at(off))
    }
}

/// An enum describing the type of a user data content stored as part of one or more
/// (if items were squashed) insert operations.
#[derive(Debug, PartialEq)]
pub enum ItemContent {
    /// Collection of consecutively inserted JSON-like primitive values.
    Any(Vec<Any>),

    /// A BLOB data eg. images. Binaries are treated as a single objects (they are not subjects to splits).
    Binary(Vec<u8>),

    /// A marker for delete item data, which describes a number of deleted elements.
    /// Deleted elements also don't contribute to an overall length of containing collection type.
    Deleted(u32),

    /// Sub-document container. Contains weak reference to a parent document and a child document.
    Doc(Option<Doc>, Doc),

    /// Obsolete: collection of consecutively inserted stringified JSON values.
    JSON(Vec<String>),

    /// A single embedded JSON-like primitive value.
    Embed(Any),

    /// Formatting attribute entry. Format attributes are not considered countable and don't
    /// contribute to an overall length of a collection they are applied to.
    Format(Arc<str>, Box<Any>),

    /// A chunk of text, usually applied by collaborative text insertion.
    String(SplittableString),

    /// A reference of a branch node. Branch nodes define a complex collection types, such as
    /// arrays, maps or XML elements.
    Type(Arc<Branch>),

    /// Marker for destination location of move operation. Move is used to change position of
    /// previously inserted element in a sequence with respect to other operations that may happen
    /// concurrently on other peers.
    Move(Box<Move>),
}

impl ItemContent {
    /// Returns a reference number used to determine a content type.
    /// It's used during encoding/decoding of a containing block.
    pub fn get_ref_number(&self) -> u8 {
        match self {
            ItemContent::Any(_) => BLOCK_ITEM_ANY_REF_NUMBER,
            ItemContent::Binary(_) => BLOCK_ITEM_BINARY_REF_NUMBER,
            ItemContent::Deleted(_) => BLOCK_ITEM_DELETED_REF_NUMBER,
            ItemContent::Doc(_, _) => BLOCK_ITEM_DOC_REF_NUMBER,
            ItemContent::JSON(_) => BLOCK_ITEM_JSON_REF_NUMBER,
            ItemContent::Embed(_) => BLOCK_ITEM_EMBED_REF_NUMBER,
            ItemContent::Format(_, _) => BLOCK_ITEM_FORMAT_REF_NUMBER,
            ItemContent::String(_) => BLOCK_ITEM_STRING_REF_NUMBER,
            ItemContent::Type(_) => BLOCK_ITEM_TYPE_REF_NUMBER,
            ItemContent::Move(_) => BLOCK_ITEM_MOVE_REF_NUMBER,
        }
    }

    /// Checks if item content can be considered countable. Countable elements contribute to
    /// a length of the block they are contained by. Most of the item content variants are countable
    /// with exception for [ItemContent::Deleted] (which length describes number of removed
    /// elements) and [ItemContent::Format] (which is used for storing text formatting tags).
    pub fn is_countable(&self) -> bool {
        match self {
            ItemContent::Any(_) => true,
            ItemContent::Binary(_) => true,
            ItemContent::Doc(_, _) => true,
            ItemContent::JSON(_) => true,
            ItemContent::Embed(_) => true,
            ItemContent::String(_) => true,
            ItemContent::Type(_) => true,
            ItemContent::Deleted(_) => false,
            ItemContent::Format(_, _) => false,
            ItemContent::Move(_) => false,
        }
    }

    /// Returns a number of separate elements contained within current item content struct.
    ///
    /// Separate elements can be split in order to put another block in between them. Definition of
    /// separation depends on a item content kin, eg. [ItemContent::String], [ItemContent::Any],
    /// [ItemContent::JSON] and [ItemContent::Deleted] can have variable length as they may be split
    /// by other insert operations. Other variants (eg. [ItemContent::Binary]) are considered as
    /// a single element and therefore their length is always 1 and are not considered as subject of
    /// splitting.
    ///
    /// In cases of counting number of visible elements, `len` method should be used together with
    /// [ItemContent::is_countable].
    pub fn len(&self, kind: OffsetKind) -> u32 {
        match self {
            ItemContent::Deleted(deleted) => *deleted,
            ItemContent::String(str) => str.len(kind) as u32,
            ItemContent::Any(v) => v.len() as u32,
            ItemContent::JSON(v) => v.len() as u32,
            _ => 1,
        }
    }

    /// Reads a contents of current [ItemContent] into a given `buf`, starting from provided
    /// `offset`. Returns a number of elements read this way (it cannot be longer than `buf`'s len.
    pub fn read(&self, offset: usize, buf: &mut [Out]) -> usize {
        if buf.is_empty() {
            0
        } else {
            match self {
                ItemContent::Any(values) => {
                    let mut i = offset;
                    let mut j = 0;
                    while i < values.len() && j < buf.len() {
                        let any = &values[i];
                        buf[j] = Out::Any(any.clone());
                        i += 1;
                        j += 1;
                    }
                    j
                }
                ItemContent::String(v) => {
                    let chars = v.chars().skip(offset).take(buf.len());
                    let mut j = 0;
                    for c in chars {
                        buf[j] = Out::Any(Any::from(c.to_string()));
                        j += 1;
                    }
                    j
                }
                ItemContent::JSON(elements) => {
                    let mut i = offset;
                    let mut j = 0;
                    while i < elements.len() && j < buf.len() {
                        let elem = elements[i].as_str();
                        buf[j] = Out::Any(Any::from(elem));
                        i += 1;
                        j += 1;
                    }
                    j
                }
                ItemContent::Binary(v) => {
                    buf[0] = Out::Any(Any::from(v.deref()));
                    1
                }
                ItemContent::Doc(_, doc) => {
                    buf[0] = Out::YDoc(doc.clone());
                    1
                }
                ItemContent::Type(c) => {
                    let branch_ref = BranchPtr::from(c);
                    buf[0] = branch_ref.into();
                    1
                }
                ItemContent::Embed(v) => {
                    buf[0] = Out::Any(v.clone());
                    1
                }
                ItemContent::Move(_) => 0,
                ItemContent::Deleted(_) => 0,
                ItemContent::Format(_, _) => 0,
            }
        }
    }

    /// Reads all contents stored in this item and returns them. Use [ItemContent::read] if you need
    /// to read only slice of elements from the corresponding item.
    pub fn get_content(&self) -> Vec<Out> {
        let len = self.len(OffsetKind::Utf16) as usize;
        let mut values = vec![Out::default(); len];
        let read = self.read(0, &mut values);
        if read == len {
            values
        } else {
            Vec::default()
        }
    }

    /// Returns a first value stored in a corresponding item.
    pub fn get_first(&self) -> Option<Out> {
        match self {
            ItemContent::Any(v) => v.first().map(|a| Out::Any(a.clone())),
            ItemContent::Binary(v) => Some(Out::Any(Any::from(v.deref()))),
            ItemContent::Deleted(_) => None,
            ItemContent::Move(_) => None,
            ItemContent::Doc(_, v) => Some(Out::YDoc(v.clone())),
            ItemContent::JSON(v) => v.first().map(|v| Out::Any(Any::from(v.deref()))),
            ItemContent::Embed(v) => Some(Out::Any(v.clone())),
            ItemContent::Format(_, _) => None,
            ItemContent::String(v) => Some(Out::Any(Any::from(v.clone().as_str()))),
            ItemContent::Type(c) => Some(BranchPtr::from(c).into()),
        }
    }

    /// Returns a last value stored in a corresponding item.
    pub fn get_last(&self) -> Option<Out> {
        match self {
            ItemContent::Any(v) => v.last().map(|a| Out::Any(a.clone())),
            ItemContent::Binary(v) => Some(Out::Any(Any::from(v.deref()))),
            ItemContent::Deleted(_) => None,
            ItemContent::Move(_) => None,
            ItemContent::Doc(_, v) => Some(Out::YDoc(v.clone())),
            ItemContent::JSON(v) => v.last().map(|v| Out::Any(Any::from(v.as_str()))),
            ItemContent::Embed(v) => Some(Out::Any(v.clone())),
            ItemContent::Format(_, _) => None,
            ItemContent::String(v) => Some(Out::Any(Any::from(v.as_str()))),
            ItemContent::Type(c) => Some(BranchPtr::from(c).into()),
        }
    }

    /// Encodes a slice of a current [ItemContent] within an index bounds of (start..=end) - both
    /// sides inclusive.
    pub fn encode_slice<E: Encoder>(&self, encoder: &mut E, start: u32, end: u32) {
        match self {
            ItemContent::Deleted(_) => encoder.write_len(end - start + 1),
            ItemContent::Binary(buf) => encoder.write_buf(buf),
            ItemContent::String(s) => {
                let length = s.len(OffsetKind::Utf16);
                let empty_string = " ".repeat(length);
                let slice = if start != 0 {
                    match split_str(&s, start as usize, OffsetKind::Utf16) {
                        Some((_, right)) => {
                            right
                        }
                        None => {
                            match split_str(&empty_string, start as usize, OffsetKind::Utf16){
                                Some((_, right)) => {
                                    right
                                }
                                None => {
                                    panic!("split_str failed a second time.");
                                }
                            }
                        }
                    }
                } else {
                    &s
                };
                let slice_length = slice.len();
                let empty_string_slice = " ".repeat(slice_length);
                let slice = if end != 0 {
                          match split_str(&slice, (end - start + 1) as usize, OffsetKind::Utf16) {
                        Some((left, _)) => {
                            left
                        }
                        None => {
                            match split_str(&empty_string_slice, (end - start + 1) as usize, OffsetKind::Utf16){
                                Some((left, _)) => {
                                    left
                                }
                                None => {
                                    panic!("split_str failed a second time.");
                                }
                            }
                        }
                    }
                } else {
                    slice
                };
                encoder.write_string(slice)
            }
            ItemContent::Embed(s) => encoder.write_json(s),
            ItemContent::JSON(s) => {
                encoder.write_len(end - start + 1);
                for i in start..=end {
                    encoder.write_string(s[i as usize].as_str())
                }
            }
            ItemContent::Format(k, v) => {
                encoder.write_key(k.as_ref());
                encoder.write_json(v.as_ref());
            }
            ItemContent::Type(inner) => {
                inner.type_ref.encode(encoder);
            }
            ItemContent::Any(any) => {
                encoder.write_len(end - start + 1);
                for i in start..=end {
                    encoder.write_any(&any[i as usize]);
                }
            }
            ItemContent::Doc(_, doc) => doc.store().options().encode(encoder),
            ItemContent::Move(m) => m.encode(encoder),
        }
    }

    pub fn encode<E: Encoder>(&self, encoder: &mut E) {
        match self {
            ItemContent::Deleted(len) => encoder.write_len(*len),
            ItemContent::Binary(buf) => encoder.write_buf(buf),
            ItemContent::String(s) => encoder.write_string(s.as_str()),
            ItemContent::Embed(s) => encoder.write_json(s),
            ItemContent::JSON(s) => {
                encoder.write_len(s.len() as u32);
                for json in s.iter() {
                    encoder.write_string(json.as_str())
                }
            }
            ItemContent::Format(k, v) => {
                encoder.write_key(k.as_ref());
                encoder.write_json(v.as_ref());
            }
            ItemContent::Type(inner) => {
                inner.type_ref.encode(encoder);
            }
            ItemContent::Any(any) => {
                encoder.write_len(any.len() as u32);
                for a in any.iter() {
                    encoder.write_any(a);
                }
            }
            ItemContent::Doc(_, doc) => doc.store().options().encode(encoder),
            ItemContent::Move(m) => m.encode(encoder),
        }
    }

    pub fn decode<D: Decoder>(decoder: &mut D, ref_num: u8) -> Result<Self, Error> {
        match ref_num & 0b1111 {
            BLOCK_ITEM_DELETED_REF_NUMBER => Ok(ItemContent::Deleted(decoder.read_len()?)),
            BLOCK_ITEM_JSON_REF_NUMBER => {
                let mut remaining = decoder.read_len()? as i32;
                let mut buf = Vec::new();
                buf.try_reserve(remaining as usize)?;

                while remaining >= 0 {
                    buf.push(decoder.read_string()?.to_owned());
                    remaining -= 1;
                }
                Ok(ItemContent::JSON(buf))
            }
            BLOCK_ITEM_BINARY_REF_NUMBER => Ok(ItemContent::Binary(decoder.read_buf()?.to_owned())),
            BLOCK_ITEM_STRING_REF_NUMBER => Ok(ItemContent::String(decoder.read_string()?.into())),
            BLOCK_ITEM_EMBED_REF_NUMBER => Ok(ItemContent::Embed(decoder.read_json()?.into())),
            BLOCK_ITEM_FORMAT_REF_NUMBER => Ok(ItemContent::Format(
                decoder.read_key()?,
                decoder.read_json()?.into(),
            )),
            BLOCK_ITEM_TYPE_REF_NUMBER => {
                let type_ref = TypeRef::decode(decoder)?;
                let inner = Branch::new(type_ref);
                Ok(ItemContent::Type(inner))
            }
            BLOCK_ITEM_ANY_REF_NUMBER => {
                let len = decoder.read_len()? as usize;
                let mut values = Vec::new();
                values.try_reserve(len)?;

                let mut i = 0;
                while i < len {
                    values.push(decoder.read_any()?);
                    i += 1;
                }
                Ok(ItemContent::Any(values))
            }
            BLOCK_ITEM_MOVE_REF_NUMBER => {
                let m = Move::decode(decoder)?;
                Ok(ItemContent::Move(Box::new(m)))
            }
            BLOCK_ITEM_DOC_REF_NUMBER => {
                let mut options = Options::decode(decoder)?;
                options.should_load = options.should_load || options.auto_load;
                Ok(ItemContent::Doc(None, Doc::with_options(options)))
            }
            _ => Err(Error::UnexpectedValue),
        }
    }

    pub(crate) fn splice(&mut self, offset: usize, encoding: OffsetKind) -> Option<ItemContent> {
        match self {
            ItemContent::Any(value) => {
                let (left, right) = value.split_at(offset);
                let left = left.to_vec();
                let right = right.to_vec();
                *self = ItemContent::Any(left);
                Some(ItemContent::Any(right))
            }
            ItemContent::String(string) => {
                // compute offset given in unicode code points into byte position
                let empty_string = " ".repeat(string.len(OffsetKind::Utf16)); 
                let (left, right) = match split_str(string, offset, encoding) {
                    Some((l, r)) => {
                        (l,r)   
                    }
                    None => {
                        // Handle the case where offset exceeds the string length
                        println!("replacing string {} with spaces, len = {}", string, string.len(OffsetKind::Utf16));  
                        match split_str(&empty_string, offset, encoding) {
                            Some((l, r)) => {
                               (l,r)
                            }
                            None => {
                                panic!("split_str failed second time!");
                            }
                        }
                    }
                };
                let left: SplittableString = left.into();
                let right: SplittableString = right.into();

                //TODO: do we need that in Rust?
                //let split_point = left.chars().last().unwrap();
                //if split_point >= 0xD800 as char && split_point <= 0xDBFF as char {
                //    // Last character of the left split is the start of a surrogate utf16/ucs2 pair.
                //    // We don't support splitting of surrogate pairs because this may lead to invalid documents.
                //    // Replace the invalid character with a unicode replacement character (� / U+FFFD)
                //    left.replace_range((offset-1)..offset, "�");
                //    right.replace_range(0..1, "�");
                //}
                *self = ItemContent::String(left);

                Some(ItemContent::String(right))
            }
            ItemContent::Deleted(len) => {
                let right = ItemContent::Deleted(*len - offset as u32);
                *len = offset as u32;
                Some(right)
            }
            ItemContent::JSON(value) => {
                let (left, right) = value.split_at(offset);
                let left = left.to_vec();
                let right = right.to_vec();
                *self = ItemContent::JSON(left);
                Some(ItemContent::JSON(right))
            }
            _ => None,
        }
    }

    /// Tries to squash two item content structures together.
    /// Returns `true` if this method had any effect on current [ItemContent] (modified it).
    /// Otherwise returns `false`.
    pub fn try_squash(&mut self, other: &Self) -> bool {
        //TODO: change `other` to Self (not ref) and return type to Option<Self> (none if merge suceeded)
        match (self, other) {
            (ItemContent::Any(v1), ItemContent::Any(v2)) => {
                v1.append(&mut v2.clone());
                true
            }
            (ItemContent::Deleted(v1), ItemContent::Deleted(v2)) => {
                *v1 = *v1 + *v2;
                true
            }
            (ItemContent::JSON(v1), ItemContent::JSON(v2)) => {
                v1.append(&mut v2.clone());
                true
            }
            (ItemContent::String(v1), ItemContent::String(v2)) => {
                v1.push_str(v2.as_str());
                true
            }
            _ => false,
        }
    }

    pub(crate) fn gc(&mut self, collector: &mut GCCollector) {
        match self {
            ItemContent::Type(branch) => {
                let b = Arc::get_mut(branch).unwrap();
                let mut curr = b.start.take();
                while let Some(mut item) = curr {
                    curr = item.right.clone();
                    item.gc(collector, true);
                }

                for (_, ptr) in b.map.drain() {
                    curr = Some(ptr);
                    while let Some(mut item) = curr {
                        curr = item.left.clone();
                        item.gc(collector, true);
                        continue;
                    }
                }
            }
            _ => {}
        }
    }
}

impl Clone for ItemContent {
    fn clone(&self) -> Self {
        match self {
            ItemContent::Any(array) => ItemContent::Any(array.clone()),
            ItemContent::Binary(bytes) => ItemContent::Binary(bytes.clone()),
            ItemContent::Deleted(len) => ItemContent::Deleted(*len),
            ItemContent::Doc(store, doc) => ItemContent::Doc(store.clone(), doc.clone()),
            ItemContent::JSON(array) => ItemContent::JSON(array.clone()),
            ItemContent::Embed(json) => ItemContent::Embed(json.clone()),
            ItemContent::Format(key, value) => ItemContent::Format(key.clone(), value.clone()),
            ItemContent::String(chunk) => ItemContent::String(chunk.clone()),
            ItemContent::Type(branch) => ItemContent::Type(Branch::new(branch.type_ref.clone())),
            ItemContent::Move(range) => ItemContent::Move(range.clone()),
        }
    }
}

impl std::fmt::Debug for Item {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::fmt::Display for Item {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, len: {}", self.id, self.len)?;
        match &self.parent {
            TypePtr::Unknown => {}
            TypePtr::Branch(b) => {
                if let Some(ptr) = b.item.as_ref() {
                    write!(f, ", parent: {}", ptr.id())?;
                } else {
                    write!(f, ", parent: <root>")?;
                }
            }
            other => {
                write!(f, ", parent: {}", other)?;
            }
        }
        if let Some(m) = self.moved {
            write!(f, ", moved-to: {}", m)?;
        }
        if let Some(id) = self.redone.as_ref() {
            write!(f, ", redone: {}", id)?;
        }
        if let Some(origin) = self.origin.as_ref() {
            write!(f, ", origin-l: {}", origin)?;
        }
        if let Some(origin) = self.right_origin.as_ref() {
            write!(f, ", origin-r: {}", origin)?;
        }
        if let Some(left) = self.left.as_ref() {
            write!(f, ", left: {}", left.id())?;
        }
        if let Some(right) = self.right.as_ref() {
            write!(f, ", right: {}", right.id())?;
        }
        if let Some(key) = self.parent_sub.as_ref() {
            write!(f, ", '{}' =>", key)?;
        } else {
            write!(f, ":")?;
        }
        if self.is_deleted() {
            write!(f, " ~{}~", &self.content)?;
        } else {
            write!(f, " {}", &self.content)?;
        }
        if self.info.is_linked() {
            write!(f, "|linked")?;
        }
        write!(f, ")")
    }
}

impl std::fmt::Display for ItemContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ItemContent::String(s) => write!(f, "'{}'", s),
            ItemContent::Any(s) => {
                write!(f, "[")?;
                let mut iter = s.iter();
                if let Some(a) = iter.next() {
                    write!(f, "{}", a.to_string())?;
                }
                while let Some(a) = iter.next() {
                    write!(f, ", {}", a.to_string())?;
                }
                write!(f, "]")
            }
            ItemContent::JSON(s) => {
                write!(f, "{{")?;
                let mut iter = s.iter();
                if let Some(a) = iter.next() {
                    write!(f, "{}", a)?;
                }
                while let Some(a) = iter.next() {
                    write!(f, ", {}", a)?;
                }
                write!(f, "}}")
            }
            ItemContent::Format(k, v) => write!(f, "<{}={}>", k, v),
            ItemContent::Deleted(s) => write!(f, "deleted({})", s),
            ItemContent::Binary(s) => write!(f, "{:?}", s),
            ItemContent::Type(inner) => match &inner.type_ref {
                TypeRef::Array => {
                    if let Some(ptr) = inner.start {
                        write!(f, "<array(head: {})>", ptr)
                    } else {
                        write!(f, "<array>")
                    }
                }
                TypeRef::Map => {
                    write!(f, "<map({{")?;
                    let mut iter = inner.map.iter();
                    if let Some((k, ptr)) = iter.next() {
                        write!(f, "'{}': {}", k, ptr)?;
                    }
                    while let Some((k, ptr)) = iter.next() {
                        write!(f, ", '{}': {}", k, ptr)?;
                    }
                    write!(f, "}})>")
                }
                TypeRef::Text => {
                    if let Some(start) = inner.start {
                        write!(f, "<text(head: {})>", start)
                    } else {
                        write!(f, "<text>")
                    }
                }
                TypeRef::XmlElement(name) => {
                    write!(f, "<xml element: {}>", name)
                }
                TypeRef::XmlFragment => write!(f, "<xml fragment>"),
                TypeRef::XmlHook => write!(f, "<xml hook>"),
                TypeRef::XmlText => write!(f, "<xml text>"),
                #[cfg(feature = "weak")]
                TypeRef::WeakLink(s) => write!(f, "<weak({}..{})>", s.quote_start, s.quote_end),
                _ => write!(f, "<undefined type ref>"),
            },
            ItemContent::Move(m) => std::fmt::Display::fmt(m.as_ref(), f),
            ItemContent::Doc(_, doc) => std::fmt::Display::fmt(doc, f),
            _ => Ok(()),
        }
    }
}

impl std::fmt::Display for ItemPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(index: {}", self.index)?;
        if let Some(l) = self.left.as_ref() {
            write!(f, ", left: {}", l)?;
        }
        if let Some(r) = self.right.as_ref() {
            write!(f, ", right: {}", r)?;
        }
        write!(f, ")")
    }
}

/// A trait used for preliminary types, that can be inserted into shared Yrs collections.
pub trait Prelim: Sized {
    /// Type of a value to be returned as a result of inserting this [Prelim] type instance.
    /// Use [Unused] if none is necessary.
    type Return: TryFrom<ItemPtr>;

    /// This method is used to create initial content required in order to create a block item.
    /// A supplied `ptr` can be used to identify block that is about to be created to store
    /// the returned content.
    ///
    /// Since this method may decide to consume `self` or not, a second optional return parameter
    /// is used when `self` was not consumed - which is the case for complex types creation such as
    /// YMap or YArray. In such case it will be passed later on to [Self::integrate] method.
    fn into_content(self, txn: &mut TransactionMut) -> (ItemContent, Option<Self>);

    /// Method called once an original item filled with content from [Self::into_content] has been
    /// added to block store. This method is used by complex types such as maps or arrays to append
    /// the original contents of prelim struct into YMap, YArray etc.
    fn integrate(self, txn: &mut TransactionMut, inner_ref: BranchPtr);
}

impl<T> Prelim for T
where
    T: Into<Any>,
{
    type Return = Unused;

    fn into_content(self, _txn: &mut TransactionMut) -> (ItemContent, Option<Self>) {
        let value: Any = self.into();
        (ItemContent::Any(vec![value]), None)
    }

    fn integrate(self, _txn: &mut TransactionMut, _inner_ref: BranchPtr) {}
}

#[derive(Debug)]
pub(crate) struct PrelimString(pub SmallString<[u8; 8]>);

impl Prelim for PrelimString {
    type Return = Unused;

    fn into_content(self, _txn: &mut TransactionMut) -> (ItemContent, Option<Self>) {
        (ItemContent::String(self.0.into()), None)
    }

    fn integrate(self, _txn: &mut TransactionMut, _inner_ref: BranchPtr) {}
}

/// Empty type marker, which can be used by a [Prelim] trait implementations when no integrated
/// value should be returned after prelim type has been integrated as a result of insertion.
#[repr(transparent)]
pub struct Unused;

impl TryFrom<ItemPtr> for Unused {
    type Error = ItemPtr;

    #[inline(always)]
    fn try_from(_: ItemPtr) -> Result<Self, Self::Error> {
        Ok(Unused)
    }
}

/// Prelim container for types passed over to [Text::insert_embed] and [Text::insert_embed_with_attributes] methods.
#[derive(Debug)]
pub enum EmbedPrelim<T> {
    Primitive(Any),
    Shared(T),
}

impl<T> Prelim for EmbedPrelim<T>
where
    T: Prelim,
{
    type Return = T::Return;

    fn into_content(self, txn: &mut TransactionMut) -> (ItemContent, Option<Self>) {
        match self {
            EmbedPrelim::Primitive(any) => (ItemContent::Embed(any), None),
            EmbedPrelim::Shared(prelim) => {
                let (branch, content) = prelim.into_content(txn);
                let carrier = if let Some(carrier) = content {
                    Some(EmbedPrelim::Shared(carrier))
                } else {
                    None
                };
                (branch, carrier)
            }
        }
    }

    fn integrate(self, txn: &mut TransactionMut, inner_ref: BranchPtr) {
        if let EmbedPrelim::Shared(carrier) = self {
            carrier.integrate(txn, inner_ref)
        }
    }
}

impl<T> From<T> for EmbedPrelim<T>
where
    T: Into<Any>,
{
    fn from(value: T) -> Self {
        EmbedPrelim::Primitive(value.into())
    }
}

impl std::fmt::Display for ID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{}#{}>", self.client, self.clock)
    }
}

impl std::fmt::Debug for ItemPtr {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::fmt::Display for ItemPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({})", self.id())
    }
}

#[cfg(test)]
mod test {
    use crate::block::{split_str, SplittableString};
    use crate::doc::OffsetKind;
    use std::ops::Deref;

    #[test]
    fn splittable_string_len() {
        let s: SplittableString = "Zażółć gęślą jaźń😀 女".into();

        assert_eq!(s.len(OffsetKind::Bytes), 34, "wrong byte length");
        assert_eq!(s.len(OffsetKind::Utf16), 21, "wrong UTF-16 length");
    }

    #[test]
    fn splittable_string_push_str() {
        let mut s: SplittableString = "Zażółć gęślą jaźń😀".into();
        s.push_str("ありがとうございます");

        assert_eq!(
            s.deref(),
            &"Zażółć gęślą jaźń😀ありがとうございます".to_string()
        );

        assert_eq!(s.len(OffsetKind::Bytes), 60, "wrong byte length");
        assert_eq!(s.len(OffsetKind::Utf16), 29, "wrong UTF-16 length");
    }

    #[test]
    fn splittable_string_split_str() {
        let s: SplittableString = "Zażółć gęślą jaźń😀ありがとうございます".into();

        if let Some((a, b)) = split_str(&s, 18, OffsetKind::Utf32) {
            assert_eq!(a, "Zażółć gęślą jaźń😀");
            assert_eq!(b, "ありがとうございます");
        } else {
            panic!("split_str did not return any value for Utf32 offset 18");
        }

        if let Some((a, b)) = split_str(&s, 19, OffsetKind::Utf16) {
            assert_eq!(a, "Zażółć gęślą jaźń😀");
            assert_eq!(b, "ありがとうございます");
        } else {
            panic!("split_str did not return any value for Utf32 offset 18");
        }
    }
}
