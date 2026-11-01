# Бумаги и канцелярия
Бумага, ручки, печати, папки и прочая канцелярия из `code/modules/paperwork` и fork-модпака `paperwork`.

### Лист бумаги
Бумагу пишут ручкой/мелком в tgui-окне (Markdown, ограничение длины, поля ввода `[___]`), её читают, ставят печати, складывают в самолётик (Alt+клик), сжигают. Цвет и шрифт текста зависят от инструмента.

- **Код:** [`/obj/item/paper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L15), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L594), [`add_raw_text()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L176), [`/datum/paper_input`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L738); tgui: [`PaperSheet`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PaperSheet)
- **BandaStation:** [`modular_bandastation/paperwork/code/paperwork.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/paperwork/code/paperwork.dm#L10) — `replace_text_keys()`: ключи `[name]`, `[age]`, `[sign]`, `[time]`, `[date]`, `[station_name]`, `[bank_id]`, `[job]`, `[department]`, `[species]`, `[gender]` в шаблонах бумаги подставляются данными игрока
- **Статус в ss13-rs:** ❌ нет — бумага на карте только предмет, письма нет

### Готовые бумаги и инструкции
Предзаполненные листы на карте: SOP, инструкции по работе, записки, мятая и окровавленная бумага.

- **Код:** [`/obj/item/paper/fluff/sop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper_premade.dm#L5), [`/obj/item/paper/crumpled`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L903)
- **BandaStation:** [`modular_bandastation/objects/code/items/papers.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/papers.dm#L2) — бумаги ЦК на русском
- **Статус в ss13-rs:** ❌ нет

### Ручки
Ручки разных цветов, четырёхцветная, перьевая, невидимые чернила, угольный стилус; клик в руке щёлкает колпачком. Ручки можно метнуть/воткнуть (embedding).

- **Код:** [`/obj/item/pen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/pen.dm#L13), [`/obj/item/pen/fourcolor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/pen.dm#L125), [`/obj/item/pen/fountain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/pen.dm#L154)
- **BandaStation:** [`code/modules/paperwork/pen.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/pen.dm#L158) — читаемый шрифт перьевой ручки; [`modular_bandastation/objects/code/items/pen.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/pen.dm#L1) — капитанская ручка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: ручка только помещается в слот хранения костюма, писать ею нельзя

### Печати
Штампы глав, «Одобрено»/«Отказано», юриста и др. ставятся на бумагу в tgui-окне листа в выбранное место с поворотом.

- **Код:** [`/obj/item/stamp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/stamps.dm#L1), [`/obj/item/paper/proc/add_stamp()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L205), [`/datum/paper_stamp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L862)
- **BandaStation:** [`modular_bandastation/objects/code/items/stamp.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/stamp.dm#L1) — печати представителя NT, магистрата, ТСФ, СССП
- **Статус в ss13-rs:** ❌ нет

### Планшет (clipboard)
Держит стопку бумаг и ручку; бумагу на верху можно писать и подписывать, не вынимая (tgui `Clipboard`).

- **Код:** [`/obj/item/clipboard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/clipboard.dm#L21); tgui: [`Clipboard.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Clipboard.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: только как допустимый предмет слота костюма, хранения бумаг нет

### Папки
Цветные папки хранят бумаги и фото (tgui `Folder`); секретные документы и папки Синдиката — цели краж.

- **Код:** [`/obj/item/folder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/folders.dm#L1), [`/obj/item/folder/documents`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/folders_premade.dm#L21); tgui: [`Folder.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Folder.tsx)
- **Статус в ss13-rs:** ❌ нет

### Конфиденциальный конверт (paper biscuit)
Запечатанный конверт с бумагой; вскрывается необратимо. Вариант с кодом сейфа запасной ID капитана.

- **Код:** [`/obj/item/folder/biscuit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper_biscuit.dm#L1), [`/obj/item/folder/biscuit/confidential/spare_id_safe_code`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper_biscuit.dm#L111)
- **Статус в ss13-rs:** ❌ нет

### Лоток с бумагой
Выдаёт новые листы (и ручку, если вложена); варианты из натуральной бумаги и с копиркой.

- **Код:** [`/obj/item/paper_bin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paperbin.dm#L3), [`/obj/item/paper_bin/carbon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paperbin.dm#L272)
- **Статус в ss13-rs:** ❌ нет

### Копировальная бумага
Лист с копиркой: написанное отрывается как копия.

- **Код:** [`/obj/item/paper/carbon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/carbonpaper.dm#L1), [`/obj/item/paper/carbon_copy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/carbonpaper.dm#L50)
- **Статус в ss13-rs:** ❌ нет

### Бумажный самолётик
Сложенная бумага метается и может попасть в глаз; синдикатский вариант острее.

- **Код:** [`/obj/item/paperplane`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paperplane.dm#L1), [`/obj/item/paper/proc/make_plane()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L325)
- **Статус в ss13-rs:** ❌ нет

### Резак для бумаги
Режет бумагу на полоски (записки, предсказания); лезвие снимается.

- **Код:** [`/obj/item/papercutter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper_cutter.dm#L1), [`/obj/item/paper/paperslip`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper_cutter.dm#L180)
- **Статус в ss13-rs:** ❌ нет

### Картотеки
Шкафы хранят бумаги и папки (tgui `FilingCabinet`); шкафы СБ, медбея и отдела кадров в начале раунда печатают записи экипажа / трудовые контракты.

- **Код:** [`/obj/structure/filingcabinet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/filingcabinet.dm#L13), [`/obj/structure/filingcabinet/security`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/filingcabinet.dm#L125), [`/obj/structure/filingcabinet/employment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/filingcabinet.dm#L185); tgui: [`FilingCabinet.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/FilingCabinet.tsx)
- **Статус в ss13-rs:** ❌ нет

### Трудовой контракт
Контракт на члена экипажа, хранится в картотеке отдела кадров.

- **Код:** [`/obj/item/paper/employment_contract`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/contract.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Ручной этикетировщик
Наклеивает на предмет ярлык (переименование в «name (label)»), заправляется рулоном.

- **Код:** [`/obj/item/hand_labeler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/handlabeler.dm#L2), [`/obj/item/hand_labeler_refill`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/handlabeler.dm#L147)
- **Статус в ss13-rs:** ❌ нет

### Отдельские документы для карго
Документы отделов, которые нужно заверить печатью соответствующего главы; проданные с верной печатью дают деньги карго, без неё — штраф.

- **Код:** [`/obj/item/paperwork`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paperwork.dm#L13), [`/obj/item/paperwork/photocopy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paperwork.dm#L216)
- **Статус в ss13-rs:** ❌ нет
