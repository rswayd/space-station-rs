//! What a container type spawns into itself: the `new /obj/...(src)` calls of its
//! `PopulateContents()` / `populate_contents_immediate()` (tg closets, crates, storage items),
//! lowered from the proc AST into a small static program ([`Fill`]).
//!
//! Recognized: `new /path(src, ...)` (also `SSwardrobe.provide_type(/path, src)` and
//! `var/x = new /path(src)`), `generate_items_inside(list(/path = n, ...), src)` with a literal list
//! or a local holding one, `for(var/i in a to b)` with constant bounds, `if(prob(n))`, `if(var)` /
//! `if(!var)` on a type var (the consumer resolves it per type), `switch` on `pick_weight(list(...))`,
//! `pick(...)` or `rand(a, b)` with constant cases, `new T(src)` where the local `T` holds a
//! `pick`/`pick_weight` of paths, `..()` and `return`. Statements that may spawn something but fit
//! none of these become [`Fill::Unsupported`] with their source line; statements without `new`
//! or a call (renames, icon changes) are dropped.

use std::collections::{BTreeMap, HashMap, HashSet};

use dreammaker::ast::{
    AssignOp, Case, Expression, Follow, FormatTypePath, Spanned, Statement, Term, UnaryOp,
};
use serde::Serialize;

use crate::{Environment, SourceCache};

/// Procs whose bodies are lowered.
pub const POPULATE_PROCS: [&str; 2] = ["PopulateContents", "populate_contents_immediate"];

/// Calls that never spawn anything; anything else with a call in it is suspicious.
const HARMLESS_CALLS: [&str; 13] = [
    "isnull",
    "pass",
    "update_appearance",
    "update_icon",
    "update_icon_state",
    "update_overlays",
    "prob",
    "rand",
    "pick_weight",
    "length",
    "istype",
    "ispath",
    "set_greyscale",
];

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Fill {
    /// One instance of the type inside the container.
    New(String),
    /// `new type_var(src)`: the type held by a var of the container type (`null`: nothing).
    NewVar(String),
    Repeat {
        count: u32,
        body: Vec<Fill>,
    },
    /// `for(var/i in 1 to type_var)`: as many times as a var of the container type says.
    RepeatVar {
        var: String,
        body: Vec<Fill>,
    },
    Prob {
        percent: f32,
        then: Vec<Fill>,
        otherwise: Vec<Fill>,
    },
    /// One arm, by weight.
    Pick {
        arms: Vec<(f32, Vec<Fill>)>,
    },
    /// `if(var)`: truthiness of a var of the container type.
    IfVar {
        var: String,
        then: Vec<Fill>,
        otherwise: Vec<Fill>,
    },
    /// `..()`: the parent type's body of the same proc.
    Parent,
    /// Ends the proc.
    Return,
    /// A statement that may spawn something but was not understood (its source line).
    Unsupported(String),
}

/// Type path -> proc name -> its program, for every type that defines one of
/// [`POPULATE_PROCS`] itself.
pub type Populate = BTreeMap<String, BTreeMap<&'static str, Vec<Fill>>>;

pub(crate) fn populate(env: &Environment, sources: &mut SourceCache) -> Populate {
    let mut out = Populate::new();
    for ty in env.tree.iter_types() {
        for name in POPULATE_PROCS {
            let Some(proc) = ty.get().procs.get(name) else {
                continue;
            };
            let Some(value) = proc.value.last() else {
                continue;
            };
            if value.location.is_builtins() {
                continue;
            }
            let Some(code) = &value.code else {
                continue;
            };
            let mut lower = Lower {
                env,
                sources,
                locals: HashMap::new(),
                created: HashSet::new(),
            };
            let fill = lower.block(code);
            out.entry(ty.get().path.clone())
                .or_default()
                .insert(name, fill);
        }
    }
    out
}

#[derive(Clone)]
enum Local {
    /// Weighted type paths (a single path has one entry).
    Paths(Vec<(f32, String)>),
    /// `list(/path = count, ...)`.
    Counts(Vec<(String, u32)>),
    /// `type_var` or `pick(type_var)`: whatever a var of the container type holds.
    TypeVar(String),
}

