//! Offline extractor of static DM type data from a BYOND source tree.
//!
//! Uses SpacemanDMM's `dreammaker` (GPL-3.0), so this crate is a standalone tool and must never be
//! a dependency of the game crates.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use dreammaker as dm;
use dreammaker::constants::Constant;
use dreammaker::objtree::{ObjectTree, TypeRef, VarValue};
use serde::Serialize;
use serde_json::{Value, json};

pub mod populate;

/// Vars whose resolved (inherited, const-evaluated) values are dumped for every type.
pub const DUMPED_VARS: [&str; 16] = [
    "name",
    "desc",
    "icon",
    "icon_state",
    "dir",
    "density",
    "opacity",
    "layer",
    "plane",
    "anchored",
    "pixel_x",
    "pixel_y",
    "color",
    "alpha",
    "invisibility",
    "mouse_opacity",
];

/// `GLOBAL_LIST_INIT` globals whose constant value is dumped (top-level `globals`): the footstep
/// tables of `code/__DEFINES/footsteps.dm`.
pub const DUMPED_GLOBALS: [&str; 5] = [
    "footstep",
    "barefootstep",
    "clawfootstep",
    "heavyfootstep",
    // `atmos_computers/__identifiers.dm`: chamber id -> name.
    "station_gas_chambers",
];

/// Map directories (relative to the source root) checked by the acceptance scan.
pub const ACCEPTANCE_MAP_DIRS: [&str; 2] =
    ["_maps/map_files/MetaStation", "_maps/map_files/tramstation"];

/// Every map the game ships (stations, mining, away templates under it), for `--all-maps`.
pub const ALL_MAPS_DIR: &str = "_maps/map_files";

/// Version of the `dm_types.json` layout; consumers reject other versions.
/// 2: `declared_vars` is a map `name -> value` (was a list of names).
/// 3: per-type `procs`, top-level `element_calls`; later additive (absent in older dumps,
/// consumers default them): top-level `populate`, per-type `new_access_flags`, top-level
/// `access_flags`, top-level `globals`.
pub const SCHEMA: u32 = 3;

/// Elements whose `AddElement`/`RemoveElement` calls (components: `AddComponent`) are dumped with
/// the type and proc that make them: what an element does to an atom is decided in procs, which
/// static vars cannot show.
pub const SCANNED_ELEMENTS: [&str; 9] = [
    "/datum/element/undertile",
    "/datum/element/processable",
    "/datum/element/microwavable",
    "/datum/element/simple_flying",
    "/datum/element/can_be_held",
    "/datum/component/bakeable",
    "/datum/component/grillable",
    "/datum/element/dryable",
    "/datum/element/dunkable",
];

pub struct Environment {
    pub context: dm::Context,
    pub tree: ObjectTree,
    pub dme: PathBuf,
}

/// Parses the `.dme` of `source` the way dreamchecker does (`SpacemanDMM.toml` next to it,
/// builtin defines), plus `CBT`: tg's build (`tools/build/build.ts`) compiles with `-DCBT`, so
/// `MAP_SWITCH(compile_time, map_time)` takes the runtime icon, not the map editor preview.
pub fn load(source: &Path) -> anyhow::Result<Environment> {
    let source = source
        .canonicalize()
        .with_context(|| format!("source repo {}", source.display()))?;
    let Some(mut dme) = dm::detect_environment(&source, dm::DEFAULT_ENV)? else {
        bail!("no .dme in {}", source.display());
    };
    let mut context = dm::Context::default();
    context.autodetect_config(&dme);
    if let Some(env) = &context.config().environment {
        dme = source.join(env);
    }
    // Proc bodies are parsed too: [`access_helpers`] reads the code of a few procs (their
    // `ACCESS_*` defines only the preprocessor resolves) and `populate` lowers
    // `PopulateContents()` from the AST.
    let mut preprocessor = dm::preprocessor::Preprocessor::new(&context, dme.clone())
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", dme.display()))?;
    // Pushed on top of the `.dme`, so it is read first.
    preprocessor
        .push_file(PathBuf::from("<-DCBT>"), &b"#define CBT\n"[..])
        .map_err(|e| anyhow::anyhow!("CBT define: {e}"))?;
    let mut parser = dm::parser::Parser::new(
        &context,
        dm::indents::IndentProcessor::new(&context, preprocessor),
    );
    parser.enable_procs();
    let tree = parser.parse_object_tree();
    Ok(Environment { context, tree, dme })
}

impl Environment {
    fn root(&self) -> &Path {
        self.dme.parent().unwrap_or(Path::new("."))
    }

    /// Parser/const-eval diagnostics by severity; errors here mean values may be missing.
    pub fn diagnostics(&self) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for e in self.context.errors().iter() {
            *out.entry(format!("{:?}", e.severity())).or_default() += 1;
        }
        out
    }
}

