//! The Spider Clan's MOD modules (server), after tg `code/modules/mod/modules/modules_ninja.dm`
//! and the DNA lock of `modules_general.dm`, run through the hooks of [`super::modules`]:
//!
//! - cloaking (`stealth`, `stealth/ninja`): while on, the wearer's sprite fades to 50 (20) alpha
//!   at `use_energy_cost`; an attack on a living thing and any damage taken (tg: hits, bullets,
//!   thrown things) turn it off ([`unstealth`]) with sparks and another `use_energy_cost`. The ninja's makes footsteps silent
//!   (`TRAIT_SILENT_FOOTSTEPS`, [`SilentFootsteps`]).
//! - camera vision (`welding/camera_vision`) is a welding module ([`super::modules::Kind`]).
//! - the star dispenser (`dispenser/ninja`): used, an energy throwing star into the hand.
//! - the hacker (`hacker`): its right clicks are `crate::antag::raiders::ninja::hack`.
//! - weapon recall: sealed gloves forbid guns ([`NoGuns`]); used, it links the held katana or
//!   brings the linked one back into a hand, the belt or under the wearer (out of sight it costs
//!   `use_energy_cost` per tile).
//! - DNA lock (plain and reinforced): used, it locks to the wearer's `unique_enzymes`; another
//!   body cannot start the suit ("dna locked!"), the reinforced one gibs it. An EMP (the plain
//!   one) or an emag clears the lock.
//! - EMP pulse (`emp_shield/pulse`): the shield, and used, an `empulse(4, 6)`.
//! - energy net: selected, a middle click on a living mob throws a net that holds it
//!   ([`EnergyNet`]: buckled to the net until it is destroyed); the nets go when the gloves
//!   unseal.
//! - adrenaline boost: used (also while incapacitated), with its 20 units of radium, it clears
//!   every stun, 200 stamina damage and the stutter, injects 5 units of stimulants and uses up
//!   three quarters of the radium; 7 s later what is left goes into the wearer.
// shortcut: the plain cloak does not drop on bumping into a mob (no bump hook), no "Get over
// here!"/battle cry speech, no net projectile flight or beam (the net
// lands at once within its 9-tile range), the star dispenser's stars do not EMP what they hit,
// the adrenaline module cannot be refilled with radium (no reagent transfer into modules), the
// weapon is never thrown back in sight (it always teleports), the status readout's clock is
// not shown. Add them with their mechanics.

use bevy::prelude::*;
use ss13_mapload::modsuit::ModuleDef;
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::modules::{Kind, ModModule, def_of};
use super::server::{ModSuit, check_charge, subtract_charge, tell_mob};
use crate::health::Health;
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::tools::now;

const ROOT: &str = ss13_mapload::modsuit::MODULE_ROOT;
const STAR: &str = "/obj/item/throwing_star/stamina/ninja";
const NET: &str = "/obj/structure/energy_net";
const KATANA: &str = crate::antag::raiders::ninja::KATANA;
const STIMULANTS: &str = "/datum/reagent/medicine/stimulants";
const RADIUM: &str = "/datum/reagent/uranium/radium";
const EMPULSE_SOUND: &str = "sound/effects/empulse.ogg";
const NET_SOUND: &str = "sound/items/weapons/punchmiss.ogg";
const SPARKS_SOUND: &str = "sound/effects/sparks/sparks1.ogg";
/// The cloak's `stealth_alpha`.
const STEALTH_ALPHA: u8 = 50;
const NINJA_STEALTH_ALPHA: u8 = 20;
/// The adrenaline boost's `reagent_required_amount`.
pub const RADIUM_REQUIRED: f32 = 20.0;
/// `boost_aftereffects` timer, seconds.
const AFTEREFFECT_SECS: f64 = 7.0;
/// `/obj/projectile/energy_net` `range`.
const NET_RANGE: u32 = 9;

