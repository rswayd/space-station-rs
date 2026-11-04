# Органы
Внутренние и внешние органы карбонов: функции, урон, отказ. Механика дыхания, голода и температуры — ../mobs/; кибернетика — [cybernetics.md](cybernetics.md). В ss13-rs органы — сущности-предметы внутри моба (`crates/ss13_game/src/medical/organs.rs`); `breath.rs`, `chem.rs`, `hunger.rs`, `sound.rs` читают свои органы.

### Орган: общая механика
Орган вставляется/извлекается (`Insert`/`Remove`), получает урон (`apply_organ_damage`), выше порогов — предупреждения и отказ (`ORGAN_FAILING`), понемногу самовосстанавливается; гниёт вне тела без холода; органы дают черты и действия.

- **Код:** [`/obj/item/organ`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/_organ.dm#L1), [`proc/Insert()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/organ_movement.dm#L15), [`proc/Remove()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/organ_movement.dm#L40), [`proc/apply_organ_damage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/_organ.dm#L250), [`proc/on_life()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/_organ.dm#L174), [`proc/handle_organs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/life.dm#L503)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/organs.rs`: органы — сущности-предметы внутри моба (`OrganOf`), видовой набор человека при спавне, `Insert`/`Remove`, урон с порогами и сообщениями tg, отказ, самолечение (`healing_factor`, сытость), гниение трупа по температуре тела (не с формальдегидом/криостиланом), извлечённые органы — предметы с иконками tg, гниют по воздуху вокруг (не в морозильном ящике/шкафу), `regenerate_organs` при полном лечении; ЭМИ синтетических органов — `crates/ss13_game/src/emp/mobs.rs` (см. [cybernetics.md](cybernetics.md)); нет черт, действий и статусов от органов, поедания органов, ДНК крови, органного бокса с криостиланом

### Сердце и остановка сердца
Сердце качает кровь; без сердца или при остановке (`set_heartattack`) — нет кровообращения, oxy-урон и смерть; дефибриллятор запускает сердце. Варианты: кибернетические, проклятое, «свобода», эволюционировавшее.

- **Код:** [`/obj/item/organ/heart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/heart/_heart.dm#L1), [`proc/set_heartattack()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/life.dm#L804), [`proc/undergoing_cardiac_arrest()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/life.dm#L789), [`proc/handle_heart()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/life.dm#L285)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/medical/organs.rs`, `crates/ss13_game/src/medical/defib.rs`, `crates/ss13_game/src/medical/blood.rs`: `set_heartattack`, остановка отказавшего сердца, остановка вынутого через 12 с, `handle_heart` (4 oxy/с и 1 brute/с без бьющегося сердца), регенерация крови по здоровью сердца, дефибриллятор запускает сердце живого и мёртвого, разряд в бою останавливает; статус-эффект инфаркта и `Unconscious(80)` — статус-эффекты (нет бессознательного статуса), кибернетические и особые сердца — cybernetics.md / антагонисты

### Лёгкие
Пороги газов и обмен (см. ../mobs/breathing-temperature.md); повреждение снижает эффективность дыхания; видовые лёгкие (плазмамены, эфириалы, Лаваленд), лёгкие курильщика, кибернетические.

- **Код:** [`/obj/item/organ/lungs`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L1), [`/obj/item/organ/lungs/plasmaman`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L949), [`/obj/item/organ/lungs/cybernetic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L989)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/medical/organs.rs`, `crates/ss13_game/src/breath.rs`: лёгкие — орган с уроном и отказом (каждый вдох пропущен), без лёгких — `check_breath` человека без лёгких, сильно повреждённые лёгкие/сердце учащают дыхание, кашель и сообщения; урон лёгким от температуры вдоха tg не наносит (ранний `return`); видовые лёгкие — видов в порту нет (species), кибернетические и курильщика — cybernetics.md / квирки

### Печень
Фильтрует токсины (толерантность), метаболизирует реагенты; при отказе — toxin-урон и желтуха; печень определяет реакцию на алкоголь и черты профессий (печень офицера любит пончики и т.п.).

- **Код:** [`/obj/item/organ/liver`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/liver/_liver.dm#L5), [`proc/handle_liver()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/life.dm#L732), [`proc/undergoing_liver_failure()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/life.dm#L749)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/organs.rs`, `crates/ss13_reagents/src/metabolism.rs`, `crates/ss13_game/src/chem.rs`: печень — орган: толерантность по здоровью печени, урон от токсинов (`toxpwr`, до 2/с), отказ со стадиями по минуте (токсин, урон другим органам), без печени или при отказе — метаболизм только `self_consuming` реагентов и `handle_liver`; нет алкогольной толерантности печени, профессиональных черт печени, желтухи в осмотре, `liver_damage_multiplier`/`silent_toxin` (нет в сгенерированных данных), рвоты и сонливости стадий

### Желудок
Переваривает еду и передаёт реагенты в кровь, отвечает за голод; повреждение вызывает рвоту; видовые и кибернетические желудки.

- **Код:** [`/obj/item/organ/stomach`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/stomach/_stomach.dm#L4), [`proc/handle_hunger()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/stomach/_stomach.dm#L137)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/organs.rs`, `crates/ss13_game/src/hunger.rs`, `crates/ss13_game/src/chem.rs`: желудок — орган: без него или при отказе нет голода (`handle_hunger`) и переваривания, проглоченное без желудка теряется (`expel_ingested`, `// shortcut:` без рвоты), содержимое уходит вместе с вынутым желудком; нет рвоты от повреждённого желудка (рвоты нет), отвращения, видовых и кибернетических желудков

### Глаза
Зрение, ночное зрение, защита от вспышек; повреждение размывает и ослепляет; видовые глаза, цветовая слепота.

- **Код:** [`/obj/item/organ/eyes`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/eyes/_eyes.dm#L1), [`/obj/item/organ/eyes/night_vision`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/eyes/_eyes.dm#L594)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/organs.rs`, `crates/ss13_game/src/human.rs`, `crates/ss13_game/src/vision.rs`: глаза — орган с уроном и порогами tg, извлекаются и лечатся операцией (снимает временную слепоту, размытие 70 с), без глаз не рисуются; без глаз или с отказавшими глазами — слепота, урон выше 20/30 — близорукость 2/3 (очки с `TRAIT_NEARSIGHTED_CORRECTED` исправляют), вспышка без глаз невозможна, жжёт глаза по `flash_act`, после 10 урона — временная слепота и размытие; нет ночного зрения, собственного `tint`/`flash_protect` и видовых глаз, шрамов, цвета глаз от органа

### Уши
Слух; повреждение вызывает глухоту; кошачьи, кибернетические (громкость, шёпот, рентген).

- **Код:** [`/obj/item/organ/ears`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/ears/_ears.dm#L1), [`/obj/item/organ/ears/cybernetic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/ears/_ears.dm#L294)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/medical/organs.rs`, `crates/ss13_game/src/sound.rs`: уши — орган, без ушей или при отказе — глухота, звон (временная глухота) при уроне выше порога, операция на ушах; громкие звуки, ранящие уши, — их источники (флешбенги), кибернетические и кошачьи уши — cybernetics.md / виды

### Язык и голосовые связки
Язык задаёт речь (акценты, языки вида), вкус и любимую еду; голосовые связки — голос (у некоторых — особые силы).

- **Код:** [`/obj/item/organ/tongue`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/tongue/_tongue.dm#L1), [`/obj/item/organ/vocal_cords`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/vocal_cords/_vocal_cords.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/organs.rs`, `crates/ss13_chat/src/speech.rs` (`Tongueless`): язык — орган, без языка речь искажается регулярным выражением tg (только латиница, как на сервере) и «булькает»; нет вкуса, любимой еды, языков и акцентов видовых языков, голосовых связок

### Мозг как орган
Мозг хранит разум игрока, травмы мозга и урон мозга; при 200 урона — смерть; извлекается в процессор мозга (см. ../mobs/aliens-brains.md).

- **Код:** [`/obj/item/organ/brain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/brain/brain_item.dm#L1), [`proc/handle_brain_damage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/life.dm#L758)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/organs.rs`, `crates/ss13_game/src/medical/defib.rs`: мозг — жизненно важный орган: урон с сообщениями tg, смерть на 200, смерть при извлечении, гниение трупа (30 мин), без естественного лечения (`healing_factor` 0), травмы мозга живут в нём и переходят с ним (`medical/traumas.rs`, ../mobs/brain-traumas.md), маннитол лечит мозг; нет `brainmob` (разум не уходит с мозгом), скиллчипов, процессора мозга (aliens-brains.md)

### Аппендикс
Может воспалиться (аппендицит — болезнь), удаляется хирургией.

- **Код:** [`/obj/item/organ/appendix`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/appendix/_appendix.dm#L5)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/organs.rs`: аппендикс — орган, извлекается и вставляется операцией; нет воспаления (`inflamation_stage`)

### Внешние органы: хвосты, крылья, рога и т.д.
Визуальные органы видов: хвосты (виляние), крылья (полёт у молей и ангелов), рога, гребни, морды, антенны, шипы; извлекаются как органы.

- **Код:** [`/obj/item/organ/tail`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/external/tails.dm#L2), [`/obj/item/organ/horns`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/external/_visual_organs.dm#L102), [`/obj/item/organ/snout`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/external/_visual_organs.dm#L189), [`/obj/item/organ/antennae`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/external/_visual_organs.dm#L234), [`/obj/item/organ/spines`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/external/spines.dm#L2), [`code/modules/surgery/organs/external/wings/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/external/wings)
- **BandaStation:** [`modular_bandastation/species/code/surgery/organs/external/tajaran_external.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/surgery/organs/external/tajaran_external.dm#L1) — хвосты таяр и вульпканинов, щупальца и головной карман скреллов
- **Статус в ss13-rs:** ❌ нет

### Видовые органы форка (BandaStation)
Внутренние органы скреллов, таяр и вульпканинов со своими параметрами; рецепты их выращивания в лимбгроуэре.

- **Код:** [`/obj/item/organ/heart/skrell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/surgery/organs/internal/skrell_internal.dm#L22), [`/obj/item/organ/heart/tajaran`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/surgery/organs/internal/tajaran_internal.dm#L77), [`/obj/item/organ/heart/vulpkanin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/surgery/organs/internal/vulpkanin_internal.dm#L68)
- **BandaStation:** [`modular_bandastation/species/code/surgery/organs/internal/skrell_internal.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/surgery/organs/internal/skrell_internal.dm#L1) — органы видов форка (`tajaran_internal.dm`, `vulpkanin_internal.dm`, `limbgrower_designs.dm`)
- **Статус в ss13-rs:** ❌ нет
