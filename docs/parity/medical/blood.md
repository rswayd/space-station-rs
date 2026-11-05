# Кровь и кровотечение
Объём крови, группы, кровопотеря, переливание и кровавые следы. В ss13-rs — `crates/ss13_game/src/medical/blood.rs`.

### Объём крови и её восполнение
У человека 560 единиц крови; при нехватке — бледность, oxy-урон, головокружение, потеря сознания и смерть по порогам `BLOOD_VOLUME_*`; кровь восполняется со временем (быстрее с железом, едой и сердцем).

- **Код:** [`human/handle_blood()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/blood.dm#L154), [`proc/get_blood_volume()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/blood.dm#L20), [`proc/adjust_blood_volume()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/blood.dm#L71), [`proc/update_blood_status()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/blood.dm#L98)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/blood.rs`: 560 ед., пороги `BLOOD_VOLUME_*`, восполнение от питания/сытости и железа, кислородный урон, потеря дыхания и выносливости, обмороки, смерть ниже 112; размытие зрения при нехватке крови (`crates/ss13_game/src/vision.rs`); нет головокружения (только сообщения), `Unconscious` — паралич, нет сердечного множителя и солевого раствора

### Кровотечение
Раны (порезы, проколы) дают скорость кровотечения; кровь капает на пол, оверлеи кровотечения на частях тела; коагулянты, бинты, жгуты и лёжа — замедляют.

- **Код:** [`proc/bleed()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/blood.dm#L270), [`carbon/get_bleed_rate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/blood.dm#L288), [`proc/bleed_warn()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/blood.dm#L305), [`proc/is_bleeding()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L1042), [`carbon/coagulant_effect()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/blood.dm#L144)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/blood.rs`, `crates/ss13_game/src/medical/wounds.rs`: `bleed_rate` ран и открытых сосудов (лёжа ×0.75), предупреждения `bleed_warn`; нет брызг, капель и следов, оверлеев кровотечения, жгутов, коагулянтов

### Группы крови и совместимость
Группы A/B/AB/O ±, универсальная, видовые (ящеры, эфириалы, слаймы, улитки, масло у роботов, вода, клоунская, ксено); переливание несовместимой крови вызывает токсины.

- **Код:** [`/datum/blood_type`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/blood_types.dm#L1), [`proc/get_bloodtype()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/blood.dm#L509), [`proc/get_blood_compatibility()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/blood.dm#L619), [`proc/set_blood_type()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L1212)
- **BandaStation:** [`modular_bandastation/species/code/human/blood_types.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/blood_types.dm#L1) — группы крови скреллов и молей
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/blood.rs`: человеческие группы A/B/AB/O ± с весами tg и универсальная, `compatible_types`, токсин при несовместимом переливании; видовые группы (ящеры, эфириалы, слаймы, улитки, масло, вода, вампиры, скреллы и моли форка), задаваемые видом (`crates/ss13_game/src/species/mod.rs`); нет клоунской и ксено, реагент видовой крови — обычная кровь

### Переливание и пакеты крови
Кровь переносится шприцами и капельницей (`transfer_blood_to`), хранится в пакетах крови (по группам), содержит ДНК, болезни и данные.

- **Код:** [`proc/transfer_blood_to()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/blood.dm#L370), [`proc/get_blood_data()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/blood.dm#L433), [`/obj/item/reagent_containers/blood`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/blood_pack.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/blood.rs`, `crates/ss13_game/src/chem.rs`: забор крови шприцем, пакеты крови по группам, `/datum/reagent/blood` с группой в `data`, введённая кровь становится объёмом, капельница; вирусы и антитела в крови — `crates/ss13_game/src/medical/diseases.rs` (`BloodDiseases` на контейнере); нет ДНК и прочих данных крови

### Брызги и кровавые следы
Удары и кровотечение оставляют брызги и лужи крови; наступивший разносит следы (`bloodysoles`); кровь на одежде и руках, криминалистика по ДНК.

- **Код:** [`proc/add_splatter_floor()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/blood.dm#L641), [`proc/spray_blood()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/blood.dm#L666), [`/datum/component/bloodysoles`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/bloodysoles.dm#L4)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/janitor.rs` (`bleed_splatter`, `add_splatter_floor`): капли и лужи кровотечения; `crates/ss13_game/src/forensics/server.rs` (`attack_effects`): удар предметом оставляет лужу под жертвой, кровь на оружии и одежде нападающего, ДНК крови в следах; нет разлёта брызг (`spray_blood`), `bloodysoles` и следов ног
