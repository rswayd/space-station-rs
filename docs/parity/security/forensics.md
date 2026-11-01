# Криминалистика
Следы на предметах (отпечатки, волокна, кровь) и инструменты детектива для их сбора и анализа.

### Отпечатки пальцев, волокна и кровь (forensics datum)
Касание предмета оставляет отпечатки (без перчаток) или волокна перчаток/одежды; кровь оставляет ДНК; скрытые отпечатки (ckey) пишутся для админов. Мытьё/очистка стирает следы, следы переносятся при крафте и смене предмета.

- **Код:** [`/datum/forensics`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/forensics/_forensics.dm#L10), [`proc/add_fingerprint()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/forensics/_forensics.dm#L120), [`proc/add_fibers()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/forensics/_forensics.dm#L157), [`proc/clean_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/forensics/_forensics.dm#L99), [`/atom/proc/add_fingerprint()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/forensics/forensics_helpers.dm#L14), [`/atom/proc/transfer_fingerprints_to()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/forensics/forensics_helpers.dm#L252)
- **Статус в ss13-rs:** ❌ нет

### Скрытые отпечатки для админов
Отдельный список «hiddenprints» с ckey и временем последнего касания — админы видят, кто трогал предмет (панель переменных / осмотр).

- **Код:** [`proc/add_hiddenprint()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/forensics/_forensics.dm#L201), [`/atom/proc/add_hiddenprint()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/forensics/forensics_helpers.dm#L67)
- **Статус в ss13-rs:** ❌ нет

### Кровь на предметах и одежде
Предметы и одежда со следами крови получают оверлей цвета крови (по ДНК/виду), видимый при осмотре; кровь смывается.

- **Код:** [`proc/get_visible_blood()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/forensics/_forensics.dm#L247), [`/atom/proc/get_blood_dna_color()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/forensics/forensics_helpers.dm#L76)
- **Статус в ss13-rs:** ❌ нет — крови нет (см. ../medicine/)

### Криминалистический сканер детектива
Сканирует соседний предмет или моба: отпечатки, кровь (ДНК и тип), волокна, реагенты, доступы ID, активные настройки, святые данные, нелегальные технологии; ведёт журнал сканов в окне tgui, печатает отчёт на бумаге, журнал можно очистить.

- **Код:** [`/obj/item/detective_scanner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/detectivework/scanner.dm#L3), [`proc/scan()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/detectivework/scanner.dm#L94), [`proc/print_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/detectivework/scanner.dm#L39), [`/datum/detective_scan_category`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/detectivework/detective_scan_category.dm#L15), [`/datum/detective_scanner_log`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/detectivework/detective_scanner_data/detective_scanner_log_entry.dm#L1); tgui: [`ForensicScanner`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ForensicScanner)
- **Статус в ss13-rs:** ❌ нет

### Пакет для улик
Вмещает один небольшой предмет, сохраняя его следы; содержимое видно на спрайте пакета и при осмотре.

- **Код:** [`/obj/item/evidencebag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/detectivework/evidence.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Доска детектива
Настенная доска: на неё прикрепляются предметы и фото как улики в нескольких делах (вкладки), улики соединяются нитями; окно tgui с перетаскиванием.

- **Код:** [`/obj/structure/detectiveboard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/detectiveboard.dm#L9); tgui: [`DetectiveBoard`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/DetectiveBoard)
- **Статус в ss13-rs:** ❌ нет

### Подделыватель улик (forensics spoofer)
Устройство синдиката: снимает отпечатки и волокна с предмета и подкидывает их на другие.

- **Код:** [`/obj/item/forensics_spoofer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/forensicsspoofer.dm#L1); tgui: [`ForensicsSpoofer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ForensicsSpoofer.tsx)
- **Статус в ss13-rs:** ❌ нет
