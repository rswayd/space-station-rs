//! Grooming (server), after tg `code/game/objects/items/cosmetics.dm`, `dyespray.dm` and
//! `code/game/objects/structures/mirror.dm`:
//!
//! - The razor ([`RAZOR`]) on a head or a mouth: in combat mode it shaves (5 s; the head to a
//!   skinhead, the face clean), out of it a barber picks someone else's new hair or beard from a
//!   list (6 s). Mouths behind a mask or a helmet, heads under headgear are refused.
//! - Lipstick ([`Lipstick`]): twisted open in hand, its style (upper, middle, lower lip) picked
//!   with an alt-click, applied to someone's lips (at once on oneself, 2 s on another); paper on
//!   the mouth wipes it off (1 s on another), washing too ([`clean_lips`]).
//! - The hair dye spray ([`DYE_SPRAY`]): hair or beard, a color, 3 s.
//! - Mirrors ([`open_mirror`]): the option list (tg's radial) — hair and beard; magic mirrors
//!   also race (with skin tone or color), sex, eyes and name, and the hair's color; the
//!   heretic's mirror changes a non-heretic at random. A mirror frame hangs on a wall
//!   ([`hang_frame`]).
//!
//! Every answer is re-checked at the moment of effect (tg `can_use_mirror`,
//! `can_perform_action`), and every `do_after` re-checks what it started on.
// shortcut: the dye spray colors the whole hair or beard: tg paints a gradient pattern masked by
// the hair (`get_gradient_overlay`, an `ICON_ADD` blend), which overlays cannot express; no organ
// dyeing (no mutant bodypart overlays). The heretic's mirror changes a non-heretic at once (no 5 s trance). The mirror's
// sex change sets the physique only (pronouns follow `HumanLook::female`). Upgrade each with its
// mechanic.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmClothing, DmItem, DmObj, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::interfaces::color_picker::Hsv;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::UiSystems;

use crate::construction::Job;
use crate::health::Stat;
use crate::human::{
    EYE_COLORS, FACIAL_HAIR_STYLE_NAMES, FACIAL_HAIR_STYLES, HAIR_STYLE_NAMES, HAIR_STYLES,
    HumanLook, SKIN_TONES, SKINHEAD, hex_rgb,
};
use crate::inventory::{CarriedBy, Inventory};
use crate::medical::{name_of, visible};
use crate::species::{LimbColor, SpeciesId};
use crate::tools::{Actor, DoAfter, do_after, tell};
use crate::zone::{BodyZone, TargetZone};

pub const RAZOR: &str = "/obj/item/razor";
pub const LIPSTICK: &str = "/obj/item/lipstick";
const LIPSTICK_RANDOM: &str = "/obj/item/lipstick/random";
pub const DYE_SPRAY: &str = "/obj/item/dyespray";
const PAPER: &str = "/obj/item/paper";
const MIRROR_FRAME: &str = "/obj/item/wallframe/mirror";
const WALL: &str = "/turf/closed/wall";
const MAGIC_MIRROR: &str = "/obj/structure/mirror/magic";
const PRIDE_MIRROR: &str = "/obj/structure/mirror/magic/pride";
const HERETIC_MIRROR: &str = "/obj/structure/mirror/magic/lesser/heretic";

pub const CLIPPERS_SOUND: &str = "sound/items/hair-clippers.ogg";
const SPRAY_SOUND: &str = "sound/effects/spray.ogg";
const CLICK_SOUND: &str = "sound/machines/click.ogg";

/// `razor/attack`: `do_after` of a shave and of a restyle, deciseconds.
const SHAVE_DS: f32 = 50.0;
const STYLE_DS: f32 = 60.0;
/// `lipstick/attack` on another, `paper/attack` wiping another's lips.
const LIPSTICK_DS: f32 = 20.0;
const WIPE_DS: f32 = 10.0;
/// `dyespray/dye`.
const DYE_DS: f32 = 30.0;
/// `wallframe/mirror::pixel_shift`.
const MIRROR_SHIFT: i32 = 28;
/// `MAX_NAME_LEN`.
const MAX_NAME_LEN: u32 = 42;

/// `/obj/item/lipstick` subtypes and their `lipstick_color` (most specific first); the last
/// `random_spawn` ones are what `/obj/item/lipstick/random` picks from.
const LIPSTICK_COLORS: [(&str, &str, [u8; 3]); 9] = [
    ("/black/death", "Поцелуй смерти", [0, 0, 0]),
    ("/syndie", "синдикатская помада", [0xf1, 0x03, 0x03]),
    ("", "красная помада", [0xff, 0, 0]),
    ("/purple", "фиолетовая помада", [0x80, 0, 0x80]),
    ("/jade", "нефритовая помада", [0x5e, 0xfb, 0x6e]),
    ("/blue", "синяя помада", [0, 0, 0xff]),
    ("/green", "зелёная помада", [0, 0x80, 0]),
    ("/white", "белая помада", [0xff, 0xff, 0xff]),
    ("/black", "чёрная помада", [0, 0, 0]),
];
/// The `random_spawn` entries of [`LIPSTICK_COLORS`].
const RANDOM_LIPSTICKS: std::ops::Range<usize> = 2..9;

/// `UPPER_LIP`, `MIDDLE_LIP`, `LOWER_LIP` of the style radial and their `style`.
const LIP_STYLES: [(&str, &str); 3] = [
    ("Верхняя губа", "lipstick_upper"),
    ("Обе губы", "lipstick"),
    ("Нижняя губа", "lipstick_lower"),
];

/// The hairstyle lists' "Bald" and the facial ones' "Shaved".
const BALD: &str = "Без волос";
const SHAVED: &str = "Сбрита";

/// `/obj/item/lipstick`: `open`, `lipstick_color`, `style`, `base_icon_state`.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Lipstick {
    pub open: bool,
    pub color: [u8; 3],
    pub style: &'static str,
    pub base: String,
    /// `lipstick_trait`: the kiss its wearer blows (`TRAIT_KISS_OF_DEATH`, `TRAIT_SYNDIE_KISS`).
    pub kiss: Option<crate::devices::hand_items::Kiss>,
}

/// The `do_after`s of grooming ([`Job::Barber`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarberJob {
    /// The razor shaves the head (`facial == false`) or the face clean.
    Shave { facial: bool },
    /// The razor gives a new style (an index of the list, 0 = bald / shaved).
    Style { facial: bool, entry: usize },
    /// Lipstick on another's lips.
    Lips,
    /// Paper wipes another's lips.
    Wipe,
    /// The dye spray's color.
    Dye { facial: bool, color: [u8; 3] },
}

/// tg `mirror_options`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MirrorOption {
    Hair,
    Beard,
    Race,
    Sex,
    Eyes,
    Name,
}

impl MirrorOption {
    fn label(self) -> &'static str {
        match self {
            Self::Hair => "Сменить причёску",
            Self::Beard => "Сменить бороду",
            Self::Race => "Сменить расу",
            Self::Sex => "Сменить пол",
            Self::Eyes => "Сменить глаза",
            Self::Name => "Сменить имя",
        }
    }
}

