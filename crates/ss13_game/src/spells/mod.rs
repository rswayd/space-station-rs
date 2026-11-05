//! Spells (server) after tg `/datum/action/cooldown/spell` (`code/modules/spells/spell.dm`) and
//! the spell types of `code/modules/spells/spell_types/` the wizard's book sells.
//!
//! - A mob's spells are [`Spells`]: each known spell with its level and the time it is ready
//!   again. Every spell is a button of the mob's [`crate::actions`] row (id `spell:<type>`,
//!   tg `button_icon_state` on `bg_spell`); its name carries the level's title ("Quickened
//!   Fireball", `get_spell_title`). Learning a known spell again levels it up
//!   (`level_spell`): `cooldown_reduction_per_rank` off the cooldown, at least 0.25 s.
//! - The button (tg `Trigger` -> `IsAvailable` -> `can_cast_spell`): conscious, not phased, the
//!   station z-level for `SPELL_REQUIRES_STATION`, no antimagic on the caster for
//!   `SPELL_REQUIRES_NO_ANTIMAGIC` ([`can_cast_magic`]), and a human needs a casting robe and a
//!   casting hat (or eyewear) for `SPELL_REQUIRES_WIZARD_GARB` (every `/obj/item/clothing/suit/
//!   wizrobe` and `/obj/item/clothing/head/wizard`, the fake ones included, as tg's
//!   `TRAIT_CASTING_CLOTHING`); other mobs need any casting item worn.
//! - Targeting: a self spell casts at once; a pointed spell (`/pointed`) arms on the button and
//!   casts on the next click within `cast_range` (a click on a floor picks a mob standing there,
//!   tg `aim_assist`), the button again disarms it; a touch spell (`/touch`) puts its hand in an
//!   empty hand and casts when the hand hits a living mob in reach, the button again takes the
//!   hand back.
//! - Casting (`Activate`): `before_cast` (a shouted or whispered spell fails without a tongue,
//!   2 s cooldown), the invocation (`INVOCATION_SHOUT` says it, `INVOCATION_WHISPER` whispers it,
//!   half the time garbled with `` ` `` for spaces) and the spell's sound, the effect, the
//!   cooldown, then `after_cast`'s sparks and smoke.
//! - Antimagic ([`can_block_magic`], [`can_cast_magic`]): a null rod or the syndicate bible
//!   (`MAGIC_RESISTANCE | MAGIC_RESISTANCE_HOLY`), a bible (`HOLY`) held or worn (not in a bag)
//!   blocks spells cast on its bearer and stops it casting; holy water in the blood is tg's
//!   `TRAIT_HOLY`. Spells and magic bolts that tg checks call it.
//!
//! The effects are in [`effects`], staves and wands in [`magic_guns`].
// shortcut: antimagic with charges (the holymelon helmet, the psyker's) and from implants or
// organs is not ported; no spell levels past the spellbook's limits from admins, no
// `SPELL_REQUIRES_MIND` or mime vows, no emote invocations, no muteness (tg `can_speak`), no
// `COMSIG_MOB_PRE_INVOCATION` (chuuni) and no splattercasting; the buttons share the action row with other antagonists'
// own buttons (`crate::actions::set_buttons` replaces the row: a heretic wizard loses the spell
// buttons until the next spell refresh).

pub mod effects;
pub mod magic_guns;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::server::{say_message, whisper_message};
use ss13_mapload::IconIndex;
use ss13_mapload::fidelity::is_under;
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{UiActed, UiSystems};
use ss13_world::SpatialIndex;

use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::click::{ClickIntercept, InterceptedClick};
use crate::health::{Health, Stat};
use crate::human::HumanLook;
use crate::inventory::Inventory;
use crate::server::PlayerMob;
use crate::tools::tell;

pub use effects::{BoltKind, JauntStep, Jaunting, MagicBolt, SummonMark, TouchHand};
pub use magic_guns::MagicGun;

/// `MAGIC_RESISTANCE`, `MAGIC_RESISTANCE_MIND`, `MAGIC_RESISTANCE_HOLY`.
pub const MAGIC_RESISTANCE: u32 = 1 << 0;
pub const MAGIC_RESISTANCE_MIND: u32 = 1 << 1;
pub const MAGIC_RESISTANCE_HOLY: u32 = 1 << 2;

/// `SPELL_REQUIRES_*` (`code/__DEFINES/magic.dm`).
pub const SPELL_REQUIRES_WIZARD_GARB: u32 = 1 << 0;
pub const SPELL_REQUIRES_HUMAN: u32 = 1 << 1;
pub const SPELL_REQUIRES_NO_ANTIMAGIC: u32 = 1 << 4;
pub const SPELL_REQUIRES_STATION: u32 = 1 << 5;
/// The default `spell_requirements`.
const GARB_AND_NO_ANTIMAGIC: u32 = SPELL_REQUIRES_WIZARD_GARB | SPELL_REQUIRES_NO_ANTIMAGIC;

pub const SPELLS_DMI: &str = "icons/mob/actions/actions_spells.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
const BUTTON_PREFIX: &str = "spell:";
/// `garbled_invocation_prob`.
const GARBLED_INVOCATION_PROB: f32 = 50.0;
/// `level_spell`: "0 second CD starts to break things".
const MIN_COOLDOWN: f64 = 0.25;
/// `before_cast`: a tongueless caster's `StartCooldown(2 SECONDS)`.
const TONGUELESS_COOLDOWN: f64 = 2.0;

/// The casting robes and hats (`TRAIT_CASTING_CLOTHING`).
const CASTING_SUITS: [&str; 1] = ["/obj/item/clothing/suit/wizrobe"];
const CASTING_HATS: [&str; 1] = ["/obj/item/clothing/head/wizard"];
const CASTING_EYES: [&str; 1] = ["/obj/item/clothing/glasses/eyepatch/medical/chuuni"];

/// `invocation_type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invocation {
    None,
    Shout,
    Whisper,
}

