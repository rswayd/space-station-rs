# Шлюзы (airlocks)
Базовая дверь и шлюз: доступ, анимации, болты, питание, сварка, взлом ломом и емагом, сборка и электроника, циклирование.

### Базовая дверь: открытие по доступу и автозакрытие
`/obj/machinery/door`: открывается от толчка или клика при доступе ID (`allowed()`), иначе мигает «отказ»; закрывается сама, если в проёме нет плотных объектов; анимации с таймингами по сегментам.

- **Код:** [`/obj/machinery/door`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L6), [`proc/try_to_activate_door()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L342), [`proc/open()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L589), [`proc/close()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L624), [`proc/autoclose()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L692)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/doors.rs`: доступ, анимация отказа, тайминги tg, автозакрытие с повтором при занятом проёме

### Стук в дверь
ПКМ пустой рукой по закрытой двери — стук со звуком (своим для стеклянных дверей).

- **Код:** [`door.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L320)
- **BandaStation:** [`door.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L93), [`airlock.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L169) — механика стука целиком добавлена форком
- **Статус в ss13-rs:** ❌ нет

### Защита проёма и раздавливание
Безопасный шлюз не закрывается на объект в проёме; с перерезанным проводом безопасности закрывается и давит мобов (`crush`).

- **Код:** [`proc/CheckForMobs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L666), [`proc/crush()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L671)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/doors.rs`: безопасная дверь ждёт свободного проёма; небезопасная закрывается, но не наносит урон

### Неограниченная сторона (unres)
С одной стороны шлюз открывается без доступа (выход из отдела); вариант с задержкой 2–4 с, которую сбивает перепад давления.

- **Код:** [`proc/unrestricted_side()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L378), [`proc/attempt_delayed_unres_open()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L383)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/doors.rs`, `crates/ss13_mapload/src/door.rs`: сторона из маппинг-хелпера и провод; отложенный вариант открывается сразу

### Болты шлюза
Заболченный шлюз не открывается и не закрывается ни доступом, ни ломом; болты ставятся проводом, ИИ, кнопками; индикатор болтов.

- **Код:** [`proc/bolt()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L258), [`proc/unbolt()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L276), [`proc/toggle_bolt()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1852)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/doors.rs`, `crates/ss13_game/src/wires.rs`: болты с карты и через провод, огни болтов

### Питание шлюза
Шлюз работает от питания области: основное и резервное питание, отключение проводами (60 с), без питания — открывается только ломом.

- **Код:** [`proc/handle_main_power()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L404), [`proc/set_main_outage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L380), [`proc/isAllPowerCut()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L360)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/doors.rs`, `crates/ss13_game/src/wires.rs`, `crates/ss13_game/src/power/mod.rs` (`Unpowered`): основное/резервное питание и отключения; внешний вид не учитывает питание области (shortcut)

### Электрификация
Шлюз под током бьёт касающегося (урон от заряда APC и сети, оглушение), временно (30 с) или постоянно; изолирующие перчатки защищают.

- **Код:** [`proc/set_electrified()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1623), [`proc/isElectrified()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L345)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wires.rs` (`Electrified`, `shock_user`): удар током с оглушением и параличом; нет искр, тряски, передачи через тягу

### Заварка шлюза
Сварка заваривает/разваривает закрытую дверь; ПКМ сваркой чинит повреждённую.

- **Код:** [`try_to_weld()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1169), [`try_to_weld_secondary()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1190), [`proc/weld_checks()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1204)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/doors.rs`: заваренные с карты шлюзы не открываются и рисуются; сваркой заварить нельзя

### Взлом ломом и силовыми инструментами
Ломом открывается обесточенный незаболченный шлюз; челюсти жизни и руки ксеноморфа вскрывают запитанный с задержкой; лом вынимает электронику из разобранного.

- **Код:** [`try_to_crowbar()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1261), [`try_to_force_door_open()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1390), [`proc/check_crowbar_force()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L440)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/doors.rs` (`crowbar_act`): ломом мгновенно открывается/закрывается обесточенный шлюз (`BYPASS_DOOR_CHECKS`, звук `airlockforced`), болты, сварка и питание останавливают с сообщением tg; `// shortcut:` нет челюстей жизни и пожарного топора по запитанному, снятия электроники

### Емаг на шлюзе
Емаг вскрывает шлюз: искры, дверь открывается и ломается (больше не закрывается и не реагирует на доступ).

- **Код:** [`emag_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1531), [`proc/finish_emag_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1544)
- **Статус в ss13-rs:** ❌ нет

### Аварийный доступ и красная тревога
Режим аварийного доступа открывает шлюз всем; при красной тревоге шлюзы с `red_alert_access` пускают людей без доступа.

- **Код:** [`proc/toggle_emergency()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1868), [`proc/has_active_reta_access()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L510), [`proc/check_security_level()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L234)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/wires.rs` (shortcut: no emergency access)

### Укрепление шлюза и уплотнитель двери
Пластины пласталевого укрепления на шлюзе мешают взлому; уплотнитель (`door_seal`) блокирует открытие, пока его не снимут.

- **Код:** [`proc/try_reinforce()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1074), [`proc/try_remove_seal()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L255), [`/obj/item/door_seal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/door_seal.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Повреждение и ЭМИ шлюза
Шлюзы получают урон и ломаются (сыплются искры, перестают работать), ЭМИ сбивает питание и может открыть; взрывы, удары разряда тесла.

- **Код:** [`take_damage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L498), [`proc/on_break()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1609), [`emp_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1615), [`ex_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L760)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/obj_damage.rs`: двери урон не получают

### Виды шлюзов
~220 подтипов: отделов (командный, СБ, инженерный, медицинский, научный, ...), стеклянные, техобслуживания, внешние, хранилище, высокозащищённые, шаттловые, минеральные (урановый фонит, плазменный горит), деревянные, культистские, абдукторские, многотайловые (`multi_tile`).

- **Код:** [`/obj/machinery/door/airlock`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L75), [`/obj/machinery/door/airlock/highsecurity`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L2411), [`/obj/machinery/door/airlock/external`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L2301), [`/obj/machinery/door/airlock/multi_tile`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L2583), [`/obj/machinery/door/airlock/material`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L2558)
- **BandaStation:** [`modular_bandastation/aesthetics/airlocks`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/airlocks/code/airlock_types.dm) — свои спрайты и звуки шлюзов
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/doors.rs`, `crates/ss13_mapload/src/door.rs`: все виды грузятся с доступом, спрайтами, многотайловые получают заполнитель; свойства минеральных шлюзов не перенесены

### Циклирующие шлюзы
Пары шлюзов с общим `closeOtherId` (или маппинг-хелпер `cyclelink`): открытие одного закрывает другой.

- **Код:** [`proc/cyclelinkairlock()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L225), [`proc/update_other_id()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L217)
- **BandaStation:** [`airlock.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L702) — санитизация ID цикла в осмотре
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_mapload/src/door.rs` (shortcut: cyclelink helpers not applied)

### Окно управления шлюзом (tgui)
Окно шлюза для ИИ и для взломанных систем: питание, болты, ток, аварийный доступ, скорость, огни, безопасность.

- **Код:** [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1771); tgui: [`AiAirlock.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AiAirlock.jsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/silicon/airlock.rs` (`AiAirlockWindow`), `crates/ss13_tgui/src/interfaces/ai_airlock.rs`: питание, ток, ID-сканер, аварийный доступ, болты, огни, безопасность, скорость, открыть/закрыть, всё с `user_allowed`; взлома ИИ нет

### Сборка и разборка шлюза
Каркас шлюза (`door_assembly`) из листов: гаечный ключ крепит, кабель, электроника, листы стекла/минералов, отвёртка завершает; разборка обратна (сварка, кусачки, лом вынимает электронику).

- **Код:** [`/obj/structure/door_assembly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/door_assembly.dm#L1), [`door_assembly_types.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/door_assembly_types.dm), [`proc/prepare_deconstruction_assembly()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1649)
- **BandaStation:** [`door_assembly.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/door_assembly.dm#L369) — санитизация имени двери; [`airlock_assembly_types.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/airlocks/code/airlock_assembly_types.dm)
- **Статус в ss13-rs:** ❌ нет — двери существуют только с карты (`crates/ss13_game/src/doors.rs`, shortcut)

### Электроника шлюза
Плата доступа: tgui-выбор списка доступа (все/любой), неограниченная сторона, имя, циклирующий ID.

- **Код:** [`/obj/item/electronics/airlock`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock_electronics.dm#L1); tgui: [`AirlockElectronics.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AirlockElectronics.tsx)
- **Статус в ss13-rs:** ❌ нет

### Покраска шлюзов
Краскопульт для шлюзов меняет раскраску шлюза и каркаса на тему отдела.

- **Код:** [`/obj/item/airlock_painter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/painter/airlock_painter.dm#L1), [`proc/change_paintjob()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L1498)
- **Статус в ss13-rs:** ❌ нет

### Контроллеры шлюзовых камер
Встроенные контроллеры шлюзовых камер: датчик давления, насос, цикл «внутрь/наружу» (tgui), контроллер доступа с кнопками и встроенные контроллеры инсинератора.

- **Код:** [`/obj/machinery/airlock_controller`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/embedded_controller/airlock_controller.dm#L8), [`/obj/machinery/airlock_sensor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/airlock_control.dm#L42), [`/obj/machinery/door_buttons/airlock_controller`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/embedded_controller/access_controller.dm#L106); tgui: [`AirlockController.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AirlockController.tsx), [`AirlockButtonController.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AirlockButtonController.tsx)
- **Статус в ss13-rs:** ❌ нет

### Пульт управления дверьми
Пульт (`door_remote`) с доступом отдела открывает, болтирует или включает аварийный доступ на расстоянии.

- **Код:** [`/obj/item/door_remote`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/control_wand.dm#L7)
- **BandaStation:** [`control_wand.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/control_wand.dm#L56) — пульт учитывает wildcard-доступ
- **Статус в ss13-rs:** ❌ нет
