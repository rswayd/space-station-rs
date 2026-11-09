//! Disposal bins and delivery chutes (`/obj/machinery/disposal`, `bin.dm`): contents, air,
//! flushing, the `DisposalUnit` window.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_atmos::GasMixture;
use ss13_atmos::gas::{CELL_VOLUME, ONE_ATMOSPHERE, R_IDEAL_GAS_EQUATION};
use ss13_mapload::fidelity::IconIndex;
use ss13_mapload::{Areas, DmAtom, DmItem, DmName};
use ss13_shared::appearance::{EMISSIVE_PLANE, Overlay};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Appearance, IconStateId, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::disposal_unit::{DisposalUnitData, DisposalUnitUi};
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject, UiWindow};
use ss13_world::Bumped;

use super::{
    DisposalHolder, DisposalJob, DisposalStats, HOLDER_STEPS, PIPE_EJECT_RANGE,
    SORT_TYPE_DISPOSALS, items_in, mobs_in, pipe_eject, pipes, vent_and_delete,
};
use crate::closets::{ClosetRelayMove, InCloset};
use crate::health::{DEFAULT_MESSAGE_RANGE, Health, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::power::{AreaPower, Channel};
use crate::storage::put_inside;
use crate::tools::Actor;

/// `SEND_PRESSURE`: a bin is ready to flush at 5% of an atmosphere.
pub const SEND_PRESSURE: f64 = 0.05 * ONE_ATMOSPHERE;
/// `flush_every_ticks`: a full bin with contents flushes by itself every 30 process ticks.
const FLUSH_EVERY_TICKS: u32 = 30;
/// `flush()` sleeps 1 s and 0.5 s before the holder leaves.
const FLUSH_DELAY_SECS: f64 = 1.5;
/// `stuff_mob_in`: `do_after(user, 2 SECONDS, target)`.
const STUFF_DELAY_DS: f32 = 20.0;
/// `bin/hitby`: `prob(75)` that a thrown item lands inside.
const THROW_IN_CHANCE: f32 = 75.0;
/// `delivery_chute/Bumped`: `prob(2)` that a mob hits the edge.
const CHUTE_EDGE_CHANCE: f32 = 2.0;
/// `relaymove` of a holder: "CLONG, clong!" within 5 tiles.
const CLONG_RANGE: u32 = 5;
const TAGGER_PATH: &str = "/obj/item/dest_tagger";

/// `/obj/machinery/disposal/bin` and `/obj/machinery/disposal/delivery_chute`.
#[derive(Component, Clone, Debug)]
pub struct DisposalBin {
    pub chute: bool,
    /// `air_contents`, at `CELL_VOLUME`.
    pub air: GasMixture,
    pub full_pressure: bool,
    pub pressure_charging: bool,
    /// The handle is engaged.
    pub flush: bool,
    /// A flush is running; its holder leaves then (seconds).
    pub flushing_until: Option<f64>,
    pub trunk: Option<Entity>,
    /// `mounted_tagger` (`/obj/machinery/disposal/bin/tagger` starts with one).
    pub mounted_tagger: bool,
    flush_count: u32,
    /// `post_machine_initialize` took its first air from the room.
    air_ready: bool,
    sprites: BinSprites,
}

#[derive(Clone, Debug, Default)]
struct BinSprites {
    base: Option<IconStateId>,
    flush: Option<IconStateId>,
    handle: Option<IconStateId>,
    full: Option<IconStateId>,
    charge: Option<IconStateId>,
    charge_glow: Option<IconStateId>,
    ready: Option<IconStateId>,
    ready_glow: Option<IconStateId>,
    tagger: Option<IconStateId>,
}

impl DisposalBin {
    pub(super) fn new(chute: bool, path: &str, atom: &DmAtom, icons: Option<&IconIndex>) -> Self {
        let get = |state: &str| icons.and_then(|i| i.get(&atom.icon, state));
        let base = atom.icon_state.as_str();
        Self {
            chute,
            air: GasMixture::vacuum(),
            full_pressure: false,
            // The chute "doesn't need charging and always works".
            pressure_charging: !chute,
            flush: false,
            flushing_until: None,
            trunk: None,
            mounted_tagger: path == "/obj/machinery/disposal/bin/tagger",
            flush_count: 0,
            air_ready: false,
            sprites: BinSprites {
                base: get(base),
                flush: get(&format!("{base}-flush")),
                handle: get("dispover-handle"),
                full: get("dispover-full"),
                charge: get("dispover-charge"),
                charge_glow: get("dispover-charge-glow"),
                ready: get("dispover-ready"),
                ready_glow: get("dispover-ready-glow"),
                tagger: get("tagger_mount"),
            },
        }
    }

    pub fn pressure(&self) -> f64 {
        self.air.pressure()
    }
}

/// Area equipment power on `pos` (tg `machine_stat & NOPOWER`; areas without APC needs are
/// always powered).
fn powered(world: &World, pos: TilePos) -> bool {
    world
        .get_resource::<Areas>()
        .and_then(|a| a.area_at(pos))
        .and_then(|a| world.get::<AreaPower>(a))
        .is_none_or(|a| a.powered(Channel::Equip))
}

/// tg `update_overlays` of a bin (the chute has none) and the `-flush` flick while flushing.
pub(super) fn refresh_look(world: &mut World, bin: Entity) {
    let (Some(state), Some(&pos)) = (
        world.get::<DisposalBin>(bin).cloned(),
        world.get::<TilePos>(bin),
    ) else {
        return;
    };
    let occupied = !items_in(world, bin).is_empty() || !mobs_in(world, bin).is_empty();
    let s = &state.sprites;
    let icon = if state.flushing_until.is_some() {
        s.flush.or(s.base)
    } else {
        s.base
    };
    let mut overlays = Vec::new();
    if !state.chute {
        let plain = |id: Option<IconStateId>| id.map(Overlay::new);
        let glow = |id: Option<IconStateId>| {
            id.map(|id| Overlay {
                plane: Some(EMISSIVE_PLANE),
                ..Overlay::new(id)
            })
        };
        if state.flush {
            overlays.extend(plain(s.handle));
        }
        if state.mounted_tagger {
            overlays.extend(plain(s.tagger));
        }
        // Only the handle shows without power.
        if powered(world, pos) {
            if occupied {
                overlays.extend(plain(s.full));
                overlays.extend(glow(s.full));
            }
            if state.pressure_charging {
                overlays.extend(plain(s.charge));
                overlays.extend(glow(s.charge_glow));
            } else if state.full_pressure {
                overlays.extend(plain(s.ready));
                overlays.extend(glow(s.ready_glow));
            }
        }
    }
    if let Some(mut appearance) = world.get_mut::<Appearance>(bin) {
        let mut look = appearance.clone();
        if let Some(icon) = icon {
            look.icon_state = icon;
        }
        look.overlays = overlays;
        appearance.set_if_neq(look);
    }
}

/// `post_machine_initialize`: a copy of the room's air, of which `SEND_PRESSURE + 1` moles
/// (tg passes a pressure as moles) go into the bin; the room keeps its air.
pub(super) fn init_air(world: &mut World) {
    if world.get_resource::<ss13_atmos::Atmos>().is_none() {
        return;
    }
    let mut q = world.query::<(Entity, &DisposalBin, &TilePos)>();
    let pending: Vec<(Entity, TilePos)> = q
        .iter(world)
        .filter(|(_, b, _)| !b.air_ready)
        .map(|(e, _, &p)| (e, p))
        .collect();
    for (bin, pos) in pending {
        let room = world.resource::<ss13_atmos::Atmos>().air(pos).cloned();
        let mut state = world.get_mut::<DisposalBin>(bin).expect("queried");
        state.air_ready = true;
        if let Some(mut room) = room
            && let Some(taken) = room.remove(SEND_PRESSURE + 1.0)
        {
            state.air.merge_raw(&taken);
        }
    }
}

/// `bin/process(seconds_per_tick)`, one 2 s machine tick of every bin.
// shortcut: bins draw no energy from the APC (`use_energy(idle_power_usage)`); add with
// machine loads for `NO_POWER_USE` machines.
pub(super) fn process(world: &mut World, now: f64, seconds: f64) {
    let mut q = world.query::<(Entity, &DisposalBin, &TilePos)>();
    let mut bins: Vec<(Entity, TilePos)> = q
        .iter(world)
        .filter(|(_, b, _)| !b.chute)
        .map(|(e, _, &p)| (e, p))
        .collect();
    bins.sort_by_key(|b| b.0);
    for (bin, pos) in bins {
        let occupied = !items_in(world, bin).is_empty() || !mobs_in(world, bin).is_empty();
        let mut state = world.get::<DisposalBin>(bin).cloned().expect("queried");
        state.flush_count += 1;
        let mut flush = false;
        if state.flush_count >= FLUSH_EVERY_TICKS {
            flush |= occupied && state.full_pressure;
            state.flush_count = 0;
        }
        // "flush can happen even without power"
        flush |= state.flush && state.pressure() >= SEND_PRESSURE;
        if powered(world, pos) && state.pressure_charging {
            charge(world, &mut state, pos, seconds);
        }
        world.entity_mut(bin).insert(state);
        if flush {
            start_flush(world, bin, now);
        }
        refresh_look(world, bin);
    }
}

/// The charging part of `process`: air from the room towards `SEND_PRESSURE * 1.01`.
fn charge(world: &mut World, state: &mut DisposalBin, pos: TilePos, seconds: f64) {
    let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() else {
        return;
    };
    let Some(temperature) = atmos.temperature(pos).filter(|&t| t > 0.0) else {
        return;
    };
    let pressure_delta = SEND_PRESSURE * 1.01 - state.pressure();
    let transfer_moles =
        0.05 * seconds * (pressure_delta * CELL_VOLUME) / (temperature * R_IDEAL_GAS_EQUATION);
    if transfer_moles > 0.0
        && let Some(removed) = atmos.remove_air(pos, transfer_moles)
    {
        state.air.merge_raw(&removed);
    }
    if state.pressure() >= SEND_PRESSURE {
        state.full_pressure = true;
        state.pressure_charging = false;
    }
}

/// `flush()` starts: the flush animation, the holder 1.5 s later.
pub(super) fn start_flush(world: &mut World, bin: Entity, now: f64) {
    let Some(mut state) = world.get_mut::<DisposalBin>(bin) else {
        return;
    };
    if state.flushing_until.is_some() {
        return;
    }
    state.flushing_until = Some(now + FLUSH_DELAY_SECS);
    refresh_look(world, bin);
}

/// The rest of `flush()` for flushes whose 1.5 s are over.
pub(super) fn finish_flushes(world: &mut World, now: f64) {
    let mut q = world.query::<(Entity, &DisposalBin)>();
    let mut done: Vec<Entity> = q
        .iter(world)
        .filter(|(_, b)| b.flushing_until.is_some_and(|t| now >= t))
        .map(|(e, _)| e)
        .collect();
    done.sort();
    for bin in done {
        flush_now(world, bin, now);
    }
}

/// `newHolderDestination`, `disposalholder/init` and `start`, then `bin/flush`'s reset.
fn flush_now(world: &mut World, bin: Entity, now: f64) {
    let Some(&pos) = world.get::<TilePos>(bin) else {
        return;
    };
    let mut state = world
        .get::<DisposalBin>(bin)
        .cloned()
        .expect("caller checked");
    let items = items_in(world, bin);
    let mobs = mobs_in(world, bin);
    let hasmob = !mobs.is_empty();
    let deliveries: Vec<u8> = items
        .iter()
        .filter_map(|&i| world.get::<super::Delivery>(i).map(|d| d.sort_tag))
        .collect();
    let mut destination_tag = SORT_TYPE_DISPOSALS;
    // `COMSIG_MOVABLE_DISPOSING` of each parcel: its tag, unless a mob rides along.
    if !hasmob && let Some(&last) = deliveries.last() {
        destination_tag = last;
    }
    let trunk = state
        .trunk
        .filter(|&t| world.get::<super::DisposalPipe>(t).is_some());
    let holder = world
        .spawn((
            Name::new("disposal holder"),
            DisposalHolder {
                pipe: trunk.unwrap_or(bin),
                pos,
                dir: pipes::DOWN,
                gas: std::mem::replace(&mut state.air, GasMixture::vacuum()),
                active: trunk.is_some(),
                count: HOLDER_STEPS,
                destination_tag,
                tomail: !deliveries.is_empty(),
                hasmob,
                ejecting: None,
                eject_hiss: false,
            },
        ))
        .id();
    for item in items {
        put_inside(world, item, holder);
    }
    for mob in mobs {
        if let Some(mut c) = world.get_mut::<InCloset>(mob) {
            c.closet = holder;
        }
    }
    state.flushing_until = None;
    state.flush = false;
    if !state.chute {
        state.full_pressure = false;
        state.pressure_charging = true;
    }
    world.entity_mut(bin).insert(state);
    world.resource_mut::<DisposalStats>().flushes += 1;
    if trunk.is_none() {
        // `start`: no trunk, out at once.
        expel_at(world, bin, holder, now);
    }
    refresh_look(world, bin);
}

/// `disposal/expel`: a holder arriving at (or leaving without a trunk from) a bin: its
/// contents are thrown around the bin.
pub(super) fn expel(world: &mut World, bin: Entity, holder: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    expel_at(world, bin, holder, now);
    world.resource_mut::<DisposalStats>().delivered += 1;
}

fn expel_at(world: &mut World, bin: Entity, holder: Entity, now: f64) {
    let pos = world
        .get::<TilePos>(bin)
        .copied()
        .or_else(|| world.get::<DisposalHolder>(holder).map(|h| h.pos));
    let Some(pos) = pos else {
        return;
    };
    if let Some(mut h) = world.get_mut::<DisposalHolder>(holder) {
        h.active = false;
    }
    pipe_eject(world, holder, pos, Some((None, PIPE_EJECT_RANGE, 1.0)), now);
    vent_and_delete(world, holder, pos);
    refresh_look(world, bin);
}

/// `eject()`: everything inside onto the bin's tile, not thrown.
pub(super) fn eject(world: &mut World, bin: Entity) {
    let Some(&pos) = world.get::<TilePos>(bin) else {
        return;
    };
    for item in items_in(world, bin) {
        crate::storage::take_out(world, item, pos);
    }
    for mob in mobs_in(world, bin) {
        let_out(world, mob);
    }
    refresh_look(world, bin);
}

pub(super) fn let_out(world: &mut World, mob: Entity) {
    crate::closets::let_out(world, mob);
}

fn tell(world: &mut World, client: Entity, span: Span, text: impl Into<String>) {
    crate::tools::tell(world, client, span, text);
}

fn name(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

/// tg `attack_hand` of a bin: the `DisposalUnit` window; right click takes the mounted tagger
/// (`attack_hand_secondary`). True when `target` is a disposal machine.
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let Some(state) = world.get::<DisposalBin>(target).cloned() else {
        return false;
    };
    if state.chute {
        return true;
    }
    if modifiers.right {
        if !state.mounted_tagger {
            tell(world, client, Span::Warning, "Нет маркировщика!");
            return true;
        }
        let Some(&pos) = world.get::<TilePos>(mob) else {
            return true;
        };
        let Some(tagger) = crate::construction::spawn_obj(world, TAGGER_PATH, pos) else {
            return true;
        };
        if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
            crate::storage::put_in_hands(world, mob, &inventory, tagger);
        }
        world
            .get_mut::<DisposalBin>(target)
            .expect("checked")
            .mounted_tagger = false;
        let (user, what) = (name(world, mob), name(world, tagger));
        tell(
            world,
            client,
            Span::Notice,
            format!("{user} снимает {what}."),
        );
        refresh_look(world, target);
        return true;
    }
    let title = world
        .get::<DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<DisposalUnitWindow>(
        client, mob, target, title,
    ));
    true
}

