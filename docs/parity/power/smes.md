# Накопители: SMES, мониторинг, аккумуляторы энергии
Большие накопители энергии станции и консоли контроля сети.

### SMES: зарядка и выдача
Накопитель берёт энергию из сети терминала (`input_level`) и выдаёт в выходную сеть (`output_level`), неиспользованную выдачу забирает обратно; ёмкость — сумма батарей платы.

- **Код:** [`/obj/machinery/power/smes`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L4), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L351), [`proc/restore()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L409), [`proc/adjust_charge()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L177)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/power/model.rs` (`Smes::process_input`, `process_output`, `restore`)

### Окно SMES
tgui-окно: заряд, режимы ввода и вывода (вкл/авто), уставки мощности входа и выхода.

- **Код:** [`ui_interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L430), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L456); tgui: [`Smes.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Smes.tsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/power/smes.rs` (`SmesWindow`, `smes_acts`), `crates/ss13_tgui/src/interfaces/smes.rs` (`Smes`): окно открывает пустая рука по SMES с терминалом; ввод авто/выкл, вывод вкл/выкл, уставки `min`/`max`/±10 кВт/значение в порядке tg, ограничены `0..=*_level_max` (`200 кВт × уровни конденсаторов`, `RefreshParts`); заряд, доступный ввод и текущий вывод из тика машин. `// shortcut:` ползунки — `NumberInput` в ваттах (в ss13_ui_kit нет `Slider`)

### Вид и монтаж SMES
Индикатор заряда и режимов; отвёртка — панель, лом — разборка, кабель — установка терминала, кусачки — снятие, ЭМИ сбивает уставки. BandaStation меняет иконку SMES.

- **Код:** [`update_overlays()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L140), [`item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L245), [`crowbar_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L305), [`emp_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L519)
- **BandaStation:** [`modular_bandastation/aesthetics/smes/code/smes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/smes/code/smes.dm#L1) — иконка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/power/smes.rs` (`smes_looks`, `item_interaction`, `complete`, `setup`, `on_deconstruction`), `crates/ss13_game/src/machine_frame.rs` (семейство `Smes`), `crates/ss13_game/src/power/grid.rs` (`remove_node`): `smes`/`smes-o`, с закрытой панелью и терминалом — `smes-op`, `smes-oc`, `smes-og1..5`, эмиссив; отвёртка — панель; 10 кабеля с кардинального тайла с поднятым полом (2 с) — терминал на тайле игрока лицом к SMES, 50% удара током из сети вместо этого; кусачки по SMES или терминалу (5 с, пол поднят) — 10 кабеля, SMES сломан до нового терминала; лом только без терминала разбирает, заряд уходит в батареи; из платы — ёмкость по батареям, их заряд, первый свободный терминал рядом; ЭМИ — `crates/ss13_game/src/emp/power.rs` (по `*_level_max`). Проверено: `crates/ss13_game/tests/it/power_machines.rs`. Нет: поворота ключом (`default_change_direction_wrench`), выбора слоя кабеля ПКМ и мультитулом, расщепления кабелей, уже связанных с тайлом SMES, при новом терминале

### Варианты SMES
Супер-SMES, корабельный, инженерный, магический (бесконечный).

- **Код:** [`/obj/machinery/power/smes/super`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L534), [`/obj/machinery/power/smes/engineering`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L548), [`/obj/machinery/power/smes/magical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L552)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/power.rs`: ёмкость и уставки из типа; магический не бесконечен

### Портативный SMES: коннектор и банк
Банк энергии на колёсах (`smesbank`) подключается к коннектору в сети.

- **Код:** [`/obj/machinery/power/smes/connector`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes_portable.dm#L3), [`/obj/machinery/smesbank`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes_portable.dm#L109)
- **Статус в ss13-rs:** ❌ нет

### Консоль мониторинга сети (power monitor)
Показывает мощность и нагрузку подключённой сети, графики и список APC с их каналами. Программа ПДА — см. ../comms/.

- **Код:** [`/obj/machinery/computer/monitor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/monitor.dm#L3), [`proc/record()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/monitor.dm#L62); tgui: [`PowerMonitor.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PowerMonitor.tsx)
- **Статус в ss13-rs:** ❌ нет

### Аккумулятор энергии (energy_accumulator)
База тесла-катушек и заземлителей: накапливает принятую энергию разряда и отдаёт её в сеть.

- **Код:** [`/obj/machinery/power/energy_accumulator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/energy_accumulator.dm#L18)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/power/accumulator.rs` (`Accumulator::energy_output`, `process`): отдача `dE/dt = -0.04 E - 2000 Вт` каждый тик машин
