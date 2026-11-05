//! Staves and wands (server) after tg `/obj/item/gun/magic` (`code/modules/projectiles/guns/
//! magic.dm`, `magic/staff.dm`, `magic/wand.dm`).
//!
//! A magic gun holds `charges` of its bolt ([`super::effects::BoltKind`]). A click with it held
//! on anything past reach, or on a living mob in reach (point blank), fires one at the clicked
//! tile (the user's click waits `CLICK_CD_RANGE`); empty, it "тихо посвистывает". The user's
//! antimagic stops it (`can_cast_magic`). Staves recharge one charge every `recharge_rate`
//! seconds; wands do not, and spawn with all, a half or a third of their charges (25%, 50%,
//! 25%). A wand clicked on its own user zaps the user (`zap_self`). The staves of chaos and door
//! creation do not work in the wizard's den (`no_den_usage`) until carried out of it once. The
//! staff of chaos picks a random ported bolt per shot.
//!
//! These guns are `/obj/item/gun` prototypes, so `crate::guns` gives them its `Gun` state on
//! spawn: [`MagicGun`] takes it away again, and `crate::guns` leaves them alone.
// shortcut: the bolts of change, animation, shrinking, the locker, the spellblade and the arcane
// barrage are not ported (their staves and wands fire nothing); no intruder effects of staves
// in non-wizard hands, no suicides, no drained icon state, no pitch by charges.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::effects::{BoltKind, apply_bolt, fire_bolt};
use super::{MAGIC_RESISTANCE, can_cast_magic, chat_to, in_wizard_den};
use crate::click::{ClickResolved, RangedClick};
use crate::combat::{CombatRng, NextMove};
use crate::health::{Health, Stat};
use crate::inventory::Inventory;
use crate::medical::name_of;

pub const MAGIC_GUN_ROOT: &str = "/obj/item/gun/magic";

/// What a magic gun fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MagicShot {
    Bolt(BoltKind),
    /// `staff/chaos`: a random bolt per shot.
    Chaos,
}

/// A staff's or wand's vars.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MagicGunDef {
    pub path: &'static str,
    pub shot: MagicShot,
    pub max_charges: u32,
    /// Seconds per charge.
    pub recharge_rate: f64,
    pub self_charging: bool,
    /// Wands' `variable_charges`.
    pub variable_charges: bool,
    pub no_den_usage: bool,
    pub fire_sound: &'static str,
}

const STAFF: MagicGunDef = MagicGunDef {
    path: "/obj/item/gun/magic/staff",
    shot: MagicShot::Bolt(BoltKind::Nothing),
    max_charges: 6,
    recharge_rate: 8.0,
    self_charging: true,
    variable_charges: false,
    no_den_usage: false,
    fire_sound: "sound/items/weapons/emitter.ogg",
};

const WAND: MagicGunDef = MagicGunDef {
    path: "/obj/item/gun/magic/wand",
    shot: MagicShot::Bolt(BoltKind::Nothing),
    max_charges: 100,
    recharge_rate: 8.0,
    self_charging: false,
    variable_charges: true,
    no_den_usage: false,
    fire_sound: "sound/items/weapons/emitter.ogg",
};

