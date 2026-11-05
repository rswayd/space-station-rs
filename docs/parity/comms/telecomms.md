# Телекоммуникации
Цепочка машин `/obj/machinery/telecomms`, через которую проходит радио и сообщения КПК, и консоли контроля.

### Сеть телекомов и прохождение сигнала
Сигнал радио идёт приёмник → шина → процессор → сервер → хаб → вещатель; машины связаны по `autolinkers`/`network`, без питания или при поломке звена канал «глохнет» или искажается; ретрансляторы дают связь на других z-уровнях.

- **Код:** [`/obj/machinery/telecomms`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/telecomunications.dm#L9), [`proc/relay_information()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/telecomunications.dm#L48), [`/datum/signal/subspace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/broadcasting.dm#L2)
- **BandaStation:** [`bus.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/machines/bus.dm#L69), [`server.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/machines/server.dm#L195) — частота Justice в шине и сервере
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/telecomms.rs`: машины карты из пресетов типов (id, `autolinkers`, `freq_listening`, `toggled`), автосвязывание (`post_machine_initialize`: общий autolinker, оба дальнего действия, ≤20 клеток или связанный уровень), `on` = включена и не обесточена/`EMPED`/сломана; `Net::route` повторяет `relay_information`/`receive_information` (приёмник → хаб или шина → процессор → шина → сервер → хаб → ретрансляторы и вещатель, лимит 20 отправок), уровни сигнала (`levels`, ретранслятор добавляет станционные уровни, вещатель — свои); `crates/ss13_game/src/radio.rs` шлёт через сеть каждую передачу, иначе `backup_transmission` для не-subspace радио; карта без машин — прежняя идеальная сеть уровня (shortcut: без трафика и задержек `slow`, map var edits `autolinkers` не читаются); частота Justice в шине 3 и сервере СБ

### Приёмник, шина, процессор, сервер, хаб, вещатель, ретранслятор
Отдельные машины цепочки: приёмник ловит сигнал, шина распределяет по частотам, процессор раскомпрессирует (выключенный — речь искажена), сервер логирует, хаб связывает z-уровни, вещатель передаёт на радио; «всё-в-одном» для аванпостов и нюкеров.

- **Код:** [`/obj/machinery/telecomms/receiver`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/machines/receiver.dm#L8), [`/obj/machinery/telecomms/bus`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/machines/bus.dm#L11), [`/obj/machinery/telecomms/processor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/machines/processor.dm#L8), [`/obj/machinery/telecomms/server`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/machines/server.dm#L9), [`/obj/machinery/telecomms/hub`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/machines/hub.dm#L10), [`/obj/machinery/telecomms/broadcaster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/machines/broadcaster.dm#L12), [`/obj/machinery/telecomms/relay`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/machines/relay.dm#L9), [`/obj/machinery/telecomms/allinone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/machines/allinone.dm#L5)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/telecomms.rs`: приёмник (`check_receive_level` через хаб и ретранслятор), шина (процессор, затем сервер/хаб/вещатель), процессор (раскомпрессия; выключенный — речь искажена `Gibberish`), сервер (только голосовые сигналы своих частот), хаб, вещатель (`recent_messages` в пределах сигнала), ретранслятор (`broadcasting`/`receiving`), «всё-в-одном» (синдикатовский вещает на все уровни); ЭМИ машины — `EMPED` на `300/severity` ±2 с; нет журналов серверов (некому читать: нет браузера логов), сжимающего режима процессора и смены частоты шины (нет окна настройки)

### Настройка машин телекомов
Мультитулом связываются машины (буфер), окно настройки: частоты, сеть, ID, включение; панель, лом для разборки.

- **Код:** [`/obj/machinery/telecomms/ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/machine_interactions.dm#L76), [`proc/add_new_link()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/machine_interactions.dm#L153); tgui: [`Telecomms.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Telecomms.jsx)
- **Статус в ss13-rs:** ❌ нет

### TTS в радио (BandaStation)
Голос TTS для радиосообщений: пространственное воспроизведение у слушателей интеркомов и гарнитур.

- **Код:** [`/datum/signal/subspace/vocal/broadcast()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/broadcasting.dm#L119)
- **BandaStation:** [`broadcasting.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/broadcasting.dm#L109) — TTS-проигрывание радио
- **Статус в ss13-rs:** ❌ нет

### Консоль мониторинга телекомов и браузер логов серверов
Консоль сканирует сеть и показывает машины; браузер логов читает записи сервера (кто и что сказал).

- **Код:** [`/obj/machinery/computer/telecomms/monitor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/computers/telemonitor.dm#L5), [`/obj/machinery/computer/telecomms/server`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/computers/logbrowser.dm#L5); tgui: [`TelecommsMonitor.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/TelecommsMonitor.tsx)
- **Статус в ss13-rs:** ❌ нет

### Сервер сообщений и монитор сообщений
Сервер хранит сообщения КПК и консолей запросов; без него КПК-мессенджер не работает. Консоль монитора сообщений (ключ из бумаги у ЦЕ) читает и подделывает сообщения.

- **Код:** [`/obj/machinery/telecomms/message_server`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/machines/message_server.dm#L81), [`/obj/machinery/computer/message_monitor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/computers/message.dm#L11), [`/obj/item/paper/monitorkey`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/computers/message.dm#L313); tgui: [`MessageMonitor.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/MessageMonitor.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/telecomms.rs`: сервер сообщений в цепочке (`server_type`, `reject` до сервера, калибровка 15 мин у непресетного), без него КПК-мессенджер отвечает «ERROR: Server is not responding.» (`crates/ss13_game/src/modpc/programs/messenger.rs`); нет журнала сообщений, ключа и консоли монитора

### Чёрный ящик
Регистратор чёрного ящика на сервере сообщений: кража стирает логи раунда.

- **Код:** [`/obj/machinery/blackbox_recorder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/machines/message_server.dm#L2), [`/obj/item/blackbox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/machines/message_server.dm#L63)
- **Статус в ss13-rs:** ❌ нет

### Реле NTNet
Реле обеспечивает сетевую связь модульных компьютеров (скачивание, чат); DoS-атака перегружает его.

- **Код:** [`/obj/machinery/ntnet_relay`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/NTNet/relays.dm#L11); tgui: [`NtnetRelay.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/NtnetRelay.tsx)
- **Статус в ss13-rs:** ❌ нет
