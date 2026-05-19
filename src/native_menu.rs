use crate::panel_window;

use std::collections::HashMap;

use muda::{
    AboutMetadata, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu,
    accelerator::{Accelerator, Code, Modifiers},
};

pub(crate) struct NativeMenu {
    _menu: Menu,
    panel_items: HashMap<MenuId, panel_window::Kind>,
}

impl NativeMenu {
    pub(crate) fn install() -> Result<Self, String> {
        let menu = Menu::new();
        let mut panel_items = HashMap::new();

        let app = app_menu(&mut panel_items)?;
        let file = file_menu(&mut panel_items)?;
        let edit = edit_menu(&mut panel_items)?;
        let view = view_menu(&mut panel_items)?;
        let window = window_menu(&mut panel_items)?;
        let help = help_menu(&mut panel_items)?;

        let pnl = custom_panel_menu(
            &mut panel_items,
            "PnL",
            "flowsurface-native-pnl",
            "Open PnL Panel",
            panel_window::Kind::Pnl,
        )?;
        let connections = custom_panel_menu(
            &mut panel_items,
            "Connections",
            "flowsurface-native-connections",
            "Open Connections Panel",
            panel_window::Kind::Connections,
        )?;
        let account = custom_panel_menu(
            &mut panel_items,
            "Account",
            "flowsurface-native-account",
            "Open Account Panel",
            panel_window::Kind::Account,
        )?;
        let analytics = custom_panel_menu(
            &mut panel_items,
            "Analytics",
            "flowsurface-native-analytics",
            "Open Analytics Panel",
            panel_window::Kind::Analytics,
        )?;
        let about = custom_panel_menu(
            &mut panel_items,
            "About",
            "flowsurface-native-about",
            "Open About Panel",
            panel_window::Kind::About,
        )?;

        menu.append_items(&[
            &app,
            &file,
            &edit,
            &view,
            &window,
            &help,
            &pnl,
            &connections,
            &account,
            &analytics,
            &about,
        ])
        .map_err(|err| format!("Failed to build native macOS menu: {err}"))?;

        menu.init_for_nsapp();

        Ok(Self {
            _menu: menu,
            panel_items,
        })
    }

    pub(crate) fn poll_panel_selection(&self) -> Option<panel_window::Kind> {
        let mut selected = None;

        for event in muda::MenuEvent::receiver().try_iter() {
            if let Some(kind) = self.panel_items.get(&event.id) {
                selected = Some(*kind);
            }
        }

        selected
    }
}

fn app_menu(panel_items: &mut HashMap<MenuId, panel_window::Kind>) -> Result<Submenu, String> {
    let open = panel_item(
        "flowsurface-native-app-panel",
        "Open App Panel",
        panel_window::Kind::App,
    );
    register(panel_items, &open);

    let native_about = PredefinedMenuItem::about(
        Some("About Flowsurface"),
        Some(AboutMetadata {
            name: Some("Flowsurface".to_string()),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            ..AboutMetadata::default()
        }),
    );

    let separator_1 = PredefinedMenuItem::separator();
    let separator_2 = PredefinedMenuItem::separator();
    let separator_3 = PredefinedMenuItem::separator();
    let hide = PredefinedMenuItem::hide(Some("Hide Flowsurface"));
    let hide_others = PredefinedMenuItem::hide_others(Some("Hide Others"));
    let show_all = PredefinedMenuItem::show_all(Some("Show All"));
    let quit = PredefinedMenuItem::quit(Some("Quit Flowsurface"));

    Submenu::with_items(
        "Flowsurface",
        true,
        &[
            &open,
            &separator_1,
            &native_about,
            &separator_2,
            &hide,
            &hide_others,
            &show_all,
            &separator_3,
            &quit,
        ],
    )
    .map_err(|err| format!("Failed to build app menu: {err}"))
}

fn file_menu(panel_items: &mut HashMap<MenuId, panel_window::Kind>) -> Result<Submenu, String> {
    let open = panel_item(
        "flowsurface-native-file-panel",
        "Open File Panel",
        panel_window::Kind::File,
    );
    register(panel_items, &open);

    let separator = PredefinedMenuItem::separator();
    let close = PredefinedMenuItem::close_window(Some("Close Window"));

    Submenu::with_items("File", true, &[&open, &separator, &close])
        .map_err(|err| format!("Failed to build file menu: {err}"))
}