/// How the spell picks what it is cast on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Targeting {
    /// Cast on the caster when the button is pressed.
    Caster,
    /// `/datum/action/cooldown/spell/pointed`: `cast_range` tiles.
    Pointed { range: u32 },
    /// `/datum/action/cooldown/spell/touch`: the `hand_path` item.
    Touch { hand: &'static str },
}

/// The ported spells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Spell {
    MagicMissile,
    Fireball,
    LightningBolt,
    Smite,
    Blind,
    DisableTech,
    Repulse,
    Smoke,
    Forcewall,
    Knock,
    Blink,
    Teleport,
    EtherealJaunt,
    Swap,
    SummonItem,
    Charge,
}

/// A spell type's vars.
#[derive(Clone, Copy, Debug)]
pub struct SpellDef {
    pub spell: Spell,
    pub path: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    /// `button_icon_state` in [`SPELLS_DMI`].
    pub icon: &'static str,
    /// Played at the caster on cast; empty: none.
    pub sound: &'static str,
    /// `cooldown_time`, seconds.
    pub cooldown: f64,
    pub cooldown_reduction_per_rank: f64,
    pub max_level: u8,
    pub invocation: &'static str,
    pub invocation_type: Invocation,
    pub requirements: u32,
    pub antimagic: u32,
    pub targeting: Targeting,
    /// `active_msg`, `deactive_msg` of pointed spells (empty: tg's defaults).
    pub active_msg: &'static str,
    pub deactive_msg: &'static str,
    /// `active_overlay_icon_state`.
    pub active_border: &'static str,
}

const fn def(spell: Spell, path: &'static str, name: &'static str) -> SpellDef {
    SpellDef {
        spell,
        path,
        name,
        desc: "",
        icon: "spell_default",
        sound: "",
        cooldown: 0.0,
        cooldown_reduction_per_rank: 0.0,
        max_level: 5,
        invocation: "",
        invocation_type: Invocation::None,
        requirements: GARB_AND_NO_ANTIMAGIC,
        antimagic: MAGIC_RESISTANCE,
        targeting: Targeting::Caster,
        active_msg: "",
        deactive_msg: "",
        active_border: "bg_spell_border_active_red",
    }
}

