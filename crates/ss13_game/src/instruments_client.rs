//! The music windows (client): tgui `InstrumentEditor220.tsx` and `Jukebox220.tsx`. The data is
//! `crate::instruments_ui`, the server half `crate::instruments`.
// shortcut: plain buttons and text instead of tgui's sliders, number inputs and the BandaStation
// multi-sync panel; the steps are the buttons' (1/10 BPM, 1 volume, 1 repeat, 1 semitone).

use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::ActParams;
use ss13_ui_kit::prelude::*;

use crate::instruments_ui::*;

pub struct InstrumentsClientPlugin;

impl Plugin for InstrumentsClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<InstrumentEditorUi>()
            .register_ui_view::<JukeboxUi>();
    }
}

fn button(label: String, action: &str, params: Vec<(String, String)>) -> Box<dyn Scene> {
    let action = action.to_owned();
    let params = ActParams(params);
    Box::new(bsn! {
        @Button { @content: label }
        Act(action)
        params
    })
}

fn toggle(label: String, action: &str, params: Vec<(String, String)>, on: bool) -> Box<dyn Scene> {
    let action = action.to_owned();
    let params = ActParams(params);
    Box::new(bsn! {
        @Button { @content: label }
        Button { selected: on }
        Act(action)
        params
    })
}

fn text(t: String) -> Box<dyn Scene> {
    Box::new(bsn! { Text(t) })
}

fn amount(value: impl ToString) -> Vec<(String, String)> {
    vec![("amount".to_owned(), value.to_string())]
}

/// A "label: value  [-] [+]" row of buttons sending `action` with `amount` = value ± step.
fn stepper(rows: &mut Vec<Box<dyn Scene>>, label: &str, value: i64, step: i64, action: &str) {
    rows.push(text(format!("{label}: {value}")));
    rows.push(button(format!("−{step}"), action, amount(value - step)));
    rows.push(button(format!("+{step}"), action, amount(value + step)));
}

impl ClientUi for InstrumentEditorUi {
    const WIDTH: f32 = 700.0;
    const HEIGHT: f32 = 500.0;

    fn content(statics: &InstrumentEditorStatic, data: &InstrumentEditorData) -> impl Scene {
        editor_view(statics, data)
    }
}

fn editor_view(s: &InstrumentEditorStatic, d: &InstrumentEditorData) -> impl Scene + use<> {
    let mut status: Vec<Box<dyn Scene>> = Vec::new();
    status.push(text(format!("Инструмент: {}", d.using_instrument)));
    if !d.instrument_ready {
        status.push(text("Инструмент не готов!".to_owned()));
    }
    status.push(toggle(
        if d.playing {
            "Стоп"
        } else {
            "Играть"
        }
        .to_owned(),
        "play_music",
        Vec::new(),
        d.playing,
    ));
    if s.can_switch_instrument {
        for instrument in &s.possible_instruments {
            let selected = instrument.name == d.using_instrument;
            let params = vec![("new_instrument".to_owned(), instrument.id.clone())];
            status.push(toggle(
                instrument.name.clone(),
                "change_instrument",
                params,
                selected,
            ));
        }
    }
    status.push(text(format!("ID синхронизации: {}", d.id)));
    let mut settings: Vec<Box<dyn Scene>> = Vec::new();
    stepper(&mut settings, "BPM", i64::from(d.bpm), 1, "set_bpm");
    let fast = |dir: &str| vec![("tempo_change".to_owned(), dir.to_owned())];
    settings.push(button(
        "−10".to_owned(),
        "tempo_big_step",
        fast("decrease_speed"),
    ));
    settings.push(button(
        "+10".to_owned(),
        "tempo_big_step",
        fast("increase_speed"),
    ));
    stepper(
        &mut settings,
        "Повторы",
        i64::from(d.repeat),
        1,
        "set_repeat_amount",
    );
    stepper(
        &mut settings,
        "Громкость",
        i64::from(d.volume),
        1,
        "set_volume",
    );
    stepper(
        &mut settings,
        "Сдвиг нот",
        i64::from(d.note_shift),
        1,
        "set_note_shift",
    );
    for mode in &s.sustain_modes {
        let params = vec![("new_mode".to_owned(), mode.clone())];
        settings.push(toggle(
            mode.clone(),
            "set_sustain_mode",
            params,
            *mode == d.sustain_mode,
        ));
    }
    settings.push(text(format!(
        "{}: {}",
        d.sustain_mode_button, d.sustain_mode_duration
    )));
    settings.push(toggle(
        "Держать последнюю ноту".to_owned(),
        "toggle_sustain_hold_indefinitely",
        Vec::new(),
        d.sustain_indefinitely,
    ));
    let mut song: Vec<Box<dyn Scene>> = vec![
        button("Новая песня".to_owned(), "start_new_song", Vec::new()),
        button("Импорт".to_owned(), "import_song", Vec::new()),
        button("Добавить строку".to_owned(), "add_new_line", Vec::new()),
    ];
    for (i, line) in d.lines.iter().enumerate() {
        let n = i + 1;
        song.push(text(format!("{n}: {line}")));
        song.push(button(
            "Изменить".to_owned(),
            "modify_line",
            vec![("line_editing".to_owned(), n.to_string())],
        ));
        song.push(button(
            "Удалить".to_owned(),
            "delete_line",
            vec![("line_deleted".to_owned(), n.to_string())],
        ));
    }
    bsn! {
        @WindowContent
        Children [
            @Section { @title: "Инструмент" }
            Children [ {status} ]
            --
            @Section { @title: "Настройки" }
            Children [ {settings} ]
            --
            @Section { @title: "Песня", @scrollable: true }
            Children [ {song} ]
        ]
    }
}

impl ClientUi for JukeboxUi {
    const WIDTH: f32 = 450.0;
    const HEIGHT: f32 = 500.0;

    fn content(_: &(), data: &JukeboxData) -> impl Scene {
        jukebox_view(data)
    }
}

fn jukebox_view(d: &JukeboxData) -> impl Scene + use<> {
    let mut controls: Vec<Box<dyn Scene>> = vec![
        toggle(
            if d.active { "Стоп" } else { "Играть" }.to_owned(),
            "toggle",
            Vec::new(),
            d.active,
        ),
        toggle(
            "Повтор".to_owned(),
            "loop",
            vec![("looping".to_owned(), (!d.looping).to_string())],
            d.looping,
        ),
    ];
    let level = |v: &str| vec![("volume".to_owned(), v.to_owned())];
    controls.push(text(format!("Громкость: {}", d.volume)));
    controls.push(button("Мин".to_owned(), "set_volume", level("min")));
    controls.push(button(
        "−5".to_owned(),
        "set_volume",
        level(&(d.volume - 5.0).max(0.0).to_string()),
    ));
    controls.push(button(
        "+5".to_owned(),
        "set_volume",
        level(&(d.volume + 5.0).to_string()),
    ));
    controls.push(button("Макс".to_owned(), "set_volume", level("max")));
    let mut tracks: Vec<Box<dyn Scene>> = Vec::new();
    for song in &d.songs {
        let selected = d.track_selected.as_deref() == Some(song.name.as_str());
        let secs = song.length / 10;
        let label = format!("{} ({}:{:02})", song.name, secs / 60, secs % 60);
        let params = vec![("track".to_owned(), song.name.clone())];
        tracks.push(toggle(label, "select_track", params, selected));
    }
    bsn! {
        @WindowContent
        Children [
            @Section { @title: "Музыкальный автомат" }
            Children [ {controls} ]
            --
            @Section { @title: "Треки", @scrollable: true }
            Children [ {tracks} ]
        ]
    }
}
