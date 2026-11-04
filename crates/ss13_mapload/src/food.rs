//! Food as prototype data: tg `/obj/item/food` (`food/_food.dm`) with what its `Initialize`
//! hands `/datum/component/edible` (`food_reagents`, `max_volume`, `bite_consumption`,
//! `eat_time`, `tastes`, `eatverbs`, `junkiness`), `trash_type` (`/datum/element/food_trash`),
//! the microwave result (`make_microwaveable()`: `/datum/element/microwavable`), the tool
//! processing (`make_processable()`: `/datum/element/processable`) and the food processor
//! recipe (`/datum/food_processor_process` with `/obj/machinery/processor`) of every food type,
//! as the [`DmFood`] component.
//!
//! The two elements are added in procs: `ss13-dm-extract` dumps their `AddElement` lines
//! (`element_calls`), and the arguments are read here with the concrete type's vars
//! (`slice_type`, `yield`, `warm_type`, ...).
//!
//! Grown food (`/obj/item/food/grown`) gets what a map-placed one gets from its seed: potency
//! 50 (`adjust_potency(50 - potency)`), `prepare_result` reagents and the potency bite size.
// shortcut: no `grind_results()`/`juice_typepath()` overrides (procs; grinding gives the item's
// reagents), no plant genes, no added reagents of microwave results (donk pockets' omnizine),
// no foodtypes, decomposition, custom materials or crafting buffs.

use std::collections::BTreeSet;

use bevy::prelude::*;
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num};
use crate::reagents::{ReagentAmount, amounts, path_value};
use crate::value::Field;

pub const FOOD_ROOT: &str = "/obj/item/food";
pub const GROWN_ROOT: &str = "/obj/item/food/grown";
pub const FOOD_COMPONENT: &str = "dm_food";
/// What `/obj/item/food/make_microwaveable()` turns food into by default.
pub const BAD_RECIPE: &str = "/obj/item/food/badrecipe";
pub const PROCESSOR_RECIPE_ROOT: &str = "/datum/food_processor_process";
pub const PROCESSOR: &str = "/obj/machinery/processor";
/// `/obj/machinery/reagentgrinder/Initialize(mapload)` puts one in.
pub const GRINDER_BEAKER: &str = "/obj/item/reagent_containers/cup/beaker/large";
pub const GRINDER: &str = "/obj/machinery/reagentgrinder";
/// `/obj/machinery/oven/Initialize(mapload)` puts one in.
pub const OVEN: &str = "/obj/machinery/oven";
pub const OVEN_TRAY: &str = "/obj/item/plate/oven_tray";
const SODA_CANS: &str = "/obj/item/reagent_containers/cup/soda_cans";
const CRUSHED_CAN: &str = "/obj/item/trash/can";
const PROCESSABLE: &str = "/datum/element/processable";
const MICROWAVABLE: &str = "/datum/element/microwavable";
const BAKEABLE: &str = "/datum/component/bakeable";
const GRILLABLE: &str = "/datum/component/grillable";
/// `DEFAULT_EDIBLE_VOLUME`.
pub const DEFAULT_EDIBLE_VOLUME: f32 = 50.0;
/// `/datum/component/edible/Initialize` defaults: `eat_time = 1 SECONDS`, `bite_consumption = 2`,
/// `reagent_purity = 0.5`.
const DEFAULT_EAT_TIME: f32 = 10.0;
const DEFAULT_BITE: f32 = 2.0;
const DEFAULT_PURITY: f32 = 0.5;
const DEFAULT_EATVERBS: [&str; 6] = ["bite", "chew", "nibble", "gnaw", "gobble", "chomp"];
/// `/datum/element/processable/Attach` defaults: `amount_created = 3`, `time_to_process = 2
/// SECONDS`.
const DEFAULT_PROCESS_AMOUNT: u32 = 3;
const DEFAULT_PROCESS_TIME: f32 = 20.0;
/// `BITE_SIZE_POTENCY_MULTIPLIER`, `BITE_SIZE_VOLUME_MULTIPLIER`.
const BITE_SIZE_POTENCY_MULTIPLIER: f32 = 0.05;
const BITE_SIZE_VOLUME_MULTIPLIER: f32 = 0.01;
/// A map-placed grown item's seed potency.
const MAP_POTENCY: f32 = 50.0;

/// One `tastes` entry: `list("bread" = 10)`.
#[derive(Reflect, Default, Clone, Debug, PartialEq)]
pub struct Taste {
    pub text: String,
    pub weight: f32,
}

