# Щиток питания зоны (APC)
`/obj/machinery/power/apc`: распределяет энергию сети по каналам своей зоны и держит резервную ячейку. Провода APC (`/datum/wires/apc`) — см. ../engineering/.

### Обработка APC: зарядка ячейки и каналы
Ранняя обработка забирает из сети статическую нагрузку зоны, затем ячейка заряжается в четыре шага; поздняя обработка выключает каналы по уровню заряда (освещение < 15%, оборудование < 30%, окружение при пустой) и включает их обратно с гистерезисом.

- **Код:** [`/obj/machinery/power/apc`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_main.dm#L17), [`proc/early_process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_main.dm#L573), [`proc/charge_channel()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_main.dm#L700), [`proc/late_process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_main.dm#L599), [`proc/autoset()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_power_proc.dm#L94)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/power/model.rs` (`Apc::early_process`, `charge_channel`, `late_process`)

### Внешний вид APC
Оверлеи замка, состояния зарядки, каналов, свет щитка по зарядке; мигание при взломе. BandaStation заменяет иконку APC и иконку взлома.

- **Код:** [`proc/check_updates()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_appearance.dm#L64), [`proc/flicker_hacked_icon()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_appearance.dm#L111)
- **BandaStation:** [`modular_bandastation/aesthetics/apc/code/apc.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/apc/code/apc.dm#L1) — иконка; [`apc_appearance.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_appearance.dm#L114) — иконка `apcemag`
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/power/mod.rs` (`ApcLook`): замок, зарядка, каналы, свет; нет открытой крышки, сломанного и взломанного вида

### Окно APC
tgui-окно: заряд ячейки, включение главного рубильника, режимы каналов (авто/вкл/выкл), ночной режим освещения, блокировка ID-картой, перегрузка освещения для ИИ; BandaStation выносит перезагрузку после сбоя в `reboot()`.

- **Код:** [`ui_interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_main.dm#L382), [`ui_data()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_main.dm#L388), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_main.dm#L479), [`proc/toggle_breaker()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_power_proc.dm#L43); tgui: [`Apc.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Apc.jsx)
- **BandaStation:** [`apc_main.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_main.dm#L561) — `proc/reboot()`
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/power/apc_ui.rs`, `crates/ss13_tgui/src/interfaces/apc.rs`: окно `Apc` для рук и синтетиков: рубильник, режим зарядки, каналы (`setsubsystem`), нагрузки, блокировка для ИИ, перезагрузка при сбое; болты крышки, аварийное и ночное освещение только переключаются, «Перегрузка» ничего не делает, сбойного ИИ нет

### Блокировка, взлом и ИИ-доступ
Замок открывается ID-картой с доступом инженеров; емаг ломает замок; ИИ управляет APC удалённо (провод ИИ).

- **Код:** [`proc/togglelock()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_tool_act.dm#L476), [`emag_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_tool_act.dm#L436), [`proc/can_use()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_attack.dm#L143), [`proc/connect_remote_access()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_main.dm#L442)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wires.rs`: провод разблокировки и замок как состояние; емаг (`crates/ss13_game/src/emag.rs`, `Apc::emagged`): с закрытой панелью интерфейс разблокируется навсегда, отвёртка панель не открывает, ИИ не может заблокировать; нет ID-карты, мигания взломанного вида и малф-взлома

### Крышка, ячейка и монтаж APC
Ломом снимается крышка, из-под неё вынимается и вставляется ячейка; отвёртка открывает панель; разборка: ячейка → электроника → терминал → сварка рамки. Сборка из рамки (`wallframe`), электроники и кабеля.

- **Код:** [`crowbar_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_tool_act.dm#L236), [`proc/cell_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_tool_act.dm#L55), [`screwdriver_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_tool_act.dm#L303), [`proc/electronics_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_tool_act.dm#L140), [`proc/wallframe_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_tool_act.dm#L201), [`welder_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_tool_act.dm#L350), [`/obj/item/electronics/apc`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_main.dm#L818)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wires.rs`: отвёртка открывает панель проводов; крышки, ячейки и монтажа нет (shortcut `opened`)

### Сбои: ЭМИ, перегрузка, поломка
ЭМИ разряжает ячейку и вырубает щиток на время, перегрузка освещения бьёт лампы зоны, `energy_fail` вызывает временный блэкаут (события grid check), поломка APC.

- **Код:** [`emp_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_tool_act.dm#L460), [`proc/overload_lighting()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_main.dm#L749), [`proc/energy_fail()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_power_proc.dm#L137), [`atom_break()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_main.dm#L368)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wires.rs`: короткое замыкание и отключение главного питания через провода; ЭМИ (`crates/ss13_game/src/emp/power.rs`): ячейка разряжается, все каналы выключены, через 60 с оборудование и среда снова auto (свет остаётся выключенным, как в tg); нет перегрузки ламп, поломки

### Ночной режим освещения
Переключатель в APC (и автоматический при низком заряде) приглушает лампы зоны.

- **Код:** [`proc/toggle_nightshift_lights()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_power_proc.dm#L20), [`proc/set_nightshift()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_power_proc.dm#L142)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/power/mod.rs` (`set_nightshift`, `toggle_nightshift_lights`, `apply_low_power_nightshift`), `power/model.rs` (`Apc::late_process`): переключатель в окне APC с откатом 10 с и отказом при низком заряде (`disable_nightshift_toggle`), принудительное включение ниже порога оборудования и снятие при восстановлении (кроме идущего события Night Shift вне красной тревоги — `events/nightshift.rs`); нагрузка освещения остаётся полной (shortcut)

### Зарядка и разрядка эфириалов
Эфириал правым кликом пустой рукой заряжает свой желудок от APC или отдаёт ему энергию.

- **Код:** [`proc/ethereal_interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_attack.dm#L28), [`proc/charge_from_ethereal()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_attack.dm#L77)
- **Статус в ss13-rs:** ❌ нет

### Взлом APC сбойным ИИ
Malf ИИ взламывает APC (рост вычислительной мощности), может переселиться в него и вернуться.

- **Код:** [`proc/malfhack()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_malf.dm#L12), [`proc/malfoccupy()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_malf.dm#L28), [`proc/malfshunt()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_malf.dm#L41)
- **Статус в ss13-rs:** ❌ нет

### Маппинг-хелперы APC
Хелперы карты: ячейка 5k/10k, без заряда, полный заряд, разблокирован, синдикатский и away-доступ, перерезанный провод ИИ; варианты APC `auto_name`, `worn_out`.

- **Код:** [`/obj/effect/mapping_helpers/apc`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/mapping_helpers.dm#L544), [`apc/cell_10k`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/mapping_helpers.dm#L617), [`/obj/machinery/power/apc/auto_name`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_mapping.dm#L5)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/power.rs`: ячейки и заряд; нет `cut_AI_wire`, `unlocked`, `syndicate_access`, `away_general_access` (shortcut)