/// The ninja module kind of `def`, before the common ones.
pub fn kind_of(def: &ModuleDef) -> Option<Kind> {
    let p = def.path.as_str();
    let under = |root: &str| is_under(p, &format!("{ROOT}/{root}"));
    Some(if under("stealth/ninja") {
        Kind::Stealth { ninja: true }
    } else if under("stealth") {
        Kind::Stealth { ninja: false }
    } else if under("hacker") {
        Kind::Hacker
    } else if under("weapon_recall") {
        Kind::WeaponRecall
    } else if under("energy_net") {
        Kind::EnergyNet
    } else if under("adrenaline_boost") {
        Kind::AdrenalineBoost
    } else if under("emp_shield/pulse") {
        Kind::EmpPulse
    } else if under("dispenser/ninja") {
        Kind::StarDispenser
    } else if under("dna_lock/reinforced") {
        Kind::DnaLock { reinforced: true }
    } else if under("dna_lock") {
        Kind::DnaLock { reinforced: false }
    } else {
        return None;
    })
}

/// On a wearer: an active cloak (`module`), the sprite's color before it.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Cloaked {
    pub module: Entity,
    pub control: Entity,
    pub color: Option<[u8; 4]>,
    /// The wearer's brute + burn when it was last seen (a rise is a hit).
    pub damage: f32,
}

/// On a wearer: `TRAIT_SILENT_FOOTSTEPS`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct SilentFootsteps;

/// On a wearer: `TRAIT_NOGUNS` from the weapon recall module.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct NoGuns;

/// The weapon recall module's `linked_weapon`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkedWeapon(pub Entity);

/// The DNA lock's `dna` (`unique_enzymes`).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct DnaLocked(pub Option<String>);

/// The adrenaline boost's radium, units.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Radium(pub f32);

/// A boost's `boost_aftereffects`: `amount` of radium into `mob` at the game time.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BoostAftereffect {
    pub mob: Entity,
    pub at: f64,
    pub amount: f32,
}

/// An energy net (`/obj/structure/energy_net`): its module and the mob it holds.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnergyNet {
    pub module: Entity,
    pub victim: Entity,
}

pub struct NinjaModulesPlugin;

impl Plugin for NinjaModulesPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A cloaked wearer attacking something living drops the cloak (never takes the click).
        app.add_click_handler(ClickPhase::EmptyHand, 3, |w, c| {
            attack_breaks_cloak(w, c.mob, c.target);
            false
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 3, |w, c| {
            attack_breaks_cloak(w, c.mob, c.target);
            false
        });
        app.add_systems(
            Update,
            (cloak_hits, nets, aftereffects).before(crate::health::HealthSystems),
        );
    }
}

fn name(world: &World, e: Entity) -> String {
    crate::medical::name_of(world, e)
}

fn sparks(world: &mut World, at: Entity) {
    if let Some(pos) = crate::explosions::turf_of(world, at) {
        crate::effects::particles::do_sparks(world, 2, true, pos);
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SPARKS_SOUND, 50.0, true),
        );
    }
}

fn wearer(world: &World, control: Entity) -> Option<Entity> {
    world.get::<ModSuit>(control).and_then(|s| s.wearer)
}

/// `drain_power(amount)`.
fn drain(world: &mut World, control: Entity, amount: f64) -> bool {
    if !check_charge(world, control, amount) {
        return false;
    }
    subtract_charge(world, control, amount);
    true
}

// ---------------------------------------------------------------------------------------------
// Hooks from `super::modules`

/// `on_install`.
pub fn on_install(world: &mut World, _control: Entity, module: Entity, kind: Kind) {
    match kind {
        // `create_reagents` + `add_reagent(radium, 20)`.
        Kind::AdrenalineBoost if world.get::<Radium>(module).is_none() => {
            world.entity_mut(module).insert(Radium(RADIUM_REQUIRED));
        }
        Kind::DnaLock { .. } if world.get::<DnaLocked>(module).is_none() => {
            world.entity_mut(module).insert(DnaLocked::default());
        }
        _ => {}
    }
}

