# Толчки, обезоруживание и таклы
Небоевой контроль: толчок правым кликом, выбивание предметов, прыжок-такл.

### Толчок (shove)
Правый клик пустой рукой по мобу толкает его на тайл; у стены или в другого моба — оба падают; толчок по оглушённому/лежачему с дейзом — пинок с параличом; «стаггер» замедляет.

- **Код:** [`proc/disarm()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L821), [`proc/disarm_precollide()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/human_defense.dm#L127)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/security.rs`

### Обезоруживание толчком
Толчок по пошатывающемуся мобу выбивает оружие из рук, по лежачему — любой предмет; некоторые предметы толкают с оружием (`disarm_attack`).

- **Код:** [`proc/disarm()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L821), [`/datum/element/disarm_attack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/disarm_attack.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/security.rs`: выбивание оружия у пошатывающегося и любого предмета у лежачего; нет `disarm_attack` предметов

### Такл (прыжок)
Перчатки-такл (гриппер, перчатки Северной звезды и т.п.) дают прыжок на цель правым кликом в режиме броска: результат от сбивания до травмы самого прыгуна, зависит от черт, роста, брони.

- **Код:** [`/datum/component/tackler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/tackle.dm#L14), [`/obj/item/clothing/gloves/tackler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/gloves/tacklers.dm#L1)
- **Статус в ss13-rs:** ❌ нет
