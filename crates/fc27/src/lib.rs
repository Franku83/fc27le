//! FC 27-specific knowledge, limited to what can be verified.
//!
//! The crate separates four kinds of information:
//!
//! * process discovery heuristics ([`discovery`]),
//! * build identity ([`build_identity`]),
//! * byte-pattern anchors taken from the existing research ([`anchors`]),
//! * capability and evidence gates ([`capabilities`], [`players`]).
//!
//! Nothing in this crate fabricates an offset, pointer chain or record layout.

#![forbid(unsafe_code)]

pub mod anchors;
pub mod build_identity;
pub mod capabilities;
pub mod db_metadata;
pub mod db_service;
pub mod discovery;
pub mod name_pool;
pub mod player_store;
pub mod players;
pub mod profile;
pub mod table_access;

pub use profile::{GAME_EXE, KNOWN_BUILDS, KnownBuild};
