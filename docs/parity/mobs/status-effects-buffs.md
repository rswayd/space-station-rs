# Статус-эффекты: баффы и нейтральные
Положительные эффекты (еда, тренировки, магия) и нейтральные взаимодействия (передача предмета, «руки вверх»).

### Тушение огня перекатом (stop, drop and roll)
Горящий моб жмёт «сопротивляться»: падает и катается, каждый тик снимая огненные стаки.

- **Код:** [`/datum/status_effect/stop_drop_roll`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/stop_drop_roll.dm#L1), [`proc/resist_fire()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L1254)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/fire.rs` (`stop_drop_roll`)

### Тренировка (exercised)
После упражнений на тренажёрах — бафф к восстановлению и опыт навыка атлетики (см. [skills.md](skills.md)).

- **Код:** [`/datum/status_effect/exercised`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L180)
- **Статус в ss13-rs:** ❌ нет

### Баффы еды
Блюда с высоким качеством дают эффекты: ускорение (`haste`), охлаждение (`chilling`), черты (иммунитет к шоку, немота, иммунитет к пепельной буре, «вразвалку»), акцент речи (итальянский, французский).

- **Код:** [`/datum/status_effect/food`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/food/_food_effect.dm#L2), [`/datum/status_effect/food/haste`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/food/haste.dm#L2), [`/datum/status_effect/food/trait`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/food/grant_trait.dm#L2), [`/datum/status_effect/food/speech`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/food/speech.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Биоварь (хирургические улучшения)
Постоянные эффекты от операций: укреплённые вены, связки, нервы, кора мозга. Сами операции — см. ../medical/.

- **Код:** [`/datum/status_effect/bioware`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/bioware/_bioware.dm#L6), [`/datum/status_effect/bioware/heart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/bioware/circulation.dm#L2), [`/datum/status_effect/bioware/nerves`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/bioware/nerves.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Лечебные баффы
Регенеративное ядро (лаваленд), клятва Гиппократа (жезл Асклепия), заращивание плоти (генокрад), бафф здоровья, хорошая музыка (лечит слушателей), гнездо (корм), ускорение.

- **Код:** [`/datum/status_effect/regenerative_core`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L380), [`/datum/status_effect/hippocratic_oath`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L268), [`/datum/status_effect/fleshmend`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L129), [`/datum/status_effect/good_music`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L359), [`/datum/status_effect/speed_boost`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L466)
- **Статус в ss13-rs:** ❌ нет

### Магические и антагонистские баффы
Милость Его (His Grace), кровавое опьянение, хаос (Mayhem), шаровая молния, благословение безумия, теневая регенерация, радиационный иммунитет, рентгеновское зрение, ореол культа, стойкость революционера.

- **Код:** [`/datum/status_effect/his_grace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L3), [`/datum/status_effect/blooddrunk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L95), [`/datum/status_effect/mayhem`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L423), [`/datum/status_effect/radiation_immunity`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L610), [`/datum/status_effect/temporary_xray`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/xray.dm#L4), [`/datum/status_effect/cult_halo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/cult_halo.dm#L2)
- **BandaStation:** [`buffs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L425) — хаос (Mayhem) длится 15 с вместо 2 мин и даёт 5 ед. админордразина вместо 25
- **Статус в ss13-rs:** ❌ нет

### Передача предмета и «дай пять»
Предложение предмета (`offering`): получатель видит алерт и берёт предмет; варианты без предмета — «дай пять», помочь подняться.

- **Код:** [`/datum/status_effect/offering`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L207), [`/datum/status_effect/offering/no_item_received/high_five`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L363)
- **Статус в ss13-rs:** ❌ нет

### «Руки вверх» и сдача
Взятие на мушку (`holdup`/`heldup`) и сдача (`surrender`): моб поднимает руки и ложится.

- **Код:** [`/datum/status_effect/holdup`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L185), [`/datum/status_effect/grouped/surrender`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L371), [`/datum/status_effect/surrender_timed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L393)
- **Статус в ss13-rs:** ❌ нет

### Прочие нейтральные эффекты
Влюблённость, успокоенное горло/голова, награда за голову, калтропы, эйгенстазиум, свечение (тинлюкс, черенков), выпотрошенность, регенерация от мытья и горячего источника, прожектор, муд-зоны, ESP админа, «heads up».

- **Код:** [`/datum/status_effect/in_love`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L73), [`/datum/status_effect/caltropped`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L417), [`/datum/status_effect/eigenstasium`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L431), [`/datum/status_effect/washing_regen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L661), [`/datum/status_effect/moodlet_in_area`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L847)
- **Статус в ss13-rs:** ❌ нет

### Предмет, прикованный к руке
Предмет пристёгнут к руке наручниками и не выпадает.

- **Код:** [`/datum/status_effect/cuffed_item`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/cuffed_item.dm#L9)
- **Статус в ss13-rs:** ❌ нет
