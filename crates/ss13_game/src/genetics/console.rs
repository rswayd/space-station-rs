//! The DNA console (`/obj/machinery/computer/dna_console`, tgui `DnaConsole`), the DNA scanner
//! (`/obj/machinery/dna_scannernew`) and DNA injectors (`/obj/item/dnainjector`).
//!
//! - The scanner: a click with an empty hand opens or closes it (`toggle_open`); closing takes
//!   the living mob on its tile inside, a mob dragged onto an open scanner is shut in
//!   (`mouse_drop_receive` -> `close_machine`); a step from inside opens it (`relaymove`).
//! - The console links to a scanner on a cardinal neighbour tile (`connect_to_scanner`); a click
//!   with an empty hand opens the window. `pulse_gene` cycles one base of the occupant's block
//!   (`domutcheck`, then `check_discovery`); `print_injector` makes an activator or a mutator
//!   of an active, discovered mutation of the occupant, with tg's cooldowns
//!   (`max(5 s, |instability| s) * 1.0` and `max(10 s, |instability| s) * 1.0` at tier 1);
//!   `nullify` removes a mutator-added or scrambled mutation. No access is needed (tg has none).
//! - An injector on a living human: on someone else after a 3 s `do_after`; it removes its
//!   `remove_mutations`, then activates each mutation in the sequence or (mutators) adds it.
// shortcut: no scanner upgrades (tier 1 coefficients), power use, genetic damage of pulses,
// door lock, joker, scramble, disks or console storage; mapped injectors with preset
// mutations are plain injectors. Add with stock parts and the disk items.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmItem, DmName, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::dna_console::{DnaConsoleData, DnaConsoleUi, DnaMutation};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::{Density, SpatialIndex};

use super::*;
use crate::closets::{ClosetRelayMove, InCloset};
use crate::tools::{Actor, tell as tell_client};

pub const CONSOLE_PATH: &str = "/obj/machinery/computer/dna_console";
pub const SCANNER_PATH: &str = "/obj/machinery/dna_scannernew";
pub const INJECTOR_ROOT: &str = "/obj/item/dnainjector";
const ACTIVATOR_PATH: &str = "/obj/item/dnainjector/activator";
/// `MIN_ACTIVATOR_TIMEOUT`, `MIN_INJECTOR_TIMEOUT`.
const MIN_ACTIVATOR_SECS: f64 = 5.0;
const MIN_INJECTOR_SECS: f64 = 10.0;
/// `ACTIVATOR_COOLDOWN_MULTIPLIER`, `INJECTOR_COOLDOWN_MULTIPLIER` at `precision_coeff` 1.
const ACTIVATOR_CD_MULT: f64 = 1.0 + 0.25 - 0.25;
const INJECTOR_CD_MULT: f64 = 1.0 + 0.15 - 0.15;
/// `dnainjector/attack`: `do_after(user, 3 SECONDS, target)`.
const INJECT_DELAY_DS: f32 = 30.0;

/// A DNA console: its linked scanner, the injector printer's cooldown and the window data.
#[derive(Component, Clone, Debug, Default)]
pub struct DnaConsole {
    pub scanner: Option<Entity>,
    /// Game time the printer is ready (`injector_ready`).
    pub injector_ready: f64,
    view: DnaConsoleData,
}

/// A DNA scanner (`state_open`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct DnaScanner {
    pub open: bool,
}

/// `/obj/item/dnainjector`.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct DnaInjector {
    pub add: Vec<usize>,
    pub remove: Vec<usize>,
    /// Activators: `force_mutate` adds a mutation not in the sequence (mutators).
    pub activator: bool,
    pub force_mutate: bool,
    pub used: bool,
}

/// A `do_after` of an injector on someone else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InjectJob;

pub(super) fn build(app: &mut App) {
    app.register_ui::<DnaConsoleWindow>()
        .add_observer(attach_machines)
        .add_observer(attach_injectors)
        .add_systems(
            Update,
            (
                refresh_views.before(UiSystems::Open),
                apply_acts.in_set(UiSystems::Handle),
                relay_moves,
            ),
        );
}