/// One `/datum/element/processable`: `tool_behaviour` turns the food into `amount` of `result`
/// after `time` deciseconds.
#[derive(Reflect, Default, Clone, Debug, PartialEq)]
pub struct Processing {
    pub tool: String,
    pub result: String,
    pub amount: u32,
    pub time: f32,
    pub table_required: bool,
}

/// What a food type eats like and turns into (every type carries all of it resolved).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmFood {
    pub food_reagents: Vec<ReagentAmount>,
    /// The holder's `maximum_volume` (`max_volume`, else [`DEFAULT_EDIBLE_VOLUME`]).
    pub volume: f32,
    /// Purity of the food reagents (`starting_reagent_purity`, else 0.5).
    pub purity: f32,
    pub bite_consumption: f32,
    /// Deciseconds.
    pub eat_time: f32,
    pub tastes: Vec<Taste>,
    pub eatverbs: Vec<String>,
    pub junkiness: f32,
    pub food_flags: u32,
    /// `trash_type`; empty: none.
    pub trash_type: String,
    /// The microwave result; empty: not microwavable.
    pub microwaved: String,
    /// `TRAIT_MICROWAVABLE` (a real recipe, not `bad_recipe`).
    pub microwave_good: bool,
    pub processing: Vec<Processing>,
    /// The food processor recipe; empty output: the processor refuses it.
    pub processor_output: String,
    pub processor_multiplier: u32,
    /// Deciseconds.
    pub processor_time: f32,
    /// `/datum/component/bakeable` (`make_bakeable()`): the oven turns it into this after a
    /// time rolled in `bake_time` (deciseconds); empty: not bakeable.
    pub bake_result: String,
    pub bake_time: [f32; 2],
    /// `positive_result`: a real recipe (else it burns).
    pub bake_good: bool,
    /// `/datum/component/grillable` (`make_grillable()`) on a griddle, the same way.
    pub grill_result: String,
    pub grill_time: [f32; 2],
    pub grill_good: bool,
}

/// A cooking component's result, time range (deciseconds) and whether it is a real recipe.
#[derive(Clone, Debug, PartialEq)]
pub struct Cook {
    pub result: String,
    pub time: [f32; 2],
    pub good: bool,
}

fn texts(value: Option<&Value>) -> Vec<String> {
    let Some(items) = value.and_then(|v| v.get("list")).and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// `list("text" = weight, ...)`.
fn tastes(value: Option<&Value>) -> Vec<Taste> {
    let Some(items) = value.and_then(|v| v.get("list")).and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let pair = item.get("assoc")?.as_array()?;
            Some(Taste {
                text: pair.first()?.as_str()?.to_owned(),
                weight: num(pair.get(1)).unwrap_or(1.0) as f32,
            })
        })
        .collect()
}

/// The arguments of an `AddElement(...)` line, split at top-level commas (the element first).
fn call_args(line: &str) -> Option<Vec<String>> {
    let start = ["AddElement(", "AddComponent("]
        .iter()
        .find_map(|call| Some(line.find(call)? + call.len()))?;
    let mut args = Vec::new();
    let (mut depth, mut current, mut quoted) = (0, String::new(), false);
    for c in line[start..].chars() {
        match c {
            '"' => {
                quoted = !quoted;
                current.push(c);
            }
            _ if quoted => current.push(c),
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' if depth == 0 => {
                args.push(current.trim().to_owned());
                return Some(args);
            }
            ')' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => args.push(std::mem::take(&mut current).trim().to_owned()),
            _ => current.push(c),
        }
    }
    None
}

/// An argument value as `path` sees it.
#[derive(Clone, Debug, PartialEq)]
enum Arg {
    Path(String),
    Num(f32),
    /// `rand(low, high)`.
    Range(f32, f32),
    Text(String),
    Null,
}

impl Arg {
    fn truthy(&self) -> bool {
        match self {
            Arg::Path(_) => true,
            Arg::Num(n) => *n != 0.0,
            Arg::Range(..) => true,
            Arg::Text(t) => !t.is_empty(),
            Arg::Null => false,
        }
    }
}

/// `TOOL_*` defines (`code/__DEFINES/tools.dm`) and the other defines processable lines use.
fn define(name: &str) -> Option<Arg> {
    Some(match name {
        "TOOL_KNIFE" => Arg::Text("knife".into()),
        "TOOL_SAW" => Arg::Text("saw".into()),
        "TOOL_ROLLINGPIN" => Arg::Text("rolling pin".into()),
        "MEATSLAB_PROCESSED_AMOUNT" => Arg::Num(3.0),
        "TRUE" => Arg::Num(1.0),
        "FALSE" | "null" => Arg::Null,
        _ => return None,
    })
}

