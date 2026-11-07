//! The list and text prompts, and Communion.

use super::*;

pub(super) fn open_list(world: &mut World, client: Entity, mob: Entity, prompt: CultPrompt) {
    let title = prompt.title.clone();
    world.entity_mut(mob).insert(prompt);
    world.write_message(OpenUi::new::<CultListWindow>(client, mob, mob, &title));
}

pub(super) fn open_text(world: &mut World, client: Entity, mob: Entity, kind: TextKind) {
    let (title, message, max_length) = match kind {
        TextKind::Communion => (
            "Voice of Blood",
            "Message to tell to the other acolytes",
            ss13_chat::MAX_MESSAGE_LEN as u32,
        ),
        // `MAX_NAME_LEN`.
        TextKind::Keyword => ("Words of Power", "Keyword for the new rune", 42),
    };
    let ask = Ask::new(
        client,
        mob,
        title,
        message,
        prompt::PromptKind::Text {
            default: String::new(),
            max_length,
            multiline: false,
        },
    );
    let prompt = ask_world(world, ask);
    world.entity_mut(prompt).insert(CultText(kind));
}

impl ServerUi for CultListWindow {
    type Source = &'static CultPrompt;
    type UserParam = ();
    type Act = Option<String>;

    fn static_data(_: &CultPrompt) {}

    fn data(prompt: &CultPrompt, _: Entity, _: &()) -> ListInputData {
        ListInputData {
            title: prompt.title.clone(),
            message: prompt.message.clone(),
            init_value: prompt.items.first().cloned().unwrap_or_default(),
            items: prompt.items.clone(),
            timeout: None,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<Option<String>, UiReject> {
        parse_entry(action, params)
    }
}

pub(super) fn parse_entry(action: &str, params: &UiParams) -> Result<Option<String>, UiReject> {
    match action {
        SUBMIT => params.text("entry").map(Some),
        CANCEL => Ok(None),
        _ => Err(UiReject::UnknownAction),
    }
}

pub(super) fn close_window(world: &mut World, window: Entity) {
    if let Some(mut window) = world.get_mut::<UiWindow>(window) {
        window.cap_status(ss13_shared::ui::UiStatus::Close);
    }
}

pub(super) fn list_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<CultListWindow>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, Entity, Option<String>)> = acts
        .read(world.resource::<Messages<UiActed<CultListWindow>>>())
        .map(|a| (a.window, a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (window, client, actor, source, pick) in pending {
        close_window(world, window);
        let Some(prompt) = world.entity_mut(source).take::<CultPrompt>() else {
            continue;
        };
        let Some(pick) = pick else {
            continue;
        };
        if actor != source || world.get::<Cultist>(actor).is_none() || !can_act(world, actor) {
            continue;
        }
        let Some(i) = prompt.items.iter().position(|it| *it == pick) else {
            continue;
        };
        match prompt.kind {
            PromptKind::Rune => {
                if let Some(kind) = RuneKind::from_name(&pick) {
                    chose_rune(world, client, actor, kind);
                }
            }
            PromptKind::Teleport { from_rune, target } => {
                teleport_to(world, client, actor, from_rune, target, prompt.values[i]);
            }
            PromptKind::Summon { rune } => summon_cultist(world, actor, rune, prompt.values[i]),
            PromptKind::PrepareSpell { empowered } => {
                if let Some(spell) = Spell::from_name(&pick) {
                    start_carving(world, client, actor, spell, empowered);
                } else if pick == "(REMOVE SPELL)" {
                    open_remove_spell(world, client, actor);
                }
            }
            PromptKind::RemoveSpell => {
                if let Some(&id) = prompt.spell_ids.get(i) {
                    remove_spell(world, actor, id);
                }
            }
        }
    }
}

pub(super) fn text_acts(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in pending {
        let Some(&CultText(kind)) = world.get::<CultText>(prompt) else {
            continue;
        };
        let Some(PromptAnswer::Text(entry)) = answer else {
            continue;
        };
        if world.get::<Cultist>(actor).is_none() || !can_act(world, actor) {
            continue;
        }
        let Ok(entry) = ss13_chat::server::sanitize_say(&entry) else {
            continue;
        };
        match kind {
            TextKind::Communion => commune(world, actor, &entry),
            TextKind::Keyword => {
                let entry: String = entry.chars().take(42).collect();
                begin_scribe(world, client, actor, RuneKind::Teleport, Some(entry));
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Communion

/// `cultist_commune`: whispered aloud, read by every cultist and the dead.
pub fn commune(world: &mut World, mob: Entity, message: &str) {
    let invocation = if world
        .get_resource_mut::<crate::combat::CombatRng>()
        .is_some_and(|mut r| r.chance(50.0))
    {
        "O bidai nabora se'sma!"
    } else {
        "O bidai nabora se`sma!"
    };
    whisper(world, mob, invocation);
    whisper(world, mob, message);
    let leader = world.resource::<CultTeam>().leader == Some(mob);
    let (title, span) = if leader {
        ("Master", Span::CultLarge)
    } else if world.get::<HumanLook>(mob).is_some() {
        ("Acolyte", Span::CultItalic)
    } else {
        ("Construct", Span::CultItalic)
    };
    let name = name_of(world, mob);
    let members = world.resource::<CultTeam>().members.clone();
    let mut ghosts = world.query_filtered::<Entity, With<Ghost>>();
    let ghosts: Vec<Entity> = ghosts.iter(world).collect();
    let listeners = world.resource::<Listeners>();
    let clients: Vec<Entity> = members
        .into_iter()
        .chain(ghosts)
        .filter_map(|e| listeners.client_of(e))
        .collect();
    info!(target: "say", "cult {name}: {message}");
    world.write_message(SendChat {
        clients,
        message: commune_message(title, &name, message, span),
    });
}

/// `<span class='cult italic'><b>Acolyte Name:</b> message</span>`.
pub fn commune_message(title: &str, name: &str, text: &str, span: Span) -> ChatMessage {
    ChatMessage {
        channel: ChatChannel::Radio,
        segments: vec![
            ChatSegment::new(Span::Bold, format!("{title} {name}: ")),
            ChatSegment::new(span, text),
        ],
    }
}
