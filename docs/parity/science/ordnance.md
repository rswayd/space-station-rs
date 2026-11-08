# Ордонанс и научные публикации

Отдел ордонанса: испытания взрывчатки и газовых смесей, запись результатов и публикация научных работ партнёрам за очки и кредиты.

### Доплеровский массив
Фиксирует взрывы на станции/полигоне (радиус, эпицентр), сохраняет записи на диск, печатает их и засчитывает эксперименты ордонанса.

- **Код:** [`/obj/machinery/doppler_array`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/ordnance/doppler_array.dm#L1), [`proc/sense_explosion()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/ordnance/doppler_array.dm#L148); tgui: [`DopplerArray.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/DopplerArray.jsx)
- **Статус в ss13-rs:** 🟡 `crates/ss13_game/src/science/ordnance_machines.rs` (`Doppler`, `sense_explosion` из `explosions::explosion`, `explosive_experiments`, печать на диск), окно `science/ordnance.rs` (`DopplerArrayUi`); реакции бака — `explosions/sources.rs` (`TankResults`, `GasMixture::react_recording` в `ss13_atmos`). Есть: направление, 150 тайлов, кулдаун 1 с, теоретический радиус капнутой бомбы, реакции/перегрев бака, запись на диск, поворот alt-кликом и ключ. Нет: связанных z-уровней станции, направленного света и оверлеев.

### Компрессор баков
Атмос-машина: сжимает газ в баке до разрыва, записывает состав утечки; используется для газовых экспериментов.

- **Код:** [`/obj/machinery/atmospherics/components/binary/tank_compressor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/ordnance/tank_compressor.dm#L5); tgui: [`TankCompressor.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/TankCompressor.tsx)
- **Статус в ss13-rs:** 🟡 `crates/ss13_game/src/science/ordnance_machines.rs` (`Compressor`, `process_compressors`, буфер утечки, `tank_destroyed`, `gaseous_experiments`), окно `TankCompressorUi` в `science/ordnance.rs`; хуки в `explosions/sources.rs` (`item_destruction`, утечка). Есть: накачка из входного порта до 5000 кПа, поглощение взрыва, запись буфера >10 моль, выброс в выходной порт, печать на диск, запрет извлечения бака >2634 кПа. Нет: поворота ключом (порты фиксируются при загрузке карты), оверлеев, тряски комнаты.

### Эксперименты ордонанса
Эксперименты техвеба на взрыв заданной силы и на газовые смеси; результаты записей доплера/компрессора закрывают их.

- **Код:** [`/datum/experiment/ordnance`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/ordnance.dm#L6), [`ordnance/explosive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/ordnance.dm#L31), [`ordnance/gaseous`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/ordnance.dm#L46)
- **Статус в ss13-rs:** ✅ `crates/ss13_game/src/science/scipaper.rs` (`EXPERIMENTS`: 5 взрывных, 4 газовых; гейны/цели/реакции из DM), `science/experiments.rs` (ордонанс-эксперименты добавляются в техвеб, прогресс «опубликуйте отчёт», завершаются только публикацией). Таблица скопирована из DM, а не из конвертера.

### Научные статьи и партнёры
Записи экспериментов публикуются через программу NT Frontier (ПДА/консоль) как статьи (взрывные/газовые) для партнёров (шахта, оборона, медицина, физика и др.); партнёры дают очки, кредиты и открывают узлы.

- **Код:** [`/datum/scientific_paper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/ordnance/_scipaper.dm#L2), [`/datum/scientific_partner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/ordnance/_scipaper.dm#L272), [`партнёры`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/ordnance/scipaper_partner.dm#L1), [`add_scientific_paper()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/techweb/_techweb.dm#L505), [`/datum/computer_file/program/scipaper_program`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/file_system/programs/frontier.dm#L1); tgui: [`NtosScipaper.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/NtosScipaper.jsx)
- **Статус в ss13-rs:** 🟡 `crates/ss13_game/src/science/scipaper.rs` (`Paper`: сигмоида гейнов, уровни, партнёры, дубликаты по записи и уровню, кооперация, финансы ×125 в научный бюджет; `PARTNERS`; бусты узлов `purchase_boost`/`boost_node`, скидка в `Techweb::price`), программа NT Frontier — `StationProgram::Frontier` в `modpc/programs/station*.rs`, экран `StationScreen::Scipaper` (`station_client.rs`); файлы ордонанса на диске — `modpc/files.rs` (`OrdnanceFile`). Нет: перепривязки техвеба мультитулом, показа/копирования файлов данных в файловом менеджере, анонса завершения обработчикам экспериментов.
