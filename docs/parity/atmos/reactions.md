# Газовые реакции
Реакции `/datum/gas_reaction/standard`, которые идут в каждой смеси (тайлы, трубы, баки) при выполнении требований по газам и температуре, в порядке приоритета tg.

### Механизм реакций и приоритеты
Смесь перебирает подходящие реакции по приоритету (формации раньше горения), учитывает `reaction_results`; гипер-ноблий выше порога подавляет все реакции.

- **Код:** [`/datum/gas_reaction`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L42), [`/datum/gas_reaction/standard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L69), [`/datum/gas_mixture/proc/react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_mixture.dm#L497)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs` (таблица в порядке объявления, ранний выход для станционного воздуха)

### Конденсация водяного пара
Пар конденсируется при низкой температуре, делая пол мокрым, а ниже 0 °C — обледенелым.

- **Код:** [`/datum/gas_reaction/standard/water_vapor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L98), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L110), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs` (`Condensation`), `crates/ss13_game/src/janitor.rs` (`condensation`): пол мокнет и моется (не на беспочвенных и нескользящих тайлах), ниже точки осаждения — вечная мерзлота; заморозка предметов и мобов → ../effects/misc-effects.md «Заморозка предметов», ../mobs/status-effects-incapacitating.md «Заморозка (freon, ледяной блок)»

### Стерилизация миазмов
Миазмы распадаются при высокой температуре (с водяным паром медленнее), выделяя кислород.

- **Код:** [`/datum/gas_reaction/standard/miaster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L140), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L152), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L10)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs` (`miaster`)

### Горение плазмы
Плазма с кислородом горит выше 373 K, выделяя CO2 и водяной пар; при избытке кислорода (super saturation) производит тритий.

- **Код:** [`/datum/gas_reaction/standard/plasmafire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L181), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L196), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L19)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs` (`plasmafire`, числа сверены с DM)

### Горение водорода
Водород с кислородом горит с выделением водяного пара и большой энергии.

- **Код:** [`/datum/gas_reaction/standard/h2fire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L260), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L274), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L40)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs` (`h2fire`)

### Горение трития
Тритий горит с кислородом, при большом выгорании испускает радиационный импульс.

- **Код:** [`/datum/gas_reaction/standard/tritfire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L316), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L330), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L51)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`; радиационный импульс с турфа и горячей точки (`RadiationRelease`, `crates/ss13_game/src/radiation.rs`); `// shortcut:` не из трубопроводов

### Горение фреона
Эндотермическая реакция фреона с кислородом охлаждает смесь; в узком окне 120–160 K может выпасть горячий лёд.

- **Код:** [`/datum/gas_reaction/standard/freonfire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L380), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L395), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L63)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`; выпадение листов горячего льда → ../engineering/stacks-materials.md «Органические и прочие листы»

### Образование N2O
Азот и кислород с BZ-катализатором образуют закись азота в окне температур.

- **Код:** [`/datum/gas_reaction/standard/nitrousformation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L454), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L469), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L79)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`

### Разложение N2O
Закись азота распадается при нагреве на азот и кислород.

- **Код:** [`/datum/gas_reaction/standard/nitrous_decomp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L496), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L509), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L91)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`

### Образование BZ
N2O и плазма при низком давлении образуют BZ.

- **Код:** [`/datum/gas_reaction/standard/bzformation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L538), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L551), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L102)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`

### Образование плюоксия
CO2, кислород и тритий в холоде дают плюоксий и водород.

- **Код:** [`/datum/gas_reaction/standard/pluox_formation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L598), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L613), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L121)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`

### Образование нитрия
Тритий, азот и BZ при высокой температуре образуют нитрий (эндотермия).

- **Код:** [`/datum/gas_reaction/standard/nitrium_formation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L646), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L660), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L132)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`

### Разложение нитрия
Нитрий с кислородом распадается ниже порога температуры.

- **Код:** [`/datum/gas_reaction/standard/nitrium_decomposition`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L693), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L706), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L145)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`

### Образование фреона
Плазма, CO2 и BZ при сильном нагреве дают фреон.

- **Код:** [`/datum/gas_reaction/standard/freonformation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L737), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L751), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L155)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`

### Образование гипер-ноблия
Азот и тритий при огромной энергии образуют гипер-ноблий; BZ снижает затраты.

- **Код:** [`/datum/gas_reaction/standard/nobliumformation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L790), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L804), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L171)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`

### Удаление кислорода галоном
Галон при горении поглощает кислород и создаёт смоляную пену, тушащую пожар.

- **Код:** [`/datum/gas_reaction/standard/halon_o2removal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L843), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L856), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L187)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`; смоляная пена → ../effects/effect-systems.md «Пена (do_foam) и её виды»

### Образование хилиума
BZ и фреон в холоде образуют хилиум.

- **Код:** [`/datum/gas_reaction/standard/healium_formation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L896), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L910), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L196)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`

### Образование заукера
Гипер-ноблий и нитрий при высокой температуре дают заукер.

- **Код:** [`/datum/gas_reaction/standard/zauker_formation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L937), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L951), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L206)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`

### Разложение заукера
Заукер распадается в присутствии азота на кислород и азот.

- **Код:** [`/datum/gas_reaction/standard/zauker_decomp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L980), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L992), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L215)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`

### Образование прото-нитрата
Плюоксий и водород при нагреве дают прото-нитрат.

- **Код:** [`/datum/gas_reaction/standard/proto_nitrate_formation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L1020), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L1034), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L224)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`

### Прото-нитрат + водород
Прото-нитрат превращает водород в себя.

- **Код:** [`/datum/gas_reaction/standard/proto_nitrate_hydrogen_response`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L1062), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L1074), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L234)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`

### Прото-нитрат + тритий
Прото-нитрат перерабатывает тритий в водород с радиационным импульсом.

- **Код:** [`/datum/gas_reaction/standard/proto_nitrate_tritium_response`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L1100), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L1114), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L244)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`; радиационный импульс с турфа и горячей точки (`RadiationRelease`, `crates/ss13_game/src/radiation.rs`); `// shortcut:` не из трубопроводов

### Прото-нитрат + BZ
Прото-нитрат разлагает BZ на азот, гелий и плазму, излучая радиацию и ядерные частицы.

- **Код:** [`/datum/gas_reaction/standard/proto_nitrate_bz_response`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L1153), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L1167), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L253)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`; радиационный импульс (за вычетом доли частиц, `crates/ss13_game/src/radiation.rs`); ядерные частицы → ../combat/ammo-projectiles.md «Особые снаряды»

### Репликация антиноблия
Антиноблий размножается за счёт любых других газов выше порога температуры.

- **Код:** [`/datum/gas_reaction/standard/antinoblium_replication`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L1205), [`react()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reactions.dm#L1222), [`init_factors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/reaction_factors.dm#L271)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/reactions.rs`
