//! Turtle — the same model as `.ttl` triples.
//!
//! `to_turtle` ports `toTurtle` in `server/onto.ts`, extended with the same
//! `onto:` annotations JSON-LD and OWL carry so `turtle → native → turtle`
//! is lossless. `parse_turtle` is a hand-rolled Turtle-subset parser —
//! `@prefix`/`PREFIX` directives, `a`, `;`/`,` lists, `<iri>`/prefixed names,
//! literals, `[ … ]` blank nodes (action parameters) — that mirrors the
//! JSON-LD classification logic.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::Value;

use super::{ONTO_NS, Parsed, local};
use crate::error::{Error, Result};
use crate::model::*;
use crate::store::{State, Store};

fn ttl_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn ttl_value(v: &Value) -> String {
    match v {
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        other => {
            let s = other
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| other.to_string());
            format!("\"{}\"", ttl_escape(&s))
        }
    }
}

/// Arrays serialize as Turtle comma-lists (`onto:p a , b , c`).
fn ttl_value_list(v: &Value) -> String {
    match v {
        Value::Array(a) => a.iter().map(ttl_value).collect::<Vec<_>>().join(" , "),
        other => ttl_value(other),
    }
}

/// Export the whole store as a Turtle document.
pub fn to_turtle(store: &Store) -> String {
    let ontology = &store.state;
    let type_by_id = |id: &str| ontology.object_types.iter().find(|t| t.id == id);
    let mut lines: Vec<String> = vec![
        format!("@prefix onto: <{ONTO_NS}> ."),
        "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .".into(),
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .".into(),
        "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .".into(),
        String::new(),
    ];

    for t in &ontology.object_types {
        lines.extend([
            format!("onto:{} a rdfs:Class ;", t.api_name),
            format!("  rdfs:label \"{}\" ;", ttl_escape(&t.display_name)),
            format!("  onto:apiName \"{}\" ;", ttl_escape(&t.api_name)),
            format!("  onto:objectTypeId \"{}\" ;", t.id),
            format!(
                "  onto:pluralDisplayName \"{}\" ;",
                ttl_escape(&t.plural_display_name)
            ),
            format!("  onto:description \"{}\" ;", ttl_escape(&t.description)),
            format!("  onto:domain \"{}\" ;", ttl_escape(&t.domain)),
            format!("  onto:color \"{}\" .", ttl_escape(&t.color)),
            String::new(),
        ]);
        for p in &t.properties {
            lines.extend([
                format!("onto:{}.{} a rdf:Property ;", t.api_name, p.api_name),
                format!("  rdfs:label \"{}\" ;", ttl_escape(&p.display_name)),
                format!("  rdfs:domain onto:{} ;", t.api_name),
                format!("  rdfs:range {} ;", p.data_type.xsd()),
                format!("  onto:apiName \"{}\" ;", ttl_escape(&p.api_name)),
                format!("  onto:required {} ;", p.required),
                format!("  onto:array {} .", p.array),
                String::new(),
            ]);
        }
    }

    for l in &ontology.link_types {
        let src = type_by_id(&l.source_type_id);
        let tgt = type_by_id(&l.target_type_id);
        lines.extend([
            format!("onto:{} a rdf:Property ;", l.api_name),
            format!("  rdfs:label \"{}\" ;", ttl_escape(&l.display_name)),
            format!(
                "  rdfs:domain onto:{} ;",
                src.map(|t| t.api_name.as_str()).unwrap_or("?")
            ),
            format!(
                "  rdfs:range onto:{} ;",
                tgt.map(|t| t.api_name.as_str()).unwrap_or("?")
            ),
            format!("  onto:apiName \"{}\" ;", ttl_escape(&l.api_name)),
            format!("  onto:objectTypeId \"{}\" ;", l.id),
            format!("  onto:description \"{}\" ;", ttl_escape(&l.description)),
            "  onto:linkType true ;".into(),
            format!(
                "  onto:sourceCardinality \"{}\" ;",
                l.source_cardinality.as_str()
            ),
            format!(
                "  onto:targetCardinality \"{}\" .",
                l.target_cardinality.as_str()
            ),
            String::new(),
        ]);
    }

    for a in &ontology.action_types {
        let on = type_by_id(&a.object_type_id);
        lines.extend([
            format!("onto:{} a onto:Action ;", a.api_name),
            format!("  rdfs:label \"{}\" ;", ttl_escape(&a.display_name)),
            format!("  onto:apiName \"{}\" ;", ttl_escape(&a.api_name)),
            format!("  onto:objectTypeId \"{}\" ;", a.id),
            format!("  onto:description \"{}\" ;", ttl_escape(&a.description)),
            format!(
                "  onto:objectType onto:{}{}",
                on.map(|t| t.api_name.as_str()).unwrap_or("?"),
                if a.parameters.is_empty() { " ." } else { " ;" }
            ),
        ]);
        if a.parameters.is_empty() {
            lines.push(String::new());
        } else {
            // blank-node parameter specs — `onto:parameter [ … ] , [ … ]`
            let params = a
                .parameters
                .iter()
                .map(|p| {
                    format!(
                        "[ onto:apiName \"{}\" ; rdfs:label \"{}\" ; rdfs:range {} ; onto:required {} ]",
                        ttl_escape(&p.api_name),
                        ttl_escape(&p.display_name),
                        p.data_type.xsd(),
                        p.required
                    )
                })
                .collect::<Vec<_>>()
                .join(" ,\n    ");
            lines.push(format!("  onto:parameter {params} ."));
            lines.push(String::new());
        }
    }

    for o in &store.state.objects {
        let t = type_by_id(&o.type_id);
        let props = o
            .properties
            .iter()
            .map(|(k, v)| format!("  onto:{k} {}", ttl_value_list(v)))
            .collect::<Vec<_>>()
            .join(" ;\n");
        lines.push(format!(
            "onto:obj_{} a onto:{} ;\n  rdfs:label \"{}\" ;\n  onto:objectId \"{}\" ;\n  onto:createdAt \"{}\"{} .",
            o.id,
            t.map(|t| t.api_name.as_str()).unwrap_or("?"),
            ttl_escape(&o.title),
            o.id,
            o.created_at,
            if props.is_empty() {
                String::new()
            } else {
                format!(" ;\n{props}")
            },
        ));
        lines.push(String::new());
    }

    let link_by_id = |id: &str| ontology.link_types.iter().find(|l| l.id == id);
    for l in &store.state.object_links {
        let lt = link_by_id(&l.link_type_id);
        lines.extend([
            format!(
                "onto:lnk_{} a onto:{} ;",
                l.id,
                lt.map(|x| x.api_name.as_str()).unwrap_or("?")
            ),
            format!("  onto:linkId \"{}\" ;", l.id),
            format!("  onto:source onto:obj_{} ;", l.source_object_id),
            format!("  onto:target onto:obj_{} .", l.target_object_id),
            String::new(),
        ]);
    }

    lines.join("\n")
}

