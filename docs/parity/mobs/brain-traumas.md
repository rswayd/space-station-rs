# Травмы мозга
`/datum/brain_trauma`: постоянные или временные расстройства, вызванные уроном мозга, реагентами, болезнями, магией или квирками; лечатся по уровню устойчивости (`resilience`: маннитол/нейрин, хирургия, лоботомия, магия).

### Каркас травм мозга
Травма живёт в мозге (`/obj/item/organ/brain`), получает `on_gain`/`on_life`/`on_lose`, может перехватывать речь и слух; уровни устойчивости (`TRAUMA_RESILIENCE_BASIC/SURGERY/LOBOTOMY/WOUND/MAGIC/ABSOLUTE`) определяют, что её снимает; случайные травмы выпадают при уроне мозга.

- **Код:** [`/datum/brain_trauma`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/brain_trauma.dm#L6), [`proc/gain_trauma()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/status_procs.dm#L28), [`proc/cure_trauma_type()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/status_procs.dm#L41)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/traumas.rs`: травмы в мозге (`Traumas` на органе, переходят с мозгом; эффекты на теле выводятся из них — `refresh`), `on_life` раз в `SSmobs`, уровни устойчивости и лимиты по ярусам, броски при уроне мозга выше 20/100 по формуле tg (20% тяжёлых — особые), выбор типа по полному списку случайных типов tg (`TG_POOL`: 14 лёгких, 13 тяжёлых, 10 особых) — непортированный тип не даёт травмы, так что частота каждой портированной как в tg, лечение по устойчивости: нейрин (8%/с, лёгкие), операция на мозге, лоботомия; строка «Mental trauma» в анализаторе; речевые искажения — `ss13_chat::speech::Impediments`; нет перехвата слуха (эхо, гипнотриггер), сообщений получения при пересадке мозга, травм от ран головы (`WOUND`), `TRAIT_SPECIAL_TRAUMA_BOOST`, админ-выдачи

### Лёгкие травмы
Галлюцинации, заикание, тупость (замедленная речь), нарушение речи, сотрясение, «здоровье» (фальшивое ощущение), мышечная слабость, мышечные спазмы, нервный кашель, экспрессивная афазия, эхо мыслей, дальтонизм, собственничество (не отпускает предметы).

- **Код:** [`/datum/brain_trauma/mild`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/mild.dm#L5), [`/datum/brain_trauma/mild/hallucinations`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/mild.dm#L8), [`/datum/brain_trauma/mild/concussion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/mild.dm#L93), [`/datum/brain_trauma/mild/muscle_weakness`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/mild.dm#L142), [`/datum/brain_trauma/mild/expressive_aphasia`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/mild.dm#L206), [`/datum/brain_trauma/mild/possessive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/mild.dm#L305)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/traumas.rs`: заикание (с кириллицей BandaStation), отупение (дерп-речь, слюни), дефект речи (перемешанные слова), сотрясение (конфузия, ступор; без рвоты, головокружения, размытия, невнятной речи, обморока), анозогнозия (−6 выносливости/с; без ложного HUD), мышечная слабость, нервный кашель, собственничество; нет галлюцинаций, мышечных спазмов, экспрессивной афазии (нет списка частых слов), мысленного эха (нет слуха), дальтонизма (нет клиентского цвета), фраз отупения из `traumas.json`

### Фобии
Страх выбранной категории (пришельцы, клоуны, кровь, пауки, охрана, космос и ещё ~20): при виде или слышании триггера — паника, дрожь, роняет вещи, заикание. Слова-триггеры в `strings/phobia.json`; используется квирком «Фобия».

- **Код:** [`/datum/brain_trauma/mild/phobia`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/phobia.dm#L1), [`/datum/brain_trauma/mild/phobia/clowns`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/phobia.dm#L68), [`/datum/brain_trauma/mild/phobia/spiders`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/phobia.dm#L129)
- **Статус в ss13-rs:** ❌ нет

### Тяжёлые травмы: немота, афазия, слепота, паралич
Полная немота, неспособность понимать язык, корковая слепота, паралич конечностей (параплегия, гемиплегия слева/справа).

- **Код:** [`/datum/brain_trauma/severe/mute`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/severe.dm#L9), [`/datum/brain_trauma/severe/aphasia`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/severe.dm#L25), [`/datum/brain_trauma/severe/blindness`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/severe.dm#L45), [`/datum/brain_trauma/severe/paralysis`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/severe.dm#L61)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/traumas.rs`: немота (речь, шёпот и радио отклоняются), паралич любой из 9 разновидностей (ноги — как отключённые, рука роняет предмет и не держит); нет афазии (языки — группа речи), корковой слепоты (зрение — ui/hud.md «Полноэкранные эффекты»), пара-/гемиплегии квирков

### Тяжёлые травмы: поведение
Нарколепсия, монофобия, дискоординация (нельзя пользоваться сложными предметами), пацифизм, гипнотический ступор и триггер, дислексия, клептомания.

- **Код:** [`/datum/brain_trauma/severe/narcolepsy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/severe.dm#L139), [`/datum/brain_trauma/severe/discoordination`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/severe.dm#L226), [`/datum/brain_trauma/severe/pacifism`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/severe.dm#L242), [`/datum/brain_trauma/severe/hypnotic_trigger`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/severe.dm#L277), [`/datum/brain_trauma/severe/kleptomaniac`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/severe.dm#L327)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/traumas.rs`: пацифизм (`Pacifist`: отказ бить предметом с уроном и рукой в боевом режиме, стрелять летальным — `combat.rs`, `guns.rs`), клептомания (свободная рука молча подбирает мелкий предмет рядом, 8 с перерыв; без отсрочки от урона); нет нарколепсии (нет сна и сонливости), монофобии, дискоординации, гипнотического ступора и триггера, дислексии

### Раздвоение личности
Вторая личность (призрак-игрок) периодически захватывает тело; варианты — промывка мозгов и «отключка» (пьяный провал).

- **Код:** [`/datum/brain_trauma/severe/split_personality`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/split_personality.dm#L4), [`/datum/brain_trauma/severe/split_personality/brainwashing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/split_personality.dm#L193), [`/datum/brain_trauma/severe/split_personality/blackout`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/split_personality.dm#L265)
- **Статус в ss13-rs:** ❌ нет

### Воображаемый друг
Призрак-игрок становится видимым только владельцу другом, который может говорить и показывать.

- **Код:** [`/datum/brain_trauma/special/imaginary_friend`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/imaginary_friend.dm#L6)
- **BandaStation:** [`imaginary_friend.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/imaginary_friend.dm#L24) — друг создаётся не сразу при получении травмы
- **Статус в ss13-rs:** ❌ нет

### Гипноз
Внушённая фраза, которой персонаж обязан следовать.

- **Код:** [`/datum/brain_trauma/hypnosis`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/hypnosis.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Одержимость (Obsessed)
Травма антагониста «Одержимый»: выбирает объект одержимости, муд рядом с ним. Сам антагонист — см. ../antagonists/.

- **Код:** [`/datum/brain_trauma/special/obsessed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/creepy_trauma.dm#L1)
- **BandaStation:** [`creepy_trauma.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/creepy_trauma.dm#L58) — переназначение объекта при уходе в криосон, исключение уходящих из раунда, убран речевой дебафф
- **Статус в ss13-rs:** ❌ нет

### Особые травмы
Богоглас (голос бога), блюспейс-пророк (порталы), квантовая связь, психотическая драка, упорство, шёпот мёртвых, экзистенциальный кризис, призрачный Бипски, ПТСР, первобытные инстинкты, обожание топора.

- **Код:** [`/datum/brain_trauma/special`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/special.dm#L4), [`/datum/brain_trauma/special/godwoken`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/special.dm#L7), [`/datum/brain_trauma/special/bluespace_prophet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/special.dm#L53), [`/datum/brain_trauma/special/tenacity`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/special.dm#L268), [`/datum/brain_trauma/special/beepsky`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/special.dm#L364), [`/datum/brain_trauma/special/ptsd`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/special.dm#L444)
- **Статус в ss13-rs:** ❌ нет

### Магические травмы
Люмифобия, полтергейст, антимагия, преследователь (и космический вариант еретика).

- **Код:** [`/datum/brain_trauma/magic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/magic.dm#L5), [`/datum/brain_trauma/magic/lumiphobia`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/magic.dm#L9), [`/datum/brain_trauma/magic/stalker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/magic.dm#L78)
- **Статус в ss13-rs:** ❌ нет