/// tg `disposal/item_interaction`: outside combat mode the held item goes in
/// (`place_item_in_disposal`); a chute flushes it at once. Right click with a destination
/// tagger mounts it on a bin (`item_interaction_secondary`).
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let Some(state) = world.get::<DisposalBin>(target).cloned() else {
        return false;
    };
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    if world.get::<CarriedBy>(held).is_none_or(|c| c.0 != mob) {
        return true;
    }
    if modifiers.right
        && !state.chute
        && world
            .get::<DmItem>(held)
            .is_some_and(|i| i.path == TAGGER_PATH)
    {
        if state.mounted_tagger {
            tell(world, client, Span::Warning, "Маркировщик уже есть!");
            return true;
        }
        world.despawn(held);
        world
            .get_mut::<DisposalBin>(target)
            .expect("checked")
            .mounted_tagger = true;
        refresh_look(world, target);
        return true;
    }
    put_inside(world, held, target);
    let (user, what, bin) = (name(world, mob), name(world, held), name(world, target));
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    crate::security::show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Info,
        (Span::Notice, format!("{user} кладёт {what} в {bin}.")),
        &[(mob, Span::Notice, format!("Вы кладёте {what} в {bin}."))],
    );
    if state.chute {
        let now = world.resource::<Time>().elapsed_secs_f64();
        start_flush(world, target, now);
    }
    refresh_look(world, target);
    true
}

