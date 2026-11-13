# Звук
Звуковая система: позиционный звук, каналы, окружение, эмбиент, циклические звуки и звуковые правки BandaStation. В ss13-rs: позиционный звук `playsound` (сервер считает громкость каждому слушателю, клиент играет через `bevy_audio`).

### Позиционный звук
`playsound`: звук от источника с дальностью, затуханием, учётом давления (тише в вакууме), стен/z-уровней, рандомизацией частоты; `playsound_local`, звуки по ключам (`get_sfx`: шаги, удары, стекло...).

- **Код:** [`playsound()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/sound/sound.dm#L19), [`/mob/proc/playsound_local()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/sound/sound.dm#L102), [`get_sfx()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/sound/sound.dm#L218), [`code/game/sound/sound_keys`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/sound/sound_keys)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/sound.rs` (`PlaySound`: дальность 15 + extrarange, евклидово расстояние, тот же z, `CALCULATE_SOUND_VOLUME_RATIO`, давление, глухие (мутация, наушники) пропускаются, `vary` 32000-55000 Гц, `get_sfx` по `<assets>/sfx.ron`, `ignore_walls = FALSE` — слушатели по `view()` от источника, призраки слышат всё), клиент `crates/ss13_game/src/sound_client.rs` (громкость, скорость, панорама по dx); `// shortcut:` нет звуков с соседних z (в проекте нет multi-z), реверба, `volume_preference`, `pitch` звука идёт как скорость. Подключены: выстрелы (энергооружие ниже тоном при разряде), осечка/зарядка/затвор/помпа, `empty_alarm`, удары предметами и кулаками, шаги, шлюзы (в т.ч. `airlockforced`), гермозаслонки и ставни, виндоры, буззер и шипение мусоропровода

### Каналы и звуковое окружение
Резервирование каналов (`SSsounds`), окружения BYOND (эхо в помещениях, искажение в космосе/под водой), громкости по категориям в настройках.

- **Код:** [`SUBSYSTEM_DEF(sounds)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/sounds.dm#L3), [`sounds.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/sounds.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Эмбиент зон
`SSambience`: фоновые звуки зоны (техтоннели, мостик, медбей, космос...), гул корабля, случайные звуки, настройка громкости.

- **Код:** [`SUBSYSTEM_DEF(ambience)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ambience.dm#L2), [`/area/proc/play_ambience()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ambience.dm#L59)
- **Статус в ss13-rs:** ✅ — данные зон: `crates/ss13_mapload/src/ambience.rs` (`ambientsounds` после `/area/Initialize` из `GLOB.ambience_assoc`, дамп-глобалы `*_ambience` в `ss13_dm_extract::DUMPED_GLOBALS`; `ambient_buzz`, `ambient_buzz_vol`, кулдауны, `forced_ambience`, `sound_environment` в `DmArea`; все файлы эмбиента — в конвертации звуков); сервер: `crates/ss13_game/src/ambience.rs` (`SSambience.fire` раз в секунду: случайный звук зоны на `CHANNEL_AMBIENCE`, кулдаун «длина звука + rand(min, max)», первый через 10 с, `forced_ambience` 5% новичку, глухие/крит/мёртвые пропускаются, призраки слышат; пещерные звуки техтоннелей в темноте; гул `ambient_buzz` в цикле на `CHANNEL_BUZZ` при входе в зону, смена/остановка, остановка без APC/энергии окружения, кроме шахтного уровня; призракам гул не обновляется, как в tg); клиент — общие канальные звуки `sound_client.rs`. Нет: настроек громкости игрока (`sound_ambience_volume`, `sound_ship_ambience_volume` — берётся 100%, в порте нет префов; общий `SS13_VOLUME` действует); `sound_environment` (реверб) не применяется — это пункт «Позиционный звук»; глухота опрашивается раз в секунду вместо сигналов `TRAIT_DEAF`; правки переменных эмбиента на картах не читаются

### Циклические звуки и звуковые токены
`looping_sound` для машин (суперматерия, генераторы, микроволновка...), звуковые токены — долгие звуки, следующие за источником.

- **Код:** [`/datum/looping_sound`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/looping_sounds/_looping_sound.dm#L4), [`/datum/sound_token`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/sound_token.dm#L4), [`SUBSYSTEM_DEF(sound_tokens)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/sound_tokens.dm#L1)
- **BandaStation:** [`looping_sound.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/sounds/code/looping_sound.dm#L1) — громкость звуков суперматерии, гипертора, часов
- **Статус в ss13-rs:** ❌ нет

### Глобальные звуки
Звуки всем игрокам (объявления, синематики, конец раунда, админ-звуки).

- **Код:** [`sound_to_playing_players()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/sound/sound.dm#L185)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/sound.rs` (`PlaySound::to_playing_players`); пока ни одна механика его не вызывает

### Звуковые правки BandaStation
Модпак `sounds`: громкость шагов MOD-ботинок по замедлению, «hugbox»-суперматерия; модпак `aesthetics_sounds`: свои звуки гиперпространства малых шаттлов, шкафчиков, интро антагонистов.

- **Код:** [`mod_shoes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/sounds/code/mod_shoes.dm#L1), [`supermatter.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/sounds/code/supermatter.dm#L1), [`shuttle.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics_sounds/code/shuttle.dm#L1), [`closet.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics_sounds/code/closet.dm#L1)
- **BandaStation:** [`_sounds.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/sounds/_sounds.dm#L1), [`_aesthetics_sounds.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics_sounds/_aesthetics_sounds.dm#L1)
- **Статус в ss13-rs:** ❌ нет
