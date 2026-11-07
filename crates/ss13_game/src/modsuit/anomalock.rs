//! Anomaly-locked MOD modules, after tg `/datum/component/anomaly_locked_module`
//! (`code/datums/components/anomalock_module.dm`) on `/obj/item/mod/module/anomaly_locked` and
//! its kinesis, anti-gravity and teleporter subtypes:
//!
//! - the module activates or is used only with a core in it ("no core!", `MOD_ABORT_USE`);
//! - an accepted core (gravitational for kinesis and anti-gravity, bluespace for the teleporter,
//!   any for the base type) clicked on the module goes in, one at a time;
//! - a screwdriver takes the core out after 3 s (into the user's hands), unless the module's
//!   core is not removable (the `locked` and `prototype` kinds);
//! - `prebuilt` kinds start with a core of an accepted type;
//! - examine says which core it holds or needs, and the icon gets its `-core` state.
// shortcut: the modules' own effects (kinesis grabbing, anti-gravity, the teleporter) are not
// ported, and the module of holding (bluespace core, `storage/holding`) has no lock here: its
// storage needs the bag-of-holding rift checks first.

use bevy::prelude::*;
use ss13_shared::chat::Span;

use crate::construction::{Job, path_of};
use crate::obj_damage::is_under;
use crate::storage::StoredIn;
use crate::tools::{Actor, ToolKind, tool_of};

const LOCKED: &str = "/obj/item/mod/module/anomaly_locked";
const CORE: &str = "/obj/item/assembly/signaler/anomaly";
const GRAV: &str = "/obj/item/assembly/signaler/anomaly/grav";
const BLUESPACE: &str = "/obj/item/assembly/signaler/anomaly/bluespace";
const CLICK: &str = "sound/machines/click.ogg";
/// `try_remove_core`'s `do_after`, deciseconds.
const REMOVE_DS: f32 = 30.0;

/// The component's vars, by module type: `accepted_anomalies`, `prebuilt`, `core_removable`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockSpec {
    pub accepted: &'static str,
    pub prebuilt: bool,
    pub removable: bool,
}

/// The lock spec of a module type (`None`: not anomaly-locked).
pub fn spec(path: &str) -> Option<LockSpec> {
    if !is_under(path, LOCKED) {
        return None;
    }
    let sub = |s: &str| is_under(path, &format!("{LOCKED}/{s}"));
    let accepted = if sub("kinesis") || sub("antigrav") {
        GRAV
    } else if sub("teleporter") {
        BLUESPACE
    } else {
        CORE
    };
    let prebuilt = [
        "kinesis/prebuilt",
        "kinesis/prototype",
        "kinesis/plus",
        "kinesis/admin",
        "antigrav/prebuilt",
        "teleporter/prebuilt",
    ]
    .iter()
    .any(|s| sub(s));
    let removable = ![
        "kinesis/prebuilt/locked",
        "kinesis/prototype",
        "antigrav/prebuilt/locked",
        "teleporter/prebuilt/locked",
    ]
    .iter()
    .any(|s| sub(s));
    Some(LockSpec {
        accepted,
        prebuilt,
        removable,
    })
}

/// The socketed `core` of a module.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnomalyLock {
    pub core: Option<Entity>,
}

/// The lock of `module` (given on first use: a prebuilt one gets its core), with a core that
/// left forgotten. `None` for a module that is not anomaly-locked.
pub fn lock(world: &mut World, module: Entity) -> Option<(LockSpec, AnomalyLock)> {
    let spec = spec(&path_of(world, module)?)?;
    if let Some(&l) = world.get::<AnomalyLock>(module) {
        let core = l
            .core
            .filter(|&c| world.get::<StoredIn>(c).is_some_and(|s| s.0 == module));
        return Some((spec, AnomalyLock { core }));
    }
    let mut l = AnomalyLock::default();
    if spec.prebuilt {
        // `pick(anomaly_types)`: the one accepted type.
        let core = crate::construction::spawn_detached(world, spec.accepted).unwrap_or_else(|| {
            world
                .spawn(ss13_mapload::DmItem {
                    path: spec.accepted.to_owned(),
                    ..default()
                })
                .id()
        });
        crate::storage::put_inside(world, core, module);
        l.core = Some(core);
    }
    world.entity_mut(module).insert(l);
    update_icon(world, module, l.core.is_some());
    Some((spec, l))
}

fn update_icon(world: &mut World, module: Entity, has_core: bool) {
    let Some(base) = world
        .get::<ss13_mapload::DmAtom>(module)
        .map(|a| a.icon_state.clone())
    else {
        return;
    };
    let base = base.trim_end_matches("-core").to_owned();
    let state = if has_core {
        format!("{base}-core")
    } else {
        base
    };
    crate::obj_damage::set_icon_state(world, module, &state);
}

