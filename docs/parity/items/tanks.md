# Газовые баллоны, джетпаки и ранцы
Баллоны `/obj/item/tank`, подключение к дыханию, джетпаки и ранцевые распылители.

### Дыхательные баллоны
Кислородные, аварийные (карманные), двойные, плазменные, анестетические, баллоны плазменов; держат газ под давлением и отдают его с заданным давлением выпуска.

- **Код:** [`/obj/item/tank`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/tanks.dm#L14), [`/obj/item/tank/internals`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/tank_types.dm#L10), [`/obj/item/tank/internals/emergency_oxygen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/tank_types.dm#L166), [`populate_gas()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/tanks.dm#L132), [`remove_air_volume()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/tanks.dm#L306)
- **BandaStation:** [`plasmaman.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/outfits/plasmaman.dm#L7) — баллон плазмамена в кармане
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/internals.rs` (`TankAir`, `populate_gas`), `crates/ss13_mapload/src/clothing.rs` (`DmTank`)

### Подключение к дыханию (internals)
Alt-клик по баллону или кнопка действия открывает клапан, если надета маска с `MASKINTERNALS` или шлем с `HEADINTERNALS`; снятие баллона или маски закрывает его.

- **Код:** [`toggle_internals()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/tanks.dm#L104), [`ui_action_click()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/tanks.dm#L107)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/internals.rs`: alt-клик по баллону в инвентаре, проверка маски/шлема, закрытие при снятии; нет кнопки действия на HUD

### Окно баллона (давление выпуска)
tgui-окно баллона: текущее давление, регулировка давления выпуска, подключение к дыханию.

- **Код:** [`ui_interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/tanks.dm#L234), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/tanks.dm#L107); tgui: [`Tank.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Tank.jsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/internals.rs` (`TankWindow`: открывается в руке, `hands_state`, `pressure`: reset/min/max/число, зажим 0..3 атм), `crates/ss13_tgui/src/interfaces/tank.rs`; `RoundGauge` — текстом, как в `ChemHeater`

### Разрыв и взрыв баллона
Баллон с перегретой или пережатой смесью трескается, протекает и взрывается (основа плазменных бомб); к баллону можно прикрепить сборку с воспламенителем.

- **Код:** [`handle_tolerances()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/tanks.dm#L354), [`bomb_assemble()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/tanks.dm#L454), [`ignite()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/tanks.dm#L500)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/explosions/sources.rs` (`tank_tolerances`, `item_break`, `item_destruction`), вызывается из `crates/ss13_game/src/fire.rs` каждую секунду: выше `TANK_LEAK_PRESSURE` баллон получает урон (броня `bomb`), на половине прочности даёт течь (четверть газа в тайл за такт), при разрушении выше `TANK_FRAGMENT_PRESSURE` ещё раз реагирует и взрывается (`dyn_explosion` с вспышкой 1.5, с лимитом), газ выходит в тайл; нет сборок с воспламенителем (`bomb_assemble`, `ignite`), урона от `TANK_MELT_TEMPERATURE` с бронёй `fire`

### Джетпаки
Включаемый джетпак расходует газ и даёт движение в невесомости, со стабилизацией; капитанский, СБ, пустотный, импровизированный, CO2, обвязка; переключение на дыхание из джетпака.

- **Код:** [`/obj/item/tank/jetpack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/jetpack.dm#L1), [`turn_on()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/jetpack.dm#L108), [`configure_jetpack()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/jetpack.dm#L42)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/jetpack.rs` (`cycle`, `allow_thrust`, стабилизация, `full_speed`, газ из бака в тайл, EMP 4 с, улучшенные типы `drift_force`), кнопки в `crates/ss13_game/src/clothing/gear.rs` (тяга, стабилизация, дыхание), `crates/ss13_game/src/spacemove.rs` (шаг и дрейф в невесомости), начальный газ — `crates/ss13_game/src/internals.rs`; нет рецепта импровизированного джетпака (крафт — чужая группа)

### Ранцевые распылители
Заплечный бак с распылителем: уборщика (космоочиститель), пожарный (огнетушитель с пеной и стеной металлической пены), перцовый для СБ, химический бак.

- **Код:** [`/obj/item/watertank`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/watertank.dm#L2), [`/obj/item/watertank/janitor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/watertank.dm#L134), [`/obj/item/watertank/atmos`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/watertank.dm#L196), [`/obj/item/reagent_containers/chemtank`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/watertank.dm#L371)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/watertank.rs` (бак на спине выдаёт сопло в руку кнопкой или кликом в слот, сопло берёт реагенты бака и возвращается, если покинуло руку или бак снят; поливочный, уборщика, перцовый, пожарный с тремя режимами: огнетушитель `crates/ss13_game/src/extinguisher.rs`, пусковая смолы 100 ед. с откатом 10 с, смоляная пена на 5 зарядов; химический инъектор 5:1), перец — `crates/ss13_game/src/janitor.rs`; нет смены иконки бака по режиму, `drag_pickup`, замедления перцем
