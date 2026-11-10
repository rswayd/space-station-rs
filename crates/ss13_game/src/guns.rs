//! Guns (server): tg `/obj/item/gun` (`code/modules/projectiles/gun.dm`), its ballistic and
//! energy families (`guns/ballistic.dm`, `guns/ballistic/revolver.dm`, `guns/energy.dm`), the
//! magazines and casings they use and the firing pin (`pins.dm`).
//!
//! - **Firing** (`fire_gun` -> `process_fire`): a click with a held gun on anything past reach
//!   ([`crate::click::RangedClick`]), or on an adjacent mob (point blank, whatever the combat
//!   mode; an adjacent object gets the item's normal interaction). The pin must authorize the
//!   user (`handle_pins`): a standard pin anyone, a mindshield pin the mindshielded jobs, the
//!   operatives' pin the operatives; without a pin the trigger is locked. An empty gun clicks
//!   ("*щёлк*") and, in combat mode, lets the click bash. A shot fires the chambered round's
//!   projectile ([`crate::projectile`]) at the clicked tile, sets the user's click cooldown to
//!   `CLICK_CD_RANGE` (0.4 s) and the gun's to `fire_delay`; a burst gun fires `burst_size`
//!   rounds `burst_delay` apart while still held.
//! - **Ballistic**: a detachable magazine goes in with a click of it on the held gun (a full gun
//!   swaps them, tg `tac_reloads`); using the gun in hand (Z) ejects an empty magazine, drops a
//!   locked bolt or racks it (ejecting the chambered round); a click on the held gun with the
//!   other hand empty takes the magazine out. Each shot ejects its casing onto the floor and
//!   chambers the next round; a locking bolt (pistols) locks back on the last one. Revolvers keep
//!   their spent casings in the cylinder until emptied (Z); a cylinder has a slot per chamber,
//!   so an empty one clicks and turns. They load from speedloaders, boxes and loose rounds of
//!   their caliber into empty or spent chambers (the spent casing falls out). The dual-tube
//!   shotgun switches tubes with Z when a round is chambered.
//! - **Misfires** (`can_misfire` guns and rounds): `misfire_probability`% of trigger pulls
//!   fire into the user instead (`blow_up`); each shot adds `misfire_percentage_increment` and
//!   the round's `misfire_increment`, up to `misfire_probability_cap`. A sawn-off gun spreads
//!   `SAWN_OFF_ACC_PENALTY` more and kicks `sawoff_bonus_recoil` harder. Fire modes, revolver
//!   spins and sawing off are `crate::gun_modes`.
//! - **Energy**: a cell (starts full) pays each shot's `e_cost`; Z switches the fire mode
//!   (disable/kill on the energy gun); rechargers charge it (`crate::power::chargers`).
//!
//! Each trigger pull spreads by the gun's `spread` plus the dual-wield bonus (light guns in
//! the other hand fire too, a decisecond apart); a casing's `variance` replaces it, and a
//! shotgun shell fires `pellets` projectiles across it. `recoil` kicks the shooter's camera
//! back (`crate::recoil`). `WEAPON_HEAVY` guns need the other hand empty. Pump shotguns rack
//! with Z, automatic ones after each shot; loose shells load one by one.
//!
//! Messages are BandaStation's translation.
//! Gunpoint holds are `crate::gunpoint`; a gun in the mouth is `handle_suicide`; seclites,
//! suppressors, bayonets and scopes are `crate::gun_attachments`.
// shortcut: no charge overlays (a clumsy user's botch is `check_botched`; of a burst only the
// first round is aimed at the leg); pins cannot be removed or swapped. Add them as their
// mechanics land.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{CasingData, DmGun, DmMagazine, MagazineData};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::protocol::ClickModifiers;

use crate::click::{ClickResolved, RangedClick};
use crate::combat::{CombatMode, CombatRng, NextMove};
use crate::health::{COMBAT_MESSAGE_RANGE, Health, Stat, ZoneSelected};
use crate::inventory::Inventory;
use crate::projectile::{ProjectileSystems, Shot};
use crate::security::{name_of, show};
use crate::sound::PlaySound;
use crate::tools::tell;

/// `WEAPON_MEDIUM`, `WEAPON_HEAVY`.
pub const WEAPON_MEDIUM: u32 = 2;
pub const WEAPON_HEAVY: u32 = 3;
/// `DUALWIELD_PENALTY_EXTRA_MULTIPLIER`, `AKIMBO_PENALTY_SCALING`.
const DUALWIELD_PENALTY_EXTRA_MULTIPLIER: f32 = 1.4;
const AKIMBO_PENALTY_SCALING: f32 = 0.75;
/// `CLICK_CD_RANGE`, seconds.
pub const CLICK_CD_RANGE: f64 = 0.4;
/// `BOLT_TYPE_*`.
pub const BOLT_TYPE_STANDARD: u32 = 1;
pub const BOLT_TYPE_OPEN: u32 = 2;
pub const BOLT_TYPE_NO_BOLT: u32 = 3;
pub const BOLT_TYPE_LOCKING: u32 = 4;
const MINDSHIELD_PIN: &str = "/obj/item/firing_pin/implant/mindshield";
const SYNDICATE_PIN: &str = "/obj/item/firing_pin/implant/pindicate";

/// One cartridge: live until fired. A revolver's empty chamber is a round without a casing
/// ([`Round::empty`], tg's `null` in a cylinder's `stored_ammo`).
#[derive(Clone, Debug, PartialEq)]
pub struct Round {
    pub casing: CasingData,
    pub live: bool,
}

/// `/obj/item/ammo_box/magazine/internal/cylinder`: keeps a slot for every chamber.
pub const CYLINDER: &str = "/obj/item/ammo_box/magazine/internal/cylinder";

impl Round {
    /// An empty chamber of a cylinder.
    pub fn empty() -> Self {
        Self {
            casing: CasingData::default(),
            live: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.casing.path.is_empty()
    }
}

/// The rounds in a magazine, a revolver's cylinder, a speedloader or an ammo box (tg
/// `stored_ammo`); the last one comes out first.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Magazine {
    pub data: MagazineData,
    pub rounds: Vec<Round>,
}

impl Magazine {
    /// A new magazine: full unless `start_empty` (a cylinder's chambers are then empty).
    pub fn new(data: &MagazineData) -> Self {
        let full = |_| Round {
            casing: data.ammo.clone(),
            live: true,
        };
        let rounds = if !data.start_empty {
            (0..data.max_ammo).map(full).collect()
        } else if is_under(&data.path, CYLINDER) {
            (0..data.max_ammo).map(|_| Round::empty()).collect()
        } else {
            Vec::new()
        };
        Self {
            data: data.clone(),
            rounds,
        }
    }

    /// A revolver's cylinder: a fixed number of chambers, empty or not.
    pub fn is_cylinder(&self) -> bool {
        is_under(&self.data.path, CYLINDER)
    }

    /// tg `ammo_count(countempties)`; a cylinder's empty chambers are not rounds.
    pub fn ammo_count(&self, count_empties: bool) -> usize {
        self.rounds
            .iter()
            .filter(|r| !r.is_empty() && (count_empties || r.live))
            .count()
    }

    /// tg `give_round`: `None` when it does not fit. A cylinder takes it into its first empty
    /// or spent chamber and hands back the spent casing it replaced (`Some(Some(..))`).
    pub fn give_round(&mut self, round: Round) -> Option<Option<Round>> {
        if round.casing.caliber != self.data.caliber {
            return None;
        }
        if self.is_cylinder() {
            let slot = self.rounds.iter().position(|r| !r.live)?;
            let old = std::mem::replace(&mut self.rounds[slot], round);
            return Some((!old.is_empty()).then_some(old));
        }
        if self.rounds.len() >= self.data.max_ammo as usize {
            return None;
        }
        self.rounds.push(round);
        Some(None)
    }

    /// tg `try_load` from another box: rounds of this caliber while there is room; the spent
    /// casings a cylinder gives up go to `spent`.
    pub fn load_from(&mut self, other: &mut Magazine, spent: &mut Vec<Round>) -> u32 {
        let mut loaded = 0;
        while let Some(i) = other
            .rounds
            .iter()
            .rposition(|r| !r.is_empty() && r.casing.caliber == self.data.caliber)
        {
            let round = other.rounds.remove(i);
            match self.give_round(round.clone()) {
                Some(old) => {
                    spent.extend(old);
                    loaded += 1;
                }
                None => {
                    other.rounds.insert(i, round);
                    break;
                }
            }
        }
        loaded
    }
}

/// A gun's state (tg `chambered`, `magazine`, `bolt_locked`, the cell, `select`, `fire_cd`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Gun {
    pub chambered: Option<Round>,
    /// A ballistic gun's magazine (detachable or internal).
    pub magazine: Option<Magazine>,
    pub bolt_locked: bool,
    /// An energy gun's cell charge, joules.
    pub charge: f64,
    /// The energy fire mode (index into `DmGun::shots`).
    pub select: usize,
    /// `fire_cd` ends.
    pub ready_at: f64,
    /// The installed pin (`pin`): the gun's own until removed.
    pub pin: String,
    /// `suppressed` (`SUPPRESSED_*`) and the installed `/obj/item/suppressor`
    /// (`crate::gun_attachments`); a built-in one has no item.
    pub suppressed: u32,
    pub suppressor: Option<Entity>,
    /// `burst_select` switched a burst gun to single shots (`burst_size = 1`, `fire_delay = 0`).
    pub single_fire: bool,
    /// `sawn_off`.
    pub sawn_off: bool,
    /// `misfire_probability`, percent.
    pub misfire: f32,
    /// The dual-tube shotgun's other tube (`alternate_magazine`) and `toggled` (tube B in use).
    pub alt_magazine: Option<Magazine>,
    pub toggled: bool,
}

