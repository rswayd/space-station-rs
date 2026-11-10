//! Client half of the cryo console window (`CryopodConsole.jsx`).

use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::ActParams;
use ss13_ui_kit::prelude::*;

use crate::cryosleep::*;

pub struct CryosleepClientPlugin;

impl Plugin for CryosleepClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<CryopodConsoleUi>();
    }
}

fn item_row(item: &FrozenItem) -> impl Scene + use<> {
    let label = item.name.clone();
    let params = ActParams(vec![(ITEM_GET.into(), item.id.clone())]);
    bsn! {
        @LabeledListItem { @label: label }
        Children [
            @Button { @icon: "exclamation-circle", @content: "Извлечь" }
            Button { color: UiColor::Bad }
            Act("item_get")
            params
        ]
    }
}

impl ClientUi for CryopodConsoleUi {
    const WIDTH: f32 = 400.0;
    const HEIGHT: f32 = 480.0;

    fn content(_: &(), data: &CryopodConsoleData) -> impl Scene {
        let welcome = format!(
            "Добро пожаловать, {}!",
            data.account_name.as_deref().unwrap_or("Неизвестный")
        );
        let crew: Vec<_> = data
            .frozen_crew
            .iter()
            .map(|p| {
                let (name, job) = (p.name.clone(), p.job.clone());
                bsn! {
                    @LabeledListItem { @label: name }
                    Children [ Text(job) ]
                }
            })
            .collect();
        let items: Vec<_> = data.items.iter().map(item_row).collect();
        let crew_empty = if data.frozen_crew.is_empty() {
            "Нет экипажа на хранении!"
        } else {
            ""
        };
        let items_note = if !data.item_retrieval_allowed {
            "Вы не авторизованы для менеджмента предметов."
        } else if data.items.is_empty() {
            "Нет предметов на хранении!"
        } else {
            ""
        };
        let items_display = if data.item_retrieval_allowed {
            Display::Flex
        } else {
            Display::None
        };
        bsn! {
            @WindowContent
            Children [
                @Section { @title: welcome }
                Children [
                    Text("Данная автоматизированная система криогенной заморозки надежно \
                          сохранит вашу материальную оболочку до следующего назначения.")
                ]
                --
                @Section { @title: "Экипаж на хранении" }
                Children [
                    Text(crew_empty)
                    --
                    @LabeledList Children [ {crew} ]
                ]
                --
                @Section { @title: "Предметы на хранении" }
                Children [
                    Text(items_note)
                    --
                    @LabeledList
                    Node { display: items_display }
                    Children [ {items} ]
                ]
            ]
        }
    }
}
