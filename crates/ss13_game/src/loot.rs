//! Alt-clicks on the world (server): tg `AltClickOn` -> `base_click_alt` (`code/_onclick/
//! click_alt.dm`) and the loot panel (`code/modules/lootpanel`).
//!
//! An alt-click ([`AltClick`] from `crate::click`) first gives the target's `click_alt` a go
//! when the mob can act on it (conscious, not incapacitated, the target next to it): a storage
//! opens (`/datum/storage/on_click_alt`, also one lying on the floor), a PDA gives back its ID
//! (`/obj/item/modular_computer/click_alt`). Otherwise, and always on alt+shift
//! (`alt_shift_click_on`), tg's `try_open_loot_panel_on` opens the tgui `LootPanel` window on the
//! target's turf: the turf and everything on it a player could click (not click-through, not
//! invisible), refreshed while it is open. Its `grab` act clicks the chosen thing with the
//! modifiers held (`user.ClickOn(thing, modifiers)` as a [`ClickOn`]), after checking that the
//! thing is still on that turf. One panel per client: opening it on another turf closes the
//! first (tg keeps one `/datum/lootpanel` per client).
//!
//! tg shows the loot panel instead of BYOND's old "turf contents" stat panel tab, so this is a
//! window (`LootPanel/index.tsx`, grouped view) rather than a chat listing: the entries are
//! clickable, which a chat listing could not be without a new protocol message.
// shortcut: the window closes at distance 6 (tgui's default distance status caps it) where tg
// keeps it while the turf is in view (7); no search box or grouping toggle; tanks lying on the
// floor do not toggle internals (no external tanks), ID cards do not withdraw money (no
// `tgui_input_number`), headsets have no high-volume mode.

