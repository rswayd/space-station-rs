# Визуал и доставка ассетов
Подсистемы отрисовки (свет, параллакс, оверлеи, сглаживание) и доставка ресурсов клиенту.

_«Освещение» описано в [../effects/lighting.md](../effects/lighting.md)._

### Параллакс космоса
`SSparallax`: слои звёзд, смещающиеся при движении; случайный слой раунда (планета, астероиды, газ), движение при полёте шаттла; настройка качества.

- **Код:** [`SUBSYSTEM_DEF(parallax)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/parallax.dm#L4), [`code/_onclick/hud/parallax`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/parallax)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_render/src/parallax.rs`: три слоя звёзд; нет случайного слоя раунда и прокрутки при полёте шаттла

### Оверлеи и vis_overlays
`SSoverlays`/`SSvis_overlays`: составные спрайты (одежда, повреждения, эффекты), кэш оверлеев.

- **Код:** [`SUBSYSTEM_DEF(overlays)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/overlays.dm#L1), [`SUBSYSTEM_DEF(vis_overlays)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/vis_overlays.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_render/src/sprites.rs` (дочерние спрайты для оверлеев из `Appearance`): нет vis_contents и эмиссивов

### Сглаживание иконок
`SSicon_smooth`: стены, окна, ковры соединяются с соседями (битмаски junction), пересглаживание при постройке/разрушении.

- **Код:** [`SUBSYSTEM_DEF(icon_smooth)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/icon_smooth.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/look.rs` (сглаживание и кромки окон BandaStation считаются при конвертации карты): пересглаживания во время игры нет

### Плоскости и мастер-плоскости
Рендер по плоскостям (мир, свет, эмиссив, игровой мир, HUD, runechat), мастер-плоскости с фильтрами, мульти-Z.

- **Код:** [`code/_onclick/hud/rendering`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/rendering)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_render/src/order.rs` (порядок по plane/layer): нет фильтров мастер-плоскостей и мульти-Z

### Доставка ассетов
`SSassets`/`SSasset_loading`/`SSearly_assets`: спрайтшиты и файлы для TGUI и чата, отправка через browse_rsc или CDN (webroot), кэш; greyscale-генерация иконок (`SSgreyscale`).

- **Код:** [`SUBSYSTEM_DEF(assets)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/assets.dm#L1), [`/datum/asset_transport`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/asset_cache/transports/asset_transport.dm#L3), [`/datum/asset_transport/webroot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/asset_cache/transports/webroot_transport.dm#L2), [`code/modules/asset_cache`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/asset_cache), [`PROCESSING_SUBSYSTEM_DEF(greyscale)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/processing/greyscale.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_assets`, `crates/ss13_mapload` (`ss13-mapload convert` готовит атласы офлайн, клиент грузит их локально): нет доставки с сервера/CDN и рантайм-greyscale

### Наведение мыши
`SSmouse_entered`: обработка наведения (подсказки, screentips) батчами.

- **Код:** [`SUBSYSTEM_DEF(mouse_entered)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/mouse_entered.dm#L2)
- **Статус в ss13-rs:** ❌ нет
