# Трубы и трубные сети
Атмосферные трубы, их сети (`/datum/pipeline`), прокладка, монтаж и сборка из фитингов.

### Трубная сеть (/datum/pipeline)
Все соединённые трубы образуют одну сеть с общим газом; машины на концах обмениваются газом с сетью, сеть пересобирается при монтаже и демонтаже.

- **Код:** [`/datum/pipeline`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/datum_pipeline.dm#L1), [`proc/build_pipeline()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/datum_pipeline.dm#L61), [`proc/reconcile_air()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/datum_pipeline.dm#L239), [`SSair.process_pipenets()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/air.dm#L272)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_atmos/src/pipenet/mod.rs`: узлы, сборка сетей (`build_pipeline`), `reconcile_air` с кастомной реконсиляцией (клапаны, коннекторы), реакции в трубах, перестройка при добавлении/удалении с сохранением газа; `process_pipenets` в `AtmosSystems::Pipenets`

### Базовая атмос-машина: узлы, слои, монтаж ключом
Общий тип для труб и устройств: соединения по направлениям на одном из 5 слоёв, прятание под пол, демонтаж гаечным ключом (под давлением — выброс газа и отбрасывание).

- **Код:** [`/obj/machinery/atmospherics`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/atmosmachinery.dm#L13), [`proc/atmos_init()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/atmosmachinery.dm#L289), [`wrench_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/atmosmachinery.dm#L425), [`proc/unsafe_pressure_release()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/atmosmachinery.dm#L492), [`proc/setup_hiding()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/atmosmachinery.dm#L133)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_atmos/src/pipenet/mod.rs` (`connection_check`, слои, цвета, `PIPING_*` флаги), `crates/ss13_game/src/atmos_machinery/mod.rs` (`set_init_directions`, `get_node_connects` по семействам); нет монтажа/демонтажа ключом (`wrench_act`, фитинги) и выброса давления в лицо

### Обычные трубы (simple, manifold, manifold4w)
Прямые/угловые трубы, тройники и крестовины разных цветов (воздух, скрубберы, общие) на всех слоях; протекают и лопаются при превышении давления.

- **Код:** [`/obj/machinery/atmospherics/pipe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/pipes/pipes.dm#L1), [`/obj/machinery/atmospherics/pipe/smart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/pipes/smart.dm#L3), [`smart/manifold4w`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/pipes/smart.dm#L92)
- **Статус в ss13-rs:** ✅ есть — умные трубы: объём по соединениям, иконка `!pipes_bitmask.dmi` по узлам (`crates/ss13_game/src/atmos_machinery/looks.rs`); прячутся под плиткой (`undertile`); нет заглушек (`cap_overlay`)

### Слоевой коллектор, мост, цветовой адаптер, межэтажная труба
Специальные трубы: соединение всех слоёв (`layer_manifold`), перекрещивание (`bridge_pipe`), соединение труб разных цветов (`color_adapter`), переход между уровнями (`multiz`).

- **Код:** [`layer_manifold`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/pipes/layermanifold.dm#L1), [`bridge_pipe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/pipes/bridge_pipe.dm#L1), [`color_adapter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/pipes/color_adapter.dm#L1), [`pipe/multiz`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/pipes/multiz.dm#L2)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/pipenet/mod.rs`: слоевой коллектор (все слои, 200 л), мост и цветовой адаптер как бинарные трубы; межэтажные трубы — вместе с многоуровневостью карт → ../mobs/movement.md «Перемещение по z-уровням» (уровни не стыкуются)

### Теплообменные трубы (HE pipes)
Трубы, которые обмениваются теплом с окружением (космосом или тайлом) и светятся при нагреве; применяются для охлаждения газа в космосе. Переходник `junction` соединяет их с обычными.

- **Код:** [`/obj/machinery/atmospherics/pipe/heat_exchanging`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/pipes/heat_exchange/he_pipes.dm#L1), [`heat_exchanging/simple`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/pipes/heat_exchange/simple.dm#L1), [`heat_exchanging/junction`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/pipes/heat_exchange/junction.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/pipenet/mod.rs` (`HeatExchange`, `temperature_interact`), `crates/ss13_game/src/atmos_machinery/devices.rs` (`HePipe`, `he_step`, `HeOnLava`), `crates/ss13_atmos/src/level/conduct.rs` (`solid_heat`, `take_solid_temperature`): соединяются только между собой и через junction, изгибы по диагональному `dir`, теплообмен с космосом (2,7 K, `HEAT_CAPACITY_VACUUM`) и воздухом тайла каждый fire, под стеной — с самой стеной (её температура и теплоёмкость, `TakeTemperature` и `temperature_expose`), на лаве порог сравнивается с 5000 K (флаг ставится при загрузке карты), свечение выше 500 K по такту машин (`HeGlow`, `glow_he_pipes`, без 2-секундного перехода цвета); нагрев и ожоги пристёгнутых к трубе мобов → [../engineering/structures-furniture.md](../engineering/structures-furniture.md) «Стулья, диваны, скамьи» (пристёгивания стоя нет)

### Отображение газа в трубах
Прозрачные участки и устройства показывают цвет смеси внутри сети.

- **Код:** [`proc/UpdateGasVisuals()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/datum_pipeline.dm#L319), [`/obj/effect/abstract/gas_visual`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/datum_pipeline.dm#L353)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/atmos_machinery/gas_visuals.rs`: цвет сети по `CalculateGasmixColor` (смешение `primary_color` газов в HSV, затемнение неполной трубы), оверлей `!pipe_gas_overlays.dmi` на умных трубах (без HE, мостов и адаптеров, как `has_gas_visuals`); цвет меняется сразу, без анимации, с шагом 4 на канал (shortcut)

### Фитинги и сборка труб
Предмет `/obj/item/pipe` (и счётчик `pipe_meter`), который гаечным ключом превращается в трубу или устройство на нужном слое и направлении.

- **Код:** [`/obj/item/pipe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/pipe/construction.dm#L10), [`wrench_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/pipe/construction.dm#L268), [`proc/build_pipe()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/pipe/construction.dm#L408), [`/obj/item/pipe_meter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/pipe/construction.dm#L460)
- **Статус в ss13-rs:** ❌ нет

### Раздатчик труб (pipe dispenser)
Машина, выдающая фитинги труб и устройств (вариант для утилизации — трубы мусоропровода и транзитные трубы); RPD — см. ../engineering/.

- **Код:** [`/obj/machinery/pipedispenser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/pipe/pipe_dispenser.dm#L5), [`pipedispenser/disposal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/pipe/pipe_dispenser.dm#L177); tgui: [`PipeDispenser.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PipeDispenser.tsx)
- **Статус в ss13-rs:** ❌ нет

### Газовый счётчик (meter)
Прибор на трубе, показывающий давление и температуру сети (и пригодный для консолей и интегральных схем).

- **Код:** [`/obj/machinery/meter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/other/meter.dm#L1), [`/obj/machinery/meter/turf`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/other/meter.dm#L237), [`circuit_component/atmos_meter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/other/meter.dm#L197)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/atmos_machinery/meter.rs`: привязка к трубе своего тайла и слоя, сдвиг по слою, шкала давления и цвет по температуре (GAGS `meter` из слоёв `meter.dmi`), светящиеся кнопки, показания при осмотре и клике; снятие ключом → «Фитинги и сборка труб», USB-порт → ../science/wiremod.md
