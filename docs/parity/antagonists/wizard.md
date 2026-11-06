# Маг
Маг Федерации волшебников: книга заклинаний, логово на Wizard Den, Великий ритуал; ученики и академия. Сами заклинания — в [spells.md](spells.md).

### Антагонист «Маг» и команда
`/datum/antagonist/wizard`: выбор имени, телепорт с логова на станцию, снаряжение (роба, шляпа, посох-книга), цели; ученики, самозванцы, маги академии, миньоны.

- **Код:** [`/datum/antagonist/wizard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/wizard.dm#L4), [`/datum/team/wizard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/wizard.dm#L113), [`/datum/antagonist/wizard/apprentice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/wizard.dm#L265), [`/datum/antagonist/wizard/apprentice/imposter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/wizard.dm#L345), [`/datum/antagonist/wizard/academy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/wizard.dm#L382), [`/datum/antagonist/wizard_minion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/wizard.dm#L32)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/wizard/server.rs`: маг (раундстарт `Roundstart Wizard` без станционной должности и мидраунд `Midround Wizard` из призраков, `SS13_DEV_TRAITOR=wizard`), логово `wizard_den.dmm` грузится со станцией на свой уровень, если все его типы сконвертированы, иначе появление у латджойна; раздевание и `/datum/outfit/wizard`, человек, цели `create_objectives`, приветствие, HUD, панель `AntagInfoGeneric`, смена имени (случайное из `wizardfirst/second.txt`), отчёт в конце раунда с покупками; ученик по контракту (школы разрушения, блюспейса, лечения, без мантии — без обмена разумом), первый призрак с `Wizard (Midround)` (опроса нет); самозванцы, маги академии, миньоны, командный HUD — нет

### Книга заклинаний
TGUI `Spellbook`: 10 очков на заклинания и артефакты, возврат покупок на логове, категории (нападение, защита, мобильность, помощь, перки, призывы, испытания).

- **Код:** [`/obj/item/spellbook`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/wizard_spellbook.dm#L1), [`/datum/spellbook_entry`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/_entry.dm#L10); tgui: [`Spellbook.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Spellbook/index.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/wizard/server.rs`, `crates/ss13_game/src/antag/wizard/client.rs`: окно `Spellbook` по категориям, 10 очков, лимит уровня, повторная покупка улучшает заклинание, возврат заклинаний только в логове (`уровень × цена`), привязка книги к владельцу; наборы, случайные книги, поиск и возврат контракта в книгу — нет

### Атакующие записи книги
Огненный шар, карты заклинаний, форма жезла, Smite, призыв обезьян, слепота, завязать шнурки, мутация, плоть в камень, удар Теслы, молния, пушки, арканный залп, проклятие скотного двора, кровавые заклинания, посохи хаоса/перемен/сжатия, Мьёльнир, молот сингулярности, клинок заклинаний, высокочастотный клинок.

- **Код:** [`offensive.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/offensive.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — огненный шар, Smite, слепота, молния, посох хаоса (случайный из перенесённых снарядов: смерть, дверь, огненный шар, воскрешение, телепорт); остальные записи (карты, форма жезла, обезьяны, шнурки, мутация, плоть в камень, удар Теслы, пушки, залп, скотный двор, кровавые заклинания, посохи перемен/сжатия, молоты, клинки) — нет

### Защитные записи книги
Магическая ракета, отключение техники, отталкивание, остановка времени, дым, силовая стена, связать душу (лич), Chuuni, искажение пространства, ловушки, пчёлы, проклятая сумка, посохи лечения и шкафчика, шар провидения, жезлы, мастерская броня.

- **Код:** [`defensive.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/defensive.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — магическая ракета, отключение техники, отталкивание, дым, силовая стена, посох лечения, набор палочек (смерть, лечение, телепорт, дверь, огненный шар; превращение и сжатие стреляют впустую); остановка времени, лич, Chuuni, искажение, ловушки, пчёлы, сумка, посох шкафчика, шар, броня — нет

### Мобильность и помощь
Обмен разумом, Knock, Blink, телепорт, эфирный джаунт, обмен местами, свисток-варп, посох дверей, телепорт-жезл, «Покинуть тело»; призыв предмета, перезарядка, оборотничество, Soul Tap, посох анимации, камни душ, некромантский камень, контракт ученика, колода стражей, бутылки демонов, скипетр вендомантии.

- **Код:** [`mobility.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/mobility.dm#L1), [`assistance.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/assistance.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — Knock, Blink, телепорт, эфирный джаунт, обмен местами, посох дверей, призыв предмета, перезарядка, контракт ученика; обмен разумом, свисток, телепорт-жезл, «Покинуть тело», оборотничество, Soul Tap, посох анимации, камни душ, некромантский камень, колода стражей, бутылки демонов, скипетр — нет

### Перки, призывы и испытания
Перки (рождённый червём, дежавю, лотерея, азарт, пожиратель сердец, друзья-слаймы, прозрачность, магнетизм); глобальные призывы (призраки, оружие, магия, события, безумие, массовое обучение); испытания (мультивселенский меч, анти-маг).

- **Код:** [`/datum/spellbook_entry/perks`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/perks.dm#L3), [`/datum/spellbook_entry/summon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/_entry.dm#L242), [`/datum/spellbook_entry/challenge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/_entry.dm#L257)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/wizard/server.rs`: ритуал Summon Events (2 очка, до 5 раз, «Провести», только в книге, подготовленной в раунде высокого хаоса — тир 4; включает режим мага, повторно ускоряет события — `crate::events::controller::summon_events`); ритуал Summon Guns (2 очка, один раз, то же условие; `crate::antag::survivalist`); перки, остальные призывы и испытания — нет

### Артефакты мага
Разрыватель покрова, шар провидения, некромантский камень, проклятое сердце, свисток-варп, скипетр вендоров, телепорт-жезл, молоты, мантии и шляпы (разные цвета, фальшивые).

- **Код:** [`/obj/item/veilrender`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/artefact.dm#L6), [`/obj/item/scrying`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/artefact.dm#L165), [`/obj/item/necromantic_stone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/artefact.dm#L210), [`/obj/item/warp_whistle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/artefact.dm#L320), [`/obj/item/runic_vendor_scepter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/artefact.dm#L398), [`/obj/item/teleport_rod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/teleport_rod.dm#L3), [`/obj/item/mjollnir`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/wizard_hammer.dm#L73), [`/obj/item/clothing/suit/wizrobe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/wiz_robe.dm#L114)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/spells/mod.rs`, `crates/ss13_game/src/spells/magic_guns.rs`: мантии и шляпы (все подтипы, включая фальшивые, как в tg) дают право колдовать; посохи и палочки (заряды, перезарядка посохов, `no_den_usage`, палочка на себя); разрыватель покрова, шар провидения, некромантский камень, проклятое сердце, свисток, скипетр, телепорт-жезл, молоты — нет

### Великий ритуал
Маг рисует руны в случайных зонах станции; каждая руна даёт побочные эффекты (аномалии, ЭМИ, перемешивание полов, оркестр, дождь еды...); седьмая — финал на выбор: бессмертие экипажа, всеобщий доступ, Мидас, сыр, клоун-мир, узурпация капитанства, армагеддон.

- **Код:** [`/datum/action/cooldown/grand_ritual`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/grand_ritual/grand_ritual.dm#L23), [`/obj/effect/grand_rune`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/grand_ritual/grand_rune.dm#L13), [`/datum/grand_side_effect`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/grand_ritual/grand_side_effect.dm#L4), [`/datum/grand_finale`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/grand_ritual/finales/grand_ritual_finale.dm#L4), [`/datum/grand_finale/immortality`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/grand_ritual/finales/immortality.dm#L10), [`/datum/grand_finale/armageddon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/grand_ritual/finales/armageddon.dm#L6)
- **Статус в ss13-rs:** ❌ нет

### Демоны резни и смеха, бесёнок
Антаг-датумы демонов из бутылки (кровавый джаунт, поедание тел; демон смеха возвращает съеденных) и бесёнка.

- **Код:** [`/datum/antagonist/slaughter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/slaughter_antag.dm#L1), [`/datum/antagonist/slaughter/laughter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/slaughter_antag.dm#L68), [`/datum/antagonist/imp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/imp_antag.dm#L7)
- **Статус в ss13-rs:** ❌ нет

### Рунический торговый автомат
Скипетр мага ставит временный рунический автомат с волшебной одеждой: при появлении отталкивает людей вокруг, через 30 с распадается, скипетр (или отвёртка) взрывает его. Общая механика автомата — ../cargo/vending.md.

- **Код:** [`/obj/machinery/vending/runic_vendor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/runic_vendor.dm#L3), [`/obj/item/runic_vendor_scepter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/artefact.dm#L398)
- **Статус в ss13-rs:** ❌ нет
