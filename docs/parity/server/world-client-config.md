# Мир, подключение клиентов и конфигурация
Запуск и перезагрузка мира, внешние запросы (world topic), подключение клиентов и конфиг-файлы сервера.

### Запуск мира
`world/New`: загрузка конфига, логов (каталог раунда; BandaStation — логи «logis»), TGS, MC; статус в хабе BYOND; FPS/tick_lag.

- **Код:** [`/world/New()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/world.dm#L128), [`/world/proc/SetupLogs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/world.dm#L227), [`/world/proc/update_status()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/world.dm#L397), [`/world/proc/change_fps()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/world.dm#L501)
- **BandaStation:** [`world.dm#L233`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/world.dm#L233) — каталог логов «logis»
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_server`, `crates/ss13_game/src/server.rs` (загрузка карты и ассетов из `SS13_ASSETS`/`SS13_MAP`, привязка к `SS13_BIND_ADDR`): нет хаба, логов раунда, TGS

_«Перезагрузка мира» описано в [../round/ticker.md](../round/ticker.md)._

### World topic
Внешние запросы к серверу: ping, playing, status (для хаба/ботов), ahelp_relay и adminmsg (межсерверные/Discord), comms_console, news, namecheck, adminwho, PR-анонсы; BandaStation — playerlist и fixtts (см. ../admin/).

- **Код:** [`/world/Topic()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/world.dm#L282), [`/datum/world_topic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/world_topic.dm#L22), [`/datum/world_topic/status`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/world_topic.dm#L213), [`/datum/world_topic/ahelp_relay`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/world_topic.dm#L91)
- **Статус в ss13-rs:** ❌ нет

### Подключение клиента
`client/New`: проверки версии BYOND (минимальная, предупреждение, чёрный список билдов), гостей, банов, panic bunker, лимита подключений; загрузка префов, ассетов, чата; приветствие и MOTD.

- **Код:** [`/client/New()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/client_procs.dm#L257), [`/client/Del()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/client_procs.dm#L609)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_net/src/server.rs`, `crates/ss13_net/src/lib.rs` (рукопожатие с версией протокола и токеном, отказ с причиной, backpressure): нет банов, гостевых правил, лимита игроков, MOTD

### Система конфигурации
`/datum/controller/configuration`: типизированные записи `config_entry` (флаги, числа, строки, списки) из `config/*.txt`, защита записей, перезагрузка, `CONFIG_GET`/`CONFIG_SET`; BandaStation — свой `config/bandastation/` (конфиг, профессии, веса антагов, фильтр брейнрота, события, TTS-замены).

- **Код:** [`/datum/controller/configuration`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/configuration/configuration.dm#L1), [`/datum/config_entry`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/configuration/config_entry.dm#L1), [`/datum/controller/configuration/proc/LoadEntries()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/configuration/configuration.dm#L176), [`config/bandastation`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/config/bandastation)
- **Статус в ss13-rs:** 🟡 частично — переменные окружения `SS13_*` (`SS13_LOBBY_SECONDS`, `SS13_ROUND_END_SECONDS`, `SS13_MAP`...), значения tg захардкожены константами (например, `crates/ss13_game/src/round.rs`); файлов `config/` не читает

### Игровые опции
`game_options.txt`: скорости бега/ходьбы, число офицеров, вероятности событий, задержки, разрешения рас, лимиты и т.д.

- **Код:** [`game_options.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/configuration/entries/game_options.dm#L1), [`game_options.txt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/config/game_options.txt#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/lib.rs` (`RUN_DELAY_SECS` = `run_delay`), `crates/ss13_game/src/round.rs` (офицеры): несколько значений перенесены константами

### Общие опции сервера
`config.txt`: имя сервера, хаб, ссылки (вики, форум, GitHub), лимиты клиентов, версии BYOND, AFK, panic bunker, таймауты, логирование.

- **Код:** [`general.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/configuration/entries/general.dm#L1), [`config.txt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/config/config.txt#L1)
- **Статус в ss13-rs:** ❌ нет

### Конфиги контента
`jobconfig.toml` (слоты профессий), `maps.txt`/`map_vote.txt` (ротация карт), `silicon_laws.txt`, `events.json`, `word_filter.toml` (IC/OOC-фильтр), `motd.txt`, `policy.json`, музыка и экраны лобби, темы перезагрузки.

- **Код:** [`jobs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/configuration/entries/jobs.dm#L1), [`is_ic_filtered()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/chat_filter.dm#L7), [`jobconfig.toml`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/config/jobconfig.toml#L1), [`word_filter.toml`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/config/word_filter.toml#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/jobs.rs` (слоты профессий берутся из DM-данных, а не из `jobconfig.toml`): фильтра слов, MOTD, ротации карт нет
