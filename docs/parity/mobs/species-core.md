# Виды (species): общая механика
Как устроен `/datum/species`: что вид меняет в человеке, как выбирается и меняется. Отдельные виды — в [species-list.md](species-list.md).

### Датум вида
Вид задаёт врождённые черты (`inherent_traits`), биотипы, замену органов (`mutantlungs`, `mutantheart`, …), части тела (`bodypart_overrides`), модификаторы урона и температуры (`brutemod`, `burnmod`, `coldmod`, `heatmod`, `siemens_coeff`), группу крови, языки, `payday_modifier`, звуки и спрайты.

- **Код:** [`/datum/species`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L18), [`proc/on_species_gain()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L364), [`proc/on_species_loss()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L446), [`proc/regenerate_organs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L291), [`proc/replace_body()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L1920)
- **BandaStation:** [`code/modules/mob/living/carbon/human/_species.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L2010) — поддержка видовых аксессуаров форка; архитектура новых видов в `modular_bandastation/species`
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/species/mod.rs`: `Species` на мобе и статическая таблица всех видов (`SpeciesDef`: врождённые черты, биотипы, набор органов `mutant*`, спрайты частей тела, `brute_modifier`/`burn_modifier` частей, `coldmod`/`heatmod`, `siemens_coeff`, пределы и норма температуры, экзотическая кровь, `no_equip_flags`, `payday_modifier`, `outfit_important_for_life`); черты читают `health.rs`, `breath.rs`, `hunger.rs`, `medical/blood.rs`, `medical/organs.rs`, `wires.rs`, `inventory.rs`, `genetics/`. Языки, звуки и аксессуары — в своих записях; `payday_modifier` применяет зарплата (cargo)

### Смена вида
`set_species` меняет вид живого моба (генетика, мутаген слаймов, админ, магия, зомби-инфекция): заменяет органы и части тела, черты, внешний вид.

- **Код:** [`carbon/set_species()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/dna.dm#L387), [`human/set_species()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/dna.dm#L419)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/species/mod.rs` `set_species`: органы заменяются набором нового вида (`regenerate_organs(replace_current)`), модификаторы урона, кровь, внешность, сброс голода при `TRAIT_NOHUNGER`, снятие мутаций при `TRAIT_GENELESS`; при спавне персонажа и при превращении генокрада. Источники смены (мутаген слаймов, админ, магия, зомби) — в своих группах

### Виды на старте раунда и выбор в настройках
Конфиг `ROUNDSTART_RACES` определяет, какие виды можно выбрать; в меню персонажа у вида есть описание, лор и список перков (плюсы/минусы).

- **Код:** [`proc/generate_selectable_species_and_languages()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L210), [`proc/check_roundstart_eligible()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L233), [`proc/create_pref_unique_perks()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L1563), [`proc/get_species_description()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L1462), [`config/game_options.txt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/config/game_options.txt#L416)
- **BandaStation:** [`config/bandastation/bandastation_config.txt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/config/bandastation/bandastation_config.txt#L32) — вульпканины и таяры включены конфигом форка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/human.rs` (`roundstart_species`), `crates/ss13_game/src/round.rs`, `crates/ss13_game/src/lobby_client.rs`: вид выбирается в настройках персонажа из видов `ROUNDSTART_RACES` конфига BandaStation (значения по умолчанию, конфиг не читается); нет описаний, лора и перков вида

### Видовые особенности внешности
Аксессуары (`sprite_accessory`): причёски, бороды, хвосты, уши, рога, крылья, морды, маркировки, антенны, грибная шляпка и т.д.; цвета мутантов (`TRAIT_MUTANT_COLORS`), тон кожи.

- **Код:** [`/datum/sprite_accessory`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/sprite_accessories/_sprite_accessory.dm#L19), [`code/datums/sprite_accessories/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/sprite_accessories)
- **BandaStation:** [`modular_bandastation/species/code/human/sprite_accessories/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/sprite_accessories) — причёски форка, аксессуары скреллов, таяр, вульпканинов
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/human.rs`: тон кожи, причёски с цветом, части тела вида (иконка, `limb_id`, диморфизм, цифровые ноги, глаза вида), мутантный цвет из палитры (`TRAIT_MUTANT_COLORS`) и фиксированный (`fixed_mut_color`); нет видовых аксессуаров (хвосты, уши, рога, морды, маркировки) и свободного выбора цвета

_«Видовые атаки без оружия» описано в [../combat/melee-attacks.md](../combat/melee-attacks.md)._

### Видовые языки (BandaStation)
Модпак форка добавляет языки видов: канилунц (вульпканины), сиик'тайр (таяры), курволиус (скреллы), спинвардер.

- **Код:** [`modular_bandastation/species/code/language/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/language)
- **BandaStation:** [`modular_bandastation/species/code/language/canilunzt.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/language/canilunzt.dm#L1) — языки форка
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_chat/src/language/defs.rs` (канилунц `,7`, сик'таир `,8`, курволиус `,9`, Неорусский `,s` — имена и описания форка), пресеты `/datum/language_holder/{vulpkanin,tajaran,skrell}` и родные языки их органов-языков (`ss13_chat::language::Tongue`), выдача видом — `crates/ss13_game/src/species/mod.rs` (`update_languages`)