struct Lower<'a> {
    env: &'a Environment,
    sources: &'a mut SourceCache,
    locals: HashMap<String, Local>,
    /// Locals holding an atom this proc created.
    created: HashSet<String>,
}

impl Lower<'_> {
    fn block(&mut self, block: &[Spanned<Statement>]) -> Vec<Fill> {
        let mut out = Vec::new();
        for statement in block {
            self.statement(statement, &mut out);
        }
        out
    }

    fn unsupported(&mut self, location: dreammaker::Location) -> Fill {
        let lines = self
            .sources
            .lines(&self.env.context, self.env.root(), location.file);
        let line = lines
            .get(location.line.saturating_sub(1) as usize)
            .map_or("", |l| l.trim());
        Fill::Unsupported(line.to_owned())
    }

    fn statement(&mut self, statement: &Spanned<Statement>, out: &mut Vec<Fill>) {
        let location = statement.location;
        match &statement.elem {
            // `thing.set_color(...)` on an atom this proc created: changes it, spawns nothing.
            Statement::Expr(Expression::Base { term, follow })
                if !follow.is_empty()
                    && matches!(&term.elem, Term::Ident(n) if self.created.contains(n))
                    && !follow.iter().any(|f| match &f.elem {
                        Follow::Call(_, _, args) => args.iter().any(has_new),
                        _ => false,
                    }) => {}
            Statement::Expr(e) => match self.expr_statement(e) {
                Some(fills) => out.extend(fills),
                None if spawns(e) => out.push(self.unsupported(location)),
                None => {}
            },
            Statement::Return(value) => {
                if value.as_ref().is_some_and(spawns) {
                    out.push(self.unsupported(location));
                }
                out.push(Fill::Return);
            }
            Statement::Var(var) => self.var(var, location, out),
            Statement::Vars(vars) => {
                for var in vars {
                    self.var(var, location, out);
                }
            }
            Statement::ForRange(range) => {
                let bounds = (int(&range.start), int(&range.end));
                let step_one = range.step.as_ref().is_none_or(|s| int(s) == Some(1));
                match bounds {
                    (Some(a), Some(b)) if step_one => {
                        let body = self.block(&range.block);
                        let count = (b - a + 1).max(0) as u32;
                        if count > 0 && !body.is_empty() {
                            out.push(Fill::Repeat { count, body });
                        }
                    }
                    (Some(1), None) if step_one => {
                        match type_var(&range.end).filter(|v| !self.locals.contains_key(v)) {
                            Some(var) => {
                                let body = self.block(&range.block);
                                if !body.is_empty() {
                                    out.push(Fill::RepeatVar { var, body });
                                }
                            }
                            None => self.opaque_block(&range.block, location, out),
                        }
                    }
                    _ => self.opaque_block(&range.block, location, out),
                }
            }
            Statement::If { arms, else_arm } => {
                let fill = self.if_chain(arms, else_arm.as_deref(), location);
                out.extend(fill);
            }
            Statement::Switch {
                input,
                cases,
                default,
            } => match self.switch(input, cases, default.as_deref()) {
                Some(fill) => out.push(fill),
                None => self.opaque_switch(cases, default.as_deref(), location, out),
            },
            Statement::ForList(f) => self.opaque_block(&f.block, location, out),
            Statement::ForKeyValue(f) => self.opaque_block(&f.block, location, out),
            Statement::ForLoop { block, .. }
            | Statement::While { block, .. }
            | Statement::DoWhile { block, .. }
            | Statement::ForInfinite { block }
            | Statement::Spawn { block, .. } => self.opaque_block(block, location, out),
            _ => {}
        }
    }

    /// A loop or branch that cannot be modelled: unsupported only if its body spawns.
    fn opaque_block(
        &mut self,
        block: &[Spanned<Statement>],
        location: dreammaker::Location,
        out: &mut Vec<Fill>,
    ) {
        let inner = self.block(block);
        if has_spawn(&inner) {
            out.push(self.unsupported(location));
        }
    }

    fn opaque_switch(
        &mut self,
        cases: &[(Spanned<Vec<Case>>, Box<[Spanned<Statement>]>)],
        default: Option<&[Spanned<Statement>]>,
        location: dreammaker::Location,
        out: &mut Vec<Fill>,
    ) {
        let mut any = false;
        for (_, block) in cases {
            any |= has_spawn(&self.block(block));
        }
        if let Some(block) = default {
            any |= has_spawn(&self.block(block));
        }
        if any {
            out.push(self.unsupported(location));
        }
    }

    fn var(
        &mut self,
        var: &dreammaker::ast::VarStatement,
        location: dreammaker::Location,
        out: &mut Vec<Fill>,
    ) {
        let name = var.name.as_str();
        self.locals.remove(name);
        self.created.remove(name);
        let Some(value) = &var.value else {
            return;
        };
        if let Some(local) = self.local_value(value) {
            self.locals.insert(name.to_owned(), local);
            return;
        }
        // `var/obj/item/x = new(src, ...)`: the declared type.
        if let Some(Term::NewImplicit { args }) = unparen(value)
            && into_src(args.as_deref()).is_some()
        {
            let declared = format!("/{}", var.var_type.type_path.join("/"));
            if self.env.tree.find(&declared).is_some() && declared.starts_with("/obj/") {
                self.created.insert(name.to_owned());
                out.push(Fill::New(declared));
            } else {
                out.push(self.unsupported(location));
            }
            return;
        }
        let fills = self.expr_statement(value);
        if fills
            .as_ref()
            .is_some_and(|f| matches!(f[..], [Fill::New(_) | Fill::NewVar(_)]))
        {
            self.created.insert(name.to_owned());
        }
        match fills {
            Some(fills) => out.extend(fills),
            None if spawns(value) => out.push(self.unsupported(location)),
            None => {}
        }
    }

    /// `pick(...)`, `pick_weight(list(...))`, `list(/a = 1)`, `/path` bound to a local.
    fn local_value(&self, value: &Expression) -> Option<Local> {
        let term = unparen(value)?;
        match term {
            Term::Prefab(p) => Some(Local::Paths(vec![(1.0, self.path(&p.path)?)])),
            Term::Ident(var) if !self.locals.contains_key(var) && type_var(value).is_some() => {
                Some(Local::TypeVar(var.clone()))
            }
            Term::Pick(args)
                if args.len() == 1
                    && args[0].0.is_none()
                    && type_var(&args[0].1).is_some_and(|v| !self.locals.contains_key(&v)) =>
            {
                Some(Local::TypeVar(type_var(&args[0].1)?))
            }
            Term::Pick(args) => {
                let mut paths = Vec::new();
                for (weight, e) in args.iter() {
                    let w = match weight {
                        Some(w) => number(w)?,
                        None => 1.0,
                    };
                    paths.push((w, self.prefab(e)?));
                }
                Some(Local::Paths(paths))
            }
            Term::Call(name, args) if &**name == "pick_weight" && args.len() == 1 => {
                let Some(Term::List(items)) = unparen(&args[0]) else {
                    return None;
                };
                let mut paths = Vec::new();
                for (k, v) in assoc(items)? {
                    paths.push((number(v?)?, self.prefab(k)?));
                }
                Some(Local::Paths(paths))
            }
            Term::List(items) => self.counts(items).map(Local::Counts),
            _ => None,
        }
    }

    fn counts(&self, items: &[Expression]) -> Option<Vec<(String, u32)>> {
        let mut out = Vec::new();
        for (k, v) in assoc(items)? {
            let n = match v {
                Some(v) => int(v)?.max(0) as u32,
                None => 1,
            };
            out.push((self.prefab(k)?, n));
        }
        Some(out)
    }

    /// The fills of a statement-level expression; `None` when not recognized.
    fn expr_statement(&mut self, e: &Expression) -> Option<Vec<Fill>> {
        // `. = ..()`
        if let Expression::AssignOp {
            op: AssignOp::Assign,
            lhs,
            rhs,
        } = e
            && matches!(unparen(lhs), Some(Term::Ident(i)) if i == ".")
        {
            return self.expr_statement(rhs);
        }
        if let Expression::Base { term, follow } = e {
            // `SSwardrobe.provide_type(/path, src)`
            if let (Term::Ident(name), [call]) = (&term.elem, &follow[..])
                && name == "SSwardrobe"
                && let Follow::Call(_, method, args) = &call.elem
                && &**method == "provide_type"
                && args.len() == 2
                && is_src(&args[1])
            {
                return Some(vec![Fill::New(self.prefab(&args[0])?)]);
            }
            if !follow.is_empty() {
                return None;
            }
            return match &term.elem {
                Term::ParentCall(_) => Some(vec![Fill::Parent]),
                Term::NewPrefab { prefab, args } => {
                    into_src(args.as_deref())?;
                    Some(vec![Fill::New(self.path(&prefab.path)?)])
                }
                Term::NewMiniExpr { expr, args } if expr.fields.is_empty() => {
                    into_src(args.as_deref())?;
                    let Some(local) = self.locals.get(&*expr.ident) else {
                        return (&*expr.ident != "src")
                            .then(|| vec![Fill::NewVar(expr.ident.to_string())]);
                    };
                    match local {
                        Local::Paths(paths) if paths.len() == 1 => {
                            Some(vec![Fill::New(paths[0].1.clone())])
                        }
                        Local::Paths(paths) => Some(vec![Fill::Pick {
                            arms: paths
                                .iter()
                                .map(|(w, p)| (*w, vec![Fill::New(p.clone())]))
                                .collect(),
                        }]),
                        Local::TypeVar(var) => Some(vec![Fill::NewVar(var.clone())]),
                        Local::Counts(_) => None,
                    }
                }
                Term::Call(name, args) if &**name == "generate_items_inside" && args.len() == 2 => {
                    if !is_src(&args[1]) {
                        return None;
                    }
                    let counts = match unparen(&args[0])? {
                        Term::List(items) => self.counts(items)?,
                        Term::Ident(local) => match self.locals.get(local)? {
                            Local::Counts(c) => c.clone(),
                            Local::Paths(_) | Local::TypeVar(_) => return None,
                        },
                        _ => return None,
                    };
                    Some(
                        counts
                            .into_iter()
                            .filter(|(_, n)| *n > 0)
                            .map(|(path, n)| match n {
                                1 => Fill::New(path),
                                count => Fill::Repeat {
                                    count,
                                    body: vec![Fill::New(path)],
                                },
                            })
                            .collect(),
                    )
                }
                _ => None,
            };
        }
        None
    }

    fn if_chain(
        &mut self,
        arms: &[(Spanned<Expression>, Box<[Spanned<Statement>]>)],
        else_arm: Option<&[Spanned<Statement>]>,
        location: dreammaker::Location,
    ) -> Vec<Fill> {
        let Some(((cond, block), rest)) = arms.split_first() else {
            return else_arm.map(|b| self.block(b)).unwrap_or_default();
        };
        let then = self.block(block);
        let otherwise = self.if_chain(rest, else_arm, location);
        let cond = &cond.elem;
        if let Some(Term::Call(name, args)) = unparen(cond)
            && &**name == "prob"
            && args.len() == 1
            && let Some(percent) = number(&args[0])
        {
            return vec![Fill::Prob {
                percent,
                then,
                otherwise,
            }];
        }
        // `isnull(var)` / `!isnull(var)` on a type var: its truthiness, as paths are never false.
        if let Some(Term::Call(name, args)) = unparen(cond)
            && &**name == "isnull"
            && let [arg] = &args[..]
            && let Some(var) = type_var(arg).filter(|v| !self.locals.contains_key(v))
        {
            return vec![Fill::IfVar {
                var,
                then: otherwise,
                otherwise: then,
            }];
        }
        if let Expression::Base { term, follow } = cond
            && let [unary] = &follow[..]
            && let Follow::Unary(UnaryOp::Not) = unary.elem
            && let Term::Call(name, args) = &term.elem
            && &**name == "isnull"
            && let [arg] = &args[..]
            && let Some(var) = type_var(arg).filter(|v| !self.locals.contains_key(v))
        {
            return vec![Fill::IfVar {
                var,
                then,
                otherwise,
            }];
        }
        if let Some(var) = type_var(cond).filter(|v| !self.locals.contains_key(v)) {
            return vec![Fill::IfVar {
                var,
                then,
                otherwise,
            }];
        }
        if let Expression::Base { term, follow } = cond
            && let [unary] = &follow[..]
            && let Follow::Unary(UnaryOp::Not) = unary.elem
            && let Term::Ident(var) = &term.elem
            && !self.locals.contains_key(var)
        {
            return vec![Fill::IfVar {
                var: var.clone(),
                then: otherwise,
                otherwise: then,
            }];
        }
        if has_spawn(&then) || has_spawn(&otherwise) {
            vec![self.unsupported(location)]
        } else {
            Vec::new()
        }
    }

    fn switch(
        &mut self,
        input: &Expression,
        cases: &[(Spanned<Vec<Case>>, Box<[Spanned<Statement>]>)],
        default: Option<&[Spanned<Statement>]>,
    ) -> Option<Fill> {
        // (case value, weight) of every outcome the input can have.
        #[derive(PartialEq)]
        enum Key {
            Str(String),
            Num(i32),
        }
        let outcomes: Vec<(Key, f32)> = match unparen(input)? {
            Term::Call(name, args) if &**name == "pick_weight" && args.len() == 1 => {
                let Some(Term::List(items)) = unparen(&args[0]) else {
                    return None;
                };
                let mut out = Vec::new();
                for (k, v) in assoc(items)? {
                    let Some(Term::String(s)) = unparen(k) else {
                        return None;
                    };
                    out.push((Key::Str(s.clone()), number(v?)?));
                }
                out
            }
            Term::Pick(args) => {
                let mut out = Vec::new();
                for (weight, e) in args.iter() {
                    let w = match weight {
                        Some(w) => number(w)?,
                        None => 1.0,
                    };
                    let key = match unparen(e)? {
                        Term::String(s) => Key::Str(s.clone()),
                        Term::Int(n) => Key::Num(*n),
                        _ => return None,
                    };
                    out.push((key, w));
                }
                out
            }
            Term::Call(name, args) if &**name == "rand" && args.len() == 2 => {
                let (a, b) = (int(&args[0])?, int(&args[1])?);
                if b < a || b - a > 1000 {
                    return None;
                }
                (a..=b).map(|n| (Key::Num(n), 1.0)).collect()
            }
            _ => return None,
        };
        let mut bodies: Vec<(Vec<Case>, Vec<Fill>)> = Vec::new();
        for (case, block) in cases {
            bodies.push((case.elem.clone(), self.block(block)));
        }
        let default = default.map(|b| self.block(b)).unwrap_or_default();
        let matches = |case: &Case, key: &Key| match (case, key) {
            (Case::Exact(e), Key::Str(s)) => matches!(unparen(e), Some(Term::String(c)) if c == s),
            (Case::Exact(e), Key::Num(n)) => int(e) == Some(*n),
            (Case::Range(lo, hi), Key::Num(n)) => {
                matches!((int(lo), int(hi)), (Some(lo), Some(hi)) if (lo..=hi).contains(n))
            }
            _ => false,
        };
        let arms = outcomes
            .iter()
            .map(|(key, weight)| {
                let body = bodies
                    .iter()
                    .find(|(case, _)| case.iter().any(|c| matches(c, key)))
                    .map_or_else(|| default.clone(), |(_, body)| body.clone());
                (*weight, body)
            })
            .collect();
        Some(Fill::Pick { arms })
    }

    /// A prefab path that exists in the object tree.
    fn path(&self, path: &[(dreammaker::ast::PathOp, String)]) -> Option<String> {
        let text = FormatTypePath(path).to_string();
        self.env.tree.find(&text).map(|_| text)
    }

    fn prefab(&self, e: &Expression) -> Option<String> {
        match unparen(e)? {
            Term::Prefab(p) => self.path(&p.path),
            _ => None,
        }
    }
}

