# Мобы

Мобы оригинального сервера, кроме силиконов, ботов и лобби: тик `Life()`, урон, крит и смерть, дыхание и температура тела, голод, передвижение и модификаторы скорости, инвентарь и раздевание, волочение и броски, осмотр и эмоции, призраки, виды, ксеноморфы и мозг-моб, статус-эффекты, настроение, квирки, навыки, личности, травмы мозга, галлюцинации, разум, а также все basic- и simple-мобы с их ИИ. Источники: `code/modules/mob` (без `living/silicon`, ботов и `dead/new_player`), `code/modules/movespeed`, `code/modules/actionspeed`, `code/modules/hallucination`, `code/datums/{ai,status_effects,mood*,quirks,skills,brain_damage,mind,storage}`, подсистемы `mobs`, `ai_controllers`, `npcpool`; форк — `modular_bandastation/{species,mobs,status_effects,quirks,movespeed_limit,pixel_shift,skills,examine_panel,ssd_indicator,emote_panel}`. В ss13-rs есть только человек: урон по частям тела, крит и смерть, дыхание, температура, голод, инвентарь и хранилища, волочение и агрессивный захват, броски, осмотр, призраки, базовые статус-эффекты и стамина (`crates/ss13_game/src/{health,breath,internals,hunger,inventory,storage,pull,throw,examine,ghost,status_effects}.rs`); basic-мобы (карпы, мыши, тараканы, питомцы станции) с ИИ на деревьях поведения — `crates/ss13_game/src/mobs/`; других видов, настроения, квирков и травм нет.

| Файл | Что внутри |
| --- | --- |
| [life-and-health.md](life-and-health.md) | SSmobs и `Life()`, типы урона, лечение, состояния и крит, HUD урона, смерть, гиббинг, хаск, гниение, электрошок, взрывы, падение, гравитация, отдых, resist, рвота, превращения |
| [breathing-temperature.md](breathing-temperature.md) | цикл дыхания, кислород, вредные и экзотические газы, internals, защита от дыма, давление, температура тела |
| [hunger-taste.md](hunger-taste.md) | сытость и её расход, голодание, ожирение, HUD голода, вкус, предпочтения в еде |
| [movement.md](movement.md) | шаг моба, ходьба/бег, модификаторы скорости движения и действий, спотыкание форка, невесомость, подскальзывание, z-уровни, вентиляция, навигация, FOV, шаги, пиксельный сдвиг, езда |
| [inventory-stripping.md](inventory-stripping.md) | руки, слоты экипировки, задержка надевания, быстрая экипировка, скрытые слоты, `/datum/storage`, меню раздевания, передача, двуручность, мобы в руках |
| [pull-grab-throw.md](pull-grab-throw.md) | волочение, захваты, удушение, смещения, переноска, режим броска, полёт, ловля, брошенный моб |
| [examine-emotes.md](examine-emotes.md) | осмотр, examine_more, осмотр с HUD, панель осмотра и SSD форка, эмоции, панель эмоций |
| [ghosts.md](ghosts.md) | призрак, возврат и DNR, Orbit, Observe, зрение призрака, облик, меню и спавнеры, deadchat, респаун, mob/eye |
| [species-core.md](species-core.md) | датум вида, смена вида, выбор видов, внешность, видовые атаки, языки форка |
| [species-list.md](species-list.md) | все виды по одному, включая скреллов, таяр и вульпканинов форка |
| [aliens-brains.md](aliens-brains.md) | ксеноморфы-карбоны, касты и способности, мозг-моб и процессоры мозга |
| [status-effects-incapacitating.md](status-effects-incapacitating.md) | каркас эффектов, stun/knockdown/paralyze/immobilize, сон, стамина, стамкрит, конфузия, daze/stagger, стазис |
| [status-effects-debuffs.md](status-effects-debuffs.md) | дрожь, головокружение, опьянение, речевые и зрительные дебаффы, огненные/мокрые стаки, приступы и прочие дебаффы |
| [status-effects-buffs.md](status-effects-buffs.md) | баффы и нейтральные эффекты |
| [mood.md](mood.md) | настроение, рассудок и события настроения |
| [quirks.md](quirks.md) | система квирков и группы положительных, нейтральных, отрицательных |
| [skills.md](skills.md) | навыки, скиллчипы и станция навыков, навыки форка |
| [personality.md](personality.md) | личности персонажа |
| [brain-traumas.md](brain-traumas.md) | лёгкие, тяжёлые и особые травмы мозга, фобии, раздвоение личности |
| [hallucinations.md](hallucinations.md) | система и виды галлюцинаций |
| [mind.md](mind.md) | разум: перенос, роли, память (без антаг-датумов) |
| [basic-mobs-core.md](basic-mobs-core.md) | база basic-мобов: атаки, атмосфера, еда и лут, приручение, команды, езда, рой |
| [basic-mobs-farm-pets.md](basic-mobs-farm-pets.md) | скот, пчёлы, гориллы, питомцы |
| [basic-mobs-vermin.md](basic-mobs-vermin.md) | мыши, крысы, тараканы и прочая мелкая фауна |
| [basic-mobs-space-fauna.md](basic-mobs-space-fauna.md) | карпы, пауки, драконы, ревенант, морф, демоны и др. |
| [basic-mobs-lavaland-icemoon.md](basic-mobs-lavaland-icemoon.md) | фауна Лаваленда, Ледяной луны и джунглей, майнботы |
| [basic-mobs-misc.md](basic-mobs-misc.md) | NPC-ксеноморфы, миньоны блоба, конструкты, стражи, слаймы, дроны, бойцы-NPC |
| [basic-mobs-bandastation.md](basic-mobs-bandastation.md) | мобы, добавленные форком |
| [simple-animals.md](simple-animals.md) | simple_animal, hostile, мегафауна, элиты |
| [mob-ai.md](mob-ai.md) | ИИ-контроллеры, деревья поведения, цели, движение, подсистемы ИИ |
