use super::*;

#[test]
fn implants_and_bioware_need_their_zone_and_state() {
    let mut world = World::new();
    world.init_resource::<Time>();
    let mob = world
        .spawn((
            Health::default(),
            Stat::Conscious,
            organs::OrganHost,
            Limbs::default(),
            BodyPosition::default(),
        ))
        .id();
    let tube =
        cyberimps::spawn_implant(&mut world, "/obj/item/organ/cyberimp/mouth/breathing_tube")
            .unwrap();
    assert!(cyberimps::insert(
        &mut world,
        tube,
        mob,
        ImplantSlot::BreathingTube
    ));
    let remove = Op::RemoveImplant(ImplantSlot::BreathingTube);
    assert!(!is_available(&world, remove, mob, TargetZone::Mouth));
    with_limbs(&mut world, mob, |l| {
        let head = l.limb_mut(BodyZone::Head);
        head.prepped = true;
        head.add_surgery(state::SKIN_OPEN | state::ORGANS_CUT);
    });
    assert!(is_available(&world, remove, mob, TargetZone::Mouth));
    assert!(!is_available(&world, remove, mob, TargetZone::Head));
    // Chest implants and bioware need the chest's bone sawed too.
    with_limbs(&mut world, mob, |l| {
        let chest = l.limb_mut(BodyZone::Chest);
        chest.prepped = true;
        chest.add_surgery(state::SKIN_OPEN | state::ORGANS_CUT);
    });
    let veins = Op::Bioware(Bioware::VeinThreading);
    assert!(!is_available(
        &world,
        Op::InsertImplant,
        mob,
        TargetZone::Chest
    ));
    assert!(!is_available(&world, veins, mob, TargetZone::Chest));
    with_limbs(&mut world, mob, |l| {
        l.limb_mut(BodyZone::Chest).add_surgery(state::BONE_SAWED)
    });
    assert!(is_available(
        &world,
        Op::InsertImplant,
        mob,
        TargetZone::Chest
    ));
    assert!(is_available(&world, veins, mob, TargetZone::Chest));
    // One circulation bioware per patient.
    super::super::bioware::gain(&mut world, mob, Bioware::MuscledVeins);
    assert!(!is_available(&world, veins, mob, TargetZone::Chest));
    // Bioware is locked: only disks and research offer it.
    assert!(!operation_pool(&[]).contains(&veins));
    assert!(operation_pool(&[veins.def().path]).contains(&veins));
}

#[test]
fn blood_filter_takes_a_share_of_each_reagent() {
    use ss13_reagents::{Holder, Owner, Reagent};
    let mut world = World::new();
    let reagent = |id, volume| Reagent {
        id,
        volume,
        purity: 1.0,
        creation_purity: 1.0,
        ph: 7.0,
        metabolizing: false,
        overdosed: false,
        current_cycle: 0,
        overdose_threshold: 0.0,
        metabolization_rate: 0.2,
        data: None,
    };
    let mut blood = Holder::new(1000.0, 0, Owner::default());
    blood.reagents = vec![reagent(1, 100.0), reagent(2, 10.0), reagent(3, 0.5)];
    blood.update_total();
    let mob = world
        .spawn(crate::chem::MobReagents {
            blood,
            stomach: Holder::new(1000.0, 0, Owner::default()),
            drunk: 0.0,
        })
        .id();
    filter_blood(&mut world, mob);
    let left: Vec<f32> = world
        .get::<crate::chem::MobReagents>(mob)
        .unwrap()
        .blood
        .reagents
        .iter()
        .map(|r| r.volume)
        .collect();
    // 22%: capped at 10, 2.2 rounded to 0.2 steps, at least 0.4.
    let want = [90.0, 7.8, 0.1];
    assert!(
        left.iter().zip(want).all(|(l, w)| (l - w).abs() < 1e-3),
        "{left:?}"
    );
    assert!(can_loop(&mut world, Op::FilterBlood, mob));
    world
        .get_mut::<crate::chem::MobReagents>(mob)
        .unwrap()
        .blood
        .reagents
        .clear();
    assert!(!can_loop(&mut world, Op::FilterBlood, mob));
}

