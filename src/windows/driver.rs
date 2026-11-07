use super::{
    demo_clip, link::Display, App, Canvas, Doors, GetLocalTime, Snapshot, SystemTime, WHITE,
};

use std::time::{Duration, Instant};

pub const DOOR_PHRASE: &str = "Ukončete, prosím, nástup a výstup. Dveře se zavírají.";
const BG: u32 = 0x0C141D;
const PANEL: u32 = 0x14222F;
const BORDER: u32 = 0x2C4457;
const MUTED: u32 = 0xACBFCC;
const CYAN: u32 = 0x65DCD5;
const AMBER: u32 = 0xFFD27B;
const GREEN: u32 = 0x84DBA1;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Transport,
    Routes(bool),
    Drive,
    Stops,
}

#[derive(Clone, Copy)]
enum Action {
    Transport,
    Routes(bool),
    Drive,
    Stops,
    PageBack,
    PageNext,
    Key(char),
    Erase,
    Clear,
    Find,
    Select(usize),
    Seek(usize),
    Previous,
    Advance,
    Open,
    Close,
    Auto,
    Announce,
    Mute,
    Display,
    Fullscreen,
}

pub struct Driver {
    screen: Screen,
    entered: Instant,
    page: usize,
    query: String,
    exact: bool,
    filtered_at: Instant,
    pub pointer: (i32, i32),
    display: Option<Display>,
    display_error: bool,
    monitor_epoch: u32,
    closing_until: Option<Instant>,
    last_sync: Instant,
    last_snapshot: Option<Snapshot>,
}

impl Driver {
    pub fn new() -> Self {
        Self {
            screen: Screen::Transport,
            entered: Instant::now(),
            page: 0,
            query: String::new(),
            exact: false,
            filtered_at: Instant::now(),
            pointer: (-1, -1),
            display: None,
            display_error: false,
            monitor_epoch: 0,
            closing_until: None,
            last_sync: Instant::now(),
            last_snapshot: None,
        }
    }

    fn show(&mut self, screen: Screen) {
        self.screen = screen;
        self.entered = Instant::now();
        self.page = 0;
        if matches!(screen, Screen::Routes(_)) {
            self.query.clear();
            self.exact = false;
            self.filtered_at = self.entered;
        }
    }

    fn filter(&mut self, action: Action) {
        if !matches!(self.screen, Screen::Routes(_)) {
            return;
        }
        match action {
            Action::Key(key) if self.query.len() < 8 => {
                self.query.push(key);
                self.exact = false;
            }
            Action::Erase => {
                self.query.pop();
                self.exact = false;
            }
            Action::Clear => {
                self.query.clear();
                self.exact = false;
            }
            Action::Find => self.exact = !self.query.trim().is_empty(),
            _ => return,
        }
        self.page = 0;
        self.filtered_at = Instant::now();
    }
}

impl App {
    pub fn start_display(&mut self) {
        let Some(driver) = &mut self.driver else {
            return;
        };
        if driver.display.as_mut().is_some_and(Display::running) {
            return;
        }
        match Display::start() {
            Ok(display) => {
                driver.display = Some(display);
                driver.display_error = false;
                driver.last_snapshot = None;
                driver.monitor_epoch = 0;
            }
            Err(_) => driver.display_error = true,
        }
    }

    fn close_doors(&mut self) {
        if !self.ready || self.journey.moving || self.doors != Doors::Open {
            return;
        }
        self.doors = Doors::Closing;
        let seconds = demo_clip(self.audio, "door", DOOR_PHRASE)
            .and_then(|wav| {
                let rate = u32::from_le_bytes(wav.get(28..32)?.try_into().ok()?);
                (rate > 0).then(|| wav.len().saturating_sub(44) as f64 / f64::from(rate) + 0.20)
            })
            .unwrap_or(3.4);
        self.driver.as_mut().unwrap().closing_until =
            Some(Instant::now() + Duration::from_secs_f64(seconds));
        self.play(DOOR_PHRASE, "door", "vehicle");
    }

