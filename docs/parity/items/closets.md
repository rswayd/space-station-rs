# Шкафы, локеры и ящики
`/obj/structure/closet` и его семейства: открытие, замки, сварка, содержимое, мобы внутри.

### Шкаф: открытие, закрытие, содержимое
Клик открывает/закрывает шкаф; при открытии содержимое высыпается на тайл, при закрытии забираются лежащие предметы и мобы (до `storage_capacity`); содержимое создаётся при первом открытии (`PopulateContents`); предметы, размещённые на карте поверх, оказываются внутри. Анимация дверцы.

- **Код:** [`/obj/structure/closet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L7), [`open()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L538), [`close()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L612), [`take_contents()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L521), [`PopulateContents()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L208), [`animate_door()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L309)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/closets.rs`: открытие/закрытие, высыпание и сбор предметов и живых мобов (`InCloset`, до `mob_storage_capacity`; ящик не закрывается над стоящим), ленивое наполнение, предметы с карты внутри, вид по `update_icon`; нет анимации дверцы, проверки «что-то крупное сверху»

### Замки и доступ (secure closets)
Защищённые шкафы запираются и отпираются ID с доступом (правый клик/клик картой); личные шкафы привязываются к первой карте; электроника шлюза и картридер перенастраивают доступ; емаг и ЭМИ ломают замок.

- **Код:** [`/obj/structure/closet/secure_closet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/secure/secure_closets.dm#L1), [`togglelock()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L1167), [`can_unlock()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L1162), [`emag_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L1222), [`/obj/structure/closet/secure_closet/personal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/secure/personal.dm#L1), [`set_access()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L687)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/closets.rs`: запирание и отпирание по доступу карты (`check_access`), оверлеи замка; ЭМИ (`crates/ss13_game/src/emp/machines.rs`): замок переключается с `50/severity`%, закрытый шкаф с `20/severity`% открывается (не заперт) или получает один случайный станционный доступ, содержимое получает импульс, моб внутри — нет; нет личных шкафов, электроники/картридера, емага, взлома (форковый взлом замков — см. ../security/)

### Сварка, разборка и перемещение
Сварщик заваривает шкаф (не открыть изнутри и снаружи) и разваривает; разобранный сваркой шкаф даёт металл; гаечный ключ (правый клик) прикручивает к полу; шкафы можно тащить.

