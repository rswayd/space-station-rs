# Исследования (техвеб, R&D, лаборатории)

Техвеб станции: очки исследований, узлы технологий, R&D-консоль, серверы, производство по чертежам, детали машин и аномальные артефакты.

### Техвеб и узлы технологий
Общая сеть исследований станции (`/datum/techweb/science`): узлы технологий (~135 узлов в 17 файлах: инженерия, медицина, мехи, киборги, MOD, схемы, BEPIS, синдикат, инопланетные и т. д.) открываются за очки и разблокируют чертежи для лат. Узлы могут требовать завершённых экспериментов (`required_experiments`) или дешеветь за них (`discount_experiments`).

- **Код:** [`/datum/techweb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/techweb/_techweb.dm#L9), [`proc/research_node()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/techweb/_techweb.dm#L337), [`/datum/techweb_node`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/techweb/_techweb_node.dm#L9), [`/datum/techweb/science`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/techweb/techweb_types.dm#L4), узлы: [`techweb/nodes/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/techweb/nodes)
- **BandaStation:** [`medbay_nodes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/techweb/nodes/medbay_nodes.dm#L80) — из узла убран чертёж диска лечащего луча
- **Статус в ss13-rs:** 🟡 `ss13_game::science::techweb` (данные — `ss13_mapload::research`, `research.ron`): узлы, чертежи, стартовые и скрытые узлы, доступные/видимые, тиры, цены, очередь исследований по правилам `_techweb.dm`; эксперименты (`science::experiments`): техвеб получает обязательные и скидочные эксперименты узлов, ставших достижимыми, и `experiments_to_unlock`; обязательные блокируют узел (из шести узлов реально блокируется medbay_equip_adv — скан галоперидола; эксперименты неподдержанных видов считаются выполненными), выполненные скидочные снижают цену, узел, изученный без скидочного эксперимента, возвращает скидку при его выполнении (`skipped_experiment_types`). Проверено: `science::experiments::tests`, `tests/it/science.rs` (`experiments_scan_and_unlock`). Нет бустов, дисков, объявлений по радио

### Начисление очков исследований (SSresearch)
Подсистема `SSresearch` раз в тик начисляет очки всем подключённым техвебам от рабочих серверов; дополнительные очки дают эксперименты, публикации, разборка предметов и бонусы битраннинга.

- **Код:** [`SUBSYSTEM_DEF(research)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/networks/research.dm#L1), [`fire()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/networks/research.dm#L73)
- **Статус в ss13-rs:** 🟡 `ss13_game::science::points`: раз в секунду `single_server_income` за секунду, пока работает хоть один сервер (запитан, не отключён, не выключен с консоли управления); очередь; премия `techweb_bounty` в бюджет науки за узел; очки экспериментов (`points_reward`, возврат скидок — `science::experiments`); нет публикаций, битраннинга, `income_modifier` мастер-сервера

### R&D-консоль
Компьютер научного отдела: дерево техвеба, покупка узлов, просмотр и выбор экспериментов, работа с дисками технологий и чертежей. Требует доступ, может быть заблокирована.

- **Код:** [`/obj/machinery/computer/rdconsole`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/rdconsole.dm#L19), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/rdconsole.dm#L369); tgui: [`Techweb/index.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Techweb/index.tsx)
- **BandaStation:** [`rdconsole.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/rdconsole.dm#L34) — консоль по умолчанию заблокирована (`locked = TRUE`)
- **Статус в ss13-rs:** 🟡 `ss13_game::science::console` + `science::client` (tgui `Techweb`: шапка, вкладки Researched/Available/Future, карточки узлов, детали): блокировка (`req_access`, BandaStation `locked = TRUE`), `researchNode`, `enqueueNode`/`dequeueNode` (RD первым), журнал исследований; нет дисков, емага, поиска, разделов экспериментов

### Серверы R&D и консоль управления серверами
Серверы (`/obj/machinery/rnd/server`, мастер-сервер с особым уничтожением) генерируют очки, перегреваются, ломаются от ЭМИ; консоль управления позволяет включать/выключать серверы и смотреть логи исследований.

- **Код:** [`/obj/machinery/rnd/server`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/server.dm#L14), [`/obj/machinery/rnd/server/master`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/server.dm#L118), [`/obj/machinery/computer/rdservercontrol`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/server_control.dm#L1); tgui: [`ServerControl.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ServerControl.tsx)
- **Статус в ss13-rs:** 🟡 `ss13_game::science::points` (`ResearchServer`): серверы дают очки, пока запитаны, не под ЭМИ (`EMPED` на 60 с — `crates/ss13_game/src/emp/machines.rs`) и не выключены; случайный шестнадцатеричный суффикс имени. Консоль управления — `ss13_game::science::server_control` + tgui `ServerControl`: серверы со статусом (`get_status_text`), R&D-консоли с блокировкой, журнал исследований; `lockdown_server`/`lock_console` требуют доступ RD; сборка из рамы (`machine_frame`). Проверено: `tests/it/science.rs` (`server_control_toggles_servers`). Нет: мастер-сервера (кража HDD, `income_modifier`), емага и перепривязки мультитулом (один техвеб)

### Базовая R&D-машина
Общий предок научных машин: подключение к техвебу, панель, разборка, взлом емагом.

- **Код:** [`/obj/machinery/rnd`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/rdmachines.dm#L5)
- **Статус в ss13-rs:** 🟡 `ss13_game::science` — связь с техвебом станции неявная (один техвеб); нет панели, проводов, разборки, мультитула, емага

### Деструктивный анализатор
Разбирает предмет, давая очки и открывая скрытые узлы (`boost_item_paths`), возвращая материалы.

- **Код:** [`/obj/machinery/rnd/destructive_analyzer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/destructive_analyzer.dm#L10); tgui: [`DestructiveAnalyzer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/DestructiveAnalyzer.tsx)
- **Статус в ss13-rs:** 🟡 `ss13_game::science::analyzer` + tgui `DestructiveAnalyzer`: загрузка, выброс, разборка за очки (`techweb_point_items`, раз на тип) и раскрытие скрытых узлов (`required_items_to_unlock`); нет alt-клика, проводов, панели, `resistance_flags` у предметов

### Протолат, импринтер схем, техфаб
Производство по исследованным чертежам из материалов (рудный силос/локальное хранилище); эффективность зависит от деталей. Протолат делает предметы, импринтер — платы, техфаб — смесь.

- **Код:** [`/obj/machinery/rnd/production`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/machinery/_production.dm#L1), [`protolathe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/machinery/protolathe.dm#L1), [`circuit_imprinter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/machinery/circuit_imprinter.dm#L1), [`techfab`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/machinery/techfab.dm#L1); tgui: [`Fabricator.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Fabricator.tsx)
- **BandaStation:** [`_production.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/machinery/_production.dm#L252) — в UI передаётся оригинальное имя чертежа (перевод названий)
- **Статус в ss13-rs:** 🟡 `ss13_game::science::lathe` + tgui `Fabricator` (категории, ×5/×10/макс., материалы с выбросом листов): чертежи изученного техвеба по типу машины и отделу, стоимость и время по tg, `RefreshParts` (`lathe::refresh_parts`: эффективность по сервоприводам, локальное хранилище по бункерам, цены окна) после замены деталей RPED (панель — отвёрткой), с карты — детали платы по умолчанию, энергия на предмет, стаки печатаются разом, вставка стаков, рудный силос; нет разборки, анимаций, направления выдачи, переноса материалов в предмет, чертежей с классами материалов (`material_requirement`), оплаты

### Отдельные латы отделов
Варианты протолата/техфаба/импринтера для инженерии, сервиса, медицины, карго, науки и СБ: печатают только чертежи своего отдела.

- **Код:** [`protolathe/department`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/machinery/departmental_protolathe.dm#L1), [`techfab/department`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/machinery/departmental_techfab.dm#L1), [`circuit_imprinter/department`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/machinery/departmental_circuit_imprinter.dm#L1)
- **Статус в ss13-rs:** 🟡 `ss13_game::science::lathe`: департаментные протолаты/техфабы/импринтеры печатают только чертежи своего отдела (`allowed_department_flags`); полос отдела и оплаты нет

### Чертежи (designs)
`/datum/design` описывает результат, материалы, категорию и тип машин-производителей; файлы по семействам (платы, детали, медицина, оружие, мехи, шахта, телекомы, автолат, схемы wiremod и т. д.). Автолат и лимбгроуер используют автооткрывающиеся техвебы.

- **Код:** [`/datum/design`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/designs/_design.dm#L19), [`/datum/techweb/autounlocking`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/techweb/techweb_types.dm#L48), чертежи: [`designs/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/designs)
- **BandaStation:** [`power_designs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/designs/power_designs.dm#L220) — удалён чертёж диода лечащего луча
- **Статус в ss13-rs:** 🟡 `ss13_mapload::research` (все `/datum/design`: тип сборки, материалы, путь, категории, отделы, время) → `ss13_game::science`; печатаются только на протолате/импринтере/техфабе (автолат, мехфаб, биогенератор и пр. не реализованы)

_«Автолат» описано в [../engineering/machines.md](../engineering/machines.md)._

### Диски технологий и чертежей
Диск техвеба переносит исследованные узлы между сетями; диск чертежей хранит отдельные чертежи; BEPIS-диск открывает экспериментальный узел.

- **Код:** [`/obj/item/disk/tech_disk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/research_disk.dm#L1), [`/obj/item/disk/design_disk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/designs/_design_disk.dm#L1), [`design_disk/bepis`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/designs/_design_disk.dm#L28)
- **Статус в ss13-rs:** ✅ `ss13_game::science::console` (`TechDisk`, `DesignDisk`, `ensure_disk`): диски вставляются в два слота R&D-консоли; диск техвеба начинает со стартовыми узлами (CentCom — со всеми), `uploadDisk` изучает его узлы бесплатно (`absorb_techdisk`), `loadTech` копирует изученные узлы на диск, обе копии — за общим кулдауном 5 с; диск чертежей добавляет свои чертежи (диски конечностей — `blueprints` из `limb_disks`), BEPIS-диск — чертежи случайного экспериментального узла, при загрузке раскрывает и изучает узел; `ejectDisk` выбрасывает диск на тайл консоли; в tgui `Techweb` — строки дисков в шапке (`t_disk`, `d_disk`). Проверено: `tests/it/science.rs` (`rd_console_disks`). Списка экспериментальных узлов BEPIS-машины (`remove_tech`) нет — машина BEPIS не портирована

### Детали машин (stock parts)
Конденсаторы, сканирующие модули, сервоприводы, микролазеры, бункеры материи четырёх тиров; их рейтинг через `RefreshParts()` определяет скорость и эффективность машин.

- **Код:** [`/obj/item/stock_parts`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/stock_parts.dm#L5), [`/datum/stock_part`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/stock_parts/stock_part_datum.dm#L7), [`proc/RefreshParts()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/_machinery.dm#L829)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/stock_parts.rs`: детали внутри машины и рамы — те же предметы (`PartOf`/`ComponentParts`, плата тоже; разборка возвращает их, ячейка с зарядом), машина с карты создаёт детали платы по умолчанию при первой надобности (`ensure_parts`); тир — `rating` (`<assets>/mecha.ron` `stock_parts`), `get_part_rating` ячейки — `maxcharge × 10 + charge`; `RefreshParts` (`refresh`, `PartRatings`: тиры по видам в порядке `component_parts`, ёмкости ячеек) после сборки и замены деталей, машины читают свои формулы tg (`ratings`) — список в [../engineering/machines.md](../engineering/machines.md) «Панель, откручивание и разборка машин». Проверено: `crates/ss13_game/tests/it/stock_parts.rs`, `stock_parts::tests`. Нет: роста `idle/active_power_usage` от `energy_rating` деталей (машины берут мощность из DM), рейтинга мензурок/сборок/картриджей торгоматов как деталей

### RPED (замена деталей)
Быстрая замена деталей машины на лучшие из сумки; блюспейс-вариант работает на расстоянии.

- **Код:** [`/obj/item/storage/part_replacer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/part_replacer.dm#L2), [`part_replacer/bluespace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/part_replacer.dm#L55), [`proc/exchange_parts()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/_machinery.dm#L1079)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/stock_parts.rs` (`exchange_parts`, `install_into_frame`, `rped_accepts`, `ranged_rped`), `crates/ss13_game/src/machine_frame.rs` (`rped_interaction`), правило хранилища — `crates/ss13_game/src/storage.rs` (`Refusal::CantHold`): RPED хранит детали с рейтингом, платы и до 30 стекла/пластали/кабеля/блюспейс-кристаллов; на машине с открытой панелью меняет каждую деталь на лучшую того же вида (по убыванию рейтинга), старые — в RPED, затем `RefreshParts`; с закрытой — список деталей в чат; на подключённой раме ставит плату и детали, на раме с платой — недостающие детали и стаки; блюспейс-RPED работает и с закрытой панелью, и на расстоянии в зоне видимости. Проверено: `crates/ss13_game/tests/it/stock_parts.rs` (`rped_upgrades_and_parts_come_back`). Нет: деления стака по квоте 30 (стак сверх квоты отклоняется целиком), быстрого сбора с пола, выброса деталей низшего тира, выбора платы из нескольких (ставится первая), взрыва заряженных подстроенных ячеек, луча и анимации; RPED киборгов → [../silicon/cyborgs.md](../silicon/cyborgs.md)

### E.X.P.E.R.I-MENTOR
Машина для опытов над странными предметами (тык, облучение, газ, нагрев, охлаждение, уничтожение, открытие): случайные эффекты, раскрытие реликвий, очки.

- **Код:** [`/obj/machinery/rnd/experimentor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/experimentor/experimentor.dm#L1), [`/datum/experimentor_result_handler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/experimentor/experiments.dm#L1); tgui: [`Experimentor.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Experimentor.tsx)
- **Статус в ss13-rs:** ❌ нет

### Странные объекты (реликвии)
`/obj/item/assembly/relic`: случайный артефакт с неизвестным эффектом (телепорт, взрыв, клоны, флеш и т. д.), раскрывается экспериментором.

- **Код:** [`/obj/item/assembly/relic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/relics.dm#L4), [`activate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/relics.dm#L106)
- **Статус в ss13-rs:** ❌ нет

### Гизмо
Случайно генерируемые устройства-головоломки (предметы и машины) с режимами: телепорт, копирование, электричество, настроение и т. п.; управляются интерфейсом/голосом.

- **Код:** [`/obj/item/gizmo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/gizmo/gizmo_items.dm#L2), [`/obj/machinery/gizmo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/gizmo/gizmo_machines.dm#L2), [`/datum/gizmo_controller`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/gizmo/gizmo_controller.dm#L4), режимы: [`gizmo/gizmodes/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/gizmo/gizmodes)
- **BandaStation:** [`gizmo_items.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/gizmo/gizmo_items.dm#L13) — случайные имена гизмо
- **Статус в ss13-rs:** ❌ нет

### Скиллчип инженера для RCD
Без скиллчипа инженера (выдаётся инженерам и атмосферникам) RCD не позволяет деконструкцию и настройку доступа шлюзов; силиконы, дроны и ремботы могут без чипа.

- **Код:** [`/obj/item/construction/rcd`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RCD.dm#L6)
- **BandaStation:** [`modular_bandastation/skillchip_rcd/code/skillchip_rcd.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/skillchip_rcd/code/skillchip_rcd.dm#L19) — проверка `TRAIT_ADVANCED_RCD_KNOWLEDGE`
- **Статус в ss13-rs:** ❌ нет
