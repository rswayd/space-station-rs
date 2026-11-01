# Генетическое оборудование
Сканер и консоль ДНК, инъекторы, ДНК-инфьюзер, экспериментальный клонер. В ss13-rs ничего из этого нет.

### Сканер ДНК
Капсула, в которую кладут пациента для работы консоли; облучает радиацией при изменениях; детали ускоряют и снижают вред.

- **Код:** [`/obj/machinery/dna_scannernew`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/dna_scanner.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/genetics/console.rs`: открыть/закрыть рукой, затащить человека, выйти шагом, спрайты; нет замка, деталей, радиации при изменениях

### Консоль ДНК
tgui-консоль: секвенирование мутаций в SE пациента (подбор пар оснований), активация/удаление, сохранение в буфер и на диски, работа с UI/UF (смена внешности), изготовление инъекторов и активаторов, хромосомы, комбинирование мутаций, разблокировка мутаций исследованием.

- **Код:** [`/obj/machinery/computer/dna_console`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/dna_console.dm#L74), [`/obj/item/disk/data`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/dna_scanner.dm#L159); tgui: [`index.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/DnaConsole/index.jsx)
- **BandaStation:** [`code/game/machinery/computer/dna_console.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/dna_console.dm#L1474) — санитизация имени инъектора
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/genetics/console.rs`, `crates/ss13_tgui/src/interfaces/dna_console.rs`: связь со сканером рядом, статус субъекта, секвенирование `pulse_gene`, открытие мутаций, печать активатора/мутатора с кулдаунами tg, `nullify`; нет хранилища, дисков, хромосом, UI/UF, перемешивания, джокера

### ДНК-инъекторы и сканер последовательностей
Инъекторы добавляют/убирают мутации или меняют внешность (одноразовые, активаторы, в шприцемёт); ручной сканер последовательностей показывает мутации и даёт их коды.

- **Код:** [`/obj/item/dnainjector`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/dna_injector.dm#L1), [`/obj/item/dnainjector/activator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/dna_injector.dm#L130), [`/obj/item/sequence_scanner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/sequence_scanner.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/genetics/console.rs`: инъектор на себя сразу, на другого за 3 с, активатор/мутатор, `used`; нет преднастроенных картовых инъекторов, временных, сканера последовательностей

### ДНК-инфьюзер
Машина вплавляет в человека ДНК существа (труп): органы животного (карп, муха, крыса, таракан, лиса, голиаф, гондола, рыба, горностай и др.) заменяют свои; полный набор органов даёт бонус; справочник уровней — книга (tgui).

- **Код:** [`/obj/machinery/dna_infuser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/dna_infuser/dna_infuser.dm#L6), [`/datum/infuser_entry`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/dna_infuser/infuser_entry.dm#L12), [`/datum/status_effect/organ_set_bonus`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/organ_set_bonus.dm#L45), [`/obj/item/tgui_book/manual/infuser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/dna_infuser/infuser_book.dm#L2); tgui: [`InfuserBook.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/InfuserBook.tsx)
- **BandaStation:** [`code/game/machinery/dna_infuser/organ_sets/fly_organs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/dna_infuser/organ_sets/fly_organs.dm#L66) — правка речи мушиного набора
- **Статус в ss13-rs:** ❌ нет

### Экспериментальный клонер
Сканер, консоль и капсула клонирования: сканирует мёртвых в записи и выращивает новое тело с разумом; с шансом сбоев («fuckups»: фелинизация, облысение, травма мозга, перемешанная ДНК, мутации, живая плоть).

- **Код:** [`/obj/machinery/experimental_cloner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/experimental_cloner/experimental_cloner_pod.dm#L5), [`/obj/machinery/computer/experimental_cloner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/experimental_cloner/experimental_cloner_computer.dm#L2), [`/obj/machinery/experimental_cloner_scanner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/experimental_cloner/experimental_cloner_scanner.dm#L2), [`/datum/experimental_cloning_record`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/experimental_cloner/experimental_cloning_record.dm#L2), [`/datum/experimental_cloner_fuckup`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/experimental_cloner/experimental_cloner_fuckup.dm#L5); tgui: [`ExperimentalCloner.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ExperimentalCloner.tsx)
- **Статус в ss13-rs:** ❌ нет

### ДНК-зонд и хранилище ДНК
Ручной зонд собирает ДНК животных и растений для станционной цели «хранилище ДНК», которое даёт экипажу генетические бонусы.

- **Код:** [`/obj/item/dna_probe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/dna_probe.dm#L11), [`/obj/machinery/dna_vault`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/station_goals/dna_vault.dm#L62); tgui: [`DnaVault.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/DnaVault.tsx)
- **Статус в ss13-rs:** ❌ нет
