//! The newscaster window (client): `PhysicalNewscaster` / `Newscaster.jsx` with the user line,
//! the channel list (the wanted issue first, "Create Channel [+]" last), the channel box
//! (description or the D-notice, Submit Story, D-Notice, Print Newspaper), the stories with
//! their censor and comment buttons and comments, and the wanted issue screen. The data is
//! `crate::newscaster`, the server half `crate::newscaster_server`.
// shortcut: the texts (stories, comments, channel name and description, the wanted issue) are
// asked by server prompts instead of the window's inputs; no photos, bounty board tab, theme
// or markdown.

use bevy::app::Propagate;
use bevy::scene::{EntityScene, SceneList};
use ss13_tgui::client::ClientUi;
use ss13_ui_kit::prelude::*;

use crate::newscaster::*;

impl ClientUi for NewscasterUi {
    const WIDTH: f32 = 575.0;
    const HEIGHT: f32 = 560.0;

    fn content(_: &(), data: &NewscasterData) -> impl Scene {
        view(data)
    }
}

fn params(key: &str, id: u32) -> ActParams {
    ActParams(vec![(key.to_owned(), id.to_string())])
}

fn act_button(
    icon: &'static str,
    content: String,
    action: &str,
    params: ActParams,
    disabled: bool,
) -> impl Scene + use<> {
    let action = action.to_owned();
    bsn! {
        @ss13_ui_kit::Button { @icon: icon, @content: content }
        ss13_ui_kit::Button { disabled: disabled }
        Act(action)
        params
    }
}

fn line(text: String) -> impl Scene + use<> {
    bsn! { Node Children [ Text(text) ] }
}

fn red_line(text: String) -> impl Scene + use<> {
    let red = UiColor::Red.base();
    bsn! {
        Node
        Propagate::<TextColor>(TextColor(red))
        Children [ Text(text) ]
    }
}

fn view(data: &NewscasterData) -> impl Scene + use<> {
    let user = format!(
        "{} ({}, {})",
        data.user.name, data.user.job, data.user.department
    );
    let wanted = data
        .viewing_wanted
        .then(|| EntityScene(wanted_screen(data)));
    let channels = EntityScene(channel_selector(data));
    let channel = EntityScene(channel_box(data));
    let messages = EntityScene(channel_messages(data));
    bsn! {
        @WindowContent
        Children [
            @Section { @title: "Newscaster" }
            Children [ {EntityScene(line(user))} ]
            --
            {wanted}
            --
            {channels}
            --
            {channel}
            --
            {messages}
        ]
    }
}

/// `NewscasterChannelSelector`.
fn channel_selector(data: &NewscasterData) -> impl Scene + use<> {
    let wanted_label = if data.wanted.is_some() {
        "Wanted Issue (active)".to_owned()
    } else {
        "Wanted Issue".to_owned()
    };
    let wanted = EntityScene(act_button(
        "skull-crossbones",
        wanted_label,
        TOGGLE_WANTED,
        ActParams(Vec::new()),
        false,
    ));
    let channels: Vec<_> = data
        .channels
        .iter()
        .map(|c| {
            let label = if c.censored {
                format!("{} [D-Notice]", c.name)
            } else {
                c.name.clone()
            };
            let selected = data.viewing_channel == Some(c.id);
            let params = params(CHANNEL, c.id);
            let icon = if c.censored { "ban" } else { "newspaper" };
            bsn! {
                @ss13_ui_kit::Button { @icon: icon, @content: label, @fluid: true }
                ss13_ui_kit::Button { selected: selected }
                Act({SET_CHANNEL.to_owned()})
                params
            }
        })
        .collect();
    let create = EntityScene(act_button(
        "plus",
        "Create Channel [+]".to_owned(),
        START_CREATE_CHANNEL,
        ActParams(Vec::new()),
        false,
    ));
    bsn! {
        @Section { @title: "Channels" }
        Children [
            Node { flex_direction: FlexDirection::Column, row_gap: px(2) }
            Children [ {wanted} -- {channels} -- {create} ]
        ]
    }
}

/// `NewscasterChannelBox`.
fn channel_box(data: &NewscasterData) -> impl Scene + use<> {
    let title = if data.channel_name.is_empty() {
        "Newscaster Net".to_owned()
    } else {
        data.channel_name.clone()
    };
    let desc = data
        .channel_censored
        .then(|| EntityScene(red_line(format!("ATTENTION: {CENSOR_MESSAGE}"))));
    let plain_desc = (!data.channel_censored).then(|| {
        EntityScene(line(format!(
            "{} (by {})",
            data.channel_desc, data.channel_author
        )))
    });
    let channel = data.viewing_channel.unwrap_or_default();
    let submit = EntityScene(act_button(
        "print",
        "Submit Story".to_owned(),
        CREATE_STORY,
        ActParams(Vec::new()),
        !data.can_submit(),
    ));
    let notice = (data.admin_mode && data.viewing_channel.is_some()).then(|| {
        EntityScene(act_button(
            "ban",
            "D-Notice".to_owned(),
            CHANNEL_D_NOTICE,
            params(CHANNEL, channel),
            false,
        ))
    });
    let print = EntityScene(act_button(
        "newspaper",
        format!("Print Newspaper ({})", data.paper),
        PRINT_NEWSPAPER,
        ActParams(Vec::new()),
        data.paper == 0,
    ));
    bsn! {
        @Section { @title: title }
        Children [
            {desc}
            --
            {plain_desc}
            --
            Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, margin: UiRect::top(px(6)) }
            Children [ {submit} -- {notice} -- {print} ]
        ]
    }
}

