//! In-memory ontology store with the same validation and cascade semantics as
//! the TS API (`server/app.ts` + `server/db.ts` FK rules):
//!
//! - `apiName` uniqueness → [`crate::error::ErrorKind::Conflict`]
//! - link endpoints must reference existing object types; instance links must
//!   match the link type's endpoint types; duplicate (linkType, source,
//!   target) → [`crate::error::ErrorKind::Conflict`]
//! - deleting an object type cascades to its link types, action types,
//!   instances, instance links and layout position; deleting a link type or an
//!   object cascades to the instance links that reference it
//!
//! Instances of this type are the "external memory" an agent queries through
//! [`crate::context`] or [`crate::mcp`].

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::*;

/// Whole-ontology snapshot — the shape `PUT /api/state` and the native export
/// use in the TS app. Also the undo/redo unit.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    #[serde(default)]
    pub object_types: Vec<ObjectType>,
    #[serde(default)]
    pub link_types: Vec<LinkType>,
    #[serde(default)]
    pub action_types: Vec<ActionType>,
    #[serde(default)]
    pub objects: Vec<Obj>,
    #[serde(default)]
    pub object_links: Vec<ObjectLink>,
    #[serde(default)]
    pub layout: Vec<LayoutEntry>,
}

/// Counts returned by `replace_all` / import — same summary shape as the API.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub object_types: usize,
    pub link_types: usize,
    pub action_types: usize,
    pub objects: usize,
    pub object_links: usize,
}

impl State {
    pub fn summary(&self) -> Summary {
        Summary {
            object_types: self.object_types.len(),
            link_types: self.link_types.len(),
            action_types: self.action_types.len(),
            objects: self.objects.len(),
            object_links: self.object_links.len(),
        }
    }
}

/// Import mode — `merge` keeps existing entities (matched by apiName/id) and
/// adds what is missing; `replace` swaps the whole ontology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    Merge,
    Replace,
}

#[derive(Debug, Clone, Default)]
pub struct Store {
    pub state: State,
}

impl Store {
    pub fn new() -> Self {
        Store::default()
    }

    pub fn from_state(state: State) -> Self {
        Store { state }
    }

    // ---------- lookups ----------

    pub fn object_type(&self, id: &str) -> Option<&ObjectType> {
        self.state.object_types.iter().find(|t| t.id == id)
    }
    pub fn object_type_by_api(&self, api_name: &str) -> Option<&ObjectType> {
        self.state
            .object_types
            .iter()
            .find(|t| t.api_name == api_name)
    }
    pub fn link_type(&self, id: &str) -> Option<&LinkType> {
        self.state.link_types.iter().find(|l| l.id == id)
    }
    pub fn link_type_by_api(&self, api_name: &str) -> Option<&LinkType> {
        self.state
            .link_types
            .iter()
            .find(|l| l.api_name == api_name)
    }
    pub fn action_type_by_api(&self, api_name: &str) -> Option<&ActionType> {
        self.state
            .action_types
            .iter()
            .find(|a| a.api_name == api_name)
    }
    pub fn object(&self, id: &str) -> Option<&Obj> {
        self.state.objects.iter().find(|o| o.id == id)
    }
    pub fn object_link(&self, id: &str) -> Option<&ObjectLink> {
        self.state.object_links.iter().find(|l| l.id == id)
    }

    /// Instances of one object type.
    pub fn objects_of_type(&self, type_id: &str) -> impl Iterator<Item = &Obj> {
        self.state
            .objects
            .iter()
            .filter(move |o| o.type_id == type_id)
    }

    /// Instance links touching an object (either direction).
    pub fn links_of_object(&self, object_id: &str) -> impl Iterator<Item = &ObjectLink> {
        self.state
            .object_links
            .iter()
            .filter(move |l| l.source_object_id == object_id || l.target_object_id == object_id)
    }

    pub fn ontology(&self) -> Ontology {
        Ontology {
            object_types: self.state.object_types.clone(),
            link_types: self.state.link_types.clone(),
            action_types: self.state.action_types.clone(),
        }
    }

    // ---------- object types ----------

