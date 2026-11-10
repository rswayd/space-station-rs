# Осмотр, эмоции и SSD
Осмотр мобов и предметов, эмоции и индикатор вышедших из игры. Речь, языки и радио — у агента round/ui.

_«Осмотр (examine)» описано в [../ui/examine.md](../ui/examine.md)._

### Подробный осмотр (examine_more)
Повторный осмотр (или с модификатором) даёт расширенное описание: шрамы, особенности, подробности предметов.

- **Код:** [`proc/examine_more()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atom/atom_examine.dm#L158), [`carbon/examine_more()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/examine.dm#L303), [`human/examine_more()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/examine.dm#L560)
- **Статус в ss13-rs:** ❌ нет

### Осмотр с медицинским и охранным HUD
С медицинским или охранным HUD осмотр человека добавляет статус из записей экипажа и ссылки на изменение статуса/записей.

- **Код:** [`human/get_medhud_examine_info()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/examine.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — охранная часть с ссылками готова (`crates/ss13_game/src/records/sechud.rs`, ссылки чата `examine::chat_link`; см. ../security/records-huds.md «Изменение статуса через HUD при осмотре»); нет медицинской части (физический/ментальный статус, обследование, черты) — группа medical, механизм ссылок можно взять готовый

_«Панель осмотра и флавор-текст (BandaStation)» описано в [../ui/examine.md](../ui/examine.md)._

_«SSD-индикатор (BandaStation)» описано в [../ui/misc-ui.md](../ui/misc-ui.md)._

_«Эмоции» описано в [../ui/emotes.md](../ui/emotes.md)._

### Панель эмоций (BandaStation)
Модпак форка: вкладка «Эмоции» с глаголами для каждой эмоции и окно пользовательских эмоций.

- **Код:** [`modular_bandastation/emote_panel/code/emote_verbs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/emote_panel/code/emote_verbs.dm#L2); tgui: [`EmotePanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/EmotePanel.tsx)
- **BandaStation:** [`modular_bandastation/custom_emote_panel/code/custom_emote_panel.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/custom_emote_panel/code/custom_emote_panel.dm#L1) — пользовательские эмоции
- **Статус в ss13-rs:** ❌ нет
