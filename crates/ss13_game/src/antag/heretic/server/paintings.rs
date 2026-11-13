//! The eldritch paintings (`items/eldritch_painting.dm`, `status_effects/debuffs.dm`
//! `/datum/status_effect/eldritch_painting`, the moodlets of `eldritch_painting_events.dm`).
//!
//! The Unsealed Painting ritual turns a canvas and an extra item into a painting's wall frame
//! ([`painting_recipe`]); the frame hangs like other frames (`crate::wallmount::frames`). A hung
//! painting curses the crew (not heretics) that comes into its view within 7 tiles for ten
//! minutes, unless holy water is in their blood (which also wears the curse off). Examining it
//! gives a heretic its boon and a victim three minutes of respite; wirecutters take it down.
//! The vines painting has no curse: it grows kudzu when hung and when a non-heretic examines it.
// shortcut: the curse is checked once a second for whoever is in view (tg: on entering the
// range); the weeping curse is a short hallucination (no heretic delusion preset), the rusting
// curse does not rust floors (no `rust_heretic_act`), the beauty painting does not mutate
// non-heretics (no random mutations) and scratches without picking a clothed bodypart; no
// status alerts; no magic resistance.

use bevy::ecs::message::Messages;

use super::*;
use crate::click::ExamineClick;
use crate::mood::MoodEvent;

const PAINTING_ROOT: &str = "/obj/structure/sign/painting/eldritch";
/// `/datum/status_effect/eldritch_painting` duration and the examine respite.
const CURSE_SECS: f64 = 600.0;
const EXAMINE_SECS: f64 = 180.0;
const RANGE: i32 = 7;
const HOLY_WATER: &str = "/datum/reagent/water/holywater";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintingKind {
    Weeping,
    Desire,
    Vines,
    Beauty,
    Rust,
}

impl PaintingKind {
    fn of(path: &str) -> Option<Self> {
        let rest = path.strip_prefix(PAINTING_ROOT)?;
        Some(match rest {
            "/weeping" => Self::Weeping,
            "/desire" => Self::Desire,
            "/vines" => Self::Vines,
            "/beauty" => Self::Beauty,
            "/rust" => Self::Rust,
            _ => return None,
        })
    }

    /// `text_to_display`.
    fn text(self) -> &'static str {
        match self {
            Self::Weeping => "О, какое искусство! Она такая прекрасная, а он... ОН ПЛАЧЕТ!!!",
            Self::Desire => "Какое произведение, от одного взгляда на него хочется есть...",
            Self::Beauty => {
                "Ее плоть сияет в бледном свете, и моя тоже бы могла... Если бы не все эти \
                 недостатки..."
            }
            Self::Rust => "Ржавчина разрушается. Мастер поднимается. Он зовет. Ты отвечаешь...",
            Self::Vines => "",
        }
    }

    /// `tick_interval` of the curse.
    fn tick_secs(self) -> f64 {
        match self {
            Self::Weeping => 10.0,
            Self::Beauty | Self::Rust => 3.0,
            Self::Desire | Self::Vines => 1.0,
        }
    }
}

/// A hung `/obj/structure/sign/painting/eldritch/*`.
#[derive(Component, Clone, Copy, Debug)]
pub struct EldritchPainting(pub PaintingKind);

/// `/datum/status_effect/eldritch_painting/*`.
#[derive(Component, Clone, Copy, Debug)]
pub struct PaintingCurse {
    pub kind: PaintingKind,
    pub until: f64,
    pub next_tick: f64,
}

/// `TRAIT_ELDRITCH_PAINTING_EXAMINE`: no curse ticks and no new examine effects until then.
#[derive(Component, Clone, Copy, Debug)]
pub struct PaintingExamined {
    pub until: f64,
}

const fn mood(id: &'static str, description: &'static str, change: f32) -> MoodEvent {
    MoodEvent {
        id,
        description,
        mood_change: change,
        timeout: 180.0,
        hidden: false,
    }
}

