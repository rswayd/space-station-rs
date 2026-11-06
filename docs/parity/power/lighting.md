# Осветительные приборы
Лампы станции (`/obj/machinery/light`), их питание, аварийный режим, монтаж и обслуживание. Сам движок освещения (источники, тени) — см. ../effects/.

### Лампа: работа от зоны
Лампа горит, если запитан канал освещения её зоны и включён выключатель зоны; яркость, мощность и цвет из лампочки, свет направлен от стены; тратит статическую мощность.

- **Код:** [`/obj/machinery/light`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L2), [`proc/update()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L232), [`proc/has_power()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L510), [`power_change()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L736)
- **BandaStation:** [`modular_bandastation/aesthetics/lights/code/lights.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/lights/code/lights.dm#L1) — свои иконки, `bulb_colour` #FFF7F2, `bulb_power` 0.8, мощность ночного режима 0.6
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/power/mod.rs` (`LightFixture`, `apply_area_power`), `crates/ss13_mapload/src/light.rs`

### Аварийное освещение
Без питания лампа с аварийной ячейкой светит красным в четверть дальности, ячейка тратится (~10 минут) и подзаряжается при питании; «major emergency» от пожарной тревоги.

- **Код:** [`proc/use_emergency_power()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L526), [`proc/has_emergency_power()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L516), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L329), [`proc/set_major_emergency_light()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L648)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/power/model.rs` (`light_mode`, `EMERGENCY_CELL_CHARGE`), `crates/ss13_game/src/power/mod.rs` (`set_area_fire`): аварийный режим и подзарядка, красный свет `fire_*` при пожарной тревоге зоны; нет «major emergency» (ставят код Эпсилон после установки и каскад суперматерии; сам код — ../security/security-levels.md)

### Поломка, перегорание и мерцание ламп
Лампы бьются ударами и взрывами (искры, осколки), перегорают при включении, случайная поломка на старте раунда, мерцание (ЭМИ, события, призраки), удар током при вынимании голой рукой горячей лампы.

- **Код:** [`proc/break_light_tube()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L690), [`proc/burn_out()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L343), [`proc/flicker()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L542), [`attack_hand_secondary()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L584), [`emp_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L721)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/power/lighting.rs` (`LightStatus`, `break_tube`, `burn_out`, `take_damage`, `flicker`, `remove_by_hand`): поломка на старте 2%/5% (трубка/лампочка), от ударов в боевом режиме (`prob(урон × 5)`, прочность 100, при 0 — каркас), взрывов (`ex_act`), жара, пожирателя света, бури; искры и звук стекла; перегорание при включении `prob(min(60, switchcount²/100))`; мерцание (`flicker`, серый прилив каждые 12 тиков); горячая лампа обжигает руку (5 burn) без перчаток >360 K. Нет: мерцания от призраков (их «boo» — ../ghosts), заряженных реагентами ламп, ethereal-подзарядки

### Лампочки и трубки (/obj/item/light)
Предметы-лампы: трубка и лампочка, состояния целая/перегоревшая/разбитая, разбиваются о пол с осколками.

- **Код:** [`/obj/item/light`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light_items.dm#L5), [`/obj/item/light/tube`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light_items.dm#L53), [`/obj/item/light/bulb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light_items.dm#L76)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/power/lighting.rs` (`LightBulb`, `bulb_of`, `shatter`): статус, яркость и `switchcount` переходят между лампой и светильником, иконки `-burned`/`-broken`; разбиваются при броске (`crate::throw`), когда на них наступают, и об моба в боевом режиме. Нет: реагентов внутри, калтропа (урон ногам от разбитой), inhand-состояний

### Замена лампы и монтаж светильника
Вставка и вынимание лампы, отвёртка разбирает светильник в каркас; каркас (`light_construct`) собирается из рамки, кабеля и отвёртки.

