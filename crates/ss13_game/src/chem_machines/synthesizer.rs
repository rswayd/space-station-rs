//! The debug chemical synthesizer (server): tg `/obj/machinery/chem_dispenser/chem_synthesizer`
//! (`chem_synthesizer.dm`) and its `ChemDebugSynthesizer` window. An admin tool: every click,
//! act and answer needs the client's `R_ADMIN`.
//!
//! - An open container clicked on it goes in (the old one to the hands); a right click takes it
//!   out; without one `makecup` makes a bluespace beaker inside.
//! - `input` lists every reagent by name; the chosen one is added to the beaker: `amount` units
//!   (1 to the beaker's volume) at `temp` (0-1000 K) and `purity` (0-120 %).
//! - Unpowered, indestructible; screwdrivers and crowbars do nothing (`tool_blocker`).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmObj, DmReagents};
use ss13_reagents::{DEFAULT_REAGENT_TEMPERATURE, ReagentId, round_to};
use ss13_shared::chat::Span;
use ss13_tgui::prompt::PromptKind;
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject};

use super::*;
use crate::chem::{Chem, Reagents};
use crate::obj_damage::is_under;
use crate::tools::{ToolKind, tool_of};

pub const SYNTHESIZER: &str = "/obj/machinery/chem_dispenser/chem_synthesizer";
pub const BLUESPACE_BEAKER: &str = "/obj/item/reagent_containers/cup/beaker/bluespace";
const NOT_ADMIN: &str = "Если вы это видите — позовите администрацию.";

#[derive(Component, Clone, Debug, PartialEq)]
#[require(Views<SynthesizerData>)]
pub struct Synthesizer {
    pub beaker: Option<Entity>,
    pub amount: f32,
    pub temperature: f32,
    /// Percent.
    pub purity: f32,
}

pub(super) fn ensure(world: &mut World, e: Entity) -> bool {
    if world.get::<Synthesizer>(e).is_some() {
        return true;
    }
    if !world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, SYNTHESIZER))
    {
        return false;
    }
    world.entity_mut(e).insert(Synthesizer {
        beaker: None,
        amount: 10.0,
        temperature: DEFAULT_REAGENT_TEMPERATURE,
        purity: 100.0,
    });
    true
}

fn beaker(world: &World, e: Entity) -> Option<Entity> {
    inside(world, e, world.get::<Synthesizer>(e)?.beaker)
}

fn refuse(world: &mut World, client: Entity) {
    crate::tools::tell(world, client, Span::Warning, NOT_ADMIN);
}

pub(super) fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    e: Entity,
    right: bool,
) -> bool {
    if world.get::<Synthesizer>(e).is_none() {
        return false;
    }
    if !is_admin(world, client) {
        refuse(world, client);
        return true;
    }
    if right {
        if let Some(b) = beaker(world, e) {
            hand_out(world, Some(mob), e, b);
        }
    } else {
        world.write_message(OpenUi::new::<ChemSynthesizerUi>(
            client,
            mob,
            e,
            title(world, e),
        ));
    }
    true
}

pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
) -> bool {
    if world.get::<Synthesizer>(e).is_none() {
        return false;
    }
    if tool_of(world, held)
        .is_some_and(|(k, _)| matches!(k, ToolKind::Screwdriver | ToolKind::Crowbar))
    {
        return true;
    }
    let open_container = world.get::<DmReagents>(held).is_some()
        && world
            .get::<Reagents>(held)
            .is_some_and(|r| r.0.is_open_container());
    if !open_container {
        return false;
    }
    if !is_admin(world, client) {
        refuse(world, client);
        return true;
    }
    if let Some(old) = beaker(world, e) {
        hand_out(world, Some(mob), e, old);
    }
    take_in(world, mob, e, held);
    if let Some(mut s) = world.get_mut::<Synthesizer>(e) {
        s.beaker = Some(held);
    }
    world.write_message(OpenUi::new::<ChemSynthesizerUi>(
        client,
        mob,
        e,
        title(world, e),
    ));
    true
}

/// The `input` answer: `amount` of the reagent into the beaker.
pub(super) fn add(world: &mut World, e: Entity, id: ReagentId) {
    let Some(s) = world.get::<Synthesizer>(e).cloned() else {
        return;
    };
    let Some(b) = beaker(world, e) else {
        return;
    };
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    if let Some(mut r) = world.get_mut::<Reagents>(b) {
        r.0.add_reagent(
            &chem,
            id,
            s.amount,
            s.temperature,
            Some(s.purity / 100.0),
            None,
            false,
            false,
        );
    }
}

