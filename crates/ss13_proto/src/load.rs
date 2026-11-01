use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::prelude::*;
use bevy::reflect::serde::TypedReflectDeserializer;
use bevy::reflect::{TypeRegistry, TypeRegistryArc};
use ron::extensions::Extensions;
use ron::value::RawValue;
use serde::Deserialize;

use crate::registry::ComponentVTable;
use crate::{ErrorKind, LoadError, ProtoComponents, ProtoError};

/// A prototype with inheritance already resolved: spawning only clones its values.
pub struct Prototype {
    id: String,
    parent: Option<String>,
    file: Option<PathBuf>,
    components: Vec<ResolvedComponent>,
}

#[derive(Clone)]
struct ResolvedComponent {
    name: Arc<str>,
    vtable: ComponentVTable,
    // Shared with parents and children that do not override this component.
    value: Arc<dyn Reflect>,
}

impl Prototype {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }

    /// File the prototype was defined in; `None` for an instance with overrides applied.
    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    /// Resolved value of a component, if the prototype carries it.
    pub fn get<T: Component + Reflect>(&self) -> Option<&T> {
        self.components
            .iter()
            .find_map(|c| c.value.downcast_ref::<T>())
    }

    /// Spawns the prototype's components, then `extra`.
    pub fn spawn<'w>(&self, world: &'w mut World, extra: impl Bundle) -> EntityWorldMut<'w> {
        let mut entity = world.spawn_empty();
        self.insert_into(&mut entity);
        entity.insert(extra);
        entity
    }

    pub fn insert_into(&self, entity: &mut EntityWorldMut) {
        for c in &self.components {
            (c.vtable.insert)(&*c.value, entity);
        }
    }
}

/// All loaded prototypes. Inserted by [`ProtoPlugin`](crate::ProtoPlugin).
#[derive(Resource)]
pub struct Prototypes {
    by_id: HashMap<String, Arc<Prototype>>,
    components: ProtoComponents,
    types: TypeRegistryArc,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProto<'a> {
    id: String,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default, borrow)]
    components: BTreeMap<String, &'a RawValue>,
}

fn ron_options() -> ron::Options {
    // `parent: "door"` instead of `parent: Some("door")`, same for `Option` fields.
    ron::Options::default().with_default_extension(Extensions::IMPLICIT_SOME)
}

impl Prototypes {
    /// Loads every `*.ron` file under `dir`, recursively.
    pub fn load_dir(
        dir: &Path,
        components: &ProtoComponents,
        types: &TypeRegistryArc,
    ) -> Result<Self, LoadError> {
        let mut paths = Vec::new();
        let mut errors = Vec::new();
        collect_ron_files(dir, &mut paths, &mut errors);
        paths.sort();
        let mut sources = Vec::with_capacity(paths.len());
        for path in paths {
            match std::fs::read_to_string(&path) {
                Ok(text) => sources.push((path, text)),
                Err(e) => errors.push(file_error(&path, e.into())),
            }
        }
        if !errors.is_empty() {
            return Err(LoadError(errors));
        }
        Self::from_sources(&sources, components, types)
    }

    /// Loads prototypes from `(path, text)` pairs; `path` is used only in errors.
    pub fn from_sources(
        sources: &[(PathBuf, String)],
        components: &ProtoComponents,
        types: &TypeRegistryArc,
    ) -> Result<Self, LoadError> {
        let options = ron_options();
        let mut errors = Vec::new();
        let mut index: HashMap<&str, (usize, &RawProto)> = HashMap::new();
        let parsed: Vec<Vec<RawProto>> = sources
            .iter()
            .map(|(path, text)| {
                options
                    .from_str(text)
                    .unwrap_or_else(|e: ron::error::SpannedError| {
                        errors.push(ProtoError {
                            pos: Some(compact((e.span.start.line, e.span.start.col))),
                            ..file_error(path, ErrorKind::Syntax(e.code.to_string()))
                        });
                        Vec::new()
                    })
            })
            .collect();
        for (file, protos) in parsed.iter().enumerate() {
            for raw in protos {
                if let Some(&(first, _)) = index.get(raw.id.as_str()) {
                    errors.push(ProtoError {
                        proto: Some(raw.id.clone()),
                        ..file_error(
                            &sources[file].0,
                            ErrorKind::DuplicateId(sources[first].0.clone()),
                        )
                    });
                } else {
                    index.insert(&raw.id, (file, raw));
                }
            }
        }

        let registry = types.read();
        let mut resolver = Resolver {
            sources,
            index: &index,
            components,
            registry: &registry,
            done: HashMap::new(),
            stack: Vec::new(),
            errors,
        };
        for protos in &parsed {
            for raw in protos {
                resolver.resolve(&raw.id);
            }
        }
        let Resolver { done, errors, .. } = resolver;
        drop(registry);
        if !errors.is_empty() {
            return Err(LoadError(errors));
        }
        Ok(Self {
            by_id: done
                .into_iter()
                .map(|(id, p)| (id.to_owned(), p.expect("no errors")))
                .collect(),
            components: components.clone(),
            types: types.clone(),
        })
    }

