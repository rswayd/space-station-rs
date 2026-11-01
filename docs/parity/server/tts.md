# Озвучка речи (TTS, BandaStation)
Модпак `tts`: речь персонажей, объявления, радио и телепатия озвучиваются голосами через провайдера Silero с постобработкой ffmpeg.

### Подсистема TTS
`SStts220`: очередь запросов к API провайдера, кэш файлов, лимиты, проигрывание у слушателей с учётом дистанции и каналов, санитизация текста (числа прописью, акронимы, замены из `tts_replacements.json`).

- **Код:** [`SUBSYSTEM_DEF(tts220)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/tts_subsystem.dm#L9), [`/datum/controller/subsystem/tts220/proc/queue_request()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/tts_subsystem.dm#L218), [`/datum/controller/subsystem/tts220/proc/play_tts()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/tts_subsystem.dm#L364), [`/datum/controller/subsystem/tts220/proc/sanitize_tts_input()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/tts_subsystem.dm#L565), [`numbers.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/numbers.dm#L1), [`tts_configuration.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/tts_configuration.dm#L1)
- **BandaStation:** [`_tts.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/_tts.dm#L1)
- **Статус в ss13-rs:** ❌ нет — звука нет

### Провайдер Silero
HTTP-провайдер Silero (токен, URL в конфиге), свой HTTP-подсистема для запросов.

- **Код:** [`/datum/tts_provider`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/tts_provider.dm#L1), [`/datum/tts_provider/silero`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/providers/silero.dm#L1), [`SSHttp.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/SSHttp.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Голоса (сиды)
848 голосов Silero по категориям (персонажи игр и фильмов и т.д.) и полу; часть голосов требует уровня доната; голоса по умолчанию для мобов и объектов.

- **Код:** [`/datum/tts_seed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/seeds/base.dm#L1), [`/datum/tts_seed/silero`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/seeds/silero.dm#L1), [`seeds/silero.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/seeds/silero.dm#L1), [`modular_bandastation/tts/code/base_seeds`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/base_seeds)
- **Статус в ss13-rs:** ❌ нет

### Голос персонажа и компонент TTS
Компонент TTS на говорящем: выбранный голос (из префов или случайный по полу), смена голоса действием (обычная/роботизированная/бесполая), подмена голоса при мимикрии.

- **Код:** [`/datum/component/tts_component`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/tts_component.dm#L1), [`/datum/action/innate/voice_change`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/actions/change_tts_actions.dm#L1), [`/datum/preference/text/tts_seed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/preferences/tts_preferences.dm#L1), [`/datum/preference_middleware/text_to_speech`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/preferences/tts_preference_middleware.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Эффекты обработки
ffmpeg-эффекты: радио, робот, мегафон, телепатия, объявление; громкость по каналам (речь, радио, объявления, телепатия) в настройках.

- **Код:** [`shell.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/shell.dm#L1), [`/datum/singleton/sound_effect/radio`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/shell.dm#L50), [`/datum/preference/numeric/volume/sound_tts_volume_announcement`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/preferences/tts_preferences.dm#L28)
- **Статус в ss13-rs:** ❌ нет

### Интеграция с речью
Хуки в `say`, радио, шёпоте, телепатии, объявлениях, мимикрии генокрада, культе, блобе; вычищение разметки ударений из чата и runechat; админский сброс TTS (world topic `fixtts`).

- **Код:** [`/atom/movable/proc/do_tts_message()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/say.dm#L114), [`/atom/movable/proc/get_tts_voice()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/say.dm#L129)
- **BandaStation:** [`living_say.dm#L412`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_say.dm#L412)
- **Статус в ss13-rs:** ❌ нет