use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName, DmObj, Turf};
use ss13_net::server::{Invisibility, Networked};
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::ui::UiStatus;
use ss13_shared::{Appearance, MouseOpacity, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::SpatialIndex;

use crate::click::{AltClick, ClickOn, ResolveClicks};
use crate::health::Stat;
use crate::hud::HudIcon;
use crate::panels::{LOOT_GRAB, LootEntry, LootPanelData, LootPanelUi};

/// The turf each client's loot panel shows (tg `client.loot_panel.source_turf`).
#[derive(Resource, Default)]
pub struct LootPanels(EntityHashMap<Entity>);

/// A parsed `grab`: the thing and the modifiers held while clicking its entry.
#[derive(Clone, Copy, Debug)]
pub struct Grab {
    pub thing: Entity,
    pub modifiers: ClickModifiers,
}

type UserQueries = (
    Res<'static, SpatialIndex>,
    Query<
        'static,
        'static,
        (
            &'static Appearance,
            Option<&'static Name>,
            Option<&'static DmName>,
            Option<&'static DmItem>,
            Option<&'static DmObj>,
            Option<&'static Invisibility>,
            Has<Turf>,
        ),
        With<Networked>,
    >,
);

/// What the panel lists on `pos`: the turf first, then what a click could hit.
fn listed(
    pos: TilePos,
    (index, things): &SystemParamItem<'_, '_, UserQueries>,
) -> Vec<(Entity, LootEntry)> {
    let entry = |e: Entity| {
        let (appearance, name, dm_name, item, obj, invisible, turf) = things.get(e).ok()?;
        // `add_new_searchable`: not click-through, not invisible to the viewer.
        if appearance.mouse_opacity == MouseOpacity::Transparent
            || invisible.is_some_and(|i| i.0 > 0)
        {
            return None;
        }
        let path = item
            .map(|i| i.path.clone())
            .or_else(|| obj.map(|o| o.path.clone()))
            .unwrap_or_default();
        let name = name
            .map(|n| n.as_str().to_owned())
            .or_else(|| dm_name.map(|n| n.name.clone()))
            .unwrap_or_default();
        Some((
            turf,
            (
                e,
                LootEntry {
                    name,
                    path,
                    icon: Some(HudIcon {
                        icon_state: appearance.icon_state,
                        color: appearance.color,
                    }),
                    key: e.to_bits(),
                },
            ),
        ))
    };
    let mut found: Vec<(bool, (Entity, LootEntry))> = index
        .on_tile(pos)
        .iter()
        .filter_map(|&e| entry(e))
        .collect();
    // The turf first (`populate_contents`), the rest in tile order.
    found.sort_by_key(|(turf, _)| !turf);
    found.into_iter().map(|(_, e)| e).collect()
}

impl ServerUi for LootPanelUi {
    type Source = &'static TilePos;
    type UserParam = UserQueries;
    type Act = Grab;

    fn static_data(_: &TilePos) {}

    fn data(
        pos: &TilePos,
        _: Entity,
        params: &SystemParamItem<'_, '_, UserQueries>,
    ) -> LootPanelData {
        LootPanelData {
            contents: listed(*pos, params).into_iter().map(|(_, e)| e).collect(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<Grab, UiReject> {
        if action != LOOT_GRAB {
            return Err(UiReject::UnknownAction);
        }
        let thing = params
            .get("ref")
            .and_then(|k| k.trim().parse::<u64>().ok())
            .and_then(Entity::try_from_bits)
            .ok_or_else(|| UiReject::BadParams("ref: expected a listed thing".into()))?;
        let flag = |key| params.get(key).is_some_and(|v| v == "1" || v == "true");
        Ok(Grab {
            thing,
            modifiers: ClickModifiers {
                shift: flag("shift"),
                ctrl: flag("ctrl"),
                alt: flag("alt"),
                middle: flag("middle"),
                right: flag("right"),
                pixel: None,
            },
        })
    }
}

pub struct LootServerPlugin;

impl Plugin for LootServerPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<LootPanelUi>()
            .init_resource::<LootPanels>()
            .add_systems(
                Update,
                (
                    alt_clicks
                        .after(ResolveClicks)
                        .before(UiSystems::Open)
                        .before(crate::inventory::inventory_intents),
                    loot_status.in_set(UiSystems::StatusHook),
                    loot_grabs.in_set(UiSystems::Handle),
                ),
            )
            .add_observer(forget_panel);
    }
}

/// tg `can_perform_action` without its reach part (the click carries that).
fn can_act(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
}

fn alt_clicks(world: &mut World, mut clicks: Local<MessageCursor<AltClick>>) {
    let clicks: Vec<AltClick> = clicks
        .read(world.resource::<Messages<AltClick>>())
        .copied()
        .collect();
    for click in clicks {
        // Something earlier this tick may have moved it.
        if world.get::<TilePos>(click.target) != Some(&click.target_pos) {
            continue;
        }
        if !click.loot_only
            && click.reach
            && can_act(world, click.mob)
            && click_alt(world, click.client, click.mob, click.target)
        {
            continue;
        }
        open_panel(world, click.client, click.mob, click.target_pos);
    }
}

/// tg `click_alt` of the types that define it here. True when it handled the click.
fn click_alt(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<crate::storage::Storage>(target).is_some() {
        crate::storage::open_view(world, client, mob, target);
        return true;
    }
    crate::bots::click_alt(world, mob, target)
        || crate::modpc::pda::alt_click(world, mob, target)
        || crate::icecream::click_alt(world, client, mob, target)
        || crate::furniture::click_alt(world, client, mob, target)
}

/// `try_open_loot_panel_on` -> `/datum/lootpanel/open(get_turf(target))`.
fn open_panel(world: &mut World, client: Entity, mob: Entity, pos: TilePos) {
    let Some(turf) = crate::construction::turf_at(world, pos) else {
        return;
    };
    let title = crate::security::name_of(world, turf);
    world.resource_mut::<LootPanels>().0.insert(client, turf);
    world.write_message(OpenUi::new::<LootPanelUi>(client, mob, turf, title));
}

/// `/datum/lootpanel/ui_status`: interactive while the turf is in reach, update-only otherwise
/// or when the mob cannot use its hands; a panel of a turf the client no longer looks at closes.
fn loot_status(
    mut windows: Query<&mut UiWindow>,
    panels: Res<LootPanels>,
    positions: Query<&TilePos>,
    index: Res<SpatialIndex>,
    mobs: Query<(
        Option<&Stat>,
        Option<&crate::status_effects::Incapacitation>,
    )>,
) {
    for mut window in &mut windows {
        if window.interface != LootPanelUi::NAME {
            continue;
        }
        if panels.0.get(&window.client) != Some(&window.source) {
            window.cap_status(UiStatus::Close);
            continue;
        }
        let (Ok(&at), Ok(&turf)) = (positions.get(window.actor), positions.get(window.source))
        else {
            continue;
        };
        let (stat, traits) = mobs.get(window.actor).unwrap_or((None, None));
        let usable = stat == Some(&Stat::Conscious)
            && !traits.is_some_and(|t| t.incapacitated || t.hands_blocked);
        if !usable || !index.adjacent(at, turf, |_| false) {
            window.cap_status(UiStatus::Update);
        }
    }
}

/// `/datum/lootpanel/proc/grab`: the thing must still be the turf or on it.
fn loot_grabs(
    mut acts: MessageReader<UiActed<LootPanelUi>>,
    positions: Query<&TilePos>,
    mut clicks: MessageWriter<ClickOn>,
) {
    for act in acts.read() {
        let (Ok(turf), Ok(at)) = (positions.get(act.source), positions.get(act.act.thing)) else {
            continue;
        };
        if turf != at {
            continue;
        }
        clicks.write(ClickOn {
            client: act.client,
            target: act.act.thing,
            modifiers: act.act.modifiers,
        });
    }
}

/// Forgets the panel of a client whose window closed.
fn forget_panel(
    remove: On<Remove<UiWindow>>,
    windows: Query<&UiWindow>,
    mut panels: ResMut<LootPanels>,
) {
    if let Ok(window) = windows.get(remove.entity)
        && window.interface == LootPanelUi::NAME
        && panels.0.get(&window.client) == Some(&window.source)
    {
        panels.0.remove(&window.client);
    }
}
