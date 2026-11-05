# Части тела
Конечности, голова и грудь как объекты `/obj/item/bodypart`: урон, отключение, протезы, приживление. Расчленение — ../combat/embedding-dismemberment.md.

### Часть тела: урон и лечение
Каждая часть хранит свой brute/burn до `max_damage`, модификаторы урона, раны и встрявшие предметы; урон переливается в общее здоровье; лечение идёт по частям.

- **Код:** [`/obj/item/bodypart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/_bodyparts.dm#L1), [`proc/receive_damage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/_bodyparts.dm#L704), [`proc/heal_damage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/_bodyparts.dm#L888)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/health.rs`, `crates/ss13_game/src/medical/wounds.rs`: brute/burn по шести частям с `max_damage` tg, раны по частям; ампутированная рука или нога — предмет со своим уроном (`crates/ss13_game/src/medical/bodyparts.rs`), урон по отсутствующей части идёт в грудь; нет модификаторов урона частей, встрявших предметов

### Отключение конечностей
Конечность с уроном на максимуме или с тяжёлой раной отключается: рука не держит, нога не ходит (ползание, падение).

- **Код:** [`proc/update_disabled()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/_bodyparts.dm#L934), [`proc/set_disabled()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/_bodyparts.dm#L975)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/wounds.rs`, `crates/ss13_game/src/medical.rs`, `crates/ss13_game/src/medical/bodyparts.rs`: ноги отключаются ранами, уроном на максимуме и отсутствием — замедление 3 дс за ногу, без ног лёжа; руки — выпадение предмета при переломе, отсутствующая рука ничего не держит (подбор не идёт в неё; `// shortcut:` прочие пути в руку не проверяют, отключённая рука не блокируется)

### Приживление и регенерация конечностей
Отрезанную конечность можно прикрепить хирургией (или сразу — видам с `TRAIT_LIMBATTACHMENT`); некоторые реагенты и способности отращивают конечности.

- **Код:** [`proc/try_attach_limb()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/dismemberment.dm#L258), [`proc/regenerate_limbs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/dismemberment.dm#L362)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/bodyparts.rs`, `crates/ss13_game/src/medical/surgery.rs`: конечность-предмет прикрепляется к обрубку операцией (с её уроном, 10 токсина отторжения), `regenerate_limbs` при полном лечении (админ, генокрад); нет `TRAIT_LIMBATTACHMENT`, реагентов, отращивающих конечности, видов (франкенштейн-отторжение 30)

### Роботизированные конечности и протезы
Кибер-конечности (прочнее к brute, слабее к ЭМИ, чинятся сваркой и кабелем), дешёвые «surplus», самодельные (деревянная нога и т.п.); в форке — десятки брендов протезов.

- **Код:** [`robot_bodyparts.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/robot_bodyparts.dm#L1), [`ghetto_parts.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/ghetto_parts.dm#L1)
- **BandaStation:** [`modular_bandastation/augmentation_preferences/code/body_modifications/body_part/robotic_limbs/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/augmentation_preferences/code/body_modifications/body_part/robotic_limbs) — бренды протезов
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/bodyparts.rs`, `crates/ss13_game/src/human.rs`, `crates/ss13_game/src/medical/surgery.rs`: кибер-конечность (`/obj/item/bodypart/*/robot`) прикрепляется операцией без токсина отторжения, рисуется из `augments.dmi` (`HumanLook::robotic`), не получает ран плоти и не кровоточит, снимается гаечным ключом/ломом (`amputate/mechanic`), падает своим типом; `// shortcut:` нет модификаторов урона 0.8 по части, ЭМИ, починки сваркой/кабелем, металлических ран, «surplus»/самодельных, брендов форка

### Видовые части тела
Каждый вид подставляет свои части (`bodypart_overrides`): спрайты, модификаторы урона, особенности (цифровые ноги, слабые конечности скелетов); форк — части скреллов, таяр, вульпканинов.

- **Код:** [`code/modules/surgery/bodyparts/species_parts/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/species_parts)
- **BandaStation:** [`modular_bandastation/species/code/surgery/bodyparts/skrell_bodyparts.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/surgery/bodyparts/skrell_bodyparts.dm#L1) — части тела видов форка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`, `crates/ss13_game/src/human.rs`, `crates/ss13_game/src/health.rs`: спрайты частей вида, `brute_modifier`/`burn_modifier`, цифровые ноги таяр, вульпканинов и эшвалкеров; части тела не отдельные предметы вида, нет выбора цифровых ног у ящеров

### Эффекты частей тела и обрубки
Эффекты частей (`bodypart_effects`: светящиеся, фотосинтез и т.п.), обрубки вместо утраченных частей, голова с волосами и губами.

- **Код:** [`code/modules/surgery/bodyparts/bodypart_effects/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/bodypart_effects), [`stumps.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/stumps.dm#L1), [`head.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/head.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/bodyparts.rs`, `crates/ss13_game/src/human.rs`: обрубок вместо отделённой руки, ноги или головы (без кожи: `SURGERY_SKIN_OPEN` врождённый), тело рисуется без неё (без головы — без волос и глаз); голова — предмет с органами (`InLimb`, острым предметом вынимаются), имя «голова <владелец>», падают надетое на голову, обувь с ногой, наручники с рукой, перчатки без рук; нет эффектов частей тела, вида/внешности головы-предмета, блокировки слотов без головы
