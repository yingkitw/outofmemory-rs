//! MCP (Model Context Protocol) surface — the same seven tools as
//! `server/mcp.ts`, over the store directly instead of through HTTP.
//!
//! [`handle_rpc`] is transport-agnostic: feed it one JSON-RPC message as a
//! string, get the response string back (`None` for notifications, matching
//! the TS `202` semantics). Mount it on stdio, HTTP, or a test harness.

use serde_json::{Value, json};

use crate::error::Error;
use crate::model::*;
use crate::store::Store;

const PROTOCOL_VERSIONS: [&str; 3] = ["2024-11-05", "2025-03-26", "2025-06-18"];
const LATEST: &str = "2025-06-18";
const SERVER_INFO: (&str, &str) = ("outofmemory", env!("CARGO_PKG_VERSION"));

const PROP_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "apiName": { "type": "string" },
    "displayName": { "type": "string" },
    "dataType": { "enum": ["string", "integer", "decimal", "boolean", "date", "timestamp"] },
    "required": { "type": "boolean" },
    "array": { "type": "boolean" }
  },
  "required": ["apiName", "displayName", "dataType"]
}"#;

fn tools() -> Value {
    let card = json!({ "enum": ["ONE", "MANY"] });
    let prop: Value = serde_json::from_str(PROP_SCHEMA).unwrap();
    json!([
        {
            "name": "get_ontology",
            "description": "Read the full ontology schema: object types (with properties), link types (with cardinalities) and action types, plus instance counts.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "create_object_type",
            "description": "Create an object type. dataType is one of string/integer/decimal/boolean/date/timestamp. Returns the created type.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "apiName": { "type": "string", "description": "stable UPPER_SNAKE identifier, e.g. SHIPMENT" },
                    "displayName": { "type": "string" },
                    "domain": { "type": "string" },
                    "description": { "type": "string" },
                    "properties": { "type": "array", "items": prop }
                },
                "required": ["apiName", "displayName"]
            }
        },
        {
            "name": "create_link_type",
            "description": "Create a link type between two object types, addressed by their apiNames. Cardinality reads source : target (ONE or MANY each).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "apiName": { "type": "string", "description": "stable UPPER_SNAKE identifier, e.g. CARRIED_BY" },
                    "displayName": { "type": "string" },
                    "sourceTypeApiName": { "type": "string" },
                    "targetTypeApiName": { "type": "string" },
                    "sourceCardinality": card,
                    "targetCardinality": card
                },
                "required": ["apiName", "displayName", "sourceTypeApiName", "targetTypeApiName", "sourceCardinality", "targetCardinality"]
            }
        },
        {
            "name": "delete_object_type",
            "description": "Delete an object type by id. Cascades: its link types, actions, instances, instance links and layout positions are removed.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "string", "description": "object type id from get_ontology" } },
                "required": ["id"]
            }
        },
        {
            "name": "list_instances",
            "description": "List object instances, optionally filtered by an object type's apiName.",
            "inputSchema": {
                "type": "object",
                "properties": { "typeApiName": { "type": "string" } }
            }
        },
        {
            "name": "create_instance",
            "description": "Create an object instance of a type (addressed by apiName) with a title and property values.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "typeApiName": { "type": "string" },
                    "title": { "type": "string" },
                    "properties": { "type": "object", "description": "property values keyed by property apiName" }
                },
                "required": ["typeApiName", "title"]
            }
        },
        {
            "name": "link_instances",
            "description": "Link two object instances with a link type (addressed by apiName). Cardinality and type matching are enforced.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "linkTypeApiName": { "type": "string" },
                    "sourceObjectId": { "type": "string" },
                    "targetObjectId": { "type": "string" }
                },
                "required": ["linkTypeApiName", "sourceObjectId", "targetObjectId"]
            }
        }
    ])
}

fn arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, Error> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::bad(format!("missing argument \"{key}\"")))
}

fn by_api_name<'a, T>(list: &'a [T], api: &str, f: impl Fn(&T) -> &str) -> Result<&'a T, Error> {
    list.iter()
        .find(|x| f(x) == api)
        .ok_or_else(|| Error::bad(format!("no type with apiName \"{api}\"")))
}