/// `on_activation` (the wearer is known).
pub fn on_activation(world: &mut World, control: Entity, module: Entity, kind: Kind) {
    let Kind::Stealth { ninja } = kind else {
        return;
    };
    let Some(mob) = wearer(world, control) else {
        return;
    };
    let alpha = if ninja {
        NINJA_STEALTH_ALPHA
    } else {
        STEALTH_ALPHA
    };
    let color = world
        .get::<ss13_shared::Appearance>(mob)
        .and_then(|a| a.color);
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(mob) {
        let [r, g, b, _] = color.unwrap_or([255; 4]);
        look.color = Some([r, g, b, alpha]);
    }
    let damage = world.get::<Health>(mob).map_or(0.0, |h| h.brute + h.burn);
    world.entity_mut(mob).insert(Cloaked {
        module,
        control,
        color,
        damage,
    });
    if ninja {
        world.entity_mut(mob).insert(SilentFootsteps);
    }
    let cost = def_of(world, module).map_or(0.0, |d| d.use_energy_cost);
    drain(world, control, cost);
}

/// `on_deactivation`: what `on_activation` gave the wearer.
pub fn strip(world: &mut World, module: Entity, kind: Kind, mob: Entity) {
    if let Kind::Stealth { .. } = kind
        && let Some(c) = world.get::<Cloaked>(mob).copied()
        && c.module == module
    {
        world.entity_mut(mob).remove::<(Cloaked, SilentFootsteps)>();
        if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(mob) {
            look.color = c.color;
        }
    }
}

/// `on_part_activation`.
pub fn on_part_activation(world: &mut World, control: Entity, _module: Entity, kind: Kind) {
    if kind == Kind::WeaponRecall
        && let Some(mob) = wearer(world, control)
    {
        world.entity_mut(mob).insert(NoGuns);
    }
}

/// `on_part_deactivation`.
pub fn on_part_deactivation(world: &mut World, control: Entity, module: Entity, kind: Kind) {
    match kind {
        Kind::WeaponRecall => {
            if let Some(mob) = wearer(world, control).filter(|&m| world.get_entity(m).is_ok()) {
                world.entity_mut(mob).remove::<NoGuns>();
            }
        }
        // `atom_destruction` of every net it made.
        Kind::EnergyNet => {
            let mut q = world.query::<(Entity, &EnergyNet)>();
            let nets: Vec<Entity> = q
                .iter(world)
                .filter(|(_, n)| n.module == module)
                .map(|(e, _)| e)
                .collect();
            for net in nets {
                destroy_net(world, net);
            }
        }
        _ => {}
    }
}

/// `used()` extras: the adrenaline boost needs its radium. False refuses the use.
pub fn may_use(world: &mut World, control: Entity, module: Entity, kind: Kind) -> bool {
    if kind != Kind::AdrenalineBoost {
        return true;
    }
    if world
        .get::<Radium>(module)
        .is_some_and(|r| r.0 >= RADIUM_REQUIRED)
    {
        return true;
    }
    if let Some(mob) = wearer(world, control) {
        crate::balloon::balloon_alert(world, mob, mob, "нет заряда!");
    }
    false
}

/// `on_use`.
pub fn on_use(world: &mut World, control: Entity, module: Entity, kind: Kind) {
    let Some(mob) = wearer(world, control) else {
        return;
    };
    let cost = def_of(world, module).map_or(0.0, |d| d.use_energy_cost);
    match kind {
        Kind::EmpPulse => {
            if let Some(pos) = crate::explosions::turf_of(world, control) {
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(pos, EMPULSE_SOUND, 60.0, true),
                );
                crate::emp::empulse(
                    world,
                    crate::emp::Empulse {
                        epicenter: pos,
                        heavy: 4,
                        light: 6,
                    },
                );
            }
            drain(world, control, cost);
        }
        Kind::StarDispenser => {
            // `dispenser/on_use`: `put_in_hands(new dispense_type)`.
            let Some(star) = crate::construction::spawn_detached(world, STAR) else {
                return;
            };
            world.entity_mut(star).insert(Networked);
            let Some(inv) = world.get::<Inventory>(mob).cloned() else {
                world.despawn(star);
                return;
            };
            crate::storage::put_in_hands(world, mob, &inv, star);
            crate::balloon::balloon_alert(world, mob, mob, "сюрикен выдан");
            drain(world, control, cost);
        }
        Kind::WeaponRecall => recall(world, control, module, mob, cost),
        Kind::DnaLock { .. } => {
            let dna = world
                .get::<crate::genetics::Dna>(mob)
                .map(|d| d.unique_enzymes.clone());
            world.entity_mut(module).insert(DnaLocked(dna));
            crate::balloon::balloon_alert(world, mob, mob, "ДНК обновлена");
            drain(world, control, cost);
        }
        Kind::AdrenalineBoost => boost(world, module, mob),
        _ => {}
    }
}

