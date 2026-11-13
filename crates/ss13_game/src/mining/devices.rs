//! Smaller mining devices (server), after tg `code/modules/mining/equipment/`: the lazarus
//! injector (`lazarus_injector.dm`).
//!
//! - A loaded injector used on a dead basic mob of organic sentience (`SENTIENCE_ORGANIC`: not
//!   megafauna, bots or humanoids) revives it fully (`revive(HEAL_ALL)`), adds the neutral
//!   faction and befriends the user (`lazarus_revive`), then is spent (`lazarus_empty`). A
//!   living or wrong creature is refused with a balloon.
//! - An EMP makes it malfunction: the revived mob is not befriended and, when a player controls
//!   it, is told the reviver is its master.
// shortcut: no wormhole jaunter, grappling gun, kheiral cuffs, vent pointer or weather radio yet;
// a malfunction does not enslave the revived mob's mind (`enslave_mind_to_creator`), and the
// mouse and bee item overrides (`COMSIG_ATOM_ON_LAZARUS_INJECTOR`) are missing.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::mobs::SENTIENCE_ORGANIC;
use ss13_shared::chat::Span;

use crate::examine::Line;
use crate::health::Stat;
use crate::mobs::BasicMob;
use crate::obj_damage::is_under;
use crate::sound::{PlaySound, playsound};

const LAZARUS: &str = "/obj/item/lazarus_injector";
const REFILL_SOUND: &str = "sound/effects/refill.ogg";
const GOOD_POLICY: &str =
    "Вы вернулись к жизни благодаря инъектору лазаруса! Вы теперь дружественны ко всем.";
const BAD_POLICY: &str = "Вы вернулись к жизни благодаря неисправному инъектору лазаруса! Вы \
                          теперь слуга того, кто вас вернул к жизни.";

/// A lazarus injector's `loaded` and `malfunctioning`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LazarusInjector {
    pub loaded: bool,
    pub malfunctioning: bool,
}

pub(super) struct DevicesPlugin;

impl Plugin for DevicesPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_item_added).add_systems(
            Update,
            injector_emps.in_set(crate::emp::EmpSystems::Effects),
        );
    }
}

fn on_item_added(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, LAZARUS))
    {
        commands.entity(add.entity).insert_if_new(LazarusInjector {
            loaded: true,
            malfunctioning: false,
        });
    }
}

/// `lazarus_injector/interact_with_atom`. True when handled.
pub fn item_interaction(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    let Some(injector) = world.get::<LazarusInjector>(held).copied() else {
        return false;
    };
    if !injector.loaded {
        return false;
    }
    let Some(sentience) = world.get::<BasicMob>(target).map(|b| b.def.sentience_type) else {
        return false;
    };
    // `compare_sentience_type(revive_type)`.
    if sentience != SENTIENCE_ORGANIC {
        crate::balloon::balloon_alert(world, mob, target, "invalid creature!");
        return true;
    }
    if world.get::<Stat>(target).is_none_or(|s| *s != Stat::Dead) {
        crate::balloon::balloon_alert(world, mob, target, "it's not dead!");
        return true;
    }
    lazarus_revive(world, target, mob, injector.malfunctioning);
    // `expend`.
    let user = crate::medical::name_of(world, mob);
    let name = crate::medical::name_of(world, target);
    let item = crate::medical::name_of(world, held);
    crate::medical::visible(
        world,
        mob,
        &format!("{user} injects {name} with {item}, reviving it."),
        &format!("{user} injects {name} with {item}, reviving it."),
        Span::Notice,
    );
    if let Some(mut i) = world.get_mut::<LazarusInjector>(held) {
        i.loaded = false;
    }
    if let Some(pos) = crate::chem::map_pos(world, held) {
        playsound(world, PlaySound::new(pos, REFILL_SOUND, 50.0, true));
    }
    crate::obj_damage::set_icon_state(world, held, "lazarus_empty");
    true
}

/// `/mob/living/lazarus_revive`.
pub fn lazarus_revive(world: &mut World, mob: Entity, reviver: Entity, malfunctioning: bool) {
    crate::spells::effects::full_heal(world, mob);
    match world.get_mut::<crate::mobs::Faction>(mob) {
        Some(mut f) => {
            if !f.0.iter().any(|x| x == crate::mobs::Faction::NEUTRAL) {
                f.0.push(crate::mobs::Faction::NEUTRAL.to_owned());
            }
        }
        None => {
            world.entity_mut(mob).insert(crate::mobs::Faction(vec![
                crate::mobs::Faction::NEUTRAL.to_owned(),
            ]));
        }
    }
    if !malfunctioning {
        crate::mobs::pets::befriend(world, mob, reviver);
    }
    if malfunctioning {
        let master = crate::medical::name_of(world, reviver);
        crate::tools::tell_mob(
            world,
            mob,
            Span::BoldWarning,
            format!(
                "{} - ваш хозяин. Помогайте хозяину во всем любой ценой.",
                ss13_shared::capitalize(&master)
            ),
        );
    }
    let policy = if malfunctioning {
        BAD_POLICY
    } else {
        GOOD_POLICY
    };
    crate::tools::tell_mob(world, mob, Span::BoldNotice, policy);
}

/// `lazarus_injector/emp_act`.
fn injector_emps(world: &mut World, mut cursor: Local<MessageCursor<crate::emp::EmpAct>>) {
    for ev in crate::emp::emp_acts(world, &mut cursor) {
        if !ev.hits_self() {
            continue;
        }
        if let Some(mut i) = world.get_mut::<LazarusInjector>(ev.target) {
            i.malfunctioning = true;
        }
    }
}

/// `lazarus_injector/examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<Line>) {
    let Some(i) = world.get::<LazarusInjector>(target) else {
        return;
    };
    let name = crate::examine::name_of(world, target);
    if !i.loaded {
        lines.push((Span::Info, format!("{name} is empty.")));
    }
    if i.malfunctioning {
        lines.push((
            Span::Info,
            format!("The display on {name} seems to be flickering."),
        ));
    }
}