- **Код:** [`item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L385), [`proc/drop_light_tube()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L656), [`screwdriver_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L433), [`/obj/structure/light_construct`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light_construct.dm#L1), [`/obj/item/wallframe/light_fixture`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light_wallframes.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/power/lighting.rs` (`fixture_interaction`, `drop_light_tube`, `deconstruct`, `frame_interaction`, `place_frame`, `build_fixture`): ПКМ пустой рукой вынимает лампу, лампа своего типа вставляется или заменяет старую, отвёртка разбирает пустой светильник в провод. каркас; каркас: кабель → отвёртка (новый пустой светильник, ночной режим от АПЦ, в зоне), кусачки, гаечный ключ (3 с, листы железа); рамка на стену с соседней клетки. Нет: батареи в каркасе, напольной рамки (`light_fixture/small` attack_self), засовывания предметов в пустой патрон (удар током)

### Варианты ламп и маппинг-хелперы
Маленькие, напольные, тёплые, холодные, красные, приглушённые, ультрафиолетовые, перегоревшие лампы.

- **Код:** [`/obj/machinery/light/small`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light_mapping_helpers.dm#L59), [`/obj/machinery/light/floor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light.dm#L773), [`/obj/machinery/light/warm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light_mapping_helpers.dm#L17), [`/obj/machinery/light/blacklight`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/lighting/light_mapping_helpers.dm#L48)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_mapload/src/light.rs` (параметры света по типу и правкам карты)

### Выключатель света (light switch)
Настенный выключатель включает и выключает все лампы своей зоны; BandaStation добавляет звук щелчка и свою иконку.

- **Код:** [`/obj/machinery/light_switch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/lightswitch.dm#L2), [`interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/lightswitch.dm#L79), [`proc/set_lights()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/lightswitch.dm#L93)
- **BandaStation:** [`modular_bandastation/aesthetics/lightswitch/code/lightswitch.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/lightswitch/code/lightswitch.dm#L4) — звук и иконка
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/wallmount/mod.rs` (`LightSwitch`, `set_lights`): клик с питанием переключает `lightswitch` зоны (лампы гаснут/зажигаются через `AreaPowerChanged`), все выключатели зоны меняют вид (`-on/-off/-nopower`, эмиссив), выключенный светится; звуки tg и BandaStation. Нет переменной `area` (управление чужой зоной), разборки отвёрткой. ЭМИ (`crates/ss13_game/src/emp/power.rs`): аварийная ячейка разряжается, рабочая лампа перегорает с `150/severity`% (`-burned`, `power::burn_out_light`)

### Лампозаменитель (light replacer)
Инструмент уборщика: меняет лампы на расстоянии, перерабатывает осколки стекла в новые лампы.

- **Код:** [`/obj/item/lightreplacer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/lightreplacer.dm#L34), [`proc/do_action()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/lightreplacer.dm#L223)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/power/lighting.rs` (`LightReplacer`, `refill`, `replace_light`): 10/20 зарядов (продвинутый 50), заправка лампами, листом стекла (+5) и чистыми осколками (4 → лампа, «ding»), замена лампы в светильнике, во всех светильниках клетки и на своей клетке (`attack_self`), разбитая уходит в осколки. Нет: дальнего действия блюспейс-версии, emag (плазменные лампы), заправки из коробок, зарядки боргов, достройки прожектора

### Прожектор (floodlight)
Собираемый прожектор на кабеле с уровнями яркости и потреблением из сети.

- **Код:** [`/obj/machinery/power/floodlight`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/floodlight.dm#L135), [`/obj/structure/floodlight_frame`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/power/floodlight.dm#L7)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/power/lighting.rs` (`Floodlight`, `FloodFrame`, `change_setting`, `floodlight_tick`): каркас — 5 кабеля, отвёртка, целая трубка (или лампозаменитель), кусачки и ключ (3 с, 5 листов железа) обратно; рукой — уровни выкл/5/10/15 тайлов (1/2/3 кВт из сети кабеля под ним, без оплаты понижается или гаснет); отвёртка — панель (с открытой рука вынимает трубку, остаётся каркас), ключ — закрепить и подключить к сети; с карты закреплён; ЭМИ `prob(150/severity)` ломает в каркас с разбитой трубкой. Нет: 2 с у ключа, покраски, смены слоя кабеля, поломки от урона
