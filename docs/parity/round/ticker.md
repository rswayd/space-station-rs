# Тикер и ход раунда
Подсистема `SSticker`: состояния игры, отсчёт лобби, старт раунда, итоговый отчёт и перезагрузка мира.

### Состояния игры и отсчёт лобби
`SSticker` проходит STARTUP → PREGAME (отсчёт `lobby_countdown`, по умолчанию 120 с) → SETTING_UP → PLAYING → FINISHED; без готовых игроков старт откладывается.

- **Код:** [`SUBSYSTEM_DEF(ticker)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L4), [`/datum/controller/subsystem/ticker/fire()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L122), [`/datum/controller/subsystem/ticker/proc/check_finished()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L195)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/round.rs` (`Ticker`, `GameState`, `SS13_LOBBY_SECONDS`)

### Подготовка раунда
`setup`: распределение профессий, Dynamic, создание персонажей, экипировка, перенос в тела в один тик, `PostSetup` (антагонисты, станционные трейты, объявления).

- **Код:** [`/datum/controller/subsystem/ticker/proc/setup()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L213), [`/datum/controller/subsystem/ticker/proc/PostSetup()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L287), [`/datum/controller/subsystem/ticker/proc/create_characters()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L465), [`/datum/controller/subsystem/ticker/proc/equip_characters()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L485), [`/datum/controller/subsystem/ticker/proc/transfer_characters()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L555)
- **BandaStation:** [`ticker.dm#L295`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L295) — назначение операций антагонистов после раздачи ролей
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/round.rs`, `crates/ss13_game/src/antag/dynamic.rs`: распределение, спавн на `/obj/effect/landmark/start`, предатели; нет станционных трейтов, объявлений, контракта найма, ИИ/киборгов

### Отчёт о вышедших на старте (BandaStation)
Через некоторое время после старта админам приходит список игроков, вышедших/SSD/мёртвых в начале раунда, с записью в blackbox.

- **Код:** [`/datum/controller/subsystem/ticker/proc/display_roundstart_logout_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L358)
- **BandaStation:** [`ticker.dm#L357`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L357) — отчёт через `/datum/roundstart_logout_report_builder`
- **Статус в ss13-rs:** ❌ нет — админов нет

### Переоткрытие ролей самоубийц
Если игрок убил себя в начале раунда, его профессия снова открывается для латджойна.

- **Код:** [`/datum/controller/subsystem/ticker/proc/reopen_roundstart_suicide_roles()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L414)
- **Статус в ss13-rs:** ❌ нет

### Итоговый отчёт раунда
`declare_completion`: личный отчёт (выжил/сбежал), отчёт выживших (статистика), законы ИИ, цели станции, рынок, медали, антагонисты, достижения; окно отчёта и кнопка «Показать отчёт»; лог в файл.

- **Код:** [`/datum/controller/subsystem/ticker/proc/declare_completion()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L214), [`/datum/controller/subsystem/ticker/proc/personal_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L426), [`/datum/controller/subsystem/ticker/proc/survivor_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L344), [`/datum/controller/subsystem/ticker/proc/build_roundend_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L317), [`/datum/action/report`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L681)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/round.rs` (`personal_report` + `survivor_report` в чат), `crates/ss13_game/src/antag/traitor.rs`, `crates/ss13_game/src/cargo/economy.rs` (`market_report`: всего у экипажа, среднее, самый богатый): нет законов, целей станции, медалей, достижений, окна отчёта

### Новостной отчёт о судьбе станции
В конце раунда — «новость» о судьбе станции (эвакуация, уничтожение ядеркой, победа культа/революции и т.д.).

- **Код:** [`/datum/controller/subsystem/ticker/proc/send_news_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L657)
- **Статус в ss13-rs:** ❌ нет

### Перезагрузка мира
После конца раунда — отсчёт `round_end_countdown`, таймер на экране, возможность отмены админом, перезагрузка и автопереподключение клиентов.

- **Код:** [`/datum/controller/subsystem/ticker/proc/Reboot()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L774), [`/datum/controller/subsystem/ticker/proc/cancel_reboot()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L822), [`/datum/controller/subsystem/ticker/proc/standard_reboot()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L307), [`/atom/movable/screen/reboot_timer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L767)
- **BandaStation:** [`ticker.dm#L781`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L781) — учёт голосования о конце раунда
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/round.rs` (`SS13_ROUND_END_SECONDS`, новый `App`, переподключение): нет таймера на экране и отмены

### Музыка лобби и конца раунда
Случайная музыка в лобби (с учётом праздника) и песня конца раунда.

- **Код:** [`/datum/controller/subsystem/ticker/proc/set_lobby_music()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L853), [`/datum/controller/subsystem/ticker/proc/choose_round_end_song()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L842), [`/datum/controller/subsystem/ticker/proc/SetRoundEndSound()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L763)
- **Статус в ss13-rs:** ❌ нет — звуков нет

### Очередь на сервер (popcap)
При превышении лимита игроков новые игроки ждут в очереди лобби.

- **Код:** [`/datum/controller/subsystem/ticker/proc/check_queue()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L576), [`/datum/controller/subsystem/job/proc/popcap_reached()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L692)
- **Статус в ss13-rs:** ❌ нет

### Хардкорный режим
Рандомный «хардкор» с очками за набор квирков; счёт выживших хардкорщиков.

- **Код:** [`/datum/controller/subsystem/ticker/proc/HandleRandomHardcoreScore()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L185), [`/datum/controller/subsystem/ticker/proc/hardcore_random_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L597)
- **Статус в ss13-rs:** ❌ нет
