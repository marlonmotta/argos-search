//! # Argos Search — Core Engine
//!
//! This crate contains all search and indexing logic, with zero GUI dependencies.
//! It can be used as a library by the CLI, GUI, or any external Rust application.

pub mod config;
pub mod engine;
pub mod extractors;
pub mod metadata;

pub use config::ArgosConfig;
pub use config::SearchScope;
pub use engine::{ArgosEngine, SearchHit, SearchResult};
pub use metadata::MetadataStore;