#[derive(Serialize)]
pub struct TypeEntry {
    /// Parent type path; `None` for types directly under the root (`/datum`, `/list`, ...).
    pub parent: Option<String>,
    pub vars: BTreeMap<&'static str, Value>,
    /// Vars this type declares or overrides itself, with the value it assigns (same encoding as
    /// `vars`, `{ "expr": ... }` when not constant). Lets consumers rebuild DM inheritance
    /// instead of flattening it.
    pub declared_vars: BTreeMap<String, Value>,
    /// Procs this type defines or overrides itself (not inherited ones).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub procs: Vec<String>,
    /// Under [`ACCESS_HELPER_ROOT`]: the accesses this type's own `get_access()` appends to the
    /// parent result (consumers concatenate the chain, root first).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub get_access: Vec<String>,
    /// Under [`ID_TRIM_ROOT`]: per access list var, the access flag groups this type's own `New()`
    /// adds (`var |= SSid_access.get_flag_access_list(FLAG) + ...`); the groups are in
    /// [`Dump::access_flags`].
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub new_access_flags: BTreeMap<String, Vec<i64>>,
}

/// tg ID trims (`code/datums/id_trim/`).
pub const ID_TRIM_ROOT: &str = "/datum/id_trim";
/// The proc that fills `SSid_access.accesses_by_flag` (`code/controllers/subsystem/networks/id_access.dm`).
pub const ACCESS_FLAGS_PROC: (&str, &str) = (
    "/datum/controller/subsystem/id_access",
    "setup_access_flags",
);

/// tg airlock access helpers: each subtype's `get_access()` is `..()` plus its `ACCESS_*`.
pub const ACCESS_HELPER_ROOT: &str = "/obj/effect/mapping_helpers/airlock/access";

/// One `AddElement(<element>, ...)` / `RemoveElement(<element>, ...)` line in a proc body.
#[derive(Serialize, Debug, PartialEq)]
pub struct ElementCall {
    /// Type that owns the proc.
    #[serde(rename = "type")]
    pub owner: String,
    pub proc: String,
    /// `AddElement`, `RemoveElement` or `AddComponent`.
    pub call: &'static str,
    /// The source line, trimmed: conditions and arguments are for people reading the dump.
    pub line: String,
}

#[derive(Serialize)]
pub struct Dump {
    pub schema: u32,
    pub vars: Vec<&'static str>,
    pub types: BTreeMap<String, TypeEntry>,
    /// Per [`SCANNED_ELEMENTS`] entry.
    pub element_calls: BTreeMap<&'static str, Vec<ElementCall>>,
    /// What container types spawn into themselves ([`populate`]).
    pub populate: populate::Populate,
    /// `SSid_access.accesses_by_flag`: access flag (as a decimal string) -> its accesses.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub access_flags: BTreeMap<String, Vec<String>>,
    /// Per [`DUMPED_GLOBALS`] entry: its constant initial value (same encoding as `vars`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub globals: BTreeMap<&'static str, Value>,
    /// `/datum/gas_reaction/standard/*/init_factors()`: the constant `factor = list(...)` of
    /// each reaction type that sets one (the atmos handbook's text).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub reaction_factors: BTreeMap<String, Value>,
    /// Access helpers whose `get_access()` [`get_access_adds`] does not understand. Their
    /// `get_access` would be empty, which leaves a door open to everyone, so
    /// [`Dump::ensure_complete`] refuses the dump instead.
    #[serde(skip)]
    pub unparsed_access_helpers: Vec<String>,
}

impl Dump {
    /// `Err` when the dump would make the game less restrictive than tg: an access helper with
    /// an unreadable `get_access()`.
    pub fn ensure_complete(&self) -> anyhow::Result<()> {
        if !self.unparsed_access_helpers.is_empty() {
            bail!(
                "access helpers with a get_access() body that is not `var/list/x = ..()`, \
                 `x += \"a\"`/`x += list(...)`, `return x` (their doors would open for \
                 everyone): {:?}",
                self.unparsed_access_helpers
            );
        }
        Ok(())
    }
}

/// Per var name: how many types got `{ "expr": ... }` for it. The pseudo-var `get_access` counts
/// access helper procs with statements [`get_access_adds`] does not understand,
/// `new_access_flags` trim `New()`s that use access flags but [`new_access_flag_adds`] cannot
/// read, `access_flags` an unreadable [`ACCESS_FLAGS_PROC`].
pub type NonConstStats = BTreeMap<&'static str, usize>;

