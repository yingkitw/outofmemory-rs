//! `outofmemory` CLI — seed, export, import, validate, pack context, and run
//! the MCP tools over stdio. State lives in a single native-JSON file
//! (`--state`, default `ontology.json`).

use std::io::{BufRead, Write};

use outofmemory::context::{self, ContextRequest};
use outofmemory::interchange::{self, Format};
use outofmemory::mcp;
use outofmemory::seed;
use outofmemory::store::{ImportMode, Store};

const USAGE: &str = "outofmemory — context engineering on an ontology

USAGE:
  outofmemory seed                          write the seeded ontology to --state
  outofmemory export   [--format F]         print the store (json|jsonld|turtle|owl|csv)
  outofmemory import   <file> --format F    import a document (--mode merge|replace)
  outofmemory validate <file>               parse + referential check, print summary
  outofmemory context  --query Q [--budget N]  print the packed context Markdown
  outofmemory mcp                           JSON-RPC over stdio (one message per line)

OPTIONS:
  --state FILE   state file (default: ontology.json)
  --format F     export/import format (default: outofmemory)
  --mode M       merge | replace (default: merge)
  --budget N     context token budget (default: 2048)
  --hops N       neighborhood expansion hops (default: 1)
";

struct Args {
    cmd: String,
    positional: Vec<String>,
    state: String,
    format: String,
    mode: String,
    query: String,
    budget: usize,
    hops: usize,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        cmd: String::new(),
        positional: vec![],
        state: "ontology.json".into(),
        format: "outofmemory".into(),
        mode: "merge".into(),
        query: String::new(),
        budget: 2048,
        hops: 1,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Err(String::new()),
            "--state" => a.state = it.next().ok_or("--state needs a value")?,
            "--format" => a.format = it.next().ok_or("--format needs a value")?,
            "--mode" => a.mode = it.next().ok_or("--mode needs a value")?,
            "--query" | "-q" => a.query = it.next().ok_or("--query needs a value")?,
            "--budget" => {
                a.budget = it
                    .next()
                    .ok_or("--budget needs a value")?
                    .parse()
                    .map_err(|_| "--budget must be a number")?
            }
            "--hops" => {
                a.hops = it
                    .next()
                    .ok_or("--hops needs a value")?
                    .parse()
                    .map_err(|_| "--hops must be a number")?
            }
            other if a.cmd.is_empty() => a.cmd = other.into(),
            other => a.positional.push(other.into()),
        }
    }
    if a.cmd.is_empty() {
        return Err(String::new());
    }
    Ok(a)
}

fn load(path: &str) -> Result<Store, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let parsed = interchange::parse_native(&text).map_err(|e| e.to_string())?;
    Ok(Store::from_state(parsed.state))
}

fn save(path: &str, store: &Store) -> Result<(), String> {
    std::fs::write(path, interchange::to_native_json(store))
        .map_err(|e| format!("cannot write {path}: {e}"))
}

fn run() -> Result<i32, String> {
    let a = parse_args()?;
    match a.cmd.as_str() {
        "seed" => {
            save(&a.state, &seed::supply_chain())?;
            eprintln!("seeded supply-chain ontology → {}", a.state);
        }
        "export" => {
            let store = load(&a.state)?;
            let format = Format::parse(&a.format)
                .ok_or_else(|| format!("unknown format \"{}\"", a.format))?;
            print!("{}", interchange::export(&store, format));
        }
        "import" => {
            let file = a.positional.first().ok_or("import needs a file")?;
            let text =
                std::fs::read_to_string(file).map_err(|e| format!("cannot read {file}: {e}"))?;
            let format = Format::parse(&a.format)
                .ok_or_else(|| format!("unknown format \"{}\"", a.format))?;
            let mode = match a.mode.as_str() {
                "merge" => ImportMode::Merge,
                "replace" => ImportMode::Replace,
                m => return Err(format!("unknown mode \"{m}\"")),
            };
            let mut store = if std::path::Path::new(&a.state).exists() {
                load(&a.state)?
            } else {
                Store::new()
            };
            let summary =
                interchange::import(&mut store, format, &text, mode).map_err(|e| e.to_string())?;
            save(&a.state, &store)?;
            eprintln!(
                "{}: +{} types, +{} link types, +{} action types, +{} objects, +{} links → {}",
                a.mode,
                summary.object_types,
                summary.link_types,
                summary.action_types,
                summary.objects,
                summary.object_links,
                a.state
            );
        }
        "validate" => {
            let file = a.positional.first().ok_or("validate needs a file")?;
            let text =
                std::fs::read_to_string(file).map_err(|e| format!("cannot read {file}: {e}"))?;
            let parsed = if a.format == "jsonld" {
                let doc: serde_json::Value =
                    serde_json::from_str(&text).map_err(|e| format!("invalid JSON: {e}"))?;
                interchange::parse_json_ld(&doc)
            } else if a.format == "owl" {
                interchange::parse_owl_rdf(&text).map_err(|e| e.to_string())?
            } else {
                interchange::parse_native(&text).map_err(|e| e.to_string())?
            };
            let store = Store::from_state(parsed.state);
            let s = &store.state;
            let mut problems = store.problems();
            for w in &parsed.warnings {
                problems.push(format!("warning: {w}"));
            }
            println!(
                "{} object types · {} link types · {} action types · {} objects · {} links",
                s.object_types.len(),
                s.link_types.len(),
                s.action_types.len(),
                s.objects.len(),
                s.object_links.len()
            );
            if problems.is_empty() {
                println!("ok — no problems found");
            } else {
                for p in &problems {
                    println!("✗ {p}");
                }
                return Ok(1);
            }
        }
        "context" => {
            let store = load(&a.state)?;
            let packed = context::pack(
                &store,
                &ContextRequest {
                    query: a.query,
                    token_budget: a.budget,
                    neighborhood_hops: a.hops,
                    ..Default::default()
                },
            );
            print!("{}", packed.markdown);
            eprintln!(
                "\n— ~{} tokens · {} seeds · {} neighbors · {} dropped · {} links",
                packed.estimated_tokens,
                packed.seeds,
                packed.neighbors,
                packed.dropped,
                packed.link_ids.len()
            );
        }
        "mcp" => {
            // empty state if the file does not exist yet — agents may create
            // the ontology from scratch through the tools
            let mut store = if std::path::Path::new(&a.state).exists() {
                load(&a.state)?
            } else {
                Store::new()
            };
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            for line in stdin.lock().lines() {
                let line = line.map_err(|e| e.to_string())?;
                if line.trim().is_empty() {
                    continue;
                }
                if let Some(resp) = mcp::handle_rpc(&mut store, &line) {
                    let mut out = stdout.lock();
                    writeln!(out, "{resp}").map_err(|e| e.to_string())?;
                    out.flush().map_err(|e| e.to_string())?;
                }
            }
            // persist whatever the agent authored
            if !store.state.object_types.is_empty() {
                save(&a.state, &store)?;
            }
        }
        other => return Err(format!("unknown command \"{other}\"\n{USAGE}")),
    }
    Ok(0)
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            if e.is_empty() {
                print!("{USAGE}");
            } else {
                eprintln!("error: {e}");
                std::process::exit(2);
            }
        }
    }
}
