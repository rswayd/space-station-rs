# Осмотр, эмоции и SSD
Осмотр мобов и предметов, эмоции и индикатор вышедших из игры. Речь, языки и радио — у агента round/ui.

_«Осмотр (examine)» описано в [../ui/examine.md](../ui/examine.md)._

### Подробный осмотр (examine_more)
Повторный осмотр (или с модификатором) даёт расширенное описание: шрамы, особенности, подробности предметов.

- **Код:** [`proc/examine_more()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atom/atom_examine.dm#L158), [`carbon/examine_more()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/examine.dm#L303), [`human/examine_more()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/examine.dm#L560)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/examine.rs` (`RecentExamines`, `examine_more_lines`): повторный осмотр того же в течение 1 с (`EXAMINE_MORE_WINDOW`) показывает подробности вместо обычного осмотра: шрамы карбона (`crates/ss13_game/src/medical/wounds/scars.rs`), у человека — искрящие датчики незакрытого комбинезона, незакрытые одеждой кибернетические конечности, видимый возраст (по записи экипажа, лицо не скрыто); пусто — «Вы осматриваете … подробнее, но не находите ничего интересного...». `// shortcut:` нет настройки `remove_double_click`; описания материалов для исследовательского сканера. Чужое: собственные `examine_more` предметов (лор оружия, устройств и т.п.) — группы этих предметов

### Осмотр с медицинским и охранным HUD
С медицинским или охранным HUD осмотр человека добавляет статус из записей экипажа и ссылки на изменение статуса/записей.

- **Код:** [`human/get_medhud_examine_info()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/examine.dm#L2)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/records/sechud.rs` (общий блок `get_hud_examine_info`: заголовок по надетым HUD — «Медицинский и безопасности анализы», «Медицинский анализ», «Анализ безопасности», должность, охранная часть со ссылками — см. ../security/records-huds.md «Изменение статуса через HUD при осмотре») и `crates/ss13_game/src/records/medhud.rs` (медицинская часть: видимые кибер-импланты, ссылки на физический/ментальный статус записи или «[Запись отсутствует]», «[Медицинское обследование]» — ушибы/ожоги по частям тела, удушье, отравление, «[Показать черты]», сломанные датчики костюма; клики `examine::chat_link` перепроверяются: ссылке ≤ 1 мин, человек с мед-HUD в сознании, не цель; статусы и черты — с `ACCESS_MEDICAL` на ID, ответ на запрос статуса перепроверяется). `// shortcut:` черты показываются именами (у портированных черт нет `medical_record_text` и флага скрытия от сканера); нет ссылок на фото записи; наблюдатели с HUD не моделируются

_«Панель осмотра и флавор-текст (BandaStation)» описано в [../ui/examine.md](../ui/examine.md)._

_«SSD-индикатор (BandaStation)» описано в [../ui/misc-ui.md](../ui/misc-ui.md)._

_«Эмоции» описано в [../ui/emotes.md](../ui/emotes.md)._

### Панель эмоций (BandaStation)
Модпак форка: вкладка «Эмоции» с глаголами для каждой эмоции и окно пользовательских эмоций.

- **Код:** [`modular_bandastation/emote_panel/code/emote_verbs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/emote_panel/code/emote_verbs.dm#L2); tgui: [`EmotePanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/EmotePanel.tsx)
- **BandaStation:** [`modular_bandastation/custom_emote_panel/code/custom_emote_panel.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/custom_emote_panel/code/custom_emote_panel.dm#L1) — пользовательские эмоции
- **Статус в ss13-rs:** ❌ нет
