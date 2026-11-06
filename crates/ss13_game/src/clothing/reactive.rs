//! Reactive armor (server), after tg `code/modules/clothing/suits/reactive_armour.dm`.
//!
//! - An anomaly core used on a reactive armor shell turns it into the core's armor
//!   (`/obj/item/reactive_armor_shell/item_interaction`): gravitational -> repulse, flux ->
//!   tesla, bluespace -> teleport, hallucination -> hallucinating, bioscrambler, dimensional,
//!   ectoplasm and weather to their own, any other core -> stealth.
//! - Use in hand toggles it (`attack_self`: `reactive` / `reactiveoff` sprite). While on and
//!   worn as the suit, every melee or projectile hit has `hit_reaction_chance` (50%) to
//!   trigger it ([`hit_reaction`], tg `check_block` -> `hit_reaction`): on its 10 s cooldown it
//!   only says so (`cooldown_activation`); otherwise its effect (`reactive_activation`) blocks
//!   the hit; within 30 s of an EMP the effect turns on the wearer (`emp_activation`), and only
//!   some of those block.
//! - Effects: teleport (the wearer up to 6 tiles away; EMP: the armor falls off and teleports
//!   alone), incendiary (8 fire stacks and ignition to every carbon within 6, the wearer soaked;
//!   EMP: 12 stacks on the wearer), stealth (invisible for 4 s; EMP: the attacker instead),
//!   tesla (a zap chain of 2.5e4 over 20 tiles; EMP: the wearer is shocked), repulse (everything
//!   loose within 7 thrown away; EMP: pulled in), table (teleported up to 10 tiles onto a new
//!   table, knocked down, 10 brute and 40 stamina), hallucinating (a hallucination pulse of
//!   radius 3; EMP: the wearer hallucinates).
// shortcut: bioscrambling, barricade, ectoplasm and weather armors exist (the shell makes them)
// but never react: their bodypart swaps, dimensional themes, ghost possession and weather are
// not ported. The stealth decoy (`/mob/living/basic/illusion/escape`) and the alpha fades are
// not ported: stealth is the mob's sprite hidden outright. The EMP'd table armor teleports
// without the shattering glass table (`table_shatter`); the table mood event is not ported;
// tesla armor's `emp_protection` and `siemens_coefficient` are not ported, and its EMP shock is
// one zap's worth of damage, not the area's power (`electrocute_mob(owner, get_area(src))`).
// Upgrade with them.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, TilePos};
use ss13_world::SpatialIndex;

use crate::combat::CombatRng;
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health};
use crate::inventory::Inventory;
use crate::security::{name_of, show};
use crate::sound::{PlaySound, playsound};
use crate::teleport::{Channel, Tele, do_teleport};

pub const REACTIVE: &str = "/obj/item/clothing/suit/armor/reactive";
pub const SHELL: &str = "/obj/item/reactive_armor_shell";
const CORE: &str = "/obj/item/assembly/signaler/anomaly";
/// `/obj/structure/table`, made by the table armor.
pub const TABLE: &str = "/obj/structure/table";

/// `hit_reaction_chance`, percent.
pub const HIT_REACTION_CHANCE: f32 = 50.0;
/// `reactivearmor_cooldown_duration`, seconds.
pub const COOLDOWN: f64 = 10.0;
/// `emp_act`: `bad_effect` lasts this long, seconds.
pub const BAD_EFFECT: f64 = 30.0;
/// The EMP'd tesla armor's shock of the wearer, burn: one zap of its `zap_power`
/// (`power / 600`).
const EMP_SHOCK: f32 = 42.0;
/// `stealth_time`, seconds.
const STEALTH_TIME: f64 = 4.0;

const BLINK: &str = "sound/effects/magic/blink.ogg";
const BUZZ: &str = "sound/machines/buzz/buzz-sigh.ogg";
const FIREBALL: &str = "sound/effects/magic/fireball.ogg";
const REPULSE: &str = "sound/effects/magic/repulse.ogg";
const TABLESLAM: &str = "sound/effects/tableslam.ogg";

/// Which reactive armor (its subtype).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Base,
    Teleport,
    Fire,
    Stealth,
    Tesla,
    Repulse,
    Table,
    Hallucinating,
    /// Bioscrambling, barricade, ectoplasm, weather: not ported, they never react.
    Inert,
}

