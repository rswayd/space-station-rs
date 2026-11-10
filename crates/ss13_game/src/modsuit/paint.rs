//! MOD paint (server), after tg `code/modules/mod/mod_paint.dm`:
//!
//! - The paint kit (`/obj/item/mod/paint`) on a control unit: a left click lists the theme's
//!   skins (tg radial menu) and applies the chosen one (`theme.set_skin`); a right click opens
//!   tgui `MODpaint` with the color matrix. `transition_color` keeps the edited matrix,
//!   `confirm` checks it (tg's limits per channel, per value and overall, [`super::paint_refusal`])
//!   and tints every part and the unit, then closes. Both need the user conscious, holding the
//!   kit, the unit inactive (`check_menu`); the window closes otherwise.
//! - A skin applier (`/obj/item/mod/skin_applier`, one use) applies its skin to an inactive unit
//!   of a theme that has it; the honkerative one also installs a waddle module.
// shortcut: the matrix is shown as a tint of the item icons (no color matrices in
// appearances, no worn-sprite colors); no preview map in the window.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmClothing;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::Appearance;
use ss13_shared::chat::Span;
use ss13_tgui::UiStatus;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

use super::server::{ModDb, ModSuit, install, update_icons};
use super::*;
use crate::health::Stat;
use crate::inventory::CarriedBy;
use crate::tools::Actor;

pub const PAINT_KIT: &str = "/obj/item/mod/paint";
pub const SKIN_APPLIER: &str = "/obj/item/mod/skin_applier";
const HONKERATIVE: &str = "/obj/item/mod/skin_applier/honkerative";
const WADDLE: &str = "/obj/item/mod/module/waddle";

/// The paint kit's `editing_mod` and `current_color`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct PaintKit {
    pub editing: Option<Entity>,
    pub matrix: Vec<f32>,
}

impl Default for PaintKit {
    fn default() -> Self {
        Self {
            editing: None,
            matrix: COLOR_MATRIX_IDENTITY.to_vec(),
        }
    }
}

/// On a skin prompt: the kit, the unit and the skins listed.
#[derive(Component, Clone, Debug)]
struct SkinChoice {
    kit: Entity,
    control: Entity,
    skins: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaintAct {
    Transition(Vec<f32>),
    Confirm,
}

pub struct ModPaintPlugin;

impl Plugin for ModPaintPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<ModPaintUi>().add_systems(
            Update,
            (
                paint_status.in_set(UiSystems::StatusHook),
                (paint_acts, skin_answers.after(PromptSystems::Answer)).in_set(UiSystems::Handle),
            ),
        );
    }
}

impl ServerUi for ModPaintUi {
    type Source = &'static PaintKit;
    type UserParam = ();
    type Act = PaintAct;

    fn static_data(_: &PaintKit) {}