fn has_spawn(fills: &[Fill]) -> bool {
    fills.iter().any(|f| match f {
        Fill::New(_) | Fill::NewVar(_) | Fill::Unsupported(_) | Fill::Parent => true,
        Fill::Repeat { body, .. } | Fill::RepeatVar { body, .. } => has_spawn(body),
        Fill::Prob {
            then, otherwise, ..
        }
        | Fill::IfVar {
            then, otherwise, ..
        } => has_spawn(then) || has_spawn(otherwise),
        Fill::Pick { arms } => arms.iter().any(|(_, b)| has_spawn(b)),
        Fill::Return => false,
    })
}

/// The single term of `e`, through parentheses.
fn unparen(e: &Expression) -> Option<&Term> {
    match e.as_term()? {
        Term::Expr(inner) => unparen(inner),
        t => Some(t),
    }
}

fn int(e: &Expression) -> Option<i32> {
    match unparen(e)? {
        Term::Int(n) => Some(*n),
        Term::Float(f) if f.fract() == 0.0 => Some(*f as i32),
        _ => None,
    }
}

fn number(e: &Expression) -> Option<f32> {
    match unparen(e)? {
        Term::Int(n) => Some(*n as f32),
        Term::Float(f) => Some(*f),
        _ => None,
    }
}

