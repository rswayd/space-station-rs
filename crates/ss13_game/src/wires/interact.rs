use super::*;

/// tg `always_reveal_wire(color)`.
pub(super) fn always_reveal(holder: WireHolder, wire: &str) -> bool {
    use crate::machine_wires::MachineWires as M;
    match holder {
        WireHolder::Machine(M::WireBundle) => true,
        WireHolder::Machine(M::FireAlarm) => wire == crate::machine_wires::WIRE_FIRE_DETECT,
        WireHolder::Machine(M::Robot) => wire == crate::device_wires::WIRE_RESET_MODEL,
        _ => false,
    }
}

pub(super) fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

/// DM `dir2text`.
pub(super) fn dir2text(dir: u8) -> &'static str {
    match dir {
        1 => "north",
        2 => "south",
        4 => "east",
        8 => "west",
        5 => "northeast",
        6 => "southeast",
        9 => "northwest",
        10 => "southwest",
        _ => "",
    }
}

/// `REVERSE_DIR` on dir bits.
pub(super) fn reverse_dir(dir: u8) -> u8 {
    ((dir & 0b0101) << 1) | ((dir & 0b1010) >> 1)
}

/// `get_status()` of the holder.
pub(super) fn status(s: &WiresSourceItem<'_, '_>) -> Vec<String> {
    match s.wires.holder {
        WireHolder::Airlock => {
            let Some(a) = s.door else {
                return Vec::new();
            };
            let mut out = vec![
                format!(
                    "The door bolts {}",
                    if a.locked {
                        "have engaged!"
                    } else {
                        "have disengaged."
                    }
                ),
                format!("The test light is {}.", on_off(a.has_power(!s.unpowered))),
                format!(
                    "The AI connection light is {}.",
                    on_off(a.ai_control_disabled == AI_WIRE_NORMAL)
                ),
                format!("The check wiring light is {}.", on_off(!a.safe)),
                format!("The timer is powered {}.", on_off(a.autoclose)),
                format!("The speed light is {}.", on_off(a.normalspeed)),
                format!("The emergency light is {}.", on_off(a.emergency)),
            ];
            out.push(if a.unres_latch {
                if a.unres_sides != 0 {
                    format!(
                        "The unrestricted exit display is indicating that it is letting people pass from the {}.",
                        dir2text(reverse_dir(a.unres_sides))
                    )
                } else {
                    "The unrestricted exit display is faintly flickering.".to_owned()
                }
            } else {
                "The unrestricted exit display is completely inactive.".to_owned()
            });
            out
        }
        WireHolder::Apc => {
            let Some(a) = s.apc else {
                return Vec::new();
            };
            let channel = |c: Channel| on_off(a.channels[c as usize] > CHANNEL_OFF);
            vec![
                format!(
                    "The interface light is {}.",
                    if a.locked { "red" } else { "green" }
                ),
                format!(
                    "The short indicator is {}.",
                    if a.shorted { "lit" } else { "off" }
                ),
                format!("The channel one light is {}.", channel(Channel::Equip)),
                format!("The channel two light is {}.", channel(Channel::Light)),
                format!("The channel three light is {}.", channel(Channel::Environ)),
                format!("The AI connection light is {}.", on_off(!a.aidisabled)),
            ]
        }
        WireHolder::Vending => {
            let hack = s.hack.copied().unwrap_or_default();
            let extended = s.vendor.is_some_and(|v| v.extended_inventory);
            vec![
                format!("The orange light is {}.", on_off(s.electrified)),
                format!(
                    "The red light is {}.",
                    if hack.shoot_inventory {
                        "off"
                    } else {
                        "blinking"
                    }
                ),
                format!("The green light is {}.", on_off(extended)),
                format!(
                    "A {} light is on.",
                    if hack.scan_id { "purple" } else { "yellow" }
                ),
                // `age_restrictions` defaults to TRUE and no vending wire changes it.
                "A white light is on.".to_owned(),
                // shortcut: vendors speak Galactic Common only; the speaker wire cycles nothing.
                format!(
                    "The speaker light is {}. The language is set to Galactic Common.",
                    on_off(!hack.shut_up)
                ),
            ]
        }
        // `/datum/wires/mulebot` and `/datum/wires/explosive` have no `get_status`.
        WireHolder::Mulebot | WireHolder::ChemGrenade => Vec::new(),
        // Refreshed by `crate::machine_wires` while the window is open.
        WireHolder::Machine(_) => s.machine_status.map(|m| m.0.clone()).unwrap_or_default(),
        WireHolder::Autolathe => {
            let Some(a) = s.autolathe else {
                return Vec::new();
            };
            vec![
                format!("The red light is {}.", on_off(a.disabled)),
                format!("The blue light is {}.", on_off(a.hacked)),
            ]
        }
    }
}

