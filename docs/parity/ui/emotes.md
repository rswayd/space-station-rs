# Эмоции
Эмоции (`*key`, верб `me`, панели эмоций): сообщения, звуки, кулдауны, ограничения по состоянию и виду.

### Механизм эмоций
`/datum/emote`: ключи (вкл. русские), сообщение и вариант с параметром, тип (видимый/слышимый), ограничения (руки, рот, состояние, вид), кулдаун, звук, отображение в runechat.

- **Код:** [`/datum/emote`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/emotes.dm#L12), [`/datum/emote/proc/run_emote()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/emotes.dm#L109), [`/mob/proc/emote()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/emote.dm#L15)
- **BandaStation:** [`emotes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/emotes.dm#L109) — `message_override` для кастомных эмоций
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_chat/src/emote.rs` (`*key`, `*key текст`, кулдаун 3 с, русские сообщения BandaStation, ограничения рук и софт-крита): нет звуков, runechat, оверлеев, вариантов мима/роботов, эффектов

### Эмоции живых и людей
~74 эмоций `living` (кивок, вздох, смех, кашель, падение в обморок, «collapse», «taunt», ...), ~36 человеческих (плач, крик, шипение, щёлканье пальцами, хлопки, салют, ...), ~20 карбоновых.

- **Код:** [`emote.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/emote.dm#L1), [`human emote.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/emote.dm#L1), [`carbon emote.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/emote.dm#L1)
- **BandaStation:** [`human.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/emote_panel/code/human.dm#L1) — звуки эмоций по полу (вздох, плач, смех...)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_chat/src/emote.rs`: 34 распространённые человеческие эмоции, только текст

### Эмоции силиконов и существ
Эмоции ИИ (дисплей), киборгов (бип, пинг, сирена), мозга, слаймов, ксеноморфов, животных.

- **Код:** [`ai emote.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/emote.dm#L1), [`robot emote.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/emote.dm#L1), [`slime emote.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/slime/emote.dm#L1)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_chat/src/emote.rs` (типы мобов `EmoteMob`, `TRAIT_SILICON_EMOTES_ALLOWED`, эмоции ИИ/силиконов/мозга в процессоре/ксеноморфов/слаймов/горилл, `message_robot`/`_AI`/`_alien`/`_larva`/`_animal`, звуки, общий аудио-кулдаун 3 с), `crates/ss13_game/src/speech.rs` (`EmoteUser` тела, звук, эмоция дисплеев ИИ `AiDisplayEmotion`, настроение слайма `SlimeMood`). Отрисовка статус-дисплеев ИИ — в «Инфо станции» (comms), лица слайма — в ксенобиологии; ксеноморфов как мобов пока нет

### Эмоции рас BandaStation
Звуковые эмоции вульпканинов, скреллов, таяран, ящеров, молей, подов, слаймолюдей.

- **Код:** [`modular_bandastation/emote_panel/code`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/emote_panel/code)
- **BandaStation:** [`_emote_panel.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/emote_panel/_emote_panel.dm#L1)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_chat/src/emote.rs`: эмоции вульпканинов, таяран и скреллов (белый список вида, свои кулдауны, звуки), звуки языков (`tongue_sounds`: по виду и полу) вульпканинов, таяран, скреллов, ящеров, молей, подов и слаймолюдей; вид и пол тела — `crates/ss13_game/src/speech.rs`. Без мим-вариантов; звуки `sniff`/`snore`/`hiss` придут с этими эмоциями (группа «Эмоции живых и людей»)

### Вербы-панель эмоций (BandaStation)
Вкладка «Эмоции» с вербами «▷ Смеяться», «▷ Плакать», «▷ Кашлять», ... для быстрого вызова.

- **Код:** [`emote_verbs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/emote_panel/code/emote_verbs.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Кастомная панель эмоций в чате (BandaStation)
Панель в окне чата: до 45 своих эмоций (стандартная, стандартная с изменённым текстом, произвольное действие), переименование, сохранение в префах, кулдауны.

- **Код:** [`/datum/tgui_panel/proc/populate_all_emotes_list()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/custom_emote_panel/code/custom_emote_panel.dm#L43)
- **BandaStation:** [`_custom_emote_panel.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/custom_emote_panel/_custom_emote_panel.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Клавиши эмоций
Настраиваемые хоткеи для эмоций (категория «Emote» в кейбиндингах).

- **Код:** [`emote.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/emote.dm#L1)
- **Статус в ss13-rs:** ❌ нет
