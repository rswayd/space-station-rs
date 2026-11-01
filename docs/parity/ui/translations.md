# Переводы (BandaStation)
Модпак `translations`: русские названия предметов со склонениями, глаголы, эмоции, местоимения, режимы речи, советы.

### Русские названия и склонения (ru_names)
Каждый атом может иметь 6 падежей и род (`declent_ru`); названия берутся из `ru_names.toml`, который генерируется из ~4400 фрагментов в `translations/public/ru_names`; отдельно — реагенты, материалы, дизайны, заказы.

- **Код:** [`/datum/proc/declent_ru()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/code/ru_names/ru_name_base.dm#L69), [`ru_names_list()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/code/ru_names/ru_name_base.dm#L9), [`modular_bandastation/translations/public/ru_names`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/public/ru_names)
- **BandaStation:** [`_translations.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/_translations.dm#L1)
- **Статус в ss13-rs:** ❌ нет — названия не склоняются (`// shortcut:` в `crates/ss13_game/src/internals.rs`)

### Русские глаголы
Глаголы атаки, еды, речи, броска (`ru_verbs.toml`) во второй и третьей форме; русские глаголы речи «говорит/спрашивает».

- **Код:** [`ru_attack_verb()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/code/translate_verbs.dm#L22), [`ru_say_verb()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/code/translate_verbs.dm#L31), [`ru_verbs.toml`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/public/ru_verbs.toml#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/food.rs` (`ru_eat_verb`), `crates/ss13_game/src/throw.rs` (глаголы броска): нет глаголов атаки и речи

### Русские эмоции
Тексты эмоций на русском (`ru_emotes.toml`) и русские ключи эмоций.

- **Код:** [`translate_emotes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/code/translate_emotes.dm#L1), [`ru_emotes.toml`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/public/ru_emotes.toml#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_chat/src/emote.rs` (сообщения из `ru_emotes.toml` для реализованных эмоций)

### Местоимения, части тела и режимы речи
Русские местоимения по роду, названия частей тела, перевод режимов речи и уровней тревоги.

- **Код:** [`pronouns.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/code/pronouns.dm#L1), [`bodyparts.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/code/bodyparts.dm#L1), [`saymode.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/code/saymode.dm#L1), [`translate_security_level.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/code/translate_security_level.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/zone.rs` (падежи частей тела): нет местоимений по роду и режимов речи

### Советы и предложения переводов
Русские советы (tips) в лобби/при входе и верб предложения перевода названия предмета.

- **Код:** [`tips.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/code/tips.dm#L1), [`translate_suggest_ru_names.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/code/translate_suggest_ru_names.dm#L1)
- **Статус в ss13-rs:** ❌ нет
