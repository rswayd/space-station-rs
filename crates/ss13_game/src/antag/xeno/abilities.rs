//! The aliens' action buttons (`alien_powers.dm`, `larva/powers.dm`, `queen.dm`,
//! `caste/*.dm`), their prompts and their click-to-use forms, the claws on structures
//! (`/obj/attack_alien`) and devouring (`devour_lad`).
//!
//! A button is an [`Ability`]; [`refresh_buttons`] draws those of the caste with their plasma
//! cost and `IsAvailable`. Prompts ([`Pending`], [`answers`]) and aimed clicks
//! ([`aimed_clicks`]) check again at the moment of effect.
// shortcut: no warning prompt before blocking a vent, the larva evolves from a list (tg's radial),
// a devoured mob cannot claw its way out (tg `stomach/relaymove`) and regurgitation lands under
// the alien (no cone), acid only melts objects and non-reinforced walls (no floors), sneaking
// has no sound or step changes, the tail sweep throws everything living in view.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_mapload::ProjectileData;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Layer, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_tgui::server::UiActed;

use super::reproduction::{self, EGG, NEST, RESIN_MEMBRANE, RESIN_WALL, WEED_NODE};
use super::{Aim, Caste, Xeno, adjust_plasma, evolve, exists};
use crate::actions::{ActionButton, ActionButtonsUi, set_buttons};
use crate::click::{ClickIntercept, InterceptedClick};
use crate::closets::InCloset;
use crate::construction::Job;
use crate::health::{DamageType, Health, Stat};
use crate::medical::{name_of, visible};
use crate::server::PlayerMob;
use crate::status_effects::Incap;
use crate::tools::{Actor, DoAfter, do_after, now, tell, tell_mob};

const XENO_DMI: &str = "icons/mob/actions/actions_xeno.dmi";
const HUD_DMI: &str = "icons/hud/screen_alien.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
/// `pounce_cooldown_time`.
pub const POUNCE_COOLDOWN: f64 = 3.0;
/// `MAX_ALIEN_LEAP_DIST`, `pounce_speed`.
const LEAP_RANGE: u32 = 7;
const LEAP_SPEED: f32 = 2.0;
/// `devour_lad`'s `devour_time`, deciseconds.
const DEVOUR_DS: f32 = 135.0;
/// `/datum/action/cooldown/spell/aoe/repulse/xeno` `cooldown_time`.
const TAIL_COOLDOWN: f64 = 15.0;
/// `aoe_radius`.
const TAIL_RADIUS: u32 = 2;
/// `ACID_PROOF`, `UNACIDABLE` of `resistance_flags`.
const ACID_PROOF: u32 = 1 << 5;
const UNACIDABLE: u32 = 1 << 4;
/// `corrosion_acid_power`, `corrosion_acid_volume`, `MOVABLE_ACID_DAMAGE_MAX`.
const ACID_POWER: f32 = 200.0;
const ACID_VOLUME: f32 = 1000.0;
const ACID_DAMAGE_MAX: f32 = 300.0;
/// A turf's `parent_integrity` under acid, seconds.
const TURF_ACID_SECS: f64 = 30.0;
/// `ABOVE_NORMAL_TURF_LAYER`.
const HIDE_LAYER: f32 = 2.08;
const SNEAK_ALPHA: u8 = 25;
/// `promotion_plasma_cost`.
const PROMOTE_COST: f32 = 500.0;
/// `/obj/attack_alien`'s `attack_generic` damage.
const CLAW_DAMAGE: f32 = 60.0;

/// One of the aliens' action buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ability {
    PlantWeeds,
    Transfer,
    Whisper,
    Resin,
    Acid,
    Neurotoxin,
    LayEgg,
    Regurgitate,
    Hide,
    LarvaEvolve,
    EvolvePraetorian,
    EvolveQueen,
    Promote,
    TailSweep,
    Sneak,
    Leap,
}

const ALL: [Ability; 16] = [
    Ability::PlantWeeds,
    Ability::Transfer,
    Ability::Whisper,
    Ability::Resin,
    Ability::Acid,
    Ability::Neurotoxin,
    Ability::LayEgg,
    Ability::Regurgitate,
    Ability::Hide,
    Ability::LarvaEvolve,
    Ability::EvolvePraetorian,
    Ability::EvolveQueen,
    Ability::Promote,
    Ability::TailSweep,
    Ability::Sneak,
    Ability::Leap,
];

impl Ability {
    fn id(self) -> String {
        format!("xeno:{self:?}")
    }

    fn from_id(id: &str) -> Option<Self> {
        ALL.into_iter().find(|a| a.id() == id)
    }

    /// `plasma_cost` (the promotion and the evolutions check their own).
    pub fn cost(self) -> f32 {
        match self {
            Self::PlantWeeds | Self::Neurotoxin => 50.0,
            Self::Whisper => 10.0,
            Self::Resin => 55.0,
            Self::Acid => 200.0,
            Self::LayEgg => 75.0,
            Self::EvolvePraetorian | Self::EvolveQueen => 500.0,
            _ => 0.0,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::PlantWeeds => "Посадить траву",
            Self::Transfer => "Передать плазму",
            Self::Whisper => "Шёпот",
            Self::Resin => "Выделить смолу",
            Self::Acid => "Едкая кислота",
            Self::Neurotoxin => "Плюнуть нейротоксином",
            Self::LayEgg => "Отложить яйцо",
            Self::Regurgitate => "Отрыгнуть",
            Self::Hide => "Спрятаться",
            Self::LarvaEvolve | Self::EvolveQueen => "Эволюционировать",
            Self::EvolvePraetorian => "Эволюционировать в преторианца",
            Self::Promote => "Королевский паразит",
            Self::TailSweep => "Взмах хвостом",
            Self::Sneak => "Скрытность",
            Self::Leap => "Прыжок",
        }
    }

