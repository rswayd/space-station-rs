# Блоб
Разумная колония-«надзиратель» (overmind), растущая по станции из ядра; цель — захватить заданное число тайлов.

### Антагонист «Блоб» и надзиратель
`/datum/antagonist/blob` и `/mob/eye/blob`: выбор места ядра, очки ресурсов, цель захвата тайлов, объявление о биоугрозе 5-го уровня; заражённый член экипажа лопается в блоба (`infection`).

- **Код:** [`/datum/antagonist/blob`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/blob_antag.dm#L1), [`/mob/eye/blob`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/overmind.dm#L16), [`/datum/objective/blob_takeover`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/blob_antag.dm#L87), [`/datum/antagonist/blob/infection`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/blob_antag.dm#L156), [`/datum/action/innate/blobpop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/blob_antag.dm#L91)
- **BandaStation:** [`overmind.dm#L364`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/overmind.dm#L364) — TTS для речи блоба; [`/mob/eye/blob`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/blob.dm#L1) — штраф вне основного этажа станции
- **Статус в ss13-rs:** ❌ нет

### Способности надзирателя
Расширение (с атакой по направлению), фабрики, ресурсные узлы, узлы, щиты/отражающие щиты, блоббернаут, перенос ядра, сбор спор, переролл штамма, удаление блоба.

- **Код:** [`/mob/eye/blob/proc/expand_blob()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/powers.dm#L278), [`/mob/eye/blob/proc/create_special()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/powers.dm#L109), [`/mob/eye/blob/proc/create_shield()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/powers.dm#L151), [`/mob/eye/blob/proc/create_blobbernaut()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/powers.dm#L174), [`/mob/eye/blob/proc/relocate_core()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/powers.dm#L227), [`/mob/eye/blob/proc/strain_reroll()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/powers.dm#L360)
- **BandaStation:** [`powers.dm#L290`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/powers.dm#L290) — переменная стоимость расширения
- **Статус в ss13-rs:** ❌ нет

### Структуры блоба
Обычный блоб, ядро (генерирует ресурсы, смерть ядра — конец), узел (расширяет зону), фабрика (споры), ресурсный блоб, щит.

- **Код:** [`/obj/structure/blob`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/structures/_blob.dm#L2), [`/obj/structure/blob/special/core`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/structures/core.dm#L1), [`/obj/structure/blob/special/node`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/structures/node.dm#L1), [`/obj/structure/blob/special/factory`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/structures/factory.dm#L1), [`/obj/structure/blob/special/resource`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/structures/resource.dm#L1), [`/obj/structure/blob/shield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/structures/shield.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Штаммы блоба
16 штаммов с реагентом и пассивом: горящее масло, криояд, пожиратель обломков, распределённые нейроны, ЭМ-паутина, энергожеле, взрывная решётка, мультиплекс, сетевые волокна, слизь под давлением, реактивные шипы, регенеративная материя, реплицирующая пена, смещающиеся фрагменты, синхронная сетка.

- **Код:** [`/datum/blobstrain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/blobstrains/_blobstrain.dm#L3), [`/datum/reagent/blob`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/blobstrains/_reagent.dm#L28), [`code/modules/antagonists/blob/blobstrains`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/blobstrains)
- **Статус в ss13-rs:** ❌ нет

### Миньоны блоба
Споры (заражают трупы в зомби-блобов), блоббернауты (игроки-призраки), зомби блоба.

- **Код:** [`/mob/living/basic/blob_minion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/blob_minions/blob_mob.dm#L2), [`/mob/living/basic/blob_minion/spore`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/blob_minions/blob_spore.dm#L4), [`/mob/living/basic/blob_minion/blobbernaut`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/blob_minions/blobbernaut.dm#L5), [`/mob/living/basic/blob_minion/zombie`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/blob_minions/blob_zombie.dm#L2), [`/datum/antagonist/blob_minion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/blob/blob_minion.dm#L1)
- **Статус в ss13-rs:** ❌ нет
