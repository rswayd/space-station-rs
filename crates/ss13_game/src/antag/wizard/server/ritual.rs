//! The Grand Ritual (server) after tg `/datum/action/cooldown/grand_ritual`
//! (`grand_ritual/grand_ritual.dm`), `/obj/effect/grand_rune` (`grand_rune.dm`), the side effects
//! (`grand_side_effect.dm`) and the finales (`finales/`).
//!
//! - **The button** ([`GrandRitual`]): points at a random whitelisted station area; inside it, a
//!   4 s `do_after` draws a 3x3 rune (not next to a rune, not on space or chasms), evaporating the
//!   dense things around, and starts a 2 minute cooldown. From the third completion on the
//!   crew gets an announcement; after six the seventh rune is the finale.
//! - **Invoking** ([`GrandRune`]): a wizard's click; three `do_after`s of `7 + 2 × potency`
//!   seconds, each saying one of the rune's magic words and sacrificing part of the cheese wheels
//!   around it. The third runs a random event of the rune's potency, a random side effect
//!   (`potency >= 1`), up to 1-3 heretic influences anywhere, leaves ash.
//! - **Finales** ([`Finale`]): the rune of the seventh ritual asks once (`Continuation` for the
//!   ordinary rune): Connection (every door opens, access gone), Annihilation (the invoker gibbed,
//!   then a singularity, a tesla ball or meteors), Jubilation (clown clothes), Perpetuation
//!   (everyone comes back 60 s after death), Transformation (a theme spreads over the station),
//!   Usurpation (see below); 50 cheese sacrificed before make it a cheesy rune instead (the
//!   Wabbajack, the crew's vendetta and trauma). The wizard's objective then is the ritual.
// shortcut: the side effects skip dimensional instability (spacetime_dist is not ported) and the
// lube foam is wet floors; food rains without pods from a short list (`FOOD`); the magic
// words' `*emote` lines are emotes; haunted cheese just disappears; the finale's menu is a list
// (tg radial icons). Usurpation only stuns and tells the crew: no outfit or manifest change.
// The vendetta objective is a message (no `wizard_prank_vendetta` datum). Evolution
// (`summon_magic`) is not offered (Summon Magic is not ported). Immortality does not bring back
// the gibbed (no human appearance profile) and suicides are not exempt. Instrument monkeys
// (orchestra) are not ported.

use bevy::ecs::message::MessageCursor;

use super::*;
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::click_handlers::ClickCtx;
use crate::spells::{Invocation, say_invocation};
use crate::status_effects::{Incap, effects_mut};
use crate::tools::{Actor, DoAfter, do_after, now};
use ss13_world::SpatialIndex;

pub const RUNE: &str = "/obj/effect/grand_rune";
pub const FINALE_RUNE: &str = "/obj/effect/grand_rune/finale";
pub const CHEESY_RUNE: &str = "/obj/effect/grand_rune/finale/cheesy";
const REMAINS: &str = "/obj/effect/decal/cleanable/grand_remains";
const CHEESE_REMAINS: &str = "/obj/effect/decal/cleanable/grand_remains/cheese";
const SPECTRE: &str = "/obj/effect/spectre_of_resurrection";
const CHEESE_WHEEL: &str = "/obj/item/food/cheese/wheel";
const WABBAJACK: &str = "/obj/item/gun/magic/staff/chaos/true_wabbajack";
/// `CHEESE_SACRIFICE_GOAL`, `GRAND_RUNE_INVOKES_TO_COMPLETE`.
const CHEESE_GOAL: u32 = 50;
const INVOKES: u32 = 3;
const FINALE_COUNT: u32 = 7;
const WARNING_POTENCY: u32 = 3;
const IMMINENT_POTENCY: u32 = 6;
/// `BASE_INVOKE_TIME`, `ADD_INVOKE_TIME`, deciseconds.
const BASE_INVOKE_DS: f32 = 70.0;
const ADD_INVOKE_DS: f32 = 20.0;
const DRAW_DS: f32 = 40.0;
const COOLDOWN_SECS: f64 = 120.0;
const BUTTON_ID: &str = "wizard:grand_ritual";
const CULT_DMI: &str = "icons/mob/actions/actions_cult.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
const IMMORTAL_PRE_SECS: f64 = 10.0;
const IMMORTAL_REVIVE_SECS: f64 = 50.0;
const CREWMATE_SECS: f64 = 9.0;
const COUNT_LERP: f32 = 1.0 / FINALE_COUNT as f32;
const FOOD: [&str; 6] = [
    "/obj/item/food/cheese/wedge",
    "/obj/item/food/grown/banana",
    "/obj/item/food/cookie",
    "/obj/item/food/donut/plain",
    "/obj/item/food/pizzaslice/margherita",
    "/obj/item/food/burger/plain",
];
const DELAYED_MOBS: [&str; 6] = [
    "/mob/living/basic/carp",
    "/mob/living/basic/illusion",
    "/mob/living/basic/killer_tomato",
    "/mob/living/basic/skeleton",
    "/mob/living/basic/wumborian_fugu",
    "/mob/living/simple_animal/hostile/ooze",
];
const AREA_WHITELIST: [&str; 12] = [
    "/area/station/cargo",
    "/area/station/command",
    "/area/station/commons",
    "/area/station/construction",
    "/area/station/engineering",
    "/area/station/maintenance/disposal",
    "/area/station/maintenance/radshelter",
    "/area/station/maintenance/tram",
    "/area/station/medical",
    "/area/station/science",
    "/area/station/security",
    "/area/station/service",
];
const AREA_BLACKLIST: [&str; 7] = [
    "/area/station/engineering/supermatter",
    "/area/station/engineering/transit_tube",
    "/area/station/science/ordnance/bomb",
    "/area/station/science/ordnance/burnchamber",
    "/area/station/science/ordnance/freezerchamber",
    "/area/station/science/server",
    "/area/station/security/prison/safe",
];
/// `blacklisted_rune_turfs`.
const BAD_TURFS: [&str; 6] = [
    "/turf/closed/indestructible",
    "/turf/open/chasm",
    "/turf/open/indestructible",
    "/turf/open/lava",
    "/turf/open/openspace",
    "/turf/open/space",
];
const MAGIC_WORDS: [[&str; 3]; 23] = [
    ["*scream", "*scream", "*scream"],
    ["Абра...", "Кадабра...", "Алаказам!"],
    ["Азарат!", "Метрион!", "Зинтрос!!"],
    ["Бибити!", "Бобити!", "Буу!"],
    ["Биш", "Баш", "Бош!"],
    ["Эни... ", "Мени... ", "Миниму!!"],
    ["Эсальп!", "Оу Натх!", "Ем Есукзме!!"],
    ["Фус", "Ро", "Да!!"],
    [
        "git checkout origin master",
        "git reset --hard HEAD~2",
        "git push origin master --force!!",
    ],
    ["Фокус Покус!", "Флим Флам!", "Вабаджак!"],
    [
        "Я желаю...",
        "Я бы хотел...",
        "Желание, которое я загадаю сегодня вечером!",
    ],
    ["Клату!", "Барада!", "Никто!!"],
    [
        "Пусть расширяются просторы!",
        "Пусть эон станет мгновением!",
        "Распахните ворота настежь!!",
    ],
    ["Левиос!", "Гравиоле!", "Експломб!!"],
    ["Микрато", "Рапиж", "Сактонишь!"],
    ["Нокту!", "Орфей!", "Ауде! Фраетор!"],
    ["Квуас!", "Векс!", "Ексорт!"],
    ["Сим!", "Сала!", "Бим!"],
    [
        "Семь теней отброшены, семь судеб предсказаны!",
        "Пусть их слова отзвучат в вашей пустой душе.!",
        "Разрушение наступило!!",
    ],
    [
        "Свифткаст! Хастега! Ограждающее поле 2! Продление 4! Продвинутое усиление Тенсера!",
        "Защита от зла 2! Щит мага! Посредственное усиление Венериуса 2! Расширить силу!",
        "Сурекастага! Обновление! Шарпкаст 10! Эфирная манипуляция! Вызвать Великий Ритуал!!",
    ],
    ["Тен!", "Чи!", "Джи!"],
    [
        "Высшая школа магии!",
        "Ультимативный ритуал!",
        "Макрокосмос!!",
    ],
    ["Яабa", "Да Ба", "Дуум!!"],
];
const CHEESE_WORDS: [&str; 3] = [
    "Здравствуйте! Приветствую!",
    " Добро пожаловать! А теперь уходи.",
    "Уходи. Беги. Или умри.",
];