/// tg `burst_size` and `fire_delay` (deciseconds) of a gun as its fire mode has them.
pub fn burst_and_delay(dm: &DmGun, gun: &Gun) -> (u32, f32) {
    if gun.single_fire {
        (1, 0.0)
    } else {
        (dm.burst_size, dm.fire_delay)
    }
}

/// A change to the chambered round's projectile before it is fired (tg edits
/// `chambered.loaded_projectile`: `handle_suicide`, the gunpoint's charged shot); the next shot
/// takes it.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ChamberBoost {
    pub damage_mult: f32,
    /// Added unless the projectile is `CANT_WOUND` (-100).
    pub wound_bonus: f32,
    pub exposed_wound_bonus: f32,
    /// `zone_override = BODY_ZONE_HEAD`.
    pub head: bool,
}

impl ChamberBoost {
    pub fn apply(&self, data: &mut ss13_mapload::ProjectileData) {
        data.damage *= self.damage_mult;
        if data.wound_bonus != CANT_WOUND {
            data.wound_bonus += self.wound_bonus;
            data.exposed_wound_bonus += self.exposed_wound_bonus;
        }
    }
}

/// `CANT_WOUND`.
const CANT_WOUND: f32 = -100.0;

/// Rounds of a burst still to fire (tg `process_burst` timers).
#[derive(Resource, Default)]
struct Bursts(Vec<Burst>);

struct Burst {
    gun: Entity,
    mob: Entity,
    aim: Aim,
    at: f64,
    left: u32,
    delay: f64,
    /// `total_random_spread` and `burst_spread_mult` of the trigger pull; the last shot's
    /// `iteration`.
    spread: f32,
    spread_mult: f32,
    iteration: u32,
    /// A dual-wielded gun's own trigger pull (`process_fire` with this `bonus_spread`, by
    /// this client) instead of a burst's next round.
    akimbo: Option<(Entity, f32)>,
}

pub struct GunsServerPlugin;

/// Gun clicks and bursts fire in this set.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GunSystems;

impl Plugin for GunsServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A gun racks, ejects, unloads or switches its fire mode.
        app.add_click_handler(ClickPhase::AttackSelf, 1000, |w, c| {
            crate::guns::attack_self(w, c.client, c.mob, c.target)
        });
        // A magazine or rounds into the other hand's gun.
        app.add_click_handler(ClickPhase::OtherHand, 1500, |w, c| {
            c.held
                .is_some_and(|h| crate::guns::item_interaction(w, c.client, c.mob, h, c.target))
        });
        // tg `/obj/item/gun/ballistic/attack_hand`: the other hand's magazine comes out.
        app.add_click_handler(ClickPhase::OtherHand, 1600, |w, c| {
            c.held.is_none() && crate::guns::attack_hand(w, c.client, c.mob, c.target)
        });
        crate::taser::plugin(app);
        crate::gunpoint::plugin(app);
        crate::gun_attachments::plugin(app);
        crate::gun_modes::plugin(app);
        crate::gunhud::plugin(app);
        crate::guns_special::plugin(app);
        app.init_resource::<Bursts>()
            .add_observer(add_gun_state)
            .add_observer(add_magazine_state)
            .add_observer(unrack)
            .add_systems(
                Update,
                (ranged_clicks, run_bursts, crate::gunpoint::upkeep)
                    .chain()
                    .in_set(GunSystems)
                    .after(crate::click::ResolveClicks)
                    .after(crate::combat::MeleeSystems)
                    .before(ProjectileSystems),
            );
    }
}

/// tg `/obj/item/gun/ballistic/Initialize` and `/obj/item/gun/energy/Initialize`.
fn add_gun_state(add: On<Add<DmGun>>, guns: Query<&DmGun>, mut commands: Commands) {
    let Ok(dm) = guns.get(add.entity) else {
        return;
    };
    let mut gun = Gun {
        pin: dm.pin.clone(),
        suppressed: dm.suppressed,
        charge: dm.cell_maxcharge,
        misfire: dm.misfire_probability,
        alt_magazine: (!dm.alt_magazine.path.is_empty()).then(|| Magazine::new(&dm.alt_magazine)),
        ..default()
    };
    if dm.ballistic {
        if dm.spawnwithmagazine && !dm.magazine.path.is_empty() {
            gun.magazine = Some(Magazine::new(&dm.magazine));
            // A standard bolt or an internal magazine chambers from it; the others get the
            // chambered round on top (`replace_new_round`).
            let extra = dm.bolt_type != BOLT_TYPE_STANDARD && !dm.internal_magazine;
            chamber_round(dm, &mut gun);
            if extra && gun.chambered.is_some() {
                let round = gun.chambered.clone().expect("checked");
                gun.magazine.as_mut().expect("set above").rounds.push(round);
            }
        } else {
            gun.bolt_locked = true;
        }
    }
    commands.entity(add.entity).insert_if_new(gun);
    if dm.spawn_sawn_off {
        let gun = add.entity;
        commands.queue(move |world: &mut World| crate::gun_modes::do_sawoff(world, gun));
    }
}

fn add_magazine_state(add: On<Add<DmMagazine>>, mags: Query<&DmMagazine>, mut commands: Commands) {
    if let Ok(dm) = mags.get(add.entity) {
        commands
            .entity(add.entity)
            .insert_if_new(Magazine::new(&dm.data));
    }
}

fn is_revolver(dm: &DmGun) -> bool {
    dm.internal_magazine && dm.bolt_type == BOLT_TYPE_NO_BOLT
}

/// tg `chamber_round`; a revolver turns its cylinder instead (the chambered round stays in it).
// shortcut: a cylinder has no empty chambers: it turns over the rounds it holds, so a partly
// loaded revolver never clicks on an empty one.
fn chamber_round(dm: &DmGun, gun: &mut Gun) {
    if is_revolver(dm) {
        if let Some(mag) = gun.magazine.as_mut()
            && !mag.rounds.is_empty()
        {
            mag.rounds.rotate_left(1);
        }
        return;
    }
    if gun.chambered.is_some() {
        return;
    }
    gun.chambered = gun.magazine.as_mut().and_then(|m| m.rounds.pop());
}

/// The round a shot would fire: the chambered one, a revolver's turned-to round, an energy
/// gun's selected mode while the cell pays for it (tg `can_shoot`).
fn ready_round(dm: &DmGun, gun: &Gun) -> Option<CasingData> {
    if dm.energy {
        let shot = dm.shots.get(gun.select)?;
        return (gun.charge >= shot.e_cost).then(|| shot.clone());
    }
    let round = if is_revolver(dm) {
        gun.magazine.as_ref()?.rounds.last()
    } else {
        gun.chambered.as_ref()
    }?;
    round.live.then(|| round.casing.clone())
}

/// tg `handle_pins` -> `pin_auth`.
fn pin_auth(world: &World, dm: &DmGun, gun: &Gun, user: Entity) -> Result<(), String> {
    if dm.pinless {
        return Ok(());
    }
    if gun.pin.is_empty() {
        return Err("спуск не работает, нужен ударник!".to_owned());
    }
    // Laser tag pins: the wearer's vest team (`crate::clothing::lasertag`).
    if let Some(check) = crate::clothing::lasertag::pin_check(world, &gun.pin, user) {
        return check.map_err(|line| format!("{} {line}", dm.pin_fail_message));
    }
    let ok = if is_under(&gun.pin, MINDSHIELD_PIN) {
        crate::antag::revolution::server::mindshielded(world, user)
    } else if is_under(&gun.pin, SYNDICATE_PIN) {
        // `req_implant = /obj/item/implant/weapons_auth`; the operatives' outfit implant is
        // their datum here.
        world
            .get::<crate::antag::nukeop::server::Nukeop>(user)
            .is_some()
            || crate::implants::implants_of(world, user)
                .iter()
                .any(|i| i.kind == crate::implants::ImplantKind::WeaponsAuth)
    } else {
        // shortcut: other special pins (test range, DNA, clown, explorer, paywall) answer with
        // their `default_pin_auth` (a standard pin: anyone).
        dm.pin_default_auth
    };
    if ok {
        Ok(())
    } else {
        Err(dm.pin_fail_message.clone())
    }
}

fn ranged_clicks(
    world: &mut World,
    mut clicks: Local<bevy::ecs::message::MessageCursor<RangedClick>>,
) {
    let clicks: Vec<RangedClick> = clicks
        .read(world.resource::<Messages<RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(gun) = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
            .filter(|&g| world.get::<Gun>(g).is_some())
        else {
            continue;
        };
        // `COMSIG_RANGED_ITEM_INTERACTING_WITH_ATOM_SECONDARY`: a scoped gun zooms instead.
        if click.modifiers.right
            && crate::gun_attachments::scope_click(
                world,
                click.mob,
                gun,
                click.target_pos,
                click.modifiers,
            )
        {
            continue;
        }
        // `ranged_interact_with_atom_secondary`: a right click within the stray range holds up.
        if click.modifiers.right
            && !world.get::<CombatMode>(click.mob).is_some_and(|c| c.0)
            && world.get::<Health>(click.target).is_some()
            && world
                .get::<TilePos>(click.mob)
                .and_then(|p| p.chebyshev(click.target_pos))
                .is_some_and(|d| d <= crate::gunpoint::GUNPOINT_SHOOTER_STRAY_RANGE)
            && crate::gunpoint::start(world, click.client, click.mob, gun, click.target)
        {
            continue;
        }
        let aim = Aim::new(click.target, click.target_pos, click.modifiers);
        try_fire(world, click.client, click.mob, gun, aim, false);
    }
}

