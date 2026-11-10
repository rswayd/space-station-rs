//! Special guns (server): tg `code/modules/projectiles/guns/special` on top of [`crate::guns`].
//!
//! - **Syringe guns** (`syringe_gun.dm`, `ammunition/special/syringe.dm`,
//!   `projectile/bullets/dart_syringe.dm`, `dnainjector.dm`): a syringe clicked onto the gun in
//!   the other hand goes in (a bluespace one does not fit) up to `max_syringes` (the rapid
//!   syringe gun 6, the others 1); Z takes the last one back out. The gun's spring (its internal
//!   casing, [`ss13_mapload::DmGun::shots`]) is chambered while a syringe is in; a shot carries
//!   the first syringe's reagents in a dart, the syringe is used up. A dart that hits a carbon
//!   injects everything unless thick clothing covers the hit zone (a piercing syringe goes
//!   through it). The DNA gun takes unused DNA injectors and fires them the same way. The dart
//!   pistol starts with an empty syringe; the blowgun costs its shooter 20 stamina and 20 oxygen
//!   per shot.
//! - **Reagent gun** (`chem_gun.dm`): a 90 u open container that synthesises a piercing dart
//!   of 15 u from it per shot; it holds 4 darts and makes a new one every 25 s.
//!
//! - **Meat hook** (`meat_hook.dm`): one charge back every 8 s; the hook reels what it hits
//!   in toward the firer's tile (`reel_in`).
//! - **Grenade launcher** (`grenade_launcher.dm`) and **medical beamgun** (`medbeam.dm`):
//!   see `process_fire` and `process_medbeams`.
//!
//! Messages are BandaStation's translation.
// shortcut: no low power mode (syringes embedding instead: there is no embedding yet), no
// overlays for the loaded syringes and no examine lines; an empty meat hook clicks instead of
// "not ready yet!" and ignores `TRAIT_NOGUNS` like any gun; the dart flies under its own type's
// name, not the syringe's.

use bevy::prelude::*;
use ss13_mapload::DmGun;
use ss13_mapload::fidelity::is_under;
use ss13_reagents::{Holder, Owner, flags};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};

use crate::chem::{Chem, MobReagents, Reagents};
use crate::genetics::console::DnaInjector;
use crate::guns::{Gun, Round};
use crate::health::DEFAULT_MESSAGE_RANGE;
use crate::inventory::{CarriedBy, Inventory};
use crate::projectile::Projectile;
use crate::security::name_of;
use crate::sound::PlaySound;
use crate::storage::Stored;
use crate::tools::tell;

pub const SYRINGE_GUN: &str = "/obj/item/gun/syringe";
pub const RAPID_SYRINGE_GUN: &str = "/obj/item/gun/syringe/rapidsyringe";
pub const DART_PISTOL: &str = "/obj/item/gun/syringe/syndicate";
pub const DNA_GUN: &str = "/obj/item/gun/syringe/dna";
pub const BLOWGUN: &str = "/obj/item/gun/syringe/blowgun";
pub const CHEM_GUN: &str = "/obj/item/gun/chem";
/// What the dart pistol starts with (`syringes = list(new /obj/item/reagent_containers/syringe())`).
pub const SYRINGE: &str = "/obj/item/reagent_containers/syringe";
const BLUESPACE_SYRINGE: &str = "/obj/item/reagent_containers/syringe/bluespace";
const PIERCING_SYRINGE: &str = "/obj/item/reagent_containers/syringe/piercing";
const DNA_INJECTOR: &str = "/obj/item/dnainjector";
/// `/obj/item/gun/syringe` `load_sound`.
const LOAD_SOUND: &str = "sound/items/weapons/gun/shotgun/insert_shell.ogg";
/// `/obj/item/reagent_containers/syringe/piercing` `armour_penetration` (a plain syringe: 0).
const PIERCING_ARMOUR_PENETRATION: f32 = 40.0;
/// `/obj/projectile/bullet/dart` `create_reagents(50)`.
const DART_VOLUME: f32 = 50.0;
/// `/obj/item/gun/chem`: `create_reagents(90, OPENCONTAINER)`, `max_syringes`,
/// `time_per_syringe` (25 s), and the 15 u each dart takes (`ammo_casing/chemgun/ready_proj`).
pub const CHEM_GUN_VOLUME: f32 = 90.0;
pub const CHEM_GUN_DARTS: u32 = 4;
pub const CHEM_GUN_SYNTH_SECS: f64 = 25.0;
pub const CHEM_DART_UNITS: f32 = 15.0;
pub const GRENADE_LAUNCHER: &str = "/obj/item/gun/grenadelauncher";
const C4: &str = "/obj/item/grenade/c4";
/// `max_grenades`; the fuse of a launched grenade (`addtimer(detonate, 1.5 SECONDS)`).
pub const MAX_GRENADES: usize = 3;
const GRENADE_LAUNCHER_FUSE: f64 = 1.5;
pub const MEDBEAM: &str = "/obj/item/gun/medbeam";
pub const MEAT_HOOK: &str = "/obj/item/gun/magic/hook";
/// The meat hook's `max_charges`, `/obj/item/gun/magic` `recharge_rate` (seconds),
/// `IMMOBILIZATION_TIMER`, the victim's `knockdown_time` and the `SSfastprocess` step period.
pub const HOOK_MAX_CHARGES: u32 = 1;
pub const HOOK_RECHARGE_SECS: f64 = 8.0;
const HOOK_IMMOBILIZE_SECS: f32 = 0.25;
const HOOK_KNOCKDOWN_SECS: f32 = 0.5;
pub const HOOK_STEP_SECS: f64 = 0.2;
/// `/obj/item/gun/medbeam` `max_range` (the beam's `maxdistance`), and its `process` period
/// (`SSobj`'s 2 s `wait`: `check_delay` is shorter).
pub const MEDBEAM_RANGE: u32 = 8;
pub const MEDBEAM_TICK_SECS: f64 = 2.0;
/// `COLOR_HEALING_CYAN`.
const HEALING_CYAN: [u8; 4] = [0x80, 0xF5, 0xFF, 0xFF];
/// `/obj/item/gun/syringe/blowgun/process_fire`.
const BLOWGUN_STAMINA: f32 = 20.0;
const BLOWGUN_OXY: f32 = 20.0;

