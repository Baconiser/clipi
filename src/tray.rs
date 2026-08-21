use tray_icon::menu::{Menu, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

pub struct TrayHandles {
    #[allow(dead_code)]
    pub tray: TrayIcon,
    #[allow(dead_code)]
    _items: (MenuItem, MenuItem, MenuItem),
    pub show_id: MenuId,
    pub settings_id: MenuId,
    pub quit_id: MenuId,
}

pub fn build() -> Result<TrayHandles, String> {
    let show = MenuItem::new("Show", true, None);
    let settings = MenuItem::new("Settings", true, None);
    let quit = MenuItem::new("Quit", true, None);
    let show_id = show.id().clone();
    let settings_id = settings.id().clone();
    let quit_id = quit.id().clone();

    let menu = Menu::new();
    menu.append(&show).map_err(|e| e.to_string())?;
    menu.append(&settings).map_err(|e| e.to_string())?;
    menu.append(&PredefinedMenuItem::separator())
        .map_err(|e| e.to_string())?;
    menu.append(&quit).map_err(|e| e.to_string())?;

    let icon = crate::icon::tray_icon()?;
    let builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("clipi")
        .with_icon(icon);
    #[cfg(target_os = "macos")]
    let builder = builder.with_icon_as_template(true);
    let tray = builder.build().map_err(|e| e.to_string())?;

    Ok(TrayHandles {
        tray,
        _items: (show, settings, quit),
        show_id,
        settings_id,
        quit_id,
    })
}

