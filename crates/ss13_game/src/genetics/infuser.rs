//! The DNA infuser (`code/game/machinery/dna_infuser/`: `/obj/machinery/dna_infuser`,
//! `dna_infusion.dm`, `/datum/infuser_entry` and its tier entries) and the organ set bonuses of
//! the infused organs (`/datum/element/organ_set_bonus`, `/datum/status_effect/organ_set_bonus`
//! from `organ_sets/*.dm`).
//!
//! - The machine: an empty hand opens or closes the door (closing shuts in a human on its tile;
//!   not while infusing or with the panel open), a step from inside opens it unless the occupant
//!   is unconscious or being infused. A dead creature dragged onto it, or an item of creature DNA
//!   (gory food, plants, fish) used on it outside combat mode, becomes the infusion source;
//!   alt-click ejects it.
//! - An empty hand with an occupant and a source starts the infusion (an occupant whose DNA can
//!   mutate): 10 brute, a scream after 3 s, and after 4 s one organ of the source's entry
//!   (the first whose inputs the source is; none: the fly entry) replaces the occupant's organic
//!   organ of its slot, the source is gone and the door opens. An entry above the machine's tier
//!   (1 at first) infuses fly DNA instead; both print an error report. Reaching an entry's set
//!   bonus with an entry of the machine's tier raises the tier (up to 2).
//! - Organ sets: a body with enough organs of one set (`organs_needed`) gets its bonus, with
//!   tg's lines; too much fly DNA turns the body into a flyperson.
// shortcut: only the organs of the modelled slots are infused (no fangs, tails, horns, frills,
// snouts, antennae, wings, pod hair: no external organ slots, see `docs/parity/medical/organs.md`);
// the infused organs carry tg's types but the standard numbers and no abilities of their own,
// and the set bonuses other than the fly's give only their lines (no lava walking, space
// swimming, ventcrawling, roach armor, fish wetness, stoat dodging, gondola zen); no fish
// trait infusions (ink, TTX, the lungs dropped for air-breathing fish), no infusion memory or
// mood, no shake animation, no DNA infusion book (`InfuserBook`), no screwdriver or crowbar
// deconstruction. Add the abilities with the organ effects.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmFood, DmItem, DmMob, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::{Density, SpatialIndex};

use crate::closets::{ClosetRelayMove, InCloset};
use crate::health::{DamageType, Health, Stat};
use crate::human::HumanLook;
use crate::medical::organs::{self, Organ, OrganOf, Slot};
use crate::medical::pods::{self, Pod};
use crate::sound::{PlaySound, playsound};
use crate::species::SpeciesId;
use crate::tools::{now, tell_mob};

pub const INFUSER_PATH: &str = "/obj/machinery/dna_infuser";
/// `INFUSING_TIME`, `SCREAM_TIME`.
const INFUSING_SECS: f64 = 4.0;
const SCREAM_SECS: f64 = 3.0;
/// The door's `message_cooldown`.
const MESSAGE_SECS: f64 = 4.0;
/// `DNA_MUTANT_TIER_ONE` (the machine's start), `DNA_INFUSER_MAX_TIER`.
const START_TIER: u8 = 1;
const MAX_TIER: u8 = 2;
const PAPER: &str = "/obj/item/paper";
/// `foodtypes & GORE`: `TRAIT_VALID_DNA_INFUSION` of edibles.
const GORE: u32 = crate::food::foodtype::GORE;
/// Grown plants carry `TRAIT_VALID_DNA_INFUSION` innately.
const GROWN: [&str; 2] = ["/obj/item/food/grown", "/obj/item/grown"];

/// A `/datum/status_effect/organ_set_bonus` subtype.
#[derive(Clone, Copy, Debug)]
pub struct OrganSetDef {
    pub key: &'static str,
    /// `organs_needed`.
    pub needed: usize,
    /// The organ types (and their subtypes) with the set's `organ_set_bonus` element.
    pub organs: &'static [&'static str],
    pub activate: &'static str,
    pub deactivate: &'static str,
}

