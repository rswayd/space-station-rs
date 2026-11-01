# Маг
Маг Федерации волшебников: книга заклинаний, логово на Wizard Den, Великий ритуал; ученики и академия. Сами заклинания — в [spells.md](spells.md).

### Антагонист «Маг» и команда
`/datum/antagonist/wizard`: выбор имени, телепорт с логова на станцию, снаряжение (роба, шляпа, посох-книга), цели; ученики, самозванцы, маги академии, миньоны.

- **Код:** [`/datum/antagonist/wizard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/wizard.dm#L4), [`/datum/team/wizard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/wizard.dm#L113), [`/datum/antagonist/wizard/apprentice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/wizard.dm#L265), [`/datum/antagonist/wizard/apprentice/imposter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/wizard.dm#L345), [`/datum/antagonist/wizard/academy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/wizard.dm#L382), [`/datum/antagonist/wizard_minion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/wizard.dm#L32)
- **Статус в ss13-rs:** ❌ нет — рулсет `Roundstart Wizard` в `crates/ss13_game/src/antag/dynamic.rs` никого не спавнит; карты Wizard Den нет

### Книга заклинаний
TGUI `Spellbook`: 10 очков на заклинания и артефакты, возврат покупок на логове, категории (нападение, защита, мобильность, помощь, перки, призывы, испытания).

- **Код:** [`/obj/item/spellbook`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/wizard_spellbook.dm#L1), [`/datum/spellbook_entry`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/_entry.dm#L10); tgui: [`Spellbook.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Spellbook/index.tsx)
- **Статус в ss13-rs:** ❌ нет

### Атакующие записи книги
Огненный шар, карты заклинаний, форма жезла, Smite, призыв обезьян, слепота, завязать шнурки, мутация, плоть в камень, удар Теслы, молния, пушки, арканный залп, проклятие скотного двора, кровавые заклинания, посохи хаоса/перемен/сжатия, Мьёльнир, молот сингулярности, клинок заклинаний, высокочастотный клинок.

- **Код:** [`offensive.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/offensive.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Защитные записи книги
Магическая ракета, отключение техники, отталкивание, остановка времени, дым, силовая стена, связать душу (лич), Chuuni, искажение пространства, ловушки, пчёлы, проклятая сумка, посохи лечения и шкафчика, шар провидения, жезлы, мастерская броня.

- **Код:** [`defensive.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/defensive.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Мобильность и помощь
Обмен разумом, Knock, Blink, телепорт, эфирный джаунт, обмен местами, свисток-варп, посох дверей, телепорт-жезл, «Покинуть тело»; призыв предмета, перезарядка, оборотничество, Soul Tap, посох анимации, камни душ, некромантский камень, контракт ученика, колода стражей, бутылки демонов, скипетр вендомантии.

- **Код:** [`mobility.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/mobility.dm#L1), [`assistance.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/assistance.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Перки, призывы и испытания
Перки (рождённый червём, дежавю, лотерея, азарт, пожиратель сердец, друзья-слаймы, прозрачность, магнетизм); глобальные призывы (призраки, оружие, магия, события, безумие, массовое обучение); испытания (мультивселенский меч, анти-маг).

- **Код:** [`/datum/spellbook_entry/perks`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/perks.dm#L3), [`/datum/spellbook_entry/summon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/_entry.dm#L242), [`/datum/spellbook_entry/challenge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/spellbook_entries/_entry.dm#L257)
- **Статус в ss13-rs:** ❌ нет

### Артефакты мага
Разрыватель покрова, шар провидения, некромантский камень, проклятое сердце, свисток-варп, скипетр вендоров, телепорт-жезл, молоты, мантии и шляпы (разные цвета, фальшивые).

- **Код:** [`/obj/item/veilrender`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/artefact.dm#L6), [`/obj/item/scrying`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/artefact.dm#L165), [`/obj/item/necromantic_stone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/artefact.dm#L210), [`/obj/item/warp_whistle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/artefact.dm#L320), [`/obj/item/runic_vendor_scepter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/artefact.dm#L398), [`/obj/item/teleport_rod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/teleport_rod.dm#L3), [`/obj/item/mjollnir`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/wizard_hammer.dm#L73), [`/obj/item/clothing/suit/wizrobe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/equipment/wiz_robe.dm#L114)
- **Статус в ss13-rs:** ❌ нет

### Великий ритуал
Маг рисует руны в случайных зонах станции; каждая руна даёт побочные эффекты (аномалии, ЭМИ, перемешивание полов, оркестр, дождь еды...); седьмая — финал на выбор: бессмертие экипажа, всеобщий доступ, Мидас, сыр, клоун-мир, узурпация капитанства, армагеддон.

- **Код:** [`/datum/action/cooldown/grand_ritual`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/grand_ritual/grand_ritual.dm#L23), [`/obj/effect/grand_rune`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/grand_ritual/grand_rune.dm#L13), [`/datum/grand_side_effect`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/grand_ritual/grand_side_effect.dm#L4), [`/datum/grand_finale`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/grand_ritual/finales/grand_ritual_finale.dm#L4), [`/datum/grand_finale/immortality`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/grand_ritual/finales/immortality.dm#L10), [`/datum/grand_finale/armageddon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/grand_ritual/finales/armageddon.dm#L6)
- **Статус в ss13-rs:** ❌ нет

### Демоны резни и смеха, бесёнок
Антаг-датумы демонов из бутылки (кровавый джаунт, поедание тел; демон смеха возвращает съеденных) и бесёнка.

- **Код:** [`/datum/antagonist/slaughter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/slaughter_antag.dm#L1), [`/datum/antagonist/slaughter/laughter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/slaughter_antag.dm#L68), [`/datum/antagonist/imp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wizard/imp_antag.dm#L7)
- **Статус в ss13-rs:** ❌ нет
