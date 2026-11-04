//! Guns, ammunition and armor as prototype data: the vars of `/obj/item/gun`
//! (`code/modules/projectiles/gun.dm`, `guns/ballistic.dm`, `guns/energy.dm`), the casings and
//! magazines they use (`ammunition/_ammunition.dm`, `boxes_magazines/_box_magazine.dm`), the
//! projectiles those casings fire (`projectile.dm`) and the firing pin (`pins.dm`). Every
//! reference is resolved here: a gun carries its casings' projectiles, so the server never
//! looks a type up by path. [`gun_closure`] adds the types spawned at runtime (magazines,
//! casings dropped on the floor, projectiles).

use std::collections::BTreeSet;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, truthy};
use crate::value::Field;

pub const GUN_ROOT: &str = "/obj/item/gun";
pub const BALLISTIC_ROOT: &str = "/obj/item/gun/ballistic";
pub const ENERGY_ROOT: &str = "/obj/item/gun/energy";
pub const AMMO_BOX_ROOT: &str = "/obj/item/ammo_box";
pub const CASING_ROOT: &str = "/obj/item/ammo_casing";
pub const PIN_ROOT: &str = "/obj/item/firing_pin";

const GUN_COMPONENT: &str = "dm_gun";
const MAGAZINE_COMPONENT: &str = "dm_magazine";
const CASING_COMPONENT: &str = "dm_casing";
const ARMOR_COMPONENT: &str = "dm_armor";
pub const CLOTHING_ROOT: &str = "/obj/item/clothing";

/// `/obj/projectile` vars a hit reads. Times in deciseconds, as in DM.
#[derive(Reflect, Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ProjectileData {
    /// Empty: no projectile (a spent casing).
    pub path: String,
    pub damage: f32,
    /// `"brute"`, `"burn"`, `"stamina"`, ...
    pub damage_type: String,
    /// The armor rating it is checked against: `"bullet"`, `"laser"`, `"energy"`, ...
    pub armor_flag: String,
    pub armour_penetration: f32,
    pub weak_against_armour: bool,
    pub stamina: f32,
    pub stun: f32,
    pub knockdown: f32,
    pub paralyze: f32,
    pub immobilize: f32,
    /// Tiles per decisecond (`SSprojectiles.pixels_per_decisecond` is one tile).
    pub speed: f32,
    /// Tiles it flies before `on_range`.
    pub range: f32,
    /// `PASSTABLE` 1, `PASSGLASS` 2, `PASSGRILLE` 4.
    pub pass_flags: u32,
    pub accurate_range: f32,
    pub accuracy_falloff: f32,
    /// Ricochets (`ricochets_max`, `ricochet_chance` percent, the `ricochet_decay_*`
    /// multipliers, `ricochet_incidence_leeway` degrees).
    pub ricochets_max: u32,
    pub ricochet_chance: f32,
    pub ricochet_decay_chance: f32,
    pub ricochet_decay_damage: f32,
    pub ricochet_incidence_leeway: f32,
    /// Wounds: `wound_bonus` (`CANT_WOUND` -100), `exposed_wound_bonus`, `wound_falloff_tile`
    /// and `sharpness` (`SHARP_EDGED` 1, `SHARP_POINTY` 2).
    pub wound_bonus: f32,
    pub exposed_wound_bonus: f32,
    pub wound_falloff_tile: f32,
    pub sharpness: u32,
    /// `damage_falloff_tile`, `stamina_falloff_tile`: added per tile flown.
    pub damage_falloff_tile: f32,
    pub stamina_falloff_tile: f32,
    /// `/obj/projectile/energy/electrode` `tase_stamina`: 0 for anything else.
    pub tase_stamina: f32,
}

/// `/obj/item/ammo_casing` vars, with its projectile resolved.
#[derive(Reflect, Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct CasingData {
    pub path: String,
    pub caliber: String,
    pub projectile: ProjectileData,
    /// Energy casings: joules per shot and the fire mode's name.
    pub e_cost: f64,
    pub select_name: String,
    pub harmful: bool,
    /// Deciseconds; 0: `CLICK_CD_RANGE`.
    pub click_cooldown_override: f32,
    /// An energy gun's `fire_delay` in this mode (`delay`), deciseconds.
    pub delay: f32,
    /// Energy casings: the gun's `fire_sound` in this mode (`fire_sound = shot.fire_sound`).
    pub fire_sound: String,
    /// `pellets` (at least 1), `variance` (degrees), `randomspread`.
    pub pellets: u32,
    pub variance: f32,
    pub randomspread: bool,
}

