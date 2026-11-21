//! Context engineering — the reason this crate exists.
//!
//! An LLM's context window is finite; an ontology is not. This module renders
//! the schema as prompt-ready Markdown and packs the most relevant instance
//! slice — query-matched seed objects plus their graph neighbourhood — under
//! a token budget. The result is a self-contained `Ontology ▸ instances ▸
//! links` document an agent can ground on without ever seeing the full store.
//!
//! ```no_run
//! use outofmemory::{seed, context::*};
//! let store = seed::supply_chain();
//! let packed = pack(&store, &ContextRequest {
//!     query: "delayed shipments through Hamburg".into(),
//!     token_budget: 1200,
//!     ..Default::default()
//! });
//! println!("{}", packed.markdown);
//! ```

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use serde_json::Value;

use crate::model::*;
use crate::store::Store;

/// Token estimation — the standard `chars / 4` heuristic, counted on
/// characters (not bytes, so `→` and CJK aren't overcharged). Accurate enough
/// for budget packing; swap in a real tokenizer upstream if precision matters.
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count() / 4 + 1
}

/// What to put in the window.
#[derive(Debug, Clone)]
pub struct ContextRequest {
    /// Free-text query used to rank candidate objects. Empty → treat every
    /// object as equally relevant (budget decides).
    pub query: String,
    /// Hard ceiling on estimated tokens for the whole rendered document.
    pub token_budget: usize,
    /// Render the schema section first. Strongly recommended — the schema is
    /// the vocabulary the model needs to interpret the instances.
    pub include_schema: bool,
    /// Top-k query-matched objects to seed the slice with.
    pub max_seed_objects: usize,
    /// Instance-link hops to expand around each seed (0 = seeds only).
    pub neighborhood_hops: usize,
}

impl Default for ContextRequest {
    fn default() -> Self {
        ContextRequest {
            query: String::new(),
            token_budget: 2048,
            include_schema: true,
            max_seed_objects: 8,
            neighborhood_hops: 1,
        }
    }
}

/// The packed window plus an accounting of what made it in.
#[derive(Debug, Clone, Default)]
pub struct PackedContext {
    /// Prompt-ready Markdown: `# Ontology` schema section, `## Instances`,
    /// `## Links` — in that order.
    pub markdown: String,
    /// Object ids that made it in (store order).
    pub object_ids: Vec<String>,
    /// Instance link ids rendered in the Links section.
    pub link_ids: Vec<String>,
    /// Estimated token count of `markdown`.
    pub estimated_tokens: usize,
    /// Seed objects included via query relevance.
    pub seeds: usize,
    /// Extra objects pulled in by neighbourhood expansion.
    pub neighbors: usize,
    /// Relevant objects that did not fit the budget.
    pub dropped: usize,
}

