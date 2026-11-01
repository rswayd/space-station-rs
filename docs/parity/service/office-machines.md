# Офисные машины
Факс, ксерокс, настольный звонок и автомат очереди из `code/modules/paperwork`.

### Факс
Отправляет бумагу, фото и документы на другой факс станции по выбранному ID или в ЦК/Синдикат (админам); принятое печатается, история отправок в tgui. Застревание бумаги, взлом emag открывает спец-адреса.

- **Код:** [`/obj/machinery/fax`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/fax.dm#L6), [`send()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/fax.dm#L451), [`receive()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/fax.dm#L475), [`/obj/machinery/fax/admin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/fax.dm#L124); tgui: [`Fax.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Fax.tsx)
- **BandaStation:** [`code/modules/paperwork/fax.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/fax.dm#L396) — уведомление о факсе в ЦК только админам с `R_ADMIN`, автопечать (`fax_autoprinting`) и передача факса ИИ-секретарю
- **Статус в ss13-rs:** ❌ нет

### ИИ-секретарь ЦК для факсов
Факсы в ЦК анализируются внешней LLM (`ai_secretary_url/token/model` в конфиге): срочность, черновик ответа по шаблону `config/blanks/markdown/centcom`, который админ может отправить. Админская панель факсов (`code/modules/admin/admin_fax_panel.dm`) вне этой папки.

- **Код:** [`/datum/ai_bridge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/ai_bridge.dm#L16), [`/datum/config_entry/string/ai_secretary_url`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/ai_bridge.dm#L309)
- **BandaStation:** [`code/modules/paperwork/ai_bridge.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/ai_bridge.dm#L16) — файл целиком добавлен форком
- **Статус в ss13-rs:** ❌ нет

### Ксерокс
Копирует бумагу, фото, документы и отдельские бумаги (тонер, запас бумаги, до 10 копий), печатает бланки; можно сесть на стекло и отксерить себя. Тонер меняется картриджем.

- **Код:** [`/obj/machinery/photocopier`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/photocopier.dm#L71), [`do_copies()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/photocopier.dm#L421), [`make_ass_copy()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/photocopier.dm#L660), [`/obj/item/toner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/photocopier.dm#L911); tgui: [`Photocopier.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Photocopier.tsx)
- **BandaStation:** [`modular_bandastation/objects/code/machinery/photocopier.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/machinery/photocopier.dm#L1) — ксерокс Синдиката
- **Статус в ss13-rs:** ❌ нет

### Бланки документов
Шаблоны форм из `config/blanks/blanks.json` печатаются ксероксом; при печати ключи подставляются данными игрока.

- **Код:** [`/proc/init_paper_blanks()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/photocopier.dm#L48), [`make_blank_print()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/photocopier.dm#L641)
- **BandaStation:** [`modular_bandastation/paperwork/code/paperwork.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/paperwork/code/paperwork.dm#L10) — подстановка `[name]`, `[job]`, `[date]` и др. в бланки; шаблон `modular_bandastation/paperwork/templates/station_report.md`
- **Статус в ss13-rs:** ❌ нет

### Настольный звонок
Звонок на стойке: клик звонит (с кулдауном), можно разобрать ключом; ручная версия.

- **Код:** [`/obj/structure/desk_bell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/desk_bell.dm#L3), [`/obj/item/inhand_desk_bell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/desk_bell.dm#L130)
- **Статус в ss13-rs:** ❌ нет

### Автомат электронной очереди
Настенный автомат выдаёт пронумерованные талоны; кнопка у HoP вызывает следующий номер с объявлением. Emag выдаёт горящие талоны и путает счётчик.

- **Код:** [`/obj/machinery/ticket_machine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/ticketmachine.dm#L4), [`emag_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/ticketmachine.dm#L58), [`/obj/machinery/button/ticket_machine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/ticketmachine.dm#L98), [`/obj/item/ticket_machine_ticket`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/ticketmachine.dm#L240)
- **Статус в ss13-rs:** ❌ нет
