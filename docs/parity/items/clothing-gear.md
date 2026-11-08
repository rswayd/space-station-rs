# Одежда: очки, перчатки, обувь, уши и шея
Мелкие слоты экипировки: `glasses`, `gloves`, `shoes`, `ears`, `neck`.

### Солнцезащитные очки и защита от вспышек
Солнечные очки и сварочные очки защищают от вспышек (`flash_protect`); у сварочных — затемнение, поднимаются; повязки на глаза ослепляют.

- **Код:** [`/obj/item/clothing/glasses`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/_glasses.dm#L2), [`/obj/item/clothing/glasses/sunglasses`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/_glasses.dm#L397), [`/obj/item/clothing/glasses/welding`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/_glasses.dm#L505), [`/obj/item/clothing/glasses/blindfold`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/_glasses.dm#L544)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/clothing.rs` (`flash_protect`), `crates/ss13_game/src/security.rs`: вспышка учитывает защиту; нет `tint`, ослепления повязкой, поднятия очков

### Очки зрения: мезоны, ночное, термальное, научные
Мезоны показывают стены и структуры сквозь темноту, ПНВ — ночное зрение, термальные — существ сквозь стены, научные очки показывают состав реагентов; инженерные очки переключают режимы (радиация, T-ray, давление).

- **Код:** [`/obj/item/clothing/glasses/meson`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/_glasses.dm#L88), [`/obj/item/clothing/glasses/night`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/_glasses.dm#L175), [`/obj/item/clothing/glasses/thermal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/_glasses.dm#L585), [`/obj/item/clothing/glasses/science`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/_glasses.dm#L137), [`/obj/item/clothing/glasses/meson/engine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/engine_goggles.dm#L15)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/vision.rs` (`worn_vision`: `color_cutoffs`, `lighting_cutoff`, `TRAIT_MESON_VISION`/`THERMAL`/`XRAY`/`TRUE_NIGHT` из `update_sight`), `crates/ss13_net/src/replication_server.rs` (частичные `Sight`: мезоны — турфы, термальные — мобы за стенами, рентген — всё), `crates/ss13_render/src/lighting` (`VisionLighting`: `light_cutdown`/`light_cutup` на плане света, показ присланного за линией взгляда), `crates/ss13_game/src/clothing/gear.rs` (кнопка ночного режима, `..._off`-спрайт, `flash_protect`); научные — `DmClothing::reagent_scanner`, как было; нет цвета линз (`wearable_client_colour`: в tg выключен, включается right-alt-кликом), `TRAIT_MATERIAL_VISON` и режимов инженерных очков (T-ray, радиация, давление: `engine_goggles.dm`)

### HUD-очки (медицинские, диагностические)
Медицинский HUD показывает здоровье и статус над людьми, диагностический — состояние роботов и мехов; совмещённые варианты с ПНВ и солнечными очками. HUD СБ — см. ../security/.

- **Код:** [`/obj/item/clothing/glasses/hud`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/hud.dm#L1), [`/obj/item/clothing/glasses/hud/health`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/hud.dm#L57), [`/obj/item/clothing/glasses/hud/diagnostic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/hud.dm#L119)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/records/hud.rs`: медицинские и охранные HUD-очки (по типу, включая medsechud) дают свой data HUD; нет диагностического HUD, переключаемых и ночных вариантов как отдельной механики

### Корректирующие и прочие очки
Очки по рецепту (убирают близорукость), монокль, повязка на глаз, цветные очки, психоделические и т.п.

- **Код:** [`/obj/item/clothing/glasses/regular`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/_glasses.dm#L322), [`/obj/item/clothing/glasses/eyepatch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/_glasses.dm#L195), [`/obj/item/clothing/glasses/monocle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/_glasses.dm#L280)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: надеваются; нет близорукости и её коррекции

### Изолированные перчатки
Жёлтые перчатки (`siemens_coefficient` 0) защищают от удара током; дешёвые подделки случайно, отрезанные пальцы — нет.

- **Код:** [`/obj/item/clothing/gloves/color/yellow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/gloves/insulated.dm#L6), [`/obj/item/clothing/gloves/color`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/gloves/insulated.dm#L1), [`/obj/item/clothing/gloves`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/gloves/_gloves.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_mapload/src/clothing.rs` (`siemens_coefficient`), `crates/ss13_game/src/wires.rs`: удар током от шлюзов гасится перчатками

### Перчатки с особыми свойствами
Боевые перчатки (изоляция и жар), латексные и нитриловые (быстрая хирургия), ботанические, перчатки без пальцев, боксёрские (несмертельный урон), перчатки-таклеры (прыжок-захват), перчатки «Северной звезды» (быстрые удары), грузовые рукавицы, пробивные рукавицы, перчатки плазменов.

- **Код:** [`/obj/item/clothing/gloves/combat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/gloves/combat.dm#L1), [`/obj/item/clothing/gloves/latex`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/gloves/special.dm#L109), [`/obj/item/clothing/gloves/boxing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/gloves/boxing.dm#L1), [`/obj/item/clothing/gloves/tackler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/gloves/tacklers.dm#L1), [`/obj/item/clothing/gloves/rapid`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/gloves/special.dm#L53), [`/obj/item/clothing/gloves/cargo_gauntlet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/gloves/special.dm#L2), [`/obj/item/clothing/gloves/fingerless/punch_mitts`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/gloves/punch_mitts.dm#L1), [`/obj/item/clothing/gloves/color/plasmaman`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/gloves/plasmaman.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: надеваются, изоляция по `siemens_coefficient`; нет таклинга, боксёрского урона, ускорения ударов и хирургии

### Магнитные ботинки
Включаемые магбутсы: не дают сдуть ветром и поскользнуться, держат в невесомости, замедляют при включении; продвинутые и синдикатовские варианты.

- **Код:** [`/obj/item/clothing/shoes/magboots`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/shoes/magboots.dm#L4), [`attack_self()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/shoes/magboots.dm#L63)
- **BandaStation:** [`magboots.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/shoes/magboots.dm#L72) — тише шаги магбутсов
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/clothing/gear.rs` (`toggle_magboots`: в руке и кнопкой надетых, `active_traits` = нет скольжения и `TRAIT_NEGATES_GRAVITY`, `slowdown_active`, спрайт `…1`), `crates/ss13_game/src/spacemove.rs` (`negates_gravity`: держат в невесомости), `crates/ss13_game/src/fire.rs` (ветер не сдувает); `TRAIT_NO_SLIP_SLIDE` есть данными; нет звуков шагов (`shoe_footstep`, в порту шагов нет), зелья скорости, верба Toggle Magboots (кнопка вместо него)

### Обувь с особыми свойствами
Галоши (не скользят), прыжковые ботинки и реактивные (рывок), роликовые и лыжи (`wheelys`), клоунские ботинки (пищат, банановые оставляют кожуру), ковбойские сапоги (прячут предмет, змеи), сапоги СБ и зимние, ботинки-пушки.

- **Код:** [`/obj/item/clothing/shoes/galoshes`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/shoes/galoshes.dm#L1), [`/obj/item/clothing/shoes/bhop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/shoes/jumpboots.dm#L1), [`/obj/item/clothing/shoes/wheelys`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/shoes/wheelys.dm#L1), [`/obj/item/clothing/shoes/clown_shoes`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/shoes/clown.dm#L1), [`/obj/item/clothing/shoes/clown_shoes/banana_shoes`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/shoes/bananashoes.dm#L3), [`/obj/item/clothing/shoes/cowboy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/shoes/cowboy.dm#L1), [`/obj/item/clothing/shoes/jackboots`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/shoes/boots.dm#L45), [`/obj/item/clothing/shoes/gunboots`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/shoes/gunboots.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: надеваются; нет скольжения вообще, рывков, роликов, звуков шагов

### Шнурки
Шнурки можно развязать или связать между собой у чужих ботинок — носитель спотыкается; завязывание занимает время.

- **Код:** [`/obj/item/clothing/shoes`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/shoes/_shoes.dm#L1), [`adjust_laces()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/shoes/_shoes.dm#L148), [`check_trip()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/shoes/_shoes.dm#L236)
- **Статус в ss13-rs:** ❌ нет

### Беруши и наушники
Наушники-беруши защищают от оглушения громкими звуками (флешбанг, хонк).

- **Код:** [`/obj/item/clothing/ears/earmuffs`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/ears/_ears.dm#L13)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: слот ушей (занят гарнитурой); нет защиты от звука (нет звуков и флешбангов)

### Галстуки, шарфы, плащи и прочее на шею
Галстуки (завязываются), бабочки, шарфы, плащи глав отделов, ожерелья, ошейники для питомцев, бусы, стетоскоп (слушать сердце и сейфы).

- **Код:** [`/obj/item/clothing/neck`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/neck/_neck.dm#L1), [`/obj/item/clothing/neck/tie`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/neck/_neck.dm#L50), [`/obj/item/clothing/neck/scarf`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/neck/_neck.dm#L381), [`/obj/item/clothing/neck/cloak`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/cloaks.dm#L3), [`/obj/item/clothing/neck/stethoscope`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/neck/_neck.dm#L234), [`/obj/item/clothing/neck/petcollar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/neck/_neck.dm#L495), [`/obj/item/clothing/neck/necklace/pearl`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/neck/pearl.dm#L5)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: слот шеи; нет завязывания галстука и стетоскопа

### Ошейник-бомба
Взрывной ошейник с пультом: нельзя снять, по кнопке взрывается.

- **Код:** [`/obj/item/clothing/neck/collar_bomb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/neck/collar_bomb.dm#L2), [`/obj/item/collar_bomb_button`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/neck/collar_bomb.dm#L89)
- **Статус в ss13-rs:** ❌ нет
