# ДНК
Датум ДНК: идентичность, черты внешности, блоки мутаций, нестабильность. В ss13-rs ДНК нет вовсе.

### Датум ДНК и его блоки
У карбона ДНК хранит уникальную идентичность (UI: пол, цвет кожи, волос, глаз), особенности вида (UF: хвосты, уши, маркировки), энзимы (UE, для криминалистики), группу крови, вид и последовательности мутаций (SE).

- **Код:** [`/datum/dna`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/dna.dm#L29), [`/datum/dna_block`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/dna_block.dm#L9), [`proc/generate_unique_identity()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/dna.dm#L185), [`proc/generate_unique_enzymes()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/dna.dm#L249), [`proc/generate_dna_blocks()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/dna.dm#L205), [`proc/update_dna_identity()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/dna.dm#L327)
- **BandaStation:** [`code/datums/dna/blocks/dna_features_block.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/blocks/dna_features_block.dm#L90) — блоки особенностей видов форка; [`code/datums/dna/dna.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/dna.dm#L405) — вампир-антаг и вид несовместимы
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/genetics/mod.rs`: `Dna` у каждого человека: энзимы (у экипажа = хэш ДНК записи), блоки UI пол/тон кожи/глаза/волосы (`construct_block`), 8 блоков мутаций с выщербленными последовательностями (`generate_dna_blocks`, `create_sequence`); нет блоков причёсок, UF, вида, группы крови в ДНК, копирования ДНК

### Нестабильность генома
Каждая активная мутация добавляет нестабильность; выше 100 — через время случайная катастрофа (превращение в желе, смерть, негативные мутации); стабилизаторы её снижают.

- **Код:** [`proc/update_instability()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/dna.dm#L299)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/genetics/mod.rs`: `update_instability` с сообщениями tg, `dna_melt` 60 с, затем распад: несмертельный — случайная негативная мутация и 30 токс., смертельный — 200 токс.; нет стабилизаторов и настоящих `instability_meltdown`

### Случайные мутации и перемешивание ДНК
Радиация, мутаген и токсины вызывают случайные мутации и перемешивание UI/UF/SE (`scramble_dna`).

- **Код:** [`proc/random_mutate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/dna.dm#L585), [`proc/easy_random_mutate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/dna.dm#L610), [`proc/scramble_dna()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/dna.dm#L646)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/genetics/mod.rs`, `crates/ss13_game/src/events/radiation_storm.rs`: `easy_random_mutate`, `random_mutate_unique_identity`, радиационный шторм мутирует незащищённых людей (90% негативная); нет мутагена, `scramble_dna`, UF, проверки радзащиты одежды

### Мутационные токсины видов форка (BandaStation)
Реагенты-мутагены и рецепты экстрактов слаймов, превращающие в вульпканина, таяра, скрелла.

- **Код:** [`/datum/reagent/mutationtoxin/vulpkanin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/reagents/mutation.dm#L1)
- **BandaStation:** [`modular_bandastation/species/code/reagents/mutation.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/reagents/mutation.dm#L1) — мутационные токсины видов форка
- **Статус в ss13-rs:** ❌ нет
