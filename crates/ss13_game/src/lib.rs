//! First playable loop: a connected client gets a mob at a spawn landmark, walks it with
//! `ClientIntent::Move`, and its camera follows it.
//!
//! - `server` (feature `server`): marks map entities for replication, spawns and despawns player
//!   mobs, validates move intents, moves each client's `Viewer` with its mob.
//! - `click` (feature `server`): validates world clicks once (sight, reach) for doors and items.
//! - `doors` (feature `server`): airlocks and other doors open on bumps and clicks, autoclose.
//! - `human` (feature `server`): player mobs drawn as tg humans (body parts, eyes, clothes, hair,
//!   held items).
//! - `inventory` (feature `server`): hands, tg's human equipment slots, pickup and drop, ID
//!   access.
//! - `jobs`, `outfit` (feature `server`): joinable jobs, the latejoin menu, job spawn landmarks,
//!   outfits and ID trims, character names.
//! - `round` (feature `server`): the lobby (new players, ready, observe, job preferences), tg's
//!   ticker states, round start job division, round end report and reboot; `lobby` holds the
//!   interfaces both halves share, `lobby_client` (feature `client`) draws the lobby screen and
//!   the job preferences window.
//! - `round_news` (feature `server`): the round end news story on the station's fate;
//!   `lobby_music` (feature `server`): the lobby's title music and the round end song;
//!   `join_queue` (feature `server`): the latejoin queue over the population cap.
//! - `antag` (feature `server`; shared interfaces and `antag::client` with `client`): roundstart
//!   antagonists (BandaStation's dynamic), the traitor, objectives, the uplink, round-end report.
//! - `power` (feature `server`): tg areas, APCs, powernets, SMES, machine loads; unpowered doors
//!   stay shut, lights go to emergency light or dark.
//! - `closets` (feature `server`): closets, lockers and crates open, close, lock and hold their
//!   contents (mapped and from `PopulateContents`).
//! - `clothing`: what worn clothing hides, masks, jumpsuit styles, hoods, clothing damage, suit
//!   sensors and the crew monitor, accessories (server); the crew monitor window (client).
//! - `storage` (feature `server`): storage items (backpacks, boxes) with tg weight and slot
//!   limits, contents programs, the open storage view; `storage_client` (feature `client`)
//!   draws it as a slot grid.
//! - `health` (feature `server`): damage, stat (crit, death), lying bodies.
//! - `breath`, `internals` (feature `server`): tg `Life()` breathing from the tile air or an
//!   internals tank, pressure and body temperature damage, the screen alerts; gas tanks.
//!   `breath_client` (feature `client`): the alert column.
//! - `combat` (feature `server`): combat mode, punches, hugs, attacks with held items.
//!   `martial` (feature `server`): martial arts (CQC, sleeping carp, boxing, wrestling, judo),
//!   their manuals, gloves and belts.
//! - `status_effects` (feature `server`): tg stuns, knockdowns, paralysis, immobilization,
//!   stamina and stamcrit, confusion; the traits moves, clicks and hands read. `restraints`
//!   (feature `server`): handcuffs, resisting out, taking them off. `security` (feature
//!   `server`): stun batons, flashes, the shove. `brig`: brig cell timers and the prisoner
//!   management console (`BrigTimer`, `PrisonerManagement`).
//! - `ghost` (feature `server`): a dead player's ghost, invisible to the living. `ghost_verbs`
//!   (feature `server`) and `ghost_menu`: its HUD, Orbit/Observe, the ghost and spawners menus,
//!   Boo, Possess, respawn.
//! - `atmos` (feature `server`): gas overlays replicate, gas analyzers print their readout.
//!   `atmos_machinery` (feature `server`): pipe networks, vents, scrubbers, pumps, valves,
//!   filters, mixers, thermomachines, canisters and portable machines, air alarms and their
//!   tgui windows.
//! - `fire` (feature `server`): open flames light hotspots, fire on mobs (fire stacks, stop drop
//!   and roll) and flammable objects, gas tank reactions, space wind. `spacemove` (feature
//!   `server`): zero gravity: pushing off, drifting, the weightless alert. `stumble` (feature
//!   `server`): BandaStation's tripping from running too fast.
//! - `pull` (feature `server`): ctrl+click pulling, pulled things following, grabs and resisting,
//!   the pull/throw HUD's server half; `bump` (feature `server`): walking into a mob swaps places
//!   or pushes it, walking into an unanchored object pushes it; `throw` (feature `server`): throw mode, flying items,
//!   hits and catches; `examine` (feature `server`): shift+click examine text; `speech` (feature
//!   `server`): the mob's stat for `ss13_chat` (crit whispers, deadchat, emote checks).
//!   `interact_client` (feature `client`): the pull, resist and throw keys and HUD buttons.
//! - `radio` (feature `server`): headsets, intercoms and station bounced radios: talking into
//!   them (`;`, `:h`, `:x`, `:i`, `:r`/`:l`), relaying nearby speech, receiving by frequency.
//! - `hud`: the inventory and health HUD data both halves share; `inventory_client` and
//!   `health_client` (feature `client`) draw them and send the inventory keys (X, Q), the combat
//!   mode key (F) and slot clicks.
//! - `client` (feature `client`): keyboard -> move intents, camera on the controlled mob, BYOND-style
//!   gliding between tiles, dev certificate pickup.
//!
//! - `shuttle` (feature `server`): docking ports, shuttle templates and moves, the emergency
//!   shuttle's modes as the round's end, the communications console's server half; `comms`
//!   holds the console window's data, `comms_client` (feature `client`) draws it.
//! - `security_level` (feature `server`): the station's security level, its announcements and
//!   consumers, evac status displays, the keycard authentication device; `keycard` holds the
//!   device window's data, `keycard_client` (feature `client`) draws it.
//! - `vending` (feature `server`): vending machines, the tgui `Vending` window's server half.
//! - `disposals` (feature `server`): disposal pipes, bins and chutes (tgui `DisposalUnit`),
//!   holders travelling to outlets, mail sorting, destination taggers and parcels.
//! - `wires` (feature `server`): tg `/datum/wires` on airlocks, APCs and vending machines
//!   (colors per round, cut, mend, pulse, the tgui `Wires` window's server half), shocks.
//! - `tools`, `construction`, `obj_damage` (feature `server`): tool behaviours, `use_tool` and
//!   `do_after`, welders and stacks; floors, girders, walls, grilles, windows and machine
//!   anchoring with tg's tool chains, turf changes (`ChangeTurf`); structure integrity, item
//!   hits on structures and their debris.
//! - `medical`, `health_scan` (feature `server`): the zone selector's server half, medical
//!   stacks healing bodyparts, gauze, health analyzers (tg `healthscan`), CPR; `zone`: body
//!   zones, the selector's hit map and numpad keys.
//! - `chem` (feature `server`): reagent holders on containers and mobs (`ss13_reagents`),
//!   pouring, drinking, syringes, pills, reactions over time, metabolism, fill overlays.
//! - `species` (feature `server`): tg `/datum/species` (BandaStation's too): traits, organ
//!   sets, limb sprites, damage and temperature modifiers, exotic blood, `set_species`.
//! - `hunger` (feature `server`): nutrition, the stomach's `handle_hunger`, obesity, the hunger
//!   bar's server half; `hunger_client` (feature `client`) draws the bar. `food` (feature
//!   `server`): food items (`/obj/item/food`): eating and feeding bite by bite, tastes, trash,
//!   tool processing (slicing). `kitchen` (feature `server`): the microwave, the food processor
//!   and the reagent grinder.
//! - `mood` (feature `server`): mood events, sanity and its effects, the mood face's server half;
//!   `mood_client` (feature `client`) draws the face. `quirks` (feature `server`): character
//!   quirks, their points and effects. `personality` (feature `server`): character
//!   personalities and their mood reactions. `hallucination` (feature `server`): the hallucination
//!   status effect and the hallucinations only their victim perceives.
//! - `cargo` (feature `server`; the window's data shared, `cargo::client` with `client`): the
//!   economy (department budgets, bank accounts on ID cards, paydays), supply packs and orders,
//!   the supply shuttle (buying on the way in, selling on arrival at CentCom) and the supply
//!   consoles' tgui `Cargo` window.
//! - `botany` (feature `server`): hydroponics trays, seeds, plant genes, the harvest, the seed
//!   extractor and the plant analyzer.
//! - `science` (feature `server`; the windows' data shared, `science::client` with `client`):
//!   the station techweb, research points from R&D servers, the R&D console's tgui `Techweb`,
//!   the protolathe, circuit imprinter and departmental techfabs (tgui `Fabricator`) with the ore
//!   silo's material storage, the destructive analyzer, and xenobiology's slimes and extracts.
//! - `silicon` (feature `server`; the windows' data shared, `silicon::client` with `client`):
//!   security cameras and the camera network, the camera console, the AI (core, eye, AI
//!   clicks), cyborgs (models, cell, recharging stations), laws, binary chat.
//! - `ui_sprites` (feature `client`): atlas sprites inside tgui windows (`UiSprite`).
//! - `drag` (feature `server`): click-drags (tg `MouseDrop`): storages, the strip menu, closets.
//!   `strip` (feature `server`): the strip menu. `loot` (feature `server`): alt-clicks
//!   (`click_alt`, the loot panel). `point` (feature `server`): shift+middle pointing. `panels`:
//!   the strip menu and loot panel data both halves share; `panels_client` (feature `client`)
//!   draws them.
//!
//! - `mecha` (feature `server`; the fabricator window's data shared, `mecha::client` with
//!   `client`): exosuits (entering, driving, equipment, damage, wreckage), building them from a
//!   chassis and the exosuit fabricator (tgui `ExosuitFabricator`).
//! - `modsuit` (server half with `server`, the `MODsuit` window with `client`): MODsuit control
//!   units, their parts, cores and modules.
//!
//! - `service` (server half with `server`, the paper and book windows with `client`): paper,
//!   pens, stamps, paper bins, the hand labeler, books, bookcases, the bible, dice, squeaky
//!   things, desk bells and the mime's vow.
//!
//! - `forensics` (server half with `server`, the scanner window with `client`): fingerprints,
//!   fibers, blood DNA and hidden prints on atoms, the detective's forensic scanner and its
//!   report, evidence bags.
//!
//! - `mobs` (feature `server`): tg basic mobs (`/mob/living/basic`) from map placements and
//!   spawns, their environment, fighting them, and their AI controllers (behavior trees).
//!
//! - `emp` (feature `server`): tg `empulse()` and `emp_act`: pulses, protection, contents, and
//!   each ported family's EMP effect. `emag` (feature `server`): the emag and the doorjack,
//!   `can_emag` and the ported families' `emag_act`. `grenades` (feature `server`): arming,
//!   fuses and the flashbang, EMP, smoke, explosive, frag and cluster grenades.
//!   `effects` (feature `server`): effect systems: sparks, steam, smoke and chemical smoke,
//!   foam and foamed metal, trails. `weather` (feature `server`): `SSweather` and the station
//!   weathers (radiation storm, the floor is lava, void storm) and lavaland's ash storms. `lava`
//!   (feature `server`): lava turfs burn what touches them.
//!
//! - `events` (feature `server`): tg `SSevents`, the random event scheduler, and the round
//!   events that act on ported systems (grid check, electrical storm, communications blackout,
//!   meteors, radiation storm, brand intelligence, market crash, false alarm, carp migration).
//!
//! - `prediction` (feature `client`): the own mob walks at once on input; the server's acks
//!   reconcile it.
//!
//! Walking is one `Move { dir, seq }` per step: while a direction is held the client predicts a
//! step whenever its copy of the run cooldown allows and sends it once (reliable, ordered). The
//! server applies each `seq` once, when the mob's `Mover` cooldown allows, and acks it with the
//! mob's authoritative tile. Spam therefore cannot walk faster than the cooldown, and releasing
//! the key stops the mob after the steps already shown.

