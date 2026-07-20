use gtk4::prelude::*;
use libadwaita as adw;

use rutile::app;

fn main() -> gtk4::glib::ExitCode {
    let application = adw::Application::builder()
        .application_id("dev.rutile.Rutile")
        .build();

    application.connect_activate(app::build);

    application.run()
}
