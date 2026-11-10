//! The implant pad (server), after tg `/obj/item/implantpad` (`implantpad.dm`) and its
//! `ImplantPad` window (`ss13_tgui::interfaces::implant_pad`).
//!
//! - An implant case clicked onto a pad without one goes in (`implantpad-1`); the window's
//!   "Eject Case" puts it into a free hand of the user, else at the user's feet.
//! - Using the pad in hand opens the window: the case's implant and its `implant_info`.
//! - Deathrattle groups: "Initialize Group" makes a new group (expeditionary for the
//!   expeditionary implant), registers the case's group-less deathrattle into it, saves it on the
//!   pad and names the case after it; "Save Group" saves the case's implant's group; "Set Group"
//!   registers the case's implant into the saved group when the kinds match.
// shortcut: no alt-click eject (the window's button does it), no examine lines.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::implant_pad::{
    ACT_EJECT, ACT_INIT_GROUP, ACT_SAVE_GROUP, ACT_SET_GROUP, ImplantPadData, ImplantPadUi,
};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiKind, UiParams, UiReject, UiSystems, UiWindow,
};

use super::{
    DEATHRATTLE_LAVALAND, DeathrattleGroups, ImplantHolder, ImplantKind, PAD_ROOT,
    new_deathrattle_group,
};
use crate::inventory::{CarriedBy, Inventory};
use crate::storage::StoredIn;

/// `/obj/item/implantpad` vars: `inserted_case`, `saved_deathrattle_group`, and the window's
/// data last read.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ImplantPad {
    pub case: Option<Entity>,
    pub saved_group: Option<u32>,
    view: ImplantPadData,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // `attack_self` -> `ui_interact`: the pad's window.
    app.add_click_handler(ClickPhase::AttackSelf, 3335, |w, c| {
        crate::implants::pad::attack_self(w, c.client, c.mob, c.target)
    });
    app.register_ui::<PadWindow>()
        .add_observer(attach)
        .add_systems(
            Update,
            (
                acts.in_set(UiSystems::Handle),
                refresh_views
                    .after(UiSystems::Handle)
                    .before(UiSystems::Send),
            ),
        );
}

fn attach(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, PAD_ROOT))
    {
        commands
            .entity(add.entity)
            .insert_if_new(ImplantPad::default());
    }
}

/// The pad's case, cleared when it left the pad (`Exited`).
fn case_of(world: &mut World, pad: Entity) -> Option<Entity> {
    let case = world.get::<ImplantPad>(pad)?.case?;
    if world.get::<StoredIn>(case).is_some_and(|s| s.0 == pad) {
        return Some(case);
    }
    if let Some(mut p) = world.get_mut::<ImplantPad>(pad) {
        p.case = None;
    }
    refresh_look(world, pad);
    None
}

/// `update_icon_state`: `implantpad-0`/`-1`.
fn refresh_look(world: &mut World, pad: Entity) {
    let full = world
        .get::<ImplantPad>(pad)
        .is_some_and(|p| p.case.is_some());
    crate::obj_damage::set_icon_state(world, pad, &format!("implantpad-{}", u8::from(full)));
}

/// `implantpad/item_interaction`: a case from the user's hand into a pad without one. True when
/// it went in.
pub fn insert_case(world: &mut World, user: Entity, case: Entity, pad: Entity) -> bool {
    if world.get::<ImplantPad>(pad).is_none()
        || case_of(world, pad).is_some()
        || world.get::<ImplantHolder>(case).is_none_or(|h| !h.case)
    {
        return false;
    }
    {
        let Some(mut inventory) = world.get_mut::<Inventory>(user) else {
            return false;
        };
        let Some(hand) = inventory.hands.iter().position(|h| *h == Some(case)) else {
            return false;
        };
        inventory.hands[hand] = None;
    }
    world.entity_mut(case).remove::<CarriedBy>();
    crate::storage::put_inside(world, case, pad);
    if let Some(mut p) = world.get_mut::<ImplantPad>(pad) {
        p.case = Some(case);
    }
    crate::balloon::balloon_alert(world, user, pad, "кейс вставлен");
    refresh_look(world, pad);
    refresh_view(world, pad);
    true
}

