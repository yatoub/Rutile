use gtk4::prelude::*;
use libadwaita as adw;

use rutile::{app, cli};

fn main() -> gtk4::glib::ExitCode {
    // The command line goes to whichever instance owns the application id
    // (`app::command_line`) — a second `rutile` hands its arguments over to
    // the running one and exits — unless `--new-process` asks for a
    // separate process.
    let mut flags = gtk4::gio::ApplicationFlags::HANDLES_COMMAND_LINE;
    // Semgrep's args-os rule warns against trusting argv[0], which the
    // caller controls: `wants_new_process` skips it and only looks for a
    // literal `--new-process` flag, so there's nothing to trust here.
    // nosemgrep: rust.lang.security.args-os.args-os
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
