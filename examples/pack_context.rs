//! Pack a query-relevant slice of the seeded ontology into a token budget.
//!
//!     cargo run --example pack_context -- "delayed shipments via Hamburg" 1200

use outofmemory::context::{self, ContextRequest};
use outofmemory::seed;

fn main() {
    let mut args = std::env::args().skip(1);
    let query = args.next().unwrap_or_else(|| "delayed shipments".into());
    let budget: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(1200);

    let store = seed::supply_chain();
    let packed = context::pack(
        &store,
        &ContextRequest {
            query,
            token_budget: budget,
            ..Default::default()
        },
    );

    println!("{}", packed.markdown);
    eprintln!(
        "— ~{} tokens · {} seeds · {} neighbors · {} dropped · {} links",
        packed.estimated_tokens,
        packed.seeds,
        packed.neighbors,
        packed.dropped,
        packed.link_ids.len()
    );
}