fn run_tool(store: &mut Store, name: &str, args: &Value) -> Result<Value, Error> {
    match name {
        "get_ontology" => {
            let o = store.ontology();
            Ok(json!({
                "objectTypes": o.object_types,
                "linkTypes": o.link_types,
                "actionTypes": o.action_types,
                "instanceCounts": {
                    "objects": store.state.objects.len(),
                    "links": store.state.object_links.len(),
                }
            }))
        }
        "create_object_type" => {
            let properties: Vec<PropertyDef> = args
                .get("properties")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|_| {
                    Error::bad("properties must be an array of valid property definitions")
                })?
                .unwrap_or_default();
            let t = store.create_object_type(ObjectType {
                id: String::new(),
                api_name: arg(args, "apiName")?.to_string(),
                display_name: arg(args, "displayName")?.to_string(),
                plural_display_name: arg(args, "pluralDisplayName")
                    .unwrap_or_default()
                    .to_string(),
                description: arg(args, "description").unwrap_or_default().to_string(),
                domain: arg(args, "domain").unwrap_or_default().to_string(),
                color: arg(args, "color").unwrap_or_default().to_string(),
                properties,
            })?;
            Ok(serde_json::to_value(t).unwrap())
        }
        "create_link_type" => {
            let src = by_api_name(
                &store.state.object_types,
                arg(args, "sourceTypeApiName")?,
                |t| &t.api_name,
            )?
            .id
            .clone();
            let tgt = by_api_name(
                &store.state.object_types,
                arg(args, "targetTypeApiName")?,
                |t| &t.api_name,
            )?
            .id
            .clone();
            let card = |k: &str| -> Result<Cardinality, Error> {
                match arg(args, k)? {
                    "ONE" => Ok(Cardinality::One),
                    "MANY" => Ok(Cardinality::Many),
                    _ => Err(Error::bad("cardinalities must be ONE or MANY")),
                }
            };
            let l = store.create_link_type(LinkType {
                id: String::new(),
                api_name: arg(args, "apiName")?.to_string(),
                display_name: arg(args, "displayName")?.to_string(),
                description: arg(args, "description").unwrap_or_default().to_string(),
                source_type_id: src,
                target_type_id: tgt,
                source_cardinality: card("sourceCardinality")?,
                target_cardinality: card("targetCardinality")?,
            })?;
            Ok(serde_json::to_value(l).unwrap())
        }
        "delete_object_type" => {
            store.delete_object_type(arg(args, "id")?)?;
            Ok(json!({ "ok": true }))
        }
        "list_instances" => {
            let filter = arg(args, "typeApiName").ok();
            let type_id = match filter {
                Some(api) => Some(
                    by_api_name(&store.state.object_types, api, |t| &t.api_name)?
                        .id
                        .clone(),
                ),
                None => None,
            };
            let out: Vec<Value> = store
                .state
                .objects
                .iter()
                .filter(|o| type_id.as_ref().map(|t| &o.type_id == t).unwrap_or(true))
                .map(|o| {
                    let mut v = serde_json::to_value(o).unwrap();
                    v["type"] = json!(
                        store
                            .object_type(&o.type_id)
                            .map(|t| t.api_name.clone())
                            .unwrap_or_default()
                    );
                    v
                })
                .collect();
            Ok(Value::Array(out))
        }
        "create_instance" => {
            let t = by_api_name(&store.state.object_types, arg(args, "typeApiName")?, |t| {
                &t.api_name
            })?
            .id
            .clone();
            let properties = args
                .get("properties")
                .and_then(Value::as_object)
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            let o = store.create_object(Obj {
                id: String::new(),
                type_id: t,
                title: arg(args, "title")?.to_string(),
                properties,
                created_at: String::new(),
            })?;
            Ok(serde_json::to_value(o).unwrap())
        }
        "link_instances" => {
            let lt = by_api_name(
                &store.state.link_types,
                arg(args, "linkTypeApiName")?,
                |l| &l.api_name,
            )?
            .id
            .clone();
            let l = store.create_object_link(ObjectLink {
                id: String::new(),
                link_type_id: lt,
                source_object_id: arg(args, "sourceObjectId")?.to_string(),
                target_object_id: arg(args, "targetObjectId")?.to_string(),
            })?;
            Ok(serde_json::to_value(l).unwrap())
        }
        _ => Err(Error::bad(format!("unknown tool \"{name}\""))),
    }
}

fn rpc(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_err(id: &Value, code: i64, message: String) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Handle one JSON-RPC message (stateless Streamable HTTP / stdio, JSON
/// response mode — same surface as `server/mcp.ts`).
///
/// Returns `None` for notifications and unparseable input that warrants no
/// reply — the caller maps those to `202` (HTTP) or silence (stdio). Parse
/// errors return the `-32700` response inside `Some`.
pub fn handle_rpc(store: &mut Store, msg: &str) -> Option<String> {
    let msg: Value = match serde_json::from_str(msg) {
        Ok(v) => v,
        Err(_) => {
            return Some(rpc_err(&Value::Null, -32700, "parse error".into()).to_string());
        }
    };
    if msg.is_array() {
        return Some(
            rpc_err(
                &Value::Null,
                -32600,
                "batching was removed from the spec".into(),
            )
            .to_string(),
        );
    }
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    if id.is_null() {
        return None; // notification
    }
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);

    let out = match method {
        "initialize" => {
            let requested = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("");
            let version = if PROTOCOL_VERSIONS.contains(&requested) {
                requested
            } else {
                LATEST
            };
            rpc(
                &id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": SERVER_INFO.0, "version": SERVER_INFO.1 },
                }),
            )
        }
        "ping" => rpc(&id, json!({})),
        "tools/list" => rpc(&id, json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let known = [
                "get_ontology",
                "create_object_type",
                "create_link_type",
                "delete_object_type",
                "list_instances",
                "create_instance",
                "link_instances",
            ];
            if !known.contains(&name) {
                rpc_err(&id, -32602, format!("unknown tool \"{name}\""))
            } else {
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                match run_tool(store, name, &args) {
                    Ok(result) => rpc(
                        &id,
                        json!({
                            "content": [{ "type": "text", "text": result.to_string() }]
                        }),
                    ),
                    Err(e) => rpc(
                        &id,
                        json!({
                            "content": [{ "type": "text", "text": e.to_string() }],
                            "isError": true
                        }),
                    ),
                }
            }
        }
        other => rpc_err(&id, -32601, format!("method \"{other}\" not found")),
    };
    Some(out.to_string())
}
