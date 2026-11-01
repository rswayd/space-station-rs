# Персональный ИИ (пИИ)
Карта пИИ и моб `/mob/living/silicon/pai`, которым управляет призрак.

### Карта пИИ и запрос личности
Предмет-карта: владелец запрашивает пИИ, призраки с заполненной анкетой (`PaiSubmit`, сохраняется в savefile) откликаются; владелец выбирает кандидата, задаёт главную директиву, может стереть пИИ.

- **Код:** [`/obj/item/pai_card`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/card.dm#L1), [`/datum/pai_candidate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/candidate.dm#L7), [`proc/savefile_save()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/personality.dm#L13); tgui: [`PaiCard.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PaiCard.tsx), [`PaiSubmit.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PaiSubmit.tsx)
- **Статус в ss13-rs:** ❌ нет

### пИИ как моб и голошасси
пИИ живёт в карте, может развернуться в голограмму-зверька (выбор шасси), отдыхать, светить, быть подобранным; поводок держит его рядом с картой; урон сворачивает обратно.

- **Код:** [`/mob/living/silicon/pai`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/pai.dm#L1), [`proc/fold_out()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/shell.dm#L107), [`proc/choose_chassis()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/shell.dm#L48), [`proc/toggle_leash()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/pai.dm#L228), [`defense.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/defense.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### ПО пИИ
Покупка программ за RAM: мессенджер, сканер здоровья хоста, HUD (мед/без), переводчик языков, атмос-сенсор, GPS, синтезатор музыки, новостник, сигнализатор, камера (печать фото), дверной взломщик, пульс ДНК. Форк добавляет программу «Монитор экипажа».

- **Код:** [`proc/buy_software()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/software.dm#L104), [`software.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/software.dm#L26), [`/obj/item/camera/siliconcam/pai_camera`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/camera.dm#L1); tgui: [`PaiInterface`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PaiInterface)
- **BandaStation:** [`software.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/software.dm#L125), [`pai.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/pai.dm#L77) — программа «Crew Monitor»
- **Статус в ss13-rs:** ❌ нет

### Дверной взломщик (door jack)
Кабель пИИ втыкается в шлюз и через время взламывает его, открывая дверь.

- **Код:** [`door_jack.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/door_jack.dm#L10)
- **Статус в ss13-rs:** ❌ нет

### Законы и емаг пИИ
пИИ подчиняется мастеру (законы `pai`), емаг меняет мастера на взломщика; ДНК-привязка мастера.

- **Код:** [`/mob/living/silicon/pai/emag_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/pai.dm#L167), [`proc/check_dna()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/software.dm#L162)
- **Статус в ss13-rs:** ❌ нет
