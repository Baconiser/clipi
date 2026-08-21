use std::str::FromStr;

use eframe::egui::{self, Event, Key};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::GlobalHotKeyManager;

pub const DEFAULT: &str = "alt+KeyC";

pub fn parse(raw: &str) -> Result<HotKey, String> {
    HotKey::from_str(raw.trim()).map_err(|e| e.to_string())
}

pub fn register(manager: &GlobalHotKeyManager, raw: &str) -> Result<HotKey, String> {
    let hotkey = parse(raw)?;
    manager.register(hotkey).map_err(|e| e.to_string())?;
    Ok(hotkey)
}

pub fn unregister(manager: &GlobalHotKeyManager, hotkey: HotKey) {
    let _ = manager.unregister(hotkey);
}

pub fn display_label(raw: &str) -> String {
    match parse(raw) {
        Ok(hotkey) => format_hotkey(hotkey),
        Err(_) => raw.to_string(),
    }
}

pub enum Capture {
    None,
    Cancel,
    Combo(HotKey),
}

pub fn poll_capture(ctx: &egui::Context) -> Capture {
    let mut result = Capture::None;
    ctx.input_mut(|i| {
        for event in &i.events {
            let Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            if *key == Key::Escape {
                result = Capture::Cancel;
                break;
            }
            let Some(code) = egui_key_to_code(*key) else {
                continue;
            };
            let mods = egui_mods_to_hotkey(*modifiers);
            if mods.is_empty() && !is_function_key(*key) {
                continue;
            }
            result = Capture::Combo(HotKey::new(Some(mods), code));
            break;
        }
        if !matches!(result, Capture::None) {
            i.events
                .retain(|e| !matches!(e, Event::Key { .. } | Event::Text(_)));
        }
    });
    result
}

fn format_hotkey(hotkey: HotKey) -> String {
    let mut parts = Vec::new();
    if hotkey.mods.contains(Modifiers::CONTROL) {
        parts.push("Ctrl");
    }
    if hotkey.mods.contains(Modifiers::ALT) {
        #[cfg(target_os = "macos")]
        parts.push("Option");
        #[cfg(not(target_os = "macos"))]
        parts.push("Alt");
    }
    if hotkey.mods.contains(Modifiers::SHIFT) {
        parts.push("Shift");
    }
    if hotkey.mods.contains(Modifiers::SUPER) {
        #[cfg(target_os = "macos")]
        parts.push("Cmd");
        #[cfg(not(target_os = "macos"))]
        parts.push("Win");
    }
    let key = code_label(hotkey.key);
    parts.push(key.as_str());
    parts.join("+")
}

fn code_label(code: Code) -> String {
    let raw = code.to_string();
    if let Some(letter) = raw.strip_prefix("Key") {
        if letter.len() == 1 {
            return letter.to_string();
        }
    }
    if let Some(digit) = raw.strip_prefix("Digit") {
        return digit.to_string();
    }
    raw
}

fn is_function_key(key: Key) -> bool {
    matches!(
        key,
        Key::F1
            | Key::F2
            | Key::F3
            | Key::F4
            | Key::F5
            | Key::F6
            | Key::F7
            | Key::F8
            | Key::F9
            | Key::F10
            | Key::F11
            | Key::F12
            | Key::F13
            | Key::F14
            | Key::F15
            | Key::F16
            | Key::F17
            | Key::F18
            | Key::F19
            | Key::F20
            | Key::F21
            | Key::F22
            | Key::F23
            | Key::F24
    )
}

fn egui_mods_to_hotkey(m: egui::Modifiers) -> Modifiers {
    let mut mods = Modifiers::empty();
    if m.alt {
        mods |= Modifiers::ALT;
    }
    if m.shift {
        mods |= Modifiers::SHIFT;
    }
    if m.ctrl {
        mods |= Modifiers::CONTROL;
    }
    if m.mac_cmd {
        mods |= Modifiers::SUPER;
    }
    mods
}

fn egui_key_to_code(key: Key) -> Option<Code> {
    Some(match key {
        Key::A => Code::KeyA,
        Key::B => Code::KeyB,
        Key::C => Code::KeyC,
        Key::D => Code::KeyD,
        Key::E => Code::KeyE,
        Key::F => Code::KeyF,
        Key::G => Code::KeyG,
        Key::H => Code::KeyH,
        Key::I => Code::KeyI,
        Key::J => Code::KeyJ,
        Key::K => Code::KeyK,
        Key::L => Code::KeyL,
        Key::M => Code::KeyM,
        Key::N => Code::KeyN,
        Key::O => Code::KeyO,
        Key::P => Code::KeyP,
        Key::Q => Code::KeyQ,
        Key::R => Code::KeyR,
        Key::S => Code::KeyS,
        Key::T => Code::KeyT,
        Key::U => Code::KeyU,
        Key::V => Code::KeyV,
        Key::W => Code::KeyW,
        Key::X => Code::KeyX,
        Key::Y => Code::KeyY,
        Key::Z => Code::KeyZ,
        Key::Num0 => Code::Digit0,
        Key::Num1 => Code::Digit1,
        Key::Num2 => Code::Digit2,
        Key::Num3 => Code::Digit3,
        Key::Num4 => Code::Digit4,
        Key::Num5 => Code::Digit5,
        Key::Num6 => Code::Digit6,
        Key::Num7 => Code::Digit7,
        Key::Num8 => Code::Digit8,
        Key::Num9 => Code::Digit9,
        Key::F1 => Code::F1,
        Key::F2 => Code::F2,
        Key::F3 => Code::F3,
        Key::F4 => Code::F4,
        Key::F5 => Code::F5,
        Key::F6 => Code::F6,
        Key::F7 => Code::F7,
        Key::F8 => Code::F8,
        Key::F9 => Code::F9,
        Key::F10 => Code::F10,
        Key::F11 => Code::F11,
        Key::F12 => Code::F12,
        Key::F13 => Code::F13,
        Key::F14 => Code::F14,
        Key::F15 => Code::F15,
        Key::F16 => Code::F16,
        Key::F17 => Code::F17,
        Key::F18 => Code::F18,
        Key::F19 => Code::F19,
        Key::F20 => Code::F20,
        Key::F21 => Code::F21,
        Key::F22 => Code::F22,
        Key::F23 => Code::F23,
        Key::F24 => Code::F24,
        Key::Space => Code::Space,
        Key::Enter => Code::Enter,
        Key::Tab => Code::Tab,
        Key::Backspace => Code::Backspace,
        Key::Delete => Code::Delete,
        Key::Insert => Code::Insert,
        Key::Home => Code::Home,
        Key::End => Code::End,
        Key::PageUp => Code::PageUp,
        Key::PageDown => Code::PageDown,
        Key::ArrowUp => Code::ArrowUp,
        Key::ArrowDown => Code::ArrowDown,
        Key::ArrowLeft => Code::ArrowLeft,
        Key::ArrowRight => Code::ArrowRight,
        Key::Minus => Code::Minus,
        Key::Equals => Code::Equal,
        Key::Comma => Code::Comma,
        Key::Period => Code::Period,
        Key::Slash => Code::Slash,
        Key::Backslash => Code::Backslash,
        Key::Semicolon => Code::Semicolon,
        Key::Quote => Code::Quote,
        Key::Backtick => Code::Backquote,
        Key::OpenBracket => Code::BracketLeft,
        Key::CloseBracket => Code::BracketRight,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_parses() {
        let hk = parse(DEFAULT).unwrap();
        assert!(hk.mods.contains(Modifiers::ALT));
        assert_eq!(hk.key, Code::KeyC);
    }
}