use std::io;
use std::path::{Path, PathBuf};

pub mod actions;
#[cfg(feature = "client")]
pub mod actions_client;
#[cfg(feature = "server")]
pub mod actionspeed;
#[cfg(feature = "server")]
pub mod addictions;
#[cfg(feature = "server")]
pub mod afflictions;
#[cfg(feature = "server")]
pub mod airlock_construction;
#[cfg(feature = "server")]
pub mod airlock_controllers;
#[cfg(feature = "server")]
pub mod airlock_cycle;
#[cfg(feature = "server")]
pub mod airlock_security;
#[cfg(feature = "server")]
pub mod alt_appearance;
#[cfg(feature = "server")]
pub mod animation;
#[cfg(feature = "client")]
pub mod animation_client;
#[cfg(feature = "server")]
pub mod announce;
pub mod antag;
#[cfg(feature = "server")]
pub mod armor;
#[cfg(feature = "server")]
pub mod arrivals;
/// Shared window data; `assemblies::server` with `server`, `assemblies::client` with `client`.
pub mod assemblies;
#[cfg(feature = "server")]
pub mod atmos;
#[cfg(feature = "server")]
pub mod atmos_exposure;
#[cfg(feature = "server")]
pub mod atmos_machinery;
#[cfg(feature = "server")]
pub mod away_shuttles;
#[cfg(feature = "server")]
pub mod balloon;
#[cfg(feature = "client")]
pub mod balloon_client;
#[cfg(feature = "server")]
pub mod bar;
#[cfg(feature = "server")]
pub mod barsign;
#[cfg(feature = "server")]
pub mod blackbox;
#[cfg(feature = "server")]
pub mod botany;
pub mod bots;
#[cfg(feature = "server")]
pub mod breath;
#[cfg(feature = "client")]
pub mod breath_client;
pub mod brig;
#[cfg(feature = "server")]
pub mod buffs;
#[cfg(feature = "server")]
pub mod bump;
#[cfg(feature = "server")]
pub mod cannons;
pub mod cargo;
#[cfg(feature = "server")]
pub mod chem;
#[cfg(feature = "server")]
pub mod chem_dispenser;
#[cfg(feature = "server")]
pub mod chem_heater;
#[cfg(feature = "server")]
pub mod chem_master;
#[cfg(feature = "server")]
pub mod click;
#[cfg(feature = "server")]
pub mod click_handlers;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod closets;
pub mod clothing;
#[cfg(feature = "server")]
pub mod coffee;
#[cfg(feature = "server")]
pub mod combat;
pub mod comms;
#[cfg(feature = "client")]
pub mod comms_client;
#[cfg(feature = "server")]
pub mod comms_server;
#[cfg(feature = "server")]
pub mod construction;
#[cfg(feature = "server")]
pub mod cooking;
#[cfg(feature = "server")]
pub mod crayons;
#[cfg(feature = "server")]
pub mod curses;
#[cfg(feature = "server")]
pub mod death;
#[cfg(feature = "server")]
pub mod debuffs;
#[cfg(feature = "server")]
pub mod decay;
#[cfg(feature = "server")]
pub mod decor;
pub mod defenses;
#[cfg(feature = "server")]
pub mod deployable_turrets;
#[cfg(feature = "server")]
pub mod device_wires;
#[cfg(feature = "server")]
pub mod devices;
#[cfg(feature = "server")]
pub mod displaycase;
#[cfg(feature = "server")]
pub mod disposals;
#[cfg(feature = "server")]
pub mod door_structures;
#[cfg(feature = "server")]
pub mod doors;
#[cfg(feature = "server")]
pub mod drag;
#[cfg(feature = "server")]
pub mod dreams;
#[cfg(feature = "server")]
pub mod effects;
#[cfg(feature = "server")]
pub mod emag;
#[cfg(feature = "server")]
pub mod emojis;
#[cfg(feature = "server")]
pub mod emp;
#[cfg(feature = "server")]
pub mod eng_machines;
pub mod escape_menu;
#[cfg(feature = "client")]
pub mod escape_menu_client;
#[cfg(feature = "server")]
pub mod escape_pods;
#[cfg(feature = "server")]
pub mod events;
#[cfg(feature = "server")]
pub mod examine;
#[cfg(feature = "server")]
pub mod explosions;
#[cfg(feature = "server")]
pub mod extinguisher;
#[cfg(feature = "server")]
pub mod falsewall;
#[cfg(feature = "server")]
pub mod fire;
#[cfg(feature = "server")]
pub mod fireplace;
pub mod fishing;
#[cfg(feature = "client")]
pub mod flatpacker_client;
pub mod flatpacker_ui;
#[cfg(feature = "server")]
pub mod flora;
#[cfg(feature = "server")]
pub mod food;
#[cfg(feature = "server")]
pub mod footsteps;
pub mod forensics;
#[cfg(feature = "server")]
pub mod fov;
#[cfg(feature = "client")]
pub mod fullscreen_client;
#[cfg(feature = "server")]
pub mod furniture;
#[cfg(feature = "server")]
pub mod genetics;
#[cfg(feature = "server")]
pub mod ghost;
pub mod ghost_menu;
#[cfg(feature = "client")]
pub mod ghost_menu_client;
#[cfg(feature = "server")]
pub mod ghost_poll;
#[cfg(feature = "server")]
pub mod ghost_roles;
#[cfg(feature = "server")]
pub mod ghost_verbs;
#[cfg(feature = "server")]
pub mod gibber;
#[cfg(feature = "server")]
pub mod gifts;
#[cfg(feature = "server")]
pub mod grenades;
#[cfg(feature = "server")]
pub mod gun_attachments;
#[cfg(feature = "server")]
pub mod gunhud;
#[cfg(feature = "client")]
pub mod gunhud_client;
#[cfg(feature = "server")]
pub mod gunpoint;
#[cfg(feature = "server")]
pub mod guns;
#[cfg(feature = "server")]
pub mod guns_special;
#[cfg(feature = "server")]
pub mod gym;
#[cfg(feature = "server")]
pub mod hallucination;
#[cfg(feature = "server")]
pub mod health;
#[cfg(feature = "client")]
pub mod health_client;
#[cfg(feature = "server")]
pub mod health_scan;
#[cfg(feature = "server")]
pub mod holodeck;
#[cfg(feature = "server")]
pub mod holosign;
pub mod hud;
#[cfg(feature = "server")]
pub mod human;
#[cfg(feature = "server")]
pub mod hunger;
#[cfg(feature = "client")]
pub mod hunger_client;
#[cfg(feature = "server")]
pub mod icecream;
#[cfg(feature = "server")]
pub mod id_access;
#[cfg(feature = "server")]
pub mod implants;
#[cfg(feature = "server")]
pub mod inducer;
#[cfg(feature = "server")]
pub mod instruments;
#[cfg(feature = "client")]
pub mod instruments_client;
pub mod instruments_ui;
#[cfg(feature = "client")]
pub mod interact_client;
#[cfg(feature = "server")]
pub mod internals;
#[cfg(feature = "server")]
pub mod inventory;
#[cfg(feature = "client")]
pub mod inventory_client;
#[cfg(feature = "server")]
pub mod janitor;
#[cfg(feature = "server")]
pub mod jetpack;
#[cfg(feature = "server")]
pub mod jobs;
#[cfg(feature = "server")]
pub mod join_queue;
pub mod keycard;
#[cfg(feature = "client")]
pub mod keycard_client;
#[cfg(feature = "server")]
pub mod kitchen;
pub mod language_menu;
#[cfg(feature = "client")]
pub mod language_menu_client;
#[cfg(feature = "server")]
pub mod lava;
pub mod lobby;
#[cfg(feature = "client")]
pub mod lobby_client;
#[cfg(feature = "server")]
pub mod lobby_music;
#[cfg(feature = "server")]
pub mod lockbox;
#[cfg(feature = "server")]
pub mod locked_pickup;
#[cfg(feature = "server")]
pub mod loot;
#[cfg(feature = "server")]
pub mod machine_frame;
#[cfg(feature = "server")]
pub mod machine_wires;
#[cfg(feature = "client")]
pub mod manufactorio_client;
pub mod manufactorio_ui;
#[cfg(feature = "server")]
pub mod martial;
pub mod mecha;
#[cfg(feature = "server")]
pub mod medical;
#[cfg(feature = "server")]
pub mod mind;
pub mod mining;
#[cfg(feature = "server")]
pub mod mining_shuttles;
#[cfg(feature = "server")]
pub mod misc_items;
#[cfg(feature = "server")]
pub mod mob_holder;
#[cfg(feature = "server")]
pub mod mobs;
pub mod modpc;
pub mod modsuit;
#[cfg(feature = "client")]
pub mod modular_shield_client;
pub mod modular_shield_ui;
#[cfg(feature = "server")]
pub mod mood;
#[cfg(feature = "client")]
pub mod mood_client;
#[cfg(feature = "server")]
pub mod multiz;
pub mod newscaster;
#[cfg(feature = "client")]
pub mod newscaster_client;
#[cfg(feature = "server")]
pub mod newscaster_server;
#[cfg(feature = "server")]
pub mod noncrew_jobs;
#[cfg(feature = "server")]
pub mod ntnet_relay;
#[cfg(feature = "server")]
pub mod obj_damage;
pub mod operating;
#[cfg(feature = "client")]
pub mod operating_client;
#[cfg(feature = "server")]
pub mod outfit;
pub mod panels;
#[cfg(feature = "client")]
pub mod panels_client;
#[cfg(feature = "server")]
pub mod papershredder;
#[cfg(feature = "server")]
pub mod password_door;
#[cfg(feature = "server")]
pub mod personality;
#[cfg(feature = "server")]
pub mod pixel_shift;
#[cfg(feature = "server")]
pub mod plumbing;
#[cfg(feature = "server")]
pub mod plumbing_net;
#[cfg(feature = "server")]
pub mod point;
#[cfg(feature = "server")]
pub mod power;
#[cfg(feature = "client")]
pub mod prediction;
#[cfg(feature = "server")]
pub mod projectile;
#[cfg(feature = "server")]
pub mod pull;
#[cfg(feature = "server")]
pub mod quirks;
#[cfg(feature = "server")]
pub mod radiation;
#[cfg(feature = "server")]
pub mod radio;
pub mod radio_ui;
#[cfg(feature = "server")]
pub mod recoil;
pub mod records;
#[cfg(feature = "server")]
pub mod recycling;
#[cfg(feature = "server")]
pub mod reinforced;
#[cfg(feature = "server")]
pub mod rest;
#[cfg(feature = "server")]
pub mod restaurant;
#[cfg(feature = "server")]
pub mod restraints;
#[cfg(feature = "server")]
pub mod round;
#[cfg(feature = "server")]
pub mod round_news;
#[cfg(feature = "client")]
pub mod safe_client;
pub mod safe_ui;
#[cfg(feature = "server")]
pub mod safes;
pub mod science;
pub mod screentips;
#[cfg(feature = "client")]
pub mod screentips_client;
#[cfg(feature = "server")]
pub mod secure_safes;
#[cfg(feature = "server")]
pub mod security;
#[cfg(feature = "server")]
pub mod security_level;
#[cfg(feature = "server")]
pub mod server;
pub mod service;
#[cfg(feature = "server")]
pub mod sheets;
#[cfg(feature = "server")]
pub mod shuttle;
#[cfg(feature = "server")]
pub mod shuttle_catalog;
pub mod shuttle_console;
#[cfg(feature = "client")]
pub mod shuttle_console_client;
#[cfg(feature = "server")]
pub mod shuttle_console_server;
#[cfg(feature = "server")]
pub mod shuttle_nav;
#[cfg(feature = "server")]
pub mod shuttle_remote;
pub mod silicon;
#[cfg(feature = "server")]
pub mod skills;
pub mod slot_machine;
#[cfg(feature = "client")]
pub mod slot_machine_client;
#[cfg(feature = "server")]
pub mod smartfridge;
#[cfg(feature = "server")]
pub mod sound;
#[cfg(feature = "client")]
pub mod sound_client;
#[cfg(feature = "server")]
pub mod spacemove;
#[cfg(feature = "server")]
pub mod spawners;
#[cfg(feature = "server")]
pub mod species;
#[cfg(feature = "server")]
pub mod speech;
#[cfg(feature = "server")]
pub mod spells;
#[cfg(feature = "server")]
pub mod ssd;
#[cfg(feature = "server")]
pub mod stack_recipes;
#[cfg(feature = "server")]
pub mod station_goals;
#[cfg(feature = "server")]
pub mod station_traits;
pub mod statpanel;
#[cfg(feature = "client")]
pub mod statpanel_client;
#[cfg(feature = "server")]
pub mod status_effects;
#[cfg(feature = "server")]
pub mod stickers;
#[cfg(feature = "server")]
pub mod stock_parts;
#[cfg(feature = "server")]
pub mod storage;
#[cfg(feature = "client")]
pub mod storage_client;
#[cfg(feature = "server")]
pub mod stove;
#[cfg(feature = "server")]
pub mod strip;
#[cfg(feature = "server")]
pub mod stumble;
#[cfg(feature = "server")]
pub mod syndie_spraycan;
#[cfg(feature = "server")]
pub mod t_scanner;
#[cfg(feature = "server")]
pub mod tables;
#[cfg(feature = "server")]
pub mod tank_storage;
#[cfg(feature = "server")]
pub mod taser;
#[cfg(feature = "server")]
pub mod teapot;
#[cfg(feature = "server")]
pub mod telecomms;
#[cfg(feature = "client")]
pub mod telecomms_client;
#[cfg(feature = "server")]
pub mod telecomms_config;
pub mod telecomms_ui;
#[cfg(feature = "server")]
pub mod teleport;
#[cfg(feature = "testing")]
pub mod testing;
#[cfg(feature = "server")]
pub mod throw;
#[cfg(feature = "server")]
pub mod tools;
#[cfg(feature = "server")]
pub mod traps;
#[cfg(feature = "server")]
pub mod turf_damage;
#[cfg(feature = "client")]
pub mod ui_sprites;
#[cfg(feature = "server")]
pub mod vehicles;
#[cfg(feature = "server")]
pub mod vending;
#[cfg(feature = "server")]
pub mod ventcrawl;
#[cfg(feature = "server")]
pub mod vision;
#[cfg(feature = "server")]
pub mod wallmount;
#[cfg(feature = "server")]
pub mod weapons;
#[cfg(feature = "server")]
pub mod weather;
/// Shared window data; `wiremod::circuit` and `wiremod::server` with `server`, `wiremod::client`
/// with `client`.
pub mod wiremod;
#[cfg(feature = "server")]
pub mod wires;
pub mod zone;