pub static ORGAN_SETS: [OrganSetDef; 8] = [
    OrganSetDef {
        key: "fly",
        needed: 4,
        organs: &[
            "/obj/item/organ/eyes/fly",
            "/obj/item/organ/tongue/fly",
            "/obj/item/organ/heart/fly",
            "/obj/item/organ/lungs/fly",
            "/obj/item/organ/liver/fly",
            "/obj/item/organ/stomach/fly",
            "/obj/item/organ/appendix/fly",
        ],
        activate: "",
        deactivate: "",
    },
    OrganSetDef {
        key: "goliath",
        needed: 4,
        organs: &[
            "/obj/item/organ/eyes/night_vision/goliath",
            "/obj/item/organ/lungs/lavaland/goliath",
            "/obj/item/organ/brain/goliath",
            "/obj/item/organ/heart/goliath",
        ],
        activate: "ДНК голиафа глубоко проникла в вас! Теперь вы можете ходить по лаве!",
        deactivate: "Вы чувствуете, как ваши мышцы уменьшаются, а щупальца на коже увядают. \
                     ДНК голиафа почти покинула вас, а с ней и способность выживать в лаве.",
    },
    OrganSetDef {
        key: "carp",
        needed: 5,
        organs: &[
            "/obj/item/organ/lungs/carp",
            "/obj/item/organ/fangs/carp",
            "/obj/item/organ/tongue/carp",
            "/obj/item/organ/brain/carp",
            "/obj/item/organ/heart/carp",
        ],
        activate: "ДНК карпа глубоко проникла в вас! Вы научились перемещаться в космосе!",
        deactivate: "Ваша ДНК снова в основном ваша, и способность плавать в космосе угасает...",
    },
    OrganSetDef {
        key: "rat",
        needed: 4,
        organs: &[
            "/obj/item/organ/eyes/night_vision/rat",
            "/obj/item/organ/stomach/rat",
            "/obj/item/organ/heart/rat",
            "/obj/item/organ/tongue/rat",
        ],
        activate: "ДНК грызуна глубоко проникла в вас! Вы научились ползать по вентиляции!",
        deactivate: "Ваша ДНК больше не крысиная, и навыки ползания по вентиляции угасают...",
    },
    OrganSetDef {
        key: "roach",
        needed: 4,
        organs: &[
            "/obj/item/organ/heart/roach",
            "/obj/item/organ/stomach/roach",
            "/obj/item/organ/liver/roach",
            "/obj/item/organ/appendix/roach",
        ],
        activate: "ДНК таракана глубоко проникла в вас! Вы чувствуете себя на удивление стойким.",
        deactivate: "Вы больше не таракан в большей части, и стойкость покидает вас.",
    },
    OrganSetDef {
        key: "fish",
        needed: 3,
        organs: &[
            "/obj/item/organ/tail/fish",
            "/obj/item/organ/lungs/fish",
            "/obj/item/organ/stomach/fish",
            "/obj/item/organ/tongue/fish",
            "/obj/item/organ/liver/fish",
        ],
        activate: "ДНК рыбы глубоко проникла в вас! Мокрым вы ползаете быстрее, скользкий и не \
                   поскальзываетесь, а высыхаете дольше.",
        deactivate: "Вы больше не чувствуете себя рыбой. Влага на теле начинает испаряться \
                     быстрее...",
    },
    OrganSetDef {
        key: "stoat",
        needed: 5,
        organs: &[
            "/obj/item/organ/heart/stoat",
            "/obj/item/organ/fangs/stoat",
            "/obj/item/organ/tongue/stoat",
            "/obj/item/organ/eyes/stoat",
            "/obj/item/organ/ears/stoat",
            "/obj/item/organ/snout/stoat",
        ],
        activate: "ДНК горностая глубоко проникла в вас! Вы бесстрашны перед теми, кто крупнее \
                   вас.",
        deactivate: "Вы больше не горностай в большей части, и бесстрашие покидает вас.",
    },
    OrganSetDef {
        key: "gondola",
        needed: 3,
        organs: &[
            "/obj/item/organ/heart/gondola",
            "/obj/item/organ/tongue/gondola",
            "/obj/item/organ/liver/gondola",
        ],
        activate: "ДНК гондолы глубоко проникла в вас! Вы — совершенный наблюдатель, \
                   безразличный к окружающей среде...",
        deactivate: "Ваша ДНК больше не безмятежна, и вы начинаете вспоминать, что дышать, \
                     вообще-то, важно...",
    },
];

/// A `/datum/infuser_entry` with inputs (the fish trait ones are not ported).
#[derive(Clone, Copy, Debug)]
pub struct InfuserEntry {
    pub key: &'static str,
    pub tier: u8,
    /// `input_obj_or_mob` (types and their subtypes).
    pub inputs: &'static [&'static str],
    pub outputs: &'static [&'static str],
    /// `infusion_desc`.
    pub desc: &'static str,
    /// `status_effect_type`: an [`ORGAN_SETS`] key.
    pub set: Option<&'static str>,
}

