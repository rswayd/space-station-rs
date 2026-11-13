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
//! - A pill's sugar shell keeps it whole in the stomach for its seconds ([`PillShell`]); a
//!   patch stuck on a mob feeds the blood 0.75 u a second ([`AppliedPatch`]); hyposprays and
//!   medipens inject at once (a medipen is spent empty); droppers draw and squirt small amounts.
//! - An explosive instant reaction (`reagent_explosion`) blows up and empties its container; a
//!   lit welder heats a cup.
//! - What goes into a mob exposes it by its method (`trans_to(methods)`): drinking and pills
//!   INGEST, syringes and hyposprays INJECT, patches PATCH; droppers and splashing TOUCH. The
//!   `expose_*` effects are `crate::janitor::expose_by`.
//! - What metabolized each tick goes on to addiction points (`crate::addictions`), the drugs
//!   and stacked effects (`crate::drugs`) and the pyrotechnics (`crate::pyrotechnics`).
// shortcut: the machines are `crate::chem_dispenser`, `crate::chem_master` and
// `crate::chem_heater`; no sprays or IV drips here; no lids; pouring into a container exposes
// nothing; most reagents' `expose_*` procs are not ported; no mixing sounds.

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
/// What makes a monkey cube expand (`crate::food::expand_cube`).
const WATER_PATH: &str = "/datum/reagent/water";
const CUP_ROOT: &str = "/obj/item/reagent_containers/cup";
const SYRINGE_ROOT: &str = "/obj/item/reagent_containers/syringe";
const PILL_ROOT: &str = "/obj/item/reagent_containers/applicator/pill";
const PATCH_ROOT: &str = "/obj/item/reagent_containers/applicator/patch";
/// `/datum/embedding/med_patch/instant` patches: all at once.
const INSTANT_PATCH: &str = "/obj/item/reagent_containers/applicator/patch/synthflesh";
const HYPOSPRAY_ROOT: &str = "/obj/item/reagent_containers/hypospray";
const MEDIPEN_ROOT: &str = "/obj/item/reagent_containers/hypospray/medipen";
/// Hyposprays with `ignore_flags` (through thick clothing).
const IGNORE_CLOTHING: [&str; 3] = [
    MEDIPEN_ROOT,
    "/obj/item/reagent_containers/hypospray/combat",
    // BandaStation: modular_bandastation/balance/code/containers/reagent_containers.dm.
    "/obj/item/reagent_containers/hypospray/cmo",
];
const DROPPER_ROOT: &str = "/obj/item/reagent_containers/dropper";
/// Deciseconds: a patch's `application_delay` and `self_delay`.
const PATCH_DELAY: f32 = 15.0;
/// `/datum/embedding/med_patch::transfer_per_second`.
const PATCH_RATE: f32 = 0.75;
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
        reacted: Vec::new(),
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
    /// A patch put on someone (anyone, oneself too).
    Patch,
}

#[derive(Resource, Default)]
struct ReagentsClock(f64);

pub struct ChemServerPlugin;

