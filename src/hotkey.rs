use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::GlobalHotKeyManager;

pub fn register_alt_c(manager: &GlobalHotKeyManager) -> Result<HotKey, String> {
    let hotkey = HotKey::new(Some(Modifiers::ALT), Code::KeyC);
    manager.register(hotkey).map_err(|e| e.to_string())?;
    Ok(hotkey)
}
