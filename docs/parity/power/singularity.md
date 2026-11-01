# Сингулярность, тесла и защитное поле
Двигатели на основе энергетических аномалий: эмиттеры, генераторы поля, сингулярность, шар теслы (переработан BandaStation), тесла-катушки и заземлители.

### Эмиттер
Стационарный лазер, который стреляет по таймеру при питании из сети: заряжает генераторы поля, кормит суперматерию и теслу; монтаж ключом и сваркой, блокировка ID, емаг, встраивание энергооружия и дисков с режимами снарядов, прототип с ручным управлением.

- **Код:** [`/obj/machinery/power/emitter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/emitter.dm#L1), [`process_early()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/emitter.dm#L209), [`proc/fire_beam()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/emitter.dm#L252), [`proc/integrate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/emitter.dm#L402), [`/obj/item/emitter_disk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/emitter.dm#L631), [`emitter/prototype`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/emitter.dm#L462)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/power/emitter.rs` (`Emitter::process_early`, `trace`, `tool_act`): питание 300 Вт из сети в раннюю фазу тика, очередь из 4 выстрелов через 2 с и пауза 2–10 с, хитскан-луч через стекло/решётки/столы/стеклянные двери с отражателями (30 урона, +60 МэВ суперматерии), включение рукой, ключ и сварка; нет блокировки ID, емага, встраивания оружия и дисков, прототипа, рисунка луча (shortcut)

### Генератор поля
Заряжается выстрелами эмиттера, после прогрева соединяется с генераторами в пределах 8 тайлов по прямой полем сдерживания; без подзарядки выключается.

- **Код:** [`/obj/machinery/field/generator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/field_generator.dm#L26), [`proc/check_power_level()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/field_generator.dm#L201), [`proc/setup_field()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/field_generator.dm#L304)
- **Статус в ss13-rs:** ❌ нет

### Поле сдерживания
Энергетическая стена между генераторами: удерживает сингулярность и теслу, бьёт током и отбрасывает живых.

- **Код:** [`/obj/machinery/field/containment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/containment_field.dm#L3), [`proc/bump_field()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/containment_field.dm#L158), [`proc/yeet_shock()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/containment_field.dm#L137)
- **Статус в ss13-rs:** ❌ нет

### Гравитационная сингулярность
Растёт от поглощённой материи (6 стадий), поглощает всё на пути, излучает радиацию, ЭМИ, оглушает смотрящих; за полем блуждает; при выходе поглощает станцию.

- **Код:** [`/obj/singularity`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/singularity.dm#L2), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/singularity.dm#L162), [`proc/expand()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/singularity.dm#L184), [`proc/consume()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/singularity.dm#L314), [`proc/event()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/singularity.dm#L419)
- **Статус в ss13-rs:** ❌ нет

### Тёмная сингулярность, разрыв реальности, Нар'Си
Особые объекты на основе сингулярности: тёмная сингулярность (событие), разрыв реальности, аватар Нар'Си (конец раунда культа). BandaStation меняет звуки, объявление и таймер Нар'Си («Cult Overhaul»).

- **Код:** [`/obj/singularity/dark_matter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/dark_matter_singularity.dm#L5), [`/obj/reality_tear`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/reality_tear.dm#L4), [`/obj/narsie`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/narsie.dm#L11), [`proc/start_ending_the_round()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/narsie.dm#L55)
- **BandaStation:** [`narsie.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/singularity/narsie.dm#L71) — звуки, текст объявления, 15 с до объявления, отключён Delta-уровень
- **Статус в ss13-rs:** ❌ нет

### Шар теслы (upstream energy_ball)
Шар энергии из ускорителя/деламинации: блуждает, бьёт молниями (`tesla_zap`) по катушкам, машинам и мобам, плодит мини-шары.

- **Код:** [`/obj/energy_ball`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/tesla/energy_ball.dm#L14), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/tesla/energy_ball.dm#L65), [`proc/tesla_zap()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/tesla/energy_ball.dm#L204)
- **Статус в ss13-rs:** ❌ нет

### Переработанный тесла-двигатель BandaStation
Модпак «Tesla Rework» заменяет теслу на `/obj/tesla_ball`: высокоуровневый источник энергии (1500 МДж на старте), энергия затухает и пополняется выстрелами эмиттера (45 МДж), реагирует на газ вокруг (температура, плазма), молнии с приоритетом целей и долей мощности, мини-шары, свои звуки движения и разрушения; деламинация суперматерии в теслу создаёт его.

- **Код:** [`/datum/sm_delam/proc/effect_tesla()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/supermatter/supermatter_delamination/delamination_effects.dm#L107)
- **BandaStation:** [`/obj/tesla_ball`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tesla_rework/code/tesla_ball.dm#L20), [`proc/process_atmos()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tesla_rework/code/tesla_ball.dm#L188), [`proc/tesla_reworked_zap()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tesla_rework/code/tesla_ball.dm#L327), [`delamination_effects.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/supermatter/supermatter_delamination/delamination_effects.dm#L113)
- **Статус в ss13-rs:** ❌ нет

### Тесла-катушка
Принимает молнии и превращает их энергию в мощность сети; с деталями — больше; может перенаправлять разряд.

- **Код:** [`/obj/machinery/power/energy_accumulator/tesla_coil`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/tesla/coil.dm#L1), [`zap_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/tesla/coil.dm#L88)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/power/accumulator.rs` (`Accumulator::zap`), `crates/ss13_game/src/power/supermatter.rs` (`zap`): берёт 25% разряда (T1-конденсатор) в сеть, остальное передаёт дальше; нет деталей, проводов, собственных разрядов и монтажа

### Заземлитель (grounding rod)
Принимает молнии и гасит их, защищая окружение.

- **Код:** [`/obj/machinery/power/energy_accumulator/grounding_rod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/tesla/coil.dm#L114), [`zap_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/tesla/coil.dm#L155)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/power/accumulator.rs`: поглощает весь разряд и не отдаёт в сеть; монтажа нет