// ---------- import ----------

/// RDF term: IRI/prefixed name, literal, or `[ pred obj ; … ]` blank node.
#[derive(Debug, Clone, PartialEq)]
enum Term {
    Iri(String),
    Lit(Value),
    Blank(BTreeMap<String, Vec<Term>>),
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Semi,
    Comma,
    Dot,
    LB,
    RB,
    Iri(String),  // `<…>` contents
    Name(String), // `prefix:local` or a bare word (`a`, `true`, `PREFIX`)
    Str(String),
    Num(String),
    At(String), // `@prefix`, `@base`
}

fn is_local_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn tokenize(text: &str) -> Result<Vec<Tok>> {
    let cs: Vec<char> = text.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '#' {
            while i < cs.len() && cs[i] != '\n' {
                i += 1;
            }
            continue;
        }
        match c {
            ';' => {
                toks.push(Tok::Semi);
                i += 1;
            }
            ',' => {
                toks.push(Tok::Comma);
                i += 1;
            }
            '.' => {
                toks.push(Tok::Dot);
                i += 1;
            }
            '[' => {
                toks.push(Tok::LB);
                i += 1;
            }
            ']' => {
                toks.push(Tok::RB);
                i += 1;
            }
            '<' => {
                let start = i + 1;
                let mut j = start;
                while j < cs.len() && cs[j] != '>' {
                    j += 1;
                }
                if j >= cs.len() {
                    return Err(Error::bad("unterminated <iri>"));
                }
                toks.push(Tok::Iri(cs[start..j].iter().collect()));
                i = j + 1;
            }
            '"' => {
                let mut s = String::new();
                i += 1;
                loop {
                    if i >= cs.len() {
                        return Err(Error::bad("unterminated string literal"));
                    }
                    match cs[i] {
                        '"' => {
                            i += 1;
                            break;
                        }
                        '\\' => {
                            i += 1;
                            let e = cs.get(i).copied().unwrap_or('\\');
                            s.push(match e {
                                'n' => '\n',
                                't' => '\t',
                                'r' => '\r',
                                other => other, // \" \\ and anything else
                            });
                            i += 1;
                        }
                        c => {
                            s.push(c);
                            i += 1;
                        }
                    }
                }
                toks.push(Tok::Str(s));
                // fold `^^xsd:string` / `@en` tags — the value is what matters
                if cs.get(i) == Some(&'^') && cs.get(i + 1) == Some(&'^') {
                    i += 2;
                    while i < cs.len() && cs[i].is_whitespace() {
                        i += 1;
                    }
                    if cs.get(i) == Some(&'<') {
                        while i < cs.len() && cs[i] != '>' {
                            i += 1;
                        }
                        i += 1;
                    } else {
                        while i < cs.len() && (is_local_char(cs[i]) || cs[i] == ':') {
                            i += 1;
                        }
                    }
                } else if cs.get(i) == Some(&'@')
                    && cs.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic())
                {
                    i += 1;
                    while i < cs.len() && (cs[i].is_ascii_alphanumeric() || cs[i] == '-') {
                        i += 1;
                    }
                }
            }
            '@' => {
                let start = i + 1;
                let mut j = start;
                while j < cs.len() && cs[j].is_ascii_alphabetic() {
                    j += 1;
                }
                toks.push(Tok::At(cs[start..j].iter().collect()));
                i = j;
            }
            '+' | '-' | '0'..='9' => {
                let start = i;
                if c == '+' || c == '-' {
                    i += 1;
                }
                while i < cs.len()
                    && (cs[i].is_ascii_digit() || matches!(cs[i], '.' | 'e' | 'E' | '+' | '-'))
                {
                    // a `.` followed by a non-digit ends the number (`x.`)
                    if cs[i] == '.' && !cs.get(i + 1).is_some_and(|n| n.is_ascii_digit()) {
                        break;
                    }
                    if matches!(cs[i], '+' | '-') && !matches!(cs[i - 1], 'e' | 'E') {
                        break;
                    }
                    i += 1;
                }
                toks.push(Tok::Num(cs[start..i].iter().collect()));
            }
            _ if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < cs.len() && is_local_char(cs[i]) {
                    i += 1;
                }
                if cs.get(i) == Some(&':') {
                    // prefixed name — local part may contain dots (never trailing)
                    i += 1;
                    while i < cs.len()
                        && (is_local_char(cs[i])
                            || (cs[i] == '.' && cs.get(i + 1).copied().is_some_and(is_local_char)))
                    {
                        i += 1;
                    }
                }
                toks.push(Tok::Name(cs[start..i].iter().collect()));
            }
            other => return Err(Error::bad(format!("unexpected character '{other}'"))),
        }
    }
    Ok(toks)
}