/// `airlock_takes_click`: whether an airlock's own click handling (opening, closing) yields
/// to its panel and wires or a tool: a screwdriver (`screwdriver_act`), a crowbar, a wire tool on an open panel
/// (`item_interaction`), a bare hand on an open panel (`INTERACT_MACHINE_WIRES_IF_OPEN`) or on
/// an electrified airlock (the shock comes first, [`touches`]).
pub fn airlock_takes_click(
    door: &Door,
    holding: bool,
    tool: Option<ToolKind>,
    electrified: bool,
) -> bool {
    if door.kind != DoorKind::Airlock {
        return false;
    }
    match tool {
        // `screwdriver_act`, `crowbar_act` (`crate::doors::crowbar_act`).
        // `welder_act` always ends the click (`crate::doors::welder_act_secondary`).
        Some(ToolKind::Screwdriver | ToolKind::Crowbar | ToolKind::Welder) => true,
        Some(ToolKind::Wirecutters | ToolKind::Multitool) => door.panel_open,
        _ => !holding && (door.panel_open || electrified),
    }
}

/// Everyone within hearing of `pos` (tg `audible_message`).
pub(super) fn hear(world: &mut World, pos: TilePos, text: &str) {
    let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
    world.write_message(SendChat {
        clients,
        message: ChatMessage {
            channel: ChatChannel::Local,
            segments: vec![ChatSegment::new(Span::Italics, text)],
        },
    });
}

pub(super) fn name_of(world: &World, e: Entity) -> String {
    if let Some(name) = world.get::<Name>(e) {
        return name.as_str().to_owned();
    }
    world.get::<DmName>(e).map_or_else(
        || "что-то".to_owned(),
        |n| crate::vending::plain_name(&n.name).to_owned(),
    )
}

pub(super) fn panel_open(world: &World, holder: Entity) -> bool {
    match world.get::<Door>(holder) {
        Some(door) => door.panel_open,
        None => world.get::<PanelOpen>(holder).is_some_and(|p| p.0),
    }
}

/// Airlock and APC `screwdriver_act`: toggles the wire panel. True when handled.
pub fn screwdriver_act(world: &mut World, actor: Actor, kind: ToolKind, target: Entity) -> bool {
    if kind != ToolKind::Screwdriver {
        return false;
    }
    let Some(holder) = world.get::<Wires>(target).map(|w| w.holder) else {
        return false;
    };
    let text = match holder {
        WireHolder::Airlock => {
            let opened = with_door(world, target, |door, appearance, _| {
                door.panel_open = !door.panel_open;
                door.apply_look(appearance);
                (door.panel_open, false)
            });
            match opened {
                Some(true) => "Вы открываете панель техобслуживания шлюза.",
                Some(false) => "Вы закрываете панель техобслуживания шлюза.",
                None => return false,
            }
        }
        // shortcut: APC covers (crowbar, `opened`) are not ported, so the cover is always shut
        // and the screwdriver always reaches the wire panel.
        // `apc/screwdriver_act`: an emagged APC's panel stays shut.
        WireHolder::Apc if world.get::<Apc>(target).is_some_and(|a| a.emagged) => {
            crate::tools::tell(world, actor.client, Span::Warning, "интерфейс сломан!");
            return true;
        }
        WireHolder::Apc => {
            let open = !panel_open(world, target);
            world.entity_mut(target).insert(PanelOpen(open));
            if open {
                "панель проводов открыта"
            } else {
                "панель проводов закрыта"
            }
        }
        // `crate::construction` handles the vending panel, `crate::machine_frame` the
        // autolathe's.
        // `crate::bots` handles the bot's panel.
        // `crate::machine_wires` handles the machines' panels.
        WireHolder::Vending
        | WireHolder::Autolathe
        | WireHolder::Mulebot
        | WireHolder::ChemGrenade
        | WireHolder::Machine(_) => return false,
    };
    crate::tools::tell(world, actor.client, Span::Notice, text);
    true
}

