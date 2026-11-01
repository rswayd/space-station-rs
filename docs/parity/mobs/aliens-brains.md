# Ксеноморфы-карбоны и мозг-моб
Игровые ксеноморфы (`/mob/living/carbon/alien`) и моб мозга в процессоре. Антаг-датум ксеноморфов и событие — см. ../antagonists/; NPC-ксеноморфы — [basic-mobs-misc.md](basic-mobs-misc.md).

### Ксеноморфы: общая механика
Карбоны-пришельцы с плазменным резервуаром, хайвмайндом (общая речь улья), ночным зрением, иммунитетом к давлению, сильной регенерацией на траве (weeds) и слабостью к огню; видят носителей эмбрионов.

- **Код:** [`/mob/living/carbon/alien`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/alien.dm#L1), [`proc/create_internal_organs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/alien.dm#L64), [`proc/AddInfectionImages()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/alien.dm#L101), [`/obj/item/organ/alien/plasmavessel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/organs.dm#L5), [`/obj/item/organ/alien/hivenode`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/organs.dm#L104)
- **BandaStation:** [`code/modules/mob/living/carbon/alien/alien_say.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/alien_say.dm#L15) — правки речи улья
- **Статус в ss13-rs:** ❌ нет

_«Личинка и эволюция» описано в [../antagonists/xenomorphs-spiders.md](../antagonists/xenomorphs-spiders.md)._

### Касты: охотник, страж, трутень, преторианец, королева
Охотник — быстрый, прыгает и сбивает; страж — плюётся нейротоксином, маскируется; трутень — строит, сажает траву; преторианец — тяжёлый страж королевы; королева — откладывает яйца, повышает до преторианца.

- **Код:** [`/mob/living/carbon/alien/adult/hunter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/caste/hunter.dm#L1), [`/mob/living/carbon/alien/adult/sentinel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/caste/sentinel.dm#L1), [`/mob/living/carbon/alien/adult/drone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/caste/drone.dm#L1), [`/mob/living/carbon/alien/adult/royal/praetorian`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/caste/praetorian.dm#L1), [`/mob/living/carbon/alien/adult/royal/queen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/queen.dm#L55)
- **Статус в ss13-rs:** ❌ нет

### Способности ксеноморфов
Посадка травы, смола (стены, гнёзда), кислота по предметам/стенам, нейротоксин, шёпот, передача плазмы, отрыгивание, откладка яиц, поедание (devour) жертв.

- **Код:** [`/datum/action/cooldown/alien`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/alien_powers.dm#L9), [`/datum/action/cooldown/alien/make_structure/plant_weeds`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/alien_powers.dm#L105), [`/datum/action/cooldown/alien/make_structure/resin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/alien_powers.dm#L326), [`/datum/action/cooldown/alien/acid/corrosion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/alien_powers.dm#L215), [`/datum/action/cooldown/alien/acid/neurotoxin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/alien_powers.dm#L264), [`proc/devour_lad()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/adult.dm#L124)
- **BandaStation:** [`code/modules/mob/living/carbon/alien/adult/alien_powers.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/alien_powers.dm#L145) — правки способностей
- **Статус в ss13-rs:** ❌ нет

### Моб мозга и процессоры мозга (MMI, позибрейн)
Извлечённый мозг игрока живёт как `/mob/living/brain` внутри процессора мозга (органический — бывший MMI, позитронный, синдикатский); процессор вставляется в борга, меха, ИИ-ядро или тело-андроида и говорит по радио.

- **Код:** [`/mob/living/brain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/brain/brain.dm#L1), [`/obj/item/organ/brain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/brain/brain_item.dm#L1), [`/obj/item/brain_processor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/brain/processor/_processor.dm#L1), [`/obj/item/brain_processor/organic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/brain/processor/organic.dm#L1), [`/obj/item/brain_processor/positronic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/brain/processor/positronic.dm#L3)
- **BandaStation:** [`code/modules/mob/living/brain/brain_item.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/brain/brain_item.dm#L65) — правки мозга форка
- **Статус в ss13-rs:** ❌ нет
