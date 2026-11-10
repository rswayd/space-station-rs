//! Security gear (server): the Compli-o-nator of security gas masks
//! (`code/modules/clothing/masks/hailer.dm`, `code/datums/hailer_phrase.dm`), the police whistle
//! and the party horn, the security helmet's straps and visor (`head/helmet.dm`
//! `/obj/item/clothing/head/helmet/sec`), and what security suits hold in their suit storage
//! (`GLOB.security_vest_allowed` and its neighbours, `code/__DEFINES/inventory.dm`).
//!
//! - The "HALT!" button of a worn security mask plays a phrase every 3 s, picked by the
//!   restrictor (`aggressiveness`: good cop phrases 2-6, bad cop 2-12, the SWAT mask's 2-19; cut,
//!   13-19); an emagged one only swears. Used 3-4 times within 18 s it warns that it heats up,
//!   the fifth time it breaks for good. A screwdriver cycles the restrictor, wirecutters break it.
//! - The whistle ("HALT!", any slot or hand, every 10 s) and the party horn ("TOOT!", worn).
//! - The security helmet: in the hand its straps loosen or tighten after 3 s (`HIDEHAIR`);
//!   alt-click flips its visor (`-novisor` sprite, no longer covering the eyes). Its seclite
//!   mount is `crate::gun_attachments`'.
//! - Armor vests and the other suits whose `Initialize` sets `allowed` from a global list hold
//!   that list's items in their suit storage ([`suit_allows`]).
// shortcut: no `voice_filter`/TTS of the masks (no TTS), no `emote_sounds` deathgasp, the party
// horn's `flick` animation is not played.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmClothing, DmItem};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::InventorySlot;

use super::cover::{HEADCOVERSEYES, HIDEHAIR};
use crate::health::{DEFAULT_MESSAGE_RANGE, Stat};
use crate::inventory::Inventory;
use crate::security::name_of;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, ToolKind, now, tell, tool_of};

pub const HAILER: &str = "/obj/item/clothing/mask/gas/sechailer";
const SWAT_HAILER: &str = "/obj/item/clothing/mask/gas/sechailer/swat";
const CYBORG_HAILER: &str = "/obj/item/clothing/mask/gas/sechailer/cyborg";
const WHISTLE: &str = "/obj/item/clothing/mask/whistle";
const PARTY_HORN: &str = "/obj/item/clothing/mask/party_horn";
pub const SEC_HELMET: &str = "/obj/item/clothing/head/helmet/sec";
pub const HALT: &str = "HALT!";
pub const TOOT: &str = "TOOT!";

/// `PHRASE_COOLDOWN`, `OVERUSE_COOLDOWN`, seconds.
const PHRASE_COOLDOWN: f64 = 3.0;
const OVERUSE_COOLDOWN: f64 = 18.0;
/// `whistle_cooldown`, `horn_cooldown`.
const WHISTLE_COOLDOWN: f64 = 10.0;
/// The straps' `do_after`, deciseconds.
const STRAPS_DS: f32 = 30.0;
/// `AGGR_GOOD_COP` .. `AGGR_BROKEN`.
const AGGR_GOOD_COP: u8 = 1;
const AGGR_BAD_COP: u8 = 2;
const AGGR_SHIT_COP: u8 = 3;
const AGGR_BROKEN: u8 = 4;
/// `GOOD_COP_PHRASES`, `BAD_COP_PHRASES`, `BROKE_PHRASES`, `ALL_PHRASES` (1-based indices).
const GOOD_COP_PHRASES: u32 = 6;
const BAD_COP_PHRASES: u32 = 12;
const BROKE_PHRASES: u32 = 13;
const ALL_PHRASES: u32 = 19;
const WHISTLE_SOUND: &str = "sound/items/whistle/whistle.ogg";
const PARTY_HORN_SOUND: &str = "sound/items/party_horn.ogg";

