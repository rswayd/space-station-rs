//! Tracking and choice beacons (server), after tg `code/game/objects/items/devices/beacon.dm`
//! and `code/game/objects/items/choice_beacon.dm`.
//!
//! - Tracking beacons (`/obj/item/beacon`): on by default (`GLOB.teleportbeacons`, here the
//!   [`TrackingBeacon`] component with `enabled`, read through [`enabled_beacons`]); used in hand
//!   or right-clicked with an empty hand they toggle (`beacon` / `beacon-off`); an EMP flips them
//!   with `75 / severity` %.
//! - Choice beacons (`/obj/item/choice_beacon`): used in hand they list what can be ordered
//!   (`generate_display_names`), check the user again after the choice (`can_use_beacon`), then
//!   say the company's message, deliver it (BandaStation: the teleporter supply pod,
//!   `crate::cargo::supplypod::launch`; the augment beacon drops it at the user's feet) and use
//!   up a charge (the last one: sparks and the beacon is gone). Music (with BandaStation's extra
//!   instruments), hero costume and augment beacons.
// shortcut: no rename (the teleporter console names beacons by their area,
// `crate::science::teleportation::teleporter`). The chaplain's armaments beacon (a radial
// of holy armour sets, `GLOB.holy_armor_type`) is not ported: it needs the chaplain armour sets
// (religion group).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_proto::Prototypes;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use crate::health::Stat;
use crate::inventory::Inventory;
use crate::modpc::pda::turf_of;
use crate::sound::{PlaySound, playsound};

const BEACON: &str = "/obj/item/beacon";
const CHOICE_BEACON: &str = "/obj/item/choice_beacon";
const BUZZ: &str = "sound/machines/buzz/buzz-sigh.ogg";
/// The augment beacon's `playsound(src, 'emitter2.ogg', 50, extrarange = SILENCED_...)`.
const EMITTER: &str = "sound/items/weapons/emitter2.ogg";

/// `/obj/item/beacon`: `enabled` (in `GLOB.teleportbeacons`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackingBeacon {
    pub enabled: bool,
}

/// `/obj/item/choice_beacon`: what it offers and `uses`.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ChoiceBeacon {
    pub kind: ChoiceKind,
    pub uses: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChoiceKind {
    Music,
    Hero,
    Augments,
    /// BandaStation `/obj/item/choice_beacon/weapon/*`: the sidearm of security, the
    /// blueshield and the head of security.
    SecurityPistol,
    Blueshield,
    HosPistol,
}

/// BandaStation `weapon_choice.dm` `selectable_gun_types`: display name, gun case.
const SECURITY_PISTOLS: [(&str, &str); 2] = [
    (
        "Пистолет GP-9 9x25мм",
        "/obj/item/storage/toolbox/guncase/ntcase/pistol/gp9/sec",
    ),
    (
        "Энергетический пистолет дизейблер",
        "/obj/item/gun/energy/disabler",
    ),
];
const BLUESHIELD_PISTOLS: [(&str, &str); 4] = [
    (
        "Энергетический пистолет EG-14",
        "/obj/item/storage/toolbox/guncase/ntspecial/pistol/eg_14",
    ),
    (
        "Термально-нанитовый пистолет TNG-3",
        "/obj/item/storage/toolbox/guncase/ntspecial/pistol/tng_3",
    ),
    (
        "Пистолет GP-38 калибра .38",
        "/obj/item/storage/toolbox/guncase/ntspecial/pistol/gp38",
    ),
    (
        "PDH-A1-M 'Ladon'",
        "/obj/item/storage/toolbox/guncase/ntspecial/pistol/ladon",
    ),
];
const HOS_PISTOLS: [(&str, &str); 4] = [
    (
        "Мультифазовый энергетический пистолет X-01",
        "/obj/item/storage/toolbox/guncase/ntspecial/pistol/hos_laser",
    ),
    (
        "Лазерный пистолет X-02",
        "/obj/item/storage/toolbox/guncase/ntspecial/pistol/hos_laser2",
    ),
    (
        "Пистолет GP-45M калибра .45",
        "/obj/item/storage/toolbox/guncase/ntspecial/pistol/hos_gp45",
    ),
    (
        "PDH-A2-NR 'Osprey'",
        "/obj/item/storage/toolbox/guncase/ntspecial/pistol/hos_pdh",
    ),
];