/// In tg's `subtypesof` order (tier one, tier two, tier zero files).
pub static ENTRIES: [InfuserEntry; 15] = [
    InfuserEntry {
        key: "goliath",
        tier: 1,
        inputs: &["/mob/living/basic/mining/goliath"],
        outputs: &[
            "/obj/item/organ/brain/goliath",
            "/obj/item/organ/eyes/night_vision/goliath",
            "/obj/item/organ/heart/goliath",
            "/obj/item/organ/lungs/lavaland/goliath",
        ],
        desc: "бронированным, щупальцеподобным",
        set: Some("goliath"),
    },
    InfuserEntry {
        key: "carp",
        tier: 1,
        inputs: &["/mob/living/basic/carp"],
        outputs: &[
            "/obj/item/organ/brain/carp",
            "/obj/item/organ/fangs/carp",
            "/obj/item/organ/heart/carp",
            "/obj/item/organ/lungs/carp",
            "/obj/item/organ/tongue/carp",
        ],
        desc: "кочевым",
        set: Some("carp"),
    },
    InfuserEntry {
        key: "rat",
        tier: 1,
        inputs: &["/obj/item/food/deadmouse"],
        outputs: &[
            "/obj/item/organ/eyes/night_vision/rat",
            "/obj/item/organ/heart/rat",
            "/obj/item/organ/stomach/rat",
            "/obj/item/organ/tongue/rat",
        ],
        desc: "пугливым",
        set: Some("rat"),
    },
    InfuserEntry {
        key: "roach",
        tier: 1,
        inputs: &["/mob/living/basic/cockroach"],
        outputs: &[
            "/obj/item/organ/heart/roach",
            "/obj/item/organ/stomach/roach",
            "/obj/item/organ/liver/roach",
            "/obj/item/organ/appendix/roach",
        ],
        desc: "кафкианским",
        set: Some("roach"),
    },
    InfuserEntry {
        key: "fish",
        tier: 1,
        inputs: &["/obj/item/fish"],
        outputs: &[
            "/obj/item/organ/lungs/fish",
            "/obj/item/organ/stomach/fish",
            "/obj/item/organ/tail/fish",
        ],
        desc: "рыбьим",
        set: Some("fish"),
    },
    InfuserEntry {
        key: "amphibious",
        tier: 1,
        inputs: &[
            "/mob/living/basic/frog",
            "/mob/living/basic/axolotl",
            "/mob/living/basic/crab",
        ],
        outputs: &["/obj/item/organ/lungs/fish/amphibious"],
        desc: "полуводным",
        set: Some("fish"),
    },
    InfuserEntry {
        key: "stoat",
        tier: 1,
        inputs: &["/mob/living/basic/stoat"],
        outputs: &[
            "/obj/item/organ/eyes/stoat",
            "/obj/item/organ/ears/stoat",
            "/obj/item/organ/fangs/stoat",
            "/obj/item/organ/heart/stoat",
            "/obj/item/organ/tongue/stoat",
            "/obj/item/organ/snout/stoat",
        ],
        desc: "горностаевым",
        set: Some("stoat"),
    },
    InfuserEntry {
        key: "gondola",
        tier: 2,
        inputs: &["/obj/item/food/meat/slab/gondola"],
        outputs: &[
            "/obj/item/organ/heart/gondola",
            "/obj/item/organ/tongue/gondola",
            "/obj/item/organ/liver/gondola",
        ],
        desc: "наблюдательным",
        set: Some("gondola"),
    },
    InfuserEntry {
        key: "fly",
        tier: 0,
        inputs: &[],
        outputs: &[
            "/obj/item/organ/appendix/fly",
            "/obj/item/organ/eyes/fly",
            "/obj/item/organ/heart/fly",
            "/obj/item/organ/lungs/fly",
            "/obj/item/organ/stomach/fly",
            "/obj/item/organ/tongue/fly",
        ],
        desc: "мухоподобным",
        set: Some("fly"),
    },
    InfuserEntry {
        key: "vulpini",
        tier: 0,
        inputs: &["/mob/living/basic/pet/fox"],
        outputs: &["/obj/item/organ/ears/fox"],
        desc: "непростительным",
        set: None,
    },
    InfuserEntry {
        key: "mothroach",
        tier: 0,
        inputs: &["/mob/living/basic/mothroach"],
        outputs: &[
            "/obj/item/organ/antennae",
            "/obj/item/organ/wings/moth",
            "/obj/item/organ/eyes/moth",
            "/obj/item/organ/tongue/moth",
        ],
        desc: "пушистым",
        set: None,
    },
    InfuserEntry {
        key: "lizard",
        tier: 0,
        inputs: &["/mob/living/basic/lizard"],
        outputs: &[
            "/obj/item/organ/horns",
            "/obj/item/organ/frills",
            "/obj/item/organ/snout",
            "/obj/item/organ/tail/lizard",
            "/obj/item/organ/tongue/lizard",
        ],
        desc: "чешуйчатым",
        set: None,
    },
    InfuserEntry {
        key: "felinid",
        tier: 0,
        inputs: &["/mob/living/basic/pet/cat"],
        outputs: &["/obj/item/organ/ears/cat", "/obj/item/organ/tail/cat"],
        desc: "домашним",
        set: None,
    },
    InfuserEntry {
        key: "penguin",
        tier: 0,
        inputs: &["/mob/living/basic/pet/penguin"],
        outputs: &["/obj/item/organ/ears/penguin"],
        desc: "переваливающимся",
        set: None,
    },
    InfuserEntry {
        key: "plants",
        tier: 0,
        inputs: &["/obj/item/food/grown", "/obj/item/grown"],
        outputs: &[
            "/obj/item/organ/appendix/pod",
            "/obj/item/organ/brain/pod",
            "/obj/item/organ/ears/pod",
            "/obj/item/organ/eyes/pod",
            "/obj/item/organ/heart/pod",
            "/obj/item/organ/liver/pod",
            "/obj/item/organ/lungs/pod",
            "/obj/item/organ/pod_hair",
            "/obj/item/organ/stomach/pod",
            "/obj/item/organ/tongue/pod",
        ],
        desc: "ботаническим",
        set: None,
    },
];

