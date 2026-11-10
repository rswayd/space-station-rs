//! Client half of the vote window (`VotePanel.tsx`). The server half is [`crate::votes`].
// shortcut: the option's tooltip is a line under its name (the window has no tooltip component
// here), the cooldown is a line instead of tg's `Dimmer`.

use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::ActParams;
use ss13_ui_kit::prelude::*;

use crate::votes::*;

pub struct VotesClientPlugin;

impl Plugin for VotesClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<VotePanelUi>();
    }
}

fn new_vote_row(option: &VoteEntry, admin: bool, upper: bool) -> impl Scene + use<> {
    let label = format!("Голосование: {}", option.name);
    let message = option.message.clone();
    let call = ActParams(vec![(VOTE_NAME.into(), option.name.clone())]);
    let toggle = ActParams(vec![(VOTE_NAME.into(), option.name.clone())]);
    let blocked = !option.can_be_initiated;
    let enabled = option.config != Some(false);
    let toggle_blocked = !upper || option.config.is_none();
    let admin_display = if admin { Display::Flex } else { Display::None };
    bsn! {
        @LabeledListItem { @label: label }
        Children [
            Node { flex_direction: FlexDirection::Column }
            Children [
                Node { flex_direction: FlexDirection::Row }
                Children [
                    @Button { @icon: "play", @content: "Начать" }
                    Button { disabled: blocked }
                    Act("callVote")
                    call
                    --
                    @Button { @content: "Активно" }
                    Button { selected: enabled, disabled: toggle_blocked }
                    Node { display: admin_display }
                    Act("toggleVote")
                    toggle
                ]
                --
                Text(message)
            ]
        ]
    }
}

fn choice_row(option: &VoteOption, vote: &ActiveVote, user: &VoteUser) -> impl Scene + use<> {
    let label = ss13_shared::capitalize(&option.name);
    let votes = option.votes.map(|v| v.to_string()).unwrap_or_default();
    let selected = user.selections.iter().any(|s| *s == option.name);
    let single = ActParams(vec![(VOTE_OPTION.into(), option.name.clone())]);
    let multi = ActParams(vec![(VOTE_OPTION.into(), option.name.clone())]);
    let blocked = user.is_ghost || (!vote.multi && selected);
    let (single_display, multi_display) = if vote.multi {
        (Display::None, Display::Flex)
    } else {
        (Display::Flex, Display::None)
    };
    let ghost = user.is_ghost;
    bsn! {
        @LabeledListItem { @label: label }
        Children [
            Node { flex_direction: FlexDirection::Row }
            Children [
                Text(votes)
                --
                @Button { @icon: "vote-yea", @content: "Голосовать" }
                Button { disabled: blocked, selected: selected }
                Node { display: single_display }
                Act("voteSingle")
                single
                --
                @Button { @icon: "vote-yea", @content: "Голосовать" }
                Button { disabled: ghost, selected: selected }
                Node { display: multi_display }
                Act("voteMulti")
                multi
            ]
        ]
    }
}

impl ClientUi for VotePanelUi {
    const WIDTH: f32 = 400.0;
    const HEIGHT: f32 = 500.0;

    fn content(_: &(), data: &VotePanelData) -> impl Scene {
        let user = &data.user;
        let (admin, upper) = (user.is_lower_admin, user.is_upper_admin);
        let admin_display = if admin { Display::Flex } else { Display::None };
        let rows: Vec<_> = data
            .possible_votes
            .iter()
            .map(|o| new_vote_row(o, admin, upper))
            .collect();
        let cooldown = if data.cooldown > 0 {
            format!("Новое голосование через {} с", data.cooldown)
        } else {
            String::new()
        };
        let dead_votes = if data.no_dead_vote {
            "Голоса мёртвых: запрещены"
        } else {
            "Голоса мёртвых: разрешены"
        };
        let dead_active = !data.no_dead_vote;
        let reset_blocked = data.cooldown == 0;
        let dead_blocked = !upper;
        let current = data.current_vote.as_ref();
        let title = current.map_or_else(
            || "Активное голосование".to_owned(),
            |v| {
                v.question
                    .clone()
                    .unwrap_or_else(|| format!("Голосование: {}", v.name))
            },
        );
        let method = current.map_or("", |v| {
            if v.multi {
                "Выберите любое число вариантов"
            } else {
                "Выберите один вариант"
            }
        });
        let empty = if current.is_none() {
            "Голосование не идёт!"
        } else {
            ""
        };
        let choices: Vec<_> = current
            .into_iter()
            .flat_map(|v| v.options.iter().map(move |o| choice_row(o, v, user)))
            .collect();
        let time = current.map_or_else(
            || "Нет голосования".to_owned(),
            |v| format!("Осталось: {} с", v.time_remaining),
        );
        let idle = current.is_none();
        bsn! {
            @WindowContent
            Children [
                @Section { @title: "Новое голосование" }
                Children [
                    Node { flex_direction: FlexDirection::Row, display: admin_display }
                    Children [
                        @Button { @icon: "refresh", @content: "Сбросить перезарядку" }
                        Button { disabled: reset_blocked }
                        Act("resetCooldown")
                        --
                        @Button { @content: dead_votes }
                        Button { selected: dead_active, disabled: dead_blocked }
                        Act("toggleDeadVote")
                    ]
                    --
                    Text(cooldown)
                    --
                    @LabeledList Children [ {rows} ]
                ]
                --
                @Section { @title: title }
                Children [
                    Text(method)
                    --
                    Text(empty)
                    --
                    @LabeledList Children [ {choices} ]
                ]
                --
                @Section { @title: time }
                Children [
                    Node { flex_direction: FlexDirection::Row, display: admin_display }
                    Children [
                        @Button { @content: "Завершить" }
                        Button { disabled: idle, color: UiColor::Green }
                        Act("endNow")
                        --
                        @Button { @content: "Отменить" }
                        Button { disabled: idle, color: UiColor::Red }
                        Act("cancel")
                    ]
                ]
            ]
        }
    }
}
