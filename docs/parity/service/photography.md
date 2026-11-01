# Фотография
Фотоаппараты, снимки, альбомы и рамки из `code/modules/photography`.

### Фотоаппарат
Клик по цели делает снимок области (размер настраивается, 1×1–7×7) со вспышкой; расходует плёнку и печатает фото. Снимок — реальный рендер тайлов с мобами и предметами.

- **Код:** [`/obj/item/camera`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/camera.dm#L1), [`capture_image()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/camera.dm#L217), [`adjust_zoom()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/camera.dm#L123), [`printpicture()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/camera.dm#L343)
- **Статус в ss13-rs:** ❌ нет

### Плёнка для фотоаппарата
Картридж плёнки, перезаряжает число снимков фотоаппарата.

- **Код:** [`/obj/item/camera_film`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/film.dm#L4)
- **Статус в ss13-rs:** ❌ нет

### Особые фотоаппараты
Призрачная камера (снимает призраков, капеллан), камера детектива с большим запасом плёнки, камеры ИИ и киборга (снимки в память, печать киборгом).

- **Код:** [`/obj/item/camera/spooky`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/other.dm#L1), [`/obj/item/camera/detective`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/other.dm#L27), [`/obj/item/camera/siliconcam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/camera/silicon_camera.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Фотография
Снимок можно рассмотреть (окно с изображением), подписать ручкой, скопировать ксероксом; хранит описание того, кто изображён.

- **Код:** [`/obj/item/photo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/photos/photo.dm#L4), [`show()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/photos/photo.dm#L99), [`/datum/picture`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/_pictures.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Фотоальбомы
Хранилище фото; именные альбомы глав сохраняют содержимое между раундами.

- **Код:** [`/obj/item/storage/photo_album`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/photos/album.dm#L4), [`/datum/storage/photo_album`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/photos/album.dm#L60)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/storage.rs`: работает только как обычное хранилище по `storage_type`, без ограничения на фото и без персистентности

### Рамки для фото
Настенная рамка показывает вставленное фото; рамки-витрины и портреты сохраняют фото между раундами.

- **Код:** [`/obj/structure/sign/picture_frame`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/photos/frame.dm#L65), [`/obj/item/wallframe/picture`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/photography/photos/frame.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Персистентность фото
Подсистема persistence хранит фото альбомов и рамок между раундами (JSON и файлы изображений).

- **Код:** [`load_photo_persistence()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/photo_albums.dm#L15)
- **Статус в ss13-rs:** ❌ нет