/// What a shot is aimed at: the clicked atom, its tile and the clicked pixel (tg `params`).
#[derive(Clone, Copy, Debug)]
pub struct Aim {
    pub target: Entity,
    pub target_pos: TilePos,
    /// Pixels from `target_pos`'s center, clamped to [`crate::projectile::MAX_AIM_PIXEL`].
    pub pixel: [i16; 2],
}

impl Aim {
    pub fn new(target: Entity, target_pos: TilePos, modifiers: ClickModifiers) -> Self {
        let max = crate::projectile::MAX_AIM_PIXEL;
        Self {
            target,
            target_pos,
            pixel: modifiers
                .pixel
                .map_or([0, 0], |p| p.map(|v| i16::from(v).clamp(-max, max))),
        }
    }
}

/// tg `interact_with_atom` of a held gun on an adjacent mob (`crate::security` hands it the
/// click): true when the click is used up (the gun fired, or refused without combat mode).
pub fn interact_with_mob(world: &mut World, click: &ClickResolved, gun: Entity) -> bool {
    if world.get::<Health>(click.target).is_none() {
        return false;
    }
    // `interact_with_atom_secondary`: in combat mode a right click bashes, otherwise it holds
    // the mob up.
    if click.modifiers.right {
        if world.get::<CombatMode>(click.mob).is_some_and(|c| c.0) {
            return false;
        }
        return crate::gunpoint::start(world, click.client, click.mob, gun, click.target);
    }
    // `fire_gun`: no shooting yourself unless the mouth is selected (`handle_suicide`).
    if click.target == click.mob && !suicide_aim(world, click.mob, gun) {
        return false;
    }
    let aim = Aim::new(click.target, click.target_pos, click.modifiers);
    try_fire(world, click.client, click.mob, gun, aim, true)
}

/// tg `fire_gun(target, user)` from afar (the gunpoint's charged shot). True when the click is
/// used up.
pub fn fire_gun(world: &mut World, client: Entity, mob: Entity, gun: Entity, aim: Aim) -> bool {
    try_fire(world, client, mob, gun, aim, false)
}

/// tg `doafter_self_shoot && user.zone_selected == BODY_ZONE_PRECISE_MOUTH`: a point blank click
/// becomes `handle_suicide`. The russian revolver (`doafter_self_shoot = FALSE`) just fires.
fn suicide_aim(world: &World, mob: Entity, gun: Entity) -> bool {
    world.get::<ZoneSelected>(mob).map(|z| z.0) == Some(crate::zone::TargetZone::Mouth)
        && world
            .get::<DmGun>(gun)
            .is_some_and(|dm| !is_under(&dm.path, "/obj/item/gun/ballistic/revolver/russian"))
}

/// tg `ishuman`.
fn is_human(world: &World, mob: Entity) -> bool {
    world.get::<crate::human::HumanLook>(mob).is_some()
}

/// tg `handle_suicide` up to its `do_after`: the gun goes in the mouth (or to the head of the
/// adjacent human) for 12 seconds ([`complete_suicide`]). False when it is not a human's.
fn handle_suicide(
    world: &mut World,
    client: Entity,
    mob: Entity,
    gun: Entity,
    target: Entity,
) -> bool {
    if !is_human(world, mob) || !is_human(world, target) {
        return false;
    }
    // `fire_cd`: one at a time.
    if world
        .get::<crate::tools::DoAfter>(mob)
        .is_some_and(|d| matches!(d.job, crate::construction::Job::GunSuicide))
    {
        return true;
    }
    let Some(&at) = world.get::<TilePos>(mob) else {
        return false;
    };
    let (user, item) = (name_of(world, mob), name_of(world, gun));
    let item = crate::obj_damage::strip(&item).to_owned();
    if mob == target {
        show(
            world,
            at,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Warning,
                format!("{user} вставляет {item} себе в рот, готовясь спустить курок..."),
            ),
            &[(
                mob,
                Span::BoldWarning,
                format!("Вы вставляете {item} себе в рот, готовясь спустить курок..."),
            )],
        );
    } else {
        let victim = name_of(world, target);
        show(
            world,
            at,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Warning,
                format!("{user} направляет {item} на голову {victim}, готовясь спустить курок..."),
            ),
            &[(
                target,
                Span::BoldWarning,
                format!("{user} направляет {item} на вашу голову, готовясь спустить курок..."),
            )],
        );
    }
    let actor = crate::tools::Actor {
        client,
        mob,
        item: Some(gun),
    };
    crate::tools::do_after(
        world,
        actor,
        target,
        SUICIDE_DELAY_DS,
        0,
        crate::construction::Job::GunSuicide,
    );
    true
}

/// `handle_suicide`'s `do_after(user, 12 SECONDS, target)`, deciseconds.
pub const SUICIDE_DELAY_DS: f32 = 120.0;

/// tg `handle_suicide` when the `do_after` stops (or the mouth is no longer selected).
pub fn suicide_interrupted(world: &mut World, mob: Entity, target: Entity) {
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    let user = name_of(world, mob);
    if mob == target {
        show(
            world,
            at,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (Span::Notice, format!("{user} решает не стрелять.")),
            &[],
        );
    } else if world
        .get::<TilePos>(target)
        .and_then(|t| t.chebyshev(at))
        .is_some_and(|d| d <= 1)
    {
        let victim = name_of(world, target);
        show(
            world,
            at,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (Span::Notice, format!("{user} решает пощадить {victim}")),
            &[(
                target,
                Span::Notice,
                format!("{user} решает пощадить вашу жизнь!"),
            )],
        );
    }
}

/// tg `handle_suicide` after its `do_after`: the trigger is pulled, the chambered round hits
/// the head for five times its damage (and +5 wound bonus).
pub fn complete_suicide(world: &mut World, mob: Entity, action: &crate::tools::DoAfter) {
    let target = action.target;
    let Some(gun) = action.item else {
        return;
    };
    if !suicide_aim(world, mob, gun) {
        suicide_interrupted(world, mob, target);
        return;
    }
    let (Some(&at), Some(&target_pos)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target))
    else {
        return;
    };
    let user = name_of(world, mob);
    let own = if mob == target {
        "Вы спускаете курок!".to_owned()
    } else {
        format!("{user} спускает курок!")
    };
    show(
        world,
        at,
        COMBAT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Warning, format!("{user} спускает курок!")),
        &[(target, Span::BoldWarning, own)],
    );
    let (Some(dm), Some(state)) = (
        world.get::<DmGun>(gun).cloned(),
        world.get::<Gun>(gun).cloned(),
    ) else {
        return;
    };
    if ready_round(&dm, &state).is_none() {
        empty_click(world, action.client, &dm, gun);
        return;
    }
    let aim = Aim {
        target,
        target_pos,
        pixel: [0, 0],
    };
    world.entity_mut(gun).insert(ChamberBoost {
        damage_mult: 5.0,
        wound_bonus: 5.0,
        exposed_wound_bonus: 0.0,
        head: true,
    });
    // shortcut: a burst gun fires only the suicide round, not the rest of its burst.
    let fired = shoot(world, mob, gun, aim, true, 0.0);
    world.entity_mut(gun).remove::<ChamberBoost>();
    if fired {
        let now = world.resource::<Time>().elapsed_secs_f64();
        let delay = f64::from(burst_and_delay(&dm, &state).1) / 10.0;
        if let Some(mut g) = world.get_mut::<Gun>(gun) {
            g.ready_at = now + delay;
        }
    }
}