    pub fn create_object_type(&mut self, mut t: ObjectType) -> Result<&ObjectType> {
        if t.api_name.trim().is_empty() {
            return Err(Error::bad("apiName is required"));
        }
        if t.display_name.trim().is_empty() {
            return Err(Error::bad("displayName is required"));
        }
        t.api_name = t.api_name.trim().to_string();
        t.display_name = t.display_name.trim().to_string();
        if t.plural_display_name.is_empty() {
            t.plural_display_name = format!("{}s", t.display_name);
        }
        if t.domain.is_empty() {
            t.domain = "General".into();
        }
        if t.color.is_empty() {
            t.color = "#f0b35c".into();
        }
        if self.object_type_by_api(&t.api_name).is_some() {
            return Err(Error::clash(format!(
                "apiName \"{}\" already exists",
                t.api_name
            )));
        }
        if t.id.is_empty() {
            t.id = uuid::Uuid::new_v4().to_string();
        }
        self.state.object_types.push(t);
        Ok(self.state.object_types.last().unwrap())
    }

    /// Partial update — fields left `None` are untouched.
    pub fn update_object_type(&mut self, id: &str, patch: ObjectTypePatch) -> Result<&ObjectType> {
        if let Some(api) = &patch.api_name
            && let Some(other) = self.object_type_by_api(api)
            && other.id != id
        {
            return Err(Error::clash(format!("apiName \"{api}\" already exists")));
        }
        let t = self
            .state
            .object_types
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| Error::gone("object type not found"))?;
        if let Some(v) = patch.api_name {
            t.api_name = v;
        }
        if let Some(v) = patch.display_name {
            t.display_name = v;
        }
        if let Some(v) = patch.plural_display_name {
            t.plural_display_name = v;
        }
        if let Some(v) = patch.description {
            t.description = v;
        }
        if let Some(v) = patch.domain {
            t.domain = v;
        }
        if let Some(v) = patch.color {
            t.color = v;
        }
        if let Some(v) = patch.properties {
            t.properties = v;
        }
        Ok(t)
    }

    /// Cascades: link types touching it, action types on it, instances of it,
    /// instance links of those (and of the removed link types), layout row.
    pub fn delete_object_type(&mut self, id: &str) -> Result<()> {
        let before = self.state.object_types.len();
        self.state.object_types.retain(|t| t.id != id);
        if self.state.object_types.len() == before {
            return Err(Error::gone("object type not found"));
        }
        self.state
            .link_types
            .retain(|l| l.source_type_id != id && l.target_type_id != id);
        self.state.action_types.retain(|a| a.object_type_id != id);
        self.state.objects.retain(|o| o.type_id != id);
        // instance links referencing removed link types or removed objects
        let link_ids: std::collections::HashSet<&str> = self
            .state
            .link_types
            .iter()
            .map(|l| l.id.as_str())
            .collect();
        let obj_ids: std::collections::HashSet<&str> =
            self.state.objects.iter().map(|o| o.id.as_str()).collect();
        self.state.object_links.retain(|l| {
            link_ids.contains(l.link_type_id.as_str())
                && obj_ids.contains(l.source_object_id.as_str())
                && obj_ids.contains(l.target_object_id.as_str())
        });
        self.state.layout.retain(|p| p.type_id != id);
        Ok(())
    }

    // ---------- link types ----------

    pub fn create_link_type(&mut self, mut l: LinkType) -> Result<&LinkType> {
        if l.api_name.trim().is_empty() {
            return Err(Error::bad("apiName is required"));
        }
        if l.display_name.trim().is_empty() {
            return Err(Error::bad("displayName is required"));
        }
        l.api_name = l.api_name.trim().to_string();
        l.display_name = l.display_name.trim().to_string();
        if self.object_type(&l.source_type_id).is_none()
            || self.object_type(&l.target_type_id).is_none()
        {
            return Err(Error::bad(
                "sourceTypeId and targetTypeId must reference existing object types",
            ));
        }
        if self.link_type_by_api(&l.api_name).is_some() {
            return Err(Error::clash(format!(
                "apiName \"{}\" already exists",
                l.api_name
            )));
        }
        if l.id.is_empty() {
            l.id = uuid::Uuid::new_v4().to_string();
        }
        self.state.link_types.push(l);
        Ok(self.state.link_types.last().unwrap())
    }

    pub fn update_link_type(&mut self, id: &str, patch: LinkTypePatch) -> Result<&LinkType> {
        if let Some(api) = &patch.api_name
            && let Some(other) = self.link_type_by_api(api)
            && other.id != id
        {
            return Err(Error::clash(format!("apiName \"{api}\" already exists")));
        }
        let l = self
            .state
            .link_types
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or_else(|| Error::gone("link type not found"))?;
        if let Some(v) = patch.api_name {
            l.api_name = v;
        }
        if let Some(v) = patch.display_name {
            l.display_name = v;
        }
        if let Some(v) = patch.description {
            l.description = v;
        }
        if let Some(v) = patch.source_cardinality {
            l.source_cardinality = v;
        }
        if let Some(v) = patch.target_cardinality {
            l.target_cardinality = v;
        }
        Ok(l)
    }

    /// Cascades to instance links of this link type.
    pub fn delete_link_type(&mut self, id: &str) -> Result<()> {
        let before = self.state.link_types.len();
        self.state.link_types.retain(|l| l.id != id);
        if self.state.link_types.len() == before {
            return Err(Error::gone("link type not found"));
        }
        self.state.object_links.retain(|l| l.link_type_id != id);
        Ok(())
    }

    // ---------- action types ----------

    pub fn create_action_type(&mut self, mut a: ActionType) -> Result<&ActionType> {
        if a.api_name.trim().is_empty() {
            return Err(Error::bad("apiName is required"));
        }
        if a.display_name.trim().is_empty() {
            return Err(Error::bad("displayName is required"));
        }
        a.api_name = a.api_name.trim().to_string();
        a.display_name = a.display_name.trim().to_string();
        if self.object_type(&a.object_type_id).is_none() {
            return Err(Error::bad(
                "objectTypeId must reference an existing object type",
            ));
        }
        if self.action_type_by_api(&a.api_name).is_some() {
            return Err(Error::clash(format!(
                "apiName \"{}\" already exists",
                a.api_name
            )));
        }
        if a.id.is_empty() {
            a.id = uuid::Uuid::new_v4().to_string();
        }
        self.state.action_types.push(a);
        Ok(self.state.action_types.last().unwrap())
    }

    pub fn delete_action_type(&mut self, id: &str) -> Result<()> {
        let before = self.state.action_types.len();
        self.state.action_types.retain(|a| a.id != id);
        if self.state.action_types.len() == before {
            return Err(Error::gone("action type not found"));
        }
        Ok(())
    }

    // ---------- objects (instances) ----------

    /// `pattern`/`enumValues`/`unique` violations for an instance against its
    /// type's property defs — `(rule, message)` pairs; `unique` maps to
    /// Conflict at write time, the rest to BadRequest.
    fn property_violations(&self, o: &Obj) -> Vec<(&'static str, String)> {
        let Some(ty) = self.object_type(&o.type_id) else {
            return vec![];
        };
        let scalar = |v: &serde_json::Value| -> String {
            v.as_str()
                .map(str::to_string)
                .unwrap_or_else(|| v.to_string())
        };
        let mut out = vec![];
        for p in &ty.properties {
            let Some(v) = o.properties.get(&p.api_name) else {
                continue;
            };
            let scalars: Vec<String> = match v {
                serde_json::Value::Array(a) => a.iter().map(scalar).collect(),
                other => vec![scalar(other)],
            };
            if let Some(pat) = &p.pattern {
                match regex_lite::Regex::new(pat) {
                    Ok(re) => {
                        for s in &scalars {
                            if !re.is_match(s) {
                                out.push((
                                    "pattern",
                                    format!(
                                        "property '{}' value '{s}' violates pattern /{pat}/",
                                        p.api_name
                                    ),
                                ));
                            }
                        }
                    }
                    Err(_) => out.push((
                        "pattern",
                        format!("property '{}' has an invalid pattern", p.api_name),
                    )),
                }
            }
            if let Some(vals) = &p.enum_values {
                for s in &scalars {
                    if !vals.contains(s) {
                        out.push((
                            "enum",
                            format!(
                                "property '{}' value '{s}' violates enum {vals:?}",
                                p.api_name
                            ),
                        ));
                    }
                }
            }
            if p.unique
                && self
                    .objects_of_type(&ty.id)
                    .any(|x| x.id != o.id && x.properties.get(&p.api_name) == Some(v))
            {
                out.push((
                    "unique",
                    format!("property '{}' value violates unique", p.api_name),
                ));
            }
        }
        out
    }

    /// Non-fatal problems in the current state — referential integrity plus
    /// property-constraint violations. Backs `outofmemory validate`.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = vec![];
        for l in &self.state.link_types {
            if self.object_type(&l.source_type_id).is_none()
                || self.object_type(&l.target_type_id).is_none()
            {
                problems.push(format!("link type {} has unresolved endpoints", l.api_name));
            }
        }
        for l in &self.state.object_links {
            if self.link_type(&l.link_type_id).is_none()
                || self.object(&l.source_object_id).is_none()
                || self.object(&l.target_object_id).is_none()
            {
                problems.push(format!("object link {} is dangling", l.id));
            }
        }
        for o in &self.state.objects {
            for (_, msg) in self.property_violations(o) {
                problems.push(format!("object {}: {msg}", o.id));
            }
        }
        // cardinality violations — imports bypass write-time checks
        let mut src_counts: std::collections::HashMap<(&str, &str), usize> =
            std::collections::HashMap::new();
        let mut tgt_counts: std::collections::HashMap<(&str, &str), usize> =
            std::collections::HashMap::new();
        for l in &self.state.object_links {
            let Some(lt) = self.link_type(&l.link_type_id) else {
                continue;
            };
            if lt.target_cardinality == Cardinality::One {
                let c = src_counts
                    .entry((&l.link_type_id, &l.source_object_id))
                    .or_default();
                *c += 1;
                if *c == 2 {
                    problems.push(format!(
                        "link type {} allows only ONE target per source object (object {})",
                        lt.api_name, l.source_object_id
                    ));
                }
            }
            if lt.source_cardinality == Cardinality::One {
                let c = tgt_counts
                    .entry((&l.link_type_id, &l.target_object_id))
                    .or_default();
                *c += 1;
                if *c == 2 {
                    problems.push(format!(
                        "link type {} allows only ONE source per target object (object {})",
                        lt.api_name, l.target_object_id
                    ));
                }
            }
        }
        problems
    }

    pub fn create_object(&mut self, mut o: Obj) -> Result<&Obj> {
        if o.title.trim().is_empty() {
            return Err(Error::bad("title is required"));
        }
        if self.object_type(&o.type_id).is_none() {
            return Err(Error::bad("typeId must reference an existing object type"));
        }
        if let Some((rule, msg)) = self.property_violations(&o).into_iter().next() {
            return Err(match rule {
                "unique" => Error::clash(msg),
                _ => Error::bad(msg),
            });
        }
        o.title = o.title.trim().to_string();
        if o.id.is_empty() {
            o.id = uuid::Uuid::new_v4().to_string();
        }
        if o.created_at.is_empty() {
            o.created_at = crate::util::now_iso();
        }
        self.state.objects.push(o);
        Ok(self.state.objects.last().unwrap())
    }

    /// Cascades to instance links where the object is source or target.
    pub fn delete_object(&mut self, id: &str) -> Result<()> {
        let before = self.state.objects.len();
        self.state.objects.retain(|o| o.id != id);
        if self.state.objects.len() == before {
            return Err(Error::gone("object not found"));
        }
        self.state
            .object_links
            .retain(|l| l.source_object_id != id && l.target_object_id != id);
        Ok(())
    }

    // ---------- object links (instance edges) ----------

    pub fn create_object_link(&mut self, mut l: ObjectLink) -> Result<&ObjectLink> {
        let lt = self
            .link_type(&l.link_type_id)
            .ok_or_else(|| Error::bad("linkTypeId must reference an existing link type"))?;
        let src = self
            .object(&l.source_object_id)
            .ok_or_else(|| Error::bad("source and target objects must exist"))?;
        let tgt = self
            .object(&l.target_object_id)
            .ok_or_else(|| Error::bad("source and target objects must exist"))?;
        if src.type_id != lt.source_type_id {
            return Err(Error::bad(
                "source object's type does not match the link type's source",
            ));
        }
        if tgt.type_id != lt.target_type_id {
            return Err(Error::bad(
                "target object's type does not match the link type's target",
            ));
        }
        if self.state.object_links.iter().any(|x| {
            x.link_type_id == l.link_type_id
                && x.source_object_id == l.source_object_id
                && x.target_object_id == l.target_object_id
        }) {
            return Err(Error::clash("this link already exists"));
        }
        // `MANY:ONE` = many sources → one target: a ONE end caps the links
        // on that side (e.g. a second CURRENT_PORT for one vessel).
        if lt.target_cardinality == Cardinality::One
            && self.state.object_links.iter().any(|x| {
                x.link_type_id == l.link_type_id && x.source_object_id == l.source_object_id
            })
        {
            return Err(Error::clash(format!(
                "link type {} allows only ONE target per source object",
                lt.api_name
            )));
        }
        if lt.source_cardinality == Cardinality::One
            && self.state.object_links.iter().any(|x| {
                x.link_type_id == l.link_type_id && x.target_object_id == l.target_object_id
            })
        {
            return Err(Error::clash(format!(
                "link type {} allows only ONE source per target object",
                lt.api_name
            )));
        }
        if l.id.is_empty() {
            l.id = uuid::Uuid::new_v4().to_string();
        }
        self.state.object_links.push(l);
        Ok(self.state.object_links.last().unwrap())
    }

    pub fn delete_object_link(&mut self, id: &str) -> Result<()> {
        let before = self.state.object_links.len();
        self.state.object_links.retain(|l| l.id != id);
        if self.state.object_links.len() == before {
            return Err(Error::gone("object link not found"));
        }
        Ok(())
    }

    // ---------- layout ----------

    /// Upsert canvas positions keyed by type id (`PUT /api/layout`).
    pub fn set_layout(&mut self, positions: Vec<LayoutEntry>) {
        for p in positions {
            if let Some(e) = self
                .state
                .layout
                .iter_mut()
                .find(|e| e.type_id == p.type_id)
            {
                *e = p;
            } else {
                self.state.layout.push(p);
            }
        }
    }

    // ---------- bulk state (undo/redo backing + import) ----------

    /// `PUT /api/state` — atomic whole-ontology replace.
    pub fn replace_all(&mut self, state: State) -> Summary {
        let summary = state.summary();
        self.state = state;
        summary
    }

    /// `POST /api/import` merge mode — keep what exists, add what is missing
    /// (matched by apiName/id). Incoming `sourceTypeId`/`targetTypeId`/
    /// `typeId`/`linkTypeId`/`objectTypeId` references that name an apiName are
    /// resolved to existing ids, which is how the AI-generation payload links
    /// to the current ontology.
    pub fn merge(&mut self, incoming: State) -> Summary {
        // Pass 1 — object types first, so apiName → id resolution exists for
        // everything that references them.
        for t in &incoming.object_types {
            if self.object_type_by_api(&t.api_name).is_none() && self.object_type(&t.id).is_none() {
                let mut t = t.clone();
                if t.plural_display_name.is_empty() {
                    t.plural_display_name = format!("{}s", t.display_name);
                }
                if t.domain.is_empty() {
                    t.domain = "General".into();
                }
                if t.color.is_empty() {
                    t.color = "#f0b35c".into();
                }
                self.state.object_types.push(t);
            }
        }

        // apiName → id over the union (existing wins on conflict, which is
        // what makes apiName-endpoints in incoming payloads reach existing
        // types); a key that is already an id passes through untouched.
        // Matching is exact first, then case-insensitive — generated payloads
        // normalize apiNames to UPPER_SNAKE while seeds/imports may carry
        // TitleCase names (the TS merge leaves those dangling; we resolve them).
        let type_id_of = |key: &str| -> String {
            self.state
                .object_types
                .iter()
                .find(|t| t.api_name == key)
                .or_else(|| {
                    self.state
                        .object_types
                        .iter()
                        .find(|t| t.api_name.eq_ignore_ascii_case(key))
                })
                .map(|t| t.id.clone())
                .unwrap_or_else(|| key.to_string())
        };

        let new_link_types: Vec<LinkType> = incoming
            .link_types
            .iter()
            .filter(|l| {
                self.link_type_by_api(&l.api_name).is_none() && self.link_type(&l.id).is_none()
            })
            .map(|l| {
                let mut l = l.clone();
                l.source_type_id = type_id_of(&l.source_type_id);
                l.target_type_id = type_id_of(&l.target_type_id);
                l
            })
            .collect();
        self.state.link_types.extend(new_link_types);

        let new_action_types: Vec<ActionType> = incoming
            .action_types
            .iter()
            .filter(|a| {
                self.action_type_by_api(&a.api_name).is_none()
                    && !self.state.action_types.iter().any(|x| x.id == a.id)
            })
            .map(|a| {
                let mut a = a.clone();
                a.object_type_id = type_id_of(&a.object_type_id);
                a
            })
            .collect();
        self.state.action_types.extend(new_action_types);

        let new_objects: Vec<Obj> = incoming
            .objects
            .iter()
            .filter(|o| self.object(&o.id).is_none())
            .map(|o| {
                let mut o = o.clone();
                o.type_id = type_id_of(&o.type_id);
                o
            })
            .collect();
        self.state.objects.extend(new_objects);

        let link_id_of = |key: &str| -> String {
            self.state
                .link_types
                .iter()
                .find(|l| l.api_name == key)
                .map(|l| l.id.clone())
                .unwrap_or_else(|| key.to_string())
        };
        let new_links: Vec<ObjectLink> = incoming
            .object_links
            .iter()
            .filter(|l| self.object_link(&l.id).is_none())
            .map(|l| {
                let mut l = l.clone();
                l.link_type_id = link_id_of(&l.link_type_id);
                l
            })
            .collect();
        self.state.object_links.extend(new_links);

        incoming.summary()
    }

    /// `POST /api/import` — dispatch on mode.
    pub fn import(&mut self, incoming: State, mode: ImportMode) -> Summary {
        match mode {
            ImportMode::Replace => self.replace_all(incoming),
            ImportMode::Merge => self.merge(incoming),
        }
    }
}