fn is_src(e: &Expression) -> bool {
    matches!(unparen(e), Some(Term::Ident(i)) if i == "src")
}

/// `(src, ...)`: the new atom goes into the container.
fn into_src(args: Option<&[Expression]>) -> Option<()> {
    args.and_then(<[Expression]>::first)
        .filter(|a| is_src(a))
        .map(|_| ())
}

/// `if(empty)`: a bare identifier is a var of the container type (locals are excluded by the
/// caller for `!var`, and `src` vars are what these procs test).
fn type_var(cond: &Expression) -> Option<String> {
    match unparen(cond)? {
        Term::Ident(v) if v != "src" && v != "." => Some(v.clone()),
        _ => None,
    }
}

/// `list(k = v, k2)` -> `[(k, Some(v)), (k2, None)]`.
fn assoc(items: &[Expression]) -> Option<Vec<(&Expression, Option<&Expression>)>> {
    items
        .iter()
        .map(|item| match item {
            Expression::AssignOp {
                op: AssignOp::Assign,
                lhs,
                rhs,
            } => Some((&**lhs, Some(&**rhs))),
            e => Some((e, None)),
        })
        .collect()
}

fn has_new(e: &Expression) -> bool {
    let mut found = false;
    walk(e, &mut |t| {
        found |= matches!(
            t,
            Term::NewImplicit { .. } | Term::NewPrefab { .. } | Term::NewMiniExpr { .. }
        );
    });
    found
}

