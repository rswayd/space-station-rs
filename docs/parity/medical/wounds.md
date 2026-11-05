# Раны
Система ран tg (`/datum/wound`): тупые (кости), режущие, колющие, ожоги, потеря конечности, трещина черепа, шрамы. В ss13-rs — `crates/ss13_game/src/medical/wounds.rs`.

### Механика ран
Удар с достаточным уроном бросает «кубик ранения» (`check_wounding`) с учётом `wound_bonus`, `exposed_wound_bonus`, брони `wound` и уже имеющихся ран; рана получает степень (умеренная, тяжёлая, критическая), эффекты, описание при осмотре и способы лечения; со временем часть ран заживает сама.

- **Код:** [`/datum/wound`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/_wounds.dm#L19), [`proc/check_wounding()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/bodyparts/wounds.dm#L50), [`proc/apply_wound()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/_wounds.dm#L193), [`proc/handle_process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/_wounds.dm#L566), [`proc/try_treating()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/_wounds.dm#L509), [`/datum/wound_pregen_data`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/_wound_static_data.dm#L6)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/wounds.rs`: `check_wounding` с `wound_bonus`, `exposed_wound_bonus`, `wound_resistance`, штрафами ран и серий, конкуренция по тяжести, замена, `damage_multiplier`, обработка раз в `Life()`; нет брони одежды `wound`, расчленения, статус-эффектов ран, замедления действий, `determination`

### Переломы и вывихи (тупые)
Вывих (умеренная) — вправляется руками или операцией; трещина (тяжёлая) и открытый перелом (критическая) — шины, костный гель с хирургической лентой, операция; мешают действиям рукой и ходьбе, вызывают боль.

- **Код:** [`/datum/wound/blunt/bone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/bones.dm#L15), [`/datum/wound/blunt/bone/moderate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/bones.dm#L230), [`/datum/wound/blunt/bone/severe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/bones.dm#L373), [`/datum/wound/blunt/bone/critical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/bones.dm#L409), [`/datum/wound/bruised`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/bruised.dm#L14)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/wounds.rs`: вывих (костоправ), трещина и открытый перелом (шины марлей, костный гель + хирургическая лента, операции), хромота (средняя), отключение конечности, внутреннее кровотечение груди; нет вправления руками через захват, травм мозга, боли при ударах/стрельбе сломанной рукой

### Порезы (режущие)
Ссадина, рваная рана, отрыв тканей: кровотечение, которое останавливают бинты, швы, прижигание, жгут; поток крови ослабевает со временем.

- **Код:** [`/datum/wound/slash/flesh`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/slash.dm#L40), [`/datum/wound/slash/flesh/moderate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/slash.dm#L308), [`/datum/wound/slash/flesh/severe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/slash.dm#L339), [`/datum/wound/slash/flesh/critical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/slash.dm#L373)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/wounds.rs`: три тяжести с потоком, свёртыванием и понижением, марля впитывает, швы и прижигание; нет вылизывания, лазерного прижигания, удержания раны, брызг крови, ксадона/синтплоти

### Проколы (колющие)
Проколы от пуль и острых предметов: внутреннее кровотечение, лечение швами/операцией; отдельная рана для глаза.

- **Код:** [`/datum/wound/pierce/bleed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/pierce.dm#L30), [`/datum/wound/pierce/bleed/moderate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/pierce.dm#L200), [`/datum/wound/pierce/bleed/severe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/pierce.dm#L270)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/wounds.rs`: три тяжести, внутреннее кровотечение при ударах, марля, прижигание, свёртывание на холоде; нет пулевых вариантов (нет снарядов), прокола глаза

### Ожоги
Ожоги 2–4 степени: инфекция растёт без дезинфекции (мазь, сетка, стерилизин), омертвевшие ткани удаляются санацией; святые и проклятые клейма.

- **Код:** [`/datum/wound/burn/flesh`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/burns.dm#L23), [`/datum/wound/burn/flesh/moderate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/burns.dm#L299), [`/datum/wound/burn/flesh/severe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/burns.dm#L324), [`/datum/wound/burn/flesh/critical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/burns.dm#L354)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/wounds.rs`: 2–4 степени, повреждение плоти и заживление, инфекция с токсинами, отключением и сепсисом, дезинфекция мазью/сеткой/марлей (`// shortcut:` без реагентов-дезинфекторов и УФ-фонаря); нет санации операцией, клейм

### Потеря конечности и трещина черепа
Рана «потеря» сопровождает расчленение; трещина черепа при большом уроне по голове позволяет вынуть мозг без операции.

- **Код:** [`/datum/wound/loss`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/loss.dm#L14), [`/datum/wound/cranial_fissure`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/cranial_fissure.dm#L29)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/wounds.rs`, `crates/ss13_game/src/medical/bodyparts.rs`: рана «потеря» — `HitOutcome::dismember` и `apply_dismember` с сообщениями tg по типу ранения (обычное/«outright»); трещины черепа нет (серия ран, операции подготовки и восстановления черепа, выпадение мозга при поскальзывании, вырывание глаз)

### Шрамы
Зажившие раны оставляют шрамы, видимые при осмотре; могут сохраняться между раундами (persistent scars).

- **Код:** [`_scars.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wounds/scars/_scars.dm#L1), [`proc/generate_fake_scars()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L1069)
- **Статус в ss13-rs:** ❌ нет
