//! Crayons and spray cans (server), after tg `code/game/objects/items/crayons.dm` and
//! `code/game/objects/effects/decals/crayon.dm`.
//!
//! - Every `/obj/item/toy/crayon` gets [`Crayon`]: charges (30, the mime and rainbow crayons and
//!   the infinite can unlimited), its paint colour and the chosen stencil (a random one at
//!   first, tg `pick(all_drawables)`).
//! - Used in hand, it asks for a stencil (graffiti, large graffiti, symbols, drawings, oriented,
//!   runes, randoms) or for text to write letter by letter (BandaStation: Cyrillic letters too,
//!   the `u<code>` states); a spray can also toggles its cap and picks a colour there.
//! - A crayon on a floor (or on a decal on one) draws after 5 s (15 s for the large graffiti,
//!   which need the tiles left and right of it): a `/obj/effect/decal/cleanable/crayon` in the
//!   paint colour, where the cursor was, turned with the user for oriented stencils. A stencil
//!   costs 1 charge (0.5 the small ones, 5 the large); an empty crayon is used up.
//!   The decal washes off only by scrubbing (`CLEAN_TYPE_HARD_DECAL`, `crate::janitor`).
//! - A crayon on a dead human draws a chalk outline under them; on oneself it is eaten, 5
//!   charges per bite (nutriment and the crayon's powder into the stomach).
//! - A spray can draws instantly, on walls too, and must be uncapped: on an object it paints it
//!   (2 charges; a dark colour with the right click is refused), on a crayon decal it recolours
//!   it (the decal's own cost), on a carbon's face it blurs and blinds them for a moment and
//!   costs 10. The hellcan sets the floor on fire and tears it to plating, the lube can lubes the
//!   floor it draws on, the silent can makes no sound and keeps its white.
// shortcut: the stencil is picked in `tgui_input_list` (the `Crayon` tgui window and its stencil
// previews are not ported); no literacy check, tagger trait, art element (mood), tool renaming,
// crayon box folding or crayon overlays on the box, venue price, chalk outline of the corpse's
// own sprite (a "body" stencil instead), feeding a crayon to someone else, washable paint
// (cleaning does not take an object's paint off), the saturation filter (a plain colour multiply,
// as `crate::service::art`), pipe colours, the face decal, `flash_color`, reagents of the spray
// (only the hellcan's fire and the lube can's lube act), robotic limb restyling (`color_limb`),
// the cyborg can's cell use (it is infinite) or the slapcraft recipe; upgrade with each mechanic.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName, DmObj, DmTurf, IconIndex, Turf};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Facing, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use crate::chem::{Chem, MobReagents, name_of, visible};
use crate::combat::{CombatMode, CombatRng};
use crate::construction::{Job, path_of, turf_at};
use crate::health::{Health, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::tools::{Actor, DoAfter, do_after, tell};

pub const CRAYON: &str = "/obj/item/toy/crayon";
pub const SPRAYCAN: &str = "/obj/item/toy/crayon/spraycan";
/// What a crayon draws (`ss13_mapload::runtime::GAME_RUNTIME_TYPES`).
pub const CRAYON_DECAL: &str = "/obj/effect/decal/cleanable/crayon";
const DECAL_DMI: &str = "icons/effects/crayondecal.dmi";
/// `PAINT_LARGE_HORIZONTAL_ICON`.
const LARGE_DMI: &str = "icons/effects/96x32.dmi";
const SPRAY_SOUND: &str = "sound/effects/spray.ogg";
/// `DRAW_TIME`, deciseconds.
const DRAW_DS: f32 = 50.0;
/// `/datum/component/edible` `eat_time`, deciseconds.
const EAT_DS: f32 = 30.0;
/// `CRAYON_COST_SMALL`, `CRAYON_COST_DEFAULT`, `CRAYON_COST_LARGE`.
const COST_SMALL: f32 = 0.5;
const COST_DEFAULT: f32 = 1.0;
const COST_LARGE: f32 = 5.0;
/// `DEFAULT_REAGENT_TEMPERATURE`.
const REAGENT_TEMPERATURE: f32 = 300.0;
const NUTRIMENT: &str = "/datum/reagent/consumable/nutriment";
/// `/obj/structure/table`, storage: a capped can is put down there
/// (`TRAIT_COMBAT_MODE_SKIP_INTERACTION`).
const SKIP_INTERACTION: [&str; 3] = [
    "/obj/structure/table",
    "/obj/item/storage",
    "/obj/structure/rack",
];

pub const GRAFFITI: [&str; 14] = [
    "amyjon",
    "antilizard",
    "body",
    "cyka",
    "dwarf",
    "end",
    "engie",
    "face",
    "guy",
    "matt",
    "prolizard",
    "revolution",
    "star",
    "uboa",
];
pub const SYMBOLS: [&str; 18] = [
    "biohazard",
    "credit",
    "danger",
    "electricdanger",
    "firedanger",
    "evac",
    "food",
    "heart",
    "like",
    "med",
    "nay",
    "peace",
    "radiation",
    "safe",
    "shop",
    "skull",
    "space",
    "trade",
];
pub const DRAWINGS: [&str; 21] = [
    "beepsky",
    "blueprint",
    "bottle",
    "brush",
    "carp",
    "cat",
    "clown",
    "corgi",
    "disk",
    "fireaxe",
    "ghost",
    "largebrush",
    "scroll",
    "shotgun",
    "smallbrush",
    "snake",
    "splatter",
    "stickman",
    "taser",
    "toilet",
    "toolbox",
];
pub const ORIENTED: [&str; 9] = [
    "arrow",
    "body",
    "chevron",
    "clawprint",
    "footprint",
    "line",
    "pawprint",
    "shortline",
    "thinline",
];
pub const RUNES: [&str; 6] = ["rune1", "rune2", "rune3", "rune4", "rune5", "rune6"];
/// `graffiti_large_h` (BandaStation removed "furrypride").
pub const LARGE: [&str; 3] = ["paint", "secborg", "yiffhell"];
/// The stencils with their own cost (`CRAYON_COST_SMALL`); the large ones cost `COST_LARGE`.
const SMALL: [&str; 6] = [
    "smallbrush",
    "arrow",
    "chevron",
    "clawprint",
    "footprint",
    "pawprint",
];
const SMALL_ORIENTED: &str = "shortline";
const RANDOM_ANY: &str = "Random Anything";
const RANDOM_DRAWING: &str = "Random Drawing";
const RANDOM_GRAFFITI: &str = "Random Graffiti";
const RANDOM_ORIENTED: &str = "Random Oriented";
const RANDOM_LETTER: &str = "Random Letter";
const RANDOM_NUMBER: &str = "Random Number";
const RANDOM_PUNCTUATION: &str = "Random Punctuation";
const RANDOM_RUNE: &str = "Random Rune";
const RANDOM_SYMBOL: &str = "Random Symbol";
const RANDOMS: [&str; 9] = [
    RANDOM_ANY,
    RANDOM_DRAWING,
    RANDOM_GRAFFITI,
    RANDOM_ORIENTED,
    RANDOM_LETTER,
    RANDOM_NUMBER,
    RANDOM_PUNCTUATION,
    RANDOM_RUNE,
    RANDOM_SYMBOL,
];
const PUNCTUATION: [&str; 11] = ["!", "?", ".", ",", "/", "+", "-", "=", "%", "#", "&"];
/// The list entries of the hand menu that are not stencils.
const WRITE_TEXT: &str = "Написать текст…";
const TOGGLE_CAP: &str = "Колпачок";
const PICK_COLOR: &str = "Цвет…";

/// `COLOR_CRAYON_*`.
const RED: [u8; 4] = [0xDA, 0x00, 0x00, 0xFF];
const ORANGE: [u8; 4] = [0xFF, 0x93, 0x00, 0xFF];
const YELLOW: [u8; 4] = [0xFF, 0xF2, 0x00, 0xFF];
const GREEN: [u8; 4] = [0xA8, 0xE6, 0x1D, 0xFF];
const BLUE: [u8; 4] = [0x00, 0xB7, 0xEF, 0xFF];
const PURPLE: [u8; 4] = [0xDA, 0x00, 0xFF, 0xFF];
const BLACK: [u8; 4] = [0x1C, 0x1C, 0x1C, 0xFF];
const WHITE: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
const RAINBOW: [u8; 4] = [0xFF, 0xF0, 0x00, 0xFF];
/// A new spray can's random colour.
const SPRAY_COLORS: [[u8; 4]; 6] = [RED, ORANGE, YELLOW, GREEN, BLUE, PURPLE];

/// The spray can kinds (`/obj/item/toy/crayon/spraycan` subtypes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spray {
    Plain,
    /// `hellcan`: chlorine trifluoride, no paint.
    Hell,
    /// `lubecan`: lube on the floor it draws on.
    Lube,
    /// `mimecan`: silent, always white.
    Mime,
}

