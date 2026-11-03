# Газы и газовые смеси
Модель газовой смеси (`/datum/gas_mixture`) и каталог 20 газов, на которых строится вся атмосфера.

### Газовая смесь (/datum/gas_mixture)
Моли каждого газа, температура и объём; давление по закону идеального газа, теплоёмкость, слияние, отбор доли, обмен с другой смесью.

- **Код:** [`/datum/gas_mixture`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_mixture.dm#L9), [`proc/return_pressure()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_mixture.dm#L113), [`proc/merge()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_mixture.dm#L142), [`proc/remove()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_mixture.dm#L197), [`proc/temperature_share()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_mixture.dm#L445)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/gas.rs` (`GasMixture`: pressure, heat_capacity, merge, remove, remove_ratio, share, conduction)

### Перекачка газа между смесями (pump_gas_to, release_gas_to)
Общая математика насосов и клапанов: сколько молей перенести, чтобы достичь целевого давления на выходе (квадратичное приближение с учётом температур).

- **Код:** [`proc/pump_gas_to()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_mixture.dm#L696), [`proc/release_gas_to()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_mixture.dm#L718), [`proc/gas_pressure_calculate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_mixture.dm#L584)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/pipenet/air.rs` (`gas_pressure_calculate` с квадратичным решением и Ньютоном, `pump_moles`, `release_moles`); используют насосы, клапаны, канистры

### Каталог газов
20 газов с удельной теплоёмкостью, молярной массой, оверлеем, ценой и флагами: кислород, азот, CO2, плазма, водяной пар, гипер-ноблий, N2O, нитрий, тритий, BZ, плюоксий, миазмы, фреон, водород, хилиум, прото-нитрат, заукер, галон, гелий, антиноблий.

- **Код:** [`/datum/gas`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_types.dm#L50), [`proc/meta_gas_list()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_types.dm#L1), [`/datum/gas/plasma`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_types.dm#L105), [`/datum/gas/antinoblium`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_types.dm#L305)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/gas.rs` (`GASES`; shortcut: таблица в коде, а не чтение `/datum/gas` из dm_types)

### Строки газа (initial_gas_mix)
Текстовый формат `o2=22;n2=82;TEMP=293.15`, которым задаются воздух тайлов, баков и канистр на карте и в коде.

- **Код:** [`SSair.parse_gas_string()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/air.dm#L779)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/gas.rs` (`GasMixture::parse`), `crates/ss13_mapload/src/atmos.rs`

### Сравнение смесей и сон тайлов (compare)
Порог разницы молей и температуры, ниже которого тайлы считаются равными и не обрабатываются.

- **Код:** [`proc/compare()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_mixture.dm#L470)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/gas.rs`, `crates/ss13_atmos/src/level.rs`

### Парциальное давление для дыхания
Давление отдельного газа в порции вдоха; используется лёгкими, см. дыхание в ../mobs/ (дыхание принадлежит мобам).

- **Код:** [`proc/get_breath_partial_pressure()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_mixture.dm#L562)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/gas.rs` (`partial_pressure`), `crates/ss13_game/src/breath.rs`
