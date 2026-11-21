//! OWL — RDF/XML (`.owl`) with `owl:Class` / `owl:ObjectProperty` /
//! `owl:DatatypeProperty`. `onto:` extension properties ride along as
//! annotation elements so the serialization is lossless for this crate while
//! staying a valid OWL document Protégé / TopBraid / Jena can open.
//! Port of `toOwlRdf` / `parseOwlRdf` in `server/onto.ts`.

use quick_xml::Reader;
use quick_xml::events::Event;
use serde_json::Value;

use super::{ONTO_NS, Parsed, local};
use crate::error::{Error, Result};
use crate::model::*;
use crate::store::Store;

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Export the whole store as an OWL (RDF/XML) document.
pub fn to_owl_rdf(store: &Store) -> String {
    let ontology = &store.state;
    let type_by_id = |id: &str| ontology.object_types.iter().find(|t| t.id == id);
    let ns = ONTO_NS;
    let mut out: Vec<String> = vec![
        r#"<?xml version="1.0" encoding="UTF-8"?>"#.to_string(),
        r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#""#.to_string(),
        r#"         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#""#.to_string(),
        r#"         xmlns:owl="http://www.w3.org/2002/07/owl#""#.to_string(),
        r#"         xmlns:xsd="http://www.w3.org/2001/XMLSchema#""#.to_string(),
        format!(r#"         xmlns:onto="{ns}">"#),
    ];

    for t in &ontology.object_types {
        out.extend([
            format!(
                r#"  <owl:Class rdf:about="{}">"#,
                xml(&format!("{ns}{}", t.api_name))
            ),
            format!("    <rdfs:label>{}</rdfs:label>", xml(&t.display_name)),
            format!("    <onto:apiName>{}</onto:apiName>", xml(&t.api_name)),
            format!(
                "    <onto:pluralDisplayName>{}</onto:pluralDisplayName>",
                xml(&t.plural_display_name)
            ),
            format!(
                "    <onto:description>{}</onto:description>",
                xml(&t.description)
            ),
            format!("    <onto:domain>{}</onto:domain>", xml(&t.domain)),
            format!("    <onto:color>{}</onto:color>", xml(&t.color)),
            format!("    <onto:objectTypeId>{}</onto:objectTypeId>", t.id),
            "  </owl:Class>".to_string(),
        ]);
        for p in &t.properties {
            out.extend([
                format!(
                    r#"  <owl:DatatypeProperty rdf:about="{}">"#,
                    xml(&format!("{ns}{}.{}", t.api_name, p.api_name))
                ),
                format!("    <rdfs:label>{}</rdfs:label>", xml(&p.display_name)),
                format!(
                    r#"    <rdfs:domain rdf:resource="{}" />"#,
                    xml(&format!("{ns}{}", t.api_name))
                ),
                format!(
                    r#"    <rdfs:range rdf:resource="http://www.w3.org/2001/XMLSchema#{}" />"#,
                    p.data_type.xsd().split(':').nth(1).unwrap()
                ),
                format!("    <onto:apiName>{}</onto:apiName>", xml(&p.api_name)),
                format!(
                    "    <onto:dataType>{}</onto:dataType>",
                    p.data_type.as_str()
                ),
                format!("    <onto:required>{}</onto:required>", p.required),
                format!("    <onto:array>{}</onto:array>", p.array),
                "  </owl:DatatypeProperty>".to_string(),
            ]);
            if let Some(pat) = &p.pattern {
                // insert before the closing tag
                let close = out.pop().unwrap_or_default();
                out.push(format!("    <onto:pattern>{}</onto:pattern>", xml(pat)));
                out.push(close);
            }
            if let Some(vals) = &p.enum_values {
                let close = out.pop().unwrap_or_default();
                for v in vals {
                    out.push(format!("    <onto:enumValues>{}</onto:enumValues>", xml(v)));
                }
                out.push(close);
            }
            if p.unique {
                let close = out.pop().unwrap_or_default();
                out.push("    <onto:unique>true</onto:unique>".to_string());
                out.push(close);
            }
        }
    }

    for l in &ontology.link_types {
        let src = type_by_id(&l.source_type_id);
        let tgt = type_by_id(&l.target_type_id);
        out.extend([
            format!(
                r#"  <owl:ObjectProperty rdf:about="{}">"#,
                xml(&format!("{ns}{}", l.api_name))
            ),
            format!("    <rdfs:label>{}</rdfs:label>", xml(&l.display_name)),
            format!(
                "    <onto:description>{}</onto:description>",
                xml(&l.description)
            ),
            format!(
                r#"    <rdfs:domain rdf:resource="{}" />"#,
                xml(&format!(
                    "{ns}{}",
                    src.map(|t| t.api_name.as_str()).unwrap_or("?")
                ))
            ),
            format!(
                r#"    <rdfs:range rdf:resource="{}" />"#,
                xml(&format!(
                    "{ns}{}",
                    tgt.map(|t| t.api_name.as_str()).unwrap_or("?")
                ))
            ),
            format!("    <onto:apiName>{}</onto:apiName>", xml(&l.api_name)),
            format!("    <onto:objectTypeId>{}</onto:objectTypeId>", l.id),
            format!(
                "    <onto:sourceCardinality>{}</onto:sourceCardinality>",
                l.source_cardinality.as_str()
            ),
            format!(
                "    <onto:targetCardinality>{}</onto:targetCardinality>",
                l.target_cardinality.as_str()
            ),
            "  </owl:ObjectProperty>".to_string(),
        ]);
    }

    for a in &ontology.action_types {
        let on = type_by_id(&a.object_type_id);
        out.extend([
            format!(
                r#"  <onto:Action rdf:about="{}">"#,
                xml(&format!("{ns}{}", a.api_name))
            ),
            format!("    <rdfs:label>{}</rdfs:label>", xml(&a.display_name)),
            format!("    <onto:apiName>{}</onto:apiName>", xml(&a.api_name)),
            format!(
                "    <onto:description>{}</onto:description>",
                xml(&a.description)
            ),
            format!("    <onto:objectTypeId>{}</onto:objectTypeId>", a.id),
            format!(
                r#"    <onto:objectType rdf:resource="{}" />"#,
                xml(&format!(
                    "{ns}{}",
                    on.map(|t| t.api_name.as_str()).unwrap_or("?")
                ))
            ),
        ]);
        for p in &a.parameters {
            out.push(format!(
                "    <onto:parameter><onto:apiName>{}</onto:apiName><rdfs:label>{}</rdfs:label><rdfs:range rdf:resource=\"http://www.w3.org/2001/XMLSchema#{}\" /><onto:required>{}</onto:required></onto:parameter>",
                xml(&p.api_name),
                xml(&p.display_name),
                p.data_type.xsd().split(':').nth(1).unwrap(),
                p.required,
            ));
        }
        out.push("  </onto:Action>".to_string());
    }

    for o in &store.state.objects {
        let t = type_by_id(&o.type_id);
        out.push(format!(
            r#"  <onto:{} rdf:about="{}">"#,
            t.map(|t| t.api_name.as_str()).unwrap_or("?"),
            xml(&format!("{ns}obj_{}", o.id)),
        ));
        out.push(format!("    <rdfs:label>{}</rdfs:label>", xml(&o.title)));
        out.push(format!("    <onto:objectId>{}</onto:objectId>", o.id));
        out.push(format!(
            "    <onto:createdAt>{}</onto:createdAt>",
            xml(&o.created_at)
        ));
        for (k, v) in &o.properties {
            let items: Vec<&Value> = match v {
                Value::Array(a) => a.iter().collect(),
                other => vec![other],
            };
            for item in items {
                let dt = match item {
                    Value::Number(_) => {
                        r#" rdf:datatype="http://www.w3.org/2001/XMLSchema#decimal""#
                    }
                    Value::Bool(_) => r#" rdf:datatype="http://www.w3.org/2001/XMLSchema#boolean""#,
                    _ => "",
                };
                let text = item
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| item.to_string());
                out.push(format!("    <onto:{k}{dt}>{}</onto:{k}>", xml(&text)));
            }
        }
        out.push(format!(
            "  </onto:{}>",
            t.map(|t| t.api_name.as_str()).unwrap_or("?")
        ));
    }

    let link_by_id = |id: &str| ontology.link_types.iter().find(|l| l.id == id);
    for l in &store.state.object_links {
        let lt = link_by_id(&l.link_type_id);
        out.extend([
            format!(
                r#"  <onto:{} rdf:about="{}">"#,
                lt.map(|x| x.api_name.as_str()).unwrap_or("?"),
                xml(&format!("{ns}lnk_{}", l.id)),
            ),
            format!("    <onto:linkId>{}</onto:linkId>", l.id),
            format!(
                r#"    <onto:source rdf:resource="{}" />"#,
                xml(&format!("{ns}obj_{}", l.source_object_id))
            ),
            format!(
                r#"    <onto:target rdf:resource="{}" />"#,
                xml(&format!("{ns}obj_{}", l.target_object_id))
            ),
            format!(
                "  </onto:{}>",
                lt.map(|x| x.api_name.as_str()).unwrap_or("?")
            ),
        ]);
    }

    out.push("</rdf:RDF>".to_string());
    out.join("\n")
}