impl Kind {
    pub fn of(path: &str) -> Option<Self> {
        if !is_under(path, REACTIVE) {
            return None;
        }
        let sub = |s: &str| is_under(path, &format!("{REACTIVE}/{s}"));
        Some(if sub("teleport") {
            Self::Teleport
        } else if sub("fire") {
            Self::Fire
        } else if sub("stealth") {
            Self::Stealth
        } else if sub("tesla") {
            Self::Tesla
        } else if sub("repulse") {
            Self::Repulse
        } else if sub("table") {
            Self::Table
        } else if sub("hallucinating") {
            Self::Hallucinating
        } else if path == REACTIVE {
            Self::Base
        } else {
            Self::Inert
        })
    }

    /// `cooldown_message`.
    fn cooldown_message(self) -> &'static str {
        match self {
            Self::Teleport => {
                "Система реактивной телепортации ещё перезаряжается! Она не срабатывает!"
            }
            Self::Fire => {
                "Реактивная зажигательная броня срабатывает, но не выпускает пламя: огнемёты ещё перезаряжаются!"
            }
            Self::Stealth => {
                "Система реактивной маскировки срабатывает, но ей не хватает заряда для полной невидимости!"
            }
            Self::Tesla => "Конденсаторы реактивной тесла-брони ещё заряжаются! Броня лишь искрит.",
            Self::Repulse => {
                "Генератор отталкивания ещё перезаряжается! Ему не хватает силы на волну!"
            }
            Self::Table => "Фабрикаторы реактивной стол-брони ещё перезаряжаются!",
            Self::Hallucinating => "Связь сейчас рассинхронизирована... Перекалибровка.",
            Self::Base | Self::Inert => {
                "Реактивная броня ничего особенного не делает, потому что перезаряжается! От чего? Это знает только реактивная броня."
            }
        }
    }

    /// `emp_message`.
    fn emp_message(self) -> &'static str {
        match self {
            Self::Teleport => "Телепортационные расчёты реактивной брони начинают сыпать ошибками!",
            Self::Fire => {
                "Система наведения реактивной зажигательной брони начинает перезагрузку..."
            }
            Self::Stealth => "Система оценки угроз реактивной маскирующей брони падает...",
            Self::Tesla => "Тесла-конденсаторы на мгновение зловеще пищат.",
            Self::Repulse => "Генератор отталкивания сбрасывается к заводским настройкам...",
            Self::Table => {
                "Фабрикаторы реактивной стол-брони на мгновение зловеще щёлкают и жужжат..."
            }
            Self::Hallucinating => "Вы чувствуете обратный удар ментального импульса.",
            Self::Base | Self::Inert => {
                "Реактивную броню поразил ЭМИ! Чёрт, теперь она ТОЧНО ничего не будет делать!"
            }
        }
    }
}

/// The armor's state (`active`, `bad_effect`, `reactivearmor_cooldown`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ReactiveArmor {
    pub kind: Kind,
    pub active: bool,
    /// Game time `bad_effect` ends.
    pub bad_until: f64,
    /// Game time the armor can react again.
    pub cooldown_until: f64,
}

/// What a hit made the armor do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reaction {
    Cooldown,
    Normal,
    Emp,
}

impl ReactiveArmor {
    /// tg `hit_reaction`'s rolls: none when off or the 50% fails.
    pub fn roll(&self, now: f64, rng: &mut CombatRng) -> Option<Reaction> {
        if !self.active || self.kind == Kind::Inert || !rng.chance(HIT_REACTION_CHANCE) {
            return None;
        }
        Some(if now < self.cooldown_until {
            Reaction::Cooldown
        } else if now < self.bad_until {
            Reaction::Emp
        } else {
            Reaction::Normal
        })
    }
}

/// Whether the reaction blocks the hit (`reactive_activation`/`emp_activation` return values).
pub fn blocks(kind: Kind, reaction: Reaction) -> bool {
    match reaction {
        Reaction::Cooldown => false,
        Reaction::Normal => true,
        Reaction::Emp => matches!(
            kind,
            Kind::Base | Kind::Tesla | Kind::Table | Kind::Hallucinating
        ),
    }
}

pub(super) fn build(app: &mut App) {
    app.add_observer(add_reactive).add_systems(
        Update,
        (
            reactive_emps.in_set(crate::emp::EmpSystems::Effects),
            end_stealth.run_if(any_with_component::<Stealthed>),
        ),
    );
}