pub fn extract(env: &Environment) -> (Dump, NonConstStats) {
    let mut sources = SourceCache::default();
    let mut stats = NonConstStats::new();
    let mut types = BTreeMap::new();
    let mut unparsed_access_helpers = Vec::new();
    for ty in env.tree.iter_types() {
        if ty.is_root() {
            continue;
        }
        let entry = type_entry(
            env,
            ty,
            &mut sources,
            &mut stats,
            &mut unparsed_access_helpers,
        );
        types.insert(ty.get().path.clone(), entry);
    }
    let element_calls = element_calls(env, &mut sources);
    let populate = populate::populate(env, &mut sources);
    let access_flags = proc_code(env, ACCESS_FLAGS_PROC.0, ACCESS_FLAGS_PROC.1)
        .and_then(access_flag_groups)
        .unwrap_or_else(|| {
            *stats.entry("access_flags").or_default() += 1;
            BTreeMap::new()
        });
    let mut globals = BTreeMap::new();
    for name in DUMPED_GLOBALS {
        match global_init(env, name) {
            Some(value) => {
                globals.insert(name, value);
            }
            None => *stats.entry("globals").or_default() += 1,
        }
    }
    let mut reaction_factors = BTreeMap::new();
    for ty in env.tree.iter_types() {
        let path = &ty.get().path;
        if !path.starts_with("/datum/gas_reaction/standard/") {
            continue;
        }
        let Some(code) = proc_code(env, path, "init_factors") else {
            continue;
        };
        match assigned_factor_list(env, code) {
            Some(value) => {
                reaction_factors.insert(path.clone(), value);
            }
            None => *stats.entry("reaction_factors").or_default() += 1,
        }
    }
    let dump = Dump {
        schema: SCHEMA,
        reaction_factors,
        vars: DUMPED_VARS.to_vec(),
        types,
        element_calls,
        populate,
        access_flags,
        globals,
        unparsed_access_helpers,
    };
    (dump, stats)
}

fn type_entry(
    env: &Environment,
    ty: TypeRef<'_>,
    sources: &mut SourceCache,
    stats: &mut NonConstStats,
    unparsed_access_helpers: &mut Vec<String>,
) -> TypeEntry {
    let mut vars = BTreeMap::new();
    for name in DUMPED_VARS {
        if ty.get_var_declaration(name).is_none() {
            continue; // e.g. `/datum` has no `icon`
        }
        let value = match ty.get_value(name) {
            Some(value) => var_value(env, value, sources),
            None => Ok(Value::Null),
        };
        let value = value.unwrap_or_else(|expr| {
            *stats.entry(name).or_default() += 1;
            expr
        });
        vars.insert(name, value);
    }
    let declared_vars = ty
        .get()
        .vars
        .iter()
        .filter(|(_, v)| !v.value.location.is_builtins())
        .map(|(k, v)| {
            let value = var_value(env, &v.value, sources)
                .or_else(|expr| static_list(ty, k, &v.value).ok_or(expr))
                .unwrap_or_else(|expr| expr);
            (k.clone(), value)
        })
        .collect();
    let procs = ty
        .get()
        .procs
        .iter()
        .filter(|(_, p)| p.value.iter().any(|v| !v.location.is_builtins()))
        .map(|(name, _)| name.clone())
        .collect();
    let path = &ty.get().path;
    let get_access = if path.starts_with(ACCESS_HELPER_ROOT) {
        let code = ty
            .get()
            .procs
            .get("get_access")
            .and_then(|p| p.value.last())
            .and_then(|v| v.code.as_deref());
        match code.map(get_access_adds) {
            Some(Some(adds)) => adds,
            Some(None) => {
                *stats.entry("get_access").or_default() += 1;
                unparsed_access_helpers.push(path.clone());
                Vec::new()
            }
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let new_access_flags = if path.starts_with(ID_TRIM_ROOT) {
        match proc_code(env, path, "New") {
            Some(code) => new_access_flag_adds(code).unwrap_or_else(|| {
                if format!("{code:?}").contains("get_flag_access_list") {
                    *stats.entry("new_access_flags").or_default() += 1;
                }
                BTreeMap::new()
            }),
            None => BTreeMap::new(),
        }
    } else {
        BTreeMap::new()
    };
    TypeEntry {
        parent: ty.parent_type_without_root().map(|p| p.get().path.clone()),
        vars,
        declared_vars,
        procs,
        get_access,
        new_access_flags,
    }
}

/// The body of `proc` as `path` itself defines it (not inherited).
fn proc_code<'a>(
    env: &'a Environment,
    path: &str,
    proc: &str,
) -> Option<&'a [dm::ast::Spanned<dm::ast::Statement>]> {
    env.tree
        .find(path)?
        .get()
        .procs
        .get(proc)?
        .value
        .last()?
        .code
        .as_deref()
}

