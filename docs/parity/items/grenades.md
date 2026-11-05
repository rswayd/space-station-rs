# Гранаты и взрывные устройства
Семейство `/obj/item/grenade`, химическая граната, C4 и TTV-бомба.

### Механика гранат
Граната взводится в руке (таймер, у части регулируется отвёрткой), затем бросается и срабатывает; неуклюжие могут взорвать её в руках; срабатывание пишется в журнал админов. Гранаты взводятся и сигналом сборки.

- **Код:** [`/obj/item/grenade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/_grenade.dm#L4), [`arm_grenade()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/_grenade.dm#L154), [`detonate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/_grenade.dm#L177), [`botch_check()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/_grenade.dm#L108)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/grenades.rs`: взвод в руке (`attack_self` -> `arm`: звук, `_active`-состояние иконки, `Armed` с моментом срабатывания по `det_time`, 5 с), срабатывание там, где граната оказалась (в руке, в полёте после броска `throw.rs`, в сумке), отвёртка циклически меняет `possible_fuse_time`, мультитул спрашивает таймер (tgui-список, проверка досягаемости при ответе), взведённую не перенастроить; `GRENADE_USED` — повторно не срабатывает; уничтоженная взрывом граната срабатывает (`atom_deconstruct`); осмотр показывает таймер. Взвод пишется в серверный лог (`info!`), не админам. Нет: неуклюжих (в порте нет клоунской неуклюжести), липкой ленты, срабатывания от попавшего снаряда (`hit_reaction`), броска взведённой гранаты дальним кликом. Взвод сигналом сборки — в [../science/assemblies.md](../science/assemblies.md)

### Светошумовые гранаты
Флешбанг ослепляет и оглушает всех в радиусе (наушники и защита глаз помогают), стингбанг дополнительно разлетается шрапнелью, праймеры.

- **Код:** [`/obj/item/grenade/flashbang`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/flashbang.dm#L1), [`bang()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/flashbang.dm#L51), [`/obj/item/grenade/stingbang`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/flashbang.dm#L92)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/grenades.rs` (`flashbang`, `bang`): флешбанг (и кластерный) роняет гранату из рук, звук, затем каждому живому мобу в поле зрения/слуха в радиусе 7: «BANG», `flash_act` (в «сладкой точке» 3 клетки и киборгам — паралич и нокдаун по расстоянию, роняет вещи), `soundbang_act` по расстоянию (вплотную — `SOUNDBANG_OVERWHELMING`, 1 клетка — сильный с оглушением, до 3 — обычный, дальше — шаткость и выпавшие вещи), без давления на тайле нет «бэнга» дальше эпицентра, глухие и в наушниках защищены. Нет: головокружения (нет статуса), света и искр при взводе, стингбанга и праймеров (нет облака шрапнели-стингболов)

### Химическая граната (корпус)
Пустой корпус: в него вставляют до двух мензурок (большая граната — больше) и сборку-детонатор (таймер, сигнальщик, датчик движения); при срабатывании реагенты смешиваются и реагируют (дым, пена, взрыв). Готовые: металлическая пена, зажигательная, очищающая, противосорняковая.

- **Код:** [`/obj/item/grenade/chem_grenade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/chem_grenade.dm#L1), [`item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/chem_grenade.dm#L187), [`stage_change()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/chem_grenade.dm#L230), [`/obj/item/grenade/chem_grenade/large`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/chem_grenade.dm#L290), [`/obj/item/grenade/chem_grenade/metalfoam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/chem_grenade.dm#L426)
- **Статус в ss13-rs:** ❌ нет — реакции в контейнерах есть (`crates/ss13_reagents`), но корпуса гранат и сборок нет

### Дымовые, ЭМИ и антигравитационные гранаты
Дымовая граната закрывает обзор, ЭМИ-граната выводит из строя электронику, антигравитационная отключает гравитацию в радиусе, гипнотическая, газовые кристаллы (хилиум, N2O, пена).

- **Код:** [`/obj/item/grenade/smokebomb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/smokebomb.dm#L6), [`/obj/item/grenade/empgrenade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/emgrenade.dm#L1), [`/obj/item/grenade/antigravity`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/antigravity.dm#L1), [`/obj/item/grenade/hypnotic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/hypno.dm#L1), [`/obj/item/grenade/gas_crystal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/atmos_grenades.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/grenades.rs`: дымовая граната (звук, облако «плохого» дыма на `DIAMOND_AREA(4)` = 41 клетку по атмос-смежности, 16 с, непрозрачное; моб в дыму без баллона раз в секунду роняет вещи, получает 1 удушья и кашляет; урона блобам нет — нет блобов), ЭМИ-граната (`empulse(4, 10)` через `crate::emp::Empulse`). Нет: антигравитационной, гипнотической, газовых кристаллов

### Боевые и синдикатовские гранаты
Мини-бомба синдиката, контузионная, осколочная, глюонная (замораживает), кластерные бомбы (разлетаются на сегменты), гранаты-спавнеры (манхаки, карпы, мыло, клоуны, коты), самодельная СВУ из банки.

- **Код:** [`/obj/item/grenade/syndieminibomb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/syndieminibomb.dm#L1), [`/obj/item/grenade/frag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/syndieminibomb.dm#L29), [`/obj/item/grenade/clusterbuster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/clusterbuster.dm#L8), [`/obj/item/grenade/spawnergrenade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/spawnergrenade.dm#L1), [`/obj/item/grenade/iedcasing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/ghettobomb.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/grenades.rs`: мини-бомба (`1, 2, 4`, огонь 2), HE-граната (`1, 2, 3`, огонь 3), осколочная (`0, 1, 3`, огонь 4) и мега-осколочная со шрапнелью — по снаряду `/obj/projectile/bullet/shrapnel` (и `/mega`) в каждую клетку кольца радиуса 4/12 через `crate::projectile::fire`; кластербанг и его варианты с портированной нагрузкой (флешбанг, ЭМИ, дым, мини-бомба): 4–8 взведённых на 1,5–6 с гранат, каждая с шансом 35% становится сегментом, который выбрасывает ещё 4–8; они разлетаются на 1–4 случайных шага. Нет: «мучеников» облака шрапнели и общих сообщений о попаданиях, глюонной гранаты, спавнеров, СВУ, кластеров с химической/спавнерной/слаймовой нагрузкой

### Пластиковая взрывчатка (C4, X4)
Заряд крепится к стене, двери или человеку, взрывается по таймеру; X4 направленный.

- **Код:** [`/obj/item/grenade/c4`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/plastic.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/explosions/sources.rs` (`c4_attack_self`, `c4_interact`, `complete`, `detonate_charges`): таймер через `tgui_input_number` (от 10 с), установка на соседний атом за 3 с (оверлей на цели), по таймеру цель (не моб) получает heavy, затем взрыв `boom_sizes` (C-4 `0,0,3`, X-4 `0,5,7`), X-4 направленный (120°) на плотной цели; уничтоженный взрывом неустановленный заряд взрывается на месте (`atom_deconstruct`, `c4_destroyed`); нет проводов и сборок (`receive_signal`), суицида, уведомления призракам

### Петарды
Праздничные петарды и хлопушки.

- **Код:** [`/obj/item/grenade/firecracker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/grenades/festive.dm#L62)
- **Статус в ss13-rs:** ❌ нет

### Клапан переноса (TTV)
Два баллона соединяются клапаном; открытие (вручную или сборкой) смешивает газы — классическая плазменная бомба. tgui-окно клапана.

- **Код:** [`/obj/item/transfer_valve`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/transfer_valve.dm#L1), [`toggle_valve()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/transfer_valve.dm#L321); tgui: [`TransferValve.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/TransferValve.jsx)
- **BandaStation:** [`transfer_valve.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/transfer_valve.dm#L178) — запись сборки TTV-бомбы (давление и температура баллонов) в лог бомберов
- **Статус в ss13-rs:** ❌ нет — баллоны взрываются от давления (`crates/ss13_game/src/explosions/sources.rs`), но клапана, смешивающего два баллона, нет
