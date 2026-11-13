# Транспорт
Базовая система `/obj/vehicle` и всё, на чём ездят без брони.

### Базовый транспорт
Посадка/высадка, водитель и пассажиры, ключи (`vehicle_key`), действия транспорта на HUD, задержка движения; форк пересчитывает скорость через общий модификатор.

- **Код:** [`/obj/vehicle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/_vehicle.dm#L1), [`/obj/item/key`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/vehicle_key.dm#L1), [`vehicle_actions.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/vehicle_actions.dm#L189)
- **BandaStation:** [`modular_bandastation/balance/code/speed/movespeed_modifier.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/speed/movespeed_modifier.dm#L1), [`car.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/cars/car.dm#L98) — скорость транспорта
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/vehicles.rs`: посадка — пристёгивание (`crate::medical::surgery`: перетащить себя или другого на транспорт, слезть пустой рукой или сопротивлением), водитель (`max_occupants = 1` у ездового), шаги водителя ведут транспорт (`relaymove`), ключи (вставляются предметом, водитель вынимает alt-кликом, подсказка в осмотре), кнопки действий водителя на HUD, задержка BandaStation `modified_move_delay(vehicle_move_delay)` (×√2 по диагонали) и темп на HUD водителя, двери открываются доступом водителя, сообщение о целостности при осмотре; пассажиры и кнопки закрытых машин — в «Закрытые машины» ниже

### Ездовой транспорт (ridden) и компонент езды
Открытый транспорт на компоненте `riding`: водитель управляет, падения при столкновении.

- **Код:** [`/obj/vehicle/ridden`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/ridden.dm#L1)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/vehicles.rs`: компоненты езды по типам (скорость, ключ, `ride_check_flags`, смещение водителя по высоте и слой/смещение транспорта по направлению), проверки `driver_move` (ключ, недееспособность, ноги, руки; с `UNBUCKLE_DISABLED_RIDER` водитель падает и оглушается на 3 с), нужна гравитация, кроме летающих (`override_allow_spacemove`), водитель едет вместе с транспортом (и когда его тянут), повернут по нему; водитель, сдвинутый иначе, слезает. Без смещения водителя по x и без проверок перехода между уровнями; зелье скорости → [../science/xenobiology.md](../science/xenobiology.md)

### Инвалидные кресла
Ручное кресло (скорость зависит от рук), моторизованное на батарее/деталях (сбивает людей), золотое.

- **Код:** [`/obj/vehicle/ridden/wheelchair`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/wheelchair.dm#L1), [`/obj/vehicle/ridden/wheelchair/motorized`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/motorized_wheelchair.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/vehicles.rs`: ручное кресло (задержка `round(RUN_DELAY × 6,2) / руки`), складывается перетаскиванием на себя и раскладывается из руки, золотое; моторизованное: своя батарея (выдаётся при первой поездке), скорость и КПД деталей tier 1, расход `energy_usage × 0,05` за шаг, «нет батареи»/«нет питания» с паузой 2 с, экран при осмотре; нет панели и замены деталей/батареи, тарана (нужна скорость выше 6,7 — детали выше tier 1), звонка и бомбы, разборки ключом, колёс поверх сидящего

### Скутеры, скейтборды, роликовые коньки
Скутер, скейт (трюки, падение при ударе о препятствие), ховерборд, самодельный скейт, «вилиз», ролики, лыжные ботинки.

- **Код:** [`/obj/vehicle/ridden/scooter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/scooter.dm#L1), [`/obj/vehicle/ridden/scooter/skateboard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/scooter.dm#L38), [`/obj/vehicle/ridden/scooter/skateboard/wheelys`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/scooter.dm#L311)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/vehicles.rs`: скутер и скейты (обычный, pro, ховерборд, святой, админский, самодельный): плотные только с ездоком, шагом медленнее на 0,6 и не врезаются; удар о плотное: `instability × 6` выносливости и отскок на клетку, а без сил (или в человека) — полёт на 3 клетки, паралич 8 с, 5 урона мозгу без шлема, сбитый падает на 4 с; кнопки «Олли» (прыжок вперёд, через столы) и «Кикфлип» (с доской в руку); доска поднимается перетаскиванием на себя и ставится из руки с посадкой; нет гринда по столам, падения ховерборда и замедления святого, въезда в мусорку, разборки/сборки скутера и самодельного скейта; ролики и лыжи (`wheelys`) — от обуви, [../items/clothing-gear.md](../items/clothing-gear.md)

### Велосипед, вездеход, сегвей, спидбайк
Велосипед, ATV (в т. ч. с турелью), сегвей охраны, спидбайк.

- **Код:** [`/obj/vehicle/ridden/bicycle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/bicycle.dm#L1), [`/obj/vehicle/ridden/atv`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/atv.dm#L2), [`/obj/vehicle/ridden/secway`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/secway.dm#L2), [`/obj/vehicle/ridden/speedbike`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/speedbike.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/vehicles.rs`: скорость, ключи (ATV, СБ), смещения водителя и слои по DM, спидбайк ездит без гравитации; нет турели ATV, банана в выхлопе сегвея, ремонта сваркой, дыма при поломке и взрыва при разрушении, попаданий пуль по водителю, «жареного» велосипеда, следа спидбайка и оверлеев поверх водителя

### Уборочная тележка уборщика (pimpin' ride)
Ездовая тележка уборщика с ведром и мешком, апгрейд (пылесос).

- **Код:** [`/obj/vehicle/ridden/janicart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/pimpin_ride.dm#L4)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/vehicles.rs`: ключ уборщика, мешок для мусора (вешается, остальное кладётся в него, пустая рука снимает; правый клик — и с водителем), оверлей мешка; нет апгрейдов (полотёр, пылесос)

### Лавовая лодка
Лодка для лавы на Лаваленде (обычная, плазменная, драконья).

- **Код:** [`/obj/vehicle/ridden/lavaboat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/lavaboat.dm#L2)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/vehicles.rs` (`Kind::Lavaboat`, `boat_turf_check`, `complete_bottle`): весло — ключ (прикрепляется кликом, снимается alt-кликом водителя, подсказки в осмотре), ходит только по лаве и воде и может выйти с них на берег (как `turf_check`), задержка 2, слой над мобом; гребец не горит в лаве (`crate::lava`: пристёгнут к несгораемому); плазменная лодка — та же; драконья — без весла, задержка 1, alt-клик за 2 с превращает её в корабль в бутылке, бутылка в руке — снова лодка (звон стекла). Нет разбивания бутылки броском; рецепты весла и лодок из шкур → [../kitchen/recipes-crafting.md](../kitchen/recipes-crafting.md) (крафт)

### Закрытые машины: клоунмобиль, спидвагон, гольф-кар
Закрытый транспорт с пассажирами внутри: клоунмобиль (запихивает людей, кнопки-эффекты, взрыв при емаге), спидвагон, гольф-кар с прицепом.

- **Код:** [`/obj/vehicle/sealed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/sealed.dm#L1), [`/obj/vehicle/sealed/car`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/cars/car.dm#L1), [`/obj/vehicle/sealed/car/clowncar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/cars/clowncar.dm#L1), [`/obj/vehicle/sealed/car/speedwagon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/cars/speedwagon.dm#L2), [`/obj/vehicle/ridden/golfcart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/cars/golfcart/golfcart.dm#L10)
- **Статус в ss13-rs:** ❌ нет
