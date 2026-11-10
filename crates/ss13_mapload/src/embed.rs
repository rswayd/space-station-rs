//! Embedding vars as prototype data: `/datum/embedding` (`code/datums/embedding.dm`) of the
//! type an item's or a projectile's `embed_type` names, resolved through the datum's parents.
//! Items with an `embed_type` get [`DmEmbed`]; projectiles carry theirs in
//! [`ProjectileData`](crate::guns::ProjectileData).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, truthy};
use crate::value::Field;

pub const EMBED_COMPONENT: &str = "dm_embed";
pub const EMBED_ROOT: &str = "/datum/embedding";

/// What bullets and fragments leave in a body: the `shrapnel_type` of every projectile that
/// has one and is an item of its own (the game spawns them by path).
pub const SHRAPNEL_RUNTIME_TYPES: [&str; 5] = [
    "/obj/item/shrapnel",
    "/obj/item/shrapnel/bullet",
    "/obj/item/shrapnel/stingball",
    "/obj/item/shrapnel/capmine",
    "/obj/item/shard",
];

/// The vars of one `/datum/embedding` type. Chances are percents, `rip_time` deciseconds.
#[derive(Reflect, Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct EmbedData {
    /// The datum type; empty: does not embed.
    pub path: String,
    pub embed_chance: f32,
    pub fall_chance: f32,
    pub pain_chance: f32,
    pub pain_mult: f32,
    pub impact_pain_mult: f32,
    pub remove_pain_mult: f32,
    pub rip_time: f32,
    pub ignore_throwspeed_threshold: bool,
    pub jostle_chance: f32,
    pub jostle_pain_mult: f32,
    pub pain_stam_pct: f32,
    /// `immune_traits = null`: `TRAIT_PIERCEIMMUNE` does not protect (tape, patches).
    pub ignores_pierce_immunity: bool,
    pub stealthy_embed: bool,
}

impl EmbedData {
    pub fn embeds(&self) -> bool {
        !self.path.is_empty()
    }

    /// tg `is_harmless`.
    pub fn is_harmless(&self, consider_stamina: bool) -> bool {
        self.pain_mult == 0.0
            && self.jostle_pain_mult == 0.0
            && (consider_stamina || self.pain_stam_pct < 1.0)
    }
}

/// An item's `embed_type` with the item vars the embed rolls read.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmEmbed {
    pub data: EmbedData,
    pub armour_penetration: f32,
    pub weak_against_armour: bool,
}

fn num(types: &DmTypes, path: &str, var: &str) -> f32 {
    match types.resolved(path, var) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) as f32,
        Some(Value::Object(o)) => o
            .get("expr")
            .and_then(Value::as_str)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

/// The type a `embed_type`/`shrapnel_type` var of `path` names; empty for null.
pub fn type_var(types: &DmTypes, path: &str, var: &str) -> String {
    match types.resolved(path, var) {
        Some(Value::Object(o)) => o
            .get("path")
            .and_then(Value::as_str)
            .filter(|p| types.types.contains_key(*p))
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    }
}

/// The vars of the embedding datum `path` (empty or unknown: none).
pub fn embed_data(types: &DmTypes, path: &str) -> EmbedData {
    if !is_under(path, EMBED_ROOT) || !types.types.contains_key(path) {
        return EmbedData::default();
    }
    let n = |var| num(types, path, var);
    let b = |var| truthy(types.resolved(path, var));
    EmbedData {
        path: path.to_owned(),
        embed_chance: n("embed_chance"),
        fall_chance: n("fall_chance"),
        pain_chance: n("pain_chance"),
        pain_mult: n("pain_mult"),
        impact_pain_mult: n("impact_pain_mult"),
        remove_pain_mult: n("remove_pain_mult"),
        rip_time: n("rip_time"),
        ignore_throwspeed_threshold: b("ignore_throwspeed_threshold"),
        jostle_chance: n("jostle_chance"),
        jostle_pain_mult: n("jostle_pain_mult"),
        pain_stam_pct: n("pain_stam_pct"),
        ignores_pierce_immunity: matches!(
            types.resolved(path, "immune_traits"),
            None | Some(Value::Null)
        ),
        stealthy_embed: b("stealthy_embed"),
    }
}

/// Converter fields of `path`: [`DmEmbed`] on items with an `embed_type`.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    if !types.types.contains_key(path) || !is_under(path, "/obj/item") {
        return Vec::new();
    }
    let data = embed_data(types, &type_var(types, path, "embed_type"));
    if !data.embeds() {
        return Vec::new();
    }
    let c = EMBED_COMPONENT;
    vec![
        ext(
            c,
            "data",
            ron::to_string(&data).expect("prototype data always serializes"),
        ),
        ext(
            c,
            "armour_penetration",
            format!("{:?}", num(types, path, "armour_penetration")),
        ),
        ext(
            c,
            "weak_against_armour",
            truthy(types.resolved(path, "weak_against_armour")).to_string(),
        ),
    ]
}