/// A medical beamgun's beam (tg `current_target`, the beam's `origin`, `last_check`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Medbeam {
    pub target: Option<Entity>,
    /// The mob that fired it: the beam leaves with the gun's holder change (`dropped`,
    /// `equipped`). For an exosuit's beamgun, the mech.
    pub origin: Option<Entity>,
    pub last_check: f64,
    /// `/obj/item/gun/medbeam/mech` (`mounted`): the gun is mech equipment, `origin` its mech.
    pub mounted: bool,
}

/// A reagent gun's darts (tg `syringes_left`, `last_synth`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ChemGun {
    pub left: u32,
    pub last_synth: f64,
}

/// What a fired dart carries (tg the dart's `reagents`, `inject_flags`, the DNA projectile's
/// `injector`).
#[derive(Component, Clone, Debug)]
pub struct Dart {
    pub reagents: Option<Holder>,
    pub dna: Option<DnaInjector>,
    /// `INJECT_CHECK_PENETRATE_THICK`.
    pub piercing: bool,
    /// `/obj/projectile/hook`: it reels in what it hits.
    pub hook: bool,
}

/// A meat hook's charge (tg `/obj/item/gun/magic` `charges`, `charge_timer`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct HookGun {
    pub charges: u32,
    pub charge_timer: f64,
}

/// Something a hook reels in (tg `/datum/hook_and_move`, `TRAIT_HOOKED`): one step toward
/// `dest` (the firer's tile at the hit) every `SSfastprocess` tick.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Hooked {
    pub firer: Entity,
    pub dest: TilePos,
    pub next_step: f64,
}

pub(crate) fn plugin(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // Z on a syringe gun: the last syringe back into the hands (before `guns::attack_self`
    // racks it).
    app.add_click_handler(ClickPhase::AttackSelf, 993, |w, c| {
        attack_self(w, c.client, c.mob, c.target)
    });
    // A syringe or DNA injector onto the syringe gun in the other hand (before
    // `chem::item_interaction` takes the syringe).
    app.add_click_handler(ClickPhase::OtherHand, 1393, |w, c| {
        c.held.is_some_and(|h| {
            item_interaction(w, c.client, c.mob, h, c.target)
                || (is_launcher(w, c.target) && load_grenade(w, c.client, h, c.target))
        })
    });
    app.add_observer(add_state).add_systems(
        Update,
        (synthesise_darts, process_medbeams, recharge_hooks, reel_in),
    );
}

fn is_syringe_gun(world: &World, gun: Entity) -> Option<String> {
    world
        .get::<DmGun>(gun)
        .map(|g| g.path.clone())
        .filter(|p| is_under(p, SYRINGE_GUN))
}