/// `GLOB.hailer_phrases`: `phrase_text` and the `sound/runtime/complionator/` file.
const PHRASES: [(&str, &str); 19] = [
    (
        "FUCK YOUR CUNT YOU SHIT EATING COCKSTORM AND EAT A DONG FUCKING ASS RAMMING SHIT FUCK EAT \
         PENISES IN YOUR FUCK FACE AND SHIT OUT ABORTIONS OF FUCK AND POO AND SHIT IN YOUR ASS YOU \
         COCK FUCK SHIT MONKEY FUCK ASS WANKER FROM THE DEPTHS OF SHIT.",
        "sound/runtime/complionator/emag.ogg",
    ),
    ("HALT! HALT! HALT!", "sound/runtime/complionator/halt.ogg"),
    (
        "Stop in the name of the Law.",
        "sound/runtime/complionator/bobby.ogg",
    ),
    (
        "Compliance is in your best interest.",
        "sound/runtime/complionator/compliance.ogg",
    ),
    (
        "Prepare for justice!",
        "sound/runtime/complionator/justice.ogg",
    ),
    (
        "Running will only increase your sentence.",
        "sound/runtime/complionator/running.ogg",
    ),
    (
        "Don't move, Creep!",
        "sound/runtime/complionator/dontmove.ogg",
    ),
    (
        "Down on the floor, Creep!",
        "sound/runtime/complionator/floor.ogg",
    ),
    (
        "Dead or alive you're coming with me.",
        "sound/runtime/complionator/robocop.ogg",
    ),
    (
        "God made today for the crooks we could not catch yesterday.",
        "sound/runtime/complionator/god.ogg",
    ),
    ("Freeze, Scum Bag!", "sound/runtime/complionator/freeze.ogg"),
    (
        "Stop right there, criminal scum!",
        "sound/runtime/complionator/imperial.ogg",
    ),
    (
        "Stop or I'll bash you.",
        "sound/runtime/complionator/bash.ogg",
    ),
    (
        "Go ahead, make my day.",
        "sound/runtime/complionator/harry.ogg",
    ),
    (
        "Stop breaking the law, ass hole.",
        "sound/runtime/complionator/asshole.ogg",
    ),
    (
        "You have the right to shut the fuck up.",
        "sound/runtime/complionator/stfu.ogg",
    ),
    ("Shut up crime!", "sound/runtime/complionator/shutup.ogg"),
    (
        "Face the wrath of the golden bolt.",
        "sound/runtime/complionator/super.ogg",
    ),
    ("I am, the LAW!", "sound/runtime/complionator/dredd.ogg"),
];

/// `GLOB.security_vest_allowed`.
const SECURITY_VEST_ALLOWED: &[&str] = &[
    "/obj/item/flashlight",
    "/obj/item/gun/ballistic",
    "/obj/item/gun/energy",
    "/obj/item/knife/combat",
    "/obj/item/melee/baton",
    "/obj/item/reagent_containers/spray/pepper",
    "/obj/item/restraints/handcuffs",
    "/obj/item/tank/internals/emergency_oxygen",
    "/obj/item/tank/internals/plasmaman",
    "/obj/item/vaporizer",
    "/obj/item/storage/belt/holster/detective",
    "/obj/item/storage/belt/holster/nukie",
    "/obj/item/storage/belt/holster/energy",
    "/obj/item/gun/ballistic/shotgun/automatic/combat/compact",
    "/obj/item/gun/energy/laser/scatter/shotty/compact",
    "/obj/item/pen/red/security",
];
/// `GLOB.detective_vest_allowed`.
const DETECTIVE_VEST_ALLOWED: &[&str] = &[
    "/obj/item/detective_scanner",
    "/obj/item/flashlight",
    "/obj/item/gun/ballistic",
    "/obj/item/gun/energy",
    "/obj/item/lighter",
    "/obj/item/melee/baton",
    "/obj/item/reagent_containers/spray/pepper",
    "/obj/item/restraints/handcuffs",
    "/obj/item/storage/fancy/cigarettes",
    "/obj/item/taperecorder",
    "/obj/item/tank/internals/emergency_oxygen",
    "/obj/item/tank/internals/plasmaman",
    "/obj/item/vaporizer",
    "/obj/item/storage/belt/holster/detective",
    "/obj/item/storage/belt/holster/nukie",
    "/obj/item/storage/belt/holster/energy",
    "/obj/item/gun/ballistic/shotgun/automatic/combat/compact",
    "/obj/item/gun/energy/laser/scatter/shotty/compact",
];
/// `GLOB.security_wintercoat_allowed`.
const SECURITY_WINTERCOAT_ALLOWED: &[&str] = &[
    "/obj/item/gun/ballistic",
    "/obj/item/gun/energy",
    "/obj/item/melee/baton",
    "/obj/item/reagent_containers/spray/pepper",
    "/obj/item/restraints/handcuffs",
    "/obj/item/storage/belt/holster/detective",
    "/obj/item/storage/belt/holster/nukie",
    "/obj/item/storage/belt/holster/energy",
    "/obj/item/gun/ballistic/shotgun/automatic/combat/compact",
    "/obj/item/gun/energy/laser/scatter/shotty/compact",
];

