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
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/round.rs`, `crates/ss13_game/src/antag/dynamic.rs`: распределение, спавн на `/obj/effect/landmark/start`, предатели, станционные трейты (`crates/ss13_game/src/station_traits.rs`); нет объявлений, контракта найма, ИИ/киборгов

### Отчёт о вышедших на старте (BandaStation)
Через некоторое время после старта админам приходит список игроков, вышедших/SSD/мёртвых в начале раунда, с записью в blackbox.

- **Код:** [`/datum/controller/subsystem/ticker/proc/display_roundstart_logout_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L358)
- **BandaStation:** [`ticker.dm#L357`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L357) — отчёт через `/datum/roundstart_logout_report_builder`
- **Статус в ss13-rs:** ✅ — `crates/ss13_admin/src/logout_report.rs` (`LogoutReport`, `PlayedBy`, `entries`): через 10 ± 3 мин после старта админы с `R_ADMIN` получают отчёт в чат и админ-лог («Disconnected», «Dying», «Dead»); тест `crates/ss13_admin/tests/admin.rs` (`the_roundstart_logout_report_lists_the_dead_and_the_disconnected`). Подробности и `// shortcut:` — [../bandastation/fork-misc.md](../bandastation/fork-misc.md); запись в blackbox — SQL, отложено ([../persistence-sql/database.md](../persistence-sql/database.md))

### Переоткрытие ролей самоубийц
Если игрок убил себя в начале раунда, его профессия снова открывается для латджойна.

- **Код:** [`/datum/controller/subsystem/ticker/proc/reopen_roundstart_suicide_roles()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L414)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/round.rs` (`reopen_roundstart_suicide_roles`, `ReopenSuicideRoles`, `Suicided`): с `SS13_REOPEN_SUICIDE_ROLES=1` (в tg флаг выключен по умолчанию) через `SS13_REOPEN_SUICIDE_ROLES_DELAY` (по умолчанию 240 с, не меньше 30) после старта каждая станционная профессия тела с `Suicided` получает слот обратно (кроме ИИ/киборга; командные — только с `SS13_REOPEN_SUICIDE_ROLES_COMMAND=1`), с `SS13_REOPEN_SUICIDE_ROLES_REPORT=1` ЦК печатает список на консолях связи; освобождённый слот не освобождается второй раз (`SlotReopened`). Тесты в модуле. Сам глагол `suicide`, ставящий `Suicided`, — группа [../mobs/life-and-health.md](../mobs/life-and-health.md) (🟡)

### Итоговый отчёт раунда
`declare_completion`: личный отчёт (выжил/сбежал), отчёт выживших (статистика), законы ИИ, цели станции, рынок, медали, антагонисты, достижения; окно отчёта и кнопка «Показать отчёт»; лог в файл.

- **Код:** [`/datum/controller/subsystem/ticker/proc/declare_completion()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L214), [`/datum/controller/subsystem/ticker/proc/personal_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L426), [`/datum/controller/subsystem/ticker/proc/survivor_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L344), [`/datum/controller/subsystem/ticker/proc/build_roundend_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L317), [`/datum/action/report`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L681)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/round.rs` (`personal_report` + `survivor_report` в чат), `crates/ss13_game/src/antag/traitor.rs`, `crates/ss13_game/src/cargo/economy.rs` (`market_report`: всего у экипажа, среднее, самый богатый), `crates/ss13_game/src/station_goals.rs` (`goal_report`): нет законов, медалей, достижений, окна отчёта

### Новостной отчёт о судьбе станции
В конце раунда — «новость» о судьбе станции (эвакуация, уничтожение ядеркой, победа культа/революции и т.д.).

- **Код:** [`/datum/controller/subsystem/ticker/proc/send_news_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L657)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/round_news.rs` (`round_result`, `news_message`, `send_news_report`, из `declare_completion`): результат раунда по `set_round_result` — культ (призыв, побег ≥ 50 %, провал), все волшебники мертвы, итог ядерных оперативников (`NUKE_RESULT_*`), революция, взрыв станции, эвакуация/угон шаттла — и новость Nanotrasen News Network о станции; тесты в модуле. `// shortcut:` связи между серверами (`send2otherserver`) нет — новость уходит в лог сервера; нет историй блоба и каскада суперматерии (механик нет), причины вызова шаттла нет (эвакуация без маяка). Отклонение: `STATION_NUKED` не перекрывает историю оперативников (баг tg, см. комментарий в коде)

