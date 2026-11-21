//! JSON-LD — `rdfs:Class` for object types, `rdf:Property` for typed
//! properties and link types, `onto:Action` for action types, and one node
//! per instance. Port of `toJsonLd` / `parseJsonLd` in `server/onto.ts`.

use serde_json::{Map, Value, json};

use super::{ONTO_NS, Parsed};
use crate::model::*;
use crate::store::Store;

fn context() -> Value {
    json!({
        "@vocab": ONTO_NS,
        "rdf": "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
        "rdfs": "http://www.w3.org/2000/01/rdf-schema#",
        "xsd": "http://www.w3.org/2001/XMLSchema#",
        "onto": ONTO_NS,
        "domain": { "@id": format!("{ONTO_NS}domain"), "@type": "@id" },
        "color": { "@id": format!("{ONTO_NS}color") },
    })
}

/// Export the whole store as a JSON-LD document (`{"@context", "@graph"}`).
pub fn to_json_ld(store: &Store) -> Value {
    let ontology = &store.state;
    let objects = &store.state.objects;
    let object_links = &store.state.object_links;
    let mut graph: Vec<Value> = Vec::new();
    let type_by_id = |id: &str| ontology.object_types.iter().find(|t| t.id == id);

    for t in &ontology.object_types {
        graph.push(json!({
            "@id": format!("{ONTO_NS}{}", t.api_name),
            "@type": "rdfs:Class",
            "rdfs:label": t.display_name,
            "onto:apiName": t.api_name,
            "onto:objectTypeId": t.id,
            "onto:pluralDisplayName": t.plural_display_name,
            "onto:description": t.description,
            "onto:domain": t.domain,
            "onto:color": t.color,
        }));
        for p in &t.properties {
            let mut node = json!({
                "@id": format!("{ONTO_NS}{}.{}", t.api_name, p.api_name),
                "@type": "rdf:Property",
                "rdfs:label": p.display_name,
                "rdfs:domain": { "@id": format!("{ONTO_NS}{}", t.api_name) },
                "rdfs:range": p.data_type.xsd(),
                "onto:apiName": p.api_name,
                "onto:dataType": p.data_type.as_str(),
                "onto:required": p.required,
                "onto:array": p.array,
            });
            if let Some(pat) = &p.pattern {
                node["onto:pattern"] = json!(pat);
            }
            if let Some(vals) = &p.enum_values {
                node["onto:enumValues"] = json!(vals);
            }
            if p.unique {
                node["onto:unique"] = json!(true);
            }
            graph.push(node);
        }
    }

    for l in &ontology.link_types {
        let src = type_by_id(&l.source_type_id);
        let tgt = type_by_id(&l.target_type_id);
        graph.push(json!({
            "@id": format!("{ONTO_NS}{}", l.api_name),
            "@type": "rdf:Property",
            "rdfs:label": l.display_name,
            "rdfs:domain": { "@id": format!("{ONTO_NS}{}", src.map(|t| t.api_name.as_str()).unwrap_or("?")) },
            "rdfs:range": { "@id": format!("{ONTO_NS}{}", tgt.map(|t| t.api_name.as_str()).unwrap_or("?")) },
            "onto:apiName": l.api_name,
            "onto:objectTypeId": l.id,
            "onto:description": l.description,
            "onto:linkType": true,
            "onto:sourceCardinality": l.source_cardinality.as_str(),
            "onto:targetCardinality": l.target_cardinality.as_str(),
        }));
    }

    for a in &ontology.action_types {
        let on = type_by_id(&a.object_type_id);
        graph.push(json!({
            "@id": format!("{ONTO_NS}{}", a.api_name),
            "@type": "onto:Action",
            "rdfs:label": a.display_name,
            "onto:apiName": a.api_name,
            "onto:objectTypeId": a.id,
            "onto:description": a.description,
            "onto:objectType": { "@id": format!("{ONTO_NS}{}", on.map(|t| t.api_name.as_str()).unwrap_or("?")) },
            "onto:parameters": a.parameters.iter().map(|p| json!({
                "onto:apiName": p.api_name,
                "rdfs:label": p.display_name,
                "rdfs:range": p.data_type.xsd(),
                "onto:required": p.required,
            })).collect::<Vec<_>>(),
        }));
    }

    for o in objects {
        let t = type_by_id(&o.type_id);
        let mut node = Map::new();
        node.insert("@id".into(), json!(format!("{ONTO_NS}obj_{}", o.id)));
        node.insert(
            "@type".into(),
            json!(format!(
                "{ONTO_NS}{}",
                t.map(|t| t.api_name.as_str()).unwrap_or("?")
            )),
        );
        node.insert("rdfs:label".into(), json!(o.title));
        node.insert("onto:objectId".into(), json!(o.id));
        node.insert("onto:createdAt".into(), json!(o.created_at));
        for (k, v) in &o.properties {
            node.insert(format!("onto:{k}"), v.clone());
        }
        graph.push(Value::Object(node));
    }

    let link_by_id = |id: &str| ontology.link_types.iter().find(|l| l.id == id);
    for l in object_links {
        let lt = link_by_id(&l.link_type_id);
        graph.push(json!({
            "@id": format!("{ONTO_NS}lnk_{}", l.id),
            "@type": format!("{ONTO_NS}{}", lt.map(|x| x.api_name.as_str()).unwrap_or("?")),
            "onto:linkId": l.id,
            "onto:source": { "@id": format!("{ONTO_NS}obj_{}", l.source_object_id) },
            "onto:target": { "@id": format!("{ONTO_NS}obj_{}", l.target_object_id) },
        }));
    }

    json!({ "@context": context(), "@graph": graph })
}

