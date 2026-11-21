//! End-to-end checks mirroring `scripts/roundtrip.ts` and the API semantics
//! in the TS workbench: interchange round-trips, import modes, store
//! validation/cascades, context packing, MCP tools and the AI-generation
//! payload path.

use outofmemory::context::{self, ContextRequest};
use outofmemory::error::ErrorKind;
use outofmemory::generate;
use outofmemory::interchange::*;
use outofmemory::mcp;
use outofmemory::model::*;
use outofmemory::seed;
use outofmemory::store::{ImportMode, State, Store};

fn seeded() -> Store {
    seed::supply_chain()
}

// ---------- interchange round-trips ----------

#[test]
fn native_json_round_trip() {
    let store = seeded();
    let text = to_native_json(&store);
    let parsed = parse_native(&text).unwrap();
    let mut fresh = Store::new();
    fresh.import(parsed.state, ImportMode::Replace);
    assert_eq!(
        serde_json::to_value(&fresh.state).unwrap(),
        serde_json::to_value(&store.state).unwrap()
    );
}

#[test]
fn jsonld_round_trip() {
    let store = seeded();
    let doc = to_json_ld(&store);
    let parsed = parse_json_ld(&doc);
    assert!(
        parsed.warnings.is_empty(),
        "warnings: {:?}",
        parsed.warnings
    );
    let mut fresh = Store::new();
    fresh.import(parsed.state, ImportMode::Replace);
    assert_eq!(
        serde_json::to_value(&fresh.state).unwrap(),
        serde_json::to_value(&store.state).unwrap(),
        "JSON-LD round-trip must be lossless"
    );
}

#[test]
fn owl_round_trip() {
    let store = seeded();
    let xml = to_owl_rdf(&store);
    assert!(xml.contains("<owl:Class"));
    assert!(xml.contains("<owl:ObjectProperty"));
    let parsed = parse_owl_rdf(&xml).unwrap();
    let mut fresh = Store::new();
    fresh.import(parsed.state, ImportMode::Replace);
    assert_eq!(
        serde_json::to_value(&fresh.state).unwrap(),
        serde_json::to_value(&store.state).unwrap(),
        "OWL round-trip must be lossless"
    );
}

#[test]
fn turtle_round_trip() {
    let store = seeded();
    let ttl = to_turtle(&store);
    assert!(ttl.contains("onto:Shipment a rdfs:Class"));
    assert!(ttl.contains("onto:CARRIED_BY a rdf:Property"));
    assert!(ttl.contains("onto:DELAY_SHIPMENT a onto:Action"));
    let parsed = parse_turtle(&ttl).unwrap();
    assert!(
        parsed.warnings.is_empty(),
        "warnings: {:?}",
        parsed.warnings
    );
    let mut fresh = Store::new();
    fresh.import(parsed.state, ImportMode::Replace);
    assert_eq!(
        serde_json::to_value(&fresh.state).unwrap(),
        serde_json::to_value(&store.state).unwrap(),
        "Turtle round-trip must be lossless"
    );
}

#[test]
fn csv_export() {
    let store = seeded();
    let csv = to_instances_csv(&store);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), store.state.objects.len() + 1); // header + rows
    assert!(lines[0].starts_with("type,title,id,"));
    assert!(csv.lines().any(|l| l.starts_with("Shipment,SH-2611,")));
}

#[test]
fn jsonld_import_of_foreign_document() {
    // a hand-written JSON-LD doc with no onto:* metadata still imports
    let doc = serde_json::json!({
        "@graph": [
            { "@id": "https://example.com/WIDGET", "@type": "rdfs:Class", "rdfs:label": "Widget" },
            {
                "@id": "https://example.com/WIDGET.size", "@type": "rdf:Property",
                "rdfs:domain": { "@id": "https://example.com/WIDGET" },
                "rdfs:range": "xsd:integer"
            },
        ]
    });
    let parsed = parse_json_ld(&doc);
    assert_eq!(parsed.state.object_types.len(), 1);
    let t = &parsed.state.object_types[0];
    assert_eq!(t.api_name, "https://example.com/WIDGET"); // no onto:apiName → falls back to @id
    assert_eq!(t.properties.len(), 1);
    assert_eq!(t.properties[0].data_type, DataType::Integer);
}

// ---------- store semantics ----------

