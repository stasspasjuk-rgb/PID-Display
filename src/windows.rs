mod driver;
mod ffi;
mod link;
mod passenger_view;

use crate::{
    route::{parse, Journey, Route},
    AudioBank,
};
use driver::{Driver, DOOR_PHRASE};
use ffi::*;
use link::{Doors, Snapshot};
use std::{
    cell::RefCell,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    ptr::{null, null_mut},
    sync::mpsc::{Receiver, TryRecvError},
    time::{Duration, Instant},
};

const STYLE: u32 = 0x00CF0000;
const BLACK: u32 = 0x000000;
const WHITE: u32 = 0xFFFFFF;
const SOFT: u32 = 0xAFB2B7;

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

struct App {
    routes: Vec<Route>,
    selection: usize,
    journey: Journey,
    folder: PathBuf,
    audio: Option<AudioBank>,
    muted: bool,
    sound: Option<Child>,
    assembled: Option<Vec<u8>>,
    sound_error: bool,
    load_error: String,
    driver: Option<Driver>,
    receiver: Option<Receiver<Snapshot>>,
    ready: bool,
    doors: Doors,
    monitor_epoch: u32,
    transition: Option<(usize, Instant)>,
    window_rect: Option<Rect>,
}

impl App {
    fn new(driver: bool, linked: bool, audio: Option<AudioBank>) -> Self {
        let folder = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        let custom = folder.join("routes.tsv");
        let mut routes = parse(include_str!("../data/prague.tsv")).expect("embedded Prague routes");
        let mut load_error = String::new();
        if custom.exists() {
            match std::fs::read_to_string(custom)
                .map_err(|e| e.to_string())
                .and_then(|s| parse(&s))
            {
                Ok(extra) => routes.extend(extra),
                Err(e) => load_error = e,
            }
        }
        Self {
            routes,
            selection: 0,
            journey: Journey::new(),
            folder,
            audio,
            muted: false,
            sound: None,
            assembled: None,
            sound_error: false,
            load_error,
            driver: driver.then(Driver::new),
            receiver: linked.then(link::listen),
            ready: false,
            doors: Doors::Closed,
            monitor_epoch: 0,
            transition: None,
            window_rect: None,
        }
    }

