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
//! Messages are BandaStation's translation.
// shortcut: no low power mode (syringes embedding instead: there is no embedding yet), no
// overlays for the loaded syringes and no examine lines; the dart flies under its own type's
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
/// `/obj/item/gun/syringe/blowgun/process_fire`.
const BLOWGUN_STAMINA: f32 = 20.0;
const BLOWGUN_OXY: f32 = 20.0;

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
        c.held
            .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
    });
    app.add_observer(add_state)
        .add_systems(Update, synthesise_darts);
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
    if is_under(&dm.path, CHEM_GUN) {
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
    let ready = if world.get::<ChemGun>(gun).is_some() {
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
        });
    }
    let gun_path = is_syringe_gun(world, gun)?;
    let first = *loaded(world, gun).first()?;
    let dart = if is_under(&gun_path, DNA_GUN) {
        Dart {
            reagents: None,
            dna: world.get::<DnaInjector>(first).cloned(),
            piercing: false,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rapid_syringe_gun_holds_six() {
        assert_eq!(max_syringes(RAPID_SYRINGE_GUN), 6);
        assert_eq!(max_syringes(SYRINGE_GUN), 1);
        assert_eq!(max_syringes(BLOWGUN), 1);
        assert_eq!(max_syringes(DNA_GUN), 1);
    }
}
