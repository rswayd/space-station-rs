# Ход раунда

Всё, что определяет жизненный цикл раунда: тикер и лобби, профессии и их распределение, аутфиты, станционные трейты и цели, голосования и смена экипажа, криосон, праздники и достижения, призрачные роли и мини-игры. Включает модпаки BandaStation `title_screen`, `jobs`, `job_restrictions`, `job_slots_preferences`, `ru_jobs`, `outfits`, `automatic_crew_transfer`, `cryosleep`. В ss13-rs реализованы лобби, состояния тикера, распределение профессий с приоритетами BandaStation, латджойн, аутфиты и перезагрузка (`crates/ss13_game/src/{round,lobby,jobs,outfit}.rs`).

| Файл | Что внутри |
| --- | --- |
| [ticker.md](ticker.md) | Состояния игры, подготовка раунда, итоговый отчёт, перезагрузка, музыка, очередь |
| [lobby.md](lobby.md) | HTML-лобби BandaStation, готовность, наблюдение, латджойн, манифест, Discord, админ-кнопки |
| [jobs-system.md](jobs-system.md) | SSjob: распределение, приоритеты укомплектования, офицеры, требования, ограничения, русские названия |
| [jobs-command-security.md](jobs-command-security.md) | Профессии командования, СБ и юстиции (включая профессии BandaStation) |
| [jobs-technical.md](jobs-technical.md) | Профессии инженерии, медицины, науки и карго |
| [jobs-service-silicon.md](jobs-service-silicon.md) | Профессии сервиса, ассистенты, ИИ и киборги |
| [outfits.md](outfits.md) | Аутфиты: экипировка, профессии, органы, фракции BandaStation, менеджер |
| [station-traits-goals.md](station-traits-goals.md) | Станционные трейты и цели станции |
| [votes-transfer-cryo.md](votes-transfer-cryo.md) | Голосования, автоматическая смена экипажа, криосон |
| [holidays-achievements.md](holidays-achievements.md) | Праздники и достижения |
| [ghost-roles.md](ghost-roles.md) | Призрачные роли, спавнеры и трупы |
| [minigames.md](minigames.md) | Мафия, CTF, дезматч, баскетбол, потерянный экипаж, обучение |
