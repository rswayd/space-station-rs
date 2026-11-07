# Голодек
Голографическая комната с программами-шаблонами (`code/modules/holodeck`).

### Компьютер голодека
Консоль загружает в связанную область программу — карту-шаблон (пустой корт, додж- и баскетбол, тандердом, пляж, лаунж и др., ~30 программ); загрузка с задержкой, смена программы стирает голо-объекты. При повреждении пола области — аварийное отключение.

- **Код:** [`/obj/machinery/computer/holodeck`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/computer.dm#L32), [`load_program()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/computer.dm#L196), [`/datum/map_template/holodeck`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/holodeck_map_templates.dm#L2); tgui: [`Holodeck.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Holodeck.jsx)
- **Статус в ss13-rs:** ✅ `ss13_game::holodeck`: консоль связывает `rec_center`, окно `Holodeck` (29 программ), загрузка шаблона в угол области только на holodeck-совместимые полы, кулдаун 2 с (+5 с при повреждении), проверка пола и аварийное отключение, дерез ушедших из области голограмм, отключение при потере питания и перезагрузка через 2,5 с. Без расхода энергии по голограммам, взрыва при перегрузке, отключения от ЭМИ/взрыва, разблокировки шаттла 1218 AD; каждая загрузка оставляет пустой уровень шаблона (shortcut).

### Взлом голодека (emag)
Emag снимает защиту: открываются опасные программы (1218 AD, тест горения, дикая природа, бункер, пчёлы и др.), голо-оружие наносит настоящий урон.

- **Код:** [`emag_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/computer.dm#L439)
- **Статус в ss13-rs:** ✅ `ss13_game::holodeck`: опасные программы в окне и в `ui_act` только после emag, голограммы возвращают свой `damtype`, перестают дерезиться вне комнаты; силикон сбрасывает защиту кнопкой Safeties. Без сообщения админам (только лог сервера).

### Голо-полы
Полы голодека (трава, пляж, асфальт, космос, часовня и др.), которые нельзя разобрать.

- **Код:** [`/turf/open/floor/holofloor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/turfs.dm#L1)
- **Статус в ss13-rs:** ✅ `ss13_game::holodeck`: голо-пол не принимает предметы (лом, плитка — `ITEM_INTERACT_BLOCKING`), не горит и не ломается (`turf_damage`), случайный вид травы/астероида/базальта/гиперкосмоса, углы часовни. 🟡 Без света базальта; рыбалка в голо-воде — у `fishing`.

### Голо-эффекты и спавнеры
Эффекты программ: колоды карт, случайные книги, спавн голо-животных (питомцы, пчёлы, обезьяны, пингвины), исчезающие при выходе из голодека.

- **Код:** [`/obj/effect/holodeck_effect`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/holo_effect.dm#L7), [`/obj/effect/holodeck_effect/mobspawner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/holo_effect.dm#L53)
- **Статус в ss13-rs:** ✅ `ss13_game::holodeck`: колода, искры с hotspot, случайное руководство, голо-животные (питомцы, пчёлы, обезьяна, пингвины, краб, карп) активируются после загрузки и исчезают со сменой программы. Без трейтов голо-мобов (дропы при смерти остаются, shortcut).

### Голо-предметы и спорт
Голографические энергомечи, додж-мяч, кнопка готовности команд для матчей, конвейер голодека.

- **Код:** [`/obj/item/melee/energy/sword/holographic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/items.dm#L10), [`/obj/item/toy/dodgeball`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/items.dm#L53), [`/obj/machinery/readybutton`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/items.dm#L75)
- **Статус в ss13-rs:** ✅ `ss13_game::holodeck`: додж-мяч (10 стамины человеку, 5% — паралич 6 с), кнопка готовности (все кнопки области готовы — голо-окна исчезают, «FIGHT!»), конвейер голодека, голо-скейтборд не поднимается (`vehicles`); голо-энергомеч — через `antag::syndie_items` по DM-переменным.