/// How a suit's `Initialize` sets `allowed` from a global list (the dump only has the type's
/// own list: global lists are not evaluated there).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Allowed {
    /// `allowed = GLOB.x`.
    Replace,
    /// `if(!allowed) allowed = GLOB.x` (`/obj/item/clothing/suit/armor`).
    ReplaceIfNull,
    /// `allowed += GLOB.x`.
    Add,
}

/// Suits with such an `Initialize`, most specific first (the first match wins).
const SUIT_ALLOWED: [(&str, Allowed, &[&str]); 18] = [
    (
        "/obj/item/clothing/suit/armor/vest/det_suit",
        Allowed::Replace,
        DETECTIVE_VEST_ALLOWED,
    ),
    (
        "/obj/item/clothing/suit/armor",
        Allowed::ReplaceIfNull,
        SECURITY_VEST_ALLOWED,
    ),
    (
        "/obj/item/clothing/suit/toggle/jacket/det_trench",
        Allowed::Replace,
        DETECTIVE_VEST_ALLOWED,
    ),
    (
        "/obj/item/clothing/suit/jacket/det_suit",
        Allowed::Replace,
        DETECTIVE_VEST_ALLOWED,
    ),
    (
        "/obj/item/clothing/suit/hooded/ablative",
        Allowed::Replace,
        SECURITY_VEST_ALLOWED,
    ),
    (
        "/obj/item/clothing/suit/costume/justice",
        Allowed::Replace,
        SECURITY_VEST_ALLOWED,
    ),
    (
        "/obj/item/clothing/suit/toggle/owlwings",
        Allowed::Replace,
        SECURITY_VEST_ALLOWED,
    ),
    (
        "/obj/item/clothing/suit/chameleon",
        Allowed::Replace,
        SECURITY_VEST_ALLOWED,
    ),
    // BandaStation `modular_bandastation/objects/code/items/clothing/suits/armor.dm`.
    (
        "/obj/item/clothing/suit/chaplainsuit/armor/crusader/ert",
        Allowed::Replace,
        SECURITY_VEST_ALLOWED,
    ),
    (
        "/obj/item/clothing/suit/apron/ert",
        Allowed::Replace,
        SECURITY_VEST_ALLOWED,
    ),
    (
        "/obj/item/clothing/suit/bio_suit/security",
        Allowed::Add,
        SECURITY_VEST_ALLOWED,
    ),
    (
        "/obj/item/clothing/suit/costume/pirate/armored",
        Allowed::Add,
        SECURITY_VEST_ALLOWED,
    ),
    (
        "/obj/item/clothing/suit/jacket/leather/armored",
        Allowed::Add,
        SECURITY_VEST_ALLOWED,
    ),
    (
        "/obj/item/clothing/suit/hooded/wintercoat/centcom",
        Allowed::Add,
        SECURITY_WINTERCOAT_ALLOWED,
    ),
    (
        "/obj/item/clothing/suit/hooded/wintercoat/captain",
        Allowed::Add,
        SECURITY_WINTERCOAT_ALLOWED,
    ),
    (
        "/obj/item/clothing/suit/hooded/wintercoat/hop",
        Allowed::Add,
        SECURITY_WINTERCOAT_ALLOWED,
    ),
    (
        "/obj/item/clothing/suit/hooded/wintercoat/security",
        Allowed::Add,
        SECURITY_WINTERCOAT_ALLOWED,
    ),
    // BandaStation `modular_bandastation/objects/code/items/clothing/suits/wintercoats.dm`.
    (
        "/obj/item/clothing/suit/hooded/wintercoat/blueshield",
        Allowed::Add,
        SECURITY_WINTERCOAT_ALLOWED,
    ),
];

