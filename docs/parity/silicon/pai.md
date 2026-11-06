# Персональный ИИ (пИИ)
Карта пИИ и моб `/mob/living/silicon/pai`, которым управляет призрак.

### Карта пИИ и запрос личности
Предмет-карта: владелец запрашивает пИИ, призраки с заполненной анкетой (`PaiSubmit`, сохраняется в savefile) откликаются; владелец выбирает кандидата, задаёт главную директиву, может стереть пИИ.

- **Код:** [`/obj/item/pai_card`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/card.dm#L1), [`/datum/pai_candidate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/candidate.dm#L7), [`proc/savefile_save()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/personality.dm#L13); tgui: [`PaiCard.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PaiCard.tsx), [`PaiSubmit.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PaiSubmit.tsx)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/silicon/pai.rs`: меню карты (список вместо `PaiCard`), запрос пингует призраков раз в 40 с, призрак кликает карту или её носителя и заполняет анкету (имя, описание, OOC — три текстовых промпта вместо `PaiSubmit`), выбор кандидата-призрака скачивает пИИ в карту, директива, стирание. Сохранение анкеты в savefile — за волной персистентности (анкета живёт раунд).

### пИИ как моб и голошасси
пИИ живёт в карте, может развернуться в голограмму-зверька (выбор шасси), отдыхать, светить, быть подобранным; поводок держит его рядом с картой; урон сворачивает обратно.

- **Код:** [`/mob/living/silicon/pai`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/pai.dm#L1), [`proc/fold_out()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/shell.dm#L107), [`proc/choose_chassis()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/shell.dm#L48), [`proc/toggle_leash()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/pai.dm#L228), [`defense.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/defense.dm#L2)
- **Статус в ss13-rs:** ✅ есть — свёрнутый пИИ невидим и стоит на тайле карты, кнопки: ПО, развернуться/свернуться, шасси (список), отдых, законы; 40 с после загрузки (сразу после ДНК), 5/10 с перезарядки; поводок 3..9 возвращает к карте; brute/burn идут в голошасси (20, +1.25/с), ниже 0 — сворачивание. Нет: подбор пИИ в руки (мехника mob holder), свет.

### ПО пИИ
Покупка программ за RAM: мессенджер, сканер здоровья хоста, HUD (мед/без), переводчик языков, атмос-сенсор, GPS, синтезатор музыки, новостник, сигнализатор, камера (печать фото), дверной взломщик, пульс ДНК. Форк добавляет программу «Монитор экипажа».

- **Код:** [`proc/buy_software()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/software.dm#L104), [`software.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/software.dm#L26), [`/obj/item/camera/siliconcam/pai_camera`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/camera.dm#L1); tgui: [`PaiInterface`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PaiInterface)
- **BandaStation:** [`software.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/software.dm#L125), [`pai.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/pai.dm#L77) — программа «Crew Monitor»
- **Статус в ss13-rs:** ✅ есть — покупка за 100 RAM по ценам tg (вкл. «Crew Monitor» форка), сканер хоста (носитель/мастер), переводчик, слот шифрования, дверной взломщик, проверка ДНК. 🟡 устройства программ в своих группах: мед/сек HUD (records HUD), атмос-сенсор, GPS, синтезатор, новостник, сигнализатор, камера, мессенджер, манифест, монитор экипажа — покупаются, но сообщают, что устройства нет.

### Дверной взломщик (door jack)
Кабель пИИ втыкается в шлюз и через время взламывает его, открывая дверь.

- **Код:** [`door_jack.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/door_jack.dm#L10)
- **Статус в ss13-rs:** ✅ есть — кабель (`/obj/item/pai_cable`) в руку носителя или на пол, втыкается в шлюз, взлом 15 с открывает дверь, кабель убирается дальше 2 тайлов.

### Законы и емаг пИИ
пИИ подчиняется мастеру (законы `pai`), емаг меняет мастера на взломщика; ДНК-привязка мастера.

- **Код:** [`/mob/living/silicon/pai/emag_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/pai.dm#L167), [`proc/check_dna()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/pai/software.dm#L162)
- **Статус в ss13-rs:** ✅ есть — законы `pAI Directives` (закон 0 «Serve your master.»), ДНК-привязка мастера, директива только при мастере (заменяет, а не дописывает — отклонение от бага tg), емаг на пИИ/карте (на полу) — мастер «The Syndicate», сброс ПО.