/// `/obj/item/ammo_box` (magazines, speedloaders, ammo boxes) vars, with its ammo resolved.
#[derive(Reflect, Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct MagazineData {
    /// Empty: none.
    pub path: String,
    pub caliber: String,
    pub max_ammo: u32,
    pub ammo: CasingData,
    pub start_empty: bool,
}

/// `/obj/item/gun` vars, deciseconds.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmGun {
    pub path: String,
    pub fire_delay: f32,
    pub burst_size: u32,
    pub burst_delay: f32,
    /// The firing pin type it starts with (`pin`); empty: none.
    pub pin: String,
    /// `pinless`: needs no pin.
    pub pinless: bool,
    /// `default_pin_auth` and `fail_message` of `pin`.
    pub pin_default_auth: bool,
    pub pin_fail_message: String,
    /// `/obj/item/gun/ballistic`.
    pub ballistic: bool,
    pub accepted_magazine: String,
    /// `spawn_magazine_type` (or `accepted_magazine_type`), resolved; empty path without one.
    pub magazine: MagazineData,
    pub spawnwithmagazine: bool,
    pub internal_magazine: bool,
    /// `BOLT_TYPE_STANDARD` 1, `OPEN` 2, `NO_BOLT` 3, `LOCKING` 4.
    pub bolt_type: u32,
    pub casing_ejector: bool,
    pub semi_auto: bool,
    /// `/obj/item/gun/energy`: its `ammo_type` list, cell and charging.
    pub energy: bool,
    pub shots: Vec<CasingData>,
    pub cell_maxcharge: f64,
    pub cell_chargerate: f64,
    pub can_charge: bool,
    /// Degrees: `spread`, `dual_wield_spread`; `randomspread` (bursts).
    pub spread: f32,
    pub randomspread: bool,
    pub dual_wield_spread: f32,
    /// `WEAPON_LIGHT` 1, `WEAPON_MEDIUM` 2, `WEAPON_HEAVY` 3 (needs both hands).
    pub weapon_weight: u32,
    /// `recoil` (tiles of camera kick), `recoil_backtime_multiplier`, `recoil_deviation`.
    pub recoil: f32,
    pub recoil_backtime_multiplier: f32,
    pub recoil_deviation: f32,
    /// `min_recoil`: the least kick (the `min_recoil_multiplier` preference at its default,
    /// 100%).
    pub min_recoil: f32,
    /// `projectile_damage_multiplier`.
    pub projectile_damage_multiplier: f32,
    /// `/obj/item/gun/ballistic/shotgun/automatic`: racks itself after a shot.
    pub auto_rack: bool,
    pub sounds: GunSounds,
}

/// The sound vars of `/obj/item/gun` and `/obj/item/gun/ballistic`: files or SFX keys (empty:
/// none), volumes and `vary`.
#[derive(Reflect, Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct GunSounds {
    pub fire_sound: String,
    pub fire_sound_volume: f32,
    pub vary_fire_sound: bool,
    pub dry_fire_sound: String,
    pub dry_fire_sound_volume: f32,
    pub load_sound: String,
    pub load_empty_sound: String,
    pub load_sound_volume: f32,
    pub load_sound_vary: bool,
    pub rack_sound: String,
    pub rack_sound_volume: f32,
    pub rack_sound_vary: bool,
    pub lock_back_sound: String,
    pub lock_back_sound_volume: f32,
    pub lock_back_sound_vary: bool,
    pub eject_sound: String,
    pub eject_empty_sound: String,
    pub eject_sound_volume: f32,
    pub eject_sound_vary: bool,
    pub bolt_drop_sound: String,
    pub bolt_drop_sound_volume: f32,
    pub click_on_low_ammo: bool,
    /// Ballistic `empty_alarm`: the last round fired plays `empty_alarm_sound`.
    pub empty_alarm: bool,
    pub empty_alarm_sound: String,
    pub empty_alarm_volume: f32,
    pub empty_alarm_vary: bool,
}

