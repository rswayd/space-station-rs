//! Sounds from the DM dump, written by `ss13-mapload convert`:
//!
//! - [`SFX_FILE`]: tg's SFX keys (`/datum/sound_effect` and `/assoc` subtypes,
//!   `code/game/sound/sound_keys/`): key -> files with their `pickweight` weights. The game
//!   (`ss13_game::sound`) reads it at startup for `get_sfx`. It also holds the footstep tables
//!   (`GLOB.footstep`, `barefootstep`, `clawfootstep`, `heavyfootstep`), dumped as globals.
//! - [`SOUND_DIR`]: the `.ogg` files the server can name — every SFX file, the sound vars
//!   ([`SOUND_VARS`]) of the converted types and [`RUNTIME_SOUNDS`] — copied at their tg path,
//!   with the `attributions.txt` / `attribution.txt` / `license.txt` of every directory they came
//!   from and a `SOURCES.txt` (file, source path, source commit).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use ss13_assets::license::Attributions;
pub use ss13_assets::license::source_commit;

use crate::convert::DmTypes;
use crate::fidelity::is_under;

/// `<assets>/sfx.ron`.
pub const SFX_FILE: &str = "sfx.ron";
/// `<assets>/sound/<tg path>`.
pub const SOUND_DIR: &str = "sound";
const SFX_ROOT: &str = "/datum/sound_effect";

/// Vars holding a sound file (or an SFX key) on the types the game ports.
pub const SOUND_VARS: [&str; 22] = [
    "fire_sound",
    "dry_fire_sound",
    "load_sound",
    "load_empty_sound",
    "rack_sound",
    "lock_back_sound",
    "eject_sound",
    "eject_empty_sound",
    "bolt_drop_sound",
    "hitsound",
    "doorOpen",
    "doorClose",
    "doorDeni",
    "boltUp",
    "boltDown",
    "animation_sound",
    // Password doors.
    "door_open",
    "door_close",
    "door_deny",
    "empty_alarm_sound",
    "stun_projectile_sound",
    "lethal_projectile_sound",
];

