//! # outofmemory — context engineering on an ontology
//!
//! LLM agents run out of memory; the ontology doesn't. This crate keeps a
//! Palantir Foundry–style ontology — object types with typed properties,
//! link types with cardinalities, action types, and the instances that live
//! under them — as the agent's external memory, and packs the relevant slice
//! into a token budget on demand.
//!
//! It is a Rust port of the [outofmemory](https://outofmemory.local) ontology
//! workbench: the same domain model, the same validation and cascade rules,
//! the same interchange formats (native JSON, JSON-LD, Turtle, OWL/RDF-XML,
//! instances CSV), the same seven MCP tools, plus the context-engineering
//! layer a library can offer that a UI cannot:
//!
//! - [`store::Store`] — validated in-memory store (apiName uniqueness,
//!   endpoint type checks, FK-style cascades, merge/replace import)
//! - [`context`] — schema + instance Markdown rendering, query relevance,
//!   neighbourhood expansion, token-budget packing
//! - [`mcp`] — seven tools over a stateless JSON-RPC surface, transport-agnostic
//! - [`interchange`] — lossless native/JSON-LD/OWL round-trips, Turtle + CSV export
//! - [`generate`] — AI-generation prompt assembly + repair-tolerant output parsing
//! - [`seed`] — the deterministic supply-chain dataset from the workbench

pub mod context;
pub mod error;
pub mod generate;
pub mod interchange;
pub mod mcp;
pub mod model;
pub mod seed;
pub mod store;
pub mod util;

pub use error::{Error, ErrorKind, Result};
pub use model::{
    ActionType, Cardinality, DataType, LayoutEntry, LinkType, Obj, ObjectLink, ObjectType,
    Ontology, PropertyDef, slugify,
};
pub use store::{ImportMode, State, Store, Summary};
