//! Text adventures of exploration sites, after tg `code/modules/explorer_drone/adventure.dm`:
//! the JSON format of `strings/exoadventures/*.json` (`/datum/adventure_db_entry`), its loading
//! and validation (`try_loading_adventure`) and the play state (`/datum/adventure`): nodes with
//! choices, qualities changed by effects, requirement groups, triggers and the special nodes
//! (`WIN`, `FAIL`, `FAIL_DEATH`, `GO BACK`).
//!
//! The engine is plain data: the drone drives it ([`Adventure::start`], [`Adventure::select_choice`],
//! [`Adventure::finish_delay`]) and reads [`Adventure::result`] after each call, where tg sends
//! `COMSIG_ADVENTURE_*` signals.
// Not tg: a choice whose delayed exit ends the adventure stays ended (tg's `finish_delay` sends
// `COMSIG_ADVENTURE_DELAY_END` after the node change and puts a finished drone back into the
// adventure state).

use std::collections::{BTreeMap, HashMap};

use serde_json::Value;
use ss13_mapload::spawner::Rng;

/// `CURRENT_ADVENTURE_VERSION`.
pub const CURRENT_VERSION: i64 = 1;
/// `WIN_NODE`: loot and exit.
pub const WIN_NODE: &str = "WIN";
/// `FAIL_NODE`: no loot, half the drone's integrity, exit.
pub const FAIL_NODE: &str = "FAIL";
/// `FAIL_DEATH_NODE`: the drone is destroyed.
pub const FAIL_DEATH_NODE: &str = "FAIL_DEATH";
/// `GO_BACK_NODE`: the previous node.
pub const GO_BACK_NODE: &str = "GO BACK";

/// Nested navigations (triggers moving to nodes whose effects fire triggers) deeper than this
/// stop; tg's `trigger_loop_safety` stops repeats of one trigger, this stops cycles through
/// `on_enter` effects as well.
const MAX_DEPTH: u32 = 32;

/// `ADVENTURE_RESULT_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdventureResult {
    Success,
    Damage,
    Death,
}

/// An adventure file (`/datum/adventure_db_entry` with its extracted metadata).
#[derive(Clone, Debug)]
pub struct AdventureEntry {
    pub filename: String,
    pub raw: Value,
    pub version: i64,
    pub name: String,
    pub required_site_traits: Vec<String>,
    /// Used for a site this round.
    pub placed: bool,
}

impl AdventureEntry {
    /// `load_adventures` + `extract_metadata`. `None` for text that is not a JSON object.
    pub fn parse(filename: &str, text: &str) -> Option<Self> {
        let raw: Value = serde_json::from_str(text).ok()?;
        if !raw.is_object() {
            return None;
        }
        Some(Self {
            filename: filename.to_owned(),
            version: raw.get("version").and_then(Value::as_i64).unwrap_or(0),
            name: str_of(&raw, "adventure_name"),
            required_site_traits: strings(&raw, "required_site_traits"),
            raw,
            placed: false,
        })
    }

    /// `valid_for_use(site_traits)`.
    pub fn valid_for_use(&self, site_traits: &[&str]) -> bool {
        self.version == CURRENT_VERSION
            && !self.placed
            && self
                .required_site_traits
                .iter()
                .all(|t| site_traits.contains(&t.as_str()))
    }

