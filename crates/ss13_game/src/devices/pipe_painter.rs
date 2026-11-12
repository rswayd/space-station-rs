//! The pipe painter (server): tg `/obj/item/pipe_painter`
//! (`code/game/objects/items/devices/pipe_painter.dm`).
//!
//! - In hand it asks for one of `GLOB.pipe_paint_colors` (a list window).
//! - On an atmos machine (not a multi-z pipe, a color adapter or an airlock pump: `paintable`)
//!   it repaints it, which reconnects it by the new color (`paint` -> `set_pipe_color`); on a
//!   pipe fitting it sets the fitting's `pipe_color`. A click and a balloon either way.
// shortcut: it starts on "omni" — tg's `"grey"` is no paint color, so a first use would paint
// pipes `null`; a cancelled list keeps the color (tg sets it to `null`).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use crate::atmos_machinery::AtmosDevice;
use crate::atmos_machinery::fitting::PipeFitting;
use crate::atmos_machinery::thermo::PIPE_PAINT_COLORS;
use crate::inventory::CarriedBy;

pub const PIPE_PAINTER: &str = "/obj/item/pipe_painter";
const UNPAINTABLE: [&str; 3] = [
    "/obj/machinery/atmospherics/pipe/multiz",
    "/obj/machinery/atmospherics/pipe/color_adapter",
    "/obj/machinery/atmospherics/components/unary/airlock_pump",
];
const CLICK: &str = "sound/machines/click.ogg";

/// `paint_color`, an index into [`PIPE_PAINT_COLORS`].
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PipePainter(pub usize);

/// On the mob: the painter whose color list is open.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PainterMenu {
    painter: Entity,
}

pub struct PainterMenuWindow;

impl UiInterface for PainterMenuWindow {
    const NAME: &'static str = ListInputUi::NAME;
    type Data = ListInputData;
    type StaticData = ();
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PainterAct {
    Pick(String),
    Cancel,
}

impl ServerUi for PainterMenuWindow {
    type Source = &'static PainterMenu;
    type UserParam = ();
    type Act = PainterAct;

    fn static_data(_: &PainterMenu) {}

    fn data(_: &PainterMenu, _: Entity, _: &()) -> ListInputData {
        let items: Vec<String> = PIPE_PAINT_COLORS
            .iter()
            .map(|(n, _)| (*n).to_owned())
            .collect();
        ListInputData {
            title: "Pipe painter".into(),
            message: "Which colour do you want to use?".into(),
            init_value: items[0].clone(),
            items,
            timeout: None,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<PainterAct, UiReject> {
        match action {
            "submit" => params
                .get("entry")
                .map(|e| PainterAct::Pick(e.to_owned()))
                .ok_or_else(|| UiReject::BadParams("entry: expected an item".into())),
            "cancel" => Ok(PainterAct::Cancel),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.add_click_handler(ClickPhase::AttackSelf, 2591, |w, c| {
        attack_self(w, c.client, c.mob, c.target)
    });
    app.add_click_handler(ClickPhase::HeldOnTarget, 2861, |w, c| {
        c.held
            .is_some_and(|h| interact_with_atom(w, c.mob, h, c.target))
    });
    app.register_ui::<PainterMenuWindow>()
        .add_systems(Update, picks.in_set(UiSystems::Handle));
}

fn is_painter(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, PIPE_PAINTER))
}

fn color_of(world: &World, painter: Entity) -> (&'static str, [u8; 3]) {
    let i = world.get::<PipePainter>(painter).map_or(0, |p| p.0);
    PIPE_PAINT_COLORS[i.min(PIPE_PAINT_COLORS.len() - 1)]
}

/// `attack_self`: the color list.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !is_painter(world, item) {
        return false;
    }
    world.entity_mut(mob).insert(PainterMenu { painter: item });
    world.write_message(OpenUi::new::<PainterMenuWindow>(
        client,
        mob,
        mob,
        "Pipe painter",
    ));
    true
}

fn picks(world: &mut World, mut acts: Local<MessageCursor<UiActed<PainterMenuWindow>>>) {
    let picked: Vec<(Entity, PainterAct)> = acts
        .read(world.resource::<Messages<UiActed<PainterMenuWindow>>>())
        .filter(|a| a.actor == a.source)
        .map(|a| (a.actor, a.act.clone()))
        .collect();
    for (mob, act) in picked {
        let Some(menu) = world.get::<PainterMenu>(mob).copied() else {
            continue;
        };
        world.entity_mut(mob).remove::<PainterMenu>();
        let PainterAct::Pick(entry) = act else {
            continue;
        };
        // Still the mob's painter.
        if world.get::<CarriedBy>(menu.painter).map(|c| c.0) != Some(mob) {
            continue;
        }
        if let Some(i) = PIPE_PAINT_COLORS.iter().position(|(n, _)| *n == entry) {
            world.entity_mut(menu.painter).insert(PipePainter(i));
        }
    }
}

/// `interact_with_atom`: an atmos machine or a pipe fitting. True when it was ours.
pub fn interact_with_atom(world: &mut World, mob: Entity, painter: Entity, target: Entity) -> bool {
    if !is_painter(world, painter) {
        return false;
    }
    let (name, rgb) = color_of(world, painter);
    let painted = if let Some(device) = world.get::<AtmosDevice>(target).copied() {
        let paintable = !crate::construction::path_of(world, target)
            .is_some_and(|p| UNPAINTABLE.iter().any(|root| is_under(&p, root)));
        // `paint` answers `paintable`; the click is the painter's either way.
        if paintable {
            crate::atmos_machinery::fitting::repaint(world, device.id, rgb);
        }
        true
    } else if world.get::<PipeFitting>(target).is_some() {
        if let Some(mut fitting) = world.get_mut::<PipeFitting>(target) {
            fitting.color = rgb;
        }
        if let Some(mut look) = world.get_mut::<Appearance>(target) {
            look.color = Some([rgb[0], rgb[1], rgb[2], 255]);
        }
        true
    } else {
        false
    };
    if !painted {
        return false;
    }
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, CLICK, 50.0, true));
    }
    crate::balloon::balloon_alert(world, mob, target, &format!("окрашено в цвет {name}"));
    true
}

/// `examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if !is_painter(world, target) {
        return;
    }
    let (name, _) = color_of(world, target);
    lines.push((Span::Notice, format!("Выбран цвет: {name}.")));
}
