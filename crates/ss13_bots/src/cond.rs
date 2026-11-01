//! Conditions of `until` and `expect`: `<subject>.<path> <op> [value]`, evaluated over JSON (a
//! bot's observation or a server test hook answer).

use serde_json::Value;

/// Where the JSON comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Subject {
    /// A bot's own observation (index into the case's bots).
    Bot(usize),
    /// Server: everything on a tile of the case's level.
    Tile(i32, i32),
    /// Server: a bot's mob.
    Mob(usize),
    /// Server: the first entity on the tile whose DM path starts with the text (or, without a
    /// leading `/`, whose name contains it).
    Obj(i32, i32, String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Seg {
    Key(String),
    /// Negative: from the end (`[-1]` is the last element).
    Index(i64),
    /// `[]`: the rest of the path for every element.
    Each,
    /// `len`: length of an array, object or string.
    Len,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// `~`: a string contains the text; an array has an element equal to the value (a string
    /// element: containing it).
    Contains,
    NotContains,
    Exists,
    Missing,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cond {
    pub subject: Subject,
    pub path: Vec<Seg>,
    pub op: Op,
    pub value: Value,
    /// As written, for failure messages.
    pub text: String,
}

/// `subject` maps a bot name to its index.
pub fn parse(tokens: &[String], bot: &dyn Fn(&str) -> Option<usize>) -> Result<Cond, String> {
    let text = tokens
        .iter()
        .map(|t| match t.strip_prefix('\0') {
            Some(quoted) => format!("{quoted:?}"),
            None => t.clone(),
        })
        .collect::<Vec<_>>()
        .join(" ");
    let (lhs, rest) = tokens.split_first().ok_or("empty condition")?;
    let (subject, path) = parse_lhs(lhs, bot)?;
    let (op, value) = match rest {
        [op] if op == "exists" => (Op::Exists, Value::Null),
        [op] if op == "missing" => (Op::Missing, Value::Null),
        [op, value] => {
            let op = match op.as_str() {
                "==" => Op::Eq,
                "!=" => Op::Ne,
                "<" => Op::Lt,
                "<=" => Op::Le,
                ">" => Op::Gt,
                ">=" => Op::Ge,
                "~" => Op::Contains,
                "!~" => Op::NotContains,
                other => return Err(format!("unknown operator {other:?}")),
            };
            (op, parse_value(value))
        }
        _ => {
            return Err(format!(
                "expected `<subject>.<path> <op> <value>` or `... exists|missing`, got {text:?}"
            ));
        }
    };
    let var = value.as_str().is_some_and(|s| s.starts_with('$'));
    if matches!(op, Op::Lt | Op::Le | Op::Gt | Op::Ge) && !value.is_number() && !var {
        return Err(format!("{text:?}: ordering needs a number"));
    }
    Ok(Cond {
        subject,
        path,
        op,
        value,
        text,
    })
}

/// A literal: JSON (`1.5`, `true`, `null`, `[1,2]`, `"text"`), a tile `x,y` (an `[x, y]`
/// array), anything else a string. Quoted tokens arrive unquoted from the tokenizer and are
/// marked by a leading NUL.
pub fn parse_value(token: &str) -> Value {
    if let Some(quoted) = token.strip_prefix('\0') {
        return Value::String(quoted.to_owned());
    }
    if let Some(tile) = parse_tile(token) {
        return serde_json::json!([tile.0, tile.1]);
    }
    serde_json::from_str(token).unwrap_or_else(|_| Value::String(token.to_owned()))
}

/// A `$name`, `$name+N` or `$name-N` value against saved values; other values as they are.
pub fn resolve(value: &Value, vars: &dyn Fn(&str) -> Option<Value>) -> Result<Value, String> {
    let Some(expr) = value.as_str().and_then(|s| s.strip_prefix('$')) else {
        return Ok(value.clone());
    };
    let (name, offset) = match expr.find(['+', '-']) {
        Some(i) => {
            let n: f64 = expr[i..]
                .parse()
                .map_err(|_| format!("${expr}: bad offset"))?;
            (&expr[..i], Some(n))
        }
        None => (expr, None),
    };
    let saved = vars(name).ok_or_else(|| format!("${name} was not saved"))?;
    match offset {
        None => Ok(saved),
        Some(n) => saved
            .as_f64()
            .map(|v| serde_json::json!(v + n))
            .ok_or_else(|| format!("${name} is not a number: {saved}")),
    }
}

/// `x,y`.
pub fn parse_tile(text: &str) -> Option<(i32, i32)> {
    let (x, y) = text.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

pub fn parse_lhs(
    lhs: &str,
    bot: &dyn Fn(&str) -> Option<usize>,
) -> Result<(Subject, Vec<Seg>), String> {
    let lhs = lhs.strip_prefix('\0').unwrap_or(lhs);
    let (subject, rest) = if let Some(open) = lhs.find('(').filter(|&i| {
        let head = &lhs[..i];
        head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }) {
        let close = lhs[open..]
            .find(')')
            .map(|i| open + i)
            .ok_or_else(|| format!("{lhs:?}: missing `)`"))?;
        let args: Vec<&str> = lhs[open + 1..close]
            .split(',')
            .map(|a| a.trim().trim_matches('"'))
            .collect();
        let tile = |args: &[&str]| -> Result<(i32, i32), String> {
            match args {
                [x, y, ..] => Ok((
                    x.parse().map_err(|_| format!("{lhs:?}: bad x"))?,
                    y.parse().map_err(|_| format!("{lhs:?}: bad y"))?,
                )),
                _ => Err(format!("{lhs:?}: expected x,y")),
            }
        };
        let subject = match &lhs[..open] {
            "tile" if args.len() == 2 => {
                let (x, y) = tile(&args)?;
                Subject::Tile(x, y)
            }
            "obj" if args.len() == 3 && !args[2].is_empty() => {
                let (x, y) = tile(&args)?;
                Subject::Obj(x, y, args[2].to_owned())
            }
            "mob" if args.len() == 1 => {
                Subject::Mob(bot(args[0]).ok_or_else(|| format!("unknown bot {:?}", args[0]))?)
            }
            other => {
                return Err(format!(
                    "{lhs:?}: unknown subject {other:?} (tile(x,y), obj(x,y,path), mob(bot))"
                ));
            }
        };
        (subject, &lhs[close + 1..])
    } else {
        let end = lhs.find(['.', '[']).unwrap_or(lhs.len());
        let name = &lhs[..end];
        let i = bot(name).ok_or_else(|| format!("unknown bot {name:?}"))?;
        (Subject::Bot(i), &lhs[end..])
    };
    Ok((subject, parse_path(rest)?))
}

pub fn parse_path(mut rest: &str) -> Result<Vec<Seg>, String> {
    let mut path = Vec::new();
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("[]") {
            path.push(Seg::Each);
            rest = r;
        } else if let Some(r) = rest.strip_prefix('[') {
            let close = r.find(']').ok_or("missing `]`")?;
            let index = r[..close]
                .parse()
                .map_err(|_| format!("bad index {:?}", &r[..close]))?;
            path.push(Seg::Index(index));
            rest = &r[close + 1..];
        } else if let Some(r) = rest.strip_prefix('.') {
            let end = r.find(['.', '[']).unwrap_or(r.len());
            let key = &r[..end];
            if key.is_empty() {
                return Err("empty path segment".into());
            }
            path.push(match key {
                "len" => Seg::Len,
                _ => match key.parse() {
                    Ok(i) => Seg::Index(i),
                    Err(_) => Seg::Key(key.to_owned()),
                },
            });
            rest = &r[end..];
        } else {
            return Err(format!("unexpected {rest:?} in path"));
        }
    }
    Ok(path)
}

/// The value at `path`; `None` when missing.
pub fn select(value: &Value, path: &[Seg]) -> Option<Value> {
    let Some((seg, rest)) = path.split_first() else {
        return Some(value.clone());
    };
    match seg {
        Seg::Key(key) => select(value.get(key)?, rest),
        Seg::Index(i) => {
            let items = value.as_array()?;
            let i = if *i < 0 { items.len() as i64 + i } else { *i };
            select(items.get(usize::try_from(i).ok()?)?, rest)
        }
        Seg::Len => {
            let len = match value {
                Value::Array(a) => a.len(),
                Value::Object(o) => o.len(),
                Value::String(s) => s.chars().count(),
                _ => return None,
            };
            select(&Value::from(len), rest)
        }
        Seg::Each => {
            let items: Vec<Value> = match value {
                Value::Array(a) => a.iter().filter_map(|v| select(v, rest)).collect(),
                Value::Object(o) => o.values().filter_map(|v| select(v, rest)).collect(),
                _ => return None,
            };
            Some(Value::Array(items))
        }
    }
}

/// Whether `actual` (missing: `None`) satisfies the condition.
pub fn holds(cond: &Cond, actual: Option<&Value>) -> bool {
    let present = actual.filter(|v| !v.is_null());
    match cond.op {
        Op::Exists => present.is_some(),
        Op::Missing => present.is_none(),
        Op::Eq => actual.is_some_and(|a| equal(a, &cond.value)),
        Op::Ne => !actual.is_some_and(|a| equal(a, &cond.value)),
        Op::Lt | Op::Le | Op::Gt | Op::Ge => {
            let (Some(a), Some(b)) = (present.and_then(Value::as_f64), cond.value.as_f64()) else {
                return false;
            };
            match cond.op {
                Op::Lt => a < b,
                Op::Le => a <= b,
                Op::Gt => a > b,
                _ => a >= b,
            }
        }
        Op::Contains => present.is_some_and(|a| contains(a, &cond.value)),
        Op::NotContains => !present.is_some_and(|a| contains(a, &cond.value)),
    }
}

fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (
                x.as_f64().unwrap_or(f64::NAN),
                y.as_f64().unwrap_or(f64::NAN),
            );
            (x - y).abs() <= 1e-6 * x.abs().max(y.abs()).max(1.0)
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| equal(a, b))
        }
        _ => a == b,
    }
}