const BUILTIN_PREFIXES: [&str; 5] = ["rdf", "rdfs", "xsd", "owl", "onto"];

struct TParser<'a> {
    toks: &'a [Tok],
    i: usize,
    prefixes: HashMap<String, String>,
}

impl<'a> TParser<'a> {
    fn peek(&self) -> Option<&'a Tok> {
        self.toks.get(self.i)
    }

    fn next(&mut self) -> Option<&'a Tok> {
        let t = self.toks.get(self.i);
        if t.is_some() {
            self.i += 1;
        }
        t
    }

    fn bad<T>(&self, msg: &str) -> Result<T> {
        Err(Error::bad(format!("turtle: {msg}")))
    }

    /// `p:l` → expanded IRI via the declared prefix table; builtin prefixes
    /// stay prefixed so `onto:Action` and friends classify like JSON-LD.
    fn resolve(&self, name: &str) -> String {
        if let Some((p, l)) = name.split_once(':') {
            if BUILTIN_PREFIXES.contains(&p) {
                return name.to_string();
            }
            if let Some(base) = self.prefixes.get(p) {
                return format!("{base}{l}");
            }
        }
        name.to_string()
    }

    fn term(&mut self) -> Result<Term> {
        match self.next() {
            Some(Tok::Iri(i)) => Ok(Term::Iri(i.clone())),
            Some(Tok::Name(n)) if n == "true" || n == "false" => {
                Ok(Term::Lit(Value::Bool(n == "true")))
            }
            Some(Tok::Name(n)) => Ok(Term::Iri(self.resolve(&n.clone()))),
            Some(Tok::Str(s)) => Ok(Term::Lit(Value::String(s.clone()))),
            Some(Tok::Num(n)) => Ok(Term::Lit(number_literal(n))),
            Some(Tok::LB) => {
                let mut map: BTreeMap<String, Vec<Term>> = BTreeMap::new();
                self.pred_obj_list(&mut |pred, obj| {
                    map.entry(pred).or_default().push(obj);
                })?;
                if self.next() != Some(&Tok::RB) {
                    return self.bad("expected ']' to close blank node");
                }
                Ok(Term::Blank(map))
            }
            _ => self.bad("expected term"),
        }
    }

    /// `pred obj , obj ; pred obj` — stops before `;`/`]`/`.`/EOF.
    fn pred_obj_list(&mut self, emit: &mut dyn FnMut(String, Term)) -> Result<()> {
        loop {
            let pred = match self.next() {
                Some(Tok::Name(n)) if n == "a" => "rdf:type".to_string(),
                Some(Tok::Iri(i)) => i.clone(),
                Some(Tok::Name(n)) => self.resolve(&n.clone()),
                _ => return self.bad("expected predicate"),
            };
            loop {
                let obj = self.term()?;
                emit(pred.clone(), obj);
                if self.peek() == Some(&Tok::Comma) {
                    self.i += 1;
                    continue;
                }
                break;
            }
            match self.peek() {
                Some(Tok::Semi) => {
                    self.i += 1;
                    // trailing `;` directly before `.`/`]` is legal
                    if matches!(self.peek(), Some(Tok::Dot) | Some(Tok::RB) | None) {
                        break;
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }
}

/// i64 before f64 — `8` must round-trip as `8`, not `8.0` ([[literal-coercion]]).
fn number_literal(raw: &str) -> Value {
    raw.parse::<i64>()
        .map(Value::from)
        .or_else(|_| raw.parse::<f64>().map(Value::from))
        .unwrap_or_else(|_| Value::String(raw.to_string()))
}

/// Parse a Turtle document into the native shape. Handles the full output of
/// [`to_turtle`] losslessly plus common foreign Turtle (`@prefix`, `;`/`,`
/// lists, `a`, literals, blank nodes).
pub fn parse_turtle(text: &str) -> Result<Parsed> {
    let toks = tokenize(text)?;
    let mut p = TParser {
        toks: &toks,
        i: 0,
        prefixes: HashMap::new(),
    };
    // subject → predicate → objects, in document order
    let mut order: Vec<String> = vec![];
    let mut subjs: HashMap<String, BTreeMap<String, Vec<Term>>> = HashMap::new();

    while p.peek().is_some() {
        match p.peek() {
            Some(Tok::At(_)) => {
                let Tok::At(dir) = p.next().unwrap().clone() else {
                    unreachable!()
                };
                match dir.as_str() {
                    "prefix" => {
                        let Some(Tok::Name(pfx)) = p.next() else {
                            return p.bad("@prefix expects `p: <iri>`");
                        };
                        let Some(Tok::Iri(iri)) = p.next() else {
                            return p.bad("@prefix expects `p: <iri>`");
                        };
                        p.prefixes
                            .insert(pfx.trim_end_matches(':').to_string(), iri.clone());
                    }
                    "base" => {
                        if !matches!(p.next(), Some(Tok::Iri(_))) {
                            return p.bad("@base expects an <iri>");
                        }
                    }
                    other => return p.bad(&format!("unsupported directive @{other}")),
                }
                if p.peek() == Some(&Tok::Dot) {
                    p.i += 1;
                }
            }
            Some(Tok::Name(n))
                if n.eq_ignore_ascii_case("prefix") || n.eq_ignore_ascii_case("base") =>
            {
                let is_prefix = n.eq_ignore_ascii_case("prefix");
                p.i += 1;
                if is_prefix {
                    let Some(Tok::Name(pfx)) = p.next() else {
                        return p.bad("PREFIX expects `p: <iri>`");
                    };
                    let Some(Tok::Iri(iri)) = p.next() else {
                        return p.bad("PREFIX expects `p: <iri>`");
                    };
                    p.prefixes
                        .insert(pfx.trim_end_matches(':').to_string(), iri.clone());
                } else if !matches!(p.next(), Some(Tok::Iri(_))) {
                    return p.bad("BASE expects an <iri>");
                }
            }
            _ => {
                let subj = match p.next() {
                    Some(Tok::Iri(i)) => i.clone(),
                    Some(Tok::Name(n)) => p.resolve(&n.clone()),
                    _ => return p.bad("expected subject"),
                };
                if !order.contains(&subj) {
                    order.push(subj.clone());
                }
                let entry = subjs.entry(subj).or_default();
                p.pred_obj_list(&mut |pred, obj| {
                    entry.entry(pred).or_default().push(obj);
                })?;
                if p.next() != Some(&Tok::Dot) {
                    return p.bad("expected '.' to end statement");
                }
            }
        }
    }

    // ---- classify & build, mirroring parse_json_ld ----
    let mut warnings: Vec<String> = Vec::new();
    let empty: BTreeMap<String, Vec<Term>> = BTreeMap::new();
    let props_of = |s: &str| -> &BTreeMap<String, Vec<Term>> { subjs.get(s).unwrap_or(&empty) };

    fn first<'m>(props: &'m BTreeMap<String, Vec<Term>>, pred: &str) -> Option<&'m Term> {
        props.get(pred).and_then(|v| v.first())
    }
    fn strip_onto(s: &str) -> &str {
        s.strip_prefix("onto:").unwrap_or(s)
    }
    /// `@id` / IRI object → local resource name (`onto:X` and `…/onto/X` → `X`).
    fn name_ref(t: Option<&Term>) -> String {
        match t {
            Some(Term::Iri(i)) => strip_onto(local(i)).to_string(),
            _ => String::new(),
        }
    }
    /// `"…"` literal → string (numbers stringify).
    fn str_lit(t: Option<&Term>) -> String {
        match t {
            Some(Term::Lit(Value::String(s))) => s.clone(),
            Some(Term::Lit(v)) => v.to_string(),
            Some(Term::Iri(i)) => strip_onto(local(i)).to_string(),
            _ => String::new(),
        }
    }
    fn bool_lit(t: Option<&Term>) -> bool {
        matches!(t, Some(Term::Lit(Value::Bool(true))))
    }
    /// `rdf:type` object — keep `onto:` terms (`onto:Action`) intact.
    fn type_ref(t: Option<&Term>) -> String {
        match t {
            Some(Term::Iri(i)) => local(i).to_string(),
            _ => String::new(),
        }
    }

    let mut class_ids: HashSet<String> = HashSet::new();
    let mut property_nodes: Vec<&str> = vec![];
    let mut link_nodes: Vec<&str> = vec![];
    let mut action_nodes: Vec<&str> = vec![];
    let mut instance_nodes: Vec<&str> = vec![];

    for s in &order {
        let t = type_ref(first(props_of(s), "rdf:type"));
        if t == "rdfs:Class" {
            let api = str_lit(first(props_of(s), "onto:apiName"));
            class_ids.insert(if api.is_empty() {
                name_ref(Some(&Term::Iri(s.clone())))
            } else {
                api
            });
        } else if t == "rdf:Property" {
            property_nodes.push(s);
        } else if t == "onto:Action" || t == "Action" {
            action_nodes.push(s);
        } else {
            instance_nodes.push(s);
        }
    }

    for s in &property_nodes {
        let props = props_of(s);
        let domain = name_ref(first(props, "rdfs:domain"));
        let range = name_ref(first(props, "rdfs:range"));
        if !domain.is_empty() && class_ids.contains(&domain) && range.starts_with("xsd:") {
            // data property — stays with its class
        } else if !domain.is_empty() && !range.is_empty() {
            link_nodes.push(s);
        } else {
            warnings.push(format!("Property {s} has no usable domain/range — skipped"));
        }
    }

    let uuid = || uuid::Uuid::new_v4().to_string();

    let is_data_prop_of = |s: &str, api: &str| -> bool {
        let props = props_of(s);
        name_ref(first(props, "rdfs:domain")) == api
            && name_ref(first(props, "rdfs:range")).starts_with("xsd:")
    };

    let mut object_types: Vec<ObjectType> = vec![];
    for s in &order {
        let props = props_of(s);
        if type_ref(first(props, "rdf:type")) != "rdfs:Class" {
            continue;
        }
        let api = {
            let a = str_lit(first(props, "onto:apiName"));
            if a.is_empty() {
                name_ref(Some(&Term::Iri(s.clone())))
            } else {
                a
            }
        };
        let properties = property_nodes
            .iter()
            .filter(|pn| is_data_prop_of(pn, &api))
            .map(|pn| {
                let pp = props_of(pn);
                PropertyDef {
                    api_name: {
                        let a = str_lit(first(pp, "onto:apiName"));
                        if !a.is_empty() {
                            a
                        } else {
                            name_ref(Some(&Term::Iri((*pn).to_string())))
                                .rsplit('.')
                                .next()
                                .unwrap_or("property")
                                .to_string()
                        }
                    },
                    display_name: {
                        let d = str_lit(first(pp, "rdfs:label"));
                        if d.is_empty() {
                            str_lit(first(pp, "onto:apiName"))
                        } else {
                            d
                        }
                    },
                    data_type: DataType::from_xsd(&name_ref(first(pp, "rdfs:range")))
                        .unwrap_or(DataType::String),
                    required: bool_lit(first(pp, "onto:required")),
                    array: bool_lit(first(pp, "onto:array")),
                }
            })
            .collect();
        object_types.push(ObjectType {
            id: {
                let s2 = str_lit(first(props, "onto:objectTypeId"));
                if s2.is_empty() { uuid() } else { s2 }
            },
            api_name: api.clone(),
            display_name: {
                let d = str_lit(first(props, "rdfs:label"));
                if d.is_empty() { api.clone() } else { d }
            },
            plural_display_name: {
                let d = str_lit(first(props, "onto:pluralDisplayName"));
                if d.is_empty() { format!("{api}s") } else { d }
            },
            description: str_lit(first(props, "onto:description")),
            domain: {
                let d = str_lit(first(props, "onto:domain"));
                if d.is_empty() { "General".into() } else { d }
            },
            color: {
                let d = str_lit(first(props, "onto:color"));
                if d.is_empty() { "#f0b35c".into() } else { d }
            },
            properties,
        });
    }

    let mut link_types: Vec<LinkType> = link_nodes
        .iter()
        .map(|s| {
            let props = props_of(s);
            let api = {
                let a = str_lit(first(props, "onto:apiName"));
                if a.is_empty() {
                    name_ref(Some(&Term::Iri((*s).to_string())))
                } else {
                    a
                }
            };
            LinkType {
                id: {
                    let s2 = str_lit(first(props, "onto:objectTypeId"));
                    if s2.is_empty() { uuid() } else { s2 }
                },
                api_name: api.clone(),
                display_name: {
                    let d = str_lit(first(props, "rdfs:label"));
                    if d.is_empty() { api.clone() } else { d }
                },
                description: str_lit(first(props, "onto:description")),
                source_type_id: String::new(),
                target_type_id: String::new(),
                source_cardinality: if str_lit(first(props, "onto:sourceCardinality")) == "ONE" {
                    Cardinality::One
                } else {
                    Cardinality::Many
                },
                target_cardinality: if str_lit(first(props, "onto:targetCardinality")) == "MANY" {
                    Cardinality::Many
                } else {
                    Cardinality::One
                },
            }
        })
        .collect();

    let mut action_types: Vec<ActionType> = action_nodes
        .iter()
        .map(|s| {
            let props = props_of(s);
            let api = {
                let a = str_lit(first(props, "onto:apiName"));
                if a.is_empty() {
                    name_ref(Some(&Term::Iri((*s).to_string())))
                } else {
                    a
                }
            };
            let parameters = props
                .get("onto:parameter")
                .or_else(|| props.get("onto:parameters"))
                .map(|ps| {
                    ps.iter()
                        .filter_map(|t| match t {
                            Term::Blank(b) => Some(PropertyDef {
                                api_name: str_lit(b.get("onto:apiName").and_then(|v| v.first())),
                                display_name: {
                                    let d = str_lit(b.get("rdfs:label").and_then(|v| v.first()));
                                    if d.is_empty() {
                                        str_lit(b.get("onto:apiName").and_then(|v| v.first()))
                                    } else {
                                        d
                                    }
                                },
                                data_type: DataType::from_xsd(&name_ref(
                                    b.get("rdfs:range").and_then(|v| v.first()),
                                ))
                                .unwrap_or(DataType::String),
                                required: bool_lit(b.get("onto:required").and_then(|v| v.first())),
                                array: false,
                            }),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            ActionType {
                id: {
                    let s2 = str_lit(first(props, "onto:objectTypeId"));
                    if s2.is_empty() { uuid() } else { s2 }
                },
                api_name: api.clone(),
                display_name: {
                    let d = str_lit(first(props, "rdfs:label"));
                    if d.is_empty() { api.clone() } else { d }
                },
                description: str_lit(first(props, "onto:description")),
                object_type_id: String::new(),
                parameters,
            }
        })
        .collect();

    let class_id_by_api = |api: &str| {
        object_types
            .iter()
            .find(|t| t.api_name == api)
            .map(|t| t.id.clone())
    };
    for (s, lt) in link_nodes.iter().zip(link_types.iter_mut()) {
        let props = props_of(s);
        lt.source_type_id =
            class_id_by_api(&name_ref(first(props, "rdfs:domain"))).unwrap_or_default();
        lt.target_type_id =
            class_id_by_api(&name_ref(first(props, "rdfs:range"))).unwrap_or_default();
    }
    for (s, at) in action_nodes.iter().zip(action_types.iter_mut()) {
        let props = props_of(s);
        at.object_type_id =
            class_id_by_api(&name_ref(first(props, "onto:objectType"))).unwrap_or_default();
    }

    let known_class: HashSet<&str> = object_types.iter().map(|t| t.api_name.as_str()).collect();
    let known_link: HashSet<&str> = link_types.iter().map(|l| l.api_name.as_str()).collect();
    let mut objects: Vec<Obj> = vec![];
    let mut object_links: Vec<ObjectLink> = vec![];

    const RESERVED_ONTO: [&str; 20] = [
        "apiName",
        "objectTypeId",
        "objectId",
        "linkId",
        "createdAt",
        "domain",
        "color",
        "dataType",
        "required",
        "array",
        "linkType",
        "sourceCardinality",
        "targetCardinality",
        "pluralDisplayName",
        "description",
        "objectType",
        "parameter",
        "parameters",
        "source",
        "target",
    ];

    let term_value = |t: &Term| -> Value {
        match t {
            Term::Lit(v) => {
                // `{"a":1}`-shaped strings are object values we emitted as JSON
                if let Value::String(s) = v
                    && s.starts_with('{')
                    && let Ok(parsed) = serde_json::from_str::<Value>(s)
                {
                    return parsed;
                }
                v.clone()
            }
            Term::Iri(i) => Value::String(strip_onto(local(i)).to_string()),
            Term::Blank(_) => Value::Null,
        }
    };

    for s in &instance_nodes {
        let props = props_of(s);
        let t = strip_onto(&type_ref(first(props, "rdf:type"))).to_string();
        if known_class.contains(t.as_str()) {
            let api = t;
            let ty = object_types.iter().find(|x| x.api_name == api);
            let mut obj_props: BTreeMap<String, Value> = BTreeMap::new();
            for (pred, terms) in props {
                let p = local(pred);
                if p.starts_with("rdf:") || p.starts_with("rdfs:") {
                    continue;
                }
                let key = p.strip_prefix("onto:").unwrap_or(p);
                if p.starts_with("onto:") && RESERVED_ONTO.contains(&key) {
                    continue;
                }
                let mut v = if terms.len() > 1 {
                    Value::Array(terms.iter().map(&term_value).collect())
                } else {
                    term_value(&terms[0])
                };
                // array-valued defs with a single item still read back as arrays
                if !v.is_array()
                    && ty.is_some_and(|ty| {
                        ty.properties
                            .iter()
                            .any(|pd| pd.api_name == key && pd.array)
                    })
                {
                    v = Value::Array(vec![v]);
                }
                obj_props.insert(key.to_string(), v);
            }
            objects.push(Obj {
                id: {
                    let s2 = str_lit(first(props, "onto:objectId"));
                    if !s2.is_empty() {
                        s2
                    } else {
                        strip_onto(local(s))
                            .strip_prefix("obj_")
                            .map(str::to_string)
                            .unwrap_or_else(uuid)
                    }
                },
                type_id: ty.map(|x| x.id.clone()).unwrap_or_default(),
                title: {
                    let d = str_lit(first(props, "rdfs:label"));
                    if d.is_empty() { "Untitled".into() } else { d }
                },
                properties: obj_props,
                created_at: {
                    let d = str_lit(first(props, "onto:createdAt"));
                    if d.is_empty() {
                        crate::util::now_iso()
                    } else {
                        d
                    }
                },
            });
        } else if known_link.contains(t.as_str()) {
            object_links.push(ObjectLink {
                id: {
                    let s2 = str_lit(first(props, "onto:linkId"));
                    if !s2.is_empty() {
                        s2
                    } else {
                        strip_onto(local(s))
                            .strip_prefix("lnk_")
                            .map(str::to_string)
                            .unwrap_or_else(uuid)
                    }
                },
                link_type_id: link_types
                    .iter()
                    .find(|l| l.api_name == t)
                    .map(|l| l.id.clone())
                    .unwrap_or_default(),
                source_object_id: name_ref(first(props, "onto:source"))
                    .trim_start_matches("obj_")
                    .to_string(),
                target_object_id: name_ref(first(props, "onto:target"))
                    .trim_start_matches("obj_")
                    .to_string(),
            });
        }
    }

    if object_types.is_empty() {
        warnings.push("No rdfs:Class found — nothing to import".into());
    }

    Ok(Parsed {
        state: State {
            object_types,
            link_types,
            action_types,
            objects,
            object_links,
            layout: vec![],
        },
        warnings,
    })
}