fn edit_menu(panel_items: &mut HashMap<MenuId, panel_window::Kind>) -> Result<Submenu, String> {
    let open = panel_item(
        "flowsurface-native-edit-panel",
        "Open Edit Panel",
        panel_window::Kind::Edit,
    );
    register(panel_items, &open);

    let separator_1 = PredefinedMenuItem::separator();
    let separator_2 = PredefinedMenuItem::separator();
    let undo = PredefinedMenuItem::undo(Some("Undo"));
    let redo = PredefinedMenuItem::redo(Some("Redo"));
    let cut = PredefinedMenuItem::cut(Some("Cut"));
    let copy = PredefinedMenuItem::copy(Some("Copy"));
    let paste = PredefinedMenuItem::paste(Some("Paste"));
    let select_all = PredefinedMenuItem::select_all(Some("Select All"));

    Submenu::with_items(
        "Edit",
        true,
        &[
            &open,
            &separator_1,
            &undo,
            &redo,
            &separator_2,
            &cut,
            &copy,
            &paste,
            &select_all,
        ],
    )
    .map_err(|err| format!("Failed to build edit menu: {err}"))
}

fn view_menu(panel_items: &mut HashMap<MenuId, panel_window::Kind>) -> Result<Submenu, String> {
    let open = panel_item(
        "flowsurface-native-view-panel",
        "Open View Panel",
        panel_window::Kind::View,
    );
    register(panel_items, &open);

    let separator = PredefinedMenuItem::separator();
    let fullscreen = PredefinedMenuItem::fullscreen(Some("Enter Full Screen"));

    Submenu::with_items("View", true, &[&open, &separator, &fullscreen])
        .map_err(|err| format!("Failed to build view menu: {err}"))
}

fn window_menu(panel_items: &mut HashMap<MenuId, panel_window::Kind>) -> Result<Submenu, String> {
    let open = panel_item(
        "flowsurface-native-window-panel",
        "Open Window Panel",
        panel_window::Kind::Window,
    );
    register(panel_items, &open);

    let separator_1 = PredefinedMenuItem::separator();
    let separator_2 = PredefinedMenuItem::separator();
    let minimize = PredefinedMenuItem::minimize(Some("Minimize"));
    let zoom = PredefinedMenuItem::maximize(Some("Zoom"));
    let bring_all = PredefinedMenuItem::bring_all_to_front(Some("Bring All to Front"));

    let submenu = Submenu::with_items(
        "Window",
        true,
        &[
            &open,
            &separator_1,
            &minimize,
            &zoom,
            &separator_2,
            &bring_all,
        ],
    )
    .map_err(|err| format!("Failed to build window menu: {err}"))?;

    submenu.set_as_windows_menu_for_nsapp();

    Ok(submenu)
}

fn help_menu(panel_items: &mut HashMap<MenuId, panel_window::Kind>) -> Result<Submenu, String> {
    let open = panel_item(
        "flowsurface-native-help-panel",
        "Open Help Panel",
        panel_window::Kind::Help,
    );
    register(panel_items, &open);

    let submenu = Submenu::with_items("Help", true, &[&open])
        .map_err(|err| format!("Failed to build help menu: {err}"))?;

    submenu.set_as_help_menu_for_nsapp();

    Ok(submenu)
}

fn custom_panel_menu(
    panel_items: &mut HashMap<MenuId, panel_window::Kind>,
    title: &'static str,
    item_id: &'static str,
    item_label: &'static str,
    kind: panel_window::Kind,
) -> Result<Submenu, String> {
    let open = panel_item(item_id, item_label, kind);
    register(panel_items, &open);

    // macOS menu bar roots are menus, not command items. The command lives inside.
    Submenu::with_items(title, true, &[&open])
        .map_err(|err| format!("Failed to build {title} menu: {err}"))
}

fn panel_item(id: &'static str, label: &'static str, kind: panel_window::Kind) -> MenuItem {
    let accelerator = match kind {
        panel_window::Kind::Pnl => Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyP)),
        panel_window::Kind::Connections => {
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyK))
        }
        panel_window::Kind::Account => Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyU)),
        panel_window::Kind::Analytics => Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyY)),
        panel_window::Kind::About => Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyI)),
        _ => None,
    };

    MenuItem::with_id(id, label, true, accelerator)
}

fn register(panel_items: &mut HashMap<MenuId, panel_window::Kind>, item: &MenuItem) {
    let kind = match item.id().as_ref() {
        "flowsurface-native-app-panel" => panel_window::Kind::App,
        "flowsurface-native-file-panel" => panel_window::Kind::File,
        "flowsurface-native-edit-panel" => panel_window::Kind::Edit,
        "flowsurface-native-view-panel" => panel_window::Kind::View,
        "flowsurface-native-window-panel" => panel_window::Kind::Window,
        "flowsurface-native-help-panel" => panel_window::Kind::Help,
        "flowsurface-native-pnl" => panel_window::Kind::Pnl,
        "flowsurface-native-connections" => panel_window::Kind::Connections,
        "flowsurface-native-account" => panel_window::Kind::Account,
        "flowsurface-native-analytics" => panel_window::Kind::Analytics,
        "flowsurface-native-about" => panel_window::Kind::About,
        _ => return,
    };

    panel_items.insert(item.id().clone(), kind);
}
