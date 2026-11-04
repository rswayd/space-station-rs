# Ячейки питания, батареи и зарядники
Элементы питания (`/obj/item/stock_parts/power_store`) и машины для их зарядки.

### Хранилище энергии (power_store)
Общая логика ячеек и батарей: заряд, `use`/`give`, проценты, заминированная плазмой ячейка взрывается, ЭМИ портит заряд, эфириалы высасывают заряд.

- **Код:** [`/obj/item/stock_parts/power_store`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/power_store.dm#L11), [`use()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/power_store.dm#L158), [`proc/give()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/power_store.dm#L173), [`proc/try_explode()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/power_store.dm#L200), [`proc/corrupt()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/power_store.dm#L230)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/power/model.rs` (`Cell`: take/give, проценты) внутри APC и ламп, `ItemCell` у предметов; ЭМИ разряжает (`crates/ss13_game/src/emp/power.rs`); нет взрыва заминированной ячейки, порчи (`corrupt`), высасывания эфириалами

### Ячейки питания (cell)
Ячейки разной ёмкости (базовая, high, super, hyper, bluespace, бесконечная, картофельная, ЭМИ-стойкая, эфириальная, кристаллическая, для ламп и оружия).

- **Код:** [`/obj/item/stock_parts/power_store/cell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/cell.dm#L7), [`cell/high`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/cell.dm#L114), [`cell/bluespace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/cell.dm#L148), [`cell/emergency_light`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/cell.dm#L234)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mecha/core.rs` (`ItemCell`: ячейки-предметы с зарядом, печать `/empty` пустыми, как в tg), `crates/ss13_mapload/src/power.rs` (ячейки APC и ламп); ЭМИ — `STANDARD_CELL_CHARGE / severity × emp_damage_modifier` по типу (high 3, super/hyper/bluespace 5, импульсная 0,5, ЭМИ-стойкая/бесконечная 0 и `EMP_PROTECT_SELF`), `crates/ss13_game/src/emp/power.rs`; нет самозаряда

### Батареи (battery)
Крупные элементы для SMES и машин (upgraded, high, super, hyper, bluespace, crap, infinite).

- **Код:** [`/obj/item/stock_parts/power_store/battery`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/battery.dm#L1), [`battery/bluespace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/battery.dm#L62)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/power.rs`: ёмкость SMES считается по батареям платы при конвертации; батарей как предметов нет

### Зарядник ячеек (cell charger)
Настольная машина, заряжающая вставленную ячейку от APC зоны.

- **Код:** [`/obj/machinery/cell_charger`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/cell_charger.dm#L1), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/cell_charger.dm#L137)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/power/chargers.rs` (`CellCharger`: вставка/извлечение, `0,25 × STANDARD_CELL_RATE ×` тиры конденсаторов только из избытка сети — `chargers::cell_charger_rate`, оверлеи заряда); разборка и RPED — `crates/ss13_game/src/machine_frame.rs`, `crates/ss13_game/src/stock_parts.rs`; ЭМИ запитанного зарядника бьёт по ячейке (`crates/ss13_game/src/emp/power.rs`); нет ключа (shortcut)

### Зарядник оружия (recharger)
Заряжает энергооружие, дубинки и подобные предметы.

- **Код:** [`/obj/machinery/recharger`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/recharger.dm#L1), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/recharger.dm#L144)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/power/chargers.rs` (`Recharger`): оглушающие дубинки, `chargerate` ячейки из сети, затем ячейки APC, `recharge_coeff` — тир конденсатора (`chargers::recharge_coeff`), оверлеи empty/charging/full, «заряжено!»; ЭМИ: дубинка в нём разряжается до нуля, ячейка оружия — по правилу ячейки; энергооружия, магазинов и КПК нет
