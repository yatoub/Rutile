use gtk4::prelude::*;
use libadwaita as adw;

use rutile::{app, cli};

fn main() -> gtk4::glib::ExitCode {
    // The command line goes to whichever instance owns the application id
    // (`app::command_line`) — a second `rutile` hands its arguments over to
    // the running one and exits — unless `--new-process` asks for a
    // separate process.
    let mut flags = gtk4::gio::ApplicationFlags::HANDLES_COMMAND_LINE;
    if cli::wants_new_process(std::env::args_os()) {
        flags |= gtk4::gio::ApplicationFlags::NON_UNIQUE;
    }
    let application = adw::Application::builder()
        .application_id("dev.rutile.Rutile")
        .flags(flags)
        .build();

    application.connect_activate(app::build);
    application.connect_command_line(app::command_line);

    application.run()
}
