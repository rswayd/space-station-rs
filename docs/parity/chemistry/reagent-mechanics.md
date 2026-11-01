# Реагенты: хранение, перенос и воздействие
Держатель реагентов `/datum/reagents`, переливание, способы воздействия, метаболизм в мобе, передозировки и зависимости. Реакции — [reactions.md](reactions.md).

### Данные реагентов и рецептов
Все `/datum/reagent` (цвет, вкус, скорость метаболизма, порог передозировки, pH, чистота, флаги) и `/datum/chemical_reaction` загружаются на сервере.

- **Код:** [`/datum/reagent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents.dm#L2), [`/datum/chemical_reaction`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/recipes.dm#L8)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_reagents/src/data.rs`: генерируется `ss13-mapload convert` в `reagents.ron` (без рандомизированных рецептов и рецептов с особыми проверками)

### Держатель реагентов
Объём, добавление/удаление, перенос (`trans_to`) с долями и чистотой, копирование, превращение одного реагента в другой, температура и теплоёмкость, средний pH; флаги контейнера (открытый, впрыскиваемый, сливаемый).

- **Код:** [`/datum/reagents`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/holder/holder.dm#L4), [`proc/add_reagent()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/holder/holder.dm#L81), [`proc/remove_reagent()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/holder/holder.dm#L212), [`proc/trans_to()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/holder/holder.dm#L422), [`proc/convert_reagent()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/holder/holder.dm#L327), [`proc/set_temperature()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/holder/properties.dm#L97), [`proc/adjust_thermal_energy()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/holder/properties.dm#L84)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_reagents/src/holder.rs`: объём, добавление, перенос, температура, pH, чистота, флаги; нет reagent `data` (ДНК крови, вирусы, данные вкуса)

### Способы воздействия (INGEST/INJECT/TOUCH/VAPOR/PATCH)
`expose` применяет реагенты к мобу, предмету или тайлу по способу: проглатывание, инъекция, контакт (с защитой кожи), вдыхание, пластырь; вода тушит огонь, кислота плавит, топливо горит, космоочиститель моет.

- **Код:** [`proc/expose()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/holder/holder.dm#L744), [`proc/expose_mob()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents.dm#L129), [`proc/expose_obj()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents.dm#L147), [`proc/expose_turf()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents.dm#L153), [`proc/reagent_expose()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/life.dm#L148)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_reagents/src/holder.rs`, `crates/ss13_game/src/chem.rs` (shortcut: `expose_*` не портированы, разливание ни на что не действует)

### Метаболизм в мобе
Каждый тик `SSmobs` реагенты из желудка переходят в кровь, а в крови метаболизируются со своей скоростью (`metabolization_rate`), вызывая `on_mob_life`; печень фильтрует токсины, у мёртвых — особая обработка.

- **Код:** [`proc/metabolize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/holder/mob_life.dm#L16), [`proc/metabolize_reagent()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/holder/mob_life.dm#L95), [`proc/on_mob_life()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents.dm#L193), [`proc/on_mob_metabolize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents.dm#L233)
- **BandaStation:** [`code/modules/reagents/chemistry/holder/mob_life.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/holder/mob_life.dm#L131) — порог «критической» передозировки для новых химикатов форка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_reagents/src/metabolism.rs`, `crates/ss13_game/src/chem.rs`: желудок → кровь, скорости tg, печень, эффекты стартового набора; остальные реагенты только убывают

### Передозировка
Выше `overdose_threshold` реагент переходит в режим передозировки с отдельными вредными эффектами (`overdose_process`).

- **Код:** [`proc/overdose_process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents.dm#L278)
- **BandaStation:** [`code/modules/reagents/chemistry/reagents.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents.dm#L38) — `overdose_crit_threshold`
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_reagents/src/metabolism.rs`: флаг передозировки и эффекты передоза стартового набора

### Зависимости и абстиненция
Наркотики, алкоголь, никотин, опиаты и т.д. копят очки зависимости; при отказе — стадии абстиненции с моодлетами и эффектами.

- **Код:** [`/datum/addiction`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/withdrawal/_addiction.dm#L4), [`generic_addictions.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/withdrawal/generic_addictions.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Накопительные эффекты метаболизма
Совместный метаболизм определённых реагентов (напитков) даёт дополнительные эффекты.

- **Код:** [`/datum/stacked_metabolization_effect`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/stacked_effects/stacked_effects.dm#L2), [`bevarages.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/stacked_effects/bevarages.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Вкус и видимость содержимого
Реагенты имеют вкус (`taste_description`); содержимое контейнера видно при осмотре только с научными очками или аналогами.

- **Код:** [`taste.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/taste.dm#L1), [`ui_data.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/holder/ui_data.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem.rs`: содержимое при осмотре видно всем (shortcut); вкус напитков — нет
