# Электросеть: кабели, powernet, каналы зон
Основа энергосистемы: подсистема машин, кабельные сети (`/datum/powernet`), подключение машин и потребление энергии зонами.

### Подсистема SSmachines
Каждые 2 с обрабатывает машины и энергосистему в порядке tg: сброс сетей, ранняя обработка APC, четыре шага зарядки APC, поздняя обработка, затем SMES, генераторы и остальные машины.

- **Код:** [`SSmachines`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/machines.dm#L1), [`fire()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/machines.dm#L92)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/power/mod.rs` (`power_tick`, `Generation`, `MachineTicks`): порядок энергосистемы сохранён, ранняя фаза (эмиттеры), РИТЭГ, солнечные панели, консоли и аккумуляторы энергии; `process()` прочих машин (P.A.C.M.A.N., ТЭГ, турбина) нет

### Powernet (/datum/powernet)
Сеть соединённых кабелей с доступной мощностью (`avail`), нагрузкой (`load`) и отложенной нагрузкой; машины кладут и берут энергию из сети.

- **Код:** [`/datum/powernet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/powernet.dm#L5), [`proc/reset()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/powernet.dm#L81), [`proc/propagate_network()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/power.dm#L389), [`proc/merge_powernets()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/power.dm#L418)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/power/grid.rs` (`Grid`, `Powernet`, сброс и surplus по tg)

### Силовые кабели (/obj/structure/cable)
Кабели соединяются с соседними на общем слое (3 слоя), многослойный хаб соединяет слои; кусачки режут кабель (с ударом током под напряжением), мультитул показывает мощность сети.

- **Код:** [`/obj/structure/cable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/cable.dm#L17), [`proc/connect_cable()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/cable.dm#L83), [`wirecutter_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/cable.dm#L210), [`multitool_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/cable.dm#L266), [`/obj/structure/cable/multilayer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/cable.dm#L751)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/power/grid.rs`, `crates/ss13_mapload/src/cable.rs`: граф кабелей строится один раз при загрузке; резать, прокладывать кабель и смотреть мощность мультитулом нельзя

### Моток кабеля и прокладка (/obj/item/stack/cable_coil)
Стак кабеля: прокладка по тайлам и слоям, выбор цвета и слоя в меню, ремонт ожогов роботов и протезов, наручники из кабеля.

- **Код:** [`/obj/item/stack/cable_coil`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/cable.dm#L481), [`proc/place_turf()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/cable.dm#L688), [`attack_self()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/cable.dm#L572)
- **Статус в ss13-rs:** ❌ нет — моток есть только как предмет-стак (наручники из кабеля — см. ../security/)

### Межэтажный кабельный хаб
Соединяет кабельные сети соседних уровней (multi-z).

- **Код:** [`/obj/structure/cable/multilayer/multiz`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/multiz.dm#L1)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/power/mod.rs`: shortcut, multiz-хабы не связывают уровни

### Терминалы
Подключение SMES и APC к сети с обратной стороны: терминал стоит на кабеле и передаёт энергию «хозяину»; ставится мотком кабеля, снимается кусачками.

- **Код:** [`/obj/machinery/power/terminal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/terminal.dm#L6), [`apc/proc/make_terminal()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/apc/apc_power_proc.dm#L10)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/power/grid.rs`: связь терминала с SMES/APC при загрузке карты; монтажа и демонтажа нет

### Силовая машина (/obj/machinery/power) и подключение к сети
Машины с узлом на кабеле своего тайла (генераторы, SMES, эмиттеры): `add_avail`, `add_load`, `surplus`, переключение слоя кабеля мультитулом.

- **Код:** [`/obj/machinery/power`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/power.dm#L9), [`proc/connect_to_network()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/power.dm#L301), [`proc/cable_layer_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/power.dm#L66)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/power/grid.rs`: подключение к сети при загрузке; нет смены слоя и машин, построенных позже (shortcut)

### Каналы питания зоны и потребление машин
Каждая зона имеет три канала (оборудование, освещение, окружение) от своего APC; машина работает, только если её канал запитан (`powered()`), и тратит энергию статически (idle/active) или разово (`use_energy`).

- **Код:** [`/area/proc/powered()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/area/areas.dm#L503), [`/area/proc/power_change()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/area/areas.dm#L530), [`/area/proc/use_energy()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/area/areas.dm#L578), [`/obj/machinery/proc/powered()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/power.dm#L119), [`/obj/machinery/proc/use_energy()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/power.dm#L159), [`proc/power_change()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/power.dm#L273)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/power/model.rs` (`AreaPower`, каналы), `crates/ss13_game/src/power/mod.rs` (`Unpowered`, `use_energy`; двери, вендоматы и кухня читают питание)

### Удар током (electrocute_mob)
Урон от сети по её мощности при касании кабеля, решётки под током, электрифицированной двери; изолированные перчатки защищают.

- **Код:** [`/proc/electrocute_mob()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/power.dm#L478), [`proc/get_electrocute_damage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/powernet.dm#L95), [`cable/shock()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/cable.dm#L282)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wires.rs`: удар от электрифицированных шлюзов, APC и вендоматов по ячейке и сети; нет ударов от кабелей и решёток (`crates/ss13_game/src/construction.rs`: shortcut)

### Мерцание света по сети и передача данных
Скачки в сети заставляют мерцать лампы вдоль кабеля; powernet передаёт данные для устройств с сетевым портом.

- **Код:** [`proc/propagate_light_flicker()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/powernet.dm#L108), [`proc/data_transmission()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/powernet.dm#L99)
- **Статус в ss13-rs:** ❌ нет

### Декоративные «трубочисты» (pipe cleaners)
Цветные проволочки, которые укладываются как кабели, но не проводят ток (поделки, украшения).

- **Код:** [`/obj/structure/pipe_cleaner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/pipecleaners.dm#L26), [`/obj/item/stack/pipe_cleaner_coil`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/pipecleaners.dm#L180)
- **Статус в ss13-rs:** ❌ нет
