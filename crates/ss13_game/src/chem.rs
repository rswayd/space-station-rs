//! Chemistry (server): reagent holders on containers and mobs, transfers, drinking, syringes,
//! pills, reactions over time and metabolism, after tg `code/modules/reagents/reagent_containers.dm`,
//! `reagent_containers/cups/_cup.dm`, `syringes.dm`, `applicator.dm`, `pill.dm` and
//! `/mob/living/carbon/human/can_inject`. The chemistry itself (holders, reactions, metabolism
//! numbers) is `ss13_reagents`.
//!
//! - Every `/obj/item/reagent_containers` gets a [`Reagents`] holder of its `volume` and
//!   `initial_reagent_flags`, filled with its `list_reagents` (no reactions at map load, as tg
//!   before the MC runs). Its fill overlay (`fill_icon_thresholds`, the mixed reagent color) and
//!   a syringe's icon state follow the contents.
//! - A held open cup clicked on a refillable container pours `amount_per_transfer_from_this`
//!   into it, on a drainable one fills from it, on a living mob drinks (yourself: a gulp of
//!   `gulp_size`; someone else: after 3 s). Right click on a drainable container fills from it;
//!   right click in combat mode splashes the contents onto anything (tg `try_splash`).
//! - A syringe injects its transfer amount (someone else: after 3 s; thick clothing on the
//!   selected zone stops it, tg `try_inject`), right click draws from a drawable container.
//! - A pill is swallowed (someone else: after 3 s) or dissolved in a refillable container.
//! - A click on the held container's own hand slot cycles the transfer amount through
//!   `possible_transfer_amounts` (tg `attack_self`); a closed soda can opens instead
//!   (`open_soda`), a thrown one may burst (`soda_thrown`).
//! - Drinking glasses, shot glasses and juice cartons take the glass style of their main reagent
//!   (`takes_reagent_appearance`).
//! - Holders that are reacting step every 0.25 s (tg `SSreagents`); mix messages go to everyone
//!   in hearing range of the container.
//! - Mobs have blood and stomach holders ([`MobReagents`]): every `SSmobs` fire the stomach
//!   drips into the blood and the blood metabolizes (`ss13_reagents::metabolism`).
// shortcut: no chem master or heater (dispensers: `crate::chem_dispenser`); no sprays,
// droppers, patches, hyposprays or IV drips; no lids; splashing and pouring expose nothing
// (reagents' `expose_*` procs are not ported: water does not put out fires, acid does not melt);
// no sounds.
// shortcut: anyone sees a container's contents on examine (tg: only with science goggles or a
// similar `can_see_reagents`).

use std::sync::Arc;

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmClothing, DmItem, DmName, DmReagents, IconIndex, MapLoadPlugin};
use ss13_net::chat::server::SendChat;
use ss13_reagents::metabolism::{self, Body, Damage};
use ss13_reagents::{
    CHEM_FILE, ChemData, Chemistry, GlassStyleDef, Holder, Owner, flags, is_under, mix_color,
    round_to,
};
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::{ClickModifiers, InventorySlot};
use ss13_shared::{Appearance, Overlay, TilePos};

use crate::breath::{MobsClock, Respiration};
use crate::combat::{CLICK_CD_MELEE, CombatMode, CombatRng, NextMove};
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health, HealthSystems, Stat, update_stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::server::PlayerMob;
use crate::tools::{Actor, DoAfter, do_after, tell};
use crate::zone::BodyZone;

pub const CONTAINER_ROOT: &str = "/obj/item/reagent_containers";
const CUP_ROOT: &str = "/obj/item/reagent_containers/cup";
const SYRINGE_ROOT: &str = "/obj/item/reagent_containers/syringe";
const PILL_ROOT: &str = "/obj/item/reagent_containers/applicator/pill";
/// tg `SSreagents` `wait`.
pub const REAGENTS_WAIT_SECS: f64 = 0.25;
/// tg `try_drink` on someone else, `try_syringe` and `applicator` `application_delay`: 3 s.
pub const CHEM_INTERACT_DELAY: f32 = 30.0;
/// A carbon's blood holder (`/mob/living/carbon` `create_reagents(1000, REAGENT_HOLDER_ALIVE)`)
/// and the stomach's (`reagent_vol`).
pub const MOB_REAGENT_VOLUME: f32 = 1000.0;
/// `THICKMATERIAL` (`clothing_flags`).
pub const THICKMATERIAL: u32 = 1 << 4;

/// The loaded chemistry (`reagents.ron`).
#[derive(Resource, Clone)]
pub struct Chem(pub Arc<Chemistry>);

/// tg `reagents` of an item.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Reagents(pub Holder);

/// tg `amount_per_transfer_from_this` (live: `attack_self` changes it).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct TransferAmount(pub f32);

/// A carbon's reagents: the blood (`reagents`), the stomach's and the drunkenness
/// (`/datum/status_effect/inebriated`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct MobReagents {
    pub blood: Holder,
    pub stomach: Holder,
    pub drunk: f32,
}

/// What a removed stomach holds (tg `stomach.reagents` goes with the organ).
#[derive(Component, Clone, Debug)]
pub struct StomachContents(pub Holder);

/// The stomach left `mob`: its contents go with it.
pub(crate) fn stomach_removed(world: &mut World, organ: Entity, mob: Entity) {
    let Some(mut m) = world.get_mut::<MobReagents>(mob) else {
        return;
    };
    let empty = Holder {
        reagents: Vec::new(),
        total_volume: 0.0,
        reactions: Vec::new(),
        is_reacting: false,
        messages: Vec::new(),
        ..m.stomach.clone()
    };
    let held = std::mem::replace(&mut m.stomach, empty);
    world.entity_mut(organ).insert(StomachContents(held));
}

/// A stomach went into `mob`: what it holds is the mob's stomach now.
pub(crate) fn stomach_inserted(world: &mut World, organ: Entity, mob: Entity) {
    let Some(StomachContents(held)) = world.entity_mut(organ).take::<StomachContents>() else {
        return;
    };
    if let Some(mut m) = world.get_mut::<MobReagents>(mob) {
        let owner = m.stomach.owner.clone();
        m.stomach = Holder { owner, ..held };
    }
}

/// On a container whose holder is reacting (stepped by [`process_reactions`]).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Reacting;

/// The overlays the item had before its fill overlay (tg `update_overlays` adds to them).
#[derive(Component, Clone, Debug, Default)]
struct BaseOverlays(Vec<Overlay>);

/// A timed chemistry action ([`crate::construction::Job::Chem`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChemJob {
    /// A cup fed to someone else.
    Feed,
    /// A syringe injected into someone else.
    Inject,
    /// A pill forced down someone else.
    ForcePill,
}

#[derive(Resource, Default)]
struct ReagentsClock(f64);

pub struct ChemServerPlugin;

