# Устройства общего назначения
Портативные устройства из `code/game/objects/items/devices` и соседних файлов, не относящиеся к другим отделам.

### GPS
GPS-маяк с меткой: в tgui показывает координаты свои и всех включённых GPS на том же уровне с направлением и расстоянием; ЭМИ отключает.

- **Код:** [`/obj/item/gps`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/gps.dm#L2), [`gps.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/gps.dm); tgui: [`Gps.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Gps.tsx)
- **Статус в ss13-rs:** ❌ нет

### Мегафон
Речь держащего мегафон становится громче (крупный шрифт); варианты СБ и командования; ЭМИ/емаг портят.

- **Код:** [`/obj/item/megaphone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/megaphone.dm#L1), [`handle_speech()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/megaphone.dm#L30)
- **BandaStation:** [`add_tts_filter()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/megaphone.dm#L41) — фильтр TTS для голоса через мегафон
- **Статус в ss13-rs:** ❌ нет

### Диктофон и кассеты
Диктофон записывает услышанную речь на кассету с отметками времени, проигрывает её и печатает расшифровку; кассету можно вынуть, перемотать, порвать.

- **Код:** [`/obj/item/taperecorder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/taperecorder.dm#L1), [`Hear()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/taperecorder.dm#L157), [`/obj/item/tape`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/taperecorder.dm#L369)
- **BandaStation:** [`taperecorder.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/taperecorder.dm#L260), [`/datum/tape_message`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/devices/recorder/recorder_message.dm#L1) — запись хранит TTS-голос говорящего и воспроизводит его
- **Статус в ss13-rs:** ❌ нет

### Пинпоинтеры
Указатель направления на цель: на диск ядерной аутентификации (капитан), на члена экипажа с датчиками, парные, на шаттл; стрелка показывает направление и дальность.

- **Код:** [`/obj/item/pinpointer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/pinpointer.dm#L2), [`scan_for_target()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/pinpointer.dm#L76), [`/obj/item/pinpointer/crew`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/pinpointer.dm#L107), [`/obj/item/pinpointer/pair`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/pinpointer.dm#L190)
- **Статус в ss13-rs:** 🟡 частично — пинпоинтер на диск у ядерных оперативников (`crates/ss13_game/src/antag/nukeop/server.rs`: включение, переключение на инфильтратор после взвода бомбы); нет капитанского, по датчикам, парных и на шаттл

_«T-ray сканер» описано в [../engineering/tools.md](../engineering/tools.md)._

### Лазерная указка
Указывает точку, ослепляет людей и камеры, отвлекает котов; заряд восстанавливается, линза меняет мощность.

- **Код:** [`/obj/item/laser_pointer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/laserpointer.dm#L1)
- **Статус в ss13-rs:** ❌ нет

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
- **Статус в ss13-rs:** ❌ нет

### Маяки и маяки выбора
Маяк-цель для телепортов и подов; маяки выбора (музыкальный инструмент, герой, оружие ГСБ) дают выбрать предмет и присылают его подом.

- **Код:** [`/obj/item/beacon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/beacon.dm#L1), [`/obj/item/choice_beacon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/choice_beacon.dm#L1)
- **BandaStation:** [`choice_beacon.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/choice_beacon.dm#L67) — доставка телепорт-подом; [`choice_beacon.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/choice_beacon.dm#L98) — больше инструментов
- **Статус в ss13-rs:** ❌ нет

### Бумбокс и музыкальные кассеты
Переносной магнитофон проигрывает песни с кассет вокруг.

- **Код:** [`/obj/item/boombox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/boombox/boombox.dm#L1), [`/obj/item/music_tape`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/boombox/music_tapes.dm#L2)
- **Статус в ss13-rs:** ❌ нет — звука нет

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
