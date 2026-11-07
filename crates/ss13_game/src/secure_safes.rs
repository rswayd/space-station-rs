//! Secure safes with a keypad (server), after tg `code/game/objects/structures/secure_safe.dm`
//! and `/datum/component/lockable_storage` (`code/datums/components/lockable_storage.dm`), with
//! tgui's `LockedSafe` window (`crate::safe_ui`).
//!
//! A secure safe ([`SecureSafe`]) is a storage (14 total w_class, anything but secure
//! briefcases' size limits aside) locked by a 5-digit code. A mapped one holds a paper and a
//! pen; the captain's spare ID safe holds the spare ID behind a random code and cannot be
//! hacked. An empty hand opens the keypad (and the storage while unlocked): digits up to five,
//! `E` with no code set sets the typed 5 digits as the code, otherwise unlocks on the right
//! code; `C` clears and locks again. A locked hackable safe: a screwdriver opens or closes its
//! service panel, a multitool on an open panel hacks it open in 40 s (the code is cleared).
// shortcut: no emag (`break_lock`), no wall frame on deconstruction (`/obj/item/wallframe/
// secure_safe`), the spare safe takes any item instead of only ID cards, light explosions are
// not ignored on purpose (structures without ported destruction take no blast damage anyway),
// the terminal typing sound is not played. Add each with its mechanic.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmObj};
use ss13_shared::chat::Span;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use crate::combat::CombatRng;
use crate::medical::name_of;
use crate::obj_damage::{is_under, set_icon_state};
use crate::safe_ui::*;
use crate::tools::{Actor, DoAfter, ToolKind, tell, tool_of, use_tool};

pub const SECURE_SAFE: &str = "/obj/structure/secure_safe";
const CAPS_SPARE: &str = "/obj/structure/secure_safe/caps_spare";
const PAPER: &str = "/obj/item/paper";
const PEN: &str = "/obj/item/pen";
const SPARE_ID: &str = "/obj/item/card/id/advanced/gold/captains_spare";
/// `use_tool(parent, user, 40 SECONDS)` of the hack.
const HACK_DS: f32 = 400.0;
const CODE_LEN: usize = 5;

/// `lockable_storage`: `lock_code`, `numeric_input`, `panel_open`, `can_hack_open`, and
/// `atom_storage.locked`; `base_icon_state` for the sprite.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct SecureSafe {
    pub code: Option<String>,
    pub input: String,
    pub locked: bool,
    pub panel_open: bool,
    pub can_hack: bool,
    base: String,
}

/// What a keypad press did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keypad {
    Typed,
    Ignored,
    CodeSet,
    Unlocked,
    Locked,
}

impl SecureSafe {
    pub fn new(code: Option<String>, can_hack: bool) -> Self {
        Self {
            locked: code.is_some(),
            code,
            can_hack,
            ..Self::default()
        }
    }

    /// `ui_act("keypad")` with `digit`.
    pub fn press(&mut self, key: &str) -> Keypad {
        match key {
            "C" => {
                self.input.clear();
                if self.locked || self.code.is_none() {
                    return Keypad::Ignored;
                }
                self.locked = true;
                Keypad::Locked
            }
            "E" => {
                let Some(code) = &self.code else {
                    if self.input.len() != CODE_LEN {
                        return Keypad::Ignored;
                    }
                    self.code = Some(std::mem::take(&mut self.input));
                    self.locked = true;
                    return Keypad::CodeSet;
                };
                if &self.input != code {
                    return Keypad::Ignored;
                }
                self.locked = false;
                self.input.clear();
                Keypad::Unlocked
            }
            d if d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()) => {
                if self.input.len() == CODE_LEN {
                    return Keypad::Ignored;
                }
                self.input.push_str(d);
                Keypad::Typed
            }
            _ => Keypad::Ignored,
        }
    }
}

/// The timed steps of this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecureSafeJob {
    /// The multitool's 40 s hack.
    Hack,
}

impl ServerUi for LockedSafeUi {
    type Source = &'static SecureSafe;
    type UserParam = ();
    type Act = String;

