# Конвейеры, утилизация и переработка
Конвейерные ленты и переключатели, система мусоропровода (disposals) с сортировкой почты, упаковка посылок и переработчик.

### Конвейерные ленты
Лента двигает предметы и мобов по направлению (с поворотами и изгибами), работает от питания; собирается из стака лент, поворачивается гаечным ключом, переворачивается отвёрткой.

- **Код:** [`/obj/machinery/conveyor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/conveyor.dm#L11), [`proc/update_move_direction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/conveyor.dm#L187), [`proc/start_conveying()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/conveyor.dm#L276), [`/obj/item/stack/conveyor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/conveyor.dm#L650)
- **Статус в ss13-rs:** ❌ нет

### Переключатель конвейера
Рычаг вперёд/стоп/назад (или односторонний) управляет всеми лентами с тем же `id`; связывание мультитулом; провода конвейера.

- **Код:** [`/obj/machinery/conveyor_switch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/conveyor.dm#L426), [`/obj/machinery/conveyor_switch/oneway`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/conveyor.dm#L604), [`/obj/item/conveyor_switch_construct`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/conveyor.dm#L614)
- **Статус в ss13-rs:** ❌ нет

### Мусорный бак (disposal unit)
Бак копит давление от питания, принимает предметы, мобов и мешки для трупов (с задержкой), смывает содержимое в трубы; изнутри можно выбраться; tgui-окно; почтовый люк (`delivery_chute`) смывает сразу.

- **Код:** [`/obj/machinery/disposal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/bin.dm#L9), [`/obj/machinery/disposal/bin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/bin.dm#L436), [`proc/flush()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/bin.dm#L304), [`proc/stuff_mob_in()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/bin.dm#L203), [`/obj/machinery/disposal/delivery_chute`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/bin.dm#L704); tgui: [`DisposalUnit.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/DisposalUnit.tsx)
- **BandaStation:** [`aesthetics/disposal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/disposal/code/disposal.dm) — спрайты бака, люка и выхода
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/disposals/bin.rs`: предмет кликом (вне боевого режима), броском (75%), моб перетаскиванием (2 с, в том числе сам себя), давление из воздуха комнаты каждые 2 с до `SEND_PRESSURE` (нужно питание оборудования зоны), смыв ручкой и автосмыв каждые 30 тиков, выход шагом, окно `DisposalUnit` (ручка, насос, выброс), оверлеи и анимация смыва, маркировщик на `bin/tagger`; почтовый люк смывает вошедшее спереди (мобы 2% отскакивают). Нет: мешков для трупов, вытряхивания сумок в бак, отвёртки/сварки (панель, разборка), расхода энергии, зверька внутри, крысиного короля, звуков

### Трубы мусоропровода
Сегменты, развилки, Y-развилки, магистрали, поворотные, многоуровневые трубы; содержимое летит в `disposalholder` по трубам; повреждённые трубы выбрасывают содержимое; трубы прячутся под полом.

- **Код:** [`/obj/structure/disposalpipe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/pipe.dm#L3), [`/obj/structure/disposalpipe/junction`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/pipe.dm#L204), [`/obj/structure/disposalpipe/trunk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/pipe.dm#L244), [`/obj/structure/disposalpipe/trunk/multiz`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/multiz.dm#L5), [`/obj/structure/disposalholder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/holder.dm#L6), [`proc/start_moving()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/holder.dm#L79)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/disposals/{mod,pipes}.rs`: сегменты, развилки (50/50), Y-развилки, магистрали под баками/люками/выходами, поворотные трубы, сломанные; `disposalholder` идёт по трубе за тик tg (0,05 с, до 1000 шагов; исчерпав их, сразу вываливает содержимое на месте, а не застревает до `struggle`, как в tg), вылет на обрыве (на 10 клеток по ходу), мобы внутри едут с ним (вид следует), «CLONG» при попытке идти; трубы прячутся под полом. Нет: многоуровневых магистралей, повреждения труб и выдавливания (`struggle`), снятия плитки на обрыве

### Сортировка почты
Сортирующие трубы отправляют посылки по метке назначения (`sortTag`) или по упаковке; тэгер назначения ставит метку на посылку.

- **Код:** [`/obj/structure/disposalpipe/sorting`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/pipe_sorting.dm#L3), [`/obj/structure/disposalpipe/sorting/mail`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/pipe_sorting.dm#L23), [`/obj/structure/disposalpipe/sorting/wrap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/pipe_sorting.dm#L81), [`/obj/item/dest_tagger`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/sortingmachinery.dm#L258)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/disposals/pipes.rs`, `mail.rs`: `sorting/mail` по `destinationTag` (`sortType` с карты и хелперы `mapping_helpers/mail_sorting`), `sorting/wrap` по `tomail`; тэгер назначения с окном `DestinationTagger` ставит `sort_tag` посылке. Нет: правки фильтров развилки тэгером (трубы под полом), тэгера киборга

### Выход мусоропровода
Выход выбрасывает содержимое с силой в направлении (в космос, в комнату утилизации); гаечный ключ меняет дальность.

- **Код:** [`/obj/structure/disposaloutlet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/outlet.dm#L16), [`eject.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/eject.dm)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/disposals/mod.rs`: ждёт 2 с с анимацией `outlet-open` и бросает содержимое на `eject_range` клеток вперёд, выпускает газ; MetaStation: петля станции в почтовое отделение, отходы — в космос. Нет: мультитула/емага (скорость выброса), звуков

### Сборка мусоропровода
Детали труб и бака из раздатчика: поставить, приварить к полу, подключить к магистрали; разборка сваркой.

- **Код:** [`/obj/structure/disposalconstruct`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/disposal/construction.dm#L4)
- **Статус в ss13-rs:** ❌ нет

### Посылки и ценники
Обёрточной бумагой предмет или ящик заворачивается в посылку (метка назначения, ценник `sales_tagger` для продажи через карго).

- **Код:** [`/obj/item/delivery`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/sortingmachinery.dm#L1), [`/obj/item/sales_tagger`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/recycling/sortingmachinery.dm#L329)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/disposals/mail.rs`: обёрточная бумага заворачивает предмет (с пола или из другой руки) в `delivery/small` по весу, тэгер ставит метку, распаковка в руке (1,5 с). Нет: больших посылок (шкафы, ящики, канистры), подарочной бумаги, записок, штрихкодов и `sales_tagger`

### Переработчик
Перемалывает въехавшие предметы в материалы (выплёвывает листы), с емагом — и людей (деталь «смертельной ловушки»); защита от застревания.

- **Код:** [`/obj/machinery/recycler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/recycler.dm#L3), [`proc/eat()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/recycler.dm#L128), [`/obj/machinery/recycler/deathtrap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/recycler.dm#L300)
- **Статус в ss13-rs:** ❌ нет
