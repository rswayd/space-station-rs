# Ядро шаттлов
Подсистема `SSshuttle`, док-порты, перенос участка карты между доками, транзитное пространство и загрузка шаблонов шаттлов.

### Подсистема SSshuttle
Регистрирует все мобильные и стационарные порты, раз в тик проверяет таймеры шаттлов, держит очередь транзитных доков, блокировку отзыва, «враждебное окружение» и торговую блокаду, автовызов при гибели экипажа.

- **Код:** [`SUBSYSTEM_DEF(shuttle)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/shuttle.dm#L12), [`proc/setup_shuttles()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/shuttle.dm#L203)
- **BandaStation:** [`modular_bandastation/shuttles/code/docking_ports.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/shuttles/code/docking_ports.dm#L2) — регистрирует дополнительные шаттлы форка в подсистеме
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/shuttle.rs`: только аварийный шаттл (`Emergency`, `StationaryDock`); нет общего реестра шаттлов, транзитных резервов, торговой блокады

### Док-порты (мобильные и стационарные)
`/obj/docking_port/mobile` — сам шаттл (размеры `width/height/dwidth/dheight`, направление, режим и таймер), `/obj/docking_port/stationary` — место стыковки с `shuttle_id`; `return_ordered_turfs()` задаёт порядок тайлов при переносе.

- **Код:** [`/obj/docking_port`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/shuttle.dm#L2), [`/obj/docking_port/mobile`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/mobile_port.dm#L1), [`/obj/docking_port/stationary`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/stationary_port/stationary_port.dm#L2), [`port_types.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/stationary_port/port_types.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/shuttle.rs`, `crates/ss13_mapload/src/dock.rs`: стационарные порты с карты регистрируются, мобильный порт есть только у аварийного шаттла; нет проверки `canDock`, перекрытия доков, скрытых доков

### Перенос шаттла (initiate_docking)
Мгновенный перенос всех тайлов шаттла (с пропуском `baseturf_skipover/shuttle`) и их содержимого на тайлы дока назначения: смена областей, перенос воздуха, раздавливание того, что стоит на месте посадки, вызовы `onShuttleMove`/`afterShuttleMove` у атомов, поворот при другом направлении дока.

- **Код:** [`proc/initiate_docking()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/shuttle_move.dm#L3), [`shuttle_move_callbacks.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/shuttle_move_callbacks.dm), [`shuttle_rotate_callbacks.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/shuttle_rotate_callbacks.dm)
- **BandaStation:** [`docking_ports.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/shuttles/code/docking_ports.dm#L72) — перед стыковкой запускает эффект «зоны посадки» для некоторых шаттлов
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/shuttle.rs` (`initiate_docking`): тайлы, содержимое, области, воздух и свет переносятся; моб на месте посадки умирает вместо гиба; поворот меняет только `Facing`; нет коллбэков атомов (кабели, трубы, двери не перестраиваются); нет стыковки насосов шлюза (`airlock_pump` `on_dock_request`/`safe_dock`/`undock`, `shuttledocked`), а связывание насосов при загрузке уже есть (`crates/ss13_game/src/atmos_machinery/airlock_pump.rs`)

### Таймеры и режимы мобильного порта
Общий цикл `request()` → `check()`: режимы idle/called/docked/igniting/escape/recall/prearrival, звуки взлёта и посадки, предупреждение перед отлётом.

- **Код:** [`proc/request()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/mobile_port.dm#L292), [`proc/check()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/mobile_port.dm#L440)
- **BandaStation:** [`mobile_port.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/mobile_port.dm#L704) — шаттлы могут переопределять звуки взлёта и посадки
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/shuttle.rs`: полный цикл режимов у аварийного шаттла и звуки гиперпространства (`hyperspace_sound`: от ближайшего двигателя, дальняя версия в 2.5 раза дальше, без двигателей — от двери; разогрев за 5 с, взлёт, посадка за 0.5 с; также у шаттла прибытия); `crates/ss13_game/src/shuttle_nav.rs` (`NavShuttle`): idle → igniting (5.5 с) → транзит (10 с) → стыковка у шаттлов с консолями; нет recharging/prearrival, предупреждения перед отлётом и переопределения звуков BandaStation (`get_custom_sound`, нужны шаттлы форка)

### Транзитное пространство
На время полёта шаттлу резервируется участок с тайлами гиперпространства нужного направления (`generate_transit_dock`); после прилёта резерв очищается, а выпавшие в транзит предметы и мобы выбрасываются.

- **Код:** [`proc/generate_transit_dock()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/shuttle.dm#L668), [`/obj/docking_port/stationary/transit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/stationary_port/port_types.dm#L39), [`/turf/open/space/transit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/space/transit.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/shuttle.rs` (`make_transit`): один фиксированный космический уровень на раунд, без тайлов гиперпространства и выбрасывания упавших

### Рябь перед посадкой
За несколько секунд до прилёта на месте посадки появляются «ряби» (`ripple`), предупреждая, что здесь будет раздавлено всё.

- **Код:** [`/obj/effect/abstract/ripple`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/misc/ripple.dm#L1), [`proc/check_effects()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/mobile_port.dm#L489)
- **BandaStation:** [`landing_zones.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/shuttles/code/landing_zones.dm#L1) — альтернативный эффект «зоны высадки» (96x96) для части шаттлов
- **Статус в ss13-rs:** ❌ нет

### Шаблоны шаттлов и загрузка
`/datum/map_template/shuttle` описывает `.dmm` шаттла из `_maps/shuttles`, его цену и доступность; `action_load`/`load_template` загружают шаблон в резерв и подменяют действующий шаттл.

- **Код:** [`/datum/map_template/shuttle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/shuttles/_shuttle.dm#L1), [`proc/action_load()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/shuttle.dm#L918)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/dock.rs`, `crates/ss13_game/src/shuttle.rs` (`ShuttleTemplates`): шаблоны грузятся вместе со станцией (аварийный из json карты, капсулы, прибытие, инфильтратор, каталог покупки `crates/ss13_game/src/shuttle_catalog.rs`); замена аварийного шаттла в раунде — подмена на заранее загруженный шаблон каталога. Загрузки шаблона посреди раунда нет: стартовые системы (двери, воздух, питание, хранилища, консоли) инициализируют объекты карты один раз, поздно загруженный шаблон остался бы неработающим — нужен проход инициализации после загрузки карты

### Манипулятор шаттлов (админ)
Админская tgui-панель: список шаблонов, предпросмотр, загрузка/замена шаттла, перемещение шаттлов по докам; `Move Shuttle`/`Lock` из запросов консолей.

- **Код:** [`/obj/machinery/shuttle_manipulator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/misc/manipulator.dm#L1); tgui: [`ShuttleManipulator.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ShuttleManipulator.jsx)
- **BandaStation:** [`shuttle_console.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/shuttle_consoles/shuttle_console.dm#L228) — запрос на перемещение шаттла уходит только админам с `R_ADMIN`
- **Статус в ss13-rs:** ❌ нет

### Враждебное окружение и блокировки отзыва
Антагонисты (культ, блоб, нюка) регистрируют «враждебное окружение» — аварийный шаттл не улетает, пока оно есть; `block_recall` запрещает отзыв, торговая блокада останавливает карго-шаттл.

- **Код:** [`proc/registerHostileEnvironment()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/shuttle.dm#L559), [`proc/block_recall()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/shuttle.dm#L286), [`proc/registerTradeBlockade()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/shuttle.dm#L568)
- **Статус в ss13-rs:** ❌ нет