/// `INERT_MIRROR_OPTIONS`, `PRIDE_MIRROR_OPTIONS`, `MAGIC_MIRROR_OPTIONS`.
const INERT_OPTIONS: &[MirrorOption] = &[MirrorOption::Hair, MirrorOption::Beard];
const PRIDE_OPTIONS: &[MirrorOption] = &[
    MirrorOption::Hair,
    MirrorOption::Beard,
    MirrorOption::Race,
    MirrorOption::Sex,
    MirrorOption::Eyes,
];
const MAGIC_OPTIONS: &[MirrorOption] = &[
    MirrorOption::Hair,
    MirrorOption::Beard,
    MirrorOption::Race,
    MirrorOption::Sex,
    MirrorOption::Eyes,
    MirrorOption::Name,
];

/// A grooming question waiting for its answer, on the prompt entity.
#[derive(Component, Clone, Debug, PartialEq)]
pub enum BarberAsk {
    /// The razor's new style for `target`.
    RazorStyle {
        razor: Entity,
        target: Entity,
        facial: bool,
    },
    /// The lipstick's style radial.
    LipStyle { lipstick: Entity },
    /// Hair or beard to dye (`dyables` as asked).
    DyeWhat {
        spray: Entity,
        target: Entity,
        dyables: Vec<bool>,
    },
    /// The dye's color.
    DyeColor {
        spray: Entity,
        target: Entity,
        facial: bool,
    },
    /// A mirror's option list.
    Mirror { mirror: Entity },
    /// A mirror's step after an option was picked.
    MirrorStep { mirror: Entity, step: MirrorStep },
}

/// The questions of the mirror options (`change_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MirrorStep {
    /// The magic mirror's "Style or color?".
    HairStyleOrColor,
    HairStyle,
    HairColor,
    FacialHairColor,
    /// A female physique's "Shave your beard?".
    ShaveBeard,
    Beard,
    Race,
    RaceConfirm(SpeciesId),
    SkinTone,
    MutantColor,
    Sex,
    Physique,
    Eyes,
    Name,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // Lipstick twisted open or shut, the dye spray on oneself.
    app.add_click_handler(ClickPhase::AttackSelf, 3010, |w, c| {
        attack_self(w, c.client, c.mob, c.target)
    });
    // Razor, lipstick, paper on lips, dye spray on a human; a mirror frame on a wall.
    app.add_click_handler(ClickPhase::HeldOnTarget, 1680, |w, c| {
        c.held
            .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
    });
    app.add_observer(add_lipstick).add_systems(
        Update,
        answers
            .in_set(UiSystems::Handle)
            .after(PromptSystems::Answer),
    );
}

/// The grooming items the melee chain leaves alone (`crate::combat`): their `attack` grooms
/// (or, for closed lipstick, does nothing) instead of hitting.
pub fn interacts_with_mobs(path: &str) -> bool {
    [RAZOR, LIPSTICK, DYE_SPRAY]
        .iter()
        .any(|root| is_under(path, root))
}

fn item_path(world: &World, item: Entity) -> Option<&str> {
    world.get::<DmItem>(item).map(|i| i.path.as_str())
}

/// `lipstick/Initialize`: its color and look; a random one picks a color and a name.
fn add_lipstick(add: On<Add<DmItem>>, mut commands: Commands) {
    let e = add.entity;
    commands.queue(move |world: &mut World| {
        let Some(path) = item_path(world, e).map(str::to_owned) else {
            return;
        };
        if !is_under(&path, LIPSTICK) || world.get::<Lipstick>(e).is_some() {
            return;
        }
        let random = is_under(&path, LIPSTICK_RANDOM);
        let index = if random {
            let n = RANDOM_LIPSTICKS.len() as u32;
            let pick = world
                .get_resource_mut::<crate::combat::CombatRng>()
                .map_or(0, |mut rng| rng.range(0, n) as usize);
            RANDOM_LIPSTICKS.start + pick.min(n as usize - 1)
        } else {
            let sub = &path[LIPSTICK.len()..];
            LIPSTICK_COLORS
                .iter()
                .position(|(s, ..)| !s.is_empty() && sub.starts_with(s))
                .unwrap_or(2)
        };
        let (_, name, color) = LIPSTICK_COLORS[index];
        if random {
            world.entity_mut(e).insert(Name::new(name));
        }
        let base = match world.get::<DmAtom>(e).map(|a| a.icon_state.as_str()) {
            Some("slipstick") => "slipstick",
            _ => "lipstick",
        }
        .to_owned();
        use crate::devices::hand_items::Kiss;
        let kiss = match index {
            0 => Some(Kiss::Death),
            1 => Some(Kiss::Syndie),
            _ => None,
        };
        world.entity_mut(e).insert(Lipstick {
            open: false,
            color,
            style: "lipstick",
            base,
            kiss,
        });
        refresh_lipstick(world, e);
    });
}

/// `lipstick/update_icon_state` and `update_overlays`: `_uncap` with the colored tip when open.
fn refresh_lipstick(world: &mut World, item: Entity) {
    let Some(lipstick) = world.get::<Lipstick>(item).cloned() else {
        return;
    };
    let state = if lipstick.open {
        format!("{}_uncap", lipstick.base)
    } else {
        lipstick.base.clone()
    };
    crate::obj_damage::set_icon_state(world, item, &state);
    let dmi = world.get::<DmAtom>(item).map(|a| a.icon.clone());
    let tip = dmi.and_then(|dmi| {
        world
            .get_resource::<IconIndex>()?
            .get(&dmi, "lipstick_uncap_color")
    });
    let overlays: Vec<Overlay> = match tip {
        Some(id) if lipstick.open => {
            let [r, g, b] = lipstick.color;
            vec![Overlay {
                color: Some([r, g, b, 255]),
                ..Overlay::new(id)
            }]
        }
        _ => Vec::new(),
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(item)
        && appearance.overlays != overlays
    {
        appearance.overlays = overlays;
    }
    if let Some(holder) = world.get::<CarriedBy>(item).map(|c| c.0)
        && let Some(mut inventory) = world.get_mut::<Inventory>(holder)
    {
        inventory.set_changed();
    }
}

/// `attack_self` of lipstick (twist) and of the dye spray (`dye(user, user)`). True when it was
/// one of them.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if let Some(open) = world.get::<Lipstick>(item).map(|l| l.open) {
        let name = name_of(world, item);
        let text = if open {
            format!("Вы закручиваете {name}.")
        } else {
            format!("Вы откручиваете {name}.")
        };
        tell(world, client, Span::Notice, text);
        if let Some(mut lipstick) = world.get_mut::<Lipstick>(item) {
            lipstick.open = !open;
        }
        refresh_lipstick(world, item);
        return true;
    }
    if item_path(world, item).is_some_and(|p| is_under(p, DYE_SPRAY)) {
        dye(world, client, mob, item, mob);
        return true;
    }
    false
}