/// Literal `playsound` files of the ported procs.
pub mod files {
    /// `/obj/item/proc/attack` of a forceless item.
    pub const TAP: &str = "sound/items/weapons/tap.ogg";
    /// `/obj/item/Initialize`: the hitsound of `BURN` items without one.
    pub const WELDER: &str = "sound/items/tools/welder.ogg";
    /// `SFX_SWING_HIT`: the hitsound of `BRUTE` items without one.
    pub const SFX_SWING_HIT: &str = "swing_hit";
    /// Human arms and legs: `unarmed_attack_sound`, `unarmed_miss_sound`.
    pub const PUNCH: &str = "sound/items/weapons/punch1.ogg";
    pub const PUNCH_MISS: &str = "sound/items/weapons/punchmiss.ogg";
    /// `/obj/machinery/door/window` `open()` / `close()`.
    pub const WINDOOR: &str = "sound/machines/windowdoor.ogg";
    /// `/obj/item/gun/ballistic/fire_sounds` on low ammo.
    pub const BALLISTIC_CLICK: &str = "sound/items/weapons/gun/general/ballistic_click.ogg";
    /// Disposals: `disposaloutlet/expel`, `expel_holder`, `disposal/flush`.
    pub const WARNING_BUZZER: &str = "sound/machines/warning-buzzer.ogg";
    pub const HISS: &str = "sound/machines/hiss.ogg";
    pub const DISPOSAL_FLUSH: &str = "sound/machines/disposalflush.ogg";
    /// `/obj/machinery/door/airlock/animation_effects` with `BYPASS_DOOR_CHECKS`.
    pub const AIRLOCK_FORCED: &str = "sound/machines/airlock/airlockforced.ogg";
    /// `/datum/element/footstep/play_craw_step`.
    pub const CRAWL: &str = "sound/effects/footstep/crawl1.ogg";
    /// `/obj/structure/table`: `flipped_table_sound`, `unflip_table_sound`.
    pub const TABLE_FLIP: &str = "sound/items/trayhit/trayhit1.ogg";
    pub const TABLE_UNFLIP: &str = "sound/items/trayhit/trayhit2.ogg";
    /// `/datum/element/table_smash/tablepush`.
    pub const TABLE_SLAM: &str = "sound/effects/tableslam.ogg";
    /// `wrench_act` of tables and table frames.
    pub const DECONSTRUCT: &str = "sound/items/deconstruct.ogg";
    /// Atmos: the analyzer's barometer (`pop`, `ping`'s click), a stationary tank over its
    /// limit (`breaking_sounds`), a canister breaking (`canister_break`).
    pub const ATMOS: [&str; 9] = [
        "sound/effects/pop.ogg",
        "sound/machines/click.ogg",
        "sound/effects/structure_stress/pop1.ogg",
        "sound/effects/structure_stress/pop2.ogg",
        "sound/effects/structure_stress/pop3.ogg",
        "sound/effects/spray.ogg",
        // `/datum/looping_sound/vent_pump_overclock`.
        "sound/machines/fan/fan_start.ogg",
        "sound/machines/fan/fan_loop.ogg",
        "sound/machines/fan/fan_stop.ogg",
    ];
    /// Wall mounts (`ss13_game::wallmount`): `/datum/looping_sound/firealarm` `mid_sounds`, the
    /// light switch's `sound_on`/`sound_off` and BandaStation's click
    /// (`modular_bandastation/aesthetics/lightswitch`), the mounted flasher's flash, the fire
    /// axe cabinet's `toggle_lock`, a firelock's `bash_sound`.
    pub const WALLMOUNT: [&str; 10] = [
        "sound/machines/fire_alarm/fire_alarm1.ogg",
        "sound/machines/fire_alarm/fire_alarm2.ogg",
        "sound/machines/fire_alarm/fire_alarm3.ogg",
        "sound/machines/fire_alarm/fire_alarm4.ogg",
        "sound/items/weapons/magin.ogg",
        "sound/items/weapons/magout.ogg",
        "modular_bandastation/aesthetics/lightswitch/sound/lightswitch.ogg",
        "sound/items/weapons/flash.ogg",
        "sound/machines/locktoggle.ogg",
        "sound/effects/glass/glassbash.ogg",
    ];
    /// Security levels (`ss13_game::security_level`): the levels' `sound`s, `minor_announce`'s
    /// notices, BandaStation's Epsilon intro and law notice, the airlocks lifting their access,
    /// the communications console's swipe prompts.
    pub const SECURITY_LEVELS: [&str; 11] = [
        "sound/announcer/notice/notice1.ogg",
        "sound/announcer/notice/notice2.ogg",
        "sound/announcer/notice/notice3.ogg",
        "sound/announcer/alarm/airraid.ogg",
        "modular_bandastation/security_levels/sound/gamma.ogg",
        "modular_bandastation/security_levels/sound/epsilon.ogg",
        "modular_bandastation/aesthetics_sounds/sound/powerloss.ogg",
        "modular_bandastation/aesthetics_sounds/sound/epsilon_laws.ogg",
        "sound/machines/airlock/boltsup.ogg",
        "sound/machines/terminal/terminal_prompt_deny.ogg",
        "sound/machines/terminal/terminal_prompt_confirm.ogg",
    ];
    /// The prisoner management console (`ss13_game::brig::prisoner`): `id_insert`/`id_eject`
    /// and its `ui_act` sounds.
    pub const PRISONER: [&str; 5] = [
        "sound/machines/terminal/terminal_insert_disc.ogg",
        "sound/machines/terminal/terminal_prompt_confirm.ogg",
        "sound/machines/terminal/terminal_on.ogg",
        "sound/machines/terminal/terminal_off.ogg",
        "sound/machines/terminal/terminal_error.ogg",
    ];
    /// The communications console (`ss13_game::comms_server`): its terminal sounds and
    /// `make_announcement`'s captain and Syndicate sounds (the announcers' are in `sfx.ron`), the
    /// admins' request notification (`ss13_admin`).
    pub const COMMS: [&str; 7] = [
        "sound/machines/terminal/terminal_prompt.ogg",
        "sound/machines/terminal/terminal_alert.ogg",
        "sound/machines/terminal/terminal_on.ogg",
        "sound/machines/terminal/terminal_off.ogg",
        "sound/announcer/announcement/announce.ogg",
        "sound/announcer/announcement/announce_syndi.ogg",
        "sound/misc/server-ready.ogg",
    ];
    /// Vending machines (`ss13_game::vending`): `dispense`, the payment component's register,
    /// `fall_and_crush`.
    pub const VENDING: [&str; 5] = [
        "sound/machines/machine_vend.ogg",
        "sound/effects/cashregister.ogg",
        "sound/effects/bang.ogg",
        "sound/effects/splat.ogg",
        "sound/effects/blob/blobattack.ogg",
    ];
    /// Station bots (`ss13_game::bots`): the voiced lines of the kinds (`generate_speak_list`),
    /// the MULEbot's `buzz`, the stun, cuff and honk sounds of the secbots and honkbots, the
    /// ED-209's shot, the dedbot's blades, the cleanbot's acid spray.
    pub const BOTS: [&str; 62] = [
        "sound/machines/buzz/buzz-sigh.ogg",
        "sound/machines/buzz/buzz-two.ogg",
        "sound/machines/ping.ogg",
        "sound/machines/chime.ogg",
        "sound/items/weapons/egloves.ogg",
        "sound/items/weapons/cablecuff.ogg",
        "sound/items/bikehorn.ogg",
        "sound/misc/sadtrombone.ogg",
        "sound/items/airhorn/airhorn.ogg",
        "sound/mobs/non-humanoids/honkbot/honkbot_evil_laugh.ogg",
        "sound/items/weapons/laser.ogg",
        "sound/items/weapons/bladeslice.ogg",
        "sound/items/weapons/drill.ogg",
        "sound/items/weapons/blade1.ogg",
        "sound/effects/spray2.ogg",
        "sound/mobs/non-humanoids/beepsky/criminal.ogg",
        "sound/mobs/non-humanoids/beepsky/freeze.ogg",
        "sound/mobs/non-humanoids/beepsky/justice.ogg",
        "sound/mobs/non-humanoids/beepsky/creep.ogg",
        "sound/mobs/non-humanoids/beepsky/iamthelaw.ogg",
        "sound/mobs/non-humanoids/beepsky/secureday.ogg",
        "sound/mobs/non-humanoids/medbot/coming.ogg",
        "sound/mobs/non-humanoids/medbot/help.ogg",
        "sound/mobs/non-humanoids/medbot/injured.ogg",
        "sound/mobs/non-humanoids/medbot/patchedup.ogg",
        "sound/mobs/non-humanoids/medbot/apple.ogg",
        "sound/mobs/non-humanoids/medbot/feelbetter.ogg",
        "sound/mobs/non-humanoids/medbot/no.ogg",
        "sound/mobs/non-humanoids/medbot/live.ogg",
        "sound/mobs/non-humanoids/medbot/lost.ogg",
        "sound/mobs/non-humanoids/medbot/delicious.ogg",
        "sound/mobs/non-humanoids/medbot/surgeon.ogg",
        "sound/mobs/non-humanoids/medbot/radar.ogg",
        "sound/mobs/non-humanoids/medbot/catch.ogg",
        "sound/mobs/non-humanoids/medbot/flies.ogg",
        "sound/mobs/non-humanoids/medbot/why.ogg",
        "sound/mobs/non-humanoids/medbot/fuck_you.ogg",
        "sound/mobs/non-humanoids/medbot/turn_off.ogg",
        "sound/mobs/non-humanoids/medbot/im_different.ogg",
        "sound/mobs/non-humanoids/medbot/close.ogg",
        "sound/mobs/non-humanoids/medbot/shindemashou.ogg",
        "sound/mobs/non-humanoids/firebot/nofires.ogg",
        "sound/mobs/non-humanoids/firebot/onlyyou.ogg",
        "sound/mobs/non-humanoids/firebot/tempnominal.ogg",
        "sound/mobs/non-humanoids/firebot/keepitcool.ogg",
        "sound/mobs/non-humanoids/firebot/detected.ogg",
        "sound/mobs/non-humanoids/firebot/stopdropnroll.ogg",
        "sound/mobs/non-humanoids/firebot/extinguishing.ogg",
        "sound/mobs/non-humanoids/firebot/candle_tip.ogg",
        "sound/mobs/non-humanoids/firebot/electric_fire_tip.ogg",
        "sound/mobs/non-humanoids/firebot/gasoline_tip.ogg",
        "sound/mobs/non-humanoids/hygienebot/unhygienicclient.ogg",
        "sound/mobs/non-humanoids/hygienebot/dragyouout.ogg",
        "sound/mobs/non-humanoids/hygienebot/foulsmelling.ogg",
        "sound/mobs/non-humanoids/hygienebot/troglodyte.ogg",
        "sound/mobs/non-humanoids/hygienebot/greencloud.ogg",
        "sound/mobs/non-humanoids/hygienebot/letmeclean.ogg",
        "sound/mobs/non-humanoids/hygienebot/cutarteries.ogg",
        "sound/mobs/non-humanoids/hygienebot/stoprunning.ogg",
        "sound/mobs/non-humanoids/hygienebot/finally.ogg",
        "sound/mobs/non-humanoids/hygienebot/thankgod.ogg",
        "sound/mobs/non-humanoids/hygienebot/degenerate.ogg",
    ];
    /// `SSexplosions.shake_the_room`: the far and distant explosion sounds (the near one, the
    /// creaking and the hull creaking are SFX keys).
    pub const EXPLOSION: [&str; 2] = [
        "sound/effects/explosion/explosionfar.ogg",
        "sound/effects/explosion/explosion_distant.ogg",
    ];
    /// `/datum/looping_sound/fire` `mid_sounds` (equal weights).
    pub const FIRE_LOOP: [&str; 7] = [
        "sound/effects/fireclip1.ogg",
        "sound/effects/fireclip2.ogg",
        "sound/effects/fireclip3.ogg",
        "sound/effects/fireclip4.ogg",
        "sound/effects/fireclip5.ogg",
        "sound/effects/fireclip6.ogg",
        "sound/effects/fireclip7.ogg",
    ];
    /// Paperwork, games, the clown's squeaky things, the desk bell and books
    /// (`ss13_game::service`).
    pub const SERVICE: [&str; 13] = [
        "sound/items/bikehorn.ogg",
        "sound/items/airhorn/airhorn2.ogg",
        "sound/effects/quack.ogg",
        "sound/items/toy_squeak/toysqueak1.ogg",
        "sound/items/toy_squeak/toysqueak2.ogg",
        "sound/items/toy_squeak/toysqueak3.ogg",
        "sound/effects/footstep/clownstep1.ogg",
        "sound/effects/footstep/clownstep2.ogg",
        "sound/items/dice_roll.ogg",
        "sound/items/handling/standard_stamp.ogg",
        "sound/machines/microwave/microwave-end.ogg",
        "sound/items/handling/paper_pickup.ogg",
        "sound/items/handling/component_pickup.ogg",
    ];
    /// Religion (`ss13_game::service`): rites, the armaments beacon, the greedy god.
    pub const RELIGION: [&str; 6] = [
        "sound/effects/pray.ogg",
        "sound/effects/pray_chaplain.ogg",
        "sound/effects/supermatter.ogg",
        "sound/effects/magic/fireball.ogg",
        "sound/effects/cashregister.ogg",
        "sound/machines/buzz/buzz-sigh.ogg",
    ];
    /// The martial arts' moves, manuals and scrolls (`ss13_game::martial`).
    pub const MARTIAL: [&str; 17] = [
        "sound/items/weapons/slam.ogg",
        "sound/items/weapons/genhit1.ogg",
        "sound/items/weapons/cqchit1.ogg",
        "sound/items/weapons/cqchit2.ogg",
        "sound/effects/hit_kick.ogg",
        "sound/effects/hit_punch.ogg",
        "sound/effects/coin2.ogg",
        "sound/items/weapons/slap.ogg",
        "sound/items/weapons/whip.ogg",
        "sound/items/weapons/slashmiss.ogg",
        "sound/items/weapons/taser.ogg",
        "sound/effects/magic/tail_swing.ogg",
        "sound/effects/page_turn/pageturn1.ogg",
        "sound/effects/page_turn/pageturn2.ogg",
        "sound/effects/page_turn/pageturn3.ogg",
        "sound/effects/explosion/explosion1.ogg",
        "sound/items/weapons/punch1.ogg",
    ];
    /// Flora (`ss13_game::flora`): a falling tree, a shovel; posters and showcases
    /// (`ss13_game::decor`): ripping, hanging, the tools.
    pub const DECOR: [&str; 7] = [
        "sound/effects/meteorimpact.ogg",
        "sound/effects/shovel_dig.ogg",
        "sound/items/poster/poster_ripped.ogg",
        "sound/items/poster/poster_being_created.ogg",
        "sound/items/tools/wirecutter.ogg",
        "sound/items/tools/screwdriver.ogg",
        "sound/items/tools/crowbar.ogg",
    ];
    /// Robot customers (`ss13_game::restaurant`): eating, drinking and paying.
    pub const RESTAURANT: [&str; 3] = [
        "sound/items/eatfood.ogg",
        "sound/items/drink.ogg",
        "sound/effects/cashregister.ogg",
    ];
    /// The hallucinations (`ss13_game::hallucination`): fake sounds, battles, announcements,
    /// the bleeding and freezing ones and the delusions.
    /// Syndicate gear (`ss13_game::antag::syndie_items`): the energy blades' `saberon`,
    /// `saberoff` and active `hitsound`, the sleepy pen's stab (none).
    /// Doors (`ss13_game::doors`): BandaStation's `knock_sound` of metal and glass doors;
    /// a containment field's `bump_field` (`ss13_game::power::field_generator`).
    pub const DOORS: [&str; 3] = [
        "modular_bandastation/aesthetics_sounds/sound/door_metal_knock_1.ogg",
        "sound/effects/glass/glassknock.ogg",
        "sound/effects/gravhit.ogg",
    ];
    /// Power tools (`ss13_game::tools`): the jaws of life and hand drill head swaps, the
    /// jaws prying a powered airlock.
    pub const POWER_TOOLS: [&str; 3] = [
        "sound/items/tools/change_jaws.ogg",
        "sound/items/tools/change_drill.ogg",
        "sound/machines/airlock/airlock_alien_prying.ogg",
    ];
    /// Sounds the game code plays by a constant path (checked by the real-asset test
    /// `runtime_types::game_sound_literals_are_converted` in ss13_game).
    pub const GAME_CODE: [&str; 377] = [
        "sound/announcer/announcement/announce_dig.ogg",
        "sound/effects/clang.ogg",
        "sound/machines/compiler/compiler-stage1.ogg",
        "sound/machines/compiler/compiler-stage2.ogg",
        "sound/mobs/non-humanoids/clown/hehe.ogg",
        "sound/items/weapons/jammed.ogg",
        "sound/items/weapons/throwtap.ogg",
        "modular_bandastation/weapon/sound/melee/electrostaff_on.ogg",
        "sound/effects/clock_tick.ogg",
        "sound/effects/woodhit.ogg",
        "sound/items/handcuff_finish.ogg",
        "sound/items/knell/knell1.ogg",
        "sound/items/knell/knell2.ogg",
        "sound/items/knell/knell3.ogg",
        "sound/items/knell/knell4.ogg",
        "sound/items/modsuit/atrocinator_step.ogg",
        "sound/items/tools/drill_use.ogg",
        "sound/effects/chemistry/catalyst.ogg",
        "sound/effects/empulse.ogg",
        "sound/items/handling/toolbox/toolbox_drop.ogg",
        "sound/items/modsuit/ballin.ogg",
        "sound/items/modsuit/loader_charge.ogg",
        "sound/items/modsuit/loader_launch.ogg",
        "sound/items/weapons/contractor_baton/contractorbatonhit.ogg",
        "sound/machines/oven/oven_close.ogg",
        "sound/machines/oven/oven_open.ogg",
        "sound/music/antag/ninja_greeting.ogg",
        "sound/effects/glug.ogg",
        "sound/items/balloon_pop.ogg",
        "sound/items/ceramic_break.ogg",
        "sound/items/lighter/cig_light.ogg",
        "sound/items/lighter/cig_snuff.ogg",
        "sound/items/lighter/lighter_off.ogg",
        "sound/items/lighter/lighter_on.ogg",
        "sound/items/lighter/zippo_off.ogg",
        "sound/items/lighter/zippo_on.ogg",
        "sound/items/modsuit/inflate_bloon.ogg",
        "sound/machines/clockcult/brass_skewer.ogg",
        "sound/machines/tram/door_chime.ogg",
        "sound/machines/tram/info_chime.ogg",
        "sound/machines/tram/other_line_processed.ogg",
        "sound/vehicles/car_crash.ogg",
        "sound/vehicles/mecha/hydraulic.ogg",
        "sound/effects/bamf.ogg",
        "sound/effects/magic/staff_animation.ogg",
        "sound/effects/magic/summon_karp.ogg",
        "sound/effects/magic/timeparadox2.ogg",
        "sound/effects/magic/voidblink.ogg",
        "sound/items/hammering_wood.ogg",
        "sound/items/weapons/bolathrow.ogg",
        "sound/misc/bloop.ogg",
        // The slaughter demon's meal (`ss13_game::antag::slaughter`).
        "sound/effects/magic/demon_consume.ogg",
        // Overload Lights (`ss13_game::antag::revenant`).
        "sound/machines/defib/defib_zap.ogg",
        // The space dragon (`ss13_game::antag::space_dragon`).
        "sound/mobs/non-humanoids/space_dragon/space_dragon_roar.ogg",
        "sound/vehicles/rocketlaunch.ogg",
        "sound/mobs/non-humanoids/mook/mook_leap_yell.ogg",
        "sound/effects/tendril_destroyed.ogg",
        "sound/vehicles/mecha/mechmove01.ogg",
        "sound/items/weapons/sear.ogg",
        "sound/items/weapons/gun/hmg/hmg.ogg",
        "sound/items/weapons/gun/smg/smgrack.ogg",
        "sound/effects/wounds/pierce1.ogg",
        "sound/items/weapons/gun/general/cannon.ogg",
        "sound/items/weapons/gun/general/mountedgun.ogg",
        "sound/items/weapons/gun/general/mountedgunend.ogg",
        "sound/items/weapons/thermalpistol.ogg",
        "sound/items/weapons/genhit1.ogg",
        "sound/items/weapons/genhit2.ogg",
        "sound/items/weapons/genhit3.ogg",
        "sound/items/weapons/punch2.ogg",
        "sound/items/weapons/punch3.ogg",
        "sound/items/weapons/punch4.ogg",
        "sound/music/lobby_music/title0.ogg",
        "sound/music/lobby_music/clown.ogg",
        "sound/announcer/notice/notice1.ogg",
        "sound/items/weapons/punch1.ogg",
        "sound/effects/magic/demon_dies.ogg",
        "sound/effects/cartoon_sfx/cartoon_pop.ogg",
        "sound/effects/ping_hit.ogg",
        "sound/machines/synth/synth_no.ogg",
        "sound/items/toy_squeak/toysqueak1.ogg",
        "sound/misc/sadtrombone.ogg",
        "sound/effects/ghost.ogg",
        "sound/misc/bang.ogg",
        "sound/misc/highlander.ogg",
        "sound/misc/scary_horn.ogg",
        "sound/misc/slip.ogg",
        "sound/machines/terminal/terminal_prompt_confirm.ogg",
        "sound/items/dodgeball.ogg",
        "sound/items/tools/screwdriver.ogg",
        "sound/mobs/humanoids/human/scream/femalescream_1.ogg",
        "sound/mobs/humanoids/human/scream/malescream_1.ogg",
        "sound/mobs/humanoids/human/scream/femalescream_2.ogg",
        "sound/mobs/humanoids/human/scream/malescream_2.ogg",
        "sound/mobs/humanoids/human/scream/femalescream_3.ogg",
        "sound/mobs/humanoids/human/scream/malescream_3.ogg",
        "sound/mobs/humanoids/human/scream/femalescream_4.ogg",
        "sound/mobs/humanoids/human/scream/malescream_4.ogg",
        "sound/mobs/humanoids/human/scream/femalescream_5.ogg",
        "sound/mobs/humanoids/human/scream/malescream_5.ogg",
        "modular_bandastation/emote_panel/audio/moth/moth_cough.ogg",
        "modular_bandastation/emote_panel/audio/moth/moth_sneeze.ogg",
        "modular_bandastation/emote_panel/audio/skrell/anger_1.ogg",
        "modular_bandastation/emote_panel/audio/skrell/anger_2.ogg",
        "modular_bandastation/emote_panel/audio/skrell/giggle_female_1.ogg",
        "modular_bandastation/emote_panel/audio/skrell/giggle_male_1.ogg",
        "modular_bandastation/emote_panel/audio/skrell/melodicwarble.ogg",
        "modular_bandastation/emote_panel/audio/skrell/warble_1.ogg",
        "modular_bandastation/emote_panel/audio/skrell/warble_2.ogg",
        "modular_bandastation/emote_panel/audio/slime/jelly_scream.ogg",
        "modular_bandastation/emote_panel/audio/slime/jelly_squish.ogg",
        "modular_bandastation/emote_panel/audio/tajaran/tajaran_annoyed_meow.ogg",
        "modular_bandastation/emote_panel/audio/tajaran/tajaran_hiss.ogg",
        "modular_bandastation/emote_panel/audio/tajaran/tajaran_meow.ogg",
        "modular_bandastation/emote_panel/audio/tajaran/tajaran_purr_long.ogg",
        "modular_bandastation/emote_panel/audio/tajaran/tajaran_purr.ogg",
        "modular_bandastation/emote_panel/audio/tajaran/tajaran_purr_short.ogg",
        "modular_bandastation/emote_panel/audio/tajaran/tajaran_scream.ogg",
        "modular_bandastation/emote_panel/audio/tajaran/tajaran_sneeze_male.ogg",
        "modular_bandastation/emote_panel/audio/vulpkanin/bark.ogg",
        "modular_bandastation/emote_panel/audio/vulpkanin/howl.ogg",
        "modular_bandastation/emote_panel/audio/vulpkanin/purr.ogg",
        "modular_bandastation/emote_panel/audio/vulpkanin/wbark.ogg",
        "sound/items/weapons/gun/pistol/shot.ogg",
        "sound/items/weapons/gun/pistol/shot_alt.ogg",
        "sound/items/weapons/gun/smg/shot.ogg",
        "sound/items/weapons/gun/smg/shot_suppressed.ogg",
        "sound/items/weapons/gun/revolver/shot.ogg",
        "sound/items/weapons/laser.ogg",
        "sound/items/weapons/laser2.ogg",
        "sound/effects/compressed_air/air_suck.ogg",
        "sound/effects/compressed_air/air_shoot.ogg",
        "sound/effects/bubbles/bubbles.ogg",
        "sound/effects/splat.ogg",
        "sound/effects/emotes/assslap.ogg",
        "sound/mobs/non-humanoids/hiss/hiss2.ogg",
        "sound/effects/cashregister.ogg",
        "sound/items/tools/welder.ogg",
        "sound/items/eatfood.ogg",
        "sound/effects/magic/clockwork/fellowship_armory.ogg",
        "sound/effects/hallucinations/growl1.ogg",
        "sound/effects/hallucinations/growl2.ogg",
        "sound/effects/hallucinations/growl3.ogg",
        "sound/effects/hallucinations/veryfar_noise.ogg",
        "sound/effects/hallucinations/wail.ogg",
        "sound/announcer/alarm/airraid.ogg",
        "sound/ambience/aurora_caelus/aurora_caelus.ogg",
        "sound/announcer/notice/notice2.ogg",
        "sound/effects/magic/lightningbolt.ogg",
        "sound/machines/ventcrawl.ogg",
        "sound/ambience/earth_rumble/earth_rumble_distant1.ogg",
        "sound/ambience/earth_rumble/earth_rumble_distant2.ogg",
        "sound/ambience/earth_rumble/earth_rumble_distant3.ogg",
        "sound/ambience/earth_rumble/earth_rumble_distant4.ogg",
        "sound/ambience/earth_rumble/earth_rumble.ogg",
        "sound/misc/metal_creak.ogg",
        "sound/effects/explosion/explosionfar.ogg",
        "sound/items/weapons/slice.ogg",
        "sound/items/weapons/tap.ogg",
        "sound/items/weapons/whip.ogg",
        "sound/items/weapons/pierce.ogg",
        "sound/effects/hit_kick.ogg",
        "sound/effects/siren.ogg",
        "sound/ambience/weather/snowstorm/snow_start.ogg",
        "sound/mobs/humanoids/human/gag_vomit/gag1.ogg",
        "sound/mobs/humanoids/human/gag_vomit/gag2.ogg",
        "sound/mobs/humanoids/human/gag_vomit/gag3.ogg",
        "sound/mobs/humanoids/human/gag_vomit/gag4.ogg",
        "sound/mobs/humanoids/human/gag_vomit/gag5.ogg",
        "sound/mobs/humanoids/human/gag_vomit/crack_vomit.ogg",
        "sound/effects/desecration/desecration-01.ogg",
        "sound/effects/desecration/desecration-02.ogg",
        "sound/effects/desecration/desecration-03.ogg",
        "sound/items/weapons/gun/shotgun/insert_shell.ogg",
        "sound/items/unsheath.ogg",
        "sound/items/weapons/block_shield.ogg",
        "sound/items/weapons/block_blade.ogg",
        "sound/effects/shieldbash.ogg",
        "sound/effects/energyshieldbash.ogg",
        "sound/effects/bang.ogg",
        "sound/effects/grillehit.ogg",
        "sound/effects/glass/glassbr3.ogg",
        "sound/items/trayhit/trayhit2.ogg",
        "sound/effects/meteorimpact.ogg",
        "sound/effects/curse/curse5.ogg",
        "sound/items/weapons/plasma_cutter.ogg",
        "sound/items/weapons/kinetic_accel.ogg",
        "sound/items/bikehorn.ogg",
        "sound/machines/terminal/terminal_button07.ogg",
        "sound/items/barcodebeep.ogg",
        "sound/items/click.ogg",
        "sound/machines/cryo_warning.ogg",
        "sound/effects/soil_plop.ogg",
        "sound/effects/meatslap.ogg",
        "sound/machines/ping.ogg",
        "sound/effects/pai_boot.ogg",
        "sound/machines/airlock/airlock_alien_prying.ogg",
        "sound/effects/magic/clockwork/invoke_general.ogg",
        "sound/effects/magic/lightning_chargeup.ogg",
        "sound/mobs/non-humanoids/beepsky/iamthelaw.ogg",
        "sound/mobs/non-humanoids/beepsky/criminal.ogg",
        "sound/items/weapons/egloves.ogg",
        "sound/machines/terminal/terminal_eject.ogg",
        "sound/effects/light_flicker.ogg",
        "sound/machines/click.ogg",
        "sound/items/deconstruct.ogg",
        "sound/effects/pop.ogg",
        "sound/items/tools/ratchet.ogg",
        "sound/machines/buzz/buzz-sigh.ogg",
        "sound/effects/magic/fireball.ogg",
        "sound/effects/tableslam.ogg",
        "sound/items/weapons/emitter.ogg",
        "sound/machines/kerchunk.ogg",
        "sound/items/handling/component_drop.ogg",
        "sound/items/weapons/emitter2.ogg",
        "sound/machines/woosh.ogg",
        "sound/machines/chime.ogg",
        "sound/machines/buzz/buzz-two.ogg",
        "sound/machines/lever/lever_start.ogg",
        "sound/machines/lever/lever_stop.ogg",
        "sound/machines/roulette/roulettewheel.ogg",
        "sound/machines/roulette/roulettejackpot.ogg",
        "sound/machines/coindrop.ogg",
        "sound/machines/coindrop2.ogg",
        "sound/items/dump_it.ogg",
        "sound/items/cards/cardshuffle.ogg",
        "sound/items/cards/cardflip.ogg",
        "sound/items/hair-clippers.ogg",
        "sound/items/weapons/thudswoosh.ogg",
        "sound/items/rattling_keys.ogg",
        "sound/machines/scanner/scanbuzz.ogg",
        "sound/effects/glass/glasshit.ogg",
        "sound/effects/footstep/glass_step.ogg",
        "sound/items/weapons/smash.ogg",
        "sound/effects/doorcreaky.ogg",
        "sound/effects/bin/bin_close.ogg",
        "sound/effects/magic/RATTLEMEBONES.ogg",
        "sound/machines/airlock/airlockopen.ogg",
        "sound/items/match_strike.ogg",
        "sound/effects/comfyfire.ogg",
        "modular_bandastation/objects/sounds/pshred.ogg",
        "sound/announcer/alarm/bloblarm.ogg",
        "sound/effects/adminhelp.ogg",
        "sound/effects/alert.ogg",
        "sound/effects/bigsplash.ogg",
        "sound/effects/blob/attackblob.ogg",
        "sound/effects/break_stone.ogg",
        "sound/effects/bubbles/bubbles2.ogg",
        "sound/effects/curtain.ogg",
        "sound/effects/dismember.ogg",
        "sound/effects/fish_splash.ogg",
        "sound/effects/fuse.ogg",
        "sound/effects/health/slowbeat.ogg",
        "sound/effects/hit_on_shattered_glass.ogg",
        "sound/effects/magic.ogg",
        "sound/effects/magic/blind.ogg",
        "sound/effects/magic/blink.ogg",
        "sound/effects/magic/clockwork/narsie_attack.ogg",
        "sound/effects/magic/cosmic_energy.ogg",
        "sound/effects/magic/demon_attack1.ogg",
        "sound/effects/magic/disable_tech.ogg",
        "sound/effects/magic/enter_blood.ogg",
        "sound/effects/magic/exit_blood.ogg",
        "sound/effects/magic/fleshtostone.ogg",
        "sound/effects/magic/knock.ogg",
        "sound/effects/magic/magic_block.ogg",
        "sound/effects/magic/magic_block_mind.ogg",
        "sound/effects/magic/magic_missile.ogg",
        "sound/effects/magic/repulse.ogg",
        "sound/effects/magic/smoke.ogg",
        "sound/effects/magic/summon_guns.ogg",
        "sound/effects/magic/summonitems_generic.ogg",
        "sound/effects/magic/swap.ogg",
        "sound/effects/nightmare_poof.ogg",
        "sound/effects/nightmare_reappear.ogg",
        "sound/effects/phasein.ogg",
        "sound/effects/pickaxe/picaxe1.ogg",
        "sound/effects/podwoosh.ogg",
        "sound/effects/pop_expl.ogg",
        "sound/effects/refill.ogg",
        "sound/effects/singlebeat.ogg",
        "sound/effects/slosh.ogg",
        "sound/effects/smoke.ogg",
        "sound/effects/space_wind.ogg",
        "sound/effects/splash.ogg",
        "sound/effects/stonedoor_openclose.ogg",
        "sound/effects/valve_opening.ogg",
        "sound/effects/whirthunk.ogg",
        "sound/effects/wounds/crack1.ogg",
        "sound/effects/wounds/crackandbleed.ogg",
        "sound/items/coinflip.ogg",
        "sound/items/duct_tape/duct_tape_snap.ogg",
        "sound/items/evidence_bag/evidence_bag_unzip.ogg",
        "sound/items/fulton/fultext_deploy.ogg",
        "sound/items/fulton/fultext_launch.ogg",
        "sound/items/handling/cardboard_box/cardboardbox_pickup.ogg",
        "sound/items/megaphone.ogg",
        "sound/items/pshoom/pshoom.ogg",
        "sound/items/taperecorder/tape_flip.ogg",
        "sound/items/taperecorder/taperecorder_close.ogg",
        "sound/items/taperecorder/taperecorder_open.ogg",
        "sound/items/taperecorder/taperecorder_play.ogg",
        "sound/items/taperecorder/taperecorder_print.ogg",
        "sound/items/taperecorder/taperecorder_stop.ogg",
        "sound/items/tools/rped.ogg",
        "sound/items/tools/screwdriver2.ogg",
        "sound/items/weapons/armbomb.ogg",
        "sound/items/weapons/handcuffs.ogg",
        "sound/items/weapons/kinetic_reload.ogg",
        "sound/items/weapons/resonator_fire.ogg",
        "sound/items/weapons/slash.ogg",
        "sound/items/zip/zip.ogg",
        "sound/machines/beep/beep.ogg",
        "sound/machines/beep/triple_beep.ogg",
        "sound/machines/beep/twobeep.ogg",
        "sound/machines/card_slide.ogg",
        "sound/machines/ding.ogg",
        "sound/machines/eject.ogg",
        "sound/machines/engine_alert/engine_alert3.ogg",
        "sound/machines/high_tech_confirm.ogg",
        "sound/machines/printer.ogg",
        "sound/machines/scanner/scanner.ogg",
        "sound/machines/sink-faucet.ogg",
        "sound/machines/terminal/terminal_processing.ogg",
        "sound/machines/terminal/terminal_select.ogg",
        "sound/machines/toilet_flush.ogg",
        "sound/misc/splort.ogg",
        "sound/mobs/non-humanoids/beepsky/beepskyspinsabre.ogg",
        "sound/mobs/non-humanoids/mouse/mousesqueek.ogg",
        "sound/mobs/non-humanoids/pony/snort.ogg",
        "sound/music/antag/malf.ogg",
        "sound/vehicles/skateboard_ollie.ogg",
        // Named by the latest wave (scanned by `game_sound_literals_are_converted`).
        "sound/ambience/misc/ticking_clock.ogg",
        "sound/announcer/alarm/clownops.ogg",
        "sound/effects/magic/clockwork/ark_activation.ogg",
        "sound/effects/magic/lightningshock.ogg",
        "sound/effects/magic/magic_block_holy.ogg",
        "sound/items/baseballhit.ogg",
        "sound/items/night_vision_on.ogg",
        "sound/items/tools/jaws_pry.ogg",
        "sound/items/weapons/fwoosh.ogg",
        "sound/items/weapons/scope.ogg",
        "sound/machines/gateway/gateway_calibrated.ogg",
        "sound/machines/gateway/gateway_close.ogg",
        "sound/machines/gateway/gateway_open.ogg",
        "sound/machines/gateway/gateway_travel.ogg",
        "sound/machines/pda_button/pda_button1.ogg",
        "sound/mobs/non-humanoids/alien/alien_eat.ogg",
        "sound/mobs/non-humanoids/alien/alien_york.ogg",
        "sound/mobs/non-humanoids/hiss/hiss5.ogg",
        // Security gear (`ss13_game::clothing::secgear`, `muzzle`, `closets`, MOD holster).
        "sound/runtime/complionator/emag.ogg",
        "sound/runtime/complionator/halt.ogg",
        "sound/runtime/complionator/bobby.ogg",
        "sound/runtime/complionator/compliance.ogg",
        "sound/runtime/complionator/justice.ogg",
        "sound/runtime/complionator/running.ogg",
        "sound/runtime/complionator/dontmove.ogg",
        "sound/runtime/complionator/floor.ogg",
        "sound/runtime/complionator/robocop.ogg",
        "sound/runtime/complionator/god.ogg",
        "sound/runtime/complionator/freeze.ogg",
        "sound/runtime/complionator/imperial.ogg",
        "sound/runtime/complionator/bash.ogg",
        "sound/runtime/complionator/harry.ogg",
        "sound/runtime/complionator/asshole.ogg",
        "sound/runtime/complionator/stfu.ogg",
        "sound/runtime/complionator/shutup.ogg",
        "sound/runtime/complionator/super.ogg",
        "sound/runtime/complionator/dredd.ogg",
        "sound/items/whistle/whistle.ogg",
        "sound/items/party_horn.ogg",
        "sound/items/duct_tape/duct_tape_rip.ogg",
        "sound/machines/closet/closet_lock.ogg",
        "sound/machines/closet/closet_unlock.ogg",
        "sound/items/weapons/gun/revolver/empty.ogg",
        // The taser's clumsy head bump, the rotfrag grenade's turns (`ss13_game::taser`,
        // `ss13_game::grenades`).
        "sound/effects/tableheadsmash.ogg",
        "sound/misc/box_deploy.ogg",
    ];
    /// Admin verbs (`ss13_admin::verbs`): BandaStation's Man Up.
    pub const ADMIN: [&str; 1] = ["modular_bandastation/admin/sound/manup1.ogg"];
    /// Quantum pads (`ss13_game::science::quantum_pad`): the beam's hum.
    pub const TELEPORT: [&str; 1] = ["sound/items/weapons/emitter2.ogg"];
    /// Singularities (`ss13_game::power::singularity`): a crystal eaten, a bag of holding.
    pub const SINGULARITY: [&str; 2] = [
        "sound/effects/supermatter.ogg",
        "sound/effects/clockcult_gateway_disrupted.ogg",
    ];
    /// Tesla balls (`ss13_game::power::tesla`): bolts, a mini-ball charging, BandaStation's
    /// moves and dispersal.
    pub const TESLA: [&str; 6] = [
        "sound/effects/magic/lightningbolt.ogg",
        "sound/effects/magic/lightning_chargeup.ogg",
        "modular_bandastation/tesla_rework/sound/tesla_move_1.ogg",
        "modular_bandastation/tesla_rework/sound/tesla_move_2.ogg",
        "modular_bandastation/tesla_rework/sound/tesla_move_3.ogg",
        "modular_bandastation/tesla_rework/sound/tesla_destroy.ogg",
    ];
    pub const SYNDIE_ITEMS: [&str; 3] = [
        "sound/items/weapons/saberon.ogg",
        "sound/items/weapons/saberoff.ogg",
        "sound/items/weapons/blade1.ogg",
    ];
    pub const HALLUCINATION: [&str; 96] = [
        "modular_bandastation/cult_overhaul/sound/announcements/cult_summon.ogg",
        "sound/announcer/alarm/nuke_alarm.ogg",
        "sound/effects/explosion/explosion1.ogg",
        "sound/effects/explosion/explosion2.ogg",
        "sound/effects/explosion/explosionfar.ogg",
        "sound/effects/ghost.ogg",
        "sound/effects/ghost2.ogg",
        "sound/effects/glass/glassbr1.ogg",
        "sound/effects/glass/glassbr2.ogg",
        "sound/effects/glass/glassbr3.ogg",
        "sound/effects/hallucinations/behind_you1.ogg",
        "sound/effects/hallucinations/behind_you2.ogg",
        "sound/effects/hallucinations/far_noise.ogg",
        "sound/effects/hallucinations/growl1.ogg",
        "sound/effects/hallucinations/growl2.ogg",
        "sound/effects/hallucinations/growl3.ogg",
        "sound/effects/hallucinations/i_see_you1.ogg",
        "sound/effects/hallucinations/i_see_you2.ogg",
        "sound/effects/hallucinations/im_here1.ogg",
        "sound/effects/hallucinations/im_here2.ogg",
        "sound/effects/hallucinations/look_up1.ogg",
        "sound/effects/hallucinations/look_up2.ogg",
        "sound/effects/hallucinations/over_here1.ogg",
        "sound/effects/hallucinations/over_here2.ogg",
        "sound/effects/hallucinations/over_here3.ogg",
        "sound/effects/hallucinations/radio_static.ogg",
        "sound/effects/hallucinations/turn_around1.ogg",
        "sound/effects/hallucinations/turn_around2.ogg",
        "sound/effects/hallucinations/veryfar_noise.ogg",
        "sound/effects/hallucinations/wail.ogg",
        "sound/effects/heart_beat.ogg",
        "sound/effects/ice_shovel.ogg",
        "sound/effects/magic/castsummon.ogg",
        "sound/effects/magic/charge.ogg",
        "sound/effects/magic/clockwork/invoke_general.ogg",
        "sound/effects/magic/curse.ogg",
        "sound/effects/magic/disintegrate.ogg",
        "sound/effects/magic/ethereal_enter.ogg",
        "sound/effects/magic/ethereal_exit.ogg",
        "sound/effects/magic/fireball.ogg",
        "sound/effects/magic/forcewall.ogg",
        "sound/effects/magic/lightningbolt.ogg",
        "sound/effects/magic/staff_change.ogg",
        "sound/effects/magic/teleport_app.ogg",
        "sound/effects/magic/teleport_diss.ogg",
        "sound/effects/pray_chaplain.ogg",
        "sound/effects/screech.ogg",
        "sound/effects/wounds/blood1.ogg",
        "sound/effects/wounds/blood2.ogg",
        "sound/effects/wounds/blood3.ogg",
        "sound/items/timer.ogg",
        "sound/items/tools/ratchet.ogg",
        "sound/items/tools/screwdriver.ogg",
        "sound/items/tools/welder.ogg",
        "sound/items/tools/welder2.ogg",
        "sound/items/weapons/blade1.ogg",
        "sound/items/weapons/cablecuff.ogg",
        "sound/items/weapons/effects/searwall.ogg",
        "sound/items/weapons/egloves.ogg",
        "sound/items/weapons/empty.ogg",
        "sound/items/weapons/flash.ogg",
        "sound/items/weapons/laser.ogg",
        "sound/items/weapons/ring.ogg",
        "sound/items/weapons/saberoff.ogg",
        "sound/items/weapons/saberon.ogg",
        "sound/items/weapons/sear.ogg",
        "sound/items/weapons/tap.ogg",
        "sound/items/weapons/taser2.ogg",
        "sound/machines/airlock/airlock.ogg",
        "sound/machines/airlock/airlock_alien_prying.ogg",
        "sound/machines/airlock/airlockforced.ogg",
        "sound/machines/beep/twobeep_high.ogg",
        "sound/machines/compiler/compiler-failure.ogg",
        "sound/machines/steam_hiss.ogg",
        "sound/machines/terminal/terminal_prompt.ogg",
        "sound/misc/highlander.ogg",
        "sound/mobs/humanoids/human/laugh/manlaugh1.ogg",
        "sound/mobs/humanoids/human/laugh/manlaugh2.ogg",
        "sound/mobs/humanoids/human/laugh/womanlaugh.ogg",
        "sound/mobs/non-humanoids/beepsky/freeze.ogg",
        "sound/mobs/non-humanoids/cyborg/liveagain.ogg",
        "sound/mobs/non-humanoids/hiss/hiss1.ogg",
        "sound/mobs/non-humanoids/hiss/hiss2.ogg",
        "sound/mobs/non-humanoids/hiss/hiss3.ogg",
        "sound/mobs/non-humanoids/hiss/hiss4.ogg",
        "sound/mobs/non-humanoids/hiss/lowHiss1.ogg",
        "sound/mobs/non-humanoids/hiss/lowHiss2.ogg",
        "sound/mobs/non-humanoids/hiss/lowHiss3.ogg",
        "sound/mobs/non-humanoids/hiss/lowHiss4.ogg",
        "sound/music/antag/heretic/ascend_blade.ogg",
        "sound/music/antag/heretic/ascend_flesh.ogg",
        "sound/music/antag/heretic/ascend_rust.ogg",
        "sound/music/antag/heretic/ascend_void.ogg",
        "sound/music/antag/monkey.ogg",
        "sound/runtime/hyperspace/hyperspace_begin.ogg",
        "sound/vehicles/mecha/mechstep.ogg",
    ];
}

