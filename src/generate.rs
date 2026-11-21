//! AI ontology generation — the offline half of `server/ai.ts`.
//!
//! The crate does not call models. It assembles the system/user prompt an
//! ontology architect model should see ([`build_prompt`]) and parses the
//! model's JSON back into a native import payload ([`parse_generated`]),
//! repairing malformed output first when the `repair` feature is on. The
//! payload feeds [`crate::store::Store::import`] so validation, merge
//! semantics and undo stay in one place — exactly like the TS `/api/generate`
//! endpoint.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::model::*;
use crate::store::Store;

/// The system prompt used by `/api/generate` — kept verbatim so a Rust
/// service produces model output interchangeable with the TS app's.
pub const SYSTEM_PROMPT: &str = "You are an ontology architect. From the supplied files, model the business domain as object types \
     (entities with typed properties), and link types between them. Rules: \
     apiNames are stable UPPER_SNAKE identifiers; displayNames are human words; \
     dataTypes are only string/integer/decimal/boolean/date/timestamp; \
     cardinality is written source:target where ONE means exactly one and MANY means any number; \
     choose a concise domain label for grouping; keep descriptions to one sentence. \
     The ontology already contains the types listed under EXISTING ONTOLOGY — do not redefine their apiNames; \
     extend the model instead (new types may link to existing ones by apiName).";

/// Input to [`build_prompt`]: extracted text of one attached file.
pub struct SourceFile {
    pub name: String,
    pub text: String,
}

/// The JSON shape the model is told to produce (`generatedSchema` in ai.ts).
/// Kept as documentation + for `structured output` modes that take a schema.
pub fn output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "objectTypes": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "apiName": { "type": "string" },
                        "displayName": { "type": "string" },
                        "description": { "type": "string" },
                        "domain": { "type": "string" },
                        "properties": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "apiName": { "type": "string" },
                                    "displayName": { "type": "string" },
                                    "dataType": { "enum": ["string", "integer", "decimal", "boolean", "date", "timestamp"] },
                                    "required": { "type": "boolean" },
                                    "array": { "type": "boolean" }
                                },
                                "required": ["apiName", "displayName", "dataType", "required", "array"]
                            }
                        }
                    },
                    "required": ["apiName", "displayName", "description", "domain", "properties"]
                }
            },
            "linkTypes": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "apiName": { "type": "string" },
                        "displayName": { "type": "string" },
                        "description": { "type": "string" },
                        "sourceApiName": { "type": "string" },
                        "targetApiName": { "type": "string" },
                        "sourceCardinality": { "enum": ["ONE", "MANY"] },
                        "targetCardinality": { "enum": ["ONE", "MANY"] }
                    },
                    "required": ["apiName", "displayName", "description", "sourceApiName", "targetApiName", "sourceCardinality", "targetCardinality"]
                }
            }
        },
        "required": ["objectTypes", "linkTypes"]
    })
}

/// Build the `(system, user)` prompt pair — same structure as ai.ts:
/// existing types are passed so the model extends rather than redefines.
pub fn build_prompt(store: &Store, files: &[SourceFile], instructions: &str) -> (String, String) {
    let o = store.ontology();
    let existing = json!({
        "objectTypes": o.object_types.iter().map(|t| json!({
            "apiName": t.api_name,
            "displayName": t.display_name,
            "properties": t.properties.iter().map(|p| p.api_name.clone()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "linkTypes": o.link_types.iter().map(|l| json!({
            "apiName": l.api_name,
            "displayName": l.display_name,
        })).collect::<Vec<_>>(),
    });

    let digest = files
        .iter()
        .map(|f| {
            format!(
                "--- FILE: {} ({} chars) ---\n{}",
                f.name,
                f.text.len(),
                f.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let user = format!(
        "EXISTING ONTOLOGY:\n{}\n\n{}FILES ({} attached):\n{}",
        existing,
        if instructions.is_empty() {
            String::new()
        } else {
            format!("USER FOCUS:\n{instructions}\n")
        },
        files.len(),
        digest,
    );
    (SYSTEM_PROMPT.to_string(), user)
}

// ---------- output parsing ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenProperty {
    api_name: String,
    #[serde(default)]
    display_name: String,
    data_type: DataType,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    array: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenObjectType {
    api_name: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    domain: String,
    #[serde(default)]
    properties: Vec<GenProperty>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenLinkType {
    api_name: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    description: String,
    source_api_name: String,
    target_api_name: String,
    source_cardinality: Cardinality,
    target_cardinality: Cardinality,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Generated {
    #[serde(default)]
    object_types: Vec<GenObjectType>,
    #[serde(default)]
    link_types: Vec<GenLinkType>,
}

fn up(s: &str) -> String {
    crate::model::slugify(s)
}

/// Turn model output into the native-format payload
/// (`{ontology: {objectTypes…}, objects: [], objectLinks: []}`).
///
/// Link endpoints stay apiNames — [`crate::store::Store::merge`] resolves
/// them to ids, which is how generated links reach existing types. apiNames
/// are normalized to UPPER_SNAKE because models drift to TitleCase.
pub fn parse_generated(text: &str) -> Result<Value> {
    let parsed: Generated = match serde_json::from_str(text) {
        Ok(g) => g,
        Err(e) => {
            #[cfg(feature = "repair")]
            {
                let repaired = anyrepair::jsonrepair(text).map_err(|r| {
                    Error::bad(format!("unparseable model output: {e}; repair failed: {r}"))
                })?;
                serde_json::from_str(&repaired)
                    .map_err(|e2| Error::bad(format!("unparseable model output: {e2}")))?
            }
            #[cfg(not(feature = "repair"))]
            return Err(Error::bad(format!("unparseable model output: {e}")));
        }
    };

    if parsed.object_types.is_empty() {
        return Err(Error::bad("model proposed no object types"));
    }

    Ok(json!({
        "ontology": {
            "objectTypes": parsed.object_types.iter().map(|t| json!({
                "id": uuid::Uuid::new_v4().to_string(),
                "apiName": up(&t.api_name),
                "displayName": if t.display_name.is_empty() { up(&t.api_name) } else { t.display_name.clone() },
                "description": t.description,
                "domain": if t.domain.is_empty() { "General".into() } else { t.domain.clone() },
                "properties": t.properties.iter().map(|p| json!({
                    "apiName": up(&p.api_name),
                    "displayName": p.display_name,
                    "dataType": p.data_type,
                    "required": p.required,
                    "array": p.array,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "linkTypes": parsed.link_types.iter().map(|l| json!({
                "id": uuid::Uuid::new_v4().to_string(),
                "apiName": up(&l.api_name),
                "displayName": l.display_name,
                "description": l.description,
                "sourceTypeId": up(&l.source_api_name),
                "targetTypeId": up(&l.target_api_name),
                "sourceCardinality": l.source_cardinality,
                "targetCardinality": l.target_cardinality,
            })).collect::<Vec<_>>(),
            "actionTypes": Vec::<Value>::new(),
        },
        "objects": Vec::<Value>::new(),
        "objectLinks": Vec::<Value>::new(),
    }))
}