/// `lipstick/click_alt`: the style radial (a list here). True when `item` is lipstick.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<Lipstick>(item).is_none() {
        return false;
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Помада",
            "Как нанести помаду?",
            PromptKind::List {
                items: LIP_STYLES.iter().map(|(n, _)| (*n).to_owned()).collect(),
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(BarberAsk::LipStyle { lipstick: item });
    true
}

/// tg `item_interaction`/`attack` of the grooming items in hand. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(path) = item_path(world, held).map(str::to_owned) else {
        return false;
    };
    if is_under(&path, MIRROR_FRAME) {
        return hang_frame(world, client, mob, held, &path, target);
    }
    if is_under(&path, DYE_SPRAY) {
        // `pre_attack`: a non-human is attacked as usual.
        if world.get::<HumanLook>(target).is_none() {
            return false;
        }
        dye(world, client, mob, held, target);
        return true;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if is_under(&path, RAZOR) {
        return razor(world, actor, target);
    }
    if is_under(&path, LIPSTICK) {
        apply_lipstick(world, actor, target);
        return true;
    }
    if is_under(&path, PAPER) && selected(world, mob) == TargetZone::Mouth {
        return wipe(world, actor, target);
    }
    false
}

fn selected(world: &World, mob: Entity) -> TargetZone {
    world
        .get::<crate::health::ZoneSelected>(mob)
        .copied()
        .unwrap_or_default()
        .0
}

fn combat_mode(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
}

fn has_head(world: &World, mob: Entity) -> bool {
    world
        .get::<HumanLook>(mob)
        .is_some_and(|l| l.missing & (1 << BodyZone::Head.index()) == 0)
}

/// `head_flags & HEAD_HAIR` / `HEAD_FACIAL_HAIR`: a head of a species that grows hair.
fn hairy_head(world: &World, mob: Entity) -> bool {
    has_head(world, mob)
        && world
            .get::<HumanLook>(mob)
            .is_some_and(|l| l.species.def().limbs.hair)
}

/// tg `TRAIT_BALD` (the Smooth-Headed quirk).
fn bald_trait(world: &World, mob: Entity) -> bool {
    world.get::<crate::quirks::BaldHead>(mob).is_some()
}

/// `is_location_accessible(BODY_ZONE_HEAD)`: no headgear or mask covering the head.
fn head_accessible(world: &World, mob: Entity) -> bool {
    const HEAD: u32 = 1 << 2;
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return true;
    };
    ![inventory.head, inventory.mask, inventory.neck]
        .into_iter()
        .flatten()
        .any(|item| {
            world
                .get::<DmClothing>(item)
                .is_some_and(|c| c.body_parts_covered & HEAD != 0)
        })
}

/// The actor can still act with `item` on `target`: conscious, able, holding it, next to it.
fn can_act(world: &World, actor: Actor, target: Entity) -> bool {
    let able = world.get::<Stat>(actor.mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, actor.mob).incapacitated;
    let holding = actor
        .item
        .is_none_or(|item| world.get::<CarriedBy>(item).map(|c| c.0) == Some(actor.mob));
    able && holding && adjacent(world, actor.mob, target)
}

fn adjacent(world: &World, a: Entity, b: Entity) -> bool {
    match (world.get::<TilePos>(a), world.get::<TilePos>(b)) {
        (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
        _ => false,
    }
}

fn sound(world: &mut World, at: Entity, file: &str, vol: f32) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, file, vol, true));
    }
}

fn set_look(world: &mut World, mob: Entity, change: impl FnOnce(&mut HumanLook)) {
    let Some(mut look) = world.get::<HumanLook>(mob).cloned() else {
        return;
    };
    change(&mut look);
    let changed = match world.get_mut::<HumanLook>(mob) {
        Some(mut current) if *current != look => {
            *current = look;
            true
        }
        _ => false,
    };
    if changed {
        crate::inventory::refresh_look(world, mob);
    }
}

fn hair_names() -> Vec<String> {
    std::iter::once(BALD)
        .chain(HAIR_STYLE_NAMES)
        .map(str::to_owned)
        .collect()
}

fn facial_names() -> Vec<String> {
    std::iter::once(SHAVED)
        .chain(FACIAL_HAIR_STYLE_NAMES)
        .map(str::to_owned)
        .collect()
}

/// A style list entry (0 = bald / shaved) as the look's style.
fn style_of(facial: bool, entry: usize) -> Option<Option<&'static str>> {
    if entry == 0 {
        return Some(None);
    }
    let styles: &[&str] = if facial {
        &FACIAL_HAIR_STYLES
    } else {
        &HAIR_STYLES
    };
    styles.get(entry - 1).map(|s| Some(*s))
}

/// `razor/attack` on a head or a mouth. False for other zones (the item's usual attack).
fn razor(world: &mut World, actor: Actor, target: Entity) -> bool {
    if world.get::<HumanLook>(target).is_none() {
        return false;
    }
    let zone = selected(world, actor.mob);
    let facial = match zone {
        TargetZone::Mouth => true,
        TargetZone::Head => false,
        TargetZone::Eyes => {
            if !has_head(world, target) {
                let text = format!("У {} нет головы!", name_of(world, target));
                tell(world, actor.client, Span::Warning, text);
                return true;
            }
            return false;
        }
        _ => return false,
    };
    let victim = name_of(world, target);
    if !has_head(world, target) {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("У {victim} нет головы!"),
        );
        return true;
    }
    let me = actor.mob == target;
    let female = world.get::<HumanLook>(target).is_some_and(|l| l.female);
    if !combat_mode(world, actor.mob) {
        // `if(human_target.gender == MALE)`: no beard to style otherwise.
        if facial && female {
            return true;
        }
        if me {
            let text = if facial {
                "Чтобы как следует уложить бороду, нужно зеркало!"
            } else {
                "Чтобы как следует уложить волосы, нужно зеркало!"
            };
            tell(world, actor.client, Span::Warning, text);
            return true;
        }
        let (message, items) = if facial {
            ("Выберите стиль бороды", facial_names())
        } else {
            ("Выберите причёску", hair_names())
        };
        let prompt = ask_world(
            world,
            Ask::new(
                actor.client,
                actor.mob,
                "Уход за собой",
                message,
                PromptKind::List {
                    items,
                    default: None,
                },
            ),
        );
        world.entity_mut(prompt).insert(BarberAsk::RazorStyle {
            razor: actor.item.expect("held"),
            target,
            facial,
        });
        return true;
    }
    if !groomable(world, actor.client, target, facial, true) {
        return true;
    }
    let look = world.get::<HumanLook>(target).expect("checked");
    let nothing = if facial {
        look.facial_hair_style.is_none()
    } else {
        look.hair_style.is_none() || look.hair_style.as_deref() == Some(SKINHEAD)
    };
    if nothing {
        let text = if facial {
            "Уже чисто выбрито!"
        } else {
            "Волос для бритья не осталось!"
        };
        tell(world, actor.client, Span::Warning, text);
        return true;
    }
    let user = name_of(world, actor.mob);
    let razor = name_of(world, actor.item.expect("held"));
    let (own, others) = match (me, facial) {
        (true, true) => (
            format!("Вы начинаете сбривать бороду с помощью {razor}..."),
            format!("{user} начинает сбривать себе бороду с помощью {razor}."),
        ),
        (true, false) => (
            format!("Вы начинаете брить голову с помощью {razor}..."),
            format!("{user} начинает брить себе голову с помощью {razor}."),
        ),
        (false, true) => (
            format!("Вы начинаете сбривать бороду {victim}..."),
            format!("{user} пытается сбрить бороду {victim} с помощью {razor}."),
        ),
        (false, false) => (
            format!("Вы начинаете брить голову {victim}..."),
            format!("{user} пытается обрить голову {victim} с помощью {razor}!"),
        ),
    };
    let span = if me { Span::Notice } else { Span::Warning };
    visible(world, actor.mob, &own, &others, span);
    sound(world, target, CLIPPERS_SOUND, 50.0);
    do_after(
        world,
        actor,
        target,
        SHAVE_DS,
        0,
        Job::Barber(BarberJob::Shave { facial }),
    );
    true
}

