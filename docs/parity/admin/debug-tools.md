# Отладка и разработка
View Variables, вызов процедур, SDQL2 и Lua, отладочные и маппинг-вербы, профилирование.

### View Variables
Окно переменных любого датума: правка значений (вкл. массовую), списки, отметка датума, теги, удаление, отслеживание ссылок, редакторы цветовых матриц, фильтров и частиц.

- **Код:** [`/client/proc/debug_variables()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/view_variables/view_variables.dm#L9), [`modify_variables.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/view_variables/modify_variables.dm#L1), [`mass_edit_variables.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/view_variables/mass_edit_variables.dm#L1), [`reference_tracking.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/view_variables/reference_tracking.dm#L1); tgui: [`Filterrific.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Filterrific.jsx), [`ParticleEdit.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ParticleEdit/index.tsx)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_admin/src/vv.rs`, вкладка «Переменные» админ-панели (`R_DEBUG`): объекты своего тайла, все компоненты сущности и верхнеуровневые поля отражённых (каждый компонент прототипов регистрируется с `ReflectComponent`), правка булевых, целых, дробных и строковых полей (`R_VAREDIT`, с проверкой типа и диапазона, в админ-лог); цель — `NetId`, видимый клиенту, права проверяются на каждый запрос. Не перенесено: массовая правка, списки и вложенные поля, отметка/теги датума, удаление, отслеживание ссылок, редакторы цветовых матриц, фильтров и частиц

### Вызов процедур
Advanced ProcCall / Atom ProcCall с аргументами, глобальные вызовы, защита от опасных процедур.

- **Код:** [`code/modules/admin/callproc`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/callproc)
- **Статус в ss13-rs:** ❌ нет — нет DM-рантайма: в Rust нет вызова процедур по имени; аналог потребует реестра админ-команд

### SDQL2
SQL-подобные запросы к объектам мира (SELECT/UPDATE/CALL/DELETE с фильтрами).

- **Код:** [`/datum/sdql_parser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/SDQL2/SDQL_2_parser.dm#L68), [`SDQL_2.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/SDQL2/SDQL_2.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет DM-рантайма и языка запросов; рефлекшн компонентов уже есть (см. View Variables)

### Lua-скрипты (auxlua)
Редактор и состояния Lua для скриптования событий админами.

- **Код:** [`/datum/lua_state`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/lua/lua_state.dm#L10), [`/datum/lua_editor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/lua/lua_editor.dm#L2); tgui: [`LuaEditor.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/LuaEditor/index.tsx)
- **Статус в ss13-rs:** ❌ нет — нет DM-рантайма (auxlua встраивается в BYOND)

### Отладочные вербы
Debug-Game, счётчики атомов/датумов, Del-All/Hard-Del-All, логи del()/Initialize/overlay, MC-зависимости, таймеры, отладка HUD/плоскостей/света/FOV/путей/спрайтов, очистка кэша ассетов, тест дальтонизма, полный доступ.

- **Код:** [`debug.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/debug.dm#L1), [`plane_debugger.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/plane_debugger.dm#L1), [`light_debug.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/light_debug.dm#L1), [`path_debugger.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/path_debugger.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_debug` (отладочный UI разработчика, см. `docs/RUNNING.md` «Debug UI»): не внутриигровые админ-вербы

### Атмосферная отладка
Панель SSair (`Atmos Control Panel`: группы возбуждённых тайлов и их показ `display_turfs`, заморозка подсистемы), `Debug Air Status`, проверка трубных сетей (`Check Plumbing`), выпуск газа в тайл. Перенесено из ../atmos/linda.md.

- **Код:** [`atmos_control`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/admin_verbs.dm#L322), [`/datum/excited_group/proc/display_turfs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_turf_tile.dm#L545), [`debug_air_status`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/diagnostics.dm#L3), [`atmos_debug`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/atmosdebug.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin/src/verbs.rs` (вкладка «Игра», `R_DEBUG`): Debug Air Status (показания анализатора своего тайла) со счётчиками SSair (активные тайлы, возбуждённые группы, очаги), выпуск газа строкой tg (`atmos_spawn_air`) в свой тайл; нет показа тайлов возбуждённых групп, заморозки SSair, Check Plumbing

### Маппинг-вербы
Отчёты карты, проверка зон, атмоса, питания, водопровода, диапазоны камер/интеркомов, счётчики объектов, загрузка шаблонов и руин, экспорт карты, дебаг z-уровней.

- **Код:** [`mapping.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/mapping.dm#L1), [`map_template_loadverb.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/map_template_loadverb.dm#L1), [`map_export.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/map_export.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload` (проверки карт без окна, см. `docs/RUNNING.md` «Map checks»): офлайн-инструмент, не верб

### Профилирование
Line profiling (старт/стоп/просмотр), профайлер BYOND, Tracy.

- **Код:** [`Start Line Profiling`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/debug.dm#L728)
- **Статус в ss13-rs:** ❌ нет — нет DM-рантайма: профайлер BYOND неприменим, админ-верба для Rust-профилирования нет

### Автомат подтипов (subtype vendor)
Админский автомат, бесплатно выдающий все подтипы заданного типа; тип меняется правым кликом (только с правами R_SERVER/R_DEBUG). Общая механика автомата — ../cargo/vending.md.

- **Код:** [`/obj/machinery/vending/subtype_vendor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/subtype.dm#L2)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_admin/src/vv.rs` `subtypes`, кнопка «Подтипы» во вкладке «Игра» (`R_SPAWN`): прототип и все `/obj`-прототипы, наследующие его (по DM-пути или цепочке родителей), выдаются по клику под собой через `Spawn`. Отклонение: админ-окно вместо машины-автомата на карте (смена типа правым кликом), до 300 строк
