# События (events)

Случайные события раунда BandaStation (tg `code/modules/events`, подсистема `SSevents`): планировщик с весами и ограничениями, админский запуск (Force Event), инженерные сбои, космические угрозы, аномалии, события экипажа и бюрократии, мидраунд-вторжения, пул событий мага и праздничные события. Планировщик и события, работающие с портированными системами, — `crates/ss13_game/src/events/` (после ветки day/events); у остальных записей в строке статуса указано, каких механик не хватает. Сами аномалии, болезни, антагонисты и погода как механики описаны в своих разделах (аномалии — наука, погода — ../effects/).

| Файл | Что внутри |
| --- | --- |
| [framework.md](framework.md) | SSevents, жизненный цикл события, Force Event, переключение событий, режим мага, праздники |
| [station-events.md](station-events.md) | Grid Check, Electrical Storm, сбои связи, камеры, Grey Tide, гравитация, радиация, суперматерия, трамвай, землетрясение, Brand Intelligence, Ion Storm, засоры вентиляции и скрубберов |
| [space-events.md](space-events.md) | Волны метеоров, пыль, песчаная буря, неподвижный стержень, кротовины, карпы, мыши, лоза, порталы, Aurora Caelus |
| [anomaly-events.md](anomaly-events.md) | События аномалий (flux, bluespace, grav, pyro, vortex, bioscrambler, dimensional, ectoplasm, hallucination, weather) |
| [crew-events.md](crew-events.md) | Болезни и травмы, фальшивые тревоги, бюрократия, грузы и шаттлы, мирные события, Lone Operative, Dynamic Tweak, bitrunning glitch |
| [wizard-holiday-events.md](wizard-holiday-events.md) | Пул событий мага (Summon Events) и праздничные события |