fn tool(path: &str, behaviour: &str, sharpness: u32, force: f32) -> ToolInfo {
    ToolInfo {
        path: Some(path.into()),
        behaviour: behaviour.into(),
        toolspeed: 1.0,
        sharpness,
        force,
        temperature: 0.0,
        shock_ready: false,
    }
}

#[test]
fn tool_quality_follows_implements() {
    let scalpel = tool("/obj/item/scalpel", "scalpel", 1, 10.0);
    assert_eq!(tool_quality(Op::InciseSkin, &scalpel), 1.0);
    let knife = tool("/obj/item/knife/kitchen", "knife", 1, 10.0);
    assert_eq!(tool_quality(Op::InciseSkin, &knife), 1.5);
    // Any edged item: `/obj/item` 3.33; a blunt one cannot cut.
    let glass = tool("/obj/item/broken_bottle", "", 1, 9.0);
    assert_eq!(tool_quality(Op::InciseSkin, &glass), 3.33);
    let toolbox = tool("/obj/item/storage/toolbox", "", 0, 12.0);
    assert_eq!(tool_quality(Op::InciseSkin, &toolbox), 0.0);
    // A saw needs an edge with 10 force, or the saw behaviour.
    assert_eq!(tool_quality(Op::SawBones, &knife), 4.0);
    assert_eq!(tool_quality(Op::SawBones, &glass), 0.0);
    let saw = tool("/obj/item/circular_saw", "saw", 1, 15.0);
    assert_eq!(tool_quality(Op::SawBones, &saw), 1.0);
    // Closing needs heat (a lit cautery) or sutures.
    let mut cautery = tool("/obj/item/cautery", "cautery", 0, 5.0);
    assert_eq!(tool_quality(Op::CloseSkin, &cautery), 0.0);
    cautery.temperature = 500.0;
    assert_eq!(tool_quality(Op::CloseSkin, &cautery), 1.0);
    // Hands reset a dislocation at 5×.
    assert_eq!(
        tool_quality(Op::RepairDislocation, &ToolInfo::default()),
        5.0
    );
    let hemostat = tool("/obj/item/hemostat", "hemostat", 0, 0.0);
    assert_eq!(
        tool_quality(Op::TendWounds(Tend::Basic, Heal::Brute), &hemostat),
        1.0
    );
    let mut fast = hemostat.clone();
    fast.toolspeed = 0.5;
    assert_eq!(tool_quality(Op::ClampBleeders, &fast), 0.5);
}

#[test]
fn surgery_step_success_modifiers() {
    assert_eq!(
        location_modifier(&["/obj/structure/table/optable".into()]),
        1.0
    );
    assert_eq!(
        location_modifier(&["/obj/structure/table/reinforced".into()]),
        1.25
    );
    assert_eq!(location_modifier(&["/obj/structure/bed".into()]), 1.5);
    assert_eq!(location_modifier(&[]), 2.0);
    assert_eq!(
        location_modifier(&[
            "/obj/structure/bed".into(),
            "/obj/structure/table/optable".into()
        ]),
        1.0
    );
    // A scalpel on an operating table: speed 1, no time penalty.
    assert_eq!(speed_modifier(1.0, 1.0, 1.0, 1.0), 1.0);
    assert_eq!(base_failure_chance(16.0, 1.0).clamp(0.0, 99.0), 0.0);
    // A conscious patient: +40; a basic tool: +5; in the dark up to +60.
    let scalpel = tool("/obj/item/scalpel", "scalpel", 1, 10.0);
    let basic = tool_mod(Op::InciseSkin, &scalpel);
    assert_eq!(basic, 5.0);
    assert_eq!(failure_chance(16.0, 1.0, false, None, basic), 45.0);
    assert_eq!(failure_chance(16.0, 1.0, true, None, basic), 5.0);
    let advanced = tool("/obj/item/scalpel/advanced", "scalpel", 1, 10.0);
    let advanced = tool_mod(Op::InciseSkin, &advanced);
    assert_eq!(failure_chance(16.0, 1.0, true, None, advanced), 0.0);
    assert_eq!(failure_chance(16.0, 1.0, true, Some(0.0), advanced), 60.0);
    // An improvised implement (listed above 1.15): +10, whatever its type.
    let knife = tool("/obj/item/knife/kitchen", "knife", 1, 10.0);
    assert_eq!(tool_mod(Op::InciseSkin, &knife), 10.0);
    assert_eq!(tool_mod(Op::RepairDislocation, &ToolInfo::default()), 10.0);
    // A kitchen knife on the floor: 1.5 × 2 = 3 -> 0.5 × 1.6 s over the threshold = 8%.
    let speed = speed_modifier(1.5, 1.0, 2.0, 1.0);
    assert_eq!(speed, 3.0);
    assert!((base_failure_chance(16.0, speed) - 8.0).abs() < 1e-4);
    // Asleep or numbed: ×0.8.
    let numb = speed_modifier(1.5, 1.0, 2.0, patient_speed_mod(true));
    assert!((numb - 2.4).abs() < 1e-5);
    assert!(base_failure_chance(16.0, 2.4) < 0.0);
}

