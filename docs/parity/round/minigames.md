# Мини-игры
Внераундовые активности для призраков и выбывших.

### Мафия
Игра для призраков на отдельной карте: роли города, мафии-генокрадов и нейтралов (16 ролей), день/ночь, голосования, способности; панель `MafiaPanel`; достижения.

- **Код:** [`/datum/mafia_controller`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mafia/controller.dm#L12), [`/datum/action/innate/mafia_panel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mafia/controller.dm#L856), [`code/modules/mafia/roles`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mafia/roles); tgui: [`MafiaPanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/MafiaPanel.tsx)
- **Статус в ss13-rs:** ❌ нет

### Захват флага (CTF)
Командная игра призраков: классы, флаги, контрольные точки, ловушки, загрузка карт, голосование за запуск, средневековый вариант.

- **Код:** [`/datum/ctf_controller`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/capture_the_flag/ctf_controller.dm#L5), [`/datum/ctf_team`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/capture_the_flag/ctf_controller.dm#L191), [`/obj/machinery/ctf/spawner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/capture_the_flag/ctf_game.dm#L26), [`/obj/item/ctf_flag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/capture_the_flag/ctf_game.dm#L184); tgui: [`CTFPanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CTFPanel.tsx)
- **Статус в ss13-rs:** ❌ нет

### Дезматч
Лобби дезматча для призраков: выбор карты, наборов снаряжения и модификаторов, бой до последнего.

- **Код:** [`/datum/deathmatch_controller`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/deathmatch/deathmatch_controller.dm#L1), [`/datum/deathmatch_lobby`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/deathmatch/deathmatch_lobby.dm#L1); tgui: [`DeathmatchPanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/DeathmatchPanel.tsx)
- **Статус в ss13-rs:** ❌ нет

### Баскетбол
Командный баскетбол призраков с рефери и картами.

- **Код:** [`/datum/basketball_controller`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/basketball/controller.dm#L19); tgui: [`BasketballPanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/BasketballPanel.tsx)
- **Статус в ss13-rs:** ❌ нет

### Потерянный экипаж
Медицинские сценарии: в карго приходят тела погибших «потерянных» членов экипажа с повреждениями; оживлённые получают роль, медики — награду.

- **Код:** [`/datum/lost_crew_manager`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/lost_crew/lost_crew_manager.dm#L5), [`code/modules/lost_crew`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/lost_crew)
- **Статус в ss13-rs:** ❌ нет

### Обучение новичков
Интерактивные подсказки для новых игроков (сменить руку, выбросить предмет).

- **Код:** [`/datum/tutorial`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tutorials/_tutorial.dm#L3), [`/datum/tutorial_manager`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tutorials/_tutorial.dm#L157)
- **BandaStation:** [`/datum/tutorial_manager/should_run()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/overrides/code/tutorial_disable.dm#L1) — обучение отключено
- **Статус в ss13-rs:** ❌ нет