pub const RUNTIME_SOUNDS: [&str; 15] = [
    files::TAP,
    files::WELDER,
    files::PUNCH,
    files::PUNCH_MISS,
    files::WINDOOR,
    files::BALLISTIC_CLICK,
    files::WARNING_BUZZER,
    files::HISS,
    files::DISPOSAL_FLUSH,
    files::AIRLOCK_FORCED,
    files::CRAWL,
    files::TABLE_FLIP,
    files::TABLE_UNFLIP,
    files::TABLE_SLAM,
    files::DECONSTRUCT,
];

/// The footstep tables, by global name (`ss13_dm_extract::DUMPED_GLOBALS`).
pub const FOOTSTEP_TABLES: [&str; 4] =
    ["footstep", "barefootstep", "clawfootstep", "heavyfootstep"];

/// One footstep table entry, `list(list(files), base volume, extra range)`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StepSound {
    pub files: Vec<String>,
    pub volume: f32,
    pub range: f32,
}

/// SFX key -> (file, weight).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SfxData {
    pub sfx: BTreeMap<String, Vec<(String, u32)>>,
    /// Footstep table name -> turf footstep key (`/turf/open` `footstep` etc.) -> sound.
    #[serde(default)]
    pub footsteps: BTreeMap<String, BTreeMap<String, StepSound>>,
    /// `/datum/centcom_announcer` subtype -> its sounds.
    #[serde(default)]
    pub announcers: BTreeMap<String, AnnouncerSounds>,
    /// tg `GLOB.vox_sounds`: the AI's VOX words ([`resolve_vox`]).
    #[serde(default)]
    pub vox: BTreeMap<String, VoxWord>,
}