/// Every ported spell, in [`Spell`] order.
pub const SPELLS: [SpellDef; 16] = [
    SpellDef {
        desc: "This spell fires several, slow moving, magic projectiles at nearby targets.",
        icon: "magicm",
        sound: "sound/effects/magic/magic_missile.ogg",
        cooldown: 20.0,
        cooldown_reduction_per_rank: 3.5,
        invocation: "FORTI GY AMA!",
        invocation_type: Invocation::Shout,
        ..def(
            Spell::MagicMissile,
            "/datum/action/cooldown/spell/aoe/magic_missile",
            "Magic Missile",
        )
    },
    SpellDef {
        desc: "This spell fires an explosive fireball at a target.",
        icon: "fireball0",
        sound: "sound/effects/magic/fireball.ogg",
        cooldown: 6.0,
        cooldown_reduction_per_rank: 1.0,
        invocation: "ONI SOMA!",
        invocation_type: Invocation::Shout,
        requirements: SPELL_REQUIRES_NO_ANTIMAGIC,
        targeting: Targeting::Pointed { range: 8 },
        active_msg: "You prepare to cast your fireball spell!",
        deactive_msg: "You extinguish your fireball... for now.",
        ..def(
            Spell::Fireball,
            "/datum/action/cooldown/spell/pointed/projectile/fireball",
            "Fireball",
        )
    },
    SpellDef {
        desc: "Fire a lightning bolt at your foes! It will jump between targets, but can't knock \
               them down.",
        icon: "lightning",
        sound: "sound/effects/magic/lightningbolt.ogg",
        cooldown: 10.0,
        cooldown_reduction_per_rank: 2.0,
        invocation: "P'WAH, UNLIM'TED P'WAH!",
        invocation_type: Invocation::Shout,
        requirements: SPELL_REQUIRES_NO_ANTIMAGIC,
        targeting: Targeting::Pointed { range: 7 },
        active_msg: "You energize your hands with arcane lightning!",
        deactive_msg: "You let the energy flow out of your hands back into yourself...",
        active_border: "bg_spell_border_active_yellow",
        ..def(
            Spell::LightningBolt,
            "/datum/action/cooldown/spell/pointed/projectile/lightningbolt",
            "Lightning Bolt",
        )
    },
    SpellDef {
        desc: "This spell charges your hand with an unholy energy that can be used to cause a \
               touched victim to violently explode.",
        icon: "gib",
        sound: "sound/effects/magic/disintegrate.ogg",
        cooldown: 60.0,
        cooldown_reduction_per_rank: 10.0,
        invocation: "EI NATH!!",
        invocation_type: Invocation::Shout,
        targeting: Targeting::Touch {
            hand: "/obj/item/melee/touch_attack/smite",
        },
        ..def(
            Spell::Smite,
            "/datum/action/cooldown/spell/touch/smite",
            "Smite",
        )
    },
    SpellDef {
        desc: "This spell temporarily blinds a single target.",
        icon: "blind",
        sound: "sound/effects/magic/blind.ogg",
        cooldown: 30.0,
        cooldown_reduction_per_rank: 6.25,
        invocation: "STI KALY.",
        invocation_type: Invocation::Whisper,
        requirements: SPELL_REQUIRES_NO_ANTIMAGIC,
        targeting: Targeting::Pointed { range: 7 },
        active_msg: "You prepare to blind a target...",
        ..def(
            Spell::Blind,
            "/datum/action/cooldown/spell/pointed/blind",
            "Blind",
        )
    },
    SpellDef {
        desc: "This spell disables all weapons, cameras and most other technology in range.",
        icon: "emp",
        sound: "sound/effects/magic/disable_tech.ogg",
        cooldown: 40.0,
        cooldown_reduction_per_rank: 5.0,
        invocation: "NEC CANTIO!",
        invocation_type: Invocation::Shout,
        ..def(
            Spell::DisableTech,
            "/datum/action/cooldown/spell/emp/disable_tech",
            "Disable Tech",
        )
    },
    SpellDef {
        desc: "This spell throws everything around the user away.",
        icon: "repulse",
        sound: "sound/effects/magic/repulse.ogg",
        cooldown: 40.0,
        cooldown_reduction_per_rank: 6.25,
        invocation: "GITTAH WEIGH!",
        invocation_type: Invocation::Shout,
        ..def(
            Spell::Repulse,
            "/datum/action/cooldown/spell/aoe/repulse/wizard",
            "Repulse",
        )
    },
    SpellDef {
        desc: "This spell spawns a cloud of smoke at your location. People within will begin to \
               choke and drop their items.",
        icon: "smoke",
        cooldown: 12.0,
        cooldown_reduction_per_rank: 2.5,
        requirements: 0,
        ..def(Spell::Smoke, "/datum/action/cooldown/spell/smoke", "Smoke")
    },
    SpellDef {
        desc: "Create a magical barrier that only you can pass through.",
        icon: "shield",
        sound: "sound/effects/magic/forcewall.ogg",
        cooldown: 10.0,
        cooldown_reduction_per_rank: 1.25,
        invocation: "TARCOL MINTI ZHERI!",
        invocation_type: Invocation::Shout,
        requirements: SPELL_REQUIRES_NO_ANTIMAGIC,
        ..def(
            Spell::Forcewall,
            "/datum/action/cooldown/spell/forcewall",
            "Forcewall",
        )
    },
    SpellDef {
        desc: "This spell opens nearby doors and closets.",
        icon: "knock",
        sound: "sound/effects/magic/knock.ogg",
        cooldown: 10.0,
        cooldown_reduction_per_rank: 2.0,
        invocation: "AULIE OXIN FIERA!",
        invocation_type: Invocation::Whisper,
        requirements: SPELL_REQUIRES_NO_ANTIMAGIC,
        ..def(
            Spell::Knock,
            "/datum/action/cooldown/spell/aoe/knock",
            "Knock",
        )
    },
    SpellDef {
        desc: "This spell randomly teleports you a short distance.",
        icon: "blink",
        sound: "sound/effects/magic/blink.ogg",
        cooldown: 2.0,
        cooldown_reduction_per_rank: 0.4,
        ..def(
            Spell::Blink,
            "/datum/action/cooldown/spell/teleport/radius_turf/blink",
            "Blink",
        )
    },
    SpellDef {
        desc: "This spell teleports you to an area of your selection.",
        icon: "teleport",
        sound: "sound/effects/magic/teleport_diss.ogg",
        cooldown: 60.0,
        cooldown_reduction_per_rank: 20.0,
        max_level: 3,
        invocation: "SCYAR NILA",
        invocation_type: Invocation::Shout,
        ..def(
            Spell::Teleport,
            "/datum/action/cooldown/spell/teleport/area_teleport/wizard",
            "Teleport",
        )
    },
    SpellDef {
        desc: "This spell turns your form ethereal, temporarily making you invisible and able to \
               pass through walls.",
        icon: "jaunt",
        sound: "sound/effects/magic/ethereal_enter.ogg",
        cooldown: 30.0,
        cooldown_reduction_per_rank: 5.0,
        ..def(
            Spell::EtherealJaunt,
            "/datum/action/cooldown/spell/jaunt/ethereal_jaunt",
            "Ethereal Jaunt",
        )
    },
    SpellDef {
        desc: "This spell allows you to swap locations with any living being. RMB: Mark a \
               secondary swap target. This secondary swap target will be discarded once you \
               swap, or else you can click yourself with the RMB to discard your secondary \
               target.",
        icon: "swap",
        cooldown: 25.0,
        cooldown_reduction_per_rank: 10.0,
        max_level: 3,
        requirements: SPELL_REQUIRES_NO_ANTIMAGIC | SPELL_REQUIRES_STATION,
        targeting: Targeting::Pointed { range: 9 },
        active_msg: "You prepare to swap locations with a target...",
        active_border: "bg_spell_border_active_blue",
        ..def(
            Spell::Swap,
            "/datum/action/cooldown/spell/pointed/swap",
            "Swap",
        )
    },
    SpellDef {
        desc: "This spell can be used to recall a previously marked item to your hand from \
               anywhere in the universe.",
        icon: "summons",
        cooldown: 10.0,
        max_level: 1,
        invocation: "Gar Yok!",
        invocation_type: Invocation::Whisper,
        requirements: SPELL_REQUIRES_NO_ANTIMAGIC,
        ..def(
            Spell::SummonItem,
            "/datum/action/cooldown/spell/summonitem",
            "Instant Summons",
        )
    },
    SpellDef {
        desc: "This spell can be used to recharge a variety of things in your hands, from \
               magical artifacts to electrical components. A creative wizard can even use it to \
               grant magical power to a fellow magic user.",
        icon: "charge",
        sound: "sound/effects/magic/charge.ogg",
        cooldown: 60.0,
        cooldown_reduction_per_rank: 5.0,
        invocation: "Diri Cel!",
        invocation_type: Invocation::Whisper,
        requirements: SPELL_REQUIRES_NO_ANTIMAGIC,
        ..def(
            Spell::Charge,
            "/datum/action/cooldown/spell/charge",
            "Charge",
        )
    },
];

impl Spell {
    pub fn def(self) -> &'static SpellDef {
        &SPELLS[self as usize]
    }

    pub fn from_path(path: &str) -> Option<Self> {
        SPELLS.iter().find(|d| d.path == path).map(|d| d.spell)
    }

    fn button_id(self) -> String {
        format!("{BUTTON_PREFIX}{}", self.def().path)
    }

    fn from_button(id: &str) -> Option<Self> {
        Self::from_path(id.strip_prefix(BUTTON_PREFIX)?)
    }
}