fn entry(key: &str) -> &'static InfuserEntry {
    ENTRIES.iter().find(|e| e.key == key).unwrap_or(&ENTRIES[8])
}

fn set_def(key: &str) -> Option<&'static OrganSetDef> {
    ORGAN_SETS.iter().find(|s| s.key == key)
}

/// `/obj/machinery/dna_infuser` vars (the door is [`Pod`]).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct DnaInfuser {
    /// `max_tier_allowed`.
    pub max_tier: u8,
    /// `infusing_from`.
    pub source: Option<Entity>,
    /// `infusing_into` while infusing, with the error report (title, text).
    pub infusing: Option<(&'static str, Option<(&'static str, &'static str)>)>,
    pub scream_at: Option<f64>,
    pub done_at: f64,
    pub message_at: f64,
}

impl Default for DnaInfuser {
    fn default() -> Self {
        Self {
            max_tier: START_TIER,
            source: None,
            infusing: None,
            scream_at: None,
            done_at: 0.0,
            message_at: 0.0,
        }
    }
}

/// The active organ set bonuses of a body (`organ_set_bonus` status effects with
/// `bonus_active`).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct OrganSetBonuses(pub Vec<&'static str>);

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // tg `interact`: the door, or the infusion.
    app.add_click_handler(ClickPhase::EmptyHand, 2615, |w, c| {
        interact(w, c.client, c.mob, c.target)
    });
    // tg `item_interaction`: an item of creature DNA becomes the source.
    app.add_click_handler(ClickPhase::HeldOnTarget, 1615, |w, c| {
        c.held
            .is_some_and(|h| item_interaction(w, c.mob, h, c.target))
    });
    app.add_observer(attach).add_systems(
        Update,
        (
            relay_moves.after(ss13_world::WorldSystems::Movement),
            timers.run_if(any_with_component::<DnaInfuser>),
        ),
    );
}

fn attach(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if objs
        .get(add.entity)
        .is_ok_and(|o| is_under(&o.path, INFUSER_PATH))
    {
        let e = add.entity;
        commands
            .entity(e)
            .insert_if_new((Pod { open: false }, DnaInfuser::default()));
        commands.queue(move |world: &mut World| refresh_look(world, e));
    }
}

fn path_of(world: &World, e: Entity) -> Option<String> {
    let entity = world.get_entity(e).ok()?;
    entity
        .get::<DmItem>()
        .map(|i| i.path.clone())
        .or_else(|| entity.get::<DmObj>().map(|o| o.path.clone()))
        .or_else(|| entity.get::<DmMob>().map(|m| m.path.clone()))
}

fn name_of(world: &World, e: Entity) -> String {
    crate::medical::name_of(world, e)
}

fn balloon(world: &mut World, mob: Entity, source: Entity, text: &str) {
    crate::balloon::balloon_alert(world, mob, source, text);
}

fn sound(world: &mut World, at: Entity, file: &str, vol: f32, vary: bool) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        playsound(world, PlaySound::new(pos, file, vol, vary));
    }
}

/// The human in the chamber (not the source, which may be a corpse inside too).
pub fn occupant(world: &mut World, infuser: Entity) -> Option<Entity> {
    let source = world.get::<DnaInfuser>(infuser)?.source;
    let mut q = world.query::<(Entity, &InCloset)>();
    q.iter(world)
        .find(|(e, c)| c.closet == infuser && Some(*e) != source)
        .map(|(e, _)| e)
}

/// `update_icon_state`.
fn refresh_look(world: &mut World, infuser: Entity) {
    let Some(state) = world.get::<DnaInfuser>(infuser) else {
        return;
    };
    let infusing = state.infusing.is_some();
    let open = world.get::<Pod>(infuser).is_some_and(|p| p.open);
    let icon = if !pods::operational(world, infuser) {
        "infuser"
    } else if pods::panel_open(world, infuser) {
        "infuser_panel"
    } else if infusing {
        "infuser_on"
    } else if open {
        "infuser_open"
    } else {
        "infuser"
    };
    crate::obj_damage::set_icon_state(world, infuser, icon);
}

/// `open_machine(drop = FALSE)` + `dump_inventory_contents(list(occupant))`: only the occupant
/// leaves.
fn open(world: &mut World, infuser: Entity) {
    if let Some(mob) = occupant(world, infuser) {
        crate::closets::let_out(world, mob);
    }
    if let Some(mut p) = world.get_mut::<Pod>(infuser) {
        p.open = true;
    }
    world.entity_mut(infuser).remove::<Density>();
    refresh_look(world, infuser);
}