    fn stop_audio(&mut self) {
        unsafe {
            PlaySoundW(null(), null_mut(), 0);
        }
        self.assembled = None;
        if let Some(mut child) = self.sound.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn announce(&mut self) {
        let route = &self.routes[self.selection];
        let stop = &route.stops[self.journey.shown()];
        let (phrase, event) = self.journey.announcement(route);
        let id = stop.id.clone();
        let composed = if demo_clip(self.audio, event, &phrase).is_none() {
            assemble(
                self.audio,
                event,
                &announcement_parts(&self.journey, route, event),
            )
        } else {
            None
        };
        self.play_audio(&phrase, event, &id, composed);
    }

    fn play(&mut self, phrase: &str, event: &str, id: &str) {
        self.play_audio(phrase, event, id, None);
    }

    fn play_audio(&mut self, phrase: &str, event: &str, id: &str, composed: Option<Vec<u8>>) {
        self.stop_audio();
        if self.muted {
            return;
        }
        let safe_id = id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c));
        let file = ["wav", "mp3"]
            .into_iter()
            .filter(|_| safe_id)
            .map(|ext| {
                self.folder
                    .join("audio")
                    .join(format!("{id}-{event}.{ext}"))
            })
            .find(|path| path.is_file());
        if file.is_none() {
            let clip = demo_clip(self.audio, event, phrase);
            if clip.is_none() {
                self.assembled = composed;
            }
            if let Some(clip) = clip.or(self.assembled.as_deref()) {
                if unsafe { PlaySoundW(clip.as_ptr().cast(), null_mut(), 0x0007) } != 0 {
                    self.sound_error = false;
                    return;
                }
            }
        }
        let mut command = Command::new("powershell.exe");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-STA",
                "-Command",
                include_str!("speak.ps1"),
            ])
            .env("PID_PHRASE", phrase)
            .env("PID_GONG", if event == "next" { "next" } else { "arrival" })
            .env_remove("PID_AUDIO")
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(path) = file {
            command.env("PID_AUDIO", path);
        }
        self.sound = command.spawn().ok();
        self.sound_error = self.sound.is_none();
    }

    fn tick_audio(&mut self) {
        if let Some(status) = self
            .sound
            .as_mut()
            .and_then(|child| child.try_wait().ok().flatten())
        {
            self.sound_error = !status.success();
            self.sound = None;
        }
    }

    fn tick_passenger(&mut self, hwnd: Handle) {
        let mut latest = None;
        let mut disconnected = false;
        if let Some(rx) = &self.receiver {
            loop {
                match rx.try_recv() {
                    Ok(snapshot) => latest = Some(snapshot),
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
        }
        if disconnected {
            self.ready = false;
            self.receiver = None;
            return;
        }
        if let Some(snapshot) = latest {
            let display_toggles = snapshot.monitor_epoch.wrapping_sub(self.monitor_epoch);
            self.monitor_epoch = snapshot.monitor_epoch;
            if display_toggles % 2 == 1 {
                unsafe { fullscreen(hwnd, self) };
            }
            if let Some(route) = self.routes.get(snapshot.selection) {
                let valid = snapshot.stop < route.stops.len()
                    && (!snapshot.moving || snapshot.stop + 1 < route.stops.len());
                let same_route = self.ready && self.selection == snapshot.selection;
                self.ready = snapshot.ready && valid;
                if self.ready {
                    if !same_route {
                        self.journey = Journey::new();
                        self.transition = None;
                    } else {
                        let shown = snapshot.stop + usize::from(snapshot.moving);
                        if self.journey.shown() != shown {
                            self.transition = Some((self.journey.shown(), Instant::now()));
                        }
                    }
                    self.selection = snapshot.selection;
                    self.journey.mirror(
                        snapshot.stop,
                        snapshot.moving,
                        snapshot.automatic,
                        Duration::from_millis(snapshot.elapsed_ms.min(86_400_000)),
                    );
                    self.doors = match snapshot.doors {
                        1 => Doors::Open,
                        2 => Doors::Closing,
                        _ => Doors::Closed,
                    };
                }
            } else {
                self.ready = false;
            }
        }
    }
}

fn announcement_parts<'a>(journey: &Journey, route: &'a Route, event: &str) -> Vec<&'a str> {
    let stop = &route.stops[journey.shown()];
    let mut parts = Vec::with_capacity(5);
    if event == "next" {
        parts.push("Příští zastávka.");
    }
    parts.push(stop.name.as_str());
    if event == "next" && route.mode != "tram" && journey.shown() + 1 < route.stops.len() {
        parts.push("Zastávka na znamení.");
    } else if event == "final" {
        parts.push("Konečná zastávka, prosíme vystupte.");
    } else if event == "arrive" {
        parts.extend(stop.transfers.chars().filter_map(|mark| match mark {
            'A' => Some("Přestup na metro A."),
            'B' => Some("Přestup na metro B."),
            'C' => Some("Přestup na metro C."),
            'S' => Some("Přestup na linky S a další vlakové spoje."),
            _ => None,
        }));
    }
    parts
}

fn assemble(bank: Option<AudioBank>, event: &str, parts: &[&str]) -> Option<Vec<u8>> {
    let mut wav = demo_clip(bank, "door", DOOR_PHRASE)?.get(..44)?.to_vec();
    let notes = if event == "next" {
        [660.0, 880.0]
    } else {
        [880.0, 660.0]
    };
    for note in notes {
        for frame in 0..2400 {
            let t = frame as f64 / 22_050.0;
            let envelope = (t / 0.008).min(1.0) * (-31.0 * t).exp();
            let sample = ((t * note * std::f64::consts::TAU).sin() * envelope * 5600.0) as i16;
            wav.extend_from_slice(&sample.to_le_bytes());
        }
    }
    wav.resize(wav.len() + 3400, 0);
    let amplitudes: [i16; 256] = std::array::from_fn(|i| {
        let x = (i as f64 - 128.0) / 127.0;
        (x.signum() * (256.0_f64.powf(x.abs()) - 1.0) * (32767.0 / 255.0)) as i16
    });
    for &part in parts {
        for &sample in demo_clip(bank, "part", part)? {
            wav.extend_from_slice(&amplitudes[sample as usize].to_le_bytes());
        }
    }
    let riff_size = (wav.len() - 8) as u32;
    let data_size = (wav.len() - 44) as u32;
    wav[4..8].copy_from_slice(&riff_size.to_le_bytes());
    wav[40..44].copy_from_slice(&data_size.to_le_bytes());
    Some(wav)
}

