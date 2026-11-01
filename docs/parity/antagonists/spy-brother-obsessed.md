# Шпион, кровные братья, одержимый
Три «малых» раундстарт/мидраунд-антагониста из экипажа.

### Шпион
`/datum/antagonist/spy`: агент с особым аплинком-сканером, выполняющий баунти (украсть предмет, просканировать машину) ради наград; шпионы соревнуются за одни и те же баунти.

- **Код:** [`/datum/antagonist/spy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/spy/spy.dm#L1), [`/datum/outfit/spy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/spy/spy.dm#L204), [`/datum/action/backup_uplink`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/spy/spy.dm#L215); tgui: [`AntagInfoSpy.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AntagInfoSpy.tsx)
- **Статус в ss13-rs:** ❌ нет — рулсет `Roundstart Spies` в `crates/ss13_game/src/antag/dynamic.rs` никого не спавнит

### Шпионский аплинк и баунти
Сканирование цели конусом выполняет баунти; обработчик раз в N минут обновляет список баунти по сложностям (настраивается в конфиге), награда — предметы из пула `spy_unique`.

- **Код:** [`/datum/component/spy_uplink`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/spy/spy_uplink.dm#L8), [`/datum/spy_bounty_handler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/spy/spy_bounty_handler.dm#L53), [`/datum/spy_bounty`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/spy/spy_bounty.dm#L9), [`/datum/spy_bounty/objective_item`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/spy/spy_bounty.dm#L233), [`/datum/spy_bounty/machine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/spy/spy_bounty.dm#L319), [`/datum/uplink_item/spy_unique`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/spy_unique.dm#L5); tgui: [`SpyUplink.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SpyUplink.tsx)
- **Статус в ss13-rs:** ❌ нет

### Кровные братья
Команда из 2+ братьев с общими целями; у братьев флешка и способность обращать вспышкой нового брата (с лимитом).

- **Код:** [`/datum/antagonist/brother`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/brother/brother.dm#L1), [`/datum/team/brother_team`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/brother/brother.dm#L208), [`/datum/antagonist/brother/proc/grant_conversion_skills()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/brother/brother.dm#L52), [`/datum/objective/convert_brother`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/brother/brother.dm#L317); tgui: [`AntagInfoBrother.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AntagInfoBrother.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/brother.rs` (`add_brother`, `BrotherTeam`, `on_successful_flash`, `roundend_report`): рулсет спавнит по брату на команду, общие цели (`forge_brother_objectives`), флешка в кармане, обращение успешной вспышкой со всеми проверками и лимитом (2, 10% — 3), командный HUD, `AntagInfoBrother`, отчёт команды; нет звука, мудлета, памяти, админ-команд, выхода из команды и цели `destroy` (нет ИИ); защита разума — по списку должностей с имплантом в tg (имплантов нет)

### Одержимый
Травма мозга «одержимость» выбирает объект; цели — проводить с ним время, сфотографировать, украсть вещь, устранить близких; бывший одержимый после выздоровления.

- **Код:** [`/datum/antagonist/obsessed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/obsessed/obsessed.dm#L7), [`/datum/brain_trauma/special/obsessed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/brain_damage/creepy_trauma.dm#L1), [`/datum/antagonist/former_obsessed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/obsessed/obsessed.dm#L31), [`/datum/outfit/obsessed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/obsessed/obsessed.dm#L102)
- **BandaStation:** [`objective_replacement.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/antagonists/code/objective_replacement.dm#L113) — новый объект одержимости при уходе старого в крио
- **Статус в ss13-rs:** ❌ нет