/// A literal, a define, `N SECONDS` or a var of `path`; `None`: not understood.
fn arg_value(types: &DmTypes, path: &str, text: &str) -> Option<Arg> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix('!') {
        return Some(Arg::Num(f32::from(!arg_value(types, path, rest)?.truthy())));
    }
    if text.starts_with('/') {
        return Some(Arg::Path(text.to_owned()));
    }
    if let Some(inner) = text.strip_prefix("rand(").and_then(|t| t.strip_suffix(')')) {
        let (low, high) = inner.split_once(',')?;
        let n = |t: &str| match arg_value(types, path, t)? {
            Arg::Num(n) => Some(n),
            _ => None,
        };
        return Some(Arg::Range(n(low)?, n(high)?));
    }
    if let Some(n) = text.strip_suffix("SECONDS") {
        return n.trim().parse::<f32>().ok().map(|n| Arg::Num(n * 10.0));
    }
    if let Ok(n) = text.parse::<f32>() {
        return Some(Arg::Num(n));
    }
    if let Some(d) = define(text) {
        return Some(d);
    }
    if !text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(match types.resolved(path, text) {
        None | Some(Value::Null) => Arg::Null,
        Some(v) => {
            if let Some(p) = path_value(Some(v)) {
                Arg::Path(p)
            } else if let Some(n) = num(Some(v)) {
                Arg::Num(n as f32)
            } else {
                Arg::Text(v.as_str()?.to_owned())
            }
        }
    })
}

/// Positional and named arguments after the element (`None`: absent or not understood).
fn bind(types: &DmTypes, path: &str, args: &[String], names: &[&str]) -> Vec<Option<Arg>> {
    let mut out: Vec<Option<Arg>> = vec![None; names.len()];
    for (i, arg) in args.iter().skip(1).enumerate() {
        let (slot, text) = match arg.split_once('=') {
            Some((name, value)) if !name.contains('(') && !name.contains('!') => {
                let Some(slot) = names.iter().position(|n| *n == name.trim()) else {
                    continue; // screentip_verb, sound_to_play: not needed
                };
                (slot, value)
            }
            _ => (i, arg.as_str()),
        };
        if slot < names.len() {
            out[slot] = arg_value(types, path, text);
        }
    }
    out
}

/// The nearest type in `path`'s chain that defines `proc` (where its element calls are).
fn proc_owner<'a>(types: &'a DmTypes, path: &'a str, proc: &str) -> Option<&'a str> {
    types
        .chain(path)
        .find(|p| types.types[*p].procs.iter().any(|q| q == proc))
}

fn calls<'a>(
    types: &'a DmTypes,
    element: &str,
    owner: &'a str,
    proc: &'a str,
) -> impl Iterator<Item = &'a str> + 'a {
    types
        .element_calls
        .get(element)
        .into_iter()
        .flatten()
        .filter(move |c| {
            matches!(c.call.as_str(), "AddElement" | "AddComponent")
                && c.owner == owner
                && c.proc == proc
        })
        .map(|c| c.line.as_str())
}

/// `make_processable()` of the nearest override, plus `Initialize()` calls up the chain.
fn processing(types: &DmTypes, path: &str) -> Vec<Processing> {
    let mut lines: Vec<&str> = Vec::new();
    if let Some(owner) = proc_owner(types, path, "make_processable") {
        lines.extend(calls(types, PROCESSABLE, owner, "make_processable"));
    }
    for ancestor in types.chain(path) {
        lines.extend(calls(types, PROCESSABLE, ancestor, "Initialize"));
    }
    let names = [
        "tool_behaviour",
        "result_atom_type",
        "amount_created",
        "time_to_process",
        "table_required",
    ];
    lines
        .into_iter()
        .filter_map(|line| {
            let args = bind(types, path, &call_args(line)?, &names);
            let Some(Arg::Text(tool)) = args[0].clone() else {
                return None;
            };
            let Some(Arg::Path(result)) = args[1].clone() else {
                return None;
            };
            let number = |arg: &Option<Arg>, default: f32| match arg {
                Some(Arg::Num(n)) => *n,
                _ => default,
            };
            Some(Processing {
                tool,
                result,
                amount: number(&args[2], DEFAULT_PROCESS_AMOUNT as f32).max(0.0) as u32,
                time: number(&args[3], DEFAULT_PROCESS_TIME),
                table_required: args[4].as_ref().is_some_and(Arg::truthy),
            })
        })
        .collect()
}

/// `make_microwaveable()` of the nearest override: the result and whether it is a real recipe.
fn microwave(types: &DmTypes, path: &str) -> Option<(String, bool)> {
    let owner = proc_owner(types, path, "make_microwaveable")?;
    let line = calls(types, MICROWAVABLE, owner, "make_microwaveable").last()?;
    let names = ["microwave_type", "reagents", "bad_recipe"];
    let args = bind(types, path, &call_args(line)?, &names);
    let Some(Arg::Path(result)) = args[0].clone() else {
        return None;
    };
    let bad = args[2].as_ref().is_some_and(Arg::truthy);
    Some((result, !bad))
}

