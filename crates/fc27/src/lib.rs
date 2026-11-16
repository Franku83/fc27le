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
pub mod discovery;
pub mod players;
pub mod profile;

pub use profile::{
    PROTECTION_MODULE, THIRD_PARTY_MODULE, EAAC_MODULE, EDITOR_DLL, GAME_EXE, KNOWN_BUILDS, KnownBuild,
};