#[test]
fn api_name_uniqueness_is_conflict() {
    let mut s = seeded();
    let dup = ObjectType {
        id: String::new(),
        api_name: "Shipment".into(), // seeded apiName
        display_name: "Duplicate".into(),
        plural_display_name: String::new(),
        description: String::new(),
        domain: String::new(),
        color: String::new(),
        properties: vec![],
    };
    assert_eq!(
        s.create_object_type(dup).unwrap_err().kind,
        ErrorKind::Conflict
    );
}

#[test]
fn merge_adds_missing_replace_swaps_all() {
    let mut store = seeded();
    let types_before = store.state.object_types.len();

    let mut incoming = State::default();
    incoming.object_types.push(ObjectType {
        id: "new-id".into(),
        api_name: "Warehouse".into(),
        display_name: "Warehouse".into(),
        plural_display_name: "Warehouses".into(),
        description: String::new(),
        domain: String::new(),
        color: String::new(),
        properties: vec![],
    });
    let summary = store.merge(incoming.clone());
    assert_eq!(store.state.object_types.len(), types_before + 1);
    assert_eq!(summary.object_types, 1);

    // merging again adds nothing (matched by apiName)
    store.merge(incoming);
    assert_eq!(store.state.object_types.len(), types_before + 1);

    // replace keeps only the incoming state
    store.import(State::default(), ImportMode::Replace);
    assert!(store.state.object_types.is_empty());
}

// ---------- context engineering ----------

#[test]
fn schema_renders_all_types() {
    let store = seeded();
    let md = context::render_schema_md(&store);
    assert!(md.contains("# Ontology"));
    assert!(md.contains("### Shipment — Shipments (Logistics)"));
    assert!(md.contains("- CARRIED_BY : Shipment —N:1→ Vessel"));
    assert!(md.contains("DELAY_SHIPMENT("));
}

#[test]
fn pack_respects_budget_and_finds_relevant() {
    let store = seeded();
    let packed = context::pack(
        &store,
        &ContextRequest {
            query: "delayed shipment".into(),
            token_budget: 2000,
            ..Default::default()
        },
    );
    assert!(packed.estimated_tokens <= 2000);
    assert!(packed.markdown.contains("# Ontology"));
    assert!(packed.markdown.contains("## Instances"));
    assert!(packed.seeds > 0);
    // neighbourhood expansion pulled in linked objects
    assert!(packed.neighbors > 0 || packed.link_ids.is_empty());
}

#[test]
fn pack_tiny_budget_still_returns_schema() {
    let store = seeded();
    let packed = context::pack(
        &store,
        &ContextRequest {
            query: "anything".into(),
            token_budget: 400, // smaller than the schema alone
            ..Default::default()
        },
    );
    assert!(packed.estimated_tokens <= 400);
    assert!(packed.seeds == 0);
}

#[test]
fn empty_query_packs_by_budget() {
    let store = seeded();
    let packed = context::pack(
        &store,
        &ContextRequest {
            query: String::new(),
            token_budget: 1200,
            max_seed_objects: 4,
            ..Default::default()
        },
    );
    assert!(packed.estimated_tokens <= 1200);
    assert!(packed.seeds >= 1);
}

// ---------- MCP ----------

fn rpc(store: &mut Store, method: &str, params: serde_json::Value) -> serde_json::Value {
    let msg = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let out = mcp::handle_rpc(store, &msg.to_string()).unwrap();
    serde_json::from_str(&out).unwrap()
}