pub fn gun_sounds(types: &DmTypes, path: &str) -> GunSounds {
    let s = |var| crate::sound::sound_var(types, path, var);
    let n = |var| num(types, path, var) as f32;
    let b = |var| truthy(types.resolved(path, var));
    GunSounds {
        fire_sound: s("fire_sound"),
        fire_sound_volume: n("fire_sound_volume"),
        vary_fire_sound: b("vary_fire_sound"),
        dry_fire_sound: s("dry_fire_sound"),
        dry_fire_sound_volume: n("dry_fire_sound_volume"),
        load_sound: s("load_sound"),
        load_empty_sound: s("load_empty_sound"),
        load_sound_volume: n("load_sound_volume"),
        load_sound_vary: b("load_sound_vary"),
        rack_sound: s("rack_sound"),
        rack_sound_volume: n("rack_sound_volume"),
        rack_sound_vary: b("rack_sound_vary"),
        lock_back_sound: s("lock_back_sound"),
        lock_back_sound_volume: n("lock_back_sound_volume"),
        lock_back_sound_vary: b("lock_back_sound_vary"),
        eject_sound: s("eject_sound"),
        eject_empty_sound: s("eject_empty_sound"),
        eject_sound_volume: n("eject_sound_volume"),
        eject_sound_vary: b("eject_sound_vary"),
        bolt_drop_sound: s("bolt_drop_sound"),
        bolt_drop_sound_volume: n("bolt_drop_sound_volume"),
        click_on_low_ammo: b("click_on_low_ammo"),
        empty_alarm: b("empty_alarm"),
        empty_alarm_sound: s("empty_alarm_sound"),
        empty_alarm_volume: n("empty_alarm_volume"),
        empty_alarm_vary: b("empty_alarm_vary"),
    }
}

/// A loose magazine, speedloader or ammo box.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmMagazine {
    pub data: MagazineData,
}

/// A loose casing.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmCasing {
    pub data: CasingData,
}

/// The ratings of a piece of clothing's `armor_type` (`/datum/armor`), percent. Only clothing
/// protects its wearer (tg `get_worn_bodypart_armor_value`).
#[derive(Component, Reflect, Serialize, Deserialize, Default, Clone, Copy, Debug, PartialEq)]
pub struct DmArmor {
    pub melee: f32,
    pub bullet: f32,
    pub laser: f32,
    pub energy: f32,
    pub bomb: f32,
    pub bio: f32,
    pub fire: f32,
    pub acid: f32,
    pub wound: f32,
}

impl DmArmor {
    /// tg `get_rating(flag)`: `"melee"`, `"bullet"`, `"laser"`, ...
    pub fn rating(&self, flag: &str) -> f32 {
        match flag {
            "melee" => self.melee,
            "bullet" => self.bullet,
            "laser" => self.laser,
            "energy" => self.energy,
            "bomb" => self.bomb,
            "bio" => self.bio,
            "fire" => self.fire,
            "acid" => self.acid,
            "wound" => self.wound,
            _ => 0.0,
        }
    }
}

pub fn armor(types: &DmTypes, path: &str) -> DmArmor {
    let datum = type_var(types, path, "armor_type");
    if datum.is_empty() {
        return DmArmor::default();
    }
    let n = |var| num(types, &datum, var) as f32;
    DmArmor {
        melee: n("melee"),
        bullet: n("bullet"),
        laser: n("laser"),
        energy: n("energy"),
        bomb: n("bomb"),
        bio: n("bio"),
        fire: n("fire"),
        acid: n("acid"),
        wound: n("wound"),
    }
}

fn num(types: &DmTypes, path: &str, var: &str) -> f64 {
    match types.resolved(path, var) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::Object(o)) => o
            .get("expr")
            .and_then(Value::as_str)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

