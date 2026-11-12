# Толчки, обезоруживание и таклы
Небоевой контроль: толчок правым кликом, выбивание предметов, прыжок-такл.

### Толчок (shove)
Правый клик пустой рукой по мобу толкает его на тайл; у стены или в другого моба — оба падают; толчок по оглушённому/лежачему с дейзом — пинок с параличом; «стаггер» замедляет.

- **Код:** [`proc/disarm()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L821), [`proc/disarm_precollide()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/human_defense.dm#L127)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/security.rs`

### Обезоруживание толчком
Толчок по пошатывающемуся мобу выбивает оружие из рук, по лежачему — любой предмет; некоторые предметы толкают с оружием (`disarm_attack`).

- **Код:** [`proc/disarm()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L821), [`/datum/element/disarm_attack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/disarm_attack.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/security.rs` (`shove`): выбивание оружия у пошатывающегося и любого предмета у лежачего; `crates/ss13_game/src/weapons/mod.rs` (`disarm_attack`): ПКМ щитом или подушкой толкает как пустая рука (блок защитника, кулдаун `attack_speed`; подушка по лежащему или схваченному душит, а не толкает). `// shortcut:` энергощит и телескопический щит всегда развёрнуты, отказа «сначала активируйте» нет

### Такл (прыжок)
Перчатки-такл (гриппер, перчатки Северной звезды и т.п.) дают прыжок на цель правым кликом в режиме броска: результат от сбивания до травмы самого прыгуна, зависит от черт, роста, брони.

- **Код:** [`/datum/component/tackler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/tackle.dm#L14), [`/obj/item/clothing/gloves/tackler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/gloves/tacklers.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/tackle.rs` (вызов из `throw.rs`): перчатки-такл (гриппер, дельфин, боевые, ракетные, «офбренд», футбольные — параметры `tacklers.dm`), ПКМ пустой рукой в режиме броска, проверки `checkTackle`, стамина и нокдаун прыгуна, `min_distance`, полёт до `range`; по карбону — бросок `rollTackle` (руки, неуклюжесть, здоровье <50, голый, `TRAIT_NOGUNS`, `skill_mod`), исходы плохой/нейтральный/хороший с броней груди, стаггером, пассивным/агрессивным захватом, блок защитника — нейтральный; в окно — «как жук», в препятствие — `splat` по скорости (вплоть до параплегии и сотрясения). `// shortcut:` прыжок мгновенный, нет опьянения/ран ног/роста/крыльев/хвостов/имплантов позвоночника/безумия/райот-брони в броске, разбора столов, опрокидывания вендоров и осколков окна
