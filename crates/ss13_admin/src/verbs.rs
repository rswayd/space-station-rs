//! Admin verbs past the player panel: the permissions panel (tg `edit_admin_permissions`, round
//! only), BandaStation's "Man Up" and admin sounds, and the atmos debug verbs. [`crate::server`]
//! checked the request's rights before any of these runs; each re-reads the world at the moment
//! of effect.
// shortcut: permission changes live in the server `App` (tg's "temporary" changes); saving them
// ("permanent", tg `admin` table) waits for the database.

use std::collections::HashSet;

use bevy::prelude::*;
use ss13_atmos::Atmos;
use ss13_game::ghost::Ghost;
use ss13_game::health::Health;
use ss13_game::server::PlayerMob;
use ss13_mapload::Opaque;
use ss13_net::admin::server::SendAdminResponse;
use ss13_net::server::ClientToken;
use ss13_net::sound::server::SendSound;
use ss13_shared::TilePos;
use ss13_shared::admin::rights::*;
use ss13_shared::admin::{AdminEntry, AdminResponse, SoundScope};
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_shared::protocol::SoundEvent;
use ss13_world::SpatialIndex;

use crate::rights::{ckey, has, may_edit_admin, resolve_rank, rights_text};
use crate::server::{
    AdminSession, Admins, AnswerResult, admin_action, chat, client_by_ckey, client_ckey, done,
    identity_trusted, mob_pos, own_mob, rights_of, sanitize_text, target_mob,
};
use crate::tickets::Tickets;

/// BandaStation `manup1.ogg`.
pub const MAN_UP_SOUND: &str = "modular_bandastation/admin/sound/manup1.ogg";
const MAN_UP_TEXT: &str = "Man up. Deal with it. Move on.";
/// tg `DEFAULT_SIGHT_DISTANCE`: "Play Sound in View" range.
const SIGHT_DISTANCE: f32 = 7.0;
/// Rank texts, sound paths and gas strings, in characters.
const MAX_TEXT: usize = 200;

/// The permissions panel's rows, sorted by ckey.
pub fn admin_list(world: &mut World) -> AdminResponse {
    let admins = world.resource::<Admins>();
    let mut rows: Vec<(String, String, u32)> = admins
        .holders
        .iter()
        .map(|(key, h)| (key.clone(), h.rank.clone(), h.rights))
        .collect();
    let ranks = admins.ranks.iter().map(|r| r.name.clone()).collect();
    rows.sort();
    let admins = rows
        .into_iter()
        .map(|(key, rank, rights)| AdminEntry {
            online: client_by_ckey(world, &key).is_some(),
            ckey: key,
            rank,
            rights,
        })
        .collect();
    AdminResponse::AdminList { admins, ranks }
}

/// The editor's `can_edit_rights_flags`: its listing's ranks' `Edit` flags.
fn editor_flags(world: &World, client: Entity) -> u32 {
    let key = client_ckey(world, client);
    world
        .resource::<Admins>()
        .holders
        .get(&key)
        .map_or(0, |h| h.edit)
}

/// tg `add_admin` + `change_admin_rank` (temporary): lists `target` with `rank`, and gives an
/// online target its session at once.
pub fn set_admin_rank(
    world: &mut World,
    client: Entity,
    key: &str,
    target: &str,
    rank: &str,
) -> AnswerResult {
    let target = ckey(target);
    if target.is_empty() {
        return Err("Пустой ckey.".into());
    }
    let rank = sanitize_text(rank, MAX_TEXT);
    let holder = resolve_rank(&rank, &world.resource::<Admins>().ranks)?;
    let current = world
        .resource::<Admins>()
        .holders
        .get(&target)
        .map(|h| h.rights);
    may_edit_admin(
        rights_of(world, client),
        editor_flags(world, client),
        current,
        Some(holder.rights),
    )?;
    world
        .resource_mut::<Admins>()
        .holders
        .insert(target.clone(), holder.clone());
    if let Some(target_client) = client_by_ckey(world, &target) {
        let rights = if world.get::<AdminSession>(target_client).is_some() {
            let mut session = world
                .get_mut::<AdminSession>(target_client)
                .expect("checked above");
            session.rank.clone_from(&holder.rank);
            session.rights = holder.rights;
            Some(session.active_rights())
        } else if identity_trusted(world) {
            let session = AdminSession {
                ckey: target.clone(),
                rank: holder.rank.clone(),
                rights: holder.rights,
                active: has(holder.rights, R_AUTOADMIN),
            };
            let rights = session.active_rights();
            world.entity_mut(target_client).insert(session);
            Some(rights)
        } else {
            // As at login: an unauthenticated token gets no session (see `crate::server`).
            None
        };
        if let Some(rights) = rights {
            world.write_message(SendAdminResponse {
                client: target_client,
                response: AdminResponse::Rights {
                    rank: holder.rank.clone(),
                    rights,
                },
            });
        }
    }
    world.resource_mut::<Tickets>().dirty = true;
    let verb = if current.is_some() {
        "changed the rank of"
    } else {
        "added"
    };
    admin_action(
        world,
        &format!(
            "{key} {verb} {target} to {} ({}) temporarily",
            holder.rank,
            rights_text(holder.rights)
        ),
    );
    done(format!("{target}: {}", holder.rank))
}