fn text(types: &DmTypes, path: &str, var: &str) -> String {
    types
        .resolved(path, var)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// A type-path var (`{"path": ...}` or a constant `{"expr": "/a/b"}`); empty for null.
fn type_var(types: &DmTypes, path: &str, var: &str) -> String {
    match types.resolved(path, var) {
        Some(Value::Object(o)) => o
            .get("path")
            .or_else(|| o.get("expr"))
            .and_then(Value::as_str)
            .filter(|p| p.starts_with('/') && types.types.contains_key(*p))
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    }
}

fn type_list(types: &DmTypes, path: &str, var: &str) -> Vec<String> {
    match types.resolved(path, var) {
        Some(Value::Object(o)) => match o.get("list") {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|v| v.get("path").and_then(Value::as_str))
                .filter(|p| types.types.contains_key(*p))
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

pub fn projectile(types: &DmTypes, path: &str) -> ProjectileData {
    if path.is_empty() {
        return ProjectileData::default();
    }
    let n = |var| num(types, path, var) as f32;
    ProjectileData {
        path: path.to_owned(),
        damage: n("damage"),
        damage_type: text(types, path, "damage_type"),
        armor_flag: text(types, path, "armor_flag"),
        armour_penetration: n("armour_penetration"),
        weak_against_armour: truthy(types.resolved(path, "weak_against_armour")),
        stamina: n("stamina"),
        stun: n("stun"),
        knockdown: n("knockdown"),
        paralyze: n("paralyze"),
        immobilize: n("immobilize"),
        speed: n("speed"),
        range: n("range"),
        pass_flags: num(types, path, "pass_flags") as u32,
        accurate_range: n("accurate_range"),
        accuracy_falloff: n("accuracy_falloff"),
        ricochets_max: n("ricochets_max").max(0.0) as u32,
        ricochet_chance: n("ricochet_chance"),
        ricochet_decay_chance: n("ricochet_decay_chance"),
        ricochet_decay_damage: n("ricochet_decay_damage"),
        ricochet_incidence_leeway: n("ricochet_incidence_leeway"),
        wound_bonus: n("wound_bonus"),
        exposed_wound_bonus: n("exposed_wound_bonus"),
        wound_falloff_tile: n("wound_falloff_tile"),
        sharpness: n("sharpness").max(0.0) as u32,
        damage_falloff_tile: n("damage_falloff_tile"),
        stamina_falloff_tile: n("stamina_falloff_tile"),
        tase_stamina: if is_under(path, "/obj/projectile/energy/electrode") {
            n("tase_stamina")
        } else {
            0.0
        },
    }
}

pub fn casing(types: &DmTypes, path: &str) -> CasingData {
    if path.is_empty() {
        return CasingData::default();
    }
    CasingData {
        path: path.to_owned(),
        caliber: text(types, path, "caliber"),
        projectile: projectile(types, &type_var(types, path, "projectile_type")),
        e_cost: num(types, path, "e_cost"),
        select_name: text(types, path, "select_name"),
        harmful: truthy(types.resolved(path, "harmful")),
        click_cooldown_override: num(types, path, "click_cooldown_override") as f32,
        delay: num(types, path, "delay") as f32,
        fire_sound: crate::sound::sound_var(types, path, "fire_sound"),
        pellets: num(types, path, "pellets").max(1.0) as u32,
        variance: num(types, path, "variance") as f32,
        randomspread: truthy(types.resolved(path, "randomspread")),
    }
}

pub fn magazine(types: &DmTypes, path: &str) -> MagazineData {
    if path.is_empty() {
        return MagazineData::default();
    }
    MagazineData {
        path: path.to_owned(),
        caliber: text(types, path, "caliber"),
        max_ammo: num(types, path, "max_ammo") as u32,
        ammo: casing(types, &type_var(types, path, "ammo_type")),
        start_empty: truthy(types.resolved(path, "start_empty")),
    }
}

pub fn gun(types: &DmTypes, path: &str) -> DmGun {
    let n = |var| num(types, path, var);
    let pin = type_var(types, path, "pin");
    let ballistic = is_under(path, BALLISTIC_ROOT);
    let energy = is_under(path, ENERGY_ROOT);
    let accepted = type_var(types, path, "accepted_magazine_type");
    let spawn = Some(type_var(types, path, "spawn_magazine_type"))
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| accepted.clone());
    let cell = type_var(types, path, "cell_type");
    DmGun {
        path: path.to_owned(),
        fire_delay: n("fire_delay") as f32,
        burst_size: n("burst_size").max(1.0) as u32,
        burst_delay: n("burst_delay") as f32,
        pin_default_auth: pin.is_empty() || truthy(types.resolved(&pin, "default_pin_auth")),
        pin_fail_message: if pin.is_empty() {
            String::new()
        } else {
            text(types, &pin, "fail_message")
        },
        pin,
        pinless: truthy(types.resolved(path, "pinless")),
        ballistic,
        accepted_magazine: if ballistic { accepted } else { String::new() },
        magazine: if ballistic {
            magazine(types, &spawn)
        } else {
            MagazineData::default()
        },
        spawnwithmagazine: ballistic && truthy(types.resolved(path, "spawnwithmagazine")),
        internal_magazine: ballistic && truthy(types.resolved(path, "internal_magazine")),
        bolt_type: if ballistic { n("bolt_type") as u32 } else { 0 },
        casing_ejector: ballistic && truthy(types.resolved(path, "casing_ejector")),
        semi_auto: ballistic && truthy(types.resolved(path, "semi_auto")),
        energy,
        shots: if energy {
            type_list(types, path, "ammo_type")
                .iter()
                .map(|c| casing(types, c))
                .collect()
        } else {
            Vec::new()
        },
        cell_maxcharge: if energy {
            num(types, &cell, "maxcharge")
        } else {
            0.0
        },
        cell_chargerate: if energy {
            num(types, &cell, "chargerate")
        } else {
            0.0
        },
        can_charge: energy && truthy(types.resolved(path, "can_charge")),
        spread: n("spread") as f32,
        randomspread: truthy(types.resolved(path, "randomspread")),
        dual_wield_spread: n("dual_wield_spread") as f32,
        weapon_weight: n("weapon_weight") as u32,
        recoil: n("recoil") as f32,
        recoil_backtime_multiplier: n("recoil_backtime_multiplier") as f32,
        recoil_deviation: n("recoil_deviation") as f32,
        min_recoil: n("min_recoil") as f32,
        projectile_damage_multiplier: n("projectile_damage_multiplier") as f32,
        auto_rack: is_under(path, "/obj/item/gun/ballistic/shotgun/automatic"),
        sounds: gun_sounds(types, path),
    }
}

fn ron<T: Serialize>(value: &T) -> String {
    ron::to_string(value).expect("prototype data always serializes")
}

/// Converter fields of `path`.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if !types.types.contains_key(path) {
        return out;
    }
    if is_under(path, GUN_ROOT) {
        let g = gun(types, path);
        let c = GUN_COMPONENT;
        out.extend([
            ext(c, "path", ron(&g.path)),
            ext(c, "fire_delay", ron(&g.fire_delay)),
            ext(c, "burst_size", ron(&g.burst_size)),
            ext(c, "burst_delay", ron(&g.burst_delay)),
            ext(c, "pin", ron(&g.pin)),
            ext(c, "pinless", ron(&g.pinless)),
            ext(c, "pin_default_auth", ron(&g.pin_default_auth)),
            ext(c, "pin_fail_message", ron(&g.pin_fail_message)),
            ext(c, "ballistic", ron(&g.ballistic)),
            ext(c, "accepted_magazine", ron(&g.accepted_magazine)),
            ext(c, "magazine", ron(&g.magazine)),
            ext(c, "spawnwithmagazine", ron(&g.spawnwithmagazine)),
            ext(c, "internal_magazine", ron(&g.internal_magazine)),
            ext(c, "bolt_type", ron(&g.bolt_type)),
            ext(c, "casing_ejector", ron(&g.casing_ejector)),
            ext(c, "semi_auto", ron(&g.semi_auto)),
            ext(c, "energy", ron(&g.energy)),
            ext(c, "shots", ron(&g.shots)),
            ext(c, "cell_maxcharge", ron(&g.cell_maxcharge)),
            ext(c, "cell_chargerate", ron(&g.cell_chargerate)),
            ext(c, "can_charge", ron(&g.can_charge)),
            ext(c, "spread", ron(&g.spread)),
            ext(c, "randomspread", ron(&g.randomspread)),
            ext(c, "dual_wield_spread", ron(&g.dual_wield_spread)),
            ext(c, "weapon_weight", ron(&g.weapon_weight)),
            ext(c, "recoil", ron(&g.recoil)),
            ext(
                c,
                "recoil_backtime_multiplier",
                ron(&g.recoil_backtime_multiplier),
            ),
            ext(c, "recoil_deviation", ron(&g.recoil_deviation)),
            ext(c, "min_recoil", ron(&g.min_recoil)),
            ext(
                c,
                "projectile_damage_multiplier",
                ron(&g.projectile_damage_multiplier),
            ),
            ext(c, "auto_rack", ron(&g.auto_rack)),
            ext(c, "sounds", ron(&g.sounds)),
        ]);
    }
    if is_under(path, AMMO_BOX_ROOT) {
        out.push(ext(MAGAZINE_COMPONENT, "data", ron(&magazine(types, path))));
    }
    if is_under(path, CASING_ROOT) {
        out.push(ext(CASING_COMPONENT, "data", ron(&casing(types, path))));
    }
    // Clothing protects its wearer; any item's own armor softens what hits it (explosions).
    if is_under(path, crate::item::ITEM_ROOT) {
        // Prototypes inherit their parent's component: only a change of ratings is written.
        let own = armor(types, path);
        let parent = types.types[path]
            .parent
            .as_deref()
            .filter(|p| is_under(p, crate::item::ITEM_ROOT))
            .map(|p| armor(types, p))
            .unwrap_or_default();
        if own != parent {
            for (key, value) in [
                ("melee", own.melee),
                ("bullet", own.bullet),
                ("laser", own.laser),
                ("energy", own.energy),
                ("bomb", own.bomb),
                ("bio", own.bio),
                ("fire", own.fire),
                ("acid", own.acid),
                ("wound", own.wound),
            ] {
                out.push(ext(ARMOR_COMPONENT, key, ron(&value)));
            }
        }
    }
    out
}

