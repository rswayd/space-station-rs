# Семена и генетика растений
Семена хранят статы и гены растения; гены переносятся прививками, опылением и мутациями.

### Пакет семян и статы растения
Семена (`/obj/item/seeds`) несут вид, продукт и статы: срок жизни (`lifespan`), выносливость (`endurance`), созревание (`maturation`), производство (`production`), урожай (`yield`), потенцию (`potency`), нестабильность (`instability`), скорость и шанс роста сорняков. Осмотр показывает статы; семена можно переименовать ручкой.

- **Код:** [`/obj/item/seeds`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seeds.dm#L5), [`examine()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seeds.dm#L115), [`adjust_potency()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seeds.dm#L355), [`nameformat()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seeds.dm#L463)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/botany/seeds.rs`, `crates/ss13_mapload/src/plants.rs`: все типы семян из DM со статами (долголетие, созревание, продуктивность, урожай, потенция, выносливость, нестабильность, сорняки), спрайтами роста, генами; пакеты сажаются и выдаются экстрактором

### Мутация статов и нестабильность
`mutate()` случайно сдвигает статы в заданных пределах; высокая нестабильность (>30/60/90) открывает случайную мутацию в родственный вид и приобретение случайных трейтов. Соматолуч по пакету семян тоже меняет статы.

- **Код:** [`/obj/item/seeds/proc/mutate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seeds.dm#L157), [`bullet_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seeds.dm#L174), [`hydroponics/traitmutate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L752)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/botany/seeds.rs`, `crates/ss13_game/src/botany/tray.rs`: мутация статов, трейтов и вида (`mutatelist`) по нестабильности, границы статов как в tg

### Реагентные гены и выход продукта
Гены `/datum/plant_gene/reagent` задают реагенты плода как процент от объёма при данной потенции (`prepare_result`); чистота реагентов зависит от срока жизни и выносливости (`get_reagent_purity`).

- **Код:** [`/datum/plant_gene/reagent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/plant_genes.dm#L55), [`prepare_result()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seeds.dm#L275), [`get_reagent_purity()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seeds.dm#L300), [`reagent/preset`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/unique_plant_genes.dm#L750)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/botany/genes.rs`, `crates/ss13_game/src/botany/plants.rs`: реагенты `reagents_add` по потенции в каждом плоде, повышение генов при опылении, `getYield()`

### Общие трейты растений
Прививаемые трейты (`/datum/plant_gene/trait`): раздавливание при броске/подскальзывании, скользкость, свечение (цвета, теневое), телепортация (блюспейс), батарейка/заряд батарей, жалящие шипы, дым при раздавливании, огнестойкость, инвазивность (распространение на соседние лотки), пивоварение/соковыжимание, смех, глаза, липкость, нагрев/охлаждение реагентов, многократный сбор, максимум химии, стабильность статов, консервация, токсоустойчивость, плотоядность, полуводные и любящие почву растения.

- **Код:** [`/datum/plant_gene/trait`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/plant_genes.dm#L120), [`trait/slip`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/plant_genes.dm#L278), [`trait/glow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/plant_genes.dm#L389), [`trait/teleport`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/plant_genes.dm#L469), [`trait/battery`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/plant_genes.dm#L572), [`trait/invasive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/plant_genes.dm#L740), [`trait/repeated_harvest`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/plant_genes.dm#L556)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/genes.rs`: все `/datum/plant_gene/trait` переносятся, копируются, мутируют и видны в анализаторе; действуют только читаемые лотком, урожаем и статами (уплотнённые химикаты и т. п.)

### Трейт раздавливания (squash)
Плод с трейтом лопается при броске или подскальзывании: пятно на полу, реагенты разливаются по тайлу и всему на нём.

- **Код:** [`/datum/plant_gene/trait/squash`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/plant_genes.dm#L211)
- **BandaStation:** [`plant_genes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/plant_genes.dm#L266) — плод можно раздавить в руках (use in hand, `do_after` `SQUASH_WITH_HANDS_DELAY`)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/botany/genes.rs`: `// shortcut:` трейтов продукта (раздавливание, скольжение, свечение, телепорт, батареи) нет

### Типы растений (plant_type)
Взаимоисключающие типы: сорнякоустойчивое (не страдает без удобрений), грибной метаболизм (растёт в темноте, быстрее в богатой почве), токсинная адаптация, инопланетные свойства.

- **Код:** [`/datum/plant_gene/trait/plant_type`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/plant_genes.dm#L1050), [`fungal_metabolism`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/plant_genes.dm#L1062), [`weed_hardy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/plant_genes.dm#L1056)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/genes.rs` (`WEED_HARDY`, `TOXIN_ADAPTATION`): типы влияют на сорняки и токсины в лотке; грибы и инопланетные растения без особого поведения

### Уникальные трейты
Непрививаемые трейты конкретных растений: удар растением (новацвет поджигает, крапива жжёт, подсолнух), «обратный огонь» при взятии в руку (шипы розы, жар чили, крапива, блюспейс-телепорт), превращение в моба (томат-убийца, ходячий гриб), бомба (вишнёвая бомба по потенции), выделение газа, сложный сбор, производство опиума, антимагия святой дыни, изменённый объём.

- **Код:** [`trait/attack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/unique_plant_genes.dm#L40), [`trait/backfire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/unique_plant_genes.dm#L156), [`trait/mob_transformation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/unique_plant_genes.dm#L319), [`trait/bomb_plant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/unique_plant_genes.dm#L508), [`trait/gas_production`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/unique_plant_genes.dm#L616), [`trait/anti_magic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/unique_plant_genes.dm#L6)
- **BandaStation:** [`unique_plant_genes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/unique_plant_genes.dm#L18) — заряды антимагии `potency / 50` вместо `/ 20`, при потенции ниже 50 щита нет
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/botany/genes.rs`: `// shortcut:` эффектов `on_new_plant` и трейтов продукта нет, только переносятся

### Прививки (grafting)
Секатор снимает с готового растения прививку (`/obj/item/graft`) с одним трейтом и частью статов; прививка, применённая к другому растению в лотке, добавляет трейт и усредняет статы. Заменяет удалённый из tg «генный модификатор» (DNA manipulator).

- **Код:** [`/obj/item/graft`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grafts.dm#L4), [`create_graft()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seeds.dm#L536), [`apply_graft()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seeds.dm#L551); tgui: [`PlantAnalyzer/Graft.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PlantAnalyzer/Graft.tsx)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/botany/mod.rs`: `// shortcut:` прививок нет

### Странные семена (random)
Пакет со случайным видом-заглушкой: при посадке получает случайные статы, реагенты, трейты и тип растения. Делается в биогенераторе (форк) и находится в техах/луте.

- **Код:** [`/obj/item/seeds/random`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/random.dm#L3), [`randomize_stats()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seeds.dm#L482), [`add_random_traits()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seeds.dm#L504)
- **Статус в ss13-rs:** ❌ нет

### Реакция семян на химию
Капание реагентов на сами семена (например, кудзу, реплика-под) меняет их особые свойства через `on_chem_reaction`.

- **Код:** [`on_chem_reaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seeds.dm#L460), [`kudzu/on_chem_reaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/grown/weeds/kudzu.dm#L60)
- **Статус в ss13-rs:** ❌ нет
