use crate::block::{
    Block, BlockPtr, Item, ItemContent, Skip, BLOCK_GC_REF_NUMBER, BLOCK_SKIP_REF_NUMBER, GC,
    HAS_ORIGIN, HAS_PARENT_SUB, HAS_RIGHT_ORIGIN,
};
#[cfg(test)]
use crate::store::Store;
use crate::types::TypePtr;
use crate::updates::decoder::{Decode, Decoder};
use crate::updates::encoder::{Encode, Encoder};
use crate::utils::client_hasher::ClientHasher;
use crate::{StateVector, Transaction, ID};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::hash::BuildHasherDefault;
use std::rc::Rc;

type ClientBlocks = HashMap<u64, VecDeque<Block>, BuildHasherDefault<ClientHasher>>;

#[derive(Debug, PartialEq, Default)]
pub struct Update {
    clients: ClientBlocks,
}

impl Update {
    pub fn new () -> Self {
        Self::default()
    }
    /**
     @todo this should be refactored.
     I'm currently using this to add blocks to the Update
     */
    pub fn add_block (&mut self, block: &Block, offset: u32) {
        let copy = block.slice(offset);
        match self.clients.entry(copy.id().client) {
            Entry::Occupied(e) => {
                e.into_mut().push_back(copy)
            }
            Entry::Vacant(e) => {
                let mut q = VecDeque::new();
                q.push_back(copy);
                e.insert(q);
            }
        }
    }
    pub fn is_empty (&self) -> bool {
        self.clients.len() == 0
    }
    pub fn state_vector(&self) -> StateVector {
        let mut sv = StateVector::default();
        for (&client, blocks) in self.clients.iter() {
            let last_id = blocks[blocks.len() - 1].last_id();
            sv.set_max(client, last_id.clock + 1);
        }
        sv
    }

