# Предпочтения и настройка персонажа
Окно `PreferencesMenu`: слоты персонажей, внешность, профессии, антагонисты, квирки, игровые настройки; сохранение в savefile.

### Система предпочтений
`/datum/preferences` и датумы `/datum/preference` (тип, категория, значение по умолчанию, применение к персонажу), сохранение и миграции savefile, мидлвари (профессии, антаги, квирки, имена, вид, кейбиндинги, TTS).

- **Код:** [`/datum/preferences`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences.dm#L3), [`/datum/preference`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/_preference.dm#L81), [`/datum/preferences/proc/save_preferences()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences_savefile.dm#L317), [`code/modules/client/preferences/middleware`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/middleware); tgui: [`PreferencesMenu.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PreferencesMenu/index.tsx)
- **BandaStation:** [`modular_bandastation/preferences/code/modules/client/preferences.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/preferences/code/modules/client/preferences.dm#L1) — слоты персонажей 5 + 1 за уровень доната (до 9), очки лоадаута, «Антагонисты» (`antag_roll`)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/round.rs` (`JobPrefsUi`, `JobPreferences`, `AntagPreferences`): только профессии и антаги, хранятся на время соединения; нет сохранения, слотов, остальных страниц

### Страница профессий
Уровни high/medium/low/never для каждой профессии (одна high), ассистент как галочка, поведение при недоступности.

- **Код:** [`jobs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/middleware/jobs.dm#L1), [`jobless_role.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/jobless_role.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/round.rs` (`JobPrefsUi`, `PrefAct::Job`, `PrefAct::Jobless`)

### Страница антагонистов
Включение ролей антагонистов (`be_special`), бан-статус по ролям.

- **Код:** [`antags.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/middleware/antags.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/round.rs` (`PrefAct::Antag`): только «Предатель»

### Внешность и вид
Вид, пол и тип тела, возраст, имя (случайное), цвет кожи, волосы и борода с градиентами, глаза (гетерохромия), нижнее бельё, особенности видов (хвосты, уши, крылья, антенны...), рост, превью персонажа.

- **Код:** [`species.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/species.dm#L1), [`names.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/names.dm#L1), [`gender.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/gender.dm#L1), [`code/modules/client/preferences/species_features`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/species_features)
- **BandaStation:** [`hair.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/customization/code/sprite_accessories/hair.dm#L1) — 124 дополнительные причёски; [`body_type.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/preferences/code/modules/client/preferences/body_type.dm#L1)
- **Статус в ss13-rs:** ❌ нет — тело всегда мужское со случайным именем (`crates/ss13_game/src/jobs.rs`)

### Квирки
Положительные/нейтральные/отрицательные черты с балансом очков; квирки-предпочтения (фобия, протезы, аллергия, шрамы...).

- **Код:** [`quirks.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/middleware/quirks.dm#L1), [`phobia.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/phobia.dm#L1), [`prosthetic_limb.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/prosthetic_limb.dm#L1)
- **Статус в ss13-rs:** ❌ нет (квирки — см. ../mobs/)

_«Аугментации, протезы и ампутации (BandaStation)» описано в [../medical/cybernetics.md](../medical/cybernetics.md)._

### Игровые настройки
Настройки клиента: FPS, параллакс, размер пикселя и масштабирование, звуки (музыка, эмбиент, инструменты), runechat, подсказки, статус-бар, цвет OOC, мигание окна, отображение призраков, тултипы, стиль UI, сглаживание и др.

- **Код:** [`fps.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/fps.dm#L1), [`parallax.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/parallax.dm#L1), [`sounds.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/sounds.dm#L1), [`ui_style.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/ui_style.dm#L1), [`ghost.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/ghost.dm#L1), [`window_flashing.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/window_flashing.dm#L1)
- **BandaStation:** [`sounds.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/sounds.dm#L131)
- **Статус в ss13-rs:** ❌ нет

### Игровые предпочтения ролей
Отдел офицера СБ, расположение аплинка, предпочитаемая карта, ИИ-дисплей/голограмма, вид оперативника, преступление заключённого, PDA-рингтон.

- **Код:** [`security_department.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/security_department.dm#L1), [`uplink_location.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/uplink_location.dm#L1), [`preferred_map.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/preferred_map.dm#L1), [`pda.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/pda.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Экспорт и импорт предпочтений
Верб «Export Preferences» выгружает savefile в JSON.

- **Код:** [`export_preferences`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/verbs/ooc.dm#L421)
- **Статус в ss13-rs:** ❌ нет