/// `/obj/item/choice_beacon/music/generate_display_names`, BandaStation's additions included.
const INSTRUMENTS: [&str; 16] = [
    "/obj/item/instrument/violin",
    "/obj/item/instrument/piano_synth",
    "/obj/item/instrument/banjo",
    "/obj/item/instrument/guitar",
    "/obj/item/instrument/eguitar",
    "/obj/item/instrument/glockenspiel",
    "/obj/item/instrument/accordion",
    "/obj/item/instrument/trumpet",
    "/obj/item/instrument/saxophone",
    "/obj/item/instrument/trombone",
    "/obj/item/instrument/recorder",
    "/obj/item/instrument/harmonica",
    "/obj/item/instrument/piano_synth/headphones",
    "/obj/item/instrument/soundhand_bass_guitar",
    "/obj/item/instrument/soundhand_rock_guitar",
    "/obj/item/instrument/drumskit_folded",
];
/// `typesof(/obj/item/storage/box/hero)`.
const HERO_BOXES: [&str; 7] = [
    "/obj/item/storage/box/hero",
    "/obj/item/storage/box/hero/astronaut",
    "/obj/item/storage/box/hero/scottish",
    "/obj/item/storage/box/hero/carphunter",
    "/obj/item/storage/box/hero/mothpioneer",
    "/obj/item/storage/box/hero/etherealwarden",
    "/obj/item/storage/box/hero/journalist",
];
/// `/obj/item/choice_beacon/augments/generate_display_names`.
const AUGMENTS: [&str; 6] = [
    "/obj/item/organ/cyberimp/brain/anti_drop",
    "/obj/item/organ/cyberimp/arm/toolkit/toolset",
    "/obj/item/organ/cyberimp/arm/toolkit/surgery",
    "/obj/item/organ/cyberimp/chest/thrusters",
    "/obj/item/organ/lungs/cybernetic/tier3",
    "/obj/item/organ/liver/cybernetic/tier3",
];

impl ChoiceKind {
    pub fn from_path(path: &str) -> Option<Self> {
        if is_under(path, "/obj/item/choice_beacon/music") {
            Some(Self::Music)
        } else if is_under(path, "/obj/item/choice_beacon/hero") {
            Some(Self::Hero)
        } else if is_under(path, "/obj/item/choice_beacon/augments") {
            Some(Self::Augments)
        } else if is_under(path, "/obj/item/choice_beacon/weapon/security_pistol") {
            Some(Self::SecurityPistol)
        } else if is_under(path, "/obj/item/choice_beacon/weapon/blueshield") {
            Some(Self::Blueshield)
        } else if is_under(path, "/obj/item/choice_beacon/weapon/hos") {
            Some(Self::HosPistol)
        } else {
            None
        }
    }

    pub fn options(self) -> Vec<&'static str> {
        match self {
            Self::Music => INSTRUMENTS.to_vec(),
            Self::Hero => HERO_BOXES.to_vec(),
            Self::Augments => AUGMENTS.to_vec(),
            _ => self.named().iter().map(|(_, p)| *p).collect(),
        }
    }

    /// The options listed under their own names (the weapon beacons' list keys) instead of
    /// their type names.
    fn named(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::SecurityPistol => &SECURITY_PISTOLS,
            Self::Blueshield => &BLUESHIELD_PISTOLS,
            Self::HosPistol => &HOS_PISTOLS,
            _ => &[],
        }
    }

    /// The prompt's name for option `path`.
    fn display_name(self, world: &World, path: &str) -> String {
        self.named().iter().find(|(_, p)| *p == path).map_or_else(
            || crate::machine_frame::type_name(world, path),
            |(name, _)| (*name).to_owned(),
        )
    }

    /// `uses`.
    fn uses(self) -> u32 {
        match self {
            Self::Augments => 3,
            _ => 1,
        }
    }

    /// `company_source`, `company_message`.
    fn company(self) -> (&'static str, &'static str) {
        match self {
            Self::Music => (
                "Sophronia Broadcasting",
                "Наслаждайтесь музыкальным инструментом «Spinward Idol» от Sophronia \
                 Broadcasting — точно как в популярном шоу!",
            ),
            Self::Hero => (
                "Sophronia Broadcasting",
                "Наслаждайтесь костюмом «История оживает» от Sophronia Broadcasting — точно как \
                 в популярном шоу!",
            ),
            Self::Augments => (
                "S.E.L.F.",
                "Статус запроса: получен. Статус посылки: доставлена. Примечание: для \
                 оптимального результата используйте приложенные автохирурги Interdyne.",
            ),
            // The base `company_source` and `company_message`.
            Self::SecurityPistol | Self::Blueshield | Self::HosPistol => (
                "Центрального командования",
                "Запрос получен. Ваша посылка в пути, пожалуйста, отойдите от места посадки.",
            ),
        }
    }
}