/// Fields the PATCH routes accept for an object type.
#[derive(Debug, Clone, Default)]
pub struct ObjectTypePatch {
    pub api_name: Option<String>,
    pub display_name: Option<String>,
    pub plural_display_name: Option<String>,
    pub description: Option<String>,
    pub domain: Option<String>,
    pub color: Option<String>,
    pub properties: Option<Vec<PropertyDef>>,
}

/// Fields the PATCH routes accept for a link type.
#[derive(Debug, Clone, Default)]
pub struct LinkTypePatch {
    pub api_name: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub source_cardinality: Option<Cardinality>,
    pub target_cardinality: Option<Cardinality>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn ty(api: &str) -> ObjectType {
        ObjectType {
            id: String::new(),
            api_name: api.into(),
            display_name: api.to_lowercase(),
            plural_display_name: String::new(),
            description: String::new(),
            domain: String::new(),
            color: String::new(),
            properties: vec![],
        }
    }

    #[test]
    fn create_validates_and_defaults() {
        let mut s = Store::new();
        assert_eq!(
            s.create_object_type(ty("")).unwrap_err().kind,
            ErrorKind::BadRequest
        );
        let t = s.create_object_type(ty("SHIPMENT")).unwrap();
        assert_eq!(t.plural_display_name, "shipments");
        assert_eq!(t.domain, "General");
        assert!(!t.id.is_empty());
        assert_eq!(
            s.create_object_type(ty("SHIPMENT")).unwrap_err().kind,
            ErrorKind::Conflict
        );
    }

