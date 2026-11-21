# Agent Development Loop — outofmemory

The continuous improvement cycle for this crate. The reference implementation
is the TypeScript workbench at `../outofmemory` — **when semantics are in
question, the TS code is the source of truth** (`src/types.ts`,
`server/app.ts`, `server/onto.ts`, `server/mcp.ts`, `server/ai.ts`).

## The Loop

### 1. Complete Remaining TODO Items
Pick the next item from `TODO.md`. Minimal, focused changes; no speculative
features.

### 2. Create Tests and Examples
- Unit tests inline (`#[cfg(test)] mod tests`) for core logic
- Integration tests in `tests/integration.rs` exercising features end-to-end
- A minimal `examples/` program for client-facing capabilities

### 3. Ensure `cargo test` Passes
```bash
cargo test                  # unit + integration + doctests
cargo clippy --all-targets  # warnings acceptable but noted
```
Fix failures before proceeding.

### 4. Loop Back
Return to `TODO.md`, pick the next item.

### 5. Audit and Optimize
- **Fidelity**: does the behaviour still match `../outofmemory`? Round-trip
  tests must keep passing; intentional diffs are recorded in `TODO.md` and
  `SPEC.md` under "Known divergences"/`[diff]`.
- **Leanness**: remove dead code, unused imports, speculative abstraction.
- **Footprint**: std first; a dep must do real work (quick-xml does; chrono
  does not — `util::now_iso` covers the one timestamp).
- **Wiring**: new capabilities reach `lib.rs` re-exports, the CLI where it
  makes sense, and the docs.

### 6. Competitive Intelligence
Compare against context-engineering/ontology tooling (LangChain context
helpers, LlamaIndex, Pion/OWL-rs, Protégé, Foundry itself). Seed the
brainstorming section of `TODO.md`; prioritize what gives agents a real edge.

### 7. Update Documentation
- **`README.md`** — quick start, feature table, CLI
- **`SPEC.md`** — the contract: model, errors, store semantics, formats, MCP, CLI
- **`TODO.md`** — completed → Done, keep brainstorming current
- **`ARCHITECTURE.md`** — module map, dependency directions, decisions
- **`AGENTS.md`** — this file

## Principles

- **The TS app is the spec.** Port semantics faithfully; document every
  deliberate improvement as a `[diff]` in SPEC/TODO.
- **Lossless interchange is a test, not a hope.** Any change to
  `interchange/` must keep the round-trip tests byte-equal.
- **Surgical changes** — touch only what you must.
- **Test before ship** — no feature is complete without passing tests.
- **Small footprint** — default feature set should stay usable without
  `anyrepair` (`cargo build --no-default-features` must keep working).

## State conventions

- Ids are UUIDv4 strings; seeds derive them from a PRNG so `seed::supply_chain()`
  is byte-identical across runs.
- `created_at`/`exported_at` are ISO-8601 UTC strings (`util::now_iso`).
- Property values are `serde_json::Value` keyed by property apiName.
- `apiName` is the cross-document identifier; merge matches by apiName/id.
