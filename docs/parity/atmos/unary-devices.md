# Однопортовые устройства, термомашины и газовые хранилища
Устройства с одним портом трубы: вентиляции и скрубберы комнат, инжекторы, термомашины, криокапсулы, стационарные газовые баки, газовые майнеры.

### Вентиляция (vent_pump)
Выпускает воздух из распределительной сети в комнату до внешнего целевого давления (или вытягивает в режиме siphon); управляется воздушной тревогой; сварка заваривает решётку.

- **Код:** [`/obj/machinery/atmospherics/components/unary/vent_pump`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/unary_devices/vent_pump.dm#L3), [`process_atmos()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/unary_devices/vent_pump.dm#L242), [`welder_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/unary_devices/vent_pump.dm#L318)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/atmos_machinery/devices.rs` (внешняя/внутренняя граница, выдув и откачка, high volume 1000 л), управление тревогой, заваривание сваркой, спрайты состояний; нет разгона (overclock) и повреждения вентилятора, нет ползания по трубам

### Скруббер (vent_scrubber)
Вытягивает из воздуха комнаты выбранные газы (CO2 и др.) или всё подряд (siphon), расширенный режим захватывает соседние тайлы.

- **Код:** [`/obj/machinery/atmospherics/components/unary/vent_scrubber`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/unary_devices/vent_scrubber.dm#L1), [`proc/scrub()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/unary_devices/vent_scrubber.dm#L221)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/atmos_machinery/devices.rs` (фильтры, siphon, widenet по `get_atmos_adjacent_turfs(alldir)`), заваривание; нет мощности по числу фильтров

### Инжектор и пассивная вентиляция (outlet_injector, passive_vent)
Инжектор выпускает газ в тайл заданным объёмом (камеры сжигания, смесительные камеры); пассивная вентиляция выравнивает давление тайла и сети без питания.

- **Код:** [`/obj/machinery/atmospherics/components/unary/outlet_injector`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/unary_devices/outlet_injector.dm#L1), [`/obj/machinery/atmospherics/components/unary/passive_vent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/unary_devices/passive_vent.dm#L1)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/atmos_machinery/devices.rs`, окно инжектора `AtmosPump`; нет датчиков и консолей камер

### Теплообменник и коннектор портативных машин
Теплообменник уравнивает температуру двух сетей через пару; коннектор присоединяет канистру или портативный насос к трубе.

- **Код:** [`/obj/machinery/atmospherics/components/unary/heat_exchanger`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/unary_devices/heat_exchanger.dm#L1), [`/obj/machinery/atmospherics/components/unary/portables_connector`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/unary_devices/portables_connector.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — коннектор: `crates/ss13_game/src/atmos_machinery/portable.rs`, `crates/ss13_atmos/src/pipenet/mod.rs` (`attach`/`detach`); теплообменника нет

### Насос шлюза (airlock_pump)
Циклирующий шлюз: откачивает и закачивает воздух между двумя дверями перед открытием внешней или внутренней.

- **Код:** [`/obj/machinery/atmospherics/components/unary/airlock_pump`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/unary_devices/airlock_pump.dm#L21)
- **Статус в ss13-rs:** ❌ нет

### Термомашина: нагреватель и охладитель (thermomachine)
Нагревает или охлаждает газ своей сети до целевой температуры, расходуя энергию; переключение режима ключом/мультитулом, мощность от деталей.

- **Код:** [`/obj/machinery/atmospherics/components/unary/thermomachine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/unary_devices/thermomachine.dm#L3); tgui: [`ThermoMachine.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ThermoMachine.jsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/atmos_machinery/devices.rs` (теплообмен с портом, tier 1), окно `ThermoMachine`, подтипы freezer/heater/coldroom; нет деталей и апгрейдов, поворота, смены слоя, расхода энергии на тепло, GAGS-иконки

_«Криокапсула (cryo cell)» описано в [../medical/medical-machines.md](../medical/medical-machines.md)._

### Стационарные газовые баки (atmospherics tank)
Многотайловые резервуары газа на трубной сети (воздух, кислород, плазма и все прочие газы), со стеклянным окном, показывающим наполнение.

- **Код:** [`/obj/machinery/atmospherics/components/tank`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/tank.dm#L3), [`tank/air`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/tank.dm#L445)
- **Статус в ss13-rs:** ❌ нет — баки входят в граф как пустые унарные машины (`// shortcut:`)

### Газовые майнеры (miner)
Бесконечный источник одного газа в тайл до порога давления, расходует энергию; используются на картах вместо канистр снабжения атмос-отсека.

- **Код:** [`/obj/machinery/atmospherics/miner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/other/miner.dm#L8), [`miner/oxygen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/other/miner.dm#L158)
- **Статус в ss13-rs:** ❌ нет