fn path_of(world: &World, e: Entity) -> String {
    crate::construction::path_of(world, e).unwrap_or_default()
}

/// `max_syringes`.
pub fn max_syringes(gun_path: &str) -> usize {
    if is_under(gun_path, RAPID_SYRINGE_GUN) {
        6
    } else {
        1
    }
}

/// tg `syringes`: what is loaded, first in first.
pub fn loaded(world: &World, gun: Entity) -> Vec<Entity> {
    let Some(gun_path) = is_syringe_gun(world, gun) else {
        return Vec::new();
    };
    let dna = is_under(&gun_path, DNA_GUN);
    world
        .get::<Stored>(gun)
        .map(|s| {
            s.items()
                .iter()
                .copied()
                .filter(|&e| {
                    let path = path_of(world, e);
                    if dna {
                        is_under(&path, DNA_INJECTOR)
                    } else {
                        is_under(&path, SYRINGE)
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// tg `Initialize`: the reagent gun's reservoir and darts; the dart pistol's syringe.
fn add_state(add: On<Add<DmGun>>, guns: Query<&DmGun>, mut commands: Commands) {
    let Ok(dm) = guns.get(add.entity) else {
        return;
    };
    let gun = add.entity;
    if is_under(&dm.path, MEAT_HOOK) {
        commands.entity(gun).insert_if_new(HookGun {
            charges: HOOK_MAX_CHARGES,
            charge_timer: 0.0,
        });
    } else if is_under(&dm.path, MEDBEAM) {
        commands.entity(gun).insert_if_new(Medbeam::default());
    } else if is_under(&dm.path, CHEM_GUN) {
        let owner = Owner {
            path: dm.path.clone(),
            living: false,
        };
        commands.entity(gun).insert_if_new((
            ChemGun {
                left: CHEM_GUN_DARTS,
                last_synth: 0.0,
            },
            Reagents(Holder::new(CHEM_GUN_VOLUME, flags::OPENCONTAINER, owner)),
        ));
    } else if is_under(&dm.path, DART_PISTOL) {
        commands.queue(move |world: &mut World| {
            let Some(at) = crate::modpc::pda::turf_of(world, gun) else {
                return;
            };
            if let Some(syringe) = crate::construction::spawn_obj(world, SYRINGE, at) {
                crate::storage::put_inside(world, syringe, gun);
            }
        });
    }
}

/// tg `recharge_newshot` / `can_shoot`: the internal casing is chambered while there is a
/// syringe, an injector or a synthesised dart to fire.
pub(crate) fn before_fire(world: &mut World, gun: Entity) {
    let ready = if let Some(hook) = world.get::<HookGun>(gun) {
        hook.charges > 0
    } else if world.get::<ChemGun>(gun).is_some() {
        world.get::<ChemGun>(gun).is_some_and(|c| c.left > 0)
    } else if is_syringe_gun(world, gun).is_some() {
        !loaded(world, gun).is_empty()
    } else {
        return;
    };
    let Some(casing) = world
        .get::<DmGun>(gun)
        .and_then(|d| d.shots.first().cloned())
    else {
        return;
    };
    if let Some(mut state) = world.get_mut::<Gun>(gun) {
        state.chambered = ready.then_some(Round { casing, live: true });
    }
}

/// tg `ammo_casing/syringegun/ready_proj`, `chemgun/ready_proj`, `dnainjector/ready_proj` and
/// the blowgun's `process_fire`: the dart of this shot, its syringe or injector used up.
pub(crate) fn ready_proj(
    world: &mut World,
    gun: Entity,
    user: Entity,
    data: &mut ss13_mapload::ProjectileData,
) -> Option<Dart> {
    if let Some(hook) = world.get::<HookGun>(gun).copied() {
        if hook.charges == 0 {
            return None;
        }
        if let Some(mut h) = world.get_mut::<HookGun>(gun) {
            h.charges -= 1;
        }
        // `hook/fire`: the firer stands still while the chain flies (`IMMOBILIZATION_TIMER`).
        crate::status_effects::apply(
            world,
            user,
            crate::status_effects::Incap::Immobilize,
            HOOK_IMMOBILIZE_SECS,
        );
        return Some(Dart {
            reagents: None,
            dna: None,
            piercing: false,
            hook: true,
        });
    }
    if let Some(chem_gun) = world.get::<ChemGun>(gun).copied() {
        if chem_gun.left == 0 {
            return None;
        }
        let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
        let mut dart = Holder::new(DART_VOLUME, flags::NO_REACT, Owner::default());
        if let (Some(chem), Some(mut reservoir)) = (chem, world.get_mut::<Reagents>(gun)) {
            reservoir
                .0
                .trans_to(&chem, &mut dart, CHEM_DART_UNITS, 1.0, None, false, false);
        }
        if let Some(mut c) = world.get_mut::<ChemGun>(gun) {
            c.left -= 1;
        }
        return Some(Dart {
            reagents: Some(dart),
            dna: None,
            piercing: true,
            hook: false,
        });
    }
    let gun_path = is_syringe_gun(world, gun)?;
    let first = *loaded(world, gun).first()?;
    let dart = if is_under(&gun_path, DNA_GUN) {
        Dart {
            reagents: None,
            dna: world.get::<DnaInjector>(first).cloned(),
            piercing: false,
            hook: false,
        }
    } else {
        let piercing = is_under(&path_of(world, first), PIERCING_SYRINGE);
        data.armour_penetration = if piercing {
            PIERCING_ARMOUR_PENETRATION
        } else {
            0.0
        };
        let holder = world
            .get_mut::<Reagents>(first)
            .map(|mut r| std::mem::replace(&mut r.0, Holder::new(0.0, 0, Owner::default())));
        Dart {
            reagents: holder,
            dna: None,
            piercing,
            hook: false,
        }
    };
    world.despawn(first);
    if is_under(&gun_path, BLOWGUN) {
        blowgun_exertion(world, gun, user);
    }
    Some(dart)
}

/// `/obj/item/gun/syringe/blowgun/process_fire`.
fn blowgun_exertion(world: &mut World, gun: Entity, user: Entity) {
    if let Some(&at) = world.get::<TilePos>(user) {
        let text = format!(
            "{} стреляет из {}!",
            name_of(world, user),
            crate::obj_damage::strip(&name_of(world, gun))
        );
        crate::security::show(
            world,
            at,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (Span::Danger, text),
            &[],
        );
    }
    crate::status_effects::adjust_stamina(world, user, BLOWGUN_STAMINA);
    crate::medical::diseases::damage(world, user, crate::health::DamageType::Oxy, BLOWGUN_OXY);
}

/// `/obj/projectile/bullet/dart/on_hit` and `dnainjector/on_hit` on a carbon: `can_inject`
/// at the hit zone, then everything in the dart goes into the blood (or the injector's
/// mutations into the DNA).
pub(crate) fn dart_hit(world: &mut World, e: Entity, p: &Projectile, target: Entity) {
    let Some(dart) = world.get::<Dart>(e).cloned() else {
        return;
    };
    if dart.hook {
        hook_hit(world, p.firer, target);
        return;
    }
    if world.get::<MobReagents>(target).is_none() {
        return;
    }
    if !dart.piercing && crate::chem::thick_covered(world, target, p.zone) {
        if let Some(&at) = world.get::<TilePos>(target) {
            let name = crate::obj_damage::strip(&name_of(world, e)).to_owned();
            crate::security::show(
                world,
                at,
                DEFAULT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (Span::Danger, format!("{name} отражается!")),
                &[(target, Span::BoldWarning, format!("Вы защищены от {name}!"))],
            );
        }
        return;
    }
    if let Some(holder) = dart.reagents {
        let total = holder.total_volume;
        world.entity_mut(e).insert(Reagents(holder));
        crate::chem::transfer_to_mob(world, e, target, total, false);
    }
    if let Some(injector) = dart.dna {
        world.entity_mut(e).insert(injector);
        crate::genetics::console::inject(world, e, target);
    }
}

/// tg `attack_self`: the last syringe into the hands.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, gun: Entity) -> bool {
    if is_syringe_gun(world, gun).is_none() {
        return false;
    }
    let Some(&last) = loaded(world, gun).last() else {
        tell(world, client, Span::Warning, "пусто!");
        return true;
    };
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return true;
    };
    crate::storage::put_in_hands(world, mob, &inventory, last);
    let name = name_of(world, last);
    tell(world, client, Span::Notice, format!("снятие {name}"));
    true
}

/// tg `item_interaction` / `attempt_insert_syringe` (the DNA gun: injectors only).
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    _mob: Entity,
    held: Entity,
    gun: Entity,
) -> bool {
    let Some(gun_path) = is_syringe_gun(world, gun) else {
        return false;
    };
    let path = path_of(world, held);
    let name = name_of(world, held);
    if is_under(&gun_path, DNA_GUN) {
        if !is_under(&path, DNA_INJECTOR) {
            return false;
        }
        if world.get::<DnaInjector>(held).is_some_and(|i| i.used) {
            tell(
                world,
                client,
                Span::Warning,
                format!("внутри {name} пусто!"),
            );
            return true;
        }
    } else {
        if !is_under(&path, SYRINGE) {
            return false;
        }
        if is_under(&path, BLUESPACE_SYRINGE) {
            tell(world, client, Span::Warning, format!("{name} не влезает!"));
            return true;
        }
    }
    if loaded(world, gun).len() >= max_syringes(&gun_path) {
        tell(world, client, Span::Warning, "нет места!");
        return true;
    }
    if crate::inventory::no_drop(world, held) {
        return true;
    }
    crate::storage::put_inside(world, held, gun);
    tell(world, client, Span::Notice, format!("вставка {name}"));
    if let Some(at) = crate::modpc::pda::turf_of(world, gun) {
        crate::sound::playsound(world, PlaySound::new(at, LOAD_SOUND, 40.0, false));
    }
    true
}

/// tg `/obj/item/gun/grenadelauncher/item_interaction`: a grenade (not C4) in, up to
/// `max_grenades`.
fn load_grenade(world: &mut World, client: Entity, held: Entity, launcher: Entity) -> bool {
    let path = path_of(world, held);
    if is_under(&path, C4) || crate::grenades::grenade(world, held).is_none() {
        return false;
    }
    let count = grenades(world, launcher).len();
    if count >= MAX_GRENADES {
        tell(world, client, Span::Warning, "уже заполнен!");
        return true;
    }
    if crate::inventory::no_drop(world, held) {
        return true;
    }
    crate::storage::put_inside(world, held, launcher);
    tell(
        world,
        client,
        Span::Notice,
        format!("{} / {MAX_GRENADES} гранат заряжено", count + 1),
    );
    true
}

/// tg `grenades`: the loaded ones, first in first.
pub fn grenades(world: &World, launcher: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(launcher)
        .map(|s| {
            s.items()
                .iter()
                .copied()
                .filter(|&e| world.get::<crate::grenades::Grenade>(e).is_some())
                .collect()
        })
        .unwrap_or_default()
}

fn is_launcher(world: &World, gun: Entity) -> bool {
    world
        .get::<DmGun>(gun)
        .is_some_and(|g| is_under(&g.path, GRENADE_LAUNCHER))
}

/// tg `/obj/item/gun/grenadelauncher/process_fire` (with `can_shoot`): the first grenade is
/// thrown from the shooter's tile at the target (30 tiles, speed 2) and goes off 1.5 s later.
/// False for anything else or an empty launcher (it clicks).
pub fn process_fire(
    world: &mut World,
    _client: Entity,
    mob: Entity,
    gun: Entity,
    aim: crate::guns::Aim,
) -> bool {
    if world.get::<Medbeam>(gun).is_some() {
        medbeam_fire(world, mob, gun, aim.target);
        return true;
    }
    if !is_launcher(world, gun) {
        return false;
    }
    let Some(&grenade) = grenades(world, gun).first() else {
        return false;
    };
    let Some(&from) = world.get::<TilePos>(mob) else {
        return true;
    };
    let (user, name) = (name_of(world, mob), name_of(world, grenade));
    crate::security::show(
        world,
        from,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, format!("{user} стреляет гранатой!")),
        &[(mob, Span::Danger, "Вы стреляете из гранатомёта!".to_owned())],
    );
    info!(
        "{mob:?} fired a grenade ({name}) from a grenade launcher from {from:?} at {:?}",
        aim.target_pos
    );
    crate::storage::take_out(world, grenade, from);
    crate::throw::throw_at(world, grenade, aim.target_pos, 30, 2.0);
    crate::grenades::arm(world, grenade, None, Some(GRENADE_LAUNCHER_FUSE));
    true
}

/// tg `/obj/item/gun/chem/process`: a dart every `time_per_syringe` while not full.
fn synthesise_darts(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &ChemGun)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.left < CHEM_GUN_DARTS && now >= c.last_synth + CHEM_GUN_SYNTH_SECS)
        .map(|(e, _)| e)
        .collect();
    for gun in due {
        if let Some(mut c) = world.get_mut::<ChemGun>(gun) {
            c.left += 1;
            c.last_synth = now;
        }
        if let Some(&CarriedBy(holder)) = world.get::<CarriedBy>(gun) {
            let name = crate::obj_damage::strip(&name_of(world, gun)).to_owned();
            crate::chem::tell_mob(
                world,
                holder,
                Span::Warning,
                &format!("Вы слышите щелчок: {name} синтезирует новый дротик."),
            );
        }
    }
}