/// The razor's checks of the target's head: covered mouth or headgear, a hairless species, a
/// bald or shaved trait (restyles only). Tells why not.
fn groomable(world: &mut World, client: Entity, target: Entity, facial: bool, shave: bool) -> bool {
    if facial {
        if let Some(cover) = crate::clothing::cover::mouth_covered(world, target) {
            let text = format!("{} мешает!", name_of(world, cover));
            tell(world, client, Span::Warning, text);
            return false;
        }
    } else if !head_accessible(world, target) {
        tell(world, client, Span::Warning, "Головной убор мешает!");
        return false;
    }
    if !hairy_head(world, target) {
        let text = match (facial, shave) {
            (true, true) => "Тут нечего брить!",
            (true, false) => "Тут нет бороды для укладки!",
            (false, true) => "Тут нет волос для бритья!",
            (false, false) => "Тут нет волос для укладки!",
        };
        tell(world, client, Span::Warning, text);
        return false;
    }
    if !shave && !facial && bald_trait(world, target) {
        let text = format!(
            "{} слишком лыс. Прямо очень-очень лыс.",
            name_of(world, target)
        );
        tell(world, client, Span::Warning, text);
        return false;
    }
    true
}

/// The razor's style list answered: tg checks the cover and the head, then 6 s.
fn razor_style(world: &mut World, actor: Actor, target: Entity, facial: bool, entry: usize) {
    if style_of(facial, entry).is_none()
        || !can_act(world, actor, target)
        || !groomable(world, actor.client, target, facial, false)
    {
        return;
    }
    let user = name_of(world, actor.mob);
    let victim = name_of(world, target);
    let what = if facial {
        "бороду"
    } else {
        "причёску"
    };
    visible(
        world,
        actor.mob,
        &format!("Вы пытаетесь изменить {what} {victim}."),
        &format!("{user} пытается изменить {what} {victim}."),
        Span::Notice,
    );
    sound(world, target, CLIPPERS_SOUND, 50.0);
    do_after(
        world,
        actor,
        target,
        STYLE_DS,
        0,
        Job::Barber(BarberJob::Style { facial, entry }),
    );
}

/// `lipstick/attack`.
fn apply_lipstick(world: &mut World, actor: Actor, target: Entity) {
    let item = actor.item.expect("held");
    let Some(lipstick) = world.get::<Lipstick>(item).cloned() else {
        return;
    };
    if !lipstick.open {
        return;
    }
    if world.get::<HumanLook>(target).is_none() {
        tell(world, actor.client, Span::Warning, "Где у этого губы?");
        return;
    }
    if !lips_free(world, actor, target) {
        return;
    }
    let me = actor.mob == target;
    let (user, victim, name) = (
        name_of(world, actor.mob),
        name_of(world, target),
        name_of(world, item),
    );
    if me {
        visible(
            world,
            actor.mob,
            &format!("Вы аккуратно наносите {name}. Идеально!"),
            &format!("{user} красит губы с помощью {name}."),
            Span::Notice,
        );
        put_lipstick(world, target, &lipstick);
        return;
    }
    visible(
        world,
        actor.mob,
        &format!("Вы начинаете наносить {name} на губы {victim}..."),
        &format!("{user} начинает красить губы {victim} с помощью {name}."),
        Span::Warning,
    );
    do_after(
        world,
        actor,
        target,
        LIPSTICK_DS,
        0,
        Job::Barber(BarberJob::Lips),
    );
}

/// The mouth is uncovered and bare. Tells why not.
fn lips_free(world: &mut World, actor: Actor, target: Entity) -> bool {
    if crate::clothing::cover::mouth_covered(world, target).is_some() {
        let text = if actor.mob == target {
            "Снимите маску!"
        } else {
            "Снимите с цели маску!"
        };
        tell(world, actor.client, Span::Warning, text);
        return false;
    }
    if world
        .get::<HumanLook>(target)
        .is_some_and(|l| l.lip_style.is_some())
    {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Сначала нужно стереть старую помаду!",
        );
        return false;
    }
    true
}

/// `update_lips(style, lipstick_color, lipstick_trait)`.
fn put_lipstick(world: &mut World, target: Entity, lipstick: &Lipstick) {
    let (style, color) = (lipstick.style, lipstick.color);
    set_look(world, target, |look| {
        look.lip_style = Some(style.to_owned());
        look.lip_color = color;
    });
    // Read only while the lips are painted, so wiping them needs no cleanup.
    match lipstick.kiss {
        Some(kiss) => {
            world
                .entity_mut(target)
                .insert(crate::devices::hand_items::LipstickKiss(kiss));
        }
        None => {
            world
                .entity_mut(target)
                .remove::<crate::devices::hand_items::LipstickKiss>();
        }
    }
}

/// tg `clean_lips` (from `wash` and `clean_face`): an uncovered mouth loses its lipstick. True
/// when there was some.
pub fn clean_lips(world: &mut World, mob: Entity) -> bool {
    let painted = world
        .get::<HumanLook>(mob)
        .is_some_and(|l| l.lip_style.is_some());
    if !painted || crate::clothing::cover::mouth_covered(world, mob).is_some() {
        return false;
    }
    set_look(world, mob, |look| look.lip_style = None);
    true
}

/// `paper/attack` on the mouth: wipes the lipstick off. True when the target is a human.
fn wipe(world: &mut World, actor: Actor, target: Entity) -> bool {
    if world.get::<HumanLook>(target).is_none() {
        return false;
    }
    let paper = name_of(world, actor.item.expect("held"));
    if actor.mob == target {
        tell(
            world,
            actor.client,
            Span::Notice,
            format!("Вы стираете помаду с помощью {paper}."),
        );
        set_look(world, target, |look| look.lip_style = None);
        return true;
    }
    let (user, victim) = (name_of(world, actor.mob), name_of(world, target));
    visible(
        world,
        actor.mob,
        &format!("Вы начинаете стирать помаду с губ {victim}..."),
        &format!("{user} начинает стирать помаду с губ {victim} с помощью {paper}."),
        Span::Warning,
    );
    do_after(
        world,
        actor,
        target,
        WIPE_DS,
        0,
        Job::Barber(BarberJob::Wipe),
    );
    true
}

