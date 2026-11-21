//! Instance data as CSV — one row per object, property columns are the union
//! across types (arrays JSON-ified). Port of `toInstancesCsv`.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::store::Store;

fn csv_cell(v: &Value) -> String {
    let s = match v {
        Value::Array(_) => v.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

/// Export instance rows — `format=csv` in the TS app.
pub fn to_instances_csv(store: &Store) -> String {
    let ontology = &store.state;
    let type_by_id = |id: &str| ontology.object_types.iter().find(|t| t.id == id);

    let mut cols: BTreeSet<&str> = BTreeSet::new();
    for t in &ontology.object_types {
        for p in &t.properties {
            cols.insert(p.api_name.as_str());
        }
    }

    let mut out = vec![
        ["type", "title", "id"]
            .into_iter()
            .chain(cols.iter().copied())
            .collect::<Vec<_>>()
            .join(","),
    ];

    for o in &store.state.objects {
        let t = type_by_id(&o.type_id);
        let mut cells = vec![
            csv_cell(&Value::String(
                t.map(|t| t.api_name.clone()).unwrap_or_else(|| "?".into()),
            )),
            csv_cell(&Value::String(o.title.clone())),
            csv_cell(&Value::String(o.id.clone())),
        ];
        for c in &cols {
            if let Some(v) = o.properties.get(*c) {
                cells.push(csv_cell(v));
            }
        }
        out.push(cells.join(","));
    }
    out.join("\n")
}
