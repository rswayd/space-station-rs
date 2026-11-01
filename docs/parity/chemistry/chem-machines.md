# Химические машины
Раздатчики, химмастер, нагреватель, пандемия, дым-машина, гриндер и прочее оборудование химлаборатории и бара. В ss13-rs из них есть только гриндер (`crates/ss13_game/src/kitchen.rs`).

### Химический раздатчик
Выдаёт базовые реагенты из энергии в вставленную мензурку (tgui), набор зависит от деталей; запись и воспроизведение рецептов; емаг открывает опасные реагенты.

- **Код:** [`/obj/machinery/chem_dispenser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/chem_dispenser.dm#L1); tgui: [`ChemDispenser.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ChemDispenser.tsx)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/chem.rs` (shortcut: нет раздатчика)

_«Раздатчики напитков и алкоголя» описано в [../kitchen/drinks-bar.md](../kitchen/drinks-bar.md)._

### ХимМастер
Анализ реагентов, перенос в буфер, разлив в таблетки, пластыри, бутылки, флаконы по стилям; кондимастер для кухни.

- **Код:** [`/obj/machinery/chem_master`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/chem_master.dm#L3), [`/obj/machinery/chem_master/condimaster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/chem_master.dm#L585); tgui: [`ChemMaster.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ChemMaster.tsx)
- **Статус в ss13-rs:** ❌ нет

### Химический нагреватель
Нагревает или охлаждает мензурку до целевой температуры, показывает pH и ход реакций, корректирует pH буферами.

- **Код:** [`/obj/machinery/chem_heater`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/chem_heater.dm#L1); tgui: [`ChemHeater.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ChemHeater.tsx)
- **Статус в ss13-rs:** ❌ нет

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
