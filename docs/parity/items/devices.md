# Устройства общего назначения
Портативные устройства из `code/game/objects/items/devices` и соседних файлов, не относящиеся к другим отделам.

### GPS
GPS-маяк с меткой: в tgui показывает координаты свои и всех включённых GPS на том же уровне с направлением и расстоянием; ЭМИ отключает.

- **Код:** [`/obj/item/gps`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/gps.dm#L2), [`gps.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/gps.dm); tgui: [`Gps.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Gps.tsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/devices/gps.rs`, окно `crates/ss13_tgui/src/interfaces/gps.rs`: метки по типу (COM0, SCI0, ENG0, MINE0, MINER, BORG0, PAI0, «Distant Signal»), имя «<имя> (<метка>)», оверлей `working`; в руке (компьютер спасательной капсулы — кликом) окно: своя зона и координаты, остальные отслеживающие GPS на всех уровнях или только своём («LOCAL»), на своём уровне — расстояние и пеленг; переименование (20 символов, проверка открытого окна после ввода), вкл/выкл отслеживания (выключенный не виден другим), ручное/автообновление; ЭМИ — 30 с от последнего импульса, оверлей `emp`, окно закрывается, «слабо шипит». Нет: alt-клика (цепочка alt-кликов инвентаря общая; то же делает кнопка питания), пеленга на связанные уровни, поиска по имени, разметки пола отладочным GPS, GPS-компонента на не-предметах (тендрилы, кейральные наручники — группа шахт)

### Мегафон
Речь держащего мегафон становится громче (крупный шрифт); варианты СБ и командования; ЭМИ/емаг портят.

- **Код:** [`/obj/item/megaphone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/megaphone.dm#L1), [`handle_speech()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/megaphone.dm#L30)
- **BandaStation:** [`add_tts_filter()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/megaphone.dm#L41) — фильтр TTS для голоса через мегафон
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/devices/mod.rs`: мегафон в активной руке даёт речи держащего свой span (`ss13_chat::speech::VoiceSpan`) и звук, затем перезаряжается 5 с (сказанное раньше — обычным текстом и предупреждение). Нет: крупного шрифта (`SPAN_COMMAND`, клоунского `SPAN_CLOWN`, `SPAN_REALLYBIG` после емага — пока жирный: в чате нет крупного span, нужен контракт `ss13_shared::chat::Span`), емага, TTS-фильтра (TTS нет), громкости речи в радио

### Диктофон и кассеты
Диктофон записывает услышанную речь на кассету с отметками времени, проигрывает её и печатает расшифровку; кассету можно вынуть, перемотать, порвать.

- **Код:** [`/obj/item/taperecorder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/taperecorder.dm#L1), [`Hear()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/taperecorder.dm#L157), [`/obj/item/tape`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/taperecorder.dm#L369)
- **BandaStation:** [`taperecorder.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/taperecorder.dm#L260), [`/datum/tape_message`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/devices/recorder/recorder_message.dm#L1) — запись хранит TTS-голос говорящего и воспроизводит его
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/devices/taperecorder.rs`: диктофон со случайной кассетой (`/empty` — без), меню в руке списком (запись, воспроизведение, расшифровка, стоп, извлечение), вставка кассеты кликом, извлечение пустой рукой из диктофона в руке; запись — секунда плёнки в секунду (10 мин на сторону), предупреждение за минуту, услышанная речь с временем плёнки и именем говорящего (`[мм:сс] Имя: текст`); воспроизведение в записанном темпе, тишина длиннее 14 с объявляется и пропускается; расшифровка на листах до 5000 символов, перезарядка 30 с; кассета — цветной суффикс, случайный цвет, две стороны (переворот), размотка и перемотка отвёрткой (12 с); иконки состояний, осмотр дисплея. Нет: TTS-голоса (TTS нет), стен и радио при подслушивании (всё в 7 тайлах, шёпот — 1), шипения, своего span, alt-клика, размотки огнём, музыкальных и предзаписанных кассет, тонировки серой кассеты

### Пинпоинтеры
Указатель направления на цель: на диск ядерной аутентификации (капитан), на члена экипажа с датчиками, парные, на шаттл; стрелка показывает направление и дальность.

- **Код:** [`/obj/item/pinpointer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/pinpointer.dm#L2), [`scan_for_target()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/pinpointer.dm#L76), [`/obj/item/pinpointer/crew`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/pinpointer.dm#L107), [`/obj/item/pinpointer/pair`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/pinpointer.dm#L190)
- **Статус в ss13-rs:** 🟡 частично — на диск (`crates/ss13_game/src/antag/nukeop/server.rs`: все `/obj/item/pinpointer/nuke`, и капитанский, и оперативников; включение, тревога и переключение синдикатовских на инфильтратор после взвода бомбы); парные: `crates/ss13_game/src/devices/mod.rs` (два из одной коробки связываются, включаются в руке, стрелка на вторую половину раз в 0,2 с, осмотр — кто держит пару); по датчикам костюмов: там же (`CrewPinpointer`: первый пользователь — владелец, список экипажа с датчиками на «Слежении» на том же или другом станционном уровне по имени с ID, «Имя (2)» для повторов, проверка, что пинпоинтер ещё в руке, после выбора; цель теряется, когда датчики перестают отслеживать; контрактный — любой режим датчиков и `minimum_range` 25; осмотр — кого отслеживает). Нет: на шаттл охотников за головами, нюхача материалов

_«T-ray сканер» описано в [../engineering/tools.md](../engineering/tools.md)._

_«Фонарики и лампы» описаны в [lights-smoking.md](lights-smoking.md)._

### Лазерная указка
Указывает точку, ослепляет людей и камеры, отвлекает котов; заряд восстанавливается, линза меняет мощность.

- **Код:** [`/obj/item/laser_pointer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/laserpointer.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/devices/laser_pointer.rs`: клик вблизи и вдаль (`RangedClick`): нужен диод, цель в 7 тайлах (у бесконечной — без предела) и в прямой видимости, батарея не на полной перезарядке; в глаза карбона (зона «глаза») — вспышка с шансом 30 × класс диода, случайной силы 0–2, повязка на глаз вдвое снижает шанс; киборг — перегрузка сенсоров (вспышка); камера — сильный ЭМИ; мёртвым — сообщение; заряд 10, −1 за луч (−2 с линзой), на нуле блокировка до полной зарядки, восстановление `SPT_PROB(10 + 10 × класс)`; варианты (улучшенная — диод 3, бесконечная — 4); отвёртка вынимает диод, микролазер вставляется, блюспейс-кристалл (один из стопки) — линза перед диодом 3+, кусачками/зажимом обратно; осмотр — класс диода, заряд, линза. Нет: точки лазера на цели, котов (нет `TRAIT_CATLIKE_INSTINCT`), проверки рентгена/термального зрения (линза сквозь стены не светит), временной слепоты киборга, неудачной вставки с телепортом (просто отказ)

_«Лампозаменитель» описано в [../power/lighting.md](../power/lighting.md)._

### Бинокль
В руках двумя руками даёт обзор вдаль в направлении взгляда.

- **Код:** [`/obj/item/binoculars`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/binoculars.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Голопроекторы и проекторы силового поля
Голознаки «мокрый пол», голобарьеры СБ и инженеров, атмосферные голобарьеры, проектор силового поля — временные непроходимые/предупреждающие голограммы.

- **Код:** [`/obj/item/holosign_creator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/holosign_creator.dm#L1), [`/obj/item/forcefield_projector`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/forcefieldprojector.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — голопроекторы: `crates/ss13_game/src/holosign.rs` (подробности в ../engineering/windows-grilles.md, «Голографические знаки и барьеры»); проектора силового поля нет

### Ручной монитор экипажа
Портативная версия консоли наблюдения за датчиками костюмов.

- **Код:** [`/obj/item/sensor_device`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/sensor_device.dm#L1)
- **BandaStation:** [`sensor_device.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/sensor_device.dm#L18) — заменён форковым монитором с NanoMap (`CrewConsole220`)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/devices/mod.rs` (`HandheldMonitor`): в руке открывает окно консоли (`crates/ss13_game/src/clothing/sensors.rs`, `show`) по уровню, где находится монитор, обновление раз в 10 с; окно активно, пока монитор у пользователя (или рядом на полу). Как у консоли, NanoMap — обычный список tg; нет `drag_to_activate`

### Маяки и маяки выбора
Маяк-цель для телепортов и подов; маяки выбора (музыкальный инструмент, герой, оружие ГСБ) дают выбрать предмет и присылают его подом.

- **Код:** [`/obj/item/beacon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/beacon.dm#L1), [`/obj/item/choice_beacon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/choice_beacon.dm#L1)
- **BandaStation:** [`choice_beacon.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/choice_beacon.dm#L67) — доставка телепорт-подом; [`choice_beacon.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/choice_beacon.dm#L98) — больше инструментов
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/devices/beacons.rs`: маяк слежения (`TrackingBeacon`, `enabled_beacons` — список `GLOB.teleportbeacons`) включается/выключается в руке и правым кликом пустой рукой, `beacon`/`beacon-off`, ЭМИ переключает с шансом 75 / сила; маяки выбора: музыкальный (с инструментами форка), героический (костюмы), аугментаций (3 использования, предмет к ногам и звук) — список в руке, повторная проверка после выбора, сообщение компании, доставка телепорт-подом (`cargo::supplypod::launch`), последнее использование — искры и маяк исчезает. Нет: маяка вооружения капеллана (радиал наборов брони — группа религии), переименования; телепортера, который читает маяки, в порту нет (группа науки/телепортов)

### Бумбокс и музыкальные кассеты
Переносной магнитофон проигрывает песни с кассет вокруг.

- **Код:** [`/obj/item/boombox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/boombox/boombox.dm#L1), [`/obj/item/music_tape`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/boombox/music_tapes.dm#L2)
- **Статус в ss13-rs:** 🟡 `crates/ss13_game/src/instruments/boombox.rs`: кассеты (`MusicTape`, треки `/datum/looping_sound/boombox`), вставка кассеты, меню (играть/стоп/извлечь) в руке, на полу и в другой руке, нативный повтор трека на `CHANNEL_JUKEBOX` для всех в 12 тайлах с затуханием (sound token), подбор перетаскиванием. Нет: режим «на плече», частицы нот, строки осмотра, трек кассеты robocop (ремикс без лицензии — молчит)

### Камера трансляции
Ручная камера транслирует картинку и звук на телеэкраны станции.

- **Код:** [`/obj/item/broadcast_camera`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/broadcast_camera.dm#L4)
- **Статус в ss13-rs:** ❌ нет

### Мелкие устройства
Настольные часы, нажимные пластины, дверной фиксатор (блокирует шлюз), квантовая ключ-карта, крюк-кошка для подъёма, инспекционный сканер N-spect (печатает отчёты), вапоризатор, набор смены личности.

- **Код:** [`/obj/item/table_clock`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/table_clock.dm#L4), [`/obj/item/pressure_plate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/pressureplates.dm#L1), [`/obj/item/door_seal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/door_seal.dm#L1), [`/obj/item/quantum_keycard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/quantum_keycard.dm#L1), [`/obj/item/climbing_hook`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/climbingrope.dm#L1), [`/obj/item/inspector`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/inspector.dm#L9), [`/obj/item/vaporizer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/vaporizer.dm#L5), [`/obj/item/fake_identity_kit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/mulligan_kit.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Ручные жесты
Невещественные «предметы» в руке: жест-игра «кружок», щелбан, высокое пять, поцелуй и т.п.

- **Код:** [`/obj/item/hand_item`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/hand_items.dm#L2)
- **Статус в ss13-rs:** ❌ нет

Шпионские устройства (хамелеон-проектор, глушилка, шпионские очки и др.) — см. раздел антагонистов; глушилка НТ форка — [bandastation-objects.md](bandastation-objects.md).