/// The wizard's `/datum/action/cooldown/grand_ritual`.
#[derive(Component, Clone, Debug, Default)]
pub struct GrandRitual {
    pub target_area: Option<Entity>,
    pub times_completed: u32,
    pub drew_finale: bool,
    pub cheese_total: u32,
    pub cheese_goal_met: bool,
    pub rune: Option<Entity>,
    pub ready_at: f64,
}

/// `/datum/grand_finale`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finale {
    AllAccess,
    Armageddon,
    Clown,
    Immortality,
    Midas,
    Usurp,
    Cheese,
}

impl Finale {
    /// The finale choices (`subtypesof(/datum/grand_finale)` with a menu entry, Evolution
    /// missing); `Cheese` is the cheesy rune's own.
    const MENU: [Self; 6] = [
        Self::AllAccess,
        Self::Armageddon,
        Self::Clown,
        Self::Immortality,
        Self::Midas,
        Self::Usurp,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::AllAccess => "Connection",
            Self::Armageddon => "Annihilation",
            Self::Clown => "Jubilation",
            Self::Immortality => "Perpetuation",
            Self::Midas => "Transformation",
            Self::Usurp => "Usurpation",
            Self::Cheese => "Cheese",
        }
    }

    /// `minimum_time`, seconds of round time.
    fn minimum_secs(self) -> f64 {
        match self {
            Self::Armageddon => 5400.0,
            Self::Cheese => 2700.0,
            Self::Immortality => 1800.0,
            _ => 0.0,
        }
    }

    /// `ritual_invoke_time`, deciseconds.
    fn invoke_ds(self) -> f32 {
        if self == Self::Armageddon {
            600.0
        } else {
            300.0
        }
    }

    /// `dire_warning`.
    fn dire(self) -> bool {
        matches!(self, Self::Armageddon | Self::Cheese)
    }
}

/// `/obj/effect/grand_rune`.
#[derive(Component, Clone, Debug)]
pub struct GrandRune {
    pub owner: Entity,
    pub potency: u32,
    pub invoke_ds: f32,
    pub times_invoked: u32,
    pub cheese: u32,
    pub words: [&'static str; 3],
    /// The finale rune; `None` for an ordinary one.
    pub finale: Option<Option<Finale>>,
    pub chosen: bool,
    pub dire_given: u32,
}

/// The finale list of a rune.
#[derive(Component, Clone, Copy, Debug)]
struct FinalePrompt {
    rune: Entity,
}

/// What `tick` still owes after a rune.
#[derive(Component, Clone, Debug)]
enum RitualTimer {
    /// `spawn_delayed_mobs`.
    Mob {
        path: &'static str,
        pos: TilePos,
        at: f64,
    },
    /// `create_food`, until the end.
    Food { pos: TilePos, until: f64, next: f64 },
    /// `summon_crewmate`.
    Crewmate {
        victim: Entity,
        dest: TilePos,
        at: f64,
    },
}

/// `midas/transform_area`: the theme's turfs by due time, `next` the first not yet done.
#[derive(Component, Clone, Debug)]
struct MidasSpread {
    theme: usize,
    turfs: Vec<(f64, TilePos)>,
    next: usize,
}

/// `Perpetuation`: everything that dies comes back (`COMSIG_GLOB_MOB_DEATH`).
#[derive(Resource)]
struct Immortality;

/// A corpse on its way back.
#[derive(Component, Clone, Copy, Debug)]
struct Reviving {
    /// The spectre once made, what holds the body.
    spectre: Option<Entity>,
    pos: TilePos,
    /// When the body turns into a spectre, and when it wakes.
    hide_at: f64,
    revive_at: f64,
}

/// Who gets the ritual: `assign_ritual` of a (non-apprentice) wizard.
pub(super) fn grant(world: &mut World, mob: Entity) {
    world.entity_mut(mob).insert(GrandRitual::default());
    set_new_area(world, mob);
    refresh_button(world, mob);
}

fn refresh_button(world: &mut World, mob: Entity) {
    let Some(ritual) = world.get::<GrandRitual>(mob).cloned() else {
        return;
    };
    let icon = |world: &World, dmi: &str, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(dmi, state))
    };
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(button, _)| button.id != BUTTON_ID)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    buttons.push((
        ActionButton {
            id: BUTTON_ID.to_owned(),
            name: "Grand Ritual".to_owned(),
            icon: icon(world, CULT_DMI, "draw"),
            background: icon(world, BACKGROUNDS_DMI, "bg_spell"),
            border: icon(world, BACKGROUNDS_DMI, "bg_spell_border"),
            available: world.get::<Stat>(mob) == Some(&Stat::Conscious),
            cooldown: 0.0,
            active: false,
            maptext: None,
        },
        ritual.ready_at,
    ));
    set_buttons(world, mob, buttons);
}

fn valid_area(path: &str) -> bool {
    AREA_WHITELIST.iter().any(|r| is_under(path, r))
        && !AREA_BLACKLIST.iter().any(|r| is_under(path, r))
}

/// `set_new_area`.
fn set_new_area(world: &mut World, mob: Entity) {
    let Some(all) = world.get_resource::<ss13_mapload::Areas>() else {
        return;
    };
    let mut areas: Vec<(Entity, String)> = all
        .entities()
        .iter()
        .copied()
        .filter_map(|a| {
            world
                .get::<ss13_mapload::DmPower>(a)
                .filter(|p| {
                    valid_area(&p.path) && !crate::spells::effects::area_tiles(world, a).is_empty()
                })
                .map(|p| (a, p.path.clone()))
        })
        .collect();
    areas.sort_by(|a, b| a.1.cmp(&b.1));
    if areas.is_empty() {
        return;
    }
    let i = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(0, areas.len() as u32 - 1) as usize;
    let area = areas[i].0;
    if let Some(mut ritual) = world.get_mut::<GrandRitual>(mob) {
        ritual.target_area = Some(area);
    }
    let name = crate::shuttle::area_name(world, area);
    chat_to(
        world,
        mob,
        Span::Warning,
        format!("Следующий узел силы находится в {name}"),
    );
}