// ---------- import: mini-DOM over quick-xml ----------

/// A tiny DOM node — namespace prefixes are kept literally (`owl:Class`),
/// which is all this format needs.
#[derive(Debug)]
struct XNode {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<XNode>,
    text: String,
}

impl XNode {
    fn new(name: String) -> Self {
        XNode {
            name,
            attrs: vec![],
            children: vec![],
            text: String::new(),
        }
    }
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

fn parse_dom(xml_text: &str) -> Result<XNode> {
    let mut reader = Reader::from_str(xml_text);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<XNode> = vec![XNode::new(String::new())]; // dummy root
    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let mut node = XNode::new(String::from_utf8_lossy(e.name().as_ref()).into_owned());
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                    let val = attr
                        .decode_and_unescape_value(reader.decoder())
                        .map_err(|e| Error::bad(format!("invalid XML attribute: {e}")))?
                        .into_owned();
                    node.attrs.push((key, val));
                }
                stack.push(node);
            }
            Event::Empty(e) => {
                let mut node = XNode::new(String::from_utf8_lossy(e.name().as_ref()).into_owned());
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                    let val = attr
                        .decode_and_unescape_value(reader.decoder())
                        .map_err(|e| Error::bad(format!("invalid XML attribute: {e}")))?
                        .into_owned();
                    node.attrs.push((key, val));
                }
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                }
            }
            Event::Text(e) => {
                let text = e
                    .unescape()
                    .map_err(|e| Error::bad(format!("invalid XML text: {e}")))?;
                if let Some(top) = stack.last_mut() {
                    if !top.text.is_empty() {
                        top.text.push(' ');
                    }
                    top.text.push_str(&text);
                }
            }
            Event::CData(e) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&String::from_utf8_lossy(&e.into_inner()));
                }
            }
            Event::End(_) => {
                if stack.len() > 1 {
                    let node = stack.pop().unwrap();
                    stack.last_mut().unwrap().children.push(node);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(stack.pop().unwrap())
}

fn kids<'a>(node: &'a XNode, tag: &str) -> Vec<&'a XNode> {
    node.children.iter().filter(|c| c.name == tag).collect()
}