/// One VOX word: its file and how long it plays (the client queues the words, tg `wait = 1`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VoxWord {
    pub file: String,
    pub secs: f32,
}

/// tg `GLOB.vox_sounds`' directory (`vox_sounds.dm`: every word is its file's name).
pub const VOX_DIR: &str = "sound/announcer/vox_fem";

/// The VOX words: every `.ogg` of [`VOX_DIR`] under `source`, by lowercase name, with its
/// length. Empty without the directory.
pub fn resolve_vox(source: &Path) -> BTreeMap<String, VoxWord> {
    let Ok(dir) = std::fs::read_dir(source.join(VOX_DIR)) else {
        return BTreeMap::new();
    };
    dir.filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let word = name.strip_suffix(".ogg")?.to_lowercase();
            let bytes = std::fs::read(e.path()).ok()?;
            let secs = ogg_duration(&bytes)? as f32;
            Some((
                word,
                VoxWord {
                    file: format!("{VOX_DIR}/{name}"),
                    secs,
                },
            ))
        })
        .collect()
}

/// The length of an Ogg Vorbis file: the last page's granule position (samples) over the
/// identification header's sample rate.
pub fn ogg_duration(bytes: &[u8]) -> Option<f64> {
    const CAPTURE: &[u8] = b"OggS";
    if !bytes.starts_with(CAPTURE) {
        return None;
    }
    // First page: 27-byte header, the segment table, then the `\x01vorbis` packet.
    let segments = usize::from(*bytes.get(26)?);
    let packet = bytes.get(27 + segments..)?;
    if packet.get(..7)? != b"\x01vorbis" {
        return None;
    }
    let rate = u32::from_le_bytes(packet.get(12..16)?.try_into().ok()?);
    let last = bytes.windows(4).rposition(|w| w == CAPTURE)?;
    let granule = u64::from_le_bytes(bytes.get(last + 6..last + 14)?.try_into().ok()?);
    (rate > 0).then(|| granule as f64 / f64::from(rate))
}

