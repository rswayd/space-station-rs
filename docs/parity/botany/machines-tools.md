# Машины и инструменты ботаники
Экстрактор семян, биогенератор, анализатор растений, бродильная бочка и ручной инструмент ботаника.

### Экстрактор семян
Перерабатывает плоды в семена (`seedify`, количество зависит от деталей), хранит пакеты, группируя одинаковые по хэшу статов; выдача через окно со списком семян и статами.

- **Код:** [`/obj/machinery/seed_extractor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seed_extractor.dm#L44), [`/proc/seedify()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seed_extractor.dm#L16), [`add_seed()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seed_extractor.dm#L178), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seed_extractor.dm#L248); tgui: [`SeedExtractor.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SeedExtractor.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/mod.rs`: плод в экстракторе даёт 2-5 пакетов с его статами; `// shortcut:` хранения семян и окна со списком нет

### Биогенератор
Превращает загруженные плоды в биомассу (по питательным веществам), из неё печатает по дизайнам: удобрения и химию в стакан, еду (молоко, мясо-заменитель), кожаные изделия, ткань, сумки и ремни. Детали ускоряют и удешевляют работу; есть слот для мензурки.

- **Код:** [`/obj/machinery/biogenerator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/biogenerator.dm#L6), [`convert_to_biomass()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/biogenerator.dm#L336), [`create_product()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/biogenerator.dm#L373), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/biogenerator.dm#L524), [`biogenerator_designs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/designs/biogenerator_designs.dm#L1); tgui: [`Biogenerator.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Biogenerator.tsx)
- **BandaStation:** [`modular_bandastation/hydroponics/code/biogenerator_designs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/hydroponics/code/biogenerator_designs.dm#L1) — добавлены дизайны «странные семена» (2000 биомассы) и сумка для материалов
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/botany/mod.rs`: `// shortcut:` биогенератора нет

### Анализатор растений
Скан лотка, семян, плода или прививки: статы, трейты, реагенты (ЛКМ — статы, ПКМ — химия) в окне tgui; на растительных существах (подмены) — здоровье и химия.

- **Код:** [`/obj/item/plant_analyzer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/plant_analyzer.dm#L4), [`interact_with_atom()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/plant_analyzer.dm#L71), [`do_plant_chem_scan()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/plant_analyzer.dm#L96); tgui: [`PlantAnalyzer/index.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PlantAnalyzer/index.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/mod.rs`: поля анализатора tg для лотка, семени или плода; `// shortcut:` вывод в чат вместо окна tgui

### Бродильная бочка
Плоды (руками или из мешка) бродят в бочке в вино/дистиллят по их `distill_reagent`; бочка — открываемая ёмкость, из которой наливают. Варианты с порохом и термитом.

- **Код:** [`/obj/structure/fermenting_barrel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/fermenting_barrel.dm#L1), [`insert_fruit()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/fermenting_barrel.dm#L154), [`process_fermentation()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/fermenting_barrel.dm#L185)
- **Статус в ss13-rs:** ❌ нет

### Культиватор, грабли и вилы
Культиватор (и ПКМ-вариант грабли) убирает сорняки из лотка; грабли — ловушка, на которую наступают; вилы — оружие на двуручном хвате.

- **Код:** [`/obj/item/cultivator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics_items.dm#L37), [`cultivator/rake`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics_items.dm#L59), [`rake/pitchfork`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics_items.dm#L90)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/plants.rs`: культиватор выпалывает сорняки в лотке; граблей и вил как инструментов нет

### Топорик, коса и секатор
Топорик рубит бревна и метается; коса срезает растения и сорняки по дуге; секатор снимает прививки; генные ножницы удаляют трейт с растения в лотке.

- **Код:** [`/obj/item/hatchet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics_items.dm#L118), [`/obj/item/scythe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics_items.dm#L171), [`/obj/item/secateurs`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics_items.dm#L233), [`/obj/item/geneshears`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics_items.dm#L272)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/grafts.rs`: секатор снимает прививки с растения в лотке; нет топорика (рубка бревен), косы, стрижки секатором (ПКМ, причёски), генных ножниц (нужен выбор из списка)

### Удобрения, гербициды и пестициды
Бутылки E-Z-Nutrient, Left 4 Zed, Robust Harvest, гербицид и пестицид, распылители от сорняков и вредителей — их реагенты действуют через лоток (`on_hydroponics_apply`).

- **Код:** [`/obj/item/reagent_containers/cup/bottle/nutrient`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics_items.dm#L296), [`bottle/killer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics_items.dm#L326), [`spray/weedspray`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics_items.dm#L5), [`spray/pestspray`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics_items.dm#L21)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/tray.rs`, `crates/ss13_game/src/chem.rs`: бутылки переливаются в лоток и действуют на растение (`on_hydroponics_apply`); распылителей нет (`// shortcut:` sprays)

Мешок для растений (`/obj/item/storage/bag/plants`) — см. ../items/; торговые автоматы MegaSeed/NutriMax — см. ../cargo/.
