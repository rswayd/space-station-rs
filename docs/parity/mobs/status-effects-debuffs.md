# Статус-эффекты: дебаффы
Негативные эффекты, кроме оглушающих (см. [status-effects-incapacitating.md](status-effects-incapacitating.md)): дрожь, опьянение, речь, зрение, горение, приступы и магические проклятия.

### Дрожь (jitter)
Спрайт моба трясётся; от стимуляторов, электрошока, ломки.

- **Код:** [`/datum/status_effect/jitter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/jitteriness.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Головокружение (dizziness)
Экран клиента раскачивается, сила зависит от остатка эффекта.

- **Код:** [`/datum/status_effect/dizziness`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/dizziness.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Сонливость (drowsiness)
Размытие зрения и шанс заснуть на ходу.

- **Код:** [`/datum/status_effect/drowsiness`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/drowsiness.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Опьянение (inebriated)
`drunk_value` растёт от алкоголя и убывает со временем; стадии «навеселе» и «пьян»: невнятная речь, конфузия, головокружение, размытие, рвота, отравление и отключка на высоких значениях.

- **Код:** [`/datum/status_effect/inebriated`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/drunk.dm#L8), [`/datum/status_effect/inebriated/drunk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/drunk.dm#L99), [`proc/adjust_drunk_effect()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/status_procs.dm#L768)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_reagents/src/metabolism.rs` (`inebriation_tick`): только значение, распад и токсин-урон; нет невнятной речи, конфузии, головокружения, размытия, рвоты, отключек

### Речевые дебаффы (заикание, невнятная речь)
Эффекты `speech/*` искажают сообщения: заикание (тревога, одержимость, «дерп»), невнятная речь (пьяная, культистская, еретическая).

- **Код:** [`/datum/status_effect/speech`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/speech_debuffs.dm#L1), [`/datum/status_effect/speech/stutter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/speech_debuffs.dm#L62), [`/datum/status_effect/speech/slurring`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/speech_debuffs.dm#L211)
- **BandaStation:** [`speech_debuffs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/speech_debuffs.dm#L81) — заикание для кириллицы (социальная тревожность), санитизация эмоутов
- **Статус в ss13-rs:** ❌ нет

### Немота (silenced)
Временный запрет речи.

- **Код:** [`/datum/status_effect/silenced`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/silenced.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Размытие зрения и наркотические искажения
`eye_blur` размывает экран; `drugginess` — радужный фильтр; `static_vision` — помехи; ложный HUD здоровья (`screwy_hud`: «мёртв», «крит», «здоров»).

- **Код:** [`/datum/status_effect/eye_blur`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/screen_blur.dm#L6), [`/datum/status_effect/drugginess`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/drugginess.dm#L2), [`/datum/status_effect/static_vision`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/static_vision.dm#L1), [`/datum/status_effect/grouped/screwy_hud`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/screwy_hud.dm#L10)
- **Статус в ss13-rs:** ❌ нет

### Пацифизм
Запрещает причинять вред (`TRAIT_PACIFISM`); временный от реагентов и гонболы.

- **Код:** [`/datum/status_effect/pacify`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/pacifism.dm#L1), [`/datum/status_effect/gonbola_pacify`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L667)
- **Статус в ss13-rs:** ❌ нет

### Горение и мокрота (fire/wet stacks)
`fire_stacks` поджигают моба, распадаются, нагревают тело и тайл, требуют кислорода; `wet_stacks` гасят огонь и повышают проводимость.

- **Код:** [`/datum/status_effect/fire_handler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/fire_stacks.dm#L1), [`/datum/status_effect/fire_handler/fire_stacks`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/fire_stacks.dm#L127), [`/datum/status_effect/fire_handler/wet_stacks`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/fire_stacks.dm#L306)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/fire.rs` (`FireStacks`): горение есть; нет `wet_stacks`, горения одежды

### Сердечный приступ
Остановка сердца по стадиям (от болезни, реагентов): боль, оксиурон, смерть без дефибрилляции; `heart_desperation` — рывок при умирании.

- **Код:** [`/datum/status_effect/heart_attack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/heart_attack.dm#L12), [`/datum/status_effect/heart_desperation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/heart_attack.dm#L163)
- **Статус в ss13-rs:** ❌ нет

### Удушье предметом
Застрявший в горле предмет: нельзя дышать, нужен приём Геймлиха.

- **Код:** [`/datum/status_effect/choke`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/choke.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Судороги, спазмы, припадки
`spasms` (случайные движения и удары), `convulsing` (роняет предметы), `seizure` (от передоза).

- **Код:** [`/datum/status_effect/spasms`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L744), [`/datum/status_effect/convulsing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L797), [`/datum/status_effect/seizure`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/drug_effects.dm#L35)
- **Статус в ss13-rs:** ❌ нет

### Эффекты наркотиков
`woozy`, `high_blood_pressure`, `stoned` (каннабис: голод, замедление), разбавление физраствором (`saline_glucose_dilution`).

- **Код:** [`/datum/status_effect/woozy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/drug_effects.dm#L1), [`/datum/status_effect/high_blood_pressure`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/drug_effects.dm#L16), [`/datum/status_effect/stoned`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/drug_effects.dm#L58), [`/datum/status_effect/stacking/saline_glucose_dilution`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/drug_effects.dm#L94)
- **Статус в ss13-rs:** ❌ нет

### Ужас (terrified)
Нарастающий страх в темноте от теневых существ: дрожь, замедление, потеря сознания при максимуме.

- **Код:** [`/datum/status_effect/terrified`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/terrified.dm#L6)
- **Статус в ss13-rs:** ❌ нет

### Токсичная рвота и температура со временем
`tox_vomit` (рвота от токсинов), `temperature_over_time` (постепенный нагрев/охлаждение тела, перегрев чипа).

- **Код:** [`/datum/status_effect/tox_vomit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/tox_vomit.dm#L2), [`/datum/status_effect/temperature_over_time`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/temperature_over_time.dm#L10)
- **Статус в ss13-rs:** ❌ нет

### Гравитационная болезнь космонавта
Квирк «Космонавт»: недомогание в гравитации и бонусы в невесомости.

- **Код:** [`/datum/status_effect/spacer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/spacer.dm#L3), [`/datum/status_effect/spacer/gravity_sickness`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/spacer.dm#L73)
- **Статус в ss13-rs:** ❌ нет

### Генетические повреждения и временные превращения
`genetic_damage` (от мутаторов, токсин-урон), `decloning`, `dna_melt`, временное превращение (жало трансформации, ДНК-инъектор). Подробно — см. ../genetics/.

- **Код:** [`/datum/status_effect/genetic_damage`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/genetic_damage.dm#L4), [`/datum/status_effect/decloning`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/decloning.dm#L4), [`/datum/status_effect/dna_melt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L825), [`/datum/status_effect/temporary_transformation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/dna_transformation.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Слизь и слаймы
Облепление слизью, кормление слайма, высасывание слаймом.

- **Код:** [`/datum/status_effect/slimed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/slime/slimed.dm#L46), [`/datum/status_effect/slime_food`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/slime/slime_food.dm#L2), [`/datum/status_effect/slime_leech`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/slime/slime_leech.dm#L7)
- **Статус в ss13-rs:** ❌ нет

### Метки и кровотечения шахтёрского/боевого снаряжения
Метка крашера (лаваленд), стекущее кровотечение пилы-тесака, перерезанное горло (`neck_slice`).

- **Код:** [`/datum/status_effect/crusher_mark`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L433), [`/datum/status_effect/stacking/saw_bleed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L525), [`/datum/status_effect/neck_slice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L551)
- **BandaStation:** [`modular_bandastation/status_effects/code/debuffs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/status_effects/code/debuffs.dm#L1) — перерезанное горло делает немым (`TRAIT_MUTE`)
- **Статус в ss13-rs:** ❌ нет

### Магические и антагонистские проклятия
Проклятие некрополя, гипнотический транс, «уходи» (go_away), фальшивый вирус, муравьи, упрёк, морозный взрыв, телепортационное безумие, проклятие Мидаса, метка цели, теневая речь, проклятие игрового автомата, Вавилонская башня, ржавая порча, огненная стена, нить-удавка, крючок рыболова, дискоординация.

- **Код:** [`/datum/status_effect/necropolis_curse`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L600), [`/datum/status_effect/trance`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L684), [`/datum/status_effect/ants`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L962), [`/datum/status_effect/midas_blight`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/debuffs.dm#L1176), [`/datum/status_effect/slot_machine_curse`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/cursed.dm#L5), [`/datum/status_effect/tower_of_babel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/tower_of_babel.dm#L1), [`/datum/status_effect/rust_corruption`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/rust_corruption.dm#L1), [`/datum/status_effect/strandling`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/strandling.dm#L5), [`/datum/status_effect/grouped/hooked`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/hooked.dm#L2)
- **Статус в ss13-rs:** ❌ нет