/// The suit's `allowed` after its `Initialize`: does it take `item_path` into suit storage?
/// `own` is the type's converted list (`null` converts to an empty one).
pub fn suit_allows(suit_path: &str, own: &[String], item_path: &str) -> bool {
    let rule = SUIT_ALLOWED
        .iter()
        .find(|(root, ..)| is_under(suit_path, root));
    let global = |list: &[&str]| list.iter().any(|r| is_under(item_path, r));
    let own_allows = || own.iter().any(|r| is_under(item_path, r));
    match rule {
        Some(&(_, Allowed::Replace, list)) => global(list),
        Some(&(_, Allowed::ReplaceIfNull, list)) if own.is_empty() => global(list),
        Some(&(_, Allowed::Add, list)) => own_allows() || global(list),
        _ => own_allows(),
    }
}

// --- The Compli-o-nator ----------------------------------------------------------------------

/// A security mask's hailer (`aggressiveness`, `broken_hailer`, `recent_uses`,
/// `overuse_cooldown`, `safety`, `hailer_cooldown`); made on first use from the type's defaults.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Hailer {
    pub aggressiveness: u8,
    pub broken: bool,
    pub recent_uses: u32,
    /// When the overuse window (`overuse_cooldown`) ends; 0 = none running.
    pub overuse_until: f64,
    pub cooldown_until: f64,
    pub safety: bool,
}

impl Hailer {
    fn of_type(path: &str) -> Self {
        let aggressiveness = if is_under(path, SWAT_HAILER) {
            AGGR_SHIT_COP
        } else if is_under(path, CYBORG_HAILER) {
            AGGR_GOOD_COP
        } else {
            AGGR_BAD_COP
        };
        Self {
            aggressiveness,
            broken: false,
            recent_uses: 0,
            overuse_until: 0.0,
            cooldown_until: 0.0,
            safety: true,
        }
    }

    /// `select_phrase`: the 1-based index into [`PHRASES`] for a roll `roll(lo, hi)`.
    pub fn select_phrase(&self, roll: impl FnOnce(u32, u32) -> u32) -> u32 {
        if !self.safety {
            return 1;
        }
        let upper = match self.aggressiveness {
            AGGR_GOOD_COP => GOOD_COP_PHRASES,
            AGGR_BAD_COP => BAD_COP_PHRASES,
            _ => ALL_PHRASES,
        };
        let lower = if self.aggressiveness == AGGR_BROKEN {
            BROKE_PHRASES
        } else {
            2
        };
        roll(lower, upper)
    }

    /// `halt`'s overuse count at `now`: what it tells and whether it overloads now.
    pub fn count_use(&mut self, now: f64) -> Option<(Span, &'static str)> {
        self.recent_uses += 1;
        if now >= self.overuse_until {
            self.recent_uses = 0;
            self.overuse_until = now + OVERUSE_COOLDOWN;
        }
        match self.recent_uses {
            3 => Some((Span::Warning, "начинает нагреваться.")),
            4 => Some((Span::BoldWarning, "опасно нагревается от перегрузки!")),
            5 => {
                self.broken = true;
                Some((Span::BoldWarning, "перегружает модулятор и ломается."))
            }
            _ => None,
        }
    }
}

fn path(world: &World, item: Entity) -> Option<&str> {
    world.get::<DmItem>(item).map(|d| d.path.as_str())
}

pub fn is_hailer(world: &World, item: Entity) -> bool {
    path(world, item).is_some_and(|p| is_under(p, HAILER))
}

