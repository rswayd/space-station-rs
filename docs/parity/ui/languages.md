# Языки
Система языков: кто какие языки понимает и на каких говорит, искажение непонятной речи, меню языков, учебники.

### Датум языка и искажение речи
`/datum/language`: ключ, иконка, «слоги» для скрамбла непонятной речи, флаги (только письменный, табу для призраков); непонимающий слышит набор слогов.

- **Код:** [`/datum/language`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/language/_language.dm#L7), [`/atom/movable/proc/translate_language()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/say.dm#L311)
- **Статус в ss13-rs:** ❌ нет — вся речь на общем языке

### Хранилище языков
`/datum/language_holder`: понимаемые и разговорные языки моба от вида, органа-языка, квирков, предметов; источники (`LANGUAGE_MIND`, `LANGUAGE_ATOM`...), выбранный язык по умолчанию.

- **Код:** [`/datum/language_holder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/language/_language_holder.dm#L38), [`/datum/language_holder/alien`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/language/_language_holder.dm#L356)
- **Статус в ss13-rs:** ❌ нет

### Языки рас и существ
Галактический общий и необщий, Драконик, Моффик, Нэкомиметик, Сильван, Террум, Кальциевый, Вольтаический, Слаймовый, Шэдоутонг, Грибной, Механический, Дронский, Ксеноморфский, Нар'Сианский, Карповый, Обезьяний, Пиратский, Пляжный, Червиный, Кодспик, Буззворды, Афазия.

- **Код:** [`code/modules/language`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/language), [`/datum/language/common`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/language/common.dm#L2), [`/datum/language/draconic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/language/draconic.dm#L1)
- **BandaStation:** [`spinwarder.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/language_improvements/code/spinwarder.dm#L1) — Неорусский (язык КССП); языки рас форка — модпак `species` (см. ../mobs/)
- **Статус в ss13-rs:** ❌ нет

### Меню языков
TGUI-окно: известные языки, выбор языка по умолчанию; админ может выдавать/забирать языки.

- **Код:** [`/datum/language_menu`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/language/_language_menu.dm#L1); tgui: [`LanguageMenu.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/LanguageMenu.tsx)
- **Статус в ss13-rs:** ❌ нет

### Учебники языков и билингвизм
Книги-учебники (кодспик, языки рас, пиратский, дронский) учат языку; квирк «Билингв» даёт второй язык.

- **Код:** [`/obj/item/language_manual`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/language/_language_manuals.dm#L1), [`/obj/item/language_manual/codespeak_manual`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/language/_language_manuals.dm#L53), [`/datum/quirk/bilingual`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/quirks/positive_quirks/bilingual.dm#L1)
- **BandaStation:** [`bilingual_add.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/language_improvements/code/bilingual_add.dm#L1) — Неорусский в выборе билингва
- **Статус в ss13-rs:** ❌ нет