/// Whether evaluating `e` may create an atom: a `new`, or a call that is not known harmless.
fn spawns(e: &Expression) -> bool {
    let mut found = false;
    walk(e, &mut |t| {
        found |= match t {
            Term::NewImplicit { .. } | Term::NewPrefab { .. } | Term::NewMiniExpr { .. } => true,
            Term::Call(name, _) | Term::GlobalCall(name, _) => !HARMLESS_CALLS.contains(&&**name),
            Term::SelfCall(_) | Term::DynamicCall(..) => true,
            _ => false,
        };
    });
    found
}

fn walk(e: &Expression, f: &mut impl FnMut(&Term)) {
    match e {
        Expression::Base { term, follow } => {
            walk_term(&term.elem, f);
            for step in follow.iter() {
                match &step.elem {
                    Follow::Index(_, i) => walk(i, f),
                    Follow::Call(_, name, args) => {
                        if !HARMLESS_CALLS.contains(&&**name) {
                            // A method call: treat like an unknown call.
                            f(&Term::SelfCall(Box::new([])));
                        }
                        args.iter().for_each(|a| walk(a, f));
                    }
                    _ => {}
                }
            }
        }
        Expression::BinaryOp { lhs, rhs, .. } | Expression::AssignOp { lhs, rhs, .. } => {
            walk(lhs, f);
            walk(rhs, f);
        }
        Expression::TernaryOp { cond, if_, else_ } => {
            walk(cond, f);
            walk(if_, f);
            walk(else_, f);
        }
    }
}

fn walk_term(t: &Term, f: &mut impl FnMut(&Term)) {
    f(t);
    let args: &[Expression] = match t {
        Term::Expr(e) => return walk(e, f),
        Term::Call(_, args)
        | Term::SelfCall(args)
        | Term::ParentCall(args)
        | Term::List(args)
        | Term::GlobalCall(_, args) => args,
        Term::NewPrefab { args, .. }
        | Term::NewMiniExpr { args, .. }
        | Term::NewImplicit { args } => args.as_deref().unwrap_or_default(),
        Term::Pick(args) => {
            for (w, e) in args.iter() {
                if let Some(w) = w {
                    walk(w, f);
                }
                walk(e, f);
            }
            return;
        }
        _ => return,
    };
    args.iter().for_each(|a| walk(a, f));
}
