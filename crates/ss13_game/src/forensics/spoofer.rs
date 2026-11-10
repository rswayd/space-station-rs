//! The syndicate forensics spoofer (server), after tg `/obj/item/forensics_spoofer`
//! (`code/game/objects/items/forensicsspoofer.dm`): it looks like the detective's scanner.
//!
//! - Scan mode (default): a click on an adjacent atom stores its prints and fibers not stored
//!   yet, up to 5 of each; over the limit a list asks which new one to leave out (cancel drops
//!   the whole scan). Prints carry the name of the crew record they match ("Unknown").
//! - Apply mode: the chosen print or fiber is planted on the clicked atom (1 s cooldown), with
//!   the user's hidden print; nothing is planted twice.
//! - Each click fakes a scanner's scan (its sound and line) unless silent mode is on. The tgui
//!   `ForensicsSpoofer` window (use in hand) toggles the modes, chooses and deletes entries; a
//!   right click in hand toggles scan/apply.
//!
//! The click itself never leaves the user's prints on the target (tg cuts the click chain at
//! `COMSIG_ITEM_INTERACTING_WITH_ATOM`; here the handler takes the click before anything else
//! touches the target).
// tg asks which *fiber* to drop when there are too many prints (`tgui_input_list(...,
// new_fibers)`); the port lists the prints. tg's storage-full warning compares with `>` and can
// never show; here it shows at the limit. The window has no notes (`make_note` has no button).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiStatus;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems,
};

use super::server::{Forensics, add_hiddenprint, add_trace};
use super::{
    ForensicsSpooferUi, SPOOF_CHOOSE, SPOOF_CHOSEN, SPOOF_DELETE, SPOOF_SCANMODE, SPOOF_STEALTH,
    SpooferData, SpooferStatic,
};
use crate::balloon::balloon_alert;
use crate::inventory::Inventory;
use crate::tools::tell;

pub const SPOOFER: &str = "/obj/item/forensics_spoofer";
/// `max_storage`, `tamper_cooldown_time`.
pub const MAX_STORAGE: usize = 5;
pub const TAMPER_COOLDOWN: f64 = 1.0;
/// A print or fiber longer than this is not one the spoofer could have stored.
const MAX_ENTRY_LEN: usize = 256;

/// `/obj/item/forensics_spoofer`'s memory and modes.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ForensicsSpoofer {
    /// (print, the name of the crew record with it).
    pub fingerprints: Vec<(String, String)>,
    pub fibers: Vec<String>,
    pub chosen_fingerprint: Option<String>,
    pub chosen_fiber: Option<String>,
    pub scan_mode: bool,
    pub silent: bool,
    pub tamper_ready_at: f64,
}

impl Default for ForensicsSpoofer {
    fn default() -> Self {
        Self {
            fingerprints: Vec::new(),
            fibers: Vec::new(),
            chosen_fingerprint: None,
            chosen_fiber: None,
            scan_mode: true,
            silent: false,
            tamper_ready_at: 0.0,
        }
    }
}

/// On a prompt: the scan waiting for the user to leave one new entry out.
#[derive(Component, Clone, Debug)]
struct Overflow {
    spoofer: Entity,
    prints: Vec<String>,
    fibers: Vec<String>,
    /// The list asked about: prints, then fibers.
    asking_prints: bool,
}

pub(super) fn build(app: &mut App) {
    app.register_ui::<ForensicsSpooferUi>()
        .add_observer(add_spoofer)
        .add_systems(
            Update,
            (
                spoofer_acts.in_set(UiSystems::Handle),
                answers
                    .in_set(UiSystems::Handle)
                    .after(PromptSystems::Answer),
            ),
        );
}

fn add_spoofer(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, SPOOFER))
    {
        commands
            .entity(add.entity)
            .insert_if_new((ForensicsSpoofer::default(), AlwaysState));
    }
}