/// tg `fire_gun` up to `process_fire`: pins, an empty chamber, `WEAPON_HEAVY`, dual wielding.
/// True when the click is used up.
fn try_fire(
    world: &mut World,
    client: Entity,
    mob: Entity,
    gun_entity: Entity,
    aim: Aim,
    point_blank: bool,
) -> bool {
    // A syringe or reagent gun chambers its spring while it holds a dart.
    crate::guns_special::before_fire(world, gun_entity);
    let (Some(dm), Some(gun)) = (
        world.get::<DmGun>(gun_entity).cloned(),
        world.get::<Gun>(gun_entity).cloned(),
    ) else {
        return false;
    };
    if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return false;
    }
    if world
        .resource::<Bursts>()
        .0
        .iter()
        .any(|b| b.gun == gun_entity)
    {
        return false;
    }
    // `/datum/status_effect/tased/block_firing`.
    if crate::taser::tasing_with(world, gun_entity) {
        return true;
    }
    // `can_trigger_gun`: `TRAIT_NOGUNS` (the sleeping carp, `crate::martial`).
    if crate::martial::no_guns(world, mob) {
        tell(
            world,
            client,
            Span::Warning,
            "Использовать огнестрельное оружие?! Ну уж нет.",
        );
        return true;
    }
    // `can_use_guns`: `TRIGGER_GUARD_NORMAL` and `TRAIT_CHUNKYFINGERS` (the Hulk).
    if crate::genetics::effects::chunky_fingers(world, mob) {
        return true;
    }
    if let Err(fail) = pin_auth(world, &dm, &gun, mob) {
        if gun.pin.is_empty() {
            let name = name_of(world, gun_entity);
            tell(
                world,
                client,
                Span::Warning,
                format!(
                    "Спусковой крючок {name} заблокирован. Это оружие не имеет установленный ударник!"
                ),
            );
        }
        tell(world, client, Span::Warning, fail);
        return true;
    }
    if point_blank && suicide_aim(world, mob, gun_entity) {
        return handle_suicide(world, client, mob, gun_entity, aim.target);
    }
    // A grenade launcher throws its grenade instead (`crate::guns_special`).
    if crate::guns_special::process_fire(world, client, mob, gun_entity, aim) {
        return true;
    }
    let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
    if ready_round(&dm, &gun).is_none() {
        empty_click(world, client, &dm, gun_entity);
        return !combat;
    }
    if check_botched(world, client, mob, gun_entity, &dm) {
        return true;
    }
    // `process_fire`: `TRAIT_PACIFISM` and a harmful round chambered.
    if world
        .get::<crate::medical::traumas::Pacifist>(mob)
        .is_some()
        && ready_round(&dm, &gun).is_some_and(|c| c.harmful)
    {
        let name = name_of(world, gun_entity);
        tell(
            world,
            client,
            Span::Warning,
            format!(
                "Летальный снаряд заряжен в {name}! Вы не хотите рисковать причинить кому-то вред..."
            ),
        );
        return true;
    }
    if world.resource::<Time>().elapsed_secs_f64() < gun.ready_at {
        return false;
    }
    let hands = world
        .get::<Inventory>(mob)
        .map(|i| i.hands)
        .unwrap_or_default();
    if dm.weapon_weight == WEAPON_HEAVY && hands.iter().any(|&h| h.is_some_and(|h| h != gun_entity))
    {
        tell(world, client, Span::Warning, "нужно обе руки!");
        return true;
    }
    // Dual wielding: the other hand's light guns fire too, every gun spreading more.
    let mut bonus_spread = 0.0;
    if combat {
        let others: Vec<Entity> = hands
            .into_iter()
            .flatten()
            .filter(|&g| g != gun_entity)
            .filter(|&g| {
                world
                    .get::<DmGun>(g)
                    .is_some_and(|d| d.weapon_weight < WEAPON_MEDIUM)
                    && world.get::<Gun>(g).is_some_and(|state| {
                        pin_auth(world, world.get::<DmGun>(g).expect("checked"), state, mob).is_ok()
                    })
            })
            .collect();
        for (n, &other) in others.iter().enumerate() {
            let spread = world
                .get::<DmGun>(other)
                .map_or(0.0, |d| d.dual_wield_spread);
            bonus_spread += spread * (1.0 + AKIMBO_PENALTY_SCALING * (n as f32 + 2.0));
        }
        if !others.is_empty() {
            bonus_spread += dm.dual_wield_spread;
            let now = world.resource::<Time>().elapsed_secs_f64();
            for (i, &other) in others.iter().enumerate() {
                world.resource_mut::<Bursts>().0.push(Burst {
                    gun: other,
                    mob,
                    aim,
                    at: now + 0.1 * (i as f64 + 1.0),
                    left: 0,
                    delay: 0.0,
                    spread: 0.0,
                    spread_mult: 0.0,
                    iteration: 0,
                    akimbo: Some((client, bonus_spread)),
                });
            }
        }
    }
    process_fire(world, mob, gun_entity, aim, bonus_spread) || !combat
}

/// The guns with `clumsy_check = FALSE`: magic staffs and wands, the toy guns.
const NO_CLUMSY_CHECK: &[&str] = &[
    "/obj/item/gun/magic",
    "/obj/item/gun/ballistic/automatic/toy",
    "/obj/item/gun/ballistic/shotgun/toy",
    "/obj/item/gun/ballistic/automatic/c20r/toy",
    "/obj/item/gun/ballistic/automatic/l6_saw/toy",
];

/// The zone a botched shot hits (`process_fire(user, user, FALSE, null, target_zone)`).
#[derive(Component, Clone, Copy, Debug)]
pub struct BotchZone(pub crate::zone::BodyZone);

/// tg `check_botched`: a clumsy user (`TRAIT_CLUMSY`) shoots a leg 40% of the time and drops
/// the gun. True when botched.
fn check_botched(world: &mut World, client: Entity, mob: Entity, gun: Entity, dm: &DmGun) -> bool {
    use crate::zone::BodyZone;
    if NO_CLUMSY_CHECK.iter().any(|p| is_under(&dm.path, p))
        || !crate::genetics::is_clumsy(world, mob)
        || !world.resource_mut::<CombatRng>().chance(40.0)
    {
        return false;
    }
    // `get_random_valid_zone(blacklisted_parts = chest, head, arms, even_weights = TRUE)`.
    let legs: Vec<BodyZone> = [BodyZone::LLeg, BodyZone::RLeg]
        .into_iter()
        .filter(|&z| {
            world
                .get::<crate::medical::wounds::Limbs>(mob)
                .is_none_or(|l| !l.limb(z).missing)
        })
        .collect();
    if legs.is_empty() {
        return false;
    }
    let zone = legs[world
        .resource_mut::<CombatRng>()
        .range(0, legs.len() as u32 - 1) as usize];
    let Some(&at) = world.get::<TilePos>(mob) else {
        return false;
    };
    let name = name_of(world, gun);
    tell(
        world,
        client,
        Span::BoldWarning,
        format!("Вы стреляете себе в ногу из {name}!"),
    );
    world.entity_mut(gun).insert(BotchZone(zone));
    let aim = Aim {
        target: mob,
        target_pos: at,
        pixel: [0, 0],
    };
    process_fire(world, mob, gun, aim, 0.0);
    if let Ok(mut e) = world.get_entity_mut(gun) {
        e.remove::<BotchZone>();
    }
    if !crate::inventory::no_drop(world, gun)
        && world
            .get::<Inventory>(mob)
            .is_some_and(|i| i.hands.contains(&Some(gun)))
    {
        crate::inventory::put_down(world, gun, at);
    }
    true
}

/// tg `process_fire`: the spread of this trigger pull, then one shot or a burst. False when the
/// gun was not ready.
fn process_fire(world: &mut World, mob: Entity, gun_entity: Entity, aim: Aim, bonus: f32) -> bool {
    let (Some(dm), Some(gun)) = (
        world.get::<DmGun>(gun_entity).cloned(),
        world.get::<Gun>(gun_entity).cloned(),
    ) else {
        return false;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < gun.ready_at {
        return false;
    }
    if misfires(world, mob, gun_entity, &dm, &gun, aim) {
        return true;
    }
    let (burst_size, fire_delay) = burst_and_delay(&dm, &gun);
    // `SAWN_OFF_ACC_PENALTY`.
    let bonus = bonus
        + if gun.sawn_off {
            SAWN_OFF_ACC_PENALTY
        } else {
            0.0
        };
    let (total, spread_mult) = {
        let rng = &mut world.resource_mut::<CombatRng>();
        let bonus = rng.range(0, bonus.max(0.0) as u32) as f32;
        let own = if dm.spread > 0.0 {
            rng.range(0, dm.spread as u32) as f32
        } else {
            0.0
        };
        ((bonus + own).max(0.0), rng.unit())
    };
    let sprd = if burst_size > 1 {
        burst_spread(
            &mut world.resource_mut::<CombatRng>(),
            dm.randomspread,
            total,
            spread_mult,
            burst_size,
            1,
        )
    } else {
        trigger_spread(&mut world.resource_mut::<CombatRng>(), total)
    };
    if !shoot(world, mob, gun_entity, aim, true, sprd) {
        return false;
    }
    let delay = f64::from(if dm.energy {
        dm.shots.get(gun.select).map_or(dm.fire_delay, |s| s.delay)
    } else {
        fire_delay
    }) / 10.0;
    if let Some(mut g) = world.get_mut::<Gun>(gun_entity) {
        g.ready_at = now + delay;
    }
    if burst_size > 1 {
        world.resource_mut::<Bursts>().0.push(Burst {
            gun: gun_entity,
            mob,
            aim,
            at: now + f64::from(dm.burst_delay) / 10.0,
            left: burst_size - 1,
            delay: f64::from(dm.burst_delay) / 10.0,
            spread: total,
            spread_mult,
            iteration: 1,
            akimbo: None,
        });
    }
    true
}

/// `SAWN_OFF_ACC_PENALTY`, degrees.
const SAWN_OFF_ACC_PENALTY: f32 = 25.0;

/// `/obj/item/gun/ballistic/process_fire`'s misfire: a gun (or a round) that `can_misfire`
/// goes off at its user `misfire_probability`% of the time (`blow_up`). True when it did.
fn misfires(
    world: &mut World,
    mob: Entity,
    gun_entity: Entity,
    dm: &DmGun,
    gun: &Gun,
    aim: Aim,
) -> bool {
    if !dm.ballistic || aim.target == mob || gun.misfire <= 0.0 {
        return false;
    }
    let Some(round) = ready_round(dm, gun) else {
        return false;
    };
    if !(dm.can_misfire || round.can_misfire)
        || !world.resource_mut::<CombatRng>().chance(gun.misfire)
    {
        return false;
    }
    let Some(&at) = world.get::<TilePos>(mob) else {
        return false;
    };
    if !blow_up(world, mob, gun_entity, at) {
        return false;
    }
    if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
        let name = ss13_shared::capitalize(&name_of(world, gun_entity));
        tell(world, client, Span::Danger, format!("{name} даёт осечку!"));
    }
    true
}

/// tg `blow_up`: the chambered live round fires at the user (`process_fire(user, user, FALSE)`).
pub fn blow_up(world: &mut World, mob: Entity, gun: Entity, at: TilePos) -> bool {
    let live = world
        .get::<DmGun>(gun)
        .zip(world.get::<Gun>(gun))
        .is_some_and(|(dm, g)| dm.ballistic && ready_round(dm, g).is_some());
    if !live {
        return false;
    }
    let aim = Aim {
        target: mob,
        target_pos: at,
        pixel: [0, 0],
    };
    shoot(world, mob, gun, aim, false, 0.0)
}

