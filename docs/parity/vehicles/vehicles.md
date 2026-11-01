# Транспорт
Базовая система `/obj/vehicle` и всё, на чём ездят без брони.

### Базовый транспорт
Посадка/высадка, водитель и пассажиры, ключи (`vehicle_key`), действия транспорта на HUD, задержка движения; форк пересчитывает скорость через общий модификатор.

- **Код:** [`/obj/vehicle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/_vehicle.dm#L1), [`/obj/item/key`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/vehicle_key.dm#L1), [`vehicle_actions.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/vehicle_actions.dm#L189)
- **BandaStation:** [`modular_bandastation/balance/code/speed/movespeed_modifier.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/speed/movespeed_modifier.dm#L1), [`car.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/cars/car.dm#L98) — скорость транспорта
- **Статус в ss13-rs:** ❌ нет — нет ни транспорта, ни пристёгивания (`buckle`), см. shortcut в `crates/ss13_game/src/server.rs`

### Ездовой транспорт (ridden) и компонент езды
Открытый транспорт на компоненте `riding`: водитель управляет, падения при столкновении.

- **Код:** [`/obj/vehicle/ridden`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/ridden.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Инвалидные кресла
Ручное кресло (скорость зависит от рук), моторизованное на батарее/деталях (сбивает людей), золотое.

- **Код:** [`/obj/vehicle/ridden/wheelchair`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/wheelchair.dm#L1), [`/obj/vehicle/ridden/wheelchair/motorized`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/motorized_wheelchair.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Скутеры, скейтборды, роликовые коньки
Скутер, скейт (трюки, падение при ударе о препятствие), ховерборд, самодельный скейт, «вилиз», ролики, лыжные ботинки.

- **Код:** [`/obj/vehicle/ridden/scooter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/scooter.dm#L1), [`/obj/vehicle/ridden/scooter/skateboard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/scooter.dm#L38), [`/obj/vehicle/ridden/scooter/skateboard/wheelys`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/scooter.dm#L311)
- **Статус в ss13-rs:** ❌ нет

### Велосипед, вездеход, сегвей, спидбайк
Велосипед, ATV (в т. ч. с турелью), сегвей охраны, спидбайк.

- **Код:** [`/obj/vehicle/ridden/bicycle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/bicycle.dm#L1), [`/obj/vehicle/ridden/atv`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/atv.dm#L2), [`/obj/vehicle/ridden/secway`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/secway.dm#L2), [`/obj/vehicle/ridden/speedbike`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/speedbike.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Уборочная тележка уборщика (pimpin' ride)
Ездовая тележка уборщика с ведром и мешком, апгрейд (пылесос).

- **Код:** [`/obj/vehicle/ridden/janicart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/pimpin_ride.dm#L4)
- **Статус в ss13-rs:** ❌ нет

### Лавовая лодка
Лодка для лавы на Лаваленде (обычная, плазменная, драконья).

- **Код:** [`/obj/vehicle/ridden/lavaboat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/lavaboat.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Закрытые машины: клоунмобиль, спидвагон, гольф-кар
Закрытый транспорт с пассажирами внутри: клоунмобиль (запихивает людей, кнопки-эффекты, взрыв при емаге), спидвагон, гольф-кар с прицепом.

- **Код:** [`/obj/vehicle/sealed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/sealed.dm#L1), [`/obj/vehicle/sealed/car`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/cars/car.dm#L1), [`/obj/vehicle/sealed/car/clowncar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/cars/clowncar.dm#L1), [`/obj/vehicle/sealed/car/speedwagon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/cars/speedwagon.dm#L2), [`/obj/vehicle/ridden/golfcart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/cars/golfcart/golfcart.dm#L10)
- **Статус в ss13-rs:** ❌ нет
