# База данных
Подключение к MySQL/MariaDB (`SSdbcore`), схема tg и таблицы BandaStation, записи о раундах, игроках, подключениях, смертях и статистике.

### Подсистема dbcore
Подключение через rust-g, асинхронные запросы с параметрами, пул, `format_table_name` с префиксом, проверка версии схемы, переподключение (админ-верб), встроенный демон БД для разработки.

- **Код:** [`SUBSYSTEM_DEF(dbcore)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dbcore.dm#L2), [`/datum/controller/subsystem/dbcore/proc/Connect()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dbcore.dm#L250), [`/datum/controller/subsystem/dbcore/proc/NewQuery()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dbcore.dm#L381), [`/datum/controller/subsystem/dbcore/proc/CheckSchemaVersion()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dbcore.dm#L304), [`/datum/controller/subsystem/dbcore/proc/start_db_daemon()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dbcore.dm#L508)
- **Статус в ss13-rs:** ❌ нет — БД нет (`grep -ri "sql\|database" crates` пусто)

### Схема tg
Таблицы `player`, `connection_log`, `round`, `ban`, `messages`, `role_time`/`role_time_log`, `poll_*`, `library`, `death`, `feedback`, `manifest`, `achievements`, `stickyban*`, `ipintel`, `discord_links`, `known_alts`, `admin*`, `ticket`, `telemetry_connections`, `fish_progress`, `pda_themes_progress`, `tutorial_completions`, `citation` и др.; журнал изменений схемы.

- **Код:** [`tgstation_schema.sql`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/SQL/tgstation_schema.sql#L1), [`database_changelog.md`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/SQL/database_changelog.md#L1)
- **Статус в ss13-rs:** ❌ нет — БД нет

### Таблицы BandaStation
Отдельная ревизия схемы `schema_revision_220` с проверкой при старте, таблицы `ckey_whitelist`, `admin_wl`, `budget`, `minesweeper`, `admin_prime`.

- **Код:** [`bandastation_update.sql`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/SQL/bandastation/bandastation_update.sql#L1), [`/datum/controller/subsystem/dbcore/CheckSchemaVersion()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/database220/code/database220.dm#L14)
- **BandaStation:** [`_database220.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/database220/_database220.dm#L1)
- **Статус в ss13-rs:** ❌ нет — БД нет

### Раунды
Запись раунда (id, время начала/конца, карта, результат, станция) — номер раунда в логах и интерфейсах.

- **Код:** [`/datum/controller/subsystem/dbcore/proc/InitializeRound()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dbcore.dm#L325), [`/datum/controller/subsystem/dbcore/proc/SetRoundStart()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dbcore.dm#L340), [`/datum/controller/subsystem/dbcore/proc/SetRoundEnd()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dbcore.dm#L350)
- **Статус в ss13-rs:** ❌ нет — БД нет

### Игроки и подключения
Запись игрока (первое/последнее появление, IP, CID, возраст аккаунта BYOND), лог подключений, валидация ключа, телеметрия подключений для поиска мультов.

- **Код:** [`/client/proc/set_client_age_from_db()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/client_procs.dm#L668), [`/client/proc/validate_key_in_db()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/client_procs.dm#L828), [`telemetry.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_panel/telemetry.dm#L1)
- **Статус в ss13-rs:** ❌ нет — клиент идентифицируется токеном без учёта

### Наигранное время
Учёт времени по ролям/отделам/призраку (`role_time`), используется для требований профессий и наград (плащ ветерана).

- **Код:** [`/client/proc/set_exp_from_db()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_exp.dm#L82), [`/client/proc/update_exp_list()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_exp.dm#L146)
- **Статус в ss13-rs:** ❌ нет — БД нет

### Blackbox и feedback
`SSblackbox`: статистика раунда (тэги, счётчики, вложенные списки) в таблицу `feedback`; запись смертей (`death`), манифеста, популяции.

- **Код:** [`SUBSYSTEM_DEF(blackbox)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/blackbox.dm#L1), [`/datum/controller/subsystem/blackbox/proc/record_feedback()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/blackbox.dm#L231)
- **Статус в ss13-rs:** ❌ нет — БД нет

### Опросы
Опросы игроков в лобби (выбор, множественный, рейтинг, текст, IRV), админ-управление опросами.

- **Код:** [`/mob/dead/new_player/proc/handle_player_polling()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/new_player/poll.dm#L14), [`/datum/poll_question`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/poll_management.dm#L7), [`/datum/admins/proc/poll_list_panel()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/poll_management.dm#L73)
- **Статус в ss13-rs:** ❌ нет — БД нет

### Библиотека
Книги игроков в БД: загрузка, поиск, печать, скрытие админами; случайные книги на полках.

- **Код:** [`/obj/machinery/computer/libraryconsole`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/lib_machines.dm#L21), [`SUBSYSTEM_DEF(library)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/library.dm#L2), [`admin_only.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/admin_only.dm#L1)
- **Статус в ss13-rs:** ❌ нет (машины библиотеки — см. ../service/)

### Достижения в БД
Хранение наград и счётов игроков, метаданные достижений (см. ../round/holidays-achievements.md).

- **Код:** [`SUBSYSTEM_DEF(achievements)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/achievements.dm#L1)
- **Статус в ss13-rs:** ❌ нет — БД нет

### Привязки Discord
Таблица `discord_links` и подсистема `SSdiscord` (токены привязки, уведомления).

- **Код:** [`SUBSYSTEM_DEF(discord)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/discord.dm#L31)
- **Статус в ss13-rs:** ❌ нет — БД нет

### IP-интел и стикибаны
`SSipintel`: проверка IP на VPN/прокси через внешний сервис с кэшем в БД и белым списком; `SSstickyban`: загрузка стикибанов из БД в BYOND.

- **Код:** [`SUBSYSTEM_DEF(ipintel)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ipintel.dm#L1), [`/datum/controller/subsystem/ipintel/proc/query_address()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ipintel.dm#L81), [`SUBSYSTEM_DEF(stickyban)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/stickyban.dm#L1)
- **Статус в ss13-rs:** ❌ нет — БД нет