    fn drive_step(&mut self) {
        if !self.ready || self.doors != Doors::Closed {
            return;
        }
        let automatic = self.journey.automatic;
        if self.journey.advance(&self.routes[self.selection]) {
            if !self.journey.moving && automatic {
                self.doors = Doors::Open;
            }
            self.announce();
        }
    }

    fn seek_stop(&mut self, index: usize) {
        self.journey.seek(index);
        self.doors = Doors::Closed;
        let driver = self.driver.as_mut().unwrap();
        driver.closing_until = None;
        driver.show(Screen::Drive);
        self.announce();
    }

    pub fn type_driver(&mut self, character: u16) {
        if let Some(driver) = &mut self.driver {
            let action = match character {
                8 => Action::Erase,
                13 => Action::Find,
                27 => Action::Clear,
                32 | 48..=57 | 65..=90 | 97..=122 => {
                    Action::Key((character as u8 as char).to_ascii_uppercase())
                }
                _ => return,
            };
            driver.filter(action);
        }
    }

    pub fn click_driver(&mut self, x: i32, y: i32) {
        let Some(driver) = &self.driver else {
            return;
        };
        let mut ui = Ui {
            canvas: None,
            point: (x, y),
            action: None,
        };
        layout(&mut ui, self, driver);
        let Some(action) = ui.action else {
            return;
        };
        let driver = self.driver.as_mut().unwrap();
        match action {
            Action::Transport => driver.show(Screen::Transport),
            Action::Routes(tram) => driver.show(Screen::Routes(tram)),
            Action::Drive => driver.show(Screen::Drive),
            Action::Stops => {
                driver.show(Screen::Stops);
                driver.page = self.journey.index / 6;
            }
            Action::PageBack => driver.page = driver.page.saturating_sub(1),
            Action::PageNext => driver.page += 1,
            Action::Key(_) | Action::Erase | Action::Clear | Action::Find => driver.filter(action),
            Action::Select(index) => {
                self.selection = index;
                self.ready = true;
                self.seek_stop(0);
            }
            Action::Seek(index) => self.seek_stop(index),
            Action::Previous => {
                self.journey.previous();
                if self.journey.automatic {
                    self.journey.toggle();
                }
                self.doors = Doors::Closed;
                driver.closing_until = None;
                self.announce();
            }
            Action::Advance => self.drive_step(),
            Action::Open => self.doors = Doors::Open,
            Action::Close => self.close_doors(),
            Action::Auto => self.journey.toggle(),
            Action::Announce => self.announce(),
            Action::Mute => {
                self.muted = !self.muted;
                if self.muted {
                    self.stop_audio();
                }
            }
            Action::Display => self.start_display(),
            Action::Fullscreen => {
                driver.monitor_epoch = driver.monitor_epoch.wrapping_add(1);
            }
        }
    }

    pub fn tick_driver(&mut self) {
        self.tick_audio();
        let driver = self.driver.as_mut().unwrap();
        if driver
            .closing_until
            .is_some_and(|time| Instant::now() >= time)
        {
            driver.closing_until = None;
            self.doors = Doors::Closed;
        }
        if self.ready
            && self.journey.automatic
            && self.journey.elapsed() >= self.journey.delay(&self.routes[self.selection])
        {
            match self.doors {
                Doors::Open => self.close_doors(),
                Doors::Closed => self.drive_step(),
                Doors::Closing => {}
            }
        }
        let driver = self.driver.as_mut().unwrap();
        if driver
            .display
            .as_mut()
            .is_some_and(|display| !display.running())
        {
            driver.display = None;
        }
        let snapshot = Snapshot {
            selection: self.selection,
            ready: self.ready,
            stop: self.journey.index,
            moving: self.journey.moving,
            automatic: self.journey.automatic,
            elapsed_ms: self.journey.elapsed().as_millis() as u64,
            doors: match self.doors {
                Doors::Closed => 0,
                Doors::Open => 1,
                Doors::Closing => 2,
            },
            monitor_epoch: driver.monitor_epoch,
        };
        let changed = driver.last_snapshot.is_none_or(|old| {
            Snapshot {
                elapsed_ms: old.elapsed_ms,
                ..snapshot
            } != old
        });
        if changed || driver.last_sync.elapsed() >= Duration::from_millis(240) {
            if let Some(display) = &driver.display {
                display.send(snapshot);
            }
            driver.last_sync = Instant::now();
            driver.last_snapshot = Some(snapshot);
        }
    }
}