fn attach_machines(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(obj) = objs.get(add.entity) else {
        return;
    };
    if is_under(&obj.path, CONSOLE_PATH) {
        commands
            .entity(add.entity)
            .insert_if_new(DnaConsole::default());
    } else if is_under(&obj.path, SCANNER_PATH) {
        commands
            .entity(add.entity)
            .insert_if_new(DnaScanner::default());
    }
}

fn attach_injectors(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if let Ok(item) = items.get(add.entity)
        && is_under(&item.path, INJECTOR_ROOT)
    {
        commands.entity(add.entity).insert_if_new(DnaInjector {
            activator: is_under(&item.path, ACTIVATOR_PATH),
            ..default()
        });
    }
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<Name>(e)
        .map(|n| n.to_string())
        .or_else(|| {
            world
                .get::<DmName>(e)
                .map(|n| crate::vending::plain_name(&n.name).to_owned())
        })
        .unwrap_or_default()
}

/// The mob shut in `scanner` (`occupant`).
pub fn occupant(world: &mut World, scanner: Entity) -> Option<Entity> {
    let mut q = world.query::<(Entity, &InCloset)>();
    q.iter(world)
        .find(|(_, c)| c.closet == scanner)
        .map(|(e, _)| e)
}

/// `connect_to_scanner`: a scanner on a cardinal neighbour.
fn connect_to_scanner(world: &mut World, console: Entity) -> Option<Entity> {
    let pos = *world.get::<TilePos>(console)?;
    let found = [Dir::North, Dir::South, Dir::East, Dir::West]
        .into_iter()
        .find_map(|d| {
            world
                .resource::<SpatialIndex>()
                .on_tile(pos.step(d))
                .iter()
                .copied()
                .find(|&e| world.get::<DnaScanner>(e).is_some())
        });
    world.get_mut::<DnaConsole>(console)?.scanner = found;
    found
}

/// `update_icon_state` of the scanner.
fn refresh_scanner(world: &mut World, scanner: Entity) {
    let open = world.get::<DnaScanner>(scanner).is_some_and(|s| s.open);
    let occupied = occupant(world, scanner).is_some();
    let state = if occupied {
        "scanner_occupied"
    } else if open {
        "scanner_open"
    } else {
        "scanner"
    };
    let id = world
        .get::<DmAtom>(scanner)
        .zip(world.get_resource::<IconIndex>())
        .and_then(|(atom, icons)| icons.get(&atom.icon, state));
    if let Some(id) = id
        && let Some(mut appearance) = world.get_mut::<Appearance>(scanner)
        && appearance.icon_state != id
    {
        appearance.icon_state = id;
    }
}

/// `open_machine`: the occupant out, undense.
pub fn open_scanner(world: &mut World, scanner: Entity) {
    if let Some(mob) = occupant(world, scanner) {
        crate::closets::let_out(world, mob);
    }
    if let Some(mut s) = world.get_mut::<DnaScanner>(scanner) {
        s.open = true;
    }
    world.entity_mut(scanner).remove::<Density>();
    refresh_scanner(world, scanner);
}

/// `close_machine(target)`: `target` (or a living mob on the tile) inside, dense.
pub fn close_scanner(world: &mut World, scanner: Entity, target: Option<Entity>) {
    let Some(&pos) = world.get::<TilePos>(scanner) else {
        return;
    };
    let target = target.or_else(|| {
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<Dna>(e).is_some() && world.get::<InCloset>(e).is_none())
    });
    if let Some(mob) = target {
        world.entity_mut(mob).insert(pos);
        crate::closets::put_mob_inside(world, mob, scanner);
    }
    if let Some(mut s) = world.get_mut::<DnaScanner>(scanner) {
        s.open = false;
    }
    world.entity_mut(scanner).insert(Density);
    refresh_scanner(world, scanner);
}

