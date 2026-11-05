# Бой

Ближний и дальний бой оригинального сервера: цепочка атаки по клику, режим боя, удары руками и оружием, толчки и таклы, броня, весь огнестрел (баллистика, энергооружие, особое), боеприпасы и снаряды, бойки, холодное оружие и щиты, встраивание и расчленение, боевые искусства. Источники: `code/_onclick` (click, item_attack), `code/datums/armor`, `code/modules/projectiles`, `code/game/objects/items/weaponry`, `code/datums/martial`, `code/datums/embedding.dm`, `code/modules/surgery/bodyparts/dismemberment.dm`; форк — `modular_bandastation/weapon`, `martial_arts`, `gunhud`. В ss13-rs есть режим боя, удары руками и предметами по `force` с выбором зоны, толчок и урон брошенными предметами (`crates/ss13_game/src/combat.rs`, `click.rs`, `security.rs`, `throw.rs`); брони мобов, огнестрела и блоков щитами нет; боевые искусства — `crates/ss13_game/src/martial/`. Батоны, наручники и флешки — в ../security/, гранаты — у агента items.

| Файл | Что внутри |
| --- | --- |
| [melee-attacks.md](melee-attacks.md) | ClickOn, режим боя, цепочка атаки предметом, удары руками, анимации, блоки, неуклюжесть, удары о стол |
| [shove-tackle.md](shove-tackle.md) | толчок, обезоруживание, такл |
| [armor.md](armor.md) | датум брони, проверка брони моба, броня от ран/био |
| [guns-core.md](guns-core.md) | выстрел, разброс, очереди, обвесы, HUD патронов и оружейные стойки форка |
| [guns-ballistic.md](guns-ballistic.md) | механика баллистики, пистолеты, револьверы, автоматы, дробовики, винтовки, пулемёты, луки, игрушки |
| [guns-energy.md](guns-energy.md) | механика энергооружия, лазеры, e-gun, тазеры/дизейблеры, импульсное, кинетическое, специальное |
| [guns-special.md](guns-special.md) | шприцемёты, химпушка, гранатомёт, медлуч, крюк, пневмопушка, огнемёт |
| [ammo-projectiles.md](ammo-projectiles.md) | снаряды, попадание, гильзы, магазины, пули, лучи, особые снаряды |
| [firing-pins.md](firing-pins.md) | механика бойков и их виды |
| [melee-weapons.md](melee-weapons.md) | ножи, мечи, энергооружие, двуручное, дубинки, особое оружие, щиты, заточка |
| [embedding-dismemberment.md](embedding-dismemberment.md) | урон брошенным, встраивание, расчленение |
| [martial-arts.md](martial-arts.md) | механика и все боевые искусства, включая дзюдо форка |