impl Plugin for ChemServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ReagentsClock>()
            .add_observer(create_container_reagents)
            .add_observer(create_mob_reagents)
            .add_systems(
                Update,
                (
                    metabolize_mobs.before(HealthSystems),
                    process_reactions,
                    refresh_containers,
                )
                    .chain(),
            );
    }

    /// After every plugin's `build`: the map plugin's assets dir is known by now.
    fn finish(&self, app: &mut App) {
        if app.world().contains_resource::<Chem>() {
            return; // inserted by a test
        }
        let path = app
            .world()
            .get_resource::<MapLoadPlugin>()
            .map(|m| m.assets.join(CHEM_FILE));
        let data = match path.as_deref().map(ChemData::read) {
            Some(Ok(data)) => data,
            Some(Err(e)) => {
                warn!("{e:#} (rerun `ss13-mapload convert`); containers start empty");
                ChemData::default()
            }
            None => ChemData::default(),
        };
        let chem = Chemistry::new(data);
        info!(
            "chemistry: {} reagents, {} reactions",
            chem.reagents.len(),
            chem.reactions.len()
        );
        app.insert_resource(Chem(Arc::new(chem)));
    }
}

/// Clicks with these items on mobs are the item's (they never attack).
pub fn interacts_with_mobs(path: &str) -> bool {
    is_under(path, CONTAINER_ROOT)
}

/// tg `/obj/item/reagent_containers/Initialize`: `create_reagents(volume, initial_reagent_flags)`
/// and `add_initial_reagents`.
fn create_container_reagents(
    add: On<Add<DmReagents>>,
    containers: Query<(&DmReagents, Option<&DmItem>)>,
    chem: Option<Res<Chem>>,
    mut rng: ResMut<CombatRng>,
    mut commands: Commands,
) {
    let Ok((dm, item)) = containers.get(add.entity) else {
        return;
    };
    let owner = Owner {
        path: item.map(|i| i.path.clone()).unwrap_or_default(),
        living: false,
    };
    let mut holder = Holder::new(dm.volume, dm.initial_reagent_flags, owner);
    if let Some(chem) = chem {
        let list: Vec<(u16, f32)> = dm
            .list_reagents
            .iter()
            .filter_map(|r| chem.0.id(&r.path).map(|id| (id, r.amount)))
            .collect();
        let purity = (dm.list_reagents_purity != 0.0).then_some(dm.list_reagents_purity);
        holder.add_reagent_list(&chem.0, &list, purity, true);
        // A blood pack is full of its type (`/obj/item/reagent_containers/blood/Initialize`).
        let blood_type =
            crate::medical::blood::pack_blood_type(&holder.owner.path, &dm.blood_type, &mut rng);
        if let (Some(blood_type), Some(id)) =
            (blood_type, chem.0.id(crate::medical::blood::BLOOD_REAGENT))
        {
            holder.add_reagent(
                &chem.0,
                id,
                dm.volume,
                ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
                None,
                None,
                true,
                false,
            );
            holder.set_data_if_none(id, blood_type);
        }
    }
    commands.entity(add.entity).insert((
        Reagents(holder),
        TransferAmount(dm.amount_per_transfer_from_this),
    ));
}

fn mob_holder() -> Holder {
    Holder::new(
        MOB_REAGENT_VOLUME,
        flags::REAGENT_HOLDER_ALIVE,
        Owner {
            path: "/mob/living/carbon/human".into(),
            living: true,
        },
    )
}

/// Every mob with [`Health`] is a carbon with a stomach and a liver.
fn create_mob_reagents(
    add: On<Add<Health>>,
    silicons: Query<(), With<crate::silicon::Silicon>>,
    basics: Query<(), With<crate::mobs::BasicMob>>,
    mut commands: Commands,
) {
    // Silicons have no blood or stomach to metabolize in.
    // shortcut: basic mobs have no reagents either (tg gives them a holder); add with feeding.
    if silicons.contains(add.entity) || basics.contains(add.entity) {
        return;
    }
    commands.entity(add.entity).insert(MobReagents {
        blood: mob_holder(),
        stomach: Holder {
            owner: Owner {
                path: "/obj/item/organ/stomach".into(),
                living: false,
            },
            ..mob_holder()
        },
        drunk: 0.0,
    });
}

pub(crate) fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<Name>(e)
        .map(|n| n.as_str().to_owned())
        .or_else(|| world.get::<DmName>(e).map(|n| n.name.clone()))
        .unwrap_or_else(|| "что-то".to_owned())
}

/// DM number text (6 significant digits): `15`, `4.8`, `343.858`.
pub fn dm_num(n: f32) -> String {
    if n.fract() == 0.0 && n.abs() < 1e6 {
        return format!("{}", n as i64);
    }
    let int_digits = (n.abs().log10().floor() as i32 + 1).max(1);
    let decimals = (6 - int_digits).max(0) as usize;
    let text = format!("{n:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// Where `e` is on the map: its tile, or the tile of whoever carries it.
pub(crate) fn map_pos(world: &World, e: Entity) -> Option<TilePos> {
    world.get::<TilePos>(e).copied().or_else(|| {
        let carrier = world.get::<CarriedBy>(e)?.0;
        world.get::<TilePos>(carrier).copied()
    })
}

pub(crate) fn clients_near(world: &World, pos: TilePos, except: Option<Entity>) -> Vec<Entity> {
    crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, except.as_slice())
}

pub(crate) fn line(span: Span, text: &str) -> ChatMessage {
    ChatMessage {
        channel: if matches!(span, Span::Warning | Span::Danger | Span::BoldWarning) {
            ChatChannel::Warning
        } else {
            ChatChannel::Info
        },
        segments: vec![ChatSegment::new(span, text)],
    }
}

/// tg `visible_message(others, own)`: `own` to the actor, `others` to everyone else in range.
// shortcut: range only, no line of sight, as `crate::health::clients_in_view`.
pub(crate) fn visible(world: &mut World, actor: Entity, own: &str, others: &str, span: Span) {
    let Some(pos) = map_pos(world, actor) else {
        return;
    };
    let clients = clients_near(world, pos, Some(actor));
    if !clients.is_empty() {
        world.write_message(SendChat {
            clients,
            message: line(span, others),
        });
    }
    if let Some(client) = world.get::<PlayerMob>(actor).map(|p| p.client) {
        world.write_message(SendChat {
            clients: vec![client],
            message: line(span, own),
        });
    }
}

/// One line to a mob's own client, if it has one.
pub(crate) fn tell_mob(world: &mut World, mob: Entity, span: Span, text: &str) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(SendChat {
            clients: vec![client],
            message: line(span, text),
        });
    }
}

/// Mix messages are DM text: tags (`<span class='danger'>`) are dropped.
fn strip_tags(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.replace("&#39;", "'").replace("\\'", "'")
}

fn chem(world: &World) -> Arc<Chemistry> {
    world.resource::<Chem>().0.clone()
}

/// The two holders of a transfer, out of the world; [`put_holders`] puts them back.
fn take(world: &mut World, e: Entity) -> Option<Holder> {
    if let Some(mut r) = world.get_mut::<Reagents>(e) {
        return Some(std::mem::replace(
            &mut r.0,
            Holder::new(0.0, 0, Owner::default()),
        ));
    }
    None
}

fn put(world: &mut World, e: Entity, holder: Holder) {
    if let Some(mut r) = world.get_mut::<Reagents>(e) {
        r.0 = holder;
    }
}

/// tg `trans_to` between two item holders. Returns the amount moved.
fn transfer(world: &mut World, from: Entity, to: Entity, amount: f32) -> f32 {
    let chem = chem(world);
    let (Some(mut a), Some(mut b)) = (take(world, from), take(world, to)) else {
        return 0.0;
    };
    let moved = a.trans_to(&chem, &mut b, amount, 1.0, None, false, false);
    put(world, from, a);
    put(world, to, b);
    moved
}