/// The ported magic guns, most specific first; the bases catch the rest.
pub const MAGIC_GUNS: [MagicGunDef; 13] = [
    MagicGunDef {
        path: "/obj/item/gun/magic/staff/healing",
        shot: MagicShot::Bolt(BoltKind::Resurrection),
        fire_sound: "sound/effects/magic/staff_healing.ogg",
        ..STAFF
    },
    MagicGunDef {
        path: "/obj/item/gun/magic/staff/door",
        shot: MagicShot::Bolt(BoltKind::Door),
        max_charges: 10,
        recharge_rate: 2.0,
        no_den_usage: true,
        fire_sound: "sound/effects/magic/staff_door.ogg",
        ..STAFF
    },
    MagicGunDef {
        path: "/obj/item/gun/magic/staff/chaos",
        shot: MagicShot::Chaos,
        max_charges: 10,
        recharge_rate: 2.0,
        no_den_usage: true,
        fire_sound: "sound/effects/magic/staff_chaos.ogg",
        ..STAFF
    },
    STAFF,
    MagicGunDef {
        path: "/obj/item/gun/magic/wand/death",
        shot: MagicShot::Bolt(BoltKind::Death),
        max_charges: 3,
        fire_sound: "sound/effects/magic/wandodeath.ogg",
        ..WAND
    },
    MagicGunDef {
        path: "/obj/item/gun/magic/wand/resurrection",
        shot: MagicShot::Bolt(BoltKind::Resurrection),
        max_charges: 10,
        fire_sound: "sound/effects/magic/staff_healing.ogg",
        ..WAND
    },
    MagicGunDef {
        path: "/obj/item/gun/magic/wand/teleport",
        shot: MagicShot::Bolt(BoltKind::Teleport),
        max_charges: 10,
        fire_sound: "sound/effects/magic/wand_teleport.ogg",
        ..WAND
    },
    MagicGunDef {
        path: "/obj/item/gun/magic/wand/safety",
        shot: MagicShot::Bolt(BoltKind::Safety),
        max_charges: 10,
        fire_sound: "sound/effects/magic/wand_teleport.ogg",
        ..WAND
    },
    MagicGunDef {
        path: "/obj/item/gun/magic/wand/door",
        shot: MagicShot::Bolt(BoltKind::Door),
        max_charges: 20,
        fire_sound: "sound/effects/magic/staff_door.ogg",
        ..WAND
    },
    MagicGunDef {
        path: "/obj/item/gun/magic/wand/fireball",
        shot: MagicShot::Bolt(BoltKind::Fireball),
        max_charges: 8,
        fire_sound: "sound/effects/magic/fireball.ogg",
        ..WAND
    },
    MagicGunDef {
        path: "/obj/item/gun/magic/wand/polymorph",
        max_charges: 10,
        fire_sound: "sound/effects/magic/staff_change.ogg",
        ..WAND
    },
    MagicGunDef {
        path: "/obj/item/gun/magic/wand/shrink",
        max_charges: 10,
        fire_sound: "sound/effects/magic/staff_shrink.ogg",
        ..WAND
    },
    WAND,
];

/// The bolts the staff of chaos picks from (its `allowed_projectile_types` that are ported).
const CHAOS_BOLTS: [BoltKind; 5] = [
    BoltKind::Death,
    BoltKind::Door,
    BoltKind::Fireball,
    BoltKind::Resurrection,
    BoltKind::Teleport,
];

/// The def of a magic gun type (its most specific ported ancestor).
pub fn gun_def(path: &str) -> Option<&'static MagicGunDef> {
    if !is_under(path, MAGIC_GUN_ROOT) {
        return None;
    }
    MAGIC_GUNS.iter().find(|d| is_under(path, d.path))
}

/// `wand/Initialize`: all (25%), a third (25%) or a half (50%) of the charges.
pub fn roll_max_charges(max: u32, rng: &mut CombatRng) -> u32 {
    if rng.chance(25.0) {
        return max;
    }
    if rng.chance(33.0) {
        max.div_ceil(3)
    } else {
        max.div_ceil(2)
    }
}

/// A staff's or wand's state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct MagicGun {
    pub def: &'static MagicGunDef,
    pub charges: u32,
    pub max_charges: u32,
    pub charge_timer: f64,
    pub no_den_usage: bool,
}

impl MagicGun {
    pub fn new(def: &'static MagicGunDef, max_charges: u32) -> Self {
        Self {
            def,
            charges: max_charges,
            max_charges,
            charge_timer: 0.0,
            no_den_usage: def.no_den_usage,
        }
    }

    /// `process(seconds_per_tick)` of a self-charging gun: true when a charge came back.
    pub fn recharge(&mut self, dt: f64) -> bool {
        if !self.def.self_charging || self.charges >= self.max_charges {
            self.charge_timer = 0.0;
            return false;
        }
        self.charge_timer += dt;
        if self.charge_timer < self.def.recharge_rate {
            return false;
        }
        self.charge_timer = 0.0;
        self.charges += 1;
        true
    }
}

pub struct MagicGunsPlugin;