    #[test]
    fn delete_object_type_cascades() {
        let mut s = Store::new();
        let a = s.create_object_type(ty("A")).unwrap().id.clone();
        let b = s.create_object_type(ty("B")).unwrap().id.clone();
        let lt = s
            .create_link_type(LinkType {
                id: String::new(),
                api_name: "A_TO_B".into(),
                display_name: "a to b".into(),
                description: String::new(),
                source_type_id: a.clone(),
                target_type_id: b.clone(),
                source_cardinality: Cardinality::Many,
                target_cardinality: Cardinality::One,
            })
            .unwrap()
            .id
            .clone();
        s.create_action_type(ActionType {
            id: String::new(),
            api_name: "DO_A".into(),
            display_name: "do a".into(),
            description: String::new(),
            object_type_id: a.clone(),
            parameters: vec![],
        })
        .unwrap();
        let oa = s
            .create_object(Obj {
                id: String::new(),
                type_id: a.clone(),
                title: "a1".into(),
                properties: Default::default(),
                created_at: String::new(),
            })
            .unwrap()
            .id
            .clone();
        let ob = s
            .create_object(Obj {
                id: String::new(),
                type_id: b.clone(),
                title: "b1".into(),
                properties: Default::default(),
                created_at: String::new(),
            })
            .unwrap()
            .id
            .clone();
        s.create_object_link(ObjectLink {
            id: String::new(),
            link_type_id: lt,
            source_object_id: oa.clone(),
            target_object_id: ob.clone(),
        })
        .unwrap();
        s.set_layout(vec![LayoutEntry {
            type_id: a.clone(),
            x: 1.0,
            y: 2.0,
        }]);

        s.delete_object_type(&a).unwrap();
        assert!(s.state.link_types.is_empty());
        assert!(s.state.action_types.is_empty());
        assert!(s.state.layout.is_empty());
        assert!(s.object(&oa).is_none());
        assert!(s.state.object_links.is_empty());
        assert!(s.object(&ob).is_some());
    }

