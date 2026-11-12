# Радио
Носимые и настенные радиостанции `/obj/item/radio`: гарнитуры, ключи шифрования, интеркомы.

### Передача и приём радио
Предмет-радио: включено/выключено, частота, приём и передача, `canhear_range`, специальные каналы (Синдикат, ЦК, бинарный); речь в радио идёт через телекоммуникации или напрямую (`subspace_transmission = FALSE`), сообщение раскрашивается по каналу.

- **Код:** [`/obj/item/radio`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L3), [`proc/talk_into_impl()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L310), [`/datum/signal/subspace/vocal/broadcast()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/telecomms/broadcasting.dm#L119)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/radio.rs`, `crates/ss13_chat/src/radio.rs`: приём и передача на всех радио, цвета каналов, шёпот, призраки слышат всё; передача через телекомы (`crate::telecomms`: уровни вещания, сжатие и искажение, `backup_transmission` не-subspace радио, `TRANSMISSION_SUPERSPACE` радио ЦК); нет проверки видимости (`get_hearers_in_LOS`), задержки резервной передачи 2 с

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
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/radio.rs`: интеркомы из карты с их переменными, питание зоны, ретрансляция, `:i`, окно настройки по клику пустой рукой (`crate::radio_ui`; частота заперта у исповедальни и допросной, свободный диапазон у командного); нет монтажа/демонтажа и рамки, открытия силиконами

### Окно радио (tgui Radio)
Включение, смена частоты, микрофон/динамик, громкий режим, выбор каналов гарнитуры.

- **Код:** [`/obj/item/radio/ui_interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L477), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L505); tgui: [`Radio.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Radio.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/radio_ui.rs` (окно `Radio`), `crates/ss13_game/src/radio.rs` (`apply_act`): частота шагами и вводом (`sanitize_frequency`, свой диапазон, `freqlock`), настройка на канал ключа, микрофон, динамик, каналы ключей; рация в руке открывает окно (`attack_self`), интерком — кликом; окно держится, пока рация у пользователя (`inventory_state`); нет громкого режима (`use_command`) и переключателя subspace (радио боргов нет); tg не читает параметр `tune` — перенесено задуманное

### Ручные рации и прочие радио
Станционная ручная рация, радио киборга, развлекательное радио (канал ТВ), игрушечное; переключение частоты, занятые частоты.

- **Код:** [`/obj/item/radio`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L3), [`/obj/item/radio/borg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L688), [`/obj/item/radio/entertainment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L719), [`proc/set_frequency()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L151)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/radio.rs`: ручные рации и `:r`/`:l` из руки, смена частоты в окне радио (`crate::radio_ui`), развлекательное радио с запертой частотой; нет радио боргов и занятых частот

### Глушилки и EMP радио
EMP временно выключает радио; глушилка радиосвязи блокирует передачу (форк: белый список каналов).

- **Код:** [`/obj/item/radio/emp_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L651)
- **BandaStation:** [`radio.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L354) — глушилка учитывает частоту
- **Статус в ss13-rs:** 🟡 частично — ЭМИ (`crates/ss13_game/src/emp/mobs.rs`): рация выключается, все каналы перестают прослушиваться, в руках или на теле — «перегружается», через 20 с после последнего импульса включается (каналы остаются выключены, как в tg); нет глушилки

### Радиошум и звук радио
Звук-помеха при передаче; форк убирает часть ванильной логики шума.

- **Код:** [`radio.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L68)
- **BandaStation:** [`radio.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/radio.dm#L453) — удалённый блок
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/radio.rs` (`radio_noise`): живой говорящий в рацию (кроме суперпространства ЦК) слышит `radio_talk.ogg` с низкой случайной частотой (`get_rand_frequency_low_range`), не чаще раза в 0,5 с на рацию, глухой не слышит, развлекательные рации (`radio_noise = FALSE`) молчат; на всех частотах, звуки приёма убраны — как в BandaStation. Нет настройки громкости `sound_radio_noise` (громкость 100) и приглушённой речи динамиков развлекательного ТВ (нет самих динамиков)

### Электропак
Ошейник-шокер на спину, срабатывает по сигналу сигнализатора на своей частоте.

- **Код:** [`/obj/item/electropack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/radio/electropack.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/electric_chair.rs` (`Electropack`, `electropack_signal` из `post_signal`): частота 144.9, код 2, включён; надетый или в руке — шаг в случайную сторону, искры, паралич 10 с, откат 10 с; в электрошоковом наборе — импульс набора. Нет: окна `Electropack` (питание, частота, код), «нужна помощь, чтобы снять» со спины, суицида