fn add_reactive(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Some(kind) = items.get(add.entity).ok().and_then(|i| Kind::of(&i.path)) else {
        return;
    };
    commands.entity(add.entity).insert_if_new(ReactiveArmor {
        kind,
        active: false,
        bad_until: 0.0,
        cooldown_until: 0.0,
    });
}

/// `attack_self`: on or off.
pub fn attack_self(world: &mut World, client: Entity, _mob: Entity, item: Entity) -> bool {
    let Some(mut armor) = world.get_mut::<ReactiveArmor>(item) else {
        return false;
    };
    armor.active = !armor.active;
    let active = armor.active;
    let name = cap(&name_of(world, item));
    let state = if active {
        "активна"
    } else {
        "неактивна"
    };
    crate::tools::tell(
        world,
        client,
        Span::Notice,
        format!("{name} теперь {state}."),
    );
    let icon_state = if active { "reactive" } else { "reactiveoff" };
    super::toggles::set_states(world, item, icon_state, String::new());
    true
}

/// `/obj/item/reactive_armor_shell/item_interaction` with an anomaly core.
pub fn shell_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let path = |e| crate::construction::path_of(world, e).unwrap_or_default();
    let (core, shell) = (path(held), path(target));
    if !is_under(&shell, SHELL) || !is_under(&core, CORE) {
        return false;
    }
    let Some(pos) = crate::explosions::turf_of(world, target) else {
        return false;
    };
    let armour = armor_for_core(&core);
    let core_name = name_of(world, held);
    crate::tools::tell(
        world,
        client,
        Span::Notice,
        format!("Вы вставляете {core_name} в нагрудную пластину, и броня тихо оживает."),
    );
    crate::construction::spawn_obj(world, armour, pos);
    world.despawn(target);
    world.despawn(held);
    crate::inventory::refresh_look(world, mob);
    true
}

/// `anomaly_armour_types`: the armor a core makes (stealth for the others).
pub fn armor_for_core(core: &str) -> &'static str {
    const TYPES: [(&str, &str); 8] = [
        ("grav", "repulse"),
        ("flux", "tesla"),
        ("bluespace", "teleport"),
        ("bioscrambler", "bioscrambling"),
        ("hallucination", "hallucinating"),
        ("dimensional", "barricade"),
        ("ectoplasm", "ectoplasm"),
        ("weather", "weather"),
    ];
    const PATHS: [&str; 8] = [
        "/obj/item/clothing/suit/armor/reactive/repulse",
        "/obj/item/clothing/suit/armor/reactive/tesla",
        "/obj/item/clothing/suit/armor/reactive/teleport",
        "/obj/item/clothing/suit/armor/reactive/bioscrambling",
        "/obj/item/clothing/suit/armor/reactive/hallucinating",
        "/obj/item/clothing/suit/armor/reactive/barricade",
        "/obj/item/clothing/suit/armor/reactive/ectoplasm",
        "/obj/item/clothing/suit/armor/reactive/weather",
    ];
    TYPES
        .iter()
        .position(|(c, _)| is_under(core, &format!("{CORE}/{c}")))
        .map_or("/obj/item/clothing/suit/armor/reactive/stealth", |i| {
            PATHS[i]
        })
}