type Bounds = (i32, i32, i32, i32);

struct Ui<'a> {
    canvas: Option<&'a Canvas>,
    point: (i32, i32),
    action: Option<Action>,
}

impl Ui<'_> {
    fn rect(&self, (x, y, w, h): Bounds, color: u32) {
        if let Some(c) = self.canvas {
            c.block(color, x, y, w, h);
        }
    }

    fn text(&self, b: Bounds, s: &str, size: i32, bold: bool, color: u32, align: u32) {
        if let Some(c) = self.canvas {
            c.text(s, b, size, bold, color, align);
        }
    }

    fn button(&mut self, b: Bounds, label: &str, action: Option<Action>, accent: u32) {
        let (x, y, w, h) = b;
        let hover = self.target(b, action);
        let color = if hover { 0x28445A } else { PANEL };
        self.rect(b, BORDER);
        self.rect((x + 1, y + 1, w - 2, h - 2), color);
        self.text(
            (x + 10, y, w - 20, h),
            label,
            24,
            true,
            if action.is_some() { accent } else { 0x637888 },
            1,
        );
    }

    fn target(&mut self, (x, y, w, h): Bounds, action: Option<Action>) -> bool {
        let (px, py) = self.point;
        if action.is_some() && px >= x && px < x + w && py >= y && py < y + h {
            self.action = action;
            true
        } else {
            false
        }
    }
}

pub fn draw(canvas: &Canvas, app: &App, driver: &Driver) {
    let mut ui = Ui {
        canvas: Some(canvas),
        point: driver.pointer,
        action: None,
    };
    layout(&mut ui, app, driver);
}

fn layout(ui: &mut Ui, app: &App, driver: &Driver) {
    let mut time = SystemTime::default();
    unsafe { GetLocalTime(&mut time) };
    ui.rect((0, 0, 1280, 720), BG);
    ui.rect((0, 0, 1280, 84), 0x101D29);
    ui.rect((0, 83, 1280, 1), BORDER);
    ui.rect((36, 26, 7, 34), CYAN);
    ui.text((56, 12, 386, 45), "TERMINÁL ŘIDIČE", 29, true, WHITE, 0);
    ui.text(
        (56, 52, 384, 22),
        "PALUBNÍ INFORMAČNÍ SYSTÉM",
        14,
        true,
        MUTED,
        0,
    );
    let connected = driver.display.is_some();
    let display_label = if connected {
        "●  Displej připojen"
    } else if driver.display_error {
        "Otevřít displej znovu"
    } else {
        "Otevřít displej"
    };
    ui.button(
        (644, 19, 241, 47),
        display_label,
        (!connected).then_some(Action::Display),
        GREEN,
    );
    ui.button(
        (898, 19, 166, 47),
        "LCD  ⛶",
        connected.then_some(Action::Fullscreen),
        CYAN,
    );
    ui.text(
        (1082, 12, 164, 55),
        &format!("{:02}:{:02}:{:02}", time.hour, time.minute, time.second),
        36,
        true,
        WHITE,
        2,
    );
    match driver.screen {
        Screen::Transport => transport(ui, app, driver),
        Screen::Routes(tram) => routes(ui, app, driver, tram),
        Screen::Drive => drive(ui, app),
        Screen::Stops => stops(ui, app, driver),
    }
    ui.rect((0, 681, 1280, 39), 0x101D29);
    let warning = if driver.display_error {
        "Displej se nepodařilo spustit. Zkuste tlačítko Otevřít displej."
    } else if !app.load_error.is_empty() {
        "Import tras se nepodařilo načíst. K dispozici jsou vestavěné linky."
    } else if app.sound_error {
        "Hlášení není k dispozici. U importovaných zastávek zkontrolujte český hlas Windows."
    } else {
        "SIMULÁTOR  /  Data PID · ROPID · CC BY 4.0 · pid.cz/en/opendata"
    };
    ui.text((36, 682, 1210, 36), warning, 17, false, MUTED, 0);
}

