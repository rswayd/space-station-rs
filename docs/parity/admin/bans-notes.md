# Баны, заметки и доступ
Баны (серверные, по ролям, стикибаны), заметки и наблюдение, белый список, известные мульты. Хранение в БД — см. ../persistence-sql/.

### Проверка при входе
`world/IsBanned`: баны по ckey/IP/CID, стикибаны, гостевой доступ, panic bunker, лимит подключений; сообщение с причиной и сроком.

- **Код:** [`/world/IsBanned()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/IsBanned.dm#L9), [`is_banned_from()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/sql_ban_system.dm#L17)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin`: забаненный ckey кикается сразу после рукопожатия (не отказом в нём). Нет panic bunker, проверок стикибанов, CID/IP

### Панель банов
Создание и редактирование бана: сервер или роли (джоббаны по отделам, антаг-баны), срок/перма, по ckey/IP/CID, применение к связанным аккаунтам; панель разбанов с историей.

- **Код:** [`/datum/admins/proc/ban_panel()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/sql_ban_system.dm#L187), [`/datum/admins/proc/create_ban()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/sql_ban_system.dm#L549), [`/datum/admins/proc/unban_panel()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/sql_ban_system.dm#L652)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin`: бан и разбан ckey с причиной (`+BAN`), кик со сменой окна клиента без переподключения — в памяти до перезагрузки раунда (SQL нет). Нет панели, сроков, ролевых банов, БД

### Джоббаны и кэш банов
Проверка бана на роль при выборе профессии/антага, кэш банов клиента.

- **Код:** [`build_ban_cache()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/sql_ban_system.dm#L151), [`retrieve_ban_cache()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/sql_ban_system.dm#L125)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Стикибаны
Бан, «прилипающий» к новым аккаунтам с тех же IP/CID; панель стикибанов (исключения, сопоставления).

- **Код:** [`/datum/admins/proc/stickyban()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/stickyban.dm#L1), [`restore_stickybans()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/IsBanned.dm#L253)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Заметки, сообщения и вотчлист
Админ-заметки о игроке (с серьёзностью и сроком), сообщения игроку при входе, вотчлист (уведомление админам о входе), memo; игрок видит свои заметки («View Admin Remarks»).

- **Код:** [`create_message()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/sql_message_system.dm#L1), [`browse_messages()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/sql_message_system.dm#L370), [`get_message_output()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/sql_message_system.dm#L645)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Известные мульты и белый список
Known Alts Panel (связанные аккаунты), белый список ckey (`whitelist.txt` / центральный сервер BandaStation).

- **Код:** [`/datum/known_alts`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/known_alts.dm#L3), [`load_whitelist()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/whitelist.dm#L5)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Политика сервера
Policy Panel: тексты правил для ролей (показываются при получении роли и по вербу «Show Policy»).

- **Код:** [`policy_panel.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/policy_panel.dm#L1)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет
