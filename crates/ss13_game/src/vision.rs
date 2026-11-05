//! Fullscreen overlays and vision (server), after tg `code/_onclick/hud/screen_objects/
//! fullscreen.dm` (`overlay_fullscreen`, `clear_fullscreen`), `carbon.dm` (`update_damage_hud`,
//! `update_tint`), `status_effects/debuffs/vision/{blindness,nearsighted}.dm`, `screen_blur.dm`
//! (`eye_blur`), `temporary_blindness`, `flash_act` and `get_eye_protection`
//! (`living_defense.dm`, `carbon_defense.dm`) and the eyes organ's sight hooks
//! (`organs/internal/eyes/_eyes.dm`).
//!
//! What a carbon sees is derived from its state, not tracked by sources ([`sight`]): missing or
//! failing eyes, being knocked out, temporary blindness, worn `tint` at `TINT_BLIND` and the
//! cortical blindness trauma blind it; eye damage past the low (high) threshold makes it
//! nearsighted 2 (3), which prescription glasses correct; worn tint at `TINT_DARKENED`
//! (`TINT_MILD`) impairs the view; achromatopsia turns it grey. [`Vision`] holds the timed effects
//! (temporary blindness, eye blur, the flash overlay) and exists only while one runs.
//!
//! Every tick each player's carbon gets its [`FullscreenHudData`] (damage overlays of
//! `update_damage_hud`, the vision overlays, blur and client colours) recomputed; a change goes
//! to the owner only, through the headless tgui interface [`FullscreenHudUi`]. tg hides the
//! overlays of a dead mob (`should_show_to`), so a corpse sends none.
// shortcut: carbons only (silicons have no static overlay, basic mobs cannot be blinded as in tg);
// no `damageoverlaytemp` pulse, no `TRAIT_SIGHT_BYPASS`, `TRAIT_NOCRITOVERLAY`, genetic or
// scarred blindness, no night vision, see_in_dark or `sight` flags; blind mobs still get visible
// messages and examine text (tg `blind_message`); no darkened-flash preference.

use bevy::prelude::*;
use ss13_mapload::DmClothing;
use ss13_shared::chat::Span;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiParams, UiReject, UiSystems};

use crate::combat::CombatRng;
use crate::health::{Health, HealthSystems, Stat};
use crate::hud::{
    ClientColour, FullscreenHudData, FullscreenHudUi, FullscreenScreen, ScreenCategory,
};
use crate::inventory::Inventory;
use crate::medical::organs::{self, OrganHost, Slot};
use crate::medical::traumas::{self, Kind};
use crate::quirks::{Quirk, Quirks};
use crate::server::PlayerMob;

/// `TINT_MILD`, `TINT_DARKENED`, `TINT_BLIND`.
pub const TINT_MILD: f32 = 1.5;
pub const TINT_DARKENED: f32 = 2.0;
pub const TINT_BLIND: f32 = 3.0;
/// tg `get_eye_protection`'s `INFINITY` (no eyes, or blind).
pub const EYE_PROTECTION_INFINITY: i32 = 1000;
/// `flash_act`'s default `length`, seconds.
pub const FLASH_LENGTH: f32 = 2.5;
/// `BLUR_DURATION_TO_INTENSITY`.
const BLUR_DURATION_TO_INTENSITY: f32 = 0.05;
/// `/datum/status_effect/grouped/nearsighted` `MAX_SEVERITY`.
const MAX_NEARSIGHT: u8 = 3;

/// The timed vision effects of a mob: seconds left of each.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Vision {
    /// `/datum/status_effect/temporary_blindness`.
    pub temp_blind: f32,
    /// `/datum/status_effect/eye_blur`.
    pub blur: f32,
    /// The `"flash"` overlay, and the length it was shown for (its fade-out).
    pub flash: f32,
    pub flash_length: f32,
}

impl Vision {
    fn idle(&self) -> bool {
        self.temp_blind <= 0.0 && self.blur <= 0.0 && self.flash <= 0.0
    }
}

/// What a carbon's eyes give it now.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sight {
    /// It has eyes.
    pub eyes: bool,
    /// tg `is_blind()`.
    pub blind: bool,
    /// Blind from more than being knocked out (tg `get_eye_protection` is infinite then).
    pub blind_awake: bool,
    /// Temporary blindness is the only source (tg's flickering overlay).
    pub temp_only: bool,
    /// The nearsighted severity, after correction (0: none).
    pub nearsighted: u8,
    /// The summed `tint` of worn clothing (tg `update_tint`).
    pub tint: f32,
    /// Achromatopsia (`/datum/client_colour/monochrome`).
    pub colorblind: bool,
}