/// `COMSIG_MODULE_TRIGGERED`: false (`MOD_ABORT_USE`, "no core!") for a locked module
/// without its core.
pub fn may_trigger(world: &mut World, module: Entity, wearer: Option<Entity>) -> bool {
    let Some((_, l)) = lock(world, module) else {
        return true;
    };
    if l.core.is_some() {
        return true;
    }
    if let Some(mob) = wearer {
        super::server::tell_mob(world, mob, Span::Warning, "нет ядра!");
    }
    false
}

/// `COMSIG_ATOM_ITEM_INTERACTION` (an accepted core in) and the screwdriver act (the core out)
/// on a locked module. True when the click was the lock's.
pub fn item_interaction(world: &mut World, actor: Actor, module: Entity) -> bool {
    let Some(held) = actor.item else {
        return false;
    };
    let Some((spec, l)) = lock(world, module) else {
        return false;
    };
    let tell = |world: &mut World, text: &str| {
        crate::tools::tell(world, actor.client, Span::Warning, text);
    };
    if tool_of(world, held).is_some_and(|(k, _)| k == ToolKind::Screwdriver) {
        if l.core.is_none() {
            tell(world, "нет ядра!");
            return true;
        }
        if !spec.removable {
            tell(world, "ядро не извлечь!");
            return true;
        }
        // The module in a hand has no tile: the timer runs on the user.
        let target = if world.get::<ss13_shared::TilePos>(module).is_some() {
            module
        } else {
            actor.mob
        };
        crate::tools::do_after(
            world,
            actor,
            target,
            REMOVE_DS,
            0,
            Job::AnomalockRemove { module },
        );
        return true;
    }
    let Some(path) = path_of(world, held) else {
        return false;
    };
    if !is_under(&path, spec.accepted) {
        return false;
    }
    if l.core.is_some() {
        tell(world, "ядро уже есть!");
        return true;
    }
    crate::storage::put_inside(world, held, module);
    world
        .entity_mut(module)
        .insert(AnomalyLock { core: Some(held) });
    update_icon(world, module, true);
    crate::tools::tell(world, actor.client, Span::Notice, "ядро вставлено");
    if let Some(pos) = crate::explosions::turf_of(world, actor.mob) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, CLICK, 30.0, true));
    }
    true
}

/// `try_remove_core` after its `do_after`: the core (still there) to the user's hands.
pub fn complete_removal(world: &mut World, mob: Entity, client: Entity, module: Entity) {
    let Some((spec, l)) = lock(world, module) else {
        return;
    };
    let Some(core) = l.core.filter(|_| spec.removable) else {
        return;
    };
    crate::assemblies::server::to_hands(world, mob, core);
    world.entity_mut(module).insert(AnomalyLock { core: None });
    update_icon(world, module, false);
    crate::tools::tell(world, client, Span::Notice, "ядро извлечено");
}

/// `on_examine`.
pub fn examine_lines(world: &World, module: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(spec) = path_of(world, module).and_then(|p| spec(&p)) else {
        return;
    };
    let core = world
        .get::<AnomalyLock>(module)
        .and_then(|l| l.core)
        .filter(|&c| world.get::<StoredIn>(c).is_some_and(|s| s.0 == module));
    let text = match core {
        Some(c) => format!(
            "В нём установлено {}. {}",
            crate::chem::name_of(world, c),
            if spec.removable {
                "Его можно извлечь отвёрткой..."
            } else {
                "К сожалению, из-за особенностей конструкции его не извлечь."
            }
        ),
        None => {
            let need = if spec.accepted == GRAV {
                "гравитационное ядро аномалии"
            } else if spec.accepted == BLUESPACE {
                "блюспейс-ядро аномалии"
            } else {
                "ядро аномалии"
            };
            let mut t = format!("Чтобы модуль работал, вставьте {need}.");
            if !spec.removable {
                t.push_str(" Вставленное ядро будет не извлечь.");
            }
            t
        }
    };
    lines.push((Span::Notice, text));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_by_module_type() {
        let k = spec("/obj/item/mod/module/anomaly_locked/kinesis").unwrap();
        assert_eq!((k.accepted, k.prebuilt, k.removable), (GRAV, false, true));
        let p = spec("/obj/item/mod/module/anomaly_locked/kinesis/prototype").unwrap();
        assert!(p.prebuilt && !p.removable);
        let t = spec("/obj/item/mod/module/anomaly_locked/teleporter/prebuilt/locked").unwrap();
        assert_eq!(
            (t.accepted, t.prebuilt, t.removable),
            (BLUESPACE, true, false)
        );
        assert_eq!(
            spec("/obj/item/mod/module/anomaly_locked")
                .unwrap()
                .accepted,
            CORE
        );
        assert!(spec("/obj/item/mod/module/flashlight").is_none());
    }
}
