# Станции и ротация карт

Карты станций из `_maps/map_jsons/*.json` (тот же формат `/datum/map_config`), их ротация и отладочные карты. В ss13-rs карта выбирается переменной `SS13_MAP` (имя каталога `_maps/map_files/<name>/<name>.dmm` или путь к `.dmm`); из json читается только раздел `"shuttles"` (`crates/ss13_mapload/src/dock.rs`, `map_shuttle`). Все `.dmm` в `_maps` разбираются парсером (`crates/ss13_assets/tests/dmm_real_maps.rs`, `every_map_parses`).

### MetaStation («ИСН Цереброн»)
Основная одноуровневая станция tg; в форке называется «ИСН Цереброн», у неё свой приветственный звук; json задаёт шаттлы (`emergency_meta`, `cargo_box`, `ferry_fancy`, `whiteship_meta`) и `job_changes` (НТ-представитель, Blueshield, Magistrate по 1, Explorer 4, у капитана нет хартии). Автомаппер форка вставляет в неё 20 шаблонов (кабинеты НТР/магистрата/адвоката, крио, порты Argos/SpecOps/Gamma, галактическая карта и т. д.).

- **Код:** [`metastation.json`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_jsons/metastation.json#L1), [`MetaStation.dmm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_files/MetaStation/MetaStation.dmm)
- **BandaStation:** [`fluff_name`/`welcome_sound_override`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_jsons/metastation.json#L6), шаблоны автомаппера в [`automapper_config.toml`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/bandastation/automapper/automapper_config.toml#L1) (см. [bandastation-mapping.md](bandastation-mapping.md))
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/load.rs`, `crates/ss13_mapload/tests/real_maps.rs`: основная карта, на которой идут раунд и все тесты; не применяются шаблоны автомаппера (нет кабинетов НТР/магистрата, крио форка), `job_changes`, `fluff_name` и приветственный звук из json

### Tramstation
Двухуровневая станция с трамваем между секторами, базовый турф — астероид; шаттлы `emergency_tram`, `whiteship_tram`; 13 шаблонов автомаппера.

- **Код:** [`tramstation.json`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_jsons/tramstation.json#L1), [`tramstation.dmm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_files/tramstation/tramstation.dmm), [`map_specific_code/Tramstation.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/map_specific_code/Tramstation.dm#L1)
- **BandaStation:** [шаблоны `tramstation` в автомаппере](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/bandastation/automapper/templates/tramstation)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/tests/real_maps.rs`: загружается полностью (тест `metastation_and_tramstation_load_fully`), но уровни не связаны по вертикали, трамвай не работает (см. ../shuttles/), игровой раунд на ней не проверялся

### DeltaStation («ИСН Керберос»)
Большая одноуровневая станция; в форке «ИСН Керберос», свой приветственный звук, `bonus_weakpoints = 2`; шаттлы `emergency_delta`, `cargo_delta`; 18 шаблонов автомаппера.

- **Код:** [`deltastation.json`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_jsons/deltastation.json#L1), [`DeltaStation2.dmm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_files/Deltastation/DeltaStation2.dmm)
- **BandaStation:** [`fluff_name`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_jsons/deltastation.json#L6), [шаблоны `deltastation`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/bandastation/automapper/templates/deltastation)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_assets/tests/dmm_real_maps.rs`: `.dmm` разбирается и грузится общим загрузчиком (`SS13_MAP=_maps/map_files/Deltastation/DeltaStation2.dmm`), json-шаттлы находятся по `map_file`; игровой раунд не проверялся, автомаппер и `job_changes` не применяются

### Cyberiad («ИСН Кибериада»)
Станция форка (порт карты SS220/Paradise), два уровня (`main_floor = 2`), игрокам выдаются крюки (`give_players_hooks`), свой приветственный звук; шаттлы `emergency_delta`, `cargo_box`.

- **Код:** [`cyberiad.json`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_jsons/cyberiad.json#L1), [`Cyberiad.dmm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_files/Cyberiad/Cyberiad.dmm)
- **BandaStation:** вся карта — форк; [`cyberiad.json`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_jsons/cyberiad.json#L6)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_assets/tests/dmm_real_maps.rs`: `.dmm` разбирается (тест `cyberiad`), грузится общим загрузчиком и запускается сервером (`SS13_MAP=Cyberiad`, раунд стартует); оба уровня станционные и стыкуются для атмоса (межэтажные трубы, тест `atmos_machinery::cyberiad`); нет остального мульти-Z ([map-loading.md](map-loading.md) «Мульти-Z»), крюков игрокам (`give_players_hooks`), приветственного звука и `job_changes`; 2 из 42 профессий без точки появления, 560 случайных спаунеров (в основном техтоннели) не раскрываются

### IceBox Station
Планетарная станция на ледяной луне: 3 уровня станции поверх подземных уровней с лавой и пещерами, 2 уровня «дикой местности» (`wilderness`), генерация пещер, ледяные руины, без космических руин и параллакса.

- **Код:** [`icebox.json`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_jsons/icebox.json#L1), [`IceBoxStation.dmm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_files/IceBoxStation/IceBoxStation.dmm), [`IceWilderness`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_files/IceWilderness)
- **BandaStation:** [шаблоны `iceboxstation`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/bandastation/automapper/templates/iceboxstation)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_assets/tests/dmm_real_maps.rs`: только разбор `.dmm`; нет планетарных уровней, дикой местности, генерации пещер, мульти-Z — как станция не играбельна

### Kilo Station
Одноуровневая станция на астероиде (`exclude_from_ci`), шаттлы `emergency_kilo`, `cargo_kilo`, у капеллана монастырь; в ротацию `config/maps.txt` не входит.

- **Код:** [`kilostation.json`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_jsons/kilostation.json#L1), [`KiloStation.dmm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_files/KiloStation/KiloStation.dmm)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_assets/tests/dmm_real_maps.rs`: только разбор `.dmm` (общий тест), загрузка не проверялась

### NebulaStation
Двухуровневая станция (`Up`/`Down` трейты), шаттлы `emergency_nebula`, `cargo_nebula`, `bonus_weakpoints = 2`.

- **Код:** [`nebulastation.json`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_jsons/nebulastation.json#L1), [`NebulaStation.dmm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_files/NebulaStation/NebulaStation.dmm)
- **BandaStation:** [шаблоны `nebulastation`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/bandastation/automapper/templates/nebulastation)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_assets/tests/dmm_real_maps.rs`: только разбор `.dmm`; уровни не связаны

### Wawastation
Двухуровневая станция на астероиде, шаттл `emergency_wawa`; в ротацию форка не входит.

- **Код:** [`wawastation.json`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_jsons/wawastation.json#L1), [`wawastation.dmm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_files/wawastation/wawastation.dmm)
- **BandaStation:** [шаблоны `wawastation`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/bandastation/automapper/templates/wawastation)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_assets/tests/dmm_real_maps.rs`: только разбор `.dmm`; уровни не связаны

### Отладочные карты
Runtime Station и её минимальная версия (карта юнит-тестов), MultiZ Debug (три связанных уровня), Gateway Test (грузит все экспедиции). Используются разработчиками и CI, в ротацию не входят. CatwalkStation убрана (`_maps/_removed_map_configs`).

- **Код:** [`runtimestation.json`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_jsons/runtimestation.json#L1), [`runtimestation_minimal.json`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_jsons/runtimestation_minimal.json#L1), [`multiz_debug.json`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_jsons/multiz_debug.json#L1), [`gateway_test.json`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/map_jsons/gateway_test.json#L1), [`catwalkstation.json`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/_maps/_removed_map_configs/catwalkstation.json#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_assets/tests/dmm_real_maps.rs`: `.dmm` разбираются; для тестов ss13-rs использует собственные фикстуры (`scripts/visual/maps/*.dmm`), а не эти карты

### Ротация карт (`config/maps.txt`)
Список карт сервера: `minplayers`/`maxplayers`, `votable`, `voteweight`, `default`, `disabled`; в форке голосуемые Delta (25+), Meta (25+), Tram (35+), Nebula (35+), IceBox (40+), Cyberiad (50+). Следующая карта выбирается голосованием (см. [map-loading.md](map-loading.md)) и сохраняется в `data/next_map.json`.

- **Код:** [`config/maps.txt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/config/maps.txt#L1), [`/proc/load_map_config`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/map_config.dm#L106)
- **Статус в ss13-rs:** ❌ нет — карта задаётся только переменной окружения `SS13_MAP`, перезагрузка мира грузит ту же карту