pub(super) fn buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if id != BUTTON_ID
            || actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<GrandRitual>(actor).is_none()
            || world.get::<Stat>(actor) != Some(&Stat::Conscious)
        {
            continue;
        }
        activate(world, client, actor);
    }
}

/// `Activate`.
fn activate(world: &mut World, client: Entity, mob: Entity) {
    let Some(ritual) = world.get::<GrandRitual>(mob).cloned() else {
        return;
    };
    if crate::status_effects::incapacitation(world, mob).hands_blocked
        || now(world) < ritual.ready_at
    {
        return;
    }
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        chat_to(world, mob, Span::Warning, "не достает до пола!");
        return;
    };
    if ritual
        .target_area
        .is_none_or(|a| crate::spells::effects::area_tiles(world, a).is_empty())
    {
        set_new_area(world, mob);
    }
    let ritual = world.get::<GrandRitual>(mob).cloned().unwrap_or_default();
    let area = world
        .get_resource::<ss13_mapload::Areas>()
        .and_then(|a| a.area_at(pos));
    if area.is_some() && area == ritual.target_area {
        start_drawing(world, client, mob, pos, &ritual);
    } else if let Some(target) = ritual.target_area {
        let text = pinpoint(world, pos, target);
        chat_to(world, mob, Span::Notice, text);
    }
}

/// `get_pinpoint_text`.
fn pinpoint(world: &World, pos: TilePos, area: Entity) -> String {
    let Some(&tile) = crate::spells::effects::area_tiles(world, area).first() else {
        return "другое место!".into();
    };
    let stations = crate::events::controller::station_levels(world);
    if tile.z != pos.z {
        return if stations.contains(&pos.z) && stations.contains(&tile.z) {
            if pos.z > tile.z {
                "под вами!"
            } else {
                "над вами!"
            }
            .into()
        } else if stations.contains(&tile.z) {
            "на станции!".into()
        } else {
            "другое место!".into()
        };
    }
    let dist = pos.chebyshev(tile).unwrap_or(0);
    let dir = dir_name(pos, tile);
    match dist {
        0..=15 => format!("очень близко, {dir}!"),
        16..=31 => format!("рядом, {dir}!"),
        32..=127 => format!("далеко, {dir}!"),
        _ => "очень далеко!".into(),
    }
}

fn dir_name(from: TilePos, to: TilePos) -> String {
    use crate::pull::dir_between;
    dir_between(from, to)
        .map(|d| format!("{d:?}"))
        .unwrap_or_default()
}

/// `start_drawing_rune`.
fn start_drawing(
    world: &mut World,
    client: Entity,
    mob: Entity,
    pos: TilePos,
    ritual: &GrandRitual,
) {
    if ritual.rune.is_some_and(|r| world.get_entity(r).is_ok()) {
        chat_to(world, mob, Span::Warning, "руна уже существует!");
        return;
    }
    if !can_draw_here(world, pos) {
        chat_to(world, mob, Span::Warning, "неверное покрытие!");
        return;
    }
    let close = world
        .resource::<SpatialIndex>()
        .in_radius(pos, 3)
        .any(|e| world.get::<GrandRune>(e).is_some());
    if close {
        chat_to(world, mob, Span::Warning, "руна слишком близко!");
        return;
    }
    if world.get::<DoAfter>(mob).is_some() {
        chat_to(world, mob, Span::Warning, "уже рисуем!");
        return;
    }
    do_after(
        world,
        Actor {
            client,
            mob,
            item: None,
        },
        mob,
        DRAW_DS,
        0,
        crate::construction::Job::Wizard(WizardJob::DrawRune),
    );
}

fn can_draw_here(world: &World, pos: TilePos) -> bool {
    for dx in -1..=1 {
        for dy in -1..=1 {
            let tile = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            let bad = crate::construction::turf_at(world, tile)
                .and_then(|t| crate::construction::path_of(world, t))
                .is_some_and(|p| BAD_TURFS.iter().any(|r| is_under(&p, r)));
            if bad {
                return false;
            }
        }
    }
    true
}

/// `draw_rune`'s second half.
pub(super) fn complete_draw(world: &mut World, mob: Entity) {
    let Some(ritual) = world.get::<GrandRitual>(mob).cloned() else {
        return;
    };
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return;
    };
    if evaporate_obstacles(world, pos) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/effects/magic/blind.ogg", 100.0, true),
        );
    }
    let (path, finale) = rune_type(&ritual);
    let Some(rune) = crate::construction::spawn_obj(world, path, pos) else {
        warn!("no prototype for {path}: no rune");
        return;
    };
    world.flush();
    let potency = ritual.times_completed;
    let i = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(0, MAGIC_WORDS.len() as u32 - 1) as usize;
    let cheesy = path == CHEESY_RUNE;
    let words = if cheesy { CHEESE_WORDS } else { MAGIC_WORDS[i] };
    let invoke_ds = if cheesy {
        Finale::Cheese.invoke_ds()
    } else {
        BASE_INVOKE_DS + potency as f32 * ADD_INVOKE_DS
    };
    world.entity_mut(rune).insert(GrandRune {
        owner: mob,
        potency,
        invoke_ds,
        times_invoked: 0,
        cheese: 0,
        words,
        finale,
        chosen: cheesy,
        dire_given: 0,
    });
    announce_rune(world, pos, potency);
    let t = now(world);
    if let Some(mut r) = world.get_mut::<GrandRitual>(mob) {
        r.rune = Some(rune);
        r.drew_finale |= finale.is_some();
        r.ready_at = t + COOLDOWN_SECS;
    }
    chat_to(world, mob, Span::Notice, "руна создана");
    refresh_button(world, mob);
}

/// `get_appropriate_rune_typepath`.
fn rune_type(ritual: &GrandRitual) -> (&'static str, Option<Option<Finale>>) {
    if ritual.times_completed < FINALE_COUNT - 1 || ritual.drew_finale {
        (RUNE, None)
    } else if ritual.cheese_total >= CHEESE_GOAL {
        (CHEESY_RUNE, Some(Some(Finale::Cheese)))
    } else {
        (FINALE_RUNE, Some(None))
    }
}

fn evaporate_obstacles(world: &mut World, pos: TilePos) -> bool {
    let mut any = false;
    for dx in -1..=1 {
        for dy in -1..=1 {
            let tile = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            let dense: Vec<Entity> = world
                .resource::<SpatialIndex>()
                .dense_on_tile(tile)
                .iter()
                .copied()
                .filter(|&e| world.get::<Health>(e).is_none())
                .collect();
            for e in dense {
                any = true;
                if world.get::<ss13_mapload::Turf>(e).is_some() {
                    crate::construction::dismantle_wall(world, e, tile);
                } else {
                    crate::obj_damage::deconstruct(world, e, false);
                }
            }
        }
    }
    any
}