/// tg `trans_to(mob, methods = INGEST)` (into the stomach) or `INJECT` (into the blood).
fn transfer_to_mob(world: &mut World, from: Entity, mob: Entity, amount: f32, ingest: bool) -> f32 {
    let chem = chem(world);
    let Some(mut a) = take(world, from) else {
        return 0.0;
    };
    let mut moved = 0.0;
    // Injected blood becomes blood volume (`crate::medical::blood::expose_inject`).
    let blood = chem.id(crate::medical::blood::BLOOD_REAGENT);
    let blood_before = blood.map_or(0.0, |id| a.amount(id));
    let blood_data = blood.and_then(|id| a.has(id)).and_then(|r| r.data.clone());
    // tg `expel_ingested`: without a stomach what is swallowed comes back out.
    // shortcut: it is lost (no vomit decal with the reagents).
    let expelled =
        ingest && crate::medical::organs::lacks(world, mob, crate::medical::organs::Slot::Stomach);
    if expelled {
        let mut gone = Holder::new(amount, 0, Owner::default());
        moved = a.trans_to(&chem, &mut gone, amount, 1.0, None, false, false);
    } else if let Some(mut m) = world.get_mut::<MobReagents>(mob) {
        let target = if ingest { &mut m.stomach } else { &mut m.blood };
        moved = a.trans_to(&chem, target, amount, 1.0, None, false, false);
    }
    let injected = blood_before - blood.map_or(0.0, |id| a.amount(id));
    put(world, from, a);
    if !ingest {
        crate::medical::blood::expose_inject(world, mob, injected, blood_data);
    }
    moved
}

/// tg `trans_to(mob, methods = INJECT)` from a container (IV drips).
pub(crate) fn inject_into_mob(world: &mut World, from: Entity, mob: Entity, amount: f32) -> f32 {
    transfer_to_mob(world, from, mob, amount, false)
}

fn holder(world: &World, e: Entity) -> Option<&Holder> {
    world.get::<Reagents>(e).map(|r| &r.0)
}

fn transfer_amount(world: &World, e: Entity) -> f32 {
    world.get::<TransferAmount>(e).map_or(5.0, |t| t.0)
}

fn path(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

/// tg `changeNext_move(CLICK_CD_MELEE)`; false while the mob's clicks are on cooldown.
pub(crate) fn melee_cooldown(world: &mut World, mob: Entity) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(mut next) = world.get_mut::<NextMove>(mob) else {
        return true;
    };
    if now < next.0 {
        return false;
    }
    next.0 = now + CLICK_CD_MELEE;
    true
}

/// tg `item_interaction` -> `interact_with_atom(_secondary)` of a held reagent container on
/// `target`. True when the click was the container's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    if world.get::<Reagents>(held).is_none() || !world.contains_resource::<Chem>() {
        return false;
    }
    let item_path = path(world, held);
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let living = world.get::<MobReagents>(target).is_some();
    if is_under(&item_path, SYRINGE_ROOT) {
        if modifiers.right {
            syringe_draw(world, actor, target);
        } else {
            syringe_inject(world, actor, target);
        }
        return true;
    }
    // `/obj/item/reagent_containers/interact_with_atom_secondary`: combat mode splashes.
    let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
    if modifiers.right && combat {
        return try_splash(world, actor, target);
    }
    if is_under(&item_path, PILL_ROOT) {
        if living {
            apply_pill(world, actor, target);
            return true;
        }
        return dissolve_pill(world, actor, target);
    }
    if !is_under(&item_path, CUP_ROOT) {
        return living;
    }
    let held_holder = holder(world, held).expect("checked above");
    if !held_holder.is_open_container() {
        return living;
    }
    let target_holder = holder(world, target);
    if modifiers.right {
        if target_holder.is_some_and(Holder::is_drainable) {
            try_drain(world, actor, target);
            return true;
        }
        return living;
    }
    if target_holder.is_some_and(Holder::is_refillable) {
        try_refill(world, actor, target);
        return true;
    }
    if target_holder.is_some_and(Holder::is_drainable) {
        try_drain(world, actor, target);
        return true;
    }
    if living {
        try_drink(world, actor, target);
        return true;
    }
    false
}

/// tg `try_refill`: from the held container into `target`.
fn try_refill(world: &mut World, actor: Actor, target: Entity) {
    let held = actor.item.expect("a held container");
    let (src, dst) = (name_of(world, held), name_of(world, target));
    if holder(world, held).is_none_or(|h| h.total_volume == 0.0) {
        tell(world, actor.client, Span::Warning, format!("{src} пуст!"));
        return;
    }
    if holder(world, target).is_some_and(Holder::holder_full) {
        tell(world, actor.client, Span::Warning, format!("{dst} полон."));
        return;
    }
    let amount = transfer_amount(world, held);
    let moved = round_to(transfer(world, held, target, amount), 0.01);
    if moved > 0.0 {
        tell(
            world,
            actor.client,
            Span::Notice,
            format!("Вы переливаете {} ед. раствора в {dst}.", dm_num(moved)),
        );
    }
}

/// tg `try_drain`: from `target` into the held container.
fn try_drain(world: &mut World, actor: Actor, target: Entity) {
    let held = actor.item.expect("a held container");
    let (src, dst) = (name_of(world, held), name_of(world, target));
    if holder(world, target).is_none_or(|h| h.total_volume == 0.0) {
        tell(world, actor.client, Span::Warning, format!("{dst} пуст!"));
        return;
    }
    if holder(world, held).is_some_and(Holder::holder_full) {
        tell(world, actor.client, Span::Warning, format!("{src} полон."));
        return;
    }
    let amount = transfer_amount(world, held);
    let moved = round_to(transfer(world, target, held, amount), 0.01);
    tell(
        world,
        actor.client,
        Span::Notice,
        format!(
            "Вы наполняете {src} {} ед. содержимого {dst}.",
            dm_num(moved)
        ),
    );
}

/// tg `canconsume`.
// shortcut: anything in the mask slot covers the mouth (tg: `MASKCOVERSMOUTH`/`HEADCOVERSMOUTH`),
// as `crate::medical`'s CPR.
fn can_consume(world: &mut World, actor: Actor, eater: Entity) -> bool {
    let held = actor.item.expect("a held container");
    if world.get::<MobReagents>(eater).is_none() {
        return false;
    }
    if holder(world, held).is_none_or(|h| h.total_volume == 0.0) {
        let src = name_of(world, held);
        tell(world, actor.client, Span::Warning, format!("{src} пуст!"));
        return false;
    }
    let masked = world
        .get::<Inventory>(eater)
        .and_then(|i| i.slot(InventorySlot::Mask))
        .is_some();
    if masked {
        let who = if eater == actor.mob {
            "свою"
        } else {
            "его"
        };
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("Сначала снимите {who} маску!"),
        );
        return false;
    }
    true
}

/// tg `try_drink` up to its `do_after`.
fn try_drink(world: &mut World, actor: Actor, target: Entity) {
    if !can_consume(world, actor, target) || !melee_cooldown(world, actor.mob) {
        return;
    }
    if target != actor.mob {
        if world.get::<DoAfter>(actor.mob).is_some() {
            return;
        }
        let held = actor.item.expect("a held container");
        let (user, victim, src) = (
            name_of(world, actor.mob),
            name_of(world, target),
            name_of(world, held),
        );
        visible(
            world,
            actor.mob,
            &format!("Вы пытаетесь напоить {victim} чем-то из {src}."),
            &format!("{user} пытается напоить {victim} чем-то из {src}."),
            Span::Danger,
        );
        do_after(
            world,
            actor,
            target,
            CHEM_INTERACT_DELAY,
            0,
            crate::construction::Job::Chem(ChemJob::Feed),
        );
        return;
    }
    tell(
        world,
        actor.client,
        Span::Notice,
        format!(
            "Вы делаете глоток из {}.",
            name_of(world, actor.item.expect("held"))
        ),
    );
    gulp(world, actor, target);
}

