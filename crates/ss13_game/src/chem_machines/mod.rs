//! The smaller chemistry machines, after tg `code/modules/reagents/chemistry/machinery/`:
//! the smoke machine ([`smoke_machine`]), the distillation apparatus ([`separator`]), the
//! high-performance liquid chromatography machine ([`mass_spec`]), the debug chemical synthesizer
//! ([`synthesizer`]), the chemical reaction tester ([`recipe_debug`]) and the PanD.E.M.I.C 2200
//! ([`pandemic`]). This module holds their windows' data, which both halves share; `client`
//! draws them.
//!
//! The synthesizer and the reaction tester are admin tools (tg: debug machines only admins can
//! spawn): here every click and act on them also needs the client's `R_ADMIN`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod mass_spec;
#[cfg(feature = "server")]
pub mod pandemic;
#[cfg(feature = "server")]
pub mod recipe_debug;
#[cfg(feature = "server")]
pub mod separator;
#[cfg(feature = "server")]
pub mod smoke_machine;
#[cfg(feature = "server")]
pub mod synthesizer;

#[cfg(feature = "server")]
pub use server::*;

/// A reagent line of a holder (`list("name" = ..., "volume" = ...)`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ReagentLine {
    pub name: String,
    pub volume: f32,
}

/// tgui `SmokeMachine`.
pub struct SmokeMachineUi;

impl UiInterface for SmokeMachineUi {
    const NAME: &'static str = "SmokeMachine";
    type Data = SmokeMachineData;
    type StaticData = ();
}

/// `/obj/machinery/smoke_machine/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SmokeMachineData {
    pub active: bool,
    pub setting: u32,
    pub max_setting: u32,
    pub max_volume: f32,
    pub current_volume: f32,
    pub contents: Vec<ReagentLine>,
}

/// tgui `ChemSeparator`.
pub struct ChemSeparatorUi;

impl UiInterface for ChemSeparatorUi {
    const NAME: &'static str = "ChemSeparator";
    type Data = SeparatorData;
    type StaticData = SeparatorStatic;
}

/// `ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SeparatorStatic {
    pub condenser_installed: bool,
    pub max_burner_knob_settings: u32,
}

/// A holder bar (`RegHolderData`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HolderBar {
    pub total_volume: f32,
    pub maximum_volume: f32,
    pub temp: f32,
    pub color: [u8; 3],
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SeparatorData {
    pub flask: HolderBar,
    pub beaker: Option<HolderBar>,
    pub fuel: Option<HolderBar>,
    pub burner_on: bool,
    pub knob: u32,
    pub condenser_on: bool,
}

/// tgui `MassSpec`.
pub struct MassSpecUi;

impl UiInterface for MassSpecUi {
    const NAME: &'static str = "MassSpec";
    type Data = MassSpecData;
    type StaticData = ();
}

/// A `beakerContents` entry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MassSpecReagent {
    pub name: String,
    pub volume: f32,
    pub mass: f32,
    /// Percent.
    pub purity: f32,
    /// "Clean" or "Inverted".
    pub kind: String,
    pub log: String,
}

/// `beaker1`, `beaker2`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MassSpecBeaker {
    pub current_volume: f32,
    pub max_volume: f32,
    pub contents: Vec<MassSpecReagent>,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MassSpecData {
    pub lower_range: f32,
    pub upper_range: f32,
    pub processing: bool,
    /// Seconds.
    pub eta: f32,
    pub peak_height: f32,
    pub graph_upper_range: f32,
    pub has_beaker_in_hand: bool,
    pub beaker1: Option<MassSpecBeaker>,
    pub beaker2: Option<MassSpecBeaker>,
}

/// tgui `ChemDebugSynthesizer`.
pub struct ChemSynthesizerUi;

impl UiInterface for ChemSynthesizerUi {
    const NAME: &'static str = "ChemDebugSynthesizer";
    type Data = SynthesizerData;
    type StaticData = ();
}

/// The `BeakerDisplay` of the synthesizer and the reaction tester.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BeakerView {
    pub max_volume: f32,
    pub current_volume: f32,
    pub ph: f32,
    pub contents: Vec<ReagentLine>,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SynthesizerData {
    pub amount: f32,
    pub temp: f32,
    /// Percent.
    pub purity: f32,
    pub beaker: Option<BeakerView>,
}

/// tgui `ChemRecipeDebug`.
pub struct RecipeDebugUi;

impl UiInterface for RecipeDebugUi {
    const NAME: &'static str = "ChemRecipeDebug";
    type Data = RecipeDebugData;
    type StaticData = ();
}