/// [`Sight`] of a carbon; `None` for anything without organs (it cannot be blinded here).
pub fn sight(world: &World, mob: Entity) -> Option<Sight> {
    world.get::<OrganHost>(mob)?;
    let eyes = organs::state(world, mob, Slot::Eyes);
    let (tint, corrected) = worn_tint(world, mob);
    let knocked_out = match (world.get::<Health>(mob), world.get::<Stat>(mob)) {
        (Some(health), Some(&stat)) => stat != Stat::Dead && health.knocked_out(stat),
        _ => false,
    } || crate::status_effects::incapacitation(world, mob).knocked_out;
    let temp = world.get::<Vision>(mob).is_some_and(|v| v.temp_blind > 0.0);
    let trauma_kinds: Vec<Kind> = traumas::of_mob(world, mob).iter().map(|t| t.kind).collect();
    let quirks = world.get::<Quirks>(mob);
    let quirk = |q: Quirk| quirks.is_some_and(|quirks| quirks.has(q));
    // `become_blind` sources: NO_EYES, EYE_DAMAGE (failing), EYES_COVERED, TRAUMA_TRAIT,
    // QUIRK_TRAIT (the Blind quirk).
    let other = eyes.is_none_or(|e| e.failing)
        || tint >= TINT_BLIND
        || trauma_kinds.contains(&Kind::Blindness)
        || quirk(Quirk::Blind);
    let blind = other || knocked_out || temp;
    // `assign_nearsightedness(EYE_DAMAGE, damage >= high ? 3 : 2, correctable = TRUE)`.
    let nearsighted = eyes.map_or(0, |e| {
        let spec = e.spec();
        if corrected || e.damage <= spec.low_threshold {
            0
        } else if e.damage > spec.high_threshold {
            MAX_NEARSIGHT
        } else {
            2
        }
    });
    // The Nearsighted quirk: `assign_nearsightedness(QUIRK_TRAIT, 1, correctable = TRUE)`.
    let nearsighted = if quirk(Quirk::Nearsighted) && !corrected {
        nearsighted.max(1)
    } else {
        nearsighted
    };
    Some(Sight {
        eyes: eyes.is_some(),
        blind,
        blind_awake: other || temp,
        temp_only: temp && !other && !knocked_out,
        nearsighted,
        tint,
        // Achromatopsia, or the Monochromacy quirk (`/datum/client_colour/monochrome`).
        colorblind: trauma_kinds.contains(&Kind::ColorBlindness) || quirk(Quirk::Monochromacy),
    })
}

/// tg `is_blind()`.
pub fn is_blind(world: &World, mob: Entity) -> bool {
    sight(world, mob).is_some_and(|s| s.blind)
}

/// The summed `tint` of worn clothing and whether something worn corrects nearsightedness
/// (`TRAIT_NEARSIGHTED_CORRECTED` from `clothing_traits`).
fn worn_tint(world: &World, mob: Entity) -> (f32, bool) {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return (0.0, false);
    };
    worn(inventory)
        .filter_map(|item| world.get::<DmClothing>(item))
        .fold((0.0, false), |(tint, corrected), c| {
            (tint + c.tint, corrected || c.vision_correction)
        })
}

/// tg `get_equipped_items()` without `INCLUDE_POCKETS`: what is worn, not held or pocketed.
pub fn worn(inventory: &Inventory) -> impl Iterator<Item = Entity> + '_ {
    inventory
        .equipped()
        .filter(|&item| !inventory.pockets.contains(&Some(item)))
}

/// What `mob` should see now (tg `update_damage_hud` plus the vision effects).
pub fn hud_data(world: &World, mob: Entity) -> FullscreenHudData {
    let (Some(health), Some(&stat), Some(sight)) = (
        world.get::<Health>(mob),
        world.get::<Stat>(mob),
        sight(world, mob),
    ) else {
        return FullscreenHudData::default();
    };
    if stat == Stat::Dead {
        return FullscreenHudData::default();
    }
    let vision = world.get::<Vision>(mob).cloned().unwrap_or_default();
    let mut screens = damage_screens(health, stat);
    if sight.blind {
        screens.push(FullscreenScreen {
            flicker: sight.temp_only,
            ..FullscreenScreen::new(ScreenCategory::Blind, None)
        });
    }
    if sight.nearsighted > 0 {
        screens.push(FullscreenScreen::new(
            ScreenCategory::Nearsighted,
            Some(sight.nearsighted),
        ));
    }
    // `update_tint`: the overlay goes at once (`clear_fullscreen("tint", 0 SECONDS)`).
    let tint = if sight.tint >= TINT_BLIND {
        None
    } else if sight.tint >= TINT_DARKENED {
        Some(2)
    } else if sight.tint >= TINT_MILD {
        Some(1)
    } else {
        None
    };
    if let Some(severity) = tint {
        screens.push(FullscreenScreen {
            fade: 0.0,
            ..FullscreenScreen::new(ScreenCategory::Tint, Some(severity))
        });
    }
    if vision.flash > 0.0 {
        screens.push(FullscreenScreen {
            fade: vision.flash_length,
            ..FullscreenScreen::new(ScreenCategory::Flash, None)
        });
    }
    screens.sort_by_key(|s| s.category);
    let mut colours = Vec::new();
    if sight.blind {
        colours.push(ClientColour::Blindness);
    }
    if sight.colorblind {
        colours.push(ClientColour::Monochrome);
    }
    FullscreenHudData {
        screens,
        blur: blur_size(vision.blur),
        colours,
    }
}

