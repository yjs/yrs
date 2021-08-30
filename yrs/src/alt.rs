use crate::block::{Block, Skip};
use crate::id_set::DeleteSet;
use crate::update::{Blocks, Update};
use crate::updates::decoder::{Decode, Decoder, DecoderV1};
use crate::updates::encoder::{Encode, Encoder, EncoderV1};
use crate::{ID, StateVector};
use lib0::decoding::Cursor;

use std::cmp::Ordering;

pub fn merge_updates(updates: &[&[u8]]) -> Vec<u8> {
    match updates.len() {
        0 => vec![0, 0],
        1 => updates[0].to_vec(),
        _ => {
            /*
            let mut iter = updates.iter();
            let mut decoder = DecoderV1::new(Cursor::new(iter.next().unwrap()));
            let mut update = Update::decode(&mut decoder);
            let mut ds = DeleteSet::decode(&mut decoder);

            while let Some(data) = iter.next() {
                let mut decoder = DecoderV1::new(Cursor::new(data));
                let u = Update::decode(&mut decoder);
                let d = DeleteSet::decode(&mut decoder);
                println!("inner {:?}", u);
                update.merge(u);
                println!("{:?}", update);
                println!("inner merged {:?}", update);
                ds.merge(&d);
            }

            let mut encoder = EncoderV1::new();
            update.encode_diff(&StateVector::default(), &mut encoder);
            ds.encode(&mut encoder);
            encoder.to_vec()
            */
            let mut block_stores = Vec::with_capacity(updates.len());
            let mut delete_sets = Vec::with_capacity(updates.len());
            updates.iter().for_each(|update| {
                let mut decoder = DecoderV1::new(Cursor::new(update));
                block_stores.push(Update::decode(&mut decoder));
                delete_sets.push(DeleteSet::decode(&mut decoder));
            });
            let blocks = merge_blocks(block_stores);
            let ds = merge_delete_sets(delete_sets);
            let mut encoder = EncoderV1::new();
            blocks.encode_diff(&StateVector::default(), &mut encoder);
            ds.encode(&mut encoder);
            encoder.to_vec()
        }
    }
}

pub fn merge_delete_sets(mut dss: Vec<DeleteSet>) -> DeleteSet {
    // this expects a list of size >= 1!
    let mut ds_iter = dss.iter_mut();
    let ds = ds_iter.next().unwrap();
    ds_iter.for_each(|other| ds.merge(other));
    ds.clone()
}

pub fn merge_blocks(block_stores: Vec<Update>) -> Update {
    let mut result = Update::new();
    let mut lazy_struct_decoders: Vec<Blocks> = block_stores.iter()
        .filter(|block_store| !block_store.is_empty())
        .map(|block_store| {
            let mut blocks = block_store.blocks();
            blocks.next(); // call next so that `blocks.current` is defined
            blocks
        }).collect();

    let mut curr_write: Option<Block> = None;

    // Note: We need to ensure that all lazyStructDecoders are fully consumed
    // Note: Should merge document updates whenever possible - even from different updates
    // Note: Should handle that some operations cannot be applied yet ()
    loop {
        // Remove the decoder if we consumed all of its blocks.
        if lazy_struct_decoders[0].current.is_none() {
            lazy_struct_decoders.remove(0);
        }
        // Write higher clients first ⇒ sort by clientID & clock and remove decoders without content
        lazy_struct_decoders.sort_by(|dec1, dec2| {
            let left = dec1.current.unwrap();
            let right= dec2.current.unwrap();
            if left.id().client == right.id().client {
                let clock_diff = left.id().clock - right.id().clock;
                if clock_diff == 0 {
                    return if left.same_type(right) { Ordering::Equal } else {
                        if left.is_skip() { Ordering::Greater } else { Ordering::Less }
                    }
                } else {
                    if clock_diff <= 0 && (!left.is_skip() || right.is_skip()) { Ordering::Less } else { Ordering::Greater }
                }
            } else {
                right.id().client.cmp(&left.id().client)
            }
        });

        if lazy_struct_decoders.is_empty() {
            break
        }

        let curr_decoder = &mut lazy_struct_decoders[0];

        // write from currDecoder until the next operation is from another client or if filler-struct
        // then we need to reorder the decoders and find the next operation to write
        let mut curr: Option<&Block> = curr_decoder.current;
        let mut tmp_curr = None;
        let first_client = curr.unwrap().id().client;

        if let Some(curr_write_block) = &mut curr_write {
            let mut iterated = false;

            // iterate until we find something that we haven't written already
            // remember: first the high client-ids are written

            loop {
                if let Some(_curr) = &curr {
                    if
                        _curr.id().clock + _curr.len() < curr_write_block.id().clock + curr_write_block.len()
                        && _curr.id().client >= curr_write_block.id().client
                    {
                        curr = curr_decoder.next();
                        iterated = true;
                        continue
                    }
                }
                break
            }
            if curr.is_none() { // continue if decoder is empty
                continue
            }
            let curr_unwrapped = curr.clone().unwrap();
            if
                // check whether there is another decoder that has has updates from `first_client`
                curr_unwrapped.id().client != first_client ||
                // the above while loop was used and we are potentially missing updates
                (iterated && curr_unwrapped.id().clock > curr_write_block.id().clock + curr_write_block.len())

            {
                continue
            }

            if first_client != curr_write_block.id().client {
                result.add_block(&curr_unwrapped, 0);
                curr = curr_decoder.next();
            } else {
                if curr_write_block.id().clock + curr_write_block.len() < curr_unwrapped.id().clock {
                    // @todo write currStruct & set currStruct = Skip(clock = currStruct.id.clock + currStruct.length, length = curr.id.clock - self.clock)
                    if let Block::Skip(curr_write_skip) = curr_write_block {
                        // extend existing skip
                        let len = curr_unwrapped.id().clock + curr_unwrapped.len() - curr_write_skip.id.clock;
                        curr_write = Some(Block::Skip(Skip { id: curr_write_skip.id, len }));
                    } else {
                        result.add_block(curr_write_block, 0);
                        let diff = curr_unwrapped.id().clock - curr_write_block.id().clock - curr_write_block.len();

                        let next_id = ID { client: first_client, clock: curr_write_block.id().clock + curr_write_block.len() };

                        curr_write = Some(Block::Skip(Skip { id: next_id, len: diff }));
                    }
                } else { // if (currWrite.struct.id.clock + currWrite.struct.length >= curr.id.clock) {

                    let diff = curr_write_block.id().clock + curr_write_block.len() - curr_unwrapped.id().clock;
                    if diff > 0 {
                        if let Block::Skip(curr_write_skip) = curr_write_block {
                            // prefer to slice Skip because the other struct might contain more information
                            curr_write_skip.len -= diff;
                        } else {
                            tmp_curr = Some(curr_unwrapped.slice(diff));
                            curr = tmp_curr.as_ref();
                        }
                    }
                    if curr_write_block.try_merge(&curr_unwrapped) {
                        result.add_block(curr_write_block, 0);
                        curr_write = Some(curr_unwrapped.clone());
                        curr = curr_decoder.next();
                    }
                }
            }
        } else {
            curr_write = curr_decoder.current.map(|block| block.clone()); // curr.clone();
            curr = curr_decoder.next();
        }
        loop {
            if let Some(next) = curr {
                let curr_write_block = curr_write.as_ref().unwrap();
                if next.id().client == first_client && next.id().clock == curr_write_block.id().clock + curr_write_block.len() {
                    result.add_block(&curr_write.unwrap(), 0);
                    curr_write = Some(next.slice(0));
                    curr_decoder.next();
                    continue
                }
            }
            break
        }
    }
    if let Some(curr_write_block) = &curr_write {
        result.add_block(&curr_write_block, 0);
    }
    result
}