    fn data(kit: &PaintKit, _: Entity, _: &()) -> ModPaintData {
        ModPaintData {
            current_color: kit.matrix.clone(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<PaintAct, UiReject> {
        match action {
            TRANSITION_COLOR => {
                let text = params.text(COLOR)?;
                let values: Result<Vec<f32>, _> =
                    text.split(',').map(|v| v.trim().parse::<f32>()).collect();
                match values {
                    Ok(v) if v.len() == 20 && v.iter().all(|x| x.is_finite()) => {
                        Ok(PaintAct::Transition(v))
                    }
                    _ => Err(UiReject::BadParams("color: 20 numbers".into())),
                }
            }
            CONFIRM => Ok(PaintAct::Confirm),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn path_of(world: &World, e: Entity) -> Option<String> {
    world.get::<DmItem>(e).map(|i| i.path.clone())
}

/// tg `check_menu`: a conscious user holding the kit, the unit there and inactive.
fn check_menu(world: &World, kit: Entity, control: Entity, user: Entity) -> bool {
    let able = world.get::<Stat>(user) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, user).incapacitated;
    let holds = world.get::<CarriedBy>(kit).is_some_and(|c| c.0 == user);
    let inactive = world
        .get::<ModSuit>(control)
        .is_some_and(|s| !s.active && !s.activating);
    able && holds && inactive
}

/// The paint kit or a skin applier used on a control unit (left: skin, right: recolor). True
/// when the click was theirs.
pub fn item_interaction(world: &mut World, actor: Actor, target: Entity, right: bool) -> bool {
    let Some(held) = actor.item else {
        return false;
    };
    let Some(path) = path_of(world, held) else {
        return false;
    };
    if is_under(&path, SKIN_APPLIER) {
        if world.get::<ModSuit>(target).is_none() {
            return false;
        }
        apply_skin_applier(world, actor, held, &path, target);
        return true;
    }
    if !is_under(&path, PAINT_KIT) || world.get::<ModSuit>(target).is_none() {
        return false;
    }
    if world.get::<PaintKit>(held).is_none() {
        world.entity_mut(held).insert(PaintKit::default());
    }
    if right {
        if !check_menu(world, held, target, actor.mob) {
            return true;
        }
        world.entity_mut(held).insert((
            PaintKit {
                editing: Some(target),
                matrix: COLOR_MATRIX_IDENTITY.to_vec(),
            },
            AlwaysState,
        ));
        let title = crate::security::name_of(world, held);
        world.write_message(OpenUi::new::<ModPaintUi>(
            actor.client,
            actor.mob,
            held,
            title,
        ));
    } else {
        paint_skin(world, actor, held, target);
    }
    true
}

/// tg `paint_skin`: the theme's skins as a list (tg radial menu).
fn paint_skin(world: &mut World, actor: Actor, kit: Entity, control: Entity) {
    let skins: Vec<String> = world
        .get::<ModSuit>(control)
        .and_then(|s| world.resource::<ModDb>().data.themes.get(s.theme))
        .map(|t| t.skins.iter().map(|s| s.name.clone()).collect())
        .unwrap_or_default();
    if skins.len() <= 1 {
        crate::tools::tell(world, actor.client, Span::Warning, "нет других скинов!");
        return;
    }
    let title = crate::security::name_of(world, control);
    let prompt = ask_world(
        world,
        Ask::new(
            actor.client,
            actor.mob,
            title,
            "Выберите скин",
            PromptKind::List {
                items: skins.clone(),
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(SkinChoice {
        kit,
        control,
        skins,
    });
}

fn skin_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(choice) = world.get::<SkinChoice>(prompt).cloned() else {
            continue;
        };
        let pick = match answer {
            Some(PromptAnswer::Item(i)) => choice.skins.get(i).cloned(),
            _ => None,
        };
        let Some(pick) = pick else {
            crate::tools::tell(world, client, Span::Warning, "скин не выбран!");
            continue;
        };
        if !check_menu(world, choice.kit, choice.control, actor) {
            continue;
        }
        set_skin(world, choice.control, &pick);
    }
}

/// tg `/datum/mod_theme/proc/set_skin`: every part takes the skin's flags for its type (sealed
/// ones keep their sealed flags), the icons follow. False when the theme has no such skin.
pub fn set_skin(world: &mut World, control: Entity, skin: &str) -> bool {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return false;
    };
    let Some(skin_def) = world
        .resource::<ModDb>()
        .data
        .themes
        .get(suit.theme)
        .and_then(|t| t.skins.iter().find(|s| s.name == skin))
        .cloned()
    else {
        return false;
    };
    let mut parts = suit.parts.clone();
    for part in &mut parts {
        let Some(def) = skin_def.parts.iter().find(|d| d.path == part.def.path) else {
            continue;
        };
        part.def = def.clone();
        if let Some(mut c) = world.get_mut::<DmClothing>(part.item) {
            c.clothing_flags = def.unsealed_clothing;
            c.flags_inv = def.unsealed_inv;
            c.flags_cover = def.unsealed_cover;
            c.visor_flags = def.sealed_clothing;
            c.visor_flags_inv = def.sealed_inv;
            c.visor_flags_cover = def.sealed_cover;
            if part.sealed {
                c.clothing_flags |= def.sealed_clothing;
                c.flags_inv |= def.sealed_inv;
                c.flags_cover |= def.sealed_cover;
            }
        }
    }
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.skin = skin.to_owned();
        s.parts = parts;
    }
    update_icons(world, control);
    true
}

/// tg `set_mod_color`: the matrix on every part and the unit.
pub fn set_mod_color(world: &mut World, control: Entity, matrix: &[f32]) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let tint = matrix_tint(matrix);
    let color = (tint != [255; 4]).then_some(tint);
    for e in suit.parts.iter().map(|p| p.item).chain([control]) {
        if let Some(mut look) = world.get_mut::<Appearance>(e)
            && look.color != color
        {
            look.color = color;
        }
    }
    if let Some(mob) = suit.wearer.or(suit.holder) {
        crate::inventory::refresh_look(world, mob);
    }
}

/// tg `ui_status`: the window closes once `check_menu` fails or the kit stopped editing.
fn paint_status(
    mut windows: Query<&mut UiWindow>,
    kits: Query<&PaintKit>,
    world_check: PaintCheck,
) {
    for mut window in &mut windows {
        if window.interface != ModPaintUi::NAME {
            continue;
        }
        let ok = kits
            .get(window.source)
            .ok()
            .and_then(|k| k.editing)
            .is_some_and(|control| world_check.ok(window.source, control, window.actor));
        if !ok {
            window.cap_status(UiStatus::Close);
        }
    }
}

/// [`check_menu`] for the status hook, from queries.
#[derive(bevy::ecs::system::SystemParam)]
struct PaintCheck<'w, 's> {
    stats: Query<'w, 's, &'static Stat>,
    carried: Query<'w, 's, &'static CarriedBy>,
    suits: Query<'w, 's, &'static ModSuit>,
}

impl PaintCheck<'_, '_> {
    fn ok(&self, kit: Entity, control: Entity, user: Entity) -> bool {
        let conscious = self.stats.get(user).is_ok_and(|s| *s == Stat::Conscious);
        let holds = self.carried.get(kit).is_ok_and(|c| c.0 == user);
        let inactive = self
            .suits
            .get(control)
            .is_ok_and(|s| !s.active && !s.activating);
        // Incapacitation is rechecked at the act (`check_menu`).
        conscious && holds && inactive
    }
}

fn paint_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ModPaintUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, PaintAct)> = acts
        .read(world.resource::<Messages<UiActed<ModPaintUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, kit, act) in pending {
        let Some(state) = world.get::<PaintKit>(kit).cloned() else {
            continue;
        };
        let Some(control) = state.editing else {
            continue;
        };
        if !check_menu(world, kit, control, actor) {
            continue;
        }
        match act {
            PaintAct::Transition(matrix) => {
                if let Some(mut k) = world.get_mut::<PaintKit>(kit) {
                    k.matrix = matrix;
                }
            }
            PaintAct::Confirm => {
                if let Some(why) = paint_refusal(&state.matrix) {
                    crate::tools::tell(world, client, Span::Warning, why);
                    continue;
                }
                set_mod_color(world, control, &state.matrix);
                // `SStgui.close_uis(src)` -> `ui_close`: the status hook closes it.
                world.entity_mut(kit).insert(PaintKit::default());
            }
        }
    }
}

/// `skin_applier/interact_with_atom`; the honkerative one installs a waddle module after.
fn apply_skin_applier(
    world: &mut World,
    actor: Actor,
    applier: Entity,
    path: &str,
    control: Entity,
) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let tell = |world: &mut World, span: Span, text: &str| {
        crate::tools::tell(world, actor.client, span, text);
    };
    if suit.active || suit.activating {
        tell(world, Span::Warning, "блок активен!");
        return;
    }
    let skin = skin_of_applier(path);
    if !set_skin(world, control, skin) {
        tell(world, Span::Warning, "скин не подходит к теме!");
        return;
    }
    tell(world, Span::Notice, "скин нанесён");
    world.despawn(applier);
    if is_under(path, HONKERATIVE)
        && let Some(module) = crate::construction::spawn_detached(world, WADDLE)
    {
        super::modules::init_module(world, module);
        if !install(world, control, module, Some(actor.mob))
            && let Some(pos) = crate::chem::map_pos(world, actor.mob)
        {
            crate::storage::take_out(world, module, pos);
        }
    }
}

/// The applier's `skin` (`civilian`; the honkerative subtype `honkerative`).
pub fn skin_of_applier(path: &str) -> &'static str {
    if is_under(path, HONKERATIVE) {
        "honkerative"
    } else {
        "civilian"
    }
}
