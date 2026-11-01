# Голодек
Голографическая комната с программами-шаблонами (`code/modules/holodeck`).

### Компьютер голодека
Консоль загружает в связанную область программу — карту-шаблон (пустой корт, додж- и баскетбол, тандердом, пляж, лаунж и др., ~30 программ); загрузка с задержкой, смена программы стирает голо-объекты. При повреждении пола области — аварийное отключение.

- **Код:** [`/obj/machinery/computer/holodeck`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/computer.dm#L32), [`load_program()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/computer.dm#L196), [`/datum/map_template/holodeck`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/holodeck_map_templates.dm#L2); tgui: [`Holodeck.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Holodeck.jsx)
- **Статус в ss13-rs:** ❌ нет

### Взлом голодека (emag)
Emag снимает защиту: открываются опасные программы (1218 AD, тест горения, дикая природа, бункер, пчёлы и др.), голо-оружие наносит настоящий урон.

- **Код:** [`emag_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/computer.dm#L439)
- **Статус в ss13-rs:** ❌ нет

### Голо-полы
Полы голодека (трава, пляж, асфальт, космос, часовня и др.), которые нельзя разобрать.

- **Код:** [`/turf/open/floor/holofloor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/turfs.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Голо-эффекты и спавнеры
Эффекты программ: колоды карт, случайные книги, спавн голо-животных (питомцы, пчёлы, обезьяны, пингвины), исчезающие при выходе из голодека.

- **Код:** [`/obj/effect/holodeck_effect`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/holo_effect.dm#L7), [`/obj/effect/holodeck_effect/mobspawner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/holo_effect.dm#L53)
- **Статус в ss13-rs:** ❌ нет

### Голо-предметы и спорт
Голографические энергомечи, додж-мяч, кнопка готовности команд для матчей, конвейер голодека.

- **Код:** [`/obj/item/melee/energy/sword/holographic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/items.dm#L10), [`/obj/item/toy/dodgeball`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/items.dm#L53), [`/obj/machinery/readybutton`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holodeck/items.dm#L75)
- **Статус в ss13-rs:** ❌ нет