fn hailer(world: &World, item: Entity) -> Option<Hailer> {
    let p = path(world, item)?;
    if !is_under(p, HAILER) {
        return None;
    }
    Some(
        world
            .get::<Hailer>(item)
            .copied()
            .unwrap_or_else(|| Hailer::of_type(p)),
    )
}

/// tg `audible_message` from `mob`, with `loud` in big red letters after `lead`.
fn audible(world: &mut World, mob: Entity, lead: String, loud: &str) {
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return;
    };
    let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
    if clients.is_empty() {
        return;
    }
    world.write_message(SendChat {
        clients,
        message: ChatMessage {
            channel: ChatChannel::Local,
            segments: vec![
                ChatSegment::new(Span::Plain, lead),
                ChatSegment::new(Span::Danger, loud),
            ],
        },
    });
}

fn sound(world: &mut World, item: Entity, file: &str, vol: f32, extrarange: f32) {
    if let Some(pos) = crate::explosions::turf_of(world, item) {
        let mut s = PlaySound::new(pos, file, vol, false);
        s.extrarange = extrarange;
        playsound(world, s);
    }
}

/// tg `sechailer/halt` by `mob` (checked by the caller: conscious, the mask in its slot).
pub fn halt(world: &mut World, client: Entity, mob: Entity, item: Entity) {
    let Some(mut state) = hailer(world, item) else {
        return;
    };
    let now = now(world);
    if now < state.cooldown_until {
        return;
    }
    let name = name_of(world, item);
    if state.broken {
        tell(
            world,
            client,
            Span::Warning,
            format!("Система оповещения {name} сломана."),
        );
        return;
    }
    let overuse = state.count_use(now);
    let broke = state.broken;
    if !broke {
        let index = state.select_phrase(|lo, hi| {
            world
                .resource_mut::<crate::combat::CombatRng>()
                .range(lo, hi)
        });
        let (text, file) = PHRASES[(index as usize - 1).min(PHRASES.len() - 1)];
        // `play_phrase`.
        state.cooldown_until = now + PHRASE_COOLDOWN;
        let who = name_of(world, mob);
        audible(world, mob, format!("Compli-o-Nator {who}: "), text);
        sound(world, item, file, 100.0, 4.0);
    }
    world.entity_mut(item).insert(state);
    if let Some((span, text)) = overuse {
        tell(
            world,
            client,
            span,
            format!("{} {text}", ss13_shared::capitalize(&name)),
        );
    }
}

/// tg `sechailer/screwdriver_act` and `wirecutter_act` with `tool` on `item`. False when it
/// is not a hailer and a fitting tool.
pub fn tool_act(world: &mut World, client: Entity, tool: Entity, item: Entity) -> bool {
    let Some(mut state) = hailer(world, item) else {
        return false;
    };
    match tool_of(world, tool).map(|(k, _)| k) {
        Some(ToolKind::Screwdriver) => {
            if state.aggressiveness == AGGR_BROKEN {
                tell(
                    world,
                    client,
                    Span::Danger,
                    "Вы подкручиваете ограничитель, но ничего не происходит: похоже, он сломан.",
                );
                return true;
            }
            let position = match state.aggressiveness {
                AGGR_GOOD_COP => "среднее",
                AGGR_BAD_COP => "последнее",
                _ => "первое",
            };
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы переводите ограничитель в {position} положение."),
            );
            state.aggressiveness = state.aggressiveness % 3 + 1;
        }
        Some(ToolKind::Wirecutters) => {
            if state.aggressiveness != AGGR_BROKEN {
                tell(world, client, Span::Danger, "Вы сломали ограничитель!");
                state.aggressiveness = AGGR_BROKEN;
            }
        }
        _ => return false,
    }
    world.entity_mut(item).insert(state);
    true
}

/// tg `sechailer/emag_act`: the vocal circuit fries once.
pub fn emag(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(mut state) = hailer(world, item) else {
        return false;
    };
    if !state.safety {
        return false;
    }
    state.safety = false;
    world.entity_mut(item).insert(state);
    crate::tools::tell_mob(world, mob, Span::Notice, "речевая цепь выжжена");
    true
}

