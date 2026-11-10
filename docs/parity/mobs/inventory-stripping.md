# Инвентарь, слоты экипировки, хранилища и раздевание
Руки, слоты одежды человека, механика `/datum/storage` и меню раздевания. Конкретные предметы и контейнеры — у агента items.

### Руки: подбор, бросание, смена руки
Предмет подбирается кликом пустой рукой, кладётся на стол/пол кликом с предметом, роняется (Q), активная рука меняется (X). У карбонов может быть произвольное число рук (по конечностям); без руки предмет не взять.

- **Код:** [`proc/put_in_hands()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/inventory.dm#L293), [`proc/put_in_active_hand()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/inventory.dm#L281), [`proc/dropItemToGround()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/inventory.dm#L409), [`proc/transferItemToLoc()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/inventory.dm#L444), [`proc/swap_hand()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob.dm#L954)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: две руки, подбор, бросание, смена руки, кладка на тайл; нет зависимости рук от конечностей, случайного пиксельного смещения при бросании

### Слоты экипировки человека
Слоты головы, глаз, ушей, маски, шеи, униформы, костюма, перчаток, обуви, пояса, ID, спины, карманов, хранилища костюма; правила `can_equip` (нужна униформа для пояса/карманов/ID, `slot_flags`, вид); экипировка и снятие кликом по слоту HUD.

- **Код:** [`proc/can_equip()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/inventory.dm#L227), [`human/can_equip()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/inventory.dm#L18), [`human/equip_to_slot()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/inventory.dm#L122), [`human/get_item_by_slot()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/inventory.dm#L27), [`proc/equip_to_slot_if_possible()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/inventory.dm#L576)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/inventory.rs`

### Задержка надевания
Некоторые предметы (скафандры, тяжёлая броня) надеваются через `do_after` (`equip_delay_self`), на другого — через `equip_delay_other`.

- **Код:** [`proc/can_equip()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/inventory.dm#L227)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/strip.rs`: надевание на другого через меню раздевания ждёт `equip_delay_other` (значение tg по умолчанию, 2 с, карманы 1 с); нет `equip_delay_self` (`crates/ss13_game/src/inventory.rs`, shortcut) и задержек конкретных типов

### Быстрая экипировка
Клавиша quick-equip надевает предмет в подходящий слот или кладёт в рюкзак/пояс; «умная» экипировка достаёт предмет из пояса/рюкзака.

- **Код:** [`verb/quick_equip`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/inventory.dm#L706), [`proc/equip_to_appropriate_slot()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/inventory.dm#L634), [`proc/smart_equip_targeted()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/inventory.dm#L385), [`/datum/keybinding/human/quick_equip`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/human.dm#L8)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/inventory.rs` (`quick_equip`, `smart_equip`), клавиши `crates/ss13_game/src/inventory_client.rs` (E, Shift+E, Shift+V, Shift+Q, Alt+1, Alt+2): предмет в первый подходящий слот, затем в открытое хранилище (`active_storage`) и надетые хранилища в порядке tg; умная экипировка кладёт в слот или достаёт последнее из него. `equip_delay_self` общий с записью «Задержка надевания» выше; `slot_equipment_priority` отдельных предметов не перенесён

### Скрытые слоты и отображение надетого
Шлемы и костюмы скрывают слоты под ними (`flags_inv`, `obscured_slots`): маску, глаза, униформу — при осмотре и раздевании; спрайт человека собирается из оверлеев надетых предметов.

- **Код:** [`human_update_icons.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/human_update_icons.dm#L1)
- **BandaStation:** [`code/modules/mob/living/carbon/human/human_update_icons.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/human_update_icons.dm#L124) — правки отрисовки под виды форка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/human.rs`, `crates/ss13_game/src/inventory.rs`: оверлеи надетого; нет скрытия слотов, GAGS-спрайтов, обрезки униформы на женском теле

_«Хранилища (/datum/storage)» описано в [../items/storage.md](../items/storage.md)._

### Меню раздевания (strip menu)
Перетаскивание человека на себя открывает tgui-меню его слотов: снять или надеть вещь через задержку (`strip_delay`), обыскать карманы (скрытно, с шансом заметить), снять наручники, переключить internals, вытащить ID.

- **Код:** [`/datum/element/strippable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/strippable.dm#L2), [`/datum/strippable_item`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/strippable.dm#L73), [`human_stripping.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/human_stripping.dm#L1), [`carbon_stripping.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon_stripping.dm#L1); tgui: [`StripMenu.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/StripMenu.tsx)
- **BandaStation:** [`code/modules/mob/living/carbon/human/human_stripping.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/human_stripping.dm#L232) — головной карман скреллов (`/datum/strippable_item/headpocket`)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/strip.rs` (окно `StripMenu`, `panels_client.rs`), перетаскивание — `crates/ss13_game/src/drag.rs`: меню открывается перетаскиванием человека на себя, снятие и надевание вещей во все слоты и руки через `do_after`, карманы скрыты (4 с / 1 с), снятие наручников, закрытие окна вне досягаемости; задержки — значения tg по умолчанию (4 с / 2 с), без `strip_delay` конкретных типов; нет альтернативных действий (internals, датчики, шнурки), скрытия слотов одеждой, ног-наручников, головного кармана скреллов

### Передача предмета (offer)
Предмет можно предложить соседу (ctrl+shift-клик/клавиша): тот получает оповещение и берёт его кликом.

- **Код:** [`/datum/status_effect/offering`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L207)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/buffs.rs` (`give`, `take`, `give_key`), `crates/ss13_game/src/click.rs` (`GiveClick`), клавиша V — `crates/ss13_game/src/inventory_client.rs` (`ClientIntent::GiveItem`): Ctrl+Shift-клик предлагает соседу, V — всем вокруг (`orange(1)`, годные из `is_taker_elligible`), оповещение и взятие кликом, проверки дистанции и состояния. «Дай пять» и «помочь подняться» — в записи [status-effects-buffs.md](status-effects-buffs.md#передача-предмета-и-дай-пять)

### Двуручные предметы
Компонент `two_handed`: предмет нужно держать двумя руками (wield) для полной силы или вообще для использования.

- **Код:** [`/datum/component/two_handed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/twohanded.dm#L7)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/weapons/two_handed.rs`: wield/unwield, сила в двух руках (значения и множители предмета), `/obj/item/offhand` с блокировкой смены руки, `require_twohands` (падает, если нельзя удержать двумя), звуки, спрайты `[base]0/1`, заточка; перечень двуручных предметов — `SPECS`. Нет суффикса « (Wielded)» в имени, `TRAIT_NO_TWOHANDING` и фэнтези/материальных бонусов (эти механизмы не перенесены)