    fn icon(self, active: bool) -> (&'static str, &'static str) {
        let state = match self {
            Self::PlantWeeds => "alien_plant",
            Self::Transfer => "alien_transfer",
            Self::Whisper => "alien_whisper",
            Self::Resin => "alien_resin",
            Self::Acid => "alien_acid",
            Self::Neurotoxin if active => "alien_neurotoxin_1",
            Self::Neurotoxin => "alien_neurotoxin_0",
            Self::LayEgg => "alien_egg",
            Self::Regurgitate => "alien_barf",
            Self::Hide => "alien_hide",
            Self::LarvaEvolve => "alien_evolve_larva",
            Self::EvolvePraetorian => "alien_evolve_drone",
            Self::EvolveQueen => "alien_evolve_praetorian",
            Self::Promote => "alien_queen_promote",
            Self::TailSweep => "tailsweep",
            Self::Sneak => "alien_sneak",
            Self::Leap => return (HUD_DMI, if active { "leap_on" } else { "leap_off" }),
        };
        (XENO_DMI, state)
    }
}

/// The abilities of a caste (its organs' `actions_types` and its innate actions).
pub fn abilities(caste: Caste) -> Vec<Ability> {
    use Ability::*;
    let mut out = vec![Whisper, Transfer];
    if caste == Caste::Larva {
        out.extend([Hide, LarvaEvolve]);
        return out;
    }
    out.extend([PlantWeeds, Regurgitate]);
    match caste {
        Caste::Hunter => out.push(Leap),
        Caste::Sentinel => out.extend([Acid, Neurotoxin, Sneak]),
        Caste::Drone => out.extend([Resin, Acid, EvolvePraetorian]),
        Caste::Praetorian => out.extend([Resin, Acid, Neurotoxin, TailSweep, EvolveQueen]),
        Caste::Queen => out.extend([Resin, Acid, Neurotoxin, LayEgg, Promote]),
        Caste::Larva => {}
    }
    out
}

/// `AB_CHECK_CONSCIOUS`, `AB_CHECK_INCAPACITATED`.
fn can_act(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && world
            .get::<crate::status_effects::Incapacitation>(mob)
            .is_none_or(|i| !i.incapacitated)
}

/// `isturf(owner.loc)`: not in a closet, stomach or pipe.
fn on_turf(world: &World, mob: Entity) -> bool {
    world.get::<InCloset>(mob).is_none()
        && world.get::<crate::ventcrawl::Ventcrawling>(mob).is_none()
        && world.get::<TilePos>(mob).is_some()
}

/// `IsAvailable` of an ability, cost aside.
fn available_now(world: &mut World, mob: Entity, x: &Xeno, a: Ability) -> bool {
    if !can_act(world, mob) || x.plasma < a.cost() {
        return false;
    }
    let cut_off = x.hive_cut_off(now(world));
    match a {
        Ability::PlantWeeds | Ability::Resin | Ability::LayEgg => {
            on_turf(world, mob)
                && world
                    .get::<TilePos>(mob)
                    .is_some_and(|&p| !crate::construction::is_space(world, p))
        }
        Ability::LarvaEvolve => {
            x.amount_grown >= super::MAX_GROWTH
                && world.get::<crate::ventcrawl::Ventcrawling>(mob).is_none()
        }
        Ability::EvolvePraetorian => {
            on_turf(world, mob) && !exists(world, Caste::royal, Some(mob)) && !cut_off
        }
        Ability::EvolveQueen => {
            on_turf(world, mob) && !exists(world, |c| c == Caste::Queen, Some(mob)) && !cut_off
        }
        Ability::Promote => {
            x.plasma >= PROMOTE_COST && !exists(world, |c| c == Caste::Praetorian, None)
        }
        Ability::Neurotoxin => on_turf(world, mob),
        Ability::TailSweep => now(world) >= x.tail_ready_at,
        _ => true,
    }
}

/// Redraws the mob's buttons (plasma, growth, availability).
pub fn refresh_buttons(world: &mut World, mob: Entity) {
    let Some(x) = world.get::<Xeno>(mob).cloned() else {
        return;
    };
    let mut buttons = Vec::new();
    for a in abilities(x.caste) {
        let active = match a {
            Ability::Neurotoxin => x.aim == Some(Aim::Neurotoxin),
            Ability::Acid => x.aim == Some(Aim::Acid),
            Ability::Promote => x.aim == Some(Aim::Promote),
            Ability::Leap => x.leap_on_click,
            Ability::Sneak => x.sneak_alpha.is_some(),
            _ => false,
        };
        let name = if a.cost() > 0.0 {
            format!("{} ({}P)", a.name(), a.cost() as u32)
        } else {
            a.name().to_owned()
        };
        let maptext = match a {
            // `update_plasma_display` on the button that uses plasma most.
            Ability::Transfer => Some(format!("{}", x.plasma.round() as u32)),
            Ability::LarvaEvolve => Some(format!(
                "{}%",
                (x.amount_grown / super::MAX_GROWTH * 100.0).round() as u32
            )),
            _ => None,
        };
        let (dmi, state) = a.icon(active);
        let icon = |world: &World, dmi: &str, state: &str| {
            world
                .get_resource::<IconIndex>()
                .and_then(|i| i.get(dmi, state))
        };
        let ready_at = match a {
            Ability::TailSweep => x.tail_ready_at,
            Ability::Leap => x.leap_ready_at,
            _ => 0.0,
        };
        let available = available_now(world, mob, &x, a);
        let button = ActionButton {
            id: a.id(),
            name,
            icon: icon(world, dmi, state),
            background: icon(world, BACKGROUNDS_DMI, "bg_alien"),
            border: icon(world, BACKGROUNDS_DMI, "bg_alien_border"),
            available,
            cooldown: 0.0,
            active,
            maptext,
        };
        buttons.push((button, ready_at));
    }
    set_buttons(world, mob, buttons);
}