fn transport(ui: &mut Ui, app: &App, driver: &Driver) {
    if let Some(canvas) = ui.canvas {
        let t = driver.entered.elapsed().as_secs_f64();
        for i in 0..20 {
            canvas.block(
                0x0C141D + (19 - i) as u32 * 0x000101,
                0,
                88 + i * 12,
                1280,
                12,
            );
        }
        canvas.block(BORDER, 145, 309, 990, 2);
        for x in [145, 392, 640, 888, 1135] {
            canvas.dot(x, 310, 5, BORDER);
        }
        let x = 145 + ((t * 0.41).fract() * 990.0) as i32;
        canvas.dot(x, 310, 9, 0x22434E);
        canvas.dot(x, 310, 4, CYAN);
    }
    ui.text(
        (65, 132, 1150, 36),
        "01   /   ZAHÁJENÍ SLUŽBY",
        20,
        true,
        CYAN,
        1,
    );
    ui.text(
        (55, 177, 1170, 88),
        "Jakým vozidlem pojedete?",
        58,
        true,
        WHITE,
        1,
    );
    ui.text(
        (60, 258, 1160, 40),
        "Vyberte typ dopravy a potom linku a směr.",
        24,
        false,
        MUTED,
        1,
    );

    let t = driver.entered.elapsed().as_secs_f64();
    for (i, (title, detail, color, tram)) in [
        ("Autobus", "Autobusové a trolejbusové linky", CYAN, false),
        ("Tramvaj", "Tramvajové linky", AMBER, true),
    ]
    .into_iter()
    .enumerate()
    {
        let x = 145 + i as i32 * 510;
        let progress = ((t - i as f64 * 0.09) / 0.33).clamp(0.0, 1.0);
        let y = 329 + ((1.0 - progress).powi(3) * 38.0) as i32;
        let b = (x, y, 480, 235);
        let hover = ui.target(b, Some(Action::Routes(tram)));
        ui.rect(b, if hover { color } else { BORDER });
        ui.rect(
            (x + 2, y + 2, 476, 231),
            if hover { 0x243B4E } else { PANEL },
        );
        ui.rect((x + 28, y + 28, 96, 83), BG);
        vehicle_icon(ui, x + 46, y + 40, tram, color);
        ui.text((x + 30, y + 117, 421, 57), title, 41, true, WHITE, 0);
        ui.text((x + 30, y + 176, 392, 42), detail, 21, false, MUTED, 0);
        ui.text((x + 403, y + 171, 45, 42), "→", 33, true, color, 2);
    }
    if app.ready {
        ui.button(
            (470, 610, 340, 46),
            "Zpět k aktivní jízdě",
            Some(Action::Drive),
            CYAN,
        );
    }
}

fn vehicle_icon(ui: &Ui, x: i32, y: i32, tram: bool, color: u32) {
    let offset = if tram { 8 } else { 0 };
    let y = y + offset;
    ui.rect((x + 4, y + 4, 52, 44), color);
    ui.rect((x + 9, y + 9, 42, 21), BG);
    ui.rect((x + 10, y + 37, 8, 5), BG);
    ui.rect((x + 42, y + 37, 8, 5), BG);
    if tram {
        ui.rect((x + 18, y - 5, 24, 3), color);
        ui.rect((x + 28, y - 5, 4, 10), color);
        ui.rect((x + 1, y + 52, 58, 3), color);
    } else {
        ui.rect((x + 7, y + 45, 10, 9), color);
        ui.rect((x + 43, y + 45, 10, 9), color);
    }
}

