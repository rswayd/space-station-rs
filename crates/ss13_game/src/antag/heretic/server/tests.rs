use super::*;

#[test]
fn objectives_explain_and_complete() {
    let mut h = Heretic::new("k".into(), 0.0);
    assert!(!HereticObjective::PickPath.complete(&h));
    let minor = HereticObjective::MinorSacrifice { target: 4 };
    assert_eq!(
        minor.explanation(),
        "Принесите в жертву не менее 4 членов экипажа."
    );
    h.total_sacrifices = 4;
    assert!(minor.complete(&h));
    assert!(!HereticObjective::MajorSacrifice.complete(&h));
    h.high_value_sacrifices = 1;
    assert!(HereticObjective::MajorSacrifice.complete(&h));
    h.objectives = vec![minor, HereticObjective::MajorSacrifice];
    assert_eq!(h.can_ascend(), Ok(()));
    h.feast_of_owls = true;
    assert!(h.can_ascend().is_err());
}

#[test]
fn points_cross_the_aura_threshold() {
    let mut h = Heretic::new("k".into(), 0.0);
    assert_eq!(h.points, 1);
    assert!(!h.adjust_points(8));
    assert!(h.adjust_points(1), "nine gained is more than eight");
    assert_eq!(h.points, 10);
    h.adjust_points(-20);
    assert_eq!((h.points, h.knowledge_gained), (0, 9));
}

#[test]
fn influences_per_heretic_follow_tg() {
    let total = |n: u32| -> u32 {
        (1..=n)
            .map(|i| (NUM_INFLUENCES_PER_HERETIC + 1).saturating_sub(i).max(1))
            .sum()
    };
    assert_eq!(total(1), 5);
    assert_eq!(total(2), 9);
    assert_eq!(total(6), 5 + 4 + 3 + 2 + 1 + 1);
}

#[test]
fn the_heart_says_how_far() {
    let at = |x, y| TilePos::new(x, y, 1);
    assert_eq!(
        heart_message(at(1, 1), Some(at(5, 1)), false),
        "очень близко, east!"
    );
    assert_eq!(
        heart_message(at(1, 1), Some(at(1, 30)), false),
        "близко, north!"
    );
    assert_eq!(
        heart_message(at(50, 50), Some(at(10, 10)), true),
        "мертвы, далеко, southwest!"
    );
    assert_eq!(
        heart_message(at(1, 1), Some(TilePos::new(1, 1, 2)), false),
        "в другом мире!"
    );
    assert_eq!(
        heart_message(at(1, 1), Some(at(200, 1)), false),
        "очень далеко!"
    );
}

#[test]
fn ascension_text_fills_name_and_spooks() {
    let mut n = 0;
    let text = ascension_text(
        "%SPOOKY% Лорд %NAME% вознесся! %SPOOKY%",
        "Боб",
        || {
            n += 1;
            format!("<{n}>")
        },
    );
    assert_eq!(text, "<1> Лорд Боб вознесся! <2>");
}