/// The temperature modes (`USE_*_TEMPERATURE`), in the window's order.
pub const TEMP_MODES: [&str; 5] = [
    "Reaction Temp",
    "Forced Temp",
    "Minimum Temp",
    "Optimal Temp",
    "Overheat Temp",
];
/// The reaction modes (`PLAY_*_REACTION`).
pub const REACTION_MODES: [&str; 3] = ["Next Reaction", "Previous Reaction", "Pick Reaction"];
/// The editable reaction vars as the window names them.
pub const EDIT_VARS: [&str; 12] = [
    "Required Temp",
    "Optimal Temp",
    "Overheat Temp",
    "Optimal Min Ph",
    "Optimal Max Ph",
    "Ph Range",
    "Temp Exp Factor",
    "Ph Exp Factor",
    "Thermic Constant",
    "H Ion Release",
    "Rate Up Limit",
    "Purity Min",
];

/// An `activeReactions` entry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DebugReaction {
    pub name: String,
    pub danger: bool,
    pub overheat: bool,
    pub purity_alert: bool,
    pub quality: f32,
    pub inverse: f32,
    pub min_pure: f32,
    pub reacted_vol: f32,
    pub target_vol: f32,
}

/// `editReaction`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EditReaction {
    pub name: String,
    pub edit_var: String,
    pub edit_value: f32,
}

/// `beaker` of the tester.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DebugBeaker {
    pub max_volume: f32,
    pub ph: f32,
    pub current_volume: f32,
    pub current_temp: f32,
    pub purity: f32,
    pub contents: Vec<ReagentLine>,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RecipeDebugData {
    pub forced_temp: f32,
    /// Index into [`TEMP_MODES`].
    pub temp_mode: u8,
    pub forced_ph: f32,
    pub use_forced_ph: bool,
    pub forced_purity: f32,
    pub use_forced_purity: bool,
    pub volume_multiplier: f32,
    pub current_reaction_name: String,
    /// Index into [`REACTION_MODES`].
    pub current_reaction_mode: u8,
    pub active_reactions: Vec<DebugReaction>,
    pub is_flashing: bool,
    pub is_reacting: bool,
    pub edit_reaction: Option<EditReaction>,
    pub beaker: Option<DebugBeaker>,
}

/// tgui `Pandemic`.
pub struct PandemicUi;

impl UiInterface for PandemicUi {
    const NAME: &'static str = "Pandemic";
    type Data = PandemicData;
    type StaticData = ();
}

/// A symptom of an advanced virus (`get_symptom_data`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SymptomView {
    pub name: String,
    pub stealth: i32,
    pub resistance: i32,
    pub stage_speed: i32,
    pub transmission: i32,
    pub level: u8,
    pub neutered: bool,
    pub cure: String,
}

/// A `viruses` entry (`get_viruses_data`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VirusView {
    /// 1-based, the `index` param of the culture and rename acts.
    pub index: u32,
    pub name: String,
    pub agent: String,
    pub description: String,
    pub spread: String,
    pub cure: String,
    pub can_rename: bool,
    pub is_adv: bool,
    pub resistance: i32,
    pub stage_speed: i32,
    pub stealth: i32,
    pub transmission: i32,
    pub symptoms: Vec<SymptomView>,
}

/// A `resistances` entry: the disease id is the vaccine act's `index`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ResistanceView {
    pub id: String,
    pub name: String,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PandemicData {
    pub is_ready: bool,
    /// (volume, capacity) of the beaker.
    pub beaker: Option<(f32, f32)>,
    /// The blood sample: (`dna`, `type`).
    pub blood: Option<(String, String)>,
    pub viruses: Vec<VirusView>,
    pub resistances: Vec<ResistanceView>,
}

#[cfg(feature = "server")]
mod server {
    use std::collections::HashMap;

    use bevy::prelude::*;
    use ss13_shared::TilePos;
    use ss13_shared::chat::Span;
    use ss13_tgui::UiInterface;
    use ss13_tgui::server::{UiSystems, UiWindow};

    use super::*;
    use crate::inventory::{CarriedBy, Inventory};
    use crate::storage::{StoredIn, put_inside, take_out};

    /// The windows' data per user (tg `ui_data(user)`), built with the world by
    /// [`refresh_views`]; a window's `ServerUi::data` reads it.
    #[derive(Component)]
    pub struct Views<D: Send + Sync + 'static>(pub HashMap<Entity, D>);