const RIPPED: MoodEvent = mood(
    "eldritch_painting",
    "Я всё слышу этот странный смех с того момента, как я снял ту картину...",
    -6.0,
);
const WEEPING: MoodEvent = MoodEvent {
    timeout: 11.0,
    ..mood("eldritch_painting/weeping", "Он здесь!", -3.0)
};
const WEEPING_HERETIC: MoodEvent = mood(
    "eldritch_painting/weeping_heretic",
    "Его страдания воодушляют меня!",
    5.0,
);
const WEEPING_WITHDRAWAL: MoodEvent = mood(
    "eldritch_painting/weeping_withdrawal",
    "Мой разум чист. Его здесь нет.",
    1.0,
);
const DESIRE_HERETIC: MoodEvent = mood("eldritch_painting/desire_heretic", "Пустота кричит.", -2.0);
const DESIRE_EXAMINE: MoodEvent = mood(
    "eldritch_painting/desire_examine",
    "Голод утолен, пока что...",
    3.0,
);
const HERETIC_VINES: MoodEvent = mood(
    "eldritch_painting/heretic_vines",
    "О, какой прекрасный цветок!",
    3.0,
);
const RUST_EXAMINE: MoodEvent = mood(
    "eldritch_painting/rust_examine",
    "Та картина была довольно стремной.",
    -2.0,
);
const RUST_HERETIC: MoodEvent = mood(
    "eldritch_painting/rust_heretic_examine",
    "Подъем. Разложение. Ржавчина.",
    6.0,
);

/// What the Desire painting gives a heretic.
const DESIRE_GIFTS: [&str; 12] = [
    "/obj/item/organ/brain",
    "/obj/item/organ/lungs",
    "/obj/item/organ/eyes",
    "/obj/item/organ/ears",
    "/obj/item/organ/heart",
    "/obj/item/organ/liver",
    "/obj/item/organ/stomach",
    "/obj/item/organ/appendix",
    "/obj/item/bodypart/arm/left",
    "/obj/item/bodypart/arm/right",
    "/obj/item/bodypart/leg/left",
    "/obj/item/bodypart/leg/right",
];
const VINE_GIFTS: [&str; 2] = [
    "/obj/item/food/grown/flower/poppy",
    "/obj/item/food/grown/flower/harebell",
];
/// The vines painting's kudzu.
const VINE_MUTATIONS: [crate::events::space_vines::Mutation; 5] = {
    use crate::events::space_vines::Mutation::*;
    [AggressiveSpread, FireProof, Hardened, Thorns, Toxicity]
};

type Recipe = (
    &'static [(&'static [&'static str], u32)],
    &'static [&'static str],
);

/// `painting/recipe_snowflake_check`: the canvas and the first extra item found pick the
/// painting.
pub fn painting_recipe(atoms: &[Candidate<Entity>]) -> Option<Recipe> {
    const OPTIONS: [(&str, Recipe); 5] = [
        (
            "/obj/item/organ/eyes",
            (
                &[(&["/obj/item/canvas"], 1), (&["/obj/item/organ/eyes"], 1)],
                &["/obj/item/wallframe/painting/eldritch/weeping"],
            ),
        ),
        (
            "/obj/item/bodypart",
            (
                &[(&["/obj/item/canvas"], 1), (&["/obj/item/bodypart"], 1)],
                &["/obj/item/wallframe/painting/eldritch/desire"],
            ),
        ),
        (
            "/obj/item/food/grown",
            (
                &[(&["/obj/item/canvas"], 1), (&["/obj/item/food/grown"], 1)],
                &["/obj/item/wallframe/painting/eldritch/vines"],
            ),
        ),
        (
            "/obj/item/clothing/gloves",
            (
                &[
                    (&["/obj/item/canvas"], 1),
                    (&["/obj/item/clothing/gloves"], 1),
                ],
                &["/obj/item/wallframe/painting/eldritch/beauty"],
            ),
        ),
        (
            "/obj/item/trash",
            (
                &[(&["/obj/item/canvas"], 1), (&["/obj/item/trash"], 1)],
                &["/obj/item/wallframe/painting/eldritch/rust"],
            ),
        ),
    ];
    OPTIONS
        .iter()
        .find(|(extra, _)| atoms.iter().any(|a| knowledge::is_type(&a.path, extra)))
        .map(|(_, recipe)| *recipe)
}

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (tag_paintings, paintings_tick, painting_examines)
            .chain()
            .after(crate::click::ResolveClicks)
            .before(crate::health::HealthSystems),
    );
}