/// `is_wire_tool(tool) && panel_open` in the holders' `item_interaction`: the wires window.
pub fn wire_tool_interaction(
    world: &mut World,
    actor: Actor,
    held: Entity,
    target: Entity,
) -> bool {
    let wire_tool = matches!(
        tool_of(world, held).map(|t| t.0),
        Some(ToolKind::Wirecutters | ToolKind::Multitool)
    );
    if !wire_tool || world.get::<Wires>(target).is_none() || !panel_open(world, target) {
        return false;
    }
    interact(world, actor.client, actor.mob, target);
    true
}

/// tg `wires.interact(user)`: the holder's shock first, then the window if `interactable`.
pub fn interact(world: &mut World, client: Entity, user: Entity, holder: Entity) {
    let Some(kind) = world.get::<Wires>(holder).map(|w| w.holder) else {
        return;
    };
    // `airlock/attempt_wire_interaction`: plating over the wires.
    if kind == WireHolder::Airlock
        && crate::airlock_security::wires_protected(world, client, holder)
    {
        return;
    }
    let electrified = world.get::<Electrified>(holder).is_some();
    // `fax/interactable`: an electrified fax shocks the hand too.
    if electrified
        && matches!(
            kind,
            WireHolder::Airlock
                | WireHolder::Vending
                | WireHolder::Machine(crate::machine_wires::MachineWires::Fax)
        )
        && shock(world, holder, user, 100.0)
    {
        return;
    }
    if !interactable(world, holder, user) {
        return;
    }
    let title = format!("{} Wires", name_of(world, holder));
    world.write_message(OpenUi::new::<WiresWindow>(client, user, holder, title));
}

/// `/datum/wires/ui_status`: `UI_CLOSE` unless `interactable`.
pub(super) fn wire_window_status(world: &mut World) {
    let mut windows = world.query::<(Entity, &UiWindow)>();
    let open: Vec<(Entity, Entity, Entity)> = windows
        .iter(world)
        .filter(|(_, w)| w.interface == WiresUi::NAME)
        .map(|(e, w)| (e, w.source, w.actor))
        .collect();
    for (window, source, actor) in open {
        if !interactable(world, source, actor) {
            world
                .get_mut::<UiWindow>(window)
                .expect("listed above")
                .cap_status(UiStatus::Close);
        }
    }
}

/// `is_holding_tool_quality(kind)`: either hand.
pub(super) fn holding_tool(world: &World, mob: Entity, kind: ToolKind) -> bool {
    world.get::<Inventory>(mob).is_some_and(|i| {
        i.hands
            .iter()
            .flatten()
            .any(|&h| tool_of(world, h).is_some_and(|t| t.0 == kind))
    })
}

/// `ui_act`, every check again at the moment of effect.
pub(super) fn wire_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<WiresWindow>>>) {
    let pending: Vec<(Entity, Entity, WireAct)> = acts
        .read(world.resource::<Messages<UiActed<WiresWindow>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, holder, act) in pending {
        if let Err(reason) = apply_act(world, actor, holder, &act) {
            debug!("wires act {act:?} on {holder} by {actor} rejected: {reason}");
        }
    }
}

pub(super) fn apply_act(
    world: &mut World,
    actor: Entity,
    holder: Entity,
    act: &WireAct,
) -> Result<(), &'static str> {
    // A held holder (a grenade) is where its carrier is.
    let (Some(&apos), Some(hpos)) = (
        world.get::<TilePos>(actor),
        crate::explosions::turf_of(world, holder),
    ) else {
        return Err("gone");
    };
    if world.get::<Stat>(actor) != Some(&Stat::Conscious)
        || distance_status(apos, hpos) != UiStatus::Interactive
    {
        return Err("too far or incapacitated");
    }
    if !interactable(world, holder, actor) {
        return Err("not interactable");
    }
    // `mod/ui_act`, `mecha/ui_act`: an electrified MOD, a short-circuited mech shock first.
    if let Some(WireHolder::Machine(kind)) = world.get::<Wires>(holder).map(|w| w.holder)
        && !crate::device_wires::act_allowed(world, holder, kind, actor)
    {
        return Err("shocked");
    }
    let color = match act {
        WireAct::Cut(c) | WireAct::Pulse(c) | WireAct::Attach(c) => c,
    };
    let wire = world
        .get::<Wires>(holder)
        .and_then(|w| w.get_wire(color))
        .map(str::to_owned)
        .ok_or("no such color")?;
    match act {
        WireAct::Cut(_) => {
            if !holding_tool(world, actor, ToolKind::Wirecutters) {
                tell_mob(world, actor, Span::Warning, "Вам нужны кусачки!");
                return Err("no wirecutters");
            }
            cut(world, holder, &wire, Some(actor));
        }
        WireAct::Pulse(_) => {
            if !holding_tool(world, actor, ToolKind::Multitool) {
                tell_mob(world, actor, Span::Warning, "Вам нужен мультитул!");
                return Err("no multitool");
            }
            pulse(world, holder, &wire, Some(actor));
        }
        WireAct::Attach(_) => attach_act(world, actor, holder, color)?,
    }
    Ok(())
}

