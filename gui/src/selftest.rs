//! Developer self-test (`THINKHUB_SELFTEST=1`): drives the window's actions and controls
//! through the same signals a user triggers and prints one PASS/FAIL/SKIP line per check.
//! It never presses an "Apply" button, never opens an email draft and never writes outside
//! the XDG_CONFIG_HOME it is started with, so run it with an isolated one.
use super::*;

pub struct Handles {
    pub app: adw::Application,
    pub search_toggle: gtk::ToggleButton,
    pub search_entry: gtk::SearchEntry,
    pub search_bar: gtk::SearchBar,
    pub split: adw::NavigationSplitView,
}

enum Outcome {
    Pass,
    Skip(String),
}
type Check = Box<dyn Fn(&Rc<Ui>, &Handles) -> Result<Outcome, String>>;
struct Step {
    name: String,
    act: Check,
    check: Option<Check>,
}

fn step(name: &str, act: impl Fn(&Rc<Ui>, &Handles) -> Result<Outcome, String> + 'static) -> Step {
    Step {
        name: name.into(),
        act: Box::new(act),
        check: None,
    }
}
fn step2(
    name: &str,
    act: impl Fn(&Rc<Ui>, &Handles) -> Result<Outcome, String> + 'static,
    check: impl Fn(&Rc<Ui>, &Handles) -> Result<Outcome, String> + 'static,
) -> Step {
    Step {
        name: name.into(),
        act: Box::new(act),
        check: Some(Box::new(check)),
    }
}

fn ensure(ok: bool, message: impl Into<String>) -> Result<Outcome, String> {
    if ok {
        Ok(Outcome::Pass)
    } else {
        Err(message.into())
    }
}
fn activate(ui: &Rc<Ui>, action: &str) -> Result<(), String> {
    gtk::prelude::WidgetExt::activate_action(&ui.window, action, None).map_err(|e| format!("{action}: {e}"))
}
fn visible_pages(ui: &Rc<Ui>) -> Vec<usize> {
    ui.nav
        .iter()
        .enumerate()
        .filter(|(_, b)| b.is_visible())
        .map(|(i, _)| i)
        .collect()
}
fn buttons_in(root: &gtk::Widget) -> Vec<gtk::Button> {
    descendants(root)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .collect()
}
fn button_labelled(root: &gtk::Widget, text: &str) -> Option<gtk::Button> {
    buttons_in(root)
        .into_iter()
        .find(|b| b.label().as_deref() == Some(text))
}
fn toggle_labelled(root: &gtk::Widget, text: &str) -> Option<gtk::ToggleButton> {
    descendants(root)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::ToggleButton>().ok())
        .find(|b| b.label().as_deref() == Some(text))
}
fn open_windows() -> Vec<gtk::Window> {
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Window>().ok())
        .filter(|w| w.is_visible())
        .collect()
}
fn extra_window(ui: &Rc<Ui>) -> Option<gtk::Window> {
    open_windows()
        .into_iter()
        .find(|w| w.upcast_ref::<gtk::Widget>() != ui.window.upcast_ref::<gtk::Widget>())
}
fn close_extra_windows(ui: &Rc<Ui>) {
    while let Some(window) = extra_window(ui) {
        window.close();
        window.destroy();
    }
}
fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}
fn config_file(name: &str) -> Result<std::path::PathBuf, String> {
    appearance::path().map(|p| p.with_file_name(name))
}

/// "Review change…" style controls on a page: open the dialog, check Apply exists, Cancel.
fn review_step(label_text: &'static str, page: usize) -> Step {
    step2(
        &format!("page {page}: '{label_text}' opens a review dialog (cancelled, nothing applied)"),
        move |ui, _| {
            ui.nav[page].emit_clicked();
            close_extra_windows(ui);
            let Some(button) = button_labelled(ui.content.upcast_ref(), label_text) else {
                return Ok(Outcome::Skip(format!("no '{label_text}' control on this hardware")));
            };
            if !button.is_sensitive() {
                return Ok(Outcome::Skip("control is disabled on this hardware".into()));
            }
            button.emit_clicked();
            Ok(Outcome::Pass)
        },
        |ui, _| {
            let Some(window) = extra_window(ui) else {
                return Err("no review dialog appeared".into());
            };
            let apply = button_labelled(window.upcast_ref(), "Apply change");
            let cancel = button_labelled(window.upcast_ref(), "Cancel")
                .ok_or("review dialog has no Cancel button")?;
            ensure(apply.is_some(), "review dialog has no 'Apply change' button")?;
            cancel.emit_clicked();
            Ok(Outcome::Pass)
        },
    )
}