/// The buttons' `trigger` acts.
pub(super) fn trigger_buttons(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        let Some(ability) = Ability::from_id(&id) else {
            continue;
        };
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<Xeno>(actor).is_none()
        {
            continue;
        }
        use_ability(world, client, actor, ability);
    }
}

fn use_ability(world: &mut World, client: Entity, mob: Entity, ability: Ability) {
    let Some(x) = world.get::<Xeno>(mob).cloned() else {
        return;
    };
    if !abilities(x.caste).contains(&ability) || !available_now(world, mob, &x, ability) {
        return;
    }
    match ability {
        Ability::PlantWeeds => {
            place(world, mob, ability, WEED_NODE, "сажает траву ксеноморфов!");
        }
        Ability::LayEgg => {
            place(world, mob, ability, EGG, "откладывает яйцо!");
        }
        Ability::Resin => ask(
            world,
            client,
            mob,
            "Смоляная постройка",
            "Выберите форму.",
            PromptKind::List {
                items: RESIN_SHAPES.iter().map(|s| s.0.to_owned()).collect(),
                default: None,
            },
            Pending::Resin,
        ),
        Ability::Whisper => start_whisper(world, client, mob),
        Ability::Transfer => start_transfer(world, client, mob),
        Ability::Acid => toggle_aim(
            world,
            mob,
            Aim::Acid,
            "Вы готовитесь отрыгнуть кислоту. Выберите цель.",
        ),
        Ability::Neurotoxin => toggle_aim(
            world,
            mob,
            Aim::Neurotoxin,
            "Вы готовите нейротоксиновую железу. Выберите цель.",
        ),
        Ability::Promote => toggle_aim(
            world,
            mob,
            Aim::Promote,
            "Выберите взрослую дочь, чтобы сделать её преторианцем.",
        ),
        Ability::Leap => {
            let on = !x.leap_on_click;
            set_leap(world, mob, on);
            tell_mob(
                world,
                mob,
                Span::Notice,
                if on {
                    "Теперь вы прыгаете на врагов."
                } else {
                    "Теперь вы режете врагов."
                },
            );
        }
        Ability::Regurgitate => regurgitate(world, mob),
        Ability::Hide => hide(world, mob, &x),
        Ability::Sneak => sneak(world, mob, &x),
        Ability::TailSweep => tail_sweep(world, mob),
        Ability::LarvaEvolve => ask(
            world,
            client,
            mob,
            "Эволюция",
            "Выберите касту.",
            PromptKind::List {
                items: CASTE_OPTIONS.iter().map(|s| s.0.to_owned()).collect(),
                default: None,
            },
            Pending::Evolve,
        ),
        Ability::EvolvePraetorian => {
            evolve_caste(world, mob, Caste::Praetorian, ability.cost());
        }
        Ability::EvolveQueen => {
            evolve_caste(world, mob, Caste::Queen, ability.cost());
        }
    }
}

fn evolve_caste(world: &mut World, mob: Entity, caste: Caste, cost: f32) {
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "Вы начинаете эволюционировать!",
        &format!("{user} начинает извиваться и изменяться!"),
        Span::Danger,
    );
    adjust_plasma(world, mob, -cost);
    evolve(world, mob, caste);
}

/// The resin shapes (`structures`).
const RESIN_SHAPES: [(&str, &str); 3] = [
    ("Смоляная стена", RESIN_WALL),
    ("Смоляная мембрана", RESIN_MEMBRANE),
    ("Смоляное гнездо", NEST),
];

const CASTE_OPTIONS: [(&str, Caste); 3] = [
    ("Охотник", Caste::Hunter),
    ("Страж", Caste::Sentinel),
    ("Трутень", Caste::Drone),
];

/// What an open prompt was for.
#[derive(Component, Clone, Debug)]
pub(super) enum Pending {
    Resin,
    Evolve,
    TransferTarget(Vec<Entity>),
    TransferAmount(Entity),
    WhisperTarget(Vec<Entity>),
    WhisperText(Entity),
}

fn ask(
    world: &mut World,
    client: Entity,
    mob: Entity,
    title: &str,
    message: &str,
    kind: PromptKind,
    pending: Pending,
) {
    let prompt = ask_world(
        world,
        Ask::new(client, mob, title.to_owned(), message.to_owned(), kind),
    );
    world.entity_mut(prompt).insert(pending);
}

/// Prompt answers, revalidated at the moment of effect (tg's `QDELETED`/`IsAvailable` checks).
pub(super) fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let Some(messages) = world.get_resource::<Messages<PromptAnswered>>() else {
        return;
    };
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(messages)
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in pending {
        let Some(job) = world.get::<Pending>(prompt).cloned() else {
            continue;
        };
        let Some(answer) = answer else {
            continue;
        };
        if world.get::<Xeno>(mob).is_none()
            || world.get::<PlayerMob>(mob).map(|p| p.client) != Some(client)
        {
            continue;
        }
        apply_answer(world, client, mob, job, answer);
    }
}