fn routes(ui: &mut Ui, app: &App, driver: &Driver, tram: bool) {
    let accent = if tram { AMBER } else { CYAN };
    ui.button(
        (36, 105, 170, 49),
        "←  Vozidlo",
        Some(Action::Transport),
        accent,
    );
    ui.text(
        (232, 102, 990, 56),
        if tram {
            "Tramvaj · linka a směr"
        } else {
            "Autobus · linka a směr"
        },
        38,
        true,
        WHITE,
        0,
    );

    ui.rect((36, 173, 344, 425), PANEL);
    ui.text((55, 181, 305, 27), "ČÍSLO LINKY", 17, true, accent, 0);
    ui.rect((54, 215, 309, 69), 0x0C1721);
    let blink = driver.filtered_at.elapsed().as_millis() % 1060 < 690;
    let input = if driver.query.is_empty() {
        "Např. 170".to_string()
    } else {
        format!("{}{}", driver.query, if blink { "▏" } else { "" })
    };
    ui.text(
        (66, 216, 285, 65),
        &input,
        if driver.query.is_empty() { 28 } else { 41 },
        true,
        if driver.query.is_empty() {
            MUTED
        } else {
            WHITE
        },
        0,
    );
    let underline = (driver.filtered_at.elapsed().as_secs_f64() / 0.26).min(1.0);
    ui.rect((54, 281, 309, 3), BORDER);
    ui.rect(
        (
            54,
            281,
            (309.0 * (1.0 - (1.0 - underline).powi(3))) as i32,
            3,
        ),
        accent,
    );

    for (i, key) in ['1', '2', '3', '4', '5', '6', '7', '8', '9', 'X', '0', '⌫']
        .into_iter()
        .enumerate()
    {
        let b = (54 + (i % 3) as i32 * 105, 300 + (i / 3) as i32 * 54, 99, 47);
        let action = if key == '⌫' {
            Action::Erase
        } else {
            Action::Key(key)
        };
        ui.button(b, &key.to_string(), Some(action), WHITE);
    }
    ui.button((54, 522, 113, 51), "Smazat", Some(Action::Clear), MUTED);
    ui.button(
        (173, 522, 190, 51),
        "Vyhledat  →",
        Some(Action::Find),
        accent,
    );

    let query = driver.query.trim();
    let entries: Vec<_> = app
        .routes
        .iter()
        .enumerate()
        .filter(|(_, route)| {
            let matches_mode = (route.mode == "tram") == tram;
            let matches_query = if driver.exact {
                route.line.eq_ignore_ascii_case(query)
            } else {
                route.line.starts_with(query)
            };
            matches_mode && matches_query
        })
        .collect();
    let info = if query.is_empty() {
        "Zadejte číslo na klávesnici nebo na panelu".to_string()
    } else {
        format!("VÝSLEDKY  ·  {}   /   {} variant", query, entries.len())
    };
    ui.text((405, 166, 821, 37), &info, 19, true, MUTED, 0);
    if entries.is_empty() {
        ui.rect((404, 211, 840, 190), PANEL);
        ui.text(
            (429, 239, 780, 53),
            "Žádná linka nenalezena",
            35,
            true,
            WHITE,
            0,
        );
        ui.text(
            (429, 303, 780, 44),
            "Zkontrolujte číslo nebo typ vozidla.",
            22,
            false,
            MUTED,
            0,
        );
    }
    for (i, (index, route)) in entries.iter().skip(driver.page * 6).take(6).enumerate() {
        let t = ((driver.filtered_at.elapsed().as_secs_f64() - i as f64 * 0.025) / 0.26)
            .clamp(0.0, 1.0);
        let x = 405 + (16.0 * (1.0 - t).powi(3)) as i32;
        let y = 208 + i as i32 * 65;
        let w = 1244 - x;
        let hover = ui.target((405, y, 839, 58), Some(Action::Select(*index)));
        ui.rect((x, y, w, 58), if hover { 0x28445A } else { PANEL });
        ui.rect((x, y, 96, 58), if tram { 0x594018 } else { 0x1C4B52 });
        let line_size = if route.line.len() > 4 { 22 } else { 33 };
        ui.text((x + 2, y, 92, 58), &route.line, line_size, true, WHITE, 1);
        let origin = &route.stops[0].name;
        let end = &route.stops.last().unwrap().name;
        let via = &route.stops[route.stops.len() / 2].name;
        ui.text(
            (x + 110, y + 1, w - 156, 31),
            &format!("{origin}   →   {end}"),
            24,
            true,
            WHITE,
            0,
        );
        ui.text(
            (x + 110, y + 32, w - 155, 22),
            &format!("přes {via}  ·  {} zastávek", route.stops.len()),
            16,
            false,
            MUTED,
            0,
        );
        ui.text((1212, y, 24, 58), "›", 30, true, accent, 1);
    }
    pager(ui, driver.page, entries.len());
}