/// `bin/hitby` (75% that a thrown item lands inside) and `delivery_chute/Bumped` (from its
/// front side, in it goes and it flushes). True when `hit` is a disposal machine.
pub fn hitby(world: &mut World, item: Entity, hit: Entity) -> bool {
    let Some(state) = world.get::<DisposalBin>(hit).cloned() else {
        return false;
    };
    if world.get::<DmItem>(item).is_none() {
        return false;
    }
    let (Some(&pos), Some(&at)) = (world.get::<TilePos>(hit), world.get::<TilePos>(item)) else {
        return true;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if state.chute {
        if from_front(world, hit, at) {
            put_inside(world, item, hit);
            start_flush(world, hit, now);
        }
        return true;
    }
    let (thing, bin) = (name(world, item), name(world, hit));
    let lands = world
        .resource_mut::<crate::combat::CombatRng>()
        .chance(THROW_IN_CHANCE);
    let text = if lands {
        put_inside(world, item, hit);
        format!("{thing} попадает в {bin}.")
    } else {
        format!("{thing} отскакивает от края {bin}!")
    };
    crate::security::show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Info,
        (Span::Notice, text),
        &[],
    );
    refresh_look(world, hit);
    true
}

/// `delivery_chute/Bumped`'s side check: the bumper stands on the tile the chute faces.
fn from_front(world: &World, chute: Entity, at: TilePos) -> bool {
    let (Some(&pos), Some(atom)) = (world.get::<TilePos>(chute), world.get::<DmAtom>(chute)) else {
        return false;
    };
    let dir = if atom.dir == 0 {
        pipes::SOUTH
    } else {
        atom.dir
    };
    let (dx, dy) = pipes::offset(dir);
    at == TilePos::new(pos.x + dx, pos.y + dy, pos.z)
}