/// The whistle's and party horn's last use (`whistle_cooldown`, `horn_cooldown`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct NoiseCooldown(pub f64);

fn noise(world: &mut World, mob: Entity, item: Entity, whistle: bool) {
    let now = now(world);
    if world.get::<NoiseCooldown>(item).is_some_and(|c| now < c.0) {
        return;
    }
    world
        .entity_mut(item)
        .insert(NoiseCooldown(now + WHISTLE_COOLDOWN));
    if whistle {
        let who = name_of(world, mob);
        audible(world, mob, format!("{who}: "), "HALT!");
        sound(world, item, WHISTLE_SOUND, 50.0, 4.0);
    } else {
        sound(world, item, PARTY_HORN_SOUND, 75.0, 0.0);
    }
}

/// The buttons of the gear here: "HALT!" of a worn hailer and of a whistle anywhere
/// (`action_slots = ALL`), "TOOT!" of a worn party horn.
pub fn buttons(world: &World, slot: InventorySlot, item: Entity) -> Vec<&'static str> {
    let Some(p) = path(world, item) else {
        return Vec::new();
    };
    let worn = world
        .get::<DmItem>(item)
        .is_some_and(|dm| dm.slot_flags & crate::inventory::slot_bit(slot) != 0);
    let has = |action: &str| {
        world
            .get::<DmClothing>(item)
            .is_some_and(|c| c.actions.iter().any(|a| a == action))
    };
    if is_under(p, WHISTLE)
        || (is_under(p, HAILER) && worn && has("/datum/action/item_action/halt"))
    {
        vec![HALT]
    } else if is_under(p, PARTY_HORN) && worn {
        vec![TOOT]
    } else {
        Vec::new()
    }
}

/// One of [`buttons`] clicked, validated by the caller.
pub fn trigger(world: &mut World, client: Entity, mob: Entity, item: Entity, name: &str) -> bool {
    let Some(p) = path(world, item).map(str::to_owned) else {
        return false;
    };
    match name {
        HALT if is_under(&p, HAILER) => halt(world, client, mob, item),
        HALT if is_under(&p, WHISTLE) => noise(world, mob, item, true),
        TOOT if is_under(&p, PARTY_HORN) => noise(world, mob, item, false),
        _ => return false,
    }
    true
}

// --- The security helmet ---------------------------------------------------------------------

/// `flipped_visor`, with the worn state the helmet had before.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct VisorFlipped {
    worn_icon_state: String,
}

fn sec_helmet(world: &World, item: Entity) -> bool {
    path(world, item).is_some_and(|p| is_under(p, SEC_HELMET))
}

/// tg `helmet/sec/attack_self`: the straps start loosening or tightening (3 s).
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !sec_helmet(world, item) {
        return false;
    }
    let hides = world
        .get::<DmClothing>(item)
        .is_some_and(|c| c.flags_inv & HIDEHAIR != 0);
    let actor = Actor {
        client,
        mob,
        item: Some(item),
    };
    let job = crate::construction::Job::Clothing(super::damage::ClothingJob::HelmetStraps { item });
    if crate::tools::do_after(world, actor, mob, STRAPS_DS, 0, job) {
        let text = if hides {
            "ослабление ремней..."
        } else {
            "затягивание ремней..."
        };
        tell(world, client, Span::Notice, text);
    }
    true
}

/// The end of the straps' `do_after`: the helmet is still in the user's hand.
pub fn complete_straps(world: &mut World, mob: Entity, client: Entity, item: Entity) {
    let held = world.get::<Inventory>(mob).is_some_and(|i| i.holds(item));
    if !held || world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return;
    }
    let Some(mut c) = world.get_mut::<DmClothing>(item) else {
        return;
    };
    c.flags_inv ^= HIDEHAIR;
    let hides = c.flags_inv & HIDEHAIR != 0;
    let text = if hides {
        "ремни затянуты"
    } else {
        "ремни ослаблены"
    };
    tell(world, client, Span::Notice, text);
}

