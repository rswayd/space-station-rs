//! The `admin` bot step (`alice admin kick bob "reason"`): one admin verb or adminhelp as an
//! `AdminRequest` over the admin channel. Players are named by bot name and become their ckeys
//! when the step runs. Syntax: docs/RUNNING.md, "Functional test bots".

use ss13_shared::admin::{AdminRequest, AntagRole, Smite, TicketState};

/// The verbs and their arguments, for parse errors.
pub const VERBS: &str = "ahelp <text>, asay <text>, readmin, deadmin, players, pm <bot> <text>, \
     ticket <bot> close|resolve|reopen, jump <bot>, get <bot>, follow <bot>, kick <bot> [reason], \
     mute <bot>, unmute <bot>, ban <bot> [reason], unban <bot>, aghost, find <text>, spawn <path>, \
     heal <bot>, smite <bot>, events, event <name>, endround, timescale <n>, \
     announce <title> <text>, antag <bot> traitor|brother|changeling|vampire|cult|heretic|headrev|rev, \
     dynamic, midround <config tag>";

/// Builds the request. `ckey` maps a bot name to its ckey; `ticket` a player's ckey to the
/// newest ticket the acting bot sees of them.
pub fn request(
    verb: &str,
    args: &[String],
    ckey: &dyn Fn(&str) -> Option<String>,
    ticket: &dyn Fn(&str) -> Option<u32>,
) -> Result<AdminRequest, String> {
    let bot = |i: usize| -> Result<String, String> {
        let name = args
            .get(i)
            .ok_or_else(|| format!("admin {verb}: which bot?"))?;
        ckey(name).ok_or_else(|| format!("admin {verb}: {name:?} is not a bot"))
    };
    let text = |i: usize| -> Result<String, String> {
        args.get(i)
            .cloned()
            .ok_or_else(|| format!("admin {verb}: text?"))
    };
    let optional = |i: usize| args.get(i).cloned().unwrap_or_default();
    let arity = |n: usize| -> Result<(), String> {
        if args.len() > n {
            Err(format!("admin {verb}: too many arguments"))
        } else {
            Ok(())
        }
    };
    use AdminRequest as R;
    let request = match verb {
        "ahelp" => R::Adminhelp(text(0)?),
        "asay" => R::Asay(text(0)?),
        "readmin" => R::Readmin,
        "deadmin" => R::Deadmin,
        "players" => R::Players,
        "pm" => R::Pm {
            ckey: bot(0)?,
            text: text(1)?,
        },
        "ticket" => {
            let player = bot(0)?;
            let state = match text(1)?.as_str() {
                "close" => TicketState::Closed,
                "resolve" => TicketState::Resolved,
                "reopen" => TicketState::Open,
                other => {
                    return Err(format!(
                        "admin ticket: {other:?} is not close|resolve|reopen"
                    ));
                }
            };
            let ticket =
                ticket(&player).ok_or_else(|| format!("admin ticket: no ticket of {player}"))?;
            R::SetTicketState { ticket, state }
        }
        "jump" => R::JumpTo(bot(0)?),
        "get" => R::Get(bot(0)?),
        "follow" => R::Follow(bot(0)?),
        "kick" => R::Kick {
            ckey: bot(0)?,
            reason: optional(1),
        },
        "mute" | "unmute" => R::Mute {
            ckey: bot(0)?,
            mute: verb == "mute",
        },
        "ban" => R::Ban {
            ckey: bot(0)?,
            reason: optional(1),
        },
        "unban" => R::Unban(bot(0)?),
        "aghost" => R::Aghost,
        "find" => R::FindPrototypes(text(0)?),
        "spawn" => R::Spawn(text(0)?),
        "heal" => R::Rejuvenate(bot(0)?),
        "smite" => R::Smite {
            ckey: bot(0)?,
            smite: Smite::Lightning,
        },
        "events" => R::Events,
        "event" => R::ForceEvent {
            name: text(0)?,
            announce: None,
        },
        "endround" => R::EndRound,
        "timescale" => R::TimeScale(
            text(0)?
                .parse()
                .map_err(|_| "admin timescale: a number".to_string())?,
        ),
        "announce" => R::Announce {
            title: text(0)?,
            text: text(1)?,
        },
        "antag" => R::MakeAntag {
            ckey: bot(0)?,
            role: match text(1)?.as_str() {
                "traitor" => AntagRole::Traitor,
                "brother" => AntagRole::Brother,
                "changeling" => AntagRole::Changeling,
                "vampire" => AntagRole::Vampire,
                "cult" | "cultist" => AntagRole::Cultist,
                "heretic" => AntagRole::Heretic,
                "headrev" => AntagRole::HeadRev,
                "rev" => AntagRole::Revolutionary,
                other => {
                    return Err(format!(
                        "admin antag: {other:?} is not \
                         traitor|brother|changeling|vampire|cult|heretic|headrev|rev"
                    ));
                }
            },
        },
        "dynamic" => R::Dynamic,
        "midround" => R::ForceMidround(text(0)?),
        other => return Err(format!("unknown admin verb {other:?}; verbs: {VERBS}")),
    };
    arity(match verb {
        "pm" | "ticket" | "kick" | "ban" | "announce" | "antag" => 2,
        "readmin" | "deadmin" | "players" | "aghost" | "events" | "endround" | "dynamic" => 0,
        _ => 1,
    })?;
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(verb: &str, args: &[&str]) -> Result<AdminRequest, String> {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        request(
            verb,
            &args,
            &|name| (name == "bob").then(|| "casebob".to_owned()),
            &|ckey| (ckey == "casebob").then_some(3),
        )
    }

    #[test]
    fn verbs_build_requests_with_ckeys() {
        assert_eq!(
            build("kick", &["bob", "flood"]),
            Ok(AdminRequest::Kick {
                ckey: "casebob".into(),
                reason: "flood".into()
            })
        );
        assert_eq!(
            build("ticket", &["bob", "resolve"]),
            Ok(AdminRequest::SetTicketState {
                ticket: 3,
                state: TicketState::Resolved
            })
        );
        assert_eq!(build("aghost", &[]), Ok(AdminRequest::Aghost));
        assert!(build("kick", &["carol"]).is_err());
        assert!(build("aghost", &["bob"]).is_err());
        assert!(build("nope", &[]).is_err());
    }
}
