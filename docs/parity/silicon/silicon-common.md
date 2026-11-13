# Общее для синтетиков
Базовый тип `/mob/living/silicon`: речь, сенсоры, урон и общие действия ИИ, боргов и пИИ.

### Базовый синтетик
Общие свойства: иммунитет к дыханию и боли, радио встроенное, камера, журнал событий, фото (`siliconcam`), датчики, список ИИ (`ai_roster`).

- **Код:** [`/mob/living/silicon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/silicon.dm#L1), [`proc/ai_roster()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/silicon.dm#L247), [`proc/logevent()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/silicon.dm#L343)
- **BandaStation:** [`silicon.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/silicon.dm#L46) — VOX-голоса; [`silicon.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/silicon.dm#L104) — убран выбор пола синтетика (всегда средний)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/mod.rs`: ни лёгких, ни желудка, ни реагентов, встроенное радио (`crates/ss13_game/src/radio.rs` `BuiltInRadio`); нет сенсоров, фото, NTOS

### Речь синтетиков и бинарный канал
Синтетики говорят по встроенному радио и в бинарном канале (`:b`), который слышат только ИИ, борги и обладатели бинарного ключа.

- **Код:** [`silicon_say.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/silicon_say.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `:b`/`:и` (сеймод binary) у ИИ и киборгов: «Робоболтовня» всем синтетикам и призракам (`crates/ss13_game/src/silicon/mod.rs`, `Span::BinarySay`), радио ИИ и киборга через встроенный передатчик; речь ИИ слышна у глаза, а не у ядра; глагол «констатирует» не перенесён

### Сенсорные HUD синтетиков
Переключение медицинского/охранного/диагностического HUD.

- **Код:** [`proc/toggle_sensors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/silicon.dm#L291)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/records/hud.rs` (`SensorsOff`, `ai_sensors`, `basic_health_visible`): сенсоры включены с рождения; у киборга полные медицинский и охранный HUD (и осмотр с записями), у ИИ через глаз — меньшие: только значок должности и здоровье лишь у людей с датчиками костюма в режиме слежения (`TRAIT_BASIC_HEALTH_HUD_VISIBLE`); переключение `toggle_sensors` — кнопка «Sensor Augmentation» ИИ и «Sensor Overlay» в RoboTact киборга (`crates/ss13_game/src/silicon/ai_hud.rs`, `crates/ss13_game/src/modpc/programs/robotact.rs`), не работает без сознания. Диагностический HUD и путь ботов — нет самих HUD (items/clothing-gear.md «HUD-очки»); у пИИ HUD покупаются (pai.md)

### Урон, EMP и флеш у синтетиков
Синтетики не получают токсинов и удушья; EMP наносит урон и оглушает, флеш ослепляет киборгов, электрошок.

- **Код:** [`silicon_defense.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/silicon_defense.dm#L2), [`damage_procs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/damage_procs.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — только brute/burn, oxy/tox обнуляются, без крита, смерть на −maxHealth (`crates/ss13_game/src/silicon/mod.rs`); ЭМИ (`crates/ss13_game/src/emp/mobs.rs`): «Предупреждение», ожог 20/10, «*БЗЗЗТ*», статика (`flash_act`); нет сбрасывания наездников, флеш руками не действует

### Автоответ и озвучка законов
Автосообщения (`set_autosay`) и проверка/озвучивание законов в канал.

- **Код:** [`proc/set_autosay()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/silicon.dm#L252), [`proc/checklaws()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/silicon.dm#L244)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/silicon/state_laws.rs`: окно `StateLawUi` (`checklaws`: кнопка «State Laws» ИИ и пИИ, RoboTact киборга) — все законы с флажками, базовые отмечены при создании и снова при каждом закрытии; «State Laws» говорит «Текущие активные законы:» и отмеченные законы по одному в секунду (с копии законов, нумерация как в списке) с префиксом `radiomod`, затем окно заблокировано 3 с; `set_autosay` (верб ИИ «Set Auto Announce Mode» во вкладке «Команды ИИ», `lawchannel` RoboTact): список Default (`;`), None и каналов встроенного радио → `:ключ`; у ИИ по умолчанию `;`. Строки идут через речь игрока: синтетик без игрока законы не озвучивает, глагол «says», не «states»; объявления о прибытии (`radiomod` у tg) — у машины-оповещателя, не у ИИ. Принудительное озвучивание (`statelaws(force)` от Voice of God, админ-панель законов) — у чужих групп (магия голоса, admin)