/// `announce_rune`.
fn announce_rune(world: &mut World, pos: TilePos, potency: u32) {
    let area = world
        .get_resource::<ss13_mapload::Areas>()
        .and_then(|a| a.area_at(pos))
        .map(|a| crate::shuttle::area_name(world, a))
        .unwrap_or_default();
    let text = if potency >= IMMINENT_POTENCY {
        format!(
            "Крупные аномальные флуктуации локального пространства-времени, обнаруженные в: {area}."
        )
    } else if potency >= WARNING_POTENCY {
        format!("Необычные аномальные флуктуации энергии, обнаруженные в: {area}.")
    } else {
        return;
    };
    crate::announce::Priority::new(text, "Обнаружена аномалия").send(world);
}

/// A wizard's click on a rune (`can_interact`, `interact`).
pub(super) fn interact(world: &mut World, c: &ClickCtx) -> bool {
    let Some(rune) = world.get::<GrandRune>(c.target).cloned() else {
        return false;
    };
    if world.get::<Wizard>(c.mob).is_none() || rune.times_invoked >= INVOKES {
        return true;
    }
    if world
        .get::<DoAfter>(c.mob)
        .is_some_and(|d| d.target == c.target)
    {
        return true;
    }
    if rune.finale.is_some() && !rune.chosen {
        ask_finale(world, c.client, c.mob, c.target);
        return true;
    }
    if let Some(Some(finale)) = rune.finale {
        let left = finale.minimum_secs() - round_secs(world);
        if left > 0.0 {
            tell(
                world,
                c.client,
                Span::Warning,
                format!("Выбранный гранд-финал будет доступен только через {left:.0} с!"),
            );
            return true;
        }
    }
    start_invoke(world, c.client, c.mob, c.target);
    true
}

fn round_secs(world: &World) -> f64 {
    now(world)
}

/// `invoke_rune`, its dire warnings.
fn start_invoke(world: &mut World, client: Entity, mob: Entity, rune_entity: Entity) {
    let Some(rune) = world.get::<GrandRune>(rune_entity).cloned() else {
        return;
    };
    if let Some(Some(finale)) = rune.finale
        && finale.dire()
        && rune.dire_given == rune.times_invoked
    {
        dire_warning(world, rune_entity, rune.dire_given);
        if let Some(mut r) = world.get_mut::<GrandRune>(rune_entity) {
            r.dire_given += 1;
        }
    }
    chat_to(world, mob, Span::Notice, "призываем руну...");
    do_after(
        world,
        Actor {
            client,
            mob,
            item: None,
        },
        rune_entity,
        rune.invoke_ds,
        0,
        crate::construction::Job::Wizard(WizardJob::InvokeRune),
    );
}

fn dire_warning(world: &mut World, rune: Entity, given: u32) {
    let area = world
        .get::<TilePos>(rune)
        .and_then(|&p| world.get_resource::<ss13_mapload::Areas>()?.area_at(p))
        .map(|a| crate::shuttle::area_name(world, a))
        .unwrap_or_default();
    let text = match given {
        0 => format!("Обнаружен большой аномальный всплеск энергии в: {area}."),
        1 => format!(
            "Автоматическая стабилизация казусов не удалась, рекомендуем срочное вмешательство в: {area}."
        ),
        2 => format!(
            "Неминуемый провал локальной реальности в: {area}. Всем членам экипажа приготовиться к эвакуации."
        ),
        _ => return,
    };
    crate::announce::Priority::new(text, "Обнаружена аномалия").send(world);
}

fn ask_finale(world: &mut World, client: Entity, mob: Entity, rune: Entity) {
    let mut items: Vec<String> = Finale::MENU.iter().map(|f| f.name().to_owned()).collect();
    items.push("Continuation".to_owned());
    let ask = Ask::new(
        client,
        mob,
        "Великий финал",
        "Высшее применение собранной вами силы!",
        PromptKind::List {
            items,
            default: None,
        },
    );
    let prompt = ask_world(world, ask);
    world.entity_mut(prompt).insert(FinalePrompt { rune });
}

pub(super) fn finale_answers(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in pending {
        let Some(asked) = world.get::<FinalePrompt>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        // After the wait: the rune and the wizard are still there and undecided.
        let open = world
            .get::<GrandRune>(asked.rune)
            .is_some_and(|r| r.finale.is_some() && !r.chosen);
        if !open || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        let picked = Finale::MENU.get(i).copied();
        if picked.is_none() && i != Finale::MENU.len() {
            continue;
        }
        if let Some(f) = picked
            && f.minimum_secs() >= round_secs(world)
        {
            tell(
                world,
                client,
                Span::Warning,
                "Выбранный гранд-финал ещё недоступен. Подождите или выберите другой вариант.",
            );
            continue;
        }
        let default_ds = world
            .get::<GrandRune>(asked.rune)
            .map_or(BASE_INVOKE_DS, |r| {
                BASE_INVOKE_DS + r.potency as f32 * ADD_INVOKE_DS
            });
        if let Some(mut r) = world.get_mut::<GrandRune>(asked.rune) {
            r.chosen = true;
            r.finale = Some(picked);
            r.invoke_ds = picked.map_or(default_ds, Finale::invoke_ds);
        }
    }
}

/// A finished `do_after` of the ritual.
pub(super) fn complete_job(world: &mut World, mob: Entity, action: &DoAfter, job: WizardJob) {
    match job {
        WizardJob::DrawRune => complete_draw(world, mob),
        WizardJob::InvokeRune => invoked(world, action.client, mob, action.target),
        _ => {}
    }
}

/// `invoke_rune` past its `do_after`.
fn invoked(world: &mut World, client: Entity, mob: Entity, rune_entity: Entity) {
    let Some(mut rune) = world.get::<GrandRune>(rune_entity).cloned() else {
        return;
    };
    rune.times_invoked += 1;
    let Some(&pos) = world.get::<TilePos>(rune_entity) else {
        return;
    };
    rune.cheese += sacrifice_cheese(world, pos, rune.times_invoked);
    let word = rune.words[(rune.times_invoked as usize - 1).min(2)];
    if word.starts_with('*') {
        crate::mobs::defense::emote(world, mob, "кричит!");
    } else {
        say_invocation(world, mob, Invocation::Shout, word);
    }
    if rune.times_invoked >= INVOKES {
        world.entity_mut(rune_entity).insert(rune.clone());
        complete_rune(world, mob, rune_entity, &rune, pos);
        return;
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/effects/magic/staff_animation.ogg", 75.0, true),
    );
    world.entity_mut(rune_entity).insert(rune);
    start_invoke(world, client, mob, rune_entity);
}

/// `round(cheese near * invocation * 0.4)` wheels go, what stays is next.
fn sacrifice_cheese(world: &mut World, pos: TilePos, times: u32) -> u32 {
    let wheels: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, 1)
        .filter(|&e| crate::construction::path_of(world, e).as_deref() == Some(CHEESE_WHEEL))
        .collect();
    let take = ((wheels.len() as f32 * times as f32 * 0.4).round() as usize).min(wheels.len());
    for &wheel in wheels.iter().take(take) {
        world.despawn(wheel);
    }
    take as u32
}