/// tg `/obj/item/gun/medbeam/process_fire`: the old beam lets go; a living target is beamed.
fn medbeam_fire(world: &mut World, mob: Entity, gun: Entity, target: Entity) {
    let living = world.get::<crate::health::Health>(target).is_some() && target != mob;
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut beam) = world.get_mut::<Medbeam>(gun) {
        *beam = if living {
            Medbeam {
                target: Some(target),
                origin: Some(mob),
                last_check: now,
                mounted: false,
            }
        } else {
            Medbeam::default()
        };
    }
}

/// `/obj/item/mecha_parts/mecha_equipment/medical/mechmedbeam/action`: the mounted gun's
/// `process_fire(target, chassis)`; the old beam lets go, a living target other than the mech is
/// beamed from the mech.
pub fn mounted_medbeam_fire(world: &mut World, mech: Entity, equip: Entity, target: Entity) {
    let living = world.get::<crate::health::Health>(target).is_some() && target != mech;
    let now = world.resource::<Time>().elapsed_secs_f64();
    let beam = if living {
        Medbeam {
            target: Some(target),
            origin: Some(mech),
            last_check: now,
            mounted: true,
        }
    } else {
        Medbeam {
            mounted: true,
            ..Medbeam::default()
        }
    };
    world.entity_mut(equip).insert(beam);
}