fn apply_answer(
    world: &mut World,
    client: Entity,
    mob: Entity,
    job: Pending,
    answer: PromptAnswer,
) {
    let Some(x) = world.get::<Xeno>(mob).cloned() else {
        return;
    };
    match (job, answer) {
        (Pending::Resin, PromptAnswer::Item(i)) => {
            if available_now(world, mob, &x, Ability::Resin)
                && let Some(&(label, path)) = RESIN_SHAPES.get(i)
            {
                place(
                    world,
                    mob,
                    Ability::Resin,
                    path,
                    &format!("придаёт густой фиолетовой смоле форму: {label}!"),
                );
            }
        }
        (Pending::Evolve, PromptAnswer::Item(i)) => {
            if available_now(world, mob, &x, Ability::LarvaEvolve)
                && let Some(&(_, caste)) = CASTE_OPTIONS.get(i)
            {
                evolve_caste(world, mob, caste, 0.0);
            }
        }
        (Pending::TransferTarget(list), PromptAnswer::Item(i)) => {
            let Some(&target) = list.get(i) else {
                return;
            };
            ask(
                world,
                client,
                mob,
                "Передача плазмы",
                "Сколько плазмы передать?",
                PromptKind::Number {
                    default: 0.0,
                    min: 0.0,
                    max: f64::from(x.plasma),
                    round: true,
                },
                Pending::TransferAmount(target),
            );
        }
        (Pending::TransferAmount(target), PromptAnswer::Number(amount)) => {
            finish_transfer(world, client, mob, target, amount as f32);
        }
        (Pending::WhisperTarget(list), PromptAnswer::Item(i)) => {
            let Some(&target) = list.get(i) else {
                return;
            };
            ask(
                world,
                client,
                mob,
                "Шёпот",
                "Что прошептать?",
                PromptKind::Text {
                    default: String::new(),
                    max_length: 1024,
                    multiline: false,
                },
                Pending::WhisperText(target),
            );
        }
        (Pending::WhisperText(target), PromptAnswer::Text(text)) => {
            finish_whisper(world, client, mob, target, text);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------
// Making structures

/// `make_structure/PreActivate` + `Activate`: the duplicate check, then the structure and the
/// plasma.
fn place(world: &mut World, mob: Entity, ability: Ability, path: &str, verb: &str) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let duplicate: Vec<&str> = match ability {
        Ability::Resin => vec![RESIN_WALL, RESIN_MEMBRANE, NEST],
        _ => vec![path],
    };
    let index = world.resource::<ss13_world::SpatialIndex>();
    let tile: Vec<Entity> = index.on_tile(pos).to_vec();
    let blocked = tile.iter().any(|&e| {
        crate::construction::path_of(world, e)
            .is_some_and(|p| duplicate.iter().any(|d| is_under(&p, d)))
    });
    if blocked {
        tell_mob(world, mob, Span::Warning, "Здесь уже есть такая постройка!");
        return;
    }
    if crate::construction::spawn_obj(world, path, pos).is_none() {
        return;
    }
    adjust_plasma(world, mob, -ability.cost());
    let user = name_of(world, mob);
    let others = format!("{user} {verb}");
    visible(world, mob, &others, &others, Span::Notice);
    refresh_buttons(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Whisper and plasma transfer

fn living_in_view(world: &World, mob: Entity, radius: u32) -> Vec<Entity> {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return Vec::new();
    };
    let mut out: Vec<Entity> = world
        .resource::<ss13_world::SpatialIndex>()
        .in_radius(pos, radius)
        .filter(|&e| e != mob && world.get::<Health>(e).is_some())
        .collect();
    out.sort();
    out
}

fn start_whisper(world: &mut World, client: Entity, mob: Entity) {
    let list = living_in_view(world, mob, 7);
    if list.is_empty() {
        tell(
            world,
            client,
            Span::Notice,
            "Вокруг нет никого, кому можно прошептать.",
        );
        return;
    }
    let items = list.iter().map(|&e| name_of(world, e)).collect();
    ask(
        world,
        client,
        mob,
        "Шёпот",
        "Кому прошептать?",
        PromptKind::List {
            items,
            default: None,
        },
        Pending::WhisperTarget(list),
    );
}

fn finish_whisper(world: &mut World, client: Entity, mob: Entity, target: Entity, text: String) {
    let Some(x) = world.get::<Xeno>(mob).cloned() else {
        return;
    };
    let text = text.trim().to_owned();
    if text.is_empty()
        || world.get::<Health>(target).is_none()
        || !available_now(world, mob, &x, Ability::Whisper)
    {
        return;
    }
    adjust_plasma(world, mob, -Ability::Whisper.cost());
    tell_mob(
        world,
        target,
        Span::Notice,
        format!("Вы слышите странный чужой голос у себя в голове... {text}"),
    );
    let victim = name_of(world, target);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы сказали: \"{text}\" {victim}"),
    );
    let (user, whisper) = (name_of(world, mob), format!("(шёпот) {text}"));
    for ghost in crate::ghost_poll::ghosts(world) {
        tell_mob(
            world,
            ghost,
            Span::Notice,
            format!("{user} -> {victim}: {whisper}"),
        );
    }
}

fn start_transfer(world: &mut World, client: Entity, mob: Entity) {
    let list: Vec<Entity> = living_in_view(world, mob, 7)
        .into_iter()
        .filter(|&e| super::is_xeno(world, e))
        .collect();
    if list.is_empty() {
        tell(world, client, Span::Notice, "Рядом нет других пришельцев.");
        return;
    }
    let items = list.iter().map(|&e| name_of(world, e)).collect();
    ask(
        world,
        client,
        mob,
        "Передача плазмы",
        "Кому передать плазму?",
        PromptKind::List {
            items,
            default: None,
        },
        Pending::TransferTarget(list),
    );
}

fn finish_transfer(world: &mut World, client: Entity, mob: Entity, target: Entity, amount: f32) {
    let Some(x) = world.get::<Xeno>(mob).cloned() else {
        return;
    };
    if amount <= 0.0
        || amount > x.plasma
        || !super::is_xeno(world, target)
        || !can_act(world, mob)
        || world.get::<Stat>(target) == Some(&Stat::Dead)
    {
        return;
    }
    let near = crate::explosions::turf_of(world, mob)
        .zip(crate::explosions::turf_of(world, target))
        .and_then(|(a, b)| a.chebyshev(b))
        .is_some_and(|d| d <= 1);
    if !near {
        tell(world, client, Span::Notice, "Нужно подойти ближе!");
        return;
    }
    adjust_plasma(world, target, amount);
    adjust_plasma(world, mob, -amount);
    let (user, other) = (name_of(world, mob), name_of(world, target));
    tell_mob(
        world,
        target,
        Span::Notice,
        format!("{user} передаёт вам {amount} плазмы."),
    );
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы передаёте {amount} плазмы {other}."),
    );
    refresh_buttons(world, mob);
    refresh_buttons(world, target);
}

