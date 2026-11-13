//! Developer screenshot run (`THINKHUB_SHOTS=<dir>`): renders ThinkHub in every window state
//! (compact, windowed, fullscreen), in light and dark, with the primary menu and dialogs open.
//! Only our own window is rendered; nothing is clicked that changes hardware. Use an isolated
//! XDG_CONFIG_HOME because the run saves appearance choices.
use super::*;
use crate::selftest::Handles;

struct Shot {
    action: Box<dyn Fn(&Rc<Ui>, &Handles)>,
    capture: Option<String>,
    popover: bool,
}

fn shot(
    action: impl Fn(&Rc<Ui>, &Handles) + 'static,
    capture: Option<String>,
    popover: bool,
) -> Shot {
    Shot {
        action: Box::new(action),
        capture,
        popover,
    }
}

fn menu_button(ui: &Rc<Ui>) -> Option<gtk::MenuButton> {
    descendants(ui.window.upcast_ref())
        .into_iter()
        .find_map(|w| w.downcast::<gtk::MenuButton>().ok())
}

fn close_everything(ui: &Rc<Ui>, h: &Handles) {
    if let Some(button) = menu_button(ui) {
        if let Some(popover) = button.popover() {
            popover.popdown();
        }
    }
    if let Some(dialog) = ui.window.visible_dialog() {
        dialog.close();
    }
    h.search_toggle.set_active(false);
    h.search_entry.set_text("");
}

fn scenes(theme: appearance::Theme, name: &str) -> Vec<Shot> {
    let n = |scene: &str| Some(format!("{name}-{scene}"));
    let mut s: Vec<Shot> = vec![];
    // Theme and a windowed layout.
    s.push(shot(
        move |ui, h| {
            close_everything(ui, h);
            let value = appearance::Appearance {
                theme,
                ..appearance::Appearance::default()
            };
            ui.set_appearance(value, &gtk::Label::new(None));
            ui.window.unfullscreen();
        },
        None,
        false,
    ));
    s.push(shot(
        |ui, _| {
            ui.window.set_default_size(1100, 760);
            ui.nav[0].emit_clicked();
        },
        None,
        false,
    ));
    s.push(shot(|_, _| {}, None, false));
    s.push(shot(|_, _| {}, None, false));
    s.push(shot(|_, _| {}, n("windowed-1-overview"), false));
    s.push(shot(
        |ui, _| ui.nav[2].emit_clicked(),
        n("windowed-2-power-and-thermals"),
        false,
    ));
    s.push(shot(
        |ui, _| ui.nav[3].emit_clicked(),
        n("windowed-3-display-and-input"),
        false,
    ));
    s.push(shot(
        |ui, _| ui.nav[6].emit_clicked(),
        n("windowed-4-customize"),
        false,
    ));
    s.push(shot(
        |ui, h| {
            ui.nav[0].emit_clicked();
            h.search_toggle.set_active(true);
            h.search_entry.set_text("power");
        },
        n("windowed-5-search"),
        false,
    ));
    s.push(shot(
        |ui, h| {
            close_everything(ui, h);
            if let Some(button) = menu_button(ui) {
                button.emit_by_name::<()>("activate", &[]);
            }
        },
        n("windowed-6-menu-open"),
        true,
    ));
    s.push(shot(
        |ui, h| {
            close_everything(ui, h);
            let _ = gtk::prelude::WidgetExt::activate_action(&ui.window, "win.about", None);
        },
        n("windowed-7-about"),
        false,
    ));
    s.push(shot(
        |ui, h| {
            close_everything(ui, h);
            let _ = gtk::prelude::WidgetExt::activate_action(&ui.window, "win.show-help-overlay", None);
        },
        n("windowed-8-keyboard-shortcuts"),
        false,
    ));
    s.push(shot(
        |ui, h| {
            close_everything(ui, h);
            let _ = gtk::prelude::WidgetExt::activate_action(&ui.window, "win.feedback", None);
        },
        n("windowed-9-send-feedback"),
        false,
    ));
    // Compact layout.
    s.push(shot(
        |ui, h| {
            close_everything(ui, h);
            ui.window.set_default_size(480, 800);
        },
        None,
        false,
    ));
    s.push(shot(|_, _| {}, None, false));
    s.push(shot(
        |_, h| h.split.set_show_content(false),
        n("compact-1-page-list"),
        false,
    ));
    s.push(shot(
        |ui, _| ui.nav[1].emit_clicked(),
        n("compact-2-batteries-page"),
        false,
    ));
    s.push(shot(
        |ui, h| {
            h.split.set_show_content(false);
            if let Some(button) = menu_button(ui) {
                button.emit_by_name::<()>("activate", &[]);
            }
        },
        n("compact-3-menu-open"),
        true,
    ));
    s.push(shot(
        |ui, h| {
            close_everything(ui, h);
            h.split.set_show_content(false);
            let _ = gtk::prelude::WidgetExt::activate_action(&ui.window, "win.about", None);
        },
        n("compact-4-about"),
        false,
    ));
    // Fullscreen.
    s.push(shot(
        |ui, h| {
            close_everything(ui, h);
            ui.window.fullscreen();
            ui.nav[0].emit_clicked();
        },
        None,
        false,
    ));
    s.push(shot(|_, _| {}, None, false));
    s.push(shot(|_, _| {}, n("fullscreen-1-overview"), false));
    s.push(shot(
        |ui, h| {
            close_everything(ui, h);
            if let Some(button) = menu_button(ui) {
                button.emit_by_name::<()>("activate", &[]);
            }
        },
        n("fullscreen-2-menu-open"),
        true,
    ));
    s
}