/// New paintings get their kind; the vines painting sprouts kudzu (`vines/Initialize`).
fn tag_paintings(
    mut commands: Commands,
    q: Query<(Entity, &ss13_mapload::DmObj, Option<&TilePos>), Added<ss13_mapload::DmObj>>,
) {
    for (e, obj, pos) in &q {
        let Some(kind) = PaintingKind::of(&obj.path) else {
            continue;
        };
        commands.entity(e).insert(EldritchPainting(kind));
        if kind == PaintingKind::Vines
            && let Some(&pos) = pos
        {
            commands.queue(move |world: &mut World| {
                crate::events::space_vines::spawn_controller(
                    world,
                    pos,
                    &VINE_MUTATIONS,
                    Some(0.0),
                    Some(10.0),
                    false,
                );
            });
        }
    }
}

fn holy(world: &World, mob: Entity) -> bool {
    crate::chem::blood_amount(world, mob, HOLY_WATER) > 0.0
}

/// Once a second: who sees a painting is cursed; every tick: the curses.
fn paintings_tick(world: &mut World, mut next_second: Local<f64>) {
    let t = now(world);
    tick_curses(world, t);
    if t < *next_second {
        return;
    }
    *next_second = t + 1.0;
    let mut q = world.query::<(&EldritchPainting, &TilePos)>();
    let paintings: Vec<(PaintingKind, TilePos)> = q
        .iter(world)
        .filter(|(p, _)| p.0 != PaintingKind::Vines)
        .map(|(p, &pos)| (p.0, pos))
        .collect();
    for (kind, pos) in paintings {
        for viewer in carbons_in_range(world, pos, RANGE) {
            let Some(&at) = world.get::<TilePos>(viewer) else {
                continue;
            };
            let stat = world.get::<Stat>(viewer).copied().unwrap_or_default();
            if !crate::mobs::ai::behaviors::can_see(world, at, pos, RANGE as u32)
                || world.get::<crate::mood::Mood>(viewer).is_none()
                || world.get::<PlayerMob>(viewer).is_none()
                || stat >= Stat::SoftCrit
                || super::items::unconscious(world, viewer)
                || world.get::<PaintingCurse>(viewer).is_some()
                || is_heretic_or_monster(world, viewer)
                || holy(world, viewer)
            {
                continue;
            }
            chat_to(world, viewer, Span::Notice, kind.text());
            world.entity_mut(viewer).insert(PaintingCurse {
                kind,
                until: t + CURSE_SECS,
                next_tick: t + kind.tick_secs(),
            });
            crate::speech::run_emote(world, viewer, "scream");
            chat_to(
                world,
                viewer,
                Span::Hypnophrase,
                "Когда вы смотрите на картину, вам разум постигает ее правду!",
            );
        }
    }
}

fn tick_curses(world: &mut World, t: f64) {
    let mut q = world.query::<(Entity, &PaintingCurse)>();
    let curses: Vec<(Entity, PaintingCurse)> = q.iter(world).map(|(e, c)| (e, *c)).collect();
    for (mob, mut curse) in curses {
        if world
            .get::<PaintingExamined>(mob)
            .is_some_and(|e| e.until <= t)
        {
            world.entity_mut(mob).remove::<PaintingExamined>();
        }
        let mut ended = t >= curse.until || world.get::<Stat>(mob) == Some(&Stat::Dead);
        while !ended && curse.next_tick <= t {
            let secs = curse.kind.tick_secs();
            curse.next_tick += secs;
            // Holy water halts the curse and wears it off three times as fast.
            if holy(world, mob) {
                curse.until -= 3.0 * secs;
                ended = t >= curse.until;
                continue;
            }
            if world.get::<PaintingExamined>(mob).is_some() {
                continue;
            }
            curse_tick(world, mob, curse.kind, secs as f32);
        }
        if ended {
            world.entity_mut(mob).remove::<PaintingCurse>();
        } else {
            world.entity_mut(mob).insert(curse);
        }
    }
}

