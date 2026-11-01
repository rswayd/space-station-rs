# Интерфейс и общение

Всё, чем игрок пользуется для взаимодействия с игрой и другими игроками: TGUI и окна ввода, чат и runechat, речь и её модификаторы, языки, эмоции, предпочтения и настройка персонажа, лоадаут, клавиши, HUD, осмотр, подсказки, статпанель, меню Escape и переводы BandaStation. Включает модпаки `preferences`, `customization`, `augmentation_preferences`, `loadout`, `emote_panel`, `custom_emote_panel`, `chat`, `emojis220`, `examine_panel`, `hud`, `language_improvements`, `autohiss`, `speech_filter`, `cyrillic_fixes`, `tgui_who`, `ssd_indicator`, `navigator`, `translations`. В ss13-rs есть свой TGUI (`crates/ss13_tgui`), чат и речь (`crates/ss13_chat`), HUD человека и осмотр (`crates/ss13_game/src/{hud,examine,inventory_client,health_client}.rs`).

| Файл | Что внутри |
| --- | --- |
| [tgui.md](tgui.md) | Ядро TGUI, состояния, интерфейсы, темы, окна ввода (alert, текст, список, цвет), панель TGUI |
| [chat.md](chat.md) | Доставка сообщений, панель чата, OOC, бейджи BandaStation, runechat |
| [speech.md](speech.md) | Речь, шёпот, радио-префиксы, режимы речи, мёртвый чат, tgui-say, модификаторы, фильтр и кириллица |
| [languages.md](languages.md) | Языки, хранилище языков, меню, учебники |
| [emotes.md](emotes.md) | Эмоции, списки по мобам, панели эмоций BandaStation |
| [preferences.md](preferences.md) | Предпочтения, страницы профессий/антагов/внешности, аугментации, игровые настройки |
| [loadout.md](loadout.md) | Лоадаут и его расширения BandaStation |
| [keybindings.md](keybindings.md) | Кейбиндинги и подсистема ввода |
| [hud.md](hud.md) | HUD, слоты, здоровье, кнопки действий, алерты, полноэкранные эффекты, радиальные меню, миникарта |
| [examine.md](examine.md) | Осмотр объектов и людей, панель осмотра BandaStation |
| [misc-ui.md](misc-ui.md) | Screentips, тултипы, баллоны, статпанель, Escape, Who, SSD, указание, лут, сны, эмодзи, навигатор |
| [translations.md](translations.md) | Русские названия и склонения, глаголы, эмоции, местоимения, советы |
