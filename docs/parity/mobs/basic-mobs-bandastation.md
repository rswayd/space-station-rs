# Мобы форка BandaStation
Существа, добавленные модпаком `modular_bandastation/mobs` («Кастомные мобы»). Правки апстримных мобов указаны в строках **BandaStation** соответствующих записей других файлов; слизнекот и борги в модпаке закомментированы (не компилируются). Поставки животных в карго (`supplypacks/livestock_pack_animals.dm`) — см. каталог карго. В ss13-rs ничего из этого нет.

### Свиньи по возрастам
Свиньи растут по стадиям: поросёнок → подсвинок → свинка → свинья → свин → хряк → прахряк; размер и мясо зависят от возраста; крупные свиньи со своим ИИ и фразами; именные свиньи.

- **Код:** [`/mob/living/basic/pig`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/pig.dm#L2)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/friendly/pigs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/pigs.dm#L1), [`modular_bandastation/mobs/code/simple_animal/friendly/pigs_named.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/pigs_named.dm#L12), [`ai_controller.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/ai_controller.dm#L123)
- **Статус в ss13-rs:** ❌ нет

### Хомяки
Хомяк и хомячок (детёныш, вырастает), мышиный подтип с собственными спрайтами; хомяк-«представитель».

- **Код:** [`/mob/living/basic/mouse`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/vermin/mouse.dm#L1)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/friendly/hamster.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/hamster.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Крупные ящеры и крокодилы
Враждебные игуана, аллигатор и крокодил (`lizard/big`) со своим ИИ; отдельный крокодил, на котором можно ездить (`riding/creature/crocodile`), ест мясо.

- **Код:** [`/mob/living/basic/lizard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/vermin/lizard.dm#L7)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/hostile/lizard.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/hostile/lizard.dm#L1), [`modular_bandastation/mobs/code/simple_animal/hostile/crocodile.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/hostile/crocodile.dm#L2), [`ai_controller.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/ai_controller.dm#L77)
- **Статус в ss13-rs:** ❌ нет

### Синдикрысы
Крысы на службе Синдиката (и реактивная «нарушитель бесполётных зон»), свой ИИ и фразы.

- **Код:** [`/mob/living/basic/mouse/rat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/vermin/mouse.dm#L322)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/hostile/syndi_rat.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/hostile/syndi_rat.dm#L1), [`ai_controller.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/ai_controller.dm#L115)
- **Статус в ss13-rs:** ❌ нет

### Опоссумы
Опоссум со своим контроллером ИИ и фразами.

- **Код:** [`/datum/ai_controller/basic_controller`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/basic_mobs/base_basic_controller.dm#L1)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/friendly/possum.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/possum.dm#L1), [`ai_controller.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/ai_controller.dm#L49)
- **Статус в ss13-rs:** ❌ нет

### Именные животные отделов
Уникальные питомцы на картах форка: коза шефа, корова Пипа, куры и петух Клаки, гусь-учёный, крокодил Гена, крысы Рататуй/Реми/Брейн, мышь, хомяк-представитель.

- **Код:** [`/mob/living/basic/pet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/pet.dm#L1)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/named_animals.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/named_animals.dm#L1)
- **Статус в ss13-rs:** ❌ нет
