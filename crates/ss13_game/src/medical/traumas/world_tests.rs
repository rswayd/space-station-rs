use super::*;
use crate::health::Health;
use crate::medical::organs::{OrganHost, insert, remove, spawn_organ};

/// A conscious human with organs, limbs, hands and a tile.
fn human(world: &mut World) -> Entity {
    world.init_resource::<Time>();
    let mob = world
        .spawn((
            Health::default(),
            Stat::Conscious,
            OrganHost,
            Limbs::default(),
            Inventory::default(),
            TilePos::new(1, 1, 1),
        ))
        .id();
    for slot in Slot::ALL {
        let organ = spawn_organ(world, slot.spec().path).unwrap();
        world.entity_mut(organ).insert(OrganOf(mob));
    }
    mob
}

fn brain(world: &World, mob: Entity) -> Entity {
    organs::organ(world, mob, Slot::Brain).unwrap()
}

#[test]
fn traumas_follow_the_brain() {
    let mut world = World::new();
    let mob = human(&mut world);
    let b = brain(&world, mob);
    assert!(gain(&mut world, b, Kind::Mute, None));
    assert!(!gain(&mut world, b, Kind::Mute, None), "one of a type");
    assert!(gain(&mut world, b, Kind::Pacifism, None));
    let speech = |w: &World| w.get::<ss13_chat::speech::Impediments>(mob).copied();
    assert!(speech(&world).is_some_and(|s| s.mute && !s.stutter));
    assert!(world.get::<Pacifist>(mob).is_some());
    // Out of the body the effects go; back in, they come back.
    remove(&mut world, b, true);
    assert!(speech(&world).is_none() && world.get::<Pacifist>(mob).is_none());
    assert_eq!(world.get::<Traumas>(b).unwrap().0.len(), 2);
    insert(&mut world, b, mob);
    assert!(speech(&world).is_some_and(|s| s.mute));
    assert!(has(&world, mob, Kind::Pacifism));
    assert_eq!(
        scan_line(&world, mob).unwrap(),
        "Mental trauma: Severe Обширное повреждение речевого центра мозга, and Severe \
         Синдром пацифизма."
    );
}

#[test]
fn obsession_counts_time_away_and_goes_with_lobotomy() {
    let mut world = World::new();
    let mob = human(&mut world);
    let b = brain(&world, mob);
    let mut rng = CombatRng(5);
    // Nobody to obsess over: no trauma.
    assert!(!gain(&mut world, b, Kind::Obsessed, None));
    let target = world.spawn((Stat::Conscious, TilePos::new(20, 20, 1))).id();
    assert!(
        !gain_obsessed(&mut world, b, Some(mob), None, &mut rng),
        "not oneself"
    );
    assert!(gain_obsessed(&mut world, b, Some(target), None, &mut rng));
    let state = |w: &World| of_mob(w, mob)[0].obsession.unwrap();
    assert_eq!(state(&world).target, target);
    // Out of view: the time away adds up, the time near resets.
    life(&mut world, mob, 2.0);
    life(&mut world, mob, 2.0);
    let s = state(&world);
    assert!(!s.viewing);
    assert!((s.away - 4.0).abs() < 1e-4 && s.creeping == 0.0);
    // A new obsession starts over.
    let other = world.spawn((Stat::Conscious, TilePos::new(2, 2, 1))).id();
    assert!(set_obsession(&mut world, mob, other));
    assert_eq!(state(&world).target, other);
    assert_eq!(state(&world).away, 0.0);
    // Brain surgery does not take it; a lobotomy does.
    assert_eq!(cure_all(&mut world, b, Resilience::Surgery), 0);
    assert_eq!(cure_all(&mut world, b, Resilience::Lobotomy), 1);
    assert!(!has(&world, mob, Kind::Obsessed));
}