fn demo_clip(bank: Option<AudioBank>, event: &str, phrase: &str) -> Option<&'static [u8]> {
    let (index, audio) = bank?;
    for row in index.lines() {
        let mut fields = row.split('\t');
        if fields.next() == Some(event) && fields.next() == Some(phrase) {
            let start: usize = fields.next()?.parse().ok()?;
            let length: usize = fields.next()?.parse().ok()?;
            return audio.get(start..start.checked_add(length)?);
        }
    }
    None
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn rgb(color: u32) -> u32 {
    ((color & 0xFF) << 16) | (color & 0xFF00) | (color >> 16)
}

struct Canvas {
    dc: Handle,
    scale: f64,
    left: i32,
    top: i32,
}

impl Canvas {
    fn point(&self, x: i32, y: i32) -> (i32, i32) {
        (
            self.left + (f64::from(x) * self.scale).round() as i32,
            self.top + (f64::from(y) * self.scale).round() as i32,
        )
    }

    fn area(&self, x: i32, y: i32, width: i32, height: i32) -> Rect {
        let (left, top) = self.point(x, y);
        let (right, bottom) = self.point(x + width, y + height);
        Rect {
            left,
            top,
            right,
            bottom,
        }
    }

    fn block(&self, color: u32, x: i32, y: i32, w: i32, h: i32) {
        unsafe { fill(self.dc, &self.area(x, y, w, h), color) };
    }

    fn text(
        &self,
        text: &str,
        bounds: (i32, i32, i32, i32),
        size: i32,
        bold: bool,
        color: u32,
        align: u32,
    ) -> i32 {
        let (x, y, w, h) = bounds;
        let mut rect = self.area(x, y, w, h);
        let text = wide(text);
        unsafe {
            let font = CreateFontW(
                -((size as f64 * self.scale).round() as i32).max(1),
                0,
                0,
                0,
                if bold { 700 } else { 400 },
                0,
                0,
                0,
                1,
                0,
                0,
                5,
                0,
                wide("Segoe UI").as_ptr(),
            );
            let previous = SelectObject(self.dc, font);
            SetTextColor(self.dc, rgb(color));
            SetBkMode(self.dc, 1);
            let mut extent = Point::default();
            GetTextExtentPoint32W(self.dc, text.as_ptr(), text.len() as i32 - 1, &mut extent);
            DrawTextW(
                self.dc,
                text.as_ptr(),
                text.len() as i32 - 1,
                &mut rect,
                0x8824 | align,
            );
            SelectObject(self.dc, previous);
            DeleteObject(font);
            ((extent.x as f64 / self.scale).round() as i32).min(w)
        }
    }

    fn dot(&self, x: i32, y: i32, radius: i32, color: u32) {
        let rect = self.area(x - radius, y - radius, radius * 2, radius * 2);
        unsafe {
            let brush = CreateSolidBrush(rgb(color));
            let old_brush = SelectObject(self.dc, brush);
            let old_pen = SelectObject(self.dc, GetStockObject(8));
            Ellipse(self.dc, rect.left, rect.top, rect.right, rect.bottom);
            SelectObject(self.dc, old_pen);
            SelectObject(self.dc, old_brush);
            DeleteObject(brush);
        }
    }

    fn clip(&self, bounds: (i32, i32, i32, i32), draw: impl FnOnce()) {
        let rect = self.area(bounds.0, bounds.1, bounds.2, bounds.3);
        unsafe {
            let saved = SaveDC(self.dc);
            if saved != 0 {
                IntersectClipRect(self.dc, rect.left, rect.top, rect.right, rect.bottom);
            }
            draw();
            if saved != 0 {
                RestoreDC(self.dc, saved);
            }
        }
    }