/// `remove_implant(user)`: the case into a free hand of the user, else at its feet.
fn eject(world: &mut World, user: Entity, pad: Entity) {
    let Some(case) = case_of(world, pad) else {
        crate::balloon::balloon_alert(world, user, pad, "внутри нет кейса!");
        return;
    };
    if let Some(mut p) = world.get_mut::<ImplantPad>(pad) {
        p.case = None;
    }
    let hand = world.get::<Inventory>(user).and_then(|i| {
        [i.active, 1 - i.active]
            .into_iter()
            .find(|&h| i.hands[h].is_none())
    });
    match hand {
        Some(h) => {
            world
                .entity_mut(case)
                .remove::<StoredIn>()
                .insert(CarriedBy(user));
            if let Some(mut inventory) = world.get_mut::<Inventory>(user) {
                inventory.hands[h] = Some(case);
            }
        }
        None => {
            if let Some(pos) = crate::explosions::turf_of(world, user) {
                crate::storage::take_out(world, case, pos);
            }
        }
    }
    crate::inventory::refresh_look(world, user);
    crate::balloon::balloon_alert(world, user, pad, "кейс извлечён");
    refresh_look(world, pad);
    refresh_view(world, pad);
}

/// `attack_self`: the window. True when `item` is a pad.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<ImplantPad>(item).is_none() {
        return false;
    }
    refresh_view(world, item);
    let title = crate::examine::name_of(world, item);
    world.write_message(OpenUi::new::<PadWindow>(client, mob, item, title));
    true
}

/// The deathrattle in the pad's case, if any: (case, its implant's group, expeditionary).
fn case_deathrattle(world: &mut World, pad: Entity) -> Option<(Entity, Option<u32>, bool)> {
    let case = case_of(world, pad)?;
    let imp = world.get::<ImplantHolder>(case)?.imp.as_ref()?;
    (imp.kind == ImplantKind::Deathrattle)
        .then(|| (case, imp.group, imp.path == DEATHRATTLE_LAVALAND))
}

/// `deathrattle_group.register(implant)` of the case's implant, and the case renamed
/// "[initial name] - [group]".
fn register(world: &mut World, case: Entity, group: u32) {
    if let Some(mut holder) = world.get_mut::<ImplantHolder>(case)
        && let Some(imp) = holder.imp.as_mut()
    {
        imp.group = Some(group);
    }
    let name = world
        .resource::<DeathrattleGroups>()
        .get(group)
        .map(|g| g.name.clone())
        .unwrap_or_default();
    let base = world
        .get::<DmName>(case)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world
        .entity_mut(case)
        .insert(Name::new(format!("{base} - {name}")));
}

fn group_name(world: &World, group: u32) -> String {
    world
        .resource::<DeathrattleGroups>()
        .get(group)
        .map(|g| g.name.clone())
        .unwrap_or_default()
}

/// `save_deathrattle_group`.
fn save_group(world: &mut World, user: Entity, pad: Entity) {
    if case_of(world, pad).is_none() {
        crate::balloon::balloon_alert(world, user, pad, "внутри нет кейса!");
        return;
    }
    let Some((_, group, _)) = case_deathrattle(world, pad) else {
        crate::balloon::balloon_alert(world, user, pad, "несовместимый имплант!");
        return;
    };
    let Some(group) = group else {
        crate::balloon::balloon_alert(world, user, pad, "нет активной группы!");
        return;
    };
    if let Some(mut p) = world.get_mut::<ImplantPad>(pad) {
        p.saved_group = Some(group);
    }
    let name = group_name(world, group);
    crate::balloon::balloon_alert(world, user, pad, &format!("группа {name} сохранена"));
}

/// `set_deathrattle_group`: the case's implant joins the saved group of its kind.
fn set_group(world: &mut World, user: Entity, pad: Entity) {
    if case_of(world, pad).is_none() {
        crate::balloon::balloon_alert(world, user, pad, "внутри нет кейса!");
        return;
    }
    let Some(saved) = world.get::<ImplantPad>(pad).and_then(|p| p.saved_group) else {
        crate::balloon::balloon_alert(world, user, pad, "нет сохранённой группы!");
        return;
    };
    let Some((case, _, lavaland)) = case_deathrattle(world, pad) else {
        crate::balloon::balloon_alert(world, user, pad, "несовместимый имплант!");
        return;
    };
    if world
        .resource::<DeathrattleGroups>()
        .get(saved)
        .is_none_or(|g| g.lavaland != lavaland)
    {
        crate::balloon::balloon_alert(world, user, pad, "несовместимая группа!");
        return;
    }
    register(world, case, saved);
    let name = group_name(world, saved);
    crate::balloon::balloon_alert(world, user, pad, &format!("привязан к группе {name}"));
}