/// `disposal/mouse_drop_receive` -> `stuff_mob_in` (from `crate::drag`): a living mob dropped
/// onto a bin goes in after 2 s; the user may climb in itself. True when `bin` takes drops.
pub fn mouse_drop_receive(world: &mut World, actor: Actor, bin: Entity, dragged: Entity) -> bool {
    if world.get::<DisposalBin>(bin).is_none() {
        return false;
    }
    if world.get::<Health>(dragged).is_none() {
        return true;
    }
    let user = actor.mob;
    // Only carbons climb in by themselves.
    if user == dragged && world.get::<crate::silicon::cyborg::Cyborg>(user).is_some() {
        return true;
    }
    if world.get::<InCloset>(user).is_some()
        || world.get::<InCloset>(dragged).is_some()
        || world
            .get::<crate::medical::surgery::Buckled>(dragged)
            .is_some()
    {
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(dragged) else {
        return true;
    };
    let (who, what, into) = (name(world, user), name(world, dragged), name(world, bin));
    let (others, personal) = if user == dragged {
        (
            (Span::Warning, format!("{who} начинает залезать в {into}.")),
            vec![(
                user,
                Span::Notice,
                format!("Вы начинаете залезать в {into}..."),
            )],
        )
    } else {
        (
            (
                Span::Danger,
                format!("{who} начинает запихивать {what} в {into}."),
            ),
            vec![(
                dragged,
                Span::Danger,
                format!("{who} начинает запихивать вас в {into}!"),
            )],
        )
    };
    crate::security::show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        others,
        &personal,
    );
    crate::tools::do_after(
        world,
        actor,
        dragged,
        STUFF_DELAY_DS,
        0,
        crate::construction::Job::Disposal(DisposalJob::StuffMob { bin }),
    );
    true
}

