//! The security warrant console (server) after tg `/obj/machinery/computer/warrant`
//! (`code/game/machinery/computer/warrant.dm`, tgui `WarrantConsole`): anyone opens it with an
//! empty hand and sees the valid citations of every record; "pay" pays towards one from the
//! bank account of the ID the user holds or wears (the money goes to the security budget, the
//! fined person's PDA hears of 100 or more paid by someone else, and of the fine paid off);
//! "print" prints a bounty paper.
// shortcut: the bounty prints at once with a 2 s busy time (tg: after 2 s) and the window stays
// open (tg closes it); no blackbox or economy logs.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::construct::DmObj;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmName, spawner::Rng};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::manifest::{Manifest, alert_owner};
use super::*;
use crate::cargo::MONEY_SYMBOL;
use crate::cargo::economy::{ACCOUNT_SEC, Economy, account_of, bank_card_talk};
use crate::click::{ClickResolved, ResolveClicks};
use crate::inventory::Inventory;
use crate::power::Unpowered;
use crate::sound::{PlaySound, playsound};
use crate::tools::tell;

pub const WARRANT_CONSOLE: &str = "/obj/machinery/computer/warrant";
const PAPER: &str = "/obj/item/paper";
const SOUND_ERROR: &str = "sound/machines/terminal/terminal_error.ogg";
const SOUND_PRINT: &str = "sound/machines/printer.ogg";
const SOUND_EJECT: &str = "sound/machines/terminal/terminal_eject.ogg";
/// The print's `addtimer(..., 2 SECONDS)`.
const PRINT_SECS: f64 = 2.0;
/// `pay_fine`'s anonymous payers.
const PAYER_TITLES: [&str; 6] = [
    "Анонимный благодетель",
    "Щедрый гражданин",
    "Добрая душа",
    "Добрый самаритянин",
    "Дружелюбное лицо",
    "Отзывчивый незнакомец",
];

/// A warrant console: game seconds until its printer is free (`printing`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct WarrantConsole {
    pub printing_until: f64,
}

pub struct WarrantConsolePlugin;

impl Plugin for WarrantConsolePlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<WarrantConsoleUi>()
            .add_observer(attach)
            .add_systems(
                Update,
                (
                    open_console.after(ResolveClicks).before(UiSystems::Open),
                    warrant_acts.in_set(UiSystems::Handle),
                ),
            );
    }
}

fn attach(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if objs
        .get(add.entity)
        .is_ok_and(|o| is_under(&o.path, WARRANT_CONSOLE))
    {
        commands
            .entity(add.entity)
            .insert(WarrantConsole::default());
    }
}

/// `ui_interact` of a powered console, with an empty hand.
fn open_console(
    mut clicks: MessageReader<ClickResolved>,
    consoles: Query<&DmName, (With<WarrantConsole>, Without<Unpowered>)>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(name) = consoles.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_some())
        {
            continue;
        }
        let title = crate::vending::plain_name(&name.name).to_owned();
        open.write(OpenUi::new::<WarrantConsoleUi>(
            click.client,
            click.mob,
            click.target,
            title,
        ));
    }
}

/// `ui_data`: every record with citations, with its valid ones.
pub fn warrant_data(manifest: &Manifest) -> WarrantData {
    WarrantData {
        records: manifest
            .general
            .iter()
            .filter(|r| !r.citations.is_empty())
            .map(|r| WarrantRecord {
                citations: r
                    .citations
                    .iter()
                    .filter(|c| c.valid)
                    .map(|c| WarrantCitation {
                        author: c.author.clone(),
                        details: c.details.clone(),
                        fine: c.fine.unwrap_or_default(),
                        fine_name: c.name.clone(),
                        fine_ref: c.id.to_string(),
                        paid: c.paid,
                        time: c.time.clone(),
                    })
                    .collect(),
                crew_name: r.name.clone(),
                crew_ref: r.id.to_string(),
                notes: r.security_note.clone(),
                rank: r.rank.clone(),
            })
            .collect(),
    }
}

