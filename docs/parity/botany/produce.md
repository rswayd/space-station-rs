# Урожай: плоды и семейства растений
Что даёт ботаника: съедобные плоды, несъедобные «выращенные» предметы и особые растения. Семейства сгруппированы; рецепты из плодов — см. ../kitchen/.

### Съедобный плод (базовый тип)
`/obj/item/food/grown` наследует статы семени: реагенты по потенции, размер укуса от потенции и объёма, трейты-гены; сохнет в сушилке, бродит в бочке (`ferment`), перемалывается/выжимается в блендере, оставляет мусор (кожура, огрызок).

- **Код:** [`/obj/item/food/grown`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown.dm#L13), [`Initialize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown.dm#L50), [`ferment()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown.dm#L115), [`grind_atom()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown.dm#L143), [`generate_trash()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown.dm#L100)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/plants.rs`, `crates/ss13_game/src/botany/produce.rs`, `crates/ss13_game/src/food.rs`: плоды из урожая несут своё семя (реагенты по потенции), едятся, режутся, превращаются в семена в экстракторе, компостируются; трейты продукта действуют (см. ./seeds-genetics.md), мусор-кожура получает семя плода; нет сушки и бочки (`ferment` — только трейтом автодистилляции), блендера (см. ../kitchen/)

### Несъедобные выращенные предметы
`/obj/item/grown` — продукты-не-еда со статами семени: бревна, хлопок, кочерыжки, новацвет, угольная роза, кустарник; часть служит оружием или материалом.

- **Код:** [`/obj/item/grown`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/growninedible.dm#L5)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/mod.rs`, `crates/ss13_game/src/botany/produce.rs`: несъедобные плоды собираются, несут семя и его трейты (скользкость, свечение, раздавливание, батарейка); собственных свойств видов (оружие, материалы) нет — см. записи семейств ниже

### Овощи, фрукты, ягоды и злаки
Основная масса семейств для кухни: яблоки, бананы, бобы/соя, ягоды/виноград, злаки (пшеница, овёс, рис, мясная пшеница), вишни, цитрусы, какао/ваниль, кукуруза, огурцы, баклажаны, чеснок, дыни/арбузы, оливки, лук, арахис, горох, ананас, сливы, картофель, тыквы, корнеплоды (морковь, пастернак, свёкла), сахарный тростник, чай/кофе, томаты, корта-орех. У большинства есть мутации в родственные виды.

- **Код:** [`grown/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown), [`/obj/item/seeds/apple`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/apple.dm#L2), [`/obj/item/seeds/wheat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/cereals.dm#L2), [`/obj/item/seeds/tomato`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/tomato.dm#L2), [`/obj/item/seeds/potato`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/potato.dm#L2), [`/obj/item/seeds/korta_nut`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/korta_nut.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/`, `crates/ss13_mapload/src/plants.rs`: растут в лотках и собираются, едятся и режутся; особых свойств отдельных видов нет

### Банан и банановая кожура
Банан оставляет кожуру, на которой поскальзываются; варианты — мимана (тихая), блюспейс-кожура (телепортирует), гро-мишель.

- **Код:** [`/obj/item/seeds/banana`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/banana.dm#L2), [`/obj/item/grown/bananapeel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/banana.dm#L62), [`bananapeel/bluespace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/banana.dm#L145)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/produce.rs`, `crates/ss13_game/src/food.rs`: съеденный банан оставляет кожуру с его семенем; кожура скользкая (`potency × 1.6` дс, до 14 с), блюспейс-кожура телепортирует поскользнувшегося, мимана и гро-мишель — как обычная кожура; нет бумеранга изогнутого банана, связки бананов (созревание, обезьянья бомба), гниения гро-мишель

### Грибы
Рейши, мухомор, ангел смерти, грибы свободы (псилоцибин), пухлый шлемник, ходячий гриб (становится мобом), лисички, светящиеся грибы (сажаются на пол, светят), теневые и т.п.; растут под `fungal_metabolism`.

- **Код:** [`/obj/item/seeds/reishi`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/mushrooms.dm#L11), [`/obj/item/seeds/amanita`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/mushrooms.dm#L38), [`walkingmushroom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/mushrooms.dm#L163), [`/obj/item/seeds/glowshroom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/mushrooms.dm#L245)
- **Статус в ss13-rs:** ❌ нет

### Цветы и букеты
Мак, лилия, колокольчик, подсолнух, новацвет (поджигает при ударе), роза (колючая), угольная роза, радужный букет; из цветов собираются букеты.

- **Код:** [`/obj/item/seeds/poppy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/flowers.dm#L11), [`/obj/item/seeds/sunflower`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/flowers.dm#L171), [`/obj/item/grown/novaflower`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/flowers.dm#L253), [`/obj/item/seeds/rose`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/flowers.dm#L277), [`/obj/item/bouquet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/bouquets.dm#L2), [`/obj/item/seeds/rainbow_bunch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/rainbow_bunch.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Башенные грибы, бревна и бамбук
Башенный гриб даёт бревна: топором/пилой — в доски, бревна + стержни — костёр; варианты: стальной гриб (бревно в металл), бамбук.

- **Код:** [`/obj/item/seeds/tower`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/towercap.dm#L1), [`/obj/item/grown/log`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/towercap.dm#L33), [`log/item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/towercap.dm#L87), [`log/steel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/towercap.dm#L119), [`/obj/item/grown/log/bamboo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/sugarcane.dm#L49), [`/obj/structure/bonfire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/bonfire.dm#L10)
- **Статус в ss13-rs:** ❌ нет

### Хлопок, трава, ковры и кустарники
Хлопок и дюратред — в ткань; трава/волшебная трава/ковровая трава — в плитки газона и ковра; кустарник — в живую изгородь (`/obj/structure/hedge`); органное дерево.

- **Код:** [`/obj/item/seeds/cotton`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/cotton.dm#L1), [`/obj/item/grown/cotton/durathread`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/cotton.dm#L66), [`/obj/item/seeds/grass`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/grass_carpet.dm#L2), [`/obj/item/seeds/shrub`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/hedges.dm#L1), [`/obj/structure/hedge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/hedges.dm#L35)
- **Статус в ss13-rs:** ❌ нет

### Курительные и лекарственные растения
Табак и космический табак, конопля (варианты: смертельная, жизни, радужная, ультра) — сушатся и скручиваются в сигареты; амброзия (вульгарис, дэус, гайя), алоэ, травы, кронкус — лечебные реагенты.

- **Код:** [`/obj/item/seeds/tobacco`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/tobacco.dm#L2), [`/obj/item/seeds/cannabis`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/cannabis.dm#L2), [`/obj/item/seeds/ambrosia`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/ambrosia.dm#L13), [`/obj/item/seeds/aloe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/aloe.dm#L3), [`/obj/item/seeds/herbs`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/herbs.dm#L2), [`/obj/item/seeds/kronkus`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/kronkus.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Опасные растения
Гатфрукт (плод — револьвер), вишнёвая бомба (взрыв по потенции), томат-убийца (моб), призрачный чили, огненный лимон, ствольная дыня, святая дыня (антимагия), блюспейс-томат.

- **Код:** [`/obj/item/seeds/gatfruit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/gatfruit.dm#L3), [`/obj/item/food/grown/cherry_bomb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/cherries.dm#L102), [`/obj/item/food/grown/tomato/killer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/tomato.dm#L125), [`/obj/item/seeds/firelemon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/citrus.dm#L94), [`holymelon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/melon.dm#L118)
- **Статус в ss13-rs:** ❌ нет

### Реплика-под (клонирование растением)
Семена капусты мутируют в реплика-под; если ввести в семена кровь с ДНК умершего, при сборе вырастает подмен (podperson) с его душой.

- **Код:** [`/obj/item/seeds/replicapod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/replicapod.dm#L33), [`replicapod/harvest()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/replicapod.dm#L114)
- **Статус в ss13-rs:** ❌ нет

### Сорняки: кудзу, крапива, чертополох
Семена кудзу высаживаются рукой на пол и запускают разрастание космической лозы (мутации лозы — событие `spacevine`); крапива и смертельная крапива жгут руки; звёздный и галактический чертополох — инвазивные сорняки.

- **Код:** [`/obj/item/seeds/kudzu`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/weeds/kudzu.dm#L3), [`kudzu/plant()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/weeds/kudzu.dm#L32), [`/obj/item/seeds/nettle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/weeds/nettle.dm#L1), [`/obj/item/seeds/starthistle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/weeds/starthistle.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Саженец (seedling)
Семена-саженец при сборе выпускают дружественного (или злого) моба-саженца.

- **Код:** [`/obj/item/seeds/seedling`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/seedling.dm#L1), [`seedling/evil`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/seedling.dm#L26)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/tray.rs`: стадии роста растения в лотке (спрайты роста из DM); отдельного предмета-саженца нет

Растения Лаваленда (`/obj/item/seeds/lavaland`, `code/modules/mining/lavaland/ash_flora.dm`) — см. ../cargo/.
