# TODO — outofmemory

## Done

- [x] Domain model ported from `src/types.ts` (serde-compatible field names)
- [x] `Store` with API validation, apiName uniqueness, endpoint checks, FK cascades
- [x] Import `merge`/`replace` with apiName→id resolution (case-insensitive fallback)
- [x] Interchange: native JSON, JSON-LD, Turtle, OWL/RDF-XML, instances CSV
- [x] Lossless round-trips (native→JSON-LD→native, native→OWL→native) — tested
- [x] Context engineering: schema/instance Markdown, query scoring, neighbourhood packing, token budget
- [x] MCP: 7 tools + stateless JSON-RPC handler, stdio transport in CLI
- [x] AI generation: prompt assembly + `parse_generated` with JSON repair (`anyrepair`)
- [x] Deterministic supply-chain seed
- [x] CLI: seed/export/import/validate/context/mcp
- [x] Integration tests + example; `cargo test`/`clippy` green
- [x] Turtle import (`parse_turtle`) — Turtle round-trip is lossless (export carries the full `onto:` annotation set)

## Next

- [ ] Property-level constraints (regex/enum/uniqueness) with instance validation — from the workbench roadmap
- [ ] Link cardinality enforcement at instance-link write time (e.g. reject a second `CURRENT_PORT` for a vessel)
- [ ] Undo/redo stack over `State` snapshots (`put_state` already backs it)
- [ ] `PackedContext` → citations: per-object source ids already tracked; add a machine-readable `context.json` sidecar option
- [ ] Query scoring: IDF weighting + property-name matches (currently title/property-value term hits)
- [ ] Optional `tiktoken`-style counter behind a feature flag; `estimate_tokens` stays the default
- [ ] HTTP transport example (`axum`/`tiny_http` sketch in `examples/`) so `handle_rpc` mounts the same way as TS `/api/mcp`
- [ ] `Store::diff` — ontology diffing for multi-environment comparison (workbench roadmap item)

## Brainstorming (competitive intelligence)

- **Ontology-aware compaction**: replace summarized objects with their type-level description when budget is tight, keeping links intact — a middle rung between "included" and "dropped"
- **Vector/hybrid retrieval**: pluggable `Scorer` trait so callers can rank objects with embeddings instead of term hits
- **Context windows over time**: `remember`/`recall` API treating `createdAt` as recency for temporal decay in packing
- **SHACL-lite validation**: shape constraints on instances (required props present, datatype match) — `validate` CLI could report them
- **SPARQL-ish query**: minimal pattern query (`?s CARRIED_BY ?vessel`) for agents to pull structured slices without full context

## Known divergences from the TS app

- `Store::merge` resolves apiName endpoints case-insensitively (TS leaves `CONTAINER`→`Container` dangling).
- `generate::up` uses `slugify` (handles spaces/camelCase) rather than plain `toUpperCase`.
- CSV rows omit undefined property columns like the TS (ragged by design).
- OWL instance-property literals coerce via i64-then-f64 so `8` round-trips as `8` (JS `Number` semantics).