/// `get_spell_title`.
pub fn spell_title(level: u8) -> &'static str {
    match level {
        2 => "Efficient ",
        3 => "Quickened ",
        4 => "Free ",
        5 => "Instant ",
        6 => "Ludicrous ",
        _ => "",
    }
}

/// `cooldown_time` at `level` (`level_spell` takes `cooldown_reduction_per_rank` per level).
pub fn cooldown_at(def: &SpellDef, level: u8) -> f64 {
    let ranks = f64::from(level.saturating_sub(1));
    if def.cooldown_reduction_per_rank == 0.0 {
        return def.cooldown;
    }
    (def.cooldown - def.cooldown_reduction_per_rank * ranks).max(MIN_COOLDOWN)
}

/// One spell a mob knows (the action datum).
#[derive(Clone, Debug, PartialEq)]
pub struct KnownSpell {
    pub spell: Spell,
    /// `spell_level`.
    pub level: u8,
    /// `next_use_time`, `Time::elapsed_secs_f64`.
    pub ready_at: f64,
    /// Summon item's `marked_item` (checked against its [`SummonMark`]).
    pub marked: Option<Entity>,
    /// A touch spell's `attached_hand`.
    pub hand: Option<Entity>,
    /// `spell_requirements &= ~SPELL_REQUIRES_WIZARD_GARB` (Robeless Casting,
    /// `crate::events::wizard`).
    pub robeless: bool,
}

impl KnownSpell {
    pub fn cooldown(&self) -> f64 {
        cooldown_at(self.spell.def(), self.level)
    }

    /// `name` after `level_spell`.
    pub fn name(&self) -> String {
        format!("{}{}", spell_title(self.level), self.spell.def().name)
    }
}

/// The spells of a mob (`owner.actions` of spell type).
#[derive(Component, Clone, Debug, Default)]
pub struct Spells {
    pub known: Vec<KnownSpell>,
    /// The pointed spell waiting for its click (tg `click_intercept`).
    pub aiming: Option<Spell>,
    /// Swap's `second_target`.
    pub second_target: Option<Entity>,
}

impl Spells {
    pub fn get(&self, spell: Spell) -> Option<&KnownSpell> {
        self.known.iter().find(|k| k.spell == spell)
    }

    pub fn get_mut(&mut self, spell: Spell) -> Option<&mut KnownSpell> {
        self.known.iter_mut().find(|k| k.spell == spell)
    }
}

/// What [`learn`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Learned {
    New,
    /// `level_spell`: the new level.
    Leveled(u8),
    /// Already at `spell_max_level`.
    MaxLevel,
}

/// A teleport prompt waiting for its answer (`tgui_input_list` in `before_cast`).
#[derive(Component, Clone, Debug)]
pub struct TeleportPrompt {
    /// The area teleport spell, or a scroll of teleportation (no invocation, its uses).
    pub scroll: Option<Entity>,
    pub areas: Vec<Entity>,
}

pub struct SpellsPlugin;

impl Plugin for SpellsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<JauntStep>()
            .add_systems(
                Update,
                (
                    trigger_buttons,
                    pointed_clicks,
                    teleport_answers.after(PromptSystems::Answer),
                )
                    .in_set(UiSystems::Handle),
            )
            .add_systems(
                Update,
                effects::touch_clicks
                    .after(crate::security::security_clicks)
                    .before(crate::combat::MeleeSystems),
            )
            .add_systems(
                Update,
                (effects::jaunt_steps, effects::tick_spells)
                    .chain()
                    .after(ss13_world::WorldSystems::Movement)
                    .before(crate::health::HealthSystems),
            )
            .add_systems(Update, (effects::jaunt_look, refresh_on_change))
            .add_plugins(magic_guns::MagicGunsPlugin);
    }
}

// ---------------------------------------------------------------------------------------------
// Learning

/// `new spell_type(...).Grant(mob)`, or `level_spell` of a known one.
pub fn learn(world: &mut World, mob: Entity, spell: Spell) -> Learned {
    if world.get::<Spells>(mob).is_none() {
        world.entity_mut(mob).insert(Spells::default());
    }
    let mut spells = world.get_mut::<Spells>(mob).expect("inserted above");
    let result = match spells.get_mut(spell) {
        Some(known) if known.level >= spell.def().max_level => Learned::MaxLevel,
        Some(known) => {
            known.level += 1;
            Learned::Leveled(known.level)
        }
        None => {
            spells.known.push(KnownSpell {
                spell,
                level: 1,
                ready_at: 0.0,
                marked: None,
                hand: None,
                robeless: false,
            });
            Learned::New
        }
    };
    refresh(world, mob);
    result
}

/// `qdel(spell)`: forgets `spell`; its level (0 when unknown).
pub fn forget(world: &mut World, mob: Entity, spell: Spell) -> u8 {
    let Some(known) = world.get::<Spells>(mob).and_then(|s| s.get(spell)).cloned() else {
        return 0;
    };
    if let Some(hand) = known.hand {
        effects::remove_hand(world, mob, hand);
    }
    if let Some(item) = known.marked
        && world
            .get::<SummonMark>(item)
            .is_some_and(|m| m.owner == mob)
    {
        world.entity_mut(item).remove::<SummonMark>();
    }
    if let Some(mut spells) = world.get_mut::<Spells>(mob) {
        spells.known.retain(|k| k.spell != spell);
        if spells.aiming == Some(spell) {
            spells.aiming = None;
        }
    }
    if world.get::<Spells>(mob).is_some_and(|s| s.aiming.is_none()) {
        world.entity_mut(mob).remove::<ClickIntercept>();
    }
    refresh(world, mob);
    known.level
}

pub fn knows(world: &World, mob: Entity, spell: Spell) -> bool {
    world
        .get::<Spells>(mob)
        .is_some_and(|s| s.get(spell).is_some())
}