    fn polygon(&self, color: u32, vertices: &[(i32, i32)]) {
        let points: Vec<_> = vertices
            .iter()
            .map(|&(x, y)| {
                let (x, y) = self.point(x, y);
                Point { x, y }
            })
            .collect();
        unsafe {
            let brush = CreateSolidBrush(rgb(color));
            let old_brush = SelectObject(self.dc, brush);
            let old_pen = SelectObject(self.dc, GetStockObject(8));
            Polygon(self.dc, points.as_ptr(), points.len() as i32);
            SelectObject(self.dc, old_pen);
            SelectObject(self.dc, old_brush);
            DeleteObject(brush);
        }
    }

    fn angled(&self, value: &str, x: i32, y: i32, size: i32, color: u32) {
        let (x, y) = self.point(x, y);
        let value = wide(value);
        unsafe {
            let font = CreateFontW(
                -((size as f64 * self.scale).round() as i32).max(1),
                0,
                550,
                550,
                600,
                0,
                0,
                0,
                1,
                0,
                0,
                4,
                0,
                wide("Segoe UI").as_ptr(),
            );
            let old = SelectObject(self.dc, font);
            SetTextColor(self.dc, rgb(color));
            SetBkMode(self.dc, 1);
            TextOutW(self.dc, x, y, value.as_ptr(), value.len() as i32 - 1);
            SelectObject(self.dc, old);
            DeleteObject(font);
        }
    }

    fn transfers(&self, text: &str, y: i32, size: i32, left: i32) {
        let marks: Vec<_> = text
            .chars()
            .filter_map(|mark| {
                let color = match mark {
                    'A' => 0x158249,
                    'B' => 0xF4CA21,
                    'C' => 0xD52E3C,
                    'S' => 0x2677B6,
                    _ => return None,
                };
                Some((mark, color))
            })
            .collect();
        let mut x = left;
        for (mark, color) in marks {
            self.block(color, x, y, size, size);
            self.text(
                &mark.to_string(),
                (x, y, size, size),
                size - 9,
                true,
                if mark == 'B' { BLACK } else { WHITE },
                1,
            );
            x += size + 8;
        }
    }
}

unsafe fn fill(dc: Handle, rect: &Rect, color: u32) {
    let brush = CreateSolidBrush(rgb(color));
    FillRect(dc, rect, brush);
    DeleteObject(brush);
}

fn idle(c: &Canvas, linked: bool) {
    c.block(0x131920, 0, 0, 1280, 720);
    c.block(0x4ACBCC, 582, 258, 116, 4);
    c.text(
        "Informační displej",
        (65, 284, 1150, 86),
        62,
        true,
        WHITE,
        1,
    );
    let text = if linked {
        "Čeká na výběr spoje na terminálu řidiče"
    } else {
        "Spusťte PID Display pro připojení displeje"
    };
    c.text(text, (65, 376, 1150, 48), 28, false, SOFT, 1);
}

fn frame(width: i32, height: i32) -> (f64, i32, i32) {
    let scale = (width as f64 / 1280.0).min(height as f64 / 720.0);
    let left = ((width as f64 - 1280.0 * scale) / 2.0).round() as i32;
    let top = ((height as f64 - 720.0 * scale) / 2.0).round() as i32;
    (scale, left, top)
}

unsafe fn paint(hwnd: Handle, app: &App) {
    let mut ps = Paint::default();
    let target = BeginPaint(hwnd, &mut ps);
    let mut rect = Rect::default();
    GetClientRect(hwnd, &mut rect);
    let (width, height) = (rect.right, rect.bottom);
    if width > 0 && height > 0 {
        let dc = CreateCompatibleDC(target);
        let bitmap = CreateCompatibleBitmap(target, width, height);
        let buffered = !dc.is_null() && !bitmap.is_null();
        let draw_dc = if buffered { dc } else { target };
        let previous = if buffered {
            SelectObject(dc, bitmap)
        } else {
            null_mut()
        };
        fill(draw_dc, &rect, BLACK);
        let (scale, left, top) = frame(width, height);
        let canvas = Canvas {
            dc: draw_dc,
            scale,
            left,
            top,
        };
        if let Some(driver) = &app.driver {
            driver::draw(&canvas, app, driver);
        } else if app.ready {
            passenger_view::draw(&canvas, app);
        } else {
            idle(&canvas, app.receiver.is_some());
        }
        if buffered {
            BitBlt(target, 0, 0, width, height, dc, 0, 0, 0x00CC0020);
            SelectObject(dc, previous);
        }
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        if !dc.is_null() {
            DeleteDC(dc);
        }
    }
    EndPaint(hwnd, &ps);
}

