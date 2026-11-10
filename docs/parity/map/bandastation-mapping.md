# Маппинг форка BandaStation

Модпаки форка, которые меняют карты: автомаппер, спаун по зонам, свои зоны и шаблоны, наномапа, галактическая карта.

### Автомаппер (SSautomapper)
Подсистема форка (из Skyrat): по `automapper_config.toml` (129 шаблонов) вставляет куски `.dmm` в апстримные карты при загрузке — кабинеты НТ-представителя, Blueshield, магистрата, адвоката, крио-камеры (обычные и пермабриг), порты Argos/SpecOps/Gamma, оружейка, ксерокс у глав, галактическая карта в библиотеке и т. п. для Meta, Delta, IceBox, Kilo, Nebula, Tram, Wawa, Birdshot, Northstar.

- **Код:** [`SUBSYSTEM_DEF(automapper)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/automapper/code/automapper_subsystem.dm#L15), [`/datum/map_template/automap_template`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/automapper/code/automap_template.dm#L1), [`automapper_config.toml`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/bandastation/automapper/automapper_config.toml#L1), [`templates`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/bandastation/automapper/templates)
- **BandaStation:** весь модуль — форк ([`_automapper.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/automapper/_automapper.dm#L1))
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_mapload` не читает `automapper_config.toml`, MetaStation грузится без 20 шаблонов форка

### Спаун по зонам (SSarea_spawn)
Подсистема форка: ставит предметы, мобов и стартовые точки ролей форка в подходящие зоны любой карты (питомцы Маркус и Бамблс, автомат GBP, стартовые точки парикмахера, офицера исправительного учреждения, специалиста телекомов, исследователя).

- **Код:** [`SUBSYSTEM_DEF(area_spawn)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/automapper/code/area_spawn_subsystem.dm#L1), [`/datum/area_spawn`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/automapper/code/area_spawn_subsystem.dm#L255), [`area_spawn_entries.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/automapper/code/area_spawn_entries.dm#L5)
- **BandaStation:** весь модуль — форк
- **Статус в ss13-rs:** ❌ нет

### Зоны, ландмарки и шаблоны форка
Модпак `mapping`: станционные и внестанционные зоны форка, ландмарки, слои, шаблоны шаттлов ОБР/синдиката (SIT, SST, Argos, SpecOps, Gamma) и ленивые шаблоны.

- **Код:** [`templates.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mapping/code/templates.dm#L13), [`areas/station.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mapping/code/areas/station.dm#L1), [`areas/off_station.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mapping/code/areas/off_station.dm#L1), [`landmarks.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mapping/code/landmarks.dm#L1)
- **BandaStation:** весь модуль — форк ([`_mapping.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mapping/_mapping.dm#L1))
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload`: типы форка на картах конвертируются, как любые DM-типы (зоны, ландмарки), но шаблоны шаттлов ОБР/синдиката не загружаются

### Наномапа (карта станции в консолях)
Карты-изображения станций, выдаваемые в tgui консоли камер и монитора экипажа (и сенсорному устройству): позиции экипажа и камер рисуются на плане станции.

- **Код:** [`get_map_ui_data()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/nanomap/code/nanomap_datum.dm#L4), [`/datum/asset/simple/nanomaps`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/nanomap/code/nanomap_assets.dm#L1), [`crew_monitor.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/nanomap/code/crew_monitor.dm#L1); tgui: [`CrewConsole220`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CrewConsole220), [`CameraConsole220.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CameraConsole220.tsx)
- **BandaStation:** весь модуль — форк ([`_nanomap.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/nanomap/_nanomap.dm#L1))
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_mapload/src/nanomap.rs` (`ss13-mapload convert` копирует 15 PNG `/datum/asset/simple/nanomaps` в `nanomap/` ассетов под ключами оригинала, «Kilo Station» → `KiloStation_nanomap_z1.png`; `nanomap/SOURCES.txt`, файлы атрибуции), `crates/ss13_game/src/nanomap.rs` (`map_ui_data` — `get_map_ui_data`: `map_name` из json карты, `minFloor`/`mainFloor` (`main_floor`)/`maxFloor`, `lavalandLevel`; картинка уровня как `getMapImage`, подложка под метками в масштабе 2 px на тайл, как стартовый масштаб tgui; выбор этажа и Лаваленда), в статических данных консоли камер и монитора экипажа (консоль и ручной). Нет: зума, панорамы и миникарты, `stairs` в данных (навигатор КПК — списком)

### Галактическая карта
Золотая статуя-карта (в библиотеках через автомаппер), открывающая tgui-окно с лорной политической картой галактики.

- **Код:** [`/obj/structure/statue/gold/galactic_map`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/galactic_map/code/galactic_map.dm#L1); tgui: [`GalacticMap.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/GalacticMap.tsx)
- **BandaStation:** весь модуль — форк ([`_galactic_map.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/galactic_map/_galactic_map.dm#L1))
- **Статус в ss13-rs:** ❌ нет

### Имя станции и приветственный звук карты
`fluff_name` из json карты перекрывает случайное и праздничное имя станции (Цереброн, Керберос, Кибериада); `welcome_sound_override` проигрывается в начале раунда вместо звука анонсера; `main_floor` — главный этаж для «подпольных» блобов.

- **Код:** [`/proc/station_name`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/names.dm#L87), [`/datum/map_config` fluff](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/map_config.dm#L80)
- **BandaStation:** [`BANDASTATION ADDITION - Station Fluff`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/names.dm#L90)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/round.rs` (`station_name`) берёт имя карты, json-поля не читаются
