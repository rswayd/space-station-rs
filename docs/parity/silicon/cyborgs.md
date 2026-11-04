# Киборги
Мобы `/mob/living/silicon/robot`: сборка, модели, модули, апгрейды и обслуживание.

### Киборг как моб
Синтетик на батарее: три слота модулей, крышка с замком (доступ робототехники), фонарь, ионные двигатели (апгрейд), опрокидывание, перенос предметов на голове, связь с ИИ. Работа «Киборг» из лобби.

- **Код:** [`/mob/living/silicon/robot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot_defines.dm#L7), [`/mob/living/silicon/robot/Initialize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot.dm#L8)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/cyborg.rs`: работа «Cyborg» спавнит «Default Cyborg-NNN» на старте киборга, связь с ИИ с наименьшим числом боргов, законы ИИ, доступ ко всем дверям (внутренняя ID со всем доступом работ), ничего не подбирает; нет HUD киборга (показаны человеческие слоты), фонаря, двигателей, опрокидывания

### Батарея и расход энергии
Модули и движение тратят заряд ячейки; на нуле киборг беспомощен; HUD показывает заряд.

- **Код:** [`proc/handle_robot_cell()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/life.dm#L9), [`proc/use_energy()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/life.dm#L19)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/cyborg.rs`: ячейка высокой ёмкости (100 кДж), расход `BORG_MINIMUM_POWER_CONSUMPTION` в секунду в сознании, при 1% модули возвращаются в модель, при 0 — режим низкого питания без модулей; нет фонаря и тревог заряда на HUD

### Модули и слоты инвентаря
Три активных слота, выбор и смена модулей, поломка слотов при уроне, HUD модулей.

- **Код:** [`proc/activate_module()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/inventory.dm#L22), [`proc/break_cyborg_slot()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/inventory.dm#L85), [`robot/inventory.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/inventory.dm#L7)
- **Статус в ss13-rs:** 🟡 частично — модули лежат в предмете модели в слоте спины как хранилище: оттуда в руку, Q возвращает (`uneq_active`), модуль нельзя положить или убрать в чужое хранилище (`crates/ss13_game/src/silicon/cyborg.rs`, `inventory.rs`); две руки вместо трёх модульных слотов, нет поломки слотов

### Модели киборгов (станционные)
Выбор модели при первой активации: стандарт, инженерный, медицинский, шахтёр, уборщик, сервис, охранный, миротворец, клоун; каждая даёт свой набор модулей, иконки и черты.

- **Код:** [`/obj/item/robot_model`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot_model.dm#L9), [`proc/pick_model()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot.dm#L161), [`/obj/item/robot_model/engineering`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot_model.dm#L395), [`/obj/item/robot_model/medical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot_model.dm#L738), [`/obj/item/robot_model/security`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot_model.dm#L840), [`/obj/item/robot_model/service`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot_model.dm#L876)
- **Статус в ss13-rs:** 🟡 частично — выбор модели (`pick_model`) окном `tgui_input_list` из моделей tg с конфигом BandaStation (без охранной): инженерная, медицинская, шахтёрская, уборщик, сервисная, миротворец; спрайт, каналы радио и `basic_modules` из дампа (`ss13_mapload::silicon`); нет анимации трансформации, скинов, трейтов моделей и сброса модели

### Модели Синдиката
Штурмовой, медицинский и диверсант-борги Синдиката (ядерные оперативники).

- **Код:** [`/obj/item/robot_model/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot_model.dm#L927), [`/obj/item/robot_model/syndicate_medical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot_model.dm#L957), [`/obj/item/robot_model/saboteur`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot_model.dm#L990)
- **Статус в ss13-rs:** ❌ нет

### Скины боргов (BandaStation)
Дополнительные портированные облики моделей (инженер, медик и др.) с выбором при смене модели и искрами при трансформации.

- **Код:** [`/obj/item/robot_model`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot_model.dm#L9)
- **BandaStation:** [`modular_bandastation/borgs/code/robot_model.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/borgs/code/robot_model.dm#L4), [`robot_model.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot_model.dm#L302)
- **Статус в ss13-rs:** ❌ нет

### Предметы-модули киборгов
Станбатон-манипулятор, объятия, зарядник, омнитулы (мед/инж/ботаника), «аппараты» для мензурок, листов, органов; гипоспрей и шейкер, леденцы и жвачкомёт, кулинарная книга, мусорный бокс, пылесос, демпфер снарядов, гармонический сигнал, щит.

- **Код:** [`/obj/item/borg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/robot/items/generic.dm#L12), [`/obj/item/borg/cyborg_omnitool`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/robot/items/tools.dm#L161), [`/obj/item/borg/apparatus`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/robot/items/storage.dm#L1), [`/obj/item/reagent_containers/borghypo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/robot/items/hypo.dm#L103), [`/obj/item/borg/lollipop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/robot/items/food.dm#L6), [`/obj/item/borg/projectile_dampen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/robot/items/tools.dm#L16); tgui: [`BorgHypo.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/BorgHypo.tsx), [`BorgShaker.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/BorgShaker.tsx)
- **BandaStation:** [`hypo.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/robot/items/hypo.dm#L1) — в медгипоспрей добавлена шахтёрская мазь
- **Статус в ss13-rs:** 🟡 частично — `basic_modules` моделей спавнятся предметами (`ss13-mapload convert` их конвертирует); работают те, чьё поведение есть (сварка, анализаторы, шприцы, марля, лом, флеш, ...), киборг-специфичные (омнитул, RCD, гипоспрей, ...) — инертные предметы

### Апгрейды киборгов
38 плат: переименование, сброс/смена модели, самовосстановление, ионные двигатели, расширение, лавоупорность, бур с алмазом, RPED, индуктор, дефибриллятор, хирургический омнитул, гипоспреи, приложения для сервиса, чертежи шаттла и т. д.

- **Код:** [`/obj/item/borg/upgrade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/robot/robot_upgrades.dm#L4), [`proc/apply_upgrade()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot.dm#L758), [`/obj/item/borg/upgrade/transform`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/robot/robot_upgrades.dm#L729), [`/obj/item/borg/upgrade/selfrepair`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/robot/robot_upgrades.dm#L294)
- **Статус в ss13-rs:** ❌ нет

### Сборка киборга (эндоскелет)
Эндоскелет из частей тела роботов (мехфаб), проводка, ячейка, флеш-глаза, MMI/позитронный мозг → киборг; без мозга — оболочка ИИ. Разборка через крышку.

- **Код:** [`/obj/item/robot_suit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/robot/robot_parts.dm#L3), [`robot_parts.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/robot/robot_parts.dm), [`proc/cyborg_deconstruct()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot.dm#L538), [`/obj/item/brain_processor/positronic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/brain/processor/positronic.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Крышка, замок, провода киборга
Крышку открывает ID с доступом, ломом снимается ячейка, внутри провода (`/datum/wires/robot`: связь с ИИ, камера, блокировка, лоусинк, сброс модели).

- **Код:** [`proc/toggle_cover()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot.dm#L299), [`/datum/wires/robot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/robot.dm#L1)
- **Статус в ss13-rs:** ❌ нет — набора проводов робота в `crates/ss13_game/src/wires.rs` нет

### Емаг, блокировка и самоуничтожение
Емаг снимает законы и привязку к ИИ (законы Синдиката), блокировка (lockdown) парализует, детонация убивает; управляется с консоли робототехники или ИИ.

- **Код:** [`proc/set_lockcharge()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot.dm#L447), [`proc/self_destruct()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot.dm#L381), [`robot_defense.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot_defense.dm#L392)
- **Статус в ss13-rs:** ❌ нет

### Консоль робототехники
Список киборгов и дронов: блокировка, детонация, ручное управление ботами.

- **Код:** [`/obj/machinery/computer/robotics`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/robot.dm#L1); tgui: [`RoboticsControlConsole.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/RoboticsControlConsole.jsx)
- **Статус в ss13-rs:** ❌ нет

### Станция зарядки киборгов
Киборг заходит внутрь: заряжается, чинится (с апгрейдами), пополняет расходники модулей.

- **Код:** [`/obj/machinery/recharge_station`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/rechargestation.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/cyborg.rs`: клик пустой рукой открывает/закрывает, закрытие забирает киборга на тайле, при питании зарядка `recharge_speed` (`5e-3 × тиры конденсаторов × ёмкость ячейки`: 1 кВт со стоковыми деталями, `cyborg::recharge_speed`; детали меняются RPED — `crates/ss13_game/src/stock_parts.rs`), выход открывает станцию, спрайты состояний; нет восстановления расходников и материалов из силоса, ремонта апгрейженными сервами, людей

### Урон, ремонт и смерть киборга
Брутальный урон чинится сваркой, ожоги — кабелем; EMP оглушает, флеш ослепляет; при смерти мозг можно извлечь.

- **Код:** [`robot_defense.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot_defense.dm#L7), [`robot/death.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/death.dm#L10), [`proc/dump_into_mmi()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot.dm#L573)
- **Статус в ss13-rs:** 🟡 частично — только brute/burn, без крита, смерть на −100 (`crates/ss13_game/src/silicon/mod.rs`), мёртвый киборг инертен, при движении — призрак; нет ремонта сваркой и кабелем, поломки модулей, обломков, EMP

### NTOS Robotact и BorgPanel
Встроенный в киборга компьютер (законы, апгрейды, журнал) и админская панель киборга.

- **Код:** [`proc/create_modularInterface()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/silicon.dm#L123); tgui: [`NtosRobotact.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/NtosRobotact.jsx), [`BorgPanel.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/BorgPanel.jsx)
- **Статус в ss13-rs:** ❌ нет