#[test]
fn drunk_surgeons_are_slow_except_at_the_ballmer_peak() {
    assert_eq!(surgeon_speed_mod(0.0), 1.0);
    assert_eq!(surgeon_speed_mod(13.0), 0.8);
    // 30^1.5 / 90 = 1.83 -> 1.8.
    assert!((surgeon_speed_mod(30.0) - 2.8).abs() < 1e-5);
}

#[test]
fn locked_operations_and_replacements() {
    let unlocked = operation_pool(&[]);
    assert!(unlocked.contains(&Op::TendWounds(Tend::Basic, Heal::Brute)));
    assert!(
        !unlocked
            .iter()
            .any(|op| matches!(op, Op::TendWounds(t, _) if *t != Tend::Basic))
    );
    // The upgrade replaces the basic tending; the combo sits next to it.
    let upgraded = operation_pool(&[Tend::Upgraded.path(), Tend::Combo.path()]);
    assert!(!upgraded.contains(&Op::TendWounds(Tend::Basic, Heal::Brute)));
    assert!(upgraded.contains(&Op::TendWounds(Tend::Upgraded, Heal::Burn)));
    assert!(upgraded.contains(&Op::TendWounds(Tend::Combo, Heal::Combo)));
    // The best combo replaces every tending below it, down the chain.
    let master = operation_pool(&[
        Tend::Upgraded.path(),
        Tend::Master.path(),
        Tend::ComboMaster.path(),
    ]);
    let tends: Vec<_> = master
        .iter()
        .filter(|op| matches!(op, Op::TendWounds(..)))
        .collect();
    assert_eq!(tends, [&Op::TendWounds(Tend::ComboMaster, Heal::Combo)]);
    // A disk path the port has no operation for is ignored.
    assert_eq!(
        operation_pool(&["/datum/surgery_operation/organ/brainwash/mechanic"]).len(),
        unlocked.len()
    );
    // The brainwashing disk unlocks the brainwash (locked without it).
    assert!(!unlocked.contains(&Op::Brainwash));
    assert!(operation_pool(&["/datum/surgery_operation/organ/brainwash"]).contains(&Op::Brainwash));
    assert_eq!(Op::BrainwashSleeper.def().time, Op::Brainwash.def().time);
    // Every type maps back to itself.
    for op in Op::types() {
        assert_eq!(Op::of_path(op.def().path), Some(op));
    }
}

#[test]
fn requirements_read_like_tg() {
    let [all, any, optional, blocked] = Op::InciseSkin.requirements();
    assert_eq!(all, ["пациент должен лежать"]);
    assert!(any.is_empty() && optional.is_empty());
    assert_eq!(
        blocked,
        [
            "кожа должна быть целой",
            "рабочая область не должна быть закрыта одеждой"
        ]
    );
    let [all, ..] = Op::Revival.requirements();
    assert_eq!(all[0], "операция на голова (цель: голова)");
    assert!(all.contains(&"требуется, чтобы кость была распилена".to_owned()));
    let [_, any, ..] = Op::TendWounds(Tend::Basic, Heal::Brute).requirements();
    assert_eq!(any, ["кожа должна быть разрезана или вскрыта"]);
}