impl ServerUi for WarrantConsoleUi {
    type Source = &'static WarrantConsole;
    type UserParam = Res<'static, Manifest>;
    type Act = WarrantAct;

    fn static_data(_: &WarrantConsole) {}

    fn data(
        _: &WarrantConsole,
        _: Entity,
        manifest: &bevy::ecs::system::SystemParamItem<'_, '_, Self::UserParam>,
    ) -> WarrantData {
        warrant_data(manifest)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<WarrantAct, UiReject> {
        let id = |key: &str| {
            params
                .get(key)
                .and_then(|v| v.parse::<u32>().ok())
                .ok_or_else(|| UiReject::BadParams(format!("{key}: expected a ref")))
        };
        Ok(match action {
            PAY => WarrantAct::Pay {
                record: id("crew_ref")?,
                fine: id("fine_ref")?,
                amount: params.number("amount")?,
            },
            PRINT => WarrantAct::Print {
                record: id("crew_ref")?,
                fine: id("fine_ref")?,
            },
            REFRESH => WarrantAct::Refresh,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum WarrantAct {
    Pay { record: u32, fine: u32, amount: f64 },
    Print { record: u32, fine: u32 },
    Refresh,
}

fn warrant_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<WarrantConsoleUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, WarrantAct)> = acts
        .read(world.resource::<Messages<UiActed<WarrantConsoleUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, user, console, act) in pending {
        if world.get::<WarrantConsole>(console).is_none()
            || world.get::<Unpowered>(console).is_some()
        {
            continue;
        }
        let done = match act {
            WarrantAct::Pay {
                record,
                fine,
                amount,
            } => pay_fine(world, client, user, console, record, fine, amount),
            WarrantAct::Print { record, fine } => {
                print_bounty(world, client, user, console, record, fine)
            }
            WarrantAct::Refresh => Ok(()),
        };
        if let Err(why) = done {
            debug!("WarrantConsole {act:?} on {console} refused: {why}");
        }
    }
}

fn sound(world: &mut World, at: Entity, file: &str) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        playsound(world, PlaySound::new(pos, file, 100.0, true));
    }
}

fn deny(world: &mut World, client: Entity, console: Entity, text: &str) {
    tell(world, client, Span::Warning, text);
    sound(world, console, SOUND_ERROR);
}

/// `pay_fine(user, params)`, every check at the moment of effect.
pub fn pay_fine(
    world: &mut World,
    client: Entity,
    user: Entity,
    console: Entity,
    record: u32,
    fine_ref: u32,
    amount: f64,
) -> Result<(), &'static str> {
    let manifest = world.resource::<Manifest>();
    let target = manifest.get(record).ok_or("no such record")?;
    let target_name = target.name.clone();
    let warrant = target
        .citations
        .iter()
        .find(|c| c.id == fine_ref && c.valid)
        .ok_or("no such citation")?;
    let fine = warrant.fine.unwrap_or_default();
    // `!isliving(user) || issilicon(user)`.
    if world.get::<crate::health::Health>(user).is_none()
        || world.get::<crate::silicon::Silicon>(user).is_some()
    {
        deny(world, client, console, "ДОСТУП ЗАПРЕЩЕН");
        return Err("not a living non-silicon");
    }
    let Some(card) = world.get::<Inventory>(user).and_then(|i| i.id_card(world)) else {
        deny(
            world,
            client,
            console,
            "ДОСТУП ЗАПРЕЩЕН: ID-карта не обнаружена.",
        );
        return Err("no ID");
    };
    let account = account_of(world, card).filter(|&id| {
        world
            .get_resource::<Economy>()
            .and_then(|e| e.account(id))
            .is_some_and(|a| !a.holder.is_empty() && a.holder != "Unassigned")
    });
    let Some(account) = account else {
        deny(
            world,
            client,
            console,
            "ДОСТУП ЗАПРЕЩЕН: к ID-карте не привязан счёт.",
        );
        return Err("no account");
    };
    let valid_amount = amount.fract() == 0.0 && amount > 0.0 && amount <= f64::from(fine);
    let paid = valid_amount
        && world
            .resource_mut::<Economy>()
            .account_mut(account)
            .is_some_and(|a| a.adjust_money(-(amount as i64)));
    if !paid {
        deny(world, client, console, "ДОСТУП ЗАПРЕЩЕН: неверная сумма.");
        return Err("invalid amount");
    }
    let amount = amount as u32;
    bank_card_talk(
        world,
        account,
        &format!(
            "Вы заплатили {amount}{MONEY_SYMBOL} в счёт штрафа {target_name} размером \
             {fine}{MONEY_SYMBOL}."
        ),
    );
    let left = {
        let mut manifest = world.resource_mut::<Manifest>();
        let warrant = manifest
            .get_mut(record)
            .and_then(|r| r.citations.iter_mut().find(|c| c.id == fine_ref))
            .expect("found above, nothing ran since");
        warrant.pay_fine(amount);
        warrant.fine.unwrap_or_default()
    };
    // tg compares the record's name with the mob, which never matches; the intent is that
    // paying one's own fine sends no alerts.
    let own = crate::examine::name_of(world, user) == target_name;
    if amount >= 100 && !own {
        let title = {
            let seed = world
                .get_resource::<Time>()
                .map_or(0, |t| t.elapsed().as_nanos() as u64);
            PAYER_TITLES[Rng::new(seed ^ u64::from(fine_ref)).range(0, 5) as usize]
        };
        alert_owner(
            world,
            console,
            &target_name,
            &format!("{title} оплатил {amount}{MONEY_SYMBOL} вашего штрафа."),
        );
    }
    if let Some(mut economy) = world.get_resource_mut::<Economy>()
        && let Some(sec) = economy.department_mut(ACCOUNT_SEC)
    {
        sec.adjust_money(i64::from(amount));
    }
    if left != 0 {
        return Ok(());
    }
    if !own {
        alert_owner(
            world,
            console,
            &target_name,
            "Один из ваших неоплаченных штрафов полностью оплачен.",
        );
    }
    if let Some(warrant) = world
        .resource_mut::<Manifest>()
        .get_mut(record)
        .and_then(|r| r.citations.iter_mut().find(|c| c.id == fine_ref))
    {
        warrant.valid = false;
    }
    Ok(())
}

