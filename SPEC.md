# SPEC — outofmemory

Interface contract for the crate and CLI. Semantics mirror the TS workbench
(`server/app.ts`, `server/onto.ts`, `server/mcp.ts`); where behaviour differs
it is marked **[diff]**.

## Domain model

All types serialize with the same camelCase field names as `src/types.ts`.

```rust
DataType      = "string" | "integer" | "decimal" | "boolean" | "date" | "timestamp"
Cardinality   = "ONE" | "MANY"

PropertyDef   { apiName, displayName, dataType, required = false, array = false }
ObjectType    { id, apiName, displayName, pluralDisplayName, description, domain, color, properties[] }
LinkType      { id, apiName, displayName, description, sourceTypeId, targetTypeId,
                sourceCardinality, targetCardinality }
ActionType    { id, apiName, displayName, description, objectTypeId, parameters[] }
Ontology      { objectTypes[], linkTypes[], actionTypes[] }
Obj           { id, typeId, title, properties{}, createdAt }
ObjectLink    { id, linkTypeId, sourceObjectId, targetObjectId }
LayoutEntry   { typeId, x, y }
State         { objectTypes[], linkTypes[], actionTypes[], objects[], objectLinks[], layout[] }
```

Cardinality reads `source : target` — `MANY:ONE` = many sources may point at
one target (`N:1` via `LinkType::cardinality_label`).

## Errors

`Error { kind, message }` where `ErrorKind` maps to the HTTP status the TS API
would have returned:

| `ErrorKind` | Status | Raised when |
| --- | --- | --- |
| `BadRequest` | 400 | missing required field, invalid reference, type mismatch on link endpoints |
| `NotFound` | 404 | update/delete of a missing entity |
| `Conflict` | 409 | duplicate apiName, duplicate `(linkType, source, target)` link |

## Store semantics (`store.rs`)

Creation defaults mirror the API: `pluralDisplayName` = `displayName + "s"`,
`domain` = `"General"`, `color` = `"#f0b35c"`, ids are UUIDv4, `createdAt` is
ISO-8601 UTC. `apiName`/`displayName`/`title` are trimmed and required.

