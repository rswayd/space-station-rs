# Мехи (экзокостюмы)
Закрытый бронированный транспорт `/obj/vehicle/sealed/mecha`: пилот, ячейка, оборудование по слотам.

### Базовый мех
Пилот внутри (защищён от среды, свой воздушный бак и кабина), энергия из ячейки, броня, интерфейс меха, освещение, внутренние повреждения, стрейф, кнопка извлечения; форк меняет расчёт скорости.

- **Код:** [`/obj/vehicle/sealed/mecha`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/_mecha.dm#L21), [`mecha_movement.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mecha_movement.dm#L67), [`mecha_ui.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mecha_ui.dm#L5); tgui: [`Mecha`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Mecha)
- **BandaStation:** [`mecha_movement.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mecha_movement.dm#L78) — скорость движения
- **Статус в ss13-rs:** 🟡 частично: `crates/ss13_game/src/mecha/core.rs` — детали внутри (ячейка, сканер, конденсатор, сервопривод), заряд ячейки, посадка перетаскиванием себя на мех с `enter_delay` и `enter_checks`, ID-замок, пилот на клетке меха (скрыт), движение пилотом (`vehicle_move`: кулдаун BandaStation `modified_move_delay`, `step_energy_drain` за каждую попытку, поворот перед шагом, `pivot_step`, стрейф alt+клик по меху), давление у Рипли (`update_pressure`), двери открываются доступом пилота, толкание незакреплённого при ударе, клики пилота (предохранитель средней кнопкой, оборудование левой/правой руки, удар меха), катапультирование по B (`exit_delay`), алерты заряда и целостности; нет окна `Mecha`, кнопок действий, света, кабины и воздуха, внутренних повреждений, оверклока, MMI/ИИ-пилотов

### Урон, защита и обломки меха
Броня по типам, направленная защита (фронт), внутренние поломки (пожар, утечка, короткое замыкание), EMP, ремонт сваркой; уничтоженный мех оставляет обломки, из которых можно снять детали.

- **Код:** [`mecha_defense.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mecha_defense.dm#L17), [`mecha_damage.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mecha_damage.dm#L17), [`/obj/structure/mecha_wreckage`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mecha_wreckage.dm#L6)
- **Статус в ss13-rs:** 🟡 частично: `crates/ss13_game/src/mecha/core.rs` — `take_damage` с бронёй по типу и модификаторами направления (фронт ×0,5, бок ×1, тыл ×1,5), удары предметами, рукой («Nothing happens»), луч эмиттера; при 0 пилот выброшен (паралич 2 с вместо сна), груз высыпается, обломки получают 30% снимаемого оборудования и ячейку; сварка (30% неудача, `salvage_num`) и лом снимают трофеи; ЭМИ (`crates/ss13_game/src/emp/power.rs`): половина ячейки и ожог `rand(100, 180)` делённые на `severity × рейтинг конденсатора`, оборудование отказывает 3 с («Equipment control unit is unresponsive»), нет камеры и маячков; нет внутренних поломок, ремонта сваркой, попаданий по пилоту, кусачек по обломкам

### Ближний бой меха и взаимодействие с мобами
Удары кулаком меха по мобам и объектам, посадка/высадка, захват ИИ (через интеллКарту/маячок).

- **Код:** [`mech_melee_attack.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mech_melee_attack.dm#L10), [`mecha_mob_interaction.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mecha_mob_interaction.dm#L1), [`mecha_ai_interaction.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mecha_ai_interaction.dm#L1)
- **Статус в ss13-rs:** 🟡 частично: `crates/ss13_game/src/mecha/core.rs` — посадка/высадка, удар меха (`mech_melee_attack`: мобы отталкиваются, в боевом режиме урон `force`; структуры `force × 3`); нет ударов по стенам, ИИ и MMI

### Рабочие мехи: Рипли, Пэдди, Кларк
Рипли (гидроклешня, бур; шахтёрский/грузовой варианты, апгрейд в Пэдди для охраны), Кларк (быстрый шахтёрский). Форк правит баланс Пэдди.

- **Код:** [`/obj/vehicle/sealed/mecha/ripley`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/working/ripley.dm#L1), [`/obj/vehicle/sealed/mecha/ripley/paddy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/working/ripley.dm#L94), [`/obj/vehicle/sealed/mecha/clarke`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/working/clarke.dm#L2)
- **BandaStation:** [`modular_bandastation/balance/code/vehicle/paddy.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/vehicle/paddy.dm#L1) — скорость и шаги давления Пэдди
- **Статус в ss13-rs:** 🟡 частично: `crates/ss13_game/src/mecha/` — Рипли MK-I (характеристики из DM, отсек груза, гидроклешня, сборка); Пэдди и Кларк заводятся как типы, но их особенности (сирена, клетка, бур Кларка, сборка Кларка) не перенесены; нет апгрейда в MK-II и шкур голиафа

### Медицинский мех: Одиссей
Мех с медицинским HUD, слипером, шприцемётом и лечебным лучом.

- **Код:** [`/obj/vehicle/sealed/mecha/odysseus`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/medical/odysseus.dm#L1)
- **Статус в ss13-rs:** 🟡 частично: `crates/ss13_game/src/mecha/` — Одиссей собирается по общим шагам и ездит с характеристиками из DM (`pivot_step`, свой `step_energy_drain`); нет медицинского HUD пилота и медицинского оборудования

### Боевые мехи
Гайгакс (ускорение), Дюранд (щит), Фазон (фазировка), Хонкер, Ретиценс (мим), Саванна-Иванов (двухместный, прыжок/ракеты), Мародёр/Серафим/Маулер (ОБР, Синдикат). Форк добавляет снаряжённые версии мехов ЦК/ОБР.

- **Код:** [`/obj/vehicle/sealed/mecha/gygax`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/combat/gygax.dm#L1), [`/obj/vehicle/sealed/mecha/durand`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/combat/durand.dm#L1), [`/obj/vehicle/sealed/mecha/phazon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/combat/phazon.dm#L1), [`/obj/vehicle/sealed/mecha/honker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/combat/honker.dm#L1), [`/obj/vehicle/sealed/mecha/reticence`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/combat/reticence.dm#L1), [`/obj/vehicle/sealed/mecha/savannah_ivanov`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/combat/savannah_ivanov.dm#L15), [`/obj/vehicle/sealed/mecha/marauder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/combat/marauder.dm#L1)
- **BandaStation:** [`modular_bandastation/objects/code/mecha/mecha.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/mecha/mecha.dm#L4) — загруженные мехи ЦК/ОБР
- **Статус в ss13-rs:** ❌ нет

### Мини-мех «Vim»
Крошечный мех для мелких существ, собирается из ведра/шлема.

- **Код:** [`/obj/vehicle/sealed/mecha/vim`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/miniature/vim.dm#L7)
- **Статус в ss13-rs:** ❌ нет

### Оборудование меха: инструменты
Гидроклешня, бур и алмазный бур, шахтёрский сканер, огнетушитель, RCD, радио, воздушный бак, генераторы, ремонтный дроид, ускорители (ионные/газовые), броневые усилители, телепортер, генератор червоточин, гравкатапульта, скрытый оружейный отсек, апгрейд Рипли.

- **Код:** [`/obj/item/mecha_parts/mecha_equipment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/equipment/mecha_equipment.dm#L5), [`/obj/item/mecha_parts/mecha_equipment/hydraulic_clamp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/equipment/tools/work_tools.dm#L5), [`/obj/item/mecha_parts/mecha_equipment/drill`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/equipment/tools/mining_tools.dm#L8), [`/obj/item/mecha_parts/mecha_equipment/rcd`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/equipment/tools/work_tools.dm#L230), [`/obj/item/mecha_parts/mecha_equipment/repair_droid`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/equipment/tools/other_tools.dm#L221), [`/obj/item/mecha_parts/mecha_equipment/thrusters`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/equipment/tools/other_tools.dm#L411), [`/obj/item/mecha_parts/mecha_equipment/armor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/equipment/tools/other_tools.dm#L153)
- **Статус в ss13-rs:** 🟡 частично: `crates/ss13_game/src/mecha/equipment.rs` — установка на руки/слоты (`try_attach_part`, `mech_flags`, лимиты категорий), кулдауны и энергия; гидроклешня (груз в отсек с `do_after`, толчок/сжатие мобов, «firmly secured»), отсек груза (высыпается при снятии/уничтожении), бур (стены за `60 / drill_level`, укреплённые — только алмазным, объекты и мобы по кругу; работает по клику, пока установлен — нет окна для переключения); нет остального оборудования, выгрузки груза, шахтёрства

### Оборудование меха: медицинское
Слипер, шприцемёт меха, медицинский луч.

- **Код:** [`/obj/item/mecha_parts/mecha_equipment/medical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/equipment/tools/medical_tools.dm#L3), [`/obj/item/mecha_parts/mecha_equipment/sleeper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/equipment/tools/medical_tools.dm#L26), [`/obj/item/mecha_parts/mecha_equipment/medical/syringe_gun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/equipment/tools/medical_tools.dm#L208), [`/obj/item/mecha_parts/mecha_equipment/medical/mechmedbeam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/equipment/tools/medical_tools.dm#L371)
- **Статус в ss13-rs:** ❌ нет

### Оборудование меха: оружие
Энергетическое (лазер, тяжёлый лазер, дизейблер, ион, тесла, пульс, плазменный резак, термо/крио/инферно, кинетический, тазер), баллистическое (карабин, глушёный, дробовик, ПКМ, ракетницы, гранатомёты, банановый и мышеловочный миномёты, перчатка-бокс), хонкер, клешня Пэдди; боезапас.

- **Код:** [`/obj/item/mecha_parts/mecha_equipment/weapon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/equipment/weapons/weapons.dm#L1), [`/obj/item/mecha_parts/mecha_equipment/weapon/energy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/equipment/weapons/weapons.dm#L91), [`/obj/item/mecha_parts/mecha_equipment/weapon/ballistic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/equipment/weapons/weapons.dm#L301), [`mecha_ammo.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/equipment/weapons/mecha_ammo.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Сборка мехов
Пошаговая сборка из шасси и частей (мехфаб) с проводкой, платами, ячейкой, бронёй; форк меняет шаг ядра для Гайгакса, Дюранда, Фазона и Саванны-Иванова.

- **Код:** [`/datum/component/construction/mecha`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mecha_construction_paths.dm#L2), [`mecha_parts.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mecha_parts.dm#L5)
- **BandaStation:** [`mecha_construction_paths.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mecha_construction_paths.dm#L430) — ядра боевых мехов
- **Статус в ss13-rs:** 🟡 частично: `crates/ss13_game/src/mecha/construction.rs` — части шасси в любом порядке (оверлеи `+o`), затем шаги tg `get_steps` (гидравлика, проводка, платы, сканер, конденсатор, сервопривод, ячейка, внутренняя и внешняя обшивка; кабина Рипли), откат `back_key` с возвратом предметов, спрайты `<base><n>`, мех из содержимого рамы; шаги Кларка, Гайгакса, Дюранда, Фазона, Хонкера и Саванны (свои процедуры, ядра BandaStation) не перенесены

### Фабрикатор экзокостюмов
Печатает части мехов, боргов, ИИ-модули из материалов по исследованным чертежам; очередь печати. Форк переводит названия чертежей.

- **Код:** [`/obj/machinery/mecha_part_fabricator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mech_fabricator.dm#L1); tgui: [`ExosuitFabricator.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ExosuitFabricator.tsx)
- **BandaStation:** [`mech_fabricator.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mech_fabricator.dm#L373) — оригинальное имя чертежа в UI
- **Статус в ss13-rs:** 🟡 частично: `crates/ss13_game/src/mecha/fab.rs`, окно `crates/ss13_game/src/mecha/client.rs` (`ExosuitFabricator`) — исследованные `MECHFAB`-чертежи, материалы через рудный силос (или своё хранилище у `maint`), очередь (добавить, «собрать сейчас», всё в очередь, старт/стоп, очистить, убрать), материалы списываются при старте детали, выход на юг с ожиданием при занятой клетке, выдача листов; нет направления выдачи, перевода названий чертежей, апгрейда деталей, активного энергопотребления

### Зарядная станция мехов
Порт перезарядки ячейки меха и консоль питания мех-отсека.

- **Код:** [`/obj/machinery/mech_bay_recharge_port`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mech_bay.dm#L1), [`/obj/machinery/computer/mech_bay_power_console`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mech_bay.dm#L94); tgui: [`MechBayPowerConsole.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/MechBayPowerConsole.jsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mecha/bay.rs`: порт заряжает меха на тайле перед собой 625 Вт (только сеть), консоль связывается (`reconnect`, «Sync»), окно `MechBayPowerConsole`, экран `recharge_comp_on`; пол `recharge_floor` декоративный, как в tg; нет деталей выше tier 1, поворота ключом и разборки

### Консоль контроля экзокостюмов и маячки
Маячки слежения на мехах: консоль показывает их состояние, отправляет сообщения, ЭМИ-импульс; ИИ-маячок даёт управление ИИ.

- **Код:** [`/obj/machinery/computer/mecha`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mecha_control_console.dm#L1), [`/obj/item/mecha_parts/mecha_tracking`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mecha_control_console.dm#L77), [`/obj/item/mecha_parts/mecha_tracking/ai_control`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mecha_control_console.dm#L157); tgui: [`ExosuitControlConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ExosuitControlConsole.tsx)
- **Статус в ss13-rs:** ❌ нет

### Диагностика меха
Диагностический HUD и сканирование меха.

- **Код:** [`mecha_diagnostic.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/mecha/mecha_diagnostic.dm#L1)
- **Статус в ss13-rs:** ❌ нет