    /// `create_adventure` -> `try_loading_adventure`. `None` when a required field is missing,
    /// a node is duplicated or has no choices (tg `CRASH`es).
    pub fn create(&self) -> Option<Adventure> {
        if self.version != CURRENT_VERSION {
            return None;
        }
        let raw = &self.raw;
        for field in ["adventure_name", "starting_node", "nodes"] {
            if !truthy(raw.get(field)) {
                return None;
            }
        }
        let mut nodes = HashMap::new();
        for node in raw.get("nodes")?.as_array()? {
            let node = Node::parse(node)?;
            if nodes.contains_key(&node.id) {
                return None;
            }
            nodes.insert(node.id.clone(), node);
        }
        // `validate`: every node has choices.
        if nodes.values().any(|n| n.choices.is_empty()) {
            return None;
        }
        Some(Adventure {
            name: str_of(raw, "adventure_name"),
            starting_node: str_of(raw, "starting_node"),
            band_modifiers: raw
                .get("scan_band_mods")
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| Some((k.clone(), v.as_f64()?)))
                        .collect()
                })
                .unwrap_or_default(),
            loot_categories: strings(raw, "loot_categories"),
            starting_qualities: raw
                .get("starting_qualities")
                .and_then(Value::as_object)
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
            deep_scan_description: raw
                .get("deep_scan_description")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            nodes,
            triggers: raw
                .get("triggers")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            current_node: None,
            previous_node_id: None,
            qualities: BTreeMap::new(),
            trigger_loop_safety: Vec::new(),
            result: None,
        })
    }
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn str_of(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// `/datum/adventure_node`.
#[derive(Clone, Debug)]
pub struct Node {
    pub id: String,
    pub description: String,
    pub image_name: Option<String>,
    /// Choices in file order, keyed by `key`.
    pub choices: Vec<(String, Value)>,
    pub on_enter_effects: Option<Value>,
    pub on_exit_effects: Option<Value>,
}

impl Node {
    fn parse(v: &Value) -> Option<Self> {
        if !v.is_object() {
            return None;
        }
        let mut choices: Vec<(String, Value)> = Vec::new();
        for choice in v
            .get("choices")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let key = match choice.get("key") {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Number(n)) => n.to_string(),
                _ => continue,
            };
            // `choices[key] = choice_data`: a repeated key replaces the earlier one in place.
            match choices.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = choice.clone(),
                None => choices.push((key, choice.clone())),
            }
        }
        let effects = |key: &str| v.get(key).filter(|e| !e.is_null()).cloned();
        Some(Self {
            id: str_of(v, "name"),
            description: str_of(v, "description"),
            image_name: v.get("image").and_then(Value::as_str).map(str::to_owned),
            choices,
            on_enter_effects: effects("on_enter_effects"),
            on_exit_effects: effects("on_exit_effects"),
        })
    }
}

/// What a choice asks of the drone.
#[derive(Clone, Debug, PartialEq)]
pub enum ChoiceOutcome {
    /// The node changed (or the adventure ended: see [`Adventure::result`]).
    Done,
    /// `delay` deciseconds of busy time with `message`, then [`Adventure::finish_delay`].
    Delay {
        delay_ds: f64,
        message: Option<String>,
        exit: String,
    },
}

/// `/datum/adventure` with its play state.
#[derive(Clone, Debug)]
pub struct Adventure {
    pub name: String,
    pub starting_node: String,
    pub band_modifiers: Vec<(String, f64)>,
    pub loot_categories: Vec<String>,
    pub starting_qualities: Vec<(String, Value)>,
    pub deep_scan_description: Option<String>,
    pub nodes: HashMap<String, Node>,
    pub triggers: Vec<Value>,
    pub current_node: Option<String>,
    pub previous_node_id: Option<String>,
    pub qualities: BTreeMap<String, Value>,
    trigger_loop_safety: Vec<String>,
    /// Set when the adventure reached an end node.
    pub result: Option<AdventureResult>,
}

impl Adventure {
    /// `start_adventure`: the starting qualities plus one per drone tool
    /// (`COMSIG_ADVENTURE_QUALITY_INIT` -> `add_tool_qualities`), then the starting node.
    pub fn start(&mut self, tools: &[&str], rng: &mut Rng) {
        self.qualities = self.starting_qualities.iter().cloned().collect();
        for tool in tools {
            self.qualities.insert((*tool).to_owned(), Value::from(1));
        }
        self.previous_node_id = Some(self.starting_node.clone());
        let start = self.starting_node.clone();
        self.navigate(&start, rng, 0);
    }

    pub fn node(&self) -> Option<&Node> {
        self.nodes.get(self.current_node.as_deref()?)
    }

