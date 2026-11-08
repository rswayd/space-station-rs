# Зомби
Зомби-инфекция: ромерол или укус заражает «опухолью», которая после смерти поднимает носителя зомби.

### Зомби-инфекция
Орган-опухоль в мозге: после смерти носитель воскресает зомби, лечится удалением органа; вариант без урона.

- **Код:** [`/obj/item/organ/zombie_infection`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/zombie/organs.dm#L1), [`/obj/item/organ/zombie_infection/nodamage`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/zombie/organs.dm#L124)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/antag/zombie.rs` (`infect`, `process_infection`, `zombify`), слот органа `Slot::Zombie` в `medical/organs.rs`: когтевой удар или ромерол вставляют орган (`nodamage` — без урона); живой хозяин получает токсины, мёртвый с мозгом и без самоубийства через 45–70 с поднимается (лечение типов урона, статус, приветствие), безмозглое тело предлагается призракам (`ghost_poll`, «не в этом раунде» `recovered_crew`); удаление органа до подъёма лечит. Нет: разблокировки шаттла Tombstone и сообщения `on_find`

### Зомби как вид
Вид `zombie`: медленный, неуязвим к боли и давлению, регенерирует, руки-когти заражают при атаке.

- **Код:** [`/datum/species/zombie`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/zombies.dm#L1), [`/obj/item/mutant_hand/zombie`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/zombie/items.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/zombie.rs`, `SpeciesId::Zombie` в `species/mod.rs`, `BodyMods::no_death` в `health.rs`: статус `Zombified` (обычный/безмозглый/«uninfected» вида) даёт трейты через `species::has_trait` (без дыхания, голода, холода, давления, токсинов, радиации, лёгкое расчленение и др.), замедление +1/+1.5, 20% стойкости к физическому урону, треть урона стамины, стан не дольше 2 с, смерть только при потере головы/мозга (иначе тяжёлый крит), хаск, блокировка боевого режима, когти в обе руки (`after_attack`: заражение с проверкой BIO/плотной одежды и `spaceacillin`; `check_feast` — гиб мёртвого не-человека с лечением), регенерация 0,5 ед./с после 6 с без урона (безмозглый 0,2 после 10 с); смерть другим путём снимает статус. Нет: языка зомби (речи, say_mod, стонов), ночного зрения `lighting_cutoff`, смены температурных пределов, биотипа `MOB_UNDEAD`, лечения ран регенерацией, возврата когтя с отрубленной рукой

### Ромерол
Реагент из аплинка/админов: заражает зомби-инфекцией любого, кто его получил.

- **Код:** [`/datum/reagent/romerol`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/reagents/other_reagents.dm#L2725)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/antag/zombie.rs` (`romerol_in_mob`, зовётся из `chem::life`, любая порция в крови или желудке), `janitor::expose` (касание ≥1 u: спреи и пр.); оставляет зомби-орган `nodamage`, повторно не заражает; `/datum/reagent/romerol` и наборы данных приходят конвертером. Нет: заражения выплёскиванием из стакана (`chem::try_splash` вообще не применяет реагенты)
