# Особое дальнобойное оружие
Шприцемёты, химпушки, пусковые и прочее нестандартное оружие. Магические жезлы и посохи — у агента antagonists (волшебник).

### Шприцемёты и духовые трубки
Стреляют заряженными шприцами (реагенты вводятся при попадании), скорострельный, синдикатский, ДНК-шприцемёт, духовая трубка.

- **Код:** [`/obj/item/gun/syringe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/special/syringe_gun.dm#L1), [`/obj/item/gun/syringe/rapidsyringe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/special/syringe_gun.dm#L137), [`/obj/item/gun/syringe/dna`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/special/syringe_gun.dm#L174), [`/obj/item/gun/syringe/blowgun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/special/syringe_gun.dm#L213)
- **Статус в ss13-rs:** ✅ `guns_special`: шприцы (ДНК-инъекторы у ДНК-шприцемёта) вставляются в шприцемёт в другой руке до `max_syringes` (скорострельный — 6), Z вынимает последний; выстрел несёт реагенты первого шприца в дротике, при попадании в карбона всё вводится в кровь, если толстая одежда не закрывает зону (пронзающий шприц пробивает её); дартс-пистолет начинает с шприцем, духовая трубка отнимает у стрелка 20 выносливости и 20 кислорода. Без режима низкого давления (встраивание шприцев — нет встраивания) и оверлеев

### Химическая пушка
Самостоятельно синтезирует дротики из своего резервуара реагентов.

- **Код:** [`/obj/item/gun/chem`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/special/chem_gun.dm#L3)
- **Статус в ss13-rs:** ✅ `guns_special`: резервуар 90 ед. (открытый контейнер), 4 пронзающих дротика по 15 ед. из резервуара, новый дротик каждые 25 с

### Гранатомёт и бласт-пушка
Ручной гранатомёт для обычных гранат; бласт-пушка стреляет взрывом бомбы-ТТВ в линию.

- **Код:** [`/obj/item/gun/grenadelauncher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/special/grenade_launcher.dm#L1), [`/obj/item/gun/blastcannon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/special/blastcannon.dm#L12)
- **Статус в ss13-rs:** 🟡 гранатомёт готов (`guns_special`): до 3 гранат (не C4) из другой руки, выстрел бросает первую с тайла стрелка на 30 тайлов и взводит её на 1,5 с; бласт-пушки (TTV) нет

### Медлуч
Лечащий луч (ЦК/ЕРТ): постоянно лечит цель, пересечение лучей взрывается.

- **Код:** [`/obj/item/gun/medbeam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/special/medbeam.dm#L1)
- **Статус в ss13-rs:** ✅ `guns_special`: выстрел по живой цели берёт её на луч (новый выстрел отпускает старый), раз в 2 с (тик SSobj) цель лечит 4 ушиба, 4 ожога, 1 токсин и 1 кислород; луч рвётся, если ружьё не в руках стрелявшего, дальше 8 тайлов или плотное препятствие на линии; пересечение с лучом другого стрелка — взрыв (0/3/5, вспышка 8). Без спрайта луча

### Мясной крюк и рука Мидаса
Крюк притягивает цель к стрелку; рука Мидаса превращает золото в снаряды.

- **Код:** [`/obj/item/gun/magic/hook`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/special/meat_hook.dm#L5), [`/obj/item/gun/magic/midas_hand`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/special/hand_of_midas.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Пневмопушка, огнемёт, метательные звёзды
Пневмопушка стреляет любыми предметами сжатым газом (пироги у клоуна); огнемёт поджигает линию тайлов из плазменного баллона; метательные звёзды встраиваются в тело.

- **Код:** [`/obj/item/pneumatic_cannon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/ranged/pneumatic_cannon.dm#L14), [`/obj/item/flamethrower`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/ranged/flamethrower.dm#L1), [`/obj/item/throwing_star`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/ranged/throwing_star.dm#L1)
- **Статус в ss13-rs:** ❌ нет