unsafe fn fullscreen(hwnd: Handle, app: &mut App) {
    let (rect, style) = if let Some(rect) = app.window_rect.take() {
        (rect, STYLE)
    } else {
        let mut old = Rect::default();
        let mut info = MonitorInfo {
            size: std::mem::size_of::<MonitorInfo>() as u32,
            ..Default::default()
        };
        if GetWindowRect(hwnd, &mut old) == 0
            || GetMonitorInfoW(MonitorFromWindow(hwnd, 2), &mut info) == 0
        {
            return;
        }
        app.window_rect = Some(old);
        (info.monitor, 0x80000000)
    };
    SetWindowLongW(hwnd, -16, (style | 0x10000000) as i32);
    SetWindowPos(
        hwnd,
        null_mut(),
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
        0x24,
    );
}

unsafe extern "system" fn window_proc(hwnd: Handle, message: u32, w: usize, l: isize) -> isize {
    match message {
        0x0002 => {
            APP.with(|cell| {
                if let Some(app) = cell.borrow_mut().as_mut() {
                    app.stop_audio();
                }
            });
            PostQuitMessage(0);
            0
        }
        0x0014 => 1,
        0x000F => {
            let painted = APP.with(|cell| {
                if let Ok(app) = cell.try_borrow() {
                    if let Some(app) = app.as_ref() {
                        paint(hwnd, app);
                        return true;
                    }
                }
                false
            });
            if painted {
                0
            } else {
                DefWindowProcW(hwnd, message, w, l)
            }
        }
        0x0113 => {
            APP.with(|cell| {
                if let Ok(mut cell) = cell.try_borrow_mut() {
                    if let Some(app) = cell.as_mut() {
                        if app.driver.is_some() {
                            app.tick_driver();
                        } else {
                            app.tick_passenger(hwnd);
                        }
                    }
                }
            });
            InvalidateRect(hwnd, null(), 0);
            0
        }
        0x0200 | 0x0202 => {
            let mut rect = Rect::default();
            GetClientRect(hwnd, &mut rect);
            let (scale, left, top) = frame(rect.right, rect.bottom);
            if scale > 0.0 {
                let x = ((l as i16 as i32 - left) as f64 / scale).floor() as i32;
                let y = (((l >> 16) as i16 as i32 - top) as f64 / scale).floor() as i32;
                APP.with(|cell| {
                    let Ok(mut cell) = cell.try_borrow_mut() else {
                        return;
                    };
                    if let Some(app) = cell.as_mut() {
                        if let Some(driver) = app.driver.as_mut() {
                            driver.pointer = (x, y);
                        }
                        if message == 0x0202 && app.driver.is_some() {
                            app.click_driver(x, y);
                        }
                    }
                });
            }
            0
        }
        0x0102 => {
            APP.with(|cell| {
                if let Ok(mut cell) = cell.try_borrow_mut() {
                    if let Some(app) = cell.as_mut() {
                        app.type_driver(w as u16);
                    }
                }
            });
            InvalidateRect(hwnd, null(), 0);
            0
        }
        0x0100 => {
            APP.with(|cell| {
                if let Ok(mut cell) = cell.try_borrow_mut() {
                    if let Some(app) = cell.as_mut() {
                        if w == 0x7A || (w == 0x1B && app.window_rect.is_some()) {
                            fullscreen(hwnd, app);
                        }
                    }
                }
            });
            0
        }
        _ => DefWindowProcW(hwnd, message, w, l),
    }
}

