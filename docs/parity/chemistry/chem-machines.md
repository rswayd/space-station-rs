# Химические машины
Раздатчики, химмастер, нагреватель, пандемия, дым-машина, гриндер и прочее оборудование химлаборатории и бара. В ss13-rs из них есть гриндер (`crates/ss13_game/src/kitchen.rs`), раздатчики (`crates/ss13_game/src/chem_dispenser.rs`), химмастер (`crates/ss13_game/src/chem_master.rs`) и нагреватель (`crates/ss13_game/src/chem_heater.rs`).

### Химический раздатчик
Выдаёт базовые реагенты из энергии в вставленную мензурку (tgui), набор зависит от деталей; запись и воспроизведение рецептов; емаг открывает опасные реагенты.

- **Код:** [`/obj/machinery/chem_dispenser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/chem_dispenser.dm#L1); tgui: [`ChemDispenser.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ChemDispenser.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem_dispenser.rs`, окно `crates/ss13_tgui/src/interfaces/chem_dispenser.rs` (общие с раздатчиками бара, [../kitchen/drinks-bar.md](../kitchen/drinks-bar.md)): базовый список, апгрейд-реагенты от сервопривода тир 4, выдача из ячейки, перезарядка, сборка из платы; запись, сохранение (имя, перезапись), выдача и очистка рецептов, виртуальная мензурка; цвета и подсказки pH, pH мензурки (кроме барных); емаг в руке; нет боковой панели рецептов реакций (поиск, теги, закрепление), окна «Reactions», разбрызгивания реагентов при ЭМИ (ЭМИ тратит заряд на `rand(7, 15)` реагентов, опустошает мензурку и разряжает ячейку, но реагенты исчезают — нет `chem_splash`, `crates/ss13_game/src/emp/machines.rs`), галлюцинаторных названий

_«Раздатчики напитков и алкоголя» описано в [../kitchen/drinks-bar.md](../kitchen/drinks-bar.md)._

### ХимМастер
Анализ реагентов, перенос в буфер, разлив в таблетки, пластыри, бутылки, флаконы по стилям; кондимастер для кухни.

- **Код:** [`/obj/machinery/chem_master`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/chem_master.dm#L3), [`/obj/machinery/chem_master/condimaster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/chem_master.dm#L585); tgui: [`ChemMaster.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ChemMaster.tsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/chem_master.rs`, окно `crates/ss13_tgui/src/interfaces/chem_master.rs`: вставка/замена/извлечение ёмкости, буфер по объёму мензурок-деталей, перенос 1/5/10/своё/всё/запрос числа, режим уничтожения, анализ реагента, выбор тары по категориям (пробирки, таблетки, пластыри; приправы у кондимастера) с подсказкой `default_container`, печать до 50 штук с именем (запрос текста), сахарная оболочка таблеток, остановка, скорость печати от сервоприводов, сборка из платы; не портированы оверлеи/свет, переключение режима платы отвёрткой, типы зависимостей в анализе (нет зависимостей, [reagent-mechanics.md](reagent-mechanics.md)), порядок тары — по пути типа

### Химический нагреватель
Нагревает или охлаждает мензурку до целевой температуры, показывает pH и ход реакций, корректирует pH буферами.

- **Код:** [`/obj/machinery/chem_heater`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/chem_heater.dm#L1); tgui: [`ChemHeater.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ChemHeater.tsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/chem_heater.rs`, окно `crates/ss13_tgui/src/interfaces/chem_heater.rs`: нагрев/охлаждение к цели (коэффициент от микролазеров) каждый тик машин и на каждом шаге реакции, запуск реакций, кислотный/щелочной буфер (впрыск, забор), варианты `withbuffer`/`debug`, список реакций с опасностью (перегрев, чистота), качеством и прогрессом, шприц по мензурке внутри, сборка из платы; круговые шкалы tgui (`RoundGauge`) показаны текстом

### Сепаратор, масс-спектрометр, синтезатор
Дистиллятор разделяет реагенты по температуре кипения; масс-спектрометр очищает от нечистых примесей; синтезатор (админ/дебаг) создаёт любые реагенты.

- **Код:** [`/obj/structure/chem_separator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/chem_separator.dm#L4), [`/obj/machinery/chem_mass_spec`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/chem_mass_spec.dm#L1), [`/obj/machinery/chem_dispenser/chem_synthesizer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/chem_synthesizer.dm#L1); tgui: [`ChemSeparator.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ChemSeparator.tsx), [`ChemSynthesizer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ChemSynthesizer.tsx)
- **Статус в ss13-rs:** ❌ нет

### Пандемия
Вирусологическая машина: анализ болезней в крови, создание культур и вакцин, переименование вирусов.

- **Код:** [`/obj/machinery/computer/pandemic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/pandemic.dm#L2); tgui: [`index.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Pandemic/index.tsx)
- **Статус в ss13-rs:** ❌ нет

### Дым-машина
Распыляет содержимое мензурки облаком дыма заданного размера.

- **Код:** [`/obj/machinery/smoke_machine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/smoke_machine.dm#L4); tgui: [`SmokeMachine.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SmokeMachine.tsx)
- **Статус в ss13-rs:** ❌ нет

_«Гриндер» описано в [../kitchen/cooking-machines.md](../kitchen/cooking-machines.md)._

### Отладка рецептов
Админ-машина для проверки всех рецептов и их параметров.

- **Код:** [`/obj/machinery/chem_recipe_debug`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/chem_recipe_debug.dm#L27); tgui: [`ChemRecipeDebug.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ChemRecipeDebug.tsx)
- **Статус в ss13-rs:** ❌ нет