/// `dyespray/dye`: what can be dyed, then the question.
fn dye(world: &mut World, client: Entity, mob: Entity, spray: Entity, target: Entity) {
    if world.get::<HumanLook>(target).is_none() {
        return;
    }
    let hairy = hairy_head(world, target);
    // "Hair", "Facial Hair" (`TRAIT_BALD` takes the hair off the list).
    let dyables = vec![hairy && !bald_trait(world, target), hairy];
    if !dyables.iter().any(|&d| d) {
        let text = if target == mob {
            "Вам нечего красить.".to_owned()
        } else {
            format!("У {} нечего красить.", name_of(world, target))
        };
        tell(world, client, Span::Warning, text);
        return;
    }
    let buttons = [("Волосы", dyables[0]), ("Борода", dyables[1])]
        .into_iter()
        .filter(|&(_, d)| d)
        .map(|(n, _)| n.to_owned())
        .collect();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Краска для волос",
            "Что вы хотите покрасить?",
            PromptKind::Alert {
                buttons,
                autofocus: true,
            },
        ),
    );
    world.entity_mut(prompt).insert(BarberAsk::DyeWhat {
        spray,
        target,
        dyables,
    });
}

/// The look's current color of the hair or the beard, `#rrggbb`.
fn hex_of([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// `#rrggbb` (a sanitized color picker answer) as bytes.
fn rgb_of(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    Some(hex_rgb(value))
}

/// `can_perform_action(src, NEED_DEXTERITY)` with the spray: conscious, able, holding it; and
/// for the end, `IsReachableBy`.
fn can_spray(world: &World, actor: Actor, target: Entity) -> bool {
    can_act(world, actor, target)
}

/// What a finished grooming `do_after` does, checked again at the moment of effect.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: BarberJob) {
    let target = action.target;
    let actor = Actor {
        client: action.client,
        mob,
        item: action.item,
    };
    if world.get::<HumanLook>(target).is_none() {
        return;
    }
    let (user, victim) = (name_of(world, mob), name_of(world, target));
    let me = mob == target;
    match job {
        BarberJob::Shave { facial } => {
            if !groomable(world, actor.client, target, facial, true) {
                return;
            }
            set_look(world, target, |look| {
                if facial {
                    look.facial_hair_style = None;
                } else {
                    look.hair_style = Some(SKINHEAD.to_owned());
                }
            });
            sound(world, target, CLIPPERS_SOUND, 20.0);
            let (own, others) = match (me, facial) {
                (true, true) => (
                    "Вы заканчиваете бриться. Быстро и чисто!".to_owned(),
                    format!("{user} начисто сбривает себе бороду."),
                ),
                (true, false) => (
                    "Вы заканчиваете брить голову.".to_owned(),
                    format!("{user} бреет себе голову."),
                ),
                (false, true) => (
                    format!("Вы начисто сбриваете бороду {victim}."),
                    format!("{user} сбривает бороду {victim}."),
                ),
                (false, false) => (
                    format!("Вы обриваете голову {victim} наголо."),
                    format!("{user} обривает голову {victim} наголо!"),
                ),
            };
            let span = if me { Span::Notice } else { Span::Warning };
            visible(world, mob, &own, &others, span);
        }
        BarberJob::Style { facial, entry } => {
            let Some(style) = style_of(facial, entry) else {
                return;
            };
            if !groomable(world, actor.client, target, facial, false) {
                return;
            }
            set_look(world, target, |look| {
                let style = style.map(str::to_owned);
                if facial {
                    look.facial_hair_style = style;
                } else {
                    look.hair_style = style;
                }
            });
            let what = if facial {
                "бороду"
            } else {
                "причёску"
            };
            visible(
                world,
                mob,
                &format!("Вы успешно меняете {what} {victim}."),
                &format!("{user} успешно меняет {what} {victim}."),
                Span::Notice,
            );
        }
        BarberJob::Lips => {
            let Some(lipstick) = action.item.and_then(|i| world.get::<Lipstick>(i)).cloned() else {
                return;
            };
            if !lipstick.open || !lips_free(world, actor, target) {
                return;
            }
            let name = name_of(world, action.item.expect("lipstick"));
            visible(
                world,
                mob,
                &format!("Вы наносите {name} на губы {victim}."),
                &format!("{user} красит губы {victim} с помощью {name}."),
                Span::Notice,
            );
            put_lipstick(world, target, &lipstick);
        }
        BarberJob::Wipe => {
            visible(
                world,
                mob,
                &format!("Вы стираете помаду {victim}."),
                &format!("{user} стирает помаду с губ {victim}."),
                Span::Notice,
            );
            set_look(world, target, |look| look.lip_style = None);
        }
        BarberJob::Dye { facial, color } => {
            set_look(world, target, |look| {
                if facial {
                    look.facial_hair_color = color;
                } else {
                    look.hair_color = color;
                }
            });
            sound(world, target, SPRAY_SOUND, 10.0);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Mirrors.

fn obj_path(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmObj>(e).map(|o| o.path.as_str())
}

fn options_of(path: &str) -> &'static [MirrorOption] {
    if is_under(path, PRIDE_MIRROR) || is_under(path, HERETIC_MIRROR) {
        PRIDE_OPTIONS
    } else if is_under(path, MAGIC_MIRROR) {
        MAGIC_OPTIONS
    } else {
        INERT_OPTIONS
    }
}

fn is_magic(world: &World, mirror: Entity) -> bool {
    obj_path(world, mirror).is_some_and(|p| is_under(p, MAGIC_MIRROR))
}

/// tg `can_use_mirror`: the mirror whole and in reach, the user conscious and able to act.
pub fn can_use_mirror(world: &World, mob: Entity, mirror: Entity) -> bool {
    let whole = world
        .get::<crate::decor::Mirror>(mirror)
        .is_some_and(|m| !m.broken);
    whole
        && world.get::<HumanLook>(mob).is_some()
        && adjacent(world, mob, mirror)
        && world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
}

/// `mirror/attack_hand` -> `display_radial_menu`: the options (a list here). The caller checked
/// that `mirror` is a mirror.
pub fn open_mirror(world: &mut World, client: Entity, mob: Entity, mirror: Entity) {
    if !can_use_mirror(world, mob, mirror) {
        return;
    }
    let options = obj_path(world, mirror).map_or(INERT_OPTIONS, options_of);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Уход за собой",
            "Что вы хотите изменить?",
            PromptKind::List {
                items: options.iter().map(|o| o.label().to_owned()).collect(),
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(BarberAsk::Mirror { mirror });
}

fn ask_step(
    world: &mut World,
    client: Entity,
    mob: Entity,
    mirror: Entity,
    step: MirrorStep,
    message: impl Into<String>,
    kind: PromptKind,
) {
    let prompt = ask_world(world, Ask::new(client, mob, "Уход за собой", message, kind));
    world
        .entity_mut(prompt)
        .insert(BarberAsk::MirrorStep { mirror, step });
}

fn list(items: Vec<String>) -> PromptKind {
    PromptKind::List {
        items,
        default: None,
    }
}

fn color(default: [u8; 3]) -> PromptKind {
    PromptKind::Color {
        default: hex_of(default),
        autofocus: true,
    }
}

fn alert(buttons: &[&str]) -> PromptKind {
    PromptKind::Alert {
        buttons: buttons.iter().map(|b| (*b).to_owned()).collect(),
        autofocus: true,
    }
}

/// The species a magic mirror offers (`selectable_races`).
// shortcut: every magic mirror offers the roundstart species (tg's `MIRROR_MAGIC`,
// `MIRROR_PRIDE`, `MIRROR_BADMIN` flags are not on the port's species).
fn races() -> Vec<SpeciesId> {
    SpeciesId::roundstart().collect()
}

/// An option of the list picked: its first question (`change_*`). False when nothing was
/// asked (the menu comes back).
fn start_option(
    world: &mut World,
    client: Entity,
    mob: Entity,
    mirror: Entity,
    option: MirrorOption,
) -> bool {
    let Some(look) = world.get::<HumanLook>(mob).cloned() else {
        return false;
    };
    let magic = is_magic(world, mirror);
    let ask = |world: &mut World, step, message: &str, kind| {
        ask_step(world, client, mob, mirror, step, message, kind);
        true
    };
    match option {
        MirrorOption::Hair if magic => ask(
            world,
            MirrorStep::HairStyleOrColor,
            "Причёска или цвет волос?",
            alert(&["Причёска", "Цвет"]),
        ),
        MirrorOption::Hair => ask(
            world,
            MirrorStep::HairStyle,
            "Выберите причёску",
            list(hair_names()),
        ),
        MirrorOption::Beard if look.female && !magic => {
            if look.facial_hair_style.is_none() {
                tell(world, client, Span::Warning, "Нечего сбривать!");
                return false;
            }
            ask(
                world,
                MirrorStep::ShaveBeard,
                "Сбрить бороду?",
                alert(&["Да", "Нет"]),
            )
        }
        MirrorOption::Beard => ask(
            world,
            MirrorStep::Beard,
            "Выберите стиль бороды",
            list(facial_names()),
        ),
        MirrorOption::Race => ask(
            world,
            MirrorStep::Race,
            "Кем мы были?",
            list(races().iter().map(|s| s.def().name.to_owned()).collect()),
        ),
        MirrorOption::Sex => ask(
            world,
            MirrorStep::Sex,
            "Стать...",
            list(
                ["Чернокнижник", "Ведьма", "Волшебник", "Волшебнечто"]
                    .map(str::to_owned)
                    .to_vec(),
            ),
        ),
        MirrorOption::Eyes => ask(
            world,
            MirrorStep::Eyes,
            "Выберите цвет глаз",
            color(look.eye_color),
        ),
        MirrorOption::Name => {
            let name = name_of(world, mob);
            ask(
                world,
                MirrorStep::Name,
                "Кто мы такие?",
                PromptKind::Text {
                    default: name,
                    max_length: MAX_NAME_LEN,
                    multiline: false,
                },
            )
        }
    }
}

/// tg `sanitize_name(allow_numbers = TRUE)` of the trimmed `tgui_input_text` answer
/// (BandaStation's Cyrillic names).
fn sanitize_name(text: &str) -> Option<String> {
    ss13_shared::text::sanitize_name(text.trim(), true, true)
}

/// A mirror step answered. False when nothing more was asked (the menu comes back).
fn mirror_step(
    world: &mut World,
    client: Entity,
    mob: Entity,
    mirror: Entity,
    step: MirrorStep,
    answer: PromptAnswer,
) -> bool {
    let Some(look) = world.get::<HumanLook>(mob).cloned() else {
        return false;
    };
    let ask = |world: &mut World, step, message: &str, kind| {
        ask_step(world, client, mob, mirror, step, message, kind);
        true
    };
    match (step, answer) {
        (MirrorStep::HairStyleOrColor, PromptAnswer::Button(0)) => ask(
            world,
            MirrorStep::HairStyle,
            "Выберите причёску",
            list(hair_names()),
        ),
        (MirrorStep::HairStyleOrColor, PromptAnswer::Button(_)) => ask(
            world,
            MirrorStep::HairColor,
            "Выберите цвет волос",
            color(look.hair_color),
        ),
        (MirrorStep::HairStyle, PromptAnswer::Item(i)) => {
            let Some(style) = style_of(false, i) else {
                return false;
            };
            if bald_trait(world, mob) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Если бы отрастить волосы было так просто... Это напоминание вас удручает.",
                );
                return false;
            }
            set_look(world, mob, |l| l.hair_style = style.map(str::to_owned));
            false
        }
        (MirrorStep::HairColor, PromptAnswer::Color(hex)) => {
            let Some(rgb) = rgb_of(&hex) else {
                return false;
            };
            set_look(world, mob, |l| l.hair_color = rgb);
            if look.female {
                return false;
            }
            ask(
                world,
                MirrorStep::FacialHairColor,
                "Выберите цвет бороды",
                color(look.facial_hair_color),
            )
        }
        (MirrorStep::FacialHairColor, PromptAnswer::Color(hex)) => {
            if let Some(rgb) = rgb_of(&hex) {
                set_look(world, mob, |l| l.facial_hair_color = rgb);
            }
            false
        }
        (MirrorStep::ShaveBeard, PromptAnswer::Button(0)) => {
            set_look(world, mob, |l| l.facial_hair_style = None);
            false
        }
        (MirrorStep::Beard, PromptAnswer::Item(i)) => {
            if let Some(style) = style_of(true, i) {
                set_look(world, mob, |l| {
                    l.facial_hair_style = style.map(str::to_owned);
                });
            }
            false
        }
        (MirrorStep::Race, PromptAnswer::Item(i)) => {
            let Some(&species) = races().get(i) else {
                return false;
            };
            let def = species.def();
            ask(
                world,
                MirrorStep::RaceConfirm(species),
                &format!("Стать: {}?", def.name),
                alert(&["Да", "Нет"]),
            )
        }
        (MirrorStep::RaceConfirm(_), PromptAnswer::Button(1)) => ask(
            world,
            MirrorStep::Race,
            "Кем мы были?",
            list(races().iter().map(|s| s.def().name.to_owned()).collect()),
        ),
        (MirrorStep::RaceConfirm(species), PromptAnswer::Button(_)) => {
            change_species(world, mob, mirror, species);
            match species.def().limbs.color {
                LimbColor::Skin => ask(
                    world,
                    MirrorStep::SkinTone,
                    "Выберите тон кожи",
                    list(SKIN_TONES.iter().map(|(n, _)| (*n).to_owned()).collect()),
                ),
                LimbColor::Mutant => ask(
                    world,
                    MirrorStep::MutantColor,
                    "Выберите цвет кожи",
                    color(look.mutant_color),
                ),
                _ => false,
            }
        }
        (MirrorStep::SkinTone, PromptAnswer::Item(i)) => {
            if let Some(&(tone, _)) = SKIN_TONES.get(i) {
                set_look(world, mob, |l| l.skin_tone = tone.to_owned());
            }
            false
        }
        (MirrorStep::MutantColor, PromptAnswer::Color(hex)) => {
            let Some(rgb) = rgb_of(&hex) else {
                return false;
            };
            // `mutantcolors must be bright`.
            if Hsv::from_rgb(rgb).v >= 50.0 {
                set_look(world, mob, |l| l.mutant_color = rgb);
            } else {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Недопустимый цвет: он недостаточно яркий.",
                );
            }
            false
        }
        (MirrorStep::Sex, PromptAnswer::Item(i)) => {
            let text = match i {
                0 => "Чувак, да вы чувствуете себя мужчиной!",
                1 => "Чувак, да вы чувствуете себя женщиной!",
                2 => "Ого, чувак, вы чувствуете себя чуваком!",
                _ => "Ого, чувак, вы чувствуете себя чем-то иным!",
            };
            tell(world, client, Span::Notice, text);
            ask(
                world,
                MirrorStep::Physique,
                "Изменить и телосложение?",
                list(
                    [
                        "Телосложение чернокнижника",
                        "Телосложение ведьмы",
                        "Волшебникам пол не нужен",
                    ]
                    .map(str::to_owned)
                    .to_vec(),
                ),
            )
        }
        (MirrorStep::Physique, PromptAnswer::Item(i)) if i < 2 => {
            let sexes = look.species.def().sexes;
            set_look(world, mob, |l| l.female = i == 1 && sexes);
            false
        }
        (MirrorStep::Eyes, PromptAnswer::Color(hex)) => {
            if let Some(rgb) = rgb_of(&hex) {
                set_look(world, mob, |l| l.eye_color = rgb);
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы смотрите на свои новые глаза своими новыми глазами. Идеально!",
                );
            }
            false
        }
        (MirrorStep::Name, PromptAnswer::Text(text)) => {
            if let Some(name) = sanitize_name(&text) {
                world.entity_mut(mob).insert(Name::new(name));
            }
            false
        }
        _ => false,
    }
}