/// tg `eye_blur/update_blur`: `clamp(seconds_left * 0.05, 0.6, 3)`, refreshed every second.
pub fn blur_size(seconds_left: f32) -> f32 {
    if seconds_left <= 0.0 {
        return 0.0;
    }
    (seconds_left.ceil() * BLUR_DURATION_TO_INTENSITY).clamp(0.6, 3.0)
}

/// tg `/mob/living/carbon/update_damage_hud`: crit, crit vision, oxygen and brute overlays.
pub fn damage_screens(health: &Health, stat: Stat) -> Vec<FullscreenScreen> {
    let mut screens = Vec::new();
    let hp = health.health();
    if hp <= crate::health::HEALTH_THRESHOLD_CRIT {
        // tg's switch has `-50 to -40` twice, so `passage5` is never shown.
        let severity = match hp {
            h if h >= -10.0 => 0,
            h if h >= -20.0 => 1,
            h if h >= -30.0 => 2,
            h if h >= -40.0 => 3,
            h if h >= -50.0 => 4,
            h if h >= -60.0 => 6,
            h if h >= -70.0 => 7,
            h if h >= -90.0 => 8,
            h if h >= -95.0 => 9,
            _ => 10,
        };
        if stat != Stat::HardCrit {
            let vision = match hp {
                h if h > -4.0 => 4,
                h if h >= -8.0 => 5,
                h if h >= -12.0 => 6,
                h if h >= -16.0 => 7,
                h if h >= -20.0 => 8,
                h if h >= -24.0 => 9,
                _ => 10,
            };
            screens.push(FullscreenScreen::new(
                ScreenCategory::CritVision,
                Some(vision),
            ));
        }
        screens.push(FullscreenScreen::new(ScreenCategory::Crit, Some(severity)));
    }
    if health.oxy > 0.0 {
        let severity = match health.oxy {
            o if o < 10.0 => 0,
            o if o <= 20.0 => 1,
            o if o <= 25.0 => 2,
            o if o <= 30.0 => 3,
            o if o <= 35.0 => 4,
            o if o <= 40.0 => 5,
            o if o <= 45.0 => 6,
            _ => 7,
        };
        screens.push(FullscreenScreen::new(ScreenCategory::Oxy, Some(severity)));
    }
    let hurt = health.brute + health.burn;
    if hurt > 0.0 {
        let severity = match hurt {
            h if h < 5.0 => 0,
            h if h <= 15.0 => 1,
            h if h <= 30.0 => 2,
            h if h <= 45.0 => 3,
            h if h <= 70.0 => 4,
            h if h <= 85.0 => 5,
            _ => 6,
        };
        screens.push(FullscreenScreen::new(ScreenCategory::Brute, Some(severity)));
    }
    screens
}

/// The mob's [`Vision`], added when missing.
fn vision_mut(world: &mut World, mob: Entity) -> Option<Mut<'_, Vision>> {
    let mut entity = world.get_entity_mut(mob).ok()?;
    if !entity.contains::<Vision>() {
        entity.insert(Vision::default());
    }
    world.get_mut::<Vision>(mob)
}

/// tg `adjust_temp_blindness(seconds)` (carbons only, as `CAN_BE_BLIND` allows).
pub fn adjust_temp_blindness(world: &mut World, mob: Entity, secs: f32) {
    if world.get::<OrganHost>(mob).is_none() {
        return;
    }
    if let Some(mut v) = vision_mut(world, mob) {
        v.temp_blind = (v.temp_blind + secs).max(0.0);
    }
}

/// tg `remove_status_effect(/datum/status_effect/temporary_blindness)`.
pub fn cure_temp_blindness(world: &mut World, mob: Entity) {
    if let Some(mut v) = world.get_mut::<Vision>(mob) {
        v.temp_blind = 0.0;
    }
}