/// `attack_hand` of the console (the window) and the scanner (`toggle_open`).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if let Some(scanner) = world.get::<DnaScanner>(target).copied() {
        if scanner.open {
            close_scanner(world, target, None);
        } else {
            open_scanner(world, target);
        }
        return true;
    }
    if world.get::<DnaConsole>(target).is_none() {
        return false;
    }
    connect_to_scanner(world, target);
    refresh_view(world, target);
    let title = name_of(world, target);
    world.write_message(OpenUi::new::<DnaConsoleWindow>(client, mob, target, title));
    true
}

/// `dna_scannernew/mouse_drop_receive`: a carbon onto an open scanner is shut in.
pub fn mouse_drop_receive(world: &mut World, scanner: Entity, dragged: Entity) -> bool {
    let Some(state) = world.get::<DnaScanner>(scanner).copied() else {
        return false;
    };
    if state.open && world.get::<Dna>(dragged).is_some() && world.get::<InCloset>(dragged).is_none()
    {
        close_scanner(world, scanner, Some(dragged));
    }
    true
}

/// `relaymove`: a conscious occupant's step opens the scanner.
fn relay_moves(world: &mut World, mut moves: Local<MessageCursor<ClosetRelayMove>>) {
    let mobs: Vec<Entity> = moves
        .read(world.resource::<Messages<ClosetRelayMove>>())
        .map(|m| m.mob)
        .collect();
    for mob in mobs {
        if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
            continue;
        }
        if let Some(c) = world.get::<InCloset>(mob).copied()
            && world.get::<DnaScanner>(c.closet).is_some()
        {
            open_scanner(world, c.closet);
        }
    }
}

/// `can_modify_occupant`: a linked, closed scanner with a living body with DNA inside.
fn modifiable_occupant(world: &mut World, console: Entity) -> Option<Entity> {
    let scanner = world.get::<DnaConsole>(console)?.scanner?;
    if world.get::<DnaScanner>(scanner)?.open {
        return None;
    }
    let mob = occupant(world, scanner)?;
    (world.get::<Dna>(mob).is_some() && world.get::<Stat>(mob) != Some(&Stat::Dead)).then_some(mob)
}

/// `ui_data` of the console (kept on it, recomputed each tick while a window is open).
fn compute_view(world: &mut World, console: Entity) -> DnaConsoleData {
    let db = db(world);
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(state) = world.get::<DnaConsole>(console).cloned() else {
        return default();
    };
    let mut data = DnaConsoleData {
        injector_seconds: (state.injector_ready - now).max(0.0).ceil() as f32,
        ..default()
    };
    let Some(scanner) = state.scanner.filter(|&s| world.get_entity(s).is_ok()) else {
        return data;
    };
    data.is_scanner_connected = true;
    data.is_open = world.get::<DnaScanner>(scanner).is_some_and(|s| s.open);
    let Some(mob) = occupant(world, scanner) else {
        return data;
    };
    let Some(dna) = world.get::<Dna>(mob).cloned() else {
        return data;
    };
    let discovered = world.resource::<DiscoveredMutations>().0.clone();
    data.is_viable_subject = true;
    data.subject_name = name_of(world, mob);
    data.subject_status = match world.get::<Stat>(mob) {
        Some(Stat::Dead) => 2,
        Some(Stat::Conscious) | None => 0,
        _ => 1,
    };
    data.subject_health = world.get::<Health>(mob).map_or(0.0, |h| h.health());
    data.subject_enzymes = dna.unique_enzymes.clone();
    data.subject_unique_identity = dna.unique_identity.clone();
    data.subject_stability = dna.stability;
    let entry = |id: usize, sequence: String, extra: bool| {
        let def = &MUTATIONS[id];
        let known = discovered.contains(&id);
        let active = dna.get_mutation(id);
        DnaMutation {
            alias: db.aliases[id].clone(),
            name: if known { def.name } else { &db.aliases[id] }.to_owned(),
            description: if known { def.desc } else { "" }.to_owned(),
            discovered: known,
            active: active.is_some(),
            sequence,
            instability: def.instability,
            quality: def.quality,
            class: active.map_or(0, |m| {
                if m.sources & SOURCE_MUTATOR != 0 {
                    2
                } else {
                    1
                }
            }),
            extra,
        }
    };
    data.mutations = dna
        .mutation_index
        .iter()
        .map(|(id, seq)| entry(*id, seq.clone(), false))
        .collect();
    data.mutations.extend(
        dna.mutations
            .iter()
            .filter(|m| !dna.in_sequence(m.id))
            .map(|m| entry(m.id, String::new(), true)),
    );
    data
}