/// `on_species_change` (the heretic's mirror breaks when the species changes) and
/// `set_species`.
fn change_species(world: &mut World, mob: Entity, mirror: Entity, species: SpeciesId) {
    let old = world.get::<HumanLook>(mob).map(|l| l.species);
    if old != Some(species) && obj_path(world, mirror).is_some_and(|p| is_under(p, HERETIC_MIRROR))
    {
        crate::decor::atom_break(world, mirror);
    }
    crate::species::set_species(world, mob, species);
    crate::inventory::refresh_look(world, mob);
}

/// `heretic/pre_change` -> `change_something` for a non-heretic: the picked part changes at
/// random. True when the change was taken over.
fn heretic_trance(
    world: &mut World,
    client: Entity,
    mob: Entity,
    mirror: Entity,
    option: MirrorOption,
) -> bool {
    let heretic_mirror = obj_path(world, mirror).is_some_and(|p| is_under(p, HERETIC_MIRROR));
    if !heretic_mirror || crate::antag::heretic::server::is_heretic(world, mob) {
        return false;
    }
    let Some(look) = world.get::<HumanLook>(mob).cloned() else {
        return true;
    };
    let mut roll = |n: usize| {
        world
            .get_resource_mut::<crate::combat::CombatRng>()
            .map_or(0, |mut rng| rng.range(0, n as u32) as usize)
            .min(n.saturating_sub(1))
    };
    let (text, change): (&str, Box<dyn FnOnce(&mut World)>) = match option {
        MirrorOption::Hair | MirrorOption::Beard => {
            let hair = HAIR_STYLES[roll(HAIR_STYLES.len())];
            // `random_facial_hairstyle(gender)`: shaved for a female.
            let beard = if look.female {
                None
            } else {
                Some(FACIAL_HAIR_STYLES[roll(FACIAL_HAIR_STYLES.len())])
            };
            (
                "Волосы — да, волосы не те — их нужно изменить...",
                Box::new(move |world: &mut World| {
                    set_look(world, mob, |l| {
                        l.hair_style = Some(hair.to_owned());
                        l.facial_hair_style = beard.map(str::to_owned);
                    });
                }),
            )
        }
        MirrorOption::Race => {
            let options: Vec<SpeciesId> =
                races().into_iter().filter(|&s| s != look.species).collect();
            let pick = options.get(roll(options.len())).copied();
            (
                "Тело — нет, тело совсем не то — я должен стать иным...",
                Box::new(move |world: &mut World| {
                    if let Some(species) = pick {
                        change_species(world, mob, mirror, species);
                    }
                }),
            )
        }
        MirrorOption::Sex => (
            "Мой облик — да, облик совсем не тот — его нужно изменить...",
            Box::new(move |world: &mut World| {
                let sexes = look.species.def().sexes;
                set_look(world, mob, |l| l.female = !l.female && sexes);
            }),
        ),
        MirrorOption::Eyes => {
            let eyes = hex_rgb(EYE_COLORS[roll(EYE_COLORS.len())]);
            (
                "Мои глаза — я не вижу ясно — они оба не те...",
                Box::new(move |world: &mut World| {
                    set_look(world, mob, |l| l.eye_color = eyes);
                }),
            )
        }
        MirrorOption::Name => (
            "Моё имя — оно мне не подходит...",
            Box::new(|_: &mut World| {}),
        ),
    };
    tell(
        world,
        client,
        Span::Warning,
        format!(
            "Вы не можете оторвать взгляд от {}...",
            name_of(world, mirror)
        ),
    );
    tell(world, client, Span::Warning, text);
    change(world);
    true
}

