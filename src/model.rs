//! Shared domain model — mirrors `src/types.ts` in the TypeScript workbench
//! so exports from either implementation can be exchanged losslessly.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Property value type. Serializes exactly like the TS union
/// (`"string" | "integer" | "decimal" | "boolean" | "date" | "timestamp"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    String,
    Integer,
    Decimal,
    Boolean,
    Date,
    Timestamp,
}

impl DataType {
    pub const ALL: [DataType; 6] = [
        DataType::String,
        DataType::Integer,
        DataType::Decimal,
        DataType::Boolean,
        DataType::Date,
        DataType::Timestamp,
    ];

    /// `xsd:string`, `xsd:integer`, … — used by JSON-LD/Turtle/OWL export.
    pub fn xsd(self) -> &'static str {
        match self {
            DataType::String => "xsd:string",
            DataType::Integer => "xsd:integer",
            DataType::Decimal => "xsd:decimal",
            DataType::Boolean => "xsd:boolean",
            DataType::Date => "xsd:date",
            DataType::Timestamp => "xsd:dateTime",
        }
    }

    /// Inverse of [`DataType::xsd`], tolerant of the wider XSD family.
    pub fn from_xsd(range: &str) -> Option<DataType> {
        Some(match range {
            "xsd:string" => DataType::String,
            "xsd:integer" | "xsd:int" | "xsd:long" => DataType::Integer,
            "xsd:decimal" | "xsd:double" | "xsd:float" => DataType::Decimal,
            "xsd:boolean" => DataType::Boolean,
            "xsd:date" => DataType::Date,
            "xsd:dateTime" => DataType::Timestamp,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DataType::String => "string",
            DataType::Integer => "integer",
            DataType::Decimal => "decimal",
            DataType::Boolean => "boolean",
            DataType::Date => "date",
            DataType::Timestamp => "timestamp",
        }
    }
}

/// Link cardinality on one endpoint. `source : target` reads
/// `MANY : ONE` as "many sources may point at one target" (`N:1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Cardinality {
    One,
    Many,
}

impl Cardinality {
    pub fn as_str(self) -> &'static str {
        match self {
            Cardinality::One => "ONE",
            Cardinality::Many => "MANY",
        }
    }
}

/// Typed property declared on an [`ObjectType`] (or a parameter on an [`ActionType`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyDef {
    pub api_name: String,
    pub display_name: String,
    pub data_type: DataType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub array: bool,
}

/// An entity class — the Foundry "object type".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectType {
    pub id: String,
    pub api_name: String,
    pub display_name: String,
    #[serde(default)]
    pub plural_display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub properties: Vec<PropertyDef>,
}

/// A relationship class between two object types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkType {
    pub id: String,
    pub api_name: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    pub source_type_id: String,
    pub target_type_id: String,
    pub source_cardinality: Cardinality,
    pub target_cardinality: Cardinality,
}

impl LinkType {
    /// `N:1` / `1:N` / `1:1` / `N:N` — the canvas label.
    pub fn cardinality_label(&self) -> String {
        format!(
            "{}:{}",
            if self.source_cardinality == Cardinality::One {
                "1"
            } else {
                "N"
            },
            if self.target_cardinality == Cardinality::One {
                "1"
            } else {
                "N"
            },
        )
    }
}

/// A named operation on an object type, with typed parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionType {
    pub id: String,
    pub api_name: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    pub object_type_id: String,
    #[serde(default)]
    pub parameters: Vec<PropertyDef>,
}

/// The schema plane: types only, no instances.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ontology {
    #[serde(default)]
    pub object_types: Vec<ObjectType>,
    #[serde(default)]
    pub link_types: Vec<LinkType>,
    #[serde(default)]
    pub action_types: Vec<ActionType>,
}

/// An object instance (`Obj` in the TS — `Object` is a reserved name elsewhere).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Obj {
    pub id: String,
    pub type_id: String,
    pub title: String,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
    #[serde(default)]
    pub created_at: String,
}

/// An edge between two object instances, typed by a [`LinkType`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectLink {
    pub id: String,
    pub link_type_id: String,
    pub source_object_id: String,
    pub target_object_id: String,
}

/// Persisted canvas position for an object type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutEntry {
    pub type_id: String,
    pub x: f64,
    pub y: f64,
}

/// Stable UPPER_SNAKE identifier from free text — same algorithm as the TS
/// `slugify` (camelCase split, non-alnum → `_`, trimmed, capped at 48 chars).
pub fn slugify(s: &str) -> String {
    // "camelCase boundary" → space, mirroring /([a-z0-9])([A-Z])/ → "$1 $2"
    let mut spaced = String::with_capacity(s.len() + 8);
    let mut prev = '\0';
    for c in s.trim().chars() {
        if (prev.is_ascii_lowercase() || prev.is_ascii_digit()) && c.is_ascii_uppercase() {
            spaced.push(' ');
        }
        spaced.push(c);
        prev = c;
    }
    let slug: String = spaced
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    // collapse runs of '_' and trim, mirroring /^_+|_+$/ cleanup
    let mut out = String::with_capacity(slug.len());
    let mut last_us = true; // trims a leading run
    for c in slug.chars() {
        if c == '_' {
            if !last_us {
                out.push('_');
            }
            last_us = true;
        } else {
            out.push(c);
            last_us = false;
        }
    }
    let out = out.trim_end_matches('_');
    out.chars().take(48).collect::<String>().to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_cases() {
        assert_eq!(slugify("Shipment"), "SHIPMENT");
        assert_eq!(slugify("carriedBy"), "CARRIED_BY");
        assert_eq!(slugify("  some type!  "), "SOME_TYPE");
        assert_eq!(slugify("A".repeat(60).as_str()), "A".repeat(48));
        assert_eq!(slugify("__weird__name__"), "WEIRD_NAME");
    }

    #[test]
    fn cardinality_labels() {
        let l = LinkType {
            id: "x".into(),
            api_name: "CARRIED_BY".into(),
            display_name: "Carried by".into(),
            description: String::new(),
            source_type_id: "a".into(),
            target_type_id: "b".into(),
            source_cardinality: Cardinality::Many,
            target_cardinality: Cardinality::One,
        };
        assert_eq!(l.cardinality_label(), "N:1");
    }

    #[test]
    fn json_field_names_match_ts() {
        let t = ObjectType {
            id: "t1".into(),
            api_name: "SHIPMENT".into(),
            display_name: "Shipment".into(),
            plural_display_name: "Shipments".into(),
            description: String::new(),
            domain: "Logistics".into(),
            color: "#f0b35c".into(),
            properties: vec![PropertyDef {
                api_name: "teu".into(),
                display_name: "Volume (TEU)".into(),
                data_type: DataType::Integer,
                required: true,
                array: false,
            }],
        };
        let v = serde_json::to_value(&t).unwrap();
        assert_eq!(v["apiName"], "SHIPMENT");
        assert_eq!(v["pluralDisplayName"], "Shipments");
        assert_eq!(v["properties"][0]["dataType"], "integer");
        assert_eq!(v["properties"][0]["required"], true);
        assert_eq!(v["properties"][0]["array"], false);
    }
}
