# Речь
Произнесение и слышимость речи: `say`, шёпот, пение, префиксы радио, режимы речи, мёртвый чат, индикатор набора, окно ввода речи и модификаторы речи.

### Произнесение речи и слышимость
`/atom/movable/proc/say` → `send_speech` → `Hear` у всех в `view`: составление сообщения (имя, глагол «говорит/спрашивает/восклицает», язык), дальность 7.

- **Код:** [`/atom/movable/proc/say()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/say.dm#L45), [`/atom/movable/proc/send_speech()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/say.dm#L132), [`/atom/movable/proc/Hear()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/say.dm#L71), [`/atom/movable/proc/compose_message()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/say.dm#L159), [`/atom/movable/proc/say_mod()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/say.dm#L202)
- **BandaStation:** [`say.dm#L61`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/say.dm#L61) — IC-фильтр при санитизации
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_chat/src/server.rs`, `crates/ss13_game/src/speech.rs`: дальность по радиусу через пространственный индекс, без линии видимости (`view`); языки — `crates/ss13_chat/src/language.rs` (перевод для каждого слушателя, ярлык языка)

### Речь живых мобов
`/mob/living/say`: проверки (немота, крит), префиксы, режимы, шёпот в софт-крите, последние слова при уходе в смерть, вывод в радио.

- **Код:** [`/mob/living/say()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_say.dm#L101), [`/mob/living/proc/treat_message()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_say.dm#L530), [`/mob/living/proc/radio()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_say.dm#L553)
- **BandaStation:** [`living_say.dm#L412`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_say.dm#L412) — отправка в TTS
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_chat/src/server.rs`, `crates/ss13_chat/src/speech.rs`: шёпот в софт-крите, молчание в хард-крите; нет последних слов (`succumb`), немоты от предметов/эффектов

### Шёпот и пение
Шёпот (`#`, верб Whisper): полностью слышен на 1 тайл, со звёздочками на 2, «что-то шепчет» дальше; пение (`%`) с нотами.

- **Код:** [`/mob/living/whisper()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_say.dm#L621), [`whisper_verb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob_say.dm#L17)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_chat/src/speech.rs`, `crates/ss13_chat/src/server.rs` (`#`, `%`, затухание шёпота)

### Префиксы радио
`;` — общий канал гарнитуры, `:x`/`.x` — каналы отделов (включая русскую раскладку), `:h` — канал отдела, `:i` — интерком.

- **Код:** [`department_radio_keys`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_say.dm#L3)
- **BandaStation:** [`living_say.dm#L16`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_say.dm#L16) — канал юстиции и клавиша «д»
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_chat/src/speech.rs` (`RADIO_EXTENSION`, русская раскладка), `crates/ss13_game/src/radio.rs` (см. ../comms/)

### Режимы речи
Особые каналы: улей генокрадов, разум ксеноморфов, голосовые связки, голопад.

- **Код:** [`/datum/saymode`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/saymode.dm#L1), [`/datum/saymode/changeling`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/saymode.dm#L33), [`/datum/saymode/xeno`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/saymode.dm#L96), [`/datum/saymode/holopad`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/saymode.dm#L166)
- **BandaStation:** [`saymode.dm#L80`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/saymode.dm#L80)
- **Статус в ss13-rs:** 🟡 частично — улей генокрадов (`crates/ss13_chat/src/server.rs`, `crates/ss13_game/src/antag/changeling/server.rs`); нет разума ксеноморфов, голосовых связок, голопада

### Мёртвый чат
Призраки и мёртвые говорят в deadchat, видимый всем мёртвым; админ может отключить.

- **Код:** [`/mob/proc/say_dead()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob_say.dm#L116)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_chat/src/server.rs` (`SpeakerStat::Dead` → deadchat)

### Окно ввода речи (tgui-say) и индикатор набора
Окно ввода речи с каналами (Say/Radio/Me/OOC/LOOC/Admin/Mentor), история, автоподстановка префиксов; индикатор «печатает/думает» над головой.

- **Код:** [`/datum/tgui_say`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_input/say_modal/modal.dm#L23), [`/datum/tgui_say/proc/delegate_speech()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_input/say_modal/speech.dm#L35), [`/client/proc/start_typing()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/verbs/typing.dm#L49), [`/client/proc/start_thinking()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/verbs/typing.dm#L32)
- **BandaStation:** [`modal.dm#L95`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_input/say_modal/modal.dm#L95) — канал менторов
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_chat/src/client/panel.rs` (строка ввода под чатом): нет каналов-переключателей, истории и индикатора набора

### Кастомная эмоция (me)
Верб `me`/`*me` — произвольное действие от третьего лица.

- **Код:** [`me_verb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob_say.dm#L38)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_chat/src/emote.rs` (`*me`)

### Модификаторы речи: языки тела и эффекты
Заикание, невнятность (опьянение), акценты языков (язык-орган: шипение ящеров, жужжание и др.), компонент `speechmod` (акценты из файлов), заглушение маской.

- **Код:** [`/datum/status_effect/speech`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/speech_debuffs.dm#L1), [`/datum/status_effect/speech/stutter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/speech_debuffs.dm#L62), [`/datum/status_effect/speech/slurring`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/speech_debuffs.dm#L211), [`/obj/item/organ/tongue/proc/modify_speech()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/tongue/_tongue.dm#L119), [`/datum/component/speechmod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/speechmod.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Автошипение (BandaStation)
Обновлённое авто-шипение и фразы у языков рас (например, приветствие крысиного языка).

- **Код:** [`autohiss.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/autohiss/code/autohiss.dm#L3)
- **BandaStation:** [`_autohiss.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/autohiss/_autohiss.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Фильтр речи (BandaStation)
Элемент на живых мобах заменяет запрещённые слова/«брейнрот» по регулярке из конфига; обход для списка ckey.

- **Код:** [`/datum/element/speech_filter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/speech_filter/code/speech_filter.dm#L3), [`/datum/element/speech_filter/proc/filter_speech()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/speech_filter/code/speech_filter.dm#L30)
- **BandaStation:** [`_speech_filter.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/speech_filter/_speech_filter.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Поддержка кириллицы (BandaStation)
Перевод русской раскладки в английские клавиши для команд и префиксов, исправления регистра и ввода кириллицы.

- **Код:** [`convert_ru_key_to_en_key()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/cyrillic_fixes/code/cyrillic_fixes.dm#L7)
- **BandaStation:** [`_cyrillic_fixes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/cyrillic_fixes/_cyrillic_fixes.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_chat/src/speech.rs`: русская раскладка в радио-префиксах; остальных исправлений нет
