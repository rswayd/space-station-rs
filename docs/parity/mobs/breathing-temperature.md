# Дыхание, давление и температура тела
Как карбоны дышат, чем травятся и как переносят давление и температуру окружения. Сами органы (лёгкие, сердце) — см. ../medical/.

### Цикл дыхания
Карбон делает вдох раз в несколько тиков (чаще после неудачного вдоха): берёт `BREATH_VOLUME` воздуха из баллона или тайла, передаёт его лёгким и возвращает выдох в атмосферу. В крите вдохи пропускаются (`losebreath`); черта `TRAIT_NOBREATH` отключает дыхание.

- **Код:** [`carbon/handle_breathing()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/life.dm#L45), [`proc/breathe()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/life.dm#L68), [`proc/check_breath()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/life.dm#L145)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/breath.rs`

### Кислород и удушье
Лёгкие требуют парциального давления O2 не ниже 16 кПа; иначе — oxy-урон, оповещение «нехватка кислорода» и пропуск вдохов; хороший вдох лечит oxyloss.

- **Код:** [`proc/check_breath()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L639), [`proc/breathe_oxygen()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L243), [`proc/handle_suffocation()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L768)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/breath.rs`

### Вредные газы: CO2, плазма, N2O
CO2 выше 10 кПа душит и в итоге вырубает, плазма выше 0.05 кПа даёт toxin-урон (плазмамены, наоборот, дышат ею), N2O усыпляет и вызывает эйфорию.

- **Код:** [`proc/too_much_co2()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L316), [`proc/too_much_plasma()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L370), [`proc/breathe_nitro()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L294), [`proc/add_gas_reaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L197)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/breath.rs`: пороги и урон CO2/плазмы, оповещение N2O; нет сна, эйфории и смеха от N2O, дыхания плазмой у плазменов

### Экзотические газы
Реакции лёгких на BZ (галлюцинации, урон мозгу), фреон, галон, хилиум (сон и лечение), гелий (голос), гиперноблий, миазмы (болезни), нитриум, тритий (радиация), антиноблий, заукер (урон), плюоксиум.

- **Код:** [`proc/too_much_bz()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L391), [`proc/too_much_freon()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L398), [`proc/consume_healium()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L425), [`proc/too_much_miasma()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L473), [`proc/too_much_nitrium()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L559), [`proc/too_much_tritium()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L579), [`proc/too_much_zauker()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L616)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/breath.rs` явно пропускает эти газы (shortcut)

### Температура вдоха
Слишком горячий или холодный вдох обжигает лёгкие и горло (burn-урон, урон органа), пар изо рта на холоде.

- **Код:** [`proc/handle_breath_temperature()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L794), [`proc/emit_breath_particle()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L847)
- **Статус в ss13-rs:** ❌ нет

### Баллоны и internals
Маска с `MASKINTERNALS` или шлем с `HEADINTERNALS` позволяют дышать из баллона (alt-клик по баллону, кнопка HUD); баллон закрывается, если его сняли или сняли маску. Есть и внешние internals (подключение к чужому баллону).

- **Код:** [`proc/toggle_internals()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/inventory.dm#L343), [`proc/open_internals()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/inventory.dm#L240), [`proc/close_internals()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/inventory.dm#L272), [`proc/invalid_internals()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/inventory.dm#L228), [`proc/get_breath_from_internal()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/life.dm#L436)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/internals.rs`, `crates/ss13_game/src/breath.rs`: alt-клик открывает/закрывает, дыхание из баллона, закрытие при снятии; нет внешних internals, опускания маски (`up`)

### Защита от дыма
Противогаз и подобное (`BLOCK_GAS_SMOKE_EFFECT`) защищают от вдыхания реагентов из дыма.

- **Код:** [`proc/has_smoke_protection()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/life.dm#L130)
- **Статус в ss13-rs:** ❌ нет

### Давление окружения
Ниже 20 кПа и выше 550 кПа моб получает brute-урон и видит оповещение; одежда со `STOPSPRESSUREDAMAGE` на голове и теле (скафандры) защищает.

- **Код:** [`proc/handle_environment_pressure()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L1299)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/breath.rs`

### Температура тела
Температура кожи и ядра стремится к окружающей с учётом теплоизоляции одежды (`get_thermal_protection`, `max_heat_protection_temperature`/`min_cold_protection_temperature`) и к нормальной за счёт естественной стабилизации; скафандры регулируют её.

- **Код:** [`proc/natural_bodytemperature_stabilization()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/life.dm#L573), [`proc/handle_body_temperature()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L1046), [`proc/get_thermal_protection()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/life.dm#L149), [`proc/get_heat_protection_flags()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/life.dm#L163), [`proc/adjust_coretemperature()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/life.dm#L122)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/breath.rs`

### Урон и эффекты от жары и холода
Вне безопасного диапазона — burn-урон, оповещения жары/холода, замедление от холода (`cold slowdown`), термальные раны и потливость; модификаторы вида `coldmod`/`heatmod`, черты `TRAIT_RESISTCOLD`/`TRAIT_RESISTHEAT`.

- **Код:** [`proc/body_temperature_damage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L1196), [`proc/body_temperature_alerts()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L1143), [`/datum/movespeed_modifier/cold`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/movespeed/modifiers/mobs.dm#L75)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/breath.rs`: урон и оповещения, пределы и норма температуры вида, `coldmod`/`heatmod`, `TRAIT_RESISTCOLD`/`TRAIT_RESISTHEAT`, холоднокровие (`crates/ss13_game/src/species/mod.rs`); нет замедления от холода, термальных ран

### Обмен теплом между мобами
Объятия и тесный контакт передают тепло (`share_bodytemperature`) — греет замёрзших.

- **Код:** [`proc/share_bodytemperature()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/life.dm#L649)
- **Статус в ss13-rs:** ❌ нет

Горение моба (огненные стаки, `/datum/status_effect/fire_handler/fire_stacks`, в Rust `crates/ss13_game/src/fire.rs`) описано среди статус-эффектов этой папки.