/// tg human `check_block` for the worn reactive suit of `owner` hit by `hitby` (`attack_text`
/// names the hit): true when the hit is blocked.
pub fn hit_reaction(
    world: &mut World,
    owner: Entity,
    hitby: Option<Entity>,
    attack_text: &str,
) -> bool {
    let Some(suit) = world.get::<Inventory>(owner).and_then(|i| i.suit) else {
        return false;
    };
    let Some(armor) = world.get::<ReactiveArmor>(suit).copied() else {
        return false;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(reaction) = armor.roll(now, &mut world.resource_mut::<CombatRng>()) else {
        return false;
    };
    react(world, owner, suit, reaction, hitby, attack_text);
    blocks(armor.kind, reaction)
}

/// The effect of a rolled reaction (the cooldown is set here).
pub fn react(
    world: &mut World,
    owner: Entity,
    suit: Entity,
    reaction: Reaction,
    hitby: Option<Entity>,
    attack_text: &str,
) {
    let Some(armor) = world.get::<ReactiveArmor>(suit).copied() else {
        return;
    };
    let Some(&at) = world.get::<TilePos>(owner) else {
        return;
    };
    let kind = armor.kind;
    let who = name_of(world, owner);
    let item = name_of(world, suit);
    if reaction == Reaction::Cooldown {
        // `cooldown_activation`: stealth stays quiet while cloaked; tesla, hallucinating spark.
        if kind == Kind::Stealth && world.get::<Stealthed>(owner).is_some() {
            return;
        }
        if matches!(kind, Kind::Tesla | Kind::Hallucinating) {
            crate::effects::particles::do_sparks(world, 1, true, at);
        }
        say(world, at, kind.cooldown_message().to_owned());
        return;
    }
    if let Some(mut a) = world.get_mut::<ReactiveArmor>(suit) {
        a.cooldown_until = world_now(world) + COOLDOWN;
    }
    let emp = reaction == Reaction::Emp;
    match (kind, emp) {
        (Kind::Base, false) => say(world, at, "Реактивная броня ничего особенного не делает! Никаких сюрпризов.".to_owned()),
        (Kind::Base, true) => say(world, at, "Реактивная броня ничего особенного не делает, несмотря на ЭМИ! Разве что выдаёт особое сообщение.".to_owned()),
        (Kind::Teleport, false) => {
            say(world, at, format!("Система реактивной телепортации отбрасывает {who} от {attack_text}!"));
            play(world, at, BLINK, 100.0);
            teleport(world, owner, at, 6);
        }
        (Kind::Teleport, true) => {
            say(world, at, format!("Система реактивной телепортации уносит саму себя от {attack_text}, оставляя кое-кого позади!"));
            drop_suit(world, owner, suit, at);
            play(world, at, BUZZ, 50.0);
            play(world, at, BLINK, 100.0);
            teleport(world, suit, at, 6);
        }
        (Kind::Fire, false) => {
            say(world, at, format!("{} блокирует {attack_text}, выпуская струи пламени!", cap(&item)));
            play(world, at, FIREBALL, 100.0);
            for victim in carbons_in(world, at, 6) {
                if victim != owner {
                    crate::fire::adjust_fire_stacks(world, victim, 8.0);
                    crate::fire::ignite_mob(world, victim);
                }
            }
            crate::fire::set_wet_stacks(world, owner, 20.0);
        }
        (Kind::Fire, true) => {
            say(world, at, format!("{} лишь усугубляет {attack_text}, поливая {who} расплавленной смертью!", cap(&item)));
            play(world, at, FIREBALL, 100.0);
            crate::fire::adjust_fire_stacks(world, owner, 12.0);
            crate::fire::ignite_mob(world, owner);
        }
        (Kind::Stealth, false) => {
            say(world, at, format!("{who} получает {attack_text} в грудь!"));
            stealth(world, owner);
        }
        (Kind::Stealth, true) => {
            // Not on a living attacker: it just does not activate.
            if let Some(attacker) = hitby.filter(|&a| world.get::<Health>(a).is_some()) {
                say(world, at, format!("{} срабатывает, маскируя не того!", cap(&item)));
                stealth(world, attacker);
            }
        }
        (Kind::Tesla, false) => {
            say(world, at, format!("{} блокирует {attack_text}, выпуская дуги молний!", cap(&item)));
            let mut shocked = vec![owner];
            crate::spells::effects::tesla_zap(world, at, 20, 2.5e4, &mut shocked, 0);
        }
        (Kind::Tesla, true) => {
            say(world, at, format!("{} блокирует {attack_text}, но затягивает в {who} огромный заряд энергии из окружения!", cap(&item)));
            // `electrocute_mob(owner, get_area(src), src, 1)`.
            crate::wires::electrocute_act(world, owner, EMP_SHOCK, suit, 1.0);
        }
        (Kind::Repulse, pull) => {
            play(world, at, REPULSE, 100.0);
            let text = if pull {
                format!("{} не блокирует {attack_text}, а создаёт притягивающую силу!", cap(&item))
            } else {
                format!("{} блокирует {attack_text}, превращая удар в силовую волну!", cap(&item))
            };
            say(world, at, text);
            repulse(world, owner, at, pull);
        }
        (Kind::Table, glass) => {
            if glass {
                say(world, at, format!("Система реактивной телепортации отбрасывает {who} от {attack_text} и швыряет на сфабрикованный стеклянный стол!"));
                teleport(world, owner, at, 10);
                return;
            }
            say(world, at, format!("Система реактивной телепортации отбрасывает {who} от {attack_text} и швыряет на сфабрикованный стол!"));
            say(world, at, format!("{} ЛОЖИТСЯ НА СТОЛ!!!", who.to_uppercase()));
            crate::status_effects::knockdown(world, owner, 3.0, 0.0);
            hurt(world, owner, DamageType::Brute, 10.0);
            crate::status_effects::adjust_stamina(world, owner, 40.0);
            play(world, at, TABLESLAM, 90.0);
            teleport(world, owner, at, 10);
            if let Some(&landed) = world.get::<TilePos>(owner) {
                crate::construction::spawn_obj(world, TABLE, landed);
            }
        }
        (Kind::Hallucinating, false) => {
            say(world, at, format!("{} блокирует {attack_text}, рассылая ментальные импульсы!", cap(&item)));
            crate::hallucination::hallucination_pulse(world, at, 3, 50.0, 300.0, &[]);
        }
        (Kind::Hallucinating, true) => {
            say(world, at, format!("{} блокирует {attack_text}, но затягивает в {who} огромный заряд ментальной энергии!", cap(&item)));
            crate::hallucination::adjust_hallucinations_up_to(world, owner, 50.0, 240.0);
        }
        (Kind::Inert, _) => {}
    }
}

fn world_now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
}