/// `init_deathrattle_group`: a new group, saved, with the case's implant in it.
fn init_group(world: &mut World, user: Entity, pad: Entity) {
    if case_of(world, pad).is_none() {
        crate::balloon::balloon_alert(world, user, pad, "внутри нет кейса!");
        return;
    }
    let Some((case, group, lavaland)) = case_deathrattle(world, pad) else {
        crate::balloon::balloon_alert(world, user, pad, "несовместимый имплант!");
        return;
    };
    if group.is_some() {
        crate::balloon::balloon_alert(world, user, pad, "группа уже задана!");
        return;
    }
    let group = new_deathrattle_group(world, lavaland);
    if let Some(mut p) = world.get_mut::<ImplantPad>(pad) {
        p.saved_group = Some(group);
    }
    register(world, case, group);
    let name = group_name(world, group);
    crate::balloon::balloon_alert(world, user, pad, &format!("привязан к новой группе {name}"));
}

/// `ui_data`.
fn compute_view(world: &mut World, pad: Entity) -> ImplantPadData {
    let saved = world.get::<ImplantPad>(pad).and_then(|p| p.saved_group);
    let saved_deathrattle_group = saved.map(|g| group_name(world, g));
    let Some(case) = case_of(world, pad) else {
        return ImplantPadData {
            saved_deathrattle_group,
            ..default()
        };
    };
    let imp = world.get::<ImplantHolder>(case).and_then(|h| h.imp.clone());
    let current_deathrattle_group = imp
        .as_ref()
        .filter(|i| i.kind == ImplantKind::Deathrattle)
        .and_then(|i| i.group)
        .map(|g| group_name(world, g));
    ImplantPadData {
        has_case: true,
        has_implant: imp.is_some(),
        case_information: imp.map(|i| i.info()).unwrap_or_default(),
        saved_deathrattle_group,
        current_deathrattle_group,
    }
}

fn refresh_view(world: &mut World, pad: Entity) {
    if world.get::<ImplantPad>(pad).is_none() {
        return;
    }
    let view = compute_view(world, pad);
    if let Some(mut p) = world.get_mut::<ImplantPad>(pad)
        && p.view != view
    {
        p.view = view;
    }
}

/// Pads with an open window get fresh data.
// Cost: per tick, one pass over the open pad windows; the data is a few short strings.
fn refresh_views(world: &mut World) {
    let mut q = world.query_filtered::<&UiWindow, With<UiKind<PadWindow>>>();
    let mut pads: Vec<Entity> = q.iter(world).map(|w| w.source).collect();
    pads.sort();
    pads.dedup();
    for pad in pads {
        refresh_view(world, pad);
    }
}

/// The server side of the `ImplantPad` interface.
pub struct PadWindow;

impl UiInterface for PadWindow {
    const NAME: &'static str = ImplantPadUi::NAME;
    type Data = ImplantPadData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadAct {
    Eject,
    SaveGroup,
    SetGroup,
    InitGroup,
}

impl ServerUi for PadWindow {
    type Source = &'static ImplantPad;
    type UserParam = ();
    type Act = PadAct;

    fn static_data(_: &ImplantPad) {}

    fn data(pad: &ImplantPad, _: Entity, _: &()) -> ImplantPadData {
        pad.view.clone()
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<PadAct, UiReject> {
        Ok(match action {
            ACT_EJECT => PadAct::Eject,
            ACT_SAVE_GROUP => PadAct::SaveGroup,
            ACT_SET_GROUP => PadAct::SetGroup,
            ACT_INIT_GROUP => PadAct::InitGroup,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// `ui_act`, at the moment of effect: the actor still holds or reaches the pad.
fn acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<PadWindow>>>) {
    let pending: Vec<(Entity, Entity, PadAct)> = acts
        .read(world.resource::<Messages<UiActed<PadWindow>>>())
        .map(|a| (a.source, a.actor, a.act))
        .collect();
    for (pad, user, act) in pending {
        let held = world.get::<CarriedBy>(pad).is_some_and(|c| c.0 == user);
        if world.get::<ImplantPad>(pad).is_none()
            || !(held || crate::storage::in_reach(world, user, pad))
        {
            continue;
        }
        match act {
            PadAct::Eject => eject(world, user, pad),
            PadAct::SaveGroup => save_group(world, user, pad),
            PadAct::SetGroup => set_group(world, user, pad),
            PadAct::InitGroup => init_group(world, user, pad),
        }
        refresh_view(world, pad);
    }
}
