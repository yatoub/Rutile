pub mod bookmarks;
pub mod config;
pub mod profiles;
pub mod triggers;
pub mod window;

pub use config::{
    BackgroundImageMode, NewInstanceMode, Preferences, SidebarPosition, ThemeVariant, WindowStyle,
};

use gtk4::prelude::*;

/// Item factory for a `ComboRow` over a `gtk4::StringList` that shows the
/// selected value in full: libadwaita's own one ellipsizes it as soon as
/// the row's title and subtitle are long (a translated one, typically),
/// leaving "Ouvrir une…" — here the title wraps instead.
pub(crate) fn combo_value_factory() -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        item.set_child(Some(&gtk4::Label::builder().xalign(0.0).build()));
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let text = item
            .item()
            .and_downcast::<gtk4::StringObject>()
            .map(|string| string.string());
        if let (Some(label), Some(text)) = (item.child().and_downcast::<gtk4::Label>(), text) {
            label.set_label(&text);
        }
    });
    factory
}
