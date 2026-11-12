# Общая основа антагонистов
Базовый `/datum/antagonist`, команды, антаг-HUD, общее окно информации, спавнеры и отчёт конца раунда — то, на чём построены все антагонисты.

### Датум антагониста
`/datum/antagonist` на разуме: получение/снятие роли, приветствие и прощание, внутренние эффекты на тело, перенос при смене тела, цели, память, запрет защищённых ролей.

- **Код:** [`/datum/antagonist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_datum.dm#L6), [`/datum/antagonist/proc/on_gain()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_datum.dm#L243), [`/datum/antagonist/proc/greet()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_datum.dm#L351), [`/datum/antagonist/proc/on_removal()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_datum.dm#L321), [`/datum/mind/proc/add_antag_datum()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/antag.dm#L2)
- **BandaStation:** [`antag_datum.dm#L621`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_datum.dm#L621) — уведомление операций о смене целей
- **Статус в ss13-rs:** 🟡 частично — единого датума и разума нет: каждая роль — свой компонент тела и модуль в `crates/ss13_game/src/antag/` (предатель, брат, генокрад, еретик, культ, маг, нюки, революция, вампир, абдукторы, кошмар, морф, ревенант, блоб, ксеноморфы и др.) с выдачей, приветствием, целями и запретом защищённых ролей (`dynamic.rs`); нет переноса роли при смене тела, памяти антагониста, общего снятия роли

### Команды антагонистов
`/datum/team`: общие цели и члены (культ, революция, нюки, братья), командный отчёт конца раунда.

- **Код:** [`/datum/team`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_team.dm#L4)
- **Статус в ss13-rs:** ✅ реализовано — команды с общими целями, членами и командным отчётом конца раунда у своих ролей: `antag/brother.rs` (`BrotherTeam`), `antag/cult/server/mod.rs` (`CultTeam`), `antag/nukeop/server.rs` (`NukeTeam`, `LoneopTeam`, `battlecruiser.rs`), `antag/abductor/server/mod.rs` (`AbductorTeams`), `antag/ert.rs` (`ErtTeam`), революция (`antag/revolution/server.rs`), рейдеры (`antag/raiders/`). `// shortcut:` общего типа `/datum/team` нет — у каждой роли своя структура

### Антаг-HUD
Иконки ролей над головами, видимые самому антагу и его союзникам (и админам/наблюдателям при включении).

- **Код:** [`/datum/atom_hud/alternate_appearance/basic/antagonist_hud`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_hud.dm#L30), [`/datum/atom_hud/alternate_appearance/basic/has_antagonist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_hud.dm#L5)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/hud.rs`, `crates/ss13_game/src/antag/client.rs` (`AntagHud`): иконка над своим мобом и над видимыми членами команды (кровные братья, обновление раз в 0.9 с); наблюдателей и админов нет

### Окно информации антагониста и кнопка
Действие «Antag Info» открывает TGUI-окно роли (`AntagInfoGeneric` или своё у каждого антага: `AntagInfoChangeling`, `AntagInfoHeretic` и т.д.).

- **Код:** [`/datum/action/antag_info`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_datum.dm#L151), [`/datum/antagonist/ui_interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_datum.dm#L120); tgui: [`AntagInfoGeneric.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AntagInfoGeneric.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/mod.rs` (`AntagHudUi`), окна `AntagInfoTraitor`, `AntagInfoBrother`, `AntagInfoBrainwashed`; у остальных ролей свои окна-панели в их модулях (генокрад, культ, вампир, еретик — `*/server/windows.rs`) или общий текст целей; нет единого `AntagInfoGeneric` для всех ролей

### Кастомный антагонист
Админ может выдать «Custom Antagonist» с произвольным названием и целями.

- **Код:** [`/datum/antagonist/custom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_datum.dm#L100)
- **Статус в ss13-rs:** ❌ нет

### Спавнеры антагонистов
Предметы, вызывающие призрака на роль: контракт ученика мага, маяки подкрепления нюков/клоун-опов/киборгов Синдиката, бутылки демонов резни и смеха, загрузочные маяки (контрактор, обезьяна-агент).

- **Код:** [`/obj/item/antag_spawner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_spawner.dm#L1), [`/obj/item/antag_spawner/contract`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_spawner.dm#L17), [`/obj/item/antag_spawner/nuke_ops`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_spawner.dm#L103), [`/obj/item/antag_spawner/slaughter_demon/laughter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_spawner.dm#L292), [`/obj/item/antag_spawner/loadout`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_spawner.dm#L306)
- **BandaStation:** [`antag_spawner.dm#L79`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_spawner.dm#L79) — при переносе префов антагу не применяются модификации тела
- **Статус в ss13-rs:** 🟡 частично — контракт ученика мага (`antag/wizard/server/mod.rs`, `CONTRACT`), маяки подкрепления нюков, клоун-опов и киборгов Синдиката (`antag/nukeop/reinforcements.rs`), бутылки демонов резни и смеха (`antag/slaughter.rs`, `use_bottle`); нет загрузочных маяков контрактора и обезьяны-агента, дроп-пода подкрепления

### Отчёт антагонистов в конце раунда
`antag_report` собирает секции всех антагов и команд: игроки, цели с отметками, особые итоги ролей.

- **Код:** [`/datum/controller/subsystem/ticker/proc/antag_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L617), [`printobjectives()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/roundend.dm#L734), [`/datum/antagonist/proc/roundend_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/_common/antag_datum.dm#L400)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/round.rs` собирает `roundend_report` всех ролей и команд: предатели, братья, вампиры, генокрады, нюки (и одиночка, крейсер), культ, еретики, маг, абдукторы, парадокс-клон, кошмар, рейдеры (пираты, ниндзя, беглецы), морф, ксеноморфы, дракон, ревенант, демон резни, блоб, промытые, валентинки, выживальщики, революция — игроки, цели с отметками, итоги ролей. Нет секции сбойного ИИ (его цели не проверяются — malf-ai.md)

### Админские панели антагов
Check Antagonists (список антагов раунда, статус шаттла, команды) и Traitor Panel разума (выдача/снятие ролей и целей).

- **Код:** [`/datum/admins/proc/check_antagonists()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/check_antagonists.dm#L93), [`/datum/mind/proc/traitor_panel()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/antag_panel.dm#L102), [`/datum/antagonist/proc/antag_panel()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/antag_panel.dm#L30)
- **Статус в ss13-rs:** ❌ нет — админов нет (см. ../admin/)
