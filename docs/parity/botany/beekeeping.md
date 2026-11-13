# Пчеловодство
Ульи, рамки, соты, дымарь и защитный костюм пчеловода. Сами пчёлы и пчеломатка — мобы, см. раздел мобов.

### Улей
Улей с пчеломаткой (вставляется предметом `/obj/item/queen_bee`) и рамками; пчёлы вылетают опылять лотки, набирают реагент последнего опылённого растения и производят соты в рамках; число пчёл и сотов ограничено числом рамок. Тревожащий улей злит пчёл (без костюма пчеловода).

- **Код:** [`/obj/structure/beebox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/beebox.dm#L8), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/beebox.dm#L69), [`item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/beebox.dm#L128), [`interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/beebox.dm#L174), [`beebox/premade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/beebox.dm#L35), [`/obj/item/queen_bee`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/bee/_bee.dm#L242)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/botany/beekeeping.rs` (`Beebox`: рамки, королева, пчёлы, ресурсы; `process_hives` каждые 2 с — соты за 100 ресурсов, новые пчёлы; рамка/предмет-королева рукой, несовместимые по реагенту пчёлы выселяются; пустая рука без костюма выпускает рабочих, с костюмом — выбор «рамка/королева»; premade и random-улья; `/obj/item/queen_bee/bought`), `crates/ss13_game/src/mobs/bees.rs` + `mobs/ai/trees/space.rs` (поиск улья `find_hive`, заселение, влёт 15%/с (королева 85%), вылет 35%/с (королева 5%), опыление приносит ресурсы, агрессия только у улья в радиусе 2). Нет: закрепления ключом и разборки на доски, маточного молочка и мутации королевы шприцем, команд пчёлам (pet commands); королева в предмете хранится как реагент (новый моб при посадке)

### Рамки и соты
Рамки вставляются в улей; извлечённые соты — еда с мёдом и реагентом пчёл, перерабатываются в мёд и воск.

- **Код:** [`/obj/item/honey_frame`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/honey_frame.dm#L2), [`/obj/item/food/honeycomb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/honeycomb.dm#L1), [`set_reagent()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/honeycomb.dm#L26)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/botany/beekeeping.rs`: до 3 рамок по 10 сот, снятие рамки соскребает до 10 сот на пол; соты (`/obj/item/food/honeycomb`, мёд 5 ед.) с реагентом королевы получают имя «(реагент)», +5 ед. реагента и цветной оверлей `greyscale_honey` (`set_comb_reagent`). Переработка в мёд/воск — общая механика измельчителя, не здесь

### Дымарь
Включается в руке, тратит топливо (заправляется растениями/бумагой); дым по улью или пчёлам успокаивает их, позволяя забрать соты без нападения.

- **Код:** [`/obj/item/bee_smoker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/bee_smoker.dm#L4), [`interact_with_atom()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/bee_smoker.dm#L43), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/bee_smoker.dm#L96)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/botany/beekeeping.rs`: включение в руке (без топлива — «нет топлива!»), 1 топливо за тик SSobj, применение по улью/пчеле/полу за 5 топлива делает пчёл на тайле (или всех пчёл улья) друзьями пользователя (`pets::befriend` → пчёлы не жалят), заправка коноплёй на `wine_power × 0.2`. Нет: частиц дыма, звукового цикла и эффекта пыли

### Костюм пчеловода
Шляпа и костюм пчеловода защищают от укусов пчёл (толстая ткань, `THICKMATERIAL`).

- **Код:** [`/obj/item/clothing/head/utility/beekeeper_head`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/beekeeper_suit.dm#L2), [`/obj/item/clothing/suit/utility/beekeeper_suit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/beekeeper_suit.dm#L16)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_mapload` (`dm_clothing`), `crates/ss13_game/src/inventory.rs`: надевается как одежда; пчёлы не нападают на того, у кого костюм и шляпа с `THICKMATERIAL` (`bee_friendly` в `crates/ss13_game/src/mobs/bees.rs`)
