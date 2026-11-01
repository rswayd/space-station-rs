# Голосования, конец смены и криосон
Голосования игроков (`SSvote`), автоматическая смена экипажа BandaStation и капсулы криосна.

### Подсистема голосований
`SSvote`: запуск голосования игроком/админом, кулдаун, одиночный и множественный выбор, голоса мёртвых (переключаемо), TGUI-панель и кнопка-действие «Голосовать».

- **Код:** [`SUBSYSTEM_DEF(vote)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/vote.dm#L4), [`/datum/vote`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/votes/_vote_datum.dm#L7), [`/datum/controller/subsystem/vote/proc/initiate_vote()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/vote.dm#L190), [`/datum/action/vote`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/vote.dm#L461); tgui: [`VotePanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/VotePanel.tsx)
- **Статус в ss13-rs:** ❌ нет

### Голосование за рестарт и пользовательское
Голосование за перезапуск сервера (если нет админов) и произвольное голосование с вариантами от админа.

- **Код:** [`/datum/vote/restart_vote`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/votes/restart_vote.dm#L4), [`/datum/vote/custom_vote`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/votes/custom_vote.dm#L4)
- **Статус в ss13-rs:** ❌ нет

_«Голосование за карту» описано в [../map/map-loading.md](../map/map-loading.md)._

### Автоматическая смена экипажа (BandaStation)
Подсистема `automatic_transfer` через настраиваемое время запускает голосование «Смена экипажа», затем повторяет через интервал; при победе вызывается эвакуационный шаттл.

- **Код:** [`SUBSYSTEM_DEF(automatic_transfer)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/automatic_crew_transfer/code/automatic_transfer_subsystem.dm#L1), [`/datum/vote/crew_transfer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/automatic_crew_transfer/code/crew_tranfer_vote.dm#L4), [`automatic_transfer_configuration.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/automatic_crew_transfer/code/automatic_transfer_configuration.dm#L1)
- **BandaStation:** [`_automatic_crew_transfer.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/automatic_crew_transfer/_automatic_crew_transfer.dm#L1)
- **Статус в ss13-rs:** ❌ нет — шаттл вызывается иначе (см. ../shuttles/)

### Капсулы криосна (BandaStation)
Игрок (или SSD-тело после `cryo_min_ssd_time`) ложится в капсулу и покидает раунд: слот профессии освобождается, вещи уходят в консоль хранения, объявление по радио; главы — с особым сообщением; тюремная и синдикатовская капсулы; ИИ может уйти в крио.

- **Код:** [`/obj/machinery/cryopod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/cryosleep/code/cryopod/cryopod.dm#L2), [`/obj/machinery/computer/cryopod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/cryosleep/code/cryoconsole.dm#L1), [`/obj/machinery/cryopod/prison`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/cryosleep/code/cryopod/prison_cryopod.dm#L2), [`/obj/machinery/cryopod/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/cryosleep/code/cryopod/syndicate_cryopod.dm#L2); tgui: [`CryopodConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CryopodConsole.jsx)
- **BandaStation:** [`_cryosleep.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/cryosleep/_cryosleep.dm#L1), [`subsystem/job.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/cryosleep/code/subsystem/job.dm#L1)
- **Статус в ss13-rs:** ❌ нет — слот освобождается только при исчезновении тела (`crates/ss13_game/src/jobs.rs`)
