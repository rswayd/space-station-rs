# Голод, сытость и вкус
Питание мобов: расход сытости, ожирение, HUD голода и система вкусов. Еда, кухня и сами блюда — у агента service; реагенты — см. ../chemistry/.

### Сытость (nutrition) и её расход
Человек начинает с 250–400 сытости и теряет `HUNGER_FACTOR` в секунду (скорость зависит от настроения и `satiety`); каждый шаг тоже тратит немного сытости (вдвое больше при беге). Потребляемые реагенты возвращают сытость при метаболизме.

- **Код:** [`proc/adjust_nutrition()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob.dm#L1500), [`proc/set_nutrition()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob.dm#L1519), [`proc/handle_hunger()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/stomach/_stomach.dm#L137), [`расход при ходьбе`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon_movement.dm#L15)
- **BandaStation:** [`code/modules/mob/mob.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob.dm#L1503) — вампиры не получают сытость от обычной еды
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/hunger.rs`: стартовая сытость, расход `HUNGER_FACTOR`, питание от реагентов; скорость зависит от рассудка (`crate::mood`); нет расхода за шаги, `TRAIT_NOHUNGER` и вампирской правки

### Голодание и эффекты сытости
Ниже порогов — «голоден»/«истощён»: моодлеты, вялый метаболизм (эффективность 0.8); хорошо сытый с высоким `satiety` метаболизирует быстрее; отрицательный `satiety` вызывает дрожь.

- **Код:** [`proc/handle_hunger()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/stomach/_stomach.dm#L137), [`/datum/mood_event/hungry`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/needs_events.dm#L18)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/hunger.rs`: пороги и эффективность метаболизма; нет моодлетов и дрожи

### Переедание и ожирение
Выше `NUTRITION_LEVEL_FULL` копится `overeatduration`; после лимита моб толстеет (`TRAIT_FAT`, замедление 1.5, моодлет), со временем худеет; `TRAIT_NOFAT` защищает. Сильное переполнение желудка может вызвать рвоту.

- **Код:** [`/datum/movespeed_modifier/obesity`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/movespeed/modifiers/mobs.dm#L1), [`/datum/mood_event/fat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/needs_events.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/hunger.rs`: ожирение и замедление; нет моодлета и рвоты от переедания

### Индикатор голода в HUD
Значок голода справа от оповещений показывает голод, сытость и переваривание.

- **Код:** [`/atom/movable/screen/hunger`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/screen_objects/screen_objects.dm#L1162)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/hunger.rs` (`HungerHudUi`)

### Вкус
Съеденное и выпитое даёт сообщение о вкусе по `tastes` еды и реагентов; чувствительность зависит от языка (вида, мутаций); вкус блокируется онемением языка.

- **Код:** [`proc/taste_container()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/taste.dm#L30), [`proc/taste_list()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/taste.dm#L44), [`proc/get_taste_sensitivity()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/taste.dm#L13), [`proc/send_taste_message()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/taste.dm#L75)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/food.rs`: сообщение о вкусе при поедании; нет чувствительности языка и вкуса напитков/реагентов

### Любимая, нелюбимая, токсичная еда и аллергии
Язык вида задаёт любимые/нелюбимые/токсичные типы еды (`foodtypes`): реакция — моодлет, рвота или отравление; квирк аллергии вызывает гистаминовую реакцию.

- **Код:** [`proc/get_food_taste_reaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/taste.dm#L175), [`carbon/get_liked_foodtypes()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/taste.dm#L91), [`proc/get_toxic_foodtypes()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/taste.dm#L126), [`proc/check_allergic_reaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/taste.dm#L157)
- **Статус в ss13-rs:** ❌ нет