/// The tiles a beam from `from` to `to` lies on (tg `Beam` segments on `get_line`).
pub fn beam_tiles(from: TilePos, to: TilePos) -> Vec<TilePos> {
    crate::radiation::get_line(from, to)
}

/// tg `los_check` with the dummy's `PASSTABLE|PASSGLASS|PASSGRILLE`: nothing dense in the way
/// (mobs do not stop the beam).
fn beam_clear(world: &World, line: &[TilePos]) -> bool {
    let index = world.resource::<ss13_world::SpatialIndex>();
    line.iter().skip(1).all(|&tile| {
        index.on_tile(tile).iter().all(|&e| {
            world.get::<ss13_world::Density>(e).is_none()
                || world.get::<crate::health::Health>(e).is_some()
                || crate::projectile::passes(
                    world,
                    e,
                    crate::projectile::PASSTABLE
                        | crate::projectile::PASSGLASS
                        | crate::projectile::PASSGRILLE,
                )
        })
    })
}

/// tg `mid_los_check`: the first tile of `line` (past its start) another origin's beam lies on.
pub fn beams_cross(line: &[TilePos], others: &[Vec<TilePos>]) -> Option<TilePos> {
    line.iter()
        .skip(1)
        .copied()
        .find(|t| others.iter().any(|o| o.contains(t)))
}