fn render(ui: &Rc<Ui>, directory: &std::path::Path, name: &str, with_popover: bool) -> Result<(), String> {
    let (w, h) = (ui.window.width() as f64, ui.window.height() as f64);
    let snapshot = gtk::Snapshot::new();
    gtk::WidgetPaintable::new(Some(&ui.window)).snapshot(&snapshot, w, h);
    if with_popover {
        let button = menu_button(ui).ok_or("no menu button")?;
        let popover = button.popover().ok_or("menu button has no popover")?;
        let (mut pw, mut ph) = (popover.width() as f64, popover.height() as f64);
        if pw < 10.0 || ph < 10.0 {
            let (_, natural, _, _) = popover.measure(gtk::Orientation::Horizontal, -1);
            pw = natural as f64;
            let (_, natural_h, _, _) = popover.measure(gtk::Orientation::Vertical, natural);
            ph = natural_h as f64;
        }
        eprintln!(
            "DEBUG popover visible={} mapped={} size={pw}x{ph}",
            popover.is_visible(),
            popover.is_mapped()
        );
        if !popover.is_visible() || pw < 10.0 || ph < 10.0 {
            return Err(format!("popover not shown ({pw}x{ph})"));
        }
        let bounds = button
            .compute_bounds(&ui.window)
            .ok_or("cannot locate the menu button")?;
        // Popovers are separate surfaces: draw ours where the menu button anchors it,
        // keeping it inside the window like the compositor does.
        let centre = bounds.x() as f64 + bounds.width() as f64 / 2.0;
        let x = (centre - pw / 2.0).clamp(6.0, (w - pw - 6.0).max(6.0));
        let y = bounds.y() as f64 + bounds.height() as f64 + 4.0;
        snapshot.save();
        snapshot.translate(&gtk::graphene::Point::new(x as f32, y as f32));
        gtk::WidgetPaintable::new(Some(&popover)).snapshot(&snapshot, pw, ph);
        snapshot.restore();
    }
    let node = snapshot.to_node().ok_or("empty snapshot")?;
    let renderer = ui.window.renderer().ok_or("no renderer")?;
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    renderer
        .render_texture(&node, None)
        .save_to_png(directory.join(format!("{name}.png")))
        .map_err(|e| e.to_string())
}

pub fn run(ui: Rc<Ui>, handles: Handles, directory: std::path::PathBuf) {
    let mut shots = scenes(appearance::Theme::Light, "light");
    shots.extend(scenes(appearance::Theme::Dark, "dark"));
    let state = Rc::new(Cell::new((0usize, false, 0u32)));
    glib::timeout_add_local(Duration::from_millis(1300), move || {
        let (index, acted, waited) = state.get();
        if !ui.loaded.get() {
            state.set((index, acted, waited + 1));
            if waited > 60 {
                println!("FAIL  backend never loaded");
                std::process::exit(1);
            }
            return glib::ControlFlow::Continue;
        }
        if index >= shots.len() {
            println!("done");
            std::process::exit(0);
        }
        let current = &shots[index];
        if !acted {
            (current.action)(&ui, &handles);
            state.set((index, true, 0));
        } else {
            if let Some(name) = &current.capture {
                match render(&ui, &directory, name, current.popover) {
                    Ok(()) => println!("saved {name}.png ({}x{})", ui.window.width(), ui.window.height()),
                    Err(e) => println!("FAILED {name}: {e}"),
                }
            }
            state.set((index + 1, false, 0));
        }
        glib::ControlFlow::Continue
    });
}
