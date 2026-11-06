# Статус-эффекты: оглушение, выносливость, контроль
Каркас `/datum/status_effect` и эффекты, которые лишают моба движения или действий: стан, нокдаун, паралич, стамина и стамкрит, сон, конфузия, стаггер.

### Каркас статус-эффектов
Общий тип `/datum/status_effect`: длительность, интервал `tick()`, `on_apply`/`on_remove`, обновление при повторном наложении (`status_type`: unique/replace/refresh/multiple), иконка-алерт на экране и строка в осмотре. Обрабатывается подсистемой `SSprocessing`/`SSfastprocess`; хелперы `apply_status_effect`, `remove_status_effect`, `has_status_effect`, сгруппированные (`/datum/status_effect/grouped`), стековые (`/datum/status_effect/stacking`) и ограниченные эффекты.

- **Код:** [`/datum/status_effect`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/_status_effect.dm#L3), [`proc/process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/_status_effect.dm#L100), [`_status_effect_helpers.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/_status_effect_helpers.dm#L1), [`/datum/status_effect/grouped`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/grouped_effect.dm#L2), [`/datum/status_effect/stacking`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/stacking_effect.dm#L3)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/status_effects.rs`: один компонент `StatusEffects` с фиксированным набором таймеров (`Incap`, `Timed`, пульс раз в 2 с) вместо общего каркаса; строки осмотра дрожи и опьянения; нет произвольных эффектов, алертов эффектов, стековых и сгруппированных эффектов

### Стан (Stun)
Моб не может действовать и двигаться (`TRAIT_INCAPACITATED`, `TRAIT_IMMOBILIZED`, `TRAIT_HANDS_BLOCKED`), но стоит. `Stun()` не укорачивает оставшееся время, `SetStun`/`AdjustStun` задают/добавляют.

- **Код:** [`/datum/status_effect/incapacitating/stun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L55), [`proc/Stun()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/status_procs.dm#L42)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/status_effects.rs` (`stun`, `Incapacitation`)

### Нокдаун (Knockdown)
Моб падает (`TRAIT_FLOORED`) и ползает, но может действовать руками; опционально накладывает дейз.

- **Код:** [`/datum/status_effect/incapacitating/knockdown`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L69), [`proc/Knockdown()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/status_procs.dm#L93)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/status_effects.rs` (`knockdown`)

### Обездвиживание (Immobilize)
Моб не может сходить с места (`TRAIT_IMMOBILIZED`), но действует руками.

- **Код:** [`/datum/status_effect/incapacitating/immobilized`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L84), [`proc/Immobilize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/status_procs.dm#L148)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/status_effects.rs` (`immobilize`)

### Паралич (Paralyze)
Полное выведение из строя: моб лежит, не двигается и не действует.

- **Код:** [`/datum/status_effect/incapacitating/paralyzed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L100), [`proc/Paralyze()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/status_procs.dm#L199)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/status_effects.rs` (`paralyze`)

### Недееспособность (Incapacitated)
Отдельный эффект `incapacitate()`: моб не может действовать, но может двигаться (используется, например, некоторыми оружиями и способностями).

- **Код:** [`/datum/status_effect/incapacitating/incapacitated`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L116), [`proc/incapacitate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/status_procs.dm#L256)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/status_effects.rs` (`Incap::Incapacitated`): только `TRAIT_INCAPACITATED`

### Групповые хелперы обездвиживания и встряхивание
`AllImmobility`/`SetAllImmobility`/`AdjustAllImmobility` меняют сразу стан, нокдаун, паралич и обездвиживание; встряхивание лежащего (`help`-клик) укорачивает эффекты (`adjust_status_effects_on_shake_up`).

- **Код:** [`proc/AllImmobility()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/status_procs.dm#L309), [`proc/adjust_status_effects_on_shake_up()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/status_procs.dm#L435)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/status_effects.rs` (`all_immobility`, `set_all_immobility`, `adjust_all_immobility`, `shake_up`), `crates/ss13_game/src/combat.rs`: встряхивание лежащего и объятия сокращают стан/нокдаун/паралич/обездвиживание/бессознательность на 6 с и сон на 10 с; нет анимации встряхивания и вставания (`set_resting(FALSE)`)

### Иммунитет к стану и поглощение стана
`check_stun_immunity` (`TRAIT_STUNIMMUNE`, `CANSTUN`, godmode) и эффект `stun_absorption` (поглощает станы с сообщением, у генокрадов, адреналина и т.п.).

- **Код:** [`proc/check_stun_immunity()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/status_procs.dm#L10), [`/datum/status_effect/stun_absorption`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/stun_absorption.dm#L10)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/status_effects.rs` (`StunImmunity`, `add_stun_immunity`, `StunAbsorption`, `StunAbsorber`, `add_stun_absorption`, `stun_blocked`): `TRAIT_STUNIMMUNE` по источникам и поглощение по приоритету (сообщения окружающим и себе с `%EFFECT_OWNER`, лимит секунд с удалением или перезарядкой, длительность, строка осмотра, не в крите и без сознания) блокируют новые стан, нокдаун (и дейз), паралич, обездвиживание, `incapacitate`, бессознательность; сон — нет; источник — соли для ванн в крови (`crate::buffs`); снятие эффектов не блокируется (в tg иммунный не может и снять стан — баг не переносится). Чужое: godmode (отладочный, `ss13_debug`), `status_flags` `CANSTUN`/`CANKNOCKDOWN` (мобы-боссы), источники-антаги (Милость Его, кровавое опьянение, клинки еретика, клеймор горца, халк)

### Бессознательное состояние (Unconscious)
Моб без сознания: `IS_UNCONSCIOUS`, экран темнеет, не слышит речь; эффект `knocked_out` для потери сознания от урона.

- **Код:** [`/datum/status_effect/incapacitating/unconscious`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L134), [`/datum/status_effect/knocked_out`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L149), [`proc/Unconscious()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/status_procs.dm#L343)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/status_effects.rs` (`Incap::Unconscious`, `knocked_out`): `TRAIT_KNOCKEDOUT` — недееспособен, обездвижен, на полу, руки заняты, слепота (`vision.rs`), не говорит (`speech.rs`: чат видит hard crit), +3 стамины/с; источники — взрывы, ЭМИ боргов, эпилепсия, деконверсия революции/культа, руны, N2O/CO2, остановка сердца, кровопотеря, боевые искусства, болезни. Моб сохраняет свой `Stat` (tg `UNCONSCIOUS`): проверки через `Health::knocked_out` видят только крит, `status_effects::knocked_out` — оба; нет картинки `unconscious_appearance` и `see_no_names`

### Сон (Sleeping)
Моб спит: без сознания, во сне медленно лечится (кровать и подушка ускоряют), видит сны, просыпается от урона или встряхивания.

- **Код:** [`/datum/status_effect/incapacitating/sleeping`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L204), [`proc/Sleeping()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/status_procs.dm#L394)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/status_effects.rs` (`Incap::Sleeping`, `sleep_pulse`, `sleep_quality`): как бессознательность; раз в 2 с лечит мелкий урон по качеству сна (настроение, кровать/стол, простыня, подушка, глухота), стамина, отрезвление на 0,3%; будят встряхивание и объятия; болезни выздоравливают быстрее во сне (`diseases.rs`); источники — N2O, сонливость, CQC, бокс, болезни, опьянение. Нет снов, храпа, иммунитета ко сну, бонуса темноты/повязки, лечения органов и опыта атлетики

### Урон по выносливости (стамина)
`staminaloss` до `max_stamina` (120); через `stamina_regen_time` (10 с) после последнего урона сбрасывается в 0 целиком.

- **Код:** [`proc/adjust_stamina_loss()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/damage_procs.dm#L440), [`proc/set_stamina_loss()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/damage_procs.dm#L452), [`proc/update_stamina()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L1444)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/status_effects.rs`: сброс и предел есть; нет множителей физиологии/брони, `required_biotype`, HUD-индикатора стамины, урона стамине от реагентов

### Стамкрит
При 100 стамины моб падает, не может двигаться и действовать, пока стамина не опустится ниже порога; повторные удары дают убывающий эффект.

- **Код:** [`/datum/status_effect/incapacitating/stamcrit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/stamcrit.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/status_effects.rs` (`stamcrit`)

### Конфузия
Шаги сбиваются в случайную сторону, вероятность растёт со временем эффекта.

- **Код:** [`/datum/status_effect/confusion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/confusion.dm#L9)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/status_effects.rs` (`confusion`)

### Дейз, стаггер и запрет бокового удара
Дейз — уязвимость к пинку лежащего (паралич); стаггер — замедление движения и шанс выронить оружие при толчке; `no_side_kick` — защита от повторного бокового пинка.

- **Код:** [`/datum/status_effect/staggered`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/staggered.dm#L5), [`/datum/status_effect/dazed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/staggered.dm#L51), [`/datum/status_effect/no_side_kick`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/staggered.dm#L122)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/status_effects.rs` (`dazed`, `staggered`, `no_side_kick`), `crates/ss13_game/src/security.rs`

### Стазис
Сгруппированный эффект стазиса (стазис-кровать, стазис-мешок): останавливает `Life()`, метаболизм и прогресс болезней.

- **Код:** [`/datum/status_effect/grouped/stasis`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L344)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/medical/stasis.rs` (`InStasis`): обездвиживание и блок рук (`status_effects.rs`), нет дыхания и давления (`breath.rs`), ран, органов, крови и болезней (`medical.rs`), метаболизма (`chem.rs`), голода, SSD-сна, эффектов радиации, боли при операциях; источник — стазис-кровать (medical-machines.md), стазис-мешок — предметы

### Заморозка (freon, ледяной блок)
Моб вморожен в лёд: не двигается, оттаивает со временем или от тепла; также `IsFrozen()` и защита гиперноблием.

- **Код:** [`/datum/status_effect/freon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/gas.dm#L1), [`/datum/status_effect/hypernob_protection`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/gas.dm#L64), [`proc/IsFrozen()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/status_procs.dm#L446)
- **Статус в ss13-rs:** ❌ нет