fn pager(ui: &mut Ui, page: usize, total: usize) {
    ui.button(
        (36, 612, 204, 48),
        "←  Předchozí",
        (page > 0).then_some(Action::PageBack),
        CYAN,
    );
    ui.text(
        (261, 611, 758, 49),
        &format!("Strana {} / {}", page + 1, total.div_ceil(6).max(1)),
        21,
        false,
        MUTED,
        1,
    );
    ui.button(
        (1040, 612, 204, 48),
        "Další  →",
        ((page + 1) * 6 < total).then_some(Action::PageNext),
        CYAN,
    );
}

fn stops(ui: &mut Ui, app: &App, driver: &Driver) {
    ui.button((36, 107, 170, 49), "←  Jízda", Some(Action::Drive), CYAN);
    ui.text(
        (234, 106, 1006, 55),
        "Ruční výběr zastávky",
        39,
        true,
        WHITE,
        0,
    );
    let route = &app.routes[app.selection];
    for (index, stop) in route.stops.iter().enumerate().skip(driver.page * 6).take(6) {
        let y = 174 + (index % 6) as i32 * 71;
        let hover = ui.target((36, y, 1208, 62), Some(Action::Seek(index)));
        ui.rect((36, y, 1208, 62), if hover { 0x28445A } else { PANEL });
        ui.text(
            (50, y, 83, 62),
            &format!("{:02}", index + 1),
            25,
            true,
            CYAN,
            1,
        );
        ui.text((154, y, 900, 62), &stop.name, 31, true, WHITE, 0);
        if index == app.journey.index {
            ui.text((1062, y, 151, 62), "AKTUÁLNÍ", 19, true, GREEN, 2);
        }
    }
    pager(ui, driver.page, route.stops.len());
}