/// Render the schema as compact Markdown — the vocabulary section of a
/// context window.
pub fn render_schema_md(store: &Store) -> String {
    let s = &store.state;
    let mut out = String::new();
    out.push_str(&format!(
        "# Ontology\n{} object types · {} link types · {} action types · {} objects · {} links\n",
        s.object_types.len(),
        s.link_types.len(),
        s.action_types.len(),
        s.objects.len(),
        s.object_links.len(),
    ));

    if !s.object_types.is_empty() {
        out.push_str("\n## Object types\n");
        for t in &s.object_types {
            out.push_str(&format!(
                "\n### {} — {} ({})\n",
                t.api_name, t.plural_display_name, t.domain
            ));
            if !t.description.is_empty() {
                out.push_str(&t.description);
                out.push('\n');
            }
            if !t.properties.is_empty() {
                let props = t
                    .properties
                    .iter()
                    .map(|p| {
                        format!(
                            "{}: {}{}{}",
                            p.api_name,
                            p.data_type.as_str(),
                            if p.array { "[]" } else { "" },
                            if p.required { " *" } else { "" },
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!("- properties: {props}\n"));
            }
            let actions: Vec<&ActionType> = s
                .action_types
                .iter()
                .filter(|a| a.object_type_id == t.id)
                .collect();
            if !actions.is_empty() {
                let acts = actions
                    .iter()
                    .map(|a| {
                        let params = a
                            .parameters
                            .iter()
                            .map(|p| {
                                format!(
                                    "{}: {}{}",
                                    p.api_name,
                                    p.data_type.as_str(),
                                    if p.required { " *" } else { "" }
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("{}({})", a.api_name, params)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!("- actions: {acts}\n"));
            }
        }
    }

    if !s.link_types.is_empty() {
        out.push_str("\n## Link types\n");
        for l in &s.link_types {
            let src = store.object_type(&l.source_type_id);
            let tgt = store.object_type(&l.target_type_id);
            out.push_str(&format!(
                "- {} : {} —{}→ {}{}\n",
                l.api_name,
                src.map(|t| t.api_name.as_str()).unwrap_or("?"),
                l.cardinality_label(),
                tgt.map(|t| t.api_name.as_str()).unwrap_or("?"),
                if l.description.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", l.description)
                },
            ));
        }
    }
    out
}

fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(value_text).collect::<Vec<_>>().join(" "),
        other => other.to_string(),
    }
}

/// One object as a Markdown block: title, properties, and its direct links.
pub fn render_object_md(store: &Store, obj: &Obj) -> String {
    let t = store.object_type(&obj.type_id);
    let mut out = format!(
        "### {} ({})\n",
        obj.title,
        t.map(|t| t.api_name.as_str()).unwrap_or("?")
    );
    for (k, v) in &obj.properties {
        out.push_str(&format!("- {k}: {}\n", value_text(v)));
    }
    let mut link_lines: Vec<String> = vec![];
    for l in store.links_of_object(&obj.id) {
        let lt = store.link_type(&l.link_type_id);
        let Some(lt) = lt else { continue };
        let (other_id, arrow) = if l.source_object_id == obj.id {
            (l.target_object_id.as_str(), "→")
        } else {
            (l.source_object_id.as_str(), "←")
        };
        let other = store.object(other_id);
        let other_type = other.and_then(|o| store.object_type(&o.type_id));
        link_lines.push(format!(
            "- {} {} {} ({})",
            lt.api_name,
            arrow,
            other.map(|o| o.title.as_str()).unwrap_or("?"),
            other_type.map(|t| t.api_name.as_str()).unwrap_or("?"),
        ));
    }
    if !link_lines.is_empty() {
        out.push_str(&link_lines.join("\n"));
        out.push('\n');
    }
    out
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 2)
        .map(str::to_string)
        .collect()
}

/// Relevance score for one object: term hits in the title weigh double.
fn score_object(obj: &Obj, terms: &[String]) -> usize {
    if terms.is_empty() {
        return 1;
    }
    let title = obj.title.to_lowercase();
    let props = obj
        .properties
        .values()
        .map(value_text)
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let mut score = 0;
    for term in terms {
        if title.contains(term.as_str()) {
            score += 2;
        }
        if props.contains(term.as_str()) {
            score += 1;
        }
    }
    score
}

/// Rank all objects against a query — `(object, score)` sorted by score
/// descending, ties broken by title for determinism.
pub fn rank_objects<'a>(store: &'a Store, query: &str) -> Vec<(&'a Obj, usize)> {
    let terms = query_terms(query);
    let mut ranked: Vec<(&Obj, usize)> = store
        .state
        .objects
        .iter()
        .map(|o| (o, score_object(o, &terms)))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.title.cmp(&b.0.title)));
    ranked
}

