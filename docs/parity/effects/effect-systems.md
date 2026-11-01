# Эффект-системы: искры, дым, пена, пар, следы

Визуальные и физические эффекты, которые порождают другие механики (`code/game/objects/effects/effect_system`, подсистема `SSfluids`): искры, облака дыма, пена, брызги воды, пар, ионные следы, частицы.

### Каркас эффект-систем
`/datum/effect_system` с `set_up(количество, кардинально, место)` и `start()` порождает N частиц `/obj/effect/particle_effect`, разлетающихся от точки и исчезающих; основа для искр, пара и частиц взрыва.

- **Код:** [`/datum/effect_system`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effect_system.dm#L21), [`/datum/effect_system/basic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effect_system.dm#L49), [`/obj/effect/particle_effect`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effect_system.dm#L11)
- **Статус в ss13-rs:** ❌ нет — в `crates/` нет эффект-систем

### Искры (do_sparks)
Короткие искры при ударе током, взломе, поломке электроники, телепортации; искра — горячая точка, поджигающая горючий газ, и источник света. Варианты: квантовые (синие, телепорт), электрические.

- **Код:** [`proc/do_sparks()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_sparks.dm#L8), [`/obj/effect/particle_effect/sparks`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_sparks.dm#L15), [`/datum/effect_system/basic/spark_spread`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_sparks.dm#L123), [`/obj/effect/particle_effect/sparks/quantum`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_sparks.dm#L80), [`/datum/effect_system/basic/lightning_spread`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_sparks.dm#L150)
- **Статус в ss13-rs:** ❌ нет — нет искр (`crates/ss13_game/src/wires.rs`: shortcut no sparks; удары током без искр)

### Подсистема жидкостей SSfluids
Распространение дыма и пены по соседним открытым тайлам и их эффекты обрабатываются каруселью корзин (`/datum/fluid_group` ограничивает общий объём облака).

- **Код:** [`SUBSYSTEM_DEF(fluids)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/fluids.dm#L20), [`/datum/fluid_group`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/_fluid_spread.dm#L8), [`/obj/effect/particle_effect/fluid`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/_fluid_spread.dm#L83), [`/datum/effect_system/fluid_spread`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/_fluid_spread.dm#L117)
- **Статус в ss13-rs:** ❌ нет — нет дыма и пены

### Дым (do_smoke)
Непрозрачное облако, расползающееся по тайлам и рассеивающееся за `lifetime` (дымовые гранаты, взрывы, реакции); блокирует обзор и свет. Варианты: быстрый, прозрачный, «плохой» (кашель, нельзя дышать без маски; зелёный и чёрный), замораживающий (тушит огонь, гасит плазму; деконтаминационный), усыпляющий.

- **Код:** [`proc/do_smoke()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_smoke.dm#L170), [`/obj/effect/particle_effect/fluid/smoke`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_smoke.dm#L4), [`/obj/effect/particle_effect/fluid/smoke/bad`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_smoke.dm#L193), [`/obj/effect/particle_effect/fluid/smoke/freezing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_smoke.dm#L259), [`/obj/effect/particle_effect/fluid/smoke/sleeping`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_smoke.dm#L341)
- **Статус в ss13-rs:** ❌ нет — нет дыма

### Химический дым (do_chem_smoke)
Дым, несущий реагенты: каждый, кто в облаке, вдыхает их (реакция дымового порошка, химгранаты, Scrubber Overflow); объём делится на тайлы облака.

- **Код:** [`proc/do_chem_smoke()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_smoke.dm#L400), [`/obj/effect/particle_effect/fluid/smoke/chem`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_smoke.dm#L362), [`/datum/effect_system/fluid_spread/smoke/chem`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_smoke.dm#L429)
- **Статус в ss13-rs:** ❌ нет — нет дыма; реагенты есть (`crates/ss13_reagents`), но реакции-эффекты не портированы

### Пена (do_foam) и её виды
Пена расползается и через время лопается, нанося реагенты на всё, что в ней (химпена, Scrubber Overflow). Огнетушащая пена тушит огонь и охлаждает; металлическая (алюминиевая/железная) застывает в стены-перегородки (`foamed metal`), «умная» не перекрывает двери; смоляная (атмосферная) застывает в непроницаемую для газа смолу и гасит пожар (вариант на галоне); грязная — от Scrubber Overflow.

- **Код:** [`proc/do_foam()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_foam.dm#L188), [`/obj/effect/particle_effect/fluid/foam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_foam.dm#L13), [`/obj/effect/particle_effect/fluid/foam/firefighting`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_foam.dm#L273), [`/obj/effect/particle_effect/fluid/foam/metal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_foam.dm#L327), [`/obj/effect/particle_effect/fluid/foam/metal/smart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_foam.dm#L399), [`/obj/effect/particle_effect/fluid/foam/metal/resin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_foam.dm#L421), [`/obj/effect/particle_effect/fluid/foam/dirty`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/fluid_spread/effects_foam.dm#L491)
- **Статус в ss13-rs:** ❌ нет — нет пены (`crates/ss13_atmos/src/reactions.rs`: shortcut no resin foam у реакции галона)

### Брызги воды и огнетушитель
Капли воды от огнетушителя, распылителя или пожарного шланга летят несколько тайлов, нанося реагент (тушат огонь, смывают, охлаждают); пар — частицы при нагреве воды.

- **Код:** [`/obj/effect/particle_effect/water`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_water.dm#L3), [`/obj/effect/particle_effect/water/extinguisher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_water.dm#L29), [`/obj/effect/particle_effect/steam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_water.dm#L81), [`/datum/effect_system/basic/steam_spread`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_water.dm#L90)
- **Статус в ss13-rs:** ❌ нет — нет огнетушителя и брызг

### Следы движения (ионные следы, дымный шлейф)
`trail_follow`: за джетпаком, MOD-движителем или ракетой остаются ионные частицы, пар или дымный шлейф.

- **Код:** [`/datum/effect_system/trail_follow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_trail.dm#L9), [`/datum/effect_system/trail_follow/ion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_trail.dm#L97), [`/obj/effect/particle_effect/ion_trails`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_trail.dm#L89), [`/datum/effect_system/trail_follow/smoke`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/effect_system/effects_trail.dm#L106)
- **Статус в ss13-rs:** ❌ нет — нет джетпаков и следов

### Частицы BYOND (particles)
`/obj/effect/abstract/particle_holder` и `/particles/*`: системы частиц BYOND для огня, дыма сигарет, кислоты, воды, нот, гравитации, слизи и т.д.

- **Код:** [`/obj/effect/abstract/particle_holder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/particle_holder.dm#L3), [`code/game/objects/effects/particles/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/particles)
- **Статус в ss13-rs:** ❌ нет — клиент не рисует BYOND particles