### Перезагрузка мира
После конца раунда — отсчёт `round_end_countdown`, таймер на экране, возможность отмены админом, перезагрузка и автопереподключение клиентов.

- **Код:** [`/datum/controller/subsystem/ticker/proc/Reboot()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L774), [`/datum/controller/subsystem/ticker/proc/cancel_reboot()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L822), [`/datum/controller/subsystem/ticker/proc/standard_reboot()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L307), [`/atom/movable/screen/reboot_timer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L767)
- **BandaStation:** [`ticker.dm#L781`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L781) — учёт голосования о конце раунда
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/round.rs` (`SS13_ROUND_END_SECONDS`, новый `App`, переподключение): нет таймера на экране и отмены

### Музыка лобби и конца раунда
Случайная музыка в лобби (с учётом праздника) и песня конца раунда.

- **Код:** [`/datum/controller/subsystem/ticker/proc/set_lobby_music()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L853), [`/datum/controller/subsystem/ticker/proc/choose_round_end_song()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L842), [`/datum/controller/subsystem/ticker/proc/SetRoundEndSound()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L763)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/lobby_music.rs` (`login_music`, `play_title_music`, `play_round_end_song`), канал звука в протоколе (`ss13_shared::protocol::ChannelSound`, `CHANNEL_LOBBYMUSIC`; клиент — `crates/ss13_game/src/sound_client.rs`, `play_channels`): трек лобби на раунд из `strings/round_start_sounds.txt` (на 1 апреля — `clown.ogg`) играет в лобби и останавливается при входе в игру или наблюдении, при возврате в лобби играет снова; в конце раунда — всем на половине громкости; перед перезагрузкой — случайная песня из `config/reboot_themes/`. Тесты в модуле. `// shortcut:` нет музыки деплоя (`config/title_music/sounds/`, редкие и картовые треки), не исключается трек прошлого раунда, громкость — значения настроек по умолчанию; `title1.mod` не играется (клиент не декодирует трекерные модули)

### Очередь на сервер (popcap)
При превышении лимита игроков новые игроки ждут в очереди лобби.

- **Код:** [`/datum/controller/subsystem/ticker/proc/check_queue()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ticker.dm#L576), [`/datum/controller/subsystem/job/proc/popcap_reached()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L692)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/join_queue.rs` (`JoinQueue`, `join_button`, `may_select`, `step`), кнопка «Присоединиться» и выбор профессии — `crates/ss13_game/src/jobs.rs`: с `SS13_HARD_POPCAP`/`SS13_EXTREME_POPCAP` (в tg не заданы по умолчанию) игрок лобби при достижении лимита живых игроков (или при непустой очереди) встаёт в очередь с сообщением о позиции; выбрать профессию может только первый и только ниже лимита; раз в секунду `check_queue`: каждые 5 с первому сообщают о свободном месте (звук `notice1.ogg`, меню латджойна), через 25 с без входа он выбывает, ушедшие из лобби выбывают, снятие лимита отпускает всех. Тесты в модуле. `// shortcut:` админы ждут в очереди наравне со всеми (игровой крейт не знает админов); отказ в подключении по `extreme_popcap` — это редирект на запасной сервер, не портирован; нет уведомления `soft_popcap` при входе

### Хардкорный режим
Рандомный «хардкор» с очками за набор квирков; счёт выживших хардкорщиков.

- **Код:** [`/datum/controller/subsystem/ticker/proc/HandleRandomHardcoreScore()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L185), [`/datum/controller/subsystem/ticker/proc/hardcore_random_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L597)
- **Статус в ss13-rs:** ❌ нет