#[test]
fn mcp_full_session() {
    let mut store = Store::new();

    let init = rpc(
        &mut store,
        "initialize",
        serde_json::json!({ "protocolVersion": "2025-06-18" }),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "outofmemory");

    let list = rpc(&mut store, "tools/list", serde_json::json!({}));
    assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 7);

    // build a tiny ontology through the tools
    let call = |s: &mut Store, name: &str, args: serde_json::Value| -> serde_json::Value {
        let res = rpc(
            s,
            "tools/call",
            serde_json::json!({ "name": name, "arguments": args }),
        );
        assert!(res["result"]["isError"].is_null(), "{name} errored: {res}");
        let text = res["result"]["content"][0]["text"].as_str().unwrap();
        serde_json::from_str(text).unwrap()
    };

    call(
        &mut store,
        "create_object_type",
        serde_json::json!({
            "apiName": "AUTHOR", "displayName": "Author",
            "properties": [{ "apiName": "email", "displayName": "Email", "dataType": "string", "required": true }]
        }),
    );
    call(
        &mut store,
        "create_object_type",
        serde_json::json!({
            "apiName": "BOOK", "displayName": "Book",
        }),
    );
    call(
        &mut store,
        "create_link_type",
        serde_json::json!({
            "apiName": "WROTE", "displayName": "Wrote",
            "sourceTypeApiName": "AUTHOR", "targetTypeApiName": "BOOK",
            "sourceCardinality": "ONE", "targetCardinality": "MANY",
        }),
    );
    let author = call(
        &mut store,
        "create_instance",
        serde_json::json!({
            "typeApiName": "AUTHOR", "title": "Octavia Butler",
        }),
    );
    let book = call(
        &mut store,
        "create_instance",
        serde_json::json!({
            "typeApiName": "BOOK", "title": "Kindred",
        }),
    );
    call(
        &mut store,
        "link_instances",
        serde_json::json!({
            "linkTypeApiName": "WROTE",
            "sourceObjectId": author["id"], "targetObjectId": book["id"],
        }),
    );

    let onto = call(&mut store, "get_ontology", serde_json::json!({}));
    assert_eq!(onto["instanceCounts"]["objects"], 2);
    assert_eq!(onto["instanceCounts"]["links"], 1);

    let instances = call(
        &mut store,
        "list_instances",
        serde_json::json!({ "typeApiName": "BOOK" }),
    );
    assert_eq!(instances.as_array().unwrap().len(), 1);
    assert_eq!(instances[0]["type"], "BOOK");

    // notifications get no reply; parse errors get -32700
    assert!(
        mcp::handle_rpc(
            &mut store,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
        )
        .is_none()
    );
    let err = mcp::handle_rpc(&mut store, "not json").unwrap();
    assert!(err.contains("-32700"));
}

// ---------- AI generation path ----------

#[test]
fn generate_prompt_includes_existing_types() {
    let store = seeded();
    let files = [generate::SourceFile {
        name: "notes.txt".into(),
        text: "warehouses store pallets".into(),
    }];
    let (system, user) = generate::build_prompt(&store, &files, "focus on storage");
    assert!(system.contains("ontology architect"));
    assert!(user.contains("\"apiName\":\"Shipment\""));
    assert!(user.contains("USER FOCUS"));
    assert!(user.contains("notes.txt"));
}

#[test]
fn parse_generated_normalizes_and_merges() {
    let mut store = seeded();
    // model output: TitleCase apiName, links to existing type by apiName
    let model_json = r#"{
        "objectTypes": [{
            "apiName": "warehouse", "displayName": "Warehouse",
            "description": "A storage facility.", "domain": "Facility",
            "properties": [{ "apiName": "capacity", "displayName": "Capacity", "dataType": "integer", "required": false, "array": false }]
        }],
        "linkTypes": [{
            "apiName": "stores", "displayName": "Stores", "description": "",
            "sourceApiName": "warehouse", "targetApiName": "Container",
            "sourceCardinality": "ONE", "targetCardinality": "MANY"
        }]
    }"#;
    let payload = generate::parse_generated(model_json).unwrap();
    // UPPER_SNAKE normalization happened
    assert_eq!(
        payload["ontology"]["objectTypes"][0]["apiName"],
        "WAREHOUSE"
    );
    assert_eq!(payload["ontology"]["linkTypes"][0]["apiName"], "STORES");

    // feed through the same merge path the TS import preview uses
    let parsed = parse_native(&payload.to_string()).unwrap();
    let summary = store.merge(parsed.state);
    assert_eq!(summary.object_types, 1);
    let link = store.link_type_by_api("STORES").unwrap();
    let container_id = store.object_type_by_api("Container").unwrap().id.clone();
    assert_eq!(link.target_type_id, container_id); // apiName resolved to existing id
}

#[cfg(feature = "repair")]
#[test]
fn parse_generated_repairs_malformed_output() {
    // trailing comma + unquoted key — typical model damage
    let broken = r#"{
        objectTypes: [{ apiName: "dock", displayName: "Dock", description: "", domain: "", properties: [], }],
        "linkTypes": [],
    }"#;
    let payload = generate::parse_generated(broken).unwrap();
    assert_eq!(payload["ontology"]["objectTypes"][0]["apiName"], "DOCK");
}
