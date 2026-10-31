//! Entwurf core: source ingestion, a traceable knowledge IR, deterministic
//! composition, verification, and local project persistence.

pub mod compiler;
pub mod contracts;
pub mod error;
pub mod ingest;
pub mod ir;
pub mod layout;
pub mod model;
pub mod project;
pub mod prompt;
pub mod storage;
pub mod validate;

pub use error::{EntwurfError, Result};
pub use ir::*;

/// The single source of truth for the in-memory version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

mod rendering;
mod scene;

pub mod diagnostics;
pub mod math_text;
pub mod provenance;
pub mod workspace;

mod obstacle_routes;
pub mod visual_repair;