/// tg `/obj/item/gun/medbeam/process`, `beam_died` and `on_beam_tick`: every `SSobj` tick the
/// beam stays while its gun is held by who fired it, within range and in sight; crossed
/// beams explode (`explosion(3 heavy, 5 light, 8 flash)`); the target heals 4 brute, 4 burn,
/// 1 toxin and 1 oxygen.
// Cost: per tick, one pass over beamguns (a handful); the line work runs every 2 s per beam.
// shortcut: the beam has no sprite (`Beam` visuals are not ported); dragging the holder away
// past the range ends it at the next check instead of at once.
fn process_medbeams(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &Medbeam)>();
    let active: Vec<(Entity, Medbeam)> = q
        .iter(world)
        .filter(|(_, b)| b.target.is_some())
        .map(|(e, b)| (e, *b))
        .collect();
    let line_of = |world: &World, b: &Medbeam| -> Option<Vec<TilePos>> {
        let from = *world.get::<TilePos>(b.origin?)?;
        let to = *world.get::<TilePos>(b.target?)?;
        Some(beam_tiles(from, to))
    };
    for (gun, beam) in active {
        let (Some(origin), Some(target)) = (beam.origin, beam.target) else {
            continue;
        };
        if world
            .get::<Medbeam>(gun)
            .is_none_or(|b| b.target != Some(target))
        {
            continue; // released by a crossing this tick
        }
        // `dropped` / `equipped` / `!isliving(loc)`: the beam ends silently. A mounted gun
        // stays while attached to its mech (`detach` -> `LoseTarget`), lets go when the safety
        // is on, and does not tick without a pilot (`mechmedbeam/process`).
        let held = if beam.mounted {
            world
                .get::<crate::mecha::Mech>(origin)
                .is_some_and(|m| m.equipment().any(|e| e == gun))
        } else {
            world.get::<CarriedBy>(gun).is_some_and(|c| c.0 == origin)
                && world
                    .get::<Inventory>(origin)
                    .is_some_and(|i| i.hands.contains(&Some(gun)))
        };
        let safe = beam.mounted
            && world
                .get::<crate::mecha::Mech>(origin)
                .is_some_and(|m| m.weapons_safety);
        if !held || safe || world.get_entity(target).is_err() {
            world.entity_mut(gun).insert(Medbeam {
                mounted: beam.mounted,
                ..Medbeam::default()
            });
            continue;
        }
        if beam.mounted
            && world
                .get::<crate::mecha::Mech>(origin)
                .is_some_and(|m| m.occupant.is_none())
        {
            continue;
        }
        if now < beam.last_check + MEDBEAM_TICK_SECS {
            continue;
        }
        if let Some(mut b) = world.get_mut::<Medbeam>(gun) {
            b.last_check = now;
        }
        let Some(line) = line_of(world, &beam) else {
            lose_beam(world, gun, origin);
            continue;
        };
        let (from, to) = (line[0], *line.last().expect("get_line has its start"));
        if from.z != to.z || from.chebyshev(to).is_none_or(|d| d > MEDBEAM_RANGE) {
            lose_beam(world, gun, origin);
            continue;
        }
        let mut q = world.query::<(Entity, &Medbeam)>();
        let others: Vec<(Entity, Medbeam)> = q
            .iter(world)
            .filter(|(e, b)| *e != gun && b.target.is_some() && b.origin != Some(origin))
            .map(|(e, b)| (e, *b))
            .collect();
        let other_lines: Vec<Vec<TilePos>> = others
            .iter()
            .map(|(_, b)| line_of(world, b).unwrap_or_default())
            .collect();
        if let Some(at) = beams_cross(&line, &other_lines) {
            info!("{origin:?} crossed medical beams at {at:?}");
            let mut ex = crate::explosions::Explosion::new(0, 3, 5);
            ex.flash = Some(8.0);
            ex.cause = "medbeam".to_owned();
            crate::explosions::explosion(world, at, ex);
            lose_beam(world, gun, origin);
            for ((other, b), l) in others.iter().zip(&other_lines) {
                if l.contains(&at)
                    && let Some(o) = b.origin
                {
                    lose_beam(world, *other, o);
                }
            }
            continue;
        }
        if !beam_clear(world, &line) {
            lose_beam(world, gun, origin);
            continue;
        }
        heal_tick(world, target, to);
    }
}

