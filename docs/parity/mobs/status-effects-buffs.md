# Статус-эффекты: баффы и нейтральные
Положительные эффекты (еда, тренировки, магия) и нейтральные взаимодействия (передача предмета, «руки вверх»).

### Тушение огня перекатом (stop, drop and roll)
Горящий моб жмёт «сопротивляться»: падает и катается, каждый тик снимая огненные стаки.

- **Код:** [`/datum/status_effect/stop_drop_roll`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/stop_drop_roll.dm#L1), [`proc/resist_fire()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L1254)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/fire.rs` (`stop_drop_roll`)

### Тренировка (exercised)
После упражнений на тренажёрах — бафф к восстановлению и опыт навыка атлетики (см. [skills.md](skills.md)).

- **Код:** [`/datum/status_effect/exercised`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L180)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/buffs.rs` (`Exercised`, `exercise`, `workout_duration`, `exercise_mood`, `sleep_workout`), `status_effects.rs` (`sleep_pulse`), `gym.rs` (груша): 15 с (+бонус × модификатор: Халк, ожирение, pumpup, оксандролон, белковая еда, уровень атлетики) до предела `SKILL_VALUE_MODIFIER` атлетики (истощение — крик и сообщение), алерт `exercised`, мудлет `exercise` по уровню атлетики и личности (Lazy, Athletic), сон с качеством > 0 переводит его в опыт атлетики (×10); только с разумом. Источники кроме груши — с их механиками: тренажёр (`weight_machine`), бокс, плавание, перчатки, гулаг-жерло, неподвижный стержень; нет `TRAIT_STIMMED`

### Баффы еды
Блюда с высоким качеством дают эффекты: ускорение (`haste`), охлаждение (`chilling`), черты (иммунитет к шоку, немота, иммунитет к пепельной буре, «вразвалку»), акцент речи (итальянский, французский).

- **Код:** [`/datum/status_effect/food`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/food/_food_effect.dm#L2), [`/datum/status_effect/food/haste`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/food/haste.dm#L2), [`/datum/status_effect/food/trait`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/food/grant_trait.dm#L2), [`/datum/status_effect/food/speech`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/food/speech.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/buffs.rs` (`apply_food_buff`, `FoodBuff`): первый укус блюда `Handmade` со сложностью 1–5 даёт `haste` силы = сложности (−0,04 дс/тайл к ходьбе — `health.rs`, −0,06 к множителю `do_after` — `tools.rs`) на 5 мин × 2 × среднюю чистоту, алерт `food_buff_N`, новый заменяет старый; нет переопределений `crafted_food_buff` отдельных блюд (охлаждение мороженого, черты, акценты: переменная не импортируется в `DmFood`); акценты (`/datum/status_effect/food/speech`): французский уже есть (`ss13_chat::accents::Accent::French`), итальянскому еды нужна английская таблица `italian_replacement.json` (у усов — русская `Accent::Italian`) — перенесено из `docs/parity/ui/speech.md`

### Биоварь (хирургические улучшения)
Постоянные эффекты от операций: укреплённые вены, связки, нервы, кора мозга. Сами операции — см. ../medical/.

- **Код:** [`/datum/status_effect/bioware`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/bioware/_bioware.dm#L6), [`/datum/status_effect/bioware/heart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/bioware/circulation.dm#L2), [`/datum/status_effect/bioware/nerves`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/bioware/nerves.dm#L2)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/medical/bioware.rs` (`Biowares`, `gain`, `grants`, `easily_wounded`), операции — `medical/surgery/defs.rs`: венозная нить (кровотечение ×¼), мышечная мембрана (без сердца), сращивание нервов (оглушение ×½, стамина ×0,8), заземление нервов (иммунитет к шоку), отпечаток коры (лечит и блокирует базовые травмы), крюк для связок (`TRAIT_EASYDISMEMBER` в `medical/wounds/roll.rs` `WoundTraits`, `TRAIT_LIMBATTACHMENT`), укрепление связок (`TRAIT_NODISMEMBER`, `TRAIT_EASILY_WOUNDED`); нет складывания коры (`TRAIT_SPECIAL_TRAUMA_BOOST`) и механических вариантов; ручное прикрепление конечности по `TRAIT_LIMBATTACHMENT` (`/obj/item/bodypart/attack`) — у агента medical

### Лечебные баффы
Регенеративное ядро (лаваленд), клятва Гиппократа (жезл Асклепия), заращивание плоти (генокрад), бафф здоровья, хорошая музыка (лечит слушателей), гнездо (корм), ускорение.

- **Код:** [`/datum/status_effect/regenerative_core`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L380), [`/datum/status_effect/hippocratic_oath`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L268), [`/datum/status_effect/fleshmend`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L129), [`/datum/status_effect/good_music`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L359), [`/datum/status_effect/speed_boost`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L466)
- **Статус в ss13-rs:** 🟡 частично — только заращивание плоти генокрада (`crates/ss13_game/src/antag/changeling/server.rs`, fleshmend); остальных нет

### Магические и антагонистские баффы
Милость Его (His Grace), кровавое опьянение, хаос (Mayhem), шаровая молния, благословение безумия, теневая регенерация, радиационный иммунитет, рентгеновское зрение, ореол культа, стойкость революционера.

- **Код:** [`/datum/status_effect/his_grace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L3), [`/datum/status_effect/blooddrunk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L95), [`/datum/status_effect/mayhem`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L423), [`/datum/status_effect/radiation_immunity`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L610), [`/datum/status_effect/temporary_xray`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/xray.dm#L4), [`/datum/status_effect/cult_halo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs/cult_halo.dm#L2)
- **BandaStation:** [`buffs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L425) — хаос (Mayhem) длится 15 с вместо 2 мин и даёт 5 ед. админордразина вместо 25
- **Статус в ss13-rs:** 🟡 частично — ореол культа (`crates/ss13_game/src/antag/cult/server.rs`, `halo1`..`halo6`, только вид); остальных нет

### Передача предмета и «дай пять»
Предложение предмета (`offering`): получатель видит алерт и берёт предмет; варианты без предмета — «дай пять», помочь подняться.

- **Код:** [`/datum/status_effect/offering`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L207), [`/datum/status_effect/offering/no_item_received/high_five`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L363)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/buffs.rs` (`give`, `take`, `Offering`), `click.rs` (`GiveClick`), кликабельные алерты (`hud.rs` `ALERT_CLICK`, `breath.rs` `clicked_alert`, `breath_client.rs`): Ctrl+Shift по соседнему мобу предлагает предмет активной руки (проверки tg: уже передаёт, недееспособность, рядом, может держать), получатель видит алерт и кликом берёт предмет в свободную руку (`take`: дистанция, предмет всё ещё в активной руке, свободная рука, `NODROP`), предложение кончается, когда предмет ушёл из рук или получатель отошёл/недееспособен; Ctrl+Shift по себе — в другую руку; клавиша V (BandaStation) предлагает всем вокруг (`ClientIntent::GiveItem`, `crates/ss13_game/src/inventory_client.rs`); нет «дай пять» и «помочь подняться» (нет `hand_item`), осмотра предмета Shift-кликом по алерту и картинки предмета на алерте

### «Руки вверх» и сдача
Взятие на мушку (`holdup`/`heldup`) и сдача (`surrender`): моб поднимает руки и ложится.

- **Код:** [`/datum/status_effect/holdup`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L185), [`/datum/status_effect/grouped/surrender`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L371), [`/datum/status_effect/surrender_timed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L393)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/buffs.rs` (`surrendered`, `surrender_timed`, `SurrenderPrompt`): эмоут `*surrender` (`ss13_chat::emote`, сообщение `ss13_chat::server::Emoted`) и клик по алерту сдачи — паралич 20 с и снятие алерта; ручка СБ (`/obj/item/pen/red/security`, клик дальше досягаемости) даёт мобу на тайле алерт на 30 с, перезарядка 30 с. Чужое: взятие на мушку (`holdup`/`heldup`) — часть `gunpoint` (оружие, `guns.rs`), голографический знак ручки (эффекты)

### Прочие нейтральные эффекты
Влюблённость, успокоенное горло/голова, награда за голову, калтропы, эйгенстазиум, свечение (тинлюкс, черенков), выпотрошенность, регенерация от мытья и горячего источника, прожектор, муд-зоны, ESP админа, «heads up».

- **Код:** [`/datum/status_effect/in_love`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L73), [`/datum/status_effect/caltropped`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L417), [`/datum/status_effect/eigenstasium`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L431), [`/datum/status_effect/washing_regen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L661), [`/datum/status_effect/moodlet_in_area`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L847)
- **Статус в ss13-rs:** 🟡 частично — свечение тинлюкса (`crates/ss13_game/src/buffs.rs` `tinlux_light`: свет 2/1,5 `#ccff33`, пока реагент в крови); муд-зоны (`moodlet_in_area`) — `personality.rs`; регенерация стамины под душем (`washing_regen`) — `plumbing.rs`. Остальное — с источниками у их групп: влюблённость (валентинки, зелье любви ксенобиологии), успокоенное горло/голова (напитки; кашель и головная боль болезней), награда за голову (магический снаряд), калтропы (элемент `caltrop`), эйгенстазиум (реагент), черенков (радиация), выпотрошенность (генокрад, ИИ мобов), горячий источник, прожектор (смайт), ESP админа, «heads up» (спавнеры)

### Предмет, прикованный к руке
Предмет пристёгнут к руке наручниками и не выпадает.

- **Код:** [`/datum/status_effect/cuffed_item`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/cuffed_item.dm#L9)
- **Статус в ss13-rs:** ✅ не требуется — в BandaStation `cuffable_item` отключён (`CUFFABLE_ITEMS_DISABLED`, `code/__DEFINES/bandastation/defines.dm`): статус-эффект недостижим