/// `close_machine()`: a human on the tile (`occupant_typecache`) goes in.
fn close(world: &mut World, infuser: Entity) {
    let Some(&pos) = world.get::<TilePos>(infuser) else {
        return;
    };
    let human = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| {
            world.get::<HumanLook>(e).is_some()
                && world.get::<Health>(e).is_some()
                && world.get::<InCloset>(e).is_none()
                && world.get::<crate::medical::surgery::Buckled>(e).is_none()
        });
    if let Some(mut p) = world.get_mut::<Pod>(infuser) {
        p.open = false;
    }
    world.entity_mut(infuser).insert(Density);
    if let Some(mob) = human.filter(|_| occupant(world, infuser).is_none()) {
        world.entity_mut(mob).insert(pos);
        crate::closets::put_mob_inside(world, mob, infuser);
    }
    refresh_look(world, infuser);
}

/// `toggle_open(user)`.
fn toggle_open(world: &mut World, infuser: Entity, user: Option<Entity>) {
    if pods::panel_open(world, infuser) {
        if let Some(user) = user {
            balloon(world, user, infuser, "сначала закройте панель!");
        }
        return;
    }
    if world.get::<Pod>(infuser).is_some_and(|p| p.open) {
        close(world, infuser);
    } else if world
        .get::<DnaInfuser>(infuser)
        .is_some_and(|i| i.infusing.is_some())
    {
        if let Some(user) = user {
            balloon(world, user, infuser, "не во время работы!");
        }
    } else {
        open(world, infuser);
    }
}

/// `interact(user)`.
pub fn interact(world: &mut World, _client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(state) = world.get::<DnaInfuser>(target).cloned() else {
        return false;
    };
    if state.infusing.is_some() {
        balloon(world, mob, target, "не во время работы!");
        return true;
    }
    if let (Some(occupant), Some(_)) = (occupant(world, target), state.source) {
        if !can_infuse(world, occupant, mob, target) {
            sound(
                world,
                target,
                "sound/machines/scanner/scanbuzz.ogg",
                35.0,
                true,
            );
            return true;
        }
        balloon(world, mob, target, "начинаю вливание ДНК...");
        start_infuse(world, target);
        return true;
    }
    toggle_open(world, target, Some(mob));
    true
}

/// `/mob/living/carbon/human/can_infuse`, with its feedback to `user`.
fn can_infuse(world: &mut World, occupant: Entity, user: Entity, infuser: Entity) -> bool {
    if world.get::<HumanLook>(occupant).is_none() {
        balloon(world, user, infuser, "слишком простая ДНК!");
        return false;
    }
    if !crate::species::can_mutate(world, occupant) {
        balloon(world, user, infuser, "ДНК отсутствует!");
        return false;
    }
    true
}

/// `is_valid_infusion`: a dead creature, or an item of creature DNA; the slot must be empty.
fn valid_source(world: &mut World, target: Entity, user: Entity, infuser: Entity) -> bool {
    if world
        .get::<DnaInfuser>(infuser)
        .is_some_and(|i| i.source.is_some())
    {
        balloon(world, user, infuser, "сначала опустошите машину!");
        return false;
    }
    if world.get::<Health>(target).is_some() {
        if world.get::<Stat>(target) != Some(&Stat::Dead) {
            balloon(world, user, infuser, "только мёртвые существа!");
            return false;
        }
        return true;
    }
    let gory = world
        .get::<DmFood>(target)
        .is_some_and(|f| f.foodtypes & GORE != 0);
    let grown = path_of(world, target).is_some_and(|p| GROWN.iter().any(|root| is_under(&p, root)));
    if !gory && !grown {
        balloon(world, user, infuser, "только существа!");
        return false;
    }
    true
}

/// `item_interaction`: out of combat mode, a valid item goes into the infusion slot.
pub fn item_interaction(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    if world.get::<DnaInfuser>(target).is_none() || crate::medical::surgery::combat_mode(world, mob)
    {
        return false;
    }
    if !valid_source(world, held, mob, target) {
        return false;
    }
    world
        .entity_mut(held)
        .remove::<crate::inventory::CarriedBy>();
    if let Some(mut inventory) = world.get_mut::<crate::inventory::Inventory>(mob)
        && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(held))
    {
        *slot = None;
    }
    crate::storage::put_inside(world, held, target);
    if let Some(mut i) = world.get_mut::<DnaInfuser>(target) {
        i.source = Some(held);
    }
    true
}

/// `mouse_drop_receive`: a dead creature (or a valid item) dragged onto it is the source.
pub fn mouse_drop_receive(
    world: &mut World,
    user: Entity,
    infuser: Entity,
    dragged: Entity,
) -> bool {
    if world.get::<DnaInfuser>(infuser).is_none() {
        return false;
    }
    if !valid_source(world, dragged, user, infuser) {
        return true;
    }
    if world.get::<Health>(dragged).is_some() {
        crate::closets::put_mob_inside(world, dragged, infuser);
    } else {
        crate::storage::put_inside(world, dragged, infuser);
    }
    if let Some(mut i) = world.get_mut::<DnaInfuser>(infuser) {
        i.source = Some(dragged);
    }
    true
}

