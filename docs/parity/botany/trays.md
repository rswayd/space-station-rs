# Гидропонные лотки и почва
Лоток — основная машина ботаники: в нём растёт посаженное семя, расходуются вода и удобрения, копятся сорняки, вредители и токсины.

### Гидропонный лоток (базовый цикл роста)
Машина, в которую сажают семя; каждые `cycledelay` растение стареет, тратит воду (`maxwater` 100) и питательные вещества (`maxnutri` 20, расход `nutridrain`), без воды/удобрений теряет здоровье, по достижении `maturation` и `production` становится готовым к сбору. Состояния: пусто, растёт, готово, мертво; световые индикаторы на лотке показывают нехватку воды/удобрений, вредителей, сорняки, готовность.

- **Код:** [`/obj/machinery/hydroponics`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L1), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L288), [`update_status_light_overlays()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L499)
- **BandaStation:** [`hydroponics.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L472) — имя лотка с названием растения через `ru_names_toml`
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/tray.rs` (`cycle`): цикл 20 с, возраст, питательные вещества, вода, свет (статическая освещённость тайла), токсины, вредители, сорняки, смерть, спрайт и индикаторы; `// shortcut:` свет считается по прямой линии, динамические источники как статические

### Посадка, полив и взаимодействие с предметами
Клик семенем сажает его; ведро/лейка/кружка с реагентами поливает и удобряет; лопатка убирает растение, культиватор — сорняки, секатор снимает прививку; мешок для растений собирает урожай. ПКМ пустой рукой (с подтверждением) сливает реагенты лотка, гаечный ключ откручивает лоток.

- **Код:** [`item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L895), [`attack_hand_secondary()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L1198), [`empty_tray()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L1211), [`set_seed()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L519)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/`: посадка пакетом, полив и подкормка ёмкостью, компост из еды, культиватор, лопата (5 с), секатор и прививки (`crates/ss13_game/src/botany/grafts.rs`), осмотр; генные ножницы (`crates/ss13_game/src/botany/shears.rs`); `// shortcut:` нет опустошения правым кликом и фиксации мутации флора-ганом

### Сбор урожая и удаление растения
Клик пустой рукой по готовому растению собирает урожай (`seeds/harvest()`: количество зависит от `yield` и `yieldmod`), по мёртвому — убирает его. Многоразовые растения (`repeated_harvest`) после сбора продолжают расти.