// ---------------------------------------------------------------------------------------------
// Aimed abilities

fn toggle_aim(world: &mut World, mob: Entity, aim: Aim, text: &str) {
    if world.get::<Xeno>(mob).and_then(|x| x.aim) == Some(aim) {
        set_aim(world, mob, None);
    } else {
        set_aim(world, mob, Some(aim));
        tell_mob(world, mob, Span::Notice, text);
    }
}

pub(super) fn set_aim(world: &mut World, mob: Entity, aim: Option<Aim>) {
    if let Some(mut x) = world.get_mut::<Xeno>(mob) {
        x.aim = aim;
    }
    if aim.is_some() {
        world.entity_mut(mob).insert(ClickIntercept);
    } else {
        world.entity_mut(mob).remove::<ClickIntercept>();
    }
    refresh_buttons(world, mob);
}

/// `toggle_leap`: the hunter's clicks leap (at any range) while it is on.
pub(super) fn set_leap(world: &mut World, mob: Entity, on: bool) {
    if let Some(mut x) = world.get_mut::<Xeno>(mob) {
        x.leap_on_click = on;
    }
    set_aim(world, mob, on.then_some(Aim::Leap));
}

/// A step one tile from `from` towards `to`.
pub(super) fn dir_towards(from: TilePos, to: TilePos) -> Dir {
    crate::pull::dir_between(from, to).unwrap_or_default()
}

/// A mob thrown from `from` along `dir` (tg `throw_at` of living things).
pub(super) fn fling(world: &mut World, target: Entity, from: TilePos, dir: (i32, i32), range: u32) {
    let to = TilePos::new(
        from.x + dir.0 * range as i32,
        from.y + dir.1 * range as i32,
        from.z,
    );
    start_throw(world, target, from, to, range, 1.0, None, None);
}

fn start_throw(
    world: &mut World,
    mob: Entity,
    from: TilePos,
    to: TilePos,
    range: u32,
    speed: f32,
    thrower: Option<Entity>,
    target: Option<Entity>,
) {
    let t_ds = now(world) * 10.0;
    let Some(path) = crate::throw::Trajectory::new(from, to, range, speed, t_ds) else {
        return;
    };
    world.entity_mut(mob).insert((
        crate::throw::Thrown {
            path,
            thrower,
            target,
            last_dir: None,
        },
        crate::throw::ThrownMob { extra_speed: 0.0 },
    ));
}

/// The clicks of an alien with a pointed ability armed (`InterceptClickOn`).
pub(super) fn aimed_clicks(world: &mut World, mut cursor: Local<MessageCursor<InterceptedClick>>) {
    let clicks: Vec<InterceptedClick> = cursor
        .read(world.resource::<Messages<InterceptedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(x) = world.get::<Xeno>(click.mob).cloned() else {
            continue;
        };
        let Some(aim) = x.aim else {
            continue;
        };
        if world.get::<PlayerMob>(click.mob).map(|p| p.client) != Some(click.client)
            || !can_act(world, click.mob)
        {
            continue;
        }
        match aim {
            Aim::Acid => acid_click(world, click, &x),
            Aim::Neurotoxin => spit(world, click, &x),
            Aim::Leap => leap(world, click, &x),
            Aim::Promote => promote(world, click, &x),
        }
    }
}

/// `/obj/projectile/neurotoxin`.
fn neurotoxin() -> ProjectileData {
    ProjectileData {
        path: "/obj/projectile/neurotoxin".to_owned(),
        damage: 65.0,
        damage_type: "stamina".to_owned(),
        armor_flag: "bio".to_owned(),
        armour_penetration: 50.0,
        speed: 1.25,
        range: 50.0,
        pass_flags: crate::projectile::PASSTABLE,
        accurate_range: 100.0,
        accuracy_falloff: 7.0,
        ..default()
    }
}