fn say(world: &mut World, at: TilePos, text: String) {
    show(
        world,
        at,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, text),
        &[],
    );
}

fn play(world: &mut World, at: TilePos, sound: &str, vol: f32) {
    playsound(world, PlaySound::new(at, sound, vol, true));
}

/// `do_teleport(thing, get_turf(owner), range, no_effects = TRUE, TELEPORT_CHANNEL_BLUESPACE)`.
fn teleport(world: &mut World, thing: Entity, at: TilePos, range: i32) {
    do_teleport(
        world,
        thing,
        at,
        Tele {
            precision: range,
            channel: Channel::Bluespace,
            no_effects: true,
            ..default()
        },
    );
}

/// `dropItemToGround(src, TRUE, TRUE)` of the worn suit.
fn drop_suit(world: &mut World, owner: Entity, suit: Entity, at: TilePos) {
    if world.get::<Inventory>(owner).and_then(|i| i.suit) != Some(suit) {
        return;
    }
    crate::inventory::unequip(world, owner, at, ss13_shared::protocol::InventorySlot::Suit);
    crate::inventory::put_down(world, suit, at);
    crate::inventory::refresh_look(world, owner);
}

fn carbons_in(world: &World, at: TilePos, range: u32) -> Vec<Entity> {
    let mut mobs: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(at, range)
        .filter(|&e| world.get::<Inventory>(e).is_some() && world.get::<Health>(e).is_some())
        .collect();
    mobs.sort();
    mobs
}

fn hurt(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    if let (Some(mut health), Some(&stat)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<crate::health::Stat>(mob),
    ) {
        let mut new_stat = stat;
        crate::health::apply_damage(&mut health, &mut new_stat, kind, amount);
        world.entity_mut(mob).insert(health);
        if new_stat != stat {
            world.entity_mut(mob).insert(new_stat);
        }
    }
}

/// `repulse/reactive_activation`: everything loose within 7 thrown to the edge away from the
/// wearer (`emp_activation`: at the wearer), 10 tiles.
fn repulse(world: &mut World, owner: Entity, at: TilePos, pull: bool) {
    let mut things: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(at, 7)
        .filter(|&e| {
            e != owner
                && world.get::<crate::projectile::Projectile>(e).is_none()
                && world.get::<crate::ghost::Ghost>(e).is_none()
                && (world.get::<Health>(e).is_some()
                    || world.get::<DmItem>(e).is_some_and(|i| !i.anchored))
        })
        .collect();
    things.sort();
    for thing in things {
        let Some(&pos) = world.get::<TilePos>(thing) else {
            continue;
        };
        let target = if pull {
            at
        } else {
            let (dx, dy) = ((pos.x - at.x).signum(), (pos.y - at.y).signum());
            if dx == 0 && dy == 0 {
                continue;
            }
            TilePos::new(pos.x + dx * 10, pos.y + dy * 10, pos.z)
        };
        crate::throw::throw_at(world, thing, target, 10, 1.0);
    }
}

/// A mob made invisible by stealth armor until the game time.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Stealthed {
    pub until: f64,
    /// The sprite's color before (`initial(alpha)`).
    pub color: Option<[u8; 4]>,
}