/// tg `beam_died`: the warning, then `LoseTarget`. A mech's pilot gets the warning.
fn lose_beam(world: &mut World, gun: Entity, holder: Entity) {
    let mounted = world.get::<Medbeam>(gun).is_some_and(|b| b.mounted);
    world.entity_mut(gun).insert(Medbeam {
        mounted,
        ..Medbeam::default()
    });
    let holder = world
        .get::<crate::mecha::Mech>(holder)
        .and_then(|m| m.occupant)
        .unwrap_or(holder);
    crate::chem::tell_mob(
        world,
        holder,
        Span::Warning,
        "Вы теряете контроль над лучом!",
    );
}

/// tg `on_beam_tick`.
fn heal_tick(world: &mut World, target: Entity, at: TilePos) {
    use crate::health::DamageType;
    let hurt = world
        .get::<crate::health::Health>(target)
        .is_some_and(|h| h.brute + h.burn + h.tox + h.oxy > 0.0);
    if hurt {
        let mut v =
            crate::effects::temp_visual::Visual::new("icons/effects/effects.dmi", "heal", 1.5);
        v.color = Some(HEALING_CYAN);
        crate::effects::temp_visual::spawn(world, at, v);
    }
    for (kind, amount) in [
        (DamageType::Brute, 4.0),
        (DamageType::Burn, 4.0),
        (DamageType::Tox, 1.0),
        (DamageType::Oxy, 1.0),
    ] {
        crate::medical::diseases::damage(world, target, kind, -amount);
    }
}

