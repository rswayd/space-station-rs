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
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/weapons/backstab.rs` (вызов из `crates/ss13_game/src/combat.rs` до блоков и брони): выкидной нож ×2 (+`force` в грудь без брони), КД 2 с, цель в сознании, оба смотрят по направлению удара; нет раны от добавочного удара и `log_combat`. Раскрытие ножа (`transforming`: 3→20) — ../combat/melee-weapons.md

### Разрывание стен (rip_and_tear)
Компонент тяжёлого оружия (кувалда, легендарные мечи): серия ударов по стене разрушает её, с затратой выносливости и требованием держать двумя руками; укреплённые стены — дольше.

- **Код:** [`/datum/component/rip_and_tear`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/~components/code/rip_and_tear_item.dm#L15)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/weapons/rip_and_tear.rs` (клик 4491, `construction::Job::TearWall`): кувалды форка (обычная 40 выносл./6 с, тактическая 30/5 с, D-6 20/3 с и укреплённые ×2) в двуручном хвате (`weapons::two_handed`) ломают стену шагами `tear_time / 3` с do_after, проверкой усталости, сообщением и звуками; `torn_wall` — три стадии, на третьей `devastate_wall`; без хвата «нужно держать двумя руками!», укреплённая без множителя — «слишком крепкая!». `// shortcut:` нет оверлея трещин, осмотра и ремонта сваркой

### Затраты выносливости за удар
Компонент: атака предметом тратит выносливость, по-разному для живых целей и объектов, в одной или двух руках.

- **Код:** [`/datum/component/stamina_cost_per_hit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/~components/code/stamina_cost_per_hit.dm#L10)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/weapons/stamina_cost.rs`: удары кувалдами по живым (`combat.rs`) и по объектам (`obj_damage::attackby`) тратят выносливость 12/10 (в хвате) и 10/8, D-6 — 0; удар, ведущий в стамкрит, отменяется («нет сил!»)

### Привязка предмета к ckey/роли
Компонент: предмет может поднять только владелец из белого списка ckey или не-станционная роль; остальные получают урон и отказ.

- **Код:** [`/datum/component/ckey_and_role_locked_pickup`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/~components/code/ckey_and_role_locked_pickup.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/locked_pickup.rs` (`CkeyLockedPickup`, `LockedPickupPlugin`): компонент вешается на все `/obj/item/dualsaber/legendary_saber*` с белым списком `mooniverse`, уроном 10 и строкой отказа своего меча; срабатывает при любом попадании в инвентарь (`Added<CarriedBy>`, как `COMSIG_ITEM_EQUIPPED`), с перепроверкой владельца; первое взятие привязывает (`binded`, как в форке), дальше чужой ckey — сбивание с ног на 10 с, предмет на пол мимо `NoDrop`, строка отказа, человеку 10–20 brute в случайную руку. `// shortcut:` нет исключения для роли `CENTCOM_ROLE_OFFICER` — ролей ЦК у разумов нет; сами легендарные мечи как оружие (двойной энергомеч) — [../combat/melee-weapons.md](../combat/melee-weapons.md)

### Нательная камера с паузой
Компонент бодикамеры, которая обновляется только пока за ней кто-то наблюдает; носитель видит алерт «за вами наблюдают».

- **Код:** [`/datum/component/pausable_bodycam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/~components/code/pausable_bodycam.dm#L2), [`/obj/machinery/camera/bodycam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/~components/code/bodycam_camera.dm#L2)
- **Статус в ss13-rs:** ❌ нет — зависимость: камеры есть (`crates/ss13_game/src/silicon/cameras.rs`), но консоли видят только картовые неподвижные камеры; движущаяся камера на мобе — ../security/cameras.md «Нательные камеры (bodycam)»

### Отчёт о вышедших на старте раунда
Список игроков, вышедших из игры в начале раунда (имя, ckey, работа, статус, антаг), для админов.

- **Код:** [`/datum/roundstart_logout_report_builder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/_helpers220/code/roundstart_logout_report.dm#L15)
- **BandaStation:** [`_helpers220.dme`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/_helpers220/_helpers220.dme#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_admin/src/logout_report.rs`: тело, которым управлял игрок, помнит его ckey (`PlayedBy`); через 10 ± 3 мин после старта раунда (`roundstart_logout_report_time_average/variance`, значения по умолчанию) админы с `R_ADMIN` получают в чат и в админ-лог «Roundstart logout report»: «Disconnected» (игрок вышел с сервера), «Dying» (в теле, hard crit), «Dead» (в теле или призраком у мёртвого тела); тело, из которого игрок перешёл в другое, пропускается. `// shortcut:` нет строк «Connected, Inactive» (у клиентов нет времени бездействия), «Suicide» (нет суицида), «Ghosted» (все призраки могут вернуться в тело); запись в blackbox — SQL, отложено ([../persistence-sql/database.md](../persistence-sql/database.md)); время и разброс не читаются из конфига

### Чейнджлог BandaStation
Окно чейнджлога читает собственный архив `html/changelogs/bandastation/archive/` и открывает интерфейс `ChangelogBandaStation`.

- **Код:** [`/datum/changelog/ui_interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/changelog/code/changelog.dm#L1); tgui: [`ChangelogBandaStation.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ChangelogBandaStation.jsx)
- **BandaStation:** [`_changelog.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/changelog/_changelog.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Автовики
Генерация шаблона вики со списком эмодзи сервера.

- **Код:** [`/datum/autowiki/emoji`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/autowiki/code/autowiki_emoji.dm#L1)
- **Статус в ss13-rs:** ✅ не портируется — генератор шаблона вики (запускается при сборке вики, не в раунде), игроку не виден; списка эмодзи сервера в ss13-rs нет

### Женские варианты униформы
Датум, задающий, как обрезается спрайт униформы для женского телосложения у отдельных вещей форка.

- **Код:** [`female_uniform_datum.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/datums/female_uniform_datum.dm#L1)
- **Статус в ss13-rs:** ❌ нет — зависимость: женской маски униформы (`wear_female_version`) нет вообще (`crates/ss13_game/src/human.rs` shortcut) — ../items/clothing-core.md, ../items/bandastation-clothing.md «Женские версии униформ»

### Служебная инфраструктура форка
Дефайны (`_defines220`), сигналы (`_signals220`) и синглтоны (`_singletons`) форка; пример модпака (`example`). Видимого игроку эффекта нет — нужны только как опора для других модпаков.

- **Код:** [`_defines220.dme`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/_defines220/_defines220.dme#L1), [`_signals220.dme`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/_signals220/_signals220.dme#L1), [`/datum/singleton`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/_singletons/code/singletons.dm#L1), [`_example.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/example/_example.dm#L1)
- **Статус в ss13-rs:** ✅ не портируется — своего поведения нет: дефайны и сигналы переносятся вместе с модпаками, которые их используют (в Rust — константы модулей, observers и сообщения Bevy), синглтоны — ресурсы Bevy, пример модпака — шаблон для разработчиков DM