/// tg `remove_admin` (temporary): the listing and an online target's session go.
pub fn remove_admin(world: &mut World, client: Entity, key: &str, target: &str) -> AnswerResult {
    let target = ckey(target);
    let current = world
        .resource::<Admins>()
        .holders
        .get(&target)
        .map(|h| h.rights)
        .ok_or_else(|| format!("{target} не администратор."))?;
    may_edit_admin(
        rights_of(world, client),
        editor_flags(world, client),
        Some(current),
        None,
    )?;
    world.resource_mut::<Admins>().holders.remove(&target);
    if let Some(target_client) = client_by_ckey(world, &target)
        && world.get::<AdminSession>(target_client).is_some()
    {
        world.entity_mut(target_client).remove::<AdminSession>();
        // An empty rank: no longer listed (the client hides the panel).
        world.write_message(SendAdminResponse {
            client: target_client,
            response: AdminResponse::Rights {
                rank: String::new(),
                rights: 0,
            },
        });
    }
    world.resource_mut::<Tickets>().dirty = true;
    admin_action(
        world,
        &format!("{key} removed {target} from the admins list temporarily"),
    );
    done(format!("{target} removed"))
}

fn send_sound(world: &mut World, client: Entity, file: &str, volume: f32) {
    world.write_message(SendSound {
        client,
        sound: SoundEvent {
            file: file.to_owned(),
            volume,
            speed: 1.0,
            offset: None,
        },
    });
}

fn man_up_to(world: &mut World, client: Entity) {
    chat(
        world,
        vec![client],
        ChatMessage::simple(ChatChannel::System, Span::BoldNotice, MAN_UP_TEXT),
    );
    send_sound(world, client, MAN_UP_SOUND, 100.0);
}

/// BandaStation "Man Up" (`/mob/living` targets).
pub fn man_up(world: &mut World, key: &str, target: &str) -> AnswerResult {
    let (target_client, mob) = target_mob(world, target)?;
    if world.get::<Health>(mob).is_none() || world.get::<Ghost>(mob).is_some() {
        return Err("Цель не живой моб.".into());
    }
    man_up_to(world, target_client);
    let target_key = crate::server::key_name(world, target_client);
    admin_action(
        world,
        &format!("{key} told {target_key} to man up and deal with it."),
    );
    done(format!("man up: {target_key}"))
}

/// BandaStation "Global Man Up": every connected player (tg `GLOB.player_list`); the panel asks
/// for the confirmation.
pub fn global_man_up(world: &mut World, key: &str) -> AnswerResult {
    let clients: Vec<Entity> = world
        .query_filtered::<Entity, With<ClientToken>>()
        .iter(world)
        .collect();
    for &client in &clients {
        man_up_to(world, client);
    }
    admin_action(
        world,
        &format!("{key} told everyone to man up and deal with it."),
    );
    done(format!("man up: {} players", clients.len()))
}

/// An asset sound path a client can play: `sound/...` or `modular_bandastation/...`, `.ogg`,
/// relative, no `..`.
pub fn valid_sound_path(file: &str) -> bool {
    file.len() <= MAX_TEXT
        && (file.starts_with("sound/") || file.starts_with("modular_bandastation/"))
        && file.ends_with(".ogg")
        && file
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.'))
        && !file.split('/').any(|part| part.is_empty() || part == "..")
}