/// `GLOBAL_LIST_INIT(name, value)` / `GLOBAL_VAR_INIT`: the constant `value` of the
/// `/datum/controller/global_vars/proc/InitGlobal<name>` the macro writes (`name = value`).
fn global_init(env: &Environment, name: &str) -> Option<Value> {
    use dm::ast::{AssignOp, Expression, Statement};
    let code = proc_code(
        env,
        "/datum/controller/global_vars",
        &format!("InitGlobal{name}"),
    )?;
    code.iter().find_map(|s| match &s.elem {
        Statement::Expr(Expression::AssignOp {
            op: AssignOp::Assign,
            rhs,
            ..
        }) => {
            let c = rhs.as_ref().clone().simple_evaluate(s.location).ok()?;
            constant_to_json(&c)
        }
        _ => None,
    })
}

/// `factor = list(<gas path or text> = "<text>", ...)` of an `init_factors()`: SpacemanDMM does
/// not fold strings that interpolate `[/datum/gas/x::name]`, so the texts are put together here
/// ([`interp_text`]). `{ "list": [{ "assoc": [key, text] }] }`, keys as `{ "path": ... }` or text.
fn assigned_factor_list(
    env: &Environment,
    code: &[dm::ast::Spanned<dm::ast::Statement>],
) -> Option<Value> {
    use dm::ast::{AssignOp, Expression, Statement, Term};
    let (rhs, loc) = code.iter().find_map(|s| match &s.elem {
        Statement::Expr(Expression::AssignOp {
            op: AssignOp::Assign,
            lhs,
            rhs,
        }) if matches!(lhs.as_ref(), Expression::Base { term, follow }
            if follow.is_empty() && matches!(&term.elem, Term::Ident(i) if i.as_str() == "factor")) =>
        {
            Some((rhs.as_ref(), s.location))
        }
        _ => None,
    })?;
    let Expression::Base { term, follow } = rhs else {
        return None;
    };
    let Term::List(items) = &term.elem else {
        return None;
    };
    if !follow.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for item in items.iter() {
        let Expression::AssignOp {
            op: AssignOp::Assign,
            lhs,
            rhs,
        } = item
        else {
            return None;
        };
        let key = match lhs.as_ref().clone().simple_evaluate(loc).ok()? {
            Constant::Prefab(pop) => json!({ "path": pop.to_string() }),
            Constant::String(s) => Value::String(s.to_string()),
            _ => return None,
        };
        out.push(json!({ "assoc": [key, interp_text(env, rhs, loc)?] }));
    }
    Some(json!({ "list": out }))
}

/// A text expression: a constant, or an interpolated string whose parts are constants or
/// `/type::var` initial values.
fn interp_text(env: &Environment, e: &dm::ast::Expression, loc: dm::Location) -> Option<String> {
    use dm::ast::{Expression, Follow, Term};
    if let Ok(c) = e.clone().simple_evaluate(loc) {
        return constant_text(&c);
    }
    let Expression::Base { term, follow } = e else {
        return Some(dm_num2text(interp_number(env, e, loc)?));
    };
    match (&term.elem, follow.as_ref()) {
        (Term::InterpString(first, parts), []) => {
            let mut s = first.to_string();
            for (expr, lit) in parts.iter() {
                if let Some(expr) = expr {
                    s.push_str(&interp_text(env, expr, loc)?);
                }
                s.push_str(lit);
            }
            Some(s)
        }
        (Term::Prefab(prefab), [f]) => {
            let Follow::StaticField(var) = &f.elem else {
                return None;
            };
            let path: String = prefab
                .path
                .iter()
                .map(|(_, seg)| format!("/{seg}"))
                .collect();
            let value = env.tree.find(&path)?.get_value(var.as_str())?;
            constant_text(value.constant.as_ref()?)
        }
        (Term::Expr(inner), []) => interp_text(env, inner, loc),
        _ => Some(dm_num2text(interp_number(env, e, loc)?)),
    }
}

/// Arithmetic over constants and `SSxxx.var` initial values (`SSair.wait`, which the
/// handbook divides by): SpacemanDMM does not fold those.
fn interp_number(env: &Environment, e: &dm::ast::Expression, loc: dm::Location) -> Option<f64> {
    use dm::ast::{BinaryOp, Expression, Follow, Term};
    if let Ok(Constant::Float(f)) = e.clone().simple_evaluate(loc) {
        return Some(f64::from(f));
    }
    match e {
        Expression::BinaryOp { op, lhs, rhs } => {
            let (a, b) = (interp_number(env, lhs, loc)?, interp_number(env, rhs, loc)?);
            match op {
                BinaryOp::Add => Some(a + b),
                BinaryOp::Sub => Some(a - b),
                BinaryOp::Mul => Some(a * b),
                BinaryOp::Div => (b != 0.0).then(|| a / b),
                _ => None,
            }
        }
        Expression::Base { term, follow } => match (&term.elem, follow.as_ref()) {
            (Term::Expr(inner), []) => interp_number(env, inner, loc),
            (Term::Ident(name), [f]) if name.starts_with("SS") => {
                let Follow::Field(_, var) = &f.elem else {
                    return None;
                };
                let ty = format!("/datum/controller/subsystem/{}", name[2..].to_lowercase());
                match env
                    .tree
                    .find(&ty)?
                    .get_value(var.as_str())?
                    .constant
                    .as_ref()?
                {
                    Constant::Float(f) => Some(f64::from(*f)),
                    _ => None,
                }
            }
            _ => None,
        },
        _ => None,
    }
}