fn holding(world: &World, mob: Entity, item: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.hands.contains(&Some(item)))
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// `attack_self` -> `ui_interact`. True when `item` is a spoofer.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<ForensicsSpoofer>(item).is_none() {
        return false;
    }
    let title = crate::examine::name_of(world, item);
    world.write_message(OpenUi::new::<ForensicsSpooferUi>(client, mob, item, title));
    true
}

/// `attack_self_secondary`: scan or apply. True when `item` is a spoofer.
pub fn toggle_mode(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(mut s) = world.get_mut::<ForensicsSpoofer>(item) else {
        return false;
    };
    s.scan_mode = !s.scan_mode;
    let text = if s.scan_mode {
        "now scanning"
    } else {
        "now applying"
    };
    balloon_alert(world, mob, item, text);
    true
}

/// `do_interact`: the click on an adjacent `target`. True when `held` is a spoofer.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(state) = world.get::<ForensicsSpoofer>(held).cloned() else {
        return false;
    };
    if state.scan_mode {
        scan(world, client, mob, held, target);
    } else {
        tamper(world, mob, held, target, &state);
    }
    true
}

/// `do_fake_scan`: what the detective's scanner would do, unless silent.
fn fake_scan(world: &mut World, mob: Entity, spoofer: Entity, target: Entity, silent: bool) {
    if silent {
        return;
    }
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "industrial_scan", 20.0, true).extrarange(-2.0),
        );
    }
    let text = format!(
        "{} points {} at {} and performs a forensic scan.",
        crate::examine::name_of(world, mob),
        crate::examine::name_of(world, spoofer),
        crate::examine::name_of(world, target),
    );
    crate::chem::visible(world, mob, &text, &text, Span::Notice);
}

/// `scan`: the target's prints and fibers the spoofer does not have yet.
pub fn scan(world: &mut World, client: Entity, mob: Entity, spoofer: Entity, target: Entity) {
    let Some(state) = world.get::<ForensicsSpoofer>(spoofer).cloned() else {
        return;
    };
    fake_scan(world, mob, spoofer, target, state.silent);
    let Some(f) = world.get::<Forensics>(target).cloned() else {
        balloon_alert(world, mob, target, "nothing!");
        return;
    };
    let new_fibers: Vec<String> = f
        .fibers
        .iter()
        .filter(|x| !state.fibers.contains(x))
        .cloned()
        .collect();
    let new_prints: Vec<String> = f
        .fingerprints
        .iter()
        .filter(|x| !state.fingerprints.iter().any(|(p, _)| p == *x))
        .cloned()
        .collect();
    let found = new_fibers.len() + new_prints.len();
    let count = if found == 0 {
        "no".to_owned()
    } else {
        found.to_string()
    };
    balloon_alert(world, mob, spoofer, &format!("{count} new prints/fibers"));
    if found > 0 {
        let mut text = String::from("Scan results (Unstored Only):");
        for fiber in &new_fibers {
            text.push_str(&format!("\nFiber: {fiber}"));
        }
        if state.fibers.len() >= MAX_STORAGE {
            text.push_str("\nFiber storage full.");
        }
        for print in &new_prints {
            text.push_str(&format!("\nFingerprint: {print}"));
        }
        if state.fingerprints.len() >= MAX_STORAGE {
            text.push_str("\nFingerprint storage full.");
        }
        tell(world, client, Span::Notice, text);
    }
    store(world, client, mob, spoofer, new_prints, new_fibers, true);
}

