# Игрушки и развлекательные предметы
Игрушки `/obj/item/toy`, плюшевые игрушки, кости, фигурки и прочие безделушки.

### Плюшевые игрушки
Мягкие игрушки пищат при сжатии, в них можно спрятать мелкий предмет или гранату; две плюшки «влюбляются» и рожают маленькую; особые плюшки (Нар'Си, ящерица, моль, рататуй…).

- **Код:** [`/obj/item/toy/plush`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/plushes.dm#L1), [`love()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/plushes.dm#L184), [`make_young()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/plushes.dm#L314)
- **BandaStation:** [`plushes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/plushes.dm#L12) — настраиваемая задержка писка; [`/obj/item/toy/plush/hampter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/hampters.dm#L1), [`/obj/item/toy/plush/pig`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/pig.dm#L1) — плюшевые хомяки и свинья
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: подбираются, помещаются в хранилище костюма; нет писка, тайника, «любви»

### Игрушечное оружие
Пистолет-пистонник, пенопластовые мечи и катаны, игрушечный кинжал, мини-метеор, пенопластовые дротики, пистоны; без урона, но выглядят настоящими.

- **Код:** [`/obj/item/toy/gun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L509), [`/obj/item/toy/sword`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L591), [`/obj/item/toy/katana`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L807), [`/obj/item/toy/foamblade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L707), [`/obj/item/toy/snappop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L836)
- **Статус в ss13-rs:** ❌ нет

### Шары и водяные бомбочки
Надувные шарики (лопаются), воздушные фигурки, водяные шарики (наполняются и разбрызгивают реагент при попадании), снежки, пляжный мяч, латексные перчатки-шарики.

- **Код:** [`/obj/item/toy/balloon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L133), [`/obj/item/toy/waterballoon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L50), [`/obj/item/toy/snowball`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L1089), [`/obj/item/toy/beach_ball`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L1110), [`/obj/item/latexballoon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/latexballoon.dm#L5)
- **Статус в ss13-rs:** ❌ нет

### Говорящие игрушки и экшен-фигурки
Фигурки всех должностей и говорящие игрушки произносят фразы при использовании; «Капитан сказал», игрушечная ядерка (отсчёт), красная кнопка, кукла чревовещателя.

- **Код:** [`/obj/item/toy/talking`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L890), [`/obj/item/toy/figure`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L1214), [`/obj/item/toy/captainsaid`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L398), [`/obj/item/toy/nuke`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L991), [`/obj/item/toy/redbutton`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L1063), [`/obj/item/toy/dummy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L1449)
- **Статус в ss13-rs:** ❌ нет

### Игрушечные мехи
Коллекционные мини-мехи: два игрока устраивают пошаговую дуэль мехов.

- **Код:** [`/obj/item/toy/mecha`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toy_mechs.dm#L14), [`mecha_brawl()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toy_mechs.dm#L282)
- **Статус в ss13-rs:** ❌ нет

### Кости и шар восьмёрка
Кубики d4–d100 (бросок в руке или броском), проклятые и фальшивые; шар восьмёрка отвечает на вопрос (одержимый — голосованием призраков через tgui).

- **Код:** [`/obj/item/dice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/dice.dm#L48), [`diceroll()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/dice.dm#L77), [`/obj/item/toy/eightball`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/eightball.dm#L1), [`/obj/item/toy/eightball/haunted`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/eightball.dm#L113); tgui: [`EightBallVote.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/EightBallVote.tsx)
- **Статус в ss13-rs:** ❌ нет

### Прочие игрушки
Заводной тулбокс, юла, кошачья игрушка, пенопластовый палец, «Интенто» (игра на память), сломанная рация, ракушки, мозг-игрушка, удлинитель-рука, бан-молот, горячая картошка (бомба, которую надо передать), подушки (бои подушками), скейтборды, скаб, бюсты и фигурки, картонные фигуры в рост (раскрашиваются мелками), песочные часы, копилка (разбивается), переноска для питомцев.

- **Код:** [`/obj/item/toy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L31), [`/obj/item/toy/intento`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L1628), [`/obj/item/hot_potato`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/hot_potato.dm#L2), [`/obj/item/pillow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/pillow.dm#L4), [`/obj/item/melee/skateboard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/skateboards.dm#L2), [`/obj/item/skub`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/skub.dm#L1), [`/obj/item/statuebust`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/busts_and_figurines.dm#L1), [`/obj/item/cardboard_cutout`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cardboard_cutouts.dm#L2), [`/obj/item/hourglass`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/hourglass.dm#L3), [`/obj/item/piggy_bank`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/piggy_bank.dm#L5), [`/obj/item/pet_carrier`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/pet_carrier.dm#L5)
- **BandaStation:** [`toys.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/toys.dm#L1886) — исправление пацифизма для игрушечного оружия
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`, `crates/ss13_game/src/throw.rs`: поднимаются, бросаются; особых действий нет

Карты и ККИ — см. ../service/.