/// BandaStation "Play Z-level Sound" and "Play Sound in View".
// shortcut: no per-client `sound_midi` / `admin_music_volume` scaling (those prefs are not
// ported); the volume plays as given.
pub fn play_sound(
    world: &mut World,
    client: Entity,
    key: &str,
    file: &str,
    volume: u8,
    scope: SoundScope,
) -> AnswerResult {
    if !valid_sound_path(file) {
        return Err("Путь к звуку: sound/… или modular_bandastation/…, .ogg.".into());
    }
    let volume = volume.clamp(1, 100);
    let own = own_mob(world, client)?;
    let pos = mob_pos(world, own)?;
    let seen: Option<HashSet<TilePos>> = match scope {
        SoundScope::ZLevel => None,
        SoundScope::View => {
            let index = world.resource::<SpatialIndex>();
            Some(ss13_game::sound::hearing_view(
                pos,
                SIGHT_DISTANCE,
                |p| {
                    index
                        .on_tile(p)
                        .iter()
                        .any(|&e| world.get::<Opaque>(e).is_some())
                },
                |p| index.in_bounds(p),
            ))
        }
    };
    let hearers: Vec<Entity> = world
        .query::<(&PlayerMob, &TilePos)>()
        .iter(world)
        .filter(|&(_, p)| match &seen {
            None => p.z == pos.z,
            Some(seen) => seen.contains(p),
        })
        .map(|(player, _)| player.client)
        .collect();
    for &hearer in &hearers {
        send_sound(world, hearer, file, f32::from(volume));
    }
    let what = match scope {
        SoundScope::ZLevel => format!("a z-level sound {file} on level {}", pos.z),
        SoundScope::View => format!("a view-sound {file}"),
    };
    admin_action(world, &format!("{key} played {what}"));
    done(format!("{file}: {} players", hearers.len()))
}

/// tg "Debug Air Status" (`atmos_scan` of the own tile) with the Atmos Control Panel counters.
pub fn air_status(world: &mut World, client: Entity) -> AnswerResult {
    let own = own_mob(world, client)?;
    let pos = mob_pos(world, own)?;
    let atmos = world
        .get_resource::<Atmos>()
        .ok_or("Атмосфера не запущена.")?;
    let scan = atmos.scan(pos, "тайл");
    let counters = format!(
        "SSair: активных тайлов {}, возбуждённых групп {}, очагов огня {}",
        atmos.active_turfs(),
        atmos.excited_groups(),
        atmos.hotspots()
    );
    match scan {
        Some(message) => chat(world, vec![client], message),
        None => crate::server::tell(
            world,
            client,
            ChatChannel::System,
            Span::Warning,
            "На этом тайле нет воздуха.",
        ),
    }
    done(counters)
}

/// tg `atmos_spawn_air` on the own tile.
pub fn spawn_air(world: &mut World, client: Entity, key: &str, text: &str) -> AnswerResult {
    let text = sanitize_text(text, MAX_TEXT);
    let own = own_mob(world, client)?;
    let pos = mob_pos(world, own)?;
    let result = world
        .get_resource_mut::<Atmos>()
        .ok_or("Атмосфера не запущена.")?
        .spawn_air(pos, &text);
    match result {
        Err(e) => return Err(format!("Газовая строка: {e}")),
        Ok(false) => return Err("У тайла нет своего воздуха.".into()),
        Ok(true) => {}
    }
    admin_action(
        world,
        &format!(
            "{key} spawned air {text} at ({}, {}, {})",
            pos.x, pos.y, pos.z
        ),
    );
    done(format!("воздух: {text}"))
}

/// tg "Give AI Controller" on the living mobs of the own tile.
pub fn give_ai(
    world: &mut World,
    client: Entity,
    key: &str,
    template: &str,
    make_hostile: bool,
) -> AnswerResult {
    let template = sanitize_text(template, MAX_TEXT);
    let own = own_mob(world, client)?;
    let pos = mob_pos(world, own)?;
    let n = ss13_game::mobs::ai::admin::give(world, pos, own, &template, make_hostile)?;
    admin_action(
        world,
        &format!(
            "{key} gave the AI template {template}{} to {n} mob(s) at ({}, {}, {})",
            if make_hostile { " (hostile)" } else { "" },
            pos.x,
            pos.y,
            pos.z
        ),
    );
    done(format!("ИИ «{template}»: {n}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_paths_stay_in_the_assets() {
        assert!(valid_sound_path("sound/misc/server-ready.ogg"));
        assert!(valid_sound_path(MAN_UP_SOUND));
        assert!(!valid_sound_path("/etc/passwd.ogg"));
        assert!(!valid_sound_path("sound/../secret.ogg"));
        assert!(!valid_sound_path("sound//a.ogg"));
        assert!(!valid_sound_path("sound/a.wav"));
        assert!(!valid_sound_path("icons/a.ogg"));
        assert!(!valid_sound_path("sound/a b.ogg"));
    }
}