/// The mob's spell buttons, after the buttons other things gave it.
pub fn refresh(world: &mut World, mob: Entity) {
    let Some(spells) = world.get::<Spells>(mob).cloned() else {
        return;
    };
    let icon = |world: &World, dmi: &str, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(dmi, state))
    };
    let background = icon(world, BACKGROUNDS_DMI, "bg_spell");
    let border = icon(world, BACKGROUNDS_DMI, "bg_spell_border");
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(button, _)| !button.id.starts_with(BUTTON_PREFIX))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    for known in &spells.known {
        let def = known.spell.def();
        let active = spells.aiming == Some(known.spell) || known.hand.is_some();
        let border = if active {
            icon(world, BACKGROUNDS_DMI, def.active_border)
        } else {
            border
        };
        let available = can_cast(world, mob, known.spell).is_ok();
        buttons.push((
            ActionButton {
                id: known.spell.button_id(),
                name: known.name(),
                icon: icon(world, SPELLS_DMI, def.icon),
                background,
                border,
                available,
                cooldown: 0.0,
                active,
                maptext: None,
            },
            known.ready_at,
        ));
    }
    set_buttons(world, mob, buttons);
}

/// The buttons follow what the caster wears and its state (`update_status_on_signal`).
// Cost: per tick, the casters whose inventory or stat changed.
fn refresh_on_change(
    mobs: Query<Entity, (With<Spells>, Or<(Changed<Inventory>, Changed<Stat>)>)>,
    mut commands: Commands,
) {
    for mob in &mobs {
        commands.queue(move |world: &mut World| refresh(world, mob));
    }
}

// ---------------------------------------------------------------------------------------------
// Checks

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

pub(crate) fn chat_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

/// An item `mob` holds or wears (not what is inside its bags).
fn carried(world: &World, mob: Entity) -> Vec<Entity> {
    world
        .get::<Inventory>(mob)
        .map(|i| {
            i.hands
                .iter()
                .flatten()
                .copied()
                .chain(i.equipped())
                .collect()
        })
        .unwrap_or_default()
}

fn path_of(world: &World, e: Entity) -> Option<String> {
    crate::construction::path_of(world, e)
}

/// The antimagic flags of an item (`/datum/component/anti_magic` of its type).
pub fn item_antimagic(path: &str) -> u32 {
    if is_under(path, "/obj/item/nullrod") || is_under(path, "/obj/item/book/bible/syndicate") {
        // `/datum/element/nullrod_core`.
        MAGIC_RESISTANCE | MAGIC_RESISTANCE_HOLY
    } else if is_under(path, "/obj/item/book/bible") {
        MAGIC_RESISTANCE_HOLY
    } else {
        0
    }
}

/// The antimagic `mob` carries: its items' and tg `TRAIT_HOLY` (holy water in the blood).
fn antimagic_of(world: &World, mob: Entity) -> u32 {
    let mut flags = carried(world, mob)
        .into_iter()
        .filter_map(|item| path_of(world, item))
        .fold(0, |acc, path| acc | item_antimagic(&path));
    if crate::chem::blood_amount(world, mob, "/datum/reagent/water/holywater") > 0.0 {
        flags |= MAGIC_RESISTANCE_HOLY;
    }
    flags
}

/// `/mob/proc/can_block_magic(casted_magic_flags)`: true when `mob` resists magic of `flags`.
pub fn can_block_magic(world: &World, mob: Entity, flags: u32) -> bool {
    flags != 0 && antimagic_of(world, mob) & flags != 0
}

/// `/mob/proc/can_cast_magic(magic_flags)`: antimagic on the caster stops its casting
/// (`COMSIG_MOB_RESTRICT_MAGIC`).
pub fn can_cast_magic(world: &World, mob: Entity, flags: u32) -> bool {
    flags == 0 || antimagic_of(world, mob) & flags == 0
}

/// tg `on_block_magic_effects` for a blocked spell: the message (no shield overlay).
pub fn block_effects(world: &mut World, mob: Entity) {
    let name = crate::medical::name_of(world, mob);
    crate::medical::visible(
        world,
        mob,
        "Сильная магическая аура пульсирует вокруг вас, когда она рассеивается в воздухе!",
        &format!("{name} пульсирует красным, поглощая магическую энергию!"),
        Span::Warning,
    );
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/effects/magic/magic_block.ogg", 50.0, true),
        );
    }
}

/// `TRAIT_CASTING_CLOTHING` of a worn item.
pub fn is_casting_clothing(path: &str) -> bool {
    CASTING_SUITS
        .iter()
        .chain(&CASTING_HATS)
        .chain(&CASTING_EYES)
        .any(|root| is_under(path, root))
}

/// The robe and the hat checks of `SPELL_REQUIRES_WIZARD_GARB` for a human.
pub fn wizard_garb(world: &World, mob: Entity) -> (bool, bool) {
    let Some(inv) = world.get::<Inventory>(mob) else {
        return (false, false);
    };
    let worn = |slot: Option<Entity>, roots: &[&str]| {
        slot.and_then(|e| path_of(world, e))
            .is_some_and(|p| roots.iter().any(|r| is_under(&p, r)))
    };
    let robe = worn(inv.suit, &CASTING_SUITS);
    let hat = worn(inv.head, &CASTING_HATS) || worn(inv.eyes, &CASTING_EYES);
    (robe, hat)
}

/// The wizard's den (`/area/centcom/wizard_station`).
pub fn in_wizard_den(world: &World, pos: TilePos) -> bool {
    area_path(world, pos).is_some_and(|p| is_under(&p, "/area/centcom/wizard_station"))
}

/// `NOTELEPORT` areas: CentCom's (the den is one).
pub fn no_teleport(world: &World, pos: TilePos) -> bool {
    area_path(world, pos).is_some_and(|p| is_under(&p, "/area/centcom"))
}

fn area_path(world: &World, pos: TilePos) -> Option<String> {
    let area = world.get_resource::<ss13_mapload::Areas>()?.area_at(pos)?;
    world
        .get::<ss13_mapload::DmPower>(area)
        .map(|p| p.path.clone())
}