#[test]
fn special_and_magic_traumas_apply_their_traits() {
    let mut world = World::new();
    let mob = human(&mut world);
    let b = brain(&world, mob);
    // Tenacity: conscious down to death.
    assert!(gain(&mut world, b, Kind::Tenacity, None));
    let health = world.get::<Health>(mob).unwrap().clone();
    assert!(health.mods.no_softcrit && health.mods.no_hardcrit);
    let mut hurt = health.clone();
    hurt.adjust(crate::health::DamageType::Brute, 180.0);
    let mut stat = Stat::Conscious;
    crate::health::update_stat(&hurt, &mut stat);
    assert_eq!(stat, Stat::Conscious);
    remove_kind(&mut world, b, Kind::Tenacity);
    assert!(!world.get::<Health>(mob).unwrap().mods.no_hardcrit);
    // Antimagic and godwoken resist magic.
    use crate::spells::{MAGIC_RESISTANCE, MAGIC_RESISTANCE_HOLY, can_block_magic};
    assert!(!can_block_magic(&world, mob, MAGIC_RESISTANCE));
    assert!(gain(&mut world, b, Kind::Antimagic, None));
    assert!(gain(&mut world, b, Kind::Godwoken, None));
    assert!(can_block_magic(&world, mob, MAGIC_RESISTANCE));
    assert!(can_block_magic(&world, mob, MAGIC_RESISTANCE_HOLY));
    // Magic traumas are lobotomy-deep; the criminal's is hidden from scanners.
    assert_eq!(cure_all(&mut world, b, Resilience::Surgery), 1, "godwoken");
    assert!(has(&world, mob, Kind::Antimagic));
    assert!(gain(&mut world, b, Kind::Beepsky, None));
    assert_eq!(
        scan_line(&world, mob).unwrap(),
        "Mental trauma: Deep-rooted Таумическая пустота."
    );
    // Quantum alignment entangles with a held item before the floor.
    use super::super::special_traumas::entangle_target;
    assert_eq!(
        entangle_target(&world, mob),
        Some(Link::Turf(TilePos::new(1, 1, 1)))
    );
    let item = world.spawn(CarriedBy(mob)).id();
    world.get_mut::<Inventory>(mob).unwrap().hands[0] = Some(item);
    assert_eq!(entangle_target(&world, mob), Some(Link::Entity(item)));
}

#[test]
fn hypnosis_keeps_its_phrase_until_surgery() {
    let mut world = World::new();
    let mob = human(&mut world);
    let b = brain(&world, mob);
    let mut rng = CombatRng(3);
    // `New(phrase)` without a phrase: no trauma; the plain gain needs one too.
    assert!(!gain_hypnosis(&mut world, b, "  ", None, &mut rng));
    assert!(!gain(&mut world, b, Kind::Hypnosis, None));
    assert!(gain_hypnosis(
        &mut world,
        b,
        " Слушай клоуна ",
        None,
        &mut rng
    ));
    assert!(
        !gain_hypnosis(&mut world, b, "другое", None, &mut rng),
        "one of a type"
    );
    assert_eq!(
        hypnotic_phrase(&world, mob).as_deref(),
        Some("Слушай клоуна")
    );
    assert_eq!(
        scan_line(&world, mob).unwrap(),
        "Mental trauma: Severe Повторяющийся мыслительный паттерн."
    );
    // Neurine's basic cure leaves it; brain surgery takes it.
    assert_eq!(cure_all(&mut world, b, Resilience::Basic), 0);
    assert_eq!(cure_all(&mut world, b, Resilience::Surgery), 1);
    assert_eq!(hypnotic_phrase(&world, mob), None);
}

#[test]
fn paralysis_disables_limbs_and_drops_the_hand() {
    let mut world = World::new();
    let mob = human(&mut world);
    let item = world.spawn(CarriedBy(mob)).id();
    world.get_mut::<Inventory>(mob).unwrap().hands[0] = Some(item);
    let b = brain(&world, mob);
    gain(&mut world, b, Kind::Paralysis(Paralysis::Left), None);
    let limbs = world.get::<Limbs>(mob).unwrap();
    assert!(limbs.limb(BodyZone::LArm).paralyzed && limbs.limb(BodyZone::LLeg).paralyzed);
    assert!(!limbs.limb(BodyZone::RLeg).paralyzed);
    // One leg: slower, not floored; the left hand let go and holds nothing.
    let limits = limbs.limits(world.get::<Health>(mob).unwrap());
    assert!(limits.slowdown >= 3.0 && !limits.floored);
    assert!(world.get::<TilePos>(item).is_some());
    assert!(!crate::medical::bodyparts::has_hand(&world, mob, 0));
    assert!(crate::medical::bodyparts::has_hand(&world, mob, 1));
    assert_eq!(cure_all(&mut world, b, Resilience::Surgery), 1);
    assert!(
        !world
            .get::<Limbs>(mob)
            .unwrap()
            .limb(BodyZone::LArm)
            .paralyzed
    );
}