/// Types guns spawn at runtime: magazines they eject, casings they drop, the projectiles they
/// fire; and the same for loose magazines and casings.
pub fn gun_closure<'a>(
    types: &DmTypes,
    needed: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let add_casing = |out: &mut BTreeSet<String>, c: &CasingData| {
        for p in [&c.path, &c.projectile.path] {
            if !p.is_empty() {
                out.insert(p.clone());
            }
        }
    };
    for path in needed {
        if !types.types.contains_key(path) {
            continue;
        }
        if is_under(path, GUN_ROOT) {
            let g = gun(types, path);
            if !g.internal_magazine && !g.magazine.path.is_empty() {
                out.insert(g.magazine.path.clone());
            }
            add_casing(&mut out, &g.magazine.ammo);
            for shot in &g.shots {
                add_casing(&mut out, shot);
            }
        } else if is_under(path, AMMO_BOX_ROOT) {
            add_casing(&mut out, &magazine(types, path).ammo);
        } else if is_under(path, CASING_ROOT) {
            add_casing(&mut out, &casing(types, path));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/obj": { "parent": null },
        "/obj/projectile": { "parent": "/obj", "declared_vars": { "damage": 10,
            "damage_type": "brute", "armor_flag": "bullet", "speed": 1.25, "range": 50,
            "pass_flags": 1, "accurate_range": 100, "accuracy_falloff": 7 } },
        "/obj/projectile/beam": { "parent": "/obj/projectile", "declared_vars": {
            "damage": 20, "damage_type": "burn", "armor_flag": "laser", "pass_flags": 7 } },
        "/obj/projectile/beam/disabler": { "parent": "/obj/projectile/beam", "declared_vars": {
            "damage": 30, "damage_type": "stamina", "armor_flag": "energy", "speed": 1.6 } },
        "/obj/projectile/bullet/c9mm": { "parent": "/obj/projectile", "declared_vars": {
            "damage": 30 } },
        "/obj/item": { "parent": "/obj" },
        "/obj/item/firing_pin": { "parent": "/obj/item", "declared_vars": {
            "default_pin_auth": 1, "fail_message": "недопустимый пользователь!" } },
        "/obj/item/ammo_casing": { "parent": "/obj/item", "declared_vars": { "harmful": 1,
            "projectile_type": null, "caliber": null } },
        "/obj/item/ammo_casing/c9mm": { "parent": "/obj/item/ammo_casing", "declared_vars": {
            "caliber": "9mm", "projectile_type": { "path": "/obj/projectile/bullet/c9mm" } } },
        "/obj/item/ammo_casing/energy": { "parent": "/obj/item/ammo_casing", "declared_vars": {
            "e_cost": 1000, "select_name": "energy" } },
        "/obj/item/ammo_casing/energy/disabler": { "parent": "/obj/item/ammo_casing/energy",
            "declared_vars": { "e_cost": 500, "harmful": 0, "select_name": "disable",
            "projectile_type": { "path": "/obj/projectile/beam/disabler" } } },
        "/obj/item/ammo_box": { "parent": "/obj/item", "declared_vars": { "max_ammo": 7,
            "start_empty": 0, "ammo_type": { "path": "/obj/item/ammo_casing" } } },
        "/obj/item/ammo_box/magazine/m9mm": { "parent": "/obj/item/ammo_box", "declared_vars": {
            "max_ammo": 12, "caliber": "9mm", "ammo_type": { "path": "/obj/item/ammo_casing/c9mm" } } },
        "/obj/item/stock_parts/power_store/cell": { "parent": "/obj/item", "declared_vars": {
            "maxcharge": 10000, "chargerate": 1000 } },
        "/obj/item/gun": { "parent": "/obj/item", "declared_vars": { "fire_delay": 0,
            "burst_size": 1, "pin": { "path": "/obj/item/firing_pin" }, "pinless": 0 } },
        "/obj/item/gun/ballistic": { "parent": "/obj/item/gun", "declared_vars": {
            "bolt_type": 1, "casing_ejector": 1, "semi_auto": 1, "spawnwithmagazine": 1,
            "internal_magazine": 0, "spawn_magazine_type": null,
            "accepted_magazine_type": { "path": "/obj/item/ammo_box/magazine/m9mm" } } },
        "/obj/item/gun/ballistic/automatic/pistol": { "parent": "/obj/item/gun/ballistic",
            "declared_vars": { "bolt_type": 4 } },
        "/obj/item/gun/energy": { "parent": "/obj/item/gun", "declared_vars": { "can_charge": 1,
            "cell_type": { "path": "/obj/item/stock_parts/power_store/cell" },
            "ammo_type": { "list": [ { "path": "/obj/item/ammo_casing/energy" } ] } } },
        "/obj/item/gun/energy/disabler": { "parent": "/obj/item/gun/energy", "declared_vars": {
            "ammo_type": { "list": [ { "path": "/obj/item/ammo_casing/energy/disabler" } ] } } }
      }
    }"##;

    #[test]
    fn guns_resolve_their_ammo_projectiles_and_pins() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let pistol = gun(&types, "/obj/item/gun/ballistic/automatic/pistol");
        assert!(pistol.ballistic && !pistol.energy && pistol.pin_default_auth);
        assert_eq!(pistol.bolt_type, 4);
        assert_eq!(pistol.magazine.max_ammo, 12);
        assert_eq!(pistol.magazine.ammo.projectile.damage, 30.0);
        assert_eq!(pistol.magazine.ammo.projectile.armor_flag, "bullet");
        let disabler = gun(&types, "/obj/item/gun/energy/disabler");
        assert_eq!(disabler.cell_maxcharge, 10000.0);
        assert_eq!(disabler.shots.len(), 1);
        let shot = &disabler.shots[0];
        assert_eq!((shot.e_cost, shot.harmful), (500.0, false));
        assert_eq!(shot.projectile.damage_type, "stamina");
        assert_eq!(shot.projectile.pass_flags, 7);
        assert_eq!(shot.projectile.speed, 1.6);
        let ron = crate::value::patch_ron(&type_fields(&types, "/obj/item/gun/energy/disabler"));
        assert!(ron.contains("select_name:\"disable\""), "{ron}");
        let closure = gun_closure(&types, ["/obj/item/gun/ballistic/automatic/pistol"]);
        assert!(closure.contains("/obj/item/ammo_box/magazine/m9mm"));
        assert!(closure.contains("/obj/item/ammo_casing/c9mm"));
        assert!(closure.contains("/obj/projectile/bullet/c9mm"));
    }
}