    /// Returns an iterator that allows a traversal of all of the blocks
    /// which consist into this [Update].
    pub fn blocks(&self) -> Blocks<'_> {
        Blocks::new(self)
    }

    pub fn merge(&mut self, other: Self) {
        for (client, other_blocks) in other.clients {
            println!("{:?}", other_blocks);
            match self.clients.entry(client) {
                Entry::Occupied(e) => {
                    let mut blocks = e.into_mut();
                    println!("{:?}", blocks);

                    let mut i2 = other_blocks.into_iter();
                    let mut n2 = i2.next();

                    let mut i1 = 0;

                    while i1 < blocks.len() {
                        let a = &mut blocks[i1];
                        if let Some(b) = n2.as_ref() {
                            if a.try_merge(b) {
                                n2 = i2.next();
                                continue;
                            } else if let Block::Item(a) = a {
                                // we only can split Block::Item
                                let diff = (a.id.clock + a.len()) as isize - b.id().clock as isize;
                                if diff > 0 {
                                    // `b`'s clock position is inside of `a` -> we need to split `a`
                                    self.split_item(client, i1, diff as u32);
                                    blocks = self.clients.get_mut(&client).unwrap();
                                }
                            }
                            i1 += 1;
                            n2 = i2.next();
                        } else {
                            break;
                        }
                    }

                    while let Some(b) = n2 {
                        blocks.push_back(b);
                        n2 = i2.next();
                    }
                }
                Entry::Vacant(e) => {
                    e.insert(other_blocks);
                }
            }
        }
    }

    fn split_item(&mut self, client: u64, mut index: usize, diff: u32) {
        let mut blocks = self.clients.get_mut(&client).unwrap();
        if let Block::Item(item) = &mut blocks[index] {
            index += 1;
            let right_split = item.split(diff);
            let right_ptr = right_split.right.clone();
            if let Some(right_ptr) = right_ptr {
                blocks = if right_ptr.id.client == client {
                    blocks
                } else {
                    self.clients.get_mut(&right_ptr.id.client).unwrap()
                };
                let right = &mut blocks[right_ptr.pivot()];
                if let Some(right_item) = right.as_item_mut() {
                    right_item.left = Some(BlockPtr::new(right_split.id.clone(), index as u32));
                }
            }
            blocks.insert(index, Block::Item(right_split));
        };
    }

    pub fn integrate(mut self, txn: &mut Transaction<'_>) -> Option<PendingUpdate> {
        if self.clients.is_empty() {
            return None;
        }
        let mut client_block_ref_ids: Vec<u64> = self.clients.keys().cloned().collect();
        client_block_ref_ids.sort_by(|a, b| b.cmp(a));

        let mut current_client_id = client_block_ref_ids.pop();
        let mut current_target = current_client_id.and_then(|id| self.clients.get_mut(&id));
        let mut stack_head = Self::next(&mut current_target);

        let mut local_sv = txn.store.blocks.get_state_vector();
        let mut missing_sv = StateVector::default();
        let mut remaining = ClientBlocks::default();
        let mut stack = Vec::new();

        while let Some(mut block) = stack_head {
            let id = block.id();
            if local_sv.contains(id) {
                let offset = local_sv.get(&id.client) - id.clock;
                if let Some(dep) = Self::missing(&block, &local_sv) {
                    stack.push(block);
                    // get the struct reader that has the missing struct
                    let block_refs = txn.store.blocks.get_client_blocks_mut(dep);
                    if block_refs.integrated_len() == block_refs.len() {
                        // This update message causally depends on another update message that doesn't exist yet
                        missing_sv.set_min(dep, local_sv.get(&dep));
                        Self::return_stack(stack, &mut self.clients, &mut remaining);
                        current_target = current_client_id.and_then(|id| self.clients.get_mut(&id));
                        stack = Vec::new();
                    } else {
                        stack_head = Self::next(&mut current_target);
                        continue;
                    }
                } else {
                    let client = id.client;
                    local_sv.set_max(client, id.clock + block.len());
                    block.as_item_mut().map(|item| item.repair(txn));
                    let should_delete = block.integrate(txn, offset, offset);
                    let delete_ptr = if should_delete {
                        Some(BlockPtr::new(block.id().clone(), offset))
                    } else {
                        None
                    };

                    let blocks = txn.store.blocks.get_client_blocks_mut(client);
                    blocks.push(block);

                    if let Some(ptr) = delete_ptr {
                        txn.delete(&ptr);
                    }
                }
            } else {
                // update from the same client is missing
                stack.push(block);
                // hid a dead wall, add all items from stack to restSS
                Self::return_stack(stack, &mut self.clients, &mut remaining);
                current_target = current_client_id.and_then(|id| self.clients.get_mut(&id));
                stack = Vec::new();
            }

            // iterate to next stackHead
            if !stack.is_empty() {
                stack_head = stack.pop();
            } else {
                current_target = match current_target.take() {
                    None => None,
                    Some(v) => {
                        if !v.is_empty() {
                            Some(v)
                        } else {
                            if let Some((client_id, target)) =
                                Self::next_target(&mut client_block_ref_ids, &mut self.clients)
                            {
                                current_client_id = Some(client_id);
                                Some(target)
                            } else {
                                current_client_id = None;
                                None
                            }
                        }
                    }
                };
                stack_head = Self::next(&mut current_target);
            }
        }

        if remaining.is_empty() {
            None
        } else {
            Some(PendingUpdate {
                update: Update { clients: remaining },
                missing: missing_sv,
            })
        }
    }

    fn next(target: &mut Option<&mut VecDeque<Block>>) -> Option<Block> {
        if let Some(v) = target {
            v.pop_front()
        } else {
            None
        }
    }

    fn missing(block: &Block, local_sv: &StateVector) -> Option<u64> {
        if let Block::Item(item) = block {
            if let Some(origin) = item.origin {
                if origin.client != item.id.client && !local_sv.contains(&item.id) {
                    return Some(origin.client);
                }
            } else if let Some(right_origin) = item.right_origin {
                if right_origin.client != item.id.client && !local_sv.contains(&item.id) {
                    return Some(right_origin.client);
                }
            } else if let TypePtr::Id(parent) = item.parent {
                if parent.id.client != item.id.client && !local_sv.contains(&item.id) {
                    return Some(parent.id.client);
                }
            }
        }
        None
    }

    fn next_target<'a, 'b>(
        client_block_ref_ids: &'a mut Vec<u64>,
        clients: &'b mut ClientBlocks,
    ) -> Option<(u64, &'b mut VecDeque<Block>)> {
        loop {
            if let Some((id, Some(client_blocks))) = client_block_ref_ids
                .pop()
                .map(move |id| (id, clients.get_mut(&id)))
            {
                if !client_blocks.is_empty() {
                    return Some((id, client_blocks));
                }
            }

            break;
        }
        None
    }

    fn return_stack(stack: Vec<Block>, refs: &mut ClientBlocks, remaining: &mut ClientBlocks) {
        for item in stack.into_iter() {
            let client = item.id().client;
            // remove client from clientsStructRefsIds to prevent users from applying the same update again
            if let Some(mut unapplicable_items) = refs.remove(&client) {
                // decrement because we weren't able to apply previous operation
                unapplicable_items.push_front(item);
                remaining.insert(client, unapplicable_items);
            } else {
                // item was the last item on clientsStructRefs and the field was already cleared.
                // Add item to restStructs and continue
                let mut blocks = VecDeque::with_capacity(1);
                blocks.push_back(item);
                remaining.insert(client, blocks);
            }
        }
    }

    fn decode_block<D: Decoder>(id: ID, decoder: &mut D) -> Block {
        let info = decoder.read_info();
        match info {
            BLOCK_SKIP_REF_NUMBER => {
                let len: u32 = decoder.read_uvar();
                Block::Skip(Skip { id, len })
            }
            BLOCK_GC_REF_NUMBER => {
                let len: u32 = decoder.read_uvar();
                Block::GC(GC { id, len })
            }
            info => {
                let cant_copy_parent_info = info & (HAS_ORIGIN | HAS_RIGHT_ORIGIN) == 0;
                let origin = if info & HAS_ORIGIN != 0 {
                    Some(decoder.read_left_id())
                } else {
                    None
                };
                let right_origin = if info & HAS_RIGHT_ORIGIN != 0 {
                    Some(decoder.read_right_id())
                } else {
                    None
                };
                let parent = if cant_copy_parent_info {
                    if decoder.read_parent_info() {
                        TypePtr::Named(Rc::new(decoder.read_string().to_owned()))
                    } else {
                        TypePtr::Id(BlockPtr::from(decoder.read_left_id()))
                    }
                } else {
                    let parent = if let Some(id) = origin.as_ref() {
                        // @TODO: this should return origin.parent, not the origin
                        id.clone()
                    } else if let Some(id) = right_origin.as_ref() {
                        // @TODO: this should return origin.parent, not the origin
                        id.clone()
                    } else {
                        panic!(
                            "Couldn't decode item (id: {:?}) - no parent was provided",
                            id
                        )
                    };
                    TypePtr::Id(BlockPtr::from(parent))
                };
                let parent_sub = if cant_copy_parent_info && (info & HAS_PARENT_SUB != 0) {
                    Some(decoder.read_string().to_owned())
                } else {
                    None
                };
                let content = ItemContent::decode(decoder, info, BlockPtr::from(id.clone()));
                let item = Item::new(
                    id,
                    None,
                    origin,
                    None,
                    right_origin,
                    parent,
                    parent_sub,
                    content,
                );
                Block::Item(item)
            }
        }
    }

    pub(crate) fn encode_diff<E: Encoder>(&self, remote_sv: &StateVector, encoder: &mut E) {
        let mut clients = HashMap::new();
        for (client, blocks) in self.clients.iter() {
            let remote_clock = remote_sv.get(client);
            let mut iter = blocks.iter();
            let mut curr = iter.next();
            while let Some(block) = curr {
                if let Block::Skip(_) = block {
                    curr = iter.next();
                } else if block.id().clock + block.len() > remote_clock {
                    let e = clients.entry(*client).or_insert_with(|| (0, Vec::new()));
                    e.0 = (remote_clock as i64 - block.id().clock as i64).max(0) as u32;
                    e.1.push(block);
                    curr = iter.next();
                    while let Some(block) = curr {
                        e.1.push(block);
                        curr = iter.next();
                    }
                } else {
                    // read until something new comes up
                    curr = iter.next();
                }
            }
        }

        // Write higher clients first ⇒ sort by clientID & clock and remove decoders without content
        let mut sorted_clients: Vec<_> =
            clients.iter().filter(|(_, (_, q))| !q.is_empty()).collect();
        sorted_clients.sort_by(|&(x_id, _), &(y_id, _)| y_id.cmp(x_id));

        // finish lazy struct writing
        encoder.write_uvar(sorted_clients.len());
        for (&client, (offset, blocks)) in sorted_clients {
            encoder.write_uvar(blocks.len());
            encoder.write_uvar(client);

            let mut block = blocks[0];
            encoder.write_uvar(block.id().clock + offset);
            block.encode_with_offset(encoder, *offset);
            for i in 1..blocks.len() {
                block = blocks[i];
                block.encode_with_offset(encoder, 0);
            }
        }
    }
}