- **Код:** [`welder_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L924), [`wrench_act_secondary()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L972), [`handle_deconstruct()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L640)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/closets.rs`, `crates/ss13_game/src/pull.rs`: заваренное на карте состояние держит шкаф закрытым, шкафы тащатся; нет сварки инструментом, разборки, прикручивания

### Моб внутри шкафа
Можно залезть в шкаф (перетаскиванием себя) или запихнуть другого; изнутри видно содержимое (затемнённый шкаф), сопротивление вырывается из запертого/заваренного шкафа за время, шкаф трясётся; толчок в открытый шкаф заталкивает внутрь.

- **Код:** [`mouse_drop_receive()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L987), [`container_resist_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L1085), [`bust_open()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L1139), [`locker_living()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L1274), [`/datum/closet_see_inside`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closet_see_inside.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/closets.rs` (`mouse_drop_receive`, `InCloset`), `crates/ss13_game/src/drag.rs`: предмет с пола перетаскиванием на открытый шкаф — на его тайл, моба — запихнуть через `do_after` 4 с (парализует на 4 с, шкаф закрывается), шаг изнутри открывает незапертый шкаф; моб внутри остаётся на тайле шкафа невидимым и некликабельным (shortcut, не настоящее содержимое); нет залезания самому, вида изнутри, вырывания из запертого/заваренного шкафа, тряски, толчка внутрь

### Воздух внутри шкафа
Закрытый шкаф хранит свою газовую смесь и температуру (морозильники холодные), из которой дышит моб внутри.

- **Код:** [`return_air()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L201), [`process_internal_air()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets.dm#L229), [`/obj/structure/closet/secure_closet/freezer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/secure/freezer.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Ящики (crates)
Ящики открываются крышкой, на них можно залезть, закрыты/открыты отдельными спрайтами; накладная (манифест) срывается; защищённые ящики с индикатором замка; синдикатовские, ящики для криттеров, мусорные контейнеры, пластиковые, картонные, деревянные.

- **Код:** [`/obj/structure/closet/crate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/crates.dm#L1), [`tear_manifest()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/crates.dm#L150), [`/obj/structure/closet/crate/secure`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/crates/secure.dm#L1), [`/obj/structure/closet/crate/critter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/crates/critter.dm#L1), [`/obj/structure/closet/crate/bin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/crates/bins.dm#L1), [`/obj/structure/closet/crate/wooden`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/crates/wooden.dm#L1)
- **BandaStation:** [`crates.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/crates.dm#L295) — пакеты крови скреллов и молей в медицинском ящике
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/closets.rs`: открытие, спрайт `<base>open`, индикаторы замка; нет манифеста, залезания сверху, приёма структур (`allow_objects`)

### Большие ящики (вскрываются ломом)
Большой деревянный ящик не открывается руками — только ломом, после чего разваливается на доски.

- **Код:** [`/obj/structure/closet/crate/large`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/crates/large.dm#L1), [`crowbar_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/crates/large.dm#L40)
- **Статус в ss13-rs:** ❌ нет

### Заброшенные ящики с кодом
Запертый ящик с кодовым замком: игра «быки и коровы» (подсказки после неверных попыток), неудача взрывает ящик, внутри случайный лут.

- **Код:** [`/obj/structure/closet/crate/secure/loot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/crates/abandoned_crates/abandoned_crates.dm#L3), [`bulls_and_cows()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/crates/abandoned_crates/abandoned_crates.dm#L198), [`abandoned_crates_loot.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/crates/abandoned_crates/abandoned_crates_loot.dm)
- **Статус в ss13-rs:** ❌ нет

### Шкафы должностей и отделов
Шкафы с наборами снаряжения (СБ, глав отделов, инженеров, медиков, учёных, бара, гидропоники, карго), гардеробы по цветам и отделам, аварийные (кислород) и пожарные шкафы, шкафы радиационной и биозащиты, спортивные, синдикатовские, шкафы-бесконечности.

- **Код:** [`security.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/secure/security.dm), [`/obj/structure/closet/gmcloset`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/job_closets.dm#L3), [`/obj/structure/closet/wardrobe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/wardrobe.dm#L1), [`/obj/structure/closet/emcloset`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/utility_closets.dm#L15), [`l3closet.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/l3closet.dm), [`/obj/structure/closet/athletic_mixed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/fitness.dm#L1), [`/obj/structure/closet/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/syndicate.dm#L1), [`/obj/structure/closet/infinite`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/infinite.dm#L1)
- **BandaStation:** [`security.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/secure/security.dm#L127) — подсчёт шкафов СБ; [`security.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/secure/security.dm#L256) — доп. предметы в шкафах брига; шкафы форковых должностей — [bandastation-objects.md](bandastation-objects.md)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/closets.rs`, `crates/ss13_mapload/src/contents.rs`: содержимое по `PopulateContents`/`populate_contents_immediate` (`DmContents`, `if(var)` по переменным типа), доступ по `req_access`; ветви, зависящие от рантайм-состояния, не исполняются

### Картонная коробка
Коробка, в которой можно спрятаться и ползать (медленно); при обнаружении «!»-эффект.

- **Код:** [`/obj/structure/closet/cardboard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/cardboardbox.dm#L1), [`relaymove()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/cardboardbox.dm#L44)
- **Статус в ss13-rs:** ❌ нет

### Мешки для тел
Мешок раскладывается из предмета, вмещает одно тело, складывается обратно; на нём пишут маркером; стазисный и блюспейс-мешки.

- **Код:** [`/obj/structure/closet/body_bag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/bodybag.dm#L1), [`attempt_fold()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/bodybag.dm#L83), [`/obj/item/bodybag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/medical/bodybag.dm#L2)
- **BandaStation:** [`bodybag.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/bodybag.dm#L462) — свои спрайты стазисного мешка
- **Статус в ss13-rs:** ❌ нет

### Шкафчики-кабинеты и прочие
Деревянные кабинеты, костюмные шкафы-ловушки и т.п.

- **Код:** [`/obj/structure/closet/cabinet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/gimmick.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/closets.rs`: как обычный шкаф
