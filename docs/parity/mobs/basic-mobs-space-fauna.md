# Космическая фауна
`code/modules/mob/living/basic/space_fauna`: враждебные и нейтральные существа космоса, событий и антагонистов-мобов. В ss13-rs есть карпы (`crates/ss13_game/src/mobs/`).

### Космические карпы
Главная угроза космоса: стаи карпов (событие миграции) летают в вакууме, кусают, телепортируются по рифтам. Варианты: голографический, ручные Лиа (питомец ГСБ) и Кайенна (нюки), мегакарп, магикарпы (стреляют заклинаниями, хаос-магикарп), продвинутый карп.

- **Код:** [`/mob/living/basic/carp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/carp/carp.dm#L13), [`/mob/living/basic/carp/magic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/carp/magicarp.dm#L55), [`/mob/living/basic/carp/mega`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/carp/megacarp.dm#L8)
- **BandaStation:** [`carp.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/carp/carp.dm#L23) — карпы роятся и проходят через столы; [`carp.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/carp/carp.dm#L121) — список еды
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs`: карп и мегакарп летают в космосе, охотятся на всех не-карпов, кусают 20, бегут раненными, проходят сквозь мобов (BandaStation `TRAIT_UNDENSE`), цвет из `carp_colors` (GAGS: тонированное тело + слой пасти), событие миграции; нет рифтов, магикарпов, приручения, езды, роя

_«Гигантские пауки» описано в [../antagonists/xenomorphs-spiders.md](../antagonists/xenomorphs-spiders.md)._

### Медведи
Космические, снежные (Миша), русские медведи; можно оседлать; масляный медведь форка.

- **Код:** [`/mob/living/basic/bear`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/bear/_bear.dm#L2)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/hostile/bear.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/hostile/bear.dm#L1) — переводы, масляный медведь
- **Статус в ss13-rs:** ❌ нет

### Король крыс
Антагонист-гост-роль (событие): Король крыс призывает крыс, ведёт их в «бунт», заражает еду, носит корону.

- **Код:** [`/mob/living/basic/regal_rat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/regal_rat/regal_rat.dm#L4), [`/datum/action/cooldown/mob_cooldown/riot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/regal_rat/regal_rat_actions.dm#L95)
- **Статус в ss13-rs:** ❌ нет

_«Космический дракон» описано в [../antagonists/midround-creatures.md](../antagonists/midround-creatures.md)._

_«Ревенант» описано в [../antagonists/midround-creatures.md](../antagonists/midround-creatures.md)._

### Демоны
Демон резни (прыгает через лужи крови, пожирает тела), демон смеха (возвращает съеденных), демон двигателя.

- **Код:** [`/mob/living/basic/demon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/demon/demon.dm#L2), [`/mob/living/basic/demon/slaughter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/demon/demon_subtypes.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Существа генокрада
Головной слизень (откладывает яйцо-генокрада в труп) и плотяной паук генокрада.

- **Код:** [`/mob/living/basic/headslug`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/changeling/headslug.dm#L6), [`/mob/living/basic/flesh_spider`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/changeling/flesh_spider.dm#L6)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/mobs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/mobs.dm#L15) — правки слизня
- **Статус в ss13-rs:** ❌ нет

### Хайвботы
Роботы из маяков-хайвов (руины, события): обычный, стрелок, скорострельный, сильный, механик (чинит своих).

- **Код:** [`/mob/living/basic/hivebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/hivebot/_hivebot.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Статуи и манекены
Статуя-«ангел» двигается только когда на неё не смотрят и гасит свет; манекены (подозрительный — двигается).

- **Код:** [`/mob/living/basic/statue`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/statue/statue.dm#L3), [`/mob/living/basic/statue/mannequin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/statue/mannequin.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Змеи
Змеи едят мышей, ядовитые полосатые змеи.

- **Код:** [`/mob/living/basic/snake`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/snake/snake.dm#L2), [`/mob/living/basic/snake/banded`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/snake/banded_snake.dm#L1)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/hostile/snake.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/hostile/snake.dm#L1) — переводы; [`modular_bandastation/mobs/code/simple_animal/pets/snake_syndicate.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/pets/snake_syndicate.dm#L2) — синдизмей и трёхголовая Руж
- **Статус в ss13-rs:** ❌ нет

### Призраки, безликие и обитатели Нижнего мира
Враждебные призраки (рой), безверные (faithless), существа Нижнего мира: тварь, Ми-Го, пустое тело.

- **Код:** [`/mob/living/basic/ghost`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/ghost.dm#L1), [`/mob/living/basic/faithless`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/faithless.dm#L1), [`/mob/living/basic/creature`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/netherworld/creature.dm#L1), [`/mob/living/basic/migo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/netherworld/migo.dm#L1), [`/mob/living/basic/blankbody`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/netherworld/blankbody.dm#L1)
- **BandaStation:** [`creature.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/netherworld/creature.dm#L45) — убрана телепортация твари
- **Статус в ss13-rs:** ❌ нет

### Мобы руин и ботаники: томаты-убийцы, грибы, гномы
Томат-убийца (из семян), ходячий гриб (лечится едой, можно съесть), садовый гном, муравьи.

- **Код:** [`/mob/living/basic/killer_tomato`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/killer_tomato.dm#L7), [`/mob/living/basic/mushroom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/mushroom.dm#L1), [`/mob/living/basic/garden_gnome`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/garden_gnome.dm#L1), [`/mob/living/basic/ant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/ant.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Прочие существа космоса
Глазное яблоко (лечит повреждённые глаза), лайтгейст (лечащий дух), бумажный волшебник, космонавт, Роро, кот-хирург (крадёт органы), вумборианский фугу (раздувается), паук суперматерии, сердце метеора (событие), робот-посетитель (ресторан повара).

- **Код:** [`/mob/living/basic/eyeball`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/eyeball/_eyeball.dm#L1), [`/mob/living/basic/lightgeist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/lightgeist.dm#L7), [`/mob/living/basic/paper_wizard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/paper_wizard/paper_wizard.dm#L1), [`/mob/living/basic/spaceman`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/spaceman.dm#L1), [`/mob/living/basic/roro`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/roro.dm#L1), [`/mob/living/basic/cat_butcherer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/cat_surgeon.dm#L7), [`/mob/living/basic/wumborian_fugu`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/wumborian_fugu/wumborian_fugu.dm#L8), [`/mob/living/basic/supermatter_spider`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/supermatter_spider.dm#L2), [`/mob/living/basic/meteor_heart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/meteor_heart/meteor_heart.dm#L7), [`/mob/living/basic/robot_customer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/robot_customer.dm#L2)
- **Статус в ss13-rs:** ❌ нет
