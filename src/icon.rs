use eframe::egui::IconData;
use image::ImageFormat;
use tray_icon::Icon;

const APP_PNG: &[u8] = include_bytes!("../assets/icon.png");

#[cfg(target_os = "macos")]
const TRAY_PNG: &[u8] = include_bytes!("../assets/tray-template.png");
#[cfg(not(target_os = "macos"))]
const TRAY_PNG: &[u8] = include_bytes!("../assets/tray.png");

pub fn window_icon() -> IconData {
    match decode_png(APP_PNG) {
        Ok((rgba, width, height)) => IconData {
            rgba,
            width,
            height,
        },
        Err(err) => {
            eprintln!("clipi: window icon: {err}");
            IconData::default()
        }
    }
}

pub fn tray_icon() -> Result<Icon, String> {
    let (rgba, width, height) = decode_png(TRAY_PNG)?;
    Icon::from_rgba(rgba, width, height).map_err(|e| e.to_string())
}

fn decode_png(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let img = image::load_from_memory_with_format(bytes, ImageFormat::Png)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let (width, height) = img.dimensions();
    Ok((img.into_raw(), width, height))
}
