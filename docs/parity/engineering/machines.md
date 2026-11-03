# Инженерные машины и сборка машин
Общая сборка/разборка машин (рамы, платы, детали), автолат, флэтпакер, большой манипулятор, зажигатели, массдрайверы, щиты, обогреватели и баррикады. Зарядники батарей и выключатели света — см. ../power/.

### Рамы машин и компьютеров
Рама из 5 листов стали: кабель, плата, детали по списку платы, отвёртка завершает; разборка обратна; компьютерная рама: плата, кабель, стекло.

- **Код:** [`/obj/structure/frame`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/constructable_frame.dm#L1), [`/obj/structure/frame/machine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/machine_frame.dm#L1), [`proc/install_board()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/constructable_frame.dm#L142), [`proc/finalize_construction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/constructable_frame.dm#L103)
- **Статус в ss13-rs:** ❌ нет — машины существуют только с карты (`crates/ss13_game/src/power/mod.rs`, shortcut)

### Панель, откручивание и разборка машин
Общие действия с машинами: отвёртка открывает панель (`default_deconstruction_screwdriver`), лом с открытой панелью разбирает в раму с деталями, гаечный ключ откручивает/закручивает и поворачивает; RPED заменяет детали, `RefreshParts` пересчитывает характеристики.

- **Код:** [`proc/default_deconstruction_screwdriver()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/_machinery.dm#L1049), [`proc/default_deconstruction_crowbar()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/_machinery.dm#L912), [`proc/default_unfasten_wrench()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/objs.dm#L288), [`proc/exchange_parts()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/_machinery.dm#L1079), [`proc/RefreshParts()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/_machinery.dm#L829)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/construction.rs`, `crates/ss13_game/src/obj_damage.rs` (`Anchored`): панель и откручивание только у торговых автоматов (и панели шлюзов/APC в `wires.rs`); разборки, деталей и RPED нет; панель термомашины — `crates/ss13_game/src/atmos_machinery/thermo.rs`, её детали (`RefreshParts`: матрицы и микролазеры) и откручивание ждут этой записи; детали электролизёра (`RefreshParts`: сервоприводы и конденсаторы, сейчас tier 1) и его разборка ломом — тоже (`crates/ss13_game/src/atmos_machinery/electrolyzer.rs`); разборка кристаллизатора ломом с выпуском внутренней смеси (`crystallizer.rs`)

### Автолат
Печатает предметы по чертежам из стали, стекла и других материалов (tgui с категориями, поиском, количеством); принимает листы и предметы на переработку; взлом проводов открывает запрещённые чертежи; дизайн-диски добавляют чертежи.

- **Код:** [`/obj/machinery/autolathe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/autolathe.dm#L1), [`ui_interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/autolathe.dm#L117); tgui: [`Autolathe.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Autolathe.tsx)
- **BandaStation:** [`autolathe.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/autolathe.dm#L180) — оригинальное имя чертежа для перевода
- **Статус в ss13-rs:** ❌ нет

### Флэтпакер
Упаковывает платы машин вместе с материалами в плоские коробки, которые разворачиваются в готовую машину.

- **Код:** [`/obj/machinery/flatpacker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/flatpacker.dm#L4); tgui: [`Flatpacker.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Flatpacker.tsx)
- **Статус в ss13-rs:** ❌ нет

### Большой манипулятор
Программируемая рука: берёт предметы с тайла-источника и кладёт/бросает/использует на тайлах-целях по фильтрам и приоритетам; tgui-настройка задач, провода.

- **Код:** [`/obj/machinery/big_manipulator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/big_manipulator/big_manipulator.dm#L1), [`big_manipulator`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/big_manipulator); tgui: [`BigManipulator`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/BigManipulator)
- **Статус в ss13-rs:** ❌ нет

### Зажигатели и искрители
Напольный зажигатель и настенный искритель поджигают газ по кнопке (инсинератор, токсины).

- **Код:** [`/obj/machinery/igniter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/igniter.dm#L1), [`/obj/item/wallframe/sparker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/igniter.dm#L135)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/fire.rs` (shortcut: no igniters as flame sources)

### Массдрайвер
По кнопке выбрасывает всё со своего тайла в направлении с силой (мусор в космос, часовня, токсины); пульт управления.

- **Код:** [`/obj/machinery/mass_driver`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/mass_driver.dm#L1); tgui: [`MassDriverControl.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/MassDriverControl.jsx)
- **Статус в ss13-rs:** ❌ нет

### Аварийные щиты и щитовые стены
Генератор аварийного щита закрывает пробоины голографическими щитами; генератор щитовой стены соединяет пары генераторов энергетической стеной (питание от кабеля).

- **Код:** [`/obj/machinery/shieldgen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/shieldgen.dm#L141), [`/obj/structure/emergency_shield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/shieldgen.dm#L1), [`/obj/machinery/power/shieldwallgen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/shieldgen.dm#L289), [`/obj/machinery/shieldwall`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/shieldgen.dm#L545)
- **Статус в ss13-rs:** ❌ нет

### Модульный генератор щита
Генератор купола-щита с модулями (узлы, зарядники, усилители), радиус и прочность зависят от модулей; консоль управления.

- **Код:** [`/obj/machinery/modular_shield_generator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/modular_shield.dm#L1), [`/obj/machinery/modular_shield/module`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/modular_shield.dm#L449); tgui: [`ModularShieldGen.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ModularShieldGen.tsx), [`ModularShieldConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ModularShieldConsole.tsx)
- **Статус в ss13-rs:** ❌ нет

### Генератор атмосферного щита
Ставит проницаемое для людей, но не для газа поле в проёме (ангары, шлюзы).

- **Код:** [`/obj/machinery/atmos_shield_gen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/atmos_field_gen.dm#L8), [`/obj/effect/atmos_shield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/atmos_field_gen.dm#L267)
- **Статус в ss13-rs:** ❌ нет

### Защита от туманности и переносной гравген
Генераторы защиты от радиационной туманности (на картах с туманностью); переносной генератор гравитации.

- **Код:** [`/obj/machinery/nebula_shielding`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/nebula_shielding.dm#L2), [`/obj/machinery/power/portagrav`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/portagrav.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Космический обогреватель
Переносной обогреватель/охладитель на батарее: греет или охлаждает воздух тайла до заданной температуры (tgui).

- **Код:** [`/obj/machinery/space_heater`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/spaceheater.dm#L7); tgui: [`SpaceHeater.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SpaceHeater.jsx)
- **Статус в ss13-rs:** ❌ нет

### Листовщик
Перерабатывает органику (мясо, кости и т. п.) в листы материалов.

- **Код:** [`/obj/machinery/sheetifier`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/sheetifier.dm#L1)
- **Статус в ss13-rs:** ❌ нет

_«Раздатчики труб» описано в [../atmos/pipes.md](../atmos/pipes.md)._