unsafe fn placement(driver: bool, linked: bool) -> (i32, i32, i32, i32) {
    let mut work = Rect::default();
    if (driver || linked) && SystemParametersInfoW(48, 0, (&mut work as *mut Rect).cast(), 0) != 0 {
        let available = work.right - work.left;
        if available >= 1560 {
            let width = (available - 60) / 2;
            let height = (width * 9 / 16 + 42).min(work.bottom - work.top - 100);
            let x = if driver {
                work.left + 20
            } else {
                work.right - width - 20
            };
            return (x, work.top + 60, width, height);
        }
    }
    (
        i32::MIN,
        i32::MIN,
        if driver { 1088 } else { 1024 },
        if driver { 644 } else { 610 },
    )
}

pub fn run(driver: bool, audio: Option<AudioBank>) {
    let mut args = std::env::args_os().skip(1);
    if let Some(arg) = args.next() {
        let routes = parse(include_str!("../data/demo.tsv")).expect("embedded demo routes");
        if arg == "--export-phrases" || arg == "--check" {
            let mut phrases = std::collections::BTreeSet::new();
            if arg == "--check" && driver {
                let (index, bank) = audio.expect("embedded audio");
                let segments: std::collections::HashSet<_> = index
                    .lines()
                    .filter_map(|row| {
                        let mut parts = row.split('\t');
                        if parts.next()? != "part" {
                            return None;
                        }
                        let name = parts.next()?;
                        let offset: usize = parts.next()?.parse().ok()?;
                        let length: usize = parts.next()?.parse().ok()?;
                        (length > 1000 && offset.checked_add(length)? <= bank.len()).then_some(name)
                    })
                    .collect();
                assert!(demo_clip(audio, "door", DOOR_PHRASE)
                    .is_some_and(|wav| wav.starts_with(b"RIFF")));
                let city = parse(include_str!("../data/prague.tsv")).expect("embedded city routes");
                for line in ["5", "20", "120", "130", "170"] {
                    assert!(city.iter().filter(|route| route.line == line).count() >= 2);
                }
                assert!(city
                    .iter()
                    .flat_map(|route| &route.stops)
                    .all(|stop| segments.contains(stop.name.as_str())));
                let route = city.iter().find(|route| route.line == "170").unwrap();
                let mut journey = Journey::new();
                journey.advance(route);
                let parts = announcement_parts(&journey, route, "next");
                assert!(assemble(audio, "next", &parts).is_some_and(|wav| wav.starts_with(b"RIFF")));
            }
            for route in &routes {
                let mut journey = Journey::new();
                loop {
                    let (text, event) = journey.announcement(route);
                    if arg == "--check" && driver {
                        assert!(demo_clip(audio, event, &text)
                            .is_some_and(|audio| audio.starts_with(b"RIFF")));
                    }
                    phrases.insert(format!("{event}\t{text}"));
                    if !journey.advance(route) {
                        break;
                    }
                }
            }
            if arg == "--export-phrases" {
                std::fs::write(
                    args.next().expect("output filename"),
                    phrases.into_iter().collect::<Vec<_>>().join("\n"),
                )
                .expect("write announcement phrases");
            }
            return;
        }
    }
    let linked = !driver
        && std::env::args_os()
            .nth(1)
            .is_some_and(|arg| arg == "--linked");
    APP.with(|cell| *cell.borrow_mut() = Some(App::new(driver, linked, audio)));
    unsafe {
        SetProcessDPIAware();
        let instance = GetModuleHandleW(null());
        let class_name = wide("PidDisplayWindow");
        let class = WindowClass {
            style: 3,
            proc: Some(window_proc),
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: null_mut(),
            cursor: LoadCursorW(null_mut(), 32512usize as *const u16),
            background: null_mut(),
            menu: null(),
            name: class_name.as_ptr(),
        };
        if RegisterClassW(&class) == 0 {
            return;
        }
        let title = wide(if driver {
            "PID Driver · Terminál řidiče"
        } else {
            "PID Passenger · Informační displej"
        });
        let (x, y, width, height) = placement(driver, linked);
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            STYLE,
            x,
            y,
            width,
            height,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        if hwnd.is_null() {
            return;
        }
        SetTimer(hwnd, 1, if driver { 40 } else { 20 }, null_mut());
        ShowWindow(hwnd, if linked { 4 } else { 5 });
        if driver {
            APP.with(|cell| {
                if let Some(app) = cell.borrow_mut().as_mut() {
                    app.start_display();
                }
            });
        }
        let mut msg = Msg::default();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