/// tg `process_fire`'s single shot spread: `round((rand(0, 1) - 0.5) *
/// DUALWIELD_PENALTY_EXTRA_MULTIPLIER * total)` (DM `round` floors; `rand(0, 1)` is 0 or 1).
pub fn trigger_spread(rng: &mut CombatRng, total: f32) -> f32 {
    ((rng.range(0, 1) as f32 - 0.5) * DUALWIELD_PENALTY_EXTRA_MULTIPLIER * total).floor()
}

/// tg `process_burst`'s spread for shot `iteration` (from 1) of `size`.
pub fn burst_spread(
    rng: &mut CombatRng,
    randomspread: bool,
    total: f32,
    mult: f32,
    size: u32,
    iteration: u32,
) -> f32 {
    if randomspread {
        trigger_spread(rng, total)
    } else {
        ((mult / size as f32 * iteration as f32 - (0.5 + mult * 0.25)) * total).floor()
    }
}

/// tg `fire_casing` (one pellet) and `pellet_cloud/create_casing_pellets` (several): each
/// pellet's deviation in degrees. `variance` replaces the gun's spread `sprd`.
pub fn pellet_spreads(rng: &mut CombatRng, casing: &CasingData, sprd: f32) -> Vec<f32> {
    let n = casing.pellets.max(1);
    let distro = casing.variance;
    (1..=n)
        .map(|i| {
            if distro == 0.0 {
                sprd
            } else if casing.randomspread {
                ((rng.unit() - 0.5) * distro).floor()
            } else if n == 1 {
                // `round(1 - 0.5) * distro`: DM's `round` floors it to nothing.
                0.0
            } else {
                ((i as f32 / n as f32 - 0.5) * distro).floor()
            }
        })
        .collect()
}

/// tg `playsound(src, ...)` of a gun: from wherever it is.
fn gun_sound(world: &mut World, gun: Entity, sound: &str, vol: f32, vary: bool) {
    if let Some(at) = crate::modpc::pda::turf_of(world, gun) {
        crate::sound::playsound(world, PlaySound::new(at, sound, vol, vary));
    }
}

/// tg `/obj/item/gun/energy/fire_sounds` pitch: from 1 at a full cell down to 0.3 on the last
/// shot it holds, by the charge before the shot (tg drains the cell after the sound).
pub fn energy_pitch(e_cost: f64, charge: f64, maxcharge: f64) -> f32 {
    if maxcharge <= 0.0 {
        return 1.0;
    }
    let cost_percent = ((e_cost / maxcharge).clamp(0.0, 1.0) * 100.0).round();
    // Oversized or infinite cells and free shots keep the plain pitch.
    if cost_percent <= 0.0 || cost_percent >= 100.0 {
        return 1.0;
    }
    let max_shots = (100.0 / cost_percent).round() - 1.0;
    // tg divides by zero here (a shot costing over 2/3 of the cell) and plays nothing; the
    // plain pitch is kept instead.
    if max_shots <= 0.0 {
        return 1.0;
    }
    let shots_left =
        (((charge / maxcharge).clamp(0.0, 1.0) * 100.0).round() / cost_percent).round() - 1.0;
    let amount = (1.0 - shots_left / max_shots).powi(2);
    (1.0 + (0.3 - 1.0) * amount) as f32
}

/// tg `fire_sounds` (`gun.dm`, `energy.dm`, `ballistic.dm`): the shot (an energy gun's lower
/// as its cell drains), and a ballistic gun's click when its ammo runs low (`current_ammo`
/// counts the spent chambered casing, as `get_ammo`). `gun` is after the shot was paid.
fn fire_sounds(world: &mut World, dm: &DmGun, gun: &Gun, entity: Entity, casing: &CasingData) {
    let s = &dm.sounds;
    // `if(suppressed)`: the quiet shot, heard only nearby (`SILENCED_SOUND_EXTRARANGE`).
    let quiet = gun.suppressed > 0;
    if quiet {
        if let Some(at) = crate::modpc::pda::turf_of(world, entity) {
            let sound = PlaySound::new(
                at,
                &*dm.suppressed_sound,
                dm.suppressed_volume,
                s.vary_fire_sound,
            );
            crate::sound::playsound(world, silenced(sound));
        }
    } else if dm.energy {
        let pitch = energy_pitch(casing.e_cost, gun.charge + casing.e_cost, dm.cell_maxcharge);
        if let Some(at) = crate::modpc::pda::turf_of(world, entity) {
            let sound = PlaySound::new(
                at,
                &*casing.fire_sound,
                s.fire_sound_volume,
                s.vary_fire_sound,
            );
            crate::sound::playsound(world, sound.pitch(pitch));
        }
    } else {
        gun_sound(
            world,
            entity,
            &s.fire_sound,
            s.fire_sound_volume,
            s.vary_fire_sound,
        );
    }
    if !dm.ballistic || !s.click_on_low_ammo {
        return;
    }
    let max_ammo = gun
        .magazine
        .as_ref()
        .map_or(dm.magazine.max_ammo, |m| m.data.max_ammo);
    let current = usize::from(gun.chambered.is_some())
        + gun.magazine.as_ref().map_or(0, |m| m.ammo_count(true));
    if max_ammo == 0 || ((max_ammo as f32 * 2.0).sqrt().round() as usize) <= current {
        return;
    }
    let frequency = ((90.0 / max_ammo as f32) * current as f32)
        .to_radians()
        .sin();
    if let Some(at) = crate::modpc::pda::turf_of(world, entity) {
        let volume = if quiet {
            dm.suppressed_volume
        } else {
            s.fire_sound_volume
        };
        let click = PlaySound::new(
            at,
            ss13_mapload::sound::files::BALLISTIC_CLICK,
            volume,
            s.vary_fire_sound,
        )
        .frequency(1.0 - frequency * 0.75);
        crate::sound::playsound(world, if quiet { silenced(click) } else { click });
    }
}

/// `ignore_walls = FALSE, extrarange = SILENCED_SOUND_EXTRARANGE, falloff_distance = 0`.
fn silenced(sound: PlaySound) -> PlaySound {
    sound
        .ignore_walls(false)
        .extrarange(-10.0)
        .falloff_distance(0.0)
}

/// tg `shoot_with_empty_chamber`; a revolver turns its cylinder.
fn empty_click(world: &mut World, client: Entity, dm: &DmGun, gun: Entity) {
    tell(world, client, Span::Warning, "*щёлк*");
    gun_sound(
        world,
        gun,
        &dm.sounds.dry_fire_sound,
        dm.sounds.dry_fire_sound_volume,
        true,
    );
    if is_revolver(dm)
        && let Some(mut g) = world.get_mut::<Gun>(gun)
    {
        chamber_round(dm, &mut g);
    }
}

