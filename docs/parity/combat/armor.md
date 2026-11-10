# Броня
Система `/datum/armor` и её применение к мобам и объектам. Сама броня-одежда — у агента items/security.

### Датум брони
Значения по типам: melee, bullet, laser, energy, bomb, bio, fire, acid, wound, consume; датумы кэшируются и создаются модификаторами/множителями (улучшения, повреждения брони).

- **Код:** [`/datum/armor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/armor/_armor.dm#L27), [`proc/get_rating()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/armor/_armor.dm#L141), [`proc/generate_new_with_modifiers()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/armor/_armor.dm#L72), [`proc/get_armor()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/armor/_atom_armor.dm#L2), [`proc/set_armor()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/armor/_atom_armor.dm#L12)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_mapload/src/guns.rs`, `crates/ss13_game/src/armor.rs`: рейтинги `armor_type` одежды (`DmArmor`), броня объектов в `crates/ss13_game/src/obj_damage.rs`

### Проверка брони моба
`run_armor_check` суммирует броню одежды, покрывающей зону (`body_parts_covered`), и врождённую; урон снижается на процент, `armor_penetration` оружия пробивает часть; сообщения «броня смягчила удар».

- **Код:** [`proc/run_armor_check()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L2), [`proc/getarmor()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L31)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/armor.rs`: `run_armor_check`/`getarmor` по зонам и `body_parts_covered`, пробитие, `weak_against_armour`, кап 90%, сообщения; в ближнем бою (`crates/ss13_game/src/combat.rs`) и для снарядов; броски и дубинки — без брони (shortcut)

### Броня от ран и био
Значение `wound` снижает шанс ран, `bio` — заражения болезнями и проникновения через кожу, `fire`/`acid` — защиту вещей от огня и кислоты.

- **Код:** [`proc/getarmor()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L31), [`proc/check_woundings_mods()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/wounds.dm#L223)
- **Статус в ss13-rs:** ✅ реализовано — `wound`: сумма рейтингов одежды на части тела (`crates/ss13_game/src/armor.rs` `worn_wound_armor`) плюс `wound_resistance` снижает бросок ран и снимает `exposed_wound_bonus` (`crates/ss13_game/src/medical/wounds/roll.rs`, `treat.rs`), в взрывах — шанс раны (`crates/ss13_game/src/explosions/ex_act.rs`); `bio`: заражение болезнями через рот и кожу (`crates/ss13_game/src/medical/diseases/spread.rs`), блоб, зомби, экзотические аномалии, слизь уборщика; `fire`: горящие и лавой подожжённые вещи (`crates/ss13_game/src/fire.rs`, `lava.rs`); нет `acid` у вещей (кислоты по вещам нет — у группы химии/ксено)