impl Encode for Update {
    fn encode<E: Encoder>(&self, encoder: &mut E) {
        self.encode_diff(&StateVector::default(), encoder);
    }
}

impl Decode for Update {
    fn decode<D: Decoder>(decoder: &mut D) -> Self {
        let clients_len: u32 = decoder.read_uvar();
        let mut clients =
            HashMap::with_capacity_and_hasher(clients_len as usize, BuildHasherDefault::default());
        for _ in 0..clients_len {
            let blocks_len = decoder.read_uvar::<u32>() as usize;

            let client = decoder.read_client();
            let mut clock: u32 = decoder.read_uvar();
            let blocks = clients
                .entry(client)
                .or_insert_with(|| VecDeque::with_capacity(blocks_len));

            for _ in 0..blocks_len {
                let id = ID::new(client, clock);
                let block = Self::decode_block(id, decoder);
                clock += block.len();
                blocks.push_back(block);
            }
        }

        Update { clients }
    }
}

#[derive(Debug, PartialEq)]
pub struct PendingUpdate {
    pub update: Update,
    pub missing: StateVector,
}

impl PendingUpdate {
    fn merge(&mut self, other: Self) {
        self.update.merge(other.update);
        self.missing.merge(other.missing);
    }
}

impl std::fmt::Display for Update {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{{")?;
        for (k, v) in self.clients.iter() {
            writeln!(f, "\t{} -> [", k)?;
            for block in v.iter() {
                writeln!(f, "\t\t{}", block)?;
            }
            writeln!(f, "\t]")?;
        }
        writeln!(f, "}}")
    }
}

/// Conversion for tests only
#[cfg(test)]
impl Into<Store> for Update {
    fn into(self) -> Store {
        let mut store = Store::new(0);
        for (client_id, vec) in self.clients {
            let blocks = store
                .blocks
                .get_client_blocks_with_capacity_mut(client_id, vec.len());
            for block in vec {
                blocks.push(block);
            }
        }
        store
    }
}

pub struct Blocks<'a> {
    current_client: std::vec::IntoIter<(&'a u64, &'a VecDeque<Block>)>,
    current_block: Option<std::collections::vec_deque::Iter<'a, Block>>,
    pub current: Option<&'a Block>,
}

