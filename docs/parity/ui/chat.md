# Чат
Панель чата (tgui-panel), доставка сообщений `to_chat`, OOC и связанные вербы, облачка речи над головами (runechat).

### Доставка сообщений
`to_chat` ставит сообщения в очередь `SSchat`, который пачками отправляет их клиентам с типом (для вкладок) и HTML-разметкой спанов.

- **Код:** [`to_chat()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgchat/to_chat.dm#L52), [`SUBSYSTEM_DEF(chat)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/chat.dm#L6), [`message.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgchat/message.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_chat/src/server.rs` (серверные `ChatMessage` со спанами и каналами)

### Панель чата
Вкладки по типам сообщений (настраиваемые), объединение повторов, подсветка слов, поиск, экспорт истории в HTML, размер шрифта и межстрочный интервал, тёмная/светлая тема, «прокрутить вниз», сохранение истории между сессиями.

- **Код:** [`tgui/packages/tgui-panel/chat`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui-panel/chat)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_chat/src/client/panel.rs`, `crates/ss13_chat/src/client/log.rs` (вкладки «Все»/«Система», объединение повторов, прокрутка, тёмная тема спанов): нет настраиваемых вкладок, подсветки, экспорта, настроек шрифта, сохранения истории

### OOC
Глобальный внеигровой чат: цвет OOC, мьют, игнор игроков, отключение админами; MOTD, админ-уведомление, свои заметки, отчёты прошлого раунда, политика сервера.

- **Код:** [`ooc verb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/verbs/ooc.dm#L5), [`select_ignore`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/verbs/ooc.dm#L201), [`motd`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/verbs/ooc.dm#L177)
- **BandaStation:** [`ooc.dm#L77`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/verbs/ooc.dm#L77) — бейджи в OOC
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin/src/ooc.rs` (`ClientIntent::Ooc`: сервер проверяет права, мут, длину, антиспам и рекламу; глобальные выключатели OOC/OOC мёртвых для админов `AdminRequest::ToggleOoc`/`ToggleDeadOoc`, вопрос «похоже на IC?», игнор игроков, MOTD из `config/motd.txt`, админ-уведомление `AdminRequest::SetAdminNotice`, кнопки на вкладке OOC статпанели), клавиши O/F2 (`crates/ss13_chat/src/client/panel.rs`): нет цвета OOC и `allow_admin_ooccolor` (префы), `CHAT_OOC`-мьюта своего канала (префы), бана OOC, словесных фильтров, своих заметок, отчётов раундов, политики сервера; игнор живёт до переподключения (префы/SQL)

### Бейджи и донат-эффекты в чате (BandaStation)
Иконки уровня подписки и роли персонала перед ником в OOC; донатерам — цвет/«сияние» ника (настройка видимости и эффекта).

- **Код:** [`/client/proc/get_ooc_badged_name()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/chat/code/badges.dm#L5), [`/datum/preference/choiced/donor_chat_effect`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/chat/code/chat_preferences.dm#L8), [`/datum/preference/toggle/donor_public`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/chat/code/chat_preferences.dm#L19)
- **BandaStation:** [`_chat.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/chat/_chat.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Облачка речи (runechat)
Текст речи и эмоций над головой говорящего (с цветом по имени, затуханием, стопкой сообщений), настраиваемо (вкл/выкл, для неговорящих, длина).

- **Код:** [`/datum/chatmessage`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/chatmessage.dm#L34), [`/mob/proc/create_chat_message()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/chatmessage.dm#L332), [`runechat.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/runechat.dm#L1)
- **BandaStation:** [`chatmessage.dm#L75`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/chatmessage.dm#L75) — вычищение TTS-разметки ударений
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/runechat.rs` (сервер: цвет по имени, обрезка, проверка состояния слушателя), `crates/ss13_game/src/runechat_client.rs` (стопка, затухание, укорочение жизни старых строк), источник — `ss13_chat::server::Overhead` (речь, шёпот, эмоции, LOOC). Настройки вкл/выкл, длина и «для неговорящих» — в `docs/parity/ui/preferences.md` (нужно меню настроек); нет иконок радио/языка и `apply_message_emphasis`
