use super::*;

/// `megaphone/handle_speech`'s power: every spoken line of a megaphone module's wearer.
pub(super) fn megaphone_lines(
    world: &mut World,
    mut cursor: Local<MessageCursor<ss13_chat::server::Spoke>>,
) {
    let speakers: Vec<Entity> = cursor
        .read(world.resource::<Messages<ss13_chat::server::Spoke>>())
        .map(|s| s.speaker)
        .collect();
    for speaker in speakers {
        if world.get_entity(speaker).is_ok() {
            modules::megaphone_spoke(world, speaker);
        }
    }
}

/// Reads `<assets>/modsuits.ron` unless a test inserted [`ModDb`].
pub(super) fn load_db(world: &mut World) {
    if world.contains_resource::<ModDb>() {
        return;
    }
    let path = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.assets.join(ss13_mapload::modsuit::MODSUIT_FILE));
    let data = match path.as_deref().map(ModData::read) {
        Some(Ok(data)) => data,
        Some(Err(e)) => {
            warn!("{e:#}; no MODsuits (rerun `ss13-mapload convert`)");
            ModData::default()
        }
        None => ModData::default(),
    };
    info!(
        "modsuits: {} themes, {} control units, {} modules",
        data.themes.len(),
        data.suits.len(),
        data.modules.len()
    );
    world.insert_resource(ModDb::new(data));
}

/// Control units that appeared (mapped, printed, from crates) are set up.
pub(super) fn init_new_suits(
    added: Query<(Entity, &DmItem), (Added<DmItem>, Without<ModSuit>)>,
    mut commands: Commands,
) {
    for (e, item) in &added {
        if is_under(&item.path, ss13_mapload::modsuit::CONTROL_ROOT) {
            commands.queue(move |world: &mut World| init_suit(world, e));
        }
    }
}

pub(super) fn base_of(part_path: &str) -> &'static str {
    match part_path {
        p if is_under(p, "/obj/item/clothing/head") => "helmet",
        p if is_under(p, "/obj/item/clothing/suit") => "chestplate",
        p if is_under(p, "/obj/item/clothing/gloves") => "gauntlets",
        p if is_under(p, "/obj/item/clothing/shoes") => "boots",
        p if is_under(p, "/obj/item/clothing/glasses") => "glasses",
        _ => "neck",
    }
}

/// The inventory slot of `ITEM_SLOT_*` bits (one slot).
pub fn slot_of_flags(flags: u32) -> Option<InventorySlot> {
    ALL_SLOTS[2..]
        .iter()
        .copied()
        .find(|&s| slot_bit(s) & flags != 0)
}

/// tg `/obj/item/mod/control/Initialize` (with `pre_equipped`'s core, cell and modules).
pub fn init_suit(world: &mut World, control: Entity) {
    if world.get_entity(control).is_err() || world.get::<ModSuit>(control).is_some() {
        return;
    }
    let Some(path) = world.get::<DmItem>(control).map(|i| i.path.clone()) else {
        return;
    };
    let Some(db) = world.get_resource::<ModDb>() else {
        return;
    };
    let Some(suit_i) = db.suit(&path) else {
        return;
    };
    let mut def: SuitDef = db.data.suits[suit_i].clone();
    // `new /obj/item/mod/control(loc, new_theme)` of a finished MOD shell.
    if let Some(theme) = world.get::<ThemeOverride>(control) {
        def.theme = theme.0.clone();
    }
    let Some(theme_i) = db.theme(&def.theme) else {
        warn!("{path}: no MOD theme {}", def.theme);
        return;
    };
    let theme: ThemeDef = db.data.themes[theme_i].clone();
    let skin_name = if def.skin.is_empty() {
        theme.default_skin.clone()
    } else {
        def.skin.clone()
    };
    let skin = theme
        .skins
        .iter()
        .find(|s| s.name == skin_name)
        .or_else(|| theme.skins.first())
        .cloned()
        .unwrap_or_default();
    let mut parts = Vec::new();
    for part_def in &skin.parts {
        let Some(item) = crate::construction::spawn_detached(world, &part_def.path) else {
            warn!("{path}: no prototype for MOD part {}", part_def.path);
            continue;
        };
        world.entity_mut(item).insert(InSuit(control));
        let Some(slot) = world
            .get::<DmItem>(item)
            .and_then(|i| slot_of_flags(i.slot_flags))
        else {
            continue;
        };
        let clothing = world.get::<DmClothing>(item).cloned().unwrap_or_default();
        parts.push(ModPart {
            item,
            slot,
            sealed: false,
            base: base_of(&part_def.path).to_owned(),
            def: part_def.clone(),
            heat_protection: clothing.heat_protection,
            cold_protection: clothing.cold_protection,
            flash_protect: clothing.flash_protect,
            overslot: None,
        });
    }
    // `set_up_parts`: names, armor, temperature, siemens; `set_skin`: the unsealed flags.
    for part in &parts {
        if let Some(mut c) = world.get_mut::<DmClothing>(part.item) {
            c.clothing_flags = part.def.unsealed_clothing;
            c.flags_inv = part.def.unsealed_inv;
            c.flags_cover = part.def.unsealed_cover;
            c.visor_flags = part.def.sealed_clothing;
            c.visor_flags_inv = part.def.sealed_inv;
            c.visor_flags_cover = part.def.sealed_cover;
            c.heat_protection = 0;
            c.cold_protection = 0;
            c.max_heat_protection_temperature = theme.max_heat_protection_temperature;
            c.min_cold_protection_temperature = theme.min_cold_protection_temperature;
            c.siemens_coefficient = theme.siemens_coefficient;
            if part.slot == InventorySlot::Suit {
                for allowed in &theme.allowed_suit_storage {
                    if !c.allowed.contains(allowed) {
                        c.allowed.push(allowed.clone());
                    }
                }
            }
        }
    }
    for e in parts.iter().map(|p| p.item).chain([control]) {
        world.entity_mut(e).insert(theme.armor);
        if let Some(mut name) = world.get_mut::<DmName>(e) {
            name.name = format!("{} {}", theme.name, name.name);
            name.desc = format!("{} {}", name.desc, theme.desc);
        }
    }
    // `mod.slot_flags = slot_flags`: some themes go on the belt.
    if let Some(mut item) = world.get_mut::<DmItem>(control) {
        item.slot_flags = theme.slot_flags;
    }
    let req_access = def.req_access.clone();
    world.entity_mut(control).insert((
        ModSuit {
            theme: theme_i,
            suit: suit_i,
            skin: skin.name.clone(),
            parts,
            modules: Vec::new(),
            core: None,
            selected: None,
            active: false,
            activating: false,
            open: false,
            locked: !req_access.is_empty(),
            req_access,
            complexity: 0,
            wearer: None,
            holder: None,
            steps: VecDeque::new(),
            step_at: 0.0,
            pending: None,
            pins: Vec::new(),
            pinned_once: Vec::new(),
            ready_until: 0.0,
        },
        ModSuitView::default(),
        AlwaysState,
    ));
    update_icons(world, control);
    if !def.core.is_empty()
        && let Some(core) = crate::construction::spawn_detached(world, &def.core)
    {
        init_core(world, core);
        if !def.cell.is_empty()
            && world
                .get::<ModCore>(core)
                .is_some_and(|c| !c.infinite && c.standard)
            && let Some(cell) = crate::construction::spawn_detached(world, &def.cell)
        {
            crate::storage::put_inside(world, cell, core);
            if let Some(mut c) = world.get_mut::<ModCore>(core) {
                c.cell = Some(cell);
            }
        }
        world.entity_mut(core).insert(InSuit(control));
        if let Some(mut suit) = world.get_mut::<ModSuit>(control) {
            suit.core = Some(core);
        }
    }
    for module_path in theme.inbuilt_modules.iter().chain(&def.modules) {
        let Some(module) = crate::construction::spawn_detached(world, module_path) else {
            continue;
        };
        modules::init_module(world, module);
        if !install(world, control, module, None) {
            world.despawn(module);
        }
    }
}

/// Gives a core item its [`ModCore`] (idempotent).
pub fn init_core(world: &mut World, core: Entity) {
    if world.get::<ModCore>(core).is_some() {
        return;
    }
    let Some(path) = world.get::<DmItem>(core).map(|i| i.path.clone()) else {
        return;
    };
    if !is_under(&path, ss13_mapload::modsuit::CORE_ROOT) {
        return;
    }
    // `maxcharge = charge = 10 (lavaland: 15) * STANDARD_CELL_CHARGE`.
    let plasma = is_under(&path, PLASMA_CORE).then(|| {
        let max = if is_under(&path, LAVALAND_CORE) {
            15.0
        } else {
            10.0
        } * STANDARD_CELL_CHARGE;
        (max, max)
    });
    world.entity_mut(core).insert(ModCore {
        infinite: is_under(&path, INFINITE_CORE),
        standard: is_under(&path, STANDARD_CORE),
        cell: None,
        ethereal: is_under(&path, ETHEREAL_CORE),
        plasma,
    });
}

/// `set_skin`/`seal_part`/`update_icon_state`: `[skin]-[base](-sealed)` for the item icon and
/// the worn sprite of the unit and every part.
pub fn update_icons(world: &mut World, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let skin = world
        .resource::<ModDb>()
        .data
        .themes
        .get(suit.theme)
        .and_then(|t| t.skins.iter().find(|s| s.name == suit.skin))
        .cloned()
        .unwrap_or_default();
    let icon = if skin.icon.is_empty() {
        MOD_ICON.to_owned()
    } else {
        skin.icon.clone()
    };
    let worn = if skin.worn_icon.is_empty() {
        MOD_WORN_ICON.to_owned()
    } else {
        skin.worn_icon.clone()
    };
    let mut targets: Vec<(Entity, String)> = suit
        .parts
        .iter()
        .map(|p| {
            let sealed = if p.sealed { "-sealed" } else { "" };
            (p.item, format!("{}-{}{sealed}", suit.skin, p.base))
        })
        .collect();
    let sealed = if suit.active { "-sealed" } else { "" };
    targets.push((control, format!("{}-control{sealed}", suit.skin)));
    for (e, state) in targets {
        let id = world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(&icon, &state));
        if let Some(mut item) = world.get_mut::<DmItem>(e) {
            item.worn_icon = worn.clone();
            item.worn_icon_state = state.clone();
        }
        if let (Some(id), Some(mut look)) = (id, world.get_mut::<Appearance>(e))
            && look.icon_state != id
        {
            look.icon_state = id;
        }
    }
    if let Some(wearer) = suit.wearer.or(suit.holder) {
        crate::inventory::refresh_look(world, wearer);
    }
}

pub(crate) fn theme_of(world: &World, suit: &ModSuit) -> ThemeDef {
    world.resource::<ModDb>().data.themes[suit.theme].clone()
}
