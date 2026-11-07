# Прочие предметы
Безделушки и одиночные предметы из `code/game/objects/items`, не входящие в другие семейства.

### Мусор
Обёртки, банки, коробки от еды — остаются после еды, выбрасываются в мусорку.

- **Код:** [`/obj/item/trash`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/trash.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/food.rs`: мусор после еды появляется в руке; утилизации нет

### Горшечные растения (kirby plants)
Декоративные растения в горшке: переносятся двумя руками, в них прячут предметы; органические поливаются, синтетические меняют вид.

- **Код:** [`/obj/item/kirbyplants`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/kirby_plants/kirbyplants.dm#L1), [`organic_plants.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/kirby_plants/organic_plants.dm), [`synthetic_plants.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/kirby_plants/synthetic_plants.dm)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: предмет подбирается; нет двуручного переноса и тайника

### Книги-навыки (granters)
Книги, прочтение которых за время учит навыку: рецепты крафта, боевые искусства (CQC), заклинания мага, язык жестов, оригами; часть одноразовые.

- **Код:** [`/obj/item/book/granter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/granters/_granters.dm#L6), [`on_reading_finished()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/granters/_granters.dm#L67), [`code/game/objects/items/granters`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/granters)
- **BandaStation:** [`_martial_arts.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/granters/martial_arts/_martial_arts.dm#L14) — ограничение для вампиров
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/martial/mod.rs`: чтение в руке (страницы с замечаниями, `uses`, отдача пустой книги), книги CQC и свиток карпа с ограничением для вампиров; нет книг рецептов, заклинаний, языка жестов, оригами

### Флэтпаки
Плоские упаковки машин: раскладываются мультитулом в готовую машину; тележка для флэтпаков, флэтпак двигателя шаттла.

- **Код:** [`/obj/item/flatpack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/flatpacks.dm#L1), [`multitool_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/flatpacks.dm#L55), [`/obj/structure/flatpack_cart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/flatpacks.dm#L100)
- **BandaStation:** [`flatpacks.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/flatpacks.dm#L29) — флэтпак носится только двумя руками
- **Статус в ss13-rs:** ❌ нет

### Хартия станции
Капитан один раз за раунд переименовывает станцию хартией (с админским вето); баннер-вариант для других карт.

- **Код:** [`/obj/item/station_charter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/charter.dm#L1), [`rename_station()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/charter.dm#L85)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/misc_items/charter.rs`: в руке в первые 5 минут спрашивает название; стандартный формат (`strings/station_*.txt`, числительные) принимается сразу, иное уходит админам (`CommsRequest`) и одобряется само через 60 с, если не отклонено (`reject_proposed`); переименование, объявление, имя и описание хартии, одноразовость (админская — без лимитов), баннер называет планету. Кнопка REJECT в админ-панели — admin раздел (нужен новый `AdminRequest`)

### Секретные документы
Папки с секретными документами НТ и Синдиката — цели краж, копируются ксероксом с подделкой.

- **Код:** [`/obj/item/documents`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/secret_documents.dm#L10)
- **BandaStation:** [`/obj/item/documents/syndicate/exchange_red`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/secret_documents.dm#L3) — документы для обмена между агентами, кейс и пинпоинтер к нему
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/misc_items/documents.rs`: неуничтожимые (`resistance_flags`, `obj_damage`), цель кражи `/datum/objective_item/steal/documents` извлекается в `antag.ron` вместе с прочими; ксерокс (`service/office.rs`) копирует их за 20 листов в фотокопию с `copy_type` (копия копии помнит оригинал), красный или синий мелок один раз подделывает печать. Обмен документами между агентами BandaStation (кейс, пинпоинтер) — цель предателя, раздел антагонистов

### Мишени тира
Мишени для стрельбы, на которых остаются следы попаданий.

- **Код:** [`/obj/item/target`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/shooting_range.dm#L1)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/misc_items/target.rs`: прицельный выстрел попадает в недвижную мишень (`projectile.rs`), враждебный brute/burn снаряд (и тренировочный лазер) в 3 из 4 случаев оставляет вмятину или подпалину в точке прицела (до 35), сварка стирает следы, мишень клоуна гудит. Без проверки пикселя иконки (клиент выбирает мишень по непрозрачному пикселю); стойки для мишеней (`target_stake`) — не в этой записи

### Ключ-карты пазлов
Цветные ключ-карты для дверей-головоломок на руинах.

- **Код:** [`/obj/item/keycard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/puzzle_pieces.dm#L6)
- **Статус в ss13-rs:** ❌ нет

### Плакаты-пикеты, молоток судьи, точилка и мелочи
Пикетный плакат (пишется ручкой), судейский молоток и подставка, точилка для лезвий, свинцовая труба из техтоннелей, приколи хвост ослу, лягушачья статуэтка (призыв лягушки), статуэтка Девы Марии, медвежья броня, эфириальный диско-шар, дегустатор, наркотики-«еда».

- **Код:** [`/obj/item/picket_sign`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/signs.dm#L1), [`/obj/item/gavelhammer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/courtroom.dm#L5), [`/obj/item/sharpener`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/sharpener.dm#L9), [`/obj/item/lead_pipe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/maintenance_loot.dm#L6), [`/obj/item/tail_pin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tail_pin.dm#L1), [`/obj/item/frog_statue`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/frog_statue.dm#L6), [`/obj/item/virgin_mary`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/virgin_mary.dm#L1), [`/obj/item/bear_armor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/bear_armor.dm#L1), [`/obj/item/etherealballdeployer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/etherealdiscoball.dm#L1), [`/obj/item/taster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/taster.dm#L1), [`/obj/item/food/drug`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/drug_items.dm#L1)
- **BandaStation:** [`/obj/item/lead_pipe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/maintenance_loot.dm#L1) — правка свинцовой трубы
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`, `crates/ss13_game/src/combat.rs`: предметы поднимаются и бьют по `force`; особых действий нет

### Настенные рамы
Предмет-рамка (APC, пожарная тревога, кнопка, экран) монтируется на стену в машину; плата электроники.

- **Код:** [`/obj/item/wallframe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/wall_mounted.dm#L1), [`/obj/item/electronics`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/wall_mounted.dm#L126)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/wallmount/frames.rs`: рама на стене (в линию, пол, занятая сторона) становится своим `result_path` со сдвигом, отвёртка вешает на стену впереди, ключ разбирает в материалы (кнопка, выключатель света, флешер без лампы, шкафы огнетушителя и топора); рамы зеркал, светильников, камер — в своих модулях. Рамы пожарной тревоги, APC, воздушной тревоги, интеркома, экранов и прочих машин со стадиями сборки — в инженерном разделе (сборка самих машин); платы `/obj/item/electronics` — там же

Помада, краска для волос — см. ../service/; голочипы — см. ../cargo/.