impl Plugin for MagicGunsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                adopt_magic_guns,
                ranged_fire
                    .after(crate::click::ResolveClicks)
                    .before(crate::projectile::ProjectileSystems),
                point_blank
                    .after(crate::security::security_clicks)
                    .before(crate::combat::MeleeSystems),
                recharge.run_if(any_with_component::<MagicGun>),
            ),
        );
    }
}

/// New magic guns get their [`MagicGun`]; `crate::guns`' state goes.
fn adopt_magic_guns(
    mut commands: Commands,
    added: Query<(Entity, &DmItem), (Added<DmItem>, Without<MagicGun>)>,
    with_gun: Query<Entity, (With<MagicGun>, With<crate::guns::Gun>)>,
    mut rng: ResMut<CombatRng>,
) {
    for (e, item) in &added {
        let Some(def) = gun_def(&item.path) else {
            continue;
        };
        let max = if def.variable_charges {
            roll_max_charges(def.max_charges, &mut rng)
        } else {
            def.max_charges
        };
        commands
            .entity(e)
            .insert(MagicGun::new(def, max))
            .remove::<crate::guns::Gun>();
    }
    for e in &with_gun {
        commands.entity(e).remove::<crate::guns::Gun>();
    }
}

/// [`MagicGun`] on `item` at once with every charge (the spellbook's wands: "in the best
/// possible condition").
pub fn adopt_full(world: &mut World, item: Entity) {
    let Some(def) = world.get::<DmItem>(item).and_then(|i| gun_def(&i.path)) else {
        return;
    };
    world
        .entity_mut(item)
        .insert(MagicGun::new(def, def.max_charges))
        .remove::<crate::guns::Gun>();
}

// Cost: per tick, one pass over the magic guns (a few per round).
fn recharge(time: Res<Time>, mut guns: Query<&mut MagicGun>) {
    let dt = time.delta_secs_f64();
    for mut gun in &mut guns {
        if gun.def.self_charging && gun.charges < gun.max_charges {
            gun.recharge(dt);
        }
    }
}

fn ranged_fire(world: &mut World, mut clicks: Local<MessageCursor<RangedClick>>) {
    let clicks: Vec<RangedClick> = clicks
        .read(world.resource::<Messages<RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(gun) = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
            .filter(|&g| world.get::<MagicGun>(g).is_some())
        else {
            continue;
        };
        try_fire(world, click.mob, gun, click.target, click.target_pos);
    }
}

/// A held magic gun on a living mob in reach fires point blank (a wand on its user zaps it).
fn point_blank(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        if world
            .resource::<crate::security::MeleeHandled>()
            .0
            .contains(&click.mob)
            || world.get::<Health>(click.target).is_none()
        {
            continue;
        }
        let Some(gun) = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
            .filter(|&g| world.get::<MagicGun>(g).is_some())
        else {
            continue;
        };
        if try_fire(world, click.mob, gun, click.target, click.target_pos) {
            world
                .resource_mut::<crate::security::MeleeHandled>()
                .0
                .insert(click.mob);
        }
    }
}