/// `owner.alpha = 0` for [`STEALTH_TIME`].
fn stealth(world: &mut World, mob: Entity) {
    let until = world_now(world) + STEALTH_TIME;
    let Some(mut look) = world.get_mut::<Appearance>(mob) else {
        return;
    };
    let color = look.color;
    let [r, g, b, _] = color.unwrap_or([255, 255, 255, 255]);
    look.color = Some([r, g, b, 0]);
    let previous = world.get::<Stealthed>(mob).map_or(color, |s| s.color);
    world.entity_mut(mob).insert(Stealthed {
        until,
        color: previous,
    });
}

/// `end_stealth`: the sprite comes back.
fn end_stealth(world: &mut World) {
    let now = world_now(world);
    let mut q = world.query::<(Entity, &Stealthed)>();
    let done: Vec<(Entity, Option<[u8; 4]>)> = q
        .iter(world)
        .filter(|(_, s)| now >= s.until)
        .map(|(e, s)| (e, s.color))
        .collect();
    for (mob, color) in done {
        world.entity_mut(mob).remove::<Stealthed>();
        if let Some(mut look) = world.get_mut::<Appearance>(mob) {
            look.color = color;
        }
    }
}

/// `reactive/emp_act`: an active armor hit by a pulse turns `bad_effect` for 30 s.
fn reactive_emps(world: &mut World, mut cursor: Local<MessageCursor<crate::emp::EmpAct>>) {
    for ev in crate::emp::emp_acts(world, &mut cursor) {
        if !ev.hits_self() {
            continue;
        }
        let now = world_now(world);
        let Some(armor) = world.get::<ReactiveArmor>(ev.target).copied() else {
            continue;
        };
        if !armor.active || now < armor.bad_until {
            continue;
        }
        if let Some(mut a) = world.get_mut::<ReactiveArmor>(ev.target) {
            a.bad_until = now + BAD_EFFECT;
        }
        if let Some(at) = crate::explosions::turf_of(world, ev.target) {
            show(
                world,
                at,
                DEFAULT_MESSAGE_RANGE,
                ChatChannel::Info,
                (Span::Warning, armor.kind.emp_message().to_owned()),
                &[],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_cores() {
        assert_eq!(Kind::of(REACTIVE), Some(Kind::Base));
        assert_eq!(
            Kind::of(&format!("{REACTIVE}/teleport")),
            Some(Kind::Teleport)
        );
        assert_eq!(
            Kind::of(&format!("{REACTIVE}/ectoplasm")),
            Some(Kind::Inert)
        );
        assert_eq!(Kind::of("/obj/item/clothing/suit/armor/vest"), None);
        assert_eq!(
            armor_for_core("/obj/item/assembly/signaler/anomaly/bluespace"),
            "/obj/item/clothing/suit/armor/reactive/teleport"
        );
        assert_eq!(
            armor_for_core("/obj/item/assembly/signaler/anomaly/pyro"),
            "/obj/item/clothing/suit/armor/reactive/stealth"
        );
    }

    #[test]
    fn rolls_cooldown_emp_and_blocks() {
        let mut armor = ReactiveArmor {
            kind: Kind::Teleport,
            active: false,
            bad_until: 0.0,
            cooldown_until: 0.0,
        };
        let mut rng = CombatRng::default();
        assert!((0..50).all(|_| armor.roll(1.0, &mut rng).is_none()), "off");
        armor.active = true;
        let rolls: Vec<_> = (0..200).filter_map(|_| armor.roll(1.0, &mut rng)).collect();
        assert!(
            rolls.len() > 50 && rolls.len() < 150,
            "about half: {}",
            rolls.len()
        );
        assert!(rolls.iter().all(|&r| r == Reaction::Normal));
        armor.cooldown_until = 5.0;
        assert!(
            (0..50)
                .filter_map(|_| armor.roll(1.0, &mut rng))
                .all(|r| r == Reaction::Cooldown)
        );
        armor.bad_until = 40.0;
        assert!(
            (0..50)
                .filter_map(|_| armor.roll(6.0, &mut rng))
                .all(|r| r == Reaction::Emp)
        );
        assert!(blocks(Kind::Teleport, Reaction::Normal));
        assert!(
            !blocks(Kind::Teleport, Reaction::Emp),
            "the armor leaves, the hit lands"
        );
        assert!(!blocks(Kind::Teleport, Reaction::Cooldown));
        assert!(blocks(Kind::Tesla, Reaction::Emp));
    }
}
