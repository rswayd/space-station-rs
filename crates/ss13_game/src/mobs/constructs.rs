//! Cult constructs and shades as mobs (server), after tg `/mob/living/basic/construct`
//! (`code/modules/mob/living/basic/cult/constructs/`) and `/mob/living/basic/shade` (`shade.dm`):
//! the hostile NPC constructs' trees (`crate::mobs::ai::trees`, `construct_ai.dm`), the
//! artificer's `healing_touch` (5 brute on a hurt construct or shade, itself included), the
//! theme's ectoplasm left on death (`death_drops` of `remains_by_theme`) and the dents on
//! examine. Making them (soulstones, shells, the cult's summons) belongs to the cult
//! (`docs/parity/antagonists/cult.md`).
// Left out: the construct spells (player actions), wall tearing, the juggernaut's
// `damage_threshold` and beam reflection, the wraith's recharging jaunt, the harvester's
// amputations and Nar'Sie seeking, cult pylon healing, structure repair, the theme's glow overlay.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::MobDef;
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};

use super::BasicMob;
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health, Stat};

pub const CONSTRUCT: &str = "/mob/living/basic/construct";
pub const SHADE: &str = "/mob/living/basic/shade";

/// `remains_by_theme`: cult (and heretic), holy, wizard (`GAME_RUNTIME_TYPES`).
pub const ECTOPLASM: &[&str] = &[
    "/obj/item/ectoplasm/construct",
    "/obj/item/ectoplasm/angelic",
    "/obj/item/ectoplasm/mystic",
];

/// `healing_touch` `heal_brute` of constructs.
pub const HEAL_BRUTE: f32 = 5.0;

/// `isconstruct(target) || istype(target, /mob/living/basic/shade)`.
pub fn is_construct_or_shade(world: &World, mob: Entity) -> bool {
    world
        .get::<BasicMob>(mob)
        .is_some_and(|b| is_under(&b.def.path, CONSTRUCT) || is_under(&b.def.path, SHADE))
}

/// The ectoplasm of type `path` by its `theme` (`angelic` THEME_HOLY, `mystic` THEME_WIZARD,
/// else THEME_CULT); `None` for anything but a construct or shade.
pub fn remains_of(path: &str) -> Option<&'static str> {
    if !is_under(path, CONSTRUCT) && !is_under(path, SHADE) {
        return None;
    }
    let themed = |theme: &str| {
        ["artificer", "juggernaut", "wraith"]
            .iter()
            .any(|t| is_under(path, &format!("{CONSTRUCT}/{t}/{theme}")))
    };
    Some(if themed("angelic") {
        ECTOPLASM[1]
    } else if themed("mystic") {
        ECTOPLASM[2]
    } else {
        ECTOPLASM[0]
    })
}

/// `can_repair` (artificers, harvesters but the heretic's) and `can_repair_self` (artificers).
fn repairs(path: &str) -> (bool, bool) {
    let artificer = is_under(path, &format!("{CONSTRUCT}/artificer"));
    let harvester = is_under(path, &format!("{CONSTRUCT}/harvester"))
        && !is_under(path, &format!("{CONSTRUCT}/harvester/heretic"));
    (artificer || harvester, artificer)
}

fn say_around(world: &mut World, pos: TilePos, text: String) {
    let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
    world.write_message(SendChat {
        clients,
        message: ChatMessage::simple(ChatChannel::Info, Span::Notice, &text),
    });
}

/// `healing_touch/try_healing` of a construct's unarmed attack on `target`: true when the attack
/// became a repair (or was cancelled by it), false to attack as usual.
pub fn healing_touch(world: &mut World, healer: Entity, target: Entity) -> bool {
    let Some(path) = world.get::<BasicMob>(healer).map(|b| b.def.path.clone()) else {
        return false;
    };
    let (can_repair, self_too) = repairs(&path);
    if !can_repair || !is_construct_or_shade(world, target) {
        return false;
    }
    if target == healer && !self_too {
        return true;
    }
    let Some(health) = world.get::<Health>(target) else {
        return true;
    };
    // "not hurt!", "can't heal that!" (no brute), "they're dead!".
    if health.health() >= health.max
        || health.brute <= 0.0
        || world.get::<Stat>(target) == Some(&Stat::Dead)
    {
        return true;
    }
    let name = |e: Entity| super::defense::name_of(world, e);
    let (who, whom) = (name(healer), name(target));
    let pos = world.get::<TilePos>(target).copied();
    if let Some(pos) = pos {
        say_around(world, pos, format!("{who} начинает чинить вмятины {whom}."));
    }
    if let Some(mut health) = world.get_mut::<Health>(target) {
        health.adjust(DamageType::Brute, -HEAL_BRUTE);
    }
    if let Some(pos) = pos {
        say_around(world, pos, format!("Вмятины {whom} починены."));
    }
    true
}