fn refresh_view(world: &mut World, console: Entity) {
    let view = compute_view(world, console);
    if let Some(mut c) = world.get_mut::<DnaConsole>(console)
        && c.view != view
    {
        c.view = view;
    }
}

/// Consoles with an open window get fresh data.
// Cost: per tick, one pass over the open windows; a console's data is a few short strings.
fn refresh_views(world: &mut World) {
    let mut q = world.query::<&ss13_tgui::server::UiWindow>();
    let mut consoles: Vec<Entity> = q
        .iter(world)
        .filter(|w| w.interface == DnaConsoleWindow::NAME)
        .map(|w| w.source)
        .collect();
    consoles.dedup();
    for console in consoles {
        refresh_view(world, console);
    }
}

/// The server side of the `DnaConsole` interface.
pub struct DnaConsoleWindow;

impl UiInterface for DnaConsoleWindow {
    const NAME: &'static str = DnaConsoleUi::NAME;
    type Data = DnaConsoleData;
    type StaticData = ();
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DnaAct {
    ToggleDoor,
    /// `alias`, 1-based `pos`, next (`pulseAction` 1) or previous (2) base, or clear (0).
    PulseGene {
        alias: String,
        pos: usize,
        action: u8,
    },
    PrintInjector {
        alias: String,
        activator: bool,
    },
    Nullify {
        alias: String,
    },
}

impl ServerUi for DnaConsoleWindow {
    type Source = &'static DnaConsole;
    type UserParam = ();
    type Act = DnaAct;

    fn static_data(_: &DnaConsole) {}

