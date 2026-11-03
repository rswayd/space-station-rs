# Газоанализатор
Ручной прибор для измерения воздуха.

### Газоанализатор (/obj/item/analyzer)
Клик по себе — анализ своего тайла, по предмету/машине/трубе — анализ их газа (`atmos_scan`, вывод в чат); правый клик открывает окно с историей сканов.

- **Код:** [`/obj/item/analyzer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/gas_analyzer.dm#L1), [`proc/atmos_scan()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/gas_analyzer.dm#L213), [`proc/scan_atom()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/gas_analyzer.dm#L180), [`ui_interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/gas_analyzer.dm#L129); tgui: [`GasAnalyzer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/GasAnalyzer.tsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/atmos.rs`, `crates/ss13_atmos/src/scan.rs`: вывод tg в чат для всего, что держит газ (`return_analyzable_air`: тайл, канистра, стационарный бак, труба, порты машин как «Node N», баллон), окно `GasAnalyzer` по правому клику по анализатору в руке (`crates/ss13_tgui/src/interfaces/gas_analyzer.rs`: смеси последнего сканирования, обновляются пока цель в поле зрения, и справочник газов и реакций из `atmos_handbook.ron`, факторы реакций выгружает `ss13_dm_extract`); в списке реакций смеси — только реакции горения; сканирование аномалий → ../effects/ (аномалий нет)

### Барометр газоанализатора
Alt-клик предсказывает время до следующей бури (на планетах), с погрешностью и откатом.

- **Код:** [`click_alt()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/gas_analyzer.dm#L68), [`proc/butchertime()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/gas_analyzer.dm#L118)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/atmos.rs` (`barometer`): alt-клик по анализатору в руке, в помещении отказ, снаружи (`outdoors` области) поиск погоды и кулдаун 25 с с сигналом готовности; погоды пока нет, поэтому прогноз всегда «не удалось отследить» → ../effects/weather.md

### Дальнобойный газоанализатор
Вариант, который сканирует атомы на расстоянии.

- **Код:** [`/obj/item/analyzer/ranged`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/gas_analyzer.dm#L260), [`ranged_interact_with_atom()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/gas_analyzer.dm#L167)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/atmos.rs` (`ranged_scans`): `/obj/item/analyzer/ranged` сканирует по клику за пределами досягаемости до 15 тайлов в прямой видимости и держит окно в этом радиусе
