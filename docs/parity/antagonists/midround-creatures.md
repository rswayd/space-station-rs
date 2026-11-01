# Мидраунд-существа из призраков
Одиночные антагонисты-существа, которых Dynamic спавнит из призраков: кошмар, космический дракон, ревенант, морф, демон резни, войдволкер, кровяные черви.

### Кошмар
Теневое существо: регенерирует во тьме, сердце-кошмар воскрешает, «пожиратель света» уничтожает источники света, теневой шаг.

- **Код:** [`/datum/antagonist/nightmare`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nightmare/nightmare.dm#L1), [`/datum/species/shadow/nightmare`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nightmare/nightmare_species.dm#L4), [`/obj/item/light_eater`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nightmare/nightmare_equipment.dm#L4), [`/obj/item/organ/heart/nightmare`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nightmare/nightmare_organs.dm#L76)
- **Статус в ss13-rs:** ❌ нет

### Космический дракон
Дракон призывает разрывы карпов (3 из 4 — победа); каждый разрыв выпускает карпов-игроков; объявление ЦК; смерть разрывов ослабляет дракона.

- **Код:** [`/datum/antagonist/space_dragon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/space_dragon/space_dragon.dm#L1), [`/mob/living/basic/space_dragon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/space_dragon/space_dragon.dm#L11), [`/obj/structure/carp_rift`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/space_dragon/carp_rift.dm#L110), [`/datum/action/innate/summon_rift`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/space_dragon/carp_rift.dm#L8), [`/datum/antagonist/space_carp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/space_dragon/space_carp.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Ревенант
Невидимый дух, питающийся эссенцией умирающих: способности (оглушение светом, порча техники, осквернение, заражение болезнью «revblight», одержимые предметы); раскрывается при использовании.

- **Код:** [`/datum/antagonist/revenant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/revenant/revenant_antag.dm#L1), [`/mob/living/basic/revenant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/revenant/_revenant.dm#L7), [`/datum/disease/revblight`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/revenant/revenant_blight.dm#L1), [`/datum/component/haunted_item`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/revenant/haunted_item.dm#L3), [`/datum/component/revenant_ability`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/revenant/revenant_skill.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Морф
Существо-оборотень: маскируется под любые предметы и мобов, поедает трупы и предметы.

- **Код:** [`/datum/antagonist/morph`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/morph/morph_antag.dm#L1), [`/mob/living/basic/morph`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/morph.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Демон резни
Кровавое ползание через лужи крови, утаскивание и поедание тел; демон смеха возвращает съеденных живыми; антаг-датум — в [wizard.md](wizard.md).

- **Код:** [`/mob/living/basic/demon/slaughter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/demon/demon_subtypes.dm#L2), [`/mob/living/basic/demon/slaughter/laughter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/demon/demon_subtypes.dm#L120)
- **Статус в ss13-rs:** ❌ нет

### Войдволкер и санволкер
Космическое существо, проходящее сквозь окна-пустоту, похищает людей в Пустоту, оставляя травму «voided»; санволкер — вариант с рывком и аллергией на планеты.

- **Код:** [`/datum/antagonist/voidwalker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/voidwalker/voidwalker.dm#L2), [`/mob/living/basic/voidwalker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/voidwalker/voidwalker_mob.dm#L5), [`/datum/brain_trauma/voided`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/voidwalker/voidwalker_traumas.dm#L2), [`/datum/antagonist/sunwalker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/voidwalker/sunwalkers.dm#L2), [`/mob/living/basic/voidwalker/sunwalker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/voidwalker/sunwalker_mob.dm#L2), [`/obj/item/void_eater`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/voidwalker/voidwalker_loot.dm#L51)
- **Статус в ss13-rs:** ❌ нет

### Кровяные черви
Черви-паразиты (детёныш, молодой, взрослый): вселяются в носителя, высасывают кровь, плюются; цели — убить, поглотить, размножиться, захватить станцию; раундстарт- и мидраунд-рулсеты; тестер крови.

- **Код:** [`/datum/antagonist/blood_worm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blood_worm/blood_worm_antag.dm#L1), [`/datum/team/blood_worm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blood_worm/blood_worm_team.dm#L1), [`/mob/living/basic/blood_worm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blood_worm/blood_worm_mob.dm#L1), [`/mob/living/blood_worm_host`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blood_worm/blood_worm_host_mob.dm#L1), [`/datum/objective/blood_worm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blood_worm/blood_worm_objectives.dm#L1), [`/obj/item/blood_worm_tester`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blood_worm/blood_worm_tester.dm#L1)
- **Статус в ss13-rs:** ❌ нет