/// The beacon a choice prompt is about and the paths behind its items.
#[derive(Component, Clone, Debug)]
struct ChoicePrompt {
    beacon: Entity,
    paths: Vec<String>,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.add_click_handler(ClickPhase::AttackSelf, 1743, |w, c| {
        attack_self(w, c.client, c.mob, c.target)
    });
    // `attack_hand_secondary` of a tracking beacon: `attack_self(user)`.
    app.add_click_handler(ClickPhase::EmptyHand, 3473, |w, c| {
        c.modifiers.right
            && w.get::<TrackingBeacon>(c.target).is_some()
            && attack_self(w, c.client, c.mob, c.target)
    });
    app.add_observer(add_beacon).add_systems(
        Update,
        (
            beacon_emps.in_set(crate::emp::EmpSystems::Effects),
            choice_answers
                .in_set(ss13_tgui::server::UiSystems::Handle)
                .after(ss13_tgui::prompt::PromptSystems::Answer),
        ),
    );
}

fn add_beacon(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if is_under(&item.path, BEACON) {
        commands
            .entity(add.entity)
            .try_insert_if_new(TrackingBeacon { enabled: true });
    } else if is_under(&item.path, CHOICE_BEACON)
        && let Some(kind) = ChoiceKind::from_path(&item.path)
    {
        commands.entity(add.entity).try_insert_if_new(ChoiceBeacon {
            kind,
            uses: kind.uses(),
        });
    }
}

/// `GLOB.teleportbeacons`: the enabled tracking beacons.
pub fn enabled_beacons(world: &mut World) -> Vec<Entity> {
    world
        .query::<(Entity, &TrackingBeacon)>()
        .iter(world)
        .filter(|(_, b)| b.enabled)
        .map(|(e, _)| e)
        .collect()
}

fn set_enabled(world: &mut World, beacon: Entity, enabled: bool) {
    if let Some(mut b) = world.get_mut::<TrackingBeacon>(beacon) {
        b.enabled = enabled;
    }
    if !enabled {
        // `COMSIG_BEACON_DISABLED`.
        crate::science::teleportation::teleporter::beacon_disabled(world, beacon);
    }
    let state = if enabled { "beacon" } else { "beacon-off" };
    crate::obj_damage::set_icon_state(world, beacon, state);
}

/// `attack_self` of a tracking or choice beacon. True when `item` is one.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if let Some(&TrackingBeacon { enabled }) = world.get::<TrackingBeacon>(item) {
        set_enabled(world, item, !enabled);
        let text = if enabled {
            "Вы выключаете маяк."
        } else {
            "Вы включаете маяк."
        };
        crate::tools::tell(world, client, Span::Notice, text);
        return true;
    }
    let Some(kind) = world.get::<ChoiceBeacon>(item).map(|b| b.kind) else {
        return false;
    };
    if !can_use_beacon(world, mob, item) {
        return true;
    }
    // `generate_display_names`: `initial(name) = type`, only the types the port has.
    let paths: Vec<String> = {
        let protos = world.get_resource::<Prototypes>();
        kind.options()
            .iter()
            .filter(|p| protos.is_some_and(|ps| ps.get(p).is_some()))
            .map(|p| (*p).to_owned())
            .collect()
    };
    if paths.is_empty() {
        return true;
    }
    let read: &World = world;
    let names = paths.iter().map(|p| kind.display_name(read, p)).collect();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Выбор предмета",
            "Что вы хотите заказать?",
            PromptKind::List {
                items: names,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(ChoicePrompt {
        beacon: item,
        paths,
    });
    true
}

/// `can_use_beacon`: `can_perform_action(src, FORBID_TELEKINESIS_REACH)` (conscious, able,
/// holding it), else a buzz.
fn can_use_beacon(world: &mut World, mob: Entity, beacon: Entity) -> bool {
    let holding = world.get::<Inventory>(mob).is_some_and(|i| i.holds(beacon));
    let able = world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated;
    if holding && able {
        return true;
    }
    if let Some(pos) = turf_of(world, beacon) {
        playsound(world, PlaySound::new(pos, BUZZ, 40.0, true));
    }
    false
}