/// The end of tg `try_drink`: `gulp_size` units to the drinker's stomach.
fn gulp(world: &mut World, actor: Actor, target: Entity) {
    let held = actor.item.expect("a held container");
    let gulp = world.get::<DmReagents>(held).map_or(5.0, |d| d.gulp_size);
    transfer_to_mob(world, held, target, gulp, true);
}

/// tg `try_inject` of a human: the selected zone's bodypart must not be covered by clothing with
/// `THICKMATERIAL`. Feedback to the user when it fails.
pub(crate) fn try_inject(world: &mut World, actor: Actor, target: Entity) -> bool {
    let zone = world
        .get::<crate::health::ZoneSelected>(actor.mob)
        .copied()
        .unwrap_or_default()
        .0
        .check_zone();
    let bit = body_part_bit(zone);
    let thick = world.get::<Inventory>(target).is_some_and(|inv| {
        inv.equipped().any(|item| {
            world.get::<DmClothing>(item).is_some_and(|c| {
                c.body_parts_covered & bit != 0 && c.clothing_flags & THICKMATERIAL != 0
            })
        })
    });
    if thick {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("нет оголённой плоти на {}!", zone.ru().0),
        );
    }
    !thick
}

/// The bodypart's `body_part` bit (`CHEST`, `HEAD`, `ARM_LEFT`, ...).
pub(crate) fn body_part_bit(zone: BodyZone) -> u32 {
    match zone {
        BodyZone::Chest => 1 << 0,
        BodyZone::Head => 1 << 2,
        BodyZone::LLeg => 1 << 3,
        BodyZone::RLeg => 1 << 4,
        BodyZone::LArm => 1 << 7,
        BodyZone::RArm => 1 << 8,
    }
}

/// tg `/obj/item/reagent_containers/syringe/interact_with_atom` up to its `do_after`.
fn syringe_inject(world: &mut World, actor: Actor, target: Entity) {
    let held = actor.item.expect("a held syringe");
    let living = world.get::<MobReagents>(target).is_some();
    if !living && holder(world, target).is_none() {
        return;
    }
    if living && !try_inject(world, actor, target) {
        return;
    }
    let (src, dst) = (name_of(world, held), name_of(world, target));
    if holder(world, held).is_none_or(|h| h.total_volume == 0.0) {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("{src} пуст! ПКМ, чтобы набрать."),
        );
        return;
    }
    if !living && !holder(world, target).is_some_and(Holder::is_injectable) {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("Нельзя напрямую наполнить {dst}!"),
        );
        return;
    }
    if target_full(world, target) {
        tell(world, actor.client, Span::Notice, format!("{dst} полон."));
        return;
    }
    if living && target != actor.mob {
        if world.get::<DoAfter>(actor.mob).is_some() {
            return;
        }
        let user = name_of(world, actor.mob);
        visible(
            world,
            actor.mob,
            &format!("Вы пытаетесь сделать укол {dst}!"),
            &format!("{user} пытается сделать укол {dst}!"),
            Span::Danger,
        );
        do_after(
            world,
            actor,
            target,
            CHEM_INTERACT_DELAY,
            0,
            crate::construction::Job::Chem(ChemJob::Inject),
        );
        return;
    }
    inject(world, actor, target);
}

fn target_full(world: &World, target: Entity) -> bool {
    match world.get::<MobReagents>(target) {
        Some(m) => m.blood.holder_full(),
        None => holder(world, target).is_some_and(Holder::holder_full),
    }
}

/// The end of tg's syringe `interact_with_atom`: `trans_to(target, amount, methods = INJECT)`.
fn inject(world: &mut World, actor: Actor, target: Entity) {
    let held = actor.item.expect("a held syringe");
    let amount = transfer_amount(world, held);
    let moved = if world.get::<MobReagents>(target).is_some() {
        transfer_to_mob(world, held, target, amount, false)
    } else {
        transfer(world, held, target, amount)
    };
    if moved > 0.0 {
        let left = holder(world, held).map_or(0.0, |h| h.total_volume);
        tell(
            world,
            actor.client,
            Span::Notice,
            format!(
                "Вы вводите {} ед. раствора. В шприце осталось {} ед.",
                dm_num(amount),
                dm_num(left)
            ),
        );
    }
}

/// tg `/obj/item/reagent_containers/syringe/interact_with_atom_secondary`.
fn syringe_draw(world: &mut World, actor: Actor, target: Entity) {
    let held = actor.item.expect("a held syringe");
    let living = world.get::<MobReagents>(target).is_some();
    if !living && holder(world, target).is_none() {
        return;
    }
    if living && !try_inject(world, actor, target) {
        return;
    }
    let (src, dst) = (name_of(world, held), name_of(world, target));
    if holder(world, held).is_some_and(Holder::holder_full) {
        tell(world, actor.client, Span::Notice, format!("{src} полон."));
        return;
    }
    if living {
        crate::medical::blood::syringe_draw_blood(world, actor, held, target);
        return;
    }
    let Some(target_holder) = holder(world, target) else {
        return;
    };
    if target_holder.total_volume == 0.0 {
        tell(world, actor.client, Span::Warning, format!("{dst} пуст!"));
        return;
    }
    if !target_holder.is_drawable() {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("Нельзя напрямую набрать из {dst}!"),
        );
        return;
    }
    let amount = transfer_amount(world, held);
    let moved = transfer(world, target, held, amount);
    if moved > 0.0 {
        let now = holder(world, held).map_or(0.0, |h| h.total_volume);
        tell(
            world,
            actor.client,
            Span::Notice,
            format!(
                "Вы наполняете {src} {} ед. раствора. Теперь в нём {} ед.",
                dm_num(moved),
                dm_num(now)
            ),
        );
    }
}

/// tg `/obj/item/reagent_containers/applicator/interact_with_atom` on a mob.
fn apply_pill(world: &mut World, actor: Actor, target: Entity) {
    let held = actor.item.expect("a held pill");
    if !can_consume(world, actor, target) || !melee_cooldown(world, actor.mob) {
        return;
    }
    let (user, victim, pill) = (
        name_of(world, actor.mob),
        name_of(world, target),
        name_of(world, held),
    );
    if target == actor.mob {
        visible(
            world,
            actor.mob,
            &format!("Вы глотаете {pill}."),
            &format!("{user} пытается проглотить {pill}."),
            Span::Notice,
        );
        swallow(world, held, target);
        return;
    }
    if world.get::<DoAfter>(actor.mob).is_some() {
        return;
    }
    visible(
        world,
        actor.mob,
        &format!("Вы пытаетесь заставить {victim} проглотить {pill}."),
        &format!("{user} пытается заставить {victim} проглотить {pill}."),
        Span::Danger,
    );
    do_after(
        world,
        actor,
        target,
        CHEM_INTERACT_DELAY,
        0,
        crate::construction::Job::Chem(ChemJob::ForcePill),
    );
}

