# Музыкальные инструменты и музыкальный автомат
Инструменты с редактором песен, стационарные пианино и барабаны, музыкальный автомат (`code/modules/instruments`, `code/game/machinery/dance_machine.dm`, модпаки `music`, `jukebox`).

### Песня и редактор песен
Песня — текст нот в BYOND-нотации (ограничение по строкам), темп, число повторов, громкость, затухание; играется аккордами по таймеру для всех в слышимости, кто включил префу инструментов. tgui-редактор: импорт текста, выбор инструмента.

- **Код:** [`/datum/song`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/songs/_song.dm#L7), [`start_playing()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/songs/_song.dm#L210), [`process_song()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/songs/_song.dm#L315), [`/datum/song/ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/songs/editor.dm#L72); tgui: [`InstrumentEditor.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/InstrumentEditor.tsx), [`InstrumentEditor220.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/InstrumentEditor220.tsx)
- **BandaStation:** [`code/modules/instruments/songs/_song.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/songs/_song.dm#L138) — снятие ограничения темпа (BPM вместо децисекунд), [`modular_bandastation/music/code/song.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/music/code/song.dm#L3) — синхронное начало игры нескольких музыкантов (auto unison в радиусе 5, передача песни другому инструменту), свой tgui-редактор
- **Статус в ss13-rs:** ❌ нет

### Синтезируемые инструменты (сэмплы)
`/datum/instrument` с сэмплами по клавишам (пианино, орган, гитары, медь, хроматическая перкуссия, синтезатор-тона, «весёлые»: honk, мяу и т.д.) и legacy-инструменты на хардкод-звуках.

- **Код:** [`/datum/instrument`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/instrument_data/_instrument_data.dm#L19), [`/datum/instrument/hardcoded`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/instrument_data/hardcoded.dm#L4), [`compile_synthesized()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/songs/play_synthesized.dm#L4)
- **BandaStation:** [`modular_bandastation/objects/code/items/instruments.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/instruments.dm#L3) — гитары Soundhand (рок, метал, бас) и барабаны
- **Статус в ss13-rs:** ❌ нет

### Ручные инструменты
Скрипка, банджо, гитара, электрогитара, глокеншпиль, аккордеон, труба, саксофон, тромбон, блокфлейта, губная гармошка, велосипедный гудок, музыкальная моль; клик в руке открывает редактор песни.

- **Код:** [`/obj/item/instrument`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/items.dm#L2), [`can_play()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/items.dm#L29), [`/obj/item/instrument/violin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/items.dm#L48), [`/obj/item/instrument/trumpet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/items.dm#L112)
- **BandaStation:** [`modular_bandastation/objects/code/items/instruments.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/instruments.dm#L33) — гитары Soundhand и складная барабанная установка
- **Статус в ss13-rs:** ❌ нет

### Синтезатор и наушники
Портативный синтезатор с выбором любого инструмента; наушники-синтезатор (играет только носителю); компонент схемы для wiremod.

- **Код:** [`/obj/item/instrument/piano_synth`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/piano_synth.dm#L2), [`/obj/item/instrument/piano_synth/headphones`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/piano_synth.dm#L23), [`/obj/item/circuit_component/synth`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/piano_synth.dm#L71)
- **Статус в ss13-rs:** ❌ нет

### Стационарные инструменты
Пианино и минимуг (на них играют, стоя рядом), разбираются/закрепляются ключом.

- **Код:** [`/obj/structure/musician`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/stationary.dm#L1), [`/obj/structure/musician/piano`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/stationary.dm#L45)
- **BandaStation:** [`code/modules/instruments/stationary.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/instruments/stationary.dm#L42) — смена иконки во время игры; [`modular_bandastation/objects/code/items/instruments.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/instruments.dm#L81) — барабанная установка (сворачивается в предмет)
- **Статус в ss13-rs:** ❌ нет

### Музыкальный автомат (jukebox)
Автомат проигрывает треки из `config/jukebox_music/sounds/` в радиусе (громкость, повтор, выбор трека), разносится ключом; диско-вариант с танцполом и светом.

- **Код:** [`/obj/machinery/jukebox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/dance_machine.dm#L1), [`toggle_playing()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/dance_machine.dm#L127), [`/obj/machinery/jukebox/disco`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/dance_machine.dm#L170), [`/datum/jukebox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/jukebox.dm#L14), [`load_songs_from_config()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/jukebox.dm#L94); tgui: [`Jukebox.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Jukebox.tsx), [`Jukebox220.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Jukebox220.tsx)
- **BandaStation:** [`modular_bandastation/jukebox/code/jukebox_ui.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jukebox/code/jukebox_ui.dm#L15) — своё tgui-окно, загрузка своих треков игроками (`upload_file()`) и их сохранение с разрешения
- **Статус в ss13-rs:** ❌ нет