/// How DM writes a constant into a string: numbers like `num2text` (6 significant digits).
fn constant_text(c: &Constant) -> Option<String> {
    match c {
        Constant::String(s) => Some(s.to_string()),
        Constant::Float(f) => Some(dm_num2text(f64::from(*f))),
        Constant::Prefab(pop) => Some(pop.to_string()),
        _ => None,
    }
}

/// BYOND `num2text(x)`: `%g` with 6 significant digits (`373.15`, `1e+06`).
pub fn dm_num2text(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    let exp = x.abs().log10().floor() as i32;
    if !(-5..6).contains(&exp) {
        let mantissa = x / 10f64.powi(exp);
        let m = format!("{mantissa:.5}");
        let m = m.trim_end_matches('0').trim_end_matches('.');
        let sign = if exp < 0 { '-' } else { '+' };
        return format!("{m}e{sign}{:02}", exp.abs());
    }
    let decimals = (5 - exp).max(0) as usize;
    let s = format!("{x:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s
    }
}

/// A constant integer expression (macros are expanded: `(1 << 3)`).
fn const_int(e: &dm::ast::Expression) -> Option<i64> {
    match e.clone().simple_evaluate(dm::Location::builtins()).ok()? {
        Constant::Float(f) if f.fract() == 0.0 => Some(f as i64),
        _ => None,
    }
}

/// `. = ..()` / `..()`: the parent call every override starts with.
fn is_parent_call(e: &dm::ast::Expression) -> bool {
    use dm::ast::{AssignOp, Expression, Term};
    match e {
        Expression::AssignOp {
            op: AssignOp::Assign,
            rhs,
            ..
        } => matches!(rhs.as_term(), Some(Term::ParentCall(_))),
        _ => matches!(e.as_term(), Some(Term::ParentCall(_))),
    }
}