    pub fn get(&self, id: &str) -> Option<&Arc<Prototype>> {
        self.by_id.get(id)
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.by_id.keys().map(String::as_str)
    }

    /// `id` with per-instance `overrides` applied by the prototype merge rule. `overrides` uses
    /// the syntax of a prototype's `components` map: `{"door": (open: true)}`.
    /// Resolve once per distinct `(id, overrides)` (one `.dmm` key) and spawn it many times.
    pub fn instance(
        &self,
        id: &str,
        overrides: Option<&str>,
    ) -> Result<Arc<Prototype>, ProtoError> {
        let base = self.by_id.get(id).ok_or_else(|| ProtoError {
            file: None,
            pos: None,
            proto: Some(id.to_owned()),
            component: None,
            kind: ErrorKind::UnknownPrototype,
        })?;
        let Some(text) = overrides else {
            return Ok(base.clone());
        };
        let components: BTreeMap<String, &RawValue> =
            ron_options().from_str(text).map_err(|e| ProtoError {
                file: None,
                pos: Some(compact((e.span.start.line, e.span.start.col))),
                proto: Some(id.to_owned()),
                component: None,
                kind: ErrorKind::Syntax(e.code.to_string()),
            })?;
        let mut errors = Vec::new();
        let registry = self.types.read();
        let derived = derive(
            Some(base),
            id,
            None,
            text,
            &components,
            &self.components,
            &registry,
            &mut errors,
        );
        match errors.into_iter().next() {
            Some(e) => Err(e),
            None => Ok(Arc::new(derived)),
        }
    }
}

struct Resolver<'s, 'a> {
    sources: &'s [(PathBuf, String)],
    index: &'s HashMap<&'a str, (usize, &'a RawProto<'a>)>,
    components: &'s ProtoComponents,
    registry: &'s TypeRegistry,
    /// `None` = failed (already reported, or its parent failed).
    done: HashMap<&'a str, Option<Arc<Prototype>>>,
    stack: Vec<&'a str>,
    errors: Vec<ProtoError>,
}

impl<'a> Resolver<'_, 'a> {
    fn resolve(&mut self, id: &'a str) -> Option<Arc<Prototype>> {
        if let Some(done) = self.done.get(id) {
            return done.clone();
        }
        let (file, raw) = self.index[id];
        let path = &self.sources[file].0;
        if let Some(start) = self.stack.iter().position(|s| *s == id) {
            let mut chain: Vec<String> =
                self.stack[start..].iter().map(|s| s.to_string()).collect();
            chain.push(id.to_owned());
            self.errors.push(ProtoError {
                proto: Some(id.to_owned()),
                ..file_error(path, ErrorKind::Cycle(chain))
            });
            return None;
        }
        let parent = match raw.parent.as_deref() {
            None => None,
            Some(parent) => {
                let Some((&parent, _)) = self.index.get_key_value(parent) else {
                    self.errors.push(ProtoError {
                        proto: Some(id.to_owned()),
                        ..file_error(path, ErrorKind::UnknownParent(parent.to_owned()))
                    });
                    self.done.insert(id, None);
                    return None;
                };
                self.stack.push(id);
                let resolved = self.resolve(parent);
                self.stack.pop();
                let Some(resolved) = resolved else {
                    self.done.insert(id, None);
                    return None;
                };
                Some(resolved)
            }
        };
        let errors_before = self.errors.len();
        let proto = derive(
            parent.as_deref(),
            id,
            Some(path),
            &self.sources[file].1,
            &raw.components,
            self.components,
            self.registry,
            &mut self.errors,
        );
        let result = (self.errors.len() == errors_before).then(|| Arc::new(proto));
        self.done.insert(id, result.clone());
        result
    }
}