/// End of the `stuff_mob_in` `do_after`, revalidated.
pub(super) fn complete_stuff(world: &mut World, user: Entity, victim: Entity, bin: Entity) {
    let Some(&pos) = world.get::<TilePos>(bin) else {
        return;
    };
    if world.get::<DisposalBin>(bin).is_none()
        || world.get::<Health>(victim).is_none()
        || world.get::<InCloset>(victim).is_some()
        || world.get::<InCloset>(user).is_some()
    {
        return;
    }
    let (who, what, into) = (name(world, user), name(world, victim), name(world, bin));
    let (others, personal) = if user == victim {
        (
            (Span::Warning, format!("{who} залезает в {into}.")),
            vec![(user, Span::Notice, format!("Вы залезаете в {into}."))],
        )
    } else {
        (
            (Span::Danger, format!("{who} запихивает {what} в {into}.")),
            vec![(
                victim,
                Span::Danger,
                format!("{who} запихивает вас в {into}."),
            )],
        )
    };
    crate::security::show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        others,
        &personal,
    );
    crate::closets::put_mob_inside(world, victim, bin);
    world.entity_mut(victim).insert(pos);
    refresh_look(world, bin);
}

/// Steps of mobs inside: out of a bin that is not flushing (`attempt_escape`), a clang in a
/// pipe (`disposalholder/relaymove`).
pub(super) fn relay_moves(world: &mut World, mut moves: Local<MessageCursor<ClosetRelayMove>>) {
    let mobs: Vec<Entity> = moves
        .read(world.resource::<Messages<ClosetRelayMove>>())
        .map(|m| m.mob)
        .collect();
    for mob in mobs {
        let Some(container) = world.get::<InCloset>(mob).map(|c| c.closet) else {
            continue;
        };
        if let Some(state) = world.get::<DisposalBin>(container) {
            if state.flushing_until.is_none() {
                let_out(world, mob);
                refresh_look(world, container);
            }
        } else if let Some(h) = world.get::<DisposalHolder>(container) {
            let pos = h.pos;
            if world.get::<Stat>(mob) != Some(&Stat::Conscious)
                || crate::status_effects::incapacitation(world, mob).incapacitated
            {
                continue;
            }
            crate::security::show(
                world,
                pos,
                CLONG_RANGE,
                ChatChannel::Local,
                (Span::Plain, "CLONG, clong!".to_owned()),
                &[],
            );
        }
    }
}