/// A trim `New()`: `list_var |= (SSid_access.get_flag_access_list(F) + ...)` lines (`|=` or
/// `+=`) and the parent call (`..()`, `. = ..()`, `return ..()`). `None` for anything else.
pub fn new_access_flag_adds(
    code: &[dm::ast::Spanned<dm::ast::Statement>],
) -> Option<BTreeMap<String, Vec<i64>>> {
    use dm::ast::{AssignOp, BinaryOp, Expression, Follow, Statement, Term};
    fn flags(e: &Expression, out: &mut Vec<i64>) -> Option<()> {
        match e {
            Expression::BinaryOp {
                op: BinaryOp::Add,
                lhs,
                rhs,
            } => {
                flags(lhs, out)?;
                flags(rhs, out)
            }
            Expression::Base { term, follow } => match (&term.elem, &follow[..]) {
                (Term::Expr(inner), []) => flags(inner, out),
                (Term::Ident(_), [call]) => match &call.elem {
                    Follow::Call(_, name, args)
                        if name.as_str() == "get_flag_access_list" && args.len() == 1 =>
                    {
                        out.push(const_int(&args[0])?);
                        Some(())
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }
    let mut adds: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for statement in code {
        match &statement.elem {
            Statement::Expr(e) if is_parent_call(e) => {}
            Statement::Return(Some(e)) if is_parent_call(e) => {}
            Statement::Expr(Expression::AssignOp {
                op: AssignOp::BitOrAssign | AssignOp::AddAssign,
                lhs,
                rhs,
            }) => {
                let Some(Term::Ident(var)) = lhs.as_term() else {
                    return None;
                };
                flags(rhs, adds.entry(var.clone()).or_default())?;
            }
            _ => return None,
        }
    }
    Some(adds)
}

/// [`ACCESS_FLAGS_PROC`]: `accesses_by_flag["[FLAG]"] = list(...)` lines; the loops filling the
/// reverse map and other assignments are skipped. `None` if no group was read or a group line
/// has another shape.
pub fn access_flag_groups(
    code: &[dm::ast::Spanned<dm::ast::Statement>],
) -> Option<BTreeMap<String, Vec<String>>> {
    use dm::ast::{AssignOp, Expression, Follow, Statement, Term};
    let mut groups = BTreeMap::new();
    for statement in code {
        let Statement::Expr(Expression::AssignOp {
            op: AssignOp::Assign,
            lhs,
            rhs,
        }) = &statement.elem
        else {
            continue;
        };
        let Expression::Base { term, follow } = &**lhs else {
            continue;
        };
        if !matches!(&term.elem, Term::Ident(name) if name == "accesses_by_flag") {
            continue;
        }
        let [index] = &follow[..] else {
            return None;
        };
        let Follow::Index(_, key) = &index.elem else {
            return None;
        };
        let flag = match key.as_term()? {
            Term::InterpString(prefix, parts) if prefix.is_empty() => match &parts[..] {
                [(Some(e), suffix)] if suffix.is_empty() => const_int(e)?,
                _ => return None,
            },
            _ => return None,
        };
        let Ok(Constant::List(items)) = rhs.clone().simple_evaluate(dm::Location::builtins())
        else {
            return None;
        };
        let accesses = items
            .iter()
            .map(|(k, _)| match k {
                Constant::String(s) => Some(s.to_string()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        groups.insert(flag.to_string(), accesses);
    }
    (!groups.is_empty()).then_some(groups)
}

/// The strings an access helper's `get_access()` appends: its body must be
/// `var/list/x = ..()` (or `list()` at the root), then `x += "a"` / `x += list("a", ...)` lines, then `return x`
/// (macros are already expanded). `None` for anything else, so a helper with logic is refused
/// ([`Dump::ensure_complete`]) instead of half-read.
pub fn get_access_adds(code: &[dm::ast::Spanned<dm::ast::Statement>]) -> Option<Vec<String>> {
    use dm::ast::{AssignOp, Expression, Statement, Term};
    let string = |e: &Expression| match e.as_term() {
        Some(Term::String(s)) => Some(s.clone()),
        _ => None,
    };
    let mut adds = Vec::new();
    for statement in code {
        match &statement.elem {
            Statement::Var(v)
                if matches!(
                    v.value.as_ref().and_then(Expression::as_term),
                    Some(Term::ParentCall(args) | Term::List(args)) if args.is_empty()
                ) => {}
            Statement::Return(Some(e)) if matches!(e.as_term(), Some(Term::Ident(_))) => {}
            Statement::Expr(Expression::AssignOp {
                op: AssignOp::AddAssign,
                lhs,
                rhs,
            }) if matches!(lhs.as_term(), Some(Term::Ident(_))) => match rhs.as_term() {
                Some(Term::List(items)) => {
                    for item in items.iter() {
                        adds.push(string(item)?);
                    }
                }
                _ => adds.push(string(rhs)?),
            },
            _ => return None,
        }
    }
    Some(adds)
}

/// Finds [`SCANNED_ELEMENTS`] calls by text inside proc bodies (the parser keeps each body's
/// source range), so the owner type and proc come from the object tree, not from guessing at
/// indentation.
fn element_calls(
    env: &Environment,
    sources: &mut SourceCache,
) -> BTreeMap<&'static str, Vec<ElementCall>> {
    // file -> (first line, last line, owner, proc) of every proc body in it.
    let mut bodies: HashMap<dm::FileId, Vec<(u32, u32, String, String)>> = HashMap::new();
    for ty in env.tree.iter_types() {
        for (name, proc) in &ty.get().procs {
            for value in &proc.value {
                let Some(range) = &value.body_range else {
                    continue;
                };
                if value.location.is_builtins() {
                    continue;
                }
                bodies.entry(range.start.file).or_default().push((
                    range.start.line,
                    range.end.line,
                    ty.get().path.clone(),
                    name.clone(),
                ));
            }
        }
    }
    let mut out: BTreeMap<&'static str, Vec<ElementCall>> =
        SCANNED_ELEMENTS.iter().map(|e| (*e, Vec::new())).collect();
    let mut files: Vec<_> = bodies.into_iter().collect();
    files.sort_by_key(|(file, _)| *file);
    for (file, procs) in files {
        let lines = sources.lines(&env.context, env.root(), file);
        for (i, line) in lines.iter().enumerate() {
            let code = line.split_once("//").map_or(line.as_str(), |(c, _)| c);
            if !code.contains("Element(") && !code.contains("Component(") {
                continue;
            }
            let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
            for element in SCANNED_ELEMENTS {
                for call in ["AddElement", "RemoveElement", "AddComponent"] {
                    if !compact.contains(&format!("{call}({element}")) {
                        continue;
                    }
                    let line_no = i as u32 + 1;
                    // The parser ends the last body of a file before its last line: fall back
                    // to the nearest body starting above (proc definitions do not nest).
                    let Some((_, _, owner, proc)) = procs
                        .iter()
                        .find(|(start, end, ..)| (*start..=*end).contains(&line_no))
                        .or_else(|| {
                            procs
                                .iter()
                                .filter(|(start, ..)| *start <= line_no)
                                .max_by_key(|(start, ..)| *start)
                        })
                    else {
                        continue; // text above every proc of the file
                    };
                    out.get_mut(element).expect("seeded").push(ElementCall {
                        owner: owner.clone(),
                        proc: proc.clone(),
                        call,
                        line: line.trim().to_owned(),
                    });
                }
            }
        }
    }
    out
}

/// `Err` carries the `{ "expr": ... }` fallback for values that are not plain data.
fn var_value(
    env: &Environment,
    value: &VarValue,
    sources: &mut SourceCache,
) -> Result<Value, Value> {
    let expr_fallback = |sources: &mut SourceCache| {
        let text = sources.expression_text(&env.context, env.root(), value.location);
        json!({ "expr": text })
    };
    match (&value.constant, &value.expression) {
        (Some(c), _) => constant_to_json(c).ok_or_else(|| expr_fallback(sources)),
        (None, None) => Ok(Value::Null),
        (None, Some(_)) => Err(expr_fallback(sources)),
    }
}

/// A `var/static/list` initializer of plain data (`list(/datum/reagent/water, ...)`): the
/// object tree leaves static vars unevaluated, and some hold the only copy of tg data (the
/// chem dispensers' reagent lists).
fn static_list(ty: TypeRef<'_>, name: &str, value: &VarValue) -> Option<Value> {
    let decl = ty.get_var_declaration(name)?;
    if !decl.var_type.flags.is_static() {
        return None;
    }
    let constant = value
        .expression
        .clone()?
        .simple_evaluate(value.location)
        .ok()?;
    matches!(constant, Constant::List(_))
        .then(|| constant_to_json(&constant))
        .flatten()
}

/// Plain data → JSON; `None` for `new`, constant-function calls and non-finite numbers.
pub fn constant_to_json(c: &Constant) -> Option<Value> {
    Some(match c {
        Constant::Null(_) => Value::Null,
        Constant::Float(f) => number(*f)?,
        Constant::String(s) => Value::String(s.to_string()),
        Constant::Resource(s) => json!({ "resource": s.to_string() }),
        Constant::Prefab(pop) => json!({ "path": pop.to_string() }),
        Constant::List(items) => {
            let items = items
                .iter()
                .map(|(k, v)| {
                    let k = constant_to_json(k)?;
                    Some(match v {
                        Some(v) => json!({ "assoc": [k, constant_to_json(v)?] }),
                        None => k,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            json!({ "list": items })
        }
        Constant::New { .. } | Constant::Call(..) => return None,
    })
}

/// DM numbers are f32; print them the way f32 prints (`0.1`, not `0.100000001`), integers without `.0`.
fn number(f: f32) -> Option<Value> {
    if !f.is_finite() {
        return None;
    }
    if f.fract() == 0.0 && f.abs() < 9.0e15 {
        return Some(Value::from(f as i64));
    }
    let wide: f64 = f.to_string().parse().ok()?;
    serde_json::Number::from_f64(wide).map(Value::Number)
}

/// Reads the right-hand side of a var assignment from the source file, for `{ "expr": ... }`.
#[derive(Default)]
struct SourceCache {
    files: HashMap<PathBuf, Option<Vec<String>>>,
}

impl SourceCache {
    fn lines(&mut self, context: &dm::Context, root: &Path, file: dm::FileId) -> &[String] {
        let path = root.join(&*context.file_path(file));
        self.files
            .entry(path)
            .or_insert_with_key(|p| {
                std::fs::read(p).ok().map(|b| {
                    String::from_utf8_lossy(&b)
                        .lines()
                        .map(str::to_owned)
                        .collect()
                })
            })
            .as_deref()
            .unwrap_or(&[])
    }

    fn expression_text(&mut self, context: &dm::Context, root: &Path, loc: dm::Location) -> String {
        // File ids hold paths as written in `#include`, relative to the .dme.
        let path = root.join(&*context.file_path(loc.file));
        let lines = self.files.entry(path).or_insert_with_key(|p| {
            std::fs::read(p).ok().map(|b| {
                String::from_utf8_lossy(&b)
                    .lines()
                    .map(str::to_owned)
                    .collect()
            })
        });
        // shortcut: only the first source line of a multi-line expression; enough to inspect by eye.
        let line = lines
            .as_ref()
            .and_then(|l| l.get(loc.line.saturating_sub(1) as usize))
            .map(String::as_str)
            .unwrap_or("");
        let from_var = line
            .char_indices()
            .nth(loc.column.saturating_sub(1) as usize)
            .map_or(line, |(i, _)| &line[i..]);
        let rhs = from_var.split_once('=').map_or(from_var, |(_, r)| r);
        let rhs = rhs.split_once("//").map_or(rhs, |(r, _)| r);
        rhs.trim().to_owned()
    }
}

/// Type paths of every prefab in a `.dmm` (TGM or classic). Only the dictionary part is read:
/// paths that open a prefab entry, i.e. come right after `(` or `,` at paren depth 1, outside
/// `{...}` var edits and strings.
pub fn dmm_type_paths(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut chars = text.char_indices().peekable();
    let mut quote: Option<char> = None;
    let mut braces = 0u32;
    let mut parens = 0u32;
    let mut prev_significant = ' ';
    while let Some((i, c)) = chars.next() {
        if let Some(q) = quote {
            if c == '\\' {
                chars.next();
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '{' => braces += 1,
            '}' => braces = braces.saturating_sub(1),
            _ if braces > 0 => {}
            '(' => parens += 1,
            ')' => parens = parens.saturating_sub(1),
            '/' if parens == 1 && matches!(prev_significant, '(' | ',') => {
                let mut end = i + 1;
                while let Some(&(j, n)) = chars.peek() {
                    if !(n.is_ascii_alphanumeric() || n == '_' || n == '/') {
                        break;
                    }
                    end = j + n.len_utf8();
                    chars.next();
                }
                out.insert(text[i..end].to_owned());
            }
            _ => {}
        }
        if !c.is_whitespace() {
            prev_significant = c;
        }
    }
    out
}

pub struct MapCheck {
    pub maps: Vec<PathBuf>,
    /// Every distinct type path used by the maps.
    pub used: BTreeSet<String>,
    /// Used paths that are not in the object tree, with one map that uses each.
    pub unresolved: BTreeMap<String, PathBuf>,
}

/// Scans every `.dmm` under `dirs` (recursively) and resolves its type paths in `tree`.
pub fn check_maps(tree: &ObjectTree, dirs: &[PathBuf]) -> anyhow::Result<MapCheck> {
    let mut maps = Vec::new();
    for dir in dirs {
        collect_dmm(dir, &mut maps).with_context(|| format!("scanning {}", dir.display()))?;
    }
    maps.sort();
    let mut used = BTreeSet::new();
    let mut unresolved = BTreeMap::new();
    for map in &maps {
        let bytes = std::fs::read(map).with_context(|| format!("reading {}", map.display()))?;
        for path in dmm_type_paths(&String::from_utf8_lossy(&bytes)) {
            if tree.find(&path).is_none() {
                unresolved
                    .entry(path.clone())
                    .or_insert_with(|| map.clone());
            }
            used.insert(path);
        }
    }
    Ok(MapCheck {
        maps,
        used,
        unresolved,
    })
}

fn collect_dmm(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_dmm(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "dmm") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dmm_paths_tgm_and_classic() {
        let tgm = "//MAP CONVERTED BY dmm2tgm.py THIS HEADER COMMENT PREVENTS RECONVERSION, DO NOT REMOVE\n\
            \"a\" = (\n/obj/machinery/door/airlock{\n\tname = \"x/y (z)\";\n\tid = /obj/not_a_prefab\n\t},\n/turf/open/floor,\n/area/station/a_b)\n\
            \"b\" = (\n/turf/closed/wall)\n\n(1,1,1) = {\"\na\nb\n\"}\n";
        let classic =
            "\"aa\" = (/obj/a{dir = 4; icon = 'i/j.dmi'},/turf/b,/area/c)\n(1,1,1) = {\"\naa\n\"}";
        let names = |s| dmm_type_paths(s).into_iter().collect::<Vec<_>>();
        assert_eq!(
            names(tgm),
            [
                "/area/station/a_b",
                "/obj/machinery/door/airlock",
                "/turf/closed/wall",
                "/turf/open/floor"
            ]
        );
        assert_eq!(names(classic), ["/area/c", "/obj/a", "/turf/b"]);
    }

    #[test]
    fn numbers_print_like_f32() {
        assert_eq!(number(2.7).unwrap().to_string(), "2.7");
        assert_eq!(number(10002.0).unwrap().to_string(), "10002");
        assert_eq!(number(-4.0).unwrap().to_string(), "-4");
        assert!(number(f32::INFINITY).is_none());
    }
}

#[cfg(test)]
mod factor_text_tests {
    use super::dm_num2text;

    /// BYOND `num2text`: 6 significant digits, exponent form from 1e6.
    #[test]
    fn numbers_read_like_dm() {
        assert_eq!(dm_num2text(373.15), "373.15");
        assert_eq!(dm_num2text(40.0), "40");
        assert_eq!(dm_num2text(0.01), "0.01");
        assert_eq!(dm_num2text(1e6), "1e+06");
        assert_eq!(dm_num2text(1_643.15), "1643.15");
    }
}