/// `IsAvailable` + `can_cast_spell`: `Err(Some(text))` with tg's feedback, `Err(None)` silent.
pub fn can_cast(world: &World, mob: Entity, spell: Spell) -> Result<(), Option<String>> {
    let def = spell.def();
    let garb = def.requirements & SPELL_REQUIRES_WIZARD_GARB != 0
        && !world
            .get::<Spells>(mob)
            .and_then(|s| s.get(spell))
            .is_some_and(|k| k.robeless);
    // `AB_CHECK_CONSCIOUS`, `AB_CHECK_PHASED`.
    if world.get::<Stat>(mob) != Some(&Stat::Conscious) || world.get::<Jaunting>(mob).is_some() {
        return Err(None);
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return Err(None);
    };
    // Touch spells: `AB_CHECK_HANDS_BLOCKED`, `MOBILITY_USE`.
    if matches!(def.targeting, Targeting::Touch { .. })
        && crate::status_effects::incapacitation(world, mob).hands_blocked
    {
        return Err(None);
    }
    if def.requirements & SPELL_REQUIRES_STATION != 0
        && !crate::events::controller::station_levels(world).contains(&pos.z)
    {
        return Err(Some(format!("You can't cast {} here!", def.name)));
    }
    if def.requirements & SPELL_REQUIRES_NO_ANTIMAGIC != 0
        && !can_cast_magic(world, mob, def.antimagic)
    {
        return Err(Some(format!(
            "Some form of antimagic is preventing you from casting {}!",
            def.name
        )));
    }
    if world.get::<HumanLook>(mob).is_some() {
        if garb {
            let (robe, hat) = wizard_garb(world, mob);
            if !robe {
                return Err(Some(
                    "You don't feel strong enough without your robe!".into(),
                ));
            }
            if !hat {
                return Err(Some(
                    "You don't feel strong enough without your hat!".into(),
                ));
            }
        }
    } else {
        if def.requirements & SPELL_REQUIRES_HUMAN != 0 {
            return Err(Some(format!("{} can only be cast by humans!", def.name)));
        }
        if garb
            && !carried(world, mob)
                .into_iter()
                .filter_map(|e| path_of(world, e))
                .any(|p| is_casting_clothing(&p))
        {
            return Err(Some(
                "You don't feel strong enough without your hat!".into(),
            ));
        }
    }
    // `/datum/action/cooldown/spell/jaunt/can_cast_spell`.
    if spell == Spell::EtherealJaunt && no_teleport(world, pos) {
        return Err(Some(
            "Some dull, universal force is stopping you from jaunting here.".into(),
        ));
    }
    Ok(())
}

/// [`can_cast`] with its feedback to the caster.
fn checked(world: &mut World, mob: Entity, spell: Spell) -> bool {
    match can_cast(world, mob, spell) {
        Ok(()) => true,
        Err(Some(text)) => {
            chat_to(world, mob, Span::Warning, text);
            false
        }
        Err(None) => false,
    }
}

fn ready(world: &World, mob: Entity, spell: Spell) -> bool {
    world
        .get::<Spells>(mob)
        .and_then(|s| s.get(spell))
        .is_some_and(|k| now(world) >= k.ready_at)
}

/// `StartCooldown(secs)`; `None`: the spell's own cooldown.
pub fn start_cooldown(world: &mut World, mob: Entity, spell: Spell, secs: Option<f64>) {
    let t = now(world);
    if let Some(mut spells) = world.get_mut::<Spells>(mob)
        && let Some(known) = spells.get_mut(spell)
    {
        known.ready_at = t + secs.unwrap_or_else(|| known.cooldown());
    }
    refresh(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Invocation

/// `invocation()`: the words, said or whispered; half the time garbled.
pub fn invoke(world: &mut World, mob: Entity, kind: Invocation, words: &str) {
    if words.is_empty() || kind == Invocation::None {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let garbled = world
        .resource_mut::<crate::combat::CombatRng>()
        .chance(GARBLED_INVOCATION_PROB);
    let text = if garbled {
        words.replace(' ', "`")
    } else {
        words.to_owned()
    };
    let name = crate::medical::name_of(world, mob);
    let (clients, message) = match kind {
        Invocation::Shout => (
            crate::chem::clients_near(world, pos, None),
            say_message(&name, &text),
        ),
        Invocation::Whisper => (
            crate::obj_damage::clients_in_range(world, pos, 1, &[]),
            whisper_message(&name, &text),
        ),
        Invocation::None => return,
    };
    world.write_message(SendChat { clients, message });
}

/// `spell_feedback`: the invocation and the sound.
fn feedback(world: &mut World, mob: Entity, spell: Spell, words: Option<&str>) {
    let def = spell.def();
    invoke(
        world,
        mob,
        def.invocation_type,
        words.unwrap_or(def.invocation),
    );
    if !def.sound.is_empty()
        && let Some(&pos) = world.get::<TilePos>(mob)
    {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, def.sound, 50.0, true),
        );
    }
}

/// `before_cast` of a shouted or whispered spell: a human without a tongue fails it.
fn tongue_check(world: &mut World, mob: Entity, spell: Spell) -> bool {
    let def = spell.def();
    if !matches!(def.invocation_type, Invocation::Shout | Invocation::Whisper)
        || world.get::<HumanLook>(mob).is_none()
        || world.get::<ss13_chat::speech::Tongueless>(mob).is_none()
    {
        return true;
    }
    invoke(world, mob, def.invocation_type, def.invocation);
    chat_to(
        world,
        mob,
        Span::Warning,
        format!(
            "Your lack of tongue is making it difficult to say the correct words to cast {}...",
            def.name
        ),
    );
    if world.get::<Spells>(mob).and_then(|s| s.aiming) == Some(spell) {
        set_aim(world, mob, None);
    }
    start_cooldown(world, mob, spell, Some(TONGUELESS_COOLDOWN));
    false
}

// ---------------------------------------------------------------------------------------------
// Casting

/// What a spell is cast on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CastOn {
    /// The caster (self spells, aoe around it).
    Caster,
    /// A clicked atom (pointed spells), `right`: a right click.
    Atom {
        target: Entity,
        pos: TilePos,
        right: bool,
    },
    /// The area teleport's chosen area.
    Area(Entity),
}