/// The rest of `scan`: prints, then fibers, while there is room; too many new ones ask which to
/// leave out and continue from the answer ([`answers`]).
fn store(
    world: &mut World,
    client: Entity,
    mob: Entity,
    spoofer: Entity,
    prints: Vec<String>,
    fibers: Vec<String>,
    prints_first: bool,
) {
    let Some(state) = world.get::<ForensicsSpoofer>(spoofer).cloned() else {
        return;
    };
    let mut prints = prints;
    if prints_first && state.fingerprints.len() < MAX_STORAGE {
        if state.fingerprints.len() + prints.len() > MAX_STORAGE {
            ask_overflow(world, client, mob, spoofer, prints, fibers, true);
            return;
        }
        let mut all = state.fingerprints.clone();
        all.extend(prints.drain(..).map(|p| (p, String::new())));
        // `get_name_from_fingerprint` for every stored print.
        for (print, name) in &mut all {
            *name = owner_of(world, print);
        }
        if let Some(mut s) = world.get_mut::<ForensicsSpoofer>(spoofer) {
            s.fingerprints = all;
        }
    }
    let Some(state) = world.get::<ForensicsSpoofer>(spoofer).cloned() else {
        return;
    };
    if state.fibers.len() < MAX_STORAGE {
        if state.fibers.len() + fibers.len() > MAX_STORAGE {
            ask_overflow(world, client, mob, spoofer, Vec::new(), fibers, false);
            return;
        }
        if let Some(mut s) = world.get_mut::<ForensicsSpoofer>(spoofer) {
            s.fibers.extend(fibers);
        }
    }
}

fn ask_overflow(
    world: &mut World,
    client: Entity,
    mob: Entity,
    spoofer: Entity,
    prints: Vec<String>,
    fibers: Vec<String>,
    asking_prints: bool,
) {
    let items = if asking_prints {
        prints.clone()
    } else {
        fibers.clone()
    };
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "What to discard",
            "Too many prints, cancel to discard all",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(Overflow {
        spoofer,
        prints,
        fibers,
        asking_prints,
    });
}

/// The overflow answers, after the prompt's sleep: the spoofer is still in the user's hands.
fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answered: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answered {
        let Some(mut q) = world.get::<Overflow>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(index)) = answer else {
            continue;
        };
        if !holding(world, mob, q.spoofer) {
            continue;
        }
        let list = if q.asking_prints {
            &mut q.prints
        } else {
            &mut q.fibers
        };
        if index >= list.len() {
            continue;
        }
        list.remove(index);
        store(
            world,
            client,
            mob,
            q.spoofer,
            q.prints,
            q.fibers,
            q.asking_prints,
        );
    }
}

/// `get_name_from_fingerprint`: the crew record with the print, "Unknown" without one.
fn owner_of(world: &World, print: &str) -> String {
    world
        .get_resource::<crate::records::manifest::Manifest>()
        .and_then(|m| m.general.iter().find(|r| r.fingerprint == print))
        .map_or_else(|| "Unknown".to_owned(), |r| r.name.clone())
}

/// `tamper`: the chosen print or fiber onto `target`.
fn tamper(
    world: &mut World,
    mob: Entity,
    spoofer: Entity,
    target: Entity,
    state: &ForensicsSpoofer,
) {
    fake_scan(world, mob, spoofer, target, state.silent);
    let fiber = state.chosen_fiber.is_some();
    let Some(chosen) = state
        .chosen_fiber
        .clone()
        .or_else(|| state.chosen_fingerprint.clone())
    else {
        balloon_alert(world, mob, spoofer, "no fingerprint selected!");
        return;
    };
    let t = now(world);
    if t < state.tamper_ready_at {
        balloon_alert(world, mob, spoofer, "please wait!");
        return;
    }
    let present = world.get::<Forensics>(target).is_some_and(|f| {
        if fiber {
            f.fibers.contains(&chosen)
        } else {
            f.fingerprints.contains(&chosen)
        }
    });
    if present {
        balloon_alert(world, mob, spoofer, "already present!");
        return;
    }
    info!("{mob:?} has tampered with the fingerprints/fibers of {target:?}. Added {chosen}");
    add_trace(world, target, fiber, chosen);
    let text = if fiber {
        "fiber added"
    } else {
        "fingerprint added"
    };
    balloon_alert(world, mob, target, text);
    add_hiddenprint(world, target, mob);
    if let Some(mut s) = world.get_mut::<ForensicsSpoofer>(spoofer) {
        s.tamper_ready_at = t + TAMPER_COOLDOWN;
    }
}