/// Seconds per tile of a running human: tg `RUN_DELAY 2.2` deciseconds (`config/game_options.txt`,
/// `/datum/movespeed_modifier/config_walk_run/run`); `move_intent` defaults to run.
pub const RUN_DELAY_SECS: f32 = 0.22;

/// Steps the client may have in flight (predicted, not acked); also the most a server queues per
/// mob. 8 run steps are ~1.8 s: a round trip longer than that stalls prediction instead of
/// letting it run away from the server.
pub const MAX_UNACKED_STEPS: usize = 8;

/// The line prefix a server prints its certificate digest after on stdout: a client that started
/// it as a child (`ss13_client` single-player launch) reads it from the pipe.
pub const LOCAL_DIGEST_LINE: &str = "ss13-certificate-digest ";

/// Where a server writes the digest of its self-signed certificate, per port, so a local client
/// can connect without copying it from the log. Dev only: the client reads it only for loopback
/// servers and only when `SS13_CERT_DIGEST` is unset.
///
/// The directory is private to the user (`$XDG_RUNTIME_DIR`, else `<temp>/ss13-rs-<user>` made
/// 0700): in a shared `/tmp` another user could plant a digest and have the client pin their
/// server.
pub fn dev_cert_digest_file(port: u16) -> io::Result<PathBuf> {
    let dir = match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        Some(dir) if dir.is_absolute() => dir,
        _ => {
            let user = std::env::var("USER").unwrap_or_else(|_| "dev".into());
            let dir = std::env::temp_dir().join(format!("ss13-rs-{user}"));
            create_private_dir(&dir)?;
            dir
        }
    };
    Ok(digest_file_in(&dir, port))
}

