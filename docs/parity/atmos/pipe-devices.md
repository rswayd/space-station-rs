# Трубные устройства (насосы, клапаны, фильтры)
Бинарные (вход-выход) и тринарные (три порта) устройства, которые управляют газом внутри трубных сетей.

### Насос давления (pump)
Перекачивает газ из входа в выход до целевого давления на выходе (до 4500 кПа); включение и настройка через окно, интегральные схемы, вариант с подсветкой «on».

- **Код:** [`/obj/machinery/atmospherics/components/binary/pump`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/binary_devices/pump.dm#L13), [`circuit_component/atmos_pump`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/binary_devices/pump.dm#L134); tgui: [`AtmosPump.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AtmosPump.tsx)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/atmos_machinery/devices.rs` (`pump_gas_to` с `output_pipenet_air`), окно `AtmosPump`, ctrl/alt-клик; нет USB-схем

### Объёмный насос (volume_pump)
Перекачивает заданный объём (до 200 л/с) независимо от давления; при перегрузке может «надорваться» (overclock через мультитул).

- **Код:** [`/obj/machinery/atmospherics/components/binary/volume_pump`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/binary_devices/volume_pump.dm#L13), [`circuit_component/atmos_volume_pump`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/binary_devices/volume_pump.dm#L195); tgui: [`AtmosPump.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AtmosPump.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/atmos_machinery/devices.rs`, окно `AtmosPump`; нет разгона мультитулом (утечка, снятие лимитов)

### Пассивный клапан (passive_gate)
Пропускает газ, пока давление на выходе ниже заданного, без питания.

- **Код:** [`/obj/machinery/atmospherics/components/binary/passive_gate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/binary_devices/passive_gate.dm#L10)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/atmos_machinery/devices.rs` (`release_gas_to`), без питания, окно `AtmosPump`

### Предохранительный клапан (pressure_valve)
Открывается, когда давление на входе превышает порог.

- **Код:** [`/obj/machinery/atmospherics/components/binary/pressure_valve`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/binary_devices/pressure_valve.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Ручной и цифровой вентиль (valve)
Соединяет или разъединяет две сети: ручной — кликом, цифровой — удалённо (консоли, схемы, сигналы).

- **Код:** [`/obj/machinery/atmospherics/components/binary/valve`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/binary_devices/valve.dm#L7), [`circuit_component/digital_valve`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/binary_devices/valve.dm#L94)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/atmos_machinery/devices.rs`, `crates/ss13_atmos/src/pipenet/mod.rs` (`set_valve_open`, объединение сетей при открытии): поворот рукой за 1 с; цифровой вентиль тоже только рукой (нет ИИ)

### Температурный клапан и тепловой насос (temperature_gate, temperature_pump)
Клапан пропускает газ по порогу температуры; тепловой насос переносит тепло между двумя сетями без переноса газа.

- **Код:** [`/obj/machinery/atmospherics/components/binary/temperature_gate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/binary_devices/temperature_gate.dm#L1), [`/obj/machinery/atmospherics/components/binary/temperature_pump`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/binary_devices/temperature_pump.dm#L1); tgui: [`AtmosTempGate.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AtmosTempGate.tsx), [`AtmosTempPump.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AtmosTempPump.tsx)
- **Статус в ss13-rs:** ❌ нет

### Двухпортовая вентиляция (dp_vent_pump)
Вентиляция с входом и выходом: выпускает в тайл из одной сети или вытягивает в другую; используется в шлюзах инсинератора и токсинов.

- **Код:** [`/obj/machinery/atmospherics/components/binary/dp_vent_pump`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/binary_devices/dp_vent_pump.dm#L5), [`dp_vent_pump/high_volume`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/binary_devices/dp_vent_pump.dm#L102)
- **Статус в ss13-rs:** ❌ нет

### Циркулятор ТЭГ (circulator)
Горячий и холодный контуры термоэлектрического генератора; передают газ и отдают тепло генератору (см. power/generators.md).

- **Код:** [`/obj/machinery/atmospherics/components/binary/circulator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/binary_devices/circulator.dm#L4)
- **Статус в ss13-rs:** ❌ нет

### Газовый фильтр (filter)
Отделяет выбранные газы во второй выход, остальное пропускает дальше; предустановленные фильтры по газам для атмос-отсека.

- **Код:** [`/obj/machinery/atmospherics/components/trinary/filter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/trinary_devices/filter.dm#L1), [`/obj/machinery/atmospherics/components/trinary`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/trinary_devices/trinary_devices.dm#L1); tgui: [`AtmosFilter.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AtmosFilter.tsx)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/atmos_machinery/devices.rs`, окно `AtmosFilter`, карточные подтипы (`filter/atmos/*`, `filter_type` с карты)

### Газовый смеситель (mixer)
Смешивает две входные сети в заданной пропорции до целевого давления (так делается воздушная смесь O2/N2).

- **Код:** [`/obj/machinery/atmospherics/components/trinary/mixer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/trinary_devices/mixer.dm#L1); tgui: [`AtmosMixer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AtmosMixer.tsx)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/atmos_machinery/devices.rs`, окно `AtmosMixer`, `airmix`-подтипы
