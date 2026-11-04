# Взрывы и ЭМИ

Механика взрывов (`SSexplosions`), реакция атомов на взрыв, ЭМИ-импульсы и связанные эффекты. Источники взрывов (бомбы, баллоны с газом, химия, суперматерия) описаны в своих разделах.

### Взрыв: explosion() и SSexplosions
Взрыв задаётся тремя радиусами (devastation / heavy / light) плюс радиусы огня и вспышки; волна распространяется от эпицентра с учётом `explosion_block` стен и дверей, может быть направленной (`explosion_direction`, `explosion_arc`). Атомы обрабатываются подсистемой пакетами по тяжести: сначала турфы, потом объекты, потом мобы.

- **Код:** [`proc/explosion()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/explosions.dm#L213), [`proc/explode()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/explosions.dm#L236), [`proc/propagate_blastwave()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/explosions.dm#L300), [`SUBSYSTEM_DEF(explosions)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/explosions.dm#L8)
- **Статус в ss13-rs:** 🟡 частично — `// shortcut:` для метеоров `crates/ss13_game/src/events/explosion.rs` `explosion()`: степени по `CHEAP_HYPOTENUSE` от эпицентра (devastation/heavy/light), горячие точки в `flame_range` с шансом 40%; нет блокировки (`explosive_resistance`), бросков, вспышек, звука, SSexplosions и других источников (баллоны, бомбы)

### Динамический взрыв dyn_explosion()
Взрыв по «мощности»: радиусы вычисляются из одного числа (используется химией, баллонами, бомбами с переменной силой); по умолчанию игнорирует лимит.

- **Код:** [`proc/dyn_explosion()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/explosions.dm#L185)
- **Статус в ss13-rs:** ❌ нет — нет взрывов

### Предел мощности бомб (bombcap)
Конфиг `bombcap` (по умолчанию 14) ограничивает радиусы взрыва: devastation = cap/4, heavy = cap/2, light, flash и flame = cap; `ignorecap` снимает лимит.

- **Код:** [`/datum/config_entry/number/bombcap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/configuration/entries/game_options.dm#L379)
- **Статус в ss13-rs:** ❌ нет — нет взрывов

### Реакция атомов на взрыв: ex_act()
Каждый атом получает уровень `EXPLODE_DEVASTATE/HEAVY/LIGHT`: турфы рушатся до основания или плитки, объекты получают урон с учётом брони `bomb`, мобы — урон, оглушение, глухоту, отрыв конечностей; содержимое контейнеров получает ослабленную волну.

- **Код:** [`/atom/proc/ex_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atom/atom_act.dm#L40)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/explosion.rs`: стены (`ScrapeAway`/`dismantle_wall`, `hardness` 40/10), полы (снятие 1–2 слоёв, листы железа), структуры с прочностью (`take_damage` INFINITY / 100–250 / 10–90), люди (см. ../mobs/); нет `ex_act` предметов, машин без прочности, решёток-лестниц (lattice)

### Вспышка и огонь взрыва
В радиусе `flash_range` мобов ослепляет вспышка, в радиусе `flame_range` ставятся горячие точки (`hotspot_expose`), поджигающие газ.

- **Код:** [`propagate_blastwave()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/explosions.dm#L300)
- **Статус в ss13-rs:** ❌ нет — горячие точки есть (`crates/ss13_atmos/src/level/hotspot.rs`), но взрывы их не ставят

### Тряска экрана и звук взрыва
Все игроки на уровне слышат взрыв в зависимости от расстояния (близкий, дальний, эхо, скрип корпуса) и получают тряску камеры.

- **Код:** [`proc/shake_the_room()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/explosions.dm#L522)
- **Статус в ss13-rs:** ❌ нет — нет взрывов и звука

### Частицы и дым взрыва
Эффект-системы `explosion` и `explosion/smoke`: облако частиц и дыма в эпицентре.

- **Код:** [`/datum/effect_system/explosion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_explosion.dm#L45), [`/datum/effect_system/explosion/smoke`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_explosion.dm#L52), [`/obj/effect/particle_effect/expl_particles`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_explosion.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет взрывов и эффект-систем

### Взрывы реагентов
`/datum/effect_system/reagents_explosion`: взрыв от химической реакции (чёрный порох, нитроглицерин и др.) с силой из объёма реагентов.

- **Код:** [`/datum/effect_system/reagents_explosion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_reagents_explosion.dm#L5)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_reagents/src/holder.rs`: «no explosions or other effects of reactions' own procs»

### Взрывоопасные предметы (explodable) и защита от взрывов
Компонент `explodable` взрывает предмет при ударе, нагреве, броске или поджоге; элемент `blocks_explosives` даёт объекту `explosion_block` (взрывозащитные двери, усиленные стены).

- **Код:** [`/datum/component/explodable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/explodable.dm#L2), [`/datum/element/blocks_explosives`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/blocks_explosives.dm#L3)
- **Статус в ss13-rs:** ❌ нет — нет взрывов

### Электромагнитный импульс (empulse / emp_act)
ЭМИ с тяжёлым и лёгким радиусом: электроника ломается или сбоит (батареи разряжаются, рации глохнут, импланты и протезы барахлят, двери искрят), у киборгов и ИИ — оглушение.

- **Код:** [`proc/empulse()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/empulse.dm#L1), [`/atom/proc/emp_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atom/atom_act.dm#L69)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/emp/mod.rs`: `Empulse` (импульс от тайла) и `EmpHit` (один атом) → `EmpAct` каждому атому в радиусе (Чебышёв: ближе `heavy` — `EMP_HEAVY`, ровно `heavy` — 50/50, до `light` — `EMP_LIGHT`), визуал `emp/pulse` при `heavy > 1` и искры `empdisable` у машин и ботов, защита `EMP_PROTECT_SELF/CONTENTS/WIRES` (компонент `EmpProtection` и типы с `empprotection` в `Initialize`), пульс проводов (`wires::emp_pulse`), содержимое: надетое и в руках, органы (тяжесть + `emp_protection` одежды — шапочка из фольги и т. п.), рюкзаки, шкафы; моб в шкафу не задет; `Emped` — машинный `EMPED`; обработчики семейств в `emp/power.rs`, `emp/machines.rs`, `emp/mobs.rs`; источник для отладки — тест-хук `{"op":"emp"}` (`ss13_debug`). Нет: голограмм, тегов осмотра защищённых, админского лога источника, админ-верба «EM Pulse» (нужен новый `AdminRequest`)