    fn data(console: &DnaConsole, _: Entity, _: &()) -> DnaConsoleData {
        console.view.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<DnaAct, UiReject> {
        let alias = || {
            params
                .get("alias")
                .filter(|a| a.len() <= 32)
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams("alias".into()))
        };
        Ok(match action {
            "toggle_door" => DnaAct::ToggleDoor,
            "pulse_gene" => {
                let pos = params.number("pos")?;
                let act = params.number("pulseAction")?;
                if !(1.0..=64.0).contains(&pos) || !(0.0..=2.0).contains(&act) {
                    return Err(UiReject::BadParams("pos 1..64, pulseAction 0..2".into()));
                }
                DnaAct::PulseGene {
                    alias: alias()?,
                    pos: pos as usize,
                    action: act as u8,
                }
            }
            "print_injector" => DnaAct::PrintInjector {
                alias: alias()?,
                activator: params.number("is_activator")? != 0.0,
            },
            "nullify" => DnaAct::Nullify { alias: alias()? },
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn apply_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<DnaConsoleWindow>>>) {
    let pending: Vec<(Entity, Entity, DnaAct)> = acts
        .read(world.resource::<Messages<UiActed<DnaConsoleWindow>>>())
        .map(|a| (a.source, a.client, a.act.clone()))
        .collect();
    for (console, client, act) in pending {
        if world.get::<DnaConsole>(console).is_none() {
            continue;
        }
        match act {
            DnaAct::ToggleDoor => {
                let scanner = world.get::<DnaConsole>(console).and_then(|c| c.scanner);
                if let Some(scanner) = scanner.filter(|&s| world.get_entity(s).is_ok()) {
                    interact_scanner(world, scanner);
                }
            }
            DnaAct::PulseGene { alias, pos, action } => {
                pulse_gene(world, console, &alias, pos, action)
            }
            DnaAct::PrintInjector { alias, activator } => {
                print_injector(world, console, client, &alias, activator);
            }
            DnaAct::Nullify { alias } => nullify(world, console, &alias),
        }
        refresh_view(world, console);
    }
}

fn interact_scanner(world: &mut World, scanner: Entity) {
    if world.get::<DnaScanner>(scanner).is_some_and(|s| s.open) {
        close_scanner(world, scanner, None);
    } else {
        open_scanner(world, scanner);
    }
}

/// `pulse_gene`, then `domutcheck` and `check_discovery(alias)`.
fn pulse_gene(world: &mut World, console: Entity, alias: &str, pos: usize, action: u8) {
    let Some(mob) = modifiable_occupant(world, console) else {
        return;
    };
    let db = db(world);
    let Some(id) = db.by_alias(alias) else {
        return;
    };
    let Some(mut dna) = world.get::<Dna>(mob).cloned() else {
        return;
    };
    let Some(i) = dna.mutation_index.iter().position(|(m, _)| *m == id) else {
        return;
    };
    let mut seq = dna.mutation_index[i].1.clone().into_bytes();
    if pos > seq.len() {
        return;
    }
    let gene = match action {
        // `CLEAR_GENE`: also in the default genes.
        0 => {
            let mut default = dna.default_mutation_genes[i].clone().into_bytes();
            if let Some(b) = default.get_mut(pos - 1) {
                *b = b'X';
            }
            dna.default_mutation_genes[i] = String::from_utf8(default).expect("ascii");
            b'X'
        }
        1 => cycle_gene(seq[pos - 1], true),
        _ => cycle_gene(seq[pos - 1], false),
    };
    seq[pos - 1] = gene;
    dna.mutation_index[i].1 = String::from_utf8(seq).expect("ascii");
    world.entity_mut(mob).insert(dna);
    domutcheck(world, mob);
    check_discovery(world, mob, id);
}

/// `check_discovery(alias)`: a complete block teaches the station its mutation.
fn check_discovery(world: &mut World, mob: Entity, id: usize) {
    let db = db(world);
    if world
        .get::<Dna>(mob)
        .is_some_and(|d| d.is_gene_active(&db, id))
    {
        let mut known = world.resource_mut::<DiscoveredMutations>();
        if !known.0.contains(&id) {
            known.0.push(id);
        }
    }
}

/// `print_injector` from the occupant (`SEARCH_OCCUPANT`): its active mutation.
fn print_injector(
    world: &mut World,
    console: Entity,
    client: Entity,
    alias: &str,
    activator: bool,
) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world
        .get::<DnaConsole>(console)
        .is_none_or(|c| now < c.injector_ready)
    {
        return;
    }
    let Some(mob) = modifiable_occupant(world, console) else {
        return;
    };
    let db = db(world);
    let Some(id) = db.by_alias(alias) else {
        return;
    };
    if world
        .get::<Dna>(mob)
        .and_then(|d| d.get_mutation(id))
        .is_none()
        || !world.resource::<DiscoveredMutations>().0.contains(&id)
    {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(console) else {
        return;
    };
    let Some(injector) = crate::construction::spawn_obj(world, ACTIVATOR_PATH, pos) else {
        return;
    };
    let def = &MUTATIONS[id];
    let instability = f64::from(def.instability.abs());
    let (suffix, cooldown) = if activator {
        (
            "activator",
            MIN_ACTIVATOR_SECS.max(instability) * ACTIVATOR_CD_MULT,
        )
    } else {
        (
            "mutator",
            MIN_INJECTOR_SECS.max(instability) * INJECTOR_CD_MULT,
        )
    };
    let name = format!("{} {suffix}", def.name);
    world.entity_mut(injector).insert((
        DnaInjector {
            add: vec![id],
            remove: Vec::new(),
            activator: true,
            force_mutate: !activator,
            used: false,
        },
        Name::new(name.clone()),
    ));
    if let Some(mut n) = world.get_mut::<DmName>(injector) {
        n.name = name.clone();
    }
    if let Some(mut c) = world.get_mut::<DnaConsole>(console) {
        c.injector_ready = now + cooldown;
    }
    tell_client(world, client, Span::Notice, format!("Напечатан {name}."));
}

/// `nullify`: a mutator-added or scrambled mutation goes.
fn nullify(world: &mut World, console: Entity, alias: &str) {
    let Some(mob) = modifiable_occupant(world, console) else {
        return;
    };
    let db = db(world);
    let Some(id) = db.by_alias(alias) else {
        return;
    };
    let Some(m) = world
        .get::<Dna>(mob)
        .and_then(|d| d.get_mutation(id))
        .cloned()
    else {
        return;
    };
    if !m.scrambled && m.sources & SOURCE_MUTATOR == 0 {
        return;
    }
    let sources = if m.scrambled {
        STANDARD_SOURCES
    } else {
        SOURCE_MUTATOR
    };
    remove_mutation(world, mob, id, sources);
}

/// `dnainjector/attack` on a mob: yourself at once, someone else after a `do_after`.
pub fn item_interaction(world: &mut World, actor: Actor, held: Entity, target: Entity) -> bool {
    let Some(injector) = world.get::<DnaInjector>(held).cloned() else {
        return false;
    };
    if world.get::<Health>(target).is_none() {
        return false;
    }
    if injector.used {
        tell_client(
            world,
            actor.client,
            Span::Warning,
            "Этот инъектор использован!",
        );
        return true;
    }
    if target == actor.mob {
        tell_client(
            world,
            actor.client,
            Span::Notice,
            "Вы делаете себе инъекцию.",
        );
        inject_and_spend(world, actor, held, target);
        return true;
    }
    let (user, victim, what) = (
        name_of(world, actor.mob),
        name_of(world, target),
        name_of(world, held),
    );
    crate::chem::visible(
        world,
        target,
        &format!("{user} пытается сделать вам инъекцию: {what}!"),
        &format!("{user} пытается сделать {victim} инъекцию: {what}!"),
        Span::Danger,
    );
    crate::tools::do_after(
        world,
        actor,
        target,
        INJECT_DELAY_DS,
        0,
        crate::construction::Job::Genetics(InjectJob),
    );
    true
}

/// The end of an injection `do_after` (`crate::tools` checked that neither side moved).
pub fn complete(world: &mut World, mob: Entity, action: &crate::tools::DoAfter) {
    let Some(held) = action.item else {
        return;
    };
    let actor = Actor {
        client: action.client,
        mob,
        item: Some(held),
    };
    if world.get::<DnaInjector>(held).is_none_or(|i| i.used) {
        return;
    }
    inject_and_spend(world, actor, held, action.target);
}

fn inject_and_spend(world: &mut World, actor: Actor, held: Entity, target: Entity) {
    if !inject(world, held, target) {
        let who = name_of(world, target);
        tell_client(
            world,
            actor.client,
            Span::Notice,
            format!("Похоже, ДНК {who} несовместима."),
        );
        return;
    }
    if let Some(mut i) = world.get_mut::<DnaInjector>(held) {
        i.used = true;
    }
    // `update_icon_state`: `dnainjector0`.
    let id = world
        .get::<DmAtom>(held)
        .zip(world.get_resource::<IconIndex>())
        .and_then(|(atom, icons)| icons.get(&atom.icon, "dnainjector0"));
    if let Some(id) = id
        && let Some(mut appearance) = world.get_mut::<Appearance>(held)
    {
        appearance.icon_state = id;
    }
}

/// `dnainjector/inject` and `activator/inject`. False when the target cannot mutate.
pub fn inject(world: &mut World, injector: Entity, target: Entity) -> bool {
    let Some(inj) = world.get::<DnaInjector>(injector).cloned() else {
        return false;
    };
    if world.get::<Dna>(target).is_none() || world.get::<Stat>(target) == Some(&Stat::Dead) {
        return false;
    }
    for &id in &inj.remove {
        remove_mutation(world, target, id, STANDARD_SOURCES);
    }
    for &id in &inj.add {
        let in_sequence = world.get::<Dna>(target).is_some_and(|d| d.in_sequence(id));
        if in_sequence {
            add_mutation(world, target, id, SOURCE_ACTIVATED);
        } else if !inj.activator || inj.force_mutate {
            add_mutation(world, target, id, SOURCE_MUTATOR);
        }
    }
    true
}
