# Части тела
Конечности, голова и грудь как объекты `/obj/item/bodypart`: урон, отключение, протезы, приживление. Расчленение — ../combat/embedding-dismemberment.md.

### Часть тела: урон и лечение
Каждая часть хранит свой brute/burn до `max_damage`, модификаторы урона, раны и встрявшие предметы; урон переливается в общее здоровье; лечение идёт по частям.

- **Код:** [`/obj/item/bodypart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/_bodyparts.dm#L1), [`proc/receive_damage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/_bodyparts.dm#L704), [`proc/heal_damage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/_bodyparts.dm#L888)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/health.rs`: brute/burn по шести частям с `max_damage` tg; нет частей как предметов, модификаторов, ран

### Отключение конечностей
Конечность с уроном на максимуме или с тяжёлой раной отключается: рука не держит, нога не ходит (ползание, падение).

- **Код:** [`proc/update_disabled()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/_bodyparts.dm#L934), [`proc/set_disabled()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/_bodyparts.dm#L975)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/wounds.rs`, `crates/ss13_game/src/medical.rs`: ноги отключаются ранами и уроном на максимуме — замедление 3 дс за ногу, без ног лёжа; руки — выпадение предмета при переломе (`// shortcut:` рука не блокируется)

### Приживление и регенерация конечностей
Отрезанную конечность можно прикрепить хирургией (или сразу — видам с `TRAIT_LIMBATTACHMENT`); некоторые реагенты и способности отращивают конечности.

- **Код:** [`proc/try_attach_limb()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/dismemberment.dm#L258), [`proc/regenerate_limbs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/dismemberment.dm#L362)
- **Статус в ss13-rs:** ❌ нет

### Роботизированные конечности и протезы
Кибер-конечности (прочнее к brute, слабее к ЭМИ, чинятся сваркой и кабелем), дешёвые «surplus», самодельные (деревянная нога и т.п.); в форке — десятки брендов протезов.

- **Код:** [`robot_bodyparts.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/robot_bodyparts.dm#L1), [`ghetto_parts.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/ghetto_parts.dm#L1)
- **BandaStation:** [`modular_bandastation/augmentation_preferences/code/body_modifications/body_part/robotic_limbs/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/augmentation_preferences/code/body_modifications/body_part/robotic_limbs) — бренды протезов
- **Статус в ss13-rs:** ❌ нет

### Видовые части тела
Каждый вид подставляет свои части (`bodypart_overrides`): спрайты, модификаторы урона, особенности (цифровые ноги, слабые конечности скелетов); форк — части скреллов, таяр, вульпканинов.

- **Код:** [`code/modules/surgery/bodyparts/species_parts/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/species_parts)
- **BandaStation:** [`modular_bandastation/species/code/surgery/bodyparts/skrell_bodyparts.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/surgery/bodyparts/skrell_bodyparts.dm#L1) — части тела видов форка
- **Статус в ss13-rs:** ❌ нет

### Эффекты частей тела и обрубки
Эффекты частей (`bodypart_effects`: светящиеся, фотосинтез и т.п.), обрубки вместо утраченных частей, голова с волосами и губами.

- **Код:** [`code/modules/surgery/bodyparts/bodypart_effects/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/bodypart_effects), [`stumps.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/stumps.dm#L1), [`head.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/head.dm#L1)
- **Статус в ss13-rs:** ❌ нет