/// A crayon or spray can's live state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Crayon {
    /// `charges`; `None` is `INFINITE_CHARGES`.
    pub charges: Option<f32>,
    pub charges_left: f32,
    pub color: [u8; 4],
    /// `drawtype`.
    pub drawtype: String,
    /// `text_buffer`: what is still to be written, one letter per drawing.
    pub text: String,
    pub spray: Option<Spray>,
    pub capped: bool,
    /// `/obj/item/toy/crayon/rainbow`: a random colour every use.
    pub rainbow: bool,
    /// `crayon_color`, the powder it is made of.
    pub crayon_color: &'static str,
}

impl Crayon {
    /// `Initialize` of `path`'s defaults; `pick` gives indices in `0..n` (tg `pick`).
    pub fn new(path: &str, mut pick: impl FnMut(usize) -> usize) -> Self {
        let spray = is_under(path, SPRAYCAN).then(|| {
            if is_under(path, "/obj/item/toy/crayon/spraycan/hellcan") {
                Spray::Hell
            } else if is_under(path, "/obj/item/toy/crayon/spraycan/lubecan") {
                Spray::Lube
            } else if is_under(path, "/obj/item/toy/crayon/spraycan/mimecan") {
                Spray::Mime
            } else {
                Spray::Plain
            }
        });
        let leaf = path.rsplit('/').next().unwrap_or_default();
        let (crayon_color, color) = match (spray, leaf) {
            (Some(Spray::Hell), _) => ("red", [0, 0, 0, 0xFF]),
            (Some(Spray::Mime), _) => ("red", WHITE),
            (Some(_), _) => ("red", SPRAY_COLORS[pick(SPRAY_COLORS.len())]),
            (None, "orange") => ("orange", ORANGE),
            (None, "yellow") => ("yellow", YELLOW),
            (None, "green") => ("green", GREEN),
            (None, "blue") => ("blue", BLUE),
            (None, "purple") => ("purple", PURPLE),
            (None, "black") => ("black", BLACK),
            (None, "white") => ("white", WHITE),
            (None, "mime") => ("mime", WHITE),
            (None, "rainbow") => ("rainbow", RAINBOW),
            // `/obj/item/toy/crayon` itself is `COLOR_RED`.
            (None, "crayon") => ("red", [0xFF, 0x00, 0x00, 0xFF]),
            _ => ("red", RED),
        };
        let infinite =
            matches!(leaf, "mime" | "rainbow" | "infinite" | "borg") && is_under(path, CRAYON);
        let charges = if infinite {
            None
        } else if spray == Some(Spray::Hell) {
            Some(100.0)
        } else {
            Some(30.0)
        };
        let all = all_drawables();
        let drawtype = if leaf == "rainbow" && spray.is_none() {
            RANDOM_ANY.to_owned()
        } else {
            all[pick(all.len())].to_owned()
        };
        Self {
            charges,
            // `refill()`: an infinite one keeps 100 for its reagents.
            charges_left: charges.unwrap_or(100.0),
            color,
            drawtype,
            text: String::new(),
            spray,
            capped: spray.is_some(),
            rainbow: leaf == "rainbow" && spray.is_none(),
            crayon_color,
        }
    }