    /// `navigate_to_node`. True when an effect or trigger moved elsewhere on the way.
    fn navigate(&mut self, node_id: &str, rng: &mut Rng, depth: u32) -> bool {
        if self.result.is_some() || depth > MAX_DEPTH {
            return true;
        }
        // Not tg: tg updates `previous_node_id` to the current node before resolving `GO BACK`,
        // which then returns to the node it left from; the node before it is meant.
        let back = self.previous_node_id.clone();
        if let Some(current) = self.current_node.clone() {
            let exit = self
                .nodes
                .get(&current)
                .and_then(|n| n.on_exit_effects.clone());
            if let Some(effects) = exit
                && self.apply_effects(&effects, true, rng, depth)
            {
                return true;
            }
            if self.previous_node_id.as_deref() != Some(current.as_str()) {
                self.previous_node_id = Some(current);
            }
        }
        match node_id {
            FAIL_NODE => {
                self.result = Some(AdventureResult::Damage);
                return true;
            }
            FAIL_DEATH_NODE => {
                self.result = Some(AdventureResult::Death);
                return true;
            }
            WIN_NODE => {
                self.result = Some(AdventureResult::Success);
                return true;
            }
            _ => {}
        }
        let node_id = match (node_id, back) {
            (GO_BACK_NODE, Some(back)) => back,
            _ => node_id.to_owned(),
        };
        let node_id = node_id.as_str();
        // tg `stack_trace`s and nulls the node; staying keeps the adventure playable.
        if !self.nodes.contains_key(node_id) {
            return false;
        }
        self.current_node = Some(node_id.to_owned());
        let enter = self.nodes[node_id].on_enter_effects.clone();
        enter.is_some_and(|effects| self.apply_effects(&effects, true, rng, depth))
    }

    /// `select_choice`. `None`: no such choice here, or its requirements fail.
    pub fn select_choice(&mut self, key: &str, rng: &mut Rng) -> Option<ChoiceOutcome> {
        if self.result.is_some() {
            return None;
        }
        let choice = self
            .node()?
            .choices
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, c)| c.clone())?;
        if !choice.is_object() || !self.check_requirements(choice.get("requirements")) {
            return None;
        }
        if let Some(effects) = choice.get("on_selection_effects").filter(|e| !e.is_null())
            && self.apply_effects(effects, true, rng, 0)
        {
            return Some(ChoiceOutcome::Done);
        }
        let exit = match choice.get("exit_node") {
            Some(Value::String(s)) if !s.is_empty() => s.clone(),
            _ => return None,
        };
        if let Some(delay) = choice.get("delay").filter(|d| truthy(Some(*d))) {
            let delay_ds = delay.as_f64()?;
            return Some(ChoiceOutcome::Delay {
                delay_ds,
                message: choice
                    .get("delay_message")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                exit,
            });
        }
        self.navigate(&exit, rng, 0);
        Some(ChoiceOutcome::Done)
    }

    /// `finish_delay(exit_id)`.
    pub fn finish_delay(&mut self, exit: &str, rng: &mut Rng) {
        self.navigate(exit, rng, 0);
    }

    /// `get_available_choices`: `(key, text)` of the choices whose requirements hold.
    pub fn available_choices(&self) -> Vec<(String, String)> {
        let Some(node) = self.node() else {
            return Vec::new();
        };
        node.choices
            .iter()
            .filter(|(_, c)| self.check_requirements(c.get("requirements")))
            .map(|(k, c)| (k.clone(), str_of(c, "name")))
            .collect()
    }

    /// `apply_adventure_effect`. True when a trigger moved to another node. Malformed effects
    /// are skipped (tg `CRASH`es out of the proc).
    fn apply_effects(
        &mut self,
        effects: &Value,
        process_triggers: bool,
        rng: &mut Rng,
        depth: u32,
    ) -> bool {
        let Some(list) = effects.as_array() else {
            return false;
        };
        for effect in list {
            let quality = match effect.get("quality") {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Number(n)) => n.to_string(),
                _ => continue,
            };
            let value = effect.get("value").map(|v| process_value(v, rng));
            match effect.get("effect_type").and_then(Value::as_str) {
                Some("Remove") => {
                    self.qualities.remove(&quality);
                }
                Some("Add") => {
                    let Some(add) = value.as_ref().and_then(Value::as_f64) else {
                        continue;
                    };
                    let old = self.qualities.get(&quality).map_or(0.0, number);
                    self.qualities.insert(quality, number_value(old + add));
                }
                Some("Set") => {
                    self.qualities.insert(quality, value.unwrap_or(Value::Null));
                }
                _ => {}
            }
        }
        if process_triggers {
            for trigger in self.triggers.clone() {
                if !self.check_requirements(trigger.get("requirements")) {
                    continue;
                }
                let name = str_of(&trigger, "name");
                if self.trigger_loop_safety.contains(&name) {
                    continue;
                }
                self.trigger_loop_safety.push(name);
                if let Some(effects) = trigger.get("on_trigger_effects").filter(|e| !e.is_null()) {
                    self.apply_effects(effects, false, rng, depth);
                }
                if let Some(target) = trigger.get("target_node").and_then(Value::as_str) {
                    self.navigate(target, rng, depth + 1);
                    return true;
                }
            }
        }
        self.trigger_loop_safety.clear();
        false
    }

    /// `check_requirements`: the top level is an AND of groups and single requirements.
    pub fn check_requirements(&self, raw: Option<&Value>) -> bool {
        let Some(list) = raw.and_then(Value::as_array) else {
            return true;
        };
        list.iter().all(|r| self.check_entry(r))
    }

    fn check_entry(&self, entry: &Value) -> bool {
        if truthy(entry.get("requirements")) {
            self.check_group(entry)
        } else {
            self.check_single(entry)
        }
    }

    /// `check_requirement_group`: `OR` / `AND`; anything else fails.
    fn check_group(&self, group: &Value) -> bool {
        let elements: &[Value] = group
            .get("requirements")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        match group.get("group_type").and_then(Value::as_str) {
            Some("OR") => elements.iter().any(|e| self.check_entry(e)),
            Some("AND") => elements.iter().all(|e| self.check_entry(e)),
            _ => false,
        }
    }

    /// `check_single_requirement`: DM comparisons, a missing quality being `null` (0 in numeric
    /// comparisons).
    fn check_single(&self, req: &Value) -> bool {
        let key = match req.get("quality") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        };
        let have = self.qualities.get(&key).cloned().unwrap_or(Value::Null);
        let want = req.get("value").cloned().unwrap_or(Value::Null);
        let order = || dm_number(&have).zip(dm_number(&want));
        match req.get("operator").and_then(Value::as_str) {
            Some("==") => dm_equal(&have, &want),
            Some("!=") => !dm_equal(&have, &want),
            Some(">") => order().is_some_and(|(a, b)| a > b),
            Some(">=") => order().is_some_and(|(a, b)| a >= b),
            Some("<=") => order().is_some_and(|(a, b)| a <= b),
            Some("<") => order().is_some_and(|(a, b)| a < b),
            Some("exists") => self.qualities.contains_key(&key),
            _ => false,
        }
    }
}