// Computes the state vector from a document update
pub fn encode_state_vector_from_update(update: &[u8]) -> Vec<u8> {
    let update = Update::decode_v1(update);
    update.state_vector().encode_v1()
}

// Encode the missing differences to another document update.
pub fn diff_updates(update: &[u8], state_vector: &[u8]) -> Vec<u8> {
    let sv = StateVector::decode_v1(state_vector);
    let cursor = Cursor::new(update);
    let mut decoder = DecoderV1::new(cursor);
    let update = Update::decode(&mut decoder);

    let mut encoder = EncoderV1::new();
    update.encode_diff(&sv, &mut encoder);

    // for delete set, don't decode/encode it - just copy the remaining part from the decoder
    let mut result = encoder.to_vec();
    result.extend_from_slice(decoder.read_to_end());
    result
}

#[cfg(test)]
mod test {
    use crate::{diff_updates, encode_state_vector_from_update, merge_updates};

    #[test]
    fn merge_updates_compatibility() {
        let a = &[
            1, 1, 220, 240, 237, 172, 15, 0, 4, 1, 4, 116, 101, 115, 116, 3, 97, 98, 99, 0,
        ];
        let b = &[
            1, 1, 201, 139, 250, 201, 1, 0, 4, 1, 4, 116, 101, 115, 116, 2, 100, 101, 0,
        ];
        let expected = &[
            2, 1, 220, 240, 237, 172, 15, 0, 4, 1, 4, 116, 101, 115, 116, 3, 97, 98, 99, 1, 201,
            139, 250, 201, 1, 0, 4, 1, 4, 116, 101, 115, 116, 2, 100, 101, 0,
        ];

        let actual = merge_updates(&[a, b]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn merge_updates_compatibility_2() {
        let a = &[
            1, 1, 129, 231, 135, 164, 7, 0, 4, 1, 4, 49, 50, 51, 52, 1, 97, 0
        ];
        let b = &[
            1, 1, 129, 231, 135, 164, 7, 1, 68, 129, 231, 135, 164, 7, 0, 1, 98, 0
        ];
        let expected = &[
            1, 2, 129, 231, 135, 164, 7, 0, 4, 1, 4, 49, 50, 51, 52, 1, 97, 68, 129, 231, 135, 164, 7, 0, 1, 98, 0
        ];

        let actual = merge_updates(&[a, b]);
        println!("actual {:?}", actual);
        assert_eq!(actual, expected);
    }


    #[test]
    fn encode_state_vector_compatibility() {
        let update = &[
            2, 1, 220, 240, 237, 172, 15, 0, 4, 1, 4, 116, 101, 115, 116, 3, 97, 98, 99, 1, 201,
            139, 250, 201, 1, 0, 4, 1, 4, 116, 101, 115, 116, 2, 100, 101, 0,
        ];
        let expected = &[2, 220, 240, 237, 172, 15, 3, 201, 139, 250, 201, 1, 2];
        let actual = encode_state_vector_from_update(update);
        assert_eq!(actual, expected);
    }

    #[test]
    fn diff_updates_compatibility() {
        let state_vector = &[1, 148, 189, 145, 162, 9, 3];
        let update = &[
            1, 2, 148, 189, 145, 162, 9, 0, 4, 1, 4, 116, 101, 115, 116, 3, 97, 98, 99, 68, 148,
            189, 145, 162, 9, 0, 2, 100, 101, 0,
        ];
        let expected = &[
            1, 1, 148, 189, 145, 162, 9, 3, 68, 148, 189, 145, 162, 9, 0, 2, 100, 101, 0,
        ];
        let actual = diff_updates(update, state_vector);
        assert_eq!(actual, expected);
    }
}
