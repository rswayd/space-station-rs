# Синтетики

Синтетики оригинального сервера: ИИ станции (ядро, камеры, VOX, оболочки), законы и стойки модулей законов (включая 26 лоусетов BandaStation и запрос смены законов у ЦК), киборги с моделями, модулями и апгрейдами, персональный ИИ, боты (клинбот, медбот, секбот, МУЛ и др.) и дроны техобслуживания. Источники: `code/modules/mob/living/silicon`, `code/modules/pai`, `code/datums/ai_laws`, `code/game/objects/items/AI_modules`, `code/game/objects/items/robot`, `code/game/machinery/ai_core`, `code/modules/mob/living/basic/bots`, `code/modules/mob/living/simple_animal/bot`, `code/modules/mob/living/basic/drone`; форк: `modular_bandastation/{borgs,ai_laws,ai_law_change,ai_vox}`. В ss13-rs ничего из этого нет: работы «AI» и «Cyborg» существуют в данных профессий, но спавнят человека.

| Файл | Что внутри |
| --- | --- |
| [ai.md](ai.md) | ИИ: моб, камеры, питание, управление машинами, VOX, голопады, ядро, интеллКарта |
| [laws.md](laws.md) | Законы, лоусеты, стойки и модули законов, запрос смены законов |
| [cyborgs.md](cyborgs.md) | Киборги: модели, модули, апгрейды, сборка, консоль, зарядка |
| [pai.md](pai.md) | Персональный ИИ: карта, голошасси, ПО |
| [bots.md](bots.md) | Боты: база, сборка, все виды |
| [drones.md](drones.md) | Дроны техобслуживания |
| [silicon-common.md](silicon-common.md) | Общее для синтетиков: речь, бинарный канал, сенсоры, урон |