    fn static_data(_: &SecureSafe) {}

    fn data(s: &SecureSafe, _: Entity, _: &()) -> LockedSafeData {
        LockedSafeData {
            input_code: if s.input.is_empty() {
                "*****".to_owned()
            } else {
                s.input.clone()
            },
            locked: s.locked,
            lock_code: s.code.is_some(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<String, UiReject> {
        if action != KEYPAD {
            return Err(UiReject::UnknownAction);
        }
        let digit = params
            .get("digit")
            .filter(|d| KEYS.contains(d))
            .ok_or_else(|| UiReject::BadParams("digit".to_owned()))?;
        Ok(digit.to_owned())
    }
}

pub struct SecureSafesPlugin;

impl Plugin for SecureSafesPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.register_ui::<LockedSafeUi>()
            .add_observer(add_secure_safe)
            .add_systems(ss13_world::MapInit, fill_mapped)
            .add_systems(Update, keypad_acts.in_set(UiSystems::Handle));
        app.add_click_handler(ClickPhase::EmptyHand, 3179, |w, c| {
            interact(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 3419, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
    }
}

fn add_secure_safe(add: On<Add<DmObj>>, objs: Query<(&DmObj, &DmAtom)>, mut commands: Commands) {
    let Ok((dm, atom)) = objs.get(add.entity) else {
        return;
    };
    if !is_under(&dm.path, SECURE_SAFE) {
        return;
    }
    let spare = is_under(&dm.path, CAPS_SPARE);
    let base = atom.icon_state.clone();
    let entity = add.entity;
    commands.queue(move |world: &mut World| {
        // `SSid_access.spare_id_safe_code`: a random 5-digit code.
        let code = spare.then(|| {
            let n = world.resource_mut::<CombatRng>().range(0, 99_999);
            format!("{n:05}")
        });
        let safe = SecureSafe {
            base,
            ..SecureSafe::new(code, !spare)
        };
        world.entity_mut(entity).insert((
            safe,
            // `create_storage(max_specific_storage = WEIGHT_CLASS_GIGANTIC, max_total_storage =
            // 14)`, default `max_slots` 7.
            crate::storage::Storage {
                max_slots: 7,
                max_specific_storage: 6,
                max_total_storage: 14,
            },
        ));
        refresh(world, entity);
    });
}

/// `PopulateContents` of mapped safes.
fn fill_mapped(world: &mut World) {
    let safes: Vec<(Entity, bool)> = world
        .query_filtered::<(Entity, &DmObj), (With<SecureSafe>, With<ss13_world::Mapped>)>()
        .iter(world)
        .map(|(e, dm)| (e, is_under(&dm.path, CAPS_SPARE)))
        .collect();
    for (safe, spare) in safes {
        let items: &[&str] = if spare { &[SPARE_ID] } else { &[PAPER, PEN] };
        for path in items {
            if let Some(item) = crate::construction::spawn_detached(world, path) {
                crate::storage::put_inside(world, item, safe);
            }
        }
    }
}

/// `on_update_icon_state`: `[base]_locked` while locked.
fn refresh(world: &mut World, safe: Entity) {
    let Some(s) = world.get::<SecureSafe>(safe) else {
        return;
    };
    let state = if s.locked {
        format!("{}_locked", s.base)
    } else {
        s.base.clone()
    };
    set_icon_state(world, safe, &state);
}

/// `on_interact` (`attack_hand`): the keypad; an unlocked safe's storage opens too.
fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(locked) = world.get::<SecureSafe>(target).map(|s| s.locked) else {
        return false;
    };
    let title = name_of(world, target);
    world.write_message(OpenUi::new::<LockedSafeUi>(client, mob, target, title));
    if !locked {
        crate::storage::open_view(world, client, mob, target);
    }
    true
}

/// The panel tools of `lockable_storage`, items into the storage.
fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(s) = world.get::<SecureSafe>(target).cloned() else {
        return false;
    };
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    let hackable = s.can_hack && s.locked;
    match tool_of(world, held).map(|(t, _)| t) {
        Some(ToolKind::Screwdriver) if hackable => {
            let open = !s.panel_open;
            if let Some(mut live) = world.get_mut::<SecureSafe>(target) {
                live.panel_open = open;
            }
            let text = if open {
                "панель открыта"
            } else {
                "панель закрыта"
            };
            tell(world, client, Span::Notice, text);
            return true;
        }
        Some(ToolKind::Multitool) if hackable => {
            if !s.panel_open {
                tell(world, client, Span::Warning, "панель закрыта!");
                return true;
            }
            tell(world, client, Span::Notice, "взлом...");
            let actor = Actor {
                client,
                mob,
                item: Some(held),
            };
            use_tool(
                world,
                actor,
                target,
                HACK_DS,
                0,
                crate::construction::Job::SecureSafe(SecureSafeJob::Hack),
            );
            return true;
        }
        _ => {}
    }
    // A locked one refuses ("закрыто!", `crate::lockbox::is_locked`).
    crate::storage::insert_from_hand(world, held, target);
    true
}

/// [`SecureSafeJob`] at the end of its `do_after`.
pub fn complete(world: &mut World, action: &DoAfter, job: SecureSafeJob) {
    let SecureSafeJob::Hack = job;
    let Some(mut s) = world.get_mut::<SecureSafe>(action.target) else {
        return;
    };
    if !s.can_hack || !s.locked {
        return;
    }
    // `set_lock_code(null)`.
    s.code = None;
    s.locked = false;
    tell(world, action.client, Span::Notice, "взломано");
    refresh(world, action.target);
}

/// Whether `storage` is a locked secure safe (tg `atom_storage.locked`).
pub fn is_locked(world: &World, storage: Entity) -> bool {
    world.get::<SecureSafe>(storage).is_some_and(|s| s.locked)
}

fn keypad_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<LockedSafeUi>>>) {
    let pending: Vec<(Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<LockedSafeUi>>>())
        .map(|a| (a.client, a.source, a.act.clone()))
        .collect();
    for (client, safe, key) in pending {
        let Some(mut s) = world.get_mut::<SecureSafe>(safe) else {
            continue;
        };
        let result = s.press(&key);
        let code = s.code.clone().unwrap_or_default();
        if result == Keypad::CodeSet {
            let name = name_of(world, safe);
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы задаёте код {name}: {code}."),
            );
        }
        if matches!(result, Keypad::CodeSet | Keypad::Unlocked | Keypad::Locked) {
            refresh(world, safe);
        }
    }
}

