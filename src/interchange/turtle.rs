//! Turtle serialization — the same model as `.ttl` triples.
//! Port of `toTurtle` in `server/onto.ts` (export only; the TS app has no
//! Turtle import either).

use serde_json::Value;

use super::ONTO_NS;
use crate::store::Store;

fn ttl_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn ttl_value(v: &Value) -> String {
    match v {
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        other => {
            let s = other
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| other.to_string());
            format!("\"{}\"", ttl_escape(&s))
        }
    }
}

/// Export the whole store as a Turtle document.
pub fn to_turtle(store: &Store) -> String {
    let ontology = &store.state;
    let type_by_id = |id: &str| ontology.object_types.iter().find(|t| t.id == id);
    let mut lines: Vec<String> = vec![
        format!("@prefix onto: <{ONTO_NS}> ."),
        "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .".into(),
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .".into(),
        "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .".into(),
        String::new(),
    ];

    for t in &ontology.object_types {
        lines.extend([
            format!("onto:{} a rdfs:Class ;", t.api_name),
            format!("  rdfs:label \"{}\" ;", ttl_escape(&t.display_name)),
            format!("  onto:apiName \"{}\" ;", ttl_escape(&t.api_name)),
            format!("  onto:domain \"{}\" ;", ttl_escape(&t.domain)),
            format!("  onto:color \"{}\" .", ttl_escape(&t.color)),
            String::new(),
        ]);
        for p in &t.properties {
            lines.extend([
                format!("onto:{}.{} a rdf:Property ;", t.api_name, p.api_name),
                format!("  rdfs:label \"{}\" ;", ttl_escape(&p.display_name)),
                format!("  rdfs:domain onto:{} ;", t.api_name),
                format!("  rdfs:range {} .", p.data_type.xsd()),
                String::new(),
            ]);
        }
    }

    for l in &ontology.link_types {
        let src = type_by_id(&l.source_type_id);
        let tgt = type_by_id(&l.target_type_id);
        lines.extend([
            format!("onto:{} a rdf:Property ;", l.api_name),
            format!("  rdfs:label \"{}\" ;", ttl_escape(&l.display_name)),
            format!(
                "  rdfs:domain onto:{} ;",
                src.map(|t| t.api_name.as_str()).unwrap_or("?")
            ),
            format!(
                "  rdfs:range onto:{} ;",
                tgt.map(|t| t.api_name.as_str()).unwrap_or("?")
            ),
            format!(
                "  onto:sourceCardinality \"{}\" ;",
                l.source_cardinality.as_str()
            ),
            format!(
                "  onto:targetCardinality \"{}\" .",
                l.target_cardinality.as_str()
            ),
            String::new(),
        ]);
    }

    for o in &store.state.objects {
        let t = type_by_id(&o.type_id);
        let props = o
            .properties
            .iter()
            .map(|(k, v)| format!("  onto:{k} {}", ttl_value(v)))
            .collect::<Vec<_>>()
            .join(" ;\n");
        lines.push(format!(
            "onto:obj_{} a onto:{} ;\n  rdfs:label \"{}\"{} .",
            o.id,
            t.map(|t| t.api_name.as_str()).unwrap_or("?"),
            ttl_escape(&o.title),
            if props.is_empty() {
                String::new()
            } else {
                format!(" ;\n{props}")
            },
        ));
        lines.push(String::new());
    }

    let link_by_id = |id: &str| ontology.link_types.iter().find(|l| l.id == id);
    for l in &store.state.object_links {
        let lt = link_by_id(&l.link_type_id);
        lines.extend([
            format!(
                "onto:lnk_{} a onto:{} ;",
                l.id,
                lt.map(|x| x.api_name.as_str()).unwrap_or("?")
            ),
            format!("  onto:source onto:obj_{} ;", l.source_object_id),
            format!("  onto:target onto:obj_{} .", l.target_object_id),
            String::new(),
        ]);
    }

    lines.join("\n")
}
