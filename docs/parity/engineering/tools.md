# Инструменты
Шесть базовых инструментов (`tool_behaviour`), таймерные действия `do_after`, сварка с топливом, силовые и особые инструменты, краскопульты, огнетушитель, индуктор.

### Инструментальные действия и do_after
Предмет с `tool_behaviour` вне боевого режима сначала вызывает `tool_act` цели; долгие шаги — `do_after` (задержка × `toolspeed`), прерывается движением, сменой предмета или цели; повторная проверка состояния в конце.

- **Код:** [`proc/use_tool()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items.dm#L1216), [`proc/tool_use_check()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items.dm#L1269), [`proc/do_after()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/timed_actions.dm#L211)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/tools.rs` (`DoAfter`, `use_tool`), `crates/ss13_game/src/construction.rs`: шесть поведений, `toolspeed`, проверки в конце; нет полосы прогресса, `extra_checks` и прочих поведений (анализатор, кирка, пила, ...)

### Лом
Поддевает плитку, вскрывает обесточенные двери, разбирает окна/шлюзы; большой и аварийный ломы, абдукторский, лом-молот.

- **Код:** [`/obj/item/crowbar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/crowbar.dm#L1), [`/obj/item/crowbar/large`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/crowbar.dm#L57), [`/obj/item/crowbar/abductor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/crowbar.dm#L47)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/tools.rs`, `construction.rs`: плитка, окна, сборки шлюзов; `doors.rs` (`try_to_crowbar`): вскрывает обесточенный шлюз (болты и сварка мешают), пожарные шлюзы; панели и платы машин — `machine_frame.rs`; варианты ломов — те же `tool_behaviour` со своим `toolspeed`. Челюсти жизни и топор по запитанной двери — запись «Силовые инструменты»

### Отвёртка
Открывает панели машин, крепит окна и решётки, разбирает балки; электроотвёртка, абдукторская.

- **Код:** [`/obj/item/screwdriver`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/screwdriver.dm#L1), [`/obj/item/screwdriver/power`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/screwdriver.dm#L85)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/construction.rs`, `wires.rs`, `machine_wires.rs`, `machine_frame.rs`: панели шлюзов, APC, автоматов и прочих машин (`default_deconstruction_screwdriver`), окна, решётки, балки; варианты — тот же `tool_behaviour` со своим `toolspeed`

### Гаечный ключ
Закрепляет и откручивает машины и структуры (`default_unfasten_wrench`), разбирает окна; медицинский, боевой, болтовёрт.

- **Код:** [`/obj/item/wrench`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/wrench.dm#L1), [`/obj/item/wrench/combat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/wrench.dm#L97), [`/obj/item/wrench/bolter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/wrench.dm#L134)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/construction.rs`: балки, окна, торговые автоматы; `machine_frame.rs` (`default_unfasten_wrench`, с повторной проверкой в конце): закрепление и откручивание машин и рам, трубы и атмосферные устройства — `atmos_machinery/`; варианты — тот же `tool_behaviour` со своим `toolspeed`

### Кусачки
Режут провода, решётки, кабели, наручники-стяжки; абдукторские.

- **Код:** [`/obj/item/wirecutters`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/wirecutters.dm#L1), [`/obj/item/wirecutters/abductor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/wirecutters.dm#L67)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/wires.rs`, `construction.rs`: провода и решётки; кабели режутся (`power/cable_coil.rs` `cut`, терминал SMES — `power/smes.rs`); стяжки и слабые наручники срезаются ПКМ (`restraints.rs`, `cuffsnapping`; абдукторские режут и обычные за 1 с); застрявшие предметы — `embedding.rs`

### Сварочный аппарат
Включается в руке, жжёт топливо (1 ед./5 с, расход на работу), нагревает до 1500+ K: режет стены, заваривает двери, чинит окна и роботов, поджигает плазму; заправка из бака топлива; без защиты глаз повреждает зрение; варианты: большой, мини, экспериментальный (самозаправка), промышленный.

- **Код:** [`/obj/item/weldingtool`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/weldingtool.dm#L1), [`proc/switched_on()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/weldingtool.dm#L249), [`use()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/weldingtool.dm#L215), [`proc/try_heal_loop()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/weldingtool.dm#L162), [`/obj/item/weldingtool/experimental`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/weldingtool.dm#L403)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/tools.rs`, `crates/ss13_game/src/fire.rs`: включение кликом по слоту, расход топлива, урон 15 ожогом, поджог газа, резка стен; начало сварки вспыхивает (`tool_flash`, сила 2 − расстояние, в радиусе 1: без защиты жжёт глаза, `crates/ss13_game/src/tools.rs` `welder_flash`); нет заправки, вспышек во время работы (10% за тик), ремонта роботов, самозаправки

### Мультитул
Пульсирует провода, хранит буфер для связывания машин (`multitool_act`), показывает сведения о сети; абдукторский (видит назначение проводов), ИИ-детектор.

- **Код:** [`/obj/item/multitool`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/multitool.dm#L13)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wires.rs`: пульс проводов; буфер (`crates/ss13_game/src/tools.rs`, `MultitoolBuffer`, строка осмотра) и связывание датчиков камер, инжекторов, вентиляций и воздушных тревог (`crates/ss13_game/src/atmos_machinery/control.rs`, `multitool_act`); нет сведений о сети, абдукторского и ИИ-детектора; связывание других машин — в их группах

### Силовые инструменты
Челюсти жизни (лом/кусачки, вскрывают запитанные шлюзы), электродрель (отвёртка/ключ), складной космический нож-мультиинструмент.

- **Код:** [`/obj/item/crowbar/power`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/crowbar.dm#L115), [`/obj/item/screwdriver/power`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/screwdriver.dm#L85), [`/obj/item/spess_knife`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/spess_knife.dm#L3)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/tools.rs` (`toggle_power_tool`, `examine_lines`): челюсти жизни (лом ↔ кусачки; челюсти восстановления — лом ↔ вправление костей) и дрель (отвёртка ↔ ключ) меняют насадку по клику в руке, спрайт `_on`, звук смены, строка осмотра; `crates/ss13_game/src/doors.rs` (`force_open_act`, `DoorJob::Pry`): челюсти вскрывают запитанный шлюз за 5 с × `toolspeed` (электрифицированный бьёт током до и после), проверка болтов/сварки в момент эффекта; челюсти восстановления не вскрывают двери с командным/охранным и т. п. доступом и раз в минуту шлют тревогу в каналы СБ и медбея. Нет: складного космического ножа (радиальное меню, нет `DmTool` при `tool_behaviour = null`), перекусывания наручников (`cuffsnapping`), урона шлюзу при вскрытии (у дверей нет прочности), пожарного топора как вскрывающего инструмента

### Щётка от ржавчины
Счищает ржавчину со стен и полов.

- **Код:** [`/obj/item/wirebrush`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/wirebrush.dm#L5)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/wall_damage.rs`: щётка (`TOOL_RUSTSCRAPER`, ПКМ, 2 с) или сварка с 1 топлива (ПКМ, 5 с) снимает ржавчину (`Rusty`) с проржавевших стен и полов; см. «Повреждение стен» в `walls.md`

### Краскопульты декалей и труб
Краскопульт декалей рисует разметку на полу (tgui с выбором цвета и направления), краскопульт труб перекрашивает трубы.

- **Код:** [`/obj/item/airlock_painter/decal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/painter/decal_painter.dm#L1), [`paintable_decals.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/painter/paintable_decals.dm), [`/obj/item/pipe_painter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/pipe_painter.dm#L1); tgui: [`DecalPainter.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/DecalPainter.tsx)
- **Статус в ss13-rs:** ❌ нет

### Огнетушитель
Распыляет воду/пену по конусу, тушит огонь и людей, толкает стрелка в невесомости; заправка из бака; мини, самодельный, продвинутый.

- **Код:** [`/obj/item/extinguisher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/extinguisher.dm#L1), [`/obj/item/extinguisher/advanced`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/extinguisher.dm#L193)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/extinguisher.rs` (данные `DmExtinguisher`): все варианты (обычный, мини, самодельный, продвинутый, «fire extender»), предохранитель в руке, спрей по соседним и дальним кликам (5 частиц по 1 ед., ряд 3 клетки, у продвинутого по одной на 5 клеток, дальность `power`, кулдаун 1,2 с), частицы проходят столы/машины/структуры, вода и пена (`janitor::expose_cooled`) гасят очаги с охлаждением воздуха, горящих мобов и объекты, толчок в невесомости, заправка из `tanktypes`, Alt-клик в руке опорожняет, строка осмотра. Нет: пены-эффекта на полу, удара с замахом ПКМ, езды на стуле, охлаждения контейнеров, звуков

### Индуктор
Заряжает батареи APC, мехов и боргов на расстоянии от своей батареи.

- **Код:** [`/obj/item/inducer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/inducer.dm#L1)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/inducer.rs`: батарея внутри (по типу: high, upgraded у `orderable`, super и ×2 у `syndicate`, пустые `empty`/`sci` с открытым отсеком), отвёртка открывает отсек, в открытый вставляется батарея, `attack_self` её достаёт, лист плазмы даёт 1,5 стандартной ячейки; вне боевого режима по APC, батарее, киборгу, меху, обогревателю или MULE — шаги `do_after` 1 с, за шаг `min(свой заряд, недостача, rating_base × rating × множитель)`, искры; досягаемость перепроверяется в конце шага (в tg `target = user` — цель могла уйти). Нет: оверлеев `inducer-bat/nobat`, ЭМИ батареи внутри, строк осмотра, контекстных подсказок, варианта киборга (батарея киборга)

### Т-сканер
Показывает трубы и кабели под полом вокруг владельца.

- **Код:** [`/obj/item/t_scanner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/t_scanner.dm#L1)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/t_scanner.rs`: включение в руке (`t-ray0`/`t-ray1`), раз в секунду (общие часы всех сканеров) объекты под плиткой в радиусе 3 от владельца (сканер в руке, на поясе, в кармане) реплицируются на 0,8 с только его клиенту (`Invisibility` + `VisibleToMobs`) с альтернативным видом: альфа 128, клики сквозь; поднятая плитка возвращает объект всем. Нет звука `SFX_INDUSTRIAL_SCAN`; режимы Т-луча мезонов и MOD-модуля — в их группах

### Инструменты для кражи ядра
Специальная отвёртка и контейнер для извлечения плутониевого ядра из ядерной бомбы (цель предателя).

- **Код:** [`/obj/item/screwdriver/nuke`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/theft_tools.dm#L88), [`/obj/item/nuke_core_container`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/theft_tools.dm#L43)
- **Статус в ss13-rs:** ❌ нет