/// `Activate`: the checks, `before_cast`, the feedback, the effect, the cooldown and
/// `after_cast`. True when it was cast.
pub fn cast(world: &mut World, client: Entity, mob: Entity, spell: Spell, on: CastOn) -> bool {
    if !ready(world, mob, spell) || !checked(world, mob, spell) || !tongue_check(world, mob, spell)
    {
        return false;
    }
    let level = world
        .get::<Spells>(mob)
        .and_then(|s| s.get(spell))
        .map_or(1, |k| k.level);
    let words = match on {
        // `area_teleport/invocation`: "[invocation], [AREA]!".
        CastOn::Area(area) if spell == Spell::Teleport => Some(format!(
            "{}, {}!",
            spell.def().invocation,
            crate::shuttle::area_name(world, area).to_uppercase()
        )),
        _ => None,
    };
    feedback(world, mob, spell, words.as_deref());
    info!("{mob} casts {spell:?} (level {level}) on {on:?}");
    if !effects::cast(world, client, mob, spell, level, on) {
        return false;
    }
    start_cooldown(world, mob, spell, None);
    true
}

fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        let Some(spell) = Spell::from_button(&id) else {
            continue;
        };
        // The client must still control the caster, and the caster know the spell (`Trigger`).
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || !knows(world, actor, spell)
        {
            continue;
        }
        trigger(world, client, actor, spell);
    }
}

/// tg `Trigger` of a spell button.
fn trigger(world: &mut World, client: Entity, mob: Entity, spell: Spell) {
    match spell.def().targeting {
        Targeting::Pointed { .. } => {
            let aiming = world.get::<Spells>(mob).and_then(|s| s.aiming);
            if aiming == Some(spell) {
                // `unset_click_ability(refund_cooldown = TRUE)` -> `on_deactivation`.
                set_aim(world, mob, None);
                let def = spell.def();
                let text = if def.deactive_msg.is_empty() {
                    format!("You dispel {}.", def.name)
                } else {
                    def.deactive_msg.to_owned()
                };
                chat_to(world, mob, Span::Notice, text);
                return;
            }
            if !ready(world, mob, spell) || !checked(world, mob, spell) {
                return;
            }
            set_aim(world, mob, Some(spell));
            let def = spell.def();
            let active = if def.active_msg.is_empty() {
                format!("You prepare to use {} on a target...", def.name)
            } else {
                def.active_msg.to_owned()
            };
            chat_to(
                world,
                mob,
                Span::Notice,
                format!("{active} Left-click to cast the spell on a target!"),
            );
        }
        Targeting::Touch { .. } => effects::toggle_hand(world, mob, spell),
        Targeting::Caster if spell == Spell::Teleport => {
            if ready(world, mob, spell) && checked(world, mob, spell) {
                open_teleport_prompt(world, client, mob, None);
            }
        }
        Targeting::Caster => {
            cast(world, client, mob, spell, CastOn::Caster);
        }
    }
}

/// `set_click_ability` / `unset_click_ability`.
pub fn set_aim(world: &mut World, mob: Entity, spell: Option<Spell>) {
    if let Some(mut spells) = world.get_mut::<Spells>(mob) {
        spells.aiming = spell;
    }
    if spell.is_some() {
        world.entity_mut(mob).insert(ClickIntercept);
    } else {
        world.entity_mut(mob).remove::<ClickIntercept>();
    }
    refresh(world, mob);
}

/// tg pointed `aim_assist`: a click on a turf means a human (else any mob) standing there.
fn aim_assist(world: &World, target: Entity, pos: TilePos) -> Entity {
    if world.get::<ss13_mapload::Turf>(target).is_none() {
        return target;
    }
    let on_tile = world.resource::<SpatialIndex>().on_tile(pos);
    on_tile
        .iter()
        .copied()
        .find(|&e| world.get::<HumanLook>(e).is_some() && world.get::<Health>(e).is_some())
        .or_else(|| {
            on_tile
                .iter()
                .copied()
                .find(|&e| world.get::<Health>(e).is_some())
        })
        .unwrap_or(target)
}

/// Clicks of a mob aiming a pointed spell: tg `InterceptClickOn`, `is_valid_target`, the
/// `cast_range` check of `before_cast`.
fn pointed_clicks(world: &mut World, mut clicks: Local<MessageCursor<InterceptedClick>>) {
    let clicks: Vec<InterceptedClick> = clicks
        .read(world.resource::<Messages<InterceptedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(spell) = world.get::<Spells>(click.mob).and_then(|s| s.aiming) else {
            // Not a spell's intercept.
            continue;
        };
        let Targeting::Pointed { range } = spell.def().targeting else {
            continue;
        };
        if world.get::<PlayerMob>(click.mob).map(|p| p.client) != Some(click.client) {
            continue;
        }
        let target = aim_assist(world, click.target, click.target_pos);
        // `swap/InterceptClickOn`: a right click marks (or on the caster clears) the second
        // target.
        if spell == Spell::Swap && click.right {
            effects::mark_swap_target(world, click.mob, target, click.target_pos);
            continue;
        }
        if !valid_target(world, click.mob, spell, target) {
            continue;
        }
        if click
            .mob_pos
            .chebyshev(click.target_pos)
            .is_none_or(|d| d > range)
        {
            chat_to(world, click.mob, Span::Warning, "too far away!");
            continue;
        }
        let on = CastOn::Atom {
            target,
            pos: click.target_pos,
            right: click.right,
        };
        if cast(world, click.client, click.mob, spell, on)
            && world.get::<Spells>(click.mob).and_then(|s| s.aiming) == Some(spell)
        {
            // `unset_after_click`.
            set_aim(world, click.mob, None);
        }
    }
}