/// tg `/obj/item/reagent_containers/applicator/pill/on_consumption`.
// shortcut: the sugar shell (`layers_remaining`, digested in the stomach over seconds) is not
// modelled: the contents go into the stomach at once.
fn swallow(world: &mut World, pill: Entity, eater: Entity) {
    let total = holder(world, pill).map_or(0.0, |h| h.total_volume);
    if total > 0.0 {
        transfer_to_mob(world, pill, eater, total, true);
    }
    world.despawn(pill);
}

/// tg `/obj/item/reagent_containers/applicator/pill/interact_with_atom` on a container.
fn dissolve_pill(world: &mut World, actor: Actor, target: Entity) -> bool {
    let held = actor.item.expect("a held pill");
    let Some(target_holder) = holder(world, target) else {
        return false;
    };
    if !target_holder.is_refillable() {
        return false;
    }
    let dst = name_of(world, target);
    if target_holder.is_drainable() && target_holder.total_volume == 0.0 {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("{dst} пуст! Не в чем растворить {}.", name_of(world, held)),
        );
        return true;
    }
    if target_holder.holder_full() {
        tell(world, actor.client, Span::Warning, format!("{dst} полон."));
        return true;
    }
    let (user, pill) = (name_of(world, actor.mob), name_of(world, held));
    visible(
        world,
        actor.mob,
        &format!("Вы растворяете {pill} в {dst}."),
        &format!("{user} что-то подбрасывает в {dst}!"),
        Span::Warning,
    );
    let total = holder(world, held).map_or(0.0, |h| h.total_volume);
    transfer(world, held, target, total);
    world.despawn(held);
    true
}

/// tg `try_splash`: the whole contents onto `target`.
fn try_splash(world: &mut World, actor: Actor, target: Entity) -> bool {
    let held = actor.item.expect("a held container");
    let Some(h) = holder(world, held) else {
        return false;
    };
    if !h.is_open_container() || h.flags & flags::NO_SPLASH != 0 || h.total_volume == 0.0 {
        return false;
    }
    if !melee_cooldown(world, actor.mob) {
        return true;
    }
    let mob_target = world.get::<Health>(target).is_some();
    let punctuation = if mob_target { "!" } else { "." };
    let (user, src, dst) = (
        name_of(world, actor.mob),
        name_of(world, held),
        name_of(world, target),
    );
    visible(
        world,
        actor.mob,
        &format!("Вы выплёскиваете содержимое {src} на {dst}{punctuation}"),
        &format!("{user} выплёскивает содержимое {src} на {dst}{punctuation}"),
        Span::Danger,
    );
    if mob_target && target != actor.mob {
        tell_mob(
            world,
            target,
            Span::BoldWarning,
            &format!("{user} выплёскивает на вас содержимое {src}!"),
        );
    }
    // `reagents.expose(target, TOUCH)`: see the module's shortcut.
    if let Some(mut r) = world.get_mut::<Reagents>(held) {
        r.0.clear();
    }
    true
}

/// tg `attack_self` of a held container: the next of `possible_transfer_amounts`. False for
/// other items.
pub fn attack_self(world: &mut World, client: Entity, item: Entity) -> bool {
    if world.get::<DmReagents>(item).is_none() {
        return false;
    }
    // `soda_cans/attack_self`: a closed can opens.
    if is_under(&path(world, item), SODA_CANS)
        && holder(world, item).is_some_and(|h| !h.is_drainable())
    {
        open_soda(world, client, item);
        return true;
    }
    let Some(dm) = world.get::<DmReagents>(item) else {
        return false;
    };
    if holder(world, item).is_some_and(|h| h.flags & flags::SEALED_CONTAINER != 0) {
        return true;
    }
    if !dm.has_variable_transfer_amount || dm.possible_transfer_amounts.is_empty() {
        return true;
    }
    let amounts = dm.possible_transfer_amounts.clone();
    let current = transfer_amount(world, item);
    let index = amounts.iter().position(|&a| a == current).unwrap_or(0);
    let next = amounts[(index + 1) % amounts.len()];
    if let Some(mut t) = world.get_mut::<TransferAmount>(item) {
        t.0 = next;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Переливание по {} ед.", dm_num(next)),
    );
    true
}

/// The rest of a [`ChemJob`] after its `do_after`, with tg's checks at that moment.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: ChemJob) {
    let Some(held) = action.item else {
        return;
    };
    let target = action.target;
    let actor = Actor {
        client: action.client,
        mob,
        item: Some(held),
    };
    if world.get::<MobReagents>(target).is_none() {
        return;
    }
    let empty = holder(world, held).is_none_or(|h| h.total_volume == 0.0);
    let (user, victim, src) = (
        name_of(world, mob),
        name_of(world, target),
        name_of(world, held),
    );
    match job {
        ChemJob::Feed => {
            // "The drink might be empty after the delay, such as by spam-feeding".
            if empty {
                return;
            }
            visible(
                world,
                mob,
                &format!("Вы поите {victim} чем-то из {src}."),
                &format!("{user} поит {victim} чем-то из {src}."),
                Span::Danger,
            );
            gulp(world, actor, target);
        }
        ChemJob::Inject => {
            // tg `extra_checks = try_syringe` and the checks after the `do_after`.
            if empty || target_full(world, target) || !try_inject(world, actor, target) {
                return;
            }
            visible(
                world,
                mob,
                &format!("Вы делаете укол {victim} шприцем!"),
                &format!("{user} делает укол {victim} шприцем!"),
                Span::Danger,
            );
            inject(world, actor, target);
        }
        ChemJob::ForcePill => {
            visible(
                world,
                mob,
                &format!("Вы заставляете {victim} проглотить {src}."),
                &format!("{user} заставляет {victim} проглотить {src}."),
                Span::Danger,
            );
            swallow(world, held, target);
        }
    }
}

/// tg `/atom/examine` for reagents (`TRANSPARENT`, `AMOUNT_VISIBLE`) and the transfer amount
/// hint of `/obj/item/reagent_containers/examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let (Some(h), Some(chem)) = (holder(world, target), world.get_resource::<Chem>()) else {
        return;
    };
    if let Some(dm) = world.get::<DmReagents>(target)
        && dm.has_variable_transfer_amount
    {
        if dm.possible_transfer_amounts.len() > 1 {
            lines.push((
                Span::Notice,
                format!(
                    "ЛКМ по предмету в руке меняет объём переливания. Сейчас: {} ед.",
                    dm_num(transfer_amount(world, target))
                ),
            ));
        } else if !dm.possible_transfer_amounts.is_empty() {
            lines.push((
                Span::Notice,
                "ЛКМ по предмету в руке показывает объём переливания.".to_owned(),
            ));
        }
    }
    if h.flags & flags::TRANSPARENT != 0 {
        if h.total_volume > 0.0 {
            lines.push((
                Span::Plain,
                format!(
                    "Имеется {} юнитов различных химикатов:",
                    dm_num(h.total_volume)
                ),
            ));
            for r in &h.reagents {
                lines.push((
                    Span::Plain,
                    format!(
                        "• {} юнитов {}",
                        dm_num(round_to(r.volume, 0.01)),
                        chem.0.def(r.id).name
                    ),
                ));
            }
            if h.is_reacting {
                lines.push((Span::Warning, "Оно сейчас вступает в реакцию!".to_owned()));
            }
            lines.push((
                Span::Notice,
                format!(
                    "pH раствора равен {} и имеет температуру в {}K.",
                    dm_num(round_to(h.ph, 0.01)),
                    dm_num(h.chem_temp)
                ),
            ));
        } else {
            lines.push((Span::Plain, "Внутри: ничего.".to_owned()));
        }
    } else if h.flags & flags::AMOUNT_VISIBLE != 0 {
        if h.total_volume > 0.0 {
            lines.push((
                Span::Notice,
                format!("Имеется {} юнитов.", dm_num(h.total_volume)),
            ));
        } else {
            lines.push((Span::Danger, "Пусто.".to_owned()));
        }
    }
}