/// `click_alt`: the source back out.
pub fn click_alt(world: &mut World, mob: Entity, target: Entity) -> bool {
    let Some(state) = world.get::<DnaInfuser>(target).cloned() else {
        return false;
    };
    if state.infusing.is_some() {
        balloon(world, mob, target, "не во время работы!");
        return true;
    }
    let Some(source) = state.source else {
        balloon(world, mob, target, "нет образца для извлечения!");
        return true;
    };
    balloon(world, mob, target, "образец извлечён");
    eject_source(world, target, source);
    true
}

fn eject_source(world: &mut World, infuser: Entity, source: Entity) {
    if let Some(mut i) = world.get_mut::<DnaInfuser>(infuser) {
        i.source = None;
    }
    if world.get_entity(source).is_err() {
        return;
    }
    if world.get::<InCloset>(source).is_some() {
        crate::closets::let_out(world, source);
    } else if let Some(&pos) = world.get::<TilePos>(infuser) {
        crate::storage::take_out(world, source, pos);
    }
}

/// `relaymove`: the door does not budge for the unconscious or during the infusion.
fn relay_moves(world: &mut World, mut moves: Local<MessageCursor<ClosetRelayMove>>) {
    let mobs: Vec<Entity> = moves
        .read(world.resource::<Messages<ClosetRelayMove>>())
        .map(|m| m.mob)
        .collect();
    let now = now(world);
    for mob in mobs {
        let Some(infuser) = world.get::<InCloset>(mob).map(|c| c.closet) else {
            continue;
        };
        let Some(state) = world.get::<DnaInfuser>(infuser).cloned() else {
            continue;
        };
        if state.source == Some(mob) {
            continue;
        }
        let stuck = if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
            Some((Span::Warning, "Дверь {} не поддаётся!"))
        } else if state.infusing.is_some() {
            Some((
                Span::Danger,
                "Дверь {} не поддаётся, пока иглы вливают в вас ДНК!",
            ))
        } else {
            None
        };
        match stuck {
            Some((span, text)) => {
                if now >= state.message_at {
                    if let Some(mut i) = world.get_mut::<DnaInfuser>(infuser) {
                        i.message_at = now + MESSAGE_SECS;
                    }
                    let name = name_of(world, infuser);
                    tell_mob(world, mob, span, text.replace("{}", &name));
                }
            }
            None => open(world, infuser),
        }
    }
}

/// `get_infusion_entry`: the first entry with the source among its inputs, else the fly's.
pub fn infusion_entry(path: Option<&str>) -> &'static InfuserEntry {
    path.and_then(|p| {
        ENTRIES
            .iter()
            .find(|e| e.inputs.iter().any(|root| is_under(p, root)))
    })
    .unwrap_or_else(|| entry("fly"))
}

/// `start_infuse`.
fn start_infuse(world: &mut World, infuser: Entity) {
    let Some(state) = world.get::<DnaInfuser>(infuser).cloned() else {
        return;
    };
    let Some(mob) = occupant(world, infuser) else {
        return;
    };
    let source_path = state.source.and_then(|s| path_of(world, s));
    let mut into = infusion_entry(source_path.as_deref());
    let mut report = None;
    if into.key == "fly" {
        report = Some((
            "Неизвестная ДНК",
            "Неизвестная ДНК. Обратитесь к «Книге вливания ДНК».",
        ));
    }
    if into.tier > state.max_tier {
        into = entry("fly");
        report = Some((
            "Чрезмерная сложность",
            "ДНК слишком сложна для вливания. Сначала машине нужно влить более простую ДНК.",
        ));
    }
    let now = now(world);
    if let Some(mut i) = world.get_mut::<DnaInfuser>(infuser) {
        i.infusing = Some((into.key, report));
        i.scream_at = Some(now + SCREAM_SECS);
        i.done_at = now + INFUSING_SECS;
    }
    let name = name_of(world, infuser);
    let text = format!("{name} оживает с гулом, начиная процесс вливания!");
    crate::medical::visible(world, infuser, &text, &text, Span::Notice);
    sound(world, infuser, "sound/machines/blender.ogg", 50.0, true);
    tell_mob(
        world,
        mob,
        Span::Danger,
        "Маленькие иголки раз за разом колют вас!",
    );
    if let Some(mut health) = world.get::<Health>(mob).cloned() {
        health.adjust(DamageType::Brute, 10.0);
        world.entity_mut(mob).insert(health);
    }
    refresh_look(world, infuser);
}