/// tg `fire_casing` + `shoot_live_shot` + `process_chamber` for one round, with the gun's
/// [`ChamberBoost`]. False when nothing was ready.
fn shoot(
    world: &mut World,
    mob: Entity,
    gun_entity: Entity,
    aim: Aim,
    message: bool,
    sprd: f32,
) -> bool {
    let Aim {
        target, target_pos, ..
    } = aim;
    let (Some(dm), Some(mut gun)) = (
        world.get::<DmGun>(gun_entity).cloned(),
        world.get::<Gun>(gun_entity).cloned(),
    ) else {
        return false;
    };
    let Some(casing) = ready_round(&dm, &gun) else {
        return false;
    };
    let Some(&from) = world.get::<TilePos>(mob) else {
        return false;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    // The boost belongs to this round only.
    let boost = world.entity_mut(gun_entity).take::<ChamberBoost>();
    let botch = world.get::<BotchZone>(gun_entity).map(|b| b.0);
    let zone = if let Some(zone) = botch {
        zone
    } else if boost.is_some_and(|b| b.head) {
        crate::zone::BodyZone::Head
    } else {
        world
            .get::<ZoneSelected>(mob)
            .copied()
            .unwrap_or_default()
            .0
            .check_zone()
    };
    let hit_prone = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
    // `/obj/item/gun/ballistic/shoot_live_shot`: the gun and the round wear it towards a
    // misfire.
    if dm.ballistic {
        let cap = dm.misfire_probability_cap;
        if dm.can_misfire {
            gun.misfire = (gun.misfire + dm.misfire_percentage_increment).clamp(0.0, cap);
        }
        if casing.can_misfire {
            gun.misfire = (gun.misfire + casing.misfire_increment).clamp(0.0, cap);
        }
    }
    // The round is spent before the projectile flies (a point blank hit can change the gun's
    // holder).
    if dm.energy {
        gun.charge -= casing.e_cost;
    } else if is_revolver(&dm) {
        if let Some(round) = gun.magazine.as_mut().and_then(|m| m.rounds.last_mut()) {
            round.live = false;
        }
    } else if let Some(round) = gun.chambered.as_mut() {
        round.live = false;
    }
    let cooldown = if casing.click_cooldown_override > 0.0 {
        f64::from(casing.click_cooldown_override) / 10.0
    } else {
        CLICK_CD_RANGE
    };
    world.entity_mut(mob).insert(NextMove(now + cooldown));
    if let Some(mut facing) = world.get_mut::<ss13_shared::Facing>(mob)
        && let Some(dir) = crate::mecha::core::dir_between(from, target_pos)
    {
        facing.set_if_neq(ss13_shared::Facing(dir));
    }
    // `shoot_live_shot`: `if(suppressed || !message) return`.
    if message && gun.suppressed == 0 {
        shot_message(world, mob, gun_entity, target, from, target_pos);
    }
    fire_sounds(world, &dm, &gun, gun_entity, &casing);
    if !dm.energy && !is_revolver(&dm) {
        handle_chamber(world, &dm, &mut gun, from, true);
        if gun.chambered.is_none()
            && gun.magazine.as_ref().is_none_or(|m| m.rounds.is_empty())
            && dm.bolt_type == BOLT_TYPE_LOCKING
            && dm.semi_auto
        {
            gun.bolt_locked = true;
        }
        // `postfire_empty_checks`: the last round is gone.
        let s = &dm.sounds;
        if s.empty_alarm
            && gun.chambered.is_none()
            && gun.magazine.as_ref().is_none_or(|m| m.rounds.is_empty())
        {
            let file = s.empty_alarm_sound.clone();
            gun_sound(
                world,
                gun_entity,
                &file,
                s.empty_alarm_volume,
                s.empty_alarm_vary,
            );
        }
    } else if is_revolver(&dm) {
        chamber_round(&dm, &mut gun);
    }
    // `/obj/item/gun/ballistic/shotgun/automatic/shoot_live_shot`: `rack()`.
    if dm.auto_rack {
        handle_chamber(world, &dm, &mut gun, from, false);
    }
    world.entity_mut(gun_entity).insert(gun);
    // `ready_proj`: the gun's `projectile_damage_multiplier`.
    let mut data = casing.projectile.clone();
    let mult = if dm.projectile_damage_multiplier > 0.0 {
        dm.projectile_damage_multiplier
    } else {
        1.0
    };
    data.damage *= mult;
    data.stamina *= mult;
    if let Some(boost) = boost {
        boost.apply(&mut data);
    }
    // `/obj/item/ammo_casing/energy/kinetic/ready_proj`: the accelerator's modkits.
    crate::mining::modkits::modify_projectile(world, gun_entity, &mut data);
    // `ready_proj` of a special casing: the syringe, injector or synthesised dart it carries.
    let mut dart = crate::guns_special::ready_proj(world, gun_entity, mob, &mut data);
    let spreads = pellet_spreads(&mut world.resource_mut::<CombatRng>(), &casing, sprd);
    for deviation in spreads {
        let shot = Shot {
            data: &data,
            firer: mob,
            from,
            target: Some(target),
            target_pos,
            zone,
            hit_prone,
            pixel: aim.pixel,
            deviation,
            fired_from: Some(gun_entity),
        };
        match dart.take() {
            Some(dart) => crate::projectile::fire_with(world, shot, dart),
            None => crate::projectile::fire(world, shot),
        };
    }
    // `shoot_live_shot` -> `simulate_recoil` -> `calculate_recoil`.
    // shortcut: the `min_recoil_multiplier` preference is always its default (100%).
    let sawn = if world.get::<Gun>(gun_entity).is_some_and(|g| g.sawn_off) {
        dm.sawoff_bonus_recoil
    } else {
        0.0
    };
    let recoil = (dm.recoil + sawn).max(dm.min_recoil);
    if recoil > 0.0 {
        let deviation = dm.recoil_deviation.max(0.0) as u32;
        let jitter =
            world.resource_mut::<CombatRng>().range(0, 2 * deviation) as f32 - deviation as f32;
        let angle = crate::projectile::aim_angle(from, target_pos, aim.pixel) + jitter + 180.0;
        crate::recoil::recoil_camera(
            world,
            mob,
            recoil + 1.0,
            recoil * dm.recoil_backtime_multiplier + 1.0,
            recoil,
            angle,
        );
    }
    true
}

/// tg `handle_chamber`: the chambered casing goes onto the floor (when fired, only with a
/// `casing_ejector`), then the next round is chambered.
fn handle_chamber(world: &mut World, dm: &DmGun, gun: &mut Gun, at: TilePos, from_firing: bool) {
    if from_firing && !dm.semi_auto {
        return;
    }
    if let Some(round) = gun.chambered.take()
        && (dm.casing_ejector || !from_firing)
    {
        drop_casing(world, &round, at);
    }
    if gun.magazine.as_ref().is_some_and(|m| m.data.max_ammo > 1) {
        chamber_round(dm, gun);
    }
}

/// A casing onto `at`: a live one keeps its round.
fn drop_casing(world: &mut World, round: &Round, at: TilePos) {
    if round.casing.path.is_empty() {
        return;
    }
    if let Some(casing) = crate::construction::spawn_obj(world, &round.casing.path, at)
        && !round.live
        && let Some(mut dm) = world.get_mut::<ss13_mapload::DmCasing>(casing)
    {
        // tg: a fired casing has no `loaded_projectile`.
        dm.data.projectile = default();
    }
}

/// tg `shoot_live_shot` messages.
fn shot_message(
    world: &mut World,
    mob: Entity,
    gun: Entity,
    target: Entity,
    from: TilePos,
    target_pos: TilePos,
) {
    let (user, item) = (name_of(world, mob), name_of(world, gun));
    let item = crate::obj_damage::strip(&item).to_owned();
    let point_blank = from.chebyshev(target_pos).is_some_and(|d| d <= 1)
        && target != mob
        && world.get::<Health>(target).is_some();
    if point_blank {
        let victim = name_of(world, target);
        show(
            world,
            from,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Danger,
                format!("{user} стреляет из {item} в упор по {victim}!"),
            ),
            &[
                (
                    mob,
                    Span::Danger,
                    format!("Вы стреляете из {item} в упор по {victim}!"),
                ),
                (
                    target,
                    Span::BoldWarning,
                    format!("{user} стреляет из {item} в упор по вам!"),
                ),
            ],
        );
    } else {
        show(
            world,
            from,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (Span::Danger, format!("{user} стреляет из {item}!")),
            &[(mob, Span::BoldWarning, format!("Вы стреляете из {item}!"))],
        );
    }
}

/// tg `process_burst` for the rounds after the first: the user must still hold the gun.
fn run_bursts(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut bursts = std::mem::take(&mut world.resource_mut::<Bursts>().0);
    bursts.retain_mut(|b| {
        if now < b.at {
            return true;
        }
        let held = world
            .get::<Inventory>(b.mob)
            .is_some_and(|inv| inv.holds(b.gun));
        let conscious = world.get::<Stat>(b.mob) == Some(&Stat::Conscious);
        if !held || !conscious {
            return false;
        }
        let mut aim = b.aim;
        aim.target_pos = world
            .get::<TilePos>(aim.target)
            .copied()
            .unwrap_or(aim.target_pos);
        if let Some((client, bonus)) = b.akimbo {
            // `process_fire` of the other gun, after `can_trigger_gun`.
            let ready = world
                .get::<DmGun>(b.gun)
                .zip(world.get::<Gun>(b.gun))
                .is_some_and(|(dm, gun)| {
                    pin_auth(world, dm, gun, b.mob).is_ok() && ready_round(dm, gun).is_some()
                });
            if ready {
                process_fire(world, b.mob, b.gun, aim, bonus);
            } else if let Some(dm) = world.get::<DmGun>(b.gun).cloned() {
                empty_click(world, client, &dm, b.gun);
            }
            return false;
        }
        b.iteration += 1;
        let size = world
            .get::<DmGun>(b.gun)
            .zip(world.get::<Gun>(b.gun))
            .map_or(1, |(d, g)| burst_and_delay(d, g).0);
        let randomspread = world.get::<DmGun>(b.gun).is_some_and(|d| d.randomspread);
        let sprd = burst_spread(
            &mut world.resource_mut::<CombatRng>(),
            randomspread,
            b.spread,
            b.spread_mult,
            size,
            b.iteration,
        );
        if !shoot(world, b.mob, b.gun, aim, true, sprd) {
            return false;
        }
        b.left -= 1;
        b.at = now + b.delay;
        b.left > 0
    });
    world.resource_mut::<Bursts>().0.extend(bursts);
}