/// `eldritch_painting/*/on_tick`.
fn curse_tick(world: &mut World, mob: Entity, kind: PaintingKind, secs: f32) {
    match kind {
        PaintingKind::Weeping => {
            if super::items::unconscious(world, mob) {
                return;
            }
            crate::hallucination::adjust_hallucinations_up_to(world, mob, 10.0, 60.0);
            crate::mood::add_event(world, mob, "eldritch_weeping", &WEEPING);
        }
        PaintingKind::Desire => {
            if let Some(mut hunger) = world.get_mut::<crate::hunger::Hunger>(mob) {
                hunger.nutrition =
                    (hunger.nutrition - 15.0 * crate::hunger::HUNGER_FACTOR * secs).max(0.0);
                hunger.overeat_secs = (hunger.overeat_secs - 200.0).max(0.0);
            }
            if world.resource_mut::<CombatRng>().chance(10.0 * secs) {
                let text = pick_str(
                    world,
                    &[
                        "Вы не можете прекратить думать о сыром мясе...",
                        "Вам НЕОБХОДИМО что-то поесть.",
                        "Голодные спазмы вернулись...",
                        "Вы жаждете плоти.",
                        "Вы голодаете!",
                    ],
                );
                chat_to(world, mob, Span::Notice, text);
            }
        }
        PaintingKind::Beauty => {
            if crate::status_effects::incapacitation(world, mob).incapacitated {
                return;
            }
            // Jumpsuits ruin the "perfection" of the body.
            let clothed = world
                .get::<Inventory>(mob)
                .is_some_and(|i| i.uniform.is_some() || i.suit.is_some());
            if !clothed {
                return;
            }
            let zones = crate::zone::BodyZone::ALL;
            let zone = zones[rand(world, zones.len())];
            hurt(world, mob, DamageType::Brute, 3.0);
            chat_to(
                world,
                mob,
                Span::Notice,
                format!("Вы яростно расцарапываете свою одежду {}!", zone.ru().0),
            );
        }
        PaintingKind::Rust => {
            chat_to(world, mob, Span::Notice, "You feel the decay...");
        }
        PaintingKind::Vines => {}
    }
}

/// `eldritch/examine` and the kinds' `examine_effects`.
fn painting_examines(world: &mut World, mut cursor: Local<MessageCursor<ExamineClick>>) {
    let Some(messages) = world.get_resource::<Messages<ExamineClick>>() else {
        return;
    };
    let clicks: Vec<ExamineClick> = cursor.read(messages).copied().collect();
    let t = now(world);
    for click in clicks {
        let Some(kind) = world.get::<EldritchPainting>(click.target).map(|p| p.0) else {
            continue;
        };
        let examiner = click.examiner;
        if world.get::<HumanLook>(examiner).is_none()
            || world.get::<PaintingExamined>(examiner).is_some()
        {
            continue;
        }
        world.entity_mut(examiner).insert(PaintingExamined {
            until: t + EXAMINE_SECS,
        });
        examine_effects(world, examiner, click.target, kind);
    }
}