impl Plugin for ChemServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::AttackSelf, 1100, |w, c| {
            crate::chem::attack_self(w, c.client, c.target)
        });
        // tg `interact_with_atom` of the active hand's container on the other hand's.
        app.add_click_handler(ClickPhase::OtherHand, 1400, |w, c| {
            c.held.is_some_and(|h| {
                crate::chem::item_interaction(
                    w,
                    c.client,
                    c.mob,
                    h,
                    c.target,
                    ss13_shared::protocol::ClickModifiers::default(),
                )
            })
        });
        // tg `interact_with_atom` of reagent containers (pour, drink, inject, splash).
        app.add_click_handler(ClickPhase::HeldOnTarget, 3700, |w, c| {
            c.held.is_some_and(|h| {
                crate::chem::item_interaction(w, c.client, c.mob, h, c.target, c.modifiers)
            })
        });
        app.init_resource::<ReagentsClock>()
            .add_observer(create_container_reagents)
            .add_observer(create_mob_reagents)
            .add_systems(
                Update,
                (
                    metabolize_mobs.before(HealthSystems),
                    digest_pills,
                    process_patches,
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
    containers: Query<(&DmReagents, Option<&DmItem>, Option<&ss13_mapload::DmObj>)>,
    chem: Option<Res<Chem>>,
    mut rng: ResMut<CombatRng>,
    mut commands: Commands,
) {
    let Ok((dm, item, obj)) = containers.get(add.entity) else {
        return;
    };
    // A tank (`reagent_dispensers`) is a structure, not an item.
    let owner = Owner {
        path: item
            .map(|i| i.path.clone())
            .or_else(|| obj.map(|o| o.path.clone()))
            .unwrap_or_default(),
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
    let pill = is_under(&holder.owner.path, PILL_ROOT);
    commands.entity(add.entity).insert((
        Reagents(holder),
        TransferAmount(dm.amount_per_transfer_from_this),
    ));
    if pill {
        commands
            .entity(add.entity)
            .insert_if_new(PillShell(crate::chem_master::PILL_LAYERS));
    }
}

pub(crate) fn mob_holder() -> Holder {
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
/// The container's turf: its own, its carrier's or its container's (tg `get_turf`).
pub(crate) fn map_pos(world: &World, e: Entity) -> Option<TilePos> {
    crate::explosions::turf_of(world, e)
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
pub(crate) fn transfer(world: &mut World, from: Entity, to: Entity, amount: f32) -> f32 {
    let chem = chem(world);
    let (Some(mut a), Some(mut b)) = (take(world, from), take(world, to)) else {
        return 0.0;
    };
    let moved = a.trans_to(&chem, &mut b, amount, 1.0, None, false, false);
    put(world, from, a);
    put(world, to, b);
    if moved > 0.0 {
        crate::medical::diseases::carry_blood(world, from, to);
    }
    moved
}

/// tg `trans_to(mob, methods = INGEST)` (into the stomach) or `INJECT` (into the blood).
pub(crate) fn transfer_to_mob(
    world: &mut World,
    from: Entity,
    mob: Entity,
    amount: f32,
    ingest: bool,
) -> f32 {
    use crate::janitor::{INGEST, INJECT};
    transfer_to_mob_by(
        world,
        from,
        mob,
        amount,
        if ingest { INGEST } else { INJECT },
    )
}

/// tg `trans_to(mob, methods)`: INGEST into the stomach, anything else (INJECT, PATCH) into the
/// blood; the mob is then exposed to what it got (`expose(mob, methods)`).
pub(crate) fn transfer_to_mob_by(
    world: &mut World,
    from: Entity,
    mob: Entity,
    amount: f32,
    methods: u32,
) -> f32 {
    let ingest = methods & crate::janitor::INGEST != 0;
    let chem = chem(world);
    let Some(mut a) = take(world, from) else {
        return 0.0;
    };
    let before: Vec<(ss13_reagents::ReagentId, f32)> =
        a.reagents.iter().map(|r| (r.id, r.volume)).collect();
    let mut moved = 0.0;
    // Injected blood becomes blood volume (`crate::medical::blood::expose_inject`).
    let blood = chem.id(crate::medical::blood::BLOOD_REAGENT);
    let blood_before = blood.map_or(0.0, |id| a.amount(id));
    let blood_data = blood.and_then(|id| a.has(id)).and_then(|r| r.data.clone());
    // A vaccine's disease ids (`crate::chem_machines::pandemic`).
    let vaccine = chem.id(crate::chem_machines::pandemic::VACCINE);
    let vaccine_before = vaccine.map_or(0.0, |id| a.amount(id));
    let vaccine_ids = vaccine
        .and_then(|id| a.has(id))
        .and_then(|r| r.data.as_deref())
        .map(crate::chem_machines::pandemic::vaccine_ids)
        .unwrap_or_default();
    let strange = chem.id(crate::medical::strange_reagent::STRANGE_REAGENT);
    let strange_before = strange.map_or(0.0, |id| a.amount(id));
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
    let vaccinated = vaccine_before - vaccine.map_or(0.0, |id| a.amount(id));
    let strange_moved = strange.map_or(0.0, |id| strange_before - a.amount(id));
    // `r_to_send`: what the transfer added, for `expose`.
    let mut exposed = Holder::new(moved.max(0.0) + 1.0, flags::NO_REACT, Owner::default());
    if !expelled {
        for &(id, volume) in &before {
            let gone = volume - a.amount(id);
            if gone > 0.0 {
                exposed.add_reagent(&chem, id, gone, a.chem_temp, None, None, false, false);
            }
        }
    }
    put(world, from, a);
    if methods != 0 && !exposed.reagents.is_empty() {
        crate::janitor::expose_methods(world, mob, &exposed, methods);
    }
    // `strange_reagent/expose_mob`: a carbon is revived by what it swallows.
    if ingest && strange_moved > 0.0 {
        crate::medical::strange_reagent::expose(world, mob, strange_moved);
    }
    if !ingest {
        crate::medical::blood::expose_inject(world, mob, injected, blood_data);
    }
    // The viruses and antibodies of the blood (`blood_reagent` `on_expose_mob`).
    if injected > 0.0 && !expelled {
        crate::medical::diseases::blood_exposed(world, from, mob, ingest);
    }
    if vaccinated > 0.0 && !expelled {
        crate::medical::diseases::vaccinate(world, mob, &vaccine_ids);
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
    if heat_with_tool(world, client, held, target) {
        return true;
    }
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
    if is_under(&item_path, HYPOSPRAY_ROOT) {
        if modifiers.right {
            return hypospray_draw(world, actor, target);
        }
        if living {
            hypospray_inject(world, actor, target);
            return true;
        }
        return false;
    }
    if is_under(&item_path, DROPPER_ROOT) {
        return dropper(world, actor, target);
    }
    if is_under(&item_path, PATCH_ROOT) {
        if living {
            apply_patch(world, actor, target);
        }
        return living;
    }
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
    // `pill/item_interaction`: a cup's sugar thickens a pill's shell, its water dissolves it.
    if world.get::<PillShell>(target).is_some() && !modifiers.right {
        return coat_pill(world, actor, target);
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

/// tg `canconsume`: headgear or a mask covering the mouth (`is_mouth_covered`) is in the way.
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
    let covered = crate::clothing::cover::mouth_covered(world, eater);
    if let Some(covered) = covered {
        let own = eater == actor.mob;
        let headgear = world.get::<Inventory>(eater).and_then(|i| i.head) == Some(covered);
        let what = match (headgear, own) {
            (true, true) => "свой головной убор",
            (true, false) => "его головной убор",
            (false, true) => "свою маску",
            (false, false) => "его маску",
        };
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("Сначала снимите {what}!"),
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
    let thick = thick_covered(world, target, zone);
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

/// tg `/mob/living/carbon/human/can_inject` without `INJECT_CHECK_PENETRATE_THICK`: worn
/// `THICKMATERIAL` clothing covers `zone`.
pub(crate) fn thick_covered(world: &World, target: Entity, zone: BodyZone) -> bool {
    let bit = body_part_bit(zone);
    world.get::<Inventory>(target).is_some_and(|inv| {
        inv.equipped().any(|item| {
            world.get::<DmClothing>(item).is_some_and(|c| {
                c.body_parts_covered & bit != 0 && c.clothing_flags & THICKMATERIAL != 0
            })
        })
    })
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

/// tg `used_up` of a medipen: emptied, its tip is broken.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsedUp;

/// tg `/datum/element/reagents_item_heatable` of cups: a hot tool (`get_temperature`: a lit
/// welder, lighter, match or cigarette) heats a non-empty cup (`expose_temperature`, 2% of the
/// difference, at least 1 K), which may start reactions.
// shortcut: no burners; no extinguisher cooling.
fn heat_with_tool(world: &mut World, client: Entity, held: Entity, target: Entity) -> bool {
    let heat = crate::medical::surgery::ToolInfo::read(world, Some(held)).temperature;
    if heat <= 0.0
        || !is_under(&path(world, target), CUP_ROOT)
        || holder(world, target).is_none_or(|h| h.total_volume <= 0.0)
    {
        return false;
    }
    let chem = chem(world);
    if let Some(mut r) = world.get_mut::<Reagents>(target) {
        r.0.expose_temperature(&chem, heat, 0.02);
    }
    let dst = name_of(world, target);
    tell(world, client, Span::Notice, format!("{dst} нагрет"));
    true
}

/// tg `/obj/item/reagent_containers/hypospray/inject`: the transfer amount into the blood at
/// once, through thick clothing for medipens and combat injectors; a medipen is spent when empty.
// shortcut: the `infinite` (admin/borg) hyposprays are not special.
fn hypospray_inject(world: &mut World, actor: Actor, target: Entity) {
    let held = actor.item.expect("a held hypospray");
    let src = name_of(world, held);
    if world.get::<UsedUp>(held).is_some() {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("Наконечник {src} сломан, им больше нельзя пользоваться!"),
        );
        return;
    }
    if world.get::<MobReagents>(target).is_none() || !melee_cooldown(world, actor.mob) {
        return;
    }
    let item_path = path(world, held);
    let ignore = IGNORE_CLOTHING.iter().any(|r| is_under(&item_path, r));
    if !ignore && !try_inject(world, actor, target) {
        return;
    }
    let victim = name_of(world, target);
    tell_mob(world, target, Span::Warning, "Вы чувствуете лёгкий укол!");
    tell(
        world,
        actor.client,
        Span::Notice,
        format!("Вы делаете инъекцию {victim} с помощью {src}."),
    );
    if let Some(pos) = map_pos(world, target) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/items/hypospray.ogg", 50.0, true),
        );
    }
    let amount = transfer_amount(world, held);
    let moved = transfer_to_mob(world, held, target, amount, false);
    let left = holder(world, held).map_or(0.0, |h| h.total_volume);
    tell(
        world,
        actor.client,
        Span::Notice,
        format!(
            "Введено {} ед. В {src} осталось {} ед.",
            dm_num(round_to(moved, 0.01)),
            dm_num(left)
        ),
    );
    if is_under(&item_path, MEDIPEN_ROOT) && left == 0.0 {
        world.entity_mut(held).insert(UsedUp);
        if let Some(mut r) = world.get_mut::<Reagents>(held) {
            r.0.flags = 0;
        }
        if let Some(state) = world
            .get::<DmAtom>(held)
            .map(|a| format!("{}0", a.icon_state))
        {
            crate::obj_damage::set_icon_state(world, held, &state);
        }
    }
}

/// tg `hypospray/interact_with_atom_secondary`: refilled from a drawable container.
fn hypospray_draw(world: &mut World, actor: Actor, target: Entity) -> bool {
    let held = actor.item.expect("a held hypospray");
    if holder(world, target).is_none() && world.get::<MobReagents>(target).is_none() {
        return false;
    }
    let (src, dst) = (name_of(world, held), name_of(world, target));
    if world.get::<MobReagents>(target).is_some() {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("{src} не может набирать кровь!"),
        );
        return true;
    }
    if holder(world, held).is_some_and(Holder::holder_full) {
        tell(world, actor.client, Span::Notice, format!("{src} полон."));
        return true;
    }
    let Some(h) = holder(world, target) else {
        return true;
    };
    if h.total_volume == 0.0 {
        tell(world, actor.client, Span::Warning, format!("{dst} пуст!"));
        return true;
    }
    if !h.is_drawable() {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("Нельзя напрямую набрать из {dst}!"),
        );
        return true;
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
                dm_num(round_to(moved, 0.01)),
                dm_num(now)
            ),
        );
    }
    true
}

/// tg `/obj/item/reagent_containers/dropper/interact_with_atom`: a full dropper squirts its
/// transfer amount into an injectable container or someone's eyes, an empty one draws from a
/// drawable container (never a mob).
// shortcut: eye protection does not catch the drops (`is_eyes_covered`); no droplet sound (not
// converted).
fn dropper(world: &mut World, actor: Actor, target: Entity) -> bool {
    let held = actor.item.expect("a held dropper");
    let living = world.get::<MobReagents>(target).is_some();
    if holder(world, target).is_none() && !living {
        return false;
    }
    let (src, dst) = (name_of(world, held), name_of(world, target));
    let amount = transfer_amount(world, held);
    if holder(world, held).is_some_and(|h| h.total_volume > 0.0) {
        if target_full(world, target) {
            tell(world, actor.client, Span::Notice, format!("{dst} полон."));
            return true;
        }
        if !living && !holder(world, target).is_some_and(Holder::is_injectable) {
            tell(
                world,
                actor.client,
                Span::Warning,
                format!("Нельзя перелить реагенты в {dst}!"),
            );
            return true;
        }
        let moved = if living {
            if !melee_cooldown(world, actor.mob) {
                return true;
            }
            let user = name_of(world, actor.mob);
            visible(
                world,
                actor.mob,
                &format!("Вы капаете что-то в глаза {dst}!"),
                &format!("{user} капает что-то в глаза {dst}!"),
                Span::Danger,
            );
            // `reagents.expose(target, TOUCH, fraction)`, then a plain `trans_to`.
            if let Some(h) = holder(world, held).cloned()
                && h.total_volume > 0.0
            {
                let fraction = (amount / h.total_volume).min(1.0);
                crate::janitor::expose(world, target, &h, fraction);
            }
            transfer_to_mob_by(world, held, target, amount, 0)
        } else {
            transfer(world, held, target, amount)
        };
        let moved = round_to(moved, 0.01);
        if moved > 0.0 {
            tell(
                world,
                actor.client,
                Span::Notice,
                format!("Вы переливаете {} ед. раствора.", dm_num(moved)),
            );
        }
        return true;
    }
    let Some(h) = holder(world, target).filter(|_| !living) else {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("Нельзя напрямую набрать из {dst}!"),
        );
        return true;
    };
    if !h.is_drawable() {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("Нельзя напрямую набрать из {dst}!"),
        );
        return true;
    }
    if h.total_volume == 0.0 {
        tell(world, actor.client, Span::Warning, format!("{dst} пуст!"));
        return true;
    }
    let moved = round_to(transfer(world, target, held, amount), 0.01);
    if moved > 0.0 {
        tell(
            world,
            actor.client,
            Span::Notice,
            format!("Вы наполняете {src} {} ед. раствора.", dm_num(moved)),
        );
    }
    true
}