/// Reacting holders step every [`REAGENTS_WAIT_SECS`] (tg `SSreagents`, real time).
// Cost: every 0.25 s one pass over the containers that are reacting (usually none or a few).
fn process_reactions(
    time: Res<Time>,
    mut clock: ResMut<ReagentsClock>,
    chem: Option<Res<Chem>>,
    mut holders: Query<&mut Reagents, With<Reacting>>,
) {
    let Some(chem) = chem else {
        return;
    };
    clock.0 += time.delta_secs_f64();
    if clock.0 < REAGENTS_WAIT_SECS {
        return;
    }
    let seconds = clock.0 as f32;
    clock.0 = 0.0;
    for mut reagents in &mut holders {
        reagents.0.process(&chem.0, seconds);
    }
}

/// After any change of a container's holder: its mix messages are heard, it starts or stops
/// [`Reacting`], and its fill overlay (tg `update_overlays`) and a syringe's icon state
/// (`update_icon_state`) follow the contents.
fn refresh_containers(world: &mut World) {
    let mut query = world.query_filtered::<Entity, Changed<Reagents>>();
    let changed: Vec<Entity> = query.iter(world).collect();
    if changed.is_empty() {
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    for e in changed {
        let messages = {
            let mut r = world.get_mut::<Reagents>(e).expect("queried");
            std::mem::take(&mut r.bypass_change_detection().0.messages)
        };
        if !messages.is_empty()
            && let Some(pos) = map_pos(world, e)
        {
            let clients = clients_near(world, pos, None);
            for text in messages {
                if !clients.is_empty() {
                    world.write_message(SendChat {
                        clients: clients.clone(),
                        message: line(Span::Notice, &strip_tags(&text)),
                    });
                }
            }
        }
        let reacting = holder(world, e).is_some_and(|h| h.is_reacting);
        match (reacting, world.get::<Reacting>(e).is_some()) {
            (true, false) => {
                world.entity_mut(e).insert(Reacting);
            }
            (false, true) => {
                world.entity_mut(e).remove::<Reacting>();
            }
            _ => {}
        }
        refresh_look(world, &chem, e);
    }
}

fn refresh_look(world: &mut World, chem: &Chemistry, e: Entity) {
    let (Some(dm), Some(atom), Some(h)) = (
        world.get::<DmReagents>(e),
        world.get::<DmAtom>(e),
        holder(world, e),
    ) else {
        return;
    };
    let Some(icons) = world.get_resource::<IconIndex>() else {
        return;
    };
    let item_path = path(world, e);
    let syringe = is_under(&item_path, SYRINGE_ROOT);
    let color = mix_color(chem, &h.reagents);
    let total = h.total_volume;
    let (mut state, mut fill) = (None, None);
    let style = glass_style(chem, &item_path, h);
    // `drinkingglass/on_cup_change`: a styled glass drops its fill thresholds.
    let styled_sprite = style.is_some_and(|s| !s.icon.is_empty() && !s.icon_state.is_empty());
    if syringe {
        // `get_rounded_vol`; the base state is `[base_icon_state]_0` at spawn.
        let rounded = if total == 0.0 {
            0
        } else {
            (round_to(total / dm.volume * 15.0, 5.0) as i32).clamp(1, 15)
        };
        let base = atom
            .icon_state
            .rsplit_once('_')
            .map_or(atom.icon_state.as_str(), |(b, _)| b);
        state = Some((
            icons.get(&atom.icon, &format!("{base}_{rounded}")),
            format!("{base}_{rounded}"),
        ));
        if total > 0.0 {
            fill = icons.get(&dm.fill_icon, &format!("syringe{rounded}"));
        }
    } else if !styled_sprite && !dm.fill_icon_thresholds.is_empty() && total > 0.0 {
        let prefix = if dm.fill_icon_state.is_empty() {
            &atom.icon_state
        } else {
            &dm.fill_icon_state
        };
        let percent = (total / dm.volume * 100.0 + 0.5).floor();
        let thresholds = &dm.fill_icon_thresholds;
        let mut pick = thresholds[0];
        for (i, &t) in thresholds.iter().enumerate() {
            let end = thresholds.get(i + 1).copied().unwrap_or(f32::INFINITY);
            if t <= percent && percent < end {
                pick = t;
            }
        }
        fill = icons.get(&dm.fill_icon, &format!("{prefix}{}", dm_num(pick)));
    }
    let base = match world.get::<BaseOverlays>(e) {
        Some(b) => b.0.clone(),
        None => {
            let current = world
                .get::<Appearance>(e)
                .map(|a| a.overlays.clone())
                .unwrap_or_default();
            world.entity_mut(e).insert(BaseOverlays(current.clone()));
            current
        }
    };
    let mut overlays = base;
    if let Some(id) = fill {
        let mut overlay = Overlay::new(id);
        overlay.color = color;
        overlays.push(overlay);
    }
    if let Some(mut appearance) = world.get_mut::<Appearance>(e) {
        if appearance.overlays != overlays {
            appearance.overlays = overlays;
        }
        if let Some((Some(id), _)) = &state
            && appearance.icon_state != *id
        {
            appearance.icon_state = *id;
        }
    }
    let style = style.cloned();
    apply_glass_style(world, e, &item_path, style.as_ref(), total > 0.0);
    if let Some((_, name)) = state
        && let Some(mut item) = world.get_mut::<DmItem>(e)
        && item.inhand_icon_state != name
    {
        item.inhand_icon_state = name;
        if let Some(carrier) = world.get::<CarriedBy>(e).map(|c| c.0)
            && let Some(mut inventory) = world.get_mut::<Inventory>(carrier)
        {
            inventory.set_changed();
        }
    }
}

pub const SODA_CANS: &str = "/obj/item/reagent_containers/cup/soda_cans";
/// `/obj/item/trash/can`: what a burst can leaves.
pub const CRUSHED_CAN: &str = "/obj/item/trash/can";
/// `SODA_FIZZINESS_THROWN`.
const FIZZINESS_THROWN: f32 = 15.0;

/// A closed soda can's `fizziness`: the chance (percent) it bursts when opened.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Fizziness(pub f32);

/// tg `open_soda`: `prob(fizziness)` sprays the opener, else the tab pops; open either way.
fn open_soda(world: &mut World, client: Entity, can: Entity) {
    let fizz = world.get::<Fizziness>(can).map_or(0.0, |f| f.0);
    let mob = world.get::<CarriedBy>(can).map(|c| c.0);
    if world.resource_mut::<CombatRng>().chance(fizz) {
        let (user, name) = (mob.map(|m| name_of(world, m)), name_of(world, can));
        if let Some(mob) = mob {
            visible(
                world,
                mob,
                &format!(
                    "Вы открываете {name}, и вас внезапно окатывает потоком жидкости! Ааа!!"
                ),
                &format!(
                    "{} открывает {name}, и его внезапно окатывает шипучим содержимым!",
                    user.unwrap_or_default()
                ),
                Span::Danger,
            );
        }
        burst_soda(world, can);
        return;
    }
    tell(
        world,
        client,
        Span::Plain,
        "Вы отгибаете язычок банки с приятным хлопком.",
    );
    if let Some(mut r) = world.get_mut::<Reagents>(can) {
        r.0.flags |= flags::OPENCONTAINER;
    }
}

/// tg `burst_soda`: open and empty.
// shortcut: the contents are not exposed to the target (`expose_*` procs are not ported).
fn burst_soda(world: &mut World, can: Entity) {
    if let Some(mut r) = world.get_mut::<Reagents>(can) {
        r.0.flags |= flags::OPENCONTAINER;
        r.0.clear();
    }
}

/// tg `soda_cans/throw_impact` of a closed, full can that was not caught: 15 more fizziness,
/// then `prob(fizziness)` it bursts into a crushed can.
pub fn soda_thrown(world: &mut World, can: Entity) {
    if !is_under(&path(world, can), SODA_CANS)
        || holder(world, can).is_none_or(|h| h.is_open_container() || h.total_volume <= 0.0)
    {
        return;
    }
    let fizz = world.get::<Fizziness>(can).map_or(0.0, |f| f.0) + FIZZINESS_THROWN;
    world.entity_mut(can).insert(Fizziness(fizz));
    if !world.resource_mut::<CombatRng>().chance(fizz) {
        return;
    }
    burst_soda(world, can);
    let name = name_of(world, can);
    visible(
        world,
        can,
        "",
        &format!("{name} разрывается от удара, всё разливается!"),
        Span::Danger,
    );
    let Some(pos) = world.get::<TilePos>(can).copied() else {
        return;
    };
    let state = world.get::<Appearance>(can).map(|a| a.icon_state);
    if let Some(crushed) = crate::construction::spawn_obj(world, CRUSHED_CAN, pos)
        && let Some(state) = state
        && let Some(mut look) = world.get_mut::<Appearance>(crushed)
    {
        // `crushed_can.icon_state = icon_state`.
        look.icon_state = state;
    }
    world.despawn(can);
    crate::construction::tile_changed(world, pos);
}

/// Containers with tg `/datum/component/takes_reagent_appearance` and the container type their
/// styles are filed under (`base_container_type`, else their own type).
const SHOT_GLASS: &str = "/obj/item/reagent_containers/cup/glass/drinkingglass/shotglass";
const DRINKING_GLASS: &str = "/obj/item/reagent_containers/cup/glass/drinkingglass";
const FILLED_GLASS: &str = "/obj/item/reagent_containers/cup/glass/drinkingglass/filled";
const SMALL_CARTON: &str = "/obj/item/reagent_containers/cup/glass/bottle/juice/smallcarton";

fn style_container(path: &str) -> Option<&'static str> {
    if path == SHOT_GLASS {
        Some(SHOT_GLASS)
    } else if path == DRINKING_GLASS || is_under(path, FILLED_GLASS) {
        Some(DRINKING_GLASS)
    } else if is_under(path, SMALL_CARTON) {
        Some(SMALL_CARTON)
    } else {
        None
    }
}