/// `on_select_use(target)` of the energy net: true when it handled the click.
pub fn on_select_use(
    world: &mut World,
    control: Entity,
    module: Entity,
    kind: Kind,
    target: Entity,
) -> bool {
    if kind != Kind::EnergyNet {
        return false;
    }
    let Some(mob) = wearer(world, control) else {
        return true;
    };
    let cost = def_of(world, module).map_or(0.0, |d| d.use_energy_cost);
    let (Some(from), Some(to)) = (
        crate::explosions::turf_of(world, mob),
        crate::explosions::turf_of(world, target),
    ) else {
        return true;
    };
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(from, NET_SOUND, 25.0, true),
    );
    drain(world, control, cost);
    if from.chebyshev(to).is_none_or(|d| d > NET_RANGE) || world.get::<Health>(target).is_none() {
        return true;
    }
    net_hit(world, module, mob, target, to);
    true
}

/// `energy_net/on_hit`: a net on the target's tile holds it.
fn net_hit(world: &mut World, module: Entity, firer: Entity, target: Entity, at: TilePos) {
    let taken = world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(at)
        .iter()
        .any(|&e| world.get::<EnergyNet>(e).is_some());
    if taken {
        return;
    }
    let Some(net) = crate::construction::spawn_obj(world, NET, at) else {
        return;
    };
    world.entity_mut(net).insert(EnergyNet {
        module,
        victim: target,
    });
    // `unbuckle_mob(force)` from what held it, then `buckle_mob(target, force = TRUE)`.
    world
        .entity_mut(target)
        .insert(crate::medical::surgery::Buckled(net));
    let (firer_name, target_name) = (name(world, firer), name(world, target));
    tell_mob(
        world,
        firer,
        Span::Notice,
        format!("Вы поймали {target_name} с помощью энергосетки!"),
    );
    tell_mob(
        world,
        target,
        Span::Danger,
        format!("{firer_name} поймал вас с помощью энергосетки!"),
    );
}

/// `energy_net/atom_destruction`: the held mob is free.
fn destroy_net(world: &mut World, net: Entity) {
    if let Some(n) = world.get::<EnergyNet>(net).copied() {
        free_victim(world, n.victim, net);
    }
    if world.get_entity(net).is_ok() {
        world.despawn(net);
    }
}

fn free_victim(world: &mut World, victim: Entity, net: Entity) {
    if world
        .get::<crate::medical::surgery::Buckled>(victim)
        .is_some_and(|b| b.0 == net)
    {
        world
            .entity_mut(victim)
            .remove::<crate::medical::surgery::Buckled>();
        tell_mob(
            world,
            victim,
            Span::Notice,
            "Вы освобождаетесь от энергосетки!",
        );
    }
}

/// Destroyed nets (their integrity ran out) let go of their mob.
fn nets(world: &mut World, mut known: Local<Vec<(Entity, Entity)>>) {
    let mut q = world.query::<(Entity, &EnergyNet)>();
    let now_nets: Vec<(Entity, Entity)> = q.iter(world).map(|(e, n)| (e, n.victim)).collect();
    for &(net, victim) in known.iter() {
        if world.get_entity(net).is_err() && world.get_entity(victim).is_ok() {
            free_victim(world, victim, net);
        }
    }
    *known = now_nets;
}

