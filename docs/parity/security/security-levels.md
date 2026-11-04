# Уровни угрозы и ключ-карты
Код станции (зелёный/синий/красный/дельта и форковые гамма/эпсилон) и устройство двойной авторизации глав.

### Подсистема уровня угрозы (SSsecurity_level)
Хранит текущий код станции, меняет его с объявлением (звук, цвет, текст из конфига), рассылает сигнал смены уровня: на него реагируют пожарные тревоги (цвет света), дисплеи статуса, время вызова шаттла, доступ к оружейной и т. д.

- **Код:** [`SUBSYSTEM_DEF(security_level)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/security_level.dm#L1), [`proc/set_level()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/security_level.dm#L50)
- **BandaStation:** [`code/controllers/subsystem/security_level.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/security_level.dm#L72) — `set_level_instantly`, задержка установки уровня (`set_delay`, таймер), передача установившего моба (для гамма-шаттла), сортировка уровней
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/security_level/mod.rs` (`SecurityLevel`, `set_level`: тот же уровень — ничего, отложенная установка с отменой новой, `pre_set`/`post_set`, объявление и звук всем игрокам, затем потребители: шаттл, пожарные тревоги, шлюзы `red_alert_access`, дисплеи статуса); админ-панель «Уровень угрозы» (`ss13_admin`, `AdminRequest::SetSecurityLevel`); ядерная бомба ставит дельту при взводе и возвращает прежний уровень при разминировании. Нет: цветов объявлений (в чате нет цветных разделителей), TTS, `disables_mail` (почтовых посылок нет), синего кода в начале раунда (часть отчёта ЦК, ../comms/), дельты от бомб вне станции (`drop_level`)

### Стандартные коды: зелёный, синий, красный, дельта
Зелёный — нет угроз (время вызова шаттла ×2), синий — подозрение на угрозу, красный — явная угроза (короче вызов шаттла, аварийный доступ), дельта — запущено самоуничтожение. У каждого свои объявления, звук, иконка дисплея и цвет пожарной тревоги.

- **Код:** [`/datum/security_level`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/security_level_datums.dm#L9), [`/datum/security_level/green`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/security_level_datums.dm#L53), [`/datum/security_level/blue`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/security_level_datums.dm#L69), [`/datum/security_level/red`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/security_level_datums.dm#L86), [`/datum/security_level/delta`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/security_level_datums.dm#L103)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/security_level/mod.rs` (`SecLevel`: номера форка, русские названия, тексты повышения/понижения из конфига BandaStation, звуки, иконки дисплеев, цвет пожарной тревоги, `shuttle_call_time_mod`); время вызова, пересчёт при смене уровня и окно отзыва шаттла по уровню — `crates/ss13_game/src/shuttle.rs`; спасательные капсулы (замок до красного) — ../shuttles/ (капсул нет)

### Код Гамма (BandaStation)
Устанавливается ЦК при крупной угрозе: свой звук и иконка, оранжевый свет тревог, вызов шаттла как на красном; установившему админу предлагается отправить оружейный шаттл «Гамма».

- **Код:** [`/datum/security_level/gamma`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/security_levels/code/security_level_datums.dm#L24)
- **BandaStation:** [`modular_bandastation/security_levels/code/security_level_datums.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/security_levels/code/security_level_datums.dm#L24) — новый уровень и гамма-шаттл
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/security_level/mod.rs` (`SecLevel::Gamma`: звук, иконка, оранжевый свет, вызов как на красном; консоль связи не меняет код); ставится из админ-панели. Предложение отправить шаттл Гамма-арсенала — вместе с самим шаттлом, ../shuttles/other-shuttles.md

### Код Эпсилон (BandaStation)
Станция списана ЦК: 15 с задержки с отключением энергии и звуком, ИИ получает законы ЦК, радио силиконов переключается на канал ЦК, вызов шаттла ×10.

- **Код:** [`/datum/security_level/epsilon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/security_levels/code/security_level_datums.dm#L53)
- **BandaStation:** [`modular_bandastation/security_levels/code/security_level_datums.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/security_levels/code/security_level_datums.dm#L53) — новый уровень
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/security_level/mod.rs`: 15 с задержки (новая установка отменяет её), звук отключения и `power_fail` на 15 с, ИИ получает законы «Эпсилон» с нулевым законом ЦК, радио только на канал ЦК (`:y`), синхронизируемые киборги — то же; после установки APC станции перезагружаются; вызов шаттла ×10. «Major emergency» свет ламп — ../power/lighting.md

### Иконки уровней на дисплеях статуса (BandaStation)
Форк заменяет спрайты дисплеев статуса на свои, включая иконки гаммы и эпсилона.

- **Код:** [`/obj/machinery/status_display`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/security_levels/code/status_display.dm#L1)
- **BandaStation:** [`modular_bandastation/security_levels/code/status_display.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/security_levels/code/status_display.dm#L1) — иконки `status_display.dmi` форка
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/security_level/displays.rs`: эвак-дисплеи (`status_display/evac`, иконки `status_display.dmi` форка) при смене уровня 30 с показывают его иконку, затем логотип. Таймер шаттла, сообщения и картинки с консоли связи — ../comms/communications-console.md

### Устройство авторизации ключ-картой (keycard auth)
Настенное устройство глав: один глава выбирает событие, второй подтверждает картой на другом устройстве; события — красный код, аварийный доступ ко всем техтоннелям (и отмена), разблокировка блюспейс-артиллерии, выдача доступа отдела (по карте главы).

- **Код:** [`/obj/machinery/keycard_auth`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/keycard_authentication.dm#L9), [`proc/trigger_event()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/keycard_authentication.dm#L144), [`/proc/make_maint_all_access()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/keycard_authentication.dm#L168), [`/proc/toggle_bluespace_artillery()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/keycard_authentication.dm#L192); tgui: [`KeycardAuth.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/KeycardAuth.jsx)
- **BandaStation:** [`code/modules/security_levels/keycard_authentication.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/keycard_authentication.dm#L101) — сравнение должности главы с русским названием (`job_title_ru`) для выдачи доступа
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/security_level/keycard.rs` (сервер, доступ `keycard_auth` проверяется в момент действия, подтверждать должен другой человек), `crates/ss13_game/src/keycard.rs`, `crates/ss13_game/src/keycard_client.rs` (окно `KeycardAuth`): красный код, аварийный доступ в техтоннели (`make_maint_all_access`, отмена — `revoke_maint_all_access`), блокировка BSA (флаг и объявление; самой BSA нет). Выдача доступа уборщику (ключ уборщика) — ../service/janitorial.md; ИИ и звери устройство не используют
