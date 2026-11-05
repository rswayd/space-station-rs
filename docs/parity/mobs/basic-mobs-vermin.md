# Мелкие паразиты и мелкая фауна
`code/modules/mob/living/basic/vermin`, улитки, горностаи, черепахи: мелкие существа техтоннелей и станции. В ss13-rs есть мыши и тараканы (`crates/ss13_game/src/mobs/`).

### Мыши и крысы
Мыши спавнятся в техтоннелях (событие нашествия), грызут провода (могут погибнуть от тока), съедаются кошками и ящерицами; мышь Том; крысы — их злой вариант, служат Королю крыс.

- **Код:** [`/mob/living/basic/mouse`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/vermin/mouse.dm#L1), [`/mob/living/basic/mouse/rat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/vermin/mouse.dm#L322)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/friendly/mouse.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/mouse.dm#L1) — шерстистая, заводная, «фабричная», джет-мыши; [`modular_bandastation/mobs/code/simple_animal/hostile/rat.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/hostile/rat.dm#L1) — серые/белые/ирландские крысы
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs`: 10 мышей на открытых проводах станции в начале раунда (`SSminor_mapping`), окрас форка, бегут от людей, «Сквик!» под ногами, грызут провода (под током 85% гибнут, провод режется в моток), умирают в предмет «мертвая мышь», кошки их ловят, змеи и ящеры их едят; крысы (`mouse_rat.bt.json`) нападают на всех не-крыс, грызут провода и пищат фразами форка (со звуком); нет сыра, Короля крыс, переноски в руках, ломания препятствий

### Тараканы
Переживают взрывы и радиацию, давятся ногой; варианты: канализационный, кровавый, хауберочь (с шипом), глокроуч (стреляет), бандитские.

- **Код:** [`/mob/living/basic/cockroach`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/vermin/cockroach/cockroach.dm#L1), [`/mob/living/basic/cockroach/glockroach`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/vermin/cockroach/cockroach.dm#L172)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/mobs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/mobs.dm#L1) — правки таракана
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs`: бродят и стрекочут, давятся под ногами (50%), исчезают при смерти (`DEL_ON_DEATH`); нет охоты на муравьев и вариантов

### Мотылеплотва
Ручной «мотылёк-таракан», ест одежду, носится на голове; барная версия.

- **Код:** [`/mob/living/basic/mothroach`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/vermin/mothroach/mothroach.dm#L1)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/friendly/ce_mothroach.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/ce_mothroach.dm#L1), [`modular_bandastation/mobs/code/simple_animal/friendly/wrynroach.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/wrynroach.dm#L1) — мотылеплотва СИ и вринроуч
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/trees.rs`, `crates/ss13_game/src/mobs/eating.rs`: бродит, трепещет (`random_speech/mothroach`), убегает от обидчиков, находит и съедает одежду с пола (раз в минуту) и из рук; нет команд, ношения на голове и в руках, барной версии

### Ящерицы и аксолотли
Ящерицы едят тараканов и мелких насекомых; космическая ящерица; аксолотль.

- **Код:** [`/mob/living/basic/lizard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/vermin/lizard.dm#L7), [`/mob/living/basic/axolotl`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/vermin/axolotl.dm#L1)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/friendly/lizard.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/lizard.dm#L1) — переводы; крупные ящеры — см. [basic-mobs-bandastation.md](basic-mobs-bandastation.md)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/trees.rs`, `crates/ss13_game/src/mobs/eating.rs`: ящерицы бродят, шипят фразами `random_speech/lizard`, выслеживают и съедают бабочек и тараканов (+5 здоровья), едят их же из рук; аксолотли бродят; нет переноски в руках, вентиляции, космической ящерицы

### Лягушки
Обычные, редкие, «безумные» (галлюциногенные) и токсичные лягушки; лягушка-самоубийца.

- **Код:** [`/mob/living/basic/frog`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/vermin/frog.dm#L1)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/friendly/frog.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/frog.dm#L1) — токсичные и кричащие лягушки
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/trees.rs`: бродят, прыгают фразами `random_speech/frog`, бьют обидчиков (`frog.bt.json`); нет плавания, бегства от рыбаков, галлюциногенных/токсичных укусов, вариантов

### Крабы
Мирные крабы, ходят боком; злой краб, кофейный краб, «креб».

- **Код:** [`/mob/living/basic/crab`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/vermin/crab.dm#L2)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/friendly/crab.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/crab.dm#L1) — старый, морской, королевский краб
- **Статус в ss13-rs:** ❌ нет

### Бабочки и космические летучие мыши
Декоративные бабочки (временные на Лаваленде), летучие мыши кусают и летают через космос.

- **Код:** [`/mob/living/basic/butterfly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/vermin/butterfly.dm#L1), [`/mob/living/basic/bat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/vermin/space_bat.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/trees.rs`: бабочки бродят (`butterfly.bt.json`), летучие мыши бьют обидчиков (`simple_retaliate_combat`); нет временных бабочек Лаваленда

### Улитки
Медленные улитки оставляют слизь; злая улитка. Также игровой вид «улиткочеловек» использует их черты.

- **Код:** [`/mob/living/basic/snail`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/snails/snail.dm#L1)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/friendly/snail.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/snail.dm#L1) — космическая и смазочная улитки
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/trees.rs`, `crates/ss13_game/src/mobs/eating.rs`: бродят, находят и едят растения и дольки яблок; нет прятанья в раковину (способность), слизи, злой улитки

### Горностаи и черепахи
Горностаи (детёныши-котята), черепаха-питомец ботаники, растит на панцире растения.

- **Код:** [`/mob/living/basic/stoat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/stoats/stoat.dm#L1), [`/mob/living/basic/turtle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/turtle/turtle.dm#L8)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/friendly/snail.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/snail.dm#L43) — правки черепахи
- **Статус в ss13-rs:** ❌ нет
