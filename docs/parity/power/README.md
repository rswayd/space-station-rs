# Энергетика

Энергосистема BandaStation (tg): подсистема `SSmachines`, кабельные сети и powernet, щитки зон (APC), накопители SMES, ячейки и зарядники, освещение, генераторы (P.A.C.M.A.N., РИТЭГ, ТЭГ, солнечные панели, турбина), генератор гравитации, двигатели (сингулярность, тесла, суперматерия). Источники: `code/modules/power`, `code/controllers/subsystem/{machines,sun}.dm`, `code/game/machinery/{cell_charger,recharger,lightswitch}.dm`, площади питания из `code/game/area/areas.dm`; форк: `modular_bandastation/tesla_rework` и эстетика ламп, APC, SMES, выключателей. В ss13-rs (`crates/ss13_game/src/power/`, `crates/ss13_mapload/src/{power,light,cable}.rs`) реализованы расчёт сети, APC, SMES, РИТЭГ, каналы зон и лампы с аварийным режимом — всё построено при загрузке карты, без окон и монтажа; солнечные панели с консолью и SSsun (`power/solar.rs`), эмиттеры с отражателями (`power/emitter.rs`), суперматерия (`power/supermatter.rs`) с тесла-катушками и заземлителями (`power/accumulator.rs`); сингулярности и теслы нет.

| Файл | Что внутри |
| --- | --- |
| [grid.md](grid.md) | SSmachines, powernet, кабели, моток кабеля, терминалы, каналы зон, удар током |
| [apc.md](apc.md) | APC: зарядка и каналы, вид, окно, замок, монтаж, сбои, ночной режим, malf, хелперы |
| [smes.md](smes.md) | SMES и варианты, портативный SMES, монитор сети, аккумулятор энергии |
| [cells.md](cells.md) | ячейки, батареи, зарядник ячеек, зарядник оружия |
| [lighting.md](lighting.md) | лампы, аварийный свет, поломка, монтаж, выключатель, лампозаменитель, прожектор |
| [generators.md](generators.md) | P.A.C.M.A.N., РИТЭГ, ТЭГ, солнечные панели и SSsun, турбина, генератор гравитации |
| [singularity.md](singularity.md) | эмиттер, генератор и поле сдерживания, сингулярность, тесла (и переработка BandaStation), катушки |
| [supermatter.md](supermatter.md) | суперматерия: цикл, газы, молнии, повреждение, деламинации, монитор, варианты |
