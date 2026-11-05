# Прочие структуры
Настенные шкафы, сейфы, витрины, отражатели, вентиляторы, ловушки, вывески, декор, сантехника и мелкие структуры, не принадлежащие другим отделам. Шкафы и ящики (`crates_lockers`) — см. ../cargo/; плакаты — см. ../effects/.

### Прочность и разрушение объектов
Общая модель урона структур и машин: `max_integrity`, броня по типам урона, `damage_deflection`, поломка на `integrity_failure`, разрушение в обломки (`deconstruct`), ремонт; удары предметами по объектам с сообщениями и звуками.

- **Код:** [`proc/take_damage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atom/atom_defense.dm#L17), [`proc/run_atom_armor()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atom/atom_defense.dm#L92), [`proc/repair_damage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atom/atom_defense.dm#L51), [`proc/deconstruct()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/obj_defense.dm#L195), [`atom_destruction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/obj_defense.dm#L208), [`attacked_by()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/objs.dm#L82)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/obj_damage.rs`: урон от предметов с отражением и бронёй ближнего боя, поломка и разрушение у окон, решёток, балок, рам, газовых ёмкостей, автоматов (плюс горение огнеопасных в `fire.rs`); урон от взрывов через броню `bomb` (`take_bomb_damage`, `crates/ss13_game/src/explosions/ex_act.rs`) также разрушает машины из плат и решётки-лестницы; нет разрушения дверей, шкафов, столов и прочих структур (их `atom_deconstruct`), `demolition_mod`

### Шкаф с огнетушителем и пожарным топором
Настенные шкафы: огнетушитель вынимается рукой; пожарный топор — только после разблокировки (ID/мультитул) или разбитого стекла; устанавливаются из настенной рамы.

- **Код:** [`/obj/structure/extinguisher_cabinet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/extinguisher.dm#L1), [`/obj/structure/fireaxecabinet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/fireaxe.dm#L1), [`/obj/item/wallframe/fireaxecabinet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/fireaxe.dm#L239)
- **BandaStation:** [`aesthetics/extinguisher_cabinet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/extinguisher_cabinet/code/extinguisher_cabinet.dm) — спрайты шкафа
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wallmount/cabinets.rs`: шкаф с огнетушителем (с карты закрыт с огнетушителем; взять/положить, открыть/закрыть ЛКМ/ПКМ/предметом, вид по типу огнетушителя и дверце), шкаф с топором (заперт; мультитул 2 с снимает/ставит замок, открыть/закрыть, взять/положить топор, оверлеи стекла и замка). Нет: повреждения и разбития стекла, ремонта, разборки ключом, монтажа из настенной рамы, проверки двуручного хвата

### Сейфы
Механический сейф с кодовым замком (крутить диск, слушать стетоскопом), напольный сейф, настенные секретные сейфы с паролем (ГСБ, капитан).

- **Код:** [`/obj/structure/safe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/safe.dm#L13), [`/obj/structure/secure_safe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/secure_safe.dm#L82)
- **Статус в ss13-rs:** ❌ нет

### Витрины и оружейные шкафы
Витрины (антикварный лазер капитана, музейные) с ID-замком и сигнализацией при разбитии; рама витрины; оружейные шкафы.

- **Код:** [`/obj/structure/displaycase`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/displaycase.dm#L1), [`/obj/structure/displaycase_chassis`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/displaycase.dm#L248), [`/obj/structure/guncase`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/guncase.dm#L2)
- **BandaStation:** [`modular_bandastation/objects/code/structures/displaycase.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/displaycase.dm) — витрины форка
- **Статус в ss13-rs:** ❌ нет

### Раздатчики и держатели баллонов
Раздатчик кислородных и плазменных баллонов (tgui), напольный держатель баллона.

- **Код:** [`/obj/structure/tank_dispenser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/tank_dispenser.dm#L3), [`/obj/structure/tank_holder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/tank_holder.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Отражатели
Отражатели лучей (одинарный, двойной, коробчатый) для разводки лучей эмиттеров; собираются из листов, поворачиваются, закрепляются сваркой.

- **Код:** [`/obj/structure/reflector`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/reflector.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/power/emitter.rs` (`Reflector::reflect`): одинарный, двойной и коробчатый отражают лучи эмиттера по углу из карты; нет сборки, поворота и сварки

### Вентиляторы-барьеры
Маленькие вентиляторы удерживают воздух в проёме (шлюзы шахт, ангары), пропуская людей.

- **Код:** [`/obj/structure/fans`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/fans.dm#L2), [`/obj/structure/fans/tiny`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/fans.dm#L24)
- **Статус в ss13-rs:** ✅ есть — воздух держат прототипы (`can_atmos_pass = ATMOS_PASS_NO`, `crates/ss13_mapload/src/atmos.rs`), проходимость — плотность типа; `crates/ss13_game/src/construction.rs`: гаечный ключ за 2 с разбирает в 5 листов стали (маленький — 2), щит шаттлового ангара не разбирается

### Ловушки и растяжки
Невидимые ловушки (оглушение, огонь, холод, телепорт), растяжки из кабеля между столбиками.

- **Код:** [`/obj/structure/trap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/traps.dm#L1), [`/obj/structure/tripwire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/tripwire.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Разворачиваемые турели и пушки
Станковые пулемёты под ручным управлением, старинные пушки с ядрами и стационарные орудия.

- **Код:** [`/obj/machinery/deployable_turret`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/deployable_turret.dm#L4), [`/obj/structure/cannon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/cannons/cannon.dm#L6), [`cannons/mounted_guns`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/cannons/mounted_guns)
- **Статус в ss13-rs:** ❌ нет

### Вывески, таблички, доски объявлений
Настенные знаки и флаги (отделы, предупреждения, карты станции), гравируемые таблички, доски объявлений (вешать бумаги), билборды, голографические вывески, доска детектива.

- **Код:** [`/obj/structure/sign`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/signs/_signs.dm#L1), [`signs`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/signs), [`/obj/structure/plaque`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/plaques/_plaques.dm#L1), [`/obj/structure/noticeboard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/noticeboard.dm#L3), [`/obj/structure/billboard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/billboard.dm#L1), [`signboards`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/signboards), [`/obj/structure/detectiveboard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/detectiveboard.dm#L9)
- **BandaStation:** [`signs_flags.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/signs/signs_flags.dm#L22) — флаг ТСФ вместо Terragov; [`modular_bandastation/objects/code/structures/signs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/signs.dm), [`flags.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/flags.dm)
- **Статус в ss13-rs:** ❌ нет — только как спрайты с карты

### Флора, статуи и декор
Деревья, кусты, камни, трава (часть срубается/собирается), декоративные объекты (`fluff`), статуи (из материалов, ломаются), окаменевшие статуи, транспортные контейнеры, обломки пола.

- **Код:** [`/obj/structure/flora`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/flora.dm#L1), [`/obj/structure/decoration`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/decorations.dm#L1), [`/obj/structure/fluff`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/fluff.dm#L6), [`/obj/structure/statue`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/statues.dm#L5), [`/obj/structure/statue/petrified`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/petrified_statue.dm#L1), [`/obj/structure/shipping_container`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/containers.dm#L1), [`/obj/structure/broken_flooring`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/broken_flooring.dm#L1)
- **BandaStation:** [`modular_bandastation/objects/code/structures/statues.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/statues.dm) — статуи форка
- **Статус в ss13-rs:** ❌ нет — только как спрайты с карты

### Костёр и камин
Костёр (поджечь, жарить на шампуре, сесть на стул-кол) и камин, горящий на дровах и бумаге.

- **Код:** [`/obj/structure/bonfire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/bonfire.dm#L10), [`/obj/structure/fireplace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/fireplace.dm#L5)
- **Статус в ss13-rs:** ❌ нет

### Сантехника
Раковины (мыть руки, набирать воду), души (моют, тушат, распыляют реагенты), туалеты (тайник в бачке), писсуары, источники воды; сборка из рам.

- **Код:** [`/obj/structure/sink`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/water_structures/sink.dm#L1), [`/obj/machinery/shower`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/shower.dm#L29), [`/obj/structure/toilet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/water_structures/toilet.dm#L1), [`/obj/structure/urinal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/water_structures/urinal.dm#L1), [`/obj/structure/water_source`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/water_structures/water_source.dm#L2)
- **BandaStation:** [`toilet.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/water_structures/toilet.dm#L42) — убрана часть апстримного поведения туалета
- **Статус в ss13-rs:** 🟡 частично — раковины: `crates/ss13_game/src/plumbing.rs` (`capacity` единиц `dispensedreagent`, утилизатор доливает `reclaim_rate`/с; пустая рука моет руки — перчатки или руки теряют кровь — или лицо за 4 с и 5 ед.; контейнер наполняется (`crate::chem`) и выливается в слив ПКМ, швабра смачивается, прочие предметы моются за 4 с; ключ разбирает на лист и утилизатор, лом снимает утилизатор; одна мойка за раз). Нет: смывания макияжа, удара током от включённой дубинки, эффектов реагентов на моющегося, установки утилизатора обратно, рам раковин, труб; душей, туалетов, писсуаров и источников воды

### Развлечения и спорт
Шахматы, спиритическая доска, урна для голосования, банка для чаевых, тренажёры и груша, тренировочная машина, пиньяты, «загадочный ящик», кошачий домик, ткацкий станок.

- **Код:** [`/obj/structure/chess`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/chess.dm#L1), [`/obj/structure/spirit_board`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/spirit_board.dm#L1), [`/obj/structure/votebox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/votingbox.dm#L4), [`/obj/structure/tipjar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/tipjar.dm#L1), [`/obj/structure/weightmachine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/gym/weight_machine.dm#L5), [`/obj/structure/punching_bag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/gym/punching_bag.dm#L1), [`/obj/structure/training_machine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/training_machine.dm#L15), [`/obj/structure/pinata`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/pinatas.dm#L2), [`/obj/structure/mystery_box`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mystery_box.dm#L116), [`/obj/structure/cat_house`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/cat_house.dm#L1), [`/obj/structure/loom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/loom.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Гильотина, зеркало и прочее
Гильотина (казнь с затачиваемым лезвием), зеркало (сменить причёску, бьётся), пика с головой, свеча жизни (воскрешение), алтари, маяк хайвботов, паровой клапан и ловушка влаги в техтоннелях, рудные контейнеры.

- **Код:** [`/obj/structure/guillotine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/guillotine.dm#L31), [`/obj/structure/mirror`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mirror.dm#L21), [`/obj/structure/headpike`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/headpike.dm#L1), [`/obj/structure/life_candle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/life_candle.dm#L1), [`/obj/structure/sacrificealtar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/divine.dm#L1), [`/obj/structure/hivebot_beacon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/hivebot.dm#L1), [`/obj/structure/steam_vent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/maintenance.dm#L263), [`/obj/structure/moisture_trap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/maintenance.dm#L4), [`/obj/structure/ore_container`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/ore_containers.dm#L2)
- **BandaStation:** [`maintenance.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/maintenance.dm#L44) — убрана часть содержимого ловушки влаги
- **Статус в ss13-rs:** ❌ нет

### Спавнеры мобов
Разрушаемые структуры, периодически создающие враждебных мобов (синдикат, скелеты, клоуны, лаваленд).

- **Код:** [`/obj/structure/spawner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/spawner.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Консоль постройки базы
Консоль-камера для удалённой постройки (вспомогательная база майнеров, ЦК): ставит стены, полы, двери, турели из запаса материалов.

- **Код:** [`/obj/machinery/computer/camera_advanced/base_construction`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/construction_console/construction_console.dm#L10), [`construction_console`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/construction_console)
- **Статус в ss13-rs:** ❌ нет