/// The weapon recall's `on_use`.
fn recall(world: &mut World, control: Entity, module: Entity, mob: Entity, cost: f64) {
    let linked = world
        .get::<LinkedWeapon>(module)
        .map(|l| l.0)
        .filter(|&w| world.get_entity(w).is_ok());
    let Some(weapon) = linked else {
        let held = world.get::<Inventory>(mob).and_then(|i| {
            i.hands.iter().flatten().copied().find(|&h| {
                crate::construction::path_of(world, h).is_some_and(|p| is_under(&p, KATANA))
            })
        });
        let Some(held) = held else {
            crate::balloon::balloon_alert(world, mob, mob, "нет привязанного оружия!");
            return;
        };
        set_weapon(world, module, held);
        let text = format!("связь с {} установлена", name(world, held));
        crate::balloon::balloon_alert(world, mob, mob, &text);
        return;
    };
    if crate::antag::objectives::all_contents(world, mob).contains(&weapon) {
        crate::balloon::balloon_alert(world, mob, mob, "уже у вас!");
        return;
    }
    let (from, at) = (
        crate::explosions::turf_of(world, mob),
        crate::explosions::turf_of(world, weapon),
    );
    let distance = match (from, at) {
        (Some(a), Some(b)) => a.chebyshev(b).unwrap_or(u32::MAX),
        _ => u32::MAX,
    };
    let in_view =
        from.is_some_and(|a| a.z == at.map_or(u16::MAX, |b| b.z)) && distance <= 7 && distance > 0;
    if !in_view && !drain(world, control, cost * f64::from(distance.min(1000))) {
        crate::balloon::balloon_alert(world, mob, mob, "недостаточно заряда!");
        return;
    }
    recall_weapon(world, mob, weapon);
}

/// `set_weapon`: the module brings this one back.
pub fn set_weapon(world: &mut World, module: Entity, weapon: Entity) {
    world.entity_mut(module).insert(LinkedWeapon(weapon));
}

/// `/datum/outfit/ninja/post_equip`: `lock.dna = ninja.dna.unique_enzymes`.
pub fn set_dna_owner(world: &mut World, module: Entity, mob: Entity) {
    let dna = world
        .get::<crate::genetics::Dna>(mob)
        .map(|d| d.unique_enzymes.clone());
    world.entity_mut(module).insert(DnaLocked(dna));
}

/// `recall_weapon`: out of where it is, into a hand, the belt or under the wearer.
fn recall_weapon(world: &mut World, mob: Entity, weapon: Entity) {
    sparks(world, weapon);
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    detach(world, weapon, pos);
    let Some(inv) = world.get::<Inventory>(mob).cloned() else {
        return;
    };
    let item_name = name(world, weapon);
    let free_hand = inv.hands.iter().any(Option::is_none);
    let alert = if free_hand {
        crate::storage::put_in_hands(world, mob, &inv, weapon);
        format!("{item_name} телепортируется в вашу руку")
    } else if inv.belt.is_none() {
        world
            .entity_mut(weapon)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        if let Some(mut i) = world.get_mut::<Inventory>(mob) {
            i.belt = Some(weapon);
        }
        crate::inventory::refresh_look(world, mob);
        format!("{item_name} закрепляется на вашем поясе")
    } else {
        if let Some(&pos) = world.get::<TilePos>(mob) {
            crate::storage::take_out(world, weapon, pos);
        }
        format!("{item_name} телепортируется под вас")
    };
    crate::balloon::balloon_alert(world, mob, mob, &alert);
}

/// `forceMove(get_turf(src))`: out of a holder's slot or a storage, onto `pos`.
fn detach(world: &mut World, item: Entity, pos: TilePos) {
    if let Some(holder) = world.get::<CarriedBy>(item).map(|c| c.0) {
        if let Some(mut inv) = world.get_mut::<Inventory>(holder)
            && let Some(slot) = inv.slot_of(item)
        {
            *inv.slot_mut(slot) = None;
        }
        world.entity_mut(item).remove::<CarriedBy>();
        crate::inventory::refresh_look(world, holder);
    }
    if world.get::<crate::storage::StoredIn>(item).is_some() {
        crate::storage::take_out(world, item, pos);
    } else {
        world.entity_mut(item).insert(pos);
    }
}