/// `make_bakeable()`/`make_grillable()` of the nearest override: `AddComponent(component,
/// result, rand(low, high), positive_result, ...)`.
// shortcut: no `added_reagents` (donk pockets' omnizine), no multi-line calls (raw pancakes);
// grilling by hot air (`on_turf_atmos_changed`) is not ported.
fn cook(types: &DmTypes, path: &str, component: &str, proc: &str, names: [&str; 3]) -> Option<Cook> {
    let owner = proc_owner(types, path, proc)?;
    let line = calls(types, component, owner, proc).last()?;
    let args = bind(types, path, &call_args(line)?, &names);
    let Some(Arg::Path(result)) = args[0].clone() else {
        return None;
    };
    let time = match args[1] {
        Some(Arg::Range(low, high)) => [low, high],
        Some(Arg::Num(n)) => [n, n],
        _ => return None,
    };
    Some(Cook {
        result,
        time,
        good: args[2].as_ref().is_some_and(Arg::truthy),
    })
}

/// The `/datum/food_processor_process` that takes `path` in a plain food processor: tg files
/// every recipe under `typesof(input) - typesof(blacklist)`, later (more specific) recipes over
/// earlier ones; the most specific input wins here.
fn processor_recipe<'a>(types: &'a DmTypes, path: &str) -> Option<&'a str> {
    types
        .types
        .keys()
        .filter(|r| is_under(r, PROCESSOR_RECIPE_ROOT) && r.as_str() != PROCESSOR_RECIPE_ROOT)
        .filter_map(|r| {
            let input = path_value(types.resolved(r, "input"))?;
            let machine = path_value(types.resolved(r, "required_machine"))
                .unwrap_or_else(|| PROCESSOR.to_owned());
            if machine != PROCESSOR || !is_under(path, &input) {
                return None;
            }
            let blacklist = amounts(types.resolved(r, "blacklist")).unwrap_or_default();
            if blacklist.iter().any(|(b, _)| is_under(path, b)) {
                return None;
            }
            Some((input.len(), r.as_str()))
        })
        .max()
        .map(|(_, r)| r)
}

/// The resolved food model of `path`; `None` for other types.
pub fn food(types: &DmTypes, path: &str) -> Option<DmFood> {
    if !is_under(path, FOOD_ROOT) || !types.types.contains_key(path) {
        return None;
    }
    let get = |var: &str| types.resolved(path, var);
    // A declared but unset var (`var/bite_consumption`) is null: the component's default.
    let n = |var: &str| match get(var) {
        None | Some(Value::Null) => None,
        v => num(v).map(|v| v as f32),
    };
    let mut food = DmFood {
        food_reagents: amounts(get("food_reagents"))
            .unwrap_or_default()
            .into_iter()
            .map(|(path, amount)| ReagentAmount { path, amount })
            .collect(),
        volume: n("max_volume")
            .filter(|v| *v > 0.0)
            .unwrap_or(DEFAULT_EDIBLE_VOLUME),
        purity: n("starting_reagent_purity").unwrap_or(DEFAULT_PURITY),
        bite_consumption: n("bite_consumption").unwrap_or(DEFAULT_BITE),
        eat_time: n("eat_time").unwrap_or(DEFAULT_EAT_TIME),
        tastes: tastes(get("tastes")),
        eatverbs: texts(get("eatverbs")),
        junkiness: n("junkiness").unwrap_or(0.0),
        food_flags: n("food_flags").unwrap_or(0.0) as u32,
        trash_type: path_value(get("trash_type")).unwrap_or_default(),
        processing: processing(types, path),
        ..default()
    };
    if food.eatverbs.is_empty() {
        food.eatverbs = DEFAULT_EATVERBS.iter().map(|v| (*v).to_owned()).collect();
    }
    if let Some((result, good)) = microwave(types, path) {
        food.microwaved = result;
        food.microwave_good = good;
    }
    if let Some(c) = cook(
        types,
        path,
        BAKEABLE,
        "make_bakeable",
        ["bake_result", "required_bake_time", "positive_result"],
    ) {
        (food.bake_result, food.bake_time, food.bake_good) = (c.result, c.time, c.good);
    }
    if let Some(c) = cook(
        types,
        path,
        GRILLABLE,
        "make_grillable",
        ["cook_result", "required_cook_time", "positive_result"],
    ) {
        (food.grill_result, food.grill_time, food.grill_good) = (c.result, c.time, c.good);
    }
    if let Some(recipe) = processor_recipe(types, path) {
        let r = |var: &str| types.resolved(recipe, var);
        food.processor_output = path_value(r("output")).unwrap_or_default();
        food.processor_multiplier = num(r("food_multiplier")).unwrap_or(1.0) as u32;
        food.processor_time = num(r("time")).unwrap_or(40.0) as f32;
    }
    if is_under(path, GROWN_ROOT) {
        grown(types, path, &mut food);
    }
    Some(food)
}

