use super::*;

fn ctx() -> StageCtx {
    StageCtx {
        secs: 2.0,
        cure_status: 0.0,
        virus_resistant: false,
        malnourished: false,
        has_required_organ: true,
        immunity_recovery: 0.0,
        mood_recovery: 0.0,
        sleep_bonus: None,
    }
}

#[test]
fn cures_in_the_blood_cure() {
    let mut rng = CombatRng(7);
    let mut cured = 0;
    for _ in 0..200 {
        let mut cold = Disease::new(Kind::Cold);
        let c = StageCtx {
            cure_status: 1.0,
            ..ctx()
        };
        if stage_act(&mut cold, &c, &mut rng).0 == (Outcome::Cure { resistance: true }) {
            cured += 1;
        }
    }
    // 4% a second over 2 s: ~7.8%.
    assert!((5..=30).contains(&cured), "{cured}");
}

#[test]
fn stages_climb_in_half_steps_and_stop_at_max() {
    let mut rng = CombatRng(3);
    let mut gbs = Disease::new(Kind::Gbs);
    for _ in 0..2000 {
        stage_act(&mut gbs, &ctx(), &mut rng);
    }
    assert_eq!(gbs.stage, gbs.max_stages);
    assert!(gbs.stage_peaked);
}

#[test]
fn missing_organ_cures_without_immunity() {
    let mut rng = CombatRng(1);
    let mut cold = Disease::new(Kind::Cold);
    let c = StageCtx {
        has_required_organ: false,
        ..ctx()
    };
    assert_eq!(
        stage_act(&mut cold, &c, &mut rng).0,
        Outcome::Cure { resistance: false }
    );
}

#[test]
fn subtypes_are_the_same_disease() {
    let gbs = Disease::new(Kind::Gbs);
    let quiet = Disease::new(Kind::GbsNoTransmission);
    assert!(gbs.is_same(&quiet));
    assert!(!quiet.is_same(&gbs));
    assert!(!gbs.is_same(&Disease::new(Kind::FakeGbs)));
    assert_eq!(Kind::from_path("/datum/disease/flu"), Some(Kind::Flu));
}

#[test]
fn facing_covers_the_front_cone() {
    let at = TilePos::new(5, 5, 1);
    assert!(faces(Dir::North, at, TilePos::new(5, 8, 1)));
    assert!(faces(Dir::North, at, TilePos::new(6, 6, 1)));
    assert!(!faces(Dir::North, at, TilePos::new(6, 5, 1)));
    assert!(!faces(Dir::North, at, TilePos::new(5, 3, 1)));
}