/// A `/datum/centcom_announcer`: the sound sets of announcements.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AnnouncerSounds {
    /// `welcome_sounds`: the round start.
    pub welcome: Vec<String>,
    /// `alert_sounds`: a priority announcement without its own sound.
    pub alert: Vec<String>,
    /// `command_report_sounds`.
    pub command_report: Vec<String>,
    /// `event_sounds`: `ANNOUNCER_*` key -> file.
    pub events: BTreeMap<String, String>,
    /// `custom_alert_message`, its markup stripped: replaces the text of announcements that are
    /// not `has_important_message`.
    pub custom_alert_message: Option<String>,
}

impl SfxData {
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        crate::read_ron(path)
    }

    pub fn to_ron(&self) -> String {
        let mut out = String::from("// Generated by `ss13-mapload convert`. Do not edit.\n");
        out.push_str(&ron::to_string(self).expect("sfx data serializes"));
        out.push('\n');
        out
    }

    pub fn files(&self) -> impl Iterator<Item = &str> {
        let steps = self.footsteps.values().flat_map(|t| t.values());
        let announcers = self.announcers.values().flat_map(|a| {
            (a.welcome.iter().chain(&a.alert).chain(&a.command_report))
                .chain(a.events.values())
                .map(String::as_str)
        });
        (self.sfx.values().flatten().map(|(f, _)| f.as_str()))
            .chain(steps.flat_map(|s| s.files.iter().map(String::as_str)))
            .chain(announcers)
            .chain(self.vox.values().map(|w| w.file.as_str()))
    }
}