/// `end_infuse`.
fn end_infuse(world: &mut World, infuser: Entity) {
    let Some(state) = world.get::<DnaInfuser>(infuser).cloned() else {
        return;
    };
    let Some((key, report)) = state.infusing else {
        return;
    };
    let into = entry(key);
    if let Some(mob) = occupant(world, infuser) {
        let source_path = state.source.and_then(|s| path_of(world, s));
        if infuse_organ(world, mob, into, source_path.as_deref()) {
            check_tier_progression(world, infuser, mob, into);
            tell_mob(
                world,
                mob,
                Span::Danger,
                format!("Вы чувствуете, что становитесь более... {}?", into.desc),
            );
        }
    }
    if let Some(mut i) = world.get_mut::<DnaInfuser>(infuser) {
        i.infusing = None;
        i.scream_at = None;
        i.source = None;
    }
    if let Some(source) = state.source {
        world.try_despawn(source).ok();
    }
    sound(
        world,
        infuser,
        "sound/machines/microwave/microwave-end.ogg",
        100.0,
        false,
    );
    if let Some((title, text)) = report
        && let Some(&pos) = world.get::<TilePos>(infuser)
    {
        sound(world, infuser, "sound/machines/printer.ogg", 100.0, true);
        let name = name_of(world, infuser);
        let line = format!("{name} печатает отчёт об ошибке.");
        crate::medical::visible(world, infuser, &line, &line, Span::Notice);
        if let Some(paper) = crate::construction::spawn_obj(world, PAPER, pos) {
            world.entity_mut(paper).insert(ss13_mapload::DmName {
                name: format!("отчёт об ошибке - '{title}'"),
                desc: String::new(),
            });
            crate::service::write_printed(world, paper, text);
        }
    }
    toggle_open(world, infuser, None);
    refresh_look(world, infuser);
}

/// The organ types of `mob`.
fn organ_paths(world: &mut World, mob: Entity) -> Vec<(Slot, String, bool)> {
    let mut q = world.query::<(&Organ, &OrganOf)>();
    q.iter(world)
        .filter(|(_, of)| of.0 == mob)
        .map(|(o, _)| (o.slot, o.path.clone(), o.robotic))
        .collect()
}

/// The entry's `get_output_organs` (the stoat's eyes and ears come last).
fn output_organs(world: &mut World, mob: Entity, into: &InfuserEntry) -> Vec<&'static str> {
    if into.key == "stoat" && set_count(world, mob, "stoat") < 3 {
        return vec![
            "/obj/item/organ/heart/stoat",
            "/obj/item/organ/tongue/stoat",
            "/obj/item/organ/snout/stoat",
        ];
    }
    into.outputs.to_vec()
}