#[test]
fn cures_go_by_resilience() {
    let mut world = World::new();
    let mob = human(&mut world);
    let b = brain(&world, mob);
    gain(&mut world, b, Kind::Stuttering, None);
    gain(&mut world, b, Kind::Mute, None);
    gain(&mut world, b, Kind::Pacifism, Some(Resilience::Lobotomy));
    gain(&mut world, b, Kind::Dumbness, Some(Resilience::Magic));
    let mut rng = CombatRng(7);
    // Neurine's cure takes a basic one only.
    cure_one(&mut world, b, Resilience::Basic, &mut rng);
    assert!(!has(&world, mob, Kind::Stuttering) && has(&world, mob, Kind::Mute));
    cure_one(&mut world, b, Resilience::Basic, &mut rng);
    assert!(has(&world, mob, Kind::Mute));
    // Brain surgery, then a lobotomy; magic stays.
    assert_eq!(cure_all(&mut world, b, Resilience::Surgery), 1);
    assert!(has(&world, mob, Kind::Pacifism));
    assert_eq!(cure_all(&mut world, b, Resilience::Lobotomy), 1);
    assert!(has(&world, mob, Kind::Dumbness));
    assert!(
        world
            .get::<ss13_chat::speech::Impediments>(mob)
            .is_some_and(|s| s.derp)
    );
}

#[test]
fn brain_damage_rolls_traumas() {
    let mut world = World::new();
    let mob = human(&mut world);
    world.insert_resource(CombatRng(3));
    let b = brain(&world, mob);
    // Below the mild threshold nothing rolls.
    organs::set_organ_damage(&mut world, mob, Slot::Brain, 20.0);
    assert!(of_mob(&world, mob).is_empty());
    // 50 more at 150: the mild roll is sure (50 x 2.3), the severe one 75%.
    organs::set_organ_damage(&mut world, mob, Slot::Brain, 70.0);
    organs::adjust_organ_loss(&mut world, mob, Slot::Brain, 80.0);
    let all = of_mob(&world, mob);
    assert!(
        all.iter().any(|t| t.kind.def().category == Category::Mild),
        "{all:?}"
    );
    // Natural gains stop at the tier limits whatever the damage.
    for _ in 0..40 {
        organs::set_organ_damage(&mut world, mob, Slot::Brain, 110.0);
        organs::adjust_organ_loss(&mut world, mob, Slot::Brain, 80.0);
    }
    let all = world.get::<Traumas>(b).unwrap().0.clone();
    let count = |r| all.iter().filter(|t| t.resilience == r).count();
    assert_eq!(count(Resilience::Basic), 3, "{all:?}");
    assert_eq!(count(Resilience::Surgery), 2, "{all:?}");
}

#[test]
fn healthy_and_weakness_act_on_life() {
    let mut world = World::new();
    let mob = human(&mut world);
    let b = brain(&world, mob);
    gain(&mut world, b, Kind::Healthy, None);
    crate::status_effects::adjust_stamina(&mut world, mob, 40.0);
    life(&mut world, mob, 2.0);
    let stamina = world
        .get::<crate::status_effects::StatusEffects>(mob)
        .unwrap()
        .stamina;
    assert_eq!(stamina, 28.0);
    // Possessiveness holds on to a held item for a while, then lets go.
    cure_all(&mut world, b, Resilience::Basic);
    gain(&mut world, b, Kind::Possessive, None);
    let item = world.spawn(CarriedBy(mob)).id();
    world.get_mut::<Inventory>(mob).unwrap().hands = [Some(item), Some(item)];
    world.insert_resource(CombatRng(1));
    for _ in 0..200 {
        life(&mut world, mob, 2.0);
        if world.get::<NoDrop>(item).is_some() {
            break;
        }
    }
    assert!(world.get::<NoDrop>(item).is_some(), "5% a second");
    // Taking the brain out lets go (no `on_life` holds it any more).
    remove(&mut world, b, true);
    assert!(world.get::<NoDrop>(item).is_none());
    insert(&mut world, b, mob);
    for _ in 0..200 {
        life(&mut world, mob, 2.0);
        if world.get::<NoDrop>(item).is_some() {
            break;
        }
    }
    cure_all(&mut world, b, Resilience::Basic);
    assert!(world.get::<NoDrop>(item).is_none(), "the cure lets go");
}
