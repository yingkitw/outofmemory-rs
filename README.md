# outofmemory — context engineering on an ontology

LLM agents run out of memory; the ontology doesn't. `outofmemory` is a Rust
crate (lib + CLI) that keeps a **Palantir Foundry–style ontology** — object
types with typed properties, link types with cardinalities, action types, and
the instances that live under them — as an agent's **external memory**, and
packs the relevant slice into a **token budget** on demand.

It is a faithful port of the [`outofmemory`](../outofmemory) ontology workbench
(Bun + TypeScript + React): same domain model, same validation and cascade
rules, same interchange formats, same seven MCP tools — plus the
context-engineering layer a library can offer that a UI cannot.

```
cargo test
cargo run --example pack_context -- "delayed shipments via Hamburg" 1200
```

## What it provides

| Capability | Module | What you get |
| --- | --- | --- |
| Domain model | [`model`](src/model.rs) | `ObjectType` / `LinkType` / `ActionType` / `Obj` / `ObjectLink`, serde-compatible with the TS `src/types.ts` byte-for-byte |
| Validated store | [`store`](src/store.rs) | In-memory `Store` with the API's rules: `apiName` uniqueness (409), endpoint type checks (400), duplicate links (409), FK-style cascades, `merge`/`replace` import, whole-state replace (undo backing) |
| Context packing | [`context`](src/context.rs) | Schema + instance Markdown rendering, query relevance scoring, graph-neighbourhood expansion, greedy packing under a token budget |
| Interchange | [`interchange`](src/interchange/) | Native JSON, JSON-LD, Turtle, OWL/RDF-XML, instances CSV — native→JSON-LD→native, native→OWL→native and native→Turtle→native round-trips are lossless |
| MCP tools | [`mcp`](src/mcp.rs) | The same 7 tools (`get_ontology`, `create_object_type`, `create_link_type`, `delete_object_type`, `list_instances`, `create_instance`, `link_instances`) over a stateless JSON-RPC handler — mount on stdio or HTTP |
| AI generation | [`generate`](src/generate.rs) | The `/api/generate` prompt assembly (existing types injected so models extend, not redefine) + repair-tolerant output parsing into the native payload |
| Seed data | [`seed`](src/seed.rs) | The workbench's deterministic supply-chain dataset: 9 types, 10 link types, 3 actions, 66 objects, ~89 links |

## Context engineering

```rust
use outofmemory::{seed, context::*};

let store = seed::supply_chain();
let packed = pack(&store, &ContextRequest {
    query: "delayed shipments through Hamburg".into(),
    token_budget: 1200,
    ..Default::default()
});
// packed.markdown — schema vocabulary + top-matched objects + their
// neighbourhood + the links between them, ≤ ~1200 estimated tokens
```

Spending order inside the window: **schema** (the vocabulary the model needs
to ground everything else) → **top-k query-matched objects** → **their
instance-link neighbourhood** → **a Links section** across included objects.
`PackedContext` reports exactly what fit (`seeds`, `neighbors`, `dropped`,
`estimated_tokens`).

## CLI

```bash
outofmemory seed --state onto.json                  # write the seeded world
outofmemory export --format turtle                  # json|jsonld|turtle|owl|csv
outofmemory import file.owl --format owl --mode merge
outofmemory validate onto.json
outofmemory context --query "delayed shipment" --budget 1200
outofmemory mcp --state onto.json                   # JSON-RPC over stdio
```

`mcp` reads one JSON-RPC message per line on stdin and writes responses to
stdout — point any MCP client at it:

```
claude mcp add outofmemory -- outofmemory mcp --state ontology.json
```

## Interchange fidelity

Exports are deterministic (stable ordering), so `native → JSON-LD → native`
and `native → OWL → native` compare equal to the source state — verified in
`tests/integration.rs`. Import accepts the native envelope or a flat
`{objectTypes…}` document, JSON-LD, or OWL/RDF-XML; `merge` matches by
apiName/id and resolves apiName endpoints (case-insensitively, so
AI-generated `CONTAINER` links reach a seeded `Container` type); `replace`
swaps the whole state atomically.

## Feature: `repair`

Malformed JSON (hand-edited files, model output) is repaired before parsing
via [`anyrepair`](https://crates.io/crates/anyrepair) in
`parse_native_lenient` and `parse_generated`. Disable with
`--no-default-features` to drop the dependency.

## Learned from the workbench

Design choices ported deliberately:

- **`apiName` is the stable identifier** — display names never leak into
  exports or cross-references; merge keys on apiName.
- **Schema and instances are separate planes** — `Ontology` (types) vs
  `objects`/`object_links` (data), exactly like the DB schema.
- **Cardinality reads `source : target`** — `MANY:ONE` = many sources point
  at one target, rendered `N:1`.
- **Cascades follow the FK graph** — deleting an object type removes its
  link types, actions, instances, instance links and layout row.
- **Interchange is lossless** — `onto:` extension terms ride along in
  JSON-LD and OWL so our metadata survives a trip through RDF tooling.
- **One JSON-RPC surface, zero extra machinery** — the MCP handler is a
  function, not a server; stateless mode like the TS app.