/// `get_main_reagent_style()`: the style of the main reagent (the largest volume, the first on
/// a tie: `get_master_reagent`) for the container's type.
pub fn glass_style<'a>(chem: &'a Chemistry, path: &str, h: &Holder) -> Option<&'a GlassStyleDef> {
    let container = style_container(path)?;
    if h.total_volume <= 0.0 {
        return None;
    }
    let mut main: Option<&ss13_reagents::Reagent> = None;
    for r in &h.reagents {
        if main.is_none_or(|m| r.volume > m.volume) {
            main = Some(r);
        }
    }
    chem.glass_style(container, main?.id)
}

/// A styled container's prototype name and description, kept to reset to.
#[derive(Component, Clone, Debug, PartialEq)]
struct UnstyledName(DmName);

/// `takes_reagent_appearance/on_update_appearance`: the name, description and sprite of the
/// style, or the container's own without one; a shot glass is only ever "(filled) shot glass"
/// (`shotglass/update_name`, `update_desc`).
fn apply_glass_style(
    world: &mut World,
    e: Entity,
    path: &str,
    style: Option<&GlassStyleDef>,
    filled: bool,
) {
    if style_container(path).is_none() {
        return;
    }
    let Some(own) = world
        .get::<UnstyledName>(e)
        .map(|u| u.0.clone())
        .or_else(|| world.get::<DmName>(e).cloned())
    else {
        return;
    };
    if world.get::<UnstyledName>(e).is_none() {
        world.entity_mut(e).insert(UnstyledName(own.clone()));
    }
    let mut name = own.clone();
    if let Some(style) = style {
        if !style.name.is_empty() {
            name.name = style.name.clone();
        }
        if !style.desc.is_empty() {
            name.desc = style.desc.clone();
        }
    }
    if path == SHOT_GLASS {
        name.name = format!("{}shot glass", if filled { "filled " } else { "" });
        name.desc = if filled {
            "The challenge is not taking as many as you can, but guessing what it is before you \
             pass out."
                .into()
        } else {
            "A shot glass - the universal symbol for bad decisions.".into()
        };
    }
    if world.get::<DmName>(e) != Some(&name) {
        world.entity_mut(e).insert(name);
    }
    let sprite = style
        .filter(|s| !s.icon.is_empty() && !s.icon_state.is_empty())
        .map(|s| (s.icon.clone(), s.icon_state.clone()))
        .or_else(|| {
            world
                .get::<DmAtom>(e)
                .map(|a| (a.icon.clone(), a.icon_state.clone()))
        });
    let id = sprite.and_then(|(icon, state)| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(&icon, &state))
    });
    if let Some(id) = id
        && let Some(mut look) = world.get_mut::<Appearance>(e)
        && look.icon_state != id
    {
        look.icon_state = id;
    }
}

/// What metabolism changes on a mob, applied after the tick.
struct MobBody<'a> {
    health: &'a mut Health,
    losebreath: &'a mut f32,
    drunk: &'a mut f32,
    dead: bool,
    rng: &'a mut CombatRng,
    told: Vec<String>,
    /// The mob's [`crate::hunger::Hunger`], if it has one.
    hunger: Option<&'a mut crate::hunger::Hunger>,
}

impl Body for MobBody<'_> {
    fn health(&self) -> f32 {
        self.health.health()
    }
    fn oxy_loss(&self) -> f32 {
        self.health.oxy
    }
    fn dead(&self) -> bool {
        self.dead
    }
    fn adjust(&mut self, kind: Damage, amount: f32) {
        let kind = match kind {
            Damage::Brute => DamageType::Brute,
            Damage::Burn => DamageType::Burn,
            Damage::Tox => DamageType::Tox,
            Damage::Oxy => DamageType::Oxy,
        };
        self.health.adjust(kind, amount);
    }
    fn heal_bodypart(&mut self, brute: f32, burn: f32) {
        self.health.adjust(DamageType::Brute, -brute);
        self.health.adjust(DamageType::Burn, -burn);
    }
    fn losebreath(&mut self) -> &mut f32 {
        self.losebreath
    }
    fn drunk(&mut self) -> &mut f32 {
        self.drunk
    }
    fn tell(&mut self, text: String) {
        self.told.push(text);
    }
    fn spt_prob(&mut self, percent: f32, seconds: f32) -> bool {
        let p = 100.0 * (1.0 - (1.0 - percent / 100.0).powf(seconds));
        self.rng.chance(p)
    }
    fn adjust_nutrition(&mut self, amount: f32) {
        if let Some(hunger) = self.hunger.as_deref_mut() {
            hunger.adjust(amount);
        }
    }
    fn metabolism_efficiency(&self) -> f32 {
        self.hunger
            .as_deref()
            .map_or(1.0, |h| h.metabolism_efficiency)
    }
}

