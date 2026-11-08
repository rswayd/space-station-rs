//! Screentips, after tg `/atom/proc/on_mouse_enter` (`code/game/atom/_atom.dm`), `build_context`
//! (`code/datums/screentips/screentips.dm`) and the HUD's `/atom/movable/screen/screentip`: the
//! line at the top of the map with the name of the atom under the cursor and, for a living
//! user, what its mouse buttons would do with it.
//!
//! - The client tells the server what it hovers (`ClientIntent::Hover`, only on a change). The
//!   server resolves it among what the client sees, builds [`ScreentipData`] and pushes it through
//!   the headless interface [`ScreentipUi`] on the player's mob; `crate::screentips_client`
//!   draws it.
//! - The name is BandaStation's `get_tip_name` (the visible name of a masked human).
//! - Contexts (`add_context`) of the target: carbons (`carbon_context.dm`), humans
//!   (`human_context.dm`), doors (`door.dm`) and airlocks (`airlock.dm`, the tools).
// shortcut: only those targets' contexts; the ~250 other `add_context`s, the held item's
// `COMSIG_ITEM_REQUESTING_CONTEXT_FOR_TARGET` contexts and the generic screentip elements
// (`contextual_screentip_bare_hands`, `_tools`, `_item_typechecks`) are not ported. No
// screentip preference (always on, text keys instead of the mouse icons) and no screentips over
// HUD buttons.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// The HUD line (no window).
pub struct ScreentipUi;

impl UiInterface for ScreentipUi {
    const NAME: &'static str = "Screentip";
    type Data = ScreentipData;
    type StaticData = ();
}

/// What the screentip shows; empty name: nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreentipData {
    pub name: String,
    /// The context lines (`subcontext`): "LMB: Обнять | RMB: Толкнуть", ...
    pub lines: Vec<String>,
}

/// `SCREENTIP_CONTEXT_*`.
pub const LMB: &str = "LMB";
pub const RMB: &str = "RMB";
pub const SHIFT_LMB: &str = "Shift-LMB";
pub const CTRL_LMB: &str = "Ctrl-LMB";
pub const CTRL_RMB: &str = "Ctrl-RMB";
pub const ALT_LMB: &str = "Alt-LMB";
pub const ALT_RMB: &str = "Alt-RMB";
pub const CTRL_SHIFT_LMB: &str = "Ctrl-Shift-LMB";

/// tg's `context` list: key -> text, a later set of a key replaces it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Context(Vec<(&'static str, String)>);

impl Context {
    pub fn set(&mut self, key: &'static str, text: impl Into<String>) {
        let text = text.into();
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some(entry) => entry.1 = text,
            None => self.0.push((key, text)),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, t)| t.as_str())
    }

    /// `on_mouse_enter`'s layout: LMB | RMB, Ctrl-LMB | Ctrl-RMB, Alt-LMB | Alt-RMB,
    /// Shift-LMB | Ctrl-Shift-LMB, one line each, empty lines left out; `build_context`
    /// without images writes `<key>: <text>`.
    pub fn lines(&self) -> Vec<String> {
        [
            [LMB, RMB],
            [CTRL_LMB, CTRL_RMB],
            [ALT_LMB, ALT_RMB],
            [SHIFT_LMB, CTRL_SHIFT_LMB],
        ]
        .into_iter()
        .filter_map(|pair| {
            let parts: Vec<String> = pair
                .into_iter()
                .filter_map(|key| self.get(key).map(|text| format!("{key}: {text}")))
                .collect();
            (!parts.is_empty()).then(|| parts.join(" | "))
        })
        .collect()
    }
}

#[cfg(feature = "server")]
pub use server::*;

