# Станционные средства связи
Консоли запросов, новостные терминалы, статус-дисплеи, система автообъявлений и голопады.

### Консоль запросов (requests console)
Настенная консоль отдела: сообщения другим отделам (обычные, приоритетные, экстренные), запрос помощи, объявление отдела (у глав, по ID), штамп сообщений, уведомления в КПК и через AAS.

- **Код:** [`/obj/machinery/requests_console`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/requests_console.dm#L13), [`proc/send_message()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/requests_console.dm#L349), [`/obj/item/wallframe/requests_console`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/requests_console.dm#L542); tgui: [`RequestsConsole`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/RequestsConsole)
- **BandaStation:** [`requests_console.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/requests_console.dm#L308) — русский заголовок объявления отдела
- **Статус в ss13-rs:** ❌ нет — только статичный спрайт из карты (`crates/ss13_mapload/src/look.rs`), без механики

### Новостной терминал (newscaster) и газеты
Каналы новостей, публикация статей с фото, комментарии, цензура (охрана), розыск (wanted), печать газет, баунти-доска; общая сеть новостей и NTOS-приложение.

- **Код:** [`/obj/machinery/newscaster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/newscaster/newscaster_machine.dm#L3), [`/datum/feed_message`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/newscaster/newscaster_data.dm#L15), [`/obj/item/newspaper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/newscaster/newspaper.dm#L6), [`/obj/item/wallframe/newscaster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/newscaster/newscaster_machine.dm#L884); tgui: [`PhysicalNewscaster.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PhysicalNewscaster.tsx), [`Newspaper.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Newspaper.tsx)
- **BandaStation:** [`newscaster_machine.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/newscaster/newscaster_machine.dm#L701) — права админов при уведомлениях
- **Статус в ss13-rs:** ❌ нет — только статичный спрайт из карты (`crates/ss13_mapload/src/look.rs`), без механики

### Статус-дисплеи
Настенные дисплеи: таймер эвакуационного шаттла, сообщения с консоли связи, картинки (тревога, биоугроза, логотипы), грузовой шаттл, дисплеи ИИ (эмоции), случайные сообщения, гринскрин.

- **Код:** [`/obj/machinery/status_display`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/status_display.dm#L14), [`/obj/machinery/status_display/evac`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/status_display.dm#L378), [`/obj/machinery/status_display/supply`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/status_display.dm#L666), [`/obj/machinery/status_display/ai`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/status_display.dm#L730), [`/obj/machinery/greenscreen_camera`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/status_display.dm#L1008)
- **Статус в ss13-rs:** ❌ нет — только статичный спрайт из карты (`crates/ss13_mapload/src/look.rs`), без механики

### Система автоматических объявлений (AAS)
Машина, объявляющая по радио о прибытии экипажа, новых главах, исследованных узлах, экстренных запросах и пр.; настраиваемые шаблоны, емаг/EMP портит объявления.

- **Код:** [`/obj/machinery/announcement_system`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/announcement_system.dm#L3), [`/datum/aas_config_entry`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/announcement_system.dm#L239), [`proc/announce()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/announcement_system.dm#L213); tgui: [`AutomatedAnnouncement.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AutomatedAnnouncement.tsx)
- **Статус в ss13-rs:** ❌ нет

### Голопады и голозвонки
Голопад вызывает другой голопад (голограмма звонящего, разговор), ИИ проецирует голограмму; защищённые голопады; запись и воспроизведение голодисков.

- **Код:** [`/obj/machinery/holopad`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/hologram.dm#L36), [`/datum/holocall`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/holocall.dm#L17), [`/obj/item/disk/holodisk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/holocall.dm#L215); tgui: [`Holopad.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Holopad.jsx)
- **BandaStation:** [`hologram.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/hologram.dm#L865) — склонение имени в записи
- **Статус в ss13-rs:** ❌ нет