/// tg `Life()` -> `handle_organs` of every carbon each `SSmobs` fire: the stomach digests, the
/// liver metabolizes the blood, the drunkenness wears off.
// Cost: every 2 s one pass over mobs with reagents (one per player); a mob with nothing in it
// costs a few comparisons.
fn metabolize_mobs(world: &mut World, mut last_fired: Local<u64>) {
    let fired = world.resource::<MobsClock>().times_fired;
    if fired == *last_fired {
        return;
    }
    let fires = fired - *last_fired;
    *last_fired = fired;
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let seconds = metabolism::SSMOBS_DT;
    let mut query = world.query::<(Entity, &MobReagents)>();
    let mobs: Vec<Entity> = query
        .iter(world)
        .filter(|(_, m)| {
            !m.blood.reagents.is_empty() || !m.stomach.reagents.is_empty() || m.drunk > 0.0
        })
        .map(|(e, _)| e)
        .collect();
    for mob in mobs {
        for _ in 0..fires {
            life(world, &chem, mob, seconds);
        }
    }
}

fn life(world: &mut World, chem: &Chemistry, mob: Entity, seconds: f32) {
    // `mutadone/on_mob_life` while it is in the blood.
    if world.get::<Stat>(mob) != Some(&Stat::Dead) && blood_amount(world, mob, MUTADONE) > 0.0 {
        crate::genetics::mutadone(world, mob);
    }
    crate::medical::traumas::brain_reagents(world, mob, seconds);
    let entity = world.entity(mob);
    let (Some(health), Some(&stat), Some(reagents)) = (
        entity.get::<Health>(),
        entity.get::<Stat>(),
        entity.get::<MobReagents>(),
    ) else {
        return;
    };
    let mut health = health.clone();
    let mut reagents = reagents.clone();
    let respiration = entity.get::<Respiration>();
    let body_temperature = respiration.map_or(310.15, |r| r.bodytemperature as f32);
    let mut losebreath = respiration.map_or(0.0, |r| r.losebreath);
    let mut hunger = entity.get::<crate::hunger::Hunger>().cloned();
    let mut rng = CombatRng(world.get_resource_or_init::<CombatRng>().0);
    let dead = stat == Stat::Dead;
    use crate::medical::organs::{self, Slot};
    let has_stomach = !organs::lacks(world, mob, Slot::Stomach);
    let liver_organ = organs::organ(world, mob, Slot::Liver);
    let liver = match liver_organ.and_then(|o| world.get::<organs::Organ>(o)) {
        Some(l) if !l.failing => metabolism::Liver::Working {
            health: (l.spec().max_health - l.damage) / l.spec().max_health,
        },
        Some(_) => metabolism::Liver::Liverless,
        None if world.get::<organs::OrganHost>(mob).is_some() => metabolism::Liver::Liverless,
        None => metabolism::Liver::Working { health: 1.0 },
    };
    let liver_damage;
    let told = {
        let mut body = MobBody {
            health: &mut health,
            losebreath: &mut losebreath,
            drunk: &mut reagents.drunk,
            dead,
            rng: &mut rng,
            told: Vec::new(),
            hunger: hunger.as_mut(),
        };
        if !dead && has_stomach {
            metabolism::digest(chem, &mut reagents.stomach, &mut reagents.blood, seconds);
        }
        let stomach = reagents.stomach.clone();
        liver_damage = metabolism::metabolize_with(
            chem,
            &mut reagents.blood,
            &stomach,
            &mut body,
            body_temperature,
            seconds,
            liver,
        );
        metabolism::inebriation_tick(&mut body);
        body.told
    };
    world.resource_mut::<CombatRng>().0 = rng.0;
    let mut new_stat = stat;
    update_stat(&health, &mut new_stat);
    let mut e = world.entity_mut(mob);
    if e.get::<Health>() != Some(&health) {
        e.insert(health);
    }
    if new_stat != stat
        && let Some(mut s) = e.get_mut::<Stat>()
    {
        *s = new_stat;
    }
    if let Some(mut r) = e.get_mut::<Respiration>()
        && r.losebreath != losebreath
    {
        r.losebreath = losebreath;
    }
    if e.get::<MobReagents>() != Some(&reagents) {
        e.insert(reagents);
    }
    if let Some(hunger) = hunger
        && let Some(mut h) = e.get_mut::<crate::hunger::Hunger>()
    {
        h.set_if_neq(hunger);
    }
    for text in told {
        tell_mob(world, mob, Span::BoldWarning, &text);
    }
    if let Some(liver) = liver_organ {
        if liver_damage > 0.0 {
            organs::damage_organ(world, liver, liver_damage, organs::STANDARD_ORGAN_THRESHOLD);
        }
        // "the higher the damage the higher the probability".
        let damage = world.get::<organs::Organ>(liver).map_or(0.0, |l| l.damage);
        if !dead
            && damage > 10.0
            && crate::medical::wounds::spt_prob(
                &mut crate::medical::fork_rng(world),
                damage / 6.0,
                seconds,
            )
        {
            tell_mob(
                world,
                mob,
                Span::Warning,
                "You feel a dull pain in your abdomen.",
            );
        }
    }
}

/// `/datum/reagent/medicine/mutadone`.
pub const MUTADONE: &str = "/datum/reagent/medicine/mutadone";

/// The units a mob has of `path` in its blood, for tests and scans.
pub fn blood_amount(world: &World, mob: Entity, path: &str) -> f32 {
    let (Some(chem), Some(m)) = (world.get_resource::<Chem>(), world.get::<MobReagents>(mob))
    else {
        return 0.0;
    };
    chem.0.id(path).map_or(0.0, |id| m.blood.amount(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_like_dm() {
        assert_eq!(dm_num(15.0), "15");
        assert_eq!(dm_num(4.8), "4.8");
        assert_eq!(dm_num(343.8576), "343.858");
        assert_eq!(dm_num(0.25), "0.25");
        assert_eq!(dm_num(6.6699996), "6.67");
    }

    #[test]
    fn body_part_bits_follow_tg() {
        // `CHEST`, `HEAD`, `LEG_LEFT`, `LEG_RIGHT`, `ARM_LEFT`, `ARM_RIGHT`.
        let bits: Vec<u32> = BodyZone::ALL.into_iter().map(body_part_bit).collect();
        assert_eq!(
            bits.iter().fold(0, |a, b| a | b),
            1 | 4 | 8 | 16 | 128 | 256
        );
        assert!(interacts_with_mobs("/obj/item/reagent_containers/syringe"));
        assert!(!interacts_with_mobs("/obj/item/reagent_containersx"));
        assert_eq!(strip_tags("<span class='danger'>Boom!</span>"), "Boom!");
    }
}