/// `/obj/item/gun/magic/process`: a spent hook charges back after `recharge_rate`.
fn recharge_hooks(time: Res<Time>, mut hooks: Query<&mut HookGun>) {
    let dt = time.delta_secs_f64();
    for mut hook in &mut hooks {
        if hook.charges >= HOOK_MAX_CHARGES {
            continue;
        }
        hook.charge_timer += dt;
        if hook.charge_timer >= HOOK_RECHARGE_SECS {
            hook.charge_timer = 0.0;
            hook.charges += 1;
        }
    }
}

/// `/obj/projectile/hook/on_hit` -> `hook_and_move/begin_pulling`: an unanchored movable is
/// snagged and reeled in toward the firer's tile; a living one falls for 0.5 s.
pub fn hook_hit(world: &mut World, firer: Entity, target: Entity) {
    if !crate::pull::can_be_pulled(world, firer, target)
        || world
            .get::<Hooked>(target)
            .is_some_and(|h| h.firer == firer)
    {
        return;
    }
    let (Some(&dest), Some(&at)) = (world.get::<TilePos>(firer), world.get::<TilePos>(target))
    else {
        return;
    };
    let text = format!(
        "{} подцеплен крюком {}!",
        name_of(world, target),
        name_of(world, firer)
    );
    crate::security::show(
        world,
        at,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, text),
        &[],
    );
    if world.get::<crate::health::Health>(target).is_some() {
        crate::status_effects::knockdown(world, target, HOOK_KNOCKDOWN_SECS, 0.0);
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(target).insert(Hooked {
        firer,
        dest,
        next_step: now,
    });
}

/// `hook_and_move/attempt_movement`: `step_towards`, else a step along x, else along y.
fn hook_step(world: &mut World, victim: Entity, from: TilePos, dest: TilePos) -> bool {
    use ss13_shared::Dir;
    if let Some(dir) = crate::pull::dir_between(from, dest)
        && crate::pull::move_to_adjacent(world, victim, from.step(dir))
    {
        return true;
    }
    let x = match dest.x.cmp(&from.x) {
        std::cmp::Ordering::Greater => Some(Dir::East),
        std::cmp::Ordering::Less => Some(Dir::West),
        std::cmp::Ordering::Equal => None,
    };
    let y = match dest.y.cmp(&from.y) {
        std::cmp::Ordering::Greater => Some(Dir::North),
        std::cmp::Ordering::Less => Some(Dir::South),
        std::cmp::Ordering::Equal => None,
    };
    [x, y]
        .into_iter()
        .flatten()
        .any(|d| crate::pull::step_once(world, victim, d, None).is_ok())
}

/// `hook_and_move/process` on `SSfastprocess`: a step every 0.2 s while the firer stays
/// immobilized; it ends when a step fails or the victim is on the destination.
// Cost: per tick, one pass over hooked things (rarely more than one).
// shortcut: no chain beam sprite.
fn reel_in(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &Hooked, &TilePos)>();
    let due: Vec<(Entity, Hooked, TilePos)> = q
        .iter(world)
        .filter(|(_, h, _)| now >= h.next_step)
        .map(|(e, h, p)| (e, *h, *p))
        .collect();
    for (victim, hook, at) in due {
        let firer_gone = world.get_entity(hook.firer).is_err();
        let moved = !firer_gone && at != hook.dest && hook_step(world, victim, at, hook.dest);
        let arrived = world.get::<TilePos>(victim) == Some(&hook.dest);
        if !moved || arrived {
            world.entity_mut(victim).remove::<Hooked>();
            continue;
        }
        crate::status_effects::apply(
            world,
            hook.firer,
            crate::status_effects::Incap::Immobilize,
            HOOK_STEP_SECS as f32 + 0.05,
        );
        if let Some(mut h) = world.get_mut::<Hooked>(victim) {
            h.next_step = now + HOOK_STEP_SECS;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beams_cross_where_their_lines_share_a_tile() {
        let t = |x, y| TilePos::new(x, y, 1);
        let mine = beam_tiles(t(3, 6), t(7, 6));
        let across = beam_tiles(t(5, 3), t(5, 9));
        let parallel = beam_tiles(t(3, 8), t(7, 8));
        assert_eq!(beams_cross(&mine, std::slice::from_ref(&parallel)), None);
        assert_eq!(beams_cross(&mine, &[parallel, across]), Some(t(5, 6)));
    }

    #[test]
    fn rapid_syringe_gun_holds_six() {
        assert_eq!(max_syringes(RAPID_SYRINGE_GUN), 6);
        assert_eq!(max_syringes(SYRINGE_GUN), 1);
        assert_eq!(max_syringes(BLOWGUN), 1);
        assert_eq!(max_syringes(DNA_GUN), 1);
    }
}