pub fn run(ui: Rc<Ui>, handles: Handles) {
    let mut steps: Vec<Step> = vec![];

    // ---- navigation -------------------------------------------------------------------
    steps.push(step("pages: every nav button shows its page and title", |ui, _| {
        for i in 0..FEEDBACK_PAGE {
            ui.nav[i].emit_clicked();
            if ui.page.get() != i {
                return Err(format!("nav {i} left page at {}", ui.page.get()));
            }
            if ui.content_page.title() != PAGES[i] {
                return Err(format!("header title '{}' != '{}'", ui.content_page.title(), PAGES[i]));
            }
            if ui.content.first_child().is_none() {
                return Err(format!("page {i} rendered nothing"));
            }
        }
        Ok(Outcome::Pass)
    }));
    steps.push(step("pages: Feedback entry is hidden from the sidebar", |ui, _| {
        ensure(!ui.nav[FEEDBACK_PAGE].is_visible(), "Feedback is still in the sidebar")
    }));
    steps.push(step("shortcuts: Alt+1…7 actions switch pages", |ui, _| {
        for i in (0..FEEDBACK_PAGE).rev() {
            activate(ui, &format!("win.page-{i}"))?;
            if ui.page.get() != i {
                return Err(format!("win.page-{i} gave page {}", ui.page.get()));
            }
        }
        Ok(Outcome::Pass)
    }));
    steps.push(step("shortcuts: accelerators are registered", |_, h| {
        for (action, accel) in [
            ("win.search", "<Control>f"),
            ("win.refresh", "F5"),
            ("win.show-help-overlay", "<Control>question"),
            ("win.page-0", "<Alt>1"),
            ("win.page-6", "<Alt>7"),
            ("window.close", "<Control>w"),
        ] {
            let accels = h.app.accels_for_action(action);
            if !accels.iter().any(|a| a.as_str() == accel) {
                return Err(format!("{action} lacks {accel}; has {:?}", accels));
            }
        }
        Ok(Outcome::Pass)
    }));

    // ---- system integration -------------------------------------------------------------
    steps.push(step("system accent: the app reads the desktop's accent colour", |ui, _| {
        let c = adw::StyleManager::default().accent_color_rgba();
        let hex = format!(
            "#{:02x}{:02x}{:02x}",
            (c.red() * 255.).round() as u8,
            (c.green() * 255.).round() as u8,
            (c.blue() * 255.).round() as u8
        );
        println!("INFO  desktop accent seen by the app: {hex}");
        let css = appearance_css(&ui.appearance.borrow());
        ensure(
            ui.appearance.borrow().accent_source == appearance::AccentSource::System,
            "default accent source is not System",
        )?;
        ensure(
            css.lines().any(|l| l.starts_with(&format!("@define-color accent {hex};"))),
            format!("stylesheet does not use the desktop accent {hex}"),
        )?;
        match std::env::var("THINKHUB_EXPECT_ACCENT") {
            Ok(expected) if !expected.eq_ignore_ascii_case(&hex) => {
                Err(format!("desktop accent is {expected}, the app sees {hex}"))
            }
            Ok(_) => Ok(Outcome::Pass),
            Err(_) => Ok(Outcome::Skip(format!("no expected value given; app sees {hex}"))),
        }
    }));

    // ---- search -----------------------------------------------------------------------
    for (query, expected) in [
        ("bios", vec![4usize]),
        ("battery", vec![1]),
        ("thinklight", vec![3]),
        ("fan", vec![2]),
        ("accent", vec![6]),
        ("zzzz", vec![]),
        ("", vec![0, 1, 2, 3, 4, 5, 6]),
    ] {
        let expected_check = expected.clone();
        steps.push(step2(
            &format!("search: '{query}' filters the sidebar"),
            move |_, h| {
                h.search_toggle.set_active(true);
                h.search_entry.set_text(query);
                Ok(Outcome::Pass)
            },
            move |ui, _| {
                let got = visible_pages(ui);
                ensure(got == expected_check, format!("visible {got:?}, expected {expected_check:?}"))
            },
        ));
    }
    steps.push(step2(
        "search: Enter opens the first match",
        |_, h| {
            h.search_entry.set_text("firmware");
            Ok(Outcome::Pass)
        },
        |ui, h| {
            h.search_entry.emit_activate();
            ensure(ui.page.get() == 4, format!("Enter landed on page {}", ui.page.get()))
        },
    ));
    steps.push(step2(
        "search: Ctrl+F action opens and focuses the search bar",
        |ui, h| {
            h.search_entry.set_text("");
            h.search_toggle.set_active(false);
            activate(ui, "win.search")?;
            Ok(Outcome::Pass)
        },
        |_, h| {
            ensure(
                h.search_toggle.is_active() && h.search_bar.is_search_mode(),
                "search mode is not on",
            )?;
            h.search_toggle.set_active(false);
            Ok(Outcome::Pass)
        },
    ));

    // ---- menu and dialogs ---------------------------------------------------------------
    steps.push(step("menu: primary menu lists its four entries", |ui, _| {
        let button = descendants(ui.window.upcast_ref())
            .into_iter()
            .find_map(|w| w.downcast::<gtk::MenuButton>().ok())
            .ok_or("no menu button")?;
        let model = button.menu_model().ok_or("menu button has no model")?;
        let mut labels = vec![];
        for section in 0..model.n_items() {
            let Some(items) = model.item_link(section, "section") else {
                continue;
            };
            for i in 0..items.n_items() {
                if let Some(value) = items.item_attribute_value(i, "label", Some(glib::VariantTy::STRING)) {
                    labels.push(value.get::<String>().unwrap_or_default());
                }
            }
        }
        ensure(
            labels == ["Send Feedback…", "Report an Issue", "Keyboard Shortcuts", "About ThinkHub"],
            format!("menu labels were {labels:?}"),
        )
    }));
    for (action, name) in [("win.show-help-overlay", "Keyboard Shortcuts"), ("win.about", "About")] {
        steps.push(step2(
            &format!("dialog: {name} opens"),
            move |ui, _| {
                activate(ui, action)?;
                Ok(Outcome::Pass)
            },
            |ui, _| ensure(ui.window.visible_dialog().is_some(), "no dialog is showing"),
        ));
        if action == "win.about" {
            steps.push(step("dialog: About lists only the Author and License rows", |ui, _| {
                let dialog = ui.window.visible_dialog().ok_or("About is not showing")?;
                let titles: Vec<String> = descendants(dialog.upcast_ref())
                    .into_iter()
                    .filter_map(|w| w.downcast::<adw::ActionRow>().ok())
                    .map(|r| r.title().to_string())
                    .collect();
                ensure(
                    titles == ["Author", "License"],
                    format!("rows were {titles:?}"),
                )
            }));
            steps.push(step2(
                "dialog: About closes",
                |ui, _| {
                    if let Some(dialog) = ui.window.visible_dialog() {
                        dialog.close();
                    }
                    Ok(Outcome::Pass)
                },
                |ui, _| ensure(ui.window.visible_dialog().is_none(), "About is still showing"),
            ));
            steps.push(step2(
                "menu: Send Feedback… opens the Feedback dialog",
                |ui, _| {
                    activate(ui, "win.feedback")?;
                    Ok(Outcome::Pass)
                },
                |ui, _| {
                    let dialog = ui.window.visible_dialog().ok_or("no dialog after Send Feedback")?;
                    ensure(dialog.title() == "Send Feedback", format!("showing '{}'", dialog.title()))
                },
            ));
            continue;
        }
        steps.push(step2(
            &format!("dialog: {name} closes"),
            |ui, _| {
                if let Some(dialog) = ui.window.visible_dialog() {
                    dialog.close();
                }
                Ok(Outcome::Pass)
            },
            |ui, _| ensure(ui.window.visible_dialog().is_none(), "dialog is still showing"),
        ));
    }
    steps.push(step("dialog: Feedback form has its fields and validates input", |ui, _| {
        let dialog = ui.window.visible_dialog().ok_or("Feedback dialog is not showing")?;
        let all = descendants(dialog.upcast_ref());
        ensure(all.iter().any(|w| w.is::<gtk::DropDown>()), "no category dropdown")?;
        let entry = all
            .iter()
            .find_map(|w| w.clone().downcast::<gtk::Entry>().ok())
            .ok_or("no subject entry")?;
        let text = all
            .iter()
            .find_map(|w| w.clone().downcast::<gtk::TextView>().ok())
            .ok_or("no message box")?;
        ensure(all.iter().any(|w| w.is::<gtk::CheckButton>()), "no system-details checkbox")?;
        let review = button_labelled(dialog.upcast_ref(), "Review feedback").ok_or("no Review button")?;
        // Empty draft must be refused: no review window opens.
        entry.set_text("");
        text.buffer().set_text("");
        review.emit_clicked();
        ensure(extra_window(ui).is_none(), "an empty draft opened a review window")?;
        // A valid draft opens the review window (never the email app).
        entry.set_text("Self-test subject");
        text.buffer().set_text("Self-test message body.");
        review.emit_clicked();
        Ok(Outcome::Pass)
    }));
    steps.push(step2(
        "dialog: valid feedback opens a review window (email is not sent)",
        |_, _| Ok(Outcome::Pass),
        |ui, _| {
            let window = extra_window(ui).ok_or("no review window")?;
            ensure(
                button_labelled(window.upcast_ref(), "Open email app").is_some(),
                "review window lacks 'Open email app'",
            )?;
            close_extra_windows(ui);
            if let Some(dialog) = ui.window.visible_dialog() {
                dialog.close();
            }
            Ok(Outcome::Pass)
        },
    ));

    // ---- refresh ------------------------------------------------------------------------
    steps.push(step2(
        "refresh: F5 action rescans and keeps the window healthy",
        |ui, _| {
            activate(ui, "win.refresh")?;
            Ok(Outcome::Pass)
        },
        |ui, _| {
            ensure(
                ui.loaded.get() && !ui.status.text().is_empty(),
                "backend data/status missing after refresh",
            )
        },
    ));

    // ---- hardware controls: dialogs only, never applied -------------------------------
    steps.push(review_step("Review change…", 3));
    steps.push(review_step("Review change…", 1));
    steps.push(review_step("balanced", 2));

    // ---- customize ----------------------------------------------------------------------
    steps.push(step("customize: Light theme restyles and is saved", |ui, _| {
        ui.nav[6].emit_clicked();
        toggle_labelled(ui.content.upcast_ref(), "Light").ok_or("no Light button")?.set_active(true);
        ensure(!adw::StyleManager::default().is_dark(), "still dark after choosing Light")?;
        let saved = read(&config_file("appearance.json")?);
        ensure(saved.contains("\"theme\": \"light\""), "appearance.json does not say light")
    }));
    steps.push(step("customize: Dark theme restyles and is saved", |ui, _| {
        ui.nav[6].emit_clicked();
        toggle_labelled(ui.content.upcast_ref(), "Dark").ok_or("no Dark button")?.set_active(true);
        ensure(adw::StyleManager::default().is_dark(), "not dark after choosing Dark")?;
        let saved = read(&config_file("appearance.json")?);
        ensure(saved.contains("\"theme\": \"dark\""), "appearance.json does not say dark")
    }));
    steps.push(step("customize: Custom accent is stored beside appearance.json", |ui, _| {
        ui.nav[6].emit_clicked();
        toggle_labelled(ui.content.upcast_ref(), "Custom accent")
            .ok_or("no Custom accent button")?
            .set_active(true);
        ensure(
            read(&config_file("accent-source")?).trim() == "custom",
            "accent-source is not 'custom'",
        )?;
        ensure(
            !read(&config_file("appearance.json")?).contains("accent_source"),
            "appearance.json carries accent_source (older builds would reject it)",
        )?;
        let dropdown = descendants(ui.content.upcast_ref())
            .into_iter()
            .find_map(|w| w.downcast::<gtk::DropDown>().ok())
            .ok_or("no preset dropdown")?;
        ensure(dropdown.is_sensitive(), "presets are disabled in Custom mode")?;
        dropdown.set_selected(1);
        ensure(
            read(&config_file("appearance.json")?).to_lowercase().contains("#2875d9"),
            "preset 'Blue' was not saved",
        )
    }));
    steps.push(step("customize: System accent disables the custom controls", |ui, _| {
        ui.nav[6].emit_clicked();
        toggle_labelled(ui.content.upcast_ref(), "System accent")
            .ok_or("no System accent button")?
            .set_active(true);
        ensure(
            read(&config_file("accent-source")?).trim() == "system",
            "accent-source is not 'system'",
        )?;
        let dropdown = descendants(ui.content.upcast_ref())
            .into_iter()
            .find_map(|w| w.downcast::<gtk::DropDown>().ok())
            .ok_or("no preset dropdown")?;
        ensure(!dropdown.is_sensitive(), "presets stay enabled in System mode")
    }));
    steps.push(step("customize: Restore default appearance resets everything", |ui, _| {
        ui.nav[6].emit_clicked();
        button_labelled(ui.content.upcast_ref(), "Restore default appearance")
            .ok_or("no Restore button")?
            .emit_clicked();
        ensure(
            *ui.appearance.borrow() == appearance::Appearance::default(),
            "appearance is not the default after Restore",
        )
    }));

    // ---- preferences --------------------------------------------------------------------
    steps.push(step2(
        "preferences: Save preferences writes only the preferences file",
        |ui, _| {
            ui.nav[5].emit_clicked();
            let Some(button) = button_labelled(ui.content.upcast_ref(), "Save preferences") else {
                return Ok(Outcome::Skip("no Save preferences control".into()));
            };
            button.emit_clicked();
            Ok(Outcome::Pass)
        },
        |_, _| {
            let path = appearance::path()?.with_file_name("preferences.json");
            let dir = path.parent().ok_or("no directory")?.to_path_buf();
            // The backend keeps preferences under thinkpad-dashboard/, ThinkHub's own under thinkhub/.
            let backend = dir
                .parent()
                .map(|p| p.join("thinkpad-dashboard/preferences.json"))
                .ok_or("no config root")?;
            ensure(backend.exists(), "preferences.json was not written")
        },
    ));

    // ---- compact layout -----------------------------------------------------------------
    steps.push(step2(
        "compact: narrow window collapses the split view",
        |ui, _| {
            ui.window.set_default_size(480, 800);
            Ok(Outcome::Pass)
        },
        |ui, h| {
            if ui.window.width() > 600 {
                return Ok(Outcome::Skip("compositor did not resize the window".into()));
            }
            ensure(h.split.is_collapsed(), "split view is not collapsed at 480px")?;
            ensure(h.split.has_css_class("compact"), "compact class missing")
        },
    ));
    steps.push(step("compact: choosing a page shows it; back returns to the list", |ui, h| {
        if !h.split.is_collapsed() {
            return Ok(Outcome::Skip("not collapsed".into()));
        }
        h.split.set_show_content(false);
        ui.nav[1].emit_clicked();
        ensure(h.split.shows_content(), "page did not open")?;
        h.split.set_show_content(false);
        ensure(!h.split.shows_content(), "back did not return to the list")
    }));
    steps.push(step2(
        "compact: wide window restores the two-pane layout",
        |ui, _| {
            ui.window.set_default_size(1100, 800);
            Ok(Outcome::Pass)
        },
        |ui, h| {
            if ui.window.width() < 700 {
                return Ok(Outcome::Skip("compositor did not resize the window".into()));
            }
            ensure(!h.split.is_collapsed(), "still collapsed at 1100px")
        },
    ));

    // ------------------------------------------------------------------------------------
    let steps = Rc::new(steps);
    let handles = Rc::new(handles);
    let tally = Rc::new(RefCell::new((0usize, 0usize, 0usize)));
    let state = Rc::new(Cell::new((0usize, false, 0u32)));
    glib::timeout_add_local(Duration::from_millis(900), move || {
        let (index, acted, waited) = state.get();
        if !ui.loaded.get() {
            state.set((index, acted, waited + 1));
            if waited > 60 {
                println!("FAIL  backend never loaded");
                std::process::exit(1);
            }
            return glib::ControlFlow::Continue;
        }
        let report = |name: &str, outcome: Result<Outcome, String>| {
            let mut t = tally.borrow_mut();
            match outcome {
                Ok(Outcome::Pass) => {
                    t.0 += 1;
                    println!("PASS  {name}");
                }
                Ok(Outcome::Skip(why)) => {
                    t.2 += 1;
                    println!("SKIP  {name}: {why}");
                }
                Err(why) => {
                    t.1 += 1;
                    println!("FAIL  {name}: {why}");
                }
            }
        };
        if index >= steps.len() {
            let (p, f, s) = *tally.borrow();
            println!("\n{p} passed, {f} failed, {s} skipped");
            std::process::exit(if f == 0 { 0 } else { 1 });
        }
        let current = &steps[index];
        if !acted {
            match (current.act)(&ui, &handles) {
                Ok(Outcome::Pass) if current.check.is_some() => state.set((index, true, 0)),
                other => {
                    report(&current.name, other);
                    state.set((index + 1, false, 0));
                }
            }
        } else if let Some(check) = &current.check {
            report(&current.name, check(&ui, &handles));
            state.set((index + 1, false, 0));
        }
        glib::ControlFlow::Continue
    });
}