fn resource(v: &Value) -> Option<&str> {
    v.get("resource").and_then(Value::as_str)
}

/// Every `/datum/sound_effect` with a `key`: `file_paths` as a plain list (weight 1) or an
/// assoc list of weights (`/datum/sound_effect/assoc`).
pub fn resolve_sfx(types: &DmTypes) -> SfxData {
    let mut sfx = BTreeMap::new();
    for path in types.types.keys().filter(|p| is_under(p, SFX_ROOT)) {
        let Some(key) = types.resolved(path, "key").and_then(Value::as_str) else {
            continue;
        };
        let Some(Value::Array(items)) = types
            .resolved(path, "file_paths")
            .and_then(|v| v.get("list"))
        else {
            continue;
        };
        let entries: Vec<(String, u32)> = items
            .iter()
            .filter_map(|item| match item.get("assoc") {
                Some(Value::Array(pair)) => Some((
                    resource(pair.first()?)?.to_owned(),
                    pair.get(1).and_then(Value::as_f64).unwrap_or(1.0).max(0.0) as u32,
                )),
                _ => Some((resource(item)?.to_owned(), 1)),
            })
            .collect();
        if !entries.is_empty() {
            sfx.insert(key.to_owned(), entries);
        }
    }
    SfxData {
        sfx,
        footsteps: resolve_footsteps(types),
        announcers: resolve_announcers(types),
        vox: BTreeMap::new(),
    }
}