/// `process_adventure_value`: `{"value_type": "random", "low", "high"}` rolls `rand(low, high)`.
fn process_value(raw: &Value, rng: &mut Rng) -> Value {
    if raw.is_object() {
        if raw.get("value_type").and_then(Value::as_str) == Some("random") {
            let low = raw.get("low").map_or(0.0, number) as i64;
            let high = raw.get("high").map_or(0.0, number) as i64;
            return Value::from(rng.range(low, high));
        }
        return Value::Null;
    }
    raw.clone()
}

fn number(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

fn number_value(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9e15 {
        Value::from(n as i64)
    } else {
        Value::from(n)
    }
}

/// A value as DM's numeric comparisons see it: `null` is 0, text does not compare.
fn dm_number(v: &Value) -> Option<f64> {
    match v {
        Value::Null => Some(0.0),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

/// DM `==`: numbers by value, text by content, `null` only equals `null`.
fn dm_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

/// `$$SITE_NAME` and `$$<quality>` in adventure text (`/obj/item/exodrone/proc/updateKeywords`).
pub fn replace_keywords(
    text: &str,
    site_name: &str,
    qualities: &BTreeMap<String, Value>,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("$$") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let end = after.find(char::is_whitespace).unwrap_or(after.len());
        let key = &after[..end];
        if key == "SITE_NAME" {
            out.push_str(site_name);
        } else if let Some(v) = qualities.get(key).filter(|v| truthy(Some(*v))) {
            match v {
                Value::String(s) => out.push_str(s),
                other => out.push_str(&other.to_string()),
            }
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "adventure_name": "Test", "version": 1, "starting_node": "start",
        "starting_qualities": {"Love": 3},
        "required_site_traits": ["in space"],
        "loot_categories": ["cash"],
        "scan_band_mods": {"Exotic Radiation": 10},
        "triggers": [{"name": "Love", "target_node": "WIN",
            "requirements": [{"quality": "Love", "operator": ">=", "value": 5}]}],
        "nodes": [
            {"name": "start", "description": "At $$SITE_NAME with $$Love love.", "choices": [
                {"key": "flirt", "name": "Flirt", "exit_node": "start",
                 "on_selection_effects": [{"effect_type": "Add", "quality": "Love", "value": 1}]},
                {"key": "leave", "name": "Leave", "exit_node": "FAIL", "delay": 30,
                 "delay_message": "Leaving..."},
                {"key": "laser", "name": "Shoot", "exit_node": "hall",
                 "requirements": [{"requirements": [
                    {"quality": "laser", "operator": "exists"},
                    {"quality": "Love", "operator": ">", "value": 100}],
                    "group_type": "OR"}]}
            ]},
            {"name": "hall", "description": "A hall.", "choices": [
                {"key": "back", "name": "Back", "exit_node": "GO BACK"},
                {"key": "boom", "name": "Boom", "exit_node": "FAIL_DEATH"}
            ]}
        ]
    }"#;

    fn adventure() -> Adventure {
        let entry = AdventureEntry::parse("t.json", SAMPLE).expect("json");
        assert!(entry.valid_for_use(&["in space", "ruins"]));
        assert!(!entry.valid_for_use(&["ruins"]));
        entry.create().expect("valid adventure")
    }

    #[test]
    fn choices_requirements_and_triggers() {
        let mut rng = Rng::new(1);
        let mut a = adventure();
        a.start(&["drill"], &mut rng);
        assert_eq!(a.current_node.as_deref(), Some("start"));
        // No laser: the OR group fails.
        let keys: Vec<String> = a.available_choices().into_iter().map(|c| c.0).collect();
        assert_eq!(keys, ["flirt", "leave"]);
        assert_eq!(a.select_choice("laser", &mut rng), None);
        assert_eq!(
            a.select_choice("flirt", &mut rng),
            Some(ChoiceOutcome::Done)
        );
        assert_eq!(a.qualities["Love"], Value::from(4));
        assert_eq!(a.result, None);
        // Love 5 fires the trigger to WIN.
        a.select_choice("flirt", &mut rng);
        assert_eq!(a.result, Some(AdventureResult::Success));
        assert_eq!(a.select_choice("flirt", &mut rng), None);
    }

    #[test]
    fn tools_delays_go_back_and_death() {
        let mut rng = Rng::new(2);
        let mut a = adventure();
        a.start(&["laser"], &mut rng);
        match a.select_choice("leave", &mut rng) {
            Some(ChoiceOutcome::Delay { delay_ds, exit, .. }) => {
                assert_eq!(delay_ds, 30.0);
                assert_eq!(exit, FAIL_NODE);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(a.result, None);
        a.select_choice("laser", &mut rng);
        assert_eq!(a.current_node.as_deref(), Some("hall"));
        a.select_choice("back", &mut rng);
        assert_eq!(a.current_node.as_deref(), Some("start"));
        a.finish_delay(FAIL_NODE, &mut rng);
        assert_eq!(a.result, Some(AdventureResult::Damage));

        let mut b = adventure();
        b.start(&["laser"], &mut rng);
        b.select_choice("laser", &mut rng);
        b.select_choice("boom", &mut rng);
        assert_eq!(b.result, Some(AdventureResult::Death));
    }

    #[test]
    fn keywords() {
        let mut q = BTreeMap::new();
        q.insert("Love".to_owned(), Value::from(3));
        assert_eq!(
            replace_keywords("At $$SITE_NAME with $$Love love, $$None.", "Mars", &q),
            "At Mars with 3 love, "
        );
    }

    #[test]
    fn broken_adventures_are_refused() {
        let no_choices = r#"{"adventure_name": "x", "version": 1, "starting_node": "a",
            "nodes": [{"name": "a", "description": "", "choices": []}]}"#;
        assert!(
            AdventureEntry::parse("x", no_choices)
                .unwrap()
                .create()
                .is_none()
        );
        let old = r#"{"adventure_name": "x", "version": 0, "starting_node": "a", "nodes": []}"#;
        assert!(!AdventureEntry::parse("x", old).unwrap().valid_for_use(&[]));
        assert!(AdventureEntry::parse("x", "[1]").is_none());
    }

    #[test]
    fn bundled_adventures_load() {
        let dir = std::path::Path::new(env!("SS13_SOURCE")).join("strings/exoadventures");
        let Ok(files) = std::fs::read_dir(&dir) else {
            return;
        };
        for file in files.flatten() {
            let text = std::fs::read_to_string(file.path()).unwrap();
            let entry = AdventureEntry::parse("f", &text).expect("json");
            let mut a = entry
                .create()
                .unwrap_or_else(|| panic!("{:?}", file.path()));
            a.start(&[], &mut Rng::new(3));
            assert!(a.node().is_some(), "{:?}", file.path());
        }
    }
}
