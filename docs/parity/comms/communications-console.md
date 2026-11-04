# Консоль связи и объявления
Консоль связи `/obj/machinery/computer/communications`, приоритетные объявления и отчёты ЦК.

### Консоль связи: авторизация
Вход по ID с доступом командования (капитан — расширенные права), ИИ и борги авторизуются сами; выход.

- **Код:** [`/obj/machinery/computer/communications`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L10), [`proc/authenticated()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L110); tgui: [`CommunicationsConsole`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CommunicationsConsole)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/comms_server.rs` (`toggle_authentication`, `CommsConsole::authenticated`/`silicon_or_captain`/`non_silicon_captain`, `emag_act`): вход/выход по ID с `ACCESS_COMMAND` (консоль Синдиката — `syndicate_leader`), капитанский доступ открывает капитанские функции, ИИ и борги авторизованы всегда и не выходят (окно открывается через `crate::silicon::interact::attack_ai`), емаг: вход как «Unknown» со всем станционным доступом и восстановление маршрутизации; окно — `crates/ss13_game/src/comms_client.rs`

### Консоль связи: вызов и отзыв эвакуационного шаттла
Раздел главной страницы: вызов с причиной (≥ 12 символов), отзыв, след места вызова. Сама механика шаттла — см. ../shuttles/.

- **Код:** [`ui_act "callShuttle"`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L180), [`ui_act "recallShuttle"`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L311)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/shuttle.rs`, `crates/ss13_game/src/comms_client.rs` (окно и проверки на сервере)

### Консоль связи: уровень тревоги
Смена кода (зелёный/синий) капитаном или главами; форк блокирует смену при Гамме и Эпсилоне.

- **Код:** [`ui_act "changeSecurityLevel"`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L188)
- **BandaStation:** [`communications.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L211) — военное положение
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/shuttle.rs` (`change_security_level`: вход с капитанским доступом, карта капитана в активной руке, только зелёный/синий, блокировка при дельте и при Гамме/Эпсилоне форка, сообщения и звуки отказа/подтверждения), `crates/ss13_game/src/comms_client.rs` (раздел «Alert Level» и окно «Swipe ID»); синтетики консолью не пользуются (их ветка `NO_SWIPE_NEEDED` в окне есть)

### Консоль связи: приоритетное объявление и сообщения ЦК/Синдикату
Объявление капитана на всю станцию (с кулдауном), сообщение Центральному командованию (или Синдикату через взломанную консоль), в другие сектора; входящие сообщения ЦК, ответы на них.

- **Код:** [`proc/make_announcement()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L805), [`ui_act "messageAssociates"`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L241), [`proc/send_cross_comms_message()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L517), [`/datum/comm_message`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L965)
- **BandaStation:** [`communications.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L391) — права на получение сообщений
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/comms_server.rs` (`MakePriorityAnnouncement` через текстовый промпт с перепроверкой консоли, входа и досягаемости, `MessageAssociates`, `AnswerMessage`/`DeleteMessage`, `syndicate_players`), `crates/ss13_game/src/announce.rs` (`make_announcement`, кулдауны 30 с; `send_message`): объявление капитана/Синдиката/ИИ, сообщение ЦК или Синдикату (взломанная консоль, консоль Синдиката) с общим кулдауном 60 с — админам запросом (`CommsRequest` → `crates/ss13_admin/src/server.rs` `comms_requests`, звук `server-ready.ogg`), список сообщений консоли с ответами и удалением; нет речевых модификаторов (`treat_message`) и проверки немоты. Чужое: сообщения в другие сектора — нет межсерверной связи (../server/); ответ админа на сообщение ЦК — ../admin/ (Fax Panel, Create Command Report)

### Консоль связи: статус-дисплеи
Установка текста или картинки на станционных статус-дисплеях.

- **Код:** [`ui_act "setStatusMessage"`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L411), [`proc/post_status()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L834)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/comms_server.rs` (`SetStatusMessage` с `reject_bad_text` BandaStation, `SetStatusPicture` по `status_display_approved_pictures`, «currentalert» — картинка текущего кода; `post_status("shuttle")` при вызове шаттла), `crates/ss13_game/src/security_level/displays.rs` (`post_status`, ручное состояние, к которому дисплей возвращается после показа кода); страница окна — `comms_client.rs`. Отрисовка текста и таймера шаттла на самих дисплеях — station-info.md «Статус-дисплеи»

### Консоль связи: покупка шаттла
Капитан покупает другой эвакуационный шаттл за деньги отдела (см. ../shuttles/).

- **Код:** [`ui_act "purchaseShuttle"`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L265), [`proc/can_buy_shuttles()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L747)
- **Статус в ss13-rs:** ❌ нет — кнопка скрыта (`can_buy_shuttles` всегда ложно), действие отклоняется: нет каталога шаблонов эвакуационных шаттлов с ценами и замены шаблона посреди раунда (`crates/ss13_game/src/shuttle.rs` грузит один шаблон при старте)

