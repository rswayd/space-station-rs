# Гранаты и взрывные устройства
Семейство `/obj/item/grenade`, химическая граната, C4 и TTV-бомба.

### Механика гранат
Граната взводится в руке (таймер, у части регулируется отвёрткой), затем бросается и срабатывает; неуклюжие могут взорвать её в руках; срабатывание пишется в журнал админов. Гранаты взводятся и сигналом сборки.

- **Код:** [`/obj/item/grenade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/_grenade.dm#L4), [`arm_grenade()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/_grenade.dm#L154), [`detonate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/_grenade.dm#L177), [`botch_check()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/_grenade.dm#L108)
- **Статус в ss13-rs:** ❌ нет — нет гранат и взрывов (бросок предметов есть: `crates/ss13_game/src/throw.rs`)

### Светошумовые гранаты
Флешбанг ослепляет и оглушает всех в радиусе (наушники и защита глаз помогают), стингбанг дополнительно разлетается шрапнелью, праймеры.

- **Код:** [`/obj/item/grenade/flashbang`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/flashbang.dm#L1), [`bang()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/flashbang.dm#L51), [`/obj/item/grenade/stingbang`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/flashbang.dm#L92)
- **Статус в ss13-rs:** ❌ нет — эффект вспышки есть только у ручной вспышки (`crates/ss13_game/src/security.rs`)

### Химическая граната (корпус)
Пустой корпус: в него вставляют до двух мензурок (большая граната — больше) и сборку-детонатор (таймер, сигнальщик, датчик движения); при срабатывании реагенты смешиваются и реагируют (дым, пена, взрыв). Готовые: металлическая пена, зажигательная, очищающая, противосорняковая.

- **Код:** [`/obj/item/grenade/chem_grenade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/chem_grenade.dm#L1), [`item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/chem_grenade.dm#L187), [`stage_change()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/chem_grenade.dm#L230), [`/obj/item/grenade/chem_grenade/large`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/chem_grenade.dm#L290), [`/obj/item/grenade/chem_grenade/metalfoam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/chem_grenade.dm#L426)
- **Статус в ss13-rs:** ❌ нет — реакции в контейнерах есть (`crates/ss13_reagents`), но корпуса гранат и сборок нет

### Дымовые, ЭМИ и антигравитационные гранаты
Дымовая граната закрывает обзор, ЭМИ-граната выводит из строя электронику, антигравитационная отключает гравитацию в радиусе, гипнотическая, газовые кристаллы (хилиум, N2O, пена).

- **Код:** [`/obj/item/grenade/smokebomb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/smokebomb.dm#L6), [`/obj/item/grenade/empgrenade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/emgrenade.dm#L1), [`/obj/item/grenade/antigravity`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/antigravity.dm#L1), [`/obj/item/grenade/hypnotic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/hypno.dm#L1), [`/obj/item/grenade/gas_crystal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/atmos_grenades.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Боевые и синдикатовские гранаты
Мини-бомба синдиката, контузионная, осколочная, глюонная (замораживает), кластерные бомбы (разлетаются на сегменты), гранаты-спавнеры (манхаки, карпы, мыло, клоуны, коты), самодельная СВУ из банки.

- **Код:** [`/obj/item/grenade/syndieminibomb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/syndieminibomb.dm#L1), [`/obj/item/grenade/frag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/syndieminibomb.dm#L29), [`/obj/item/grenade/clusterbuster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/clusterbuster.dm#L8), [`/obj/item/grenade/spawnergrenade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/spawnergrenade.dm#L1), [`/obj/item/grenade/iedcasing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/ghettobomb.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Пластиковая взрывчатка (C4, X4)
Заряд крепится к стене, двери или человеку, взрывается по таймеру; X4 направленный.

- **Код:** [`/obj/item/grenade/c4`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/plastic.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Петарды
Праздничные петарды и хлопушки.

- **Код:** [`/obj/item/grenade/firecracker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/festive.dm#L62)
- **Статус в ss13-rs:** ❌ нет

### Клапан переноса (TTV)
Два баллона соединяются клапаном; открытие (вручную или сборкой) смешивает газы — классическая плазменная бомба. tgui-окно клапана.

- **Код:** [`/obj/item/transfer_valve`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/transfer_valve.dm#L1), [`toggle_valve()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/transfer_valve.dm#L321); tgui: [`TransferValve.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/TransferValve.jsx)
- **BandaStation:** [`transfer_valve.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/transfer_valve.dm#L178) — запись сборки TTV-бомбы (давление и температура баллонов) в лог бомберов
- **Статус в ss13-rs:** ❌ нет — газ в баллонах реагирует (`crates/ss13_game/src/fire.rs`), но клапана и взрывов нет
