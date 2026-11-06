# Тайлы и полы
Базовая механика тайлов (`/turf`): смена типа, стек базовых тайлов, полы и плитка, космос, многоуровневость и особые поверхности.

### Смена тайла и стек baseturfs
`ChangeTurf` заменяет тайл новым типом с сохранением содержимого, `place_on_top`/`ScrapeAway` кладут слой сверху или снимают верхний (пол → обшивка → космос); `baseturf_skipover` отмечает границу шаттла.

- **Код:** [`proc/ChangeTurf()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/change_turf.dm#L56), [`proc/place_on_top()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/baseturfs.dm#L31), [`proc/ScrapeAway()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/baseturfs.dm#L2), [`/turf/baseturf_skipover`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/baseturf_skipover.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/construction.rs` (`change_turf`, `Baseturfs`): смена на месте с пересчётом воздуха, света, сглаживания и подпольных объектов; низ стека всегда космос (`ZTRAIT_BASETURF` не читается)

### Напольная плитка: снять и положить
Ломом плитка снимается (выпадает её стек, остаётся обшивка), плитка на обшивке кладётся обратно; на уже покрытом полу замена плитки идёт с ломом во второй руке (`try_replace_tile`).

- **Код:** [`crowbar_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor.dm#L144), [`proc/pry_tile()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor.dm#L159), [`proc/try_replace_tile()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor.dm#L148), [`plating/item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor/plating.dm#L54)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/construction.rs`: снятие ломом и укладка на обшивку; замены плитки на покрытом полу нет

### Повреждение пола
Пол ломается и обгорает (`break_tile`, `burn_tile`) от взрывов, огня и ударов, превращаясь в повреждённую плитку или обшивку; обшивку чинят сваркой.

- **Код:** [`proc/break_tile_to_plating()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor.dm#L103), [`proc/make_plating()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor.dm#L110), [`plating/welder_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor/plating.dm#L150)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/turf_damage.rs` (`burn_tile` от сильного огня, `break_tile`, оверлеи `damaged_dmi`), `crates/ss13_mapload/src/turf_damage.rs`: пол ломают взрывы (`crates/ss13_game/src/explosions/ex_act.rs`: лёгкий взрыв с шансом 50%, тяжёлый в одном из трёх исходов); нет поломки ударами, `break_tile_to_plating` и ремонта сваркой

### Виды полов
Стальные полы (сотни раскрасок `iron`), деревянные, ковры, минеральные (плазма — горит, уран — фонит, бананиум — пищит), бетон, «цепные» полы, полы-схемы (светятся), голые решётчатые (`catwalk_floor`), корпус, светящиеся полы (мультитулом меняют цвет), стеклянные (видно, что под ними).

- **Код:** [`/turf/open/floor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor.dm#L2), [`iron_floor.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor/iron_floor.dm), [`fancy_floor.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor/fancy_floor.dm), [`mineral_floor.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor/mineral_floor.dm), [`misc_floor.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor/misc_floor.dm), [`light/multitool_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor/light_floor.dm#L120), [`/turf/open/floor/glass`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor/glass.dm#L1), [`catwalk_plating.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor/catwalk_plating.dm)
- **BandaStation:** [`modular_bandastation/aesthetics/floors`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/floors/code/floors.dm) — свои спрайты полов; [`modular_bandastation/objects/code/floors/wooden.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/floors/wooden.dm) — генерация подтипов деревянного пола
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/undertile.rs`, `crates/ss13_mapload/src/smooth.rs`: полы грузятся с карты со спрайтами, стеклянные и решётчатые показывают трубы и кабели под собой; особых свойств (горение плазмы, радиация, светящиеся полы) нет

### Укреплённые полы (engine)
`/turf/open/floor/engine` не снимаются ломом: гаечный ключ снимает пластины за несколько секунд (выпадают стальные прутья); устойчивы к взрывам; есть варианты с газом для камер.

- **Код:** [`/turf/open/floor/engine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor/reinforced_floor.dm#L2), [`engine/wrench_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor/reinforced_floor.dm#L44), [`/turf/open/floor/plating/reinforced`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/floor/plating.dm#L230)
- **Статус в ss13-rs:** ❌ нет

### Космос и стройка в космосе
Космос (`/turf/open/space`) — вакуум и свет звёзд; прутья на космосе дают решётку (`lattice`), плитка на решётке — обшивку, прутья на решётке — мостик (`catwalk`).

- **Код:** [`/turf/open/space`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/space/space.dm#L45), [`space/item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/space/space.dm#L130), [`proc/update_starlight()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/space/space.dm#L106)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_atmos`, `crates/ss13_shared/src/light.rs`: вакуум и звёздный свет; `crates/ss13_game/src/construction.rs` (`build_over_space`): прутья — решётка, на решётке — мостик, плитка на решётке или рядом с полом — обшивка (воздух тайла наследуется); `CanBuildHere` (зоны шаттлов) не проверяется (shortcut)

### Открытое пространство и падение между уровнями
`openspace` показывает уровень ниже; всё, что на нём не держится, падает (`zFall`) с уроном; решётка или мостик закрывают его.

- **Код:** [`/turf/open/openspace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/openspace.dm#L1), [`proc/zFall()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/turf.dm#L349), [`proc/zPassIn()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/turf.dm#L333)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/multiz.rs` (`fall_through_openspace`, `z_fall`, `z_impact`): падает всё живое, предметы и незакреплённые объекты, пришедшие или появившиеся на открытом пространстве, с продолжением на уровень ниже; урон `(5·уровни)^1.5` по ногам стоящего, сбивание с ног; решётки и мостики (`BLOCK_Z_*`) держат; виден уровень ниже со своим светом (`crates/ss13_render/src/sprites.rs`, вторая карта света в `crates/ss13_render/src/lighting/mod.rs`), его можно кликать и осматривать; воздух через открытое пространство обменивается с уровнем ниже (`ss13_atmos::Atmos::share_vertical`), клиент предсказывает падение своего моба (`ss13_world::FallsThrough`). Кошачья грация, крылья и паркур — в своих группах; постройка пола на открытом пространстве — в записи о полах

### Пропасти, лава, вода
Пропасть (`chasm`) роняет всё на уровень ниже или уничтожает; лава поджигает и сжигает всё, что не огнеупорно (`burn_stuff`); вода мочит и тушит, в ней можно рыбачить.

- **Код:** [`/turf/open/chasm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/chasm.dm#L2), [`proc/drop()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/chasm.dm#L30), [`/turf/open/lava`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/lava.dm#L7), [`proc/burn_stuff()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/lava.dm#L258), [`/turf/open/water`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/water.dm#L1)
- **BandaStation:** [`modular_bandastation/turfs/code/water.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/turfs/code/water.dm#L4) — альтернативная и илистая вода, варианты без рыбалки
- **Статус в ss13-rs:** 🟡 частично — лава: `crates/ss13_game/src/lava.rs` (`burn_stuff`, `can_burn`, `do_burn`): при входе и раз в секунду живое получает 20 огненных стаков, поджигается и 20 ожогов; предметы и структуры — `fire_act(10000)` (20 урона) и горение (`crate::fire::set_burning`, предметы — через огнестойкость брони, срезанную до 50), лава снимает `FIRE_PROOF` и даёт `FLAMMABLE`; не горят `LAVA_PROOF`, летящие, брошенные, в невесомости, `TRAIT_LAVA_IMMUNE` (виды, шахтёрская фауна и мегафауна), пристёгнутые к негорящему, в костюме и шлеме с `LAVAPROTECT`, пилоты мехов; мостки и каменные плиты (`TRAIT_LAVA_STOPPED`) защищают. `// shortcut:` нет плазменной лавы, замедления (нет замедления тайлов), `TRAIT_NO_EXTINGUISH`, подсветки краёв, рыбалки, прикуривания, жаропрочных прутьев, возвышений. Пропастей (нет на Лаваленде и в его генерации) и воды нет

### Планетарные и природные поверхности
Пепел, базальт, астероид, песок/пляж, снег, лёд (скользкий), трава, грязь, сено, утёсы; часть с собственной атмосферой планеты.

- **Код:** [`/turf/open/misc`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/misc.dm#L4), [`asteroid.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/asteroid.dm), [`/turf/open/misc/ice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/ice.dm#L1), [`/turf/open/cliff`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/cliff.dm#L2), [`sand.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/sand.dm), [`snow.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/snow.dm)
- **Статус в ss13-rs:** ❌ нет — грузятся с карты как обычные открытые тайлы; планетарная атмосфера не перенесена (`crates/ss13_atmos/src/lib.rs`, shortcut)

### Скользкие полы
Мокрый, смазанный, ледяной пол заставляет поскользнуться (сбивание с ног, выпадение предметов); сохнет со временем.

- **Код:** [`proc/MakeSlippery()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/_open.dm#L631), [`handle_slip()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/_open.dm#L560)
- **BandaStation:** [`_open.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/_open.dm#L560) — поскальзывание дополнительно обездвиживает
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/janitor.rs` (`WetFloor`, `make_slippery`, поскальзывание; источники — швабра, спреи, конденсация пара), подробности в ../service/janitorial.md

### Неразрушимые и минеральные стены-тайлы
`/turf/closed/indestructible` — стены ЦК и руин, их нельзя разрушить; `/turf/closed/mineral` — породы с рудой, копаются киркой/буром (добыча руды — см. ../cargo/).

- **Код:** [`/turf/closed/indestructible`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/indestructible.dm#L1), [`/turf/closed/mineral`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/minerals.dm#L6), [`proc/gets_drilled()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/minerals.dm#L278)
- **BandaStation:** [`_closed.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/_closed.dm#L32) — сглаживание скал; [`indestructible.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/indestructible.dm#L211); [`aesthetics/walls/code/rocks.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/walls/code/rocks.dm) — спрайты пород
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/smooth.rs`: грузятся и сглаживаются как непроходимые тайлы; копания нет

### Области (areas)
`/area` делит карту на помещения: имя, питание и APC, пожарная тревога области, гравитация, звуки окружения, свет; админы и чертежи могут создавать новые области.

- **Код:** [`/area`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/area/areas.dm#L6), [`code/game/area/areas`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/area/areas), [`proc/set_fire_effect()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/area/areas.dm#L464), [`proc/create_area()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/areas.dm#L108)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload` (`Areas`), `crates/ss13_game/src/power/mod.rs` (`AreaPower`): области с карты и питание по ним; нет создания областей, тревог, звуков окружения, гравитации

### Чертежи станции
Чертежи (`/obj/item/blueprints`) показывают исходную схему проводки и труб, позволяют создать или переименовать область.

- **Код:** [`/obj/item/blueprints`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/blueprints.dm#L19); tgui: [`Blueprints.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Blueprints.tsx)
- **Статус в ss13-rs:** ❌ нет