/// A patch stuck on `mob` (tg `/datum/embedding/med_patch`): off the map, it gives `rate` units
/// a second to the blood (`trans_to(owner, transfer_per_second, methods = PATCH)`) and falls
/// off empty.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct AppliedPatch {
    pub mob: Entity,
    pub rate: f32,
    /// Seconds towards the next transfer (`SSprocessing`, 1 s).
    pub clock: f32,
}

/// tg `/obj/item/reagent_containers/applicator/interact_with_atom` of a patch: 1.5 s on
/// anyone, oneself included.
// shortcut: no limb checks (missing or robotic limbs), no overlay on the mob, patches cannot
// be ripped off (no embedded-object removal yet).
fn apply_patch(world: &mut World, actor: Actor, target: Entity) {
    let held = actor.item.expect("a held patch");
    if !melee_cooldown(world, actor.mob) || world.get::<DoAfter>(actor.mob).is_some() {
        return;
    }
    let (user, victim, patch) = (
        name_of(world, actor.mob),
        name_of(world, target),
        name_of(world, held),
    );
    if target == actor.mob {
        visible(
            world,
            actor.mob,
            &format!("Вы пытаетесь наклеить {patch}."),
            &format!("{user} пытается наклеить {patch}."),
            Span::Notice,
        );
    } else {
        visible(
            world,
            actor.mob,
            &format!("Вы пытаетесь наклеить {patch} на {victim}."),
            &format!("{user} пытается наклеить {patch} на {victim}."),
            Span::Danger,
        );
    }
    do_after(
        world,
        actor,
        target,
        PATCH_DELAY,
        0,
        crate::construction::Job::Chem(ChemJob::Patch),
    );
}