    /// `actually_paints`.
    pub fn paints(&self) -> bool {
        self.spray != Some(Spray::Hell)
    }

    /// `pre_noise || post_noise`.
    fn noisy(&self) -> bool {
        matches!(self.spray, Some(s) if s != Spray::Mime)
    }

    /// `use_charges(amount, requires_full)`: false when there is not enough.
    pub fn use_charges(&mut self, amount: f32, requires_full: bool) -> bool {
        if self.charges.is_none() {
            return true;
        }
        if self.charges_left <= 0.0 || (requires_full && self.charges_left < amount) {
            return false;
        }
        self.charges_left -= self.charges_left.min(amount);
        true
    }

    /// Out of charges (an infinite one never is).
    pub fn empty(&self) -> bool {
        self.charges.is_some() && self.charges_left <= 0.0
    }
}

/// A drawn decal and its stencil (`icon_state`), for a spray can's recolouring cost.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct CrayonArt {
    pub drawing: String,
}

/// What a crayon's `do_after` finishes ([`crate::construction::Job::Crayon`]).
#[derive(Clone, Debug, PartialEq)]
pub enum CrayonJob {
    Draw(Drawing),
    /// A chalk outline around a dead human.
    Outline,
    /// A bite of the crayon.
    Eat,
}

/// A drawing decided at the start of `use_on`.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawing {
    pub drawing: String,
    /// What it is called (`temp`): "rune", "letter", "graffiti", ...
    pub kind: &'static str,
    pub cost: f32,
    pub rotation: u16,
    pub offset: [i16; 2],
    pub large: bool,
}

/// The hand menu's question, on the prompt entity.
#[derive(Component, Clone, Debug)]
enum CrayonAsk {
    Menu { item: Entity, entries: Vec<String> },
    Text { item: Entity },
    Color { item: Entity },
}

pub struct CrayonsPlugin;

impl Plugin for CrayonsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_observer(add_crayon);
        // The stencil menu (tg `ui_interact`).
        app.add_click_handler(ClickPhase::AttackSelf, 3203, |w, c| {
            attack_self(w, c.client, c.mob, c.target)
        });
        // Drawing, painting, spraying, outlining, eating (`interact_with_atom`, `attack`).
        app.add_click_handler(ClickPhase::HeldOnTarget, 4253, |w, c| {
            c.held.is_some_and(|held| {
                interact(w, c.client, c.mob, held, c.target, c.pos, c.modifiers)
            })
        });
        app.add_systems(
            Update,
            answers
                .in_set(ss13_tgui::server::UiSystems::Handle)
                .after(ss13_tgui::prompt::PromptSystems::Answer),
        );
    }
}

fn add_crayon(
    add: On<Add<DmItem>>,
    items: Query<&DmItem>,
    rng: Option<ResMut<CombatRng>>,
    mut commands: Commands,
) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if !is_under(&item.path, CRAYON) {
        return;
    }
    let mut rng = rng;
    let crayon = Crayon::new(&item.path, |n| match rng.as_mut() {
        Some(rng) if n > 1 => rng.range(0, n as u32 - 1) as usize,
        _ => 0,
    });
    commands.entity(add.entity).insert(crayon);
}

/// Every stencil (`all_drawables`), in tg's order.
pub fn all_drawables() -> Vec<&'static str> {
    let mut all: Vec<&'static str> = Vec::new();
    for list in [
        &GRAFFITI[..],
        &SYMBOLS[..],
        &DRAWINGS[..],
        &ORIENTED[..],
        &RUNES[..],
        &LARGE[..],
    ] {
        for &s in list {
            if !all.contains(&s) {
                all.push(s);
            }
        }
    }
    all
}

/// `all_drawables[drawing] || CRAYON_COST_DEFAULT`.
pub fn cost_of(drawing: &str) -> f32 {
    if LARGE.contains(&drawing) {
        COST_LARGE
    } else if SMALL.contains(&drawing) || drawing == SMALL_ORIENTED {
        COST_SMALL
    } else {
        COST_DEFAULT
    }
}

/// `use_on`'s `temp`: what the drawing is called.
pub fn kind_of(drawing: &str) -> &'static str {
    let mut chars = drawing.chars();
    let single = chars.next().filter(|_| chars.next().is_none());
    match single {
        Some(c) if c.is_ascii_lowercase() => "letter",
        Some(c) if c.is_ascii_digit() => "number",
        _ if PUNCTUATION.contains(&drawing) => "punctuation mark",
        _ if SYMBOLS.contains(&drawing) => "symbol",
        _ if DRAWINGS.contains(&drawing) => "drawing",
        _ if GRAFFITI.contains(&drawing) || ORIENTED.contains(&drawing) => "graffiti",
        _ => "rune",
    }
}

/// `crayon_text_strip`: only `[\wА-Яа-яЁё!?,.=&%#+/\-]`, lowercase.
pub fn strip_text(text: &str) -> String {
    text.chars()
        .filter(|&c| {
            c.is_ascii_alphanumeric()
                || c == '_'
                || ('А'..='я').contains(&c)
                || c == 'Ё'
                || c == 'ё'
                || "!?,.=&%#+/-".contains(c)
        })
        .flat_map(char::to_lowercase)
        .collect()
}

