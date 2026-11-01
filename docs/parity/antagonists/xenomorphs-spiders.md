# Ксеноморфы и гигантские пауки
Мидраунд-«гнёзда» из призраков: ксеноморфы с маткой и пауки с бродмазер/няньками. Сами мобы как существа — см. ../mobs/.

### Ксеноморфы: антагонист и улей
`/datum/antagonist/xeno` и `/datum/team/xeno`: цель — развивать улей; пленные ксеноморфы (цель — сбежать из неволи); появление личинок из вентиляции.

- **Код:** [`/datum/antagonist/xeno`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/xeno/xeno.dm#L18), [`/datum/team/xeno`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/xeno/xeno.dm#L8), [`/datum/antagonist/xeno/captive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/xeno/xeno.dm#L54), [`/datum/objective/advance_hive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/xeno/xeno.dm#L92), [`/datum/objective/escape_captivity`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/xeno/xeno.dm#L83)
- **Статус в ss13-rs:** ❌ нет

### Касты ксеноморфов
Личинка растёт и выбирает касту: охотник (прыжок, невидимость), страж (кислотный плевок, нейротоксин), трутень (смола, эволюция в преторианца/матку), преторианец, матка (яйца, единственная).

- **Код:** [`/mob/living/carbon/alien/larva`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/larva/larva.dm#L1), [`/mob/living/carbon/alien/adult/hunter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/caste/hunter.dm#L1), [`/mob/living/carbon/alien/adult/sentinel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/caste/sentinel.dm#L1), [`/mob/living/carbon/alien/adult/drone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/caste/drone.dm#L1), [`/mob/living/carbon/alien/adult/royal/praetorian`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/caste/praetorian.dm#L1), [`/mob/living/carbon/alien/adult/royal/queen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien/adult/queen.dm#L55)
- **Статус в ss13-rs:** ❌ нет

### Структуры и размножение ксеноморфов
Сорняки, смоляные стены/мембраны, яйца с лицехватами, гель-капсулы; лицехват откладывает эмбрион, грудолом вырывается из носителя.

- **Код:** [`/obj/structure/alien/weeds`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/aliens.dm#L131), [`/obj/structure/alien/resin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/aliens.dm#L50), [`/obj/structure/alien/egg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/aliens.dm#L333), [`/obj/structure/alien/gelpod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/aliens.dm#L38), [`code/modules/mob/living/carbon/alien`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/alien)
- **Статус в ss13-rs:** ❌ нет

### Гигантские пауки
`/datum/antagonist/spider`: директива от бродмазер; виды: страж, засадный, охотник, разведчик, нянька (кладёт яйца, плетёт коконы), тарантул, гадюка, бродмазер, ледяные и плотские пауки; паутина и коконы.

- **Код:** [`/datum/antagonist/spider`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/spiders/spiders.dm#L1), [`/datum/antagonist/spider/flesh`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/spiders/spiders.dm#L40), [`/datum/objective/spider`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/spiders/spiders.dm#L23), [`/mob/living/basic/spider/giant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/spider/giant_spider/giant_spiders.dm#L8), [`/mob/living/basic/spider/giant/midwife`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/spider/giant_spider/giant_spiders.dm#L422)
- **BandaStation:** [`giant_spiders.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/giant_spiders.dm#L1) — больше здоровья паукам
- **Статус в ss13-rs:** ❌ нет