/// `on_consumption` of a patch: it leaves the hand and sticks.
fn stick_patch(world: &mut World, patch: Entity, mob: Entity) {
    if let Some(carrier) = world.get::<CarriedBy>(patch).map(|c| c.0)
        && let Some(mut inventory) = world.get_mut::<Inventory>(carrier)
        && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(patch))
    {
        *slot = None;
    }
    let rate = if is_under(&path(world, patch), INSTANT_PATCH) {
        world.get::<DmReagents>(patch).map_or(40.0, |d| d.volume)
    } else {
        PATCH_RATE
    };
    world
        .entity_mut(patch)
        .remove::<(CarriedBy, TilePos)>()
        .insert(AppliedPatch {
            mob,
            rate,
            clock: 0.0,
        });
}

/// Stuck patches feed the blood once a second; empty ones (or their wearer's) are gone.
// Cost: one pass over the stuck patches each tick (a few per medbay).
fn process_patches(world: &mut World) {
    let delta = world.resource::<Time>().delta_secs();
    let mut q = world.query::<(Entity, &mut AppliedPatch)>();
    let mut due = Vec::new();
    for (e, mut p) in q.iter_mut(world) {
        p.clock += delta;
        if p.clock >= 1.0 {
            p.clock -= 1.0;
            due.push((e, p.mob, p.rate));
        }
    }
    for (patch, mob, rate) in due {
        if world.get::<MobReagents>(mob).is_none()
            || holder(world, patch).is_none_or(|h| h.total_volume == 0.0)
        {
            world.despawn(patch);
            continue;
        }
        transfer_to_mob_by(world, patch, mob, rate, crate::janitor::PATCH);
    }
}

