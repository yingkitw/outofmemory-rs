//! Interchange formats — ports of `server/onto.ts`.
//!
//! | Format | Export | Import |
//! |---|---|---|
//! | native `outofmemory` JSON | [`to_native_json`] | [`parse_native`] |
//! | JSON-LD | [`to_json_ld`] | [`parse_json_ld`] |
//! | Turtle | [`to_turtle`] | [`parse_turtle`] |
//! | OWL (RDF/XML) | [`to_owl_rdf`] | [`parse_owl_rdf`] |
//! | instances CSV | [`to_instances_csv`] | — |
//!
//! The native → JSON-LD → native, native → OWL → native and
//! native → Turtle → native chains are lossless, which the integration
//! tests verify.

mod csv;
mod jsonld;
mod owl;
mod turtle;

pub use csv::to_instances_csv;
pub use jsonld::{parse_json_ld, to_json_ld};
pub use owl::{parse_owl_rdf, to_owl_rdf};
pub use turtle::{parse_turtle, to_turtle};

use serde::Serialize;

use crate::error::{Error, Result};
use crate::model::*;
use crate::store::{ImportMode, State, Store, Summary};

/// The `onto:` namespace every RDF serialization is written under.
pub const ONTO_NS: &str = "https://outofmemory.local/onto/";

/// Strip the `onto:` URL namespace down to the local name — accepts the
/// pre-rebrand "ontic.local" prefix so old exports still import. Mirrors the
/// TS `local` exactly: prefixed terms like `onto:Action` and `rdfs:Class`
/// pass through untouched.
pub(crate) fn local(id: &str) -> &str {
    for prefix in [
        "https://outofmemory.local/onto/",
        "http://outofmemory.local/onto/",
        "https://ontic.local/onto/",
        "http://ontic.local/onto/",
    ] {
        if let Some(rest) = id.strip_prefix(prefix) {
            return rest;
        }
    }
    id
}

/// Export format selector — `format=` on `/api/export` in the TS app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Full-fidelity native JSON (schema, instances, links, layout, ids).
    Native,
    /// `rdfs:Class`/`rdf:Property`/`xsd` ranges — Protégé, TopBraid, Jena.
    JsonLd,
    /// Same model as Turtle triples.
    Turtle,
    /// RDF/XML with `owl:Class`/`owl:ObjectProperty`/`owl:DatatypeProperty`.
    Owl,
    /// One row per instance, property columns unioned across types.
    Csv,
}

impl Format {
    pub fn parse(s: &str) -> Option<Format> {
        Some(match s {
            "outofmemory" | "json" | "native" => Format::Native,
            "jsonld" | "json-ld" | "json_ld" => Format::JsonLd,
            "turtle" | "ttl" => Format::Turtle,
            "owl" | "rdf" | "rdfxml" | "rdf-xml" => Format::Owl,
            "csv" => Format::Csv,
            _ => return None,
        })
    }
}

/// Everything a document yields on import — a [`State`] plus non-fatal
/// warnings (the TS `ParsedOntology`).
#[derive(Debug, Clone, Default)]
pub struct Parsed {
    pub state: State,
    pub warnings: Vec<String>,
}

/// The native export envelope (`GET /api/export?format=outofmemory`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeExport<'a> {
    pub exported_at: String,
    pub format: &'static str,
    pub ontology: Ontology,
    pub objects: &'a [Obj],
    pub object_links: &'a [ObjectLink],
    pub layout: &'a [LayoutEntry],
}

/// Serialize the store to native JSON — the full-fidelity export.
pub fn to_native_json(store: &Store) -> String {
    let doc = NativeExport {
        exported_at: crate::util::now_iso(),
        format: "outofmemory",
        ontology: store.ontology(),
        objects: &store.state.objects,
        object_links: &store.state.object_links,
        layout: &store.state.layout,
    };
    serde_json::to_string_pretty(&doc).expect("native export serialization")
}

/// Parse native JSON. Accepts both the export envelope
/// (`{ontology: {objectTypes…}, objects, objectLinks}`) and a flat
/// `{objectTypes, linkTypes, …}` document — same as `POST /api/import`.
pub fn parse_native(text: &str) -> Result<Parsed> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    parse_native_value(&value)
}

/// Like [`parse_native`] but repairs malformed JSON first (LLM output,
/// hand-edited files). Requires the `repair` feature.
#[cfg(feature = "repair")]
pub fn parse_native_lenient(text: &str) -> Result<Parsed> {
    match parse_native(text) {
        ok @ Ok(_) => ok,
        Err(_) => {
            let repaired = anyrepair::jsonrepair(text)
                .map_err(|e| Error::bad(format!("unparseable JSON: {e}")))?;
            let value: serde_json::Value = serde_json::from_str(&repaired)?;
            parse_native_value(&value)
        }
    }
}

fn parse_native_value(v: &serde_json::Value) -> Result<Parsed> {
    // envelope `{ontology: {...}}` and flat `{objectTypes: …}` are both legal
    let onto = v.get("ontology").unwrap_or(v);
    let arr = |v: Option<&serde_json::Value>| v.cloned().unwrap_or_else(|| serde_json::json!([]));
    let mut state: State = serde_json::from_value(serde_json::json!({
        "objectTypes": arr(onto.get("objectTypes")),
        "linkTypes": arr(onto.get("linkTypes")),
        "actionTypes": arr(onto.get("actionTypes")),
        "objects": arr(v.get("objects")),
        "objectLinks": arr(v.get("objectLinks")),
        "layout": arr(v.get("layout")),
    }))?;
    // normalize like the API: plural/domain/color defaults
    for t in &mut state.object_types {
        if t.plural_display_name.is_empty() {
            t.plural_display_name = format!("{}s", t.display_name);
        }
        if t.domain.is_empty() {
            t.domain = "General".into();
        }
        if t.color.is_empty() {
            t.color = "#f0b35c".into();
        }
    }
    if state.object_types.is_empty() {
        return Err(Error::bad("nothing to import"));
    }
    Ok(Parsed {
        state,
        warnings: vec![],
    })
}

/// Import any supported text format into the store (`POST /api/import`).
pub fn import(
    store: &mut Store,
    format: Format,
    payload: &str,
    mode: ImportMode,
) -> Result<Summary> {
    let parsed = match format {
        Format::Native => parse_native(payload)?,
        Format::JsonLd => {
            let doc: serde_json::Value = serde_json::from_str(payload)?;
            parse_json_ld(&doc)
        }
        Format::Owl => parse_owl_rdf(payload)?,
        Format::Turtle => parse_turtle(payload)?,
        Format::Csv => {
            return Err(Error::bad(
                "format must be 'outofmemory', 'jsonld', 'owl' or 'turtle'",
            ));
        }
    };
    if parsed.state.object_types.is_empty() {
        return Err(Error::bad(
            parsed
                .warnings
                .first()
                .cloned()
                .unwrap_or_else(|| "nothing to import".into()),
        ));
    }
    Ok(store.import(parsed.state, mode))
}

/// Serialize the store in any export format.
pub fn export(store: &Store, format: Format) -> String {
    match format {
        Format::Native => to_native_json(store),
        Format::JsonLd => {
            serde_json::to_string_pretty(&to_json_ld(store)).expect("JSON-LD serialization")
        }
        Format::Turtle => to_turtle(store),
        Format::Owl => to_owl_rdf(store),
        Format::Csv => to_instances_csv(store),
    }
}
