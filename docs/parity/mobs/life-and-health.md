# Жизненный цикл, урон и здоровье мобов
Тик `Life()`, типы урона, пороги крита, смерть, гиббинг и связанные с ними механики живых мобов.

### Подсистема SSmobs и тик Life()
Каждые 2 секунды подсистема мобов вызывает `Life()` каждого живого моба: дыхание, окружение, органы, кровь, метаболизм, статус-эффекты, обновление HUD. Все «периодические» эффекты мобов привязаны к этому тику (`seconds_per_tick`).

- **Код:** [`SUBSYSTEM_DEF(mobs)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/mobs.dm#L1), [`proc/Life()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/life.dm#L12), [`carbon/Life()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/life.dm#L1), [`human/Life()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/life.dm#L21)
- **BandaStation:** [`code/modules/mob/living/life.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/life.dm#L52) — вызов `handle_ssd()` (SSD-индикатор) в тике
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/breath.rs` (`MobsClock`, тик 2 с: дыхание, давление, температура тела, урон в крите), `crates/ss13_game/src/medical.rs` (`medical_life`: раны, органы, кровь, болезни, сброс `damageoverlaytemp`), `crates/ss13_game/src/hunger.rs`, `crates/ss13_game/src/chem.rs` (метаболизм); стазис (`InStasis`) пропускает обработку, как `TRAIT_STASIS`. Повышенная гравитация — отдельная запись ниже

### Типы урона и apply_damage
Урон делится на brute, burn, toxin, oxy, stamina и урон органов (brain и т.д.); `apply_damage` распределяет его по выбранной части тела с учётом брони, модификаторов вида (`brutemod`/`burnmod`) и блокировки. Здоровье = `maxHealth` минус сумма brute+burn+tox+oxy.

- **Код:** [`proc/apply_damage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/damage_procs.dm#L23), [`proc/apply_damages()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/damage_procs.dm#L166), [`proc/adjust_brute_loss()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/damage_procs.dm#L271), [`proc/adjust_fire_loss()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/damage_procs.dm#L400), [`proc/adjust_tox_loss()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/damage_procs.dm#L354), [`proc/adjust_oxy_loss()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/damage_procs.dm#L320), [`proc/adjust_organ_loss()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/damage_procs.dm#L421), [`carbon/updatehealth()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L426)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/health.rs`: brute/burn по шести частям тела, tox, oxy, `apply_damage`; модификаторы частей тела вида, `TRAIT_TOXINLOVER`/`TRAIT_TOXIMMUNE`, `TRAIT_NOSOFTCRIT` (`crates/ss13_game/src/species/mod.rs`); нет урона органов, брони в `apply_damage`, `required_bodytype`

### Лечение и полное исцеление
`heal_overall_damage`/`take_overall_damage` распределяют лечение и урон по всем частям тела; `fully_heal` (админ-исцеление, некоторые реагенты и эффекты) снимает урон, раны, болезни, восстанавливает органы и конечности по флагам `HEAL_*`.

- **Код:** [`proc/heal_overall_damage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/damage_procs.dm#L513), [`proc/take_overall_damage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/damage_procs.dm#L523), [`proc/fully_heal()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L976), [`carbon/fully_heal()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L673)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/health.rs`, `crates/ss13_game/src/medical.rs`: лечение brute/burn по частям тела; нет `fully_heal` и флагов `HEAL_*`

### Состояния: сознание, софт-крит, хард-крит, смерть
`update_stat` переводит моба между CONSCIOUS, SOFT_CRIT (здоровье ≤ 0: ползает, руки заблокированы), HARD_CRIT (≤ −30: без сознания) и DEAD (≤ −100). В крите моб получает oxy-урон и пропускает вдохи; черта `TRAIT_NOSOFTCRIT`/`TRAIT_NOHARDCRIT` меняет пороги.

- **Код:** [`carbon/update_stat()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L610), [`living/set_stat()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L2518), [`HEALTH_THRESHOLD_*`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__DEFINES/combat.dm#L88), [`proc/check_passout()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon_defense.dm#L523)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/health.rs`, `crates/ss13_game/src/breath.rs`: четыре состояния с порогами tg, ползание в софт-крите, пропуск вдохов и oxy-урон в крите, крит-урон без дыхания (`TRAIT_NOCRITDAMAGE`), потеря сознания от oxyloss, `TRAIT_NOSOFTCRIT` вида; нет `TRAIT_NOHARDCRIT`/`TRAIT_NODEATH` (их источники — мементо мори, пентрит, халк, сердце-аномалок — не перенесены)

### Отображение урона: оверлеи и HUD здоровья
Повреждённые части тела рисуются оверлеями крови/ожогов; HUD показывает куклу здоровья по частям тела, индикатор здоровья и экранные эффекты (красная виньетка боли, затемнение в крите, oxy-оверлей).

- **Код:** [`carbon/update_damage_overlays()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon_update_icons.dm#L283), [`carbon/update_damage_hud()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L480), [`human/update_health_hud()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/human.dm#L679)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/health.rs`, `crates/ss13_game/src/hud.rs`, `crates/ss13_game/src/vision.rs` (`damage_screens`): кукла здоровья по частям тела, экранные эффекты `update_damage_hud` (красная виньетка по brute+burn, `oxydamageoverlay`, `passage` и крит-зрение по порогам tg), вспышка урона `damageoverlaytemp` (brute/burn по части тела до следующего `Life()`, `crates/ss13_game/src/health.rs` `apply_damage_zone`); нет оверлеев повреждений на спрайте (`dam_mob.dmi`)

### Замедление от урона
Человек с большим уроном двигается медленнее (`damage_slowdown` от 40 потерянного здоровья), в софт-крите и лёжа ползает с отдельными модификаторами.

- **Код:** [`/datum/movespeed_modifier/damage_slowdown`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/movespeed/modifiers/mobs.dm#L26), [`/datum/movespeed_modifier/carbon_softcrit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/movespeed/modifiers/mobs.dm#L122), [`/datum/movespeed_modifier/carbon_crawling`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/movespeed/modifiers/mobs.dm#L126)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/health.rs` (`move_delay`)

### Смерть
`death()` фиксирует время смерти, роняет моба, отправляет сообщение и моодлеты окружающим, отключает HUD-элементы; тело остаётся, игрок может стать призраком. Смерть — точка входа для дефибрилляции и клонирования (см. ../medical/).

- **Код:** [`proc/death()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/death.dm#L224), [`carbon/death.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/death.dm#L1), [`human/death.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/death.dm#L1), [`proc/send_death_moodlets()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/death.dm#L185)
- **BandaStation:** [`code/modules/mob/living/death.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/death.dm#L269) — контекстное обновление `time_of_death`
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/health.rs`, `crates/ss13_game/src/ghost.rs`: стат DEAD, сообщение о смерти, тело остаётся, гостинг; моодлеты (`crates/ss13_game/src/mood.rs` `send_death_moodlets`); время смерти (`crates/ss13_game/src/death.rs` `TimeOfDeath`, стазис останавливает отсчёт, анализатор здоровья печатает «Время смерти» и «Субъект умер … назад»); оживление — дефибриллятор и хирургия (../medical/). BandaStation `persistent_client.time_of_death` (таймер респавна) не нужен: респавна нет

### Гиббинг и превращение в пыль
`gib()` разрывает моба на ошмётки, разбрасывая органы, конечности и вещи (флаги `DROP_*`); `inflate_gib` — с анимацией раздувания; `dust()` оставляет кучку пепла и, возможно, скелет.

- **Код:** [`proc/gib()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/death.dm#L11), [`proc/inflate_gib()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/death.dm#L39), [`proc/spread_bodyparts()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/death.dm#L86), [`proc/spawn_dust()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/death.dm#L165)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/health.rs` (`gib`/`gib_with`, флаги `DROP_*`, `inflate_gib`, `dust`/`dust_with`), `crates/ss13_game/src/medical/bodyparts.rs` (`spill_remains`), `crates/ss13_game/src/effects/gibs.rs`: гибы по типу тела и флагам (человек/без частей тела, ксеноморф, личинка, робот, прочее), анимация стоящего тела, моодлеты свидетелей, призрак без тела, вещи падают с шансом сдвига, органы и конечности по флагам, вылезают съеденные; `inflate_gib` с раздуванием, вызывается при смертельном избытке крови (`crates/ss13_game/src/medical/blood.rs`); `dust` с кучкой/скелетом по типу. Нет анимации распада (пыль мгновенная, тело исчезает сразу), робота `dust` (MMI); прочие источники `inflate_gib` (мятный экстракт, баллон) и `dust` — за своими механиками

### Хаск (husk)
Сильные ожоги (или высасывание генокрадом) превращают тело в хаска: обезличенное, неклонируемое без лечения ожогов/синтфлеша. Черта `TRAIT_UNHUSKABLE` защищает.

- **Код:** [`proc/become_husk()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/status_procs.dm#L577), [`proc/cure_husk()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/status_procs.dm#L570)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/death.rs` (`become_husk`, `cure_husk`, `BurnHusk`): мёртвый с ожогами больше `maxHealth` + `max_damage` груди становится хаском (`Husk`, вид хаска `HumanLook::husk`), кроме видов с `TRAIT_UNHUSKABLE`; хаск не дефибриллируется (`DEFIB_FAIL_HUSK`), не оживляется и не фильтруется хирургией, виден при осмотре и в анализаторе; `cure_husk(BURN)` снимает только ожоговый источник (хаск генокрада остаётся), админ-исцеление — все. Снятие синтплотью и резадоном — за `expose_mob` реагентов (../chemistry/reagent-families.md)

### Разложение трупа
Мёртвое тело без бальзамирования через время начинает гнить (`attach_rot`), выделяя миазмы и не поддаваясь дефибрилляции.

- **Код:** [`proc/attach_rot()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L1167)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/rot.rs` (`Rot`, `attach_rot`): органический или нежить-карбон после настоящей смерти (не фейк, не голограмма) гниёт: задержка 6 мин, нарастание 10 мин; формальдегид ≥ 15 / криостилан / ядро ≤ T0C-10 / хаск останавливают отсчёт (с сохранением отбытой задержки), оживление убирает; касание, удар, клик, захват (`Pulling`) и вход на тайл дают `prob(сила × нарастание)` на слабую случайную болезнь `Unknown` (воздух ≤ T0C-10 не заражает). В tg гниль не влияет на дефибрилляцию и не выделяет миазмов — описание пункта устарело. Нет реакции на удар летящим телом (bump), блокеры опрашиваются раз в секунду

### Поддаться смерти (succumb) и самоубийство
В крите игрок может поддаться (`succumb`) и умереть сразу; глагол `suicide` убивает моба способом, зависящим от предмета в руке (у предметов свои `suicide_act`), и запрещает клонирование/возрождение.

- **Код:** [`verb/succumb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L556), [`verb/suicide`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/verbs/suicide.dm#L4), [`human/perform_basic_suicide()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/human_suicide.dm#L7)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/death.rs`: в софт- и хард-крите уведомление «Сдаться» (`ALERT_SUCCUMB`), клик спрашивает последние слова (`tgui_input_text`, случайный заголовок), шепчет их соседним тайлам и убивает oxy-уроном до порога смерти; отмена ничего не делает, проверка крита повторяется после ответа. Нет глагола `suicide` и `suicide_act` предметов (нет механизма глаголов в протоколе), `TRAIT_SUICIDED`; глагол `suicide` должен ставить `crate::round::Suicided` (по нему тикер переоткрывает роли, [../round/ticker.md](../round/ticker.md))

### Электрошок
`electrocute_act` наносит burn-урон от тока с учётом `siemens_coeff` (перчатки, вид), оглушает и вызывает дрожь; источник — провода, шлюзы под током, ловушки, батоны с флагами.

- **Код:** [`proc/electrocute_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L630), [`proc/should_electrocute()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L1094)
- **BandaStation:** [`code/modules/mob/living/living_defense.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L648) — сообщение об ударе током
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wires.rs` (`electrocute_mob`): удар током от электрифицированных машин с burn-уроном, оглушением и защитой изолированных перчаток; нет других источников (провода, батоны), дрожи, `siemens_coeff` вида

### Взрывы по мобам
`ex_act` наносит мобу урон по степени взрыва с учётом брони (bomb armor), может оторвать конечности и гибнуть при эпицентре.

- **Код:** [`living/ex_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L620), [`human/ex_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/human_defense.dm#L257)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/explosions/ex_act.rs` (`ex_act_human`, `ex_act_basic`, `ex_act_silicon`, `soundbang_act`): люди — гиб при devastation и броне `bomb` < 50 (вещи получают взрыв следом), иначе brute/burn 300/60/30 по частям тела через броню каждой части, раны (порез/перелом/ожог) и отрыв конечностей с шансами tg, «потеря сознания» (паралич), нокдаун минус броня, урон ушам и глухота (наушники и вакуум защищают); простые мобы — гиб или 600/60/30; силиконы — 60/60 и 30 (devastation убивает без гиба). Не перенесено: отбрасывание тела при devastation (нет бросков мобов) и урон одежде (`damage_clothes`, нет прочности одежды)

### Падение с высоты (z-уровни)
Падая на нижний z-уровень, моб получает brute-урон за каждый уровень и сбивается с ног; некоторые черты и ботинки смягчают падение.

- **Код:** [`proc/ZImpactDamage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L81), [`carbon/on_fall()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L148)
- **BandaStation:** [`code/modules/mob/living/living.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L2790) — убраны проверки plating для перемещения по z
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/multiz.rs`: падение в открытый космос, `(5 × уровни)^1.5` brute (стоя — по ногам), нокдаун `уровни × 5` с, последняя ступенька лестницы ловит падение на один уровень; нет крыльев, free running и кошачьих приземлений (их черты не перенесены)

### Повышенная гравитация
При гравитации выше нормы моб получает brute-урон за тик и замедляется; при нулевой гравитации — плавает (см. movement.md).

- **Код:** [`proc/handle_gravity()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/life.dm#L125), [`proc/handle_high_gravity()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/life.dm#L135)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/gravity.rs` (`ForcedGravity`, `high_gravity`): выше 1 замедление на `гравитация - 1` децисекунд за тайл (`MoveSpeedModifier`) и уведомления высокой/сокрушительной гравитации, от порога 3 brute-урон `min(0.5 × (g - 3), 1.5)` в секунду (мёртвым тоже, кроме стазиса); источник — инверсный гравитум в крови (5) и компонент `ForcedGravity`. Нет генераторов гравитации и портативных генераторов (поля с тайловой гравитацией — их группа: инженерия), размытия `gravity_animate` и отрицательной гравитации

### Лёжа, отдых и сон
Моб может лечь (`toggle_resting`, клавиша), встать с задержкой, уснуть глаголом `Sleep`; лёжа он не плотный, ползает медленнее, поза влияет на попадания и спрайт.

- **Код:** [`proc/toggle_resting()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L669), [`proc/set_resting()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L674), [`proc/get_up()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L714), [`proc/set_body_position()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L2718), [`verb/mob_sleep`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L615)
- **BandaStation:** [`modular_bandastation/mobs/code/mobs_resting.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/mobs_resting.dm#L5) — спрайт отдыха для basic-мобов
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/rest.rs`: отдых по Shift+B и кнопке HUD (лечь сразу, встать за 1 с), лёжа не плотный, ползает, не может тащить (`crates/ss13_game/src/pull.rs`); `crates/ss13_game/src/health.rs`, `crates/ss13_game/src/status_effects.rs`: лежание от крита и нокдауна; нет сна, спрайтов отдыха basic-мобов, вставание после нокдауна мгновенное (в tg тоже 1 с)

### Сопротивление (resist)
Клавиша resist по приоритету снимает захват, пристёгивание, горение, наручники или выбирает из шкафа/контейнера.

- **Код:** [`proc/resist()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L1147), [`proc/execute_resist()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L1151), [`carbon/resist_buckle()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L152), [`carbon/resist_restraints()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L181)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/pull.rs`, `crates/ss13_game/src/restraints.rs`: вырывание из захвата и наручников; нет снятия с пристёгивания, тушения себя, выхода из шкафа

### Рвота
`vomit()` опустошает желудок, оставляет лужу рвоты, отнимает сытость и может нанести toxin-урон или выплюнуть кровь; вызывается реагентами, болезнями, перееданием.

- **Код:** [`proc/vomit()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L293)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/diseases.rs` (`vomit`): потеря сытости, оглушение, кровавая рвота брызгает кровью; вызывают болезни и статус-эффекты (`crates/ss13_game/src/status_effects.rs`); нет лужи рвоты (декаль с реагентами), моодлета, очищения желудка, рвоты от переедания

### Превращения моба (monkeyize, humanize и др.)
Набор процедур превращения: человек ↔ обезьяна (генетика, мутаген), в корги, краба, гориллу, слайма, оверминда блоба и в произвольный тип моба.

- **Код:** [`proc/monkeyize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/transform_procs.dm#L7), [`proc/humanize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/transform_procs.dm#L47), [`proc/corgize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/transform_procs.dm#L193), [`proc/gorillize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/transform_procs.dm#L210), [`proc/transform_into_mob()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/transform_procs.dm#L232)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/transform.rs`: общий API `crate::transform::{monkeyize, humanize, transform_into_mob, corgize, crabize, gorillize, slimeize, alienize, is_monkey, is_transforming}`; монки/человек с анимацией 2,2 с (`Transforming`, оглушение, скрытие, спрайт `h2monkey`/`monkey2h`) или мгновенно, снятие наручников; перенос разума в новый моб, вещи падают или удаляются. Смена вида роняет то, что новый вид не может носить (`worn_items_fit_body_check` в `crates/ss13_game/src/species/mod.rs`). Генетика (`crates/ss13_game/src/genetics/effects.rs`) вызывает этот API мгновенно (иначе ломается ожидание в тесте); нет `Robotize`, `AIize`, `become_overmind`, `Animalize`; остальные источники (jungle flu, шлем обезьяны, кости, ген. мелтдаун, лессерформ) — за своими механиками