/// `open_options_menu` after the choice: the user checked again, then `consume_use`.
fn choice_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answers {
        let Some(ChoicePrompt { beacon, paths }) = world.get::<ChoicePrompt>(prompt).cloned()
        else {
            continue;
        };
        let Some(PromptAnswer::Item(index)) = answer else {
            continue;
        };
        let Some(path) = paths.get(index) else {
            continue;
        };
        // Spent or gone while the list was open.
        if world
            .get::<ChoiceBeacon>(beacon)
            .is_none_or(|b| b.uses == 0)
            || !can_use_beacon(world, mob, beacon)
        {
            continue;
        }
        consume_use(world, client, mob, beacon, path);
    }
}

/// `consume_use`: the message, the delivery, a use gone (the last: sparks, deleted).
fn consume_use(world: &mut World, client: Entity, mob: Entity, beacon: Entity, path: &str) {
    let Some(kind) = world.get::<ChoiceBeacon>(beacon).map(|b| b.kind) else {
        return;
    };
    let (source, message) = kind.company();
    crate::tools::tell(
        world,
        client,
        Span::Notice,
        format!(
            "Вы слышите, как маяк на мгновение потрескивает, а затем раздаётся голос. «Ожидайте \
             сообщения от {source}. Текст сообщения: {message} Конец сообщения.»"
        ),
    );
    spawn_option(world, kind, mob, beacon, path);
    let uses = {
        let Some(mut b) = world.get_mut::<ChoiceBeacon>(beacon) else {
            return;
        };
        b.uses = b.uses.saturating_sub(1);
        b.uses
    };
    if uses == 0 {
        if let Some(pos) = turf_of(world, beacon) {
            crate::effects::particles::do_sparks(world, 3, false, pos);
        }
        world.despawn(beacon);
        return;
    }
    crate::tools::tell(
        world,
        client,
        Span::Notice,
        format!("Осталось использований: {uses}."),
    );
}

/// `spawn_option`: a teleporter pod onto the beacon's tile; the augment beacon's box at the
/// user's feet, quietly.
fn spawn_option(world: &mut World, kind: ChoiceKind, mob: Entity, beacon: Entity, path: &str) {
    if kind == ChoiceKind::Augments {
        let Some(pos) = turf_of(world, mob) else {
            return;
        };
        crate::construction::spawn_obj(world, path, pos);
        if let Some(at) = turf_of(world, beacon) {
            playsound(world, PlaySound::new(at, EMITTER, 50.0, false));
        }
        return;
    }
    let Some(pos) = turf_of(world, beacon) else {
        return;
    };
    crate::cargo::supplypod::launch(
        world,
        pos,
        crate::cargo::supplypod::PodCargo::Items(vec![path.to_owned()]),
    );
}

/// `emp_act`: `prob(75 / severity)` flips the beacon.
fn beacon_emps(world: &mut World, mut cursor: Local<MessageCursor<crate::emp::EmpAct>>) {
    for ev in crate::emp::emp_acts(world, &mut cursor) {
        let Some(&TrackingBeacon { enabled }) = world.get::<TrackingBeacon>(ev.target) else {
            continue;
        };
        if !ev.hits_self() {
            continue;
        }
        let chance = 75.0 / f32::from(ev.severity.max(1));
        if world
            .resource_mut::<crate::combat::CombatRng>()
            .chance(chance)
        {
            set_enabled(world, ev.target, !enabled);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choice_beacons_by_path() {
        assert_eq!(
            ChoiceKind::from_path("/obj/item/choice_beacon/music"),
            Some(ChoiceKind::Music)
        );
        assert_eq!(
            ChoiceKind::from_path("/obj/item/choice_beacon/augments"),
            Some(ChoiceKind::Augments)
        );
        assert_eq!(ChoiceKind::from_path("/obj/item/choice_beacon/holy"), None);
        assert_eq!(ChoiceKind::Augments.uses(), 3);
        assert_eq!(ChoiceKind::Hero.uses(), 1);
        // BandaStation's sidearm beacons offer gun cases under their list names.
        let hos = ChoiceKind::from_path("/obj/item/choice_beacon/weapon/hos").unwrap();
        assert_eq!(hos, ChoiceKind::HosPistol);
        assert_eq!(hos.options().len(), 4);
        assert_eq!(hos.uses(), 1);
        let world = World::new();
        assert_eq!(
            ChoiceKind::SecurityPistol.display_name(&world, "/obj/item/gun/energy/disabler"),
            "Энергетический пистолет дизейблер"
        );
        assert_eq!(
            ChoiceKind::from_path("/obj/item/choice_beacon/weapon"),
            None
        );
    }
}
