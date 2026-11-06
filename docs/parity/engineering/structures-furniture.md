# Мебель и перемещение между уровнями
Столы, стеллажи, стулья, кровати и прочая мебель; лестницы, ступени и транзитные трубы.

### Столы
Предметы кладутся на стол кликом (с учётом точки клика), на стол залезают, через него перекидывают и «прикладывают» схваченного (table slam); стол можно перевернуть как укрытие; разборка: отвёртка снимает столешницу, гаечный ключ разбирает (укреплённый — сначала сварка).

- **Код:** [`/obj/structure/table`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/tables_racks.dm#L15), [`proc/table_place_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/tables_racks.dm#L387), [`proc/flip_table()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/tables_racks.dm#L133), [`attack_hand()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/tables_racks.dm#L272), [`/obj/structure/table/reinforced`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/tables_racks.dm#L744)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/tables.rs`: предмет кладётся кликом в точку клика (не в боевом режиме), поднос высыпается на стол, переворот правым кликом (укрытие с одной стороны, лежащее на нём сталкивается), залезание перетаскиванием себя (подъём на 12 px, со стола на стол можно ходить — `ClimbWalkable` в `crates/ss13_world`), захваченного кладут/швыряют на стол, толчок в стол сбивает на него, разборка отвёрткой (остаётся рама) и ключом, укреплённый — после сварки; брошенное перелетает через стол (`LETPASSTHROW`, `crates/ss13_game/src/throw.rs`), стол на углу не мешает дотянуться по диагонали; `crates/ss13_game/src/food.rs` (`on_table`): поверхность для готовки, деревянные горят (`crates/ss13_game/src/fire.rs`); нет колоды карт (нет карт), удара от кабеля под укреплённым столом, отдыха (уложенный стоит), урона столу от ударов

### Виды столов и рамы
Деревянные, стеклянные (бьются, если на них лечь или бросить человека), пластальные, бронзовые, покерные, «каталки» на колёсах, операционный стол (см. ../medical/); рама стола (`table_frame`) из прутьев + лист материала = стол.

- **Код:** [`/obj/structure/table/wood`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/tables_racks.dm#L618), [`glass/proc/table_shatter()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/tables_racks.dm#L575), [`/obj/structure/table/rolling`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/tables_racks.dm#L449), [`/obj/structure/table_frame`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/table_frames.dm#L12)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/tables.rs`: рама + лист (`table_type` стака) = стол, ключ разбирает раму, деревянная рама — только дерево/ковёр; стеклянный стол бьётся под залезшим/брошенным (рама + осколок, паралич); нет «каталок», материальных (greyscale) столов и сборки рамы из прутьев

### Стеллажи
Стеллаж (`rack`) держит предметы, разбирается гаечным ключом в детали, детали собираются обратно.

- **Код:** [`/obj/structure/rack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/tables_racks.dm#L1330), [`/obj/item/rack_parts`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/tables_racks.dm#L1430)
- **BandaStation:** [`aesthetics/rack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/rack/code/rack.dm) — спрайты стеллажа
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/tables.rs`: предмет кладётся по центру, брошенное перелетает, на стеллаж залезают, пинок в боевом режиме, ключ (правый клик) разбирает на детали, детали собираются в руке, ключ превращает детали в лист

### Стулья, диваны, скамьи
На стул садятся (пристёгиваются), его поворачивают, складные стулья берутся в руки и бьют; офисные кресла катаются; разборка гаечным ключом.

- **Код:** [`/obj/structure/chair`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/beds_chairs/chair.dm#L3), [`sofa.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/beds_chairs/sofa.dm), [`pew.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/beds_chairs/pew.dm)
- **BandaStation:** [`chair.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/beds_chairs/chair.dm#L1) — русские склонения в сообщениях; [`modular_bandastation/objects/code/structures/chairs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/chairs.dm)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/furniture.rs` поверх пристёгивания `crates/ss13_game/src/medical/surgery.rs` (`Buckled`): сесть перетаскиванием себя (другого — за 2 с), встать пустой рукой или сопротивлением; сидящий (`Seated`) не лежит, не ходит и смотрит, куда стул; стул лицом на север рисуется поверх сидящего, табурет поднимает на 4 px, барный — на 7; Alt-клик поворачивает стул вместе с сидящим (скамья — только с ключом в руке); гаечный ключ (ПКМ) разбирает на `buildstacktype`; стул с `item_chair` перетаскиванием на себя берётся в руки, из руки ставится лицом по взгляду; звуки `buckle_sound`/`unbuckle_sound`. Нет: подлокотников поверх сидящего, удара/разбивания стулом, катания офисных кресел, материальных (greyscale) стульев, бронзового кресла, поворота призраками; HE-трубы (`can_buckle`: выравнивание температуры тела с трубой, ожог выше 1000 K) — к атмосу

### Кровати и роллеры
Кровати и медицинские роллеры: лечь/пристегнуть, роллер складывается и катается; собачья лежанка.

- **Код:** [`/obj/structure/bed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/beds_chairs/bed.dm#L10), [`/obj/structure/bed/medical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/beds_chairs/bed.dm#L116)
- **Статус в ss13-rs:** 🟡 частично — лечь/уложить (`crates/ss13_game/src/medical/surgery.rs`, `Buckled`); `crates/ss13_game/src/furniture.rs`: стоящие и лежащие на кровати рисуются на `elevation` (8 px) выше, гаечный ключ (ПКМ) разбирает на `build_stack_type` при `can_deconstruct`, звуки ремней медицинской кровати. Нет: складывания и катания роллера с пациентом, сторон изголовья (`buckle_lying` 90/270), собачьей лежанки, мягкого приземления (`soft_landing`)

### Электрический стул
Стул с собранной сборкой (сигналер) убивает током пристёгнутого при активации, если в области есть питание.

- **Код:** [`/obj/structure/chair/e_chair`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/electricchair.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Шкафы, комоды, манекены, витрины
Комод (сменить нижнее бельё), манекены (одевать), декоративные витрины (`showcase`), гардероб с нарядами, шторы (открыть/закрыть, срезать), корзина для простыней.

- **Код:** [`/obj/structure/dresser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/dresser.dm#L1), [`/obj/structure/mannequin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mannequin.dm#L6), [`/obj/structure/showcase`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/showcase.dm#L5), [`/obj/structure/outfit_wardrobe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/outfit_wardrobe.dm#L4), [`/obj/structure/curtain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/curtains.dm#L4), [`bedsheet_bin.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/bedsheet_bin.dm)
- **Статус в ss13-rs:** 🟡 частично — шторы: `crates/ss13_game/src/decor.rs` (см. «Флора, статуи и декор» в `structures-misc.md`); витрины `showcase` там же: отвёртка ослабляет/закручивает винты незакреплённой, ключ (2 с) откручивает/закрепляет с закрученными винтами, лом (2 с) разбирает на 4 листа железа (как в tg — в любом состоянии); нет строки осмотра о винтах. Нет: комода, манекенов, гардероба с нарядами, корзины для простыней

### Лестницы (ladder)
Лестница связывает уровни по вертикали: клик — подъём или спуск с задержкой; разборка сваркой, крафтовая лестница.

- **Код:** [`/obj/structure/ladder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/ladders.dm#L2), [`proc/link_down()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/ladders.dm#L151), [`/obj/structure/ladder/crafted`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/ladders.dm#L475)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/multiz.rs` (`Ladder`, `interact`, `ghost_clicks`): связь со стремянками точно над и под, ЛКМ — вверх, ПКМ — вниз за секунду, утаскиваемое следом, призраки сразу, спрайт `ladder[up][down]`; нет радиального меню (кликнуть ещё раз), дыры в полу, разборки сваркой и крафта, атлетики

### Ступени и марши (stairs)
Марш лестницы переводит идущего на уровень выше при шаге в направлении подъёма; декоративные ступени (`steps`) и фальшлестницы, телепортирующие между точками карты.

- **Код:** [`/obj/structure/stairs`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/stairs.dm#L14), [`proc/on_exit_stairs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/stairs.dm#L181), [`/obj/structure/steps`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/steps.dm#L2), [`/obj/structure/fake_stairs`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/fake_stairs.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/multiz.rs` (`Stairs`, `climb_stairs`): последняя ступень марша не выпускает за свой дальний край (граница без плотности, `ss13_world::NonDenseBorder`, клиент предсказывает и сам подъём — `Blocks::Stairs`), шаг туда — на этаж выше, если там не открытое пространство; спуск — падением на неё (ловит без урона, бессознательные скатываются с уроном), составные спрайты `-l/-m/-r`; нет указателей и меток миникарты, рамы ступеней, `force_open_above`, толчка при скатывании, декоративных ступеней (`steps`) и фальшлестниц-телепортов

### Транзитные трубы
Пневмотрубы со станциями: в капсуле игрок едет по трубам между станциями; сборка труб из деталей, капсулы из РПД.

- **Код:** [`/obj/structure/transit_tube`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/transit_tubes/transit_tube.dm#L2), [`/obj/structure/transit_tube/station`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/transit_tubes/station.dm#L9), [`/obj/structure/transit_tube_pod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/transit_tubes/transit_tube_pod.dm#L4), [`transit_tube_construction.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/transit_tubes/transit_tube_construction.dm)
- **Статус в ss13-rs:** ❌ нет
