# Религия и капеллан
Выбор религии, секты с благосклонностью и ритуалами, алтарь, библия, нуллрод и облачения капеллана (`code/modules/religion`, `code/modules/jobs/job_types/chaplain`). Сама работа капеллана как профессия — в джобах; здесь её механики.

### Выбор религии при спавне капеллана
Капеллан задаёт название религии, божество и библии (из префов); глобально хранится `GLOB.religion`, последующие капелланы наследуют выбор первого.

- **Код:** [`/datum/job/chaplain/after_spawn()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/chaplain/chaplain.dm#L43), [`GLOBAL_VAR(religion)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_globalvars/religion.dm#L2)
- **Статус в ss13-rs:** ❌ нет — профессия капеллана есть в `jobs.ron`, религии нет

### Библия
Выбор облика библии при первом использовании, благословение (лечение верующих, удар по голове неверующих), превращение стола в алтарь, освящение воды, блок пули с шансом; сжигание библии проклинает. Варианты: синдикатская, «booze».

- **Код:** [`/obj/item/book/bible`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/bibles.dm#L67), [`bless()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/bibles.dm#L210), [`make_new_altar()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/bibles.dm#L201), [`/obj/item/book/bible/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/bibles.dm#L348)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/library.rs`: у капеллана лечит другого человека (по 10 brute/burn на раненую часть, mood `blessing`) в 40%, иначе бьёт по голове (5 урона мозгу без шлема); не-капеллана обжигает (10 burn); благословляет воду и нечестивую воду в ёмкости в святую; чтение как книги; нет скинов, сект, создания алтаря, проявления рун, мёртвых, неуклюжих

### Алтарь богов и религиозный инструмент
Алтарь (и библия как инструмент) открывает tgui выбора секты, приём жертв за благосклонность и запуск ритуалов.

- **Код:** [`/obj/structure/altar/of_gods`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/religion_structures.dm#L47), [`/obj/structure/altar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/religion_structures.dm#L1), [`/datum/component/religious_tool`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/religious_tool.dm#L6); tgui: [`ReligiousTool.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ReligiousTool.jsx)
- **Статус в ss13-rs:** ❌ нет

### Секты и благосклонность
Секта задаёт, что принимается в жертву, как благословляют и какие ритуалы доступны: пуританизм, механикус, пиромания (pyre), жадность, обременённые (burden), честь (honorbound), техобслуживание, спарринг, музыка (festival), сны.

- **Код:** [`/datum/religion_sect`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/religion_sects.dm#L10), [`adjust_favor()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/religion_sects.dm#L94), [`/datum/religion_sect/mechanical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/religion_sects.dm#L159), [`/datum/religion_sect/dreams`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/religion_sects.dm#L535)
- **Статус в ss13-rs:** ❌ нет

### Ритуалы (общие и механикус/жадность/техобслуживание/спарринг)
Ритуал за благосклонность у алтаря: синтетическая конверсия, благословение машин, торговый автомат, адаптация к техтоннелям, тотем, контракт спарринга, объявление арены, церемониальное оружие и др.

- **Код:** [`/datum/religion_rites`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/rites.dm#L1), [`/datum/religion_rites/synthconversion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/rites.dm#L80), [`/datum/religion_rites/greed/custom_vending`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/rites.dm#L204), [`/obj/item/ritual_totem`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/religion_structures.dm#L104)
- **Статус в ss13-rs:** ❌ нет

### Ритуалы пиромании
Огнеупорность, жертвоприношение горящего тела, вечная свеча, «пылающая звезда».

- **Код:** [`/datum/religion_rites/fireproof`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/pyre/pyre_rites.dm#L9), [`/datum/religion_rites/burning_sacrifice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/pyre/pyre_rites.dm#L44)
- **Статус в ss13-rs:** ❌ нет

### Секта чести (honorbound)
Капеллан получает травму «кодекс чести»: нельзя бить невиновных, безоружных и т.п., иначе наказание; ритуалы прощения, свитка правил, посвящения крестоносцев.

- **Код:** [`/datum/brain_trauma/special/honorbound`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/honorbound/honorbound_trauma.dm#L5), [`/datum/religion_rites/forgive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/honorbound/honorbound_rites.dm#L2), [`/obj/item/paper/holy_writ`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/honorbound/honorbound_rites.dm#L67)
- **Статус в ss13-rs:** ❌ нет

### Секта обременённых и псайкер
Благосклонность растёт от увечий самого капеллана; на максимуме ритуал превращает его в псайкера (слепой, видит эхолокацией, нуллрод становится револьвером).

- **Код:** [`/datum/religion_sect/burden`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/religion_sects.dm#L315), [`/datum/brain_trauma/special/burdened`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/burdened/burdened_trauma.dm#L2), [`/obj/item/organ/brain/psyker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/burdened/psyker.dm#L1), [`/datum/religion_rites/nullrod_transformation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/burdened/psyker.dm#L91)
- **Статус в ss13-rs:** ❌ нет

### Секта музыки (festival)
Священная скрипка, настройка инструментов на эффекты песен: евангелизм, свет, нуль-волна, боль, колыбельная.

- **Код:** [`/datum/religion_sect/music`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/religion_sects.dm#L513), [`/datum/religion_rites/song_tuner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/festival/instrument_rites.dm#L56), [`/obj/item/instrument/violin/festival`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/festival/festival_violin.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Секта снов
Ритуалы сна: пророческий сон, проекция сна (дух бродит по станции), защита от кошмаров, изгнание кошмара, общий сон (slumber party).

- **Код:** [`/datum/religion_rites/dream_portent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/dreams/dream_portent.dm#L1), [`/datum/religion_rites/dream_projection`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/dreams/dream_projection.dm#L1), [`/datum/religion_rites/slumber_party`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/dreams/slumber_party.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Секта спарринга
Контракт поединка между двумя игроками с выбором ставки и правил (tgui), матч отслеживается, нарушение карается; церемониальный клинок.

- **Код:** [`/datum/religion_sect/spar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/religion_sects.dm#L484), [`/obj/item/sparring_contract`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/sparring/sparring_contract.dm#L1), [`/datum/sparring_match`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/sparring/sparring_datum.dm#L1), [`/obj/item/ceremonial_blade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/sparring/ceremonial_gear.dm#L2); tgui: [`SparringContract.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SparringContract.tsx)
- **Статус в ss13-rs:** ❌ нет

### Посвящение в дьяконы
Ритуал делает верующего члена экипажа помощником капеллана (может проводить благословения).

- **Код:** [`/datum/religion_rites/deaconize`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/religion/deaconize.dm#L6)
- **Статус в ss13-rs:** ❌ нет

### Нуллрод (святое оружие)
Святой жезл блокирует магию; капеллан один раз выбирает из ~30 форм (клеймор, катана, бензопила, хлыст, посох, рука-клинок, лук божественного лучника и т.д.). Боевые числа — у агента оружия.

- **Код:** [`/obj/item/nullrod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/chaplain/chaplain_nullrod.dm#L30), [`/proc/init_nullrod_variants()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/chaplain/chaplain_nullrod.dm#L5), [`/obj/item/vorpalscythe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/chaplain/chaplain_vorpal_scythe.dm#L31), [`/obj/item/gun/ballistic/bow/divine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/bows/bow_types.dm#L16)
- **BandaStation:** [`code/modules/jobs/job_types/chaplain/chaplain_nullrod.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/chaplain/chaplain_nullrod.dm#L52) — освящение против вампиров (`sanctify_force`)
- **Статус в ss13-rs:** ❌ нет — в `crates/ss13_mapload/src/security.rs` лишь иконка `nullrod` у телескопической дубинки

### Облачения капеллана и святые наборы
Рясы, броня крестоносца/темплара, коробки с костюмами (`/obj/item/storage/box/holy`), пластинчатые доспехи и посохи для арены; знамёна отделов воодушевляют своих.

- **Код:** [`/obj/item/clothing/suit/chaplainsuit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/chaplain/chaplain_costumes.dm#L3), [`/obj/item/storage/box/holy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/boxes/clothes_boxes.dm#L163), [`/obj/item/banner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/religion.dm#L1), [`/obj/item/godstaff`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/religion.dm#L328)
- **Статус в ss13-rs:** ❌ нет — одежда носится как обычная (см. ../items/), святых эффектов нет