impl<'a> Blocks<'a> {
    fn new(update: &'a Update) -> Self {
        let mut client_blocks: Vec<(&'a u64, &'a VecDeque<Block>)> = update.clients.iter().collect();
        // sorting to return higher client ids first
        client_blocks.sort_by(|a, b| b.0.cmp(a.0));
        let mut current_client = client_blocks.into_iter();

        let current_block = current_client.next().map(|(_, v)| v.iter());
        Blocks {
            current_client,
            current_block,
            current: None
        }
    }

    fn _next(&mut self) -> Option<&'a Block> {
        if let Some(blocks) = self.current_block.as_mut() {
            let block = blocks.next();
            if block.is_some() {
                return block;
            }
        }

        if let Some(entry) = self.current_client.next() {
            self.current_block = Some(entry.1.iter());
            self.next()
        } else {
            None
        }
    }
}

impl<'a> Iterator for Blocks<'a> {
    type Item = &'a Block;

    fn next(&mut self) -> Option<Self::Item> {
        self.current = self._next();
        self.current
    }
}

#[cfg(test)]
mod test {
    use crate::block::{Block, Item, ItemContent};
    use crate::id_set::DeleteSet;
    use crate::types::TypePtr;
    use crate::update::Update;
    use crate::updates::decoder::{Decode, DecoderV1};
    use crate::{Doc, ID};
    use lib0::decoding::Cursor;
    use std::rc::Rc;

