# Записи охраны и HUD
Записи экипажа и преступлений, консоли записей и ордеров, плакаты розыска, значки охранного HUD.

### Записи экипажа (crew records)
При появлении каждого члена экипажа создаётся запись: имя, возраст, должность, вид, отпечатки, ДНК, фото анфас/профиль, статус розыска, преступления и штрафы, заметки; закрытые записи (`locked`) хранят данные для клонирования/антагов. Список ведёт глобальный манифест (окно манифеста экипажа — см. ../round/ или раздел интерфейса).

- **Код:** [`/datum/record`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/records/record.dm#L4), [`/datum/record/crew`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/records/record.dm#L71), [`/datum/record/locked`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/records/record.dm#L146), [`/datum/manifest/proc/inject()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/records/manifest.dm#L106)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/records/manifest.rs`: при входе в раунд создаются запись экипажа и закрытая запись (имя, должность из трима карты, случайные хэши отпечатков и ДНК как у tg, группа крови), консоль ID пишет в запись должность (`modify`), программа «Plexagon Crew List» читает записи по отделам; возраст, вид и пол — заглушки (нет настроек персонажа), нет фото/внешности и квирков

### Преступления и штрафы (crime, citation)
Преступление — запись с названием, деталями, автором и временем; может быть аннулировано; штраф (citation) имеет сумму, которую нарушитель оплачивает, а владельцу приходит уведомление на PDA.

- **Код:** [`/datum/crime`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/records/crime.dm#L4), [`/datum/crime/citation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/records/crime.dm#L24), [`proc/pay_fine()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/records/crime.dm#L36), [`proc/alert_owner()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/records/crime.dm#L51)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/records/manifest.rs`, `records/console.rs`: преступления и штрафы с автором и временем раунда, изменение и аннулирование (автор или доступ к оружейной); нет оплаты штрафа и уведомления на КПК

### Консоль записей охраны
Вход по ID с доступом охраны; просмотр записей с превью персонажа, добавление/редактирование/аннулирование преступлений и штрафов, статус розыска (Нет, Подозрение, Арест, Заключение, УДО, Оправдано), заметки, удаление записи, очистка базы, печать: досье, плакат «Разыскивается», плакат «Пропал»; ноутбук и синдикатский вариант. Медицинская консоль записей — тот же базовый тип (см. ../medical/medical-machines.md).

- **Код:** [`/obj/machinery/computer/records`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/records/records.dm#L4), [`/obj/machinery/computer/records/security`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/records/security.dm#L8), [`proc/add_crime()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/records/security.dm#L207), [`proc/print_record()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/records/security.dm#L337); tgui: [`SecurityRecords`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SecurityRecords)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/records/console.rs`, `records/client.rs`: вход по ID (охрана или ГП), статусы розыска (Арест требует преступления), преступления и штрафы, заметка и поля записи, удаление, очистка (доступ к оружейной, сразу — у tg 5 с); нет превью персонажа, печати, новых записей по фото, порчи записей ЭМИ

### Консоль ордеров (warrant console)
Публичная консоль: любой видит список штрафов и ордеров экипажа, может оплатить штраф (деньгами с карты/голочипом) и распечатать ордер.

- **Код:** [`/obj/machinery/computer/warrant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/warrant.dm#L1), [`proc/pay_fine()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/warrant.dm#L81); tgui: [`WarrantConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/WarrantConsole.tsx)
- **Статус в ss13-rs:** ❌ нет

### Плакаты розыска
Печатаемый плакат «Разыскивается»/«Пропал» с фото из записи, именем и описанием; вешается на стену как обычный плакат.

- **Код:** [`/obj/item/poster/wanted`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/wanted_poster.dm#L10), [`/obj/structure/sign/poster/wanted`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/wanted_poster.dm#L41)
- **Статус в ss13-rs:** ❌ нет

### Охранный HUD (значки над мобами)
Охранный data HUD (очки, импланты, киборги, ИИ) показывает над людьми значок должности по ID, статус розыска из записей и значки имплантов (майндщит, трекер, химический, ...). Продвинутый вариант — с имплантами; синие/фиолетовые значки как у tg.

- **Код:** [`/datum/atom_hud/data/human/security`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/data_huds.dm#L33), [`proc/update_ID_card()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/data_huds.dm#L265), [`proc/sec_hud_set_implants()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/data_huds.dm#L277), [`proc/sec_hud_set_security_status()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/data_huds.dm#L298)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/records/hud.rs`: носящий очки охранного HUD (по типу очков) видит над людьми значок должности по триму надетой карты и значок розыска из записи (headless tgui `DataHud`, только носителю); нет имплантов, HUD киборгов/ИИ и имплантов-HUD, запись ищется по настоящему имени (у tg — по лицу)

### Изменение статуса через HUD при осмотре
Носящий охранный HUD при осмотре человека видит его статус розыска и ссылки, по которым из чата меняет статус, добавляет преступление, штраф или заметку прямо в запись.

- **Код:** [`/mob/living/carbon/human/Topic()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/human.dm#L92)
- **Статус в ss13-rs:** ❌ нет