/// `death_drops` of a dead construct or shade at `pos`.
pub(super) fn on_death(world: &mut World, def: &MobDef, pos: TilePos) {
    if let Some(remains) = remains_of(&def.path) {
        crate::construction::spawn_obj(world, remains, pos);
    }
}

/// `/mob/living/basic/construct/examine`: how dented it looks.
pub fn examine_lines(world: &World, mob: Entity, lines: &mut Vec<crate::examine::Line>) {
    let construct = world
        .get::<BasicMob>(mob)
        .is_some_and(|b| is_under(&b.def.path, CONSTRUCT));
    let Some(health) = world.get::<Health>(mob).filter(|_| construct) else {
        return;
    };
    let hp = health.health();
    if hp >= health.max {
        return;
    }
    if hp >= health.max / 2.0 {
        lines.push((Span::Warning, "Выглядит слегка помятым.".to_owned()));
    } else {
        lines.push((Span::Danger, "Выглядит сильно помятым!".to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remains_follow_the_theme() {
        assert_eq!(
            remains_of("/mob/living/basic/construct/juggernaut/hostile"),
            Some(ECTOPLASM[0])
        );
        assert_eq!(
            remains_of("/mob/living/basic/construct/wraith/angelic"),
            Some(ECTOPLASM[1])
        );
        assert_eq!(
            remains_of("/mob/living/basic/construct/artificer/mystic"),
            Some(ECTOPLASM[2])
        );
        assert_eq!(remains_of("/mob/living/basic/shade"), Some(ECTOPLASM[0]));
        assert_eq!(remains_of("/mob/living/basic/carp"), None);
    }

    #[test]
    fn ectoplasm_is_a_runtime_type() {
        let registry = ss13_mapload::runtime::runtime_types();
        assert!(ECTOPLASM.iter().all(|p| registry.contains(p)));
    }

    #[test]
    fn artificers_repair_constructs_and_themselves() {
        assert_eq!(
            repairs("/mob/living/basic/construct/artificer/hostile"),
            (true, true)
        );
        assert_eq!(
            repairs("/mob/living/basic/construct/harvester"),
            (true, false)
        );
        assert_eq!(
            repairs("/mob/living/basic/construct/harvester/heretic"),
            (false, false)
        );
        assert_eq!(
            repairs("/mob/living/basic/construct/juggernaut"),
            (false, false)
        );
    }

    #[test]
    fn healing_touch_mends_five_brute_of_a_hurt_construct() {
        let mut world = World::new();
        world.init_resource::<Messages<SendChat>>();
        let mob = |world: &mut World, path: &str, brute: f32| {
            let mut health = Health::basic(150.0, [1.0; 5]);
            health.adjust(DamageType::Brute, brute);
            world
                .spawn((
                    BasicMob {
                        def: std::sync::Arc::new(MobDef {
                            path: path.to_owned(),
                            ..Default::default()
                        }),
                        icon_living: String::new(),
                        icon_dead: String::new(),
                        mouth: false,
                    },
                    health,
                    Stat::Conscious,
                ))
                .id()
        };
        let artificer = mob(
            &mut world,
            "/mob/living/basic/construct/artificer/hostile",
            0.0,
        );
        let juggernaut = mob(
            &mut world,
            "/mob/living/basic/construct/juggernaut/hostile",
            20.0,
        );
        let carp = mob(&mut world, "/mob/living/basic/carp", 20.0);
        assert!(healing_touch(&mut world, artificer, juggernaut));
        assert_eq!(world.get::<Health>(juggernaut).unwrap().brute, 15.0);
        // Not a construct: the artificer punches it instead.
        assert!(!healing_touch(&mut world, artificer, carp));
        // A juggernaut cannot repair.
        assert!(!healing_touch(&mut world, juggernaut, artificer));
    }
}