    #[test]
    fn update_decode() {
        /* Generated with:

           ```js
           var Y = require('yjs');

           var doc = new Y.Doc()
           var map = doc.getMap()
           map.set('keyB', 'valueB')

           // Merge changes from remote
           var update = Y.encodeStateAsUpdate(doc)
           ```
        */
        let update: &[u8] = &[
            1, 1, 176, 249, 159, 198, 7, 0, 40, 1, 0, 4, 107, 101, 121, 66, 1, 119, 6, 118, 97,
            108, 117, 101, 66, 0,
        ];
        let mut decoder = DecoderV1::from(update);
        let u = Update::decode(&mut decoder);

        let id = ID::new(2026372272, 0);
        let block = u.clients.get(&id.client).unwrap();
        let mut expected = Vec::new();
        expected.push(Block::Item(Item::new(
            id,
            None,
            None,
            None,
            None,
            TypePtr::Named(Rc::new("".to_owned())),
            Some("keyB".to_owned()),
            ItemContent::Any(vec!["valueB".into()]),
        )));
        assert_eq!(block, &expected);
    }

    #[test]
    fn update_merge() {
        let d1 = Doc::new();
        let mut t1 = d1.transact();
        let txt1 = t1.get_text("test");

        let d2 = Doc::new();
        let mut t2 = d2.transact();
        let txt2 = t2.get_text("test");

        txt1.insert(&mut t1, 0, "aaa");
        txt1.insert(&mut t1, 0, "aaa");

        txt1.insert(&mut t1, 0, "bbb");
        txt1.insert(&mut t1, 2, "bbb");

        let binary1 = t1.encode_update();
        let binary2 = t2.encode_update();

        d1.apply_update(&mut t1, binary2.as_slice());
        d2.apply_update(&mut t2, binary1.as_slice());

        let mut u1 = Update::decode(&mut DecoderV1::new(Cursor::new(binary1.as_slice())));
        let u2 = Update::decode(&mut DecoderV1::new(Cursor::new(binary2.as_slice())));

        // a crux of our test: merged update upon applying should produce
        // the same output as sequence of updates applied individually
        u1.merge(u2);

        let d3 = Doc::new();
        let mut t3 = d3.transact();
        let txt3 = t3.get_text("test");
        t3.apply_update(u1, DeleteSet::default());

        let str1 = txt1.to_string(&t1);
        let str2 = txt2.to_string(&t2);
        let str3 = txt3.to_string(&t3);

        assert_eq!(str1, str2);
        assert_eq!(str2, str3);
    }
}