/// The icon state of one written character: BandaStation's `get_unicode_letter` for Cyrillic.
pub fn letter_state(c: char) -> String {
    let code = c as u32;
    if (1072..=1103).contains(&code) || code == 1105 {
        format!("u{code}")
    } else {
        c.to_string()
    }
}

/// `user.dir` to `graf_rot`.
fn rotation_of(dir: Dir) -> u16 {
    match dir {
        Dir::East => 90,
        Dir::South => 180,
        Dir::West => 270,
        _ => 0,
    }
}

/// tg `is_color_dark(color, 25)`: HSL lightness under 25 %.
pub fn is_dark(color: [u8; 4]) -> bool {
    let max = color[..3].iter().copied().max().unwrap_or(0) as f32;
    let min = color[..3].iter().copied().min().unwrap_or(0) as f32;
    (max + min) / 2.0 / 255.0 * 100.0 < 25.0
}

/// `#rrggbb` to RGBA.
fn parse_hex(hex: &str) -> Option<[u8; 4]> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8, 0xFF])
}

fn held_by(world: &World, item: Entity, mob: Entity) -> bool {
    world.get::<CarriedBy>(item).is_some_and(|c| c.0 == mob)
        && world
            .get::<Inventory>(mob)
            .is_some_and(|i| i.hands.contains(&Some(item)))
}

fn pick_index(world: &mut World, n: usize) -> usize {
    if n <= 1 {
        return 0;
    }
    world.resource_mut::<CombatRng>().range(0, n as u32 - 1) as usize
}

fn turf_path(world: &World, pos: TilePos) -> Option<String> {
    turf_at(world, pos).and_then(|t| path_of(world, t))
}

/// `isValidSurface`: a floor; a plain or lube-less spray can also a wall.
fn valid_surface(world: &World, crayon: &Crayon, pos: TilePos) -> bool {
    let Some(path) = turf_path(world, pos) else {
        return false;
    };
    let groundless = turf_at(world, pos)
        .and_then(|t| world.get::<DmTurf>(t))
        .is_some_and(|t| t.groundless);
    if groundless {
        return false;
    }
    is_under(&path, "/turf/open/floor")
        || (matches!(crayon.spray, Some(Spray::Plain | Spray::Mime))
            && is_under(&path, "/turf/closed/wall"))
}

fn play_spray(world: &mut World, mob: Entity, vol: f32) {
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SPRAY_SOUND, vol, true).extrarange(5.0),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The hand menu.

/// tg `ui_interact` (`hands_state`): the stencil list. True when the item is a crayon.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(crayon) = world.get::<Crayon>(item).cloned() else {
        return false;
    };
    if !crayon.paints() {
        // The hellcan only toggles its cap.
        return toggle_cap(world, client, item);
    }
    let mut entries: Vec<String> = Vec::new();
    if crayon.spray.is_some() {
        entries.push(TOGGLE_CAP.to_owned());
        if crayon.spray != Some(Spray::Mime) {
            entries.push(PICK_COLOR.to_owned());
        }
    }
    entries.push(WRITE_TEXT.to_owned());
    entries.extend(all_drawables().into_iter().map(str::to_owned));
    entries.extend(RANDOMS.iter().map(|&s| s.to_owned()));
    let default = Some(crayon.drawtype.clone()).filter(|d| entries.contains(d));
    let title = name_of(world, item);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            "Что рисовать?",
            PromptKind::List {
                items: entries.clone(),
                default,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(CrayonAsk::Menu { item, entries });
    true
}

/// `click_alt` of a spray can: the cap. True when the item has one.
pub fn toggle_cap(world: &mut World, client: Entity, item: Entity) -> bool {
    let Some(mut crayon) = world.get_mut::<Crayon>(item) else {
        return false;
    };
    if crayon.spray.is_none() {
        return false;
    }
    crayon.capped = !crayon.capped;
    let text = if crayon.capped {
        "Колпачок надет."
    } else {
        "Колпачок снят."
    };
    tell(world, client, Span::Notice, text);
    true
}

/// The answers, checked now: the crayon still in the actor's hands (`hands_state`).
fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(ask) = world.get::<CrayonAsk>(prompt).cloned() else {
            continue;
        };
        let item = match &ask {
            CrayonAsk::Menu { item, .. } | CrayonAsk::Text { item } | CrayonAsk::Color { item } => {
                *item
            }
        };
        if world.get::<Crayon>(item).is_none() || !held_by(world, item, actor) {
            continue;
        }
        match (ask, answer) {
            (CrayonAsk::Menu { entries, .. }, Some(PromptAnswer::Item(i))) => {
                let Some(entry) = entries.get(i) else {
                    continue;
                };
                menu_choice(world, client, actor, item, entry);
            }
            (CrayonAsk::Text { .. }, Some(PromptAnswer::Text(text))) => {
                let text = strip_text(&text);
                if let Some(mut crayon) = world.get_mut::<Crayon>(item)
                    && crayon.text != text
                {
                    crayon.text = text;
                    // `enter_text`: `drawtype = "a"`.
                    crayon.drawtype = "a".to_owned();
                }
            }
            (CrayonAsk::Color { .. }, Some(PromptAnswer::Color(hex))) => {
                if let Some(color) = parse_hex(&hex)
                    && let Some(mut crayon) = world.get_mut::<Crayon>(item)
                    && crayon.spray.is_some_and(|s| s != Spray::Mime)
                {
                    crayon.color = color;
                }
            }
            _ => {}
        }
    }
}

