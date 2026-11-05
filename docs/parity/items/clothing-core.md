# Одежда: общие механики
Базовое поведение `/obj/item/clothing`: слоты, броня, защита от среды, переключаемые части, повреждения и маскировка.

### Надевание одежды по слотам
Одежда надевается в свой слот человека (униформа, костюм, голова, маска, глаза, уши, шея, перчатки, обувь, пояс, спина, ID, карманы, хранилище костюма); снятие — клик пустой рукой по слоту. Правила `can_equip`: карманы и ID требуют униформу, пояс — униформу, хранилище костюма — костюм и тип из его `allowed`.

- **Код:** [`/obj/item/clothing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/clothing.dm#L1), [`equipped()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/clothing.dm#L257), [`dropped()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/clothing.dm#L240)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: все 17 слотов, правила `can_equip`, `allowed` для хранилища костюма (`ANY_SUIT_STORAGE`); нет `equip_delay_self`/`strip_delay` для скафандров, нет `TRAIT_NODROP`

### Отрисовка одетого (worn-спрайты, GAGS, женские версии)
Надетая вещь рисуется на мобе из `worn_icon`/`worn_icon_state`; GAGS-вещи перекрашиваются по конфигу, униформы на женском теле обрезаются маской, на одежде видна кровь и повреждения.

- **Код:** [`update_greyscale()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/clothing.dm#L276), [`generate_female_clothing()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/clothing.dm#L558), [`get_blood_overlay()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/clothing.dm#L669)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/human.rs`, `crates/ss13_game/src/inventory.rs`: worn-спрайты из `worn_icon`; нет GAGS-спрайтов на мобе, женской маски униформы, крови на одежде

_«Броня одежды» описано в [../combat/armor.md](../combat/armor.md)._

### Защита от давления и температуры
Флаг `STOPSPRESSUREDAMAGE` на костюме и шлеме защищает от вакуума и высокого давления; `cold_protection`/`heat_protection` с `min_cold_protection_temperature`/`max_heat_protection_temperature` защищают покрытые зоны от холода и жары.

- **Код:** [`/obj/item/clothing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/clothing.dm#L1) (`clothing_flags`, `cold_protection`, `heat_protection`)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_mapload/src/clothing.rs` (`DmClothing`), `crates/ss13_game/src/breath.rs`: давление по груди и голове, теплоизоляция по зонам

### Сокрытие слотов (`flags_inv`, `flags_cover`)
Шлемы скрывают маску, глаза, уши и волосы, костюмы — униформу и перчатки; `flags_cover` закрывает рот и глаза от еды, брызг и пепперспрея.

- **Код:** [`/obj/item/clothing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/clothing.dm#L1) (`flags_inv`, `flags_cover`)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/clothing/cover.rs`: `obscured_slots` из живых `flags_inv` надетого (без карманов); скрытые слоты не рисуются (`crate::inventory`), волосы и глаза прячутся (`HIDEHAIR`, `HIDEEYES`), осмотр их не показывает, скрытое лицо даёт имя с ID или «Неизвестный»; `flags_cover`: `is_mouth_covered` (еда и питьё, `crate::food`, `crate::chem`), `is_eyes_covered`, `is_pepper_proof` как API. Меню раздевания скрытые слоты пока не прячет (см. `crate::strip`)

### Замедление от одежды
`slowdown` вещи прибавляется к задержке шага (тяжёлые скафандры, магбутсы во включённом состоянии, некоторые костюмы); часть вещей снижает замедление.

- **Код:** [`slowdown`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items.dm#L150)
- **Статус в ss13-rs:** ❌ нет — скорость мобов (`crates/ss13_game/src/health.rs`) не учитывает `slowdown` одежды

### Регулируемые маски и визоры
Маски, сварочные щитки и шлемы с визором сдвигаются вверх/вниз (`adjust_visor`), меняя `flags_inv`, `flags_cover`, защиту от вспышек и работу с баллоном.

- **Код:** [`adjust_visor()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/clothing.dm#L567), [`visor_toggling()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/clothing.dm#L604)
- **BandaStation:** [`_masks.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/_masks.dm#L25) — хирургическая маска не регулируется (`can_be_adjusted`)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/clothing/toggles.rs`: маски с действием «Adjust» (дыхательные, стерильные, балаклавы, хейлер) сдвигаются в руке, кнопкой действия надетыми и alt-кликом (дыхательные): `visor_flags`/`visor_flags_inv`/`visor_flags_cover`, состояние `_up`, `MASKINTERNALS` пропадает (баллон закрывается следующим вдохом); сварочные щитки (`crates/ss13_game/src/vision.rs`) тоже переключают `visor_flags*`. Нет: шлемов с визором (`helmet/toggleable`, космические), `adjusted_flags` (бандана), звуков

### Капюшоны и переключаемые костюмы
`/obj/item/clothing/suit/hooded` создаёт капюшон, который надевается кнопкой действия; `/obj/item/clothing/suit/toggle` расстёгивается/застёгивается (меняет спрайт).

- **Код:** [`/obj/item/clothing/suit/hooded`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/toggles.dm#L3), [`/obj/item/clothing/suit/toggle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/toggles.dm#L63), [`/obj/item/clothing/head/hooded`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/toggles.dm#L72)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/clothing/toggles.rs`: у надетого костюма с `hoodtype` кнопка «Toggle Hood» — капюшон надевается на пустую голову, костюм берёт `hood_up_affix`; снятие капюшона или костюма убирает капюшон; `/obj/item/clothing/suit/toggle` расстёгивается alt-кликом (`_t`). Нет авторазворачивания капюшона нарядом (`auto_deploy_on_outfit_equip`)

### Повреждение и починка одежды
Одежда получает урон по зонам: при износе зона «рвётся» и перестаёт защищать, вещь может развалиться на лоскуты; ткань или нитки чинят её. Моли поедают одежду.

- **Код:** [`take_damage_zone()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/clothing.dm#L176), [`disable_zone()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/clothing.dm#L200), [`repair()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/clothing.dm#L151), [`/obj/item/food/clothing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/clothing.dm#L82)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/clothing/damage.rs`: режущий и жгущий удар, бросающий на ранение (`crate::medical::wounds::after_hit`), бьёт по зоне одежды с `limb_integrity`; зона отключается (перестаёт покрывать часть тела; баг tg с `NO_ZONE_DISABLING` не перенесён), имя «damaged/mangy/tattered», при `integrity_failure` — «распадается», без покрытия или целостности — «shredded»: спадает и не надевается; ткань чинит (1 лист сразу, 3 листа за 6 с у изорванной), осмотр по зонам, оверлей `damageduniform`/`damagedmask` на мобе. Нет: оверлея повреждения на иконке предмета, моли (нет вида)

### Трейты одежды
Вещи выдают трейты носителю (`clothing_traits`): иммунитет к скольжению, тихие шаги, защита от прослушки и т.п.

- **Код:** [`attach_clothing_traits()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/clothing.dm#L293)
- **Статус в ss13-rs:** ❌ нет

### Хамелеон-одежда
Агентская одежда с действием «сменить облик»: копирует вид любой вещи своего слота, полный комплект меняется разом; ЭМИ сбивает облик. Сканер хамелеона запоминает наряды людей.

- **Код:** [`/datum/action/item_action/chameleon/change`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/chameleon/_chameleon_action.dm#L6), [`code/modules/clothing/chameleon`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/chameleon), [`chameleon_scanner.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/chameleon/chameleon_scanner.dm)
- **Статус в ss13-rs:** ❌ нет — в каталоге аплинка (`crates/ss13_mapload/src/antag.rs`) хамелеон не продаётся

### Пояс-полиморф
Аномальный пояс: после сканирования существа превращает носителя в него.

- **Код:** [`/obj/item/polymorph_belt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/belts/polymorph_belt.dm#L2)
- **Статус в ss13-rs:** ❌ нет

_«Наборы снаряжения (`/datum/outfit`)» описано в [../round/outfits.md](../round/outfits.md)._

### Меню перекраски GAGS
`/datum/greyscale_modify_menu`: окно выбора цветов GAGS-предмета (предпросмотр, слои), которым торговые автоматы перекрашивают выдаваемые `colorable` товары, а админы — любые GAGS-атомы.

- **Код:** [`/datum/greyscale_modify_menu`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/greyscale_modify_menu.dm#L3), [`_vend_greyscale()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/ui_data.dm#L178)
- **Статус в ss13-rs:** ❌ нет — автоматы выдают товары в цветах по умолчанию (`colorable` всегда false)