pub(super) fn view(world: &mut World, e: Entity, _user: Entity) -> Option<SynthesizerData> {
    let s = world.get::<Synthesizer>(e)?.clone();
    let chem = world.get_resource::<Chem>()?.0.clone();
    let beaker = beaker(world, e)
        .and_then(|b| world.get::<Reagents>(b))
        .map(|r| BeakerView {
            max_volume: r.0.maximum_volume,
            current_volume: r.0.total_volume,
            ph: round_to(r.0.ph, 0.01),
            contents: lines(&chem, &r.0),
        });
    Some(SynthesizerData {
        amount: s.amount,
        temp: s.temperature,
        purity: s.purity,
        beaker,
    })
}

impl ServerUi for ChemSynthesizerUi {
    type Source = &'static Views<SynthesizerData>;
    type UserParam = ();
    type Act = SynthAct;

    fn static_data(_: &Views<SynthesizerData>) {}

    fn data(views: &Views<SynthesizerData>, user: Entity, _: &()) -> SynthesizerData {
        views.of(user)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<SynthAct, UiReject> {
        let amount = || params.number("amount").map(|v| v as f32);
        Ok(match action {
            "input" => SynthAct::Input,
            "makecup" => SynthAct::MakeCup,
            "amount" => SynthAct::Amount(amount()?),
            "temp" => SynthAct::Temp(amount()?),
            "purity" => SynthAct::Purity(amount()?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SynthAct {
    Input,
    MakeCup,
    Amount(f32),
    Temp(f32),
    Purity(f32),
}

pub(super) fn acts(
    world: &mut World,
    mut cursor: Local<MessageCursor<UiActed<ChemSynthesizerUi>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, SynthAct)> = cursor
        .read(world.resource::<Messages<UiActed<ChemSynthesizerUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, e, act) in pending {
        apply_act(world, client, actor, e, act);
    }
}

/// `handle_ui_act`, admins only; values within the window's limits.
pub fn apply_act(world: &mut World, client: Entity, actor: Entity, e: Entity, act: SynthAct) {
    if world.get::<Synthesizer>(e).is_none() || !is_admin(world, client) {
        return;
    }
    let max = beaker(world, e)
        .and_then(|b| world.get::<Reagents>(b))
        .map_or(0.0, |r| r.0.maximum_volume);
    let set = |world: &mut World, f: &dyn Fn(&mut Synthesizer)| {
        if let Some(mut s) = world.get_mut::<Synthesizer>(e) {
            f(&mut *s);
        }
    };
    match act {
        SynthAct::Input => {
            if beaker(world, e).is_none() {
                return;
            }
            let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
                return;
            };
            let mut ids: Vec<ReagentId> = (0..chem.reagents.len() as ReagentId).collect();
            ids.sort_by(|a, b| chem.def(*a).name.cmp(&chem.def(*b).name));
            ids.dedup_by(|a, b| chem.def(*a).name == chem.def(*b).name);
            let items = ids.iter().map(|&id| chem.def(id).name.clone()).collect();
            ask(
                world,
                client,
                actor,
                e,
                "Select reagent",
                PromptKind::List {
                    items,
                    default: None,
                },
                Asked::SynthReagent(ids),
            );
        }
        SynthAct::MakeCup => {
            if beaker(world, e).is_some() {
                return;
            }
            let Some(cup) = crate::construction::spawn_detached(world, BLUESPACE_BEAKER) else {
                return;
            };
            crate::storage::put_inside(world, cup, e);
            set(world, &|s| s.beaker = Some(cup));
            let name = title(world, e);
            notice(world, client, format!("{name} выдаёт блюспейс-мензурку."));
        }
        SynthAct::Amount(v) if v.is_finite() && max > 0.0 => {
            set(world, &|s| s.amount = v.clamp(1.0, max));
        }
        SynthAct::Temp(v) if v.is_finite() => set(world, &|s| s.temperature = v.clamp(0.0, 1000.0)),
        SynthAct::Purity(v) if v.is_finite() => set(world, &|s| s.purity = v.clamp(0.0, 120.0)),
        SynthAct::Amount(_) | SynthAct::Temp(_) | SynthAct::Purity(_) => {}
    }
}