/// `/obj/item/food/grown/Initialize` of a map-placed item: its seed at potency 50.
fn grown(types: &DmTypes, path: &str, food: &mut DmFood) {
    if food.tastes.is_empty() {
        let name = types
            .resolved(path, "name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        food.tastes = vec![Taste {
            text: name.to_owned(),
            weight: 1.0,
        }];
    }
    let mod_ = num(types.resolved(path, "bite_consumption_mod")).unwrap_or(1.0) as f32;
    // DM `round(x)` is a floor.
    food.bite_consumption = 1.0
        + ((MAP_POTENCY * BITE_SIZE_POTENCY_MULTIPLIER).max(1.0)
            * (food.volume * BITE_SIZE_VOLUME_MULTIPLIER)
            * mod_)
            .floor();
    let Some(seed) = path_value(types.resolved(path, "seed")) else {
        return;
    };
    let s = |var: &str| num(types.resolved(&seed, var)).unwrap_or(0.0) as f32;
    // `get_reagent_purity()` without the instability roll.
    food.purity = (0.5 + s("lifespan") / 400.0 + s("endurance") / 400.0).clamp(0.0, 1.0);
    let adds = amounts(types.resolved(&seed, "reagents_add")).unwrap_or_default();
    let sum: f32 = adds.iter().map(|(_, pct)| pct).sum();
    food.food_reagents = adds
        .into_iter()
        .map(|(path, pct)| ReagentAmount {
            path,
            amount: (food.volume * (MAP_POTENCY / 100.0) * pct * (1.0 / sum).min(1.0))
                .floor()
                .max(1.0),
        })
        .collect();
}

/// Every type food on the map can turn into (microwave and processor results, slices, trash),
/// transitively, plus the burned mess and the grinder's beaker.
pub fn food_closure<'a>(
    types: &DmTypes,
    needed: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack: Vec<String> = needed.into_iter().map(str::to_owned).collect();
    let mut seen = BTreeSet::new();
    while let Some(path) = stack.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let mut next = Vec::new();
        if is_under(&path, GRINDER) {
            next.push(GRINDER_BEAKER.to_owned());
        }
        if is_under(&path, OVEN) {
            next.push(OVEN_TRAY.to_owned());
        }
        // A soda can that bursts leaves a crushed can (`soda_cans/throw_impact`).
        if is_under(&path, SODA_CANS) {
            next.push(CRUSHED_CAN.to_owned());
        }
        if let Some(f) = food(types, &path) {
            next.push(BAD_RECIPE.to_owned());
            next.extend([
                f.microwaved,
                f.processor_output,
                f.trash_type,
                f.bake_result,
                f.grill_result,
            ]);
            next.extend(f.processing.into_iter().map(|p| p.result));
        }
        for p in next {
            if types.types.contains_key(&p) && out.insert(p.clone()) {
                stack.push(p);
            }
        }
    }
    out
}

/// A reflected `[f32; 2]` is a RON tuple.
fn pair([a, b]: [f32; 2]) -> String {
    format!("({a:?}, {b:?})")
}