/// `ui_act("attach")`: the attached assembly to the user's hands, else the assembly in the
/// active hand onto the wire (`assembly_behavior & wire_behavior`).
pub(super) fn attach_act(
    world: &mut World,
    actor: Entity,
    holder: Entity,
    color: &str,
) -> Result<(), &'static str> {
    let color = world
        .get::<Wires>(holder)
        .and_then(|w| w.colors.iter().find(|(c, _)| *c == color).map(|(c, _)| *c))
        .ok_or("no such color")?;
    if let Some(a) = world
        .get::<WireAssemblies>(holder)
        .and_then(|w| w.at(color))
    {
        detach_assembly(world, holder, color);
        crate::assemblies::server::to_hands(world, actor, a);
        return Ok(());
    }
    let held = world
        .get::<Inventory>(actor)
        .and_then(Inventory::active_item)
        .filter(|&h| {
            world
                .get::<crate::assemblies::server::Assembly>(h)
                .is_some()
        })
        .ok_or("no assembly held")?;
    if !crate::assemblies::server::attachable_to_wires(world, held) {
        tell_mob(
            world,
            actor,
            Span::Warning,
            "You cannot attach this assembly to these wires!",
        );
        return Err("not an input assembly");
    }
    attach_assembly(world, holder, color, held);
    Ok(())
}

/// tg `attach_assembly(color, assembly)` (the chemical grenade's override first).
pub fn attach_assembly(world: &mut World, holder: Entity, color: &'static str, a: Entity) {
    if world
        .get::<WireAssemblies>(holder)
        .is_some_and(|w| w.at(color).is_some())
    {
        return;
    }
    if world.get::<Wires>(holder).map(|w| w.holder) == Some(WireHolder::ChemGrenade) {
        crate::assemblies::chem_grenade::on_wire_attach(world, holder, a);
    }
    crate::assemblies::server::attach_to_wires(world, holder, a);
    let mut entity = world.entity_mut(holder);
    match entity.get_mut::<WireAssemblies>() {
        Some(mut w) => w.0.push((color, a)),
        None => {
            entity.insert(WireAssemblies(vec![(color, a)]));
        }
    }
}

/// tg `detach_assembly(color)`: off the wire, out onto the holder's tile (`on_detach`).
pub fn detach_assembly(world: &mut World, holder: Entity, color: &str) -> Option<Entity> {
    let a = world.get::<WireAssemblies>(holder)?.at(color)?;
    if let Some(mut w) = world.get_mut::<WireAssemblies>(holder) {
        w.0.retain(|(c, _)| *c != color);
    }
    if world.get::<Wires>(holder).map(|w| w.holder) == Some(WireHolder::ChemGrenade) {
        crate::assemblies::chem_grenade::on_wire_detach(world, holder, a);
    }
    crate::assemblies::server::detach(world, a, holder);
    Some(a)
}

/// tg `pulse_assembly(assembly)`: the wire the assembly is on is pulsed, cut or not.
pub fn pulse_assembly(world: &mut World, holder: Entity, a: Entity) {
    let Some(wire) = world
        .get::<WireAssemblies>(holder)
        .and_then(|w| w.0.iter().find(|(_, e)| *e == a).map(|(c, _)| *c))
        .and_then(|color| world.get::<Wires>(holder)?.get_wire(color))
        .map(str::to_owned)
    else {
        return;
    };
    on_pulse(world, holder, &wire, None);
}