const ANNOUNCER_ROOT: &str = "/datum/centcom_announcer";

/// Every `/datum/centcom_announcer` subtype with its sound lists.
fn resolve_announcers(types: &DmTypes) -> BTreeMap<String, AnnouncerSounds> {
    let files = |path: &str, var: &str| -> Vec<String> {
        types
            .resolved(path, var)
            .map(list)
            .unwrap_or_default()
            .iter()
            .filter_map(|f| resource(f).map(str::to_owned))
            .collect()
    };
    types
        .types
        .keys()
        .filter(|p| is_under(p, ANNOUNCER_ROOT) && p.as_str() != ANNOUNCER_ROOT)
        .map(|path| {
            let events = types
                .resolved(path, "event_sounds")
                .map(list)
                .unwrap_or_default()
                .iter()
                .filter_map(|item| match item.get("assoc")?.as_array()?.as_slice() {
                    [key, file] => Some((key.as_str()?.to_owned(), resource(file)?.to_owned())),
                    _ => None,
                })
                .collect();
            let custom_alert_message = types
                .resolved(path, "custom_alert_message")
                .and_then(Value::as_str)
                .map(strip_markup)
                .filter(|m| !m.is_empty());
            let sounds = AnnouncerSounds {
                welcome: files(path, "welcome_sounds"),
                alert: files(path, "alert_sounds"),
                command_report: files(path, "command_report_sounds"),
                events,
                custom_alert_message,
            };
            (path.clone(), sounds)
        })
        .collect()
}

/// Chat is not HTML: `<br>` becomes a line break, other tags go.
fn strip_markup(html: &str) -> String {
    let html = html.replace("<br>", "\n");
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_owned()
}

fn list(v: &Value) -> &[Value] {
    v.get("list")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// [`FOOTSTEP_TABLES`] from the dumped globals: `list(key = list(list(files), volume, range))`.
fn resolve_footsteps(types: &DmTypes) -> BTreeMap<String, BTreeMap<String, StepSound>> {
    let mut out = BTreeMap::new();
    for name in FOOTSTEP_TABLES {
        let Some(table) = types.globals.get(name) else {
            continue;
        };
        let mut entries = BTreeMap::new();
        for item in list(table) {
            let Some([key, value]) = item
                .get("assoc")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
            else {
                continue;
            };
            let (Some(key), [files, volume, range]) = (key.as_str(), list(value)) else {
                continue;
            };
            entries.insert(
                key.to_owned(),
                StepSound {
                    files: list(files)
                        .iter()
                        .filter_map(|f| resource(f).map(str::to_owned))
                        .collect(),
                    volume: volume.as_f64().unwrap_or_default() as f32,
                    range: range.as_f64().unwrap_or_default() as f32,
                },
            );
        }
        out.insert(name.to_owned(), entries);
    }
    out
}

/// A sound var as the game uses it: a file path, or an SFX key; empty for `null`.
pub fn sound_var(types: &DmTypes, path: &str, var: &str) -> String {
    match types.resolved(path, var) {
        Some(Value::String(s)) => s.clone(),
        Some(v) => resource(v).unwrap_or_default().to_owned(),
        None => String::new(),
    }
}

/// The files the server can name: SFX files, [`SOUND_VARS`] of `converted`, [`RUNTIME_SOUNDS`],
/// the fire loop and the other `files` lists.
pub fn referenced<'a>(
    types: &DmTypes,
    sfx: &SfxData,
    converted: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = sfx.files().map(str::to_owned).collect();
    out.extend(RUNTIME_SOUNDS.map(str::to_owned));
    out.extend(files::FIRE_LOOP.map(str::to_owned));
    out.extend(files::ATMOS.map(str::to_owned));
    out.extend(files::WALLMOUNT.map(str::to_owned));
    out.extend(files::SECURITY_LEVELS.map(str::to_owned));
    out.extend(files::VENDING.map(str::to_owned));
    out.extend(files::COMMS.map(str::to_owned));
    out.extend(files::PRISONER.map(str::to_owned));
    out.extend(files::BOTS.map(str::to_owned));
    out.extend(files::EXPLOSION.map(str::to_owned));
    out.extend(files::MARTIAL.map(str::to_owned));
    out.extend(files::SERVICE.map(str::to_owned));
    out.extend(files::RELIGION.map(str::to_owned));
    out.extend(files::HALLUCINATION.map(str::to_owned));
    out.extend(files::SYNDIE_ITEMS.map(str::to_owned));
    out.extend(files::DOORS.map(str::to_owned));
    out.extend(files::POWER_TOOLS.map(str::to_owned));
    out.extend(files::ADMIN.map(str::to_owned));
    out.extend(files::TELEPORT.map(str::to_owned));
    out.extend(files::SINGULARITY.map(str::to_owned));
    out.extend(files::TESLA.map(str::to_owned));
    out.extend(files::GAME_CODE.map(str::to_owned));
    out.extend(files::RESTAURANT.map(str::to_owned));
    out.extend(files::DECOR.map(str::to_owned));
    for path in converted {
        for var in SOUND_VARS {
            let s = sound_var(types, path, var);
            if s.ends_with(".ogg") {
                out.insert(s);
            }
        }
    }
    out
}