fn contains(a: &Value, needle: &Value) -> bool {
    match (a, needle) {
        (Value::String(s), Value::String(n)) => s.contains(n.as_str()),
        (Value::Array(items), _) => items.iter().any(|item| match (item, needle) {
            (Value::String(s), Value::String(n)) => s.contains(n.as_str()),
            _ => equal(item, needle),
        }),
        (Value::Object(o), Value::String(n)) => o.keys().any(|k| k.contains(n.as_str())),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn bots(name: &str) -> Option<usize> {
        ["alice", "bob"].iter().position(|b| *b == name)
    }

    fn cond(text: &str) -> Cond {
        let tokens: Vec<String> = crate::case::tokenize(text).unwrap();
        parse(&tokens, &bots).unwrap()
    }

    #[test]
    fn parses_subjects_and_paths() {
        let c = cond("alice.windows.Vending.data.records[0].amount >= 2");
        assert_eq!(c.subject, Subject::Bot(0));
        assert_eq!(
            c.path,
            vec![
                Seg::Key("windows".into()),
                Seg::Key("Vending".into()),
                Seg::Key("data".into()),
                Seg::Key("records".into()),
                Seg::Index(0),
                Seg::Key("amount".into()),
            ]
        );
        assert_eq!((c.op, c.value), (Op::Ge, json!(2)));
        let c = cond("tile(3,4).entities[].path ~ \"/obj/item/toolbox\"");
        assert_eq!(c.subject, Subject::Tile(3, 4));
        assert_eq!(
            c.path,
            vec![
                Seg::Key("entities".into()),
                Seg::Each,
                Seg::Key("path".into())
            ]
        );
        assert_eq!(c.value, json!("/obj/item/toolbox"));
        let c = cond("obj(5,3,/obj/machinery/door).door.open == true");
        assert_eq!(c.subject, Subject::Obj(5, 3, "/obj/machinery/door".into()));
        let c = cond("mob(bob).health.brute > 0");
        assert_eq!(c.subject, Subject::Mob(1));
        let c = cond("bob.pos == 4,3");
        assert_eq!(c.value, json!([4, 3]));
        let c = cond("alice.chat.len exists");
        assert_eq!(c.path, vec![Seg::Key("chat".into()), Seg::Len]);
        assert_eq!(c.op, Op::Exists);
    }

    #[test]
    fn rejects_bad_conditions() {
        let parse_text = |t: &str| parse(&crate::case::tokenize(t).unwrap(), &bots);
        assert!(
            parse_text("carol.pos == 1,1")
                .unwrap_err()
                .contains("unknown bot")
        );
        assert!(
            parse_text("alice.pos =~ 1")
                .unwrap_err()
                .contains("operator")
        );
        assert!(parse_text("alice.pos").is_err());
        assert!(
            parse_text("alice.hunger > full")
                .unwrap_err()
                .contains("number")
        );
        assert!(
            parse_text("box(1,2).x == 1")
                .unwrap_err()
                .contains("subject")
        );
    }

    #[test]
    fn evaluates_ops() {
        let v = json!({
            "pos": [4, 3],
            "chat": ["Alice says, \"hi\"", "radio: [Common] Bob: ok"],
            "health": { "health": 82.5 },
            "hands": [null, "toolbox"],
            "entities": [{ "path": "/obj/item/toolbox" }, { "path": "/turf/open/floor" }],
        });
        let check = |text: &str| {
            let c = cond(text);
            let actual = select(&v, &c.path);
            holds(&c, actual.as_ref())
        };
        assert!(check("alice.pos == 4,3"));
        assert!(!check("alice.pos != 4,3"));
        assert!(check("alice.chat ~ \"[Common] Bob\""));
        assert!(check("alice.chat !~ Carol"));
        assert!(check("alice.health.health < 100"));
        assert!(check("alice.health.health == 82.5"));
        assert!(!check("alice.health.health > 90"));
        assert!(check("alice.hands[0] missing"));
        assert!(check("alice.hands[1] == toolbox"));
        assert!(check("alice.entities[].path ~ toolbox"));
        assert!(check("alice.entities.len == 2"));
        assert!(check("alice.entities[-1].path == \"/turf/open/floor\""));
        assert!(check("alice.entities[-3] missing"));
        assert!(check("alice.nothing.here missing"));
        let vars = |n: &str| (n == "hp").then(|| json!(82.5));
        assert_eq!(resolve(&json!("$hp-2.5"), &vars), Ok(json!(80.0)));
        assert_eq!(resolve(&json!("$hp"), &vars), Ok(json!(82.5)));
        assert!(resolve(&json!("$nope"), &vars).is_err());
        assert_eq!(resolve(&json!(3), &vars), Ok(json!(3)));
        assert_eq!(cond("alice.health.health < $hp").value, json!("$hp"));
        assert!(check("alice.nothing != 1"));
        assert!(!check("alice.nothing > 1"));
    }
}
