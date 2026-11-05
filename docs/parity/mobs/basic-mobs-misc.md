# Прочие basic-мобы
Мобы антагонистов, руин, событий и отделов: ксеноморфы-NPC, миньоны блоба, конструкты, стражи, призывы еретика, слаймы, дроны, бойцы-NPC. Сами антагонисты описаны в каталоге антагонистов; здесь — только мобы. В ss13-rs ничего из этого нет.

### Ксеноморфы-NPC
Неигровые ксеноморфы руин и событий: дрон, страж, королева (большая), горничная-ксено.

- **Код:** [`/mob/living/basic/alien`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/alien/_alien.dm#L1), [`/mob/living/basic/alien/queen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/alien/queen.dm#L1)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/hostile/alien.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/hostile/alien.dm#L1) — переводы
- **Статус в ss13-rs:** ❌ нет

_«Миньоны блоба» описано в [../antagonists/blob.md](../antagonists/blob.md)._

### Клоуны-мобы
Враждебные клоуны (Клоунский мир, сатирические руины): смазочный, хонклинг, плотяной клоун, длиннолицый, клоунохалк.

- **Код:** [`/mob/living/basic/clown`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/clown/clown.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Конструкты и тени культа
Конструкты культа из оболочек и камней душ: артифицер (строит, лечит), джаггернаут (танк), призрак (телепорт), протеон, жнец (Нар-Си); ангельские/мистические варианты, тень в камне душ.

- **Код:** [`/mob/living/basic/construct`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/cult/constructs/_construct.dm#L1), [`/mob/living/basic/construct/artificer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/cult/constructs/artificer.dm#L1), [`/mob/living/basic/construct/juggernaut`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/cult/constructs/juggernaut.dm#L1), [`/mob/living/basic/construct/wraith`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/cult/constructs/wraith.dm#L1), [`/mob/living/basic/construct/harvester`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/cult/constructs/harvester.dm#L1), [`/mob/living/basic/shade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/cult/shade.dm#L1)
- **BandaStation:** [`_construct.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/cult/constructs/_construct.dm#L110) — склонения имени при осмотре
- **Статус в ss13-rs:** ❌ нет

### Стражи (голопаразиты)
Привязанный к хозяину дух из набора (предатель, волшебник, генокрад): типы стандарт, ассасин, рывок, ловкий, взрывной, газовый, гравитокинетик, молния, защитник, стрелок, поддержка; урон делится с хозяином.

- **Код:** [`/mob/living/basic/guardian`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/guardian/guardian.dm#L5), [`guardian_types/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/guardian/guardian_types)
- **BandaStation:** [`guardian_creator.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/guardian/guardian_creator.dm#L77) — вампиры не могут использовать
- **Статус в ss13-rs:** ❌ нет

_«Призывы еретика» описано в [../antagonists/heretic-items.md](../antagonists/heretic-items.md)._

### Слаймы
Слаймы ксенобиологии: питаются кровью мобов, растут, делятся с мутацией цвета, становятся злыми голодными, успокаиваются; ядра — экстракты.

- **Код:** [`/mob/living/basic/slime`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/slime/slime.dm#L6), [`proc/reproduce()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/slime/actions.dm#L106)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/science/xenobio.rs`: 22 цвета (`/datum/slime_type`, вес мутаций), малыш/взрослый (здоровье /0.75, урон ×2), голод, рост и заряд (`handle_nutrition`), стазис от BZ, ИИ-охота голодного/бешеного слайма на ближайшего в 7 тайлах, присасывание и высасывание (`slime_leech`: бурн/токсины, питание ×1.8, самолечение), эволюция за 100 питания и деление на 4 с мутацией (перенаселение — 2 слайма на тайле), смерть взрослого — деление на двух малышей, извлечение ядер гемостатом/ломом с мёртвого (1.6 с), подвиды random/pet/hilbert; нет полного дерева ИИ (команды питомцев, друзья, эмоции, месть), действий игрока-слайма, удара током при атаке, холодового оцепенения, окрашивания жертвы

_«Ремонтные дроны» описано в [../silicon/drones.md](../silicon/drones.md)._

### Бойцы-NPC (troopers)
Гуманоидные NPC руин и шаттлов: Нанотрейзен, Синдикат (штурмовики, мечники), пираты, русские, абдукторы; ближний бой и стрельба.

- **Код:** [`/mob/living/basic/trooper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/trooper/trooper.dm#L1), [`/mob/living/basic/trooper/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/trooper/syndicate.dm#L2), [`/mob/living/basic/trooper/pirate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/trooper/pirate.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Защитники руин
Мимики (ящик-мимик, копия предмета/машины), живой пол, кусок плоти, плотяной шар, безумное пианино, скелеты (поселенцы, тамплиеры, ледяные, плазменные шахтёры), стикмены, тёмный волшебник и волшебники, зомби, ИИ-ядро Киберсана.

- **Код:** [`/mob/living/basic/mimic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/ruin_defender/mimic/mimic.dm#L10), [`/mob/living/basic/living_floor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/ruin_defender/living_floor.dm#L9), [`/mob/living/basic/living_limb_flesh`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/ruin_defender/flesh.dm#L17), [`/mob/living/basic/fleshblob`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/ruin_defender/blob_of_flesh.dm#L11), [`/mob/living/basic/mad_piano`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/ruin_defender/mad_piano.dm#L4), [`/mob/living/basic/skeleton`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/ruin_defender/skeleton.dm#L1), [`/mob/living/basic/stickman`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/ruin_defender/stickman.dm#L1), [`/mob/living/basic/wizard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/ruin_defender/wizard/wizard.dm#L1), [`/mob/living/basic/dark_wizard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/ruin_defender/dark_wizard.dm#L2), [`/mob/living/basic/zombie`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/ruin_defender/zombie.dm#L2), [`/mob/living/basic/cybersun_ai_core`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/ruin_defender/cybersun_aicore.dm#L6)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/hostile/undead.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/hostile/undead.dm#L1) — переводы зомби
- **Статус в ss13-rs:** ❌ нет

### Иллюзии
Копии моба (от бутылки иллюзий, способностей): толкающие, убегающие, миражи.

- **Код:** [`/mob/living/basic/illusion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/illusion/illusion.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Торговцы
NPC-торговец (Мистер Боунс) продаёт и покупает товары через радиальное меню.

- **Код:** [`/mob/living/basic/trader`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/trader/trader.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Цитология и прочие одиночки
Ватзверь из цитологии (растёт в чане из образцов клеток), фестивус-шест, живое дерево (событие), NPC-революционер.

- **Код:** [`/mob/living/basic/vatbeast`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/cytology/vatbeast.dm#L2), [`/mob/living/basic/festivus`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/festivus_pole.dm#L4), [`/mob/living/basic/tree`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/tree.dm#L1), [`/mob/living/basic/revolutionary`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/revolutionary.dm#L1)
- **Статус в ss13-rs:** ❌ нет
