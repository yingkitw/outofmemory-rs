# ARCHITECTURE — outofmemory

## Shape

```
src/
  lib.rs            crate docs + public re-exports (model types, Store, Error, Result)
  model.rs          domain model — serde-compatible with src/types.ts
  error.rs          Error/ErrorKind/Result — 400/404/409 semantics
  store.rs          Store: validation, cascades, merge/replace, whole-state replace
  context.rs        context engineering: markdown render, relevance, packing
  mcp.rs            7 tools + stateless JSON-RPC handler over &mut Store
  generate.rs       AI-generation prompt assembly + repair-tolerant parsing
  seed.rs           deterministic supply-chain dataset (mulberry32)
  interchange/
    mod.rs          Format, Parsed, dispatch, local(), native JSON envelope
    jsonld.rs       to_json_ld / parse_json_ld
    turtle.rs       to_turtle / parse_turtle (hand-rolled Turtle-subset parser)
    owl.rs          to_owl_rdf / parse_owl_rdf (mini-DOM over quick-xml)
    csv.rs          to_instances_csv (export only)
  util.rs           now_iso() — ISO-8601 UTC without a datetime dep
  main.rs           CLI: seed/export/import/validate/context/mcp
tests/
  integration.rs    round-trips, import modes, MCP session, context packing, generate
examples/
  pack_context.rs   pack a query into a token budget and print the window
```

## Dependency directions

```
model ────────────────────────► (leaf — no crate deps)
util  ────────────────────────► (leaf)
error ────────────────────────► (leaf)
store ──► model, error, util
interchange ──► model, store, error, util
context ──► model, store
mcp ──► model, store, error
generate ──► model, store, error, (anyrepair behind `repair` feature)
seed ──► model, store
main ──► everything (binary only)
```

`Store` is the single owner of all entities (`State` = five `Vec`s +
`layout`). Everything else is a view or a codec onto it — there is no
database, no runtime, no async. A server (HTTP/REST) can sit on the same
core exactly the way `mcp.rs` sits on it today.

## Key design decisions

- **`State` is the interchange and undo unit.** `replace_all`/`merge`/
  `import` all take `State`; `Parsed { state, warnings }` is what every
  importer produces. Every mutator checkpoints a `State` snapshot after
  validation (`undo()`/`redo()` restore it — bulk ops are one step), so the
  workbench's undo model holds without a command pattern. The MCP tools and
  REST routes in the TS app reduce to `Store` method calls.
- **apiName is the foreign key across documents.** Merge resolution and the
  generated-payload path both rely on apiName → id lookups; case-insensitive
  fallback is deliberate (see `Store::merge` doc) because apiName casing
  conventions differ between the seed (`Container`) and generated payloads
  (`CONTAINER`).
- **Lossless round-trips via `onto:` annotations.** Both RDF serializations
  carry our metadata (`onto:apiName`, `onto:objectTypeId`, cardinalities),
  so `native → JSON-LD/OWL → native` restores identical state — enforced by
  tests, not convention.
- **Context packing is spend-ordered, not truncation.** The schema is the
  vocabulary, so it is funded first; instances are ranked by query
  relevance, then expanded over the instance-link graph within remaining
  budget. `PackedContext` exposes the accounting so callers can re-pack
  wider/narrower.
- **Transport-agnostic MCP.** `handle_rpc(&mut Store, &str) -> Option<String>`
  is a pure function over the store — the stdio loop in `main.rs` is ~15
  lines, an HTTP mount is the same call.
- **Hand-rolled where cheap, deps where real work is.** `now_iso` (20 lines)
  avoids chrono; OWL parsing uses a 60-line mini-DOM over quick-xml rather
  than a full RDF stack; `serde_json` does all JSON work; `anyrepair`
  (optional) handles malformed JSON.

## Data flow — context packing

```
Store ──► render_schema_md ─────────────────────────────┐
     ──► rank_objects(query) ──► top-k seeds ──┐        │
                              └─► BFS over object_links (hops) ──► try_include (budget gate)
     ──► links between included ids ──► ## Links ──► PackedContext {markdown, ids, tokens}
```

## Verification

`cargo test` — unit tests per module plus `tests/integration.rs`:
native/JSON-LD/OWL round-trip equality, merge/replace semantics, cascade
behaviour, budget compliance, a full MCP session, and the
generate→parse→merge chain (including a repaired malformed payload).
`cargo clippy --all-targets` is clean.