fn examine_effects(world: &mut World, examiner: Entity, painting: Entity, kind: PaintingKind) {
    let heretic = is_heretic(world, examiner);
    let say = |world: &mut World, span: Span, text: &str| chat_to(world, examiner, span, text);
    match kind {
        PaintingKind::Weeping if !heretic => {
            say(world, Span::Hypnophrase, "Передохни, пока что...");
            crate::mood::clear_event(world, examiner, "eldritch_weeping");
            crate::mood::add_event(world, examiner, "weeping_withdrawal", &WEEPING_WITHDRAWAL);
        }
        PaintingKind::Weeping => {
            say(
                world,
                Span::Notice,
                "О, какое искусство! Один только взгляд на него проясняет ваши мысли.",
            );
            crate::hallucination::adjust_hallucinations(world, examiner, -10_000.0);
            crate::mood::add_event(
                world,
                examiner,
                "heretic_eldritch_painting",
                &WEEPING_HERETIC,
            );
        }
        PaintingKind::Desire if !heretic => {
            if let Some(mut hunger) = world.get_mut::<crate::hunger::Hunger>(examiner) {
                hunger.nutrition += 50.0;
            }
            say(world, Span::Warning, "Вы чувствуете жгучую боль в животе!");
            crate::medical::organs::adjust_organ_loss(
                world,
                examiner,
                crate::medical::organs::Slot::Stomach,
                5.0,
            );
            say(world, Span::Notice, "Вы чувствуете себя менее голодным.");
            say(
                world,
                Span::Warning,
                "Вам следует запастись сырым мясом и органами, прежде чем вы снова проголодаетесь.",
            );
            crate::mood::add_event(world, examiner, "respite_eldritch_hunger", &DESIRE_EXAMINE);
        }
        PaintingKind::Desire => {
            let gift = pick_str(world, &DESIRE_GIFTS);
            if let Some(&pos) = world.get::<TilePos>(painting) {
                crate::construction::spawn_obj(world, gift, pos);
            }
            say(
                world,
                Span::Notice,
                "Кусок плоти выползает из картины и падает на пол.",
            );
            say(world, Span::Warning, "Пустота кричит!");
            crate::mood::add_event(world, examiner, "heretic_eldritch_hunger", &DESIRE_HERETIC);
        }
        PaintingKind::Vines => {
            let Some(&pos) = world.get::<TilePos>(examiner) else {
                return;
            };
            if !heretic {
                crate::events::space_vines::spawn_controller(
                    world,
                    pos,
                    &VINE_MUTATIONS,
                    Some(0.0),
                    Some(10.0),
                    false,
                );
                say(
                    world,
                    Span::Hypnophrase,
                    "Заросли пролезают сквозь рамку, и вы вдруг обнаруживаете под собой лианы...",
                );
                say(
                    world,
                    Span::Notice,
                    "Ты чувствуешь, как что-то корчится вокруг тебя!",
                );
                return;
            }
            say(
                world,
                Span::Notice,
                "На мгновение вы замираете, глядя на хаотичные узоры, которые создают лианы.",
            );
            say(
                world,
                Span::Notice,
                "Вы чувствуете, как жизнь сгущается и расцветает под вами.",
            );
            let gift = pick_str(world, &VINE_GIFTS);
            crate::construction::spawn_obj(world, gift, pos);
            crate::mood::add_event(world, examiner, "heretic_vines", &HERETIC_VINES);
        }
        PaintingKind::Beauty if !heretic => {
            say(world, Span::Hypnophrase, "Вы еще недостаточно чисты.");
        }
        PaintingKind::Beauty => {
            say(world, Span::Notice, "Ваши недостатки исчезают.");
            super::items::add_to_blood(world, examiner, "/datum/reagent/medicine/mutadone", 5.0);
        }
        PaintingKind::Rust if !heretic => {
            say(world, Span::Hypnophrase, "Вы чувствуете ржавчину. Гниль.");
            crate::mood::add_event(world, examiner, "rusted_examine", &RUST_EXAMINE);
        }
        PaintingKind::Rust => {
            say(world, Span::Notice, "Картина наполняет вас решимостью!");
            crate::mood::add_event(world, examiner, "rusted_examine", &RUST_HERETIC);
        }
    }
}

/// `eldritch/wirecutter_act`: the painting comes down, and its laugh stays.
pub(super) fn cut(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<EldritchPainting>(target).is_none()
        || crate::tools::tool_of(world, held).map(|(k, _)| k)
            != Some(crate::tools::ToolKind::Wirecutters)
    {
        return false;
    }
    crate::mood::add_event(world, mob, "ripped_eldritch_painting", &RIPPED);
    tell(
        world,
        client,
        Span::Hypnophrase,
        "Смех отдается эхом в вашем разуме...",
    );
    despawn(world, target);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(path: &str) -> Candidate<Entity> {
        Candidate {
            thing: Entity::PLACEHOLDER,
            path: path.into(),
            stack: None,
        }
    }

    #[test]
    fn the_extra_item_picks_the_painting() {
        let eyes = [
            atom("/obj/item/canvas"),
            atom("/obj/item/organ/eyes/robotic"),
        ];
        assert_eq!(
            painting_recipe(&eyes).map(|r| r.1),
            Some(&["/obj/item/wallframe/painting/eldritch/weeping"][..])
        );
        let gloves = [
            atom("/obj/item/clothing/gloves/color/black"),
            atom("/obj/item/canvas"),
        ];
        assert_eq!(
            painting_recipe(&gloves).map(|r| r.1),
            Some(&["/obj/item/wallframe/painting/eldritch/beauty"][..])
        );
        assert!(painting_recipe(&[atom("/obj/item/canvas")]).is_none());
    }

    #[test]
    fn painting_kinds_by_type() {
        assert_eq!(
            PaintingKind::of("/obj/structure/sign/painting/eldritch/rust"),
            Some(PaintingKind::Rust)
        );
        assert_eq!(
            PaintingKind::of("/obj/structure/sign/painting/eldritch"),
            None
        );
    }
}