fn menu_choice(world: &mut World, client: Entity, mob: Entity, item: Entity, entry: &str) {
    match entry {
        TOGGLE_CAP => {
            toggle_cap(world, client, item);
        }
        PICK_COLOR => {
            let default = world.get::<Crayon>(item).map_or(WHITE, |c| c.color);
            let default = format!("#{:02x}{:02x}{:02x}", default[0], default[1], default[2]);
            let prompt = ask_world(
                world,
                Ask::new(
                    client,
                    mob,
                    "Цвет",
                    "Выберите цвет краски",
                    PromptKind::Color {
                        default,
                        autofocus: true,
                    },
                ),
            );
            world.entity_mut(prompt).insert(CrayonAsk::Color { item });
        }
        WRITE_TEXT => {
            let default = world
                .get::<Crayon>(item)
                .map(|c| c.text.clone())
                .unwrap_or_default();
            let prompt = ask_world(
                world,
                Ask::new(
                    client,
                    mob,
                    "Scribbles",
                    "Choose what to write",
                    PromptKind::Text {
                        default,
                        // `MAX_MESSAGE_LEN`.
                        max_length: 1024,
                        multiline: false,
                    },
                ),
            );
            world.entity_mut(prompt).insert(CrayonAsk::Text { item });
        }
        stencil => {
            // `select_stencil`: only a listed stencil, and the text is dropped.
            if (all_drawables().contains(&stencil) || RANDOMS.contains(&stencil))
                && let Some(mut crayon) = world.get_mut::<Crayon>(item)
            {
                crayon.drawtype = stencil.to_owned();
                crayon.text.clear();
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Using it on things.

/// tg `interact_with_atom` (and the edible `attack`) of a held crayon. True when it was its.
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    pos: TilePos,
    modifiers: ss13_shared::protocol::ClickModifiers,
) -> bool {
    let Some(crayon) = world.get::<Crayon>(held).cloned() else {
        return false;
    };
    if crayon.rainbow {
        // `rainbow/interact_with_atom`: `rgb(rand(0,255), ...)`.
        let color = {
            let mut rng = world.resource_mut::<CombatRng>();
            [0; 3].map(|_| rng.range(0, 255) as u8)
        };
        if let Some(mut c) = world.get_mut::<Crayon>(held) {
            c.color = [color[0], color[1], color[2], 0xFF];
        }
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let surface = world.get::<Turf>(target).is_some()
        || world.get::<crate::janitor::Cleanable>(target).is_some();
    let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
    let Some(spray) = crayon.spray else {
        if surface {
            return use_on_surface(world, actor, pos, modifiers);
        }
        let living = world.get::<Health>(target).is_some();
        if living && target == mob && !combat {
            // The edible component's `attack` on oneself.
            return do_after(world, actor, mob, EAT_DS, 0, Job::Crayon(CrayonJob::Eat));
        }
        let dead_human = world.get::<crate::human::HumanLook>(target).is_some()
            && world.get::<Stat>(target) == Some(&Stat::Dead);
        if dead_human && !combat {
            tell(world, client, Span::Notice, "Вы обводите тело...");
            return do_after(
                world,
                actor,
                target,
                DRAW_DS,
                0,
                Job::Crayon(CrayonJob::Outline),
            );
        }
        return false;
    };
    let carbon = world.get::<MobReagents>(target).is_some();
    let obj = world.get::<DmObj>(target).is_some() || world.get::<DmItem>(target).is_some();
    // `spraycan/can_use_on`.
    if !carbon && !obj && !surface {
        return false;
    }
    if crayon.capped
        && path_of(world, target).is_some_and(|p| SKIP_INTERACTION.iter().any(|r| is_under(&p, r)))
    {
        return false;
    }
    if crayon.capped {
        tell(world, client, Span::Warning, "Сначала снимите колпачок!");
        return true;
    }
    if crayon.empty() {
        tell(world, client, Span::Warning, "Баллончик пуст!");
        return true;
    }
    if carbon {
        spray_face(world, mob, held, target);
        return true;
    }
    if world.get::<Turf>(target).is_some() {
        return use_on_surface(world, actor, pos, modifiers);
    }
    paint_obj(world, client, mob, held, spray, target, modifiers.right);
    true
}

/// `use_on` on a turf or a decal on it: decides the drawing, then draws (now, or after the
/// `do_after`).
fn use_on_surface(
    world: &mut World,
    actor: Actor,
    pos: TilePos,
    modifiers: ss13_shared::protocol::ClickModifiers,
) -> bool {
    let Some(item) = actor.item else {
        return false;
    };
    let Some(crayon) = world.get::<Crayon>(item).cloned() else {
        return false;
    };
    let Some(turf) = turf_at(world, pos) else {
        return false;
    };
    if !valid_surface(world, &crayon, pos) {
        tell(world, actor.client, Span::Warning, "Здесь нельзя рисовать!");
        return true;
    }
    let drawing = resolve_random(world, &crayon.drawtype);
    let large = LARGE.contains(&drawing.as_str());
    let cost = cost_of(&drawing);
    // `check_empty(user, cost)`.
    if crayon.charges.is_some() && crayon.charges_left < cost {
        if crayon.charges_left <= 0.0 && crayon.spray.is_none() {
            world.despawn(item);
        } else {
            tell(world, actor.client, Span::Warning, "Не хватает краски!");
        }
        return true;
    }
    let kind = kind_of(&drawing);
    let rotation = if ORIENTED.contains(&drawing.as_str()) {
        rotation_of(world.get::<Facing>(actor.mob).map_or(Dir::South, |f| f.0))
    } else {
        0
    };
    let offset = modifiers.pixel.map_or([0, 0], |[x, y]| {
        [i16::from(x).clamp(-16, 16), i16::from(y).clamp(-16, 16)]
    });
    let drawing = Drawing {
        drawing,
        kind,
        cost,
        rotation,
        offset,
        large,
    };
    if crayon.noisy() {
        play_spray(world, actor.mob, 5.0);
    }
    if crayon.spray.is_some() {
        // `instant`.
        finish_draw(world, actor, pos, drawing);
        return true;
    }
    let target_name = name_of(world, turf);
    tell(
        world,
        actor.client,
        Span::Notice,
        format!("Вы начинаете рисовать {kind} на {target_name}..."),
    );
    let delay = if large { DRAW_DS * 3.0 } else { DRAW_DS };
    do_after(
        world,
        actor,
        turf,
        delay,
        0,
        Job::Crayon(CrayonJob::Draw(drawing)),
    );
    true
}

/// The `RANDOM_*` stencils resolved (`use_on`'s `switch(drawtype)`).
fn resolve_random(world: &mut World, drawtype: &str) -> String {
    fn pick(world: &mut World, list: &[&'static str]) -> String {
        list[pick_index(world, list.len())].to_owned()
    }
    match drawtype {
        RANDOM_LETTER => {
            let i = pick_index(world, 26) as u8;
            char::from(b'a' + i).to_string()
        }
        RANDOM_NUMBER => {
            let i = pick_index(world, 10) as u8;
            char::from(b'0' + i).to_string()
        }
        RANDOM_PUNCTUATION => pick(world, &PUNCTUATION[..]),
        RANDOM_SYMBOL => pick(world, &SYMBOLS[..]),
        RANDOM_DRAWING => pick(world, &DRAWINGS[..]),
        RANDOM_GRAFFITI => pick(world, &GRAFFITI[..]),
        RANDOM_RUNE => pick(world, &RUNES[..]),
        RANDOM_ORIENTED => pick(world, &ORIENTED[..]),
        RANDOM_ANY => pick(world, &all_drawables()),
        other => other.to_owned(),
    }
}

/// The end of `use_on`: the charges, the decal, the next letter, the spray's reagents.
fn finish_draw(world: &mut World, actor: Actor, pos: TilePos, mut d: Drawing) {
    let Some(item) = actor.item.filter(|&i| held_by(world, i, actor.mob)) else {
        return;
    };
    let Some(mut crayon) = world.get::<Crayon>(item).cloned() else {
        return;
    };
    if !valid_surface(world, &crayon, pos) {
        return;
    }
    // The large graffiti go on the left tile and need both neighbours (tg checks after use).
    let at = if d.large {
        let (left, right) = (
            TilePos::new(pos.x - 1, pos.y, pos.z),
            TilePos::new(pos.x + 1, pos.y, pos.z),
        );
        if !valid_surface(world, &crayon, left) || !valid_surface(world, &crayon, right) {
            tell(world, actor.client, Span::Warning, "Не хватает места!");
            return;
        }
        left
    } else {
        pos
    };
    if !crayon.use_charges(d.cost, true) {
        tell(world, actor.client, Span::Warning, "Не хватает краски!");
        return;
    }
    if let Some(c) = crayon.text.chars().next() {
        d.drawing = letter_state(c);
        crayon.text = crayon.text.chars().skip(1).collect();
    }
    if crayon.paints() {
        spawn_decal(world, at, &d, crayon.color, d.large);
    }
    let turf_name = turf_at(world, pos)
        .map(|t| name_of(world, t))
        .unwrap_or_default();
    let text = if crayon.spray.is_none() {
        format!("Вы заканчиваете рисовать {}.", d.kind)
    } else {
        format!("Вы рисуете {} на {turf_name}.", d.kind)
    };
    tell(world, actor.client, Span::Notice, text);
    match crayon.spray {
        // `clf3/expose_turf`: the floor is torn to plating, fire around it.
        Some(Spray::Hell) => {
            if turf_path(world, pos).is_some_and(|p| {
                is_under(&p, "/turf/open/floor") && !is_under(&p, "/turf/open/floor/plating")
            }) {
                crate::construction::scrape_away(world, pos, true);
            }
            if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        let near = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
                        atmos.hotspot_expose(near, 1000.0, 125.0, false);
                    }
                }
            }
        }
        // `lube/expose_turf`: `MakeSlippery(TURF_WET_LUBE, 15 SECONDS, 12 SECONDS)`.
        Some(Spray::Lube) => {
            crate::janitor::make_slippery(world, pos, crate::janitor::TURF_WET_LUBE, 15.0, 12.0);
        }
        _ => {}
    }
    let used_up = crayon.empty() && crayon.spray.is_none();
    world.entity_mut(item).insert(crayon);
    // `check_empty`: a used-up crayon is gone (`self_contained`).
    if used_up {
        world.despawn(item);
    }
}

/// `new /obj/effect/decal/cleanable/crayon(loc, paint_color, drawing, temp, graf_rot, alt_icon)`.
pub fn spawn_decal(
    world: &mut World,
    pos: TilePos,
    d: &Drawing,
    color: [u8; 4],
    large: bool,
) -> Option<Entity> {
    let decal = crate::construction::spawn_obj(world, CRAYON_DECAL, pos)?;
    let dmi = if large { LARGE_DMI } else { DECAL_DMI };
    let state = world
        .get_resource::<IconIndex>()
        .and_then(|ix| ix.get(dmi, &d.drawing));
    // Wall graffiti is drawn over the wall (`SET_PLANE_IMPLICIT(GAME_PLANE)`,
    // `CLEANABLE_OBJECT_LAYER`).
    let on_wall = turf_at(world, pos)
        .and_then(|t| world.get::<DmTurf>(t))
        .is_some_and(|t| t.closed);
    if let Some(mut a) = world.get_mut::<Appearance>(decal) {
        if let Some(id) = state {
            a.icon_state = id;
        }
        a.color = Some(color);
        a.rotation = d.rotation;
        a.pixel_offset = d.offset;
        if on_wall {
            a.plane = -4;
            a.layer = ss13_shared::Layer(2.55);
        }
    }
    let desc = format!("A {} vandalizing the station.", d.kind);
    if let Some(mut name) = world.get_mut::<DmName>(decal) {
        name.name = d.kind.to_owned();
        name.desc = desc;
    }
    world.entity_mut(decal).insert((
        Name::new(d.kind),
        CrayonArt {
            drawing: d.drawing.clone(),
        },
    ));
    Some(decal)
}

/// [`crate::construction::Job::Crayon`] done.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: CrayonJob) {
    let actor = Actor {
        client: action.client,
        mob,
        item: action.item,
    };
    match job {
        CrayonJob::Draw(d) => finish_draw(world, actor, action.target_pos, d),
        CrayonJob::Outline => outline(world, actor, action.target),
        CrayonJob::Eat => eat(world, actor),
    }
}

/// The chalk outline: a "body" stencil under the corpse, at its offset.
fn outline(world: &mut World, actor: Actor, corpse: Entity) {
    let Some(item) = actor.item.filter(|&i| held_by(world, i, actor.mob)) else {
        return;
    };
    let (Some(&pos), Some(mut crayon)) = (
        world.get::<TilePos>(corpse),
        world.get::<Crayon>(item).cloned(),
    ) else {
        return;
    };
    if !crayon.use_charges(1.0, true) {
        return;
    }
    let offset = world
        .get::<Appearance>(corpse)
        .map_or([0, 0], |a| a.pixel_offset);
    let d = Drawing {
        drawing: "body".to_owned(),
        kind: "chalk outline",
        cost: 1.0,
        rotation: 0,
        offset,
        large: false,
    };
    let corpse_name = name_of(world, corpse);
    if let Some(decal) = spawn_decal(world, pos, &d, crayon.color, false)
        && let Some(mut name) = world.get_mut::<DmName>(decal)
    {
        name.desc = format!("A vaguely {corpse_name} shaped body outline.");
    }
    tell(
        world,
        actor.client,
        Span::Notice,
        format!("Вы обводите мелом {corpse_name}."),
    );
    let used_up = crayon.empty();
    world.entity_mut(item).insert(crayon);
    if used_up {
        world.despawn(item);
    }
}

/// A bite (`bite_consumption = reagents / (charges_left / 5)`, `after_eat`: 5 charges): a sixth
/// of a full crayon's 30 units, a quarter nutriment and the rest its colour's powder.
fn eat(world: &mut World, actor: Actor) {
    let Some(item) = actor.item.filter(|&i| held_by(world, i, actor.mob)) else {
        return;
    };
    let Some(mut crayon) = world.get::<Crayon>(item).cloned() else {
        return;
    };
    let bite = 5.0_f32.min(crayon.charges_left.max(0.0));
    if let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) {
        let powder = match crayon.crayon_color {
            "mime" => "/datum/reagent/colorful_reagent/powder/invisible".to_owned(),
            "rainbow" => "/datum/reagent/colorful_reagent".to_owned(),
            c => format!("/datum/reagent/colorful_reagent/powder/{c}/crayon"),
        };
        let parts = [(NUTRIMENT.to_owned(), 0.25_f32), (powder, 0.75_f32)];
        if let Some(mut m) = world.get_mut::<MobReagents>(actor.mob) {
            for (path, part) in parts {
                if let Some(id) = chem.id(&path) {
                    m.stomach.add_reagent(
                        &chem,
                        id,
                        bite * part,
                        REAGENT_TEMPERATURE,
                        None,
                        None,
                        false,
                        false,
                    );
                }
            }
        }
    }
    let name = name_of(world, item);
    visible(
        world,
        actor.mob,
        &format!("Вы откусываете кусочек {name}."),
        &format!("{} откусывает кусочек {name}.", name_of(world, actor.mob)),
        Span::Notice,
    );
    // `use_charges(5, requires_full = FALSE, override_infinity = TRUE)`.
    crayon.charges_left -= bite;
    let gone = crayon.charges_left <= 0.0;
    world.entity_mut(item).insert(crayon);
    if gone {
        world.despawn(item);
    }
}