/// `pick_infusion_organ`: an organ of the entry whose slot holds a different organic organ.
pub fn pick_infusion_organ(
    world: &mut World,
    mob: Entity,
    into: &InfuserEntry,
    rng: &mut crate::combat::CombatRng,
) -> Option<&'static str> {
    let have = organ_paths(world, mob);
    let candidates: Vec<&'static str> = output_organs(world, mob, into)
        .into_iter()
        .filter(|&path| {
            // External organs have no slot here.
            let Some(slot) = Slot::of_path(path) else {
                return false;
            };
            have.iter()
                .find(|(s, ..)| *s == slot)
                .is_some_and(|(_, old, robotic)| old.as_str() != path && !*robotic)
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    Some(candidates[rng.range(0, candidates.len() as u32 - 1) as usize])
}

/// `infuse_organ`: the picked organ replaces the old one (`replace_into`).
fn infuse_organ(
    world: &mut World,
    mob: Entity,
    into: &InfuserEntry,
    _source: Option<&str>,
) -> bool {
    let mut rng = crate::medical::fork_rng(world);
    let Some(path) = pick_infusion_organ(world, mob, into, &mut rng) else {
        return false;
    };
    let Some(new) = organs::spawn_organ(world, path) else {
        return false;
    };
    let slot = world.get::<Organ>(new).map(|o| o.slot);
    if let Some(old) = slot.and_then(|slot| organs::organ(world, mob, slot)) {
        organs::remove(world, old, true);
        world.try_despawn(old).ok();
    }
    if !organs::insert(world, new, mob) {
        world.try_despawn(new).ok();
        return false;
    }
    true
}

/// `check_tier_progression`.
fn check_tier_progression(world: &mut World, infuser: Entity, mob: Entity, into: &InfuserEntry) {
    let Some(max) = world.get::<DnaInfuser>(infuser).map(|i| i.max_tier) else {
        return;
    };
    let bonus = into.set.is_some_and(|set| {
        world
            .get::<OrganSetBonuses>(mob)
            .is_some_and(|b| b.0.contains(&set))
    });
    if max == MAX_TIER || into.tier != max || !bonus {
        return;
    }
    if let Some(mut i) = world.get_mut::<DnaInfuser>(infuser) {
        i.max_tier += 1;
    }
    sound(world, infuser, "sound/machines/ding.ogg", 50.0, true);
    let name = name_of(world, infuser);
    let text = format!("{name} звенит, записывая результаты полного вливания.");
    crate::medical::visible(world, infuser, &text, &text, Span::Notice);
}

/// How many organs of `set` the body holds.
pub fn set_count(world: &mut World, mob: Entity, set: &str) -> usize {
    let Some(def) = set_def(set) else {
        return 0;
    };
    organ_paths(world, mob)
        .iter()
        .filter(|(_, path, _)| def.organs.iter().any(|root| is_under(path, root)))
        .count()
}

/// `organ_set_bonus/set_organs` of every set after the organs changed: a bonus turns on at
/// `organs_needed` (`enable_bonus`: an organic body that can mutate) and off below it.
pub fn refresh_set_bonuses(world: &mut World, mob: Entity) {
    if world.get_entity(mob).is_err() {
        return;
    }
    let had = world
        .get::<OrganSetBonuses>(mob)
        .cloned()
        .unwrap_or_default();
    let mutable = crate::species::can_mutate(world, mob);
    let mut active = Vec::new();
    let (mut gained, mut lost) = (Vec::new(), Vec::new());
    for def in ORGAN_SETS.iter() {
        let count = set_count(world, mob, def.key);
        let was = had.0.contains(&def.key);
        if count >= def.needed && (was || mutable) {
            active.push(def.key);
            if !was {
                gained.push(def);
            }
        } else if was {
            lost.push(def);
        }
    }
    if had.0 == active {
        return;
    }
    // Stored first: a bonus that changes the species changes organs and comes back here.
    if active.is_empty() {
        world.entity_mut(mob).remove::<OrganSetBonuses>();
    } else {
        world.entity_mut(mob).insert(OrganSetBonuses(active));
    }
    for def in lost {
        disable_bonus(world, mob, def);
    }
    for def in gained {
        enable_bonus(world, mob, def);
    }
}

fn enable_bonus(world: &mut World, mob: Entity, def: &OrganSetDef) {
    if !def.activate.is_empty() {
        tell_mob(world, mob, Span::Notice, def.activate);
    }
    // `organ_set_bonus/fly/flyify`.
    if def.key == "fly" && crate::species::species_of(world, mob) != SpeciesId::Fly {
        tell_mob(
            world,
            mob,
            Span::Danger,
            "Слишком много мушиной ДНК! Ваша кожа начинает чернеть, и вы становитесь больше \
             мухой, чем человеком!",
        );
        crate::species::set_species(world, mob, SpeciesId::Fly);
        crate::inventory::refresh_look(world, mob);
    }
}

fn disable_bonus(world: &mut World, mob: Entity, def: &OrganSetDef) {
    if !def.deactivate.is_empty() {
        tell_mob(world, mob, Span::Notice, def.deactivate);
    }
}

/// The infusion's scream and end.
// Cost: per tick one pass over the infusers (a few per map).
fn timers(world: &mut World) {
    let now = now(world);
    let due: Vec<(Entity, bool, bool)> = world
        .query::<(Entity, &DnaInfuser)>()
        .iter(world)
        .filter(|(_, i)| i.infusing.is_some())
        .map(|(e, i)| (e, i.scream_at.is_some_and(|t| now >= t), now >= i.done_at))
        .collect();
    for (infuser, scream, done) in due {
        if scream {
            if let Some(mut i) = world.get_mut::<DnaInfuser>(infuser) {
                i.scream_at = None;
            }
            if let Some(mob) = occupant(world, infuser) {
                crate::medical::diseases::emote(world, mob, "scream");
            }
        }
        if done {
            end_infuse(world, infuser);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_pick_their_entry_and_unknown_dna_is_flies() {
        assert_eq!(
            infusion_entry(Some("/mob/living/basic/carp/mega")).key,
            "carp"
        );
        assert_eq!(
            infusion_entry(Some("/obj/item/food/grown/apple")).key,
            "plants"
        );
        assert_eq!(
            infusion_entry(Some("/obj/item/food/meat/slab/gondola")).key,
            "gondola"
        );
        assert_eq!(infusion_entry(Some("/mob/living/basic/mouse")).key, "fly");
        assert_eq!(infusion_entry(None).key, "fly");
        // Every set an entry names exists.
        for e in &ENTRIES {
            assert!(e.set.is_none_or(|s| set_def(s).is_some()), "{}", e.key);
        }
    }

    #[test]
    fn infusion_replaces_an_organic_organ_and_counts_the_set() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn((Health::default(), organs::OrganHost)).id();
        for path in [
            "/obj/item/organ/heart",
            "/obj/item/organ/stomach",
            "/obj/item/organ/tongue",
            "/obj/item/organ/eyes",
        ] {
            let organ = organs::spawn_organ(&mut world, path).unwrap();
            world.entity_mut(organ).insert(OrganOf(mob));
        }
        let rat = entry("rat");
        let mut rng = crate::combat::CombatRng::default();
        for _ in 0..4 {
            assert!(infuse_organ(&mut world, mob, rat, None));
        }
        assert_eq!(set_count(&mut world, mob, "rat"), 4);
        // Nothing of the set left to replace.
        assert_eq!(pick_infusion_organ(&mut world, mob, rat, &mut rng), None);
        // No lungs: carp lungs have nothing to replace; external fangs have no slot.
        let carp = entry("carp");
        let picked = pick_infusion_organ(&mut world, mob, carp, &mut rng);
        assert!(picked.is_some_and(|p| p.ends_with("/carp") && !p.contains("fangs")));
        assert_ne!(picked, Some("/obj/item/organ/lungs/carp"));
    }
}