/// `neurotoxin/InterceptClickOn`.
fn spit(world: &mut World, click: InterceptedClick, x: &Xeno) {
    if x.plasma < Ability::Neurotoxin.cost() || !on_turf(world, click.mob) {
        set_aim(world, click.mob, None);
        return;
    }
    adjust_plasma(world, click.mob, -Ability::Neurotoxin.cost());
    let data = neurotoxin();
    crate::projectile::fire(
        world,
        crate::projectile::Shot {
            data: &data,
            firer: click.mob,
            from: click.mob_pos,
            target: Some(click.target),
            target_pos: click.target_pos,
            zone: crate::zone::BodyZone::Chest,
            hit_prone: false,
            pixel: [0, 0],
            deviation: 0.0,
            fired_from: None,
        },
    );
    let user = name_of(world, click.mob);
    visible(
        world,
        click.mob,
        "Вы плюёте нейротоксином.",
        &format!("{user} плюёт нейротоксином!"),
        Span::Danger,
    );
    // "It'd be really annoying if click-to-fire actions stayed active".
    if world
        .get::<Xeno>(click.mob)
        .is_some_and(|x| x.plasma < Ability::Neurotoxin.cost())
    {
        set_aim(world, click.mob, None);
    } else {
        refresh_buttons(world, click.mob);
    }
}

/// `corrosion/PreActivate` + `Activate`: an adjacent object or wall, 200 plasma; mobs are spared.
fn acid_click(world: &mut World, click: InterceptedClick, x: &Xeno) {
    let near = click
        .mob_pos
        .chebyshev(click.target_pos)
        .is_some_and(|d| d <= 1);
    if !near || x.plasma < Ability::Acid.cost() {
        return;
    }
    if world.get::<Health>(click.target).is_some() {
        tell_mob(world, click.mob, Span::Warning, "Не действует на существ!");
        return;
    }
    if !corrode(world, click.target) {
        tell_mob(
            world,
            click.mob,
            Span::Notice,
            "Вы не можете это растворить.",
        );
        return;
    }
    adjust_plasma(world, click.mob, -Ability::Acid.cost());
    let user = name_of(world, click.mob);
    let what = name_of(world, click.target);
    visible(
        world,
        click.mob,
        &format!("Вы изрыгаете кислоту на {what}. Оно начинает шипеть и плавиться."),
        &format!("{user} извергает мерзкую жижу на {what}. Оно начинает шипеть и плавиться!"),
        Span::Notice,
    );
    if world
        .get::<Xeno>(click.mob)
        .is_some_and(|x| x.plasma < Ability::Acid.cost())
    {
        set_aim(world, click.mob, None);
    } else {
        refresh_buttons(world, click.mob);
    }
}

/// Melting `/datum/component/acid`.
#[derive(Component, Clone, Copy, Debug)]
pub(super) struct Corrosion {
    volume: f32,
    /// A wall's `parent_integrity`, the game time it collapses at.
    melts_at: Option<f64>,
    tick_at: f64,
}

/// `acid_act(corrosion_acid_power, corrosion_acid_volume)`: objects that have integrity, and
/// walls that are not reinforced. False when it cannot be dissolved.
fn corrode(world: &mut World, target: Entity) -> bool {
    let t = now(world);
    if world.get::<Corrosion>(target).is_some() {
        return true;
    }
    if let Some(obj) = world.get::<ss13_mapload::DmObj>(target) {
        if obj.resistance_flags & (ACID_PROOF | UNACIDABLE) != 0
            || world.get::<crate::obj_damage::Integrity>(target).is_none()
        {
            return false;
        }
        world.entity_mut(target).insert(Corrosion {
            volume: ACID_VOLUME,
            melts_at: None,
            tick_at: t + 1.0,
        });
        return true;
    }
    let wall = crate::construction::path_of(world, target).is_some_and(|p| {
        is_under(&p, "/turf/closed/wall")
            && !is_under(&p, "/turf/closed/wall/r_wall")
            && !is_under(&p, "/turf/closed/wall/rust")
    });
    if !wall || world.get::<ss13_mapload::Turf>(target).is_none() {
        return false;
    }
    world.entity_mut(target).insert(Corrosion {
        volume: ACID_VOLUME,
        melts_at: Some(t + TURF_ACID_SECS),
        tick_at: t + 1.0,
    });
    true
}

/// Each second: damage to an object until it is gone or the acid runs out; a wall's collapse.
pub(super) fn corrosion_tick(world: &mut World) {
    let t = now(world);
    let burning: Vec<(Entity, Corrosion)> = world
        .query::<(Entity, &Corrosion)>()
        .iter(world)
        .filter(|(_, c)| t >= c.tick_at)
        .map(|(e, c)| (e, *c))
        .collect();
    for (target, c) in burning {
        if let Some(at) = c.melts_at {
            if t >= at {
                if let Some(&pos) = world.get::<TilePos>(target) {
                    crate::construction::dismantle_wall(world, target, pos);
                }
                continue;
            }
            if let Some(mut c) = world.get_mut::<Corrosion>(target) {
                c.tick_at = t + 1.0;
            }
            continue;
        }
        let damage = (1.0 + (ACID_POWER * c.volume).sqrt() * 0.3).min(ACID_DAMAGE_MAX);
        let hit = crate::obj_damage::take_damage(world, target, damage, DamageType::Burn);
        let volume = c.volume - (1.0 + c.volume.sqrt());
        if hit.destroyed || volume <= 0.0 {
            world.entity_mut(target).remove::<Corrosion>();
            continue;
        }
        if let Some(mut c) = world.get_mut::<Corrosion>(target) {
            c.volume = volume;
            c.tick_at = t + 1.0;
        }
    }
}