/// `on_invocation_complete` + `on_rune_complete`.
fn complete_rune(
    world: &mut World,
    mob: Entity,
    rune_entity: Entity,
    rune: &GrandRune,
    pos: TilePos,
) {
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/effects/magic/staff_change.ogg", 75.0, true),
    );
    if let Some(Some(finale)) = rune.finale {
        info!("{mob} completed the grand ritual finale {finale:?}");
        trigger_finale(world, mob, finale, pos);
    } else {
        summon_round_event(world, mob, rune.potency);
    }
    if rune.potency > 0 {
        side_effect(world, mob, rune.potency, pos);
    }
    tear_reality(world, rune.potency);
    on_rune_complete(world, rune.owner, rune.cheese);
    let cheesy = rune.finale.flatten() == Some(Finale::Cheese);
    crate::construction::spawn_obj(world, if cheesy { CHEESE_REMAINS } else { REMAINS }, pos);
    world.despawn(rune_entity);
}

/// `on_rune_complete` of the action.
fn on_rune_complete(world: &mut World, mob: Entity, cheese: u32) {
    let Some(mut ritual) = world.get::<GrandRitual>(mob).cloned() else {
        return;
    };
    ritual.cheese_total += cheese;
    ritual.times_completed += 1;
    ritual.rune = None;
    if ritual.cheese_total >= CHEESE_GOAL {
        let text = if ritual.cheese_goal_met {
            "Вы слышите безумный смех, когда на вас обрушивается непреодолимый запах прекрасного чеддера...".to_owned()
        } else {
            ritual.cheese_goal_met = true;
            "ДА! СЫР! СЫР ДЛЯ ВСЕХ! ТАКОЙ ГРАНДИОЗНЫЙ ПРАЗДНИК! ТЫ ПОЛУЧИШЬ СВОЙ ПРИЗ, МОЙ ЧЕМПИОН!!".to_owned()
        };
        chat_to(world, mob, Span::Notice, text);
    } else if ritual.cheese_total > 0 {
        chat_to(
            world,
            mob,
            Span::Notice,
            format!(
                "Ты радуешь меня, смертный. Продолжай присылать сыр, мой пир все еще нуждается в еще {}, чтобы быть великолепным...",
                CHEESE_GOAL - ritual.cheese_total
            ),
        );
    }
    let times = ritual.times_completed;
    let goal_met = ritual.cheese_total >= CHEESE_GOAL;
    world.entity_mut(mob).insert(ritual);
    set_new_area(world, mob);
    match times {
        WARNING_POTENCY => chat_to(
            world,
            mob,
            Span::Warning,
            "Ваша собранная сила растет, но дальнейшие ритуалы предупредят врагов о вашем положении.",
        ),
        IMMINENT_POTENCY => chat_to(
            world,
            mob,
            Span::Warning,
            if goal_met {
                "Вы переполнены хаотической энергией! Ваш следующий Великий ритуал наколдует могущественный артефакт для вашего использования и подарит вам победу."
            } else {
                "Вы переполнены силой! Ваш следующий Великий ритуал позволит вам выбрать мощный эффект и подарит вам победу."
            },
        ),
        FINALE_COUNT => {
            if let Some(mut wizard) = world.get_mut::<Wizard>(mob) {
                wizard.objectives = vec![WizardObjective::RitualDone];
            }
        }
        _ => {}
    }
}

/// The tg names of the ported round events a rune may run, with
/// `min/max_wizard_trigger_potency` (`can_spawn_event(allow_magic = TRUE)`).
const EVENT_POTENCY: [(&str, u32, u32); 18] = [
    ("Radiation Storm", 3, 7),
    ("Spontaneous Brain Trauma", 2, 6),
    ("Chasmic Earthquake", 3, 7),
    ("Wormholes", 3, 7),
    ("Carp Migration", 0, 3),
    ("Random Heart Attack", 6, 7),
    ("Immovable Rod", 6, 7),
    ("Communications Blackout", 0, 3),
    ("Supermatter Surge", 4, 7),
    ("Space Vines", 4, 7),
    ("Radiation Leak", 3, 7),
    ("Grey Tide", 0, 7),
    ("Brand Intelligence", 2, 6),
    ("Grid Check", 0, 4),
    ("Stray Cargo Pod", 3, 6),
    ("Electrical Storm", 0, 4),
    ("Mass Hallucination", 0, 2),
    ("Ion Storm", 2, 7),
];

/// `summon_round_event`.
fn summon_round_event(world: &mut World, mob: Entity, potency: u32) {
    use crate::events::controller as ec;
    let players = ec::active_players(world);
    let secs = now(world);
    let events = world.resource::<ec::Events>();
    let pool: Vec<usize> = events
        .controls
        .iter()
        .enumerate()
        .filter(|(i, c)| {
            !events.unavailable.contains(i)
                && c.can_spawn(players, secs, false)
                && EVENT_POTENCY
                    .iter()
                    .any(|&(n, min, max)| n == c.name && min <= potency && potency <= max)
        })
        .map(|(i, _)| i)
        .collect();
    if pool.is_empty() {
        chat_to(
            world,
            mob,
            Span::Notice,
            "Печальный свистящий звук издается из руны.",
        );
        return;
    }
    let i = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(0, pool.len() as u32 - 1) as usize;
    let control = pool[i];
    let name = world.resource::<ec::Events>().controls[control].name;
    ec::run_event(world, control, true, None);
    chat_to(
        world,
        mob,
        Span::Notice,
        format!("Ваша выпущенная магия поражает экипаж: {name}!"),
    );
}

fn lerp(a: f32, b: f32, potency: u32) -> f32 {
    a + (b - a) * (potency as f32 * COUNT_LERP).min(1.0)
}

fn rng_range(world: &mut World, low: u32, high: u32) -> u32 {
    world
        .resource_mut::<crate::combat::CombatRng>()
        .range(low, high)
}

/// Free tiles within `radius` of `pos` (`is_blocked_turf(exclude_mobs = TRUE)`), `orange`
/// leaving the centre out.
fn free_tiles(world: &World, pos: TilePos, radius: i32, orange: bool) -> Vec<TilePos> {
    let index = world.resource::<SpatialIndex>();
    let mut out = Vec::new();
    for x in (pos.x - radius)..=(pos.x + radius) {
        for y in (pos.y - radius)..=(pos.y + radius) {
            let tile = TilePos::new(x, y, pos.z);
            if orange && tile == pos {
                continue;
            }
            let blocked = index
                .dense_on_tile(tile)
                .iter()
                .any(|&e| world.get::<Health>(e).is_none());
            if index.in_bounds(tile) && !blocked {
                out.push(tile);
            }
        }
    }
    out
}

/// `trigger_side_effects`.
fn side_effect(world: &mut World, mob: Entity, potency: u32, pos: TilePos) {
    match rng_range(world, 0, 9) {
        0 => anomalies(world, potency, pos),
        1 => {
            world.write_message(crate::emp::Empulse {
                epicenter: pos,
                heavy: lerp(0.0, 3.0, potency).round() as _,
                light: lerp(3.0, 6.0, potency).round() as _,
            });
        }
        2 => translocate(world, pos),
        3 => {
            let range = lerp(2.0, 4.0, potency).round() as i32;
            for tile in free_tiles(world, pos, range, false) {
                crate::janitor::make_slippery(
                    world,
                    tile,
                    crate::janitor::TURF_WET_LUBE,
                    20.0,
                    0.0,
                );
            }
        }
        4 => summon_crewmate(world, mob, pos),
        5 => {
            crate::grenades::do_smoke(world, pos, lerp(2.0, 4.0, potency).round() as _);
        }
        6 => {
            let tiles = free_tiles(world, pos, 2, false);
            for _ in 0..4 {
                if tiles.is_empty() {
                    break;
                }
                let i = rng_range(world, 0, tiles.len() as u32 - 1) as usize;
                crate::effects::gibs::spawn_gibs(
                    world,
                    crate::effects::gibs::Gibspawner::Generic,
                    tiles[i],
                );
            }
        }
        7 => food_rain(world, potency, pos),
        8 => delayed_mobs(world, potency, pos),
        _ => transmogrify(world, potency, pos),
    }
}