/// `try_fire_gun` -> `process_fire` of a magic gun. True when the click was used.
pub fn try_fire(
    world: &mut World,
    mob: Entity,
    gun: Entity,
    target: Entity,
    target_pos: TilePos,
) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world.get::<Stat>(mob) != Some(&Stat::Conscious)
        || crate::status_effects::incapacitation(world, mob).hands_blocked
        || world.get::<NextMove>(mob).is_some_and(|n| now < n.0)
    {
        return false;
    }
    let Some(&mob_pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    let Some(state) = world.get::<MagicGun>(gun).cloned() else {
        return false;
    };
    let name = name_of(world, gun);
    if state.charges == 0 {
        chat_to(
            world,
            mob,
            Span::Warning,
            format!("{} тихо посвистывает.", capitalize(&name)),
        );
        return true;
    }
    let wand = is_under(state.def.path, "/obj/item/gun/magic/wand");
    // `no_den_usage`.
    if state.no_den_usage {
        if in_wizard_den(world, mob_pos) {
            chat_to(
                world,
                mob,
                Span::Warning,
                format!(
                    "Вы прекрасно знаете, что не стоит нарушать безопасность Логова. Лучше для \
                     начала выйти отсюда, прежде чем использовать {name}."
                ),
            );
            return true;
        }
        if let Some(mut g) = world.get_mut::<MagicGun>(gun) {
            g.no_den_usage = false;
        }
    }
    if !can_cast_magic(world, mob, MAGIC_RESISTANCE) {
        return true;
    }
    let kind = match state.def.shot {
        MagicShot::Bolt(kind) => kind,
        MagicShot::Chaos => {
            let i = world
                .resource_mut::<CombatRng>()
                .range(0, CHAOS_BOLTS.len() as u32 - 1) as usize;
            CHAOS_BOLTS[i]
        }
    };
    if let Some(mut g) = world.get_mut::<MagicGun>(gun) {
        g.charges -= 1;
    }
    world
        .entity_mut(mob)
        .insert(NextMove(now + crate::guns::CLICK_CD_RANGE));
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(mob_pos, state.def.fire_sound, 50.0, true),
    );
    if wand && target == mob {
        // `wand/zap_self`.
        let user = name_of(world, mob);
        crate::medical::visible(
            world,
            mob,
            &format!("Вы бьёте себя {name}."),
            &format!("{user} бьёт себя {name}."),
            Span::Danger,
        );
        apply_bolt(world, kind, mob, mob);
        return true;
    }
    fire_bolt(world, mob, mob_pos, Some(target), target_pos, kind, 1);
    true
}

/// What `COMSIG_ITEM_MAGICALLY_CHARGED` did to an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Charged {
    Nothing,
    Charged,
    /// `COMPONENT_ITEM_BURNT_OUT`.
    BurntOut,
}

/// The charge spell on `item`: a magic gun refills (a wand may lose a charge for good, 80%), an
/// energy gun's cell fills.
pub fn magically_charge(world: &mut World, item: Entity) -> Charged {
    if world.get::<MagicGun>(item).is_some() {
        let loses = world.resource_mut::<CombatRng>().chance(80.0);
        let mut gun = world.get_mut::<MagicGun>(item).expect("checked");
        if !gun.def.self_charging && gun.max_charges > 0 && loses {
            gun.max_charges -= 1;
        }
        gun.charges = gun.max_charges;
        return if gun.max_charges == 0 {
            Charged::BurntOut
        } else {
            Charged::Charged
        };
    }
    let max = world
        .get::<ss13_mapload::DmGun>(item)
        .filter(|d| d.energy)
        .map(|d| d.cell_maxcharge);
    if let Some(max) = max
        && let Some(mut gun) = world.get_mut::<crate::guns::Gun>(item)
    {
        gun.charge = max;
        return Charged::Charged;
    }
    Charged::Nothing
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guns_resolve_to_their_most_specific_def() {
        assert_eq!(
            gun_def("/obj/item/gun/magic/staff/healing/unrestricted").map(|d| d.shot),
            Some(MagicShot::Bolt(BoltKind::Resurrection))
        );
        assert_eq!(
            gun_def("/obj/item/gun/magic/staff/change").map(|d| d.path),
            Some("/obj/item/gun/magic/staff")
        );
        assert_eq!(
            gun_def("/obj/item/gun/magic/wand/death").map(|d| d.max_charges),
            Some(3)
        );
        assert!(gun_def("/obj/item/gun/energy/laser").is_none());
    }

    #[test]
    fn staves_recharge_wands_do_not() {
        let staff = gun_def("/obj/item/gun/magic/staff/door").unwrap();
        let mut gun = MagicGun::new(staff, staff.max_charges);
        gun.charges = 0;
        assert!(!gun.recharge(1.0));
        assert!(gun.recharge(1.0));
        assert_eq!(gun.charges, 1);
        let wand = gun_def("/obj/item/gun/magic/wand/fireball").unwrap();
        let mut gun = MagicGun::new(wand, 8);
        gun.charges = 0;
        assert!(!gun.recharge(100.0));
        assert_eq!(gun.charges, 0);
    }

    #[test]
    fn wand_charges_are_all_a_half_or_a_third() {
        let mut rng = CombatRng::default();
        for _ in 0..50 {
            let n = roll_max_charges(20, &mut rng);
            assert!([20, 10, 7].contains(&n), "{n}");
        }
    }
}
