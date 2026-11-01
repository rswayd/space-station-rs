# Осмотр
Shift-клик по объекту: описание, подробный осмотр, осмотр людей (одежда, раны, состояние) и панель осмотра BandaStation.

### Осмотр объектов
`examine`: «Это <имя>.», описание, теги (размер, свойства), сигналы для дополнений компонентов; `examine_more` — подробный осмотр по повторному осмотру.

- **Код:** [`/mob/proc/run_examinate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob.dm#L558), [`/atom/proc/examine()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atom/atom_examine.dm#L15), [`/atom/proc/examine_tags()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atom/atom_examine.dm#L99), [`/atom/proc/examine_more()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atom/atom_examine.dm#L158)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/examine.rs` (заголовок, описание, класс веса, поворот к цели): нет `examine_more` и большинства дополнений компонентов

### Осмотр людей
Одежда и предметы в руках, повреждения по частям тела, кровотечение, наручники, захват, крит/смерть, SSD, ID-карта, состояние (пьян, бледен...), секретные метки (HUD СБ/медицины).

- **Код:** [`/mob/living/carbon/examine()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/examine.dm#L7), [`human examine.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/examine.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/examine.rs` (одежда, руки, повреждения, захват, крит, смерть, SSD со строками BandaStation): нет ран/кровотечения, HUD-записей, состояний

### Панель осмотра (BandaStation)
Кнопка в осмотре открывает TGUI-окно с превью персонажа, флафф-текстом (для людей и силиконов) и OOC-заметками из префов.

- **Код:** [`/datum/component/examine_panel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/examine_panel/code/examine_panel_component.dm#L1), [`/datum/preference/text/flavor_text`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/examine_panel/code/examine_panel_prefs.dm#L1), [`/datum/preference/text/silicon_flavor_text`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/examine_panel/code/examine_panel_prefs.dm#L10); tgui: [`ExaminePanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ExaminePanel.tsx)
- **BandaStation:** [`_examine_panel.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/examine_panel/_examine_panel.dm#L1)
- **Статус в ss13-rs:** ❌ нет