/// `is_valid_target` of the pointed spells.
fn valid_target(world: &mut World, mob: Entity, spell: Spell, target: Entity) -> bool {
    match spell {
        // `pointed/projectile/is_valid_target`: anything.
        Spell::Fireball | Spell::LightningBolt => true,
        _ if target == mob => {
            chat_to(
                world,
                mob,
                Span::Warning,
                format!("You cannot cast {} on yourself!", spell.def().name),
            );
            false
        }
        Spell::Blind => {
            world.get::<HumanLook>(target).is_some()
                && world.get::<Health>(target).is_some()
                && !crate::vision::is_blind(world, target)
        }
        Spell::Swap => {
            let living = world.get::<Health>(target).is_some();
            if !living {
                chat_to(
                    world,
                    mob,
                    Span::Warning,
                    "You can only swap locations with living beings!",
                );
            }
            living
        }
        _ => true,
    }
}

// ---------------------------------------------------------------------------------------------
// Area teleport

/// `GLOB.teleportlocs`: the station's areas, by name.
pub fn teleport_areas(world: &World) -> Vec<(String, Entity)> {
    let mut out: Vec<(String, Entity)> = world
        .resource::<ss13_mapload::Areas>()
        .entities()
        .iter()
        .copied()
        .filter(|&a| {
            world
                .get::<ss13_mapload::DmPower>(a)
                .is_some_and(|p| is_under(&p.path, "/area/station"))
        })
        .map(|a| (crate::shuttle::area_name(world, a), a))
        .filter(|(name, _)| !name.is_empty())
        .collect();
    out.sort();
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// `area_teleport/before_cast`: "Chose an area to teleport to." (`scroll`: a scroll's).
pub fn open_teleport_prompt(
    world: &mut World,
    client: Entity,
    mob: Entity,
    scroll: Option<Entity>,
) {
    let areas = teleport_areas(world);
    if areas.is_empty() {
        return;
    }
    let items: Vec<String> = areas.iter().map(|(n, _)| n.clone()).collect();
    let ask = Ask::new(
        client,
        mob,
        "Teleport",
        "Chose an area to teleport to.",
        PromptKind::List {
            items,
            default: None,
        },
    );
    let prompt = ask_world(world, ask);
    world.entity_mut(prompt).insert(TeleportPrompt {
        scroll,
        areas: areas.into_iter().map(|(_, a)| a).collect(),
    });
}

fn teleport_answers(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in pending {
        let Some(asked) = world.get::<TeleportPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(&area) = asked.areas.get(i) else {
            continue;
        };
        // Checked again after the wait (`QDELETED(cast_on) || !can_cast_spell()`).
        if world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        match asked.scroll {
            None => {
                if knows(world, actor, Spell::Teleport) {
                    cast(world, client, actor, Spell::Teleport, CastOn::Area(area));
                }
            }
            Some(scroll) => effects::scroll_teleport(world, actor, scroll, area),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spells_are_in_enum_order_and_unique() {
        for (i, def) in SPELLS.iter().enumerate() {
            assert_eq!(def.spell as usize, i, "{}", def.path);
            assert_eq!(Spell::from_path(def.path), Some(def.spell));
            assert_eq!(Spell::from_button(&def.spell.button_id()), Some(def.spell));
        }
    }

    #[test]
    fn levels_shorten_the_cooldown_to_a_floor() {
        let fireball = Spell::Fireball.def();
        assert_eq!(cooldown_at(fireball, 1), 6.0);
        assert_eq!(cooldown_at(fireball, 5), 2.0);
        // Blink: 2 s - 4 * 0.4 s = 0.4 s; never below 0.25 s.
        let blink = Spell::Blink.def();
        assert!((cooldown_at(blink, 5) - 0.4).abs() < 1e-9);
        assert_eq!(cooldown_at(Spell::Teleport.def(), 3), 20.0);
        let swap = Spell::Swap.def();
        assert_eq!(cooldown_at(swap, 3), 5.0);
        assert_eq!(
            cooldown_at(
                &SpellDef {
                    cooldown: 1.0,
                    cooldown_reduction_per_rank: 1.0,
                    ..*swap
                },
                3
            ),
            MIN_COOLDOWN
        );
        assert_eq!(spell_title(1), "");
        assert_eq!(spell_title(3), "Quickened ");
    }

    #[test]
    fn antimagic_items_and_casting_clothes() {
        assert_eq!(
            item_antimagic("/obj/item/nullrod/claymore"),
            MAGIC_RESISTANCE | MAGIC_RESISTANCE_HOLY
        );
        assert_eq!(
            item_antimagic("/obj/item/book/bible"),
            MAGIC_RESISTANCE_HOLY
        );
        assert_eq!(item_antimagic("/obj/item/toolbox"), 0);
        assert!(is_casting_clothing("/obj/item/clothing/suit/wizrobe/red"));
        // tg's fake robes inherit the robe's `TRAIT_CASTING_CLOTHING`.
        assert!(is_casting_clothing("/obj/item/clothing/head/wizard/fake"));
        assert!(!is_casting_clothing("/obj/item/clothing/suit/armor"));
    }

    #[test]
    fn learning_levels_up_to_the_cap() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn_empty().id();
        assert_eq!(learn(&mut world, mob, Spell::Teleport), Learned::New);
        assert_eq!(learn(&mut world, mob, Spell::Teleport), Learned::Leveled(2));
        assert_eq!(learn(&mut world, mob, Spell::Teleport), Learned::Leveled(3));
        assert_eq!(learn(&mut world, mob, Spell::Teleport), Learned::MaxLevel);
        let known = world
            .get::<Spells>(mob)
            .unwrap()
            .get(Spell::Teleport)
            .unwrap();
        assert_eq!(known.name(), "Quickened Teleport");
        assert_eq!(forget(&mut world, mob, Spell::Teleport), 3);
        assert!(!knows(&world, mob, Spell::Teleport));
    }
}
