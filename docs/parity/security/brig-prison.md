# Бриг, тюрьма и трудовой лагерь (ГУЛАГ)
Камеры брига с таймерами, вспышки камер, консоль заключённых, отправка в трудовой лагерь и работа там за очки.

### Таймер камеры брига (door timer)
Дисплей у камеры, связанный по `id` с дверями камеры и вспышками: охранник выставляет срок (пресеты короткий/средний/длинный), двери запираются на время, по истечении открываются с объявлением по радио охраны; из окна можно сработать вспышки камеры. Двери-окна брига (`brigdoor`) открываются только таймером или доступом охраны.

- **Код:** [`/obj/machinery/status_display/door_timer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/brigdoors.dm#L14), [`proc/timer_start()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/brigdoors.dm#L98), [`proc/timer_end()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/brigdoors.dm#L133), [`/obj/machinery/door/window/brigdoor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/windowdoor.dm#L497); tgui: [`BrigTimer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/BrigTimer.tsx)
- **BandaStation:** [`code/game/machinery/doors/brigdoors.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/brigdoors.dm#L1) — максимум 30 мин, пресеты 5/10/15 мин, подписанные пресеты в интерфейсе
- **Статус в ss13-rs:** ❌ нет

### Настенная вспышка (flasher)
Вспышка в стене камеры: по кнопке или таймеру ослепляет всех в радиусе 2 (5 с), перезарядка 15 с; лампу можно вынуть кусачками и заменить; переносная вспышка на треноге срабатывает на движение рядом.

- **Код:** [`/obj/machinery/flasher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/flasher.dm#L3), [`proc/flash()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/flasher.dm#L110), [`/obj/machinery/flasher/portable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/flasher.dm#L167), [`/obj/item/wallframe/flasher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/flasher.dm#L219)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wallmount/controls.rs` (`Flasher`, `flash`), `crates/ss13_game/src/security.rs` (`flash_mob_from`): по кнопке-вспышке с тем же `id` при питании и без отката 15 с ослепляет живых мобов в поле зрения в радиусе `flash_range` (направленный `flash_mob` от вспышки, путаница `strength*1.5`, паралич `strength`), лампа может перегореть (`flash_recharge(30)`), вид `mflash1`/`-p`, звук. Нет: замены лампы кусачками, flick и вспышки света, переносной вспышки, ИИ-клика, рамы

### Консоль управления заключёнными
Вход по ID охраны: вставить ID-карту заключённого и выставить цель очков, сбросить её; список заключённых с имплантами — следящие импланты показывают местонахождение, химическим можно ввести вещества, всем отправить сообщение.

- **Код:** [`/obj/machinery/computer/prisoner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/prisoner/_prisoner.dm#L1), [`/obj/machinery/computer/prisoner/management`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/prisoner/management.dm#L4); tgui: [`PrisonerManagement.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PrisonerManagement.tsx)
- **Статус в ss13-rs:** ❌ нет

### Телепорт в трудовой лагерь (gulag teleporter)
Капсула, куда кладут заключённого; консоль задаёт цель очков на его тюремной ID (по умолчанию 200), меняет статус в записях и телепортирует к маяку лагеря, забирая вещи в хранилище с возвратом.

- **Код:** [`/obj/machinery/gulag_teleporter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/gulag_teleporter.dm#L9), [`/obj/structure/gulag_beacon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/gulag_teleporter.dm#L180), [`/obj/machinery/computer/prisoner/gulag_teleporter_computer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/prisoner/gulag_teleporter.dm#L2); tgui: [`GulagTeleporterConsole.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/GulagTeleporterConsole.jsx)
- **Статус в ss13-rs:** ❌ нет

### Хранилище вещей заключённых (item reclaimer)
Хранит снятые при телепортации вещи; освобождённый (или охрана) забирает их по ID.

- **Код:** [`/obj/machinery/gulag_item_reclaimer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/gulag_item_reclaimer.dm#L1), [`proc/drop_items()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/gulag_item_reclaimer.dm#L99); tgui: [`GulagItemReclaimer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/GulagItemReclaimer.tsx)
- **Статус в ss13-rs:** ❌ нет

### Тюремная ID и очки труда
Карта заключённого хранит цель и накопленные очки; набравший цель может вызвать шаттл обратно. Сама карта — ID-карта (см. ../items/).

- **Код:** [`/obj/item/card/id/advanced/prisoner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L1397)
- **Статус в ss13-rs:** ❌ нет

### Консоль сдачи руды и укладчик лагеря
Заключённые добывают руду; укладчик лагеря начисляет за переплавленные листы очки, консоль переносит их на ID и при выполнении цели отпускает на шаттл; проверка очков — отдельный терминал.

- **Код:** [`/obj/machinery/mineral/labor_claim_console`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/laborcamp/laborstacker.dm#L3), [`/obj/machinery/mineral/stacking_machine/laborstacker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/laborcamp/laborstacker.dm#L183), [`/obj/machinery/mineral/labor_points_checker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/laborcamp/laborstacker.dm#L214); tgui: [`LaborClaimConsole.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/LaborClaimConsole.jsx)
- **Статус в ss13-rs:** ❌ нет

### Шаттл трудового лагеря
Шаттл между станцией и лагерем; консоль на шаттле в лагере работает в одну сторону и требует выполненной цели.

- **Код:** [`/obj/machinery/computer/shuttle/labor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/laborcamp/laborshuttle.dm#L1), [`/obj/machinery/computer/shuttle/labor/one_way`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/laborcamp/laborshuttle.dm#L9), [`/obj/docking_port/stationary/laborcamp_home`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/laborcamp/laborshuttle.dm#L30)
- **Статус в ss13-rs:** ❌ нет — из шаттлов есть только эвакуационный (`crates/ss13_game/src/shuttle.rs`)

### Тюремные ворота (prison gate)
Голографические ворота пропускают только заключённых с тюремной ID, выполнившей цель (трудовой вариант), и не пропускают ящики с людьми внутри.

- **Код:** [`/obj/machinery/prisongate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/prisongate.dm#L6), [`/obj/machinery/prisongate/labour`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/prisongate.dm#L111)
- **Статус в ss13-rs:** ❌ нет

### Тюремный труд: пресс номерных знаков и отправка урожая
Пресс номерных знаков в тюрьме брига: заготовка → знак, +10 очков на ID; телепорт урожая (`produceporter` и `producepad`) отправляет выращенное, по 2 очка за плод (вдвое за потенцию > 50).

- **Код:** [`/obj/machinery/plate_press`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/prisonlabor.dm#L2), [`proc/post_press()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/prisonlabor.dm#L77), [`/obj/machinery/produceporter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/prisonlabor.dm#L89), [`/obj/item/producepad`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/prisonlabor.dm#L223)
- **Статус в ss13-rs:** ❌ нет
