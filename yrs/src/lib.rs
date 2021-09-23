#![feature(new_uninit)]
#![feature(map_entry_replace)]
//! Yrs "wires" is a high performance CRDT implementation based on the idea of **Shared
//! Types**. It is a compatible port of the [Yjs](https://github.com/yjs/yjs) CRDT.
//!
//! **Shared Types** work just like normal data types, but they automatically
//! sync with other peers. In Rust, they can automatically sync with their
//! counterparts in other threads.
//!
//! A **Shared Document** is the access point to create shared types,
//! and to listen to update events.
//!
//! # Implement a Provider
//!
//! A **provider** connects to a shared document and automatically syncs updates
//! through a medium. A provider could sync document updates to other peers through
//! a network protocol, or sync document updates to a database so that they are
//! available without a network connection. You can combine providers with each
//! other to make your application more resilient.
//!
//! In Yjs, we already have a rich collection of providers that allow you to
//! build resilient applications that sync through multiple communication
//! mediums all at once. We don't have this ecosystem yet in Yrs, but you can
//! build them easily on your own.

mod alt;
pub mod block;
mod block_store;
mod doc;
mod event;
mod id_set;
mod store;
mod transaction;
pub mod types;
mod update;
pub mod updates;
mod utils;

#[cfg(test)]
mod compatibility_tests;

#[cfg(test)]
mod test_utils;

pub use crate::alt::{diff_updates, encode_state_vector_from_update, merge_updates};
pub use crate::block::ID;
pub use crate::block_store::StateVector;
pub use crate::doc::Doc;
pub use crate::id_set::DeleteSet;
pub use crate::transaction::Transaction;
pub use crate::types::array::Array;
pub use crate::types::array::PrelimArray;
pub use crate::types::map::Map;
pub use crate::types::map::PrelimMap;
pub use crate::types::text::Text;
pub use crate::types::xml::Xml;
pub use crate::types::xml::XmlElement;
pub use crate::types::xml::XmlText;
pub use crate::update::Update;