/// tg `layers_remaining` of a pill: seconds its sugar shell lasts in a stomach.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PillShell(pub f32);

/// A swallowed pill whose shell is dissolving in `eater`'s stomach (tg `stomach.consume_thing`
/// and `on_digestion`): off the map, its contents go in at `at` (`Time` seconds).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct DigestingPill {
    pub eater: Entity,
    pub at: f64,
}

/// tg `/obj/item/reagent_containers/applicator/pill/on_consumption`: a pill with a shell waits
/// in the stomach for its layers' seconds, one without (or a mob without a stomach) goes in at
/// once.
// shortcut: no taste messages ("starchy sugar") and no red pill question.
fn swallow(world: &mut World, pill: Entity, eater: Entity) {
    let layers = world.get::<PillShell>(pill).map_or(0.0, |s| s.0);
    let stomach =
        !crate::medical::organs::lacks(world, eater, crate::medical::organs::Slot::Stomach);
    if layers > 0.0 && stomach && world.get::<MobReagents>(eater).is_some() {
        if let Some(carrier) = world.get::<CarriedBy>(pill).map(|c| c.0)
            && let Some(mut inventory) = world.get_mut::<Inventory>(carrier)
            && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(pill))
        {
            *slot = None;
        }
        let at = world.resource::<Time>().elapsed_secs_f64() + f64::from(layers);
        world
            .entity_mut(pill)
            .remove::<(CarriedBy, TilePos)>()
            .insert(DigestingPill { eater, at });
        return;
    }
    digest(world, pill, eater);
}

/// The end of a pill: its contents into the eater's stomach.
fn digest(world: &mut World, pill: Entity, eater: Entity) {
    let total = holder(world, pill).map_or(0.0, |h| h.total_volume);
    if total > 0.0 && world.get::<MobReagents>(eater).is_some() {
        transfer_to_mob(world, pill, eater, total, true);
    }
    world.despawn(pill);
}

/// `PILL_MAX_LAYERS`: a minute of shell.
const PILL_MAX_LAYERS: f32 = 60.0;
const SUGAR: &str = "/datum/reagent/consumable/sugar";
const WATER: &str = "/datum/reagent/water";

/// tg `pill/item_interaction` with a held cup: sugar adds layers (whole units, up to a
/// minute), else water removes them. False when the cup has neither (it falls through).
fn coat_pill(world: &mut World, actor: Actor, pill: Entity) -> bool {
    let held = actor.item.expect("a held cup");
    let chem = chem(world);
    let (src, dst) = (name_of(world, held), name_of(world, pill));
    let Some(h) = holder(world, held) else {
        return false;
    };
    if !h.is_drainable() {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("Нельзя вылить содержимое {src} на {dst}!"),
        );
        return true;
    }
    let layers = world.get::<PillShell>(pill).map_or(0.0, |s| s.0);
    let amount = transfer_amount(world, held);
    let (sugar, water) = (chem.id(SUGAR), chem.id(WATER));
    let sugar_volume = sugar.map_or(0.0, |id| h.amount(id));
    let water_volume = water.map_or(0.0, |id| h.amount(id));
    let (id, delta, text) = if sugar_volume > 0.0 {
        if layers >= PILL_MAX_LAYERS {
            tell(
                world,
                actor.client,
                Span::Warning,
                format!("Оболочка {dst} слишком толстая, больше сахара не нанести!"),
            );
            return true;
        }
        let apply = amount
            .min(sugar_volume)
            .min(PILL_MAX_LAYERS - layers)
            .floor();
        (
            sugar,
            apply,
            format!("Вы поливаете {dst} содержимым {src}, утолщая сахарную оболочку."),
        )
    } else if water_volume > 0.0 {
        if layers <= 0.0 {
            tell(
                world,
                actor.client,
                Span::Warning,
                format!("У {dst} больше нет внешних слоёв, которые можно растворить!"),
            );
            return true;
        }
        let apply = amount.min(water_volume).min(layers).floor();
        (
            water,
            -apply,
            format!("Вы поливаете {dst} содержимым {src}, растворяя сахарную оболочку."),
        )
    } else {
        return false;
    };
    if let (Some(id), Some(mut r)) = (id, world.get_mut::<Reagents>(held)) {
        r.0.remove_reagent(id, delta.abs());
    }
    world.entity_mut(pill).insert(PillShell(layers + delta));
    tell(world, actor.client, Span::Notice, text);
    true
}