/// Applies `patches` on top of `parent` by the merge rule. `text` is the source the raw values
/// borrow from, used to turn their offsets into file positions.
fn derive(
    parent: Option<&Prototype>,
    id: &str,
    file: Option<&Path>,
    text: &str,
    patches: &BTreeMap<String, &RawValue>,
    components: &ProtoComponents,
    registry: &TypeRegistry,
    errors: &mut Vec<ProtoError>,
) -> Prototype {
    let options = ron_options();
    let mut resolved = parent.map(|p| p.components.clone()).unwrap_or_default();
    for (name, raw) in patches {
        let ron = raw.get_ron();
        let start = position(text, ron);
        let mut error = |pos, kind| {
            errors.push(ProtoError {
                file: file.map(Path::to_path_buf),
                pos: Some(compact(pos)),
                proto: Some(id.to_owned()),
                component: Some(name.clone()),
                kind,
            });
        };
        let Some(vtable) = components.get(name) else {
            error(start, ErrorKind::UnknownComponent);
            continue;
        };
        let registration = registry
            .get(vtable.type_id)
            .expect("register_proto_component registers the type");
        let patch = match options
            .from_str_seed(ron, TypedReflectDeserializer::new(registration, registry))
        {
            Ok(patch) => patch,
            Err(e) => {
                let at = offset_position(start, (e.span.start.line, e.span.start.col));
                error(at, ErrorKind::BadValue(e.code.to_string()));
                continue;
            }
        };
        let existing = resolved.iter().position(|c| *c.name == **name);
        match (vtable.merge)(existing.map(|i| &*resolved[i].value), &*patch, registry) {
            Ok(value) => match existing {
                Some(i) => resolved[i].value = value,
                None => resolved.push(ResolvedComponent {
                    name: name.as_str().into(),
                    vtable: *vtable,
                    value,
                }),
            },
            Err(kind) => error(start, kind),
        }
    }
    Prototype {
        id: id.to_owned(),
        parent: parent.map(|p| p.id.clone()),
        file: file.map(Path::to_path_buf),
        components: resolved,
    }
}

/// 1-based line/column of `part`, a subslice of `text`.
fn position(text: &str, part: &str) -> (usize, usize) {
    let offset = (part.as_ptr() as usize).saturating_sub(text.as_ptr() as usize);
    let before = &text[..offset.min(text.len())];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    (
        before.matches('\n').count() + 1,
        before[line_start..].chars().count() + 1,
    )
}

/// Turns a position inside a fragment that starts at `start` into a position in the file.
fn offset_position(start: (usize, usize), inner: (usize, usize)) -> (usize, usize) {
    if inner.0 <= 1 {
        (start.0, start.1 + inner.1.saturating_sub(1))
    } else {
        (start.0 + inner.0 - 1, inner.1)
    }
}

// Keeps `ProtoError` small enough to return by value.
fn compact((line, col): (usize, usize)) -> (u32, u32) {
    (line as u32, col as u32)
}

fn file_error(path: &Path, kind: ErrorKind) -> ProtoError {
    ProtoError {
        file: Some(path.to_path_buf()),
        pos: None,
        proto: None,
        component: None,
        kind,
    }
}

fn collect_ron_files(dir: &Path, out: &mut Vec<PathBuf>, errors: &mut Vec<ProtoError>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => return errors.push(file_error(dir, e.into())),
    };
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(e) => {
                errors.push(file_error(dir, e.into()));
                continue;
            }
        };
        if path.is_dir() {
            collect_ron_files(&path, out, errors);
        } else if path.extension().is_some_and(|e| e == "ron") {
            out.push(path);
        }
    }
}
