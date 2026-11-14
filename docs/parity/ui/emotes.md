# Эмоции
Эмоции (`*key`, верб `me`, панели эмоций): сообщения, звуки, кулдауны, ограничения по состоянию и виду.

### Механизм эмоций
`/datum/emote`: ключи (вкл. русские), сообщение и вариант с параметром, тип (видимый/слышимый), ограничения (руки, рот, состояние, вид), кулдаун, звук, отображение в runechat.

- **Код:** [`/datum/emote`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/emotes.dm#L12), [`/datum/emote/proc/run_emote()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/emotes.dm#L109), [`/mob/proc/emote()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/emote.dm#L15)
- **BandaStation:** [`emotes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/emotes.dm#L109) — `message_override` для кастомных эмоций
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_chat/src/emote.rs` (`*key`, `*key текст`, кулдауны (свои у `taunt`, `point`, `dance`, `screech`/`roar`...) и общий звуковой кулдаун BandaStation, русские сообщения, ограничения рук, крита и сна — `SpeakerStat::Unconscious`, требования к органам, одежде и трейтам — биты `HAS_*` у `Emote::needs`/`EmoteUser::has`, звук `get_sound` с немотой и кляпом, runechat, сообщения роботов/ИИ/ксеноморфов/обезьян `message_monkey`, мим-варианты `message_mime` под обетом мима, `sign`/`signal` только с числом); `EmoteUser` тела (органы, очки, кошачьи уши, вид, обет `MimeVow`) и эффекты `run_emote` — `crates/ss13_game/src/speech.rs` (`sync_emote_user`, `apply_emote_effect`, `emote_spammed` для `check_cooldown` у `flip`/`spin`); эмоции от кода (`mob.emote(key)` из боли, болезней, дыхания, наркотиков, тазера...) идут тем же путём — `run_emote` (без кулдаунов, как неумышленные в tg). Нет оверлеев эмоций (`tongue`, `blush`, `cry`, `sigh`, `glasses`) — в порте нет временных оверлеев частей тела

### Эмоции живых и людей
~74 эмоций `living` (кивок, вздох, смех, кашель, падение в обморок, «collapse», «taunt», ...), ~36 человеческих (плач, крик, шипение, щёлканье пальцами, хлопки, салют, ...), ~20 карбоновых.

- **Код:** [`emote.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/emote.dm#L1), [`human emote.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/emote.dm#L1), [`carbon emote.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/emote.dm#L1)
- **BandaStation:** [`human.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/emote_panel/code/human.dm#L1) — звуки эмоций по полу (вздох, плач, смех...)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_chat/src/emote.rs`: все эмоции `living`/`carbon`/`human`/`mob` tg и BandaStation (`laughevil` со звуками злого смеха по виду и полу, `sniffle`) с текстами BandaStation (недостающие переводы — свои: `tongue`, `halt`, `purr`, мим-строки `meow`/`chitter`/`squeak`), флагами и звуками (`human_sounds`, салют, `flare`/`flicker`, `meow`/`purr` кошек, человеческий `roar` — звук крика); условия `can_run_emote`: глаза (`blink`, `blink_r`), язык (`tongue`), очки `glasses/regular`/`sunglasses` (`glasses`), свечение эфириалов/люстров (`glow`, `flare`, `flicker`), `TRAIT_CAT_EMOTES_ALLOWED` (кошки, язык фелинида, кошачьи уши), `TRAIT_SIMIAN` (обезьяны, гориллы: `gnarl`, `roll`, `scratch`, `sign`), эмоции ксеноморфов (`roll`, `scratch`, `sign`, `tail`). Эффекты — `crates/ss13_game/src/speech.rs`: `flip`/`backflip` (поворот на 360°, падение/пошатывание и 1 урона при спаме), `spin`/`taunt`/`dance` (смена направления по таймеру `Spinning`; у `spin` при спаме головокружение и спутанность, рвота при спутанности > 30 с; `dance` ещё и дрожит), `collapse` (без сознания 4 с), `faint` (сон 20 с), `sway`/`tremble`/`shiver`/`twitch`/`twitch_s` (сдвиги `pixel_w`). Упрощения: `point` без рук (ногой/взглядом/языком) не бывает — руки в порте блокирует только крит, который и так запрещает `point`; у `glow`/`flare`/`flicker` нет смены света (у свечения эфириалов нет света в порте); `blink` не рисует веки; `flap`/`aflap` — только текст, `wing` не перенесён (крыльев-органов нет); нет `TRAIT_NO_EYELIDS`, звука Вильгельма (1% у мужчины-человека), запрета `laugh`/`laughevil` немому `can_speak`; `noogie`, `hand`, `kiss` — у предметов-жестов (`devices::hand_items`)

### Эмоции силиконов и существ
Эмоции ИИ (дисплей), киборгов (бип, пинг, сирена), мозга, слаймов, ксеноморфов, животных.

- **Код:** [`ai emote.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/emote.dm#L1), [`robot emote.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/emote.dm#L1), [`slime emote.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/slime/emote.dm#L1)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_chat/src/emote.rs` (типы мобов `EmoteMob`, `TRAIT_SILICON_EMOTES_ALLOWED`, эмоции ИИ/силиконов/мозга в процессоре/ксеноморфов/слаймов/горилл, `message_robot`/`_AI`/`_alien`/`_larva`/`_animal`, звуки, общий аудио-кулдаун 3 с), `crates/ss13_game/src/speech.rs` (`EmoteUser` тела, звук, эмоция дисплеев ИИ `AiDisplayEmotion`, настроение слайма `SlimeMood`). Отрисовка статус-дисплеев ИИ — в «Инфо станции» (comms), лица слайма — в ксенобиологии; ксеноморфов как мобов пока нет

### Эмоции рас BandaStation
Звуковые эмоции вульпканинов, скреллов, таяран, ящеров, молей, подов, слаймолюдей.

- **Код:** [`modular_bandastation/emote_panel/code`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/emote_panel/code)
- **BandaStation:** [`_emote_panel.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/emote_panel/_emote_panel.dm#L1)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_chat/src/emote.rs`: эмоции вульпканинов, таяран и скреллов (белый список вида, свои кулдауны, звуки), звуки языков (`tongue_sounds`: по виду и полу) вульпканинов, таяран, скреллов, ящеров, молей, подов и слаймолюдей; вид и пол тела — `crates/ss13_game/src/speech.rs`; мим-варианты таяран и вульпканинов (`message_mime`)

### Вербы-панель эмоций (BandaStation)
Вкладка «Эмоции» с вербами «▷ Смеяться», «▷ Плакать», «▷ Кашлять», ... для быстрого вызова.

- **Код:** [`emote_verbs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/emote_panel/code/emote_verbs.dm#L1)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/statpanel.rs`: вкладка «Эмоции» статпанели у людей, вербы BandaStation (общие, таяран, вульпканинов, моли, хвосты), отсортированы по имени; кнопка проходит тот же путь, что `*key` (кулдауны, мут, стат). Вербы эмоций, которых в порте ещё нет, скрыты и появятся вместе с ними; мёртвые `GAME_VERB_PROC` (flap, ping, beep, buzz) не добавляются никем и не портированы

### Кастомная панель эмоций в чате (BandaStation)
Панель в окне чата: до 45 своих эмоций (стандартная, стандартная с изменённым текстом, произвольное действие), переименование, сохранение в префах, кулдауны.

- **Код:** [`/datum/tgui_panel/proc/populate_all_emotes_list()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/custom_emote_panel/code/custom_emote_panel.dm#L43)
- **BandaStation:** [`_custom_emote_panel.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/custom_emote_panel/_custom_emote_panel.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_chat/src/client/settings_ui.rs` (кнопка панели в чате, кнопки по имени: обычная синяя, с текстом фиолетовая, `*me` оранжевая, подсказки; «+» и ПКМ открывают форму добавления/правки: тип, ключ, текст, название, удалить; проверки и предупреждения BandaStation, максимум 45, длины 128/32), `crates/ss13_chat/src/client/prefs.rs` (список в `chat.emotes`), `ClientIntent::PanelEmote` (`crates/ss13_shared/src/protocol.rs`) → `crates/ss13_chat/src/server.rs` (`say_request`: путь `*key` с проверками эмоции, `message_override` для своего текста, кулдаун 0.4/1 с «Не так быстро!», только живым): список хранится в клиентских настройках, а не в префах на сервере (префов в порте нет — перенести с меню настроек). Эмоция выбирается из списка всех эмоций с поиском, как `all_emotes` в `tgui_input_list`: `crates/ss13_chat/src/emote_names.rs` (ключи и русские названия из `ru_emotes.toml`, тест сверяет с таблицей сервера); название по умолчанию — имя эмоции

### Клавиши эмоций
Настраиваемые хоткеи для эмоций (категория «Emote» в кейбиндингах).

- **Код:** [`emote.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/emote.dm#L1)
- **Статус в ss13-rs:** ❌ нет — в tg/BandaStation у всех клавиш эмоций по умолчанию `UNBOUND_KEY` (`link_to_emote`), их назначают только переназначением в префах; в порте переназначения нет (группа «Система кейбиндингов», `docs/parity/ui/keybindings.md`), поэтому дефолтных привязок для переноса нет. Сам вызов по клавише — тот же путь, что `*key` (`crates/ss13_chat/src/emote.rs`)