/// Pack a query-relevant slice of the ontology under a token budget.
///
/// Order of spending: schema (the vocabulary) → top-k matched objects →
/// their neighbourhood (up to `hops` instance-link hops) → a Links section
/// listing every instance link between included objects.
pub fn pack(store: &Store, req: &ContextRequest) -> PackedContext {
    let mut packed = PackedContext::default();
    let mut body = String::new();

    if req.include_schema {
        let schema = render_schema_md(store);
        body.push_str(&schema);
        if estimate_tokens(&body) > req.token_budget {
            // even the schema overflows — truncate hard, still return a window.
            // len/4+1 is the estimator, so keep 4·budget−4 chars to land ≤ budget.
            let keep = req.token_budget.saturating_mul(4).saturating_sub(4);
            packed.markdown = body.chars().take(keep).collect();
            packed.estimated_tokens = estimate_tokens(&packed.markdown);
            return packed;
        }
    }

    let mut remaining = req.token_budget.saturating_sub(estimate_tokens(&body));

    // Adjacency for neighbourhood expansion: object → [(link, other)].
    let mut adj: HashMap<&str, Vec<(&ObjectLink, &str)>> = HashMap::new();
    for l in &store.state.object_links {
        adj.entry(l.source_object_id.as_str())
            .or_default()
            .push((l, l.target_object_id.as_str()));
        adj.entry(l.target_object_id.as_str())
            .or_default()
            .push((l, l.source_object_id.as_str()));
    }

    let mut included: HashSet<&str> = HashSet::new();
    let mut blocks: Vec<(usize, String)> = vec![]; // (score, markdown)

    let ranked = rank_objects(store, &req.query);

    // Pass 1 — seeds: top-k scored objects (score 0 objects only come in
    // when the query was empty).
    let mut seeds_taken = 0;
    for (obj, score) in &ranked {
        if seeds_taken >= req.max_seed_objects {
            break;
        }
        if *score == 0 && !req.query.trim().is_empty() {
            packed.dropped += 1;
            continue;
        }
        if try_include(
            store,
            obj,
            *score,
            &mut remaining,
            &mut included,
            &mut blocks,
        ) {
            seeds_taken += 1;
            packed.seeds += 1;
        } else {
            packed.dropped += 1;
        }
    }

    // Pass 2 — neighbourhood expansion (BFS from included seeds).
    if req.neighborhood_hops > 0 {
        let score_of: BTreeMap<&str, usize> =
            ranked.iter().map(|(o, s)| (o.id.as_str(), *s)).collect();
        let mut frontier: VecDeque<(String, usize)> =
            included.iter().map(|id| (id.to_string(), 0usize)).collect();
        while let Some((id, depth)) = frontier.pop_front() {
            if depth >= req.neighborhood_hops {
                continue;
            }
            let Some(edges) = adj.get(id.as_str()) else {
                continue;
            };
            // prefer higher-scoring neighbours first
            let mut edges = edges.clone();
            edges.sort_by_key(|(_, other)| {
                std::cmp::Reverse(score_of.get(*other).copied().unwrap_or(0))
            });
            for (_, other) in edges {
                if included.contains(other) {
                    continue;
                }
                let Some(obj) = store.object(other) else {
                    continue;
                };
                let s = score_of.get(other).copied().unwrap_or(0);
                if try_include(store, obj, s, &mut remaining, &mut included, &mut blocks) {
                    packed.neighbors += 1;
                    frontier.push_back((other.to_string(), depth + 1));
                } else {
                    packed.dropped += 1;
                }
            }
        }
    }

    // Render: included blocks in the order they were selected.
    if !blocks.is_empty() {
        body.push_str("\n## Instances\n\n");
        body.push_str(
            &blocks
                .iter()
                .map(|(_, b)| b.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    // Links between included objects — the connective tissue.
    let link_lines: Vec<String> = store
        .state
        .object_links
        .iter()
        .filter(|l| {
            included.contains(l.source_object_id.as_str())
                && included.contains(l.target_object_id.as_str())
        })
        .filter_map(|l| {
            let lt = store.link_type(&l.link_type_id)?;
            let src = store.object(&l.source_object_id)?;
            let tgt = store.object(&l.target_object_id)?;
            packed.link_ids.push(l.id.clone());
            Some(format!("- {} —{}→ {}", src.title, lt.api_name, tgt.title))
        })
        .collect();
    if !link_lines.is_empty() && estimate_tokens(&link_lines.join("\n")) <= remaining {
        body.push_str(&format!("\n## Links\n{}\n", link_lines.join("\n")));
    } else {
        packed.link_ids.clear();
    }

    packed.object_ids = store
        .state
        .objects
        .iter()
        .filter(|o| included.contains(o.id.as_str()))
        .map(|o| o.id.clone())
        .collect();
    packed.markdown = body;
    packed.estimated_tokens = estimate_tokens(&packed.markdown);
    packed
}

/// Include one object if its rendered block still fits the remaining budget.
fn try_include<'a>(
    store: &'a Store,
    obj: &'a Obj,
    score: usize,
    remaining: &mut usize,
    included: &mut HashSet<&'a str>,
    blocks: &mut Vec<(usize, String)>,
) -> bool {
    if included.contains(obj.id.as_str()) {
        return true;
    }
    let block = render_object_md(store, obj);
    let cost = estimate_tokens(&block) + 1; // + separator
    if cost > *remaining {
        return false;
    }
    *remaining -= cost;
    included.insert(obj.id.as_str());
    blocks.push((score, block));
    true
}
