use std::borrow::Cow;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use arboard::{Clipboard, ImageData};
use eframe::egui;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};

use crate::config::Settings;
use crate::store::{ClipKind, Store};

const POLL_MS: u64 = 300;
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const THUMB_MAX: u32 = 40;

pub enum WatcherCmd {
    RestoreText(String),
    RestoreImage { width: u32, height: u32, rgba: Vec<u8> },
    Reopen,
    Shutdown,
}

pub fn start(
    settings: Arc<RwLock<Settings>>,
    ctx: egui::Context,
) -> Result<Sender<WatcherCmd>, String> {
    let (tx, rx) = mpsc::channel();
    let path = settings
        .read()
        .map_err(|_| "settings lock".to_string())?
        .db_path
        .clone();
    let store = Store::open(&path)?;
    thread::Builder::new()
        .name("clipi-clipboard".into())
        .spawn(move || run(store, settings, ctx, rx))
        .map_err(|e| e.to_string())?;
    Ok(tx)
}

fn run(
    mut store: Store,
    settings: Arc<RwLock<Settings>>,
    ctx: egui::Context,
    rx: Receiver<WatcherCmd>,
) {
    let mut clipboard = match Clipboard::new() {
        Ok(c) => c,
        Err(err) => {
            eprintln!("clipi: clipboard open failed: {err}");
            return;
        }
    };
    let mut last_hash = String::new();
    loop {
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                WatcherCmd::Shutdown => return,
                WatcherCmd::Reopen => {
                    if let Ok(s) = settings.read() {
                        match store.reopen(&s.db_path) {
                            Ok(()) => last_hash.clear(),
                            Err(err) => eprintln!("clipi: reopen db failed: {err}"),
                        }
                    }
                }
                WatcherCmd::RestoreText(text) => {
                    last_hash = hash_bytes(text.as_bytes());
                    if let Err(err) = clipboard.set_text(&text) {
                        eprintln!("clipi: set text failed: {err}");
                    }
                }
                WatcherCmd::RestoreImage {
                    width,
                    height,
                    rgba,
                } => {
                    last_hash = hash_bytes(&rgba);
                    let img = ImageData {
                        width: width as usize,
                        height: height as usize,
                        bytes: Cow::Owned(rgba),
                    };
                    if let Err(err) = clipboard.set_image(img) {
                        eprintln!("clipi: set image failed: {err}");
                    }
                }
            }
        }

        capture_once(&mut clipboard, &store, &settings, &mut last_hash, &ctx);
        thread::sleep(Duration::from_millis(POLL_MS));
    }
}

fn capture_once(
    clipboard: &mut Clipboard,
    store: &Store,
    settings: &Arc<RwLock<Settings>>,
    last_hash: &mut String,
    ctx: &egui::Context,
) {
    let Ok(guard) = settings.read() else {
        return;
    };

    if let Ok(text) = clipboard.get_text() {
        let text = text.trim_end_matches('\0').to_string();
        if !text.is_empty() {
            let hash = hash_bytes(text.as_bytes());
            if hash == *last_hash {
                return;
            }
            if let Err(err) = store.upsert(
                ClipKind::Text,
                Some(&text),
                None,
                None,
                &hash,
                &guard,
            ) {
                eprintln!("clipi: store text failed: {err}");
                return;
            }
            *last_hash = hash;
            ctx.request_repaint();
            return;
        }
    }

    let Ok(img) = clipboard.get_image() else {
        return;
    };
    let width = img.width as u32;
    let height = img.height as u32;
    let raw = img.bytes.into_owned();
    let uncompressed = (width as usize).saturating_mul(height as usize).saturating_mul(4);
    if uncompressed == 0 || uncompressed > MAX_IMAGE_BYTES {
        return;
    }
    if raw.len() < uncompressed {
        return;
    }
    let hash = hash_bytes(&raw[..uncompressed]);
    if hash == *last_hash {
        return;
    }
    let Some(rgba) = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width, height, raw[..uncompressed].to_vec())
    else {
        return;
    };
    let Ok(full_png) = encode_png(&rgba) else {
        return;
    };
    let thumb = make_thumb(&rgba);
    let label = format!("image {width}x{height}");
    if let Err(err) = store.upsert(
        ClipKind::Image,
        Some(&label),
        thumb.as_deref(),
        Some(&full_png),
        &hash,
        &guard,
    ) {
        eprintln!("clipi: store image failed: {err}");
        return;
    }
    *last_hash = hash;
    ctx.request_repaint();
}

fn make_thumb(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Option<Vec<u8>> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let long = w.max(h) as f32;
    let scale = THUMB_MAX as f32 / long;
    let nw = ((w as f32) * scale).round().max(1.0) as u32;
    let nh = ((h as f32) * scale).round().max(1.0) as u32;
    let resized = image::imageops::thumbnail(img, nw, nh);
    encode_png(&resized).ok()
}

fn encode_png(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<Vec<u8>, String> {
    let dyn_img = DynamicImage::ImageRgba8(img.clone());
    let mut buf = Vec::new();
    dyn_img
        .write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

pub fn decode_png_rgba(png: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let img = image::load_from_memory(png).map_err(|e| e.to_string())?.to_rgba8();
    let (w, h) = img.dimensions();
    Ok((w, h, img.into_raw()))
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