/// `NewscasterChannelMessages`.
fn channel_messages(data: &NewscasterData) -> impl Scene + use<> {
    let censored = data.channel_censored.then(|| {
        EntityScene(red_line(
            "ATTENTION: Comments cannot be read at this time. Thank you for your understanding, \
             and have a secure day."
                .to_owned(),
        ))
    });
    let stories: Vec<_> = if data.channel_censored {
        Vec::new()
    } else {
        data.messages.iter().rev().map(|m| story(data, m)).collect()
    };
    bsn! {
        Node { flex_direction: FlexDirection::Column }
        Children [ {censored} -- {stories} ]
    }
}

fn story(data: &NewscasterData, m: &NewsMessage) -> impl Scene + use<> {
    let title = if m.censored_author {
        "By: [REDACTED]. D-Notice Notice.".to_owned()
    } else {
        format!("By: {} at {}", m.auth, m.time)
    };
    let censor = data.admin_mode.then(|| {
        EntityScene(act_button(
            "comment-slash",
            String::new(),
            STORY_CENSOR,
            params(MESSAGE_ID, m.id),
            false,
        ))
    });
    let author = data.admin_mode.then(|| {
        EntityScene(act_button(
            "user-slash",
            String::new(),
            AUTHOR_CENSOR,
            params(MESSAGE_ID, m.id),
            false,
        ))
    });
    let comment = EntityScene(act_button(
        "comment",
        String::new(),
        START_COMMENT,
        params(MESSAGE_ID, m.id),
        !data.can_comment(m),
    ));
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row }
        Children [ {censor} -- {author} -- {comment} ]
    }));
    let body = m.censored_message.then(|| {
        EntityScene(red_line(
            "This message was deemed dangerous to the general welfare of the station and \
             therefore marked with a D-Notice."
                .to_owned(),
        ))
    });
    let plain = (!m.censored_message).then(|| EntityScene(line(m.body.clone())));
    let comments: Vec<_> = m
        .comments
        .iter()
        .map(|c| {
            let head = format!("By: {} at {}", c.auth, c.time);
            let text = c.body.clone();
            bsn! {
                Node { flex_direction: FlexDirection::Column, margin: UiRect::left(px(20)) }
                Children [
                    Node Children [ Text(head) ]
                    --
                    Node Children [ Text(text) ]
                ]
            }
        })
        .collect();
    bsn! {
        @Section { @title: title, @buttons: buttons }
        Children [ {body} -- {plain} -- {comments} ]
    }
}

/// `NewscasterWantedScreen`.
fn wanted_screen(data: &NewscasterData) -> impl Scene + use<> {
    let close: Box<dyn SceneList> = Box::new(EntityScene(act_button(
        "times",
        String::new(),
        CANCEL_CREATION,
        ActParams(Vec::new()),
        false,
    )));
    let issue = data.wanted.as_ref().map(|w| {
        let criminal = w.criminal.clone();
        let crime = w.crime.clone();
        let author = format!("Posted by {}", w.author);
        EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Column }
            Children [
                {EntityScene(red_line("Active Wanted Issue:".to_owned()))}
                --
                Node Children [ Text(criminal) ]
                --
                Node Children [ Text(crime) ]
                --
                Node Children [ Text(author) ]
            ]
        })
    });
    let security = data.security_mode.then(|| {
        let name = if data.criminal_name.is_empty() {
            "Criminal Name: N/A".to_owned()
        } else {
            format!("Criminal Name: {}", data.criminal_name)
        };
        let crime = if data.crime_description.is_empty() {
            "Criminal Activity: N/A".to_owned()
        } else {
            format!("Criminal Activity: {}", data.crime_description)
        };
        let none = || ActParams(Vec::new());
        let set_name = EntityScene(act_button("pen", name, SET_CRIMINAL_NAME, none(), false));
        let set_crime = EntityScene(act_button("pen", crime, SET_CRIME_DATA, none(), false));
        let submit = EntityScene(act_button(
            "volume-up",
            "Set Wanted Issue".to_owned(),
            SUBMIT_WANTED_ISSUE,
            none(),
            false,
        ));
        let clear = EntityScene(act_button(
            "times",
            "Clear Wanted".to_owned(),
            CLEAR_WANTED_ISSUE,
            none(),
            false,
        ));
        EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Column, row_gap: px(2), margin: UiRect::top(px(6)) }
            Children [ {set_name} -- {set_crime} -- {submit} -- {clear} ]
        })
    });
    let notice = (!data.security_mode).then(|| {
        let text = if data.wanted.is_some() {
            "Please contact your local security officer if spotted."
        } else {
            "No wanted issue posted. Have a secure day."
        };
        EntityScene(line(text.to_owned()))
    });
    bsn! {
        @Section { @title: "Wanted Issue", @buttons: close }
        Children [ {issue} -- {security} -- {notice} ]
    }
}