/// tg `helmet/sec/click_alt`: the visor flips (`-novisor`, the eyes uncovered) and back.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !sec_helmet(world, item) || world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return false;
    }
    let base = world
        .get::<ss13_mapload::DmAtom>(item)
        .map(|a| a.icon_state.clone())
        .unwrap_or_default();
    let flipped = world.get::<VisorFlipped>(item).cloned();
    let (state, worn, sfx) = match flipped {
        Some(VisorFlipped { worn_icon_state }) => {
            world.entity_mut(item).remove::<VisorFlipped>();
            if let Some(mut c) = world.get_mut::<DmClothing>(item) {
                c.flags_cover |= HEADCOVERSEYES;
            }
            (base, worn_icon_state, "visor_up")
        }
        None => {
            let worn_icon_state = world
                .get::<DmItem>(item)
                .map(|i| i.worn_icon_state.clone())
                .unwrap_or_default();
            let state = format!("{base}-novisor");
            let worn = if worn_icon_state.is_empty() {
                state.clone()
            } else {
                worn_icon_state.clone()
            };
            world
                .entity_mut(item)
                .insert(VisorFlipped { worn_icon_state });
            if let Some(mut c) = world.get_mut::<DmClothing>(item) {
                c.flags_cover &= !HEADCOVERSEYES;
            }
            (state, worn, "visor_down")
        }
    };
    super::toggles::set_states(world, item, &state, worn);
    tell(world, client, Span::Notice, "визор откинут");
    if let Some(pos) = crate::explosions::turf_of(world, item) {
        let mut s = PlaySound::new(pos, sfx, 20.0, true);
        s.extrarange = -1.0;
        playsound(world, s);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vests_hold_the_security_list_and_det_suits_the_detective_one() {
        let vest = "/obj/item/clothing/suit/armor/vest";
        assert!(suit_allows(vest, &[], "/obj/item/melee/baton/security"));
        assert!(!suit_allows(vest, &[], "/obj/item/detective_scanner"));
        // A type list of its own wins over `if(!allowed)`.
        let own = vec!["/obj/item/tank".to_owned()];
        assert!(!suit_allows(vest, &own, "/obj/item/melee/baton"));
        let det = "/obj/item/clothing/suit/armor/vest/det_suit";
        assert!(suit_allows(det, &[], "/obj/item/detective_scanner"));
        // `+=` keeps the type's own list.
        let coat = "/obj/item/clothing/suit/hooded/wintercoat/security";
        assert!(suit_allows(coat, &own, "/obj/item/tank/internals"));
        assert!(suit_allows(coat, &own, "/obj/item/gun/energy/disabler"));
        assert!(!suit_allows(
            "/obj/item/clothing/suit/apron",
            &[],
            "/obj/item/gun/energy"
        ));
    }

    #[test]
    fn hailer_phrases_follow_the_restrictor_and_overuse_breaks_it() {
        let mut h = Hailer::of_type(HAILER);
        assert_eq!(h.aggressiveness, AGGR_BAD_COP);
        assert_eq!(
            h.select_phrase(|lo, hi| hi * 100 + lo),
            BAD_COP_PHRASES * 100 + 2
        );
        h.aggressiveness = AGGR_BROKEN;
        assert_eq!(
            h.select_phrase(|lo, hi| hi * 100 + lo),
            ALL_PHRASES * 100 + 13
        );
        h.safety = false;
        assert_eq!(h.select_phrase(|_, _| 7), 1);
        assert_eq!(Hailer::of_type(SWAT_HAILER).aggressiveness, AGGR_SHIT_COP);
        // A use every 3 s: the first resets the count, the sixth overloads it.
        let mut h = Hailer::of_type(HAILER);
        let told: Vec<_> = (0..6).map(|i| h.count_use(f64::from(i) * 3.0)).collect();
        assert!(told[..3].iter().all(Option::is_none));
        assert!(told[3..].iter().all(Option::is_some));
        assert!(h.broken);
        // A new window after 18 s starts over.
        let mut h = Hailer::of_type(HAILER);
        h.count_use(0.0);
        h.count_use(3.0);
        h.count_use(20.0);
        assert_eq!(h.recent_uses, 0);
    }
}