/// `transmogrify_area`: a random theme around the rune (no stagger).
fn transmogrify(world: &mut World, potency: u32, pos: TilePos) {
    use crate::events::anomalies::exotic::{THEMES, apply_theme};
    let theme = rng_range(world, 0, THEMES.len() as u32 - 1) as usize;
    let range = lerp(2.0, 4.0, potency).round() as i32;
    for x in (pos.x - range)..=(pos.x + range) {
        for y in (pos.y - range)..=(pos.y + range) {
            apply_theme(world, &THEMES[theme], TilePos::new(x, y, pos.z));
        }
    }
}

fn anomalies(world: &mut World, potency: u32, pos: TilePos) {
    use crate::events::anomalies::{AnomalyKind, spawn_anomaly};
    let kinds = [
        AnomalyKind::Bioscrambler,
        AnomalyKind::Hallucination,
        AnomalyKind::Hallucination,
        AnomalyKind::Grav,
        AnomalyKind::Grav,
        AnomalyKind::Flux,
        AnomalyKind::Flux,
        AnomalyKind::Flux,
    ];
    let mut tiles = free_tiles(world, pos, 4, true);
    let base = rng_range(world, 1, 4) as f32 + lerp(-1.0, 1.0, potency);
    let count = (base.round() as usize).clamp(1, 4).min(kinds.len());
    let t = now(world);
    for &kind in kinds.iter().take(count) {
        if tiles.is_empty() {
            break;
        }
        let i = rng_range(world, 0, tiles.len() as u32 - 1) as usize;
        let tile = tiles.swap_remove(i);
        let lifespan = f64::from(rng_range(world, 15, 30));
        let e = spawn_anomaly(world, kind, tile, None);
        world
            .entity_mut(e)
            .insert(crate::traps::DeleteAt(t + lifespan));
    }
}

/// `translocate`.
fn translocate(world: &mut World, pos: TilePos) {
    let mut mobs: Vec<(Entity, TilePos)> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, 5)
        .filter(|&e| world.get::<Health>(e).is_some())
        .filter_map(|e| world.get::<TilePos>(e).map(|&p| (e, p)))
        .collect();
    mobs.sort_by_key(|(e, _)| *e);
    if mobs.len() < 2 {
        return;
    }
    let mut places: Vec<TilePos> = mobs.iter().map(|(_, p)| *p).collect();
    for i in (1..places.len()).rev() {
        let j = rng_range(world, 0, i as u32) as usize;
        places.swap(i, j);
    }
    for ((mob, _), place) in mobs.into_iter().zip(places) {
        world.entity_mut(mob).insert(place);
    }
}

/// `summon_crewmate`.
fn summon_crewmate(world: &mut World, mob: Entity, pos: TilePos) {
    let our_area = world
        .get_resource::<ss13_mapload::Areas>()
        .and_then(|a| a.area_at(pos));
    let mut humans: Vec<Entity> = world
        .query_filtered::<Entity, (With<HumanLook>, With<PlayerMob>)>()
        .iter(world)
        .filter(|&e| {
            e != mob
                && world
                    .get::<TilePos>(e)
                    .and_then(|&p| world.get_resource::<ss13_mapload::Areas>()?.area_at(p))
                    != our_area
        })
        .collect();
    humans.sort();
    let dests = free_tiles(world, pos, 1, false);
    if humans.is_empty() || dests.is_empty() {
        return;
    }
    let victim = humans[rng_range(world, 0, humans.len() as u32 - 1) as usize];
    let dest = dests[rng_range(world, 0, dests.len() as u32 - 1) as usize];
    if let Some(mut effects) = effects_mut(world, victim) {
        effects.apply(Incap::Immobilize, CREWMATE_SECS as f32);
    }
    let name = crate::medical::name_of(world, victim);
    crate::medical::visible(
        world,
        victim,
        "You feel a tug in your chest, and are lifted upwards into the air!",
        &format!("{name} suddenly floats up into the air!"),
        Span::Warning,
    );
    let t = now(world);
    world.spawn((
        Name::new("grand ritual crewmate pull"),
        RitualTimer::Crewmate {
            victim,
            dest,
            at: t + CREWMATE_SECS,
        },
    ));
}

/// `create_food` of `local_food_rain`.
fn food_rain(world: &mut World, potency: u32, pos: TilePos) {
    let t = now(world);
    let until = t + f64::from(lerp(10.0, 30.0, potency));
    world.spawn((
        Name::new("grand ritual food rain"),
        RitualTimer::Food {
            pos,
            until,
            next: t,
        },
    ));
}

/// `spawn_delayed_mobs`.
fn delayed_mobs(world: &mut World, potency: u32, pos: TilePos) {
    let mut tiles = free_tiles(world, pos, 3, false);
    let path = DELAYED_MOBS[rng_range(world, 0, DELAYED_MOBS.len() as u32 - 1) as usize];
    let t = now(world);
    let mut count = lerp(1.0, 4.0, potency).round() as u32;
    while count > 0 && !tiles.is_empty() {
        count -= 1;
        let i = rng_range(world, 0, tiles.len() as u32 - 1) as usize;
        let tile = tiles.swap_remove(i);
        let at = t + f64::from(rng_range(world, 10, 15));
        world.spawn((
            Name::new("grand ritual portal"),
            RitualTimer::Mob {
                path,
                pos: tile,
                at,
            },
        ));
    }
}

