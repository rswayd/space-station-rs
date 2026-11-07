# Модульные компьютеры (NTOS)
Планшеты/КПК, ноутбуки и консоли на NtOS `/obj/item/modular_computer`; приложения — в [ntos-apps.md](ntos-apps.md).

### Модульный компьютер и NtOS
Общая основа: включение, рабочий стол NtOS (`NtosMain`), запуск/сворачивание программ, заголовок с зарядом, сетью и временем, журнал, звонок/вибрация, фонарик, печать, сохранение фото, тревожные звонки по уровню угрозы.

- **Код:** [`/obj/item/modular_computer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/computer.dm#L4), [`proc/open_program()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/computer.dm#L712), [`computer_ui.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/computer_ui.dm#L47); tgui: [`NtosMain.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/NtosMain.tsx)
- **BandaStation:** [`computer.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/computer.dm#L1107) — уровни Гамма/Эпсилон для тревожных звонков
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/modpc/` (`NtosMain`: заголовок со временем, сигналом, зарядом; запуск, сворачивание и закрытие программ; фонарик): только КПК; нет журнала, печати, фото, тревожных звонков, тем (`// shortcut:` в `modpc/client.rs`, `modpc/pda.rs`)

### КПК (PDA)
Планшет в слоте КПК/ID: ID-карта внутри, мессенджер, предустановленные программы по должности, ручка, раскраска (GAGS, раскрасчик КПК), КПК ролей и Синдиката; форк добавляет «Навигатор» в набор по умолчанию и КПК своих должностей.

- **Код:** [`/obj/item/modular_computer/pda`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/pda.dm#L1), [`role_tablet_presets.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/role_tablet_presets.dm#L5), [`/obj/machinery/pdapainter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/PDApainter.dm#L2)
- **BandaStation:** [`pda.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/pda.dm#L49) — навигатор; [`modular_bandastation/objects/code/items/role_tablet_presets.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/role_tablet_presets.dm#L1), [`modular_bandastation/jobs/code/job_types/blueshield/pda.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/blueshield/pda.dm#L1) — КПК ЦК и должностей форка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/modpc/pda.rs`: КПК из аутфита профессии в поясе/кармане, привязка имени и должности («КПК «Имя» (Ассистент)»), ID-слот, мессенджер, аплинк предателя; нет ручки, раскраски и раскрасчика, КПК синдиката/ролей форка, «Навигатора», рингтона по профессии (`job_tone`)

### КПК синтетиков
Встроенные «КПК» ИИ, киборгов и пИИ (мессенджер, Robotact).

- **Код:** [`/obj/item/modular_computer/pda/silicon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/pda.dm#L317), [`/obj/item/modular_computer/pda/silicon/cyborg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/pda.dm#L347)
- **Статус в ss13-rs:** ❌ нет

### Ноутбуки и консоли NTOS
Ноутбук (раскрывается, ставится на стол) и стационарные консоли с NtOS (инженерная, научная, командная и др. пресеты).

- **Код:** [`/obj/item/modular_computer/laptop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/laptop.dm#L1), [`/obj/machinery/modular_computer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/machinery/modular_computer.dm#L4), [`/obj/machinery/modular_computer/preset`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/machinery/console_presets.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Питание модульных компьютеров
Расход энергии ячейки на программы, подзарядка от APC (консоли) или зарядников, отключение при разряде.

- **Код:** [`proc/handle_power()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/computer_power.dm#L38), [`proc/power_failure()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/computer_power.dm#L25)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/modpc/power.rs`: ячейка (`ComputerCell`: КПК — стандартная, консоль — AA), `handle_power` раз в 2 с (экран, фонарик ×1.1, программы спереди и в фоне), консоль питается от зоны и подзаряжает ячейку, без питания — от ячейки; разряд закрывает программы и выключает (`power_failure`), пустой не включается; зарядник (`power/chargers.rs`) заряжает КПК; заряд в заголовке. `// shortcut:` ячейка не отдельный предмет (без отвёртки и замены, без ЭМИ)

### Файлы, хранилище и диски
Файловая система компьютера: программы и данные занимают место, диски данных (обычные, продвинутые, супер, ролевые, техтоннельные, Синдиката, чёрного рынка), файловый менеджер.

- **Код:** [`/datum/computer_file`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/file_system/computer_file.dm#L1), [`/datum/computer_file/program`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/file_system/program.dm#L2), [`proc/store_file()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/computer_files.dm#L8), [`/obj/item/disk/computer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/disks/computer_disk.dm#L1), [`/obj/item/disk/computer/maintenance`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/disks/maintenance_disks.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### ID и доступ в компьютере
Слот ID (основной и второй), привязка имени/должности (imprint), доступ программ по ID.

- **Код:** [`proc/insert_secondary_id()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/computer.dm#L332), [`proc/imprint_id()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/computer.dm#L809)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/modpc/pda.rs`: ID-слот (вставка картой, alt-клик и «Вытащить ID»), привязка, КПК с картой открывает двери, шкафы и автоматы (`GetID`); второго слота нет, доступ копируется при вставке (`// shortcut:`)

### NTNet: доступность сети
Программы требуют связи с NTNet (реле и телекомы), уровень сигнала зависит от z-уровня и состояния реле.

- **Код:** [`proc/get_ntnet_status()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/computer.dm#L763)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/modpc/ntnet.rs`: консоль — провод (3), переносной — только при запитанном реле `/obj/machinery/ntnet_relay`: станционный z — хороший (ноутбук — провод), шахтёрский или `long_ranged` — слабый, иначе нет; сетевые программы (`PROGRAM_REQUIRES_NTNET`) не запускаются без сигнала и падают при его потере (`event_networkfailure`), сигнал в заголовке. Сама машина реле (вкл/выкл, DoS, UI) — в [telecomms.md](telecomms.md#реле-ntnet); `// shortcut:` скорость загрузки не зависит от сигнала

### Модульный компьютер как оболочка схем
Компьютер принимает интегральную схему (wiremod) и даёт ей компоненты программ.

- **Код:** [`computer_circuit.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/computers/item/computer_circuit.dm#L2)
- **Статус в ss13-rs:** ❌ нет