impl ServerUi for ForensicsSpooferUi {
    type Source = &'static ForensicsSpoofer;
    type UserParam = ();
    type Act = SpooferAct;

    fn static_data(_: &ForensicsSpoofer) -> SpooferStatic {
        SpooferStatic {
            max_storage: MAX_STORAGE as u32,
        }
    }

    fn data(s: &ForensicsSpoofer, _: Entity, _: &()) -> SpooferData {
        SpooferData {
            scanmode: s.scan_mode,
            silent: s.silent,
            fibers: s.fibers.clone(),
            fingerprints: s.fingerprints.clone(),
            chosen_fiber: s.chosen_fiber.clone(),
            chosen_fingerprint: s.chosen_fingerprint.clone(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<SpooferAct, UiReject> {
        let chosen = || {
            params
                .get(SPOOF_CHOSEN)
                .filter(|c| !c.is_empty() && c.len() <= MAX_ENTRY_LEN)
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams(format!("{SPOOF_CHOSEN}: missing")))
        };
        Ok(match action {
            SPOOF_SCANMODE => SpooferAct::ScanMode,
            SPOOF_STEALTH => SpooferAct::Stealth,
            SPOOF_CHOOSE => SpooferAct::Choose(chosen()?),
            SPOOF_DELETE => SpooferAct::Delete(chosen()?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpooferAct {
    ScanMode,
    Stealth,
    Choose(String),
    Delete(String),
}

/// `ui_act` on the spoofer in the actor's hands (`hands_state`), at the moment of effect:
/// a chosen entry must be one it holds.
fn spoofer_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ForensicsSpooferUi>>>) {
    let pending: Vec<(Entity, Entity, SpooferAct)> = acts
        .read(world.resource::<Messages<UiActed<ForensicsSpooferUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, spoofer, act) in pending {
        if !holding(world, actor, spoofer) {
            continue;
        }
        let Some(mut s) = world.get_mut::<ForensicsSpoofer>(spoofer) else {
            continue;
        };
        match act {
            SpooferAct::ScanMode => s.scan_mode = !s.scan_mode,
            SpooferAct::Stealth => s.silent = !s.silent,
            SpooferAct::Choose(chosen) => {
                if s.fibers.contains(&chosen) {
                    s.chosen_fiber = Some(chosen);
                    s.chosen_fingerprint = None;
                } else if s.fingerprints.iter().any(|(p, _)| *p == chosen) {
                    s.chosen_fingerprint = Some(chosen);
                    s.chosen_fiber = None;
                }
            }
            SpooferAct::Delete(chosen) => {
                if let Some(i) = s.fibers.iter().position(|f| *f == chosen) {
                    s.fibers.remove(i);
                    if s.chosen_fiber.as_ref() == Some(&chosen) {
                        s.chosen_fiber = None;
                    }
                } else if let Some(i) = s.fingerprints.iter().position(|(p, _)| *p == chosen) {
                    s.fingerprints.remove(i);
                    if s.chosen_fingerprint.as_ref() == Some(&chosen) {
                        s.chosen_fingerprint = None;
                    }
                }
            }
        }
    }
}

/// `hands_state` of the window: closed out of the hands, disabled out of consciousness
/// (`crate::forensics::server`'s status hook covers it).
pub fn window_status(holding: bool, conscious: bool) -> Option<UiStatus> {
    if !holding {
        Some(UiStatus::Close)
    } else if !conscious {
        Some(UiStatus::Disabled)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_spoofer_scans_and_is_empty() {
        let s = ForensicsSpoofer::default();
        assert!(s.scan_mode && !s.silent);
        assert!(s.fingerprints.is_empty() && s.fibers.is_empty());
    }

    #[test]
    fn the_window_needs_hands_and_consciousness() {
        assert_eq!(window_status(false, true), Some(UiStatus::Close));
        assert_eq!(window_status(true, false), Some(UiStatus::Disabled));
        assert_eq!(window_status(true, true), None);
    }
}
