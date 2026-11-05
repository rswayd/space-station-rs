# Библиотека и книги
Книжные шкафы, книги, библиотечные машины с базой книг в SQL, библия как книга и скиллчипы из `code/modules/library`.

### Книжный шкаф
Шкаф хранит книги, библии, свитки и тетради; книги кладутся и достаются рукой, пустой шкаф разбирается ломом, отвинчивается ключом. Шкафы в библиотеке регистрируют содержимое для библиотечных машин.

- **Код:** [`/obj/structure/bookcase`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/bookcase.dm#L5), [`item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/bookcase.dm#L123)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/library.rs`: шкаф с карты забирает книги со своего тайла, пустая рука выбирает книгу из списка (в пустую активную руку), книга ставится на полку, спрайт `book-N`; нет сборки/разборки, ломом/ключом, высыпания из сумки

### Случайные книжные шкафы
Подтипы `/obj/structure/bookcase/random/*` (fiction, nonfiction, religion, adult, reference, wizard) при старте раунда заполняются случайными книгами из базы библиотеки по категории.

- **Код:** [`/obj/structure/bookcase/random`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/random_books.dm#L38), [`/obj/item/book/random`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/random_books.dm#L19)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/look.rs`: только случайный спрайт шкафа (`book-N`) по сиду карты, самих книг и базы нет

### Книга
Книгу открывают в руке и читают (tgui `MarkdownViewer`); ручкой можно переписать название, автора и текст, острым предметом вырезать тайник внутри. Чтение даёт настроение «прочитал книгу».

- **Код:** [`/obj/item/book`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/book.dm#L1), [`attack_self()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/book.dm#L122), [`carving_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/book.dm#L236), [`/datum/book_info`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/book_info.dm#L7); tgui: [`MarkdownViewer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/MarkdownViewer.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/library.rs`: `starting_title/author/content` (`DmService`), чтение в руке — окно `MarkdownViewer` (текст без рендера markdown), «book is blank!», сообщение окружающим, mood `book_nerd` за первое прочтение названия; нет вырезания тайника, порчи ручкой, падения на голову, базы книг

### Руководства с tgui (DSM, IDC)
Справочники-книги со своим tgui-окном: руководство по психиатрии (DSM) и по ID-картам (IDC).

- **Код:** [`/obj/item/tgui_book`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/book.dm#L263), [`/obj/item/tgui_book/manual/dsm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/book.dm#L305), [`/obj/item/tgui_book/manual/idc`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/book.dm#L383); tgui: [`DSMBook.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/DSMBook.tsx), [`IDCBook.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/IDCBook.tsx)
- **Статус в ss13-rs:** ❌ нет

### Библиотечный терминал посетителя
Публичная консоль поиска по базе книг (название, автор, категория, постранично) с ограничением частоты SQL-запросов.

- **Код:** [`/obj/machinery/computer/libraryconsole`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/lib_machines.dm#L21), [`SUBSYSTEM_DEF(library)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/library.dm#L2); tgui: [`LibraryVisitor.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/LibraryVisitor.jsx)
- **Статус в ss13-rs:** ❌ нет

### Библиотечный компьютер библиотекаря
Учёт выдачи книг (кто взял, срок), инвентарь шкафов, печать книг из базы и плакатов, загрузка отсканированной книги в базу по категории; emag открывает печать запретных книг (кодекс, свитки).

- **Код:** [`/obj/machinery/computer/libraryconsole/bookmanagement`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/lib_machines.dm#L277), [`print_book()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/lib_machines.dm#L657), [`upload_from_scanner()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/lib_machines.dm#L595), [`emag_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/lib_machines.dm#L564), [`/datum/borrowbook`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/lib_machines.dm#L251); tgui: [`LibraryConsole`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/LibraryConsole)
- **Статус в ss13-rs:** ❌ нет

### Сканер книг и переплётная машина
Сканер копирует содержимое книги для загрузки в базу; переплётчик превращает исписанную бумагу в книгу.

- **Код:** [`/obj/machinery/libraryscanner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/lib_machines.dm#L693), [`/obj/machinery/bookbinder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/lib_machines.dm#L778), [`bind_book()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/lib_machines.dm#L839); tgui: [`LibraryScanner.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/LibraryScanner.jsx)
- **Статус в ss13-rs:** ❌ нет

### Штрих-код сканер библиотекаря
Ручной сканер, связывается с компьютером библиотеки: отмечает выдачу/возврат книг и добавляет их в инвентарь.

- **Код:** [`/obj/item/barcodescanner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/barcode_scanner.dm#L5)
- **Статус в ss13-rs:** ❌ нет

### Админский просмотр и удаление книг
Невыставляемая на карту админ-консоль: поиск по всей базе, история правок, скрытие/восстановление книг игроков.

- **Код:** [`/obj/machinery/computer/libraryconsole/admin_only_do_not_map_in_you_fucker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/admin_only.dm#L5), [`/datum/admin_book_viewer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/admin_only.dm#L304); tgui: [`LibraryAdmin`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/LibraryAdmin), [`AdminBookViewer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AdminBookViewer.tsx)
- **Статус в ss13-rs:** ❌ нет

### Codex Gigas
Демоническая книга: чтение позволяет узнать истинное имя/слабости (наследие devil), выдаётся через emag-печать.

- **Код:** [`/obj/item/book/codex_gigas`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/lib_codex_gigas.dm#L1)
- **Статус в ss13-rs:** ❌ нет

_«Скиллчипы» описано в [../mobs/skills.md](../mobs/skills.md)._

### Станция имплантации скиллчипов
Капсула, в которую ложатся для вставки, активации и извлечения скиллчипов (tgui со списком чипов и временем имплантации).

- **Код:** [`/obj/machinery/skill_station`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/skill_learning/skill_station.dm#L4); tgui: [`SkillStation.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SkillStation.jsx)
- **Статус в ss13-rs:** ❌ нет
