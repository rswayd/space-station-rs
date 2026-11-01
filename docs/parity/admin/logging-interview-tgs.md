# Логи, интервью, Discord и TGS
Игровые логи и расследования, интервью новичков, интеграция с Discord через TGS и дополнительные world topic BandaStation.

### Игровые логи
Категории логов (game, attack, say, admin, href, uplink, PDA, silo, target zone...), запись в файлы раунда и JSON, логирование действий на атоме (`log_message`), индивидуальные логи моба.

- **Код:** [`/datum/log_holder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/logging/log_holder.dm#L5), [`/datum/log_category`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/logging/log_category.dm#L2), [`/datum/log_entry`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/logging/log_entry.dm#L8), [`/atom/proc/log_message()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/logging/_logging.dm#L98), [`log_game()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/logging/game.dm#L2), [`individual_logging.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/individual_logging.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — серверный лог через `tracing`; админ-лог `crates/ss13_admin` (`SS13_ADMIN_LOG`, строки `[UTC] ADMIN/ASAY/ADMINPRIVATE/ACCESS: ...`). Нет остальных категорий tg, файлов раунда, индивидуальных логов

### Расследования и получение логов
Investigate (журналы по темам: двигатель, отравления, ботаника, ...), Get Server Logs / Get Current Logs (скачать логи).

- **Код:** [`/atom/proc/investigate_log()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/admin_investigate.dm#L1), [`investigate_show`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/admin_investigate.dm#L15), [`getlogs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/getlogs.dm#L1)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Интервью новичков
При panic bunker новые игроки заполняют анкету (вопросы из конфига); админы одобряют или отклоняют; BandaStation — только с привязанным Discord и вебхук в Discord.

- **Код:** [`/datum/interview`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/interview/interview.dm#L9), [`/datum/interview_manager`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/interview/interview_manager.dm#L9); tgui: [`Interview.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Interview.tsx)
- **BandaStation:** [`interview.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/metaserver/code/interview.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Discord: привязка аккаунта и уведомления
Привязка Discord-аккаунта (одноразовый токен), подписка на уведомление о новом раунде, эмбеды.

- **Код:** [`accountlink.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/discord/accountlink.dm#L1), [`toggle_notify.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/discord/toggle_notify.dm#L1), [`discord_embed.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/discord/discord_embed.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### TGS и чат-команды
Интеграция с tgstation-server (перезапуски, обновления, события раунда), чат-команды из Discord: check, gameversion, notify, ahelp, namecheck, adminwho, sdql, reload_admins.

- **Код:** [`code/modules/tgs`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgs), [`/datum/tgs_chat_command`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__DEFINES/tgs.dm#L331), [`tgs_commands.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/discord/tgs_commands.dm#L1), [`chat_commands.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/chat_commands.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Дополнительные world topic (BandaStation)
Топики для внешних сервисов: список игроков (`playerlist`), сброс TTS (`fixtts`). Базовые world topic — см. ../server/.

- **Код:** [`/datum/world_topic/playerlist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/world_topics/code/world_topics.dm#L16), [`/datum/world_topic/fixtts`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/world_topics/code/world_topics.dm#L1)
- **BandaStation:** [`_world_topics.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/world_topics/_world_topics.dm#L1)
- **Статус в ss13-rs:** ❌ нет
