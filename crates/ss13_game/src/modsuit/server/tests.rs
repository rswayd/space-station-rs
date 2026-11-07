use super::*;

#[test]
fn slots_follow_item_slot_bits() {
    assert_eq!(slot_of_flags(1 << 6), Some(InventorySlot::Head));
    assert_eq!(slot_of_flags(1 << 0), Some(InventorySlot::Suit));
    assert_eq!(slot_of_flags(1 << 2), Some(InventorySlot::Gloves));
    assert_eq!(slot_of_flags(1 << 7), Some(InventorySlot::Shoes));
    assert_eq!(slot_of_flags(1 << 10), Some(InventorySlot::Back));
    assert_eq!(base_of("/obj/item/clothing/shoes/mod"), "boots");
}

#[test]
fn plasma_refuels_only_what_fits() {
    // 100 kJ core at 75 kJ: two sheets (20 kJ each) fill it, the rest stays.
    assert_eq!(plasma_uses(75_000.0, 100_000.0, 20_000.0, 5), 2);
    assert_eq!(plasma_uses(75_000.0, 100_000.0, 20_000.0, 1), 1);
    assert_eq!(plasma_uses(100_000.0, 100_000.0, 20_000.0, 5), 0);
}
