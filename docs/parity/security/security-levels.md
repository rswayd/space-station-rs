# Уровни угрозы и ключ-карты
Код станции (зелёный/синий/красный/дельта и форковые гамма/эпсилон) и устройство двойной авторизации глав.

### Подсистема уровня угрозы (SSsecurity_level)
Хранит текущий код станции, меняет его с объявлением (звук, цвет, текст из конфига), рассылает сигнал смены уровня: на него реагируют пожарные тревоги (цвет света), дисплеи статуса, время вызова шаттла, доступ к оружейной и т. д.

- **Код:** [`SUBSYSTEM_DEF(security_level)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/security_level.dm#L1), [`proc/set_level()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/security_level.dm#L50)
- **BandaStation:** [`code/controllers/subsystem/security_level.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/security_level.dm#L72) — `set_level_instantly`, задержка установки уровня (`set_delay`, таймер), передача установившего моба (для гамма-шаттла), сортировка уровней
- **Статус в ss13-rs:** ❌ нет — уровня нет; `crates/ss13_game/src/shuttle.rs` лишь использует фиксированный коэффициент зелёного кода для времени вызова

### Стандартные коды: зелёный, синий, красный, дельта
Зелёный — нет угроз (время вызова шаттла ×2), синий — подозрение на угрозу, красный — явная угроза (короче вызов шаттла, аварийный доступ), дельта — запущено самоуничтожение. У каждого свои объявления, звук, иконка дисплея и цвет пожарной тревоги.

- **Код:** [`/datum/security_level`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/security_level_datums.dm#L9), [`/datum/security_level/green`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/security_level_datums.dm#L53), [`/datum/security_level/blue`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/security_level_datums.dm#L69), [`/datum/security_level/red`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/security_level_datums.dm#L86), [`/datum/security_level/delta`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/security_level_datums.dm#L103)
- **Статус в ss13-rs:** ❌ нет

### Код Гамма (BandaStation)
Устанавливается ЦК при крупной угрозе: свой звук и иконка, оранжевый свет тревог, вызов шаттла как на красном; установившему админу предлагается отправить оружейный шаттл «Гамма».

- **Код:** [`/datum/security_level/gamma`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/security_levels/code/security_level_datums.dm#L24)
- **BandaStation:** [`modular_bandastation/security_levels/code/security_level_datums.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/security_levels/code/security_level_datums.dm#L24) — новый уровень и гамма-шаттл
- **Статус в ss13-rs:** ❌ нет

### Код Эпсилон (BandaStation)
Станция списана ЦК: 15 с задержки с отключением энергии и звуком, ИИ получает законы ЦК, радио силиконов переключается на канал ЦК, вызов шаттла ×10.

- **Код:** [`/datum/security_level/epsilon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/security_levels/code/security_level_datums.dm#L53)
- **BandaStation:** [`modular_bandastation/security_levels/code/security_level_datums.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/security_levels/code/security_level_datums.dm#L53) — новый уровень
- **Статус в ss13-rs:** ❌ нет

### Иконки уровней на дисплеях статуса (BandaStation)
Форк заменяет спрайты дисплеев статуса на свои, включая иконки гаммы и эпсилона.

- **Код:** [`/obj/machinery/status_display`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/security_levels/code/status_display.dm#L1)
- **BandaStation:** [`modular_bandastation/security_levels/code/status_display.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/security_levels/code/status_display.dm#L1) — иконки `status_display.dmi` форка
- **Статус в ss13-rs:** ❌ нет — дисплеи статуса рисуются только как статичные спрайты карты, уровень на них не выводится

### Устройство авторизации ключ-картой (keycard auth)
Настенное устройство глав: один глава выбирает событие, второй подтверждает картой на другом устройстве; события — красный код, аварийный доступ ко всем техтоннелям (и отмена), разблокировка блюспейс-артиллерии, выдача доступа отдела (по карте главы).

- **Код:** [`/obj/machinery/keycard_auth`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/keycard_authentication.dm#L9), [`proc/trigger_event()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/keycard_authentication.dm#L144), [`/proc/make_maint_all_access()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/keycard_authentication.dm#L168), [`/proc/toggle_bluespace_artillery()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/keycard_authentication.dm#L192); tgui: [`KeycardAuth.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/KeycardAuth.jsx)
- **BandaStation:** [`code/modules/security_levels/keycard_authentication.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/security_levels/keycard_authentication.dm#L101) — сравнение должности главы с русским названием (`job_title_ru`) для выдачи доступа
- **Статус в ss13-rs:** ❌ нет