fn child<'a>(node: &'a XNode, tag: &str) -> Option<&'a XNode> {
    node.children.iter().find(|c| c.name == tag)
}

fn about(node: &XNode) -> String {
    node.attr("rdf:about").map(local).unwrap_or("").to_string()
}

fn resource(node: Option<&XNode>) -> String {
    node.and_then(|n| n.attr("rdf:resource"))
        .map(local)
        .unwrap_or("")
        .to_string()
}

fn text_of(node: Option<&XNode>) -> String {
    node.map(|n| n.text.clone()).unwrap_or_default()
}

/// Literal coercion by `rdf:datatype` — numbers and booleans come back as
/// JSON values, everything else as strings (mirrors the TS `literal`).
fn literal(node: &XNode) -> Value {
    let raw = node.text.as_str();
    match node.attr("rdf:datatype") {
        Some(t) if t.ends_with("#integer") || t.ends_with("#decimal") => {
            // JS `Number("8")` is integer-valued — try i64 before f64 so a
            // round-trip keeps `8` rather than producing `8.0`.
            raw.parse::<i64>()
                .map(Value::from)
                .or_else(|_| raw.parse::<f64>().map(Value::from))
                .unwrap_or_else(|_| Value::from(raw))
        }
        Some(t) if t.ends_with("#boolean") => Value::from(raw == "true"),
        _ => Value::from(raw),
    }
}

fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn data_type_of(range: &str) -> DataType {
    let short = range
        .strip_prefix("http://www.w3.org/2001/XMLSchema#")
        .map(|l| format!("xsd:{l}"))
        .unwrap_or_else(|| range.to_string());
    DataType::from_xsd(&short).unwrap_or(DataType::String)
}

/// Parse an OWL (RDF/XML) document into the native shape. Round-trips
/// [`to_owl_rdf`] exactly.
pub fn parse_owl_rdf(xml_text: &str) -> Result<Parsed> {
    let dom = parse_dom(xml_text)?;
    let warnings: Vec<String> = Vec::new();
    // doc root is `rdf:RDF` when present, else whatever came back
    let root: &XNode = dom
        .children
        .iter()
        .find(|c| c.name == "rdf:RDF")
        .unwrap_or(&dom);

    let class_nodes = kids(root, "owl:Class");
    let object_props = kids(root, "owl:ObjectProperty");
    let data_props = kids(root, "owl:DatatypeProperty");
    let actions = kids(root, "onto:Action");
    let instances: Vec<&XNode> = root
        .children
        .iter()
        .filter(|c| c.name.starts_with("onto:") && c.name != "onto:Action")
        .collect();

    let mut object_types: Vec<ObjectType> = class_nodes
        .iter()
        .map(|node| {
            let id = {
                let s = text_of(child(node, "onto:objectTypeId"));
                if s.is_empty() { uuid() } else { s }
            };
            let api = {
                let s = text_of(child(node, "onto:apiName"));
                if s.is_empty() { about(node) } else { s }
            };
            ObjectType {
                id,
                api_name: api.clone(),
                display_name: {
                    let d = text_of(child(node, "rdfs:label"));
                    if d.is_empty() {
                        let a = about(node);
                        if a.is_empty() { "?".into() } else { a }
                    } else {
                        d
                    }
                },
                plural_display_name: {
                    let d = text_of(child(node, "onto:pluralDisplayName"));
                    if d.is_empty() { format!("{api}s") } else { d }
                },
                description: text_of(child(node, "onto:description")),
                domain: {
                    let d = text_of(child(node, "onto:domain"));
                    if d.is_empty() { "General".into() } else { d }
                },
                color: {
                    let d = text_of(child(node, "onto:color"));
                    if d.is_empty() { "#f0b35c".into() } else { d }
                },
                properties: vec![],
            }
        })
        .collect();

    // attach datatype properties to their domain class
    for t in &mut object_types {
        t.properties = data_props
            .iter()
            .filter(|p| resource(child(p, "rdfs:domain")) == t.api_name)
            .map(|p| {
                let api = {
                    let s = text_of(child(p, "onto:apiName"));
                    if !s.is_empty() {
                        s
                    } else {
                        about(p).rsplit('.').next().unwrap_or("prop").to_string()
                    }
                };
                PropertyDef {
                    api_name: api.clone(),
                    display_name: {
                        let d = text_of(child(p, "rdfs:label"));
                        if d.is_empty() { api } else { d }
                    },
                    data_type: data_type_of(&resource(child(p, "rdfs:range"))),
                    required: text_of(child(p, "onto:required")) == "true",
                    array: text_of(child(p, "onto:array")) == "true",
                    pattern: {
                        let s = text_of(child(p, "onto:pattern"));
                        if s.is_empty() { None } else { Some(s) }
                    },
                    enum_values: {
                        let vals: Vec<String> = kids(p, "onto:enumValues")
                            .iter()
                            .map(|e| text_of(Some(e)))
                            .collect();
                        if vals.is_empty() { None } else { Some(vals) }
                    },
                    unique: text_of(child(p, "onto:unique")) == "true",
                }
            })
            .collect();
    }

    let mut link_types: Vec<LinkType> = object_props
        .iter()
        .map(|node| {
            let api = {
                let s = text_of(child(node, "onto:apiName"));
                if s.is_empty() { about(node) } else { s }
            };
            LinkType {
                id: {
                    let s = text_of(child(node, "onto:objectTypeId"));
                    if s.is_empty() { uuid() } else { s }
                },
                api_name: api.clone(),
                display_name: {
                    let d = text_of(child(node, "rdfs:label"));
                    if d.is_empty() { api.clone() } else { d }
                },
                description: text_of(child(node, "onto:description")),
                source_type_id: String::new(),
                target_type_id: String::new(),
                source_cardinality: if text_of(child(node, "onto:sourceCardinality")) == "ONE" {
                    Cardinality::One
                } else {
                    Cardinality::Many
                },
                target_cardinality: if text_of(child(node, "onto:targetCardinality")) == "MANY" {
                    Cardinality::Many
                } else {
                    Cardinality::One
                },
            }
        })
        .collect();

    let mut action_types: Vec<ActionType> = actions
        .iter()
        .map(|node| {
            let api = {
                let s = text_of(child(node, "onto:apiName"));
                if s.is_empty() { about(node) } else { s }
            };
            ActionType {
                id: {
                    let s = text_of(child(node, "onto:objectTypeId"));
                    if s.is_empty() { uuid() } else { s }
                },
                api_name: api.clone(),
                display_name: {
                    let d = text_of(child(node, "rdfs:label"));
                    if d.is_empty() { api.clone() } else { d }
                },
                description: text_of(child(node, "onto:description")),
                object_type_id: String::new(),
                parameters: kids(node, "onto:parameter")
                    .iter()
                    .map(|p| {
                        let api = text_of(child(p, "onto:apiName"));
                        PropertyDef {
                            api_name: if api.is_empty() {
                                "?".into()
                            } else {
                                api.clone()
                            },
                            display_name: {
                                let d = text_of(child(p, "rdfs:label"));
                                if d.is_empty() { api } else { d }
                            },
                            data_type: data_type_of(&resource(child(p, "rdfs:range"))),
                            required: text_of(child(p, "onto:required")) == "true",
                            array: false,
                            pattern: None,
                            enum_values: None,
                            unique: false,
                        }
                    })
                    .collect(),
            }
        })
        .collect();

    let class_id_by_api = |api: &str| {
        object_types
            .iter()
            .find(|t| t.api_name == api)
            .map(|t| t.id.clone())
    };
    // resolve endpoints first — the map borrows link_types immutably
    let link_endpoints: Vec<(String, String)> = object_props
        .iter()
        .map(|node| {
            (
                class_id_by_api(&resource(child(node, "rdfs:domain"))).unwrap_or_default(),
                class_id_by_api(&resource(child(node, "rdfs:range"))).unwrap_or_default(),
            )
        })
        .collect();
    for (lt, (src, tgt)) in link_types.iter_mut().zip(link_endpoints) {
        lt.source_type_id = src;
        lt.target_type_id = tgt;
    }
    let action_targets: Vec<String> = actions
        .iter()
        .map(|node| class_id_by_api(&resource(child(node, "onto:objectType"))).unwrap_or_default())
        .collect();
    for (at, target) in action_types.iter_mut().zip(action_targets) {
        at.object_type_id = target;
    }
    let link_id_by_api = |api: &str| {
        link_types
            .iter()
            .find(|l| l.api_name == api)
            .map(|l| l.id.clone())
    };

    let known_class: std::collections::HashSet<&str> =
        object_types.iter().map(|t| t.api_name.as_str()).collect();
    let known_link: std::collections::HashSet<&str> =
        link_types.iter().map(|l| l.api_name.as_str()).collect();
    let mut objects: Vec<Obj> = vec![];
    let mut object_links: Vec<ObjectLink> = vec![];

    const SKIP: [&str; 3] = ["rdfs:label", "onto:objectId", "onto:createdAt"];

    for node in instances {
        let api = node.name.trim_start_matches("onto:");
        if api == "?" {
            continue;
        }
        if known_link.contains(api) {
            object_links.push(ObjectLink {
                id: {
                    let s = text_of(child(node, "onto:linkId"));
                    if !s.is_empty() {
                        s
                    } else {
                        let a = about(node);
                        let stripped = a.strip_prefix("lnk_").map(str::to_string);
                        stripped.unwrap_or_else(uuid)
                    }
                },
                link_type_id: link_id_by_api(api).unwrap_or_default(),
                source_object_id: resource(child(node, "onto:source"))
                    .trim_start_matches("obj_")
                    .to_string(),
                target_object_id: resource(child(node, "onto:target"))
                    .trim_start_matches("obj_")
                    .to_string(),
            });
            continue;
        }
        if !known_class.contains(api) {
            continue;
        }
        let mut props: std::collections::BTreeMap<String, Value> = Default::default();
        for c in &node.children {
            if !c.name.starts_with("onto:") || SKIP.contains(&c.name.as_str()) {
                continue;
            }
            let key = c.name.trim_start_matches("onto:").to_string();
            match props.entry(key) {
                std::collections::btree_map::Entry::Vacant(e) => {
                    e.insert(literal(c));
                }
                std::collections::btree_map::Entry::Occupied(mut e) => {
                    let v = e.get_mut();
                    match v {
                        Value::Array(a) => a.push(literal(c)),
                        _ => {
                            let first = v.clone();
                            *v = Value::Array(vec![first, literal(c)]);
                        }
                    }
                }
            }
        }
        objects.push(Obj {
            id: {
                let s = text_of(child(node, "onto:objectId"));
                if !s.is_empty() {
                    s
                } else {
                    let a = about(node);
                    let stripped = a.strip_prefix("obj_").map(str::to_string);
                    stripped.unwrap_or_else(uuid)
                }
            },
            type_id: class_id_by_api(api).unwrap_or_default(),
            title: {
                let d = text_of(child(node, "rdfs:label"));
                if d.is_empty() { "Untitled".into() } else { d }
            },
            properties: props,
            created_at: {
                let d = text_of(child(node, "onto:createdAt"));
                if d.is_empty() {
                    crate::util::now_iso()
                } else {
                    d
                }
            },
        });
    }

    let mut warnings = warnings;
    if object_types.is_empty() {
        warnings.push("No owl:Class found — nothing to import".into());
    }

    Ok(Parsed {
        state: crate::store::State {
            object_types,
            link_types,
            action_types,
            objects,
            object_links,
            layout: vec![],
        },
        warnings,
    })
}