/// `delivery_chute/Bumped` by a walking mob, from the front: in it goes (2% hit the edge) and
/// the chute flushes.
pub(super) fn chute_bumps(world: &mut World, mut bumps: Local<MessageCursor<Bumped>>) {
    let bumps: Vec<Bumped> = bumps
        .read(world.resource::<Messages<Bumped>>())
        .copied()
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for bump in bumps {
        let (mob, chute) = (bump.entity, bump.obstacle);
        if !world.get::<DisposalBin>(chute).is_some_and(|b| b.chute)
            || world.get::<Health>(mob).is_none()
            || world.get::<InCloset>(mob).is_some()
        {
            continue;
        }
        let (Some(&at), Some(&pos)) = (world.get::<TilePos>(mob), world.get::<TilePos>(chute))
        else {
            continue;
        };
        if !from_front(world, chute, at) {
            continue;
        }
        if world
            .resource_mut::<crate::combat::CombatRng>()
            .chance(CHUTE_EDGE_CHANCE)
        {
            crate::chem::tell_mob(world, mob, Span::Warning, "Вы ударяетесь о край люка.");
            continue;
        }
        crate::closets::put_mob_inside(world, mob, chute);
        world.entity_mut(mob).insert(pos);
        start_flush(world, chute, now);
    }
}