/// `print_bounty(user, params)`: a bounty paper for an unpaid citation.
pub fn print_bounty(
    world: &mut World,
    client: Entity,
    user: Entity,
    console: Entity,
    record: u32,
    fine_ref: u32,
) -> Result<(), &'static str> {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world
        .get::<WarrantConsole>(console)
        .is_some_and(|c| now < c.printing_until)
    {
        crate::balloon::balloon_alert(world, user, console, "принтер занят");
        sound(world, console, SOUND_ERROR);
        return Err("printer busy");
    }
    let manifest = world.resource::<Manifest>();
    let target = manifest.get(record).ok_or("no such record")?;
    let warrant = target
        .citations
        .iter()
        .find(|c| c.id == fine_ref)
        .ok_or("no such citation")?;
    let fine = warrant.fine.filter(|&f| f > 0).ok_or("nothing to pay")?;
    let issuer = crate::examine::name_of(world, user);
    let note = if target.security_note.is_empty() {
        "Отсутствуют."
    } else {
        target.security_note.as_str()
    };
    let text = format!(
        "<center><h2><b>Награда за {name}</b></h2></center><br><center>Разыскивается за \
         {crime}</center><br><br><b>Детали:</b><br>{details}<br><b>Выдано:</b><br>{issuer}<br>\
         <b>Дата выдачи:</b><br>{time}<br><b>Комментарии:</b><br>{note}<br><br><center><b>\
         ШТРАФ:</b> {fine}{MONEY_SYMBOL}</center>",
        name = target.name,
        crime = warrant.name,
        details = warrant.details,
        time = warrant.time,
    );
    let name = format!("Награда за {}", target.name);
    let desc = format!("Награда в {fine}{MONEY_SYMBOL} за {}.", target.name);
    let pos = world
        .get::<TilePos>(console)
        .copied()
        .ok_or("off the map")?;
    if let Some(mut c) = world.get_mut::<WarrantConsole>(console) {
        c.printing_until = now + PRINT_SECS;
    }
    crate::balloon::balloon_alert(world, user, console, "печать");
    sound(world, console, SOUND_PRINT);
    let paper = crate::construction::spawn_obj(world, PAPER, pos).ok_or("no paper type")?;
    world.entity_mut(paper).insert(DmName { name, desc });
    crate::service::write_printed(world, paper, text);
    sound(world, console, SOUND_EJECT);
    let _ = client;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::manifest::{Crime, Joiner};
    use ss13_net::chat::server::SendChat;

    fn citation(id: u32, fine: u32) -> Crime {
        Crime {
            id,
            name: "Мусор".into(),
            details: "На полу".into(),
            author: "Алиса".into(),
            author_mob: None,
            time: "00:01:00".into(),
            valid: true,
            voider: None,
            fine: Some(fine),
            paid: 0,
        }
    }

    #[test]
    fn data_lists_only_valid_citations_of_fined_records() {
        let mut m = Manifest::default();
        let mob = Entity::PLACEHOLDER;
        let bob = m.inject(Joiner {
            name: "Боб",
            assignment: "Ассистент",
            job: None,
            mob,
        });
        m.inject(Joiner {
            name: "Ева",
            assignment: "Клоун",
            job: None,
            mob,
        });
        let r = m.get_mut(bob).unwrap();
        r.citations.push(citation(100, 300));
        let mut void = citation(101, 50);
        void.valid = false;
        r.citations.push(void);
        let data = warrant_data(&m);
        assert_eq!(data.records.len(), 1);
        assert_eq!(data.records[0].crew_name, "Боб");
        assert_eq!(data.records[0].citations.len(), 1);
        assert_eq!(data.records[0].citations[0].fine, 300);
    }

    #[test]
    fn paying_takes_the_money_and_voids_a_paid_off_fine() {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<Messages<PlaySound>>();
        let mut m = Manifest::default();
        let bob = m.inject(Joiner {
            name: "Боб",
            assignment: "Ассистент",
            job: None,
            mob: Entity::PLACEHOLDER,
        });
        m.get_mut(bob).unwrap().citations.push(citation(100, 80));
        world.insert_resource(m);
        let mut economy = Economy::new(1);
        let account = economy.open_account("Боб", 0, "CIV");
        economy.account_mut(account).unwrap().balance = 100;
        let sec_before = economy.department(ACCOUNT_SEC).unwrap().balance;
        world.insert_resource(economy);
        let card = world
            .spawn((
                crate::inventory::IdCard { access: Vec::new() },
                crate::cargo::economy::BankAccountId(account),
            ))
            .id();
        let user = world
            .spawn((
                Name::new("Боб"),
                crate::health::Health::default(),
                Inventory {
                    id: Some(card),
                    ..default()
                },
            ))
            .id();
        let console = world.spawn(WarrantConsole::default()).id();
        let client = Entity::PLACEHOLDER;
        // More than the fine, a fraction, nothing: refused, nothing paid.
        for bad in [81.0, 0.5, 0.0, -5.0] {
            assert!(pay_fine(&mut world, client, user, console, bob, 100, bad).is_err());
        }
        pay_fine(&mut world, client, user, console, bob, 100, 30.0).unwrap();
        let c = world.resource::<Manifest>().get(bob).unwrap().citations[0].clone();
        assert_eq!((c.fine, c.paid, c.valid), (Some(50), 30, true));
        pay_fine(&mut world, client, user, console, bob, 100, 50.0).unwrap();
        let c = world.resource::<Manifest>().get(bob).unwrap().citations[0].clone();
        assert_eq!((c.fine, c.paid, c.valid), (Some(0), 80, false));
        let economy = world.resource::<Economy>();
        assert_eq!(economy.account(account).unwrap().balance, 20);
        assert_eq!(
            economy.department(ACCOUNT_SEC).unwrap().balance,
            sec_before + 80
        );
        // A voided citation takes no more money; nor does a bigger sum than the balance.
        assert!(pay_fine(&mut world, client, user, console, bob, 100, 10.0).is_err());
    }
}