/// `hunter/leap_at`: the click at any range starts the throw.
fn leap(world: &mut World, click: InterceptedClick, x: &Xeno) {
    let t = now(world);
    if x.leaping.is_some() || !on_turf(world, click.mob) {
        return;
    }
    if t < x.leap_ready_at {
        tell_mob(
            world,
            click.mob,
            Span::Warning,
            "Вы слишком устали для прыжка!",
        );
        return;
    }
    let incap = world
        .get::<crate::status_effects::Incapacitation>(click.mob)
        .is_some_and(|i| i.floored || i.immobilized || i.resting);
    if incap {
        return;
    }
    let victim = world.get::<Health>(click.target).map(|_| click.target);
    start_throw(
        world,
        click.mob,
        click.mob_pos,
        click.target_pos,
        LEAP_RANGE,
        LEAP_SPEED,
        Some(click.mob),
        victim,
    );
    if let Some(mut x) = world.get_mut::<Xeno>(click.mob) {
        x.leaping = Some((victim, click.target_pos));
    }
}

/// `queen_promotion/attack`: an adult, non-royal, played daughter becomes a praetorian.
fn promote(world: &mut World, click: InterceptedClick, x: &Xeno) {
    set_aim(world, click.mob, None);
    if x.plasma < PROMOTE_COST || exists(world, |c| c == Caste::Praetorian, None) {
        tell_mob(
            world,
            click.mob,
            Span::Notice,
            "Сейчас вы не можете никого повысить!",
        );
        return;
    }
    let Some(daughter) = world.get::<Xeno>(click.target).map(|d| d.caste) else {
        tell_mob(
            world,
            click.mob,
            Span::Notice,
            "Этим можно повысить только взрослых нецарственных дочерей!",
        );
        return;
    };
    if !daughter.adult()
        || daughter.royal()
        || world.get::<Stat>(click.target) != Some(&Stat::Conscious)
        || world.get::<PlayerMob>(click.target).is_none()
    {
        tell_mob(
            world,
            click.mob,
            Span::Notice,
            "Этим можно повысить только взрослых нецарственных дочерей!",
        );
        return;
    }
    let near = click
        .mob_pos
        .chebyshev(click.target_pos)
        .is_some_and(|d| d <= 1);
    if !near {
        return;
    }
    adjust_plasma(world, click.mob, -PROMOTE_COST);
    let (user, target) = (name_of(world, click.mob), name_of(world, click.target));
    visible(
        world,
        click.mob,
        &format!("Вы возвели {target} в преторианцы!"),
        &format!("{user} возводит {target} в преторианцы!"),
        Span::Danger,
    );
    tell_mob(
        world,
        click.target,
        Span::Notice,
        "Королева пожаловала вам повышение до преторианца!",
    );
    evolve(world, click.target, Caste::Praetorian);
}

// ---------------------------------------------------------------------------------------------
// The rest

fn regurgitate(world: &mut World, mob: Entity) {
    let contents = world
        .query::<&InCloset>()
        .iter(world)
        .any(|c| c.closet == mob);
    if !contents {
        tell_mob(
            world,
            mob,
            Span::Notice,
            "В вашем желудке пусто, вам нечего отрыгнуть.",
        );
        return;
    }
    crate::antag::morph::barf(world, mob);
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "Вы извергаете содержимое желудка!",
        &format!("{user} извергает содержимое желудка!"),
        Span::Danger,
    );
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(
                pos,
                "sound/mobs/non-humanoids/alien/alien_york.ogg",
                100.0,
                false,
            ),
        );
    }
}

/// `alien/hide`: down to the floor layer and back.
fn hide(world: &mut World, mob: Entity, x: &Xeno) {
    let user = name_of(world, mob);
    let Some(mut look) = world.get::<Appearance>(mob).cloned() else {
        return;
    };
    let current = look.layer.0;
    let hidden = x.hidden_layer;
    match hidden {
        Some(layer) => look.layer = Layer(layer),
        None => look.layer = Layer(HIDE_LAYER),
    }
    world.entity_mut(mob).insert(look);
    if let Some(mut x) = world.get_mut::<Xeno>(mob) {
        x.hidden_layer = match hidden {
            Some(_) => None,
            None => Some(current),
        };
    }
    let (own, others) = if hidden.is_some() {
        (
            "Вы перестаёте прятаться.",
            format!("{user} медленно поднимается с земли..."),
        )
    } else {
        ("Вы прячетесь.", format!("{user} прижимается к земле!"))
    };
    visible(world, mob, own, &others, Span::Notice);
}

/// `mob_cooldown/sneak`: `sneak_alpha` while on.
fn sneak(world: &mut World, mob: Entity, x: &Xeno) {
    let Some(mut look) = world.get::<Appearance>(mob).cloned() else {
        return;
    };
    let [r, g, b, a] = look.color.unwrap_or([255; 4]);
    let was = x.sneak_alpha;
    let (new, alpha, text) = match was {
        Some(old) => (None, old, "Вы перестаёте красться."),
        None => (Some(a), SNEAK_ALPHA, "Вы растворяетесь в тенях."),
    };
    look.color = Some([r, g, b, alpha]);
    world.entity_mut(mob).insert(look);
    if let Some(mut x) = world.get_mut::<Xeno>(mob) {
        x.sneak_alpha = new;
    }
    tell_mob(world, mob, Span::Notice, text);
    refresh_buttons(world, mob);
}

