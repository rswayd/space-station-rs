# Фотография
Фотоаппараты, снимки, альбомы и рамки из `code/modules/photography`.

### Фотоаппарат
Клик по цели делает снимок области (размер настраивается, 1×1–7×7) со вспышкой; расходует плёнку и печатает фото. Снимок — реальный рендер тайлов с мобами и предметами.

- **Код:** [`/obj/item/camera`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/camera.dm#L1), [`capture_image()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/camera.dm#L217), [`adjust_zoom()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/camera.dm#L123), [`printpicture()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/camera.dm#L343)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/photo.rs` (`snap`, `capture`, `print`), окно — `crates/ss13_game/src/service/client.rs` (`PhotoUi`): клик вблизи и издалека снимает квадрат 1×1–7×7 тайлов вокруг цели из видимых пользователю тайлов (BYOND `view()`), alt-click задаёт полуширину/полувысоту 1–4, вспышка (свет 6 тайлов на 0,2 с), звук polaroid, перезарядка 6,4 с, настройка названия/описания/подписи, печать в руки с расходом плёнки, описание мобов (`get_photo_description`: имя или «Неизвестный», ранен ли, что держит). Снимок хранит не пиксели, а спрайты (базовые + оверлеи с направлением, цветом и смещением в порядке отрисовки), клиент рисует их в окне фото. Нет: мульти-z (уровень под open space), освещения/темноты на снимке, повёрнутых (лежачих) спрайтов, монохромной печати (ctrl-click), голодиска, оболочки схем, маленькой картинки на иконке фото, флага чертежей

### Плёнка для фотоаппарата
Картридж плёнки, перезаряжает число снимков фотоаппарата.

- **Код:** [`/obj/item/camera_film`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/film.dm#L4)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/service/photo.rs` (`item_interaction`): картридж заряжает только пустой фотоаппарат до `pictures_max` («не пуст!» иначе)

### Особые фотоаппараты
Призрачная камера (снимает призраков, капеллан), камера детектива с большим запасом плёнки, камеры ИИ и киборга (снимки в память, печать киборгом).

- **Код:** [`/obj/item/camera/spooky`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/other.dm#L1), [`/obj/item/camera/detective`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/other.dm#L27), [`/obj/item/camera/siliconcam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/silicon_camera.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/photo.rs` (`Camera::of`): камера детектива (30 снимков, без вспышки, звук слышит только владелец), camera obscura (`spooky`, `badmin`) снимает призраков (и строку «п-п-п-призрак» в описании). Нет: кражи души у духов (у basic-мобов нет биотипов), камер ИИ/киборга/пИИ (группа silicon), приложения камеры ПК и шпионской камеры-вспышки (не считаются камерами)

### Фотография
Снимок можно рассмотреть (окно с изображением), подписать ручкой, скопировать ксероксом; хранит описание того, кто изображён.

- **Код:** [`/obj/item/photo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/photos/photo.dm#L4), [`show()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/photos/photo.dm#L99), [`/datum/picture`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/_pictures.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/photo.rs` (`show`, `examined`, `PhotoAsk::Scribble`), копия — `crates/ss13_game/src/service/office.rs` (`next_copy`): в руке или осмотр вблизи открывает окно с картинкой и надписью на обороте (издалека — «подойдите ближе»), пустое фото «кажется пустой», ручка пишет на обороте (до 128 символов), ксерокс копирует фото (`PHOTO_TONER_USE`, без надписи). Нет: ч/б копий, переименования (verb), сжигания, бутылочных посланий

### Фотоальбомы
Хранилище фото; именные альбомы глав сохраняют содержимое между раундами.

- **Код:** [`/obj/item/storage/photo_album`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/photos/album.dm#L4), [`/datum/storage/photo_album`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/photos/album.dm#L60)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/storage.rs`: работает только как обычное хранилище по `storage_type`, без ограничения на фото и без персистентности

### Рамки для фото
Настенная рамка показывает вставленное фото; рамки-витрины и портреты сохраняют фото между раундами.

- **Код:** [`/obj/structure/sign/picture_frame`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/photos/frame.dm#L65), [`/obj/item/wallframe/picture`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/photos/frame.dm#L3)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/photo.rs` (`PictureFrame`, `frame_moved`), навешивание — `crates/ss13_game/src/wallmount/frames.rs`: фото вставляется в рамку-предмет и в настенную рамку (оверлей фото), рамка в руке/на стене показывает фото, из рамки в другой руке фото вынимается пустой рукой, кусачки вырезают фото, отвёртка/ключ снимают рамку со стены за 3 с вместе с фото (витрины и портреты не снимаются), рамка-предмет вешается на стену как прочие wallframe и разбирается ключом в доску; портрет показывает одну из 4 картин, пока в нём нет фото. Нет: сохранения фото витрин/портретов между раундами (см. «Персистентность фото»), art-элемента (настроение)

### Персистентность фото
Подсистема persistence хранит фото альбомов и рамок между раундами (JSON и файлы изображений).

- **Код:** [`load_photo_persistence()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/photo_albums.dm#L15)
- **Статус в ss13-rs:** ❌ нет — отложено: SQL/сохранения делаются в самом конце
