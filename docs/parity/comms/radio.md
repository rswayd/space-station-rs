# Радио
Носимые и настенные радиостанции `/obj/item/radio`: гарнитуры, ключи шифрования, интеркомы.

### Передача и приём радио
Предмет-радио: включено/выключено, частота, приём и передача, `canhear_range`, специальные каналы (Синдикат, ЦК, бинарный); речь в радио идёт через телекоммуникации или напрямую (`subspace_transmission = FALSE`), сообщение раскрашивается по каналу.

- **Код:** [`/obj/item/radio`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L3), [`proc/talk_into_impl()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L310), [`/datum/signal/subspace/vocal/broadcast()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/broadcasting.dm#L119)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/radio.rs`, `crates/ss13_chat/src/radio.rs`: приём и передача на всех радио, цвета каналов, шёпот, призраки слышат всё; нет телекомов (всё доходит в пределах z-уровня), сжатия сигнала, проверки видимости

### Гарнитуры (headset)
Гарнитуры отделов и глав (две ячейки под ключи у глав), бинарная и синдикатовская, «громкий режим» командных гарнитур (`use_command`), защита ушей у bowman-гарнитур.

- **Код:** [`/obj/item/radio/headset`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/headset.dm#L19), [`/obj/item/radio/headset/heads`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/headset.dm#L269), [`/obj/item/radio/headset/headset_sec/alt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/headset.dm#L196)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/radio.rs`: гарнитура в слоте ушей, каналы из ключей, префиксы `;`, `:h`, `:x`; нет громкого режима, защиты ушей bowman, выдачи гарнитур ИИ/боргам

### Ключи шифрования
Ключи задают каналы гарнитуры; отвёрткой ключи вынимаются и меняются местами; форк добавляет канал «Юридический» в ключи командования и глав.

- **Код:** [`/obj/item/encryptionkey`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/encryptionkey.dm#L1), [`/obj/item/encryptionkey/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/encryptionkey.dm#L48), [`/obj/item/encryptionkey/binary`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/encryptionkey.dm#L58), [`/obj/item/radio/screwdriver_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L596)
- **BandaStation:** [`encryptionkey.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/encryptionkey.dm#L168) — канал Justice
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/radio.rs`: каналы стартовых ключей читаются при конвертации; ключи нельзя вынуть и вставить (shortcut в `crates/ss13_mapload/src/antag.rs`)

### Интеркомы
Настенные радио: работают при питании оборудования зоны, слушают и транслируют речь рядом, `:i`; варианты (тюремный, часовня, допросная, командный, ИИ-приватный, свободный диапазон, отдельские и глав), рамка для монтажа, отвёрткой снимается.

- **Код:** [`/obj/item/radio/intercom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/intercom.dm#L1), [`/obj/item/radio/intercom/departmental`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/intercom.dm#L347), [`/obj/item/radio/intercom/ai_private`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/intercom.dm#L315), [`/obj/item/wallframe/intercom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/intercom.dm#L229)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/radio.rs`: интеркомы из карты с их переменными, питание зоны, ретрансляция, `:i`; нет окна настройки, монтажа/демонтажа

### Окно радио (tgui Radio)
Включение, смена частоты, микрофон/динамик, громкий режим, выбор каналов гарнитуры.

- **Код:** [`/obj/item/radio/ui_interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L477), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L505); tgui: [`Radio.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Radio.tsx)
- **Статус в ss13-rs:** ❌ нет — частота и режимы только из переменных карты

### Ручные рации и прочие радио
Станционная ручная рация, радио киборга, развлекательное радио (канал ТВ), игрушечное; переключение частоты, занятые частоты.

- **Код:** [`/obj/item/radio`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L3), [`/obj/item/radio/borg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L688), [`/obj/item/radio/entertainment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L719), [`proc/set_frequency()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L151)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/radio.rs`: ручные рации и `:r`/`:l` из руки; нет смены частоты, радио боргов

### Глушилки и EMP радио
EMP временно выключает радио; глушилка радиосвязи блокирует передачу (форк: белый список каналов).

- **Код:** [`/obj/item/radio/emp_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L651)
- **BandaStation:** [`radio.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L354) — глушилка учитывает частоту
- **Статус в ss13-rs:** ❌ нет

### Радиошум и звук радио
Звук-помеха при передаче; форк убирает часть ванильной логики шума.

- **Код:** [`radio.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L68)
- **BandaStation:** [`radio.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L453) — удалённый блок
- **Статус в ss13-rs:** ❌ нет

### Электропак
Ошейник-шокер на спину, срабатывает по сигналу сигнализатора на своей частоте.

- **Код:** [`/obj/item/electropack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/electropack.dm#L1)
- **Статус в ss13-rs:** ❌ нет
