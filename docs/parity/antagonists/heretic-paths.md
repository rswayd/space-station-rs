# Еретик: пути Мансуса
Восемь путей еретика. Каждый путь — цепочка знаний: стартовый клинок и метка, заклинания, броня, улучшение клинка, финальное вознесение.

### Путь Пепла
Огонь и пепел: «Секрет Ночного Дозорного», пепельный проход (джаунт), взрыв вулкана, выжженная мантия, фонарь Ночного Дозорного, огненный клинок, перерождение Ночного Дозорного; вознесение «Ритуал Пепельного Лорда» — иммунитет к огню и огненное кольцо.

- **Код:** [`ash_lore.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/ash_lore.dm#L1), [`/datum/heretic_knowledge/limited_amount/starting/base_ash`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/ash_lore.dm#L50), [`/datum/heretic_knowledge/ultimate/ash_final`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/ash_lore.dm#L193)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/heretic/server.rs`: стартовое знание (пепельный клинок, метка, ожог глаз — сообщение), «Взрыв вулкана» (2.5 с, 20 ожога, 3 стака огня, перескоки на горящих, 45 с), «Перерождение Ночного Дозорного» (радиус 14, лечение 10 за жертву, смерть в крите), «Огненный клинок» (поджог), вознесение; `// shortcut:` «Пепельный проход», пассив, усиление мантией и фонарь не работают; без проверки `view()` у луча

### Путь Клинка
Дуэлянт: «Совершенство Остроты», реорганизация, стойка истерзанного чемпиона, разбитые доспехи, «Волк среди овец» (арена), усиление клинков, «Ярость Стали» (кружащие клинки); вознесение «Серебряный Вихрь».

- **Код:** [`blade_lore.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/blade_lore.dm#L1), [`/datum/heretic_knowledge/limited_amount/starting/base_blade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/blade_lore.dm#L53), [`/datum/heretic_knowledge/ultimate/blade_final`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/blade_lore.dm#L359)
- **Статус в ss13-rs:** ❌ нет

### Путь Космоса
Звёзды и телепорты: «Вечные Врата», космические руны, звёздный взрыв, плащ из звёзд, касание звезды, космический клинок, космическая экспансия; вознесение «Дар Создателей» — призыв Звёздного наблюдателя.

- **Код:** [`cosmic_lore.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/cosmic_lore.dm#L1), [`/datum/heretic_knowledge/limited_amount/starting/base_cosmic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/cosmic_lore.dm#L50), [`/datum/heretic_knowledge/ultimate/cosmic_final`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/cosmic_lore.dm#L232)
- **Статус в ss13-rs:** ❌ нет

### Путь Плоти
Гули и монстры: «Главенство Голода», несовершенный ритуал (гули), переплетение плоти, извивающиеся объятия, сырой пророк, кровоточащая сталь, сталкер; вознесение «Последний Гимн Жреца» — превращение в червя-Armsy.

- **Код:** [`flesh_lore.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/flesh_lore.dm#L1), [`/datum/heretic_knowledge/limited_amount/starting/base_flesh`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/flesh_lore.dm#L52), [`/datum/heretic_knowledge/ultimate/flesh_final`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/flesh_lore.dm#L301)
- **Статус в ss13-rs:** ❌ нет

### Путь Замка
Двери и доступ: «Секрет Хранителя Ключей», связка ключей (открывает двери), обряд Хранителя, меняющийся облик, ловкость воришки, открывающий клинок, убежище Хранителя; вознесение «Открытие Лабиринта» — разрывы реальности со спавном монстров.

- **Код:** [`lock_lore.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/lock_lore.dm#L1), [`/datum/heretic_knowledge/limited_amount/starting/base_lock`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/lock_lore.dm#L48), [`/datum/heretic_knowledge/ultimate/lock_final`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/lock_lore.dm#L237)
- **Статус в ss13-rs:** ❌ нет

### Путь Луны
Безумие: «Лунная тропа», врата разума, лунный амулет, лунный парад, лунный клинок, «Восшествие Артистов»; вознесение «Последний Акт» — массовое безумие и лунатики.

- **Код:** [`moon_lore.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/moon_lore.dm#L1), [`/datum/heretic_knowledge/limited_amount/starting/base_moon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/moon_lore.dm#L54), [`/datum/heretic_knowledge/ultimate/moon_final`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/moon_lore.dm#L204)
- **Статус в ss13-rs:** ❌ нет

### Путь Ржавчины
Ржавчина и разрушение: «Сказание Кузнеца», ржавый рывок, возведение ржавчины, уцелевшие обломки, агрессивное распространение, токсичный клинок, шлейф энтропии; вознесение «Клятва Несущего Ржавчину» — ржавление станции.

- **Код:** [`rust_lore.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/rust_lore.dm#L1), [`/datum/heretic_knowledge/limited_amount/starting/base_rust`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/rust_lore.dm#L53), [`/datum/heretic_knowledge/ultimate/rust_final`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/rust_lore.dm#L194)
- **Статус в ss13-rs:** ❌ нет

### Путь Пустоты
Холод и космос: «Проблеск Зимы», фаза Пустоты, пустотная тюрьма, сплетение Пустоты, притяжение Пустоты, ищущий клинок, поток Пустоты; вознесение «Вальс Конца Времен» — буря и космическая неуязвимость.

- **Код:** [`void_lore.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/void_lore.dm#L1), [`/datum/heretic_knowledge/limited_amount/starting/base_void`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/void_lore.dm#L50), [`/datum/heretic_knowledge/ultimate/void_final`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/void_lore.dm#L294)
- **Статус в ss13-rs:** ❌ нет

### Броня еретика
Знания брони по путям дают особые робы/доспехи с капюшоном и свойствами пути.

- **Код:** [`heretic_armor_knowledge.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/knowledge/heretic_armor_knowledge.dm#L1), [`/obj/item/clothing/suit/hooded/cultrobes/eldritch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/items/heretic_armor.dm#L6)
- **Статус в ss13-rs:** 🟡 частично — ритуалы брони всех путей в дереве `crates/ss13_game/src/antag/heretic/data.rs` создают робу при наличии прототипа; свойств нет

### Заклинания еретика
Около 40 заклинаний в `magic/`: хватка Мансуса, телепатия, слепота, ЭМИ, теневой плащ, проход сквозь зеркала, космическое ползание, переплетение плоти, ржавая волна, холодный конус Пустоты, кровавое рассечение и др.

- **Код:** [`code/modules/antagonists/heretic/magic`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/magic), [`/datum/heretic_knowledge/spell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/heretic/heretic_knowledge.dm#L210)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/heretic/server.rs` (`refresh`, `cast_spell`): кнопки с зарядами для всех заклинаний знаний; работают хватка Мансуса, «Взрыв вулкана», «Перерождение», «Клятва пламени», «Огненный каскад»; остальные сообщают, что не перенесены
