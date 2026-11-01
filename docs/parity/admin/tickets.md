# Тикеты и личные сообщения
Админ-помощь (ahelp), личные сообщения админ–игрок (bwoink), менторские тикеты и менеджер тикетов BandaStation.

### Админ-хелп тикеты
Верб Adminhelp создаёт тикет: взятие, ответ, закрытие/решение/отклонение/IC-issue, переоткрытие, таймаут, ретайтл, история взаимодействий, передача в Discord через TGS.

- **Код:** [`/datum/admin_help`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/adminhelp.dm#L169), [`/datum/admin_help_tickets`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/adminhelp.dm#L11), [`adminhelp verb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/adminhelp.dm#L804)
- **BandaStation:** [`adminhelp.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/adminhelp.dm#L805)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin` (`tickets.rs`), по менеджеру тикетов BandaStation: тикет из adminhelp (F1) всем админам, ответ привязывает админа, закрыть/решить/переоткрыть, автозакрытие через 20 минут, сообщения BandaStation. Нет REJT/IC-кнопок tg, ретайтла, передачи в Discord/TGS

### Личные сообщения (Admin PM)
Переписка админа с игроком со звуком «bwoink» и миганием окна; от игрока — ответ в тикет; отправка из Discord.

- **Код:** [`/client/proc/cmd_admin_pm()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/adminpm.dm#L147), [`adminpm.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/adminpm.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin`: PM игроку в его тикет или новый PM-тикет («Личное сообщение от администратора»), ответ игрока через F1 (раз в 5 с). Нет звука bwoink, мигания окна, PM из Discord

### История тикетов игрока
Просмотр прошлых тикетов игрока админом.

- **Код:** [`player_ticket_history.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/player_ticket_history.dm#L1)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Менеджер тикетов BandaStation
Свои тикеты админов и менторов (`/datum/help_ticket`): окно создания тикета `TicketCreation` с выбором типа, TGUI-менеджер `TicketManager` (список, привязка админа, конвертация админ↔ментор, ключевые слова для авто-подсказок), вебхуки в Discord о создании и автозакрытии.

- **Код:** [`/datum/help_ticket`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ticket_manager/code/help_ticket/help_ticket.dm#L1), [`/datum/help_ticket_type`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ticket_manager/code/help_ticket/help_ticket_type.dm#L10), [`/datum/ticket_manager`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ticket_manager/code/ticket_manager/manager_ui.dm#L4), [`/datum/help_ui_handler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ticket_manager/code/help_ui_handler/help_ui_handler.dm#L3); tgui: [`TicketManager.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/TicketManager/index.tsx), [`TicketCreation.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/TicketCreation.tsx)
- **BandaStation:** [`_ticket_manager.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ticket_manager/_ticket_manager.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin`: окно тикета игрока (F1: создание и переписка) и вкладка тикетов админ-панели (список, переписка, ответ, состояние) на egui вместо TGUI `TicketCreation`/`TicketManager`; только админ-тикеты (нет менторских и конвертации), нет индикатора набора, ключевых слов и вебхуков