/// tg `attack_self` of a gun: a ballistic gun ejects an empty magazine, unloads a revolver,
/// drops a locked bolt or racks (the dual-tube shotgun switches tubes when chambered); an
/// energy gun switches its fire mode. True when it was a gun.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let (Some(dm), Some(mut gun)) = (
        world.get::<DmGun>(item).cloned(),
        world.get::<Gun>(item).cloned(),
    ) else {
        return false;
    };
    let Some(&at) = world.get::<TilePos>(mob) else {
        return true;
    };
    if dm.energy {
        if dm.shots.len() > 1 {
            gun.select = (gun.select + 1) % dm.shots.len();
            let name = &dm.shots[gun.select].select_name;
            tell(
                world,
                client,
                Span::Notice,
                format!("выбран режим: {}", mode_name(name)),
            );
            world.entity_mut(item).insert(gun);
        } else if is_under(&dm.path, PULSE_DESTROYER) {
            // `/obj/item/gun/energy/pulse/destroyer/attack_self`.
            let name = crate::obj_damage::strip(&name_of(world, item)).to_owned();
            tell(
                world,
                client,
                Span::Danger,
                format!("У {name} три режима, и все они — УНИЧТОЖЕНИЕ."),
            );
        }
        return true;
    }
    // `/obj/item/gun/ballistic/shotgun/automatic/dual_tube/attack_self`.
    if gun.alt_magazine.is_some() {
        let ammo = gun.magazine.as_ref().map_or(0, |m| m.ammo_count(true));
        if gun.chambered.is_none() && ammo > 0 {
            rack(world, client, item);
        } else {
            crate::gun_modes::toggle_tube(world, client, item);
        }
        return true;
    }
    if !dm.internal_magazine && gun.magazine.as_ref().is_some_and(|m| m.rounds.is_empty()) {
        eject_magazine(world, client, mob, item);
        return true;
    }
    if dm.bolt_type == BOLT_TYPE_NO_BOLT {
        // tg `unload_ammo`: every casing in it falls out (a cylinder keeps its chambers, empty).
        let rounds: Vec<Round> = gun
            .magazine
            .as_mut()
            .map(|m| {
                let taken = std::mem::take(&mut m.rounds);
                if m.is_cylinder() {
                    m.rounds = taken.iter().map(|_| Round::empty()).collect();
                }
                taken
            })
            .unwrap_or_default()
            .into_iter()
            .filter(|r| !r.is_empty())
            .collect();
        if rounds.is_empty() {
            tell(world, client, Span::Warning, "пуст!");
        } else {
            for round in &rounds {
                drop_casing(world, round, at);
            }
            gun_sound(
                world,
                mob,
                &dm.sounds.eject_sound,
                dm.sounds.eject_sound_volume,
                dm.sounds.eject_sound_vary,
            );
            tell(
                world,
                client,
                Span::Notice,
                format!(
                    "{} {} разряжен{}",
                    rounds.len(),
                    patron(rounds.len()),
                    plural(rounds.len())
                ),
            );
        }
        world.entity_mut(item).insert(gun);
        return true;
    }
    if dm.bolt_type == BOLT_TYPE_LOCKING && gun.bolt_locked {
        // tg `drop_bolt`.
        chamber_round(&dm, &mut gun);
        gun.bolt_locked = false;
        tell(world, client, Span::Notice, "спуск затвора");
        gun_sound(
            world,
            item,
            &dm.sounds.bolt_drop_sound,
            dm.sounds.bolt_drop_sound_volume,
            false,
        );
        world.entity_mut(item).insert(gun);
        return true;
    }
    rack(world, client, item);
    true
}

/// `/obj/item/gun/energy/pulse/destroyer`.
const PULSE_DESTROYER: &str = "/obj/item/gun/energy/pulse/destroyer";

/// tg `rack(user)`: the chambered round out onto the gun's tile, the next one in; a locking bolt
/// locks back on an empty gun.
pub fn rack(world: &mut World, client: Entity, item: Entity) {
    let (Some(dm), Some(mut gun)) = (
        world.get::<DmGun>(item).cloned(),
        world.get::<Gun>(item).cloned(),
    ) else {
        return;
    };
    if dm.bolt_type == BOLT_TYPE_NO_BOLT {
        return;
    }
    let Some(at) = crate::modpc::pda::turf_of(world, item) else {
        return;
    };
    if dm.bolt_type == BOLT_TYPE_OPEN {
        if !gun.bolt_locked {
            tell(
                world,
                client,
                Span::Warning,
                "повторный взвод затвора невозможен!",
            );
            return;
        }
        gun.bolt_locked = false;
    }
    tell(world, client, Span::Notice, "взвод затвора");
    handle_chamber(world, &dm, &mut gun, at, false);
    let s = &dm.sounds;
    if dm.bolt_type == BOLT_TYPE_LOCKING && gun.chambered.is_none() {
        gun.bolt_locked = true;
        gun_sound(
            world,
            item,
            &s.lock_back_sound,
            s.lock_back_sound_volume,
            s.lock_back_sound_vary,
        );
    } else {
        gun_sound(
            world,
            item,
            &s.rack_sound,
            s.rack_sound_volume,
            s.rack_sound_vary,
        );
    }
    world.entity_mut(item).insert(gun);
}

/// BandaStation's `ammo_mode_translations`.
fn mode_name(select_name: &str) -> &str {
    match select_name {
        "stun" => "оглушение",
        "disable" => "обезвреживание",
        "kill" => "летальный",
        "scatter" => "разброс",
        "snare" => "силки",
        "DESTROY" => "УНИЧТОЖЕНИЕ",
        "ion" => "ионный",
        other => other,
    }
}

/// `declension_ru(n, "", "а", "ов")` after "патрон".
fn patron(n: usize) -> &'static str {
    match (n % 10, n % 100) {
        (_, 11..=14) => "патронов",
        (1, _) => "патрон",
        (2..=4, _) => "патрона",
        _ => "патронов",
    }
}

/// `declension_ru(n, "", "о", "ы")`.
fn plural(n: usize) -> &'static str {
    match (n % 10, n % 100) {
        (_, 11..=14) => "о",
        (1, _) => "",
        (2..=4, _) => "ы",
        _ => "о",
    }
}

/// tg `eject_magazine`: the magazine goes into the user's hands.
fn eject_magazine(world: &mut World, client: Entity, mob: Entity, item: Entity) {
    let Some(mut gun) = world.get::<Gun>(item).cloned() else {
        return;
    };
    let (Some(dm), Some(mag)) = (world.get::<DmGun>(item).cloned(), gun.magazine.take()) else {
        return;
    };
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    if dm.bolt_type == BOLT_TYPE_OPEN {
        gun.chambered = None;
    }
    world.entity_mut(item).insert(gun);
    eject_sound(world, &dm, item, &mag);
    if let Some(entity) = crate::construction::spawn_obj(world, &mag.data.path, at) {
        world.entity_mut(entity).insert(mag);
        if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
            crate::storage::put_in_hands(world, mob, &inventory, entity);
        }
    }
    tell(world, client, Span::Notice, "магазин извлечён");
}

/// tg `eject_magazine`'s sound: full or empty.
fn eject_sound(world: &mut World, dm: &DmGun, gun: Entity, mag: &Magazine) {
    let s = &dm.sounds;
    let file = if mag.ammo_count(true) > 0 {
        &s.eject_sound
    } else {
        &s.eject_empty_sound
    };
    gun_sound(world, gun, file, s.eject_sound_volume, s.eject_sound_vary);
}

/// tg `attack_hand` of a held ballistic gun with a magazine: it comes out. True when it did.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let detachable = world
        .get::<DmGun>(item)
        .is_some_and(|dm| dm.ballistic && !dm.internal_magazine);
    let loaded = world.get::<Gun>(item).is_some_and(|g| g.magazine.is_some());
    let holding = world.get::<Inventory>(mob).is_some_and(|i| i.holds(item));
    if !(detachable && loaded && holding) {
        return false;
    }
    eject_magazine(world, client, mob, item);
    true
}

/// tg `/obj/item/gun/ballistic/item_interaction`: a magazine into the gun, rounds from a box or
/// speedloader into a revolver. True when it was the gun's click.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let (Some(dm), Some(mut gun)) = (
        world.get::<DmGun>(target).cloned(),
        world.get::<Gun>(target).cloned(),
    ) else {
        return false;
    };
    // A loose casing into an internal magazine (tg `ammo_box/attackby` -> `give_round`).
    if let Some(casing) = world
        .get::<ss13_mapload::DmCasing>(held)
        .map(|c| c.data.clone())
    {
        if !dm.ballistic || !dm.internal_magazine {
            return false;
        }
        let Some(mag) = gun.magazine.as_mut() else {
            return true;
        };
        let round = Round {
            live: !casing.projectile.path.is_empty(),
            casing,
        };
        // A cylinder's spent casing falls out of the chamber it frees.
        let Some(spent) = mag.give_round(round) else {
            return true;
        };
        if let (Some(spent), Some(at)) = (spent, crate::modpc::pda::turf_of(world, target)) {
            drop_casing(world, &spent, at);
        }
        if gun.chambered.is_none() && dm.bolt_type == BOLT_TYPE_NO_BOLT && !is_revolver(&dm) {
            chamber_round(&dm, &mut gun);
        }
        world.entity_mut(target).insert(gun);
        if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
            for hand in &mut inventory.hands {
                if *hand == Some(held) {
                    *hand = None;
                }
            }
        }
        world.despawn(held);
        tell(world, client, Span::Notice, "1 патрон заряжен");
        let s = &dm.sounds;
        gun_sound(
            world,
            target,
            &s.load_sound,
            s.load_sound_volume,
            s.load_sound_vary,
        );
        return true;
    }
    let Some(mut ammo) = world.get::<Magazine>(held).cloned() else {
        return false;
    };
    if !dm.ballistic {
        return false;
    }
    let is_magazine = is_under(&ammo.data.path, "/obj/item/ammo_box/magazine");
    if !dm.internal_magazine && is_magazine {
        if !is_under(&ammo.data.path, &dm.accepted_magazine) {
            let name = name_of(world, held);
            tell(
                world,
                client,
                Span::Warning,
                format!("{name} не вставляется!"),
            );
            return true;
        }
        let s = &dm.sounds;
        if let Some(old) = &gun.magazine {
            eject_sound(world, &dm, target, old);
        }
        let file = if ammo.ammo_count(true) > 0 {
            &s.load_sound
        } else {
            &s.load_empty_sound
        };
        gun_sound(world, target, file, s.load_sound_volume, s.load_sound_vary);
        let old = gun.magazine.replace(ammo);
        if dm.bolt_type == BOLT_TYPE_OPEN && !gun.bolt_locked {
            chamber_round(&dm, &mut gun);
        }
        world.entity_mut(target).insert(gun);
        if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
            for hand in &mut inventory.hands {
                if *hand == Some(held) {
                    *hand = None;
                }
            }
        }
        world.despawn(held);
        if let Some(old) = old
            && let Some(&at) = world.get::<TilePos>(mob)
            && let Some(entity) = crate::construction::spawn_obj(world, &old.data.path, at)
        {
            world.entity_mut(entity).insert(old);
            if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
                crate::storage::put_in_hands(world, mob, &inventory, entity);
            }
        }
        tell(world, client, Span::Notice, "вставка магазина");
        return true;
    }
    if dm.internal_magazine || dm.bolt_type == BOLT_TYPE_NO_BOLT {
        // tg `load_gun`: a spent chambered casing falls out first.
        if gun.chambered.as_ref().is_some_and(|r| !r.live)
            && let Some(round) = gun.chambered.take()
            && let Some(&at) = world.get::<TilePos>(mob)
        {
            drop_casing(world, &round, at);
        }
        let Some(mag) = gun.magazine.as_mut() else {
            return true;
        };
        let mut spent = Vec::new();
        let loaded = mag.load_from(&mut ammo, &mut spent) as usize;
        if loaded == 0 {
            return true;
        }
        if let Some(at) = crate::modpc::pda::turf_of(world, target) {
            for round in &spent {
                drop_casing(world, round, at);
            }
        }
        if gun.chambered.is_none() && dm.bolt_type == BOLT_TYPE_NO_BOLT && !is_revolver(&dm) {
            chamber_round(&dm, &mut gun);
        }
        world.entity_mut(target).insert(gun);
        world.entity_mut(held).insert(ammo);
        let s = &dm.sounds;
        gun_sound(
            world,
            target,
            &s.load_sound,
            s.load_sound_volume,
            s.load_sound_vary,
        );
        tell(
            world,
            client,
            Span::Notice,
            format!("{loaded} {} заряжен{}", patron(loaded), plural(loaded)),
        );
        return true;
    }
    false
}

/// BandaStation `/obj/structure/rack/gunrack`.
const GUN_RACK: &str = "/obj/structure/rack/gunrack";

/// A gun stood up on a gun rack (`rotate_weapon`) until it is picked up.
#[derive(Component, Clone, Copy, Debug)]
pub struct Racked;

/// BandaStation `gunrack/base_item_interaction` (out of combat mode): where a gun goes on the
/// rack, at the clicked column and 2 pixels up. `None` when `held` is not a gun or `rack` not a
/// gun rack.
// shortcut: wide (48 and 64 px) guns are not scaled down or shifted back by their width.
pub fn gun_rack_offset(
    world: &World,
    held: Entity,
    rack: Entity,
    modifiers: ClickModifiers,
) -> Option<[i16; 2]> {
    if world.get::<Gun>(held).is_none()
        || !crate::construction::path_of(world, rack).is_some_and(|p| is_under(&p, GUN_RACK))
    {
        return None;
    }
    let x = modifiers.pixel.map_or(0, |p| i16::from(p[0]));
    Some([x.clamp(-16, 16), 2])
}

/// `rotate_weapon(being_removed = FALSE)`: `Turn(-90)`.
pub fn rack_gun(world: &mut World, gun: Entity) {
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(gun) {
        look.rotation = 270;
    }
    world.entity_mut(gun).insert(Racked);
}

/// `item_picked_up` -> `rotate_weapon(being_removed = TRUE)`: upright again in a hand.
fn unrack(
    add: On<Add<crate::inventory::CarriedBy>>,
    racked: Query<(), With<Racked>>,
    mut looks: Query<&mut ss13_shared::Appearance>,
    mut commands: Commands,
) {
    if racked.get(add.entity).is_err() {
        return;
    }
    commands.entity(add.entity).remove::<Racked>();
    if let Ok(mut look) = looks.get_mut(add.entity) {
        look.rotation = 0;
    }
}

/// The energy gun's cell for a recharger: (charge, maxcharge, chargerate), when it takes one.
pub fn energy_cell(world: &World, item: Entity) -> Option<(f64, f64, f64)> {
    let dm = world.get::<DmGun>(item).filter(|d| d.energy)?;
    let gun = world.get::<Gun>(item)?;
    Some((gun.charge, dm.cell_maxcharge, dm.cell_chargerate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_mapload::ProjectileData;

    #[test]
    fn energy_shots_drop_in_pitch_as_the_cell_drains() {
        // A shot costing 10% of the cell: 9 more shots at full charge, none on the last one.
        assert_eq!(energy_pitch(100.0, 1000.0, 1000.0), 1.0);
        assert!((energy_pitch(100.0, 100.0, 1000.0) - 0.3).abs() < 1e-6);
        // Half a cell: 4 left of 9, (5/9)^2 of the way down: 1 - 0.7 * 0.3086 = 0.784.
        assert!((energy_pitch(100.0, 500.0, 1000.0) - 0.784).abs() < 1e-3);
        // Free shots and huge cells keep the plain pitch.
        assert_eq!(energy_pitch(0.0, 500.0, 1000.0), 1.0);
        assert_eq!(energy_pitch(1.0, 500.0, 1e9), 1.0);
    }

    fn casing(caliber: &str) -> CasingData {
        CasingData {
            path: "/obj/item/ammo_casing/c9mm".into(),
            caliber: caliber.into(),
            projectile: ProjectileData {
                path: "/obj/projectile/bullet/c9mm".into(),
                damage: 30.0,
                ..default()
            },
            ..default()
        }
    }

    fn pistol() -> DmGun {
        DmGun {
            ballistic: true,
            bolt_type: BOLT_TYPE_LOCKING,
            casing_ejector: true,
            semi_auto: true,
            spawnwithmagazine: true,
            magazine: MagazineData {
                path: "/obj/item/ammo_box/magazine/m9mm".into(),
                caliber: "9mm".into(),
                max_ammo: 3,
                ammo: casing("9mm"),
                start_empty: false,
            },
            ..default()
        }
    }

    #[test]
    fn a_locking_bolt_pistol_starts_with_one_in_the_chamber_on_top() {
        let dm = pistol();
        let mut gun = Gun {
            magazine: Some(Magazine::new(&dm.magazine)),
            ..default()
        };
        chamber_round(&dm, &mut gun);
        assert!(gun.chambered.is_some());
        assert_eq!(gun.magazine.as_ref().unwrap().rounds.len(), 2);
        assert!(ready_round(&dm, &gun).is_some());
    }

    #[test]
    fn spread_follows_tg_rounding() {
        let mut rng = CombatRng(7);
        for _ in 0..50 {
            // `rand(0, 1)` is 0 or 1: always -0.7 or +0.7 of the total, floored.
            let s = trigger_spread(&mut rng, 10.0);
            assert!(s == -7.0 || s == 7.0, "{s}");
        }
        assert_eq!(trigger_spread(&mut rng, 0.0).abs(), 0.0);
        // Smart burst spread walks across the burst.
        assert_eq!(burst_spread(&mut rng, false, 20.0, 0.0, 3, 1), -10.0);
    }

    #[test]
    fn pellets_spread_by_variance() {
        let mut rng = CombatRng(3);
        let mut buckshot = CasingData {
            pellets: 6,
            variance: 15.0,
            randomspread: true,
            ..default()
        };
        let spreads = pellet_spreads(&mut rng, &buckshot, 5.0);
        assert_eq!(spreads.len(), 6);
        assert!(
            spreads
                .iter()
                .all(|s| (-8.0..=7.0).contains(s) && s.fract() == 0.0)
        );
        // Smart spread fans the pellets evenly: floor((i / n - 0.5) * variance).
        buckshot.randomspread = false;
        let fan = pellet_spreads(&mut rng, &buckshot, 5.0);
        assert_eq!((fan[0], fan[1], fan[2], fan[3]), (-5.0, -3.0, 0.0, 2.0));
        assert!(fan.windows(2).all(|w| w[0] < w[1]) && fan[5] <= 7.5);
        // One pellet without variance keeps the gun's spread; with it, smart spread is none.
        let bullet = CasingData {
            pellets: 1,
            ..default()
        };
        assert_eq!(pellet_spreads(&mut rng, &bullet, 5.0), vec![5.0]);
        let slug = CasingData {
            pellets: 1,
            variance: 10.0,
            ..default()
        };
        assert_eq!(pellet_spreads(&mut rng, &slug, 5.0), vec![0.0]);
    }

    #[test]
    fn revolvers_keep_spent_casings_and_load_by_caliber() {
        let dm = DmGun {
            ballistic: true,
            internal_magazine: true,
            bolt_type: BOLT_TYPE_NO_BOLT,
            ..default()
        };
        let data = MagazineData {
            caliber: ".38".into(),
            max_ammo: 2,
            ammo: casing(".38"),
            ..default()
        };
        let mut gun = Gun {
            magazine: Some(Magazine::new(&data)),
            ..default()
        };
        for round in &mut gun.magazine.as_mut().unwrap().rounds {
            round.live = false;
        }
        assert!(ready_round(&dm, &gun).is_none());
        let mut cylinder = Magazine {
            data: data.clone(),
            rounds: Vec::new(),
        };
        let mut spent = Vec::new();
        let mut box_ = Magazine::new(&MagazineData {
            caliber: ".38".into(),
            max_ammo: 5,
            ammo: casing(".38"),
            ..default()
        });
        box_.rounds.push(Round {
            casing: casing("9mm"),
            live: true,
        });
        assert_eq!(cylinder.load_from(&mut box_, &mut spent), 2);
        assert!(spent.is_empty());
        assert_eq!(box_.rounds.len(), 4);
        gun.magazine = Some(cylinder);
        assert!(ready_round(&dm, &gun).is_some());
    }

    #[test]
    fn russian_counts() {
        assert_eq!((patron(1), plural(1)), ("патрон", ""));
        assert_eq!((patron(3), plural(3)), ("патрона", "ы"));
        assert_eq!((patron(12), plural(12)), ("патронов", "о"));
    }
}
