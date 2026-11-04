# Броски, встраивание и расчленение
Урон брошенными предметами, застревающие в теле предметы и отрывание конечностей. Механика самого броска — ../mobs/pull-grab-throw.md; приживление конечностей и раны — ../medical/.

### Урон брошенным предметом
Попавший в моба предмет наносит `throwforce` своего `damtype` по случайной части тела (с броней, блоком), звук зависит от силы; попадание в стоящего толкает его.

- **Код:** [`living/hitby()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L272), [`carbon/hitby()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon_defense.dm#L77)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/throw.rs`: урон `throwforce` и отталкивание; нет брони, блоков, звука по силе, встраивания

### Встраивание (embedding)
Острые предметы (метательные звёзды, осколки, шприцы, стрелы, пули с шрапнелью) застревают в конечности: периодическая боль и урон, шанс выпасть, кровотечение; вытаскиваются рукой с задержкой (с уроном) или хирургией; некоторые «липкие» безвредны.

- **Код:** [`/datum/embedding`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/embedding.dm#L13), [`proc/try_embed()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/embedding.dm#L125), [`proc/try_embed_projectile()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/embedding.dm#L141), [`proc/embed_into()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/embedding.dm#L215), [`proc/rip_out()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/embedding.dm#L315), [`proc/has_embedded_objects()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/helpers.dm#L222)
- **Статус в ss13-rs:** ❌ нет

### Расчленение (dismemberment)
Сильный острый/рубящий удар или выстрел по повреждённой конечности (через рану «потеря конечности») отрывает её; голову — при большом уроне; виды с `TRAIT_NODISMEMBER` защищены, `TRAIT_EASYDISMEMBER` — уязвимы; оторванная конечность падает на пол с содержимым.

- **Код:** [`proc/can_dismember()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/dismemberment.dm#L2), [`proc/try_dismember()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/dismemberment.dm#L149), [`proc/dismember()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/dismemberment.dm#L8), [`proc/drop_limb()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/dismemberment.dm#L83), [`proc/check_projectile_dismemberment()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L244)
- **Статус в ss13-rs:** ❌ нет — отделение конечности есть только хирургией (`crates/ss13_game/src/medical/bodyparts.rs` `drop_limb`, surgery-operations.md); раны «потеря конечности» и броска на расчленение нет