/// tg `set_eye_blur_if_lower(seconds)`.
pub fn set_eye_blur_if_lower(world: &mut World, mob: Entity, secs: f32) {
    if world.get::<OrganHost>(mob).is_none() {
        return;
    }
    if let Some(mut v) = vision_mut(world, mob)
        && v.blur < secs
    {
        v.blur = secs;
    }
}

/// tg `remove_status_effect(/datum/status_effect/eye_blur)`.
pub fn cure_eye_blur(world: &mut World, mob: Entity) {
    if let Some(mut v) = world.get_mut::<Vision>(mob) {
        v.blur = 0.0;
    }
}

/// tg `get_eye_protection` of a carbon: infinite without eyes or when blind (not only knocked
/// out), else the eyes' own (0 for the modelled ones) plus `worn`, the worn `flash_protect`.
pub fn eye_protection(world: &World, mob: Entity, worn: i32) -> i32 {
    match sight(world, mob) {
        Some(s) if !s.eyes || s.blind_awake => EYE_PROTECTION_INFINITY,
        _ => worn,
    }
}

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: &str) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(world, client, span, text);
    }
}

/// tg `flash_act(intensity, override_blindness_check, visual = visual, length = length)` of a
/// living mob (the carbon one for a mob with organs): the flash overlay for `length` seconds,
/// then, unless `visual`, the eyes burn by how much `intensity` beats the eye protection. True
/// when the mob was flashed.
pub fn flash_act(
    world: &mut World,
    mob: Entity,
    intensity: i32,
    override_blindness: bool,
    visual: bool,
    length: f32,
) -> bool {
    let carbon = world.get::<OrganHost>(mob).is_some();
    // The carbon override: "can't flash what can't see!"
    if carbon && organs::organ(world, mob, Slot::Eyes).is_none() {
        return false;
    }
    let protection = crate::security::eye_protection(world, mob);
    let flashed = protection < intensity && (override_blindness || !is_blind(world, mob));
    let damage = intensity - protection;
    if !flashed {
        if carbon && damage == 0 && world.resource_mut::<CombatRng>().chance(20.0) {
            tell_mob(
                world,
                mob,
                Span::Notice,
                "Что-то яркое вспыхивает в углу вашего зрения!",
            );
        }
        return false;
    }
    if let Some(mut v) = vision_mut(world, mob) {
        v.flash = length;
        v.flash_length = length;
    }
    if !carbon || visual {
        return true;
    }
    let (text, eye_damage) = {
        let mut rng = world.resource_mut::<CombatRng>();
        match damage {
            1 => (
                "Ваши глаза немного щиплет.",
                if rng.chance(40.0) { 1.0 } else { 0.0 },
            ),
            2 => ("Ваши глаза горят.", rng.range(2, 4) as f32),
            _ => ("Ваши глаза сильно зудят и горят!", rng.range(12, 16) as f32),
        }
    };
    tell_mob(world, mob, Span::Warning, text);
    if eye_damage > 0.0 {
        organs::adjust_organ_loss(world, mob, Slot::Eyes, eye_damage);
    }
    let Some(eyes) = organs::state(world, mob, Slot::Eyes).cloned() else {
        return true;
    };
    if eyes.damage > 10.0 {
        adjust_temp_blindness(world, mob, damage as f32 * 2.0);
        let ds = world.resource_mut::<CombatRng>().range(60, 120);
        set_eye_blur_if_lower(world, mob, damage as f32 * ds as f32 / 10.0);
        // Past the low threshold tg rolls for more eye damage only when the mob is not
        // nearsighted from it (it is, since crossing that threshold) or not blind (it is: the
        // temporary blindness above), so nothing more happens there.
        if eyes.damage <= eyes.spec().low_threshold {
            tell_mob(
                world,
                mob,
                Span::Warning,
                "Ваши глаза начинают сильно болеть. Это не хорошо!",
            );
        }
    }
    true
}

/// The welding visors (`attack_self` -> `adjust_visor`, `visor_vars_to_toggle = VISOR_FLASHPROTECT
/// | VISOR_TINT`, `update_icon_state` adds "up").
const VISORS: [&str; 3] = [
    "/obj/item/clothing/head/utility/welding",
    "/obj/item/clothing/glasses/welding",
    "/obj/item/clothing/mask/gas/welding",
];

/// A flipped-up visor: the item's `initial()` values it restores when pulled down.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct VisorUp {
    flash_protect: i32,
    tint: f32,
    inhand: String,
}

