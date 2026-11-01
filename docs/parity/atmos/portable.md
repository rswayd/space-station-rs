# Портативная атмосфера и газовые баллоны
Передвижные канистры, насосы и скрубберы, а также ручные газовые баллоны и джетпаки.

### Базовая портативная машина
Подключается к коннектору трубы гаечным ключом, принимает газовый баллон, получает урон от экстремальной температуры и давления, вставляется кристалл гипер-ноблия (подавление реакций).

- **Код:** [`/obj/machinery/portable_atmospherics`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/portable/portable_atmospherics.dm#L3), [`proc/connect()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/portable/portable_atmospherics.dm#L168), [`proc/replace_tank()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/portable/portable_atmospherics.dm#L236), [`proc/take_atmos_damage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/portable/portable_atmospherics.dm#L136)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/atmos_machinery/portable.rs`: газ, подключение ключом к порту, вставка/извлечение баллона, реакции; нет повреждений давлением/жаром и кристалла гипер-ноблия

### Газовая канистра (canister)
Ёмкость 1000 л с газом (воздух, O2, N2, плазма, N2O и все остальные газы); клапан выпуска с настраиваемым давлением в тайл или баллон, защитное поле, перекраска, разрушение при перегреве с выбросом газа, провода (см. ../engineering/).

- **Код:** [`/obj/machinery/portable_atmospherics/canister`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/portable/canister.dm#L6), [`proc/toggle_valve()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/portable/canister.dm#L706), [`proc/canister_break()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/portable/canister.dm#L494), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/portable/canister.dm#L630); tgui: [`Canister.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Canister.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/atmos_machinery/portable.rs`: `create_gas`, клапан и давление выпуска, баллон, индикатор давления, окно `Canister`; нет экрана/ячейки, перемаркировки, перекраски, разрушения

### Портативный насос (pump)
Закачивает газ из тайла в себя или выпускает в тайл до целевого давления; маленький вариант `lil_pump`.

- **Код:** [`/obj/machinery/portable_atmospherics/pump`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/portable/pump.dm#L1), [`pump/lil_pump`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/portable/pump.dm#L174); tgui: [`PortablePump.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PortablePump.tsx)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/atmos_machinery/portable.rs`, окно `PortablePump`

### Портативный скруббер и большой скруббер
Фильтрует выбранные газы из воздуха вокруг; стационарный большой скруббер работает на нескольких тайлах.

- **Код:** [`/obj/machinery/portable_atmospherics/scrubber`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/portable/scrubber.dm#L1), [`scrubber/huge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/portable/scrubber.dm#L201); tgui: [`PortableScrubber.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PortableScrubber.tsx)
- **Статус в ss13-rs:** 🟡 частично — портативный скруббер: `crates/ss13_game/src/atmos_machinery/portable.rs`, окно `PortableScrubber`; большого скруббера нет

### Трубный скруббер (pipe_scrubber)
Портативная машина, которая фильтрует газ прямо из трубной сети, к которой подключена.

- **Код:** [`/obj/machinery/portable_atmospherics/pipe_scrubber`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/portable/pipe_scrubber.dm#L1); tgui: [`PipeScrubber.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PipeScrubber.tsx)
- **Статус в ss13-rs:** ❌ нет

### Блюспейс-скруббер и газовый приёмник
Скруббер, отправляющий собранный газ через блюспейс в привязанный приёмник на трубной сети.

- **Код:** [`scrubber/bluespace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/portable/bluespace_scrubber.dm#L2), [`/obj/machinery/portable_atmospherics/gas_receiver`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/portable/bluespace_scrubber.dm#L73)
- **Статус в ss13-rs:** ❌ нет

_«Газовые баллоны (/obj/item/tank)» описано в [../items/tanks.md](../items/tanks.md)._

_«Джетпаки (/obj/item/tank/jetpack)» описано в [../items/tanks.md](../items/tanks.md)._
