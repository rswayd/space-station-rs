# Бумаги и канцелярия
Бумага, ручки, печати, папки и прочая канцелярия из `code/modules/paperwork` и fork-модпака `paperwork`.

### Лист бумаги
Бумагу пишут ручкой/мелком в tgui-окне (Markdown, ограничение длины, поля ввода `[___]`), её читают, ставят печати, складывают в самолётик (Alt+клик), сжигают. Цвет и шрифт текста зависят от инструмента.

- **Код:** [`/obj/item/paper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L15), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L594), [`add_raw_text()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L176), [`/datum/paper_input`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L738); tgui: [`PaperSheet`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PaperSheet)
- **BandaStation:** [`modular_bandastation/paperwork/code/paperwork.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/paperwork/code/paperwork.dm#L10) — `replace_text_keys()`: ключи `[name]`, `[age]`, `[sign]`, `[time]`, `[date]`, `[station_name]`, `[bank_id]`, `[job]`, `[department]`, `[species]`, `[gender]` в шаблонах бумаги подставляются данными игрока
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/paper.rs`: `Paper` (входы с шрифтом/цветом/жирностью, печати), чтение в руке — окно `PaperSheet` (`crates/ss13_game/src/service/client.rs`, текст без рендера markdown/HTML), ручка/мелок на бумаге — многострочный prompt вместо поля tgui, лимит `MAX_PAPER_LENGTH` 5000, иконка `paper_words`, звук `writing_pen`; поля `[input_field]`: список «в конец / поле N», текст поля — дочерний вход (`add_raw_text_to_input_field`, `crates/ss13_game/src/service/mod.rs` `PaperInput::fill_field`), в окне пустое поле — черта; самолётик по Alt+клику (`crates/ss13_game/src/service/office.rs`); `write_printed` для распечаток; нет autofill полей, подписей, ключей BandaStation `[name]`…, сжигания, переименования, чтения при осмотре

### Готовые бумаги и инструкции
Предзаполненные листы на карте: SOP, инструкции по работе, записки, мятая и окровавленная бумага.

- **Код:** [`/obj/item/paper/fluff/sop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper_premade.dm#L5), [`/obj/item/paper/crumpled`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L903)
- **BandaStation:** [`modular_bandastation/objects/code/items/papers.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/papers.dm#L2) — бумаги ЦК на русском
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/service.rs` (`DmService.text` из `default_raw_text`, и правки карты) → первый вход `Paper`; строки-выражения (`[station_name()]`) пусты, ключи `replace_text_keys` не подставляются

### Ручки
Ручки разных цветов, четырёхцветная, перьевая, невидимые чернила, угольный стилус; клик в руке щёлкает колпачком. Ручки можно метнуть/воткнуть (embedding).

- **Код:** [`/obj/item/pen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/pen.dm#L13), [`/obj/item/pen/fourcolor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/pen.dm#L125), [`/obj/item/pen/fountain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/pen.dm#L154)
- **BandaStation:** [`code/modules/paperwork/pen.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/pen.dm#L158) — читаемый шрифт перьевой ручки; [`modular_bandastation/objects/code/items/pen.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/pen.dm#L1) — капитанская ручка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/paper.rs`: ручки пишут своим цветом и шрифтом (синяя, красная, невидимая, угольная, survival), четырёхцветная переключается в руке, мелки пишут жирно цветом мелка; нет щелчка ручкой, сонной ручки, энергокинжала, ручек в дротиках

### Печати
Штампы глав, «Одобрено»/«Отказано», юриста и др. ставятся на бумагу в tgui-окне листа в выбранное место с поворотом.

- **Код:** [`/obj/item/stamp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/stamps.dm#L1), [`/obj/item/paper/proc/add_stamp()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L205), [`/datum/paper_stamp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L862)
- **BandaStation:** [`modular_bandastation/objects/code/items/stamp.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/stamp.dm#L1) — печати представителя NT, магистрата, ТСФ, СССП
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/paper.rs`: печать на бумаге ставится сразу (без выбора места в окне), до `MAX_PAPER_STAMPS` 30, первые 5 — оверлеи `paper_<icon_state>`, звук; в окне печати перечислены по имени

### Планшет (clipboard)
Держит стопку бумаг и ручку; бумагу на верху можно писать и подписывать, не вынимая (tgui `Clipboard`).

- **Код:** [`/obj/item/clipboard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/clipboard.dm#L21); tgui: [`Clipboard.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Clipboard.tsx)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/service/office.rs`: бумаги внутри (последняя — сверху), одна ручка; бумага прикрепляется, ручка вставляется, прочее применяется к верхней бумаге; в руке список вместо tgui `Clipboard`: прочитать, писать ручкой планшета, вынуть, наверх, вынуть ручку; оверлеи верхнего листа, `clipboard_pen`, `clipboard_over`; планшет на бумаге подбирает её (без скинов-перекраски)

### Папки
Цветные папки хранят бумаги и фото (tgui `Folder`); секретные документы и папки Синдиката — цели краж.

- **Код:** [`/obj/item/folder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/folders.dm#L1), [`/obj/item/folder/documents`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/folders_premade.dm#L21); tgui: [`Folder.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Folder.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/office.rs`: бумаги внутри (и папка, кликнутая по бумаге), в руке список «прочитать / вынуть» вместо tgui `Folder`, оверлей `folder_paper`; нет фото/документов, переименования ручкой, разрезания; документы-цели краж — группа антагонистов

### Конфиденциальный конверт (paper biscuit)
Запечатанный конверт с бумагой; вскрывается необратимо. Вариант с кодом сейфа запасной ID капитана.

- **Код:** [`/obj/item/folder/biscuit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper_biscuit.dm#L1), [`/obj/item/folder/biscuit/confidential/spare_id_safe_code`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper_biscuit.dm#L111)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/service/stationery.rs`: `Biscuit` — папка, запечатанная не принимает и не отдаёт бумаги («open first!»), вскрытие в руке через alert необратимо (звук, `_cracked`, оверлей `paperbiscuit_paper` только вскрытой), `unsealed` запечатывается один раз; карточки кода сейфа запасной ID создают свой листок (код `[REDACTED]`: сейфа запасной ID в порте нет)

### Лоток с бумагой
Выдаёт новые листы (и ручку, если вложена); варианты из натуральной бумаги и с копиркой.

- **Код:** [`/obj/item/paper_bin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paperbin.dm#L3), [`/obj/item/paper_bin/carbon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paperbin.dm#L272)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/paper.rs`: 30 листов, пустая рука берёт верхний (вложенный или новый лист типа лотка), бумага кладётся внутрь, оверлей листа и края; нет ручки в лотке, обвала стопки, поднятия лотка перетаскиванием, горения

### Копировальная бумага
Лист с копиркой: написанное отрывается как копия.

- **Код:** [`/obj/item/paper/carbon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/carbonpaper.dm#L1), [`/obj/item/paper/carbon_copy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/carbonpaper.dm#L50)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/service/stationery.rs`: `CarbonPaper` — один раз отрывается копия `carbon_copy` «Copy - имя» (текст, печати) в руки, лист становится `paper`/`paper_words`, дальше складывается в самолётик; отличие: отрыв по Alt+клику по листу в руке (правый клик по слоту руки в порте не перехватывается)

### Бумажный самолётик
Сложенная бумага метается и может попасть в глаз; синдикатский вариант острее.

- **Код:** [`/obj/item/paperplane`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paperplane.dm#L1), [`/obj/item/paper/proc/make_plane()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper.dm#L325)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/office.rs`: Alt+клик по бумаге (в руке или рядом) складывает самолётик (в руку, если рядом), в руке разворачивается обратно, печать ставится на бумагу внутри, ручка — «unfold first»; нет попадания в глаз при броске, синдикатского самолётика (`TRAIT_PAPER_MASTER`), оверлеев печатей и вида копирки

### Резак для бумаги
Режет бумагу на полоски (записки, предсказания); лезвие снимается.

- **Код:** [`/obj/item/papercutter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper_cutter.dm#L1), [`/obj/item/paper/paperslip`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paper_cutter.dm#L180)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/service/stationery.rs`: `PaperCutter` с лезвием `cutterblade` внутри; бумага вставляется (кроме листков, памфлетов, holy writ), отвёртка фиксирует/освобождает лезвие, Alt+клик вынимает бумагу, иначе незакреплённое лезвие, правый клик пустой рукой режет на два `paperslip` (звук, оверлеи `paper`/`cutter_overlay`); нет пореза неуклюжего (нет трейта clumsy)

### Картотеки
Шкафы хранят бумаги и папки (tgui `FilingCabinet`); шкафы СБ, медбея и отдела кадров в начале раунда печатают записи экипажа / трудовые контракты.

- **Код:** [`/obj/structure/filingcabinet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/filingcabinet.dm#L13), [`/obj/structure/filingcabinet/security`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/filingcabinet.dm#L125), [`/obj/structure/filingcabinet/employment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/filingcabinet.dm#L185); tgui: [`FilingCabinet.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/FilingCabinet.tsx)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/service/stationery.rs`: `FilingCabinet` принимает предметы меньше normal (с карты — мелочь со своего тайла), вне боевого режима прочее отказывает, пустая рука — список содержимого вместо tgui `FilingCabinet` (вспышка `-open`), гаечный ключ правым кликом 2 с крепит/открепляет; шкафы СБ/медбея при первом открытии печатают рапшиты (`SR-N`) и медкарты из манифеста; нет телекинеза

### Трудовой контракт
Контракт на члена экипажа, хранится в картотеке отдела кадров.

- **Код:** [`/obj/item/paper/employment_contract`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/contract.dm#L3)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/service/stationery.rs`: шкаф отдела кадров при открытии кладёт `employment_contract` «paper- имя employment contract» с текстом tg на каждую запись `Manifest.locked`, ещё не подшитую (так же и поздние входы, как `AddEmploymentContract`)

### Ручной этикетировщик
Наклеивает на предмет ярлык (переименование в «name (label)»), заправляется рулоном.

- **Код:** [`/obj/item/hand_labeler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/handlabeler.dm#L2), [`/obj/item/hand_labeler_refill`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/handlabeler.dm#L147)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/paper.rs`: включение в руке спрашивает текст (до 42 символов), включённый дописывает ` (метка)` к имени цели (лимит 64, 30 меток, рулон-заправка), мобов не метит; метка не отдельный предмет и не отклеивается

### Отдельские документы для карго
Документы отделов, которые нужно заверить печатью соответствующего главы; проданные с верной печатью дают деньги карго, без неё — штраф.

- **Код:** [`/obj/item/paperwork`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paperwork.dm#L13), [`/obj/item/paperwork/photocopy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/paperwork.dm#L216)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/stationery.rs`: `Paperwork` — печать нужного главы (по подтипу; древние — случайный глава) завершает документ оверлеем `paper_stamp-*`, чужая печать отказывает, VOID аннулирует фотокопию; продажа (`/datum/export/paperwork`, отрицательные цены) — группа карго; нет текста при осмотре для своей должности, копирования на ксероксе
