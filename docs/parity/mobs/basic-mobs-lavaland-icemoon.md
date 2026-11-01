# Фауна Лаваленда, Ледяной луны и джунглей
Шахтёрские мобы (`basic/lavaland`, `basic/icemoon`, `basic/jungle`), боссы на basic-основе (`basic/boss`) и шахтёрские помощники. Мегафауна на старой основе — в [simple-animals.md](simple-animals.md). В ss13-rs нет ни планет, ни этих мобов.

### Базовый шахтёрский моб
Общий тип `/mob/living/basic/mining`: устойчив к пеплу и лаве, уязвим к кинетическому акселератору (полный урон при низком давлении), трофеи для крашера.

- **Код:** [`/mob/living/basic/mining`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/mining.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Голиафы
Медленные бронированные звери, бьют щупальцами из-под земли; шкура — броня шахтёров; приручаемые (ездовые); древний голиаф.

- **Код:** [`/mob/living/basic/mining/goliath`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/goliath/goliath.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Легионы
Черепа-легионы выпускают летающих выводков; убитый ими человек становится легионом; снежный, карликовый, большой, «легион-мартышка». Сердце легиона и мёртвый шахтёр с лутом.

- **Код:** [`/mob/living/basic/mining/legion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/legion/legion.dm#L5), [`/mob/living/basic/mining/legion_brood`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/legion/legion_brood.dm#L2), [`/mob/living/basic/mining/legion/lesser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/legion/legion_monkey.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Наблюдатели (watchers)
Летающие глаза: замораживающие лучи, взгляд-ослепление; редкие магмовые и ледяные крылья.

- **Код:** [`/mob/living/basic/mining/watcher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/watcher/watcher.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Хайвлорды, бильчерви, базилиски, бримдемоны
Хайвлорд плодит выводок (ядро хайвлорда — лечение), бильчерви зарываются и плюются желчью (вилечервь), базилиск охлаждает лучом, бримдемон стреляет лучом по прямой.

- **Код:** [`/mob/living/basic/mining/hivelord`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/hivelord/hivelord.dm#L2), [`/mob/living/basic/mining/bileworm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/bileworm/_bileworm.dm#L1), [`/mob/living/basic/mining/basilisk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/basilisk/basilisk.dm#L2), [`/mob/living/basic/mining/brimdemon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/brimdemon/brimdemon.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Голдграбы, гатланчеры, лобстрозити
Голдграб глотает руду (убить — получить), гатланчеры (пепельные ходоки разводят их: молоко, воины), лобстрозити (обычные/лавовые, молодь).

- **Код:** [`/mob/living/basic/mining/goldgrub`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/goldgrub/goldgrub.dm#L2), [`/mob/living/basic/mining/gutlunch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/gutlunchers/gutlunchers.dm#L6), [`/mob/living/basic/mining/lobstrosity`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/lobstrosity/lobstrosity.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Муки (mook)
Деревня муков: рабочие собирают руду, бард играет, вождь племени.

- **Код:** [`/mob/living/basic/mining/mook`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/mook/mook.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Рапторы
Ездовые рапторы ранчо: цвета с разными свойствами (красный, фиолетовый, зелёный, белый, чёрный), разведение и наследование черт, птенцы.

- **Код:** [`/mob/living/basic/raptor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/raptor/_raptor.dm#L10)
- **Статус в ss13-rs:** ❌ нет

### Тендрилы
Логова на Лаваленде порождают мобов; разрушенный тендрил открывает сундук с некрополисным лутом и проваливает землю.

- **Код:** [`/mob/living/basic/mining/tendril`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/tendril/tendril.dm#L7)
- **Статус в ss13-rs:** ❌ нет

### Мобы Ледяной луны
Ледяной демон (послеобразы, телепорт), ледяные/пепельные виверны (огонь), полярные медведи (ярость при ранении), волки (стаи, приручаются).

- **Код:** [`/mob/living/basic/mining/ice_demon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/icemoon/ice_demon/ice_demon.dm#L1), [`/mob/living/basic/mining/ice_whelp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/icemoon/ice_whelp/ice_whelp.dm#L1), [`/mob/living/basic/mining/polarbear`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/icemoon/polar_bear/polar_bear.dm#L2), [`/mob/living/basic/mining/wolf`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/icemoon/wolf/wolf.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Мобы джунглей
Липер (ядовитые пузыри, прыжки), мега-арахнид (сети, кислота), сеянцы (лучи, лечат растения), венерина мухоловка (лозы-захват, гост-роль из семян).

- **Код:** [`/mob/living/basic/leaper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/jungle/leaper/leaper.dm#L1), [`/mob/living/basic/mega_arachnid`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/jungle/mega_arachnid/mega_arachnid.dm#L3), [`/mob/living/basic/seedling`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/jungle/seedling/seedling.dm#L9), [`/mob/living/basic/venus_human_trap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/jungle/venus_human_trap.dm#L129)
- **Статус в ss13-rs:** ❌ нет

### Боссы на basic-основе
Кровожадный шахтёр (рывки, крашер; варианты guidance/hunter/doom) и «Тварь» (thing) — с фазами и наградами.

- **Код:** [`/mob/living/basic/boss`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/boss/boss.dm#L4), [`/mob/living/basic/boss/blood_drunk_miner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/boss/blood_drunk_miner/_blood_drunk_miner.dm#L12), [`/mob/living/basic/boss/thing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/boss/thing/thing.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Майнботы и нод-дроны
Майнбот шахтёра (собирает руду, стреляет, команды, улучшения, ИИ-личность) и нод-дрон сканера жил (охраняется от волн мобов).

- **Код:** [`/mob/living/basic/mining_drone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/minebots/minebot.dm#L1), [`/mob/living/basic/node_drone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/lavaland/node_drone/node_drone.dm#L12)
- **Статус в ss13-rs:** ❌ нет