/// The adrenaline boost's `on_use`.
fn boost(world: &mut World, module: Entity, mob: Entity) {
    tell_mob(
        world,
        mob,
        Span::Notice,
        "Вы использовали адреналиновый заряд",
    );
    if let Some(mut e) = crate::status_effects::effects_mut(world, mob) {
        e.set_all_immobility(0.0);
    }
    crate::status_effects::adjust_stamina(world, mob, -200.0);
    crate::status_effects::set_timed(
        world,
        mob,
        crate::status_effects::Timed::Stutter,
        0.0,
        false,
    );
    add_to_blood(world, mob, STIMULANTS, 5.0);
    let left = world.get_mut::<Radium>(module).map_or(0.0, |mut r| {
        r.0 -= r.0 * 0.75;
        r.0
    });
    let at = now(world) + AFTEREFFECT_SECS;
    world.entity_mut(module).insert(BoostAftereffect {
        mob,
        at,
        amount: left,
    });
}

/// `boost_aftereffects`: the rest of the radium into the wearer.
fn aftereffects(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &BoostAftereffect)>();
    let due: Vec<(Entity, BoostAftereffect)> = q
        .iter(world)
        .filter(|(_, b)| b.at <= t)
        .map(|(e, b)| (e, *b))
        .collect();
    for (module, b) in due {
        world.entity_mut(module).remove::<BoostAftereffect>();
        let left = world
            .get::<Radium>(module)
            .map_or(0.0, |r| r.0)
            .min(b.amount);
        if let Some(mut r) = world.get_mut::<Radium>(module) {
            r.0 = (r.0 - left).max(0.0);
        }
        if world.get_entity(b.mob).is_err() {
            continue;
        }
        add_to_blood(world, b.mob, RADIUM, left);
        tell_mob(
            world,
            b.mob,
            Span::Danger,
            "Вы начинаете ощущать последствия инъекции.",
        );
    }
}

