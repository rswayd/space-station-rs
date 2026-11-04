# Химические реакции
Мгновенные и пошаговые реакции, температура, pH, чистота, катализаторы, взрывы и эффекты реакций.

### Запуск реакций
`handle_reactions` проверяет рецепты по реагентам, катализаторам, контейнеру, температуре и pH; мгновенные (`REACTION_INSTANT`) завершаются сразу, остальные становятся равновесием; сообщение о смешивании видят рядом.

- **Код:** [`proc/handle_reactions()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/holder/reactions.dm#L5), [`/datum/chemical_reaction`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/recipes.dm#L8)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_reagents/src/holder.rs` (`handle_reactions`), `crates/ss13_game/src/chem.rs`: сообщения о смешивании

### Пошаговые реакции (equilibrium)
Подсистема `SSreagents` шагает реакцию каждые 0.25 с со скоростью, зависящей от температуры и pH; реакция поглощает/выделяет тепло и меняет pH; рецепты-оптимумы (fermichem).

- **Код:** [`/datum/equilibrium`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/equilibrium.dm#L13), [`PROCESSING_SUBSYSTEM_DEF(reagents)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/processing/reagents.dm#L3)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_reagents/src/holder.rs` (`Equilibrium`, `Holder::process`)

### Чистота, нечистые и обратные реагенты
Чистота продукта зависит от pH реакции; нечистые реагенты (`impurity`) и обратные (`inverse`, ниже `inverse_chem_val`) имеют свои, часто вредные эффекты; перегрев и чрезмерная нечистота портят реакцию.

- **Код:** [`/datum/reagent/impurity`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents/impure_reagents.dm#L6), [`/datum/reagent/inverse`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents/impure_reagents.dm#L31), [`proc/overheated()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/recipes.dm#L146), [`proc/overly_impure()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/recipes.dm#L165), [`impure_medicine_reagents.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents/impure_reagents/impure_medicine_reagents.dm#L1)
- **BandaStation:** [`code/modules/reagents/chemistry/reagents/impure_reagents.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents/impure_reagents.dm#L105) — исправление криогелидии
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_reagents/src/holder.rs`: чистота, превращение в обратный реагент, потеря выхода и чистоты при перегреве/нечистоте; эффектов нечистых/обратных реагентов на моба нет

### Катализаторы и реакционные агенты
Катализаторы нужны для реакции и не расходуются; агенты-катализаторы ускоряют или меняют pH/температуру; реакционные агенты (кислотные/щелочные буферы) корректируют pH.

- **Код:** [`/datum/reagent/catalyst_agent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents/catalyst_reagents.dm#L2), [`/datum/reagent/reaction_agent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents/reaction_agents_reagents.dm#L1), [`recipes/catalysts.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/recipes/catalysts.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_reagents/src/holder.rs`: обязательные катализаторы рецептов; нет агентов-катализаторов (shortcut), буферы работают как обычные реагенты

### Взрывы и эффекты реакций
Реакции со своими эффектами (`on_reaction`): взрывы (нитроглицерин, чёрный порох, плазма), вспышки, ЭМИ, дым, пена, огонь (CLF3, напалм), создание мобов и предметов, телепорт.

- **Код:** [`proc/on_reaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/recipes.dm#L82), [`/datum/chemical_reaction/reagent_explosion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/recipes/pyrotechnics.dm#L10), [`recipes/pyrotechnics.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/recipes/pyrotechnics.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_reagents/src/holder.rs` (`reacted`), `crates/ss13_game/src/chem.rs` (`reagent_explosions`), данные `ExplosionDef` (`crates/ss13_mapload/src/reagents.rs`): взрывы `reagent_explosion` без своего `on_reaction` (калий+вода, нитроглицерин при нагреве, RDX, пентрит и др.) — сила `modifier + round(объём / strengthdiv)`, `dyn_explosion`, сообщение, очистка ёмкости; ЭМИ-реакция (`emp_pulse`: уран + железо + алюминий → `empulse(round(V/1,5/12), round(V/1,5/7))`, ёмкость очищается — `crates/ss13_game/src/chem.rs` `reagent_emps`); нет взрывов со своими условиями (стабилизаторы, TATP, святая вода, порох, теслиум), взрывов в мобах, вспышек, дыма, пены, огня, создания мобов и предметов

### Нагрев от внешних источников
Содержимое нагревается от зажигалок, сварки, горелок, огня и пожара в комнате (`expose_temperature`), запуская температурные реакции.

- **Код:** [`proc/expose_temperature()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/holder/holder.dm#L769)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_reagents/src/holder.rs` (`expose_temperature`), `crates/ss13_game/src/chem.rs` (`heat_with_tool`): зажжённая сварка греет стакан (`reagents_item_heatable`); химнагреватель — [chem-machines.md](chem-machines.md); нет зажигалок, спичек, горелок (у них нет состояния «горит»), огнетушителя, огня и пожара в комнате, тепла тела

### Реакции экстрактов слаймов
Экстракты слаймов с плазмой/кровью/водой дают особые эффекты (см. агента science).

- **Код:** [`recipes/slime_extracts.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/recipes/slime_extracts.dm#L1)
- **BandaStation:** [`code/modules/reagents/chemistry/recipes/slime_extracts.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/recipes/slime_extracts.dm#L583) — убрано зелье полёта
- **Статус в ss13-rs:** ❌ нет

### Брызги химии и химический дым/пена
Разбитый контейнер или реакция разбрызгивают реагенты (`chem_splash`), дым и пена разносят реагенты по площади и воздействуют на всех.

- **Код:** [`proc/chem_splash()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chem_splash.dm#L22), [`/datum/effect_system/fluid_spread/smoke/chem`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_smoke.dm#L429), [`/datum/effect_system/fluid_spread/foam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_foam.dm#L218)
- **Статус в ss13-rs:** ❌ нет
