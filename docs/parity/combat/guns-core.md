# Огнестрел: общая механика
Как стреляет любое `/obj/item/gun`: выстрел, разброс, очереди, отдача, обвесы. Семейства оружия — в [guns-ballistic.md](guns-ballistic.md), [guns-energy.md](guns-energy.md), [guns-special.md](guns-special.md). Огнестрел в ss13-rs: `crates/ss13_game/src/guns.rs`, снаряды — `crates/ss13_game/src/projectile.rs`.

### Выстрел
Клик с оружием по дальней цели (или в упор) стреляет: проверка бойка (`firing_pin`), `can_shoot`, затем гильза создаёт снаряд, летящий в цель с разбросом; звук, вспышка, сообщение; выстрел в упор не промахивается.

- **Код:** [`/obj/item/gun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/gun.dm#L5), [`proc/fire_gun()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/gun.dm#L396), [`proc/process_fire()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/gun.dm#L565), [`proc/can_shoot()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/gun.dm#L227), [`proc/shoot_live_shot()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/gun.dm#L258)
- **BandaStation:** [`modular_bandastation/weapon/code/ranged/overrides/ballistic_master.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/ranged/overrides/ballistic_master.dm#L1) — правки параметров оружия форка
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/guns.rs`: выстрел по клику вдаль и в упор по мобу с точностью до пикселя клика (`ICON_X`/`ICON_Y`, сервер ограничивает смещение тайлом), боёк, пустой щелчок, `fire_delay`, `CLICK_CD_RANGE`, `WEAPON_HEAVY` (нужны обе руки), сообщения; без звуков и вспышки

### Разброс, отдача и стрельба с двух рук
Разброс (`spread`) растёт при стрельбе с двух рук и без обеих рук на двуручном оружии; отдача трясёт камеру; оружие в обеих руках стреляет по очереди (akimbo).

- **Код:** [`proc/process_fire()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/gun.dm#L565), [`proc/fire_sounds()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/gun.dm#L242)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/guns.rs`: `spread` оружия (у обреза болтовки 36, Jezail 50), `variance`/`randomspread` гильзы, разброс очереди (умный и случайный), стрельба с двух рук (лёгкие стволы во второй руке стреляют через децисекунду, `dual_wield_spread` с `AKIMBO_PENALTY_SCALING`), звуки выстрела (`fire_sounds`, глушитель); отдача `recoil`/`min_recoil` толкает камеру (`crates/ss13_game/src/recoil.rs`). Настройка `min_recoil_multiplier` — в настройках игрока (ui)

### Очереди и автоматический огонь
`burst_size` даёт очередь из нескольких выстрелов; компонент `automatic_fire` — стрельба при зажатой кнопке; переключение режимов огня.

- **Код:** [`proc/process_burst()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/gun.dm#L495), [`/datum/component/automatic_fire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/fullauto.dm#L4)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/guns.rs`: очередь `burst_size`/`burst_delay` (C-20r); нет переключения режимов и автоогня

### Самоубийство из оружия
Выстрел себе в голову в упор с задержкой (`handle_suicide`) — мгновенная смерть.

- **Код:** [`proc/handle_suicide()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/gun.dm#L681)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/guns.rs` (`handle_suicide`, `complete_suicide`): зона «рот» и клик оружием по себе или соседнему человеку — 12 с `do_after`, затем выстрел в голову с уроном ×5 и `wound_bonus` +5; отмена при движении или смене зоны («решает не стрелять» / «решает пощадить»); русская рулетка (`doafter_self_shoot = FALSE`) исключена; у очереди стреляет только первый патрон

### Взятие на мушку (gunpoint)
Наведение оружия на моба: цель получает `heldup` и алерт сдачи, стрелок — `holdup` с алертом отмены; движение или действие цели — выстрел.

- **Код:** [`/datum/component/gunpoint`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/gunpoint.dm#L15), [`/datum/status_effect/holdup`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/neutral.dm#L185)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/gunpoint.rs`: ПКМ оружием без боевого режима по живому мобу в пределах 4 тайлов (BandaStation) — `do_after` 0,5 с, обездвиживание обоих на 0,75 с / дистанция, алерты `holdup` (клик — опустить оружие), `heldup` и сдачи у цели, мудлет `gunpoint`; стадии ×1,25 → ×1,75 (2,5 с) → ×2,25 (ещё 7,5 с) с бонусом ранения; шаг цели, её атака моба или выстрел — заряженный выстрел; потеря видимости за 4 тайла или оружие не в руках — отмена; без выстрела от вздрагивания при ранении стрелка, блокировки толчков и строк осмотра

### Обвесы: фонарь, глушитель, штык, прицел
Подствольный фонарь (`seclite_attachable`), глушитель (тихие выстрелы), штык (ближний бой оружием), оптический прицел (`scope`, дальний обзор).

- **Код:** [`/datum/component/seclite_attachable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/seclight_attachable.dm#L5), [`/obj/item/suppressor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic.dm#L927), [`proc/install_suppressor()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic.dm#L653), [`/datum/component/bayonet_attachable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/bayonet_attachable.dm#L5), [`/datum/component/scope`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/scope.dm#L5)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/gun_attachments.rs`: крепления по спискам `add_seclight_point`/`add_bayonet_point`/`scope` tg и форка; секлайт на оружие (кнопка действия «Переключить фонарик», свет от владельца или от лежащего оружия; три ствола со стартовым фонарём), глушитель на баллистику с `can_suppress` (тихий `suppressed_sound` с `SILENCED_SOUND_EXTRARANGE`, без сообщения о выстреле, снятие Alt-кликом), боевой нож-штык (в боевом режиме удар ножом, `crates/ss13_game/src/combat.rs`), отвёртка снимает фонарь и штык; прицел — ПКМ вдаль сдвигает обзор на `range_modifier` (новый `CameraEffect::Scope`), шаг/смена руки/оглушение возвращают; без оверлеев креплений, строк осмотра, слежения прицела за курсором и экрана прицела

### Счётчик патронов на HUD (BandaStation)
Модпак форка: при оружии в руке на экране показывается счётчик патронов/заряда и режим огня.

- **Код:** [`/datum/component/gunhud`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/gunhud/code/gunhud_component.dm#L1)
- **BandaStation:** [`modular_bandastation/gunhud/code/gunhud_component.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/gunhud/code/gunhud_component.dm#L1) — весь механизм
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/gunhud.rs` (сервер) и `crates/ss13_game/src/gunhud_client.rs` (клиент), безоконный интерфейс `GunHud` (`crates/ss13_game/src/hud.rs`): у человека с оружием или сваркой в руке в `ui_gunhud` — патроны баллистики тремя цифрами `oth` (с патронником; у револьвера только живые), `no_mag`/`empty_flash`, у энергооружия процент батареи (лайм/жёлтый ≤25%/красный без выстрела) и цена выстрела, у сварки топливо и пламя; показывается оружие активной руки, иначе другой; без индикатора режима огня (переключения режимов нет)

### Оружейные стойки и маяки выбора оружия (BandaStation)
Форк добавляет оружейные стойки для арсенала (с наполненными спавнерами) и маяки выбора табельного оружия для СБ, ГСБ и блюшилда; кобуры с быстрым выхватыванием.

- **Код:** [`/obj/structure/rack/gunrack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/gun_rack.dm#L1), [`/obj/item/choice_beacon/weapon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/ranged/weapon_choice.dm#L1), [`proc/toggle_holster()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/ranged/quik_accessory.dm#L1)
- **BandaStation:** [`modular_bandastation/weapon/code/ranged/weapon_choice.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/ranged/weapon_choice.dm#L1) — маяки выбора оружия
- **Статус в ss13-rs:** ✅ реализовано — маяки СБ, блюшилда и ГСБ (`crates/ss13_game/src/devices/beacons.rs`: списки оружейных кейсов под их названиями форка, доставка подом); оружейная стойка (`crates/ss13_game/src/guns.rs` `gun_rack_offset`/`rack_gun`, `crates/ss13_game/src/tables.rs`): оружие вне боевого режима ставится в столбец клика на 2 пикселя выше и поворачивается на −90°, при подборе выпрямляется; без уменьшения широких (48/64 px) стволов. Кобуры с быстрым выхватыванием (`toggle_holster`) — механизм аксессуаров одежды, не этой группы; маяк в рюкзаке СБ — забота экипировки профессий
