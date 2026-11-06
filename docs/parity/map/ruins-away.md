# Руины, планеты, экспедиции и CentCom

Всё, что грузится вне станции: уровень CentCom, Лаваленд и ледяная луна с генерацией пещер, случайные руины, экспедиции через гейтвей. Виртуальные домены битраннеров — см. ../science/, карты Deathmatch — см. ../round/, шаблоны шаттлов — см. ../shuttles/.

### Уровень CentCom
Отдельный z-уровень с Центральным командованием: зал прибытия эвакошаттла, конец раунда, тюрьма ЦК, база ОБР, тандердом, база нюкеров (ленивый шаблон), ферри-шаттл, ключ-карты базы синдиката.

- **Код:** [`CentCom.dmm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_files/generic/CentCom.dmm), [`/area/centcom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/area/areas/centcom.dm#L4), [`mapfluff/centcom/nuke_ops.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapfluff/centcom/nuke_ops.dm#L2)
- **Статус в ss13-rs:** ❌ нет — CentCom.dmm не грузится; уровень, куда прибывает эвакошаттл, — это уровень его шаблона (`crates/ss13_game/src/shuttle.rs`, «CentCom stand-in»)

### Лаваленд (уровень шахты)
Вулканическая планета под станцией при `minetype = lavaland`: база шахтёров, лавовые реки, пепельные бури, мегафауна; поверхность генерируется клеточным автоматом и биомами.

- **Код:** [`Lavaland.dmm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_files/Mining/Lavaland.dmm), [`LoadGroup(... "Lavaland" ...)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/mapping.dm#L493), [`/datum/map_generator/cave_generator/lavaland`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mapgen/Cavegens/LavalandGenerator.dm#L1), [`/datum/biome/lavaland`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mapgen/biomes/lavaland.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/lib.rs` (`MiningLevel`, `mining_z`): `Lavaland.dmm` грузится на свой z сразу после станции (не станционный уровень) при `minetype = lavaland` в json карты (по умолчанию в tg), `SS13_MINING_LEVEL=0` отключает; планетарная атмосфера `LAVALAND_ATMOS` — `ss13_atmos::planetary`; пещеры — `crates/ss13_mapload/src/cave.rs` (см. «Генераторы карт»); доки шахтёрского, общего и трудового шаттлов и случайные доки капсул работают; лавовые реки (`crates/ss13_mapload/src/cave.rs`, `spawn_rivers`), руины (`crates/ss13_mapload/src/ruins.rs`), лава жжёт (`crates/ss13_game/src/lava.rs`), пепельные бури и emberfall (`crates/ss13_game/src/weather.rs`), трейты уровня (бомбкап ×2, базовый тайл — лава). Мегафауна и фауна — в ../mobs/

### Ледяная луна и дикая местность (IceBox)
Планетарные уровни IceBox: подземные слои с плазменной лавой, поверхность из случайно выбранных кусков `IceWilderness` (скальные равнины, лес, горы), генерация пещер (поверхность, лес, скалы, глубины).

- **Код:** [`/datum/map_generator/cave_generator/icemoon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mapgen/Cavegens/IcemoonCaves.dm#L1), [`IceWilderness`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_files/IceWilderness), [`ZTRAITS_ICY_WILDS`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__DEFINES/maps.dm#L138)
- **Статус в ss13-rs:** ❌ нет

### Генераторы карт
Генератор пещер (клеточный автомат + биомы + флора/фауна/руда), генератор джунглей, старая система процедурного маппинга (`mapGenerator` с модулями), запуск генерации рельефа после загрузки.

- **Код:** [`/datum/map_generator/cave_generator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mapgen/CaveGenerator.dm#L9), [`jungle_generator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mapgen/JungleGenerator.dm#L4), [`/datum/map_generator` (procedural)](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/procedural_mapping/mapGenerator.dm#L3), [`/datum/map_generator_module`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/procedural_mapping/mapGeneratorModule.dm#L2), [`run_map_terrain_generation()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/mapping.dm#L532)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/cave.rs`: тайлы `/turf/open/genturf` уровня шахты до спавна заменяются клеточным автоматом (`noise_percent` 51, `ca_steps` 8, `birth_limit` 6, `survival_limit` 4, края — скала) на базальт или вулканическую породу (`cave_generator/lavaland`); затем лавовые реки tg `spawn_rivers`: 3 случайные точки в квадрате 50–200 плюс точка с карты, извилистые реки к другой точке (повороты на 45° с шансом 20%), обход зон вне `/area/lavaland/surface/outdoors/unexplored`, расширение `Spread(25, 11)` в породу; нет BSP-комнат и коридоров rust-g `cave_system_generator`, биомов, флоры, фич (гейзеры, рудные жерла), мобов, генератора джунглей и процедурного `mapGenerator`

### Размещение руин
Руины ставятся случайно по бюджету (конфиг `space_budget`, `lavaland_budget`, `icemoon_budget`) в разрешённые зоны, с учётом `always_place`, `unpickable`, тематических руин и рудного бюджета.

- **Код:** [`setup_ruins()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/mapping.dm#L260), [`/proc/seedRuins`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/ruins.dm#L80), [`/datum/map_template/ruin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ruins.dm#L1), [`try_to_place()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/ruins.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/ruins.rs` (`seed`): алгоритм `seedRuins` для Лаваленда — сначала `always_place`, затем случайные по весу, бюджет 60 (`lavaland_budget` по умолчанию), `allow_duplicates`/`unpickable` по `id`, 100 попыток `try_to_place` (центр не ближе `TRANSITIONEDGE + SPACERUIN_MAP_EDGE_PAD` + полруины от края, весь след — в `/area/lavaland/surface/outdoors/unexplored` и не на другой руине), вставка до спавна карты с `template_noop` и `clear_below`; шаблон ставится, только если все его типы сконвертированы. Нет конфигов бюджета и чёрного списка, космических и ледяных руин, тематических руин, рудного бюджета (жерла), `always_spawn_with`/`never_spawn_with`

### Космические руины
~67 шаблонов (`_maps/RandomRuins/SpaceRuins`, 87 файлов): заброшенные станции и корабли (TheDerelict, oldstation, forgottenship, whiteship), отель Хилберта, DJ-станция, листенинг-пост синдиката, Interdyne, космические закусочные и т. д.; у многих свой код (`mapfluff/ruins/spaceruin_code`).

- **Код:** [`/datum/map_template/ruin/space`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ruins/space.dm#L3), [`SpaceRuins`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/RandomRuins/SpaceRuins), [`spaceruin_code`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapfluff/ruins/spaceruin_code)
- **Статус в ss13-rs:** ❌ нет

### Руины Лаваленда
~34 шаблона: логово пепельных ящеров, шахтёрская база, храмы, колизей, руины с портальными комнатами, ловушки, «звёздные врата»; свой код в `lavalandruin_code`.

- **Код:** [`/datum/map_template/ruin/lavaland`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ruins/lavaland.dm#L3), [`LavaRuins`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/RandomRuins/LavaRuins), [`lavalandruin_code`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapfluff/ruins/lavalandruin_code)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/ruins.rs` (`LAVALAND_RUINS`): 38 шаблонов из `LavaRuins` с `cost`, `always_place` (иерофант, опухоль элиты, случайный вариант кровавого шахтёра), `allow_duplicates`, BandaStation `unpickable` у лени; карты ставятся на уровень шахты (`ss13-mapload convert` берёт `LavaRuins`), их предметы и структуры работают тем, что уже портировано. Нет жерл руды (`vent`), руин из `AnywhereRuins` (golem ship, fountain hall, shoe factory), кода `lavalandruin_code` (грехи, куб, сид-волт, пепельные ящеры и т. п.)

### Ледяные руины
~35 шаблонов для поверхности и подземелий ледяной луны: деревня, хижины, база синдиката, логова мегафауны; свой код в `icemoonruin_code`; плюс «AnywhereRuins» (fountain_hall, golem_ship, shoe_factory), которые ставятся на любой тип уровня.

- **Код:** [`/datum/map_template/ruin/icemoon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ruins/icemoon.dm#L3), [`IceRuins`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/RandomRuins/IceRuins), [`AnywhereRuins`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/RandomRuins/AnywhereRuins), [`icemoonruin_code`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapfluff/ruins/icemoonruin_code), [`shoe_factory.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapfluff/ruins/shoe_factory.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Экспедиции (away missions)
Отдельные z-уровни, куда ходят через гейтвей: Academy, Heretic, Moon Outpost 19, Museum, Research, Snow Cabin, The Beach, Underground Outpost 45 (`_maps/RandomZLevels`), со своим кодом, листовками, кордоном и указателями; одна выбирается случайно на раунд (конфиг `awaymissionconfig`). Сама машина гейтвея — см. ../science/.

- **Код:** [`/proc/createRandomZlevel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/awaymissions/zlevel.dm#L5), [`/obj/effect/landmark/awaystart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/awaymissions/zlevel.dm#L21), [`mission_code`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/awaymissions/mission_code), [`/obj/item/paper/pamphlet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/awaymissions/pamphlet.dm#L1), [`RandomZLevels`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/RandomZLevels)
- **Статус в ss13-rs:** ❌ нет