Cascades (the DB's `ON DELETE CASCADE` rules):

- `delete_object_type` → its link types (either endpoint), action types,
  objects, object links (of removed objects and removed link types), layout row
- `delete_link_type` / `delete_object` → the object links referencing them

`merge(incoming)` — keep what exists, add what is missing (matched by apiName
or id); `sourceTypeId`/`targetTypeId`/`typeId`/`objectTypeId`/`linkTypeId`
values equal to an apiName resolve to that entity's id. **[diff]** Resolution
is exact then ASCII case-insensitive, so UPPER_SNAKE-normalized generated
payloads link to TitleCase seeded types (the TS merge leaves those dangling).

`replace_all(state)` — atomic whole-state replace; returns a `Summary` of counts.

## Interchange

`Format::{Native, JsonLd, Turtle, Owl, Csv}`; `Format::parse` also accepts
`json`, `json-ld`, `ttl`, `rdf`, `rdfxml`, `rdf-xml`, `native`, `outofmemory`.

- **Native** — `{exportedAt, format:"outofmemory", ontology, objects, objectLinks, layout}`
  Export is pretty-printed; import accepts the envelope or a flat
  `{objectTypes…}` document.
- **JSON-LD** — `rdfs:Class` per object type, `rdf:Property` per typed
  property (domain→class, range→`xsd:*`) and per link type (domain/range→
  classes, `onto:linkType`, `onto:sourceCardinality`, …), `onto:Action` per
  action type, `onto:{Type}` nodes per instance, `onto:{LinkType}` nodes per
  instance link. `local()` strips `https?://(outofmemory|ontic).local/onto/`
  only; `onto:`/`rdfs:`/`rdf:` terms pass through.
- **Turtle** — same model as `.ttl` triples; carries the same `onto:*`
  annotations as JSON-LD (including `onto:Action` with `[ … ]` blank-node
  parameters and `onto:linkId`/`onto:objectId`/`onto:createdAt`). Import is a
  hand-rolled Turtle-subset parser: `@prefix`/`PREFIX`/`BASE`, `a`, `;`/`,`
  lists, `<iri>`/prefixed names, literals (`i64` before `f64`), `[ … ]` blank
  nodes. Array-valued properties serialize as comma-lists and read back as
  arrays when the property def says `array`.
- **OWL** — RDF/XML: `owl:Class`, `owl:DatatypeProperty`,
  `owl:ObjectProperty`, `onto:Action`, `onto:{Type}` instances and
  `onto:{LinkType}` links. `onto:*` annotations keep the format lossless.
- **CSV** — one row per object; columns `type,title,id` + union of property
  apiNames; arrays JSON-encoded. Rows only carry columns the object defines
  (mirrors the TS, ragged by design). Export only.

`import(store, format, payload, mode)` accepts `Native`, `JsonLd`, `Owl`,
`Turtle`;
`parse_native_lenient` (feature `repair`) repairs malformed JSON via
`anyrepair` first.

## Context engineering (`context.rs`)

`estimate_tokens(text) = chars/4 + 1` — approximation for packing only.

`ContextRequest { query, token_budget = 2048, include_schema = true,
max_seed_objects = 8, neighborhood_hops = 1 }`

`pack(store, req) -> PackedContext`:

1. `render_schema_md` (object types with properties/actions grouped, link
   types with `—N:1→` cardinalities) — always included first; if it alone
   exceeds the budget the window is truncated at `4·budget−4` chars.
2. Seed pass — top-`max_seed_objects` objects by relevance score
   (query term hit in title = 2, in any property value = 1; score-0 objects
   are skipped unless the query is empty).
3. Neighbourhood pass — BFS up to `neighborhood_hops` over instance links
   from included seeds, higher-scoring neighbours first, each included only
   if its rendered block fits the remaining budget.
4. `## Links` — every instance link between included objects, if it fits.

`PackedContext { markdown, object_ids, link_ids, estimated_tokens, seeds,
neighbors, dropped }` — the markdown order is `# Ontology` → `## Object
types` → `## Link types` → `## Instances` → `## Links`.

## MCP (`mcp.rs`)

`handle_rpc(store, msg) -> Option<String>` — stateless JSON-RPC
(versions `2024-11-05`, `2025-03-26`, `2025-06-18`). Notifications and
unparseable input get `None`/`-32700`; batch is rejected (`-32600`).

Tools (input/output identical to `server/mcp.ts`): `get_ontology`,
`create_object_type`, `create_link_type`, `delete_object_type`,
`list_instances`, `create_instance`, `link_instances`. Tool errors return
`{isError: true}` with the message — never a JSON-RPC error.

## AI generation (`generate.rs`)

`build_prompt(store, files, instructions) -> (system, user)` — the verbatim
`/api/generate` system prompt plus `EXISTING ONTOLOGY` (apiName + displayName
+ property apiNames, link apiNames) and the file digest.
`output_schema()` is the `generatedSchema` zod shape as JSON Schema.
`parse_generated(text) -> {ontology:…, objects:[], objectLinks:[]}` —
normalizes apiNames via `slugify` (robust version of the TS `toUpperCase`),
fresh UUIDs, link endpoints left as apiNames for `Store::merge` to resolve.

## CLI

```
outofmemory seed     [--state F]                     write seeded ontology
outofmemory export   [--state F] [--format F]        print export on stdout
outofmemory import   <file> [--state F] [--format F] [--mode merge|replace]
outofmemory validate <file> [--format F]             parse + referential check
outofmemory context  [--state F] --query Q [--budget N] [--hops N]
outofmemory mcp      [--state F]                     newline-delimited JSON-RPC
```

Defaults: `--state ontology.json`, `--format outofmemory`, `--mode merge`,
`--budget 2048`, `--hops 1`. Exit codes: `0` ok, `1` validation problems
(`validate` only), `2` usage/IO error.
