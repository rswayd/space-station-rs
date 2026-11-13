# Законы синтетиков
Законы ИИ, киборгов, дронов и пИИ, стойки модулей законов и сами модули.

### Набор законов (`/datum/ai_laws`)
Законы синтетика: нулевой, ион-законы, взломанные, базовые (inherent) и дополнительные (supplied); показ (`Show Laws`), озвучивание законов (`State Laws`), лог смены законов и уведомление в дедчат.

- **Код:** [`/datum/ai_laws`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai_laws/ai_laws.dm#L113), [`proc/show_laws()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/laws.dm#L6), [`proc/statelaws()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/silicon.dm#L241), [`proc/announce_law_change()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/laws.dm#L35); tgui: [`StateLawUi.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/StateLawUi.tsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/silicon/laws.rs`: нулевой (с защищённым `protected_zeroth`), основные (с ионными, список по тексту закона как assoc-список tg: `add_inherent_law`), дополнительные, взломанные законы, нумерация `get_law_list`, вывод `show_laws` в чат при спавне и по клику ИИ по своему ядру; меняются стойками модулей (`law_rack.rs`) и синхронизацией. Ионные законы событий — см. события (нет `generate_ion_law`)

### Лоусеты на стороне станции
Asimov, Asimov++, Corporate, Paladin, Robocop, Hippocratic, Peacekeeper, Ten Commandments, Live and Let Live, Nutimov, Drone и т. д.; один выбирается на раунд (конфиг `default_laws`).

- **Код:** [`laws_station_sided.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai_laws/laws_station_sided.dm#L5), [`laws_neutral.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai_laws/laws_neutral.dm#L5)
- **Статус в ss13-rs:** ✅ реализовано — все `/datum/ai_laws` переносятся `ss13-mapload convert` в `silicon.ron`; набор раунда (`DEFAULT_LAWS 4` + `SPECIFIED_LAWS`) — приваренный модуль ядерной стойки, остальные загружаются полными модулями ядра (`law_id`) в стойку (`crates/ss13_game/src/silicon/law_rack.rs`)

### Антагонистские лоусеты
Antimov, Tyrant, Thermodynamic/Thermurderdynamic, Syndicate override, Ninja override, Balance, Overlord и прочие.

- **Код:** [`laws_antagonistic.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai_laws/laws_antagonistic.dm#L5)
- **Статус в ss13-rs:** ✅ реализовано — все переносятся в `silicon.ron` и загружаются своими модулями ядра (`law_rack.rs`); Syndicate override (`syndie`) получает емагнутый киборг (`crates/ss13_game/src/silicon/chassis.rs`, емаг отвязывает его от стойки)

### Лоусеты BandaStation
Форк добавляет 26 русскоязычных наборов (НТ Стандарт, Крюзимов, Крюзимов++, ЕРТ, Детектив, Петрович, Мать, Друид, Клоун, Эпсилон, Коллектив синтетиков и др.) с модулями ядра и чертежами в техвебе.

- **Код:** [`/obj/item/ai_module/law/core/full`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/_AI_modules.dm#L220)
- **BandaStation:** [`modular_bandastation/ai_laws/code/nt_default.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ai_laws/code/nt_default.dm#L5), [`modular_bandastation/ai_laws/code`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ai_laws/code) — 26 лоусетов; [`full_lawsets.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/full_lawsets.dm#L33) — убрана нерабочая после перевода логика Asimov++
- **Статус в ss13-rs:** ✅ реализовано — лоусеты модпака переносятся в `silicon.ron`, их модули ядра (`law_id`) работают в стойке; набор раунда по конфигу BandaStation — `crewsimovpp` (модуль на ядерной стойке, `crates/ss13_game/src/silicon/law_rack.rs`)

### Стойки модулей законов (law rack)
Законы физически лежат в стойках модулей: модули вставляются, закручиваются отвёрткой или завариваются; стойка-ядро задаёт базовый набор, ретранслятор (broadcaster) передаёт законы связанным синтетикам; разрезание проводов, мультитул для привязки, окно `LawRack`.

- **Код:** [`/obj/machinery/ai_law_rack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/ai_core/law.dm#L21), [`/obj/machinery/ai_law_rack/broadcaster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/ai_core/law.dm#L596), [`/obj/machinery/ai_law_rack/base/core`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/ai_core/law.dm#L1019), [`proc/link_to_first_rack()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/laws.dm#L50); tgui: [`LawRack.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/LawRack.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/law_rack.rs`: обычная, стандартная, ядерная (греческое имя, приваренный модуль набора раунда) и портативная стойки, ретранслятор; окно `LawRack` (вставка в пустой слот, слот ядра только для модулей ядра, извлечение незакреплённого, отвёртка 3 с, сварка 3 с и 1 топлива, ремонт ионного модуля мультитулом 4 с), привязка/отвязка синтетиков (тот же z, не привязан, ИИ с управлением, киборг не емагнут и не scrambled) и ретрансляторов, `refresh` с откатом 30 с; обесточенная стойка не обновляет; ИИ при входе привязывается к первой ядерной стойке, киборг — если нет ИИ; «Ваши законы были изменены.» и новые законы; удаление стойки отвязывает. Нет оглушения и файрвола при снятии ядра/потере стойки (только сообщение), крепления ключом и кабелем, привязки ядра ИИ мультитулом (нет сборки ядра), GPS, осмотра слотов; меню привязки — списком кнопок

### Модули законов: ядро и полные наборы
Платы, задающие весь базовый набор (Asimov, Corporate, ... и `custom`), плюс модуль ядра раунда по умолчанию.

- **Код:** [`/obj/item/ai_module`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/_AI_modules.dm#L5), [`/obj/item/ai_module/law/core`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/_AI_modules.dm#L201), [`/obj/item/ai_module/law/core/full`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/_AI_modules.dm#L220), [`/obj/effect/spawner/round_default_module`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/_AI_modules.dm#L239)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/silicon/law_rack.rs`: модули ядра дают основные законы, полные (`law_id`, из `silicon.ron`) — законы своего лоусета; Asimov настраивается в руке (`tgui_input_text` субъекта); `config_custom` пуст (нет лоусета из конфига)

### Модули законов: дополнительные, свободные, нулевой
Safeguard, Protect Station, Quarantine, Oxygen is toxic, Freeform (свой текст), Freeform core, One Human (нулевой закон).

- **Код:** [`/obj/item/ai_module/law/supplied`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/supplied.dm#L9), [`/obj/item/ai_module/law/supplied/freeform`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/freeform.dm#L33), [`/obj/item/ai_module/law/core/freeformcore`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/freeform.dm#L6), [`/obj/item/ai_module/law/zeroth/onehuman`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/zeroth.dm#L4)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/silicon/law_rack.rs`: дополнительные модули (Safeguard, ProtectStation, Quarantine, OxygenIsToxic) — дополнительные законы, Freeform/Freeform core — текст из `tgui_input_text` в руке (пустой не ставится), OneHuman — нулевой закон с именем, Safeguard без имени не ставится; IC-фильтр текста не перенесён

### Взломанные и ионные модули
Модуль Синдиката (взломанный закон с приоритетом), повреждённый модуль/игрушка ИИ с ион-законами, плата малфа.

- **Код:** [`/obj/item/ai_module/law/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/hacked.dm#L1), [`/obj/item/ai_module/law/toy_ai`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/ion.dm#L4), [`/obj/item/malf_board`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/hacked.dm#L26)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/law_rack.rs`: взломанный модуль даёт взломанный закон (над нумерацией), игрушка ИИ — ионный первый основной закон, повреждённый модуль ядра начинает ионным и чинится мультитулом; нет генерации ион-законов (`generate_ion_law`: игрушка не генерирует закон, повреждённый не перегенерирует), плата малфа — см. ../antagonists/malf-ai.md

### Синхронизация законов боргов с ИИ
Привязанные к ИИ киборги получают его законы (lawsync), отвязка — через провод или консоль.

- **Код:** [`proc/lawsync()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/laws.dm#L57), [`proc/lawset_updated_sync_borgs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai.dm#L1304)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/cyborg.rs`: связь с ИИ (`connected_ai`, `connected_robots`), `lawsync` по `ai_to_cyborg` при связи и при смене законов ИИ, после чего борг видит законы (`show_laws` робота: «Laws synced with AI», кто мастер — `crates/ss13_game/src/silicon/chassis.rs`); без ИИ при спавне `lawupdate` выключен, емаг отвязывает; нет `lawupdate`-провода

### Запрос смены законов у ЦК (BandaStation)
Верб ИИ «Request Law Change»: ИИ составляет новый набор, админы получают fax-подобное уведомление, одобряют или отклоняют; при одобрении законы ядра заменяются и экипаж получает объявление.

- **Код:** [`/mob/living/silicon/ai`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai_defines.dm#L1); tgui: [`AiLawChangeRequest.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AiLawChangeRequest.tsx)
- **BandaStation:** [`modular_bandastation/ai_law_change/code/ai_law_change.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ai_law_change/code/ai_law_change.dm#L27) — весь механизм
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/silicon/law_change.rs`: верб ИИ «Request Law Change» (вкладка «Команды ИИ»; отказы по порядку: отклонён, уже одобрен, прошло 10 минут раунда, ждёт рассмотрения; не в недееспособности) открывает `AiLawChangeRequest` — три закона (обрезка до `MAX_LAW_LEN` 1024, все обязательны, иначе «ПЕРЕПРОВЕРЬТЕ ВАШ ЗАПРОС.»); админам с `R_ADMIN` — уведомление со ссылками в чате «Одобрить / Рассмотреть / Отклонить» (права проверяются снова при клике); «Рассмотреть» открывает окно админу с правкой законов и переключателем оповещения экипажа; одобрение заменяет законы модуля ядра базовой стойки ИИ и синхронизирует стойку (`crates/ss13_game/src/silicon/law_rack.rs` `set_core_laws`), при оповещении — приоритетное объявление «Отдел работы с искуственным интеллектом»; отклонение — «ВАШ ЗАПРОС БЫЛ ОТКЛОНЁН.» и запрет новых запросов. Нет кнопок VV/PM/PP/FLW в окне рецензии (инструменты `ss13_admin`), мигания окна и звука у админов

### Админ-панель законов
Админский просмотр и правка законов любого синтетика.

- **Код:** [`lawpanel.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/lawpanel.dm#L8); tgui: [`Lawpanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Lawpanel.tsx)
- **Статус в ss13-rs:** ❌ нет
