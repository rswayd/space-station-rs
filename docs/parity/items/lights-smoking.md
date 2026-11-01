# Фонари, зажигалки и курение
Переносные источники света и огня, сигареты, сигары, трубки и вейпы.

### Фонарики и лампы
Фонарик включается в руке (`attack_self`) и светит вокруг носителя; сек-фонарь крепится к шлему и оружию; настольные лампы стоят включёнными; ЭМИ-фонарь — шпионский. Ручкой-фонариком (и любым фонарём) осматривают зрачки и рот.

- **Код:** [`/obj/item/flashlight`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/flashlight.dm#L6), [`toggle_light()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/flashlight.dm#L96), [`eye_examine()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/flashlight.dm#L129), [`/obj/item/flashlight/pen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/flashlight.dm#L336), [`/obj/item/flashlight/seclite`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/flashlight.dm#L393), [`/obj/item/flashlight/lamp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/flashlight.dm#L411), [`/obj/item/flashlight/emp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/flashlight.dm#L818)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/light.rs`: свет фонаря и ламп по `start_on` при загрузке карты; нет включения/выключения, осмотра глаз, крепления к шлему

### Сигнальные огни, свечи, факелы, светящиеся палочки
Одноразовые источники: фальшфейер горит ограниченное время и обжигает, свечи и факелы зажигаются от огня, светящиеся палочки ломаются для включения и гаснут со временем; фонари-лантерны, слайм-светильник.

- **Код:** [`/obj/item/flashlight/flare`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/flashlight.dm#L443), [`/obj/item/flashlight/flare/candle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/flashlight.dm#L590), [`/obj/item/flashlight/flare/torch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/flashlight.dm#L730), [`/obj/item/flashlight/glowstick`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/flashlight.dm#L875), [`/obj/item/flashlight/lantern`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/flashlight.dm#L765)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/light.rs`: только начальный свет зажжённых на карте; нет горения, расхода топлива и зажигания

### Зажигалки и спички
Зажигалка (Zippo и дешёвые цветные) открывается и зажигается, поджигает сигареты, свечи и людей; спички зажигаются о коробок, гаснут со временем.

- **Код:** [`/obj/item/lighter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/lighter.dm#L1), [`set_lit()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/lighter.dm#L121), [`/obj/item/match`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cigarettes.dm#L16), [`matchignite()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cigarettes.dm#L96)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/fire.rs`: «no lighters, matches, cigarettes or igniters as flame sources»

### Сигареты и окурки
Сигарета поджигается от огня, тлеет, выпуская свои реагенты (никотин и др.) в курильщика, оставляет окурок; марки отличаются составом (синдикатовские лечат, «flash powder» взрываются); самокрутки из бумаги и сушёных растений.

- **Код:** [`/obj/item/cigarette`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cigarettes.dm#L193), [`light()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cigarettes.dm#L418), [`handle_reagents()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cigarettes.dm#L544), [`/obj/item/cigbutt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cigarettes.dm#L917), [`/obj/item/rollingpaper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cigarettes.dm#L1040)
- **Статус в ss13-rs:** ❌ нет — сигареты существуют только как предметы (влезают в хранилище костюма)

### Сигары и трубки
Сигары курятся дольше; трубки набиваются сушёными растениями и курят их реагенты.

- **Код:** [`/obj/item/cigarette/cigar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cigarettes.dm#L875), [`/obj/item/cigarette/pipe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cigarettes.dm#L942)
- **Статус в ss13-rs:** ❌ нет

### Вейпы
Электронная сигарета: заправляется реагентами, отвёрткой включается мощный режим (облака дыма, взлом емагом — взрыв).

- **Код:** [`/obj/item/vape`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cigarettes.dm#L1055), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cigarettes.dm#L1204)
- **Статус в ss13-rs:** ❌ нет

Пачки сигарет — [storage.md](storage.md), кальяны форка — см. ../kitchen/.
