# Еретик: предметы, структуры и призывы
Создаваемые ритуалами предметы, структуры и монстры еретика.

### Клинки Мансуса
Серповидные клинки по путям (ржавчина, пепел, плоть, пустота, клинок, космос, замок, луна): ломаются в руке для телепорта, детонируют метки, улучшаются знаниями; лимит созданных.

- **Код:** [`/obj/item/melee/sickly_blade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/heretic_blades.dm#L2), [`/obj/item/melee/sickly_blade/rust`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/heretic_blades.dm#L105)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/heretic/server.rs` (`attack_self`, `heretic_clicks`, `after_melee`): ломание клинка — телепорт на безопасный тайл (пока не сняты ограничения), не-еретик парализован на 5 с, удар еретика активирует метку и улучшение пути, лимит 2 с правкой BandaStation; особых эффектов клинков других путей нет

### Фокусы и амулеты
Фокус (нужен для заклинаний в броне не-еретика), багровый медальон, лунный амулет, медальон (тир-1).

- **Код:** [`/obj/item/clothing/neck/heretic_focus`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/heretic_necks.dm#L1), [`/obj/item/clothing/neck/heretic_focus/crimson_medallion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/heretic_necks.dm#L36)
- **Статус в ss13-rs:** ❌ нет

### Кодексы и книги
Кодекс Цикатрикс (быстрое рисование рун, сбор влияний), справочник Лабиринта (стены-силовые поля).

- **Код:** [`/obj/item/codex_cicatrix`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/forbidden_book.dm#L2), [`/obj/item/heretic_labyrinth_handbook`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/labyrinth_handbook.dm#L21)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/heretic/server.rs` (`item_interaction`): Кодекс Цикатрикс добывает влияние за 2 очка; ритуал создаёт его; нет открытия книги и быстрого рисования руны

### Оружие и снаряжение
Багровый тесак, винтовка Охотника на львов и патроны, граната «Сеятель ржавчины», маска Безумия, поножи Пророка, фонарь Ночного Дозорного, непостижимая реликвия (пояс), связка ключей и ID-карта еретика (порталы замка), шар пустотной тюрьмы, благословенный мак.

- **Код:** [`/obj/item/knife/butcher/heretic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/crimson_cleaver.dm#L1), [`/obj/item/gun/ballistic/rifle/lionhunter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/hunter_rifle.dm#L7), [`/obj/item/grenade/chem_grenade/rust_sower`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/heretic_grenade.dm#L6), [`/obj/item/clothing/mask/madness_mask`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/madness_mask.dm#L2), [`/obj/item/clothing/shoes/greaves_of_the_prophet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/heretic_shoes.dm#L1), [`/obj/item/flashlight/lantern/heretic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/heretic_lantern.dm#L1), [`/obj/item/storage/belt/unfathomable_curio`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/unfathomable_curio.dm#L3), [`/obj/item/card/id/advanced/heretic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/keyring.dm#L97), [`/obj/item/void_prison`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/voidballd.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Фляги, филактерия и испорченные органы
Эльдрическая фляга, филактерия проклятия, испорченные глаза/язык и др. органы с побочными эффектами.

- **Код:** [`/obj/item/reagent_containers/cup/beaker/eldritch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/eldritch_flask.dm#L3), [`/obj/item/reagent_containers/cup/phylactery`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/eldritch_flask.dm#L12), [`/obj/item/organ/eyes/corrupt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/corrupted_organs.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Картины Мансуса
Незапечатанные эльдрические картины на стенах, накладывающие эффекты на смотрящих.

- **Код:** [`/obj/item/wallframe/painting/eldritch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/eldritch_painting.dm#L2), [`/obj/structure/sign/painting/eldritch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/eldritch_painting.dm#L12)
- **Статус в ss13-rs:** ❌ нет

### Структуры
Зубастый тигель (зелья из органов), резной нож и эльдрические ловушки-руны, поток Пустоты, разрыв Лабиринта (вознесение замка).

- **Код:** [`/obj/structure/destructible/eldritch_crucible`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/structures/mawed_crucible.dm#L2), [`/obj/item/eldritch_potion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/structures/mawed_crucible.dm#L244), [`/obj/item/melee/rune_carver`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/structures/carving_knife.dm#L2), [`/obj/structure/trap/eldritch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/structures/carving_knife.dm#L167), [`/obj/structure/void_conduit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/structures/void_conduit.dm#L1), [`/obj/structure/lock_tear`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/structures/lock_final.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Призванные монстры
Сырой пророк (телепатическая связь, дальний взгляд), сталкер плоти, червь Armsy, горничная из Зазеркалья, ржавый ходок, огненная акула, пепельный дух, Звёздный наблюдатель.

- **Код:** [`/mob/living/basic/heretic_summon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/heretic/_heretic_summon.dm#L1), [`/mob/living/basic/heretic_summon/raw_prophet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/heretic/raw_prophet.dm#L6), [`/mob/living/basic/heretic_summon/stalker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/heretic/flesh_stalker.dm#L2), [`/mob/living/basic/heretic_summon/armsy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/heretic/flesh_worm.dm#L6), [`/mob/living/basic/heretic_summon/maid_in_the_mirror`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/heretic/maid_in_the_mirror.dm#L2), [`/mob/living/basic/heretic_summon/star_gazer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/heretic/star_gazer.dm#L1)
- **Статус в ss13-rs:** ❌ нет
