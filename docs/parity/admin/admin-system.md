# Админ-система
Права администраторов, ранги, регистрация вербов, админ-чаты и инфраструктура BandaStation (центральный сервер, Discord, анти-рейд).

### Админы, ранги и права
`/datum/admins` у клиента: ранги из БД или `admins.txt`, флаги прав (R_ADMIN, R_BAN, R_DEBUG, R_FUN, R_SERVER, R_SOUND, R_SPAWN, R_VAREDIT, ...), деадмин/реадмин, стелс-режим, авто-деадмин при входе в роль.

- **Код:** [`/datum/admins`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/holder2.dm#L12), [`/datum/admin_rank`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/admin_ranks.dm#L7), [`admin_ranks.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/admin_ranks.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin` (`rights.rs`, `server.rs`): `admin_ranks.txt`/`admins.txt` в формате BandaStation (Include/Exclude/Edit, `@`, `EVERYTHING`, ранги через `+`), флаги `R_*` тех же битов, проверка прав на сервере при каждом запросе, деадмин/реадмин, `AUTOADMIN`. Идентичность — ckey токена (настоящей авторизации нет). Нет загрузки из БД, стелс-режима, авто-деадмина при входе в роль

### Панель прав
Редактирование админов и рангов (добавить, удалить, сменить ранг и флаги), с записью в БД.

- **Код:** [`/datum/admins/proc/edit_admin_permissions()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/permissionedit.dm#L18)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Регистрация вербов и панель вербов
`ADMIN_VERB` регистрирует ~380 вербов по категориям (Main, Game, Fun, Events, Server, Debug, Mapping, Shuttle, Profile...), скрытие вербов; TGUI-панель «Admin Verb Panel» с поиском и выбором цели.

- **Код:** [`/datum/admin_verb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verb_datums/_admin_verb_datum.dm#L8), [`/datum/admin_verb_panel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/admin_verb_panel.dm#L12), [`admin_verbs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/admin_verbs.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin`: фиксированный набор вербов в админ-панели (egui, F7) с кнопками по правам; нет ~380 вербов, категорий и поиска TGUI-панели; нет админ-режима меню языков (выдать/забрать язык моба, omnitongue — перенесено из `ui/languages.md`, окно `crates/ss13_game/src/language_menu.rs`)

### Админ-чаты
ASay (админы), MSay (менторы, BandaStation), DSay (в мёртвый чат), Announce (всем игрокам), админ-уведомление (adminnotice).

- **Код:** [`adminsay.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/adminsay.dm#L1), [`deadsay.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/deadsay.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin`: ASay (`ADMIN: key: текст`, всем с `+ADMIN`, в лог); нет MSay, DSay, OOC-Announce и adminnotice

### Админ-хоткеи и вспомогательное (BandaStation)
Свои хоткеи админов, «Global Man Up», проигрывание звука на z-уровне и в поле зрения, свои секреты.

- **Код:** [`admin_hotkeys.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/admin/code/admin_hotkeys.dm#L1), [`admin_verbs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/admin/code/admin_verbs.dm#L1), [`admin_secrets.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/admin/code/admin_secrets.dm#L1)
- **BandaStation:** [`_admin.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/admin/_admin.dm#L1)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Анти-рейд контроллер (BandaStation)
Админ-панель «Raider Controller» для защиты от рейдов (ограничения взрывов и др.).

- **Код:** [`raider_controller.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/admin/code/raider_controller.dm#L1)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Панический бункер и вход
Panic bunker (новые ckey не пускаются или идут на интервью), запрет гостей, переключатели входа/респавна/OOC/хаба.

- **Код:** [`panicbunker.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/panicbunker.dm#L1), [`server.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/server.dm#L1)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Центральный сервер SS220 (BandaStation)
Подсистема `central`: связь с метасервером — привязка Discord, белый список (с банами по нему), уровень доната, запросы банов на центральный сервер.

- **Код:** [`SUBSYSTEM_DEF(central)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/metaserver/code/ss_central.dm#L12), [`/datum/controller/subsystem/central/proc/is_player_whitelisted()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/metaserver/code/ss_central.dm#L127), [`/datum/controller/subsystem/central/proc/update_player_donate_tier_async()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/metaserver/code/ss_central.dm#L204)
- **BandaStation:** [`_metaserver.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/metaserver/_metaserver.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Привязка Discord и вебхуки (BandaStation)
Обязательная верификация Discord (OAuth в лобби), вебхуки интервью и конца раунда (с пингом роли), верб «discord» со ссылкой.

- **Код:** [`discord_link.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/metaserver/code/discord_link.dm#L1), [`interview.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/metaserver/code/interview.dm#L1), [`discord.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/discord/code/discord.dm#L1)
- **BandaStation:** [`_discord.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/discord/_discord.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Особенности сервера Prime (BandaStation)
Изменения только для сервера Prime: вендомат бодикамер, аутфит Apex, генератор рыболовного портала, оружие, событийная роль «выживший кхаарут» (инфекция, улей, способности).

- **Код:** [`modular_bandastation/prime_only/code`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/prime_only/code), [`/datum/status_effect/khaaroot_survivor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/prime_only/code/khaaroot_survivor/status_effect.dm#L7)
- **BandaStation:** [`_prime_only.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/prime_only/_prime_only.dm#L1)
- **Статус в ss13-rs:** ❌ нет
