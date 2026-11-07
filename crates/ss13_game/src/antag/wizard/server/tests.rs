use super::*;

#[test]
fn entries_are_unique_and_cost_what_tg_says() {
    for (i, e) in ENTRIES.iter().enumerate() {
        assert_eq!(entry(e.id).map(|(j, _)| j), Some(i), "{}", e.id);
    }
    let cost = |id: &str| entry(id).unwrap().1.cost;
    assert_eq!(cost("/datum/spellbook_entry/fireball"), 2);
    assert_eq!(cost("/datum/spellbook_entry/blind"), 1);
    assert_eq!(cost("/datum/spellbook_entry/item/contract"), 2);
    let teleport = entry("/datum/spellbook_entry/teleport").unwrap().1;
    assert_eq!(teleport.limit(), Some(3));
    assert!(teleport.refundable());
    let staff = entry("/datum/spellbook_entry/item/staffhealing").unwrap().1;
    assert_eq!((staff.limit(), staff.refundable()), (None, false));
    assert_eq!(staff.buy_word(), "Призвать");
}

#[test]
fn rituals_need_a_high_chaos_book() {
    let book = Spellbook::default();
    let (i, events) = entry("/datum/spellbook_entry/summon/events").unwrap();
    assert!(!book.offers(i));
    assert_eq!((events.limit(), events.refundable()), (Some(5), false));
    assert_eq!(events.buy_word(), "Провести");
    let chaos = Spellbook {
        rituals: true,
        ..default()
    };
    assert!(chaos.offers(i));
    let st = SpellbookUi::static_data(&chaos);
    assert_eq!(st.entries.len(), ENTRIES.len());
    assert_eq!(st.categories.last().map(String::as_str), Some(RITUALS));
    let data = SpellbookUi::data(&chaos, Entity::PLACEHOLDER, &());
    assert_eq!(data.times.len(), st.entries.len());
    assert_eq!(data.cooldowns.len(), st.entries.len());
}

#[test]
fn static_data_lists_categories_in_book_order() {
    let st = SpellbookUi::static_data(&Spellbook::default());
    assert_eq!(st.categories, [OFFENSIVE, DEFENSIVE, MOBILITY, ASSISTANCE]);
    assert_eq!(st.entries.len(), ENTRIES.len() - 2, "no rituals");
    let data = SpellbookUi::data(&Spellbook::default(), Entity::PLACEHOLDER, &());
    assert_eq!(data.times.len(), st.entries.len());
    let smoke = st.entries.iter().find(|e| e.name == "Smoke").unwrap();
    assert!(!smoke.requires_wizard_garb);
    let fireball = st.entries.iter().find(|e| e.name == "Fireball").unwrap();
    // `SPELL_REQUIRES_NO_ANTIMAGIC` only.
    assert!(!fireball.requires_wizard_garb);
    let missile = st
        .entries
        .iter()
        .find(|e| e.name == "Magic Missile")
        .unwrap();
    assert!(missile.requires_wizard_garb);
}

#[test]
fn apprentice_schools_teach_their_spells() {
    assert_eq!(
        School::Destruction.spells(),
        &[Spell::MagicMissile, Spell::Fireball]
    );
    assert_eq!(
        School::ALL.map(School::key),
        ["destruction", "bluespace", "healing", "robeless"]
    );
}
