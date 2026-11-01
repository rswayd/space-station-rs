# Уборка и стирка
Грязь и её очистка, инвентарь уборщика, мокрый пол, стиральная машина, душ и раковины.

_«Грязь и пятна (cleanables)» описано в [../effects/decals.md](../effects/decals.md)._

### Механика очистки
Компонент очистки (швабра, мыло, тряпка, клинбот) по `do_after` смывает декали, кровь и отпечатки с тайла/предмета/моба; уборка даёт опыт навыка, ускоряющего её.

- **Код:** [`/datum/component/cleaner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/cleaner.dm#L7), [`clean()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/cleaner.dm#L94), [`/datum/element/cleaning`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/cleaning.dm#L1), [`/datum/skill/cleaning`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/cleaning.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/janitor.rs`: `do_after` швабры и мыла, `wash()` с `clean_type` (тайл смывает свои cleanables, космоочиститель — всё на тайле); нет навыка уборки (модификатор всегда 1), пузырей, клинбота, мытья мобов и предметов

### Швабра и ведро
Швабру мочат в ведре/тележке (вода из реагентов) и моют пол — очищает и делает пол мокрым; продвинутая швабра сама набирает воду. Ведро для швабры с водой.

- **Код:** [`/obj/item/mop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/mop.dm#L1), [`/obj/item/mop/advanced`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/mop.dm#L77), [`/obj/structure/mop_bucket`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/janitor.dm#L3)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/janitor.rs`: ведро (100 ед., открытая ёмкость, наполняется переливанием; раковин нет), правый клик шваброй смачивает её (15 ед.), мытьё за 1,5 с смывает при ≥1 ед. чистящего реагента, `expose` ×10 мочит пол, швабра теряет 1 ед.; продвинутая швабра не набирает воду сама

### Тележка уборщика
Тележка-ведро с местами для швабры, мусорного мешка, знаков, распылителя и заменителя ламп; таскается за собой.

- **Код:** [`/obj/structure/mop_bucket/janitorialcart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/janitor.dm#L53)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/janitor.rs`: работает как ведро (свой оверлей воды); мест для инструментов нет

_«Джанимобиль (pimpin' ride)» описано в [../vehicles/vehicles.md](../vehicles/vehicles.md)._

### Мокрый пол и скольжение
Мокрый пол (вода, мыльная пена, лёд, смазка) роняет бегущих (ходьба шагом защищает, кроме смазки), высыхает со временем; компонент скольжения у мыла, банановой кожуры, клоунских вещей.

- **Код:** [`/datum/component/wet_floor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/wet_floor.dm#L1), [`/datum/component/slippery`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/slippery.dm#L18), [`Slip()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/slippery.dm#L187)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/janitor.rs`: `WetFloor` с `time_left_list` и `add_wet(min, add, max)`, высыхание по температуре воздуха каждую секунду, оверлей `wet_floor_static`; бегущий стоящий моб поскальзывается (0,5 с паралича BandaStation, нокдаун 6 с, 3 с daze), шагом — нет (`ClientIntent::ToggleMoveIntent`, `WALK_DELAY` 0,4 с); нет льда, смазки как источника, скольжения по тайлам, галош, скользких мыла/кожуры, конденсации пара; у клиента нет кнопки/клавиши ходьбы

### Знаки «мокрый пол» и голобарьеры
Знак-предупреждение (носится как костюм, ставится на пол); голопроектор уборщика ставит голографические знаки мокрого пола.

- **Код:** [`/obj/item/clothing/suit/caution`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/wetfloor.dm#L1), [`/obj/item/holosign_creator/janibarrier`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/holosign_creator.dm#L117), [`/obj/structure/holosign/barrier/wetsign`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/holosign.dm#L166)
- **Статус в ss13-rs:** ❌ нет

### Мыло
Мыло очищает тайл/моба/предмет быстрее швабры, расходуется (число использований по сорту: самодельное, NT, люкс, синдикат, омега); на полу скользкое.

- **Код:** [`/obj/item/soap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/clown_items.dm#L14), [`should_clean()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/clown_items.dm#L152), [`/obj/item/soap/omega`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/clown_items.dm#L108)
- **BandaStation:** [`modular_bandastation/aesthetics/soap/code/soap.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/soap/code/soap.dm#L1) — свои спрайты мыла
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/janitor.rs`: чистит тайл/декаль за `cleanspeed` своего сорта, тратит использование и крошится; не скользкое, мобов не моет

### Космоочиститель и ранец уборщика
Распылитель space cleaner смывает пятна на расстоянии; ранец-бак уборщика с распылителем и режимом пены.

- **Код:** [`/obj/item/reagent_containers/spray/cleaner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/spray.dm#L184), [`/obj/item/watertank/janitor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/watertank.dm#L134)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/janitor.rs`: распылитель по соседнему тайлу выпускает `amount_per_transfer_from_this` (`VAPOR`): космоочиститель ≥1 ед. смывает тайл и всё на нём, вода мочит пол; нет облачка и дальности (кликов дальше соседнего тайла нет), ранца уборщика

### Мусорный мешок
Мешок собирает мелкий мусор с тайла кликом; синий мешок большей вместимости; вытряхивается в мусоропровод.

- **Код:** [`/obj/item/storage/bag/trash`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/bags.dm#L27)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/storage.rs`: открывается и хранит предметы как обычное хранилище; сбора с тайла нет

### Метла
Двуручная метла сметает предметы с тайлов перед собой при ходьбе.

- **Код:** [`/obj/item/pushbroom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/janitorial/broom.dm#L4)
- **Статус в ss13-rs:** ❌ нет

_«Заменитель ламп» описано в [../power/lighting.md](../power/lighting.md)._

### Стиральная машина
Стирает одежду и предметы (цикл с водой), окрашивает их цветным предметом внутри (мелок и т.д.), можно залезть внутрь; разбирается.

- **Код:** [`/obj/machinery/washing_machine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/washing_machine.dm#L176), [`wash_cycle()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/washing_machine.dm#L234), [`/obj/item/proc/dye_item()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/washing_machine.dm#L249), [`/atom/movable/proc/machine_wash()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/washing_machine.dm#L291)
- **Статус в ss13-rs:** ❌ нет

### Душ и раковины
Душ моет и тушит стоящих под ним (температура воды настраивается, есть бак), раковина моет руки и предметы, наполняет ёмкости водой.

- **Код:** [`/obj/machinery/shower`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/shower.dm#L29), [`/obj/structure/sink`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/water_structures/sink.dm#L1)
- **Статус в ss13-rs:** ❌ нет
