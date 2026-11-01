# Революция
Главы революции обращают экипаж вспышкой и должны убить или изгнать всех глав станции; СБ подавляет мятеж имплантами лояльности.

### Революционеры и главы революции
`/datum/antagonist/rev/head` — получают флешку и умение обращать вспышкой; `/datum/antagonist/rev` — рядовые (деконвертируются ударом по голове/имплантом лояльности); команда с целями mutiny на каждого главу.

- **Код:** [`/datum/antagonist/rev`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/revolution/revolution.dm#L1), [`/datum/antagonist/rev/head`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/revolution/revolution.dm#L171), [`/datum/team/revolution`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/revolution/revolution.dm#L418), [`/datum/outfit/revolutionary`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/revolution/revolution.dm#L594)
- **BandaStation:** [`revolution.dm#L233`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/revolution/revolution.dm#L233), [`modular_bandastation/revolution_overhaul/code/revolution.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/revolution_overhaul/code/revolution.dm#L1) — проверка роста революции при каждом обращении
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/revolution/server.rs` (`add_revolutionary`, `on_successful_flash`, `make_dormant`, `reveal_head`, `complete_implant`, `head_trauma`): главы со вспышкой, конверсия вспышкой, HUD `rev`/`rev_head`, имплант защиты разума (деконверсия, глава сопротивляется), деконверсия ударом по голове; без кодовых фраз, цели конверсии и HUD-импланта

### Обработчик революции и исход
`/datum/revolution_handler`: проверки победы (все главы мертвы/вне станции) или поражения (все главы революции нейтрализованы), новые цели при поздних главах, итог в отчёте, «враги государства» после поражения.

- **Код:** [`/datum/revolution_handler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/revolution/revolution_handler.dm#L3), [`/datum/revolution_handler/proc/declare_revs_win()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/revolution/revolution_handler.dm#L78), [`/datum/antagonist/enemy_of_the_state`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/revolution/enemy_of_the_state.dm#L6)
- **BandaStation:** [`revolution_handler.dm#L70`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/revolution/revolution_handler.dm#L70) — новые цели раздаются всем революционерам; [`revolution_handler.dm#L89`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/revolution/revolution_handler.dm#L70) — объявление Синдиката о победе революции
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/revolution/server.rs` (`RevolutionHandler`, `check_rev_victory`, `check_heads_victory`, `roundend_report`): цели «убить или изгнать» глав (и латджойн-глав), победа революции без конца раунда (правка BandaStation), победа станции с деконверсией всех; без AFK-грации и враждебной среды для шаттла

### Пороги роста революции (BandaStation)
При достижении доли революционеров (разный порог для низкого и высокого населения) — звук и сообщение революционерам и объявление ЦК с разрешением СБ применять летальную силу.

- **Код:** [`/datum/team/revolution/proc/check_size()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/revolution_overhaul/code/revolution.dm#L9)
- **BandaStation:** [`_revolution_overhaul.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/revolution_overhaul/_revolution_overhaul.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/revolution/server.rs` (`check_size`): 50% / 40% при 70+ живых, оповещение станции
