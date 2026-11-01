# Мутации
Генетические мутации `/datum/mutation`: силы, недостатки, сенсорные, речевые и телесные. В ss13-rs мутаций нет.

### Механика мутаций
Мутация активируется через последовательность в SE (секвенирование в консоли) или инъектор; имеет качество (положительная/отрицательная/малая), нестабильность, хромосомные модификаторы (сила, синхронизация, энергоэффективность, стабильность); силы дают действие с перезарядкой; некоторые смешиваются в новые (`generecipe`).

- **Код:** [`/datum/mutation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/_mutations.dm#L23), [`proc/on_acquiring()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/_mutations.dm#L121), [`proc/on_losing()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/_mutations.dm#L163), [`proc/add_mutation()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/dna.dm#L113), [`proc/remove_mutation()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dna/dna.dm#L147), [`/datum/generecipe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/_combined.dm#L1), [`/obj/item/chromosome`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/chromosome.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/genetics/mod.rs`: источники activated/mutator, `add_mutation`/`remove_mutation`/`domutcheck`, качество, нестабильность, сложность, алиасы «Mutation N» на раунд, открытие мутаций станцией; нет хромосом, рецептов, сил с перезарядкой, конфликтов, оверлеев

### Халк
Огромная сила (урон рукой, ломает стены), игнор оглушения, неспособность пользоваться оружием, зелёная кожа, крик; варианты — волшебный, сверхчеловек, орк.

- **Код:** [`/datum/mutation/hulk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/hulk.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Активные силы
Телекинез, телепатия, чтение мыслей (антенна), огненное дыхание, пиро-/крио-/цинди-/гелади-кинез, лазерные глаза, удлинённые руки, хамелеон, паутина, шип-язык, электрический удар рукой, наложение рук, адреналиновый рывок, самоампутация, пустотный магнит, обоняние (выслеживание), дальнее зрение.

- **Код:** [`/datum/mutation/telekinesis`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/reach.dm#L2), [`/datum/mutation/telepathy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/telepathy.dm#L1), [`/datum/mutation/firebreath`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/fire_breath.dm#L1), [`/datum/mutation/pyrokinesis`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/hot.dm#L24), [`/datum/mutation/cryokinesis`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/cold.dm#L23), [`/datum/mutation/laser_eyes`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/sight.dm#L128), [`/datum/mutation/chameleon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/chameleon.dm#L2), [`/datum/mutation/webbing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/webbing.dm#L2), [`/datum/mutation/shock`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/touch.dm#L1), [`/datum/mutation/olfaction`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/olfaction.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Пассивные полезные мутации
Адаптации к холоду, жару, давлению, ночное зрение, рентгеновское и тепловое зрение, сила, изоляция, светимость/антисвет, регенерация конечностей, поедание камней, ускоренный метаболизм, неудержимость, умный, совместимость с имплантами.

- **Код:** [`/datum/mutation/adaptation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/adaptation.dm#L1), [`/datum/mutation/xray`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/sight.dm#L108), [`/datum/mutation/thermal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/sight.dm#L40), [`/datum/mutation/night_vision`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/sight.dm#L192), [`/datum/mutation/strong`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/body.dm#L323), [`/datum/mutation/insulated`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/body.dm#L341), [`/datum/mutation/glow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/body.dm#L274), [`/datum/mutation/limb_regeneration`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/body.dm#L710), [`/datum/mutation/biotechcompat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/passive.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/genetics/mod.rs`: светимость (свет моба), адаптация к давлению (без урона и тревог давления, `breath.rs`); холод/жар/изоляция и рентген — только числа tg, без эффекта

### Недостатки (дисабилити)
Эпилепсия, кашель, паранойя, Туретт, неуклюжесть, близорукость, слепота, глухота, немота, безграмотность, карликовость/гигантизм, спазмы, радиоактивность, самовозгорание, кислотная плоть, плохие ДНК, аллергия, кровоточивость, мученичество, безголовость.

- **Код:** [`/datum/mutation/epilepsy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/body.dm#L4), [`/datum/mutation/clumsy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/body.dm#L199), [`/datum/mutation/nearsight`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/sight.dm#L2), [`/datum/mutation/blind`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/sight.dm#L21), [`/datum/mutation/deaf`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/body.dm#L230), [`/datum/mutation/mute`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/speech.dm#L136), [`/datum/mutation/dwarfism`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/body.dm#L116), [`/datum/mutation/radioactive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/radioactive.dm#L1), [`/datum/mutation/headless`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/body.dm#L529)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/genetics/mod.rs`: эпилепсия (паралич 20 с вместо `Unconscious`), кашель (роняет предметы), глухота — без эффекта; остальных нет

### Речевые мутации
Нервозность (заикание), невнятность, шведский, гопник, Элвис, укурок, средневековый, поросячья латынь, «wacky», «heckacious».

- **Код:** [`/datum/mutation/nervousness`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/speech.dm#L6), [`/datum/mutation/unintelligible`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/speech.dm#L155), [`/datum/mutation/swedish`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/speech.dm#L174), [`/datum/mutation/chav`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/speech.dm#L187), [`/datum/mutation/medieval`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/speech.dm#L245)
- **Статус в ss13-rs:** ❌ нет

### Мутация вида (обезьяна)
Мутация `race` превращает в обезьяну и обратно (ключевая мутация генетиков).

- **Код:** [`/datum/mutation/race`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/body.dm#L239)
- **Статус в ss13-rs:** ❌ нет

### Мутации настроения окружающих
Аура, поднимающая или понижающая настроение соседей.

- **Код:** [`/datum/mutation/aoe_moodlet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mutations/aoe_moodlet.dm#L1)
- **Статус в ss13-rs:** ❌ нет