/// `wallframe/try_build` + `attach` of a mirror frame: on a wall next to the user (in a cardinal
/// direction), its mirror on the user's tile, shifted onto that wall. True when `target` is a
/// wall.
fn hang_frame(
    world: &mut World,
    client: Entity,
    mob: Entity,
    frame: Entity,
    path: &str,
    target: Entity,
) -> bool {
    let is_wall = world
        .get::<ss13_mapload::DmTurfBuild>(target)
        .is_some_and(|t| is_under(&t.path, WALL));
    if !is_wall {
        return false;
    }
    let (Some(&wall), Some(&at)) = (world.get::<TilePos>(target), world.get::<TilePos>(mob)) else {
        return true;
    };
    let Some(dir) =
        crate::pull::dir_between(at, wall).filter(|d| ss13_shared::Dir::CARDINALS.contains(d))
    else {
        tell(
            world,
            client,
            Span::Warning,
            "Нужно стоять прямо перед стеной!",
        );
        return true;
    };
    let (dx, dy) = dir.offset();
    let offset = [(dx * MIRROR_SHIFT) as i16, (dy * MIRROR_SHIFT) as i16];
    let taken = world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(at)
        .iter()
        .any(|&e| {
            world.get::<crate::decor::Mirror>(e).is_some()
                && world
                    .get::<Appearance>(e)
                    .is_some_and(|a| a.pixel_offset == offset)
        });
    if taken {
        tell(world, client, Span::Warning, "Здесь уже что-то висит!");
        return true;
    }
    // `result_path` of `/obj/item/wallframe/mirror`, `/broken`, `/heretic`.
    let result = match &path[MIRROR_FRAME.len()..] {
        s if s.starts_with("/broken") => "/obj/structure/mirror/broken",
        s if s.starts_with("/heretic") => HERETIC_MIRROR,
        _ => "/obj/structure/mirror",
    };
    let Some(mirror) = crate::construction::spawn_obj(world, result, at) else {
        return true;
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(mirror) {
        appearance.pixel_offset = offset;
    }
    world.despawn(frame);
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        for hand in inventory.hands.iter_mut() {
            if *hand == Some(frame) {
                *hand = None;
            }
        }
    }
    let (user, name) = (name_of(world, mob), name_of(world, mirror));
    visible(
        world,
        mob,
        &format!("Вы закрепляете {name} на стене."),
        &format!("{user} закрепляет {name} на стене."),
        Span::Notice,
    );
    sound(world, mirror, CLICK_SOUND, 75.0);
    true
}

