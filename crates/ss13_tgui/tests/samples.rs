//! Every view the client registers has a gallery sample that decodes as that view expects.

use bevy::prelude::*;
use ss13_tgui::client::{TguiClientPlugin, view_decodes, view_names};
use ss13_tgui::samples::sample;

#[test]
fn every_view_has_a_sample_it_decodes() {
    let mut app = App::new();
    app.add_plugins(TguiClientPlugin);
    let names = view_names(app.world());
    assert!(names.len() >= 30, "{names:?}");
    let mut missing = Vec::new();
    for name in names {
        match sample(name) {
            Some(s) => assert!(
                view_decodes(app.world(), name, &s.static_data, &s.data),
                "{name}: the sample does not decode"
            ),
            None => missing.push(name),
        }
    }
    assert!(missing.is_empty(), "views without a sample: {missing:?}");
}