/// `/datum/action/cooldown/spell/aoe/repulse/xeno` (`cast_on_thing_in_aoe`): those on the tile
/// are floored, the others stunned and thrown back.
fn tail_sweep(world: &mut World, mob: Entity) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let t = now(world);
    if let Some(mut x) = world.get_mut::<Xeno>(mob) {
        x.tail_ready_at = t + TAIL_COOLDOWN;
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/effects/magic/tail_swing.ogg", 80.0, true),
    );
    let victims: Vec<Entity> = living_in_view(world, mob, TAIL_RADIUS)
        .into_iter()
        .filter(|&e| !super::is_xeno(world, e))
        .collect();
    for victim in victims {
        let Some(&vpos) = world.get::<TilePos>(victim) else {
            continue;
        };
        if vpos == pos {
            crate::status_effects::apply(world, victim, Incap::Paralyze, 10.0);
            crate::mobs::defense::hurt(
                world,
                victim,
                DamageType::Brute,
                5.0,
                Some(crate::zone::BodyZone::Chest),
            );
            tell_mob(world, victim, Span::BoldWarning, "Вас бросает на пол!");
            continue;
        }
        crate::status_effects::apply(world, victim, Incap::Paralyze, 4.0);
        tell_mob(world, victim, Span::BoldWarning, "Вас отбрасывает назад!");
        let d = vpos.chebyshev(pos).unwrap_or(1) as i32;
        let range = (5 - (d - 2).max(0)).clamp(3, 5) as u32;
        fling(
            world,
            victim,
            vpos,
            ((vpos.x - pos.x).signum(), (vpos.y - pos.y).signum()),
            range,
        );
    }
    refresh_buttons(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Structures and devouring

/// A click on what is not alive: eggs, nests, huggers, airlocks; else the claws.
pub(super) fn structure_click(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    caste: Caste,
) {
    if reproduction::xeno_click(world, client, mob, target, caste) {
        return;
    }
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    if caste.adult() && crate::doors::alien_pry(world, actor, target) {
        return;
    }
    // `/obj/attack_alien` (`attack_generic`), not for loose items.
    if world.get::<ss13_mapload::DmItem>(target).is_some() {
        return;
    }
    if world.get::<crate::obj_damage::Integrity>(target).is_none() {
        return;
    }
    let t = now(world);
    world
        .entity_mut(mob)
        .insert(crate::combat::NextMove(t + crate::combat::CLICK_CD_MELEE));
    crate::obj_damage::take_damage(world, target, CLAW_DAMAGE, DamageType::Brute);
    if let Some(&pos) = world.get::<TilePos>(target) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/items/weapons/slash.ogg", 100.0, true),
        );
    }
}

/// The timed steps of aliens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XenoJob {
    Devour,
    NestEscape,
}

/// Out of combat mode, a click on a grabbed victim swallows it, a held hugger leaps at it.
pub(super) fn try_hug_or_devour(world: &mut World, mob: Entity, target: Entity) -> bool {
    if reproduction::hug_with_carried(world, mob, target) {
        return true;
    }
    can_consume(world, mob, target) && start_devour(world, mob, target)
}

/// `can_consume`.
fn can_consume(world: &World, mob: Entity, target: Entity) -> bool {
    world.get::<Xeno>(mob).is_some_and(|x| x.caste.adult())
        && world.get::<crate::pull::Pulling>(mob).map(|p| p.0) == Some(target)
        && world
            .get::<crate::pull::GrabState>(mob)
            .is_some_and(|g| *g >= crate::pull::GrabState::Aggressive)
        && can_act(world, mob)
}

fn start_devour(world: &mut World, mob: Entity, target: Entity) -> bool {
    let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) else {
        return false;
    };
    let (user, victim) = (name_of(world, mob), name_of(world, target));
    visible(
        world,
        mob,
        &format!("Вы пытаетесь поглотить {victim}!"),
        &format!("{user} пытается поглотить {victim}!"),
        Span::Danger,
    );
    if let Some(&pos) = world.get::<TilePos>(target) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(
                pos,
                "sound/mobs/non-humanoids/alien/alien_eat.ogg",
                100.0,
                false,
            ),
        );
    }
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    do_after(
        world,
        actor,
        target,
        DEVOUR_DS,
        0,
        Job::Xeno(XenoJob::Devour),
    )
}

/// `complete` of [`XenoJob`].
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: XenoJob) {
    match job {
        XenoJob::Devour => complete_devour(world, mob, action.target),
        XenoJob::NestEscape => reproduction::complete_escape(world, mob, action.target),
    }
}

/// The end of `devour_lad`.
fn complete_devour(world: &mut World, mob: Entity, target: Entity) {
    if !can_consume(world, mob, target) {
        return;
    }
    let (user, victim) = (name_of(world, mob), name_of(world, target));
    world.entity_mut(mob).remove::<crate::pull::Pulling>();
    crate::closets::put_mob_inside(world, target, mob);
    visible(
        world,
        mob,
        &format!("Вы поглощаете {victim}!"),
        &format!("{user} поглощает {victim}!"),
        Span::Danger,
    );
    tell_mob(
        world,
        target,
        Span::BoldWarning,
        format!("{user} поглощает вас!"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_caste_has_the_abilities_of_its_organs() {
        let drone = abilities(Caste::Drone);
        assert!(drone.contains(&Ability::Resin));
        assert!(drone.contains(&Ability::Acid));
        assert!(!drone.contains(&Ability::Neurotoxin));
        assert!(abilities(Caste::Queen).contains(&Ability::LayEgg));
        assert!(abilities(Caste::Hunter).contains(&Ability::Leap));
        let larva = abilities(Caste::Larva);
        assert_eq!(
            larva,
            [
                Ability::Whisper,
                Ability::Transfer,
                Ability::Hide,
                Ability::LarvaEvolve
            ]
        );
    }

    #[test]
    fn ids_round_trip() {
        for a in ALL {
            assert_eq!(Ability::from_id(&a.id()), Some(a));
        }
    }
}