/// The answers of the grooming questions, re-checked at the moment of effect.
fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answers {
        let Some(ask) = world.get::<BarberAsk>(prompt).cloned() else {
            continue;
        };
        answer_one(world, client, mob, ask, answer);
    }
}

fn answer_one(
    world: &mut World,
    client: Entity,
    mob: Entity,
    ask: BarberAsk,
    answer: Option<PromptAnswer>,
) {
    match ask {
        BarberAsk::RazorStyle {
            razor,
            target,
            facial,
        } => {
            let Some(PromptAnswer::Item(entry)) = answer else {
                return;
            };
            let actor = Actor {
                client,
                mob,
                item: Some(razor),
            };
            razor_style(world, actor, target, facial, entry);
        }
        BarberAsk::LipStyle { lipstick } => {
            let Some(PromptAnswer::Item(i)) = answer else {
                return;
            };
            // `check_menu`: able and still holding it.
            let holding = world.get::<CarriedBy>(lipstick).map(|c| c.0) == Some(mob);
            if !holding || crate::status_effects::incapacitation(world, mob).incapacitated {
                return;
            }
            if let (Some(&(_, style)), Some(mut l)) =
                (LIP_STYLES.get(i), world.get_mut::<Lipstick>(lipstick))
            {
                l.style = style;
            }
        }
        BarberAsk::DyeWhat {
            spray,
            target,
            dyables,
        } => {
            let Some(PromptAnswer::Button(i)) = answer else {
                return;
            };
            // The buttons were the dyable parts in order.
            let Some(facial) = dyables
                .iter()
                .enumerate()
                .filter(|(_, d)| **d)
                .nth(i)
                .map(|(part, _)| part == 1)
            else {
                return;
            };
            let actor = Actor {
                client,
                mob,
                item: Some(spray),
            };
            if !can_spray(world, actor, target) {
                return;
            }
            let Some(look) = world.get::<HumanLook>(target) else {
                return;
            };
            let current = if facial {
                look.facial_hair_color
            } else {
                look.hair_color
            };
            let prompt = ask_world(
                world,
                Ask::new(
                    client,
                    mob,
                    "Краска для волос",
                    "Выберите цвет краски:",
                    color(current),
                ),
            );
            world.entity_mut(prompt).insert(BarberAsk::DyeColor {
                spray,
                target,
                facial,
            });
        }
        BarberAsk::DyeColor {
            spray,
            target,
            facial,
        } => {
            let Some(PromptAnswer::Color(hex)) = answer else {
                return;
            };
            let Some(rgb) = rgb_of(&hex) else {
                return;
            };
            let actor = Actor {
                client,
                mob,
                item: Some(spray),
            };
            if !can_spray(world, actor, target) {
                return;
            }
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете наносить краску...",
            );
            do_after(
                world,
                actor,
                target,
                DYE_DS,
                0,
                Job::Barber(BarberJob::Dye { facial, color: rgb }),
            );
        }
        BarberAsk::Mirror { mirror } => {
            let Some(PromptAnswer::Item(i)) = answer else {
                return;
            };
            let options = obj_path(world, mirror).map_or(INERT_OPTIONS, options_of);
            let Some(&option) = options.get(i) else {
                return;
            };
            if !can_use_mirror(world, mob, mirror) {
                return;
            }
            if heretic_trance(world, client, mob, mirror, option) {
                return;
            }
            if !start_option(world, client, mob, mirror, option) {
                open_mirror(world, client, mob, mirror);
            }
        }
        BarberAsk::MirrorStep { mirror, step } => {
            // tg's steps bail on a closed prompt or a mirror out of use; the menu comes back
            // after any of them (`display_radial_menu` loops).
            let asked = match answer {
                Some(answer) if can_use_mirror(world, mob, mirror) => {
                    mirror_step(world, client, mob, mirror, step, answer)
                }
                _ => false,
            };
            if !asked {
                open_mirror(world, client, mob, mirror);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_entries_map_to_looks() {
        assert_eq!(style_of(false, 0), Some(None));
        assert_eq!(style_of(false, 1), Some(Some(HAIR_STYLES[0])));
        assert_eq!(
            style_of(true, FACIAL_HAIR_STYLES.len()),
            Some(FACIAL_HAIR_STYLES.last().copied())
        );
        assert_eq!(style_of(true, FACIAL_HAIR_STYLES.len() + 1), None);
        assert_eq!(hair_names().len(), HAIR_STYLES.len() + 1);
        assert_eq!(facial_names().len(), FACIAL_HAIR_STYLES.len() + 1);
    }

    #[test]
    fn colors_round_trip_and_names_are_sanitized() {
        assert_eq!(
            rgb_of(&hex_of([0x12, 0xab, 0xff])),
            Some([0x12, 0xab, 0xff])
        );
        assert_eq!(rgb_of("#12ab"), None);
        assert_eq!(rgb_of("12abff"), None);
        assert_eq!(
            sanitize_name("  вася пупкин ").as_deref(),
            Some("Вася Пупкин")
        );
        assert_eq!(sanitize_name("Вася   Пупкин"), None);
        assert_eq!(sanitize_name(" \n "), None);
    }

    #[test]
    fn mirror_options_follow_the_type() {
        assert_eq!(options_of("/obj/structure/mirror"), INERT_OPTIONS);
        assert_eq!(options_of("/obj/structure/mirror/broken"), INERT_OPTIONS);
        assert_eq!(options_of("/obj/structure/mirror/magic"), MAGIC_OPTIONS);
        assert_eq!(
            options_of("/obj/structure/mirror/magic/lesser"),
            MAGIC_OPTIONS
        );
        assert_eq!(options_of(HERETIC_MIRROR), PRIDE_OPTIONS);
        assert_eq!(options_of(PRIDE_MIRROR), PRIDE_OPTIONS);
        assert!(interacts_with_mobs("/obj/item/lipstick/black/death"));
        assert!(interacts_with_mobs(RAZOR) && !interacts_with_mobs(PAPER));
    }
}