/// `tear_reality`: some of the influences are spent.
fn tear_reality(world: &mut World, potency: u32) {
    let max = match potency {
        0..=2 => 1,
        3..=5 => 2,
        _ => 3,
    };
    let count = rng_range(world, 0, max);
    for _ in 0..count {
        if let Some(pos) = crate::events::electrical_storm::random_station_turf(world) {
            crate::antag::heretic::server::spawn_influence(world, pos);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Finales

fn trigger_finale(world: &mut World, mob: Entity, finale: Finale, pos: TilePos) {
    match finale {
        Finale::AllAccess => all_access(world, mob),
        Finale::Armageddon => armageddon(world, mob, pos),
        Finale::Clown => clown(world, mob),
        Finale::Immortality => immortality(world, mob),
        Finale::Midas => midas(world, pos),
        Finale::Usurp => usurp(world, mob),
        Finale::Cheese => cheese(world, mob),
    }
}

fn human_players(world: &mut World) -> Vec<Entity> {
    let mut humans: Vec<Entity> = world
        .query_filtered::<Entity, (With<HumanLook>, With<PlayerMob>)>()
        .iter(world)
        .collect();
    humans.sort();
    humans
}

/// `SEND_SOUND(world, ...)`.
fn world_sound(world: &mut World, file: &str) {
    if !world.contains_resource::<Messages<ss13_net::sound::server::SendSound>>() {
        return;
    }
    let clients: Vec<Entity> = world
        .query::<&PlayerMob>()
        .iter(world)
        .map(|p| p.client)
        .collect();
    for client in clients {
        world.write_message(ss13_net::sound::server::SendSound {
            client,
            sound: ss13_shared::protocol::SoundEvent {
                file: file.into(),
                volume: 100.0,
                speed: 1.0,
                offset: None,
            },
        });
    }
}

fn all_access(world: &mut World, mob: Entity) {
    let stations = crate::events::controller::station_levels(world);
    let doors: Vec<Entity> = world
        .query_filtered::<(Entity, &TilePos), With<crate::doors::Door>>()
        .iter(world)
        .filter(|(_, p)| stations.contains(&p.z))
        .map(|(e, _)| e)
        .collect();
    for door in doors {
        if let Some(mut d) = world.get_mut::<crate::doors::Door>(door) {
            d.req_access.clear();
            d.req_one_access.clear();
        }
        crate::spells::effects::open_door(world, door);
    }
    let name = crate::medical::name_of(world, mob);
    crate::announce::Priority::new("AULIE OXIN FIERA!!", "")
        .sound("sound/effects/magic/knock.ogg")
        .sender(name)
        .send(world);
}

fn armageddon(world: &mut World, mob: Entity, pos: TilePos) {
    const LAST_WORDS: [&str; 13] = [
        "Flames and ruin!",
        "Dooooooooom!!",
        "HAHAHAHAHAHA!! AHAHAHAHAHAHAHAHAA!!",
        "Hee hee hee!! Hoo hoo hoo!! Ha ha haaa!!",
        "Ohohohohohoho!!",
        "Cower in fear, puny mortals!",
        "Tremble before my glory!",
        "Pick a god and pray!",
        "It's no use!",
        "If the gods wanted you to live, they would not have created me!",
        "God stays in heaven out of fear of what I have created!",
        "Ruination is come!",
        "All of creation, bend to my will!",
    ];
    let words = LAST_WORDS[rng_range(world, 0, 12) as usize];
    let name = crate::medical::name_of(world, mob);
    crate::announce::Priority::new(words, "")
        .sound("sound/effects/magic/voidblink.ogg")
        .sender(name)
        .send(world);
    crate::health::gib(world, mob);
    match rng_range(world, 0, 2) {
        0 => {
            crate::power::singularity::spawn_singularity(
                world,
                crate::power::singularity::SINGULARITY,
                pos,
            );
        }
        1 => {
            crate::power::tesla::effect_tesla(world, pos);
        }
        _ => {
            crate::events::meteors::start_meteor_mode(world, 0.0, 0.5);
            crate::announce::Priority::new(
                "Зафиксировано движение астероидов на встречном со станцией курсе.",
                "Астероиды",
            )
            .send(world);
        }
    }
}

const CRAZY_TEXT: &str = "The world spins and dissolves. Your past flashes before your eyes, \
    backwards. Life strolls back into the ocean and shrinks into nothingness, planets explode \
    into storms of solar dust, the stars rush back to greet each other at the beginning of \
    things and then... you snap back to the present. Everything is just as it was and always \
    has been.";

fn clown(world: &mut World, mob: Entity) {
    for victim in human_players(world) {
        if let Some(mut effects) = effects_mut(world, victim) {
            effects.apply(Incap::Unconscious, 3.0);
        }
        if victim == mob {
            continue;
        }
        chat_to(
            world,
            victim,
            Span::Notice,
            format!("{CRAZY_TEXT} I'm so glad that I work at Clown Research Station!"),
        );
        dress_as_clown(world, victim);
        if world
            .resource_mut::<crate::combat::CombatRng>()
            .chance(15.0)
        {
            vendetta(world, victim, mob);
        }
    }
}

fn dress_as_clown(world: &mut World, victim: Entity) {
    let (uniform, mask) = world
        .get::<Inventory>(victim)
        .map_or((None, None), |i| (i.uniform, i.mask));
    for old in [uniform, mask].into_iter().flatten() {
        world.despawn(old);
    }
    let new_uniform = gain::carried(
        world,
        victim,
        "/obj/item/clothing/under/rank/civilian/clown/magic",
    );
    let new_mask = gain::carried(world, victim, "/obj/item/clothing/mask/gas/clown_hat");
    if let Some(mut inventory) = world.get_mut::<Inventory>(victim) {
        inventory.uniform = new_uniform;
        inventory.mask = new_mask;
    }
}

fn vendetta(world: &mut World, victim: Entity, wizard: Entity) {
    let name = crate::medical::name_of(world, wizard);
    chat_to(world, victim, Span::Warning, "No! This isn't right!");
    chat_to(
        world,
        victim,
        Span::Warning,
        format!("Kill {name}, the one who did this."),
    );
}

fn immortality(world: &mut World, _mob: Entity) {
    world.insert_resource(Immortality);
    let mobs: Vec<Entity> = world
        .query_filtered::<Entity, (With<Health>, With<PlayerMob>)>()
        .iter(world)
        .collect();
    for mob in mobs {
        chat_to(world, mob, Span::Notice, "You feel extremely healthy.");
    }
    world_sound(world, "sound/effects/magic/teleport_diss.ogg");
}

/// `midas/trigger`: the theme spreads over the station by distance.
fn midas(world: &mut World, pos: TilePos) {
    use crate::events::anomalies::exotic;
    let names = ["Gold", "Meat", "Pizza", "Natural"];
    let name = names[rng_range(world, 0, 3) as usize];
    let Some(theme) = exotic::THEMES.iter().position(|t| t.name == name) else {
        return;
    };
    let t = now(world);
    let stations = crate::events::controller::station_levels(world);
    let station_areas: Vec<Entity> = crate::spells::teleport_areas(world)
        .into_iter()
        .map(|(_, a)| a)
        .collect();
    let mut turfs: Vec<(f64, TilePos)> = Vec::new();
    for area in station_areas {
        for tile in crate::spells::effects::area_tiles(world, area) {
            if !stations.contains(&tile.z) {
                continue;
            }
            let dist = pos.chebyshev(tile).unwrap_or(1);
            turfs.push((t + 5.0 * f64::from(dist), tile));
        }
    }
    turfs.sort_by(|a, b| a.0.total_cmp(&b.0));
    world.spawn((
        Name::new("grand ritual midas"),
        MidasSpread {
            theme,
            turfs,
            next: 0,
        },
    ));
}

fn usurp(world: &mut World, mob: Entity) {
    let name = crate::medical::name_of(world, mob);
    world_sound(world, "sound/effects/magic/timeparadox2.ogg");
    for victim in human_players(world) {
        if let Some(mut effects) = effects_mut(world, victim) {
            effects.apply(Incap::Unconscious, 3.0);
        }
        if victim != mob {
            chat_to(
                world,
                victim,
                Span::Notice,
                format!("{CRAZY_TEXT} I'm so glad that {name} is our legally appointed Captain!"),
            );
        }
    }
}

fn cheese(world: &mut World, mob: Entity) {
    crate::announce::Priority::new(
        "Опасность: на станции был вызван чрезвычайно мощный объект, искажающий саму ткань реальности. Рекомендуется немедленная эвакуация. Приготовиться к удару.",
        "Отдел паранормальных явлений",
    )
    .sound("sound/effects/glass/glassbr1.ogg")
    .send(world);
    let name = crate::medical::name_of(world, mob);
    for victim in human_players(world) {
        if victim == mob {
            continue;
        }
        let brain =
            crate::medical::organs::organ(world, victim, crate::medical::organs::Slot::Brain);
        let gained = brain.is_some_and(|brain| {
            crate::medical::traumas::gain(
                world,
                brain,
                crate::medical::traumas::Kind::Hallucinations,
                None,
            )
        });
        if gained {
            chat_to(
                world,
                victim,
                Span::Warning,
                format!("One phrase rebounds ceaselessly in your head: {name} must die."),
            );
            vendetta(world, victim, mob);
        }
    }
    if let Some(staff) = crate::construction::spawn_detached(world, WABBAJACK) {
        gain::give(world, mob, staff);
    }
}

// ---------------------------------------------------------------------------------------------
// Timers

/// The delayed mobs, the food rain, the pulled crewmates, Midas's spread, Perpetuation.
pub(super) fn tick(world: &mut World) {
    let t = now(world);
    let timers: Vec<(Entity, RitualTimer)> = world
        .query::<(Entity, &RitualTimer)>()
        .iter(world)
        .map(|(e, r)| (e, r.clone()))
        .collect();
    for (e, timer) in timers {
        match timer {
            RitualTimer::Mob { path, pos, at } => {
                if t < at {
                    continue;
                }
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(
                        pos,
                        "sound/effects/magic/teleport_app.ogg",
                        60.0,
                        true,
                    ),
                );
                crate::mobs::spawn::spawn_mob(world, path, pos);
                world.despawn(e);
            }
            RitualTimer::Food { pos, until, next } => {
                if t >= until {
                    world.despawn(e);
                    continue;
                }
                if t < next {
                    continue;
                }
                rain_food(world, pos);
                world.entity_mut(e).insert(RitualTimer::Food {
                    pos,
                    until,
                    next: t + 1.0,
                });
            }
            RitualTimer::Crewmate { victim, dest, at } => {
                if t < at {
                    continue;
                }
                if world.get_entity(victim).is_ok() {
                    world.entity_mut(victim).insert(dest);
                }
                world.despawn(e);
            }
        }
    }
    spread_midas(world, t);
    if world.contains_resource::<Immortality>() {
        immortal_deaths(world, t);
    }
}

/// `transform_area` of the rings that are due.
// Cost: one comparison per spread per tick; each turf is converted once.
fn spread_midas(world: &mut World, t: f64) {
    use crate::events::anomalies::exotic::{THEMES, apply_theme};
    let due: Vec<Entity> = world
        .query::<(Entity, &MidasSpread)>()
        .iter(world)
        .filter(|(_, m)| m.turfs.get(m.next).is_some_and(|&(at, _)| at <= t))
        .map(|(e, _)| e)
        .collect();
    for e in due {
        let Some(mut spread) = world.entity_mut(e).take::<MidasSpread>() else {
            continue;
        };
        while let Some(&(at, tile)) = spread.turfs.get(spread.next) {
            if at > t {
                break;
            }
            apply_theme(world, &THEMES[spread.theme], tile);
            spread.next += 1;
        }
        if spread.next < spread.turfs.len() {
            world.entity_mut(e).insert(spread);
        } else {
            world.despawn(e);
        }
    }
}

fn rain_food(world: &mut World, pos: TilePos) {
    let mut tiles = free_tiles(world, pos, 3, false);
    let count = rng_range(world, 0, 3);
    for _ in 0..count {
        if tiles.is_empty() {
            break;
        }
        let i = rng_range(world, 0, tiles.len() as u32 - 1) as usize;
        let tile = tiles.swap_remove(i);
        let path = FOOD[rng_range(world, 0, FOOD.len() as u32 - 1) as usize];
        crate::construction::spawn_obj(world, path, tile);
    }
}

/// `something_died`, `reverse_death`, the spectre's `revive`.
// Cost: while Perpetuation is active, one query of the living per tick; revivals are rare.
fn immortal_deaths(world: &mut World, t: f64) {
    let newly_dead: Vec<(Entity, TilePos)> = world
        .query_filtered::<(Entity, &Stat, &TilePos), (With<Health>, Without<Reviving>)>()
        .iter(world)
        .filter(|(_, s, _)| **s == Stat::Dead)
        .map(|(e, _, p)| (e, *p))
        .collect();
    for (mob, pos) in newly_dead {
        chat_to(
            world,
            mob,
            Span::BoldNotice,
            "Your spirit surges! You will return to life in 60 seconds.",
        );
        world.entity_mut(mob).insert(Reviving {
            spectre: None,
            pos,
            hide_at: t + IMMORTAL_PRE_SECS,
            revive_at: t + IMMORTAL_PRE_SECS + IMMORTAL_REVIVE_SECS,
        });
    }
    let reviving: Vec<(Entity, Reviving)> = world
        .query::<(Entity, &Reviving)>()
        .iter(world)
        .map(|(e, r)| (e, *r))
        .collect();
    for (mob, mut r) in reviving {
        if r.spectre.is_none() && t >= r.hide_at {
            if world.get::<Stat>(mob) != Some(&Stat::Dead) {
                world.entity_mut(mob).remove::<Reviving>();
                continue;
            }
            if let Some(spectre) = crate::construction::spawn_obj(world, SPECTRE, r.pos) {
                crate::closets::put_mob_inside(world, mob, spectre);
                r.spectre = Some(spectre);
                world.entity_mut(mob).insert(r);
            }
            continue;
        }
        if let Some(spectre) = r.spectre
            && t >= r.revive_at
        {
            world.entity_mut(mob).remove::<Reviving>();
            let at = crate::explosions::turf_of(world, spectre).unwrap_or(r.pos);
            world.entity_mut(mob).insert(at);
            crate::closets::let_out(world, mob);
            crate::spells::effects::full_heal(world, mob);
            world.entity_mut(mob).insert(Stat::Conscious);
            crate::ghost::return_to_body(world, mob);
            world.despawn(spectre);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seventh_rune_is_the_finale_once() {
        let mut r = GrandRitual {
            times_completed: 6,
            ..default()
        };
        assert_eq!(rune_type(&r).0, FINALE_RUNE);
        r.cheese_total = CHEESE_GOAL;
        assert_eq!(rune_type(&r), (CHEESY_RUNE, Some(Some(Finale::Cheese))));
        r.drew_finale = true;
        assert_eq!(rune_type(&r).0, RUNE);
        r.times_completed = 3;
        r.drew_finale = false;
        assert_eq!(rune_type(&r).0, RUNE);
    }

    #[test]
    fn areas_follow_the_whitelist() {
        assert!(valid_area("/area/station/medical/chemistry"));
        assert!(!valid_area("/area/station/engineering/supermatter/room"));
        assert!(!valid_area("/area/station/maintenance/port"));
        assert!(valid_area("/area/station/maintenance/disposal/incinerator"));
    }

    #[test]
    fn potency_lerps_to_seven() {
        assert_eq!(lerp(1.0, 4.0, 0), 1.0);
        assert!((lerp(1.0, 4.0, 7) - 4.0).abs() < 1e-3);
    }
}