/// `spraycan/use_on` on a carbon: in the face.
fn spray_face(world: &mut World, user: Entity, can: Entity, victim: Entity) {
    let noisy = world.get::<Crayon>(can).is_some_and(Crayon::noisy);
    if noisy {
        play_spray(world, user, 25.0);
    }
    let (user_name, victim_name, can_name) = (
        name_of(world, user),
        name_of(world, victim),
        name_of(world, can),
    );
    visible(
        world,
        user,
        &format!("Вы брызгаете {can_name} в лицо {victim_name}!"),
        &format!("{user_name} брызгает {can_name} в лицо {victim_name}!"),
        Span::Danger,
    );
    crate::chem::tell_mob(
        world,
        victim,
        Span::Danger,
        &format!("{user_name} брызгает {can_name} вам в лицо!"),
    );
    if world.get::<crate::server::PlayerMob>(victim).is_some() {
        crate::vision::set_eye_blur_if_lower(world, victim, 6.0);
        crate::vision::adjust_temp_blindness(world, victim, 2.0);
    }
    // tg also burns covered eyes (`|| is_eyes_covered()`), against its own comment: only bare,
    // unprotected eyes sting here.
    let covered = crate::clothing::cover::eyes_covered(world, victim).is_some();
    if crate::security::eye_protection(world, victim) <= 0 && !covered {
        crate::status_effects::adjust_timed(
            world,
            victim,
            crate::status_effects::Timed::Jitter,
            1.0,
            None,
        );
        crate::vision::set_eye_blur_if_lower(world, victim, 0.5);
    }
    if let Some(mut c) = world.get_mut::<Crayon>(can) {
        c.use_charges(10.0, false);
    }
}