fn add_to_blood(world: &mut World, mob: Entity, reagent: &str, amount: f32) {
    if amount <= 0.0 {
        return;
    }
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let Some(id) = chem.id(reagent) else {
        return;
    };
    if let Some(mut m) = world.get_mut::<crate::chem::MobReagents>(mob) {
        m.blood.add_reagent(
            &chem,
            id,
            amount,
            ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
            None,
            None,
            false,
            false,
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The cloak

/// `unstealth`: sparks, `use_energy_cost` and off.
pub fn unstealth(world: &mut World, mob: Entity) {
    let Some(c) = world.get::<Cloaked>(mob).copied() else {
        return;
    };
    let module_name = name(world, c.module);
    tell_mob(
        world,
        mob,
        Span::Warning,
        format!(
            "{} отключается от контакта!",
            ss13_shared::capitalize(&module_name)
        ),
    );
    sparks(world, mob);
    let cost = def_of(world, c.module).map_or(0.0, |d| d.use_energy_cost);
    drain(world, c.control, cost);
    super::modules::deactivate(world, c.control, c.module, true);
}

/// `on_unarmed_attack` / `COMSIG_MOB_ITEM_ATTACK` on a living target.
fn attack_breaks_cloak(world: &mut World, mob: Entity, target: Entity) {
    if target != mob && world.get::<Cloaked>(mob).is_some() && world.get::<Health>(target).is_some()
    {
        unstealth(world, mob);
    }
}

/// `COMSIG_ATOM_ATTACKBY`, `ATTACK_HAND`, `BULLET_ACT`, `HITBY`: any damage taken.
fn cloak_hits(world: &mut World) {
    let mut q = world.query::<(Entity, &Cloaked, &Health)>();
    let seen: Vec<(Entity, f32, f32)> = q
        .iter(world)
        .map(|(e, c, h)| (e, c.damage, h.brute + h.burn))
        .collect();
    for (mob, before, now_damage) in seen {
        if now_damage > before {
            unstealth(world, mob);
        } else if let Some(mut c) = world.get_mut::<Cloaked>(mob) {
            c.damage = now_damage;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The DNA lock (`COMSIG_MOD_ACTIVATE`)

/// `dna_check(user)` of every DNA lock of the suit; the reinforced one gibs a wrong wearer.
/// False cancels the activation.
pub fn activation_allowed(world: &mut World, control: Entity, user: Option<Entity>) -> bool {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return true;
    };
    for &module in &suit.modules {
        let Some(Kind::DnaLock { reinforced }) = world.get::<ModModule>(module).map(|m| m.kind)
        else {
            continue;
        };
        let Some(lock) = world.get::<DnaLocked>(module).and_then(|d| d.0.clone()) else {
            continue;
        };
        let Some(user) = user else {
            continue;
        };
        let user_dna = world
            .get::<crate::genetics::Dna>(user)
            .map(|d| d.unique_enzymes.clone());
        if dna_check(Some(&lock), user_dna.as_deref()) {
            continue;
        }
        crate::balloon::balloon_alert(world, user, user, "ДНК заблокирована!");
        if reinforced && suit.wearer == Some(user) {
            tell_mob(
                world,
                user,
                Span::Danger,
                "фАТальНая ОШибкА: 382200-*#00КОД КРАСНЫЙ\nОБНАруЖЕно НЕАВТОРИЗИРОВАННОЕ \
                 ИСпОЛЬзоВанИе\nНаЧАЛО ПОДР0СиСТЕМы3 13...\nУНИчТОЖЕНИЕ П-П-ПОЛЬЗОВАТЕЛЯ...",
            );
            crate::health::gib(world, user);
        }
        return false;
    }
    true
}

/// `dna_check`: no lock, or the user's `unique_enzymes` are the lock's.
pub fn dna_check(lock: Option<&str>, user: Option<&str>) -> bool {
    lock.is_none_or(|l| user == Some(l))
}

/// `on_emp` / `on_emag`: the plain lock forgets its DNA (the reinforced one ignores EMPs).
pub fn clear_lock(world: &mut World, control: Entity, emp: bool) {
    let modules = world
        .get::<ModSuit>(control)
        .map(|s| s.modules.clone())
        .unwrap_or_default();
    for module in modules {
        match world.get::<ModModule>(module).map(|m| m.kind) {
            Some(Kind::DnaLock { reinforced }) if !(emp && reinforced) => {
                world.entity_mut(module).insert(DnaLocked(None));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(path: &str) -> ModuleDef {
        ModuleDef {
            path: path.into(),
            ..Default::default()
        }
    }

    #[test]
    fn ninja_kinds_come_before_their_parents() {
        assert_eq!(
            kind_of(&def("/obj/item/mod/module/stealth/ninja")),
            Some(Kind::Stealth { ninja: true })
        );
        assert_eq!(
            kind_of(&def("/obj/item/mod/module/emp_shield/pulse")),
            Some(Kind::EmpPulse)
        );
        assert_eq!(
            kind_of(&def("/obj/item/mod/module/dna_lock/reinforced")),
            Some(Kind::DnaLock { reinforced: true })
        );
        assert_eq!(kind_of(&def("/obj/item/mod/module/emp_shield")), None);
        assert_eq!(
            Kind::of(&def("/obj/item/mod/module/emp_shield/pulse")),
            Kind::EmpPulse
        );
        assert_eq!(
            Kind::of(&def("/obj/item/mod/module/welding/camera_vision")),
            Kind::Welding
        );
    }

    #[test]
    fn the_dna_lock_takes_only_its_owner() {
        assert!(dna_check(None, None));
        assert!(dna_check(Some("aaa"), Some("aaa")));
        assert!(!dna_check(Some("aaa"), Some("bbb")));
        assert!(!dna_check(Some("aaa"), None));
    }
}