/// `on_examine`: the service panel of a hackable safe.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(s) = world.get::<SecureSafe>(target).filter(|s| s.can_hack) else {
        return;
    };
    let panel = if s.panel_open {
        "откручена"
    } else {
        "закручена"
    };
    lines.push((
        Span::Notice,
        format!("Сервисная панель сейчас <b>{panel}</b>."),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press_all(s: &mut SecureSafe, keys: &str) -> Keypad {
        let mut last = Keypad::Ignored;
        for k in keys.chars() {
            last = s.press(&k.to_string());
        }
        last
    }

    #[test]
    fn five_digits_and_enter_set_the_code_then_unlock() {
        let mut s = SecureSafe::new(None, true);
        assert!(!s.locked);
        assert_eq!(press_all(&mut s, "1234E"), Keypad::Ignored);
        assert_eq!(press_all(&mut s, "5E"), Keypad::CodeSet);
        assert!(s.locked);
        assert_eq!(s.code.as_deref(), Some("12345"));
        assert_eq!(press_all(&mut s, "12344E"), Keypad::Ignored);
        assert_eq!(press_all(&mut s, "C12345E"), Keypad::Unlocked);
        assert!(!s.locked);
        assert_eq!(s.press("C"), Keypad::Locked);
        assert!(s.locked);
    }

    #[test]
    fn the_input_stops_at_five_digits() {
        let mut s = SecureSafe::new(Some("00000".into()), false);
        assert_eq!(press_all(&mut s, "123456"), Keypad::Ignored);
        assert_eq!(s.input, "12345");
        assert_eq!(s.press("X"), Keypad::Ignored);
    }
}