/// tg `adjust_visor` of a welding helmet, goggles or mask in the active hand (`visor_toggling`:
/// `up` flips, `flash_protect` and `tint` go to 0 or back, the icon gets its "up" state). False for
/// other items.
// shortcut: only from the hand (tg also toggles worn ones through their action button); no
// sounds or fishing.
pub fn visor_attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(path) = world
        .get::<ss13_mapload::DmItem>(item)
        .map(|i| i.path.clone())
    else {
        return false;
    };
    let Some(&root) = VISORS
        .iter()
        .find(|root| ss13_mapload::fidelity::is_under(&path, root))
    else {
        return false;
    };
    // `can_use`: not incapacitated.
    if crate::status_effects::incapacitation(world, mob).incapacitated {
        return true;
    }
    let (Some(clothing), Some(item_vars), Some(atom)) = (
        world.get::<DmClothing>(item).cloned(),
        world.get::<ss13_mapload::DmItem>(item).cloned(),
        world.get::<ss13_mapload::DmAtom>(item).cloned(),
    ) else {
        return true;
    };
    let up = world.get::<VisorUp>(item).cloned();
    let (flash_protect, tint, worn, inhand) = match &up {
        // Down again: the initial values.
        Some(initial) => (
            initial.flash_protect,
            initial.tint,
            String::new(),
            initial.inhand.clone(),
        ),
        None => {
            world.entity_mut(item).insert(VisorUp {
                flash_protect: clothing.flash_protect,
                tint: clothing.tint,
                inhand: item_vars.inhand_icon_state.clone(),
            });
            // Only the helmet has an "off" in-hand state.
            let inhand = if root == VISORS[0] {
                format!("{}off", item_vars.inhand_icon_state)
            } else {
                item_vars.inhand_icon_state.clone()
            };
            (0, 0.0, format!("{}up", atom.icon_state), inhand)
        }
    };
    if up.is_some() {
        world.entity_mut(item).remove::<VisorUp>();
    }
    if let Some(mut c) = world.get_mut::<DmClothing>(item) {
        c.flash_protect = flash_protect;
        c.tint = tint;
        // What the visor hides and covers (`crate::clothing::cover`).
        let c = &mut *c;
        c.clothing_flags ^= c.visor_flags;
        c.flags_inv ^= c.visor_flags_inv;
        c.flags_cover ^= c.visor_flags_cover;
    }
    if let Some(mut i) = world.get_mut::<ss13_mapload::DmItem>(item) {
        i.worn_icon_state = worn;
        i.inhand_icon_state = inhand;
    }
    let state = if up.is_none() {
        format!("{}up", atom.icon_state)
    } else {
        atom.icon_state.clone()
    };
    let id = world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|icons| icons.get(&atom.icon, &state));
    if let (Some(id), Some(mut appearance)) = (id, world.get_mut::<ss13_shared::Appearance>(item)) {
        appearance.icon_state = id;
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
    let name = crate::security::name_of(world, item);
    let text = match (root == VISORS[2], up.is_none()) {
        (true, true) => "You push the visor up.".to_owned(),
        (true, false) => "You pull the visor down.".to_owned(),
        (false, true) => format!("Вы убираете {name}."),
        (false, false) => format!("Вы ставите обратно {name}."),
    };
    crate::tools::tell(world, client, Span::Notice, text);
    true
}

impl ServerUi for FullscreenHudUi {
    type Source = &'static FullscreenHudData;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &FullscreenHudData) {}

    fn data(source: &FullscreenHudData, _: Entity, _: &()) -> FullscreenHudData {
        source.clone()
    }

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

pub struct VisionServerPlugin;

impl Plugin for VisionServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A welding visor flips up or down.
        app.add_click_handler(ClickPhase::AttackSelf, 800, |w, c| {
            crate::vision::visor_attack_self(w, c.client, c.mob, c.target)
        });
        app.register_ui::<FullscreenHudUi>().add_systems(
            Update,
            (tick_vision, sync_fullscreens)
                .chain()
                .after(HealthSystems)
                .before(UiSystems::Open),
        );
    }
}

/// The timed effects run down (tg durations); temporary blindness heals twice as fast under a
/// blinding cover (`temporary_blindness/tick`: 2 s more every 2 s). Cost: one pass over the
/// mobs with an effect running (a few).
fn tick_vision(world: &mut World) {
    let secs = world.resource::<Time>().delta_secs();
    let mut query = world.query::<(Entity, &Vision)>();
    let running: Vec<Entity> = query.iter(world).map(|(e, _)| e).collect();
    for mob in running {
        let covered = sight(world, mob).is_some_and(|s| s.tint >= TINT_BLIND);
        let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
        let Some(mut v) = world.get_mut::<Vision>(mob) else {
            continue;
        };
        let was_blind = v.temp_blind > 0.0;
        // A corpse's temporary blindness does not heal faster (`tick` returns when dead).
        let blind_rate = if covered && !dead { 2.0 } else { 1.0 };
        v.temp_blind = (v.temp_blind - secs * blind_rate).max(0.0);
        v.blur = (v.blur - secs).max(0.0);
        v.flash = (v.flash - secs).max(0.0);
        let healed_covered = was_blind && v.temp_blind <= 0.0 && covered && !dead;
        if v.idle() {
            world.entity_mut(mob).remove::<Vision>();
        }
        if healed_covered {
            tell_mob(
                world,
                mob,
                Span::GreenText,
                "Your eyes start to feel better!",
            );
        }
    }
}

