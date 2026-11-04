# Связь

Средства связи оригинального сервера: радио (гарнитуры, ключи шифрования, интеркомы), телекоммуникации и сервер сообщений, консоль связи и приоритетные объявления, консоли запросов, новостные терминалы, статус-дисплеи, система автообъявлений, голопады, модульные компьютеры (КПК, ноутбуки, консоли NtOS) со всеми приложениями, а также модуль форка с LOOC и клавишей шёпота. Источники: `code/game/machinery/telecomms`, `code/game/objects/items/devices/radio`, `code/modules/NTNet`, `code/modules/modular_computers`, `code/game/machinery/newscaster`, `requests_console.dm`, `status_display.dm`, `announcement_system.dm`, `hologram.dm`, `computer/communications.dm`, `code/datums/communications.dm`; форк: `modular_bandastation/{communication,announcers,navigator,minesweeper}`. В ss13-rs есть радио без телекомов (гарнитуры, интеркомы, каналы), консоль связи (вход, шаттл, код тревоги, объявления, сообщения ЦК и Синдикату, запросы кодов и ОБР, аварийный доступ, статус-дисплеи, список сообщений), приоритетные объявления с дикторами и отчёт ЦК в начале раунда и КПК (`crates/ss13_game/src/modpc/`: `NtosMain`, мессенджер без телекомов, манифест экипажа, блокнот, атмосферный сканер, ID-слот).

| Файл | Что внутри |
| --- | --- |
| [radio.md](radio.md) | Радио, гарнитуры, ключи шифрования, интеркомы, окно радио |
| [telecomms.md](telecomms.md) | Машины телекомов, мониторы, сервер сообщений, реле NTNet |
| [communications-console.md](communications-console.md) | Консоль связи по разделам, приоритетные объявления, дикторы, отчёт ЦК |
| [station-info.md](station-info.md) | Консоли запросов, новостные терминалы, статус-дисплеи, AAS, голопады |
| [modular-computers.md](modular-computers.md) | Модульные компьютеры: NtOS, КПК, ноутбуки, консоли, файлы |
| [ntos-apps.md](ntos-apps.md) | Приложения NtOS по группам |
| [looc-whisper.md](looc-whisper.md) | LOOC и клавиша шёпота (BandaStation) |