/// `rel` under `root`, matching each component case-insensitively when the exact one is
/// missing (BYOND resolves resources case-insensitively).
fn resolve_case(root: &Path, rel: &str) -> Option<PathBuf> {
    let exact = root.join(rel);
    if exact.is_file() {
        return Some(PathBuf::from(rel));
    }
    let mut found = PathBuf::new();
    for part in rel.split('/') {
        let dir = root.join(&found);
        let name = std::fs::read_dir(&dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .find(|n| n.to_string_lossy().eq_ignore_ascii_case(part))?;
        found.push(name);
    }
    Some(found)
}

/// What [`copy_sounds`] did.
#[derive(Debug, Default)]
pub struct CopyReport {
    pub copied: usize,
    pub bytes: u64,
    pub attributions: usize,
    pub missing: Vec<String>,
}

/// Copies `files` from `source` to `<assets>/sound/<file>`, with the attribution files of every
/// directory on each file's path, and writes `SOURCES.txt`.
pub fn copy_sounds(
    source: &Path,
    assets: &Path,
    files: &BTreeSet<String>,
    commit: &str,
) -> anyhow::Result<CopyReport> {
    let out = assets.join(SOUND_DIR);
    let mut report = CopyReport::default();
    let mut sources = format!(
        "# Sound files copied by `ss13-mapload convert` from BandaStation (tgstation fork), \
         commit {commit}.\n# Licenses: the attribution/license files next to them, and the \
         source repository's.\n# file\tsource path\n"
    );
    let mut attributions = Attributions::default();
    for rel in files {
        let Some(found) = resolve_case(source, rel) else {
            report.missing.push(rel.clone());
            continue;
        };
        let dest = out.join(rel);
        std::fs::create_dir_all(dest.parent().expect("file under the sound dir"))?;
        report.bytes +=
            std::fs::copy(source.join(&found), &dest).with_context(|| format!("copying {rel}"))?;
        report.copied += 1;
        sources.push_str(&format!("{rel}\t{}\n", found.display()));
        attributions.add(&found, rel);
    }
    report.attributions = attributions.copy(source, &out)?;
    std::fs::create_dir_all(&out)?;
    std::fs::write(out.join("SOURCES.txt"), sources)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/datum": { "parent": null },
        "/datum/sound_effect": { "parent": "/datum", "declared_vars": { "key": null,
            "file_paths": null } },
        "/datum/sound_effect/punch": { "parent": "/datum/sound_effect", "declared_vars": {
            "key": "punch", "file_paths": { "list": [ { "resource": "sound/p1.ogg" },
            { "resource": "sound/p2.ogg" } ] } } },
        "/datum/sound_effect/assoc": { "parent": "/datum/sound_effect" },
        "/datum/sound_effect/assoc/meow": { "parent": "/datum/sound_effect/assoc",
            "declared_vars": { "key": "cat_meow", "file_paths": { "list": [
            { "assoc": [ { "resource": "sound/m1.ogg" }, 8 ] },
            { "assoc": [ { "resource": "sound/m2.ogg" }, 23 ] } ] } } },
        "/datum/centcom_announcer": { "parent": "/datum", "declared_vars": {
            "welcome_sounds": { "list": [] }, "alert_sounds": { "list": [] },
            "command_report_sounds": { "list": [] }, "event_sounds": { "list": [] },
            "custom_alert_message": null } },
        "/datum/centcom_announcer/intern": { "parent": "/datum/centcom_announcer",
            "declared_vars": { "alert_sounds": { "list": [ { "resource": "sound/a1.ogg" } ] },
            "event_sounds": { "list": [ { "assoc": [ "announcer_meteors",
            { "resource": "sound/meteors.ogg" } ] } ] },
            "custom_alert_message": "<span class='alert'>Wait for the intern.<br></span>" } },
        "/obj": { "parent": null },
        "/obj/item": { "parent": "/obj", "declared_vars": { "hitsound": null } },
        "/obj/item/gun": { "parent": "/obj/item", "declared_vars": {
            "fire_sound": { "resource": "sound/weapons/shot.ogg" }, "hitsound": "swing_hit" } }
      },
      "globals": { "footstep": { "list": [ { "assoc": [ "floor", { "list": [ { "list": [
          { "resource": "sound/effects/footstep/floor1.ogg" },
          { "resource": "sound/effects/footstep/floor2.ogg" } ] }, 75, -1 ] } ] } ] } }
    }"##;

    #[test]
    fn sfx_keys_and_referenced_files_come_from_the_dump() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let sfx = resolve_sfx(&types);
        assert_eq!(
            sfx.sfx["punch"],
            vec![("sound/p1.ogg".into(), 1), ("sound/p2.ogg".into(), 1)]
        );
        assert_eq!(sfx.sfx["cat_meow"][1], ("sound/m2.ogg".into(), 23));
        assert_eq!(sound_var(&types, "/obj/item/gun", "hitsound"), "swing_hit");
        let files = referenced(&types, &sfx, ["/obj/item/gun"]);
        assert!(files.contains("sound/weapons/shot.ogg"));
        assert!(files.contains("sound/m1.ogg"));
        assert!(!files.contains("swing_hit"));
        // `GLOB.footstep[FOOTSTEP_FLOOR]`: its files are copied too.
        let floor = &sfx.footsteps["footstep"]["floor"];
        assert_eq!(
            (floor.volume, floor.range, floor.files.len()),
            (75.0, -1.0, 2)
        );
        assert!(files.contains("sound/effects/footstep/floor2.ogg"));
        // `/datum/centcom_announcer/intern`: its lists, the message without markup, its files.
        assert_eq!(sfx.announcers.len(), 1);
        let intern = &sfx.announcers["/datum/centcom_announcer/intern"];
        assert_eq!(intern.alert, ["sound/a1.ogg"]);
        assert_eq!(intern.events["announcer_meteors"], "sound/meteors.ogg");
        assert_eq!(
            intern.custom_alert_message.as_deref(),
            Some("Wait for the intern.")
        );
        assert!(files.contains("sound/meteors.ogg"));
        assert_eq!(SfxData::read_str(&sfx.to_ron()), sfx);
    }

    impl SfxData {
        fn read_str(text: &str) -> Self {
            ron::from_str(text).unwrap()
        }
    }

    #[test]
    fn copied_files_bring_their_attributions() {
        let dir = std::env::temp_dir().join(format!("ss13-sound-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (source, assets) = (dir.join("src"), dir.join("assets"));
        std::fs::create_dir_all(source.join("sound/items/weapons")).unwrap();
        std::fs::write(source.join("sound/attributions.txt"), "root").unwrap();
        std::fs::write(source.join("sound/items/license.txt"), "items").unwrap();
        std::fs::write(source.join("sound/items/weapons/Shot.ogg"), "ogg").unwrap();
        let files = BTreeSet::from(["sound/items/weapons/shot.ogg".to_owned()]);
        let report = copy_sounds(&source, &assets, &files, "abc").unwrap();
        assert_eq!((report.copied, report.attributions), (1, 2));
        let out = assets.join(SOUND_DIR);
        // Case-insensitive like BYOND, stored at the referenced path.
        assert!(out.join("sound/items/weapons/shot.ogg").is_file());
        assert!(out.join("sound/attributions.txt").is_file());
        assert!(out.join("sound/items/license.txt").is_file());
        let sources = std::fs::read_to_string(out.join("SOURCES.txt")).unwrap();
        assert!(sources.contains("commit abc"));
        assert!(sources.contains("sound/items/weapons/shot.ogg\tsound/items/weapons/Shot.ogg"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A first page with the Vorbis identification header (22050 Hz) and a last page at
    /// 44100 samples: two seconds.
    fn ogg(rate: u32, granule: u64) -> Vec<u8> {
        let page = |granule: u64, packet: &[u8]| {
            let mut p = b"OggS".to_vec();
            p.extend([0, 0]);
            p.extend(granule.to_le_bytes());
            p.extend([0; 12]);
            p.push(1);
            p.push(packet.len() as u8);
            p.extend(packet);
            p
        };
        let mut id = b"\x01vorbis".to_vec();
        id.extend(0u32.to_le_bytes());
        id.push(1);
        id.extend(rate.to_le_bytes());
        id.extend([0; 13]);
        let mut out = page(0, &id);
        out.extend(page(granule / 2, b"audio"));
        out.extend(page(granule, b"audio"));
        out
    }

    #[test]
    fn ogg_duration_is_the_last_granule_over_the_rate() {
        assert_eq!(ogg_duration(&ogg(22050, 44100)), Some(2.0));
        assert_eq!(ogg_duration(b"RIFF...."), None);
        assert_eq!(ogg_duration(&ogg(0, 44100)), None);
        // A real VOX word, when the source checkout is there.
        let source = std::env::var_os("SS13_SOURCE")
            .map_or_else(|| PathBuf::from(env!("SS13_SOURCE")), PathBuf::from);
        let vox = resolve_vox(&source);
        if let Some(word) = vox.get("alert") {
            assert_eq!(word.file, "sound/announcer/vox_fem/alert.ogg");
            assert!((0.2..3.0).contains(&word.secs), "{}", word.secs);
            assert!(vox.contains_key(",") && vox.len() > 1000);
        }
    }
}