/// Swallowed pills whose shells are gone.
// Cost: one pass over the pills being digested (a few at most).
fn digest_pills(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &DigestingPill)>();
    let due: Vec<(Entity, Entity)> = q
        .iter(world)
        .filter(|(_, d)| now >= d.at)
        .map(|(e, d)| (e, d.eater))
        .collect();
    for (pill, eater) in due {
        if world.get_entity(eater).is_err() {
            world.despawn(pill);
            continue;
        }
        digest(world, pill, eater);
    }
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
    if let Some(h) = holder(world, held).cloned() {
        crate::janitor::expose(world, target, &h, 1.0);
    }
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
    // `medipen/attack_self`: into oneself.
    if is_under(&path(world, item), MEDIPEN_ROOT)
        && let Some(mob) = world.get::<CarriedBy>(item).map(|c| c.0)
    {
        let actor = Actor {
            client,
            mob,
            item: Some(item),
        };
        hypospray_inject(world, actor, mob);
        return true;
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
        ChemJob::Patch => {
            if target == mob {
                tell(
                    world,
                    action.client,
                    Span::Notice,
                    format!("Вы наклеиваете {src}."),
                );
            } else {
                visible(
                    world,
                    mob,
                    &format!("Вы наклеиваете {src} на {victim}."),
                    &format!("{user} наклеивает {src} на {victim}."),
                    Span::Danger,
                );
            }
            stick_patch(world, held, target);
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

/// tg `can_see_reagents`: the dead (ghosts too), silicons and whoever wears glasses with
/// `TRAIT_REAGENT_SCANNER` see what a container holds.
pub fn can_see_reagents(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Dead)
        || world.get::<crate::ghost::Ghost>(mob).is_some()
        || world.get::<crate::silicon::Silicon>(mob).is_some()
        || crate::modsuit::science::sees_reagents(world, mob)
        || world
            .get::<Inventory>(mob)
            .and_then(|i| i.slot(InventorySlot::Eyes))
            .and_then(|g| world.get::<DmClothing>(g))
            .is_some_and(|c| c.reagent_scanner)
}

/// tg `/atom/examine` for reagents (`TRANSPARENT`, `AMOUNT_VISIBLE`) and the transfer amount
/// hint of `/obj/item/reagent_containers/examine`.
pub fn examine_lines(
    world: &World,
    examiner: Entity,
    target: Entity,
    lines: &mut Vec<(Span, String)>,
) {
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
    if let Some(shell) = world.get::<PillShell>(target) {
        if shell.0 > 0.0 {
            lines.push((
                Span::Notice,
                format!(
                    "Его сахарная оболочка продержится в желудке примерно {} сек.",
                    dm_num(shell.0)
                ),
            ));
        } else {
            lines.push((
                Span::Warning,
                "Его оболочка полностью растворилась!".to_owned(),
            ));
        }
    }
    if h.flags & flags::TRANSPARENT != 0 {
        if h.total_volume > 0.0 {
            let sees = can_see_reagents(world, examiner);
            lines.push((
                Span::Plain,
                format!(
                    "Имеется {} юнитов различных химикатов{}",
                    dm_num(h.total_volume),
                    if sees { ":" } else { "." }
                ),
            ));
            if !sees {
                return;
            }
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

/// Reacting holders step every [`REAGENTS_WAIT_SECS`] (tg `SSreagents`, real time); one in a
/// reaction chamber is heated first (`COMSIG_REAGENTS_REACTION_STEP`).
// Cost: every 0.25 s one pass over the containers that are reacting (usually none or a few).
fn process_reactions(world: &mut World) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let delta = world.resource::<Time>().delta_secs_f64();
    let mut clock = world.resource_mut::<ReagentsClock>();
    clock.0 += delta;
    if clock.0 < REAGENTS_WAIT_SECS {
        return;
    }
    let seconds = clock.0 as f32;
    clock.0 = 0.0;
    let mut q = world.query_filtered::<Entity, With<Reacting>>();
    let reacting: Vec<Entity> = q.iter(world).collect();
    for e in reacting {
        crate::chem_heater::reaction_step(world, e, seconds);
        crate::teapot::reaction_step(world, e, seconds);
        crate::chem_machines::recipe_debug::reaction_step(world, e, seconds);
        if let Some(mut reagents) = world.get_mut::<Reagents>(e) {
            reagents.0.process(&chem, seconds);
        }
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
        refresh_container(world, &chem, e);
    }
}

/// One container after its holder changed (see [`refresh_containers`]); the chemical grenade
/// calls it at once for its mixed payload (`crate::assemblies::chem_grenade`).
pub(crate) fn refresh_container(world: &mut World, chem: &Chemistry, e: Entity) {
    {
        let Some(mut r) = world.get_mut::<Reagents>(e) else {
            return; // blown up by an earlier container's explosion
        };
        let holder_mut = &mut r.bypass_change_detection().0;
        let messages = std::mem::take(&mut holder_mut.messages);
        let reacted = std::mem::take(&mut holder_mut.reacted);
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
        if reacted
            .iter()
            .any(|&(i, _)| chem.reactions[i as usize].def.explosion.is_some())
        {
            reagent_explosions(world, chem, e, &reacted);
            if world.get_entity(e).is_err() {
                return;
            }
        }
        if reacted
            .iter()
            .any(|&(i, _)| chem.reactions[i as usize].def.path == EMP_REACTION)
        {
            reagent_emps(world, chem, e, &reacted);
        }
        // Water put into a monkey cube (a syringe, a dropper) expands it.
        if world
            .get::<Reagents>(e)
            .is_some_and(|r| r.0.reagents.iter().any(|x| chem.is_type(x.id, WATER_PATH)))
            && crate::food::expand_cube(world, e)
        {
            return;
        }
        // `/datum/chemical_reaction/food/on_reaction`: `created_volume` of the reaction's
        // food where the container is (dough, cheese, tofu, chocolate...).
        let foods: Vec<(String, u32)> = reacted
            .iter()
            .filter_map(|&(i, made)| {
                let food = chem.reactions[i as usize].def.food.clone()?;
                Some((food, made.floor() as u32))
            })
            .collect();
        if !foods.is_empty()
            && let Some(pos) = map_pos(world, e)
        {
            for (food, amount) in foods {
                for _ in 0..amount {
                    crate::construction::spawn_obj(world, &food, pos);
                }
            }
            if world.get_entity(e).is_err() {
                return;
            }
        }
        // Slime extracts: their uses and `on_reaction` effects (`crate::science::xenobio`).
        if !reacted.is_empty()
            && world
                .get::<crate::science::xenobio::SlimeExtract>(e)
                .is_some()
        {
            crate::science::xenobio::extract_reacted(world, chem, e, &reacted);
        }
        // Smoke and foam reactions (`crate::effects::reactions`).
        if crate::effects::reactions::any(chem, &reacted) {
            crate::effects::reactions::on_reactions(world, chem, e, &reacted);
            if world.get_entity(e).is_err() {
                return;
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
    }
    refresh_look(world, chem, e);
}

/// tg `reagent_explosion/on_reaction` -> `reagent_explode` of each explosive instant reaction a
/// container ran: power `modifier + round(volume / strengthdiv, 1)`, "The solution violently
/// explodes!" and `dyn_explosion(power)` with the default flame and flash ranges
/// (`reagents_explosion`); the container's reagents are gone either way.
// shortcut: reactions with their own `on_reaction` (stabilizers, TATP, holy water, gunpowder,
// teslium...) are not explosions here (`ExplosionDef`); mobs' holders do not explode.
fn reagent_explosions(world: &mut World, chem: &Chemistry, e: Entity, reacted: &[(u32, f32)]) {
    for &(index, volume) in reacted {
        let Some(ex) = chem.reactions[index as usize].def.explosion else {
            continue;
        };
        // DM `round(x, 1)`: to the nearest, halves up.
        let power = ex.modifier + (volume / ex.strengthdiv.max(f32::EPSILON) + 0.5).floor();
        if power > 0.0
            && let Some(pos) = map_pos(world, e)
        {
            let clients = clients_near(world, pos, None);
            if !clients.is_empty() {
                world.write_message(SendChat {
                    clients,
                    message: line(Span::Danger, "Раствор бурно взрывается!"),
                });
            }
            crate::explosions::dyn_explosion(world, pos, power, None, None, true);
        }
        let Some(mut r) = world.get_mut::<Reagents>(e) else {
            return;
        };
        r.0.clear();
    }
}

/// `/datum/chemical_reaction/emp_pulse`.
const EMP_REACTION: &str = "/datum/chemical_reaction/emp_pulse";

/// `emp_pulse/on_reaction`: an EMP from the container's tile, `round(volume / 1.5 / 12)` heavy
/// and `round(volume / 1.5 / 7)` light, and the container is emptied.
/// `emp_pulse/on_reaction` ranges: "pretending this reaction took two ingredients", DM
/// one-argument `round` (down).
fn emp_reaction_ranges(volume: f32) -> (i32, i32) {
    let two_thirds = volume / 1.5;
    (
        (two_thirds / 12.0).floor() as i32,
        (two_thirds / 7.0).floor() as i32,
    )
}

fn reagent_emps(world: &mut World, chem: &Chemistry, e: Entity, reacted: &[(u32, f32)]) {
    for &(index, volume) in reacted {
        if chem.reactions[index as usize].def.path != EMP_REACTION {
            continue;
        }
        if let Some(pos) = map_pos(world, e) {
            let (heavy, light) = emp_reaction_ranges(volume);
            world.write_message(crate::emp::Empulse {
                epicenter: pos,
                heavy,
                light,
            });
        }
        if let Some(mut r) = world.get_mut::<Reagents>(e) {
            r.0.clear();
        }
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
                &format!("Вы открываете {name}, и вас внезапно окатывает потоком жидкости! Ааа!!"),
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
    chem.glass_style(container, h.master_reagent()?.id)
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
    /// [`crate::quirks::booze_multiplier`].
    booze: f32,
    /// tg `IS_UNCONSCIOUS`, `on_fire`.
    unconscious: bool,
    on_fire: bool,
    /// tg `reagents_metabolized` of this tick.
    metabolized: Vec<metabolism::Metabolized>,
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
    fn booze_power_multiplier(&self) -> f32 {
        self.booze
    }
    fn unconscious(&self) -> bool {
        self.unconscious
    }
    fn on_fire(&self) -> bool {
        self.on_fire
    }
    fn metabolized(&mut self, reagent: metabolism::Metabolized) {
        self.metabolized.push(reagent);
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
    // `TRAIT_STASIS`: nothing metabolizes (`handle_stasis_chems` runs none of the ported).
    // A drug that just left the blood still ends its effects (`crate::drugs::DrugsActive`).
    let mut query = world.query_filtered::<
        (Entity, &MobReagents, Has<crate::drugs::DrugsActive>),
        Without<crate::medical::stasis::InStasis>,
    >();
    let mobs: Vec<Entity> = query
        .iter(world)
        .filter(|(_, m, drugs)| {
            !m.blood.reagents.is_empty()
                || !m.stomach.reagents.is_empty()
                || m.drunk > 0.0
                || *drugs
        })
        .map(|(e, _, _)| e)
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
    // `romerol/expose_mob`: any amount, before it is gone (`metabolization_rate = INFINITY`).
    crate::antag::zombie::romerol_in_mob(world, chem, mob);
    crate::medical::traumas::brain_reagents(world, mob, seconds);
    crate::hallucination::reagents(world, mob, seconds);
    crate::radiation::reagents(world, mob, seconds);
    crate::buffs::reagents(world, mob, seconds);
    crate::service::laughter(world, mob);
    crate::quirks::allergic_reagents(world, mob, seconds);
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
    // A mob's holders are silent (`ismob(my_atom)`), and their explosions are not ported.
    for h in [&mut reagents.blood, &mut reagents.stomach] {
        h.messages.clear();
        h.reacted.clear();
    }
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
    let booze = crate::quirks::booze_multiplier(world, mob);
    let unconscious = matches!(stat, Stat::HardCrit | Stat::Dead)
        || crate::status_effects::incapacitation(world, mob).knocked_out;
    let on_fire = world
        .get::<crate::fire::FireStacks>(mob)
        .is_some_and(|f| f.on_fire);
    let liver_damage;
    let (told, metabolized) = {
        let mut body = MobBody {
            health: &mut health,
            losebreath: &mut losebreath,
            drunk: &mut reagents.drunk,
            dead,
            rng: &mut rng,
            told: Vec::new(),
            hunger: hunger.as_mut(),
            booze,
            unconscious,
            on_fire,
            metabolized: Vec::new(),
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
        (body.told, body.metabolized)
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
    let drunk = reagents.drunk;
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
    // The drunk stage's side effects on the same 2 s tick.
    if !dead {
        crate::status_effects::drunk_pulse(world, mob, drunk);
    }
    // What `metabolize_reagent` and `metabolize` do beyond `ss13_reagents`, in tg's order:
    // addiction points, the reagents' own effects, then the stacked effects.
    crate::addictions::metabolized(world, chem, mob, &metabolized);
    crate::drugs::metabolized(world, chem, mob, &metabolized, seconds);
    crate::genetics::mutation_toxin::metabolized(world, chem, mob, &metabolized, seconds);
    crate::pyrotechnics::metabolized(world, chem, mob, &metabolized, seconds);
    crate::drugs::stacked_effects(world, chem, mob, &metabolized, seconds);
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
    fn emp_reaction_ranges_follow_volume() {
        assert_eq!(emp_reaction_ranges(100.0), (5, 9));
        assert_eq!(emp_reaction_ranges(200.0), (11, 19));
        assert_eq!(emp_reaction_ranges(10.0), (0, 0));
    }

    #[test]
    fn reagents_are_seen_with_scanner_glasses_or_dead() {
        let mut world = World::new();
        let glasses = |scanner: bool| DmClothing {
            reagent_scanner: scanner,
            ..DmClothing::default()
        };
        let sunglasses = world.spawn(glasses(false)).id();
        let science = world.spawn(glasses(true)).id();
        let mob = world.spawn((Inventory::default(), Stat::Conscious)).id();
        assert!(!can_see_reagents(&world, mob), "bare eyes");
        world.get_mut::<Inventory>(mob).unwrap().eyes = Some(sunglasses);
        assert!(!can_see_reagents(&world, mob), "plain glasses");
        world.get_mut::<Inventory>(mob).unwrap().eyes = Some(science);
        assert!(can_see_reagents(&world, mob), "science goggles");
        world.get_mut::<Inventory>(mob).unwrap().eyes = None;
        world.entity_mut(mob).insert(Stat::Dead);
        assert!(can_see_reagents(&world, mob), "the dead");
    }

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
