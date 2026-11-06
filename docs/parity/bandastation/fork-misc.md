# Прочие модпаки форка
Модпаки BandaStation, которые не относятся ни к одному другому разделу каталога: мелкие фичи, переопределения и общая инфраструктура форка.

### Сапёр (Minesweeper)
Программа для модульных компьютеров и КПК: поле с минами, выбор размера поля, открытие клеток и флажки, проверка победы, TGUI-окно.

- **Код:** [`/datum/computer_file/program/minesweeper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/minesweeper/code/minesweeper.dm#L19); tgui: [`NtosMinesweeperPanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/NtosMinesweeperPanel.tsx)
- **BandaStation:** [`_minesweeper.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/minesweeper/_minesweeper.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/modpc/programs/minesweeper.rs` (сервер), `crates/ss13_game/src/modpc/client.rs` (`minesweeper_screen`): программа «Сапёр» ставится на КПК емагом (`modpc::pda::emag_act`, флаг `emagged` модульного компьютера, емаг в другой руке или по КПК на полу); безопасный первый клик (3×3), заливка пустых клеток, флажки, победа (пинг, ник в рейтинг раунда, новое поле через 5 с), проигрыш (звук взрыва, на емагнутом КПК — взрыв −1/1/3/2, новое поле через 3 с), смена размеров 9–25 и бомб 10–min(площадь/5, 100) с перепроверкой после каждого ответа, 3BV и 3BV/с; режим клика переключается кнопкой (в tgui — ПКМ); клиент не получает бомб и чисел закрытых клеток; `// shortcut:` нет глобального рейтинга (таблица `minesweeper` — [../persistence-sql/database.md](../persistence-sql/database.md)), нет установки через загрузчик программ NTNet ([../comms/ntos-apps.md](../comms/ntos-apps.md)), рейтинг без сортировки по столбцам, окно одного размера

### Отключения апстрима (overrides)
На живого моба нельзя наклеить больше одного стикера; эффект «сплющивания» (squish) ограничен тремя наложениями; обучение новичков (tutorial) отключено.

- **Код:** [`/obj/item/sticker/attempt_attach()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/overrides/code/stickers.dm#L2), [`/datum/element/squish/Attach()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/overrides/code/squish.dm#L8), [`/datum/tutorial_manager/should_run()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/overrides/code/tutorial_disable.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — обучения новичков нет, как и в форке; стикеров и squish нет (их механики — ../service/ и ../mobs/)

### Удар в спину (backstab)
Компонент оружия: удар сзади наносит повышенный урон с перезарядкой.

- **Код:** [`/datum/component/backstabs`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/~components/code/backstab.dm#L1)
- **Статус в ss13-rs:** ❌ нет — единственный носитель, выкидной нож (`knives.dm`), без трансформации (раскрытия) не наносит урона; нужен сначала `transforming` компонент (../combat/melee-weapons.md)

### Разрывание стен (rip_and_tear)
Компонент тяжёлого оружия (кувалда, легендарные мечи): серия ударов по стене разрушает её, с затратой выносливости и требованием держать двумя руками; укреплённые стены — дольше.

- **Код:** [`/datum/component/rip_and_tear`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/~components/code/rip_and_tear_item.dm#L15)
- **Статус в ss13-rs:** ❌ нет

### Затраты выносливости за удар
Компонент: атака предметом тратит выносливость, по-разному для живых целей и объектов, в одной или двух руках.

- **Код:** [`/datum/component/stamina_cost_per_hit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/~components/code/stamina_cost_per_hit.dm#L10)
- **Статус в ss13-rs:** ❌ нет

### Привязка предмета к ckey/роли
Компонент: предмет может поднять только владелец из белого списка ckey или не-станционная роль; остальные получают урон и отказ.

- **Код:** [`/datum/component/ckey_and_role_locked_pickup`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/~components/code/ckey_and_role_locked_pickup.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Нательная камера с паузой
Компонент бодикамеры, которая обновляется только пока за ней кто-то наблюдает; носитель видит алерт «за вами наблюдают».

- **Код:** [`/datum/component/pausable_bodycam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/~components/code/pausable_bodycam.dm#L2), [`/obj/machinery/camera/bodycam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/~components/code/bodycam_camera.dm#L2)
- **Статус в ss13-rs:** ❌ нет — камер нет

### Отчёт о вышедших на старте раунда
Список игроков, вышедших из игры в начале раунда (имя, ckey, работа, статус, антаг), для админов.

- **Код:** [`/datum/roundstart_logout_report_builder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/_helpers220/code/roundstart_logout_report.dm#L15)
- **BandaStation:** [`_helpers220.dme`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/_helpers220/_helpers220.dme#L1)
- **Статус в ss13-rs:** ❌ нет — админов нет

### Чейнджлог BandaStation
Окно чейнджлога читает собственный архив `html/changelogs/bandastation/archive/` и открывает интерфейс `ChangelogBandaStation`.

- **Код:** [`/datum/changelog/ui_interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/changelog/code/changelog.dm#L1); tgui: [`ChangelogBandaStation.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ChangelogBandaStation.jsx)
- **BandaStation:** [`_changelog.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/changelog/_changelog.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Автовики
Генерация шаблона вики со списком эмодзи сервера.

- **Код:** [`/datum/autowiki/emoji`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/autowiki/code/autowiki_emoji.dm#L1)
- **Статус в ss13-rs:** ❌ нет — инструмент сборки, игроку не виден

### Женские варианты униформы
Датум, задающий, как обрезается спрайт униформы для женского телосложения у отдельных вещей форка.

- **Код:** [`female_uniform_datum.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/datums/female_uniform_datum.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Служебная инфраструктура форка
Дефайны (`_defines220`), сигналы (`_signals220`) и синглтоны (`_singletons`) форка; пример модпака (`example`). Видимого игроку эффекта нет — нужны только как опора для других модпаков.

- **Код:** [`_defines220.dme`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/_defines220/_defines220.dme#L1), [`_signals220.dme`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/_signals220/_signals220.dme#L1), [`/datum/singleton`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/_singletons/code/singletons.dm#L1), [`_example.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/example/_example.dm#L1)
- **Статус в ss13-rs:** ❌ нет — прямого аналога нет (в Rust используются свои механизмы)