/// Each player's carbon: its overlays are recomputed and sent to it when they change. Cost: per
/// tick and player a few component reads (organs, worn items, traumas).
fn sync_fullscreens(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &PlayerMob), With<OrganHost>>();
    let players: Vec<(Entity, Entity)> = query.iter(world).map(|(e, p)| (e, p.client)).collect();
    for (mob, client) in players {
        let data = hud_data(world, mob);
        let mut entity = world.entity_mut(mob);
        let changed = match entity.get_mut::<FullscreenHudData>() {
            Some(mut old) => old.set_if_neq(data),
            None => {
                let shown = data != FullscreenHudData::default();
                entity.insert(data);
                shown
            }
        };
        if changed {
            world.write_message(OpenUi::new::<FullscreenHudUi>(client, mob, mob, ""));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::DamageType;
    use crate::medical::organs::{OrganOf, spawn_organ};

    fn human(world: &mut World) -> Entity {
        world.init_resource::<Time>();
        world.insert_resource(CombatRng(1));
        let mob = world
            .spawn((Health::default(), Stat::Conscious, OrganHost))
            .id();
        for slot in Slot::ALL {
            let organ = spawn_organ(world, slot.spec().path).unwrap();
            world.entity_mut(organ).insert(OrganOf(mob));
        }
        mob
    }

    fn categories(data: &FullscreenHudData) -> Vec<(ScreenCategory, Option<u8>)> {
        data.screens
            .iter()
            .map(|s| (s.category, s.severity))
            .collect()
    }

    fn hurt(brute: f32, oxy: f32) -> Health {
        let mut health = Health::default();
        health.adjust(DamageType::Brute, brute);
        health.adjust(DamageType::Oxy, oxy);
        health
    }

    #[test]
    fn damage_overlays_follow_update_damage_hud() {
        let at = |brute, oxy| {
            let h = hurt(brute, oxy);
            let stat = Stat::for_health(h.health());
            damage_screens(&h, stat)
                .iter()
                .map(|s| (s.category, s.severity))
                .collect::<Vec<_>>()
        };
        assert!(at(0.0, 0.0).is_empty());
        assert_eq!(at(4.0, 0.0), [(ScreenCategory::Brute, Some(0))]);
        assert_eq!(at(15.0, 0.0), [(ScreenCategory::Brute, Some(1))]);
        assert_eq!(at(15.5, 0.0), [(ScreenCategory::Brute, Some(2))]);
        assert_eq!(at(90.0, 0.0), [(ScreenCategory::Brute, Some(6))]);
        assert_eq!(at(0.0, 9.0), [(ScreenCategory::Oxy, Some(0))]);
        assert_eq!(at(0.0, 20.0), [(ScreenCategory::Oxy, Some(1))]);
        assert_eq!(at(0.0, 45.5), [(ScreenCategory::Oxy, Some(7))]);
        // Soft crit at health -5: crit vision 5 and crit 0, with the brute overlay.
        assert_eq!(
            at(105.0, 0.0),
            [
                (ScreenCategory::CritVision, Some(5)),
                (ScreenCategory::Crit, Some(0)),
                (ScreenCategory::Brute, Some(6)),
            ]
        );
        assert_eq!(at(100.0, 0.0)[0], (ScreenCategory::CritVision, Some(4)));
        // Hard crit (health -40): no crit vision.
        assert_eq!(at(140.0, 0.0)[0], (ScreenCategory::Crit, Some(3)));
        assert_eq!(at(199.0, 0.0)[0], (ScreenCategory::Crit, Some(10)));
        assert_eq!(at(155.0, 0.0)[0], (ScreenCategory::Crit, Some(6)));
    }

    #[test]
    fn blur_follows_time_left() {
        assert_eq!(blur_size(0.0), 0.0);
        assert_eq!(blur_size(2.0), 0.6);
        assert_eq!(blur_size(20.0), 1.0);
        assert_eq!(blur_size(19.2), 1.0);
        assert_eq!(blur_size(120.0), 3.0);
    }

    #[test]
    fn eyes_blind_and_nearsight_by_damage() {
        let mut world = World::new();
        let mob = human(&mut world);
        assert_eq!(hud_data(&world, mob), FullscreenHudData::default());
        organs::set_organ_damage(&mut world, mob, Slot::Eyes, 25.0);
        let data = hud_data(&world, mob);
        assert_eq!(categories(&data), [(ScreenCategory::Nearsighted, Some(2))]);
        organs::set_organ_damage(&mut world, mob, Slot::Eyes, 35.0);
        assert_eq!(
            categories(&hud_data(&world, mob)),
            [(ScreenCategory::Nearsighted, Some(3))]
        );
        // Failing eyes blind: the static overlay and the dim client colour.
        organs::set_organ_damage(&mut world, mob, Slot::Eyes, 50.0);
        let data = hud_data(&world, mob);
        assert_eq!(
            categories(&data),
            [
                (ScreenCategory::Nearsighted, Some(3)),
                (ScreenCategory::Blind, None)
            ]
        );
        assert!(!data.screens[1].flicker);
        assert_eq!(data.colours, [ClientColour::Blindness]);
        // No eyes: blind, and flashes find nothing to hurt.
        organs::set_organ_damage(&mut world, mob, Slot::Eyes, 0.0);
        let eyes = organs::organ(&world, mob, Slot::Eyes).unwrap();
        organs::remove(&mut world, eyes, false);
        assert!(is_blind(&world, mob));
        assert_eq!(eye_protection(&world, mob, 0), EYE_PROTECTION_INFINITY);
        assert!(!flash_act(&mut world, mob, 1, true, false, FLASH_LENGTH));
        // Dead: nothing is shown.
        world.entity_mut(mob).insert(Stat::Dead);
        assert_eq!(hud_data(&world, mob), FullscreenHudData::default());
    }

    #[test]
    fn temporary_blindness_flickers_and_runs_out() {
        let mut world = World::new();
        let mob = human(&mut world);
        adjust_temp_blindness(&mut world, mob, 2.0);
        let data = hud_data(&world, mob);
        assert_eq!(categories(&data), [(ScreenCategory::Blind, None)]);
        assert!(
            data.screens[0].flicker,
            "temporary blindness alone flickers"
        );
        assert_eq!(
            eye_protection(&world, mob, 0),
            EYE_PROTECTION_INFINITY,
            "blind eyes are not flashed"
        );
        // Knocked out too: the static overlay.
        world.get_mut::<Health>(mob).unwrap().oxy = 60.0;
        assert!(!hud_data(&world, mob).screens.iter().any(|s| s.flicker));
        world.get_mut::<Health>(mob).unwrap().oxy = 0.0;
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(2.5));
        tick_vision(&mut world);
        assert!(world.get::<Vision>(mob).is_none(), "idle effects go away");
        assert!(!is_blind(&world, mob));
    }

    #[test]
    fn knocked_out_blindness_still_lets_flashes_through() {
        let mut world = World::new();
        let mob = human(&mut world);
        world.get_mut::<Health>(mob).unwrap().oxy = 60.0;
        assert!(is_blind(&world, mob));
        assert_eq!(eye_protection(&world, mob, 0), 0);
        // A targeted flash overrides the blindness check; an area one does not.
        assert!(!flash_act(&mut world, mob, 1, false, false, FLASH_LENGTH));
        assert!(flash_act(&mut world, mob, 1, true, false, FLASH_LENGTH));
        assert_eq!(world.get::<Vision>(mob).unwrap().flash, FLASH_LENGTH);
    }

    #[test]
    fn cortical_blindness_and_achromatopsia_follow_the_brain() {
        let mut world = World::new();
        let mob = human(&mut world);
        let brain = organs::organ(&world, mob, Slot::Brain).unwrap();
        assert!(traumas::gain(&mut world, brain, Kind::ColorBlindness, None));
        let data = hud_data(&world, mob);
        assert!(data.screens.is_empty());
        assert_eq!(data.colours, [ClientColour::Monochrome]);
        assert!(traumas::gain(&mut world, brain, Kind::Blindness, None));
        let data = hud_data(&world, mob);
        assert_eq!(categories(&data), [(ScreenCategory::Blind, None)]);
        assert_eq!(
            data.colours,
            [ClientColour::Blindness, ClientColour::Monochrome]
        );
        // The brain goes, and its traumas with it.
        organs::remove(&mut world, brain, true);
        assert!(hud_data(&world, mob).colours.is_empty());
    }

    #[test]
    fn worn_tint_impairs_blinds_and_glasses_correct() {
        let mut world = World::new();
        let mob = human(&mut world);
        let helmet = world
            .spawn(DmClothing {
                tint: 2.0,
                ..DmClothing::default()
            })
            .id();
        world.entity_mut(mob).insert(Inventory {
            head: Some(helmet),
            ..Default::default()
        });
        let data = hud_data(&world, mob);
        assert_eq!(categories(&data), [(ScreenCategory::Tint, Some(2))]);
        assert_eq!(data.screens[0].fade, 0.0, "tint clears at once");
        world.get_mut::<DmClothing>(helmet).unwrap().tint = 1.5;
        assert_eq!(
            categories(&hud_data(&world, mob)),
            [(ScreenCategory::Tint, Some(1))]
        );
        // A blindfold blinds (and protects from flashes).
        world.get_mut::<DmClothing>(helmet).unwrap().tint = 1e31;
        assert!(is_blind(&world, mob));
        assert_eq!(eye_protection(&world, mob, 0), EYE_PROTECTION_INFINITY);
        // Prescription glasses correct eye-damage nearsightedness.
        world.get_mut::<DmClothing>(helmet).unwrap().tint = 0.0;
        organs::set_organ_damage(&mut world, mob, Slot::Eyes, 25.0);
        assert_eq!(sight(&world, mob).unwrap().nearsighted, 2);
        let glasses = world
            .spawn(DmClothing {
                vision_correction: true,
                ..DmClothing::default()
            })
            .id();
        world.get_mut::<Inventory>(mob).unwrap().eyes = Some(glasses);
        assert_eq!(sight(&world, mob).unwrap().nearsighted, 0);
        // In a pocket they do nothing.
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        inventory.eyes = None;
        inventory.pockets[0] = Some(glasses);
        assert_eq!(sight(&world, mob).unwrap().nearsighted, 2);
    }

    #[test]
    fn welding_visors_flip_protection_and_tint() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        let mob = human(&mut world);
        let helmet = world
            .spawn((
                ss13_mapload::DmItem {
                    path: "/obj/item/clothing/head/utility/welding".to_owned(),
                    inhand_icon_state: "welding".to_owned(),
                    ..Default::default()
                },
                ss13_mapload::DmAtom {
                    icon_state: "welding".to_owned(),
                    ..Default::default()
                },
                DmClothing {
                    flash_protect: 2,
                    tint: 2.0,
                    ..DmClothing::default()
                },
            ))
            .id();
        world.entity_mut(mob).insert(Inventory {
            head: Some(helmet),
            ..Default::default()
        });
        let client = world.spawn_empty().id();
        assert_eq!(
            categories(&hud_data(&world, mob)),
            [(ScreenCategory::Tint, Some(2))]
        );
        assert!(visor_attack_self(&mut world, client, mob, helmet));
        let c = world.get::<DmClothing>(helmet).unwrap();
        assert_eq!((c.flash_protect, c.tint), (0, 0.0));
        let i = world.get::<ss13_mapload::DmItem>(helmet).unwrap();
        assert_eq!(
            (i.worn_icon_state.as_str(), i.inhand_icon_state.as_str()),
            ("weldingup", "weldingoff")
        );
        assert!(hud_data(&world, mob).screens.is_empty(), "up: a clear view");
        assert!(visor_attack_self(&mut world, client, mob, helmet));
        let c = world.get::<DmClothing>(helmet).unwrap();
        assert_eq!((c.flash_protect, c.tint), (2, 2.0));
        let i = world.get::<ss13_mapload::DmItem>(helmet).unwrap();
        assert_eq!(
            (i.worn_icon_state.as_str(), i.inhand_icon_state.as_str()),
            ("", "welding")
        );
        // Other clothing has no visor here.
        let hat = world.spawn(ss13_mapload::DmItem::default()).id();
        assert!(!visor_attack_self(&mut world, client, mob, hat));
    }

    #[test]
    fn hard_flashes_burn_blur_and_blind() {
        let mut world = World::new();
        let mob = human(&mut world);
        // Intensity 3 with no protection: 12-16 eye damage, then temporary blindness and blur.
        assert!(flash_act(&mut world, mob, 3, false, false, FLASH_LENGTH));
        let eyes = organs::state(&world, mob, Slot::Eyes).unwrap().damage;
        assert!((12.0..=16.0).contains(&eyes), "{eyes}");
        let v = world.get::<Vision>(mob).unwrap().clone();
        assert_eq!(v.temp_blind, 6.0);
        assert!((18.0..=36.0).contains(&v.blur), "{}", v.blur);
        let data = hud_data(&world, mob);
        assert_eq!(
            categories(&data),
            [(ScreenCategory::Flash, None), (ScreenCategory::Blind, None)]
        );
        assert_eq!(data.screens[0].fade, FLASH_LENGTH);
        assert!(data.blur >= 0.9);
        // A visual flash only shows the overlay.
        let other = human(&mut world);
        assert!(flash_act(&mut world, other, 3, false, true, 4.5));
        assert_eq!(
            organs::state(&world, other, Slot::Eyes).unwrap().damage,
            0.0
        );
        assert_eq!(world.get::<Vision>(other).unwrap().flash_length, 4.5);
    }
}
