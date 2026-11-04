# Консоль связи и объявления
Консоль связи `/obj/machinery/computer/communications`, приоритетные объявления и отчёты ЦК.

### Консоль связи: авторизация
Вход по ID с доступом командования (капитан — расширенные права), ИИ и борги авторизуются сами; выход.

- **Код:** [`/obj/machinery/computer/communications`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L10), [`proc/authenticated()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L110); tgui: [`CommunicationsConsole`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CommunicationsConsole)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/shuttle.rs`, `crates/ss13_game/src/comms.rs`, `crates/ss13_game/src/comms_client.rs`: вход/выход по ID с командным доступом, капитанский доступ открывает раздел уровня тревоги; нет синтетиков, взлома

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
- **Статус в ss13-rs:** ❌ нет

### Консоль связи: статус-дисплеи
Установка текста или картинки на станционных статус-дисплеях.

- **Код:** [`ui_act "setStatusMessage"`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L411), [`proc/post_status()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L834)
- **Статус в ss13-rs:** ❌ нет

### Консоль связи: покупка шаттла
Капитан покупает другой эвакуационный шаттл за деньги отдела (см. ../shuttles/).

- **Код:** [`ui_act "purchaseShuttle"`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L265), [`proc/can_buy_shuttles()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L747)
- **Статус в ss13-rs:** ❌ нет

### Консоль связи: аварийный доступ, коды ядерной бомбы, ОБР
Включение аварийного доступа в техтоннели, запрос кодов самоуничтожения у ЦК; форк добавляет запрос ОБР от капитана.

- **Код:** [`ui_act "toggleEmergencyAccess"`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L459), [`ui_act "requestNukeCodes"`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L316)
- **BandaStation:** [`communications.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L328) — действие `requestERT`
- **Статус в ss13-rs:** ❌ нет

### Взлом консоли связи
Нинзя/антагонисты взламывают консоль: случайный эффект (пираты, ложная тревога, сообщение Синдикату).

- **Код:** [`proc/hack_console()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L913), [`/obj/machinery/computer/communications/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/communications.dm#L62)
- **Статус в ss13-rs:** ❌ нет

### Приоритетные объявления
`priority_announce`, `minor_announce`, отчёт ЦК (`print_command_report`): крупный текст всем игрокам, звук диктора, печать в консоли связи.

- **Код:** [`/proc/priority_announce()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/priority_announce.dm#L40), [`/proc/minor_announce()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/priority_announce.dm#L142), [`/proc/print_command_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/priority_announce.dm#L110); tgui: [`CommandReport.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CommandReport.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/shuttle.rs` (`announce`): приоритетные объявления шаттла в чат всем; `minor_announce` со звуком и объявления уровня угрозы — `crates/ss13_game/src/security_level/mod.rs`; нет звуков приоритетных объявлений, цветов, отчётов ЦК

### Дикторы объявлений (announcers)
Наборы звуков дикторов ЦК (приветствие, тревоги, шаттл и т. д.), выбирается на раунд; форк добавляет диктора «стажёр».

- **Код:** [`/datum/centcom_announcer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/announcers/_announcer.dm#L2)
- **BandaStation:** [`modular_bandastation/announcers/code/intern_announcer.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/announcers/code/intern_announcer.dm#L1) — звуки «стажёра»
- **Статус в ss13-rs:** ❌ нет

### Контроллер связи и отчёт о начале раунда
`GLOB.communications_controller`: кулдауны объявлений, сообщения в консоли, отчёт ЦК о целях и угрозах в начале раунда; форк заменяет шаблон отчёта и добавляет управление целями станции.

- **Код:** [`/datum/communciations_controller`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/communications.dm#L8)
- **BandaStation:** [`communications.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/communications.dm#L78) — шаблон `modular_bandastation/paperwork/templates/station_report.md`, задержка отчёта
- **Статус в ss13-rs:** ❌ нет