fn drive(ui: &mut Ui, app: &App) {
    let route = &app.routes[app.selection];
    let current = &route.stops[app.journey.index];
    let next = route.stops.get(app.journey.index + 1);
    let destination = route.stops.last().unwrap();

    ui.rect((36, 108, 1208, 73), PANEL);
    ui.rect(
        (36, 108, 105, 73),
        if route.mode == "tram" {
            0x594018
        } else {
            0x1C4B52
        },
    );
    ui.text((39, 109, 99, 71), &route.line, 42, true, WHITE, 1);
    ui.text((161, 113, 566, 31), "SMĚR", 16, true, MUTED, 0);
    ui.text((161, 136, 661, 39), &destination.name, 31, true, WHITE, 0);
    ui.button(
        (1032, 120, 195, 48),
        "Změnit linku",
        Some(Action::Transport),
        CYAN,
    );

    ui.rect((36, 195, 724, 105), PANEL);
    ui.rect(
        (36, 195, 5, 105),
        if app.journey.moving { MUTED } else { CYAN },
    );
    ui.text(
        (57, 203, 674, 30),
        if app.journey.moving {
            "POSLEDNÍ ZASTÁVKA"
        } else {
            "AKTUÁLNÍ ZASTÁVKA"
        },
        19,
        true,
        MUTED,
        0,
    );
    ui.text((57, 231, 674, 63), &current.name, 39, true, WHITE, 0);

    ui.rect((36, 310, 724, 112), PANEL);
    ui.rect(
        (36, 310, 5, 112),
        if app.journey.moving { CYAN } else { BORDER },
    );
    let next_label = if next.is_some() {
        format!(
            "PŘÍŠTÍ ZASTÁVKA   ·   {} MIN",
            app.journey.minutes(route, app.journey.index + 1)
        )
    } else {
        "KONEČNÁ ZASTÁVKA".into()
    };
    ui.text((57, 318, 676, 32), &next_label, 19, true, MUTED, 0);
    ui.text(
        (57, 345, 676, 64),
        next.map_or("Konec spoje", |stop| &stop.name),
        39,
        true,
        WHITE,
        0,
    );

    ui.rect((774, 195, 470, 227), PANEL);
    let door = match app.doors {
        Doors::Closed => ("Zavřené", MUTED),
        Doors::Open => ("Otevřené", GREEN),
        Doors::Closing => ("Zavírají se", AMBER),
    };
    ui.text((796, 205, 426, 32), "STAV VOZIDLA", 19, true, MUTED, 0);
    ui.text(
        (796, 238, 425, 49),
        if app.journey.moving {
            "V jízdě"
        } else {
            "V zastávce"
        },
        36,
        true,
        WHITE,
        0,
    );
    ui.text(
        (796, 288, 425, 37),
        &format!("Dveře  ·  {}", door.0),
        24,
        true,
        door.1,
        0,
    );
    ui.text(
        (796, 335, 425, 43),
        &format!(
            "Zastávka {} / {}    ·    Pásmo {}",
            app.journey.index + 1,
            route.stops.len(),
            current.zone
        ),
        21,
        false,
        MUTED,
        0,
    );
    ui.rect((796, 389, 425, 6), BORDER);
    ui.rect(
        (
            796,
            389,
            425 * app.journey.index as i32 / (route.stops.len() as i32 - 1),
            6,
        ),
        CYAN,
    );

    ui.button(
        (36, 440, 230, 73),
        "←  Předchozí",
        (app.journey.index > 0 || app.journey.moving).then_some(Action::Previous),
        WHITE,
    );
    ui.button(
        (279, 440, 481, 73),
        if app.journey.moving {
            "Potvrdit příjezd  →"
        } else {
            "Potvrdit odjezd  →"
        },
        (next.is_some() && app.doors == Doors::Closed).then_some(Action::Advance),
        CYAN,
    );
    ui.button(
        (774, 440, 230, 73),
        "Otevřít dveře",
        (!app.journey.moving && app.doors == Doors::Closed).then_some(Action::Open),
        GREEN,
    );
    ui.button(
        (1016, 440, 228, 73),
        "Zavřít dveře",
        (app.doors == Doors::Open).then_some(Action::Close),
        AMBER,
    );

    ui.button(
        (36, 528, 230, 73),
        "Vybrat zastávku",
        Some(Action::Stops),
        WHITE,
    );
    ui.button(
        (279, 528, 481, 73),
        if app.journey.automatic {
            "Pozastavit automatickou jízdu"
        } else {
            "Spustit automatickou jízdu"
        },
        next.is_some().then_some(Action::Auto),
        CYAN,
    );
    ui.button(
        (774, 528, 230, 73),
        "Hlášení zastávky",
        (app.doors != Doors::Closing).then_some(Action::Announce),
        WHITE,
    );
    ui.button(
        (1016, 528, 228, 73),
        if app.muted {
            "Zapnout zvuk"
        } else {
            "Ztlumit zvuk"
        },
        Some(Action::Mute),
        WHITE,
    );
    ui.text(
        (38, 617, 1203, 43),
        "Pro ruční jízdu potvrďte odjezd a poté příjezd. Před odjezdem zavřete dveře.",
        20,
        false,
        MUTED,
        0,
    );
}