    impl<D: Send + Sync + 'static> Default for Views<D> {
        fn default() -> Self {
            Self(HashMap::new())
        }
    }

    impl<D: Clone + Default + Send + Sync + 'static> Views<D> {
        pub fn of(&self, user: Entity) -> D {
            self.0.get(&user).cloned().unwrap_or_default()
        }
    }

    /// Each open `U` window's data from `build(world, source, user)`.
    // Cost: one `build` per open window of the kind per tick (a few at most).
    pub(crate) fn refresh_views<U: UiInterface>(
        world: &mut World,
        build: fn(&mut World, Entity, Entity) -> Option<U::Data>,
    ) where
        U::Data: PartialEq + Send + Sync + 'static,
    {
        let mut windows = world.query::<&UiWindow>();
        let open: Vec<(Entity, Entity)> = windows
            .iter(world)
            .filter(|w| w.interface == U::NAME)
            .map(|w| (w.source, w.actor))
            .collect();
        let mut by_source: HashMap<Entity, HashMap<Entity, U::Data>> = HashMap::new();
        for (source, actor) in open {
            if world.get::<Views<U::Data>>(source).is_none() {
                continue;
            }
            if let Some(data) = build(world, source, actor) {
                by_source.entry(source).or_default().insert(actor, data);
            }
        }
        let mut q = world.query::<(Entity, &mut Views<U::Data>)>();
        for (e, mut views) in q.iter_mut(world) {
            let fresh = by_source.remove(&e).unwrap_or_default();
            if views.0 != fresh {
                views.0 = fresh;
            }
        }
    }

    /// `item`, if it is still inside `machine` (tg `Exited` clears the slot otherwise).
    pub(crate) fn inside(world: &World, machine: Entity, item: Option<Entity>) -> Option<Entity> {
        let item = item?;
        world
            .get::<StoredIn>(item)
            .filter(|s| s.0 == machine)
            .map(|_| item)
    }

    /// `user.transferItemToLoc(item, machine)`.
    pub(crate) fn take_in(world: &mut World, mob: Entity, machine: Entity, item: Entity) {
        if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
            && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(item))
        {
            *slot = None;
        }
        world.entity_mut(item).remove::<CarriedBy>();
        put_inside(world, item, machine);
    }

    /// `temporarilyRemoveItemFromInventory` and `qdel` of a held item.
    pub(crate) fn consume_held(world: &mut World, mob: Entity, item: Entity) {
        if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
            && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(item))
        {
            *slot = None;
        }
        world.despawn(item);
    }

    /// tg `try_put_in_hand`: out of `machine` into a free hand of `mob` next to it, else onto
    /// the machine's tile.
    pub(crate) fn hand_out(world: &mut World, mob: Option<Entity>, machine: Entity, item: Entity) {
        if let Some(&pos) = world.get::<TilePos>(machine) {
            take_out(world, item, pos);
        }
        let Some(mob) = mob.filter(|&m| crate::storage::in_reach(world, m, machine)) else {
            return;
        };
        if let Some(inventory) = world.get::<Inventory>(mob).cloned()
            && let Some(hand) = [inventory.active, 1 - inventory.active]
                .into_iter()
                .find(|&h| inventory.hands[h].is_none())
        {
            crate::inventory::pick_up(world, mob, item, hand);
        }
    }

    /// The client's active `R_ADMIN` (kept by `ss13_admin`).
    pub(crate) fn is_admin(world: &World, client: Entity) -> bool {
        world
            .get::<crate::votes::VoteRights>(client)
            .is_some_and(|r| r.admin)
    }

    /// The active hand's item.
    pub(crate) fn held_item(world: &World, mob: Entity) -> Option<Entity> {
        world.get::<Inventory>(mob).and_then(Inventory::active_item)
    }

    /// `(name, volume)` lines of a holder, rounded to `CHEMICAL_VOLUME_ROUNDING`.
    pub(crate) fn lines(
        chem: &ss13_reagents::Chemistry,
        h: &ss13_reagents::Holder,
    ) -> Vec<ReagentLine> {
        h.reagents
            .iter()
            .map(|r| ReagentLine {
                name: chem.def(r.id).name.clone(),
                volume: ss13_reagents::round_to(r.volume, ss13_reagents::CHEMICAL_VOLUME_ROUNDING),
            })
            .collect()
    }

    /// `name` of an atom without its article.
    pub(crate) fn title(world: &World, e: Entity) -> String {
        world
            .get::<ss13_mapload::DmName>(e)
            .map(|n| crate::vending::plain_name(&n.name).to_owned())
            .unwrap_or_default()
    }

    /// tg `say` of a machine.
    pub(crate) fn say(world: &mut World, machine: Entity, text: &str) {
        crate::cargo::console::machine_say(world, machine, text);
    }

    pub(crate) fn notice(world: &mut World, client: Entity, text: impl Into<String>) {
        crate::tools::tell(world, client, Span::Notice, text);
    }

    /// tg `/datum/reagent/var/mass`: `rand(10, 800)` per reagent type, rolled once a round.
    #[derive(Resource, Clone)]
    pub struct ReagentMasses(pub std::sync::Arc<Vec<f32>>);

    /// The mass of every reagent, by [`ss13_reagents::ReagentId`].
    pub(crate) fn masses(world: &mut World) -> std::sync::Arc<Vec<f32>> {
        let count = world
            .get_resource::<crate::chem::Chem>()
            .map_or(0, |c| c.0.reagents.len());
        if let Some(m) = world.get_resource::<ReagentMasses>()
            && m.0.len() == count
        {
            return m.0.clone();
        }
        let mut rng = crate::medical::fork_rng(world);
        let table: std::sync::Arc<Vec<f32>> =
            std::sync::Arc::new((0..count).map(|_| rng.range(10, 800) as f32).collect());
        world.insert_resource(ReagentMasses(table.clone()));
        table
    }

    /// What a prompt opened by one of these machines was for (on the prompt entity).
    #[derive(Component, Clone, Debug)]
    pub(crate) struct MachinePrompt {
        pub machine: Entity,
        pub asked: Asked,
    }

    #[derive(Clone, Debug)]
    pub(crate) enum Asked {
        /// The synthesizer's `input`: the listed reagents.
        SynthReagent(Vec<ss13_reagents::ReagentId>),
        /// The tester's `pick_reaction`: "All" or "Specific".
        DebugAllOrOne,
        /// The tester's reaction list (`pick_reaction`, `edit_reaction`): reaction indexes.
        DebugReaction { edit: bool, listed: Vec<u32> },
        /// The tester's `PLAY_USER_REACTION`: indexes into its test list.
        DebugPlay,
    }

    /// `tgui_input_list` / `tgui_alert` for `machine`.
    pub(crate) fn ask(
        world: &mut World,
        client: Entity,
        actor: Entity,
        machine: Entity,
        title: &str,
        kind: ss13_tgui::prompt::PromptKind,
        asked: Asked,
    ) {
        let prompt = ss13_tgui::prompt::ask_world(
            world,
            ss13_tgui::prompt::Ask::new(client, actor, title, title, kind),
        );
        world
            .entity_mut(prompt)
            .insert(MachinePrompt { machine, asked });
    }

    /// The answers to [`MachinePrompt`]s, checked again at the moment of effect.
    fn prompt_answers(
        world: &mut World,
        mut cursor: Local<bevy::ecs::message::MessageCursor<ss13_tgui::prompt::PromptAnswered>>,
    ) {
        use ss13_tgui::prompt::{PromptAnswer, PromptAnswered};
        let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
            .read(world.resource::<bevy::ecs::message::Messages<PromptAnswered>>())
            .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
            .collect();
        for (prompt, client, actor, answer) in answers {
            let Some(asked) = world.get::<MachinePrompt>(prompt).cloned() else {
                continue;
            };
            let Some(answer) = answer else {
                continue;
            };
            if !is_admin(world, client) || !crate::storage::in_reach(world, actor, asked.machine) {
                continue;
            }
            match asked.asked {
                Asked::SynthReagent(listed) => {
                    if let PromptAnswer::Item(i) = answer
                        && let Some(&id) = listed.get(i)
                    {
                        synthesizer::add(world, asked.machine, id);
                    }
                }
                other => recipe_debug::answer(world, client, actor, asked.machine, other, answer),
            }
        }
    }

    /// tg `message_admins` (delivered by `ss13_admin`).
    pub(crate) fn admin_notice(world: &mut World, text: String) {
        if world.contains_resource::<bevy::ecs::message::Messages<crate::votes::AdminNotice>>() {
            world.write_message(crate::votes::AdminNotice(text));
        }
    }

    pub struct ChemMachinesPlugin;

    impl Plugin for ChemMachinesPlugin {
        fn build(&self, app: &mut App) {
            // Click chain hooks (`crate::click_handlers`).
            use crate::click_handlers::{AddClickHandler, ClickPhase};
            use ss13_tgui::server::AppTguiExt;
            // Before `chem_dispenser::interact`: the synthesizer is a dispenser subtype.
            app.add_click_handler(ClickPhase::EmptyHand, 1295, |w, c| {
                interact(w, c.client, c.mob, c.target, c.modifiers.right)
            });
            // Before `machine_frame` (the machines refuse tools while working).
            app.add_click_handler(ClickPhase::HeldOnTarget, 1535, |w, c| {
                c.held
                    .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target, c.modifiers))
            });
            app.register_ui::<SmokeMachineUi>()
                .register_ui::<ChemSeparatorUi>()
                .register_ui::<MassSpecUi>()
                .register_ui::<ChemSynthesizerUi>()
                .register_ui::<RecipeDebugUi>()
                .register_ui::<PandemicUi>()
                .init_resource::<pandemic::DiseaseNames>()
                .add_systems(
                    Update,
                    (
                        smoke_machine::acts,
                        separator::acts,
                        mass_spec::acts,
                        synthesizer::acts,
                        recipe_debug::acts,
                        pandemic::acts,
                    )
                        .in_set(UiSystems::Handle),
                )
                .add_systems(
                    Update,
                    prompt_answers
                        .in_set(UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
                )
                .add_systems(
                    Update,
                    (
                        smoke_machine::process.after(crate::power::PowerSystems),
                        separator::process,
                        // The separator's `soundloop` while `boiling`.
                        crate::looping_sound::sync::<separator::Separator>(
                            &crate::looping_sound::defs::BOILING,
                            |s| s.boiling,
                        ),
                        mass_spec::process.after(crate::power::PowerSystems),
                        recipe_debug::process,
                        pandemic::process,
                    ),
                )
                .add_systems(
                    Update,
                    (
                        |w: &mut World| refresh_views::<SmokeMachineUi>(w, smoke_machine::view),
                        |w: &mut World| refresh_views::<ChemSeparatorUi>(w, separator::view),
                        |w: &mut World| refresh_views::<MassSpecUi>(w, mass_spec::view),
                        |w: &mut World| refresh_views::<ChemSynthesizerUi>(w, synthesizer::view),
                        |w: &mut World| refresh_views::<RecipeDebugUi>(w, recipe_debug::view),
                        |w: &mut World| refresh_views::<PandemicUi>(w, pandemic::view),
                    )
                        .after(UiSystems::Handle)
                        .before(UiSystems::Send),
                );
        }
    }

    /// The machine's state, set up on first use when it has none yet (mapped, built from a
    /// frame or spawned by an admin). False: not one of these machines.
    pub(crate) fn ensure(world: &mut World, e: Entity) -> bool {
        smoke_machine::ensure(world, e)
            || separator::ensure(world, e)
            || mass_spec::ensure(world, e)
            || synthesizer::ensure(world, e)
            || recipe_debug::ensure(world, e)
            || pandemic::ensure(world, e)
    }

    /// `attack_hand` (left) and `attack_hand_secondary` (right). True when it was ours.
    pub fn interact(
        world: &mut World,
        client: Entity,
        mob: Entity,
        target: Entity,
        right: bool,
    ) -> bool {
        if !ensure(world, target) {
            return false;
        }
        smoke_machine::interact(world, client, mob, target, right)
            || separator::interact(world, client, mob, target, right)
            || mass_spec::interact(world, client, mob, target, right)
            || synthesizer::interact(world, client, mob, target, right)
            || recipe_debug::interact(world, client, mob, target, right)
            || pandemic::interact(world, client, mob, target, right)
    }

    /// `item_interaction` and the tool acts. True when it was ours.
    pub fn item_interaction(
        world: &mut World,
        client: Entity,
        mob: Entity,
        held: Entity,
        target: Entity,
        modifiers: ss13_shared::protocol::ClickModifiers,
    ) -> bool {
        if !ensure(world, target) {
            return false;
        }
        smoke_machine::item_interaction(world, client, mob, held, target)
            || separator::item_interaction(world, client, mob, held, target, modifiers.right)
            || mass_spec::item_interaction(world, client, mob, held, target, modifiers.right)
            || synthesizer::item_interaction(world, client, mob, held, target)
            || recipe_debug::item_interaction(world, client, mob, held, target)
            || pandemic::item_interaction(world, client, mob, held, target, modifiers)
    }

    /// tg `click_alt` (from `crate::loot`). True when it was ours.
    pub fn click_alt(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
        if !ensure(world, target) {
            return false;
        }
        separator::click_alt(world, target) || mass_spec::click_alt(world, client, mob, target)
    }

    /// Examine lines (from `crate::examine`).
    pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
        smoke_machine::examine_lines(world, target, lines);
        separator::examine_lines(world, target, lines);
        mass_spec::examine_lines(world, target, lines);
        pandemic::examine_lines(world, target, lines);
    }

    /// `RefreshParts` (from `crate::stock_parts::refresh`).
    pub fn refresh_parts(world: &mut World, e: Entity) {
        smoke_machine::refresh_parts(world, e);
        mass_spec::refresh_parts(world, e);
    }
}
