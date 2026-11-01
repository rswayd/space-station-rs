# Накопители: SMES, мониторинг, аккумуляторы энергии
Большие накопители энергии станции и консоли контроля сети.

### SMES: зарядка и выдача
Накопитель берёт энергию из сети терминала (`input_level`) и выдаёт в выходную сеть (`output_level`), неиспользованную выдачу забирает обратно; ёмкость — сумма батарей платы.

- **Код:** [`/obj/machinery/power/smes`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L4), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L351), [`proc/restore()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L409), [`proc/adjust_charge()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L177)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/power/model.rs` (`Smes::process_input`, `process_output`, `restore`)

### Окно SMES
tgui-окно: заряд, режимы ввода и вывода (вкл/авто), уставки мощности входа и выхода.

- **Код:** [`ui_interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L430), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L456); tgui: [`Smes.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Smes.tsx)
- **Статус в ss13-rs:** ❌ нет — уставки берутся из карты, менять их нельзя

### Вид и монтаж SMES
Индикатор заряда и режимов; отвёртка — панель, лом — разборка, кабель — установка терминала, кусачки — снятие, ЭМИ сбивает уставки. BandaStation меняет иконку SMES.

- **Код:** [`update_overlays()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L140), [`item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L245), [`crowbar_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L305), [`emp_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/smes.dm#L519)
- **BandaStation:** [`modular_bandastation/aesthetics/smes/code/smes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/smes/code/smes.dm#L1) — иконка
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/power/mod.rs` не обновляет вид SMES по заряду; монтажа нет

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