/// Converter fields of a food type: the whole [`DmFood`].
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let Some(f) = food(types, path) else {
        return Vec::new();
    };
    let s = |text: &str| ron::to_string(text).expect("strings serialize");
    let c = FOOD_COMPONENT;
    let reagents: Vec<String> = f
        .food_reagents
        .iter()
        .map(|r| format!("(path: {}, amount: {:?})", s(&r.path), r.amount))
        .collect();
    let tastes: Vec<String> = f
        .tastes
        .iter()
        .map(|t| format!("(text: {}, weight: {:?})", s(&t.text), t.weight))
        .collect();
    let verbs: Vec<String> = f.eatverbs.iter().map(|v| s(v)).collect();
    let processing: Vec<String> = f
        .processing
        .iter()
        .map(|p| {
            format!(
                "(tool: {}, result: {}, amount: {}, time: {:?}, table_required: {})",
                s(&p.tool),
                s(&p.result),
                p.amount,
                p.time,
                p.table_required
            )
        })
        .collect();
    vec![
        ext(c, "food_reagents", format!("[{}]", reagents.join(", "))),
        ext(c, "volume", format!("{:?}", f.volume)),
        ext(c, "purity", format!("{:?}", f.purity)),
        ext(c, "bite_consumption", format!("{:?}", f.bite_consumption)),
        ext(c, "eat_time", format!("{:?}", f.eat_time)),
        ext(c, "tastes", format!("[{}]", tastes.join(", "))),
        ext(c, "eatverbs", format!("[{}]", verbs.join(", "))),
        ext(c, "junkiness", format!("{:?}", f.junkiness)),
        ext(c, "food_flags", f.food_flags.to_string()),
        ext(c, "trash_type", s(&f.trash_type)),
        ext(c, "microwaved", s(&f.microwaved)),
        ext(c, "microwave_good", f.microwave_good.to_string()),
        ext(c, "processing", format!("[{}]", processing.join(", "))),
        ext(c, "processor_output", s(&f.processor_output)),
        ext(
            c,
            "processor_multiplier",
            f.processor_multiplier.to_string(),
        ),
        ext(c, "processor_time", format!("{:?}", f.processor_time)),
        ext(c, "bake_result", s(&f.bake_result)),
        ext(c, "bake_time", pair(f.bake_time)),
        ext(c, "bake_good", f.bake_good.to_string()),
        ext(c, "grill_result", s(&f.grill_result)),
        ext(c, "grill_time", pair(f.grill_time)),
        ext(c, "grill_good", f.grill_good.to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/datum": { "parent": null },
        "/datum/food_processor_process": { "parent": "/datum", "declared_vars": {
            "input": null, "output": null, "time": 40, "food_multiplier": 1,
            "required_machine": { "path": "/obj/machinery/processor" }, "blacklist": null } },
        "/datum/food_processor_process/meat": { "parent": "/datum/food_processor_process",
            "declared_vars": { "input": { "path": "/obj/item/food/meat/slab" },
            "output": { "path": "/obj/item/food/raw_meatball" }, "food_multiplier": 3,
            "blacklist": { "list": [{ "path": "/obj/item/food/meat/slab/human" }] } } },
        "/datum/food_processor_process/meat/human": { "parent": "/datum/food_processor_process/meat",
            "declared_vars": { "input": { "path": "/obj/item/food/meat/slab/human" },
            "output": { "path": "/obj/item/food/raw_meatball/human" }, "blacklist": null } },
        "/datum/food_processor_process/slime": { "parent": "/datum/food_processor_process",
            "declared_vars": { "input": { "path": "/obj/item/food/meat/slab" },
            "output": { "path": "/obj/item/food/egg" },
            "required_machine": { "path": "/obj/machinery/processor/slime" } } },
        "/atom": { "parent": "/datum" },
        "/obj": { "parent": "/atom" },
        "/obj/item": { "parent": "/obj", "declared_vars": { "name": "item" } },
        "/obj/item/food": { "parent": "/obj/item", "declared_vars": {
            "food_reagents": null, "max_volume": null, "bite_consumption": null,
            "eat_time": null, "tastes": null, "eatverbs": null, "trash_type": null,
            "junkiness": null, "food_flags": null, "starting_reagent_purity": null },
            "procs": ["make_microwaveable", "make_processable", "make_bakeable", "make_grillable"] },
        "/obj/item/food/dough": { "parent": "/obj/item/food", "procs": ["make_bakeable"] },
        "/obj/item/food/donkpocket": { "parent": "/obj/item/food", "declared_vars": {
            "warm_type": { "path": "/obj/item/food/bread/plain" }, "baking_time_short": 250,
            "baking_time_long": 300 }, "procs": ["make_bakeable"] },
        "/obj/item/food/badrecipe": { "parent": "/obj/item/food", "declared_vars": {
            "name": "burned mess" } },
        "/obj/item/food/bread": { "parent": "/obj/item/food", "declared_vars": {
            "max_volume": 80, "eat_time": 30, "slice_type": null, "yield": 5,
            "tastes": { "list": [{ "assoc": ["bread", 10] }] } },
            "procs": ["make_processable"] },
        "/obj/item/food/bread/plain": { "parent": "/obj/item/food/bread", "declared_vars": {
            "food_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/consumable/nutriment" }, 10] }] },
            "slice_type": { "path": "/obj/item/food/breadslice/plain" } } },
        "/obj/item/food/bread/empty": { "parent": "/obj/item/food/bread" },
        "/obj/item/food/breadslice/plain": { "parent": "/obj/item/food" },
        "/obj/item/food/egg": { "parent": "/obj/item/food", "procs": ["make_microwaveable"] },
        "/obj/item/food/boiledegg": { "parent": "/obj/item/food", "declared_vars": {
            "trash_type": { "path": "/obj/item/trash/shell" } } },
        "/obj/item/trash/shell": { "parent": "/obj/item" },
        "/obj/item/food/meat/slab": { "parent": "/obj/item/food", "procs": ["make_processable"] },
        "/obj/item/food/meat/slab/human": { "parent": "/obj/item/food/meat/slab" },
        "/obj/item/food/meat/slab/corgi": { "parent": "/obj/item/food/meat/slab" },
        "/obj/item/food/meat/rawcutlet/plain": { "parent": "/obj/item/food" },
        "/obj/item/food/raw_meatball": { "parent": "/obj/item/food" },
        "/obj/item/food/raw_meatball/human": { "parent": "/obj/item/food/raw_meatball" },
        "/obj/item/food/grown": { "parent": "/obj/item/food", "declared_vars": {
            "max_volume": 100, "bite_consumption_mod": 1, "seed": null } },
        "/obj/item/food/grown/potato": { "parent": "/obj/item/food/grown", "declared_vars": {
            "name": "potato", "seed": { "path": "/obj/item/seeds/potato" } } },
        "/obj/item/seeds/potato": { "parent": "/obj/item", "declared_vars": {
            "lifespan": 30, "endurance": 15,
            "reagents_add": { "list": [
                { "assoc": [{ "path": "/datum/reagent/consumable/nutriment/vitamin" }, 0.04] },
                { "assoc": [{ "path": "/datum/reagent/consumable/nutriment" }, 0.1] }] } } }
      },
      "element_calls": {
        "/datum/element/microwavable": [
          { "type": "/obj/item/food", "proc": "make_microwaveable", "call": "AddElement",
            "line": "AddElement(/datum/element/microwavable, /obj/item/food/badrecipe, bad_recipe = TRUE)" },
          { "type": "/obj/item/food/egg", "proc": "make_microwaveable", "call": "AddElement",
            "line": "AddElement(/datum/element/microwavable, /obj/item/food/boiledegg)" }
        ],
        "/datum/component/bakeable": [
          { "type": "/obj/item/food", "proc": "make_bakeable", "call": "AddComponent",
            "line": "AddComponent(/datum/component/bakeable, /obj/item/food/badrecipe, rand(25 SECONDS, 40 SECONDS), FALSE)" },
          { "type": "/obj/item/food/dough", "proc": "make_bakeable", "call": "AddComponent",
            "line": "AddComponent(/datum/component/bakeable, /obj/item/food/bread/plain, rand(30 SECONDS, 45 SECONDS), TRUE, TRUE)" },
          { "type": "/obj/item/food/donkpocket", "proc": "make_bakeable", "call": "AddComponent",
            "line": "AddComponent(/datum/component/bakeable, warm_type, rand(baking_time_short, baking_time_long), TRUE, TRUE, list(/datum/reagent/medicine/omnizine = 6))" }
        ],
        "/datum/component/grillable": [
          { "type": "/obj/item/food", "proc": "make_grillable", "call": "AddComponent",
            "line": "AddComponent(/datum/component/grillable, /obj/item/food/badrecipe, rand(20 SECONDS, 30 SECONDS), FALSE)" }
        ],
        "/datum/element/processable": [
          { "type": "/obj/item/food/bread", "proc": "make_processable", "call": "AddElement",
            "line": "AddElement(/datum/element/processable, TOOL_KNIFE, slice_type, yield, 3 SECONDS, table_required = TRUE, screentip_verb = \"Slice\", sound_to_play = SFX_KNIFE_SLICE)" },
          { "type": "/obj/item/food/meat/slab", "proc": "make_processable", "call": "AddElement",
            "line": "AddElement(/datum/element/processable, TOOL_KNIFE,  /obj/item/food/meat/rawcutlet/plain, MEATSLAB_PROCESSED_AMOUNT, 3 SECONDS, table_required = TRUE, screentip_verb = \"Cut\")" }
        ]
      }
    }"##;

    fn types() -> DmTypes {
        DmTypes::parse(TYPES.as_bytes()).unwrap()
    }

    #[test]
    fn microwave_recipes_follow_make_microwaveable_overrides() {
        let types = types();
        let egg = food(&types, "/obj/item/food/egg").unwrap();
        assert_eq!(
            (egg.microwaved.as_str(), egg.microwave_good),
            ("/obj/item/food/boiledegg", true)
        );
        let bread = food(&types, "/obj/item/food/bread/plain").unwrap();
        assert_eq!(
            (bread.microwaved.as_str(), bread.microwave_good),
            (BAD_RECIPE, false),
            "everything else burns"
        );
        // Unset vars take the edible component's defaults.
        let slice = food(&types, "/obj/item/food/breadslice/plain").unwrap();
        assert_eq!(
            (
                slice.volume,
                slice.bite_consumption,
                slice.eat_time,
                slice.purity
            ),
            (50.0, 2.0, 10.0, 0.5)
        );
        assert_eq!(slice.eatverbs.len(), 6);
        assert_eq!(bread.volume, 80.0);
        assert_eq!(bread.tastes[0].text, "bread");
    }

    #[test]
    fn bake_and_grill_components_follow_the_nearest_override() {
        let types = types();
        let dough = food(&types, "/obj/item/food/dough").unwrap();
        assert_eq!(
            (dough.bake_result.as_str(), dough.bake_time, dough.bake_good),
            ("/obj/item/food/bread/plain", [300.0, 450.0], true)
        );
        // `rand(baking_time_short, baking_time_long)` reads the type's vars.
        let pocket = food(&types, "/obj/item/food/donkpocket").unwrap();
        assert_eq!(
            (pocket.bake_result.as_str(), pocket.bake_time),
            ("/obj/item/food/bread/plain", [250.0, 300.0])
        );
        // Everything else burns: `/obj/item/food`'s own procs.
        let egg = food(&types, "/obj/item/food/egg").unwrap();
        assert_eq!(
            (egg.bake_result.as_str(), egg.bake_time, egg.bake_good),
            (BAD_RECIPE, [250.0, 400.0], false)
        );
        assert_eq!(
            (egg.grill_result.as_str(), egg.grill_time, egg.grill_good),
            (BAD_RECIPE, [200.0, 300.0], false)
        );
    }

    #[test]
    fn processable_lines_resolve_with_the_types_vars() {
        let types = types();
        let bread = food(&types, "/obj/item/food/bread/plain").unwrap();
        assert_eq!(
            bread.processing,
            [Processing {
                tool: "knife".into(),
                result: "/obj/item/food/breadslice/plain".into(),
                amount: 5,
                time: 30.0,
                table_required: true,
            }]
        );
        assert!(
            food(&types, "/obj/item/food/bread/empty")
                .unwrap()
                .processing
                .is_empty(),
            "no slice_type: no element"
        );
        let slab = food(&types, "/obj/item/food/meat/slab/corgi").unwrap();
        assert_eq!(slab.processing[0].amount, 3, "MEATSLAB_PROCESSED_AMOUNT");
    }

    #[test]
    fn processor_recipes_match_like_the_typecache() {
        let types = types();
        let slab = food(&types, "/obj/item/food/meat/slab/corgi").unwrap();
        assert_eq!(slab.processor_output, "/obj/item/food/raw_meatball");
        assert_eq!((slab.processor_multiplier, slab.processor_time), (3, 40.0));
        // Blacklisted by the parent recipe, taken by its own; the slime processor's recipe for
        // the same input is not a plain processor's.
        let human = food(&types, "/obj/item/food/meat/slab/human").unwrap();
        assert_eq!(human.processor_output, "/obj/item/food/raw_meatball/human");
        assert!(
            food(&types, "/obj/item/food/egg")
                .unwrap()
                .processor_output
                .is_empty()
        );
    }

    #[test]
    fn grown_food_gets_its_seed_at_potency_50() {
        let types = types();
        let potato = food(&types, "/obj/item/food/grown/potato").unwrap();
        // max(1, floor(100 * 0.5 * pct * min(1, 1 / 0.14))).
        assert_eq!(potato.food_reagents[0].amount, 2.0);
        assert_eq!(potato.food_reagents[1].amount, 5.0);
        // 1 + floor(max(50 * 0.05, 1) * (100 * 0.01) * 1).
        assert_eq!(potato.bite_consumption, 3.0);
        assert_eq!(potato.tastes[0].text, "potato");
        assert!((potato.purity - (0.5 + 30.0 / 400.0 + 15.0 / 400.0)).abs() < 1e-6);
    }

    #[test]
    fn closure_reaches_results_and_their_trash() {
        let types = types();
        let closure = food_closure(&types, ["/obj/item/food/egg", "/obj/item/food/bread/plain"]);
        for p in [
            "/obj/item/food/boiledegg",
            "/obj/item/trash/shell",
            BAD_RECIPE,
            "/obj/item/food/breadslice/plain",
        ] {
            assert!(closure.contains(p), "{p}: {closure:?}");
        }
        let ron = crate::value::patch_ron(&type_fields(&types, "/obj/item/food/egg"));
        assert!(
            ron.contains(r#"microwaved: "/obj/item/food/boiledegg""#),
            "{ron}"
        );
    }
}