/// A removed bin lets its mobs out where it stood.
pub(super) fn release_on_despawn(
    remove: On<Remove<DisposalBin>>,
    inside: Query<(Entity, &InCloset)>,
    mut commands: Commands,
) {
    for (mob, c) in &inside {
        if c.closet == remove.entity {
            commands.queue(move |world: &mut World| let_out(world, mob));
        }
    }
}

/// The server side of the `DisposalUnit` interface.
pub struct DisposalUnitWindow;

impl UiInterface for DisposalUnitWindow {
    const NAME: &'static str = DisposalUnitUi::NAME;
    type Data = DisposalUnitData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisposalUnitAct {
    /// `handle-0` / `handle-1`.
    Handle(bool),
    /// `pump-0` / `pump-1`.
    Pump(bool),
    Eject,
}

impl ServerUi for DisposalUnitWindow {
    type Source = &'static DisposalBin;
    type UserParam = ();
    type Act = DisposalUnitAct;

    fn static_data(_: &DisposalBin) {}

    fn data(bin: &DisposalBin, _: Entity, _: &()) -> DisposalUnitData {
        DisposalUnitData {
            flush: bin.flush,
            full_pressure: bin.full_pressure,
            pressure_charging: bin.pressure_charging,
            panel_open: false,
            per: (bin.pressure() / SEND_PRESSURE).clamp(0.0, 1.0) as f32,
            isai: false,
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<DisposalUnitAct, UiReject> {
        match action {
            "handle-0" => Ok(DisposalUnitAct::Handle(false)),
            "handle-1" => Ok(DisposalUnitAct::Handle(true)),
            "pump-0" => Ok(DisposalUnitAct::Pump(false)),
            "pump-1" => Ok(DisposalUnitAct::Pump(true)),
            "eject" => Ok(DisposalUnitAct::Eject),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `bin/ui_act`, applied on the bin's current state.
pub(super) fn apply_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<DisposalUnitWindow>>>,
) {
    let pending: Vec<(Entity, DisposalUnitAct)> = acts
        .read(world.resource::<Messages<UiActed<DisposalUnitWindow>>>())
        .map(|a| (a.source, a.act))
        .collect();
    for (bin, act) in pending {
        let Some(mut state) = world.get_mut::<DisposalBin>(bin) else {
            continue;
        };
        match act {
            // `handle-1` needs a closed panel; panels do not open here.
            DisposalUnitAct::Handle(on) => state.flush = on,
            DisposalUnitAct::Pump(on) => state.pressure_charging = on,
            DisposalUnitAct::Eject => {
                eject(world, bin);
                continue;
            }
        }
        refresh_look(world, bin);
    }
}

/// `notcontained_state`: a mob inside the bin has no window of it.
pub(super) fn window_status(mut windows: Query<&mut UiWindow>, inside: Query<&InCloset>) {
    for mut window in &mut windows {
        if window.interface == DisposalUnitWindow::NAME
            && inside
                .get(window.actor)
                .is_ok_and(|c| c.closet == window.source)
        {
            window.cap_status(ss13_tgui::UiStatus::Close);
        }
    }
}
