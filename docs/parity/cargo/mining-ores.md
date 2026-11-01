# Добыча руды и её переработка
Минеральные стены, руды, рудные жилы-вентили, валуны и цепочка переработки до листов.

### Минеральные стены (горная порода)
`/turf/closed/mineral`: копается киркой/дрелью/руками с `do_after`, превращается в пол астероида/лаваленда и роняет руду; жилы руды распространяются при генерации; варианты по биомам (астероид, вулкан, лёд, снег) и частоте руды (`random`, `high_chance`, `low_chance`). Подсветка жил шахтёрским сканером.

- **Код:** [`/turf/closed/mineral`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/minerals.dm#L6), [`gets_drilled()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/minerals.dm#L278), [`manual_mine()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/minerals.dm#L232), [`/turf/closed/mineral/random`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/minerals.dm#L390), [`spread_vein()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/minerals.dm#L74)
- **BandaStation:** [`minerals.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/minerals.dm#L684) — вырезана смена иконки для снежных стен с рудой
- **Статус в ss13-rs:** ❌ нет — стены грузятся с карты как непроходимые турфы, копать их нельзя

### Гибтонит
Взрывоопасная порода: при копании начинает отсчёт, его можно обезвредить анализатором и вынести гибтонит как взрывчатку (сила зависит от момента остановки).

- **Код:** [`/turf/closed/mineral/gibtonite`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/minerals.dm#L1074), [`explosive_reaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/minerals.dm#L1105), [`/obj/item/gibtonite`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/ores_coins.dm#L332)
- **Статус в ss13-rs:** ❌ нет

### Руды
Стопки руды: железо, песок (стекло, базальт), плазма, серебро, золото, алмазы, уран, бананиум, титан, блюспейс-кристаллы, шлак. Руда тяжёлая, ломается при броске, на ORM даёт очки.

- **Код:** [`/obj/item/stack/ore`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/ores_coins.dm#L5), [`/obj/item/stack/ore/iron`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/ores_coins.dm#L109), [`/obj/item/stack/ore/plasma`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/ores_coins.dm#L222), [`/obj/item/stack/ore/bluespace_crystal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/bscrystal.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Рудные вентили и волны защиты (ore vents)
На лаваленде/ледяной луне вентили с набором минералов: шахтёр сканирует вентиль, отбивается от волн фауны, затем вентиль (подключённый дроном) производит валуны; босс-вентили. Подсистема `SSore_generation` распределяет минералы и производит валуны.

- **Код:** [`/obj/structure/ore_vent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/lavaland/vent_mining/ore_vent.dm#L8), [`scan_and_confirm()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/lavaland/vent_mining/ore_vent.dm#L480), [`start_wave_defense()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/lavaland/vent_mining/ore_vent.dm#L286), [`produce_boulder()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/lavaland/vent_mining/ore_vent.dm#L549), [`/obj/structure/ore_vent/boss`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/lavaland/vent_mining/boss_vent.dm#L2), [`SUBSYSTEM_DEF(ore_generation)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ore_generation.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Валуны и маяк валунов
`/obj/item/boulder` — тяжёлый камень с минералами (артефактные — блюспейс/алмазы, гулаговские); маяк вызывает валуны к станции.

- **Код:** [`/obj/item/boulder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/boulder_processing/boulder.dm#L7), [`boulder_types.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/boulder_processing/boulder_types.dm#L5), [`/obj/item/boulder_beacon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/boulder_processing/beacon.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Переработка валунов: BRM, очиститель, плавильня
BRM телепортирует валуны с вентилей на станцию; очиститель (refinery) и плавильня (smelter) по конвейеру дробят валуны на материалы, отправляя их в силос.

- **Код:** [`/obj/machinery/brm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/boulder_processing/brm.dm#L12), [`/obj/machinery/bouldertech`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/boulder_processing/_boulder_processing.dm#L1), [`/obj/machinery/bouldertech/refinery`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/boulder_processing/refinery.dm#L6), [`/obj/machinery/bouldertech/refinery/smelter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/boulder_processing/refinery.dm#L110)
- **Статус в ss13-rs:** ❌ нет

### Машина переработки руды (ORM)
Принимает руду (с конвейера или руками), плавит в материалы прямо в силос, начисляет шахтёрские очки на ID; печать сплавов; объявления о новых материалах.

- **Код:** [`/obj/machinery/mineral/ore_redemption`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/machine_redemption.dm#L4), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/machine_redemption.dm#L306), [`local_redeem_points()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/machine_redemption.dm#L87); tgui: [`OreRedemptionMachine.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/OreRedemptionMachine.tsx)
- **Статус в ss13-rs:** ❌ нет

### Рудный силос (ore silo)
Общее хранилище материалов станции: к нему подключены лата, ORM и прочие машины; журнал операций, блокировка отдельных машин, взлом емагом.

- **Код:** [`/obj/machinery/ore_silo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/machine_silo.dm#L26), [`connect_receptacle()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/machine_silo.dm#L195), [`/datum/ore_silo_log`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/machine_silo.dm#L561); tgui: [`OreSilo.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/OreSilo.tsx)
- **Статус в ss13-rs:** 🟡 `ss13_game::science::lathe` (`OreSilo`): общее хранилище R&D-лат своего уровня, требование ID с банковским счётом (или доступа QM); нет окна силоса, журнала, удержаний, банов, ORM

### Плавильный блок, укладчик и разгрузчик
Классическая цепочка на конвейере: плавильный блок с консолью (выбор материала/сплава), укладчик листов в стопки с консолью, разгрузчик ящиков.

- **Код:** [`/obj/machinery/mineral/processing_unit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/machine_processing.dm#L131), [`/obj/machinery/mineral/processing_unit_console`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/machine_processing.dm#L68), [`/obj/machinery/mineral/stacking_machine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/machine_stacking.dm#L88), [`/obj/machinery/mineral/unloading_machine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/machine_unloading.dm#L4); tgui: [`ProcessingConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ProcessingConsole.tsx), [`StackingConsole.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/StackingConsole.jsx)
- **Статус в ss13-rs:** ❌ нет

### Рудный ящик, контейнеры и шахтёрская сумка
Тачка-ящик для руды (выгрузка содержимого), рудные контейнеры, сумка для руды, автоматически собирающая руду под ногами.

- **Код:** [`/obj/structure/ore_box`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/satchel_ore_box.dm#L4), [`/obj/structure/ore_container`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/ore_containers.dm#L2), [`/obj/item/storage/bag/ore`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/bags.dm#L97); tgui: [`OreBox.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/OreBox.tsx), [`OreContainer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/OreContainer.tsx)
- **Статус в ss13-rs:** ❌ нет

Трудовой лагерь (гулаг: консоль сдачи руды, укладчик заключённых, шаттл) — см. ../security/.
