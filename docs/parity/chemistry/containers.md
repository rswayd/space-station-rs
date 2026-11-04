# Контейнеры и средства введения
Мензурки, бутылки, шприцы, таблетки, пластыри, гипоспреи, распылители и баки.

### Открытые контейнеры: мензурки, бутылки, стаканы
`/obj/item/reagent_containers/cup`: переливание `amount_per_transfer_from_this` (циклически меняется кликом по себе), наполнение из сливаемых ёмкостей, питьё глотками и спаивание другому, разбрызгивание правым кликом в бою, оверлей уровня заполнения; крышки у бутылок; бьющиеся бутылки.

- **Код:** [`/obj/item/reagent_containers`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers.dm#L1), [`/obj/item/reagent_containers/cup`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/_cup.dm#L1), [`/obj/item/reagent_containers/cup/beaker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/_cup.dm#L442), [`/obj/item/reagent_containers/cup/bottle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/bottle.dm#L3), [`/obj/item/reagent_containers/cup/glass`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/drinks.dm#L4)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem.rs`: переливание, наполнение, питьё и спаивание, выбор объёма, разбрызгивание (без воздействия), уровень заполнения; нет крышек, разбивания бутылок

### Шприцы
Ввод объёма в моба (другому — 3 с, блокируется плотной одеждой зоны), забор из контейнера или крови моба; режимы набора/ввода; виды: обычный, большой, блюспейс, ядовитый, с биотрекером.

- **Код:** [`/obj/item/reagent_containers/syringe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/syringes.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem.rs`: ввод и забор из контейнера; нет забора крови и особых шприцев

### Таблетки и пластыри
Таблетку глотают или растворяют в стакане; пластырь клеится на кожу и медленно всасывается; производятся химмастером и пресс-машиной.

- **Код:** [`/obj/item/reagent_containers/applicator/pill`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/pill.dm#L1), [`/obj/item/reagent_containers/applicator/patch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/patch.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem.rs`: таблетки (проглатывание, растворение), сахарная оболочка (`PillShell`: содержимое попадает в желудок через её секунды, сахар из стакана утолщает, вода растворяет, осмотр), пластыри (1,5 с на себя и на другого, `AppliedPatch`: 0,75 ед./с в кровь, синтплоть — сразу, отпадают пустыми), печать химмастером (`crates/ss13_game/src/chem_master.rs`); нет проверки конечностей, оверлея пластыря на мобе и снятия пластыря, вкуса оболочки

### Пипетки
Капают малый объём, в том числе в глаза.

- **Код:** [`/obj/item/reagent_containers/dropper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/dropper.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem.rs` (`dropper`): набор из набираемых ёмкостей (не из мобов), закапывание в ёмкости и в глаза (в кровь), объём 1-5, пипетка по мензурке в нагревателе; нет защиты глаз (очки ловят капли), воздействия на глаза (`expose`), звука капли (не сконвертирован), оверлея заполнения

### Гипоспреи, медипены, ингаляторы, медгель
Мгновенный безыгольный ввод (гипоспрей КМО, медипены с эпинефрином и др.), ингалятор для астматиков, медгель-распылитель на кожу; борг-гипоспрей.

- **Код:** [`/obj/item/reagent_containers/hypospray`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/hypospray.dm#L1), [`/obj/item/reagent_containers/hypospray/medipen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/hypospray.dm#L140), [`/obj/item/inhaler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/inhaler.dm#L1), [`/obj/item/reagent_containers/medigel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/medigel.dm#L29), [`/obj/item/reagent_containers/borghypo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/robot/items/hypo.dm#L103); tgui: [`BorgHypo.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/BorgHypo.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem.rs`: гипоспрей (мгновенный ввод в кровь, проверка плотной одежды, ПКМ — набор из ёмкостей, не из мобов, звук), медипены и боевые инъекторы сквозь одежду, медипен ЛКМ в руке — себе, пустой ломается (`UsedUp`, спрайт `…0`); нет ингалятора, медгеля, борг-гипоспрея (окно `BorgHypo`), бесконечных гипоспреев, наклейки состава медипена при осмотре

### Распылители
Пульверизатор (космоочиститель, перцовый баллончик, огнетушащий), химраспылитель; струя переносит реагенты на тайлы и мобов.

- **Код:** [`/obj/item/reagent_containers/spray`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/spray.dm#L1), [`/obj/item/reagent_containers/spray/chemsprayer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/spray.dm#L309)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/janitor.rs`: пульверизатор с космоочистителем брызгает по соседнему тайлу и мобу (`amount_per_transfer_from_this`), чистит; нет перцового баллончика, огнетушащего и химраспылителя, струи на расстояние

### Баки и канистры
Водяные и топливные баки (топливный взрывается), пивная бочка, канистры, кулеры, кувшины, лейка, тряпка (моет и душит), приправы, пакеты капельниц.

- **Код:** [`/obj/structure/reagent_dispensers`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_dispenser.dm#L4), [`/obj/structure/reagent_dispensers/fueltank`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_dispenser.dm#L255), [`/obj/structure/reagent_dispensers/watertank`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_dispenser.dm#L233), [`/obj/item/rag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/rag.dm#L1), [`/obj/item/reagent_containers/condiment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/condiment.dm#L8), [`/obj/item/reagent_containers/chem_pack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/chem_pack.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem.rs`: баки как сливаемые контейнеры с содержимым; `crates/ss13_game/src/explosions/sources.rs` (`boom`, `fueltank_welder_act`): топливный бак взрывается по количеству топлива от взрыва, огня, пули и зажжённой сварки, незажжённая сварка заправляется из бака; нет `chem_splash` остальных реагентов, сборок-детонаторов, тряпки, приправ
