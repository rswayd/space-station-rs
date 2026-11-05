# Стены, балки и фальшстены
Стены-тайлы (`/turf/closed/wall`), их постройка и разборка, балки (`girder`) и фальшстены.

### Обычная стена: постройка и разборка
Два листа стали на балке дают стену; горящая сварка (нагрев от 1500 K) разрезает стену за 10 с на 2 листа и балку, пол под ней возвращается.

- **Код:** [`/turf/closed/wall`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/walls.dm#L1), [`proc/try_decon()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/walls.dm#L234), [`proc/dismantle_wall()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/walls.dm#L99)
- **BandaStation:** [`modular_bandastation/aesthetics/walls/code/walls.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/walls/code/walls.dm) — свои спрайты стен
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/construction.rs`: стена из 2 листов на балке за 4 с, разрезка сваркой за 10 с; нет вмятин, повреждений и разрушения стен

### Укреплённая стена: многошаговая разборка
`r_wall` разбирается в 7 шагов (`d_state`): кусачки → отвёртка → сварка → лом → гаечный ключ → сварка → лом, каждый шаг обратим; не режется обычной сваркой за один раз, устойчива к взрывам.

- **Код:** [`/turf/closed/wall/r_wall`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/wall/reinf_walls.dm#L1), [`r_wall/try_decon()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/wall/reinf_walls.dm#L50)
- **BandaStation:** [`reinf_walls.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/wall/reinf_walls.dm#L207) — свои спрайты стадий разборки
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/reinforced.rs` (`wall_tool`, `RWallState`, `set_wall_state`): все 7 шагов в обе стороны с задержками tg, проверка сварки (1500 K, 2 топлива на старте), спрайты стадий форка (`reinforced_states.dmi`) без сглаживания вне `INTACT`, подсказки при осмотре; последний шаг — `dismantle_wall` (лист пластали и укреплённая балка); устойчивость к взрывам — `crates/ss13_game/src/explosions/ex_act.rs`; звуков инструментов нет (как у остальной стройки)

### Повреждение стен
Стены получают вмятины от пуль, ломаются и сносятся взрывами (`ex_act`), халком, блобом, сингулярностью, кислотой; ржавчина ослабляет стены (`rust_turf`), её счищают щёткой.

- **Код:** [`ex_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/walls.dm#L124), [`attack_hulk()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/walls.dm#L157), [`proc/add_dent()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/walls.dm#L310), [`rust_turf()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/walls.dm#L332), [`/turf/closed/wall/rust`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/wall/misc_walls.dm#L45)
- **Статус в ss13-rs:** ❌ нет

### Опора для настенного оборудования
Временная опора (`wall_support`) позволяет разобрать стену, не роняя APC, лампы и прочее висящее на ней.

- **Код:** [`/obj/structure/wall_support`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/wall_support.dm#L7), [`proc/try_build_support()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/walls.dm#L249)
- **Статус в ss13-rs:** ❌ нет

### Минеральные и материальные стены
Стены из золота, серебра, алмаза, урана (фонит), плазмы (горит), бананиума, песчаника, дерева, бамбука, титана, пластитана, снега, культа и т. д.; материальные стены из любого материала наследуют его свойства.

- **Код:** [`/turf/closed/wall/mineral`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/wall/mineral_walls.dm#L1), [`/turf/closed/wall/material`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/wall/material_walls.dm#L1), [`misc_walls.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/wall/misc_walls.dm), [`concrete_walls.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/wall/concrete_walls.dm)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/smooth.rs`: грузятся с карты, сглаживаются и блокируют воздух; свойства материалов и разборка не перенесены

### Стена из металлопены
Металлическая пена затвердевает в хрупкую стену (`metal_foam_base`), которую легко сломать.

- **Код:** [`/turf/closed/wall/metal_foam_base`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/walls.dm#L339)
- **Статус в ss13-rs:** ❌ нет

### Балки (girder)
Каркас стены: лист стали на полу (ПКМ) — смещённая балка, гаечный ключ закрепляет, отвёртка/кусачки/сварка разбирают; пласталь укрепляет балку (стойки → укреплённая балка → r_wall); листы других материалов дают минеральные стены; железо на смещённой балке — фальшстена.

- **Код:** [`/obj/structure/girder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/girders.dm#L1), [`girder/item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/girders.dm#L56), [`/obj/structure/girder/displaced`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/girders.dm#L167), [`/obj/structure/girder/reinforced`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/girders.dm#L179), [`/obj/structure/girder/cult`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/girders.dm#L205), [`/obj/structure/girder/bronze`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/girders.dm#L252), [`/obj/structure/girder/tram`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/girders.dm#L188)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/construction.rs` (`GIRDER_RECIPES`, `girder_stack`), `crates/ss13_game/src/obj_damage.rs`: обычная и смещённая балка, закрепление, разборка отвёрткой, стена из стали, повреждение ударами; пласталь укрепляет балку (5 с), отвёртка откручивает/закручивает стойки, кусачки снимают решётку (лист пластали и обычная балка), пласталь на укреплённой балке — `r_wall` (5 с), 2 листа стали/пластали на смещённой — фальшстена (2 с); нет минеральных и материальных рецептов, культовых/бронзовых балок (сварка, `narsie_act`)

### Фальшстены
Выглядят как стена, открываются рукой как дверь (пропускают воздух в открытом виде); разбираются инструментами; минеральные и укреплённые варианты, урановая фонит.

- **Код:** [`/obj/structure/falsewall`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/false_walls.dm#L4), [`proc/toggle_open()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/false_walls.dm#L61), [`/obj/structure/falsewall/reinforced`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/false_walls.dm#L166), [`/obj/structure/falsewall/uranium`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/false_walls.dm#L190)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/falsewall.rs`: открывается и закрывается рукой (0,5 с анимации `-opening`/`-closing`, затем без плотности, непрозрачности и блокировки воздуха; не закрывается, пока внутри моб), отвёртка на закрытой на полу превращает её в `walltype`, сварка (укреплённая — кусачки) разбирает в смещённую балку и `mineral_amount` листов `mineral`; постройка — `crates/ss13_game/src/construction.rs`; поля `mineral`, `mineral_amount`, `walltype` в `DmObj` (`crates/ss13_mapload/src/construct.rs`); нет радиации урановой (механики радиации нет), прислонения (`leanable`), маскировки при осмотре
