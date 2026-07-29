use gtk4::gdk;
use gtk4::gio;
use gtk4::prelude::*;
use vte4::TerminalExt;

/// Ctrl+Click-to-open for OSC 8 hyperlinks (the escape sequence modern CLI
/// tools — `ls --hyperlink`, `rg`, various formatters — use to mark up a
/// piece of output as a real link).
///
/// Deliberately scoped to OSC 8 only, not arbitrary plain-text URL
/// detection: `docs/ROADMAP.md`'s Phase 3 plan flagged "click a bare URL
/// in output" as the riskiest item, contingent on `vte_terminal_
/// match_check_event` — that call isn't exposed by the `vte4` crate at
/// all (checked: no `match_check`/`hyperlink_check` binding anywhere in
/// vte4 0.10's generated API, only the OSC-8-specific
/// `check_hyperlink_at`). Reaching for raw FFI to call the missing C
/// function was rejected as disproportionate for one feature; plain-URL
/// detection stays deferred until the crate exposes it (or a regex-based
/// fallback becomes worth the FFI cost on its own).
pub fn attach(terminal: &vte4::Terminal) {
    terminal.set_allow_hyperlink(true);

    let click = gtk4::GestureClick::new();
    click.set_button(gdk::BUTTON_PRIMARY);
    // Capture phase: run before VTE's own click handling gets a chance to
    // treat this as a normal selection click.
    click.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let terminal = terminal.clone();
        click.connect_pressed(move |gesture, _n_press, x, y| {
            let state = gesture.current_event_state();
            if !state.contains(gdk::ModifierType::CONTROL_MASK) {
                return;
            }
            let Some(uri) = terminal.check_hyperlink_at(x, y) else {
                return;
            };
            if let Err(err) =
                gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE)
            {
                eprintln!("[rutile] failed to open hyperlink {uri}: {err}");
            }
        });
    }
    terminal.add_controller(click);
}
