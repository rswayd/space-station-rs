//! Bar signs (server): tg `/obj/machinery/barsign` (`code/game/machinery/barsigns.dm`) with
//! BandaStation's own signs (`modular_bandastation/barsigns`, `ss220_icon`).
//!
//! A sign starts off. An empty-hand click by someone whose ID has its `req_access` (the bar)
//! asks which sign to show (tg's sorted `tgui_input_list` of the signs that are not hidden);
//! the sign takes its sprite, `bar sign (<name>)` and its description. Without power it shows
//! nothing ("empty") and does not listen.
// shortcut: no renaming of the bar's area, neon light, emissive mask, emag signs,
// breaking, cable repair or taking it off the wall.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmBarsign, DmName, IconIndex};
use ss13_shared::Appearance;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use crate::inventory::{IdCard, Inventory};
use crate::power::Unpowered;
use crate::tools::tell;

pub const BARSIGN_ROOT: &str = ss13_mapload::barsign::BARSIGN_ROOT;
/// `/datum/barsign/hiddensigns/signoff`.
const SIGN_OFF: &str = "Off";
/// `initial(name)` of the machine.
const BASE_NAME: &str = "bar sign";

/// The shown sign (`chosen_sign`), by name.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Barsign {
    pub sign: String,
}

/// On a prompt: the sign it picks for.
#[derive(Component, Clone, Copy, Debug)]
struct PickFor(Entity);

pub struct BarsignPlugin;

impl Plugin for BarsignPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_hand` of a bar sign: pick the sign.
        app.add_click_handler(ClickPhase::EmptyHand, 1100, |w, c| {
            crate::barsign::interact(w, c.client, c.mob, c.target)
        });
        app.add_systems(Startup, setup_mapped)
            .add_systems(
                Update,
                pick_answers
                    .in_set(ss13_tgui::server::UiSystems::Handle)
                    .after(ss13_tgui::prompt::PromptSystems::Answer),
            )
            .add_observer(power_lost)
            .add_observer(power_back);
    }
}

fn setup_mapped(world: &mut World) {
    let mut q = world.query_filtered::<Entity, With<DmBarsign>>();
    let signs: Vec<Entity> = q.iter(world).collect();
    for e in signs {
        set_sign(world, e, SIGN_OFF);
    }
}

fn power_lost(add: On<Add<Unpowered>>, signs: Query<(), With<Barsign>>, mut commands: Commands) {
    if signs.contains(add.entity) {
        let e = add.entity;
        commands.queue(move |world: &mut World| refresh_look(world, e));
    }
}

fn power_back(
    remove: On<Remove<Unpowered>>,
    signs: Query<(), With<Barsign>>,
    mut commands: Commands,
) {
    if signs.contains(remove.entity) {
        let e = remove.entity;
        commands.queue(move |world: &mut World| refresh_look(world, e));
    }
}

/// tg `allowed(user)`: the mob's ID has the sign's `req_access`.
fn allowed(world: &World, mob: Entity, e: Entity) -> bool {
    let req = world
        .get::<DmBarsign>(e)
        .map(|b| b.req_access.clone())
        .unwrap_or_default();
    let access = world
        .get::<Inventory>(mob)
        .and_then(|i| i.id_card(world))
        .and_then(|c| world.get::<IdCard>(c))
        .map(|c| c.access.clone())
        .unwrap_or_default();
    crate::inventory::check_access(&req, &[], &access)
}

/// `attack_hand`. True when the click was the sign's.
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity) -> bool {
    let Some(dm) = world.get::<DmBarsign>(e).cloned() else {
        return false;
    };
    if !allowed(world, mob, e) {
        tell(world, client, Span::Warning, "в доступе отказано!");
        return true;
    }
    if world.get::<Unpowered>(e).is_some() || world.get::<crate::emp::Emped>(e).is_some() {
        tell(world, client, Span::Warning, "controls are unresponsive!");
        return true;
    }
    let mut items: Vec<String> = dm
        .signs
        .iter()
        .filter(|s| !s.hidden)
        .map(|s| s.name.clone())
        .collect();
    items.sort();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Bar Sign",
            "Available Signage",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(PickFor(e));
    true
}

/// The picks, revalidated (still in reach, conscious, allowed, powered).
fn pick_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(&PickFor(e)) = world.get::<PickFor>(prompt) else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let able = world.get_entity(e).is_ok()
            && world.get::<crate::health::Stat>(actor) == Some(&crate::health::Stat::Conscious)
            && crate::storage::in_reach(world, actor, e)
            && allowed(world, actor, e)
            && world.get::<Unpowered>(e).is_none();
        if !able {
            continue;
        }
        let Some(dm) = world.get::<DmBarsign>(e).cloned() else {
            continue;
        };
        let mut items: Vec<String> = dm
            .signs
            .iter()
            .filter(|s| !s.hidden)
            .map(|s| s.name.clone())
            .collect();
        items.sort();
        if let Some(name) = items.get(i) {
            set_sign(world, e, name);
        }
    }
}

/// `set_sign`: the name and description follow the sign (`update_name`, `update_desc`).
pub fn set_sign(world: &mut World, e: Entity, name: &str) {
    let Some(sign) = world
        .get::<DmBarsign>(e)
        .and_then(|b| b.signs.iter().find(|s| s.name == name).cloned())
    else {
        return;
    };
    // Hidden signs (off, EMP, syndicate) do not rename (`rename_area = FALSE`).
    let title = if sign.hidden {
        BASE_NAME.to_owned()
    } else {
        format!("{BASE_NAME} ({})", sign.name)
    };
    world
        .entity_mut(e)
        .insert((Barsign { sign: sign.name }, Name::new(title)));
    if let Some(mut dm) = world.get_mut::<DmName>(e) {
        dm.desc = sign.desc;
    }
    refresh_look(world, e);
}

/// `update_icon_state`: the sign's sprite, "empty" without power.
fn refresh_look(world: &mut World, e: Entity) {
    let Some(name) = world.get::<Barsign>(e).map(|b| b.sign.clone()) else {
        return;
    };
    let sign = world
        .get::<DmBarsign>(e)
        .and_then(|b| b.signs.iter().find(|s| s.name == name).cloned());
    let own_icon = world
        .get::<DmAtom>(e)
        .map(|a| a.icon.clone())
        .unwrap_or_default();
    let (icon, state) = match sign.filter(|_| world.get::<Unpowered>(e).is_none()) {
        Some(s) if !s.icon_state.is_empty() => {
            let icon = if s.icon.is_empty() { own_icon } else { s.icon };
            (icon, s.icon_state)
        }
        _ => (own_icon, "empty".to_owned()),
    };
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&icon, &state));
    if let Some(id) = id
        && let Some(mut look) = world.get_mut::<Appearance>(e)
        && look.icon_state != id
    {
        look.icon_state = id;
    }
}
