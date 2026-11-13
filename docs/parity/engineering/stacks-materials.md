# Стаки и материалы
Стаковые предметы (листы, прутья, плитка) с рецептами постройки, а также система материалов (`/datum/material`) и хранилища материалов машин.

### Стаки: количество, объединение, разделение
Стак хранит количество и вес по размеру; одинаковые стаки сливаются (в руке и на полу), из стака берётся часть (клик второй рукой, ввод количества); пустой стак исчезает.

- **Код:** [`/obj/item/stack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/stack.dm#L12), [`proc/merge()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/stack.dm#L744), [`proc/split_stack()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/stack.dm#L784), [`use()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/stack.dm#L589), [`proc/merge_with_loc()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/stack.dm#L152)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/tools.rs` (`use_stack`), `crates/ss13_game/src/inventory.rs`, `crates/ss13_game/src/construction.rs` (`split_into_hand`): расход на постройку, одна единица во вторую руку, слияние кликом; нет ввода количества, автослияния на полу, веса по размеру

### Рецепты стаков (меню постройки)
Использование стака в руке открывает меню рецептов (tgui `StackCrafting` или радиальное): стулья, столы, двери, рамы, балки, решётки и т. д. с проверками места, времени и количества; категории и множественная постройка.

- **Код:** [`/datum/stack_recipe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/stack_recipe.dm#L5), [`/datum/stack_recipe_list`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/stack_recipe.dm#L104), [`proc/make_item()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/stack.dm#L425), [`proc/building_checks()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/stack.dm#L532), [`proc/show_construction_radial()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/stack.dm#L365); tgui: [`StackCrafting.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/StackCrafting.tsx)
- **Статус в ss13-rs:** ✅ `crates/ss13_game/src/stack_recipes/` — меню рецептов всех 52 списков tg/BandaStation (`data.rs` генерируется `scripts/gen-stack-recipes.py` из DM, с правкой модпака дерева), подсписки, множитель для стаковых результатов, `building_checks` до и после `do_after`, результат на клетке строителя по его направлению, в руки; меню — список tg (`tgui_input_list`) вместо окна `StackCrafting` (shortcut: без иконок и поиска), `TRAIT_QUICK_BUILD` и материалы результата не применяются; ПКМ-решётка из прутьев осталась (`construction.rs`)

### Листы стали и пластали
Сталь — основа строительства (балки, стены, рамы, стулья, ...), пласталь — укреплённые стены и балки, гермодвери, сейфы.

- **Код:** [`/obj/item/stack/sheet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/sheets.dm#L1), [`/obj/item/stack/sheet/iron`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/sheet_types.dm#L161), [`/obj/item/stack/sheet/plasteel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/sheet_types.dm#L308)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/construction.rs`: сталь для балок и стен; пласталь не используется; нет рецептов

### Стекло
Обычное, укреплённое, плазменное, плазменное укреплённое, титановое и пластитановое стекло: окна, окно-двери, стеклянные полы; режется в осколки, осколки переплавляются.

- **Код:** [`/obj/item/stack/sheet/glass`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/glass.dm#L19), [`/obj/item/stack/sheet/rglass`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/glass.dm#L163), [`/obj/item/stack/sheet/plasmaglass`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/glass.dm#L100), [`/obj/item/stack/sheet/plasmarglass`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/glass.dm#L204)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/construction.rs`, `crates/ss13_game/src/obj_damage.rs`: обычное стекло на решётку и из разобранного окна, осколки при разрушении; остальные виды не используются

### Минеральные листы
Золото, серебро, алмаз, уран, плазма (горит), бананиум, титан, пластитан, песчаник, мешки с песком, адамантин, мифрил, металлический водород, горячий лёд, руническая сталь, бронза.

- **Код:** [`/obj/item/stack/sheet/mineral`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/mineral.dm#L33), [`mineral.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/mineral.dm), [`/obj/item/stack/sheet/hot_ice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/hot_ice.dm#L1), [`/obj/item/stack/sheet/runed_metal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/runed_metal.dm#L71), [`/obj/item/stack/sheet/bronze`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/sheet_types.dm#L791)
- **Статус в ss13-rs:** ✅ `crates/ss13_game/src/stack_recipes/` (меню всех минеральных листов, бронзы, рунической стали — её строит культ, `antag::cult`), `crates/ss13_game/src/sheets.rs`: плазма и горячий лёд — `combustible_flooder` (огонь выше `FIRE_MINIMUM_TEMPERATURE_TO_EXIST` или горячий инструмент: 5 / 150 молей плазмы на лист, лёд — при `20 × n + 300` K), уголь — CO2, пять прутьев на титан — прутья шаттла; горячий инструмент — только сварка (shortcut: зажигалок нет); бананиум не пищит и не скользит (shortcut, вместе с материалами)

### Органические и прочие листы
Дерево, бамбук, ткань и дюраткань, картон (коробки), пластик, кожа и шкуры (выделка), кости и т. п.; горячий лёд — выпадает при горении фреона (../atmos/reactions.md), горит, выделяя плазму.

- **Код:** [`/obj/item/stack/sheet/mineral/wood`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/sheet_types.dm#L408), [`/obj/item/stack/sheet/cloth`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/sheet_types.dm#L544), [`/obj/item/stack/sheet/cardboard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/sheet_types.dm#L720), [`/obj/item/stack/sheet/plastic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/sheet_types.dm#L906), [`leather.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/leather.dm), [`/obj/item/stack/sheet/hot_ice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/sheets/hot_ice.dm#L1)
- **Статус в ss13-rs:** ✅ `crates/ss13_game/src/stack_recipes/` (меню дерева с полами BandaStation, бамбука, ткани, дюраткани, картона, пластика, кожи, шкур, жил, бумажных рам), `crates/ss13_game/src/sheets.rs`: выделка (острый предмет 5 с — шкура без шерсти; вода или стиральная машина — мокрая; на клетке горячее 500 K `wetness` 30 тиков воздуха — по листу кожи), штампы клоуна и хамелеона на картоне — коробки, горячий лёд горит плазмой; 🟡 чужое: образование горячего льда — ../atmos/reactions.md, починка деревянного протеза деревом — медицина; сушилка, микроволновка, гриль и духовка для кожи не сушат (shortcut)

### Прутья
Решётки, решётчатый настил над космосом, рамы столов, укрепление; гнутся в разные поделки; лавовые и шаттловые прутья.

- **Код:** [`/obj/item/stack/rods`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/rods.dm#L18), [`/obj/item/stack/rods/lava`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/rods.dm#L133)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/construction.rs`: решётка из 2 прутьев (ПКМ), ремонт решётки, выпадают из решётки; нет настила и рецептов

### Напольная плитка
Плитка всех видов (сталь с вариантами формы, дерево, ковры, трава, минеральная, светящаяся, ...): укладка на обшивку, смена вида (`tile_reskinning`), поворот направленной плитки.

- **Код:** [`/obj/item/stack/tile`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/tiles/tile_types.dm#L6), [`proc/place_tile()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/tiles/tile_types.dm#L76), [`/obj/item/stack/tile/iron`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/tiles/tile_iron.dm#L1), [`tile_reskinning.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/tiles/tile_reskinning.dm), [`/obj/item/stack/tile/light`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/tiles/light.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/construction.rs`: укладка плитки на обшивку; нет смены вида и поворота

### Заполнитель стен шаттлов
Стаки для заделки пробоин в пластитановых стенах и окнах шаттлов и капсул.

- **Код:** [`/obj/item/stack/wall_filling`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/wall_filling.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Обёрточная бумага и скотч
Обёрточная бумага (упаковка посылок, подарки) и клейкая лента (заклеить рот, починить, прилепить к предмету).

- **Код:** [`/obj/item/stack/package_wrap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/wrap.dm#L86), [`/obj/item/stack/wrapping_paper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/wrap.dm#L7), [`/obj/item/stack/medical/wrap/sticky_tape`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/tape.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — обёрточная бумага заворачивает предметы в посылки (`crates/ss13_game/src/disposals/mail.rs`); нет шкафов и ящиков, подарочной бумаги, скотча

### Блюспейс-кристаллы и прочие стаки
Блюспейс-кристалл (раздавить — телепорт на короткое расстояние), номерные знаки (труд заключённых); телекристаллы, деньги и билеты — см. ../antagonists/, ../cargo/.

- **Код:** [`/obj/item/stack/ore/bluespace_crystal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/bscrystal.dm#L2), [`/obj/item/stack/license_plates`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/license_plates.dm#L1)
- **BandaStation:** [`bscrystal.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/bscrystal.dm#L41) — раздавить кристалл можно только после задержки `do_after`
- **Статус в ss13-rs:** ✅ `crates/ss13_game/src/sheets.rs`: кристалл в руке давится после `do_after` 1,5 с (BandaStation) — искры, звук портала, телепорт до 8 клеток (`do_teleport`, блюспейс-канал), минус один кристалл; меню очищенного кристалла — `stack_recipes`; номерные знаки — `crates/ss13_game/src/brig/labor.rs`; брошенный кристалл не телепортирует цель (shortcut)

### Материалы (datum/material)
Синглтоны материалов (сталь, стекло, золото, плазма, уран, бананиум, дерево, бумага, мясо, пицца, ...): цвет, сила, плотность, свойства (`material_property`), эффекты на предмет из материала (урановый фонит, плазменный горит, мясной съедобен); сплавы.

- **Код:** [`/datum/material`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/materials/_material.dm#L8), [`basemats.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/materials/basemats.dm), [`/datum/material/alloy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/materials/alloys.dm#L2), [`/datum/material_property`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/materials/properties/_properties.dm#L2), [`/datum/material_requirement`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/materials/requirements/_requirement.dm#L2), [`/datum/material/meat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/materials/meat.dm#L2)
- **Статус в ss13-rs:** 🟡 `ss13_mapload::research` (материалы с `MATERIAL_SILO_STORED`: имя, цвет, тип листа; `mats_per_unit` стаков) → `ss13_game::science::lathe`; свойств, эффектов и сплавов нет

### Предметы и структуры из материалов
`custom_materials` и слоты материалов: предметы, стены, полы, двери, статуи, сделанные из любого материала, получают его цвет, имя и свойства.

- **Код:** [`proc/on_applied()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/materials/_material.dm#L109), [`/datum/material_slot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/materials/material_slots/_slot.dm#L2), [`material_slots`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/materials/material_slots)
- **Статус в ss13-rs:** ❌ нет — сюда же стационарные баки из других материалов (обшивка рамы `tank_frame` любым жёстким материалом: прочность и предел давления, цвет, приставка к имени; сейчас только железо, `crates/ss13_game/src/atmos_machinery/tank_frame.rs`); меню крафта (`crates/ss13_game/src/crafting.rs`) и меню стаков материалы ингредиентов результату тоже не передают (`set_custom_materials`) — ждут этот механизм

### Хранилища материалов машин и рудный силос
`material_container` хранит листы в машинах (автолат, протолат, ...), `remote_materials` связывает машины с общим рудным силосом.

- **Код:** [`/datum/material_container`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/materials/material_container/material_container.dm#L15), [`/datum/remote_materials`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/materials/material_container/remote_materials.dm#L8), [`/obj/machinery/ore_silo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/machine_silo.dm#L26)
- **Статус в ss13-rs:** 🟡 `ss13_game::science::lathe` (`MaterialStore`): R&D-латы на уровне силоса делят его хранилище, без силоса — локальное (`matter bins × 37.5` листов); вставка стаков с разбиением при нехватке места, выброс листов, `OPTIMAL_COST`; нет удержаний, банов, логов, мультитула