/// `spraycan/use_on` on an object: paint it, or recolour a crayon decal.
fn paint_obj(
    world: &mut World,
    client: Entity,
    user: Entity,
    can: Entity,
    spray: Spray,
    target: Entity,
    right: bool,
) {
    let Some(crayon) = world.get::<Crayon>(can).cloned() else {
        return;
    };
    let paints = spray != Spray::Hell;
    if paints && world.get::<Appearance>(target).and_then(|a| a.color) == Some(crayon.color) {
        tell(world, client, Span::Warning, "Оно уже этого цвета!");
        return;
    }
    let art = world.get::<CrayonArt>(target).map(|a| a.drawing.clone());
    // `ALLOW_DARK_PAINTS_1` is set on crayon decals only.
    if paints && right && is_dark(crayon.color) && art.is_none() {
        tell(
            world,
            client,
            Span::Warning,
            "Настолько тёмный цвет на таком предмете? Вряд ли...",
        );
        return;
    }
    if paints && let Some(mut a) = world.get_mut::<Appearance>(target) {
        a.color = Some(crayon.color);
    }
    // `crayon/on_painted`: a decal costs its stencil.
    let cost = art.as_deref().map_or(2.0, cost_of);
    if let Some(mut c) = world.get_mut::<Crayon>(can) {
        c.use_charges(cost, false);
    }
    if crayon.noisy() {
        play_spray(world, user, 5.0);
    }
    let (user_name, target_name) = (name_of(world, user), name_of(world, target));
    visible(
        world,
        user,
        &format!("Вы покрываете {target_name} краской."),
        &format!("{user_name} покрывает {target_name} краской!"),
        Span::Notice,
    );
}