- **Код:** [`attack_hand()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L1167), [`/obj/item/seeds/proc/harvest()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seeds.dm#L208), [`update_tray()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L1224), [`remove_plant()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L1272)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/plants.rs` (`harvest`): пустая рука собирает `getYield()` плодов с реагентами по потенции, убирает мёртвое растение; сумок для растений нет

### Автополив (self-sustaining)
Ctrl+клик по закреплённому и запитанному лотку включает автороста: лоток сам доливает воду, снижает сорняки и вредителей, но потребляет много энергии; при пропаже питания отключается.

- **Код:** [`click_ctrl()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L1183), [`set_self_sustaining()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L543)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/botany/mod.rs` (`ctrl_clicks`, `self_sustain`, `set_self_sustaining`): ctrl+клик по закреплённому лотку в запитанной зоне включает/выключает автополив, каждые 2 с вода `rand(1, 2) × 2 × 0.5`, сорняки и вредители −1; расход удобрений ≤ 0.5, нет мутации вида и захвата сорняками (`tray.rs`, `Tray::self_sustain`); нагрузка `active_power_usage` на зону, свет радиуса 3 и оверлей `gaia_blessing`; без питания — отключается с сообщением; сбор однократного растения и выкапывание выключают

### Сорняки, вредители и токсичность
Уровни сорняков, вредителей и токсинов (0–10/100) растут со временем и от реагентов, снижают здоровье растения; на 10 сорняков пустой или слабый лоток захватывает сорняк (`weedinvasion`), растение может мутировать в сорняк (`mutateweed`), вредители — в мутацию (`mutatepest`).

- **Код:** [`weedinvasion()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L692), [`mutateweed()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L800), [`mutatepest()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L877), [`adjust_toxic()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L630)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/tray.rs`: рост сорняков и вредителей, урон от токсинов; `// shortcut:` `mutatepest` предупреждает без паучат

### Реагенты в лотке и мутации
Каждый цикл реагенты лотка применяются к растению через `/datum/reagent/proc/on_hydroponics_apply()`: удобрения (E-Z-Nutrient, Left 4 Zed, Robust Harvest) меняют статы и мутации, мутаген/радий/урановая радиация вызывают `mutate`/`hardmutate`/смену вида (`mutatespecie`), сахар/яды/гербициды бьют по здоровью и сорнякам.

- **Код:** [`apply_chemicals()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics_chemreact.dm#L5), [`radioactive_exposure()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics_chemreact.dm#L30), [`mutate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L724), [`mutatespecie()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L766), [`polymorph()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L783), [`/datum/reagent/proc/on_hydroponics_apply()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents.dm#L302)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/tray.rs`: ~40 реагентов с `on_hydroponics_apply` (E-Z-Nutrient, Left 4 Zed, Robust Harvest, аммиак, мутаген, гербициды, пестициды, вода, зола...), мутации по нестабильности (статы, трейты, вид из `mutatelist`); странный реагент не оживляет растение

### Соматолуч (флора-ган) по лотку
Снаряд флора-гана при попадании в лоток мутирует растение или меняет урожай/потенцию в зависимости от режима.

- **Код:** [`bullet_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L267)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/botany/tray.rs` (`somatoray_hit`), `crates/ss13_game/src/botany/mod.rs` (`bullet_act`, вызывается из `crates/ss13_game/src/projectile.rs`): альфа-луч — `mutate()`, бета — +`rating` урожая с шансом `100 / yield²` (`/obj/item/seeds/bullet_act`), гамма — нестабильность пополам и `mutatespecie()`; пустой лоток принимает выстрел как обычный объект; фиксация мутации флора-ганом (клик ружьём по лотку) — в «Посадка, полив и взаимодействие с предметами»

### Опыление
Соседние лотки в радиусе 1 обмениваются реагентными генами растений (`perform_reagent_pollination`); пчёлы из улья опыляют лотки, повышая урожай и потенцию.

- **Код:** [`pollinate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L843), [`can_bee_pollinate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L865), [`perform_reagent_pollination()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/seeds.dm#L590)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/botany/mod.rs` (`pollinate`), `crates/ss13_game/src/botany/genes.rs` (повышение реагентных генов); нет: окна между лотками не мешают (`// shortcut:`), опыление пчёлами (пчёл нет)

### Улитка в лотке
Посаженная в лоток улитка поедает вредителей и сорняки; её видно оверлеем на лотке, её можно забрать рукой.

- **Код:** [`handle_snail()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L647), [`/obj/effect/overlay/vis_effect/snail`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/snail_overlay.dm#L4)
- **BandaStation:** [`hydroponics.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L184) — у лотка отключена рыболовная точка (`fishing_spot`)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/botany/mod.rs` (`snail_interaction`, `handle_snails`, `TraySnail`): живая улитка в руке (`crate::mob_holder`) по лотку без улитки за 2 с садится в него, каждые 2 с 5% −2 сорняка, оверлей `snail_hydrotray`; при попытке шагнуть или смерти выходит на тайл лотка; `// shortcut:` оверлей не ползает (`handle_animation`), опустошения лотка ПКМ нет (его группа)

### Собираемые лотки и улучшения
Лоток собирается из платы (`/obj/machinery/hydroponics/constructable`), детали (матлазеры/манипуляторы) повышают `maxwater`, `maxnutri` и снижают расход; есть старый вид лотка и «делюкс»-вариант с максимальными деталями. Отвёртка/лом — разборка.

- **Код:** [`/obj/machinery/hydroponics/constructable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L165), [`fullupgrade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L174)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/plants.rs`: типы лотков и грядок с карты; отвёртка/лом разбирают лоток в раму, из рамы с платой, 2 бункерами материи, сервоприводом и стеклом собирается пустой сухой лоток (`crates/ss13_game/src/machine_frame.rs`, `botany::setup_tray`); детали — предметы внутри, `RefreshParts` (`botany::refresh_parts`: `maxwater = 50 × бункеры`, `maxnutri = 5 × бункеры + 10`, рейтинг — тир сервопривода) после сборки и замены RPED (`crates/ss13_game/src/stock_parts.rs`); почвы — `crates/ss13_game/src/botany/soil.rs`; лотки с карты — с деталями платы по умолчанию

### Компонент схемы лотка
USB-компонент wiremod для лотка: выдаёт уровни воды/удобрений/сорняков/вредителей/здоровья, событие сбора и смерти, управляет автополивом.

- **Код:** [`/obj/item/circuit_component/hydroponics`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/hydroponics.dm#L1284)
- **Статус в ss13-rs:** ❌ нет — компоненты wiremod есть (`crates/ss13_game/src/wiremod/`), но USB-порта и USB-кабеля нет (❌ в [wiremod.md](../science/wiremod.md), «Инструменты схем»); компонент лотка ждёт их. Состояние для него уже есть: `Tray::self_sustaining` и `botany::set_self_sustaining` (вход «Auto-Grow Setting»)

### Почва и мешки с грунтом
Грядка почвы — беспитательный лоток без энергии (`maxnutri` 15), выкапывается лопатой обратно в мешок. Виды грунта дают бонусы: вермикулит, гель (не нужна вода), кокосовое волокно, червячная почва (медленное выделение азота), богатая почва (быстрые грибы).

- **Код:** [`/obj/machinery/hydroponics/soil`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/soil.dm#L3), [`/obj/item/soil_sack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/soil.dm#L115), [`soil/worm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/soil.dm#L92), [`soil/rich`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/soil.dm#L106)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/botany/soil.rs`: мешок по открытому полу (не космос/openspace/пропасть/лава) за 1 с ставит грядку своего вида (свежая — полная воды и `maxnutri / 2` E-Z-Nutrient, выкопанная — со своим состоянием), ПКМ лопатой по грядке за 3 с убирает её обратно в мешок (растение удаляется), мешок по пустому лотку за 2 с засыпает грунт: флаги грунта, у собираемого лотка ёмкость `soil.maxwater + (бункеры − 2) × 50`, `soil.maxnutri + (бункеры − 2) × 5`, оверлей `<icon_state>_tray`, строка осмотра; бонусы: кокос — грибы стареют ×1.4 (`tray.rs`, `FAST_MUSHROOMS`), гель — `SUPERWATER`, червячная — `SLOW_RELEASE`, вермикулит — прививки; у грядки нет автополива; `// shortcut:` мешок не двуручный, грядка не разбирается в песок, лоток не переименовывается (BandaStation), черви из компоста в червячной почве (`WORM_HABITAT`) не появляются
