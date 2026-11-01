# Ксенобиология и цитология

Разведение слаймов, экстракты и зелья, кроссбридинг, консоль ксенобиологии, выращивание клеточных культур в чанах.

Сами мобы слаймов (`/mob/living/basic/slime`, мутации окраса, питание, деление) — см. ../mobs/; реакции экстрактов с реагентами (`/datum/chemical_reaction/slime`) — см. ../chemistry/.

### Слаймовые экстракты
23 цвета экстрактов (серый, золотой, серебряный, металлический, фиолетовый, ..., радужный): реагируют с плазмой/кровью/водой, дают предметы, мобов, взрывы, телепортацию; люминесценты активируют их в себе.

- **Код:** [`/obj/item/slime_extract`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/xenobiology.dm#L3), [`proc/activate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/xenobiology.dm#L60)
- **Статус в ss13-rs:** ❌ нет

### Слаймовые зелья
Зелья из экстрактов: послушание, разум (sentience), перенос сознания, стероид, усилитель, стабилизатор, мутатор, скорость, огнеупорность, смена пола, переименование, радио для слаймов.

- **Код:** [`/obj/item/slimepotion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/xenobiology.dm#L682), [`slimepotion/sentience`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/xenobiology.dm#L735), [`slimepotion/transference`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/xenobiology.dm#L833), [`slimepotion/speed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/xenobiology.dm#L955)
- **Статус в ss13-rs:** ❌ нет

### Блюспейс- и сепиевая плитка
Плитки из экстрактов: ускоряют передвижение / замедляют время на тайле.

- **Код:** [`/obj/item/stack/tile/bluespace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/xenobiology.dm#L1131), [`/obj/item/stack/tile/sepia`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/xenobiology.dm#L1148)
- **Статус в ss13-rs:** ❌ нет

### Консоль ксенобиологии
Камера-консоль с удалённым «глазом»: всасывает и выплёвывает слаймов, кормит обезьянками (переработка трупов обезьян в кубы), применяет зелья, сканирует слаймов; горячие клавиши.

- **Код:** [`/obj/machinery/computer/camera_advanced/xenobio`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/xenobio_camera.dm#L28), [`slime_place()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/xenobio_camera.dm#L214), [`recycle_monkey()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/xenobio_camera.dm#L338)
- **Статус в ss13-rs:** ❌ нет

### Кроссбридинг экстрактов
Экстракт, накормленный 10 экстрактами одного цвета, становится кросс-экстрактом одного из 11 семейств: горящие, заряженные, охлаждающие, поглощающие, промышленные, призматические, повторяющиеся, регенеративные, репродуктивные, самоподдерживающиеся, стабилизированные (постоянный эффект в рюкзаке, 22 вида).

- **Код:** [`/obj/item/slimecross`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/crossbreeding/__corecross.dm#L27), [`stabilized`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/crossbreeding/stabilized.dm#L9), [`regenerative`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/crossbreeding/regenerative.dm#L6), [`burning`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/crossbreeding/burning.dm#L6), [`crossbreeding/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/crossbreeding)
- **BandaStation:** [`regenerative.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/crossbreeding/regenerative.dm#L33) — правка регенеративного экстракта и фикс клонирования
- **Статус в ss13-rs:** ❌ нет

### Чан выращивания (vatgrower)
Чан, питаемый реагентами, выращивает мобов из образцов клеточных линий (34 обычных линии и органы); вирусы в образце мешают росту.

- **Код:** [`/obj/machinery/vatgrower`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/vatgrowing/vatgrower.dm#L2), [`/datum/micro_organism/cell_line`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/vatgrowing/samples/_micro_organism.dm#L13), [`/datum/biological_sample`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/vatgrowing/samples/_sample.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Сбор образцов: мазок, биопсия, чашка Петри
Мазок (swab) берёт пробы с предметов и существ, биопсийный инструмент — с органов/мобов; образцы хранятся в чашках Петри.

- **Код:** [`/obj/item/swab`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/vatgrowing/swab.dm#L2), [`/obj/item/biopsy_tool`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/vatgrowing/biopsy_tool.dm#L2), [`/obj/item/petri_dish`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/vatgrowing/petri_dish.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Микроскоп
Показывает состав образца в чашке Петри (клеточные линии, вирусы, условия роста).

- **Код:** [`/obj/structure/microscope`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/xenobiology/vatgrowing/microscope.dm#L1); tgui: [`Microscope.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Microscope.tsx)
- **Статус в ss13-rs:** ❌ нет