### Консоль связи: аварийный доступ, коды ядерной бомбы, ОБР
Включение аварийного доступа в техтоннели, запрос кодов самоуничтожения у ЦК; форк добавляет запрос ОБР от капитана.

- **Код:** [`ui_act "toggleEmergencyAccess"`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L459), [`ui_act "requestNukeCodes"`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L316)
- **BandaStation:** [`communications.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L328) — действие `requestERT`
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/comms_server.rs` (`ToggleEmergencyAccess` с тремя бесплатными переключениями и кулдауном 30 с, `RequestNukeCodes`, `RequestErt` BandaStation: приоритетное объявление со звуком отчёта и запрос админам, общий кулдаун 60 с). Чужое: выдача кодов и отправка ОБР админом — ../admin/; запрос кодов сейфа запасной карты капитана — нет сейфа запасной ID (../security/)

### Взлом консоли связи
Нинзя/антагонисты взламывают консоль: случайный эффект (пираты, ложная тревога, сообщение Синдикату).

- **Код:** [`proc/hack_console()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L913), [`/obj/machinery/computer/communications/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L62)
- **Статус в ss13-rs:** ❌ нет — `hack_console` некому вызывать: нет ниндзя и других антагонистов со взломом консоли; емаг консоли — раздел «авторизация» (✅)

### Приоритетные объявления
`priority_announce`, `minor_announce`, отчёт ЦК (`print_command_report`): крупный текст всем игрокам, звук диктора, печать в консоли связи.

- **Код:** [`/proc/priority_announce()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/priority_announce.dm#L40), [`/proc/minor_announce()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/priority_announce.dm#L142), [`/proc/print_command_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/priority_announce.dm#L110); tgui: [`CommandReport.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CommandReport.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/announce.rs` (`Priority`: заголовки по типу — ЦК, отправитель, «Приоритетное оповещение», капитан, Синдикат; `custom_alert_message` диктора, кроме важных; звук диктора по ключу `ANNOUNCER_*` или случайный звук тревоги; получатели — игроки, кроме глухих; `print_command_report` в консоли связи с печатью листа), `minor_announce` — `crates/ss13_game/src/security_level/mod.rs`; шаттл и события передают свои `ANNOUNCER_*`; нет цветных рамок объявлений (в `ss13_shared::chat` нет спанов-рамок — контракт) и TTS

### Дикторы объявлений (announcers)
Наборы звуков дикторов ЦК (приветствие, тревоги, шаттл и т. д.), выбирается на раунд; форк добавляет диктора «стажёр».

- **Код:** [`/datum/centcom_announcer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/announcers/_announcer.dm#L2)
- **BandaStation:** [`modular_bandastation/announcers/code/intern_announcer.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/announcers/code/intern_announcer.dm#L1) — звуки «стажёра»
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_mapload/src/sound.rs` (`resolve_announcers`: все `/datum/centcom_announcer` в `sfx.ron`, их звуки копируются, включая «стажёра» BandaStation), `crates/ss13_game/src/announce.rs` (`Announcer`, `welcome_sound`, `report_sound`, звуки событий и тревоги); звук приветствия в начале раунда — `crates/ss13_game/src/round.rs`. Чужое: выбор стажёра/медбота станционным трейтом — ../round/station-traits-goals.md

### Контроллер связи и отчёт о начале раунда
`GLOB.communications_controller`: кулдауны объявлений, сообщения в консоли, отчёт ЦК о целях и угрозах в начале раунда; форк заменяет шаблон отчёта и добавляет управление целями станции.

- **Код:** [`/datum/communciations_controller`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/communications.dm#L8)
- **BandaStation:** [`communications.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/communications.dm#L78) — шаблон `modular_bandastation/paperwork/templates/station_report.md`, задержка отчёта
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/announce.rs` (`CommsController`: кулдауны объявлений и консолей, последний текст дисплеев; `send_message`; `queue_roundstart_report` через 1–5 с после старта, `station_report` по шаблону `station_report.md`, печать в консолях, синий код без своего объявления и объявление «Уровень угрозы повышен.» со звуком перехвата — `ROUNDSTART_BLUE_ALERT` BandaStation); лист отчёта хранит текст в описании (нет текста бумаги). Чужое: разделы целей станции и трейтов в отчёте — ../round/station-traits-goals.md