// ---------- import ----------

use super::local;

fn str_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// `{ "@id": "…" }` or a plain string → local name.
fn ref_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => local(s).to_string(),
        Some(Value::Object(o)) => o
            .get("@id")
            .and_then(Value::as_str)
            .map(local)
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

/// `"onto:X"`, `["onto:X"]`, or `"rdfs:Class"` → local type name.
fn type_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => local(s).to_string(),
        Some(Value::Array(a)) => a
            .first()
            .and_then(Value::as_str)
            .map(local)
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Parse a JSON-LD document into the native shape. Round-trips our own
/// export exactly.
pub fn parse_json_ld(doc: &Value) -> Parsed {
    let mut warnings: Vec<String> = Vec::new();
    let graph: Vec<&Value> = if let Some(Value::Array(g)) = doc.get("@graph") {
        g.iter().collect()
    } else if let Value::Array(g) = doc {
        g.iter().collect()
    } else if doc.is_object() {
        vec![doc]
    } else {
        vec![]
    };

    let mut class_ids: std::collections::HashSet<String> = Default::default();
    let mut property_nodes: Vec<&Value> = vec![];
    let mut link_nodes: Vec<&Value> = vec![];
    let mut action_nodes: Vec<&Value> = vec![];
    let mut instance_nodes: Vec<&Value> = vec![];

    // Pass 1 — classify.
    for node in &graph {
        let t = type_of(node.get("@type"));
        if t == "rdfs:Class" {
            let api = str_of(node.get("onto:apiName"));
            class_ids.insert(if api.is_empty() {
                local(node.get("@id").and_then(Value::as_str).unwrap_or("")).to_string()
            } else {
                api
            });
        } else if t == "rdf:Property" {
            property_nodes.push(node);
        } else if t == "onto:Action" {
            action_nodes.push(node);
        } else {
            instance_nodes.push(node);
        }
    }

    // Property nodes split into data properties (stay with their class) and
    // object properties (become link types).
    for node in &property_nodes {
        let domain = ref_of(node.get("rdfs:domain"));
        let range = ref_of(node.get("rdfs:range"));
        if !domain.is_empty() && class_ids.contains(&domain) && range.starts_with("xsd:") {
            // data property
        } else if !domain.is_empty() && !range.is_empty() {
            link_nodes.push(node);
        } else {
            warnings.push(format!(
                "Property {} has no usable domain/range — skipped",
                node.get("@id").and_then(Value::as_str).unwrap_or("?")
            ));
        }
    }

    let is_data_prop_of = |node: &Value, api: &str| -> bool {
        ref_of(node.get("rdfs:domain")) == api && ref_of(node.get("rdfs:range")).starts_with("xsd:")
    };

    let mut object_types: Vec<ObjectType> = vec![];
    for node in &graph {
        if type_of(node.get("@type")) != "rdfs:Class" {
            continue;
        }
        let api = {
            let a = str_of(node.get("onto:apiName"));
            if a.is_empty() {
                local(node.get("@id").and_then(Value::as_str).unwrap_or("")).to_string()
            } else {
                a
            }
        };
        let properties = property_nodes
            .iter()
            .filter(|p| is_data_prop_of(p, &api))
            .map(|p| {
                let api_name = {
                    let a = str_of(p.get("onto:apiName"));
                    if !a.is_empty() {
                        a
                    } else {
                        local(p.get("@id").and_then(Value::as_str).unwrap_or(""))
                            .rsplit('.')
                            .next()
                            .unwrap_or("property")
                            .to_string()
                    }
                };
                let display = {
                    let d = str_of(p.get("rdfs:label"));
                    if d.is_empty() {
                        str_of(p.get("onto:apiName"))
                    } else {
                        d
                    }
                };
                PropertyDef {
                    api_name,
                    display_name: display,
                    data_type: DataType::from_xsd(&ref_of(p.get("rdfs:range")))
                        .unwrap_or(DataType::String),
                    required: p
                        .get("onto:required")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    array: p
                        .get("onto:array")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    pattern: p
                        .get("onto:pattern")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    enum_values: p.get("onto:enumValues").and_then(Value::as_array).map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    }),
                    unique: p
                        .get("onto:unique")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                }
            })
            .collect();
        let id = {
            let s = str_of(node.get("onto:objectTypeId"));
            if s.is_empty() { uuid() } else { s }
        };
        object_types.push(ObjectType {
            id,
            api_name: api.clone(),
            display_name: {
                let d = str_of(node.get("rdfs:label"));
                if d.is_empty() { api.clone() } else { d }
            },
            plural_display_name: {
                let d = str_of(node.get("onto:pluralDisplayName"));
                if d.is_empty() { format!("{api}s") } else { d }
            },
            description: str_of(node.get("onto:description")),
            domain: {
                let d = str_of(node.get("onto:domain"));
                if d.is_empty() { "General".into() } else { d }
            },
            color: {
                let d = str_of(node.get("onto:color"));
                if d.is_empty() { "#f0b35c".into() } else { d }
            },
            properties,
        });
    }

    let mut link_types: Vec<LinkType> = link_nodes
        .iter()
        .map(|node| {
            let api = {
                let a = str_of(node.get("onto:apiName"));
                if a.is_empty() {
                    local(node.get("@id").and_then(Value::as_str).unwrap_or("")).to_string()
                } else {
                    a
                }
            };
            LinkType {
                id: {
                    let s = str_of(node.get("onto:objectTypeId"));
                    if s.is_empty() { uuid() } else { s }
                },
                api_name: api.clone(),
                display_name: {
                    let d = str_of(node.get("rdfs:label"));
                    if d.is_empty() {
                        str_of(node.get("onto:apiName"))
                    } else {
                        d
                    }
                },
                description: str_of(node.get("onto:description")),
                source_type_id: String::new(),
                target_type_id: String::new(),
                source_cardinality: if str_of(node.get("onto:sourceCardinality")) == "ONE" {
                    Cardinality::One
                } else {
                    Cardinality::Many
                },
                target_cardinality: if str_of(node.get("onto:targetCardinality")) == "MANY" {
                    Cardinality::Many
                } else {
                    Cardinality::One
                },
            }
        })
        .collect();

    let mut action_types: Vec<ActionType> = action_nodes
        .iter()
        .map(|node| {
            let api = {
                let a = str_of(node.get("onto:apiName"));
                if a.is_empty() {
                    local(node.get("@id").and_then(Value::as_str).unwrap_or("")).to_string()
                } else {
                    a
                }
            };
            let parameters = node
                .get("onto:parameters")
                .and_then(Value::as_array)
                .map(|ps| {
                    ps.iter()
                        .map(|p| PropertyDef {
                            api_name: str_of(p.get("onto:apiName")),
                            display_name: {
                                let d = str_of(p.get("rdfs:label"));
                                if d.is_empty() {
                                    str_of(p.get("onto:apiName"))
                                } else {
                                    d
                                }
                            },
                            data_type: DataType::from_xsd(&ref_of(p.get("rdfs:range")))
                                .unwrap_or(DataType::String),
                            required: p
                                .get("onto:required")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            array: false,
                            pattern: None,
                            enum_values: None,
                            unique: false,
                        })
                        .collect()
                })
                .unwrap_or_default();
            ActionType {
                id: {
                    let s = str_of(node.get("onto:objectTypeId"));
                    if s.is_empty() { uuid() } else { s }
                },
                api_name: api.clone(),
                display_name: {
                    let d = str_of(node.get("rdfs:label"));
                    if d.is_empty() {
                        str_of(node.get("onto:apiName"))
                    } else {
                        d
                    }
                },
                description: str_of(node.get("onto:description")),
                object_type_id: String::new(),
                parameters,
            }
        })
        .collect();

    // Resolve cross references (apiName → id) now that every type exists.
    let class_id_by_api = |api: &str| {
        object_types
            .iter()
            .find(|t| t.api_name == api)
            .map(|t| t.id.clone())
    };
    for (node, lt) in link_nodes.iter().zip(link_types.iter_mut()) {
        lt.source_type_id = class_id_by_api(&ref_of(node.get("rdfs:domain"))).unwrap_or_default();
        lt.target_type_id = class_id_by_api(&ref_of(node.get("rdfs:range"))).unwrap_or_default();
    }
    for (node, at) in action_nodes.iter().zip(action_types.iter_mut()) {
        at.object_type_id =
            class_id_by_api(&ref_of(node.get("onto:objectType"))).unwrap_or_default();
    }

    // Instances: a node whose @type names a class we know.
    let known_class: std::collections::HashSet<&str> =
        object_types.iter().map(|t| t.api_name.as_str()).collect();
    let known_link: std::collections::HashSet<&str> =
        link_types.iter().map(|l| l.api_name.as_str()).collect();
    let mut objects: Vec<Obj> = vec![];
    let mut object_links: Vec<ObjectLink> = vec![];

    const RESERVED_ONTO: [&str; 11] = [
        "apiName",
        "objectId",
        "domain",
        "color",
        "dataType",
        "required",
        "array",
        "createdAt",
        "pattern",
        "enumValues",
        "unique",
    ];

    for node in &instance_nodes {
        let t = type_of(node.get("@type"));
        if !known_class.contains(t.as_str()) {
            continue;
        }
        let mut props: std::collections::BTreeMap<String, Value> = Default::default();
        if let Value::Object(map) = node {
            for (k, v) in map {
                if k.starts_with('@') || k.starts_with("rdfs:") || k.starts_with("onto:") {
                    continue;
                }
                props.insert(k.clone(), v.clone());
            }
            for (k, v) in map {
                let Some(key) = k.strip_prefix("onto:") else {
                    continue;
                };
                if RESERVED_ONTO.contains(&key) {
                    continue;
                }
                props.insert(key.to_string(), v.clone());
            }
        }
        let id = {
            let s = str_of(node.get("onto:objectId"));
            if !s.is_empty() {
                s
            } else {
                let l = local(node.get("@id").and_then(Value::as_str).unwrap_or(""))
                    .strip_prefix("obj_")
                    .map(str::to_string);
                l.unwrap_or_else(uuid)
            }
        };
        objects.push(Obj {
            id,
            type_id: class_id_by_api(&t).unwrap_or_default(),
            title: {
                let d = str_of(node.get("rdfs:label"));
                if d.is_empty() { "Untitled".into() } else { d }
            },
            properties: props,
            created_at: {
                let d = str_of(node.get("onto:createdAt"));
                if d.is_empty() {
                    crate::util::now_iso()
                } else {
                    d
                }
            },
        });
    }

    for node in &graph {
        let t = type_of(node.get("@type"));
        if !known_link.contains(t.as_str()) {
            continue;
        }
        let id = {
            let s = str_of(node.get("onto:linkId"));
            if !s.is_empty() {
                s
            } else {
                let l = local(node.get("@id").and_then(Value::as_str).unwrap_or(""))
                    .strip_prefix("lnk_")
                    .map(str::to_string);
                l.unwrap_or_else(uuid)
            }
        };
        object_links.push(ObjectLink {
            id,
            link_type_id: link_types
                .iter()
                .find(|l| l.api_name == t)
                .map(|l| l.id.clone())
                .unwrap_or_default(),
            source_object_id: ref_of(node.get("onto:source"))
                .trim_start_matches("obj_")
                .to_string(),
            target_object_id: ref_of(node.get("onto:target"))
                .trim_start_matches("obj_")
                .to_string(),
        });
    }

    if object_types.is_empty() {
        warnings.push("No rdfs:Class found — nothing to import".into());
    }

    Parsed {
        state: crate::store::State {
            object_types,
            link_types,
            action_types,
            objects,
            object_links,
            layout: vec![],
        },
        warnings,
    }
}
