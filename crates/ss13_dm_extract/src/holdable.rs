//! What each `/datum/storage` type may hold: the `set_holdable(can_hold_list, cant_hold_list,
//! exception_hold_list)` call of its `New()` (`code/datums/storage/storage.dm`), lowered to
//! plain type lists. A storage type without the call keeps its parent's lists (`New` calls
//! `..()`), so consumers walk the parent chain to the nearest entry.

use std::collections::BTreeMap;

use dreammaker as dm;
use dreammaker::ast::{AssignOp, BinaryOp, Expression, Follow, Spanned, Statement, Term};
use serde::Serialize;
use serde_json::Value;

use crate::Environment;

pub const STORAGE_ROOT: &str = "/datum/storage";

/// The three lists of one `set_holdable` call; `None`: the argument is absent or null.
#[derive(Serialize, Debug, Default, PartialEq)]
pub struct HoldLists {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_hold: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cant_hold: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exception_hold: Option<Vec<String>>,
    /// An argument that is not a constant list of types (its source is not understood): the
    /// storage is treated as unrestricted for that list.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unsupported: bool,
}

/// Storage type -> its own `set_holdable` call.
pub type Holdable = BTreeMap<String, HoldLists>;

const ARGS: [&str; 3] = ["can_hold_list", "cant_hold_list", "exception_hold_list"];

pub(crate) fn holdable(env: &Environment) -> Holdable {
    let mut out = Holdable::new();
    for ty in env.tree.iter_types() {
        let path = &ty.get().path;
        if !(path == STORAGE_ROOT || path.starts_with("/datum/storage/")) {
            continue;
        }
        let Some(code) = crate::proc_code(env, path, "New") else {
            continue;
        };
        if let Some(lists) = scan(env, code) {
            out.insert(path.clone(), lists);
        }
    }
    out
}

/// The last `set_holdable(...)` statement of `code`, through `if` and other blocks.
fn scan(env: &Environment, code: &[Spanned<Statement>]) -> Option<HoldLists> {
    let mut found = None;
    for statement in code {
        match &statement.elem {
            Statement::Expr(e) => {
                if let Some(args) = call_args(e) {
                    found = Some(lower(env, args, statement.location));
                }
            }
            Statement::If { arms, else_arm } => {
                for (_, block) in arms {
                    found = scan(env, block).or(found);
                }
                if let Some(block) = else_arm {
                    found = scan(env, block).or(found);
                }
            }
            _ => {}
        }
    }
    found
}

fn call_args(e: &Expression) -> Option<&[Expression]> {
    let Expression::Base { term, follow } = e else {
        return None;
    };
    match (&term.elem, &follow[..]) {
        (Term::Call(name, args), []) if &**name == "set_holdable" => Some(args),
        _ => None,
    }
}

fn lower(env: &Environment, args: &[Expression], location: dm::Location) -> HoldLists {
    let mut lists = HoldLists::default();
    for (index, arg) in args.iter().enumerate() {
        let (name, value) = match arg {
            Expression::AssignOp {
                op: AssignOp::Assign,
                lhs,
                rhs,
            } => match lhs.as_term() {
                Some(Term::Ident(name)) => (name.as_str(), &**rhs),
                _ => {
                    lists.unsupported = true;
                    continue;
                }
            },
            e => match ARGS.get(index) {
                Some(name) => (*name, e),
                None => continue,
            },
        };
        let Some(slot) = ARGS.iter().position(|a| *a == name) else {
            continue;
        };
        match types(env, value, location) {
            Some(list) => {
                let target = match slot {
                    0 => &mut lists.can_hold,
                    1 => &mut lists.cant_hold,
                    _ => &mut lists.exception_hold,
                };
                *target = list;
            }
            None => lists.unsupported = true,
        }
    }
    lists
}

/// `e` as a list of type paths: `Some(None)` for `null`, `None` when it is not constant.
fn types(env: &Environment, e: &Expression, location: dm::Location) -> Option<Option<Vec<String>>> {
    // `GLOB.tool_items + list(...)`.
    if let Expression::BinaryOp {
        op: BinaryOp::Add,
        lhs,
        rhs,
    } = e
    {
        let mut all = types(env, lhs, location)?.unwrap_or_default();
        all.extend(types(env, rhs, location)?.unwrap_or_default());
        return Some(Some(all));
    }
    if let Expression::Base { term, follow } = e
        && let (Term::Ident(glob), [field]) = (&term.elem, &follow[..])
        && glob == "GLOB"
        && let Follow::Field(_, name) = &field.elem
    {
        return json_types(&crate::global_init(env, name)?);
    }
    let constant = e.clone().simple_evaluate(location).ok()?;
    json_types(&crate::constant_to_json(&constant)?)
}

fn json_types(value: &Value) -> Option<Option<Vec<String>>> {
    let path = |v: &Value| v.get("path")?.as_str().map(str::to_owned);
    match value {
        Value::Null => Some(None),
        Value::Object(o) => match o.get("list") {
            Some(Value::Array(items)) => {
                Some(Some(items.iter().map(path).collect::<Option<Vec<_>>>()?))
            }
            _ => Some(Some(vec![path(value)?])),
        },
        _ => None,
    }
}