#[cfg(feature = "server")]
mod server {
    use bevy::ecs::message::{MessageCursor, Messages};
    use bevy::prelude::*;
    use ss13_mapload::DmDoor;
    use ss13_net::server::{IntentReceived, Interest};
    use ss13_shared::TilePos;
    use ss13_shared::protocol::{ClientIntent, NetId};
    use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiSystems};

    use super::*;
    use crate::combat::CombatMode;
    use crate::doors::Door;
    use crate::ghost::Ghost;
    use crate::health::{BodyPosition, Health, ZoneSelected};
    use crate::human::HumanLook;
    use crate::inventory::Inventory;
    use crate::pull::{GrabState, PulledBy};
    use crate::server::{PlayerMob, Players};
    use crate::tools::ToolKind;
    use crate::zone::BodyZone;

    /// On a player's mob: the source of its [`ScreentipUi`], the tip shown now.
    #[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
    pub struct Screentip(pub ScreentipData);

    pub struct ScreentipsServerPlugin;

    impl Plugin for ScreentipsServerPlugin {
        fn build(&self, app: &mut App) {
            app.register_ui::<ScreentipUi>()
                .add_observer(give_screentip)
                .add_observer(take_screentip)
                .add_systems(Update, hover.before(UiSystems::Open));
        }
    }

    /// Every player mob (a ghost too: the name is shown to everyone) has the HUD line.
    fn give_screentip(
        add: On<Add<PlayerMob>>,
        players: Query<&PlayerMob>,
        mut open: MessageWriter<OpenUi>,
        mut commands: Commands,
    ) {
        let Ok(player) = players.get(add.entity) else {
            return;
        };
        commands.entity(add.entity).try_insert(Screentip::default());
        open.write(OpenUi::new::<ScreentipUi>(
            player.client,
            add.entity,
            add.entity,
            "",
        ));
    }

    /// The mob lost its player: the window closes with its source.
    fn take_screentip(remove: On<Remove<PlayerMob>>, mut commands: Commands) {
        if let Ok(mut mob) = commands.get_entity(remove.entity) {
            mob.try_remove::<Screentip>();
        }
    }

    impl ServerUi for ScreentipUi {
        type Source = &'static Screentip;
        type UserParam = ();
        type Act = ();

        fn static_data(_: &Screentip) {}

        fn data(source: &Screentip, _: Entity, _: &()) -> ScreentipData {
            source.0.clone()
        }
    }

    /// `on_mouse_enter`: the latest hover of each client this tick, resolved among what it
    /// sees, becomes its mob's tip; a changed tip is sent at once.
    // Cost: per tick, one tip per client that moved the cursor onto another atom (a name
    // lookup and a few component reads).
    fn hover(world: &mut World, mut intents: Local<MessageCursor<IntentReceived>>) {
        let mut latest: Vec<(Entity, Option<NetId>)> = Vec::new();
        for i in intents.read(world.resource::<Messages<IntentReceived>>()) {
            let ClientIntent::Hover(target) = i.intent else {
                continue;
            };
            match latest.iter_mut().find(|(c, _)| *c == i.client) {
                Some(entry) => entry.1 = target,
                None => latest.push((i.client, target)),
            }
        }
        for (client, target) in latest {
            let Some(mob) = world.resource::<Players>().mob(client) else {
                continue;
            };
            if world
                .get::<PlayerMob>(mob)
                .is_none_or(|p| p.client != client)
                || world.get::<Screentip>(mob).is_none()
            {
                continue;
            }
            let target = target.and_then(|id| world.resource::<Interest>().entity(client, id));
            let data = target.map_or_else(ScreentipData::default, |t| tip(world, mob, t));
            let Some(mut shown) = world.get_mut::<Screentip>(mob) else {
                continue;
            };
            if shown.0 == data {
                continue;
            }
            shown.0 = data;
            world.write_message(OpenUi::new::<ScreentipUi>(client, mob, mob, ""));
        }
    }

    /// The screentip of `target` for `user`.
    pub fn tip(world: &World, user: Entity, target: Entity) -> ScreentipData {
        let mut name = crate::examine::name_of(world, target);
        if world.get::<Health>(target).is_some()
            && let Some(visible) = crate::clothing::cover::hidden_face_name(world, target)
        {
            name = visible;
        }
        let mut context = Context::default();
        // `isliving(user)`: ghosts (and the AI eye) see names only.
        if world.get::<Health>(user).is_some() && world.get::<Ghost>(user).is_none() {
            let held = world
                .get::<Inventory>(user)
                .and_then(Inventory::active_item);
            if is_carbon(world, target) {
                carbon_context(world, user, target, held, &mut context);
                if world.get::<HumanLook>(target).is_some() {
                    human_context(world, user, target, &mut context);
                }
            }
            door_context(world, user, target, held, &mut context);
        }
        ScreentipData {
            name,
            lines: context.lines(),
        }
    }

    fn is_carbon(world: &World, mob: Entity) -> bool {
        world.get::<Health>(mob).is_some()
            && world.get::<Inventory>(mob).is_some()
            && world.get::<crate::silicon::Silicon>(mob).is_none()
            && world.get::<crate::mobs::BasicMob>(mob).is_none()
    }

    fn adjacent(world: &World, a: Entity, b: Entity) -> bool {
        match (world.get::<TilePos>(a), world.get::<TilePos>(b)) {
            (Some(&a), Some(&b)) => a.chebyshev(b).is_some_and(|d| d <= 1),
            _ => false,
        }
    }

    /// `/mob/living/carbon/add_context`.
    // shortcut: no "Схватить конечность" (Ctrl-LMB on a bleeding own limb), no tail pull on the
    // groin (no tail organs) and no `TRAIT_FAKEDEATH`.
    fn carbon_context(
        world: &World,
        user: Entity,
        target: Entity,
        held: Option<Entity>,
        context: &mut Context,
    ) {
        if held.is_some() {
            context.set(CTRL_SHIFT_LMB, "Предложить предмет");
            return;
        }
        if world.get::<HumanLook>(user).is_none() {
            return;
        }
        let combat = world.get::<CombatMode>(user).is_some_and(|c| c.0);
        if combat {
            context.set(LMB, "Атаковать");
        } else if user == target {
            context.set(LMB, "Проверить ранения");
        }
        if user == target {
            return;
        }
        context.set(RMB, "Толкнуть");
        if combat {
            return;
        }
        let lying = world.get::<BodyPosition>(target).is_some_and(|p| p.lying());
        let health = world.get::<Health>(target);
        if !lying {
            let zone = world
                .get::<ZoneSelected>(user)
                .map(|z| z.0)
                .unwrap_or_default();
            let head = health.is_some_and(|h| h.has_part(BodyZone::Head));
            if zone.check_zone() == BodyZone::Head && head {
                context.set(LMB, "Погладить");
            } else {
                context.set(LMB, "Обнять");
            }
        } else if health.is_some_and(|h| h.health() >= 0.0) {
            context.set(LMB, "Потрясти");
        } else {
            context.set(LMB, "СЛР");
        }
    }

    /// `/mob/living/carbon/human/add_context`.
    fn human_context(world: &World, user: Entity, target: Entity, context: &mut Context) {
        if world.get::<HumanLook>(user).is_none() || user == target {
            return;
        }
        let pulled_by_user = world
            .get::<PulledBy>(target)
            .is_some_and(|p| p.puller() == user);
        let text = if pulled_by_user {
            match world.get::<GrabState>(user).copied().unwrap_or_default() {
                GrabState::Passive => "Схватить",
                GrabState::Aggressive => "За горло",
                GrabState::Neck => "Задушить",
                GrabState::Kill => return,
            }
        } else {
            "Потащить"
        };
        context.set(CTRL_LMB, text);
    }

    /// `/obj/machinery/door/add_context` and, for airlocks, `/obj/machinery/door/airlock/
    /// add_context`'s tool lines.
    // shortcut: no silicon lines (bolt, electrify, emergency access), no electronics removal;
    // every airlock has an access panel.
    fn door_context(
        world: &World,
        user: Entity,
        target: Entity,
        held: Option<Entity>,
        context: &mut Context,
    ) {
        let (Some(door), Some(dm)) = (world.get::<Door>(target), world.get::<DmDoor>(target))
        else {
            return;
        };
        let silicon = world.get::<crate::silicon::Silicon>(user).is_some();
        let near = adjacent(world, user, target);
        if dm.can_open_with_hands && !silicon && held.is_none() && near {
            context.set(LMB, "Open");
            context.set(RMB, "Постучать"); // BandaStation: knocking
        }
        if dm.kind != ss13_mapload::DoorKind::Airlock || !near {
            return;
        }
        match held
            .and_then(|h| crate::tools::tool_of(world, h))
            .map(|t| t.0)
        {
            Some(ToolKind::Screwdriver) => {
                let text = if door.panel_open {
                    "Close panel"
                } else {
                    "Open panel"
                };
                context.set(LMB, text);
            }
            Some(ToolKind::Crowbar) => {
                use crate::airlock_security::{PLASTEEL_I_S, PLASTEEL_O_S};
                let shielding =
                    door.panel_open && matches!(door.security_level, PLASTEEL_O_S | PLASTEEL_I_S);
                context.set(
                    LMB,
                    if shielding {
                        "Remove shielding"
                    } else {
                        "Pry open"
                    },
                );
            }
            Some(ToolKind::Welder) => {
                use crate::airlock_security::{IRON, PLASTEEL_I, PLASTEEL_O};
                context.set(RMB, if door.welded { "Unweld" } else { "Weld shut" });
                let shielding = door.panel_open
                    && matches!(door.security_level, IRON | PLASTEEL_I | PLASTEEL_O);
                context.set(LMB, if shielding { "Cut shielding" } else { "Repair" });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_lines_follow_on_mouse_enter() {
        let mut c = Context::default();
        assert!(c.lines().is_empty());
        c.set(RMB, "Толкнуть");
        c.set(LMB, "Атаковать");
        c.set(LMB, "Обнять");
        c.set(CTRL_LMB, "Потащить");
        c.set(CTRL_SHIFT_LMB, "Предложить предмет");
        assert_eq!(
            c.lines(),
            [
                "LMB: Обнять | RMB: Толкнуть",
                "Ctrl-LMB: Потащить",
                "Ctrl-Shift-LMB: Предложить предмет",
            ]
        );
    }
}