fn digest_file_in(dir: &Path, port: u16) -> PathBuf {
    dir.join(format!("ss13-rs-dev-cert-{port}.txt"))
}

pub fn write_dev_cert_digest(path: &Path, digest: &str) -> io::Result<()> {
    check_private(path.parent().unwrap_or(Path::new(".")), None)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    io::Write::write_all(&mut options.open(path)?, digest.as_bytes())
}

/// The digest at `path`, only if the directory and the file are private to their owner and the
/// file belongs to the directory's owner.
pub fn read_dev_cert_digest(path: &Path) -> io::Result<String> {
    let dir = check_private(path.parent().unwrap_or(Path::new(".")), None)?;
    check_private(path, Some(&dir))?;
    std::fs::read_to_string(path)
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    let builder = &mut std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(builder, 0o700);
    match builder.create(dir) {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        other => other,
    }
}

/// Not a symlink, no group/other permission bits and, with `owner`, the same owner as it.
#[cfg(unix)]
fn check_private(path: &Path, owner: Option<&std::fs::Metadata>) -> io::Result<std::fs::Metadata> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(path)?;
    let refuse = |why: &str| {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{}: {why}", path.display()),
        ))
    };
    if meta.file_type().is_symlink() {
        return refuse("is a symlink");
    }
    if meta.mode() & 0o077 != 0 {
        return refuse("accessible to other users");
    }
    if owner.is_some_and(|owner| owner.uid() != meta.uid()) {
        return refuse("owned by another user than its directory");
    }
    Ok(meta)
}

// Windows: the temp dir is per user already.
#[cfg(not(unix))]
fn check_private(path: &Path, _: Option<&std::fs::Metadata>) -> io::Result<std::fs::Metadata> {
    std::fs::symlink_metadata(path)
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn digest_is_trusted_only_in_private_files() {
        let dir = std::env::temp_dir().join(format!("ss13-rs-digest-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        create_private_dir(&dir).unwrap();
        let path = digest_file_in(&dir, 1);
        write_dev_cert_digest(&path, "abc").unwrap();
        assert_eq!(read_dev_cert_digest(&path).unwrap(), "abc");

        let chmod =
            |p: &Path, mode| std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode));
        chmod(&path, 0o644).unwrap();
        assert!(read_dev_cert_digest(&path).is_err());
        chmod(&path, 0o600).unwrap();

        let link = digest_file_in(&dir, 2);
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_dev_cert_digest(&link).is_err());

        chmod(&dir, 0o777).unwrap();
        assert!(read_dev_cert_digest(&path).is_err());
        assert!(write_dev_cert_digest(&path, "planted").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