/// Whether a held `path` takes a click on a mob instead of attacking it (`crate::combat`): a
/// spray can always sprays a face, a crayon out of combat mode is eaten or outlines a corpse.
pub fn interacts_with_mobs(path: &str, combat_mode: bool) -> bool {
    is_under(path, SPRAYCAN) || (is_under(path, CRAYON) && !combat_mode)
}

/// `spraycan/examine`: how full it is.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(crayon) = world.get::<Crayon>(target) else {
        return;
    };
    let (Some(_), Some(charges)) = (crayon.spray, crayon.charges) else {
        return;
    };
    let text = if crayon.charges_left > 0.0 {
        format!(
            "Он заполнен примерно на {}%.",
            (crayon.charges_left / charges * 100.0).round()
        )
    } else {
        "Он пуст.".to_owned()
    };
    lines.push((Span::Notice, text));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crayons_by_type() {
        let red = Crayon::new("/obj/item/toy/crayon/red", |_| 0);
        assert_eq!(red.color, RED);
        assert_eq!(red.charges, Some(30.0));
        assert!(red.spray.is_none() && !red.capped);
        let mime = Crayon::new("/obj/item/toy/crayon/mime", |_| 0);
        assert_eq!(mime.charges, None);
        let rainbow = Crayon::new("/obj/item/toy/crayon/rainbow", |_| 0);
        assert!(rainbow.rainbow);
        assert_eq!(rainbow.drawtype, RANDOM_ANY);
        let can = Crayon::new("/obj/item/toy/crayon/spraycan", |_| 2);
        assert_eq!(can.spray, Some(Spray::Plain));
        assert_eq!(can.color, YELLOW);
        assert!(can.capped);
        let hell = Crayon::new("/obj/item/toy/crayon/spraycan/hellcan", |_| 0);
        assert_eq!(hell.charges, Some(100.0));
        assert!(!hell.paints());
        let infinite = Crayon::new("/obj/item/toy/crayon/spraycan/infinite", |_| 0);
        assert_eq!(infinite.charges, None);
        let mime_can = Crayon::new("/obj/item/toy/crayon/spraycan/mimecan", |_| 0);
        assert_eq!(mime_can.color, WHITE);
        assert!(!mime_can.noisy());
    }

    #[test]
    fn costs_and_kinds() {
        assert_eq!(cost_of("rune1"), 1.0);
        assert_eq!(cost_of("arrow"), 0.5);
        assert_eq!(cost_of("smallbrush"), 0.5);
        assert_eq!(cost_of("secborg"), 5.0);
        assert_eq!(kind_of("a"), "letter");
        assert_eq!(kind_of("7"), "number");
        assert_eq!(kind_of("!"), "punctuation mark");
        assert_eq!(kind_of("skull"), "symbol");
        assert_eq!(kind_of("corgi"), "drawing");
        assert_eq!(kind_of("arrow"), "graffiti");
        assert_eq!(kind_of("rune3"), "rune");
        assert_eq!(kind_of("u1072"), "rune");
        // "body" is both graffiti and oriented: listed once.
        assert_eq!(
            all_drawables().len(),
            GRAFFITI.len() + SYMBOLS.len() + DRAWINGS.len() + ORIENTED.len() - 1
                + RUNES.len()
                + LARGE.len()
        );
    }

    #[test]
    fn text_is_stripped_and_cyrillic_has_its_states() {
        assert_eq!(strip_text("Hi, Мир! <b>"), "hi,мир!b");
        assert_eq!(letter_state('a'), "a");
        assert_eq!(letter_state('а'), "u1072");
        assert_eq!(letter_state('ё'), "u1105");
    }

    #[test]
    fn charges_run_out() {
        let mut c = Crayon::new("/obj/item/toy/crayon/red", |_| 0);
        c.charges_left = 1.5;
        assert!(c.use_charges(1.0, true));
        assert!(!c.use_charges(1.0, true), "half a charge is not enough");
        assert!(c.use_charges(1.0, false));
        assert!(c.empty());
        let mut mime = Crayon::new("/obj/item/toy/crayon/mime", |_| 0);
        assert!(mime.use_charges(100.0, true));
        assert!(!mime.empty());
    }

    #[test]
    fn dark_colors() {
        assert!(is_dark(BLACK));
        assert!(!is_dark(RED));
        assert!(!is_dark(WHITE));
    }
}