    #[test]
    fn link_endpoint_and_dup_checks() {
        let mut s = Store::new();
        let a = s.create_object_type(ty("A")).unwrap().id.clone();
        let b = s.create_object_type(ty("B")).unwrap().id.clone();
        let lt = s
            .create_link_type(LinkType {
                id: String::new(),
                api_name: "A_TO_B".into(),
                display_name: "a to b".into(),
                description: String::new(),
                source_type_id: a.clone(),
                target_type_id: b.clone(),
                source_cardinality: Cardinality::Many,
                target_cardinality: Cardinality::Many,
            })
            .unwrap()
            .id
            .clone();
        let oa = s
            .create_object(Obj {
                id: String::new(),
                type_id: a.clone(),
                title: "a".into(),
                properties: Default::default(),
                created_at: String::new(),
            })
            .unwrap()
            .id
            .clone();
        let ob = s
            .create_object(Obj {
                id: String::new(),
                type_id: b.clone(),
                title: "b".into(),
                properties: Default::default(),
                created_at: String::new(),
            })
            .unwrap()
            .id
            .clone();
        let ob2 = s
            .create_object(Obj {
                id: String::new(),
                type_id: b.clone(),
                title: "b2".into(),
                properties: Default::default(),
                created_at: String::new(),
            })
            .unwrap()
            .id
            .clone();

        // wrong direction: B is not a valid source
        let bad = ObjectLink {
            id: String::new(),
            link_type_id: lt.clone(),
            source_object_id: ob.clone(),
            target_object_id: oa.clone(),
        };
        assert_eq!(
            s.create_object_link(bad).unwrap_err().kind,
            ErrorKind::BadRequest
        );

        let ok = ObjectLink {
            id: String::new(),
            link_type_id: lt.clone(),
            source_object_id: oa.clone(),
            target_object_id: ob.clone(),
        };
        s.create_object_link(ok.clone()).unwrap();
        // duplicate triple → 409
        assert_eq!(
            s.create_object_link(ok).unwrap_err().kind,
            ErrorKind::Conflict
        );
        // different target → fine
        let ok2 = ObjectLink {
            id: String::new(),
            link_type_id: lt,
            source_object_id: oa,
            target_object_id: ob2,
        };
        s.create_object_link(ok2).unwrap();
    }

    #[test]
    fn merge_resolves_api_name_endpoints() {
        let mut s = Store::new();
        let a = s.create_object_type(ty("A")).unwrap().id.clone();

        // incoming payload references the existing type by apiName, not id
        let mut incoming = State::default();
        incoming.link_types.push(LinkType {
            id: "l1".into(),
            api_name: "A_TO_B".into(),
            display_name: "a to b".into(),
            description: String::new(),
            source_type_id: "A".into(), // apiName, not id
            target_type_id: "b-id".into(),
            source_cardinality: Cardinality::Many,
            target_cardinality: Cardinality::One,
        });
        incoming.object_types.push(ObjectType {
            id: "b-id".into(),
            ..ty("B")
        });
        s.merge(incoming);

        assert_eq!(s.state.object_types.len(), 2);
        let l = &s.state.link_types[0];
        assert_eq!(l.source_type_id, a);
        assert_eq!(l.target_type_id, "b-id");
    }
}
