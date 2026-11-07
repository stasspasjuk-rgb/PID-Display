use super::{App, Canvas, Doors, GetLocalTime, SystemTime, BLACK, WHITE};
use crate::route::{Journey, Route};

const GREY: u32 = 0xC8C8C8;
const BUS_BG: u32 = 0x333331;
const BUS_INK: u32 = 0x1B1B19;
const TRAM_BG: u32 = 0x0C1D3E;
const TRAM_DARK: u32 = 0x08162F;
const ICE: u32 = 0xDBEAFB;
const TRACK: u32 = 0xAEC5E4;

struct Motion {
    previous: usize,
    progress: f64,
}

impl Motion {
    fn for_app(app: &App) -> Self {
        let current = app.journey.shown();
        if let Some((previous, started)) = app.transition {
            let elapsed = started.elapsed().as_secs_f64() / 0.64;
            if elapsed < 1.0 && previous != current {
                let smooth = elapsed * elapsed * (3.0 - 2.0 * elapsed);
                return Self {
                    previous,
                    progress: smooth,
                };
            }
        }
        Self {
            previous: current,
            progress: 1.0,
        }
    }

    fn cursor(&self, current: usize) -> f64 {
        if current.abs_diff(self.previous) == 1 {
            self.previous as f64 + (current as f64 - self.previous as f64) * self.progress
        } else {
            current as f64
        }
    }

    fn offset(&self, current: usize, distance: i32) -> (i32, i32) {
        let direction = if current >= self.previous { 1.0 } else { -1.0 };
        let position = distance as f64 * direction;
        (
            (-position * self.progress).round() as i32,
            (position * (1.0 - self.progress)).round() as i32,
        )
    }
}

pub fn draw(c: &Canvas, app: &App) {
    let route = &app.routes[app.selection];
    let mut time = SystemTime::default();
    unsafe { GetLocalTime(&mut time) };
    let clock = format!("{:02}:{:02}", time.hour, time.minute);
    let motion = Motion::for_app(app);
    if route.mode == "tram" {
        tram(c, route, &app.journey, app.doors, &clock, &motion);
    } else {
        bus(c, route, &app.journey, app.doors, &clock, &motion);
    }
}

fn size(text: &str, preferred: i32, budget: i32) -> i32 {
    let count = text.chars().count().max(1) as i32;
    preferred.min((budget * 2 / count).max(18))
}

fn heading(
    c: &Canvas,
    route: &Route,
    index: usize,
    bounds: (i32, i32, i32, i32),
    color: u32,
    font: i32,
) {
    let stop = &route.stops[index];
    let (x, y, w, h) = bounds;
    let width = c.text(
        &stop.name,
        bounds,
        size(&stop.name, font, w),
        true,
        color,
        0,
    );
    c.transfers(&stop.transfers, y + (h - 31) / 2, 31, x + width + 12);
}

fn phase(journey: &Journey, terminal: bool) -> &'static str {
    if terminal {
        "Konečná zastávka / Final stop"
    } else if journey.moving {
        "Příští zastávka / Next stop"
    } else {
        "Zastávka / Stop"
    }
}

fn bus(c: &Canvas, route: &Route, journey: &Journey, doors: Doors, clock: &str, motion: &Motion) {
    let index = journey.shown();
    let last = route.stops.len() - 1;
    let terminal = !journey.moving && index == last;
    let stop = &route.stops[index];
    let axis = 319;
    let name = 375;
    c.block(BUS_BG, 0, 0, 1280, 720);
    c.block(GREY, 0, 0, if terminal { 1280 } else { 266 }, 153);
    c.text(
        &route.line,
        (8, 1, 250, 145),
        size(&route.line, 112, 225),
        true,
        BUS_INK,
        1,
    );
    c.text(
        &format!("pásmo zone  {}", stop.zone),
        (1024, 11, 232, 42),
        24,
        true,
        if terminal { BUS_INK } else { WHITE },
        2,
    );
    c.block(WHITE, axis - 3, 0, 6, if terminal { 102 } else { 648 });
    if motion.progress < 1.0 {
        let radius = 30 + (16.0 * motion.progress) as i32;
        c.dot(axis, 93, radius, mix(0xBCBCB9, BUS_BG, motion.progress));
        c.dot(axis, 93, radius - 2, BUS_BG);
    }
    c.dot(axis, 93, 29, WHITE);
    if !terminal {
        c.dot(axis, 93, 26, BUS_BG);
    }

    let context = if journey.moving {
        format!(
            "{} min   ·   {}",
            journey.minutes(route, index),
            phase(journey, terminal)
        )
    } else {
        phase(journey, terminal).to_string()
    };
    c.text(
        &context,
        (name, 10, 626, 36),
        21,
        false,
        if terminal { BUS_INK } else { WHITE },
        0,
    );
    if terminal {
        heading(c, route, index, (name, 45, 702, 94), BUS_INK, 65);
        c.block(0x1C1C1B, 38, 203, 220, 190);
        c.block(WHITE, 65, 229, 151, 134);
        c.block(0x1C1C1B, 74, 238, 133, 116);
        c.block(0x1C1C1B, 199, 259, 19, 68);
        c.text("→", (117, 245, 115, 96), 69, true, WHITE, 1);
        c.text(
            "Konečná zastávka,",
            (name, 170, 850, 72),
            58,
            true,
            WHITE,
            0,
        );
        c.text(
            "prosíme, vystupte.",
            (name, 236, 850, 72),
            58,
            true,
            WHITE,
            0,
        );
        c.text(
            "Final stop, please leave the vehicle.",
            (name, 330, 863, 64),
            38,
            false,
            WHITE,
            0,
        );
    } else {
        c.clip((name, 48, 880, 87), || {
            let (old, new) = motion.offset(index, 85);
            if motion.progress < 1.0 {
                heading(
                    c,
                    route,
                    motion.previous,
                    (name, 48 + old, 711, 87),
                    WHITE,
                    63,
                );
            }
            heading(c, route, index, (name, 48 + new, 711, 87), WHITE, 63);
        });
        if journey.moving && index < last {
            c.text(
                "Na znamení / Request stop",
                (name, 133, 762, 32),
                20,
                false,
                0xE4E4E1,
                0,
            );
        }

        let cursor = motion.cursor(index);
        c.clip((145, 172, 1110, 464), || {
            let start = cursor.floor() as usize + 1;
            for i in start..last.min(start + 8) {
                if i == index {
                    continue;
                }
                let y = 173 + ((i as f64 - cursor - 1.0) * 74.0).round() as i32;
                let upcoming = &route.stops[i];
                c.dot(axis, y + 36, 19, WHITE);
                c.dot(axis, y + 36, 16, BUS_BG);
                if i >= index {
                    c.text(
                        &format!("{} min", journey.minutes(route, i)),
                        (155, y, 128, 72),
                        21,
                        false,
                        WHITE,
                        2,
                    );
                }
                heading(c, route, i, (name, y, 715, 71), WHITE, 48);
                if !upcoming.zone.is_empty() && upcoming.zone != stop.zone {
                    c.text(&upcoming.zone, (1130, y, 115, 72), 21, false, GREY, 2);
                }
            }
        });
    }
    c.block(GREY, 0, 641, 1280, 79);
    c.text(clock, (17, 641, 169, 78), 49, true, BUS_INK, 0);
    if !terminal {
        c.dot(axis, 679, 19, BUS_INK);
        c.text("↓", (axis - 31, 611, 62, 56), 45, true, WHITE, 1);
        c.text(
            &format!("{} min", journey.minutes(route, last)),
            (185, 646, 98, 67),
            22,
            false,
            BUS_INK,
            2,
        );
        heading(c, route, last, (name, 643, 708, 74), BUS_INK, 48);
    }
    match doors {
        Doors::Closing => door_notice(c, 505, 136, BUS_INK),
        Doors::Open => {
            c.text("Dveře otevřeny", (16, 560, 189, 47), 20, true, WHITE, 0);
        }
        Doors::Closed => {}
    }
}

fn tram(c: &Canvas, route: &Route, journey: &Journey, doors: Doors, clock: &str, motion: &Motion) {
    let index = journey.shown();
    let last = route.stops.len() - 1;
    let terminal = !journey.moving && index == last;
    c.block(BLACK, 0, 0, 1280, 720);
    c.block(TRAM_BG, 0, 150, 1280, 420);
    c.block(0xF6F8FB, 18, 170, 229, 379);
    c.text(
        &route.line,
        (23, 175, 219, 356),
        size(&route.line, 166, 198),
        true,
        TRAM_DARK,
        1,
    );

    c.block(TRAM_DARK, 266, 168, 997, 56);
    c.text(
        "Výchozí zastávka / Origin",
        (279, 170, 415, 23),
        14,
        false,
        TRACK,
        0,
    );
    let origin = &route.stops[0].name;
    c.text(
        origin,
        (279, 191, 418, 31),
        size(origin, 21, 400),
        true,
        ICE,
        0,
    );
    c.text(
        &format!("Pásmo {}", route.stops[index].zone),
        (700, 171, 160, 50),
        18,
        false,
        TRACK,
        0,
    );
    let end = &route.stops[last].name;
    c.text(end, (830, 170, 299, 52), size(end, 21, 287), true, ICE, 2);
    c.text(clock, (1141, 170, 108, 52), 28, true, WHITE, 2);

    c.block(ICE, 266, 232, 997, 91);
    c.text(
        phase(journey, terminal),
        (281, 233, 749, 25),
        16,
        false,
        TRAM_DARK,
        0,
    );
    c.clip((277, 257, 975, 64), || {
        let (old, new) = motion.offset(index, 950);
        if motion.progress < 1.0 {
            heading(
                c,
                route,
                motion.previous,
                (283 + old, 258, 825, 62),
                TRAM_DARK,
                48,
            );
        }
        heading(c, route, index, (283 + new, 258, 825, 62), TRAM_DARK, 48);
    });
    if terminal {
        c.text(
            "Prosíme, vystupte.",
            (283, 357, 966, 75),
            51,
            true,
            WHITE,
            0,
        );
        c.text(
            "Please leave the tram.",
            (285, 431, 948, 57),
            29,
            false,
            TRACK,
            0,
        );
    } else {
        tram_track(c, route, index, motion);
    }
    match doors {
        Doors::Closing => door_notice(c, 423, 127, TRAM_DARK),
        Doors::Open => {
            c.text("Dveře otevřeny", (18, 508, 229, 37), 17, true, TRAM_DARK, 1);
        }
        Doors::Closed => {}
    }
}

fn tram_track(c: &Canvas, route: &Route, index: usize, motion: &Motion) {
    let cursor = motion.cursor(index);
    let start = cursor.floor() as usize;
    let base = 327;
    let step = 98;
    let line_y = 515;
    let slots = 10;
    let end = route.stops.len().min(start + slots);
    c.clip((266, 328, 997, 223), || {
        let end_x = base + ((route.stops.len() as f64 - 1.0 - cursor) * step as f64).round() as i32;
        let last_x = end_x.min(1243);
        c.block(0x607CA8, 266, line_y - 3, (last_x - 266).max(1), 6);
        c.block(WHITE, base, line_y - 3, (last_x - base).max(1), 6);
        for i in start..end {
            let x = base + ((i as f64 - cursor) * step as f64).round() as i32;
            let selected = if i == index {
                motion.progress
            } else if i == motion.previous {
                1.0 - motion.progress
            } else {
                0.0
            };
            if selected > 0.0 {
                c.polygon(
                    mix(TRAM_BG, 0xC1D7F2, selected),
                    &[(x - 19, 506), (x + 14, 516), (x + 132, 354), (x + 100, 343)],
                );
            }
            if i == index && motion.progress < 1.0 {
                let radius = 14 + (15.0 * motion.progress) as i32;
                c.dot(x, line_y, radius, mix(TRACK, TRAM_BG, motion.progress));
                c.dot(x, line_y, radius - 2, TRAM_BG);
            }
            c.dot(x, line_y, if i == index { 13 } else { 8 }, WHITE);
            if i != index {
                c.dot(x, line_y, 5, TRAM_BG);
            }
            let stop = &route.stops[i];
            let label_size = size(&stop.name, 24, 173);
            let projection = stop.name.chars().count() as i32 * label_size / 3;
            if x + projection < 1247 {
                let text_color = if selected > 0.52 { TRAM_DARK } else { ICE };
                c.angled(&stop.name, x - 15, 486, label_size, text_color);
            }
            if !stop.transfers.is_empty() {
                c.transfers(&stop.transfers, 530, 17, x - 8);
            }
        }
        if end_x > 1243 {
            c.text("›", (1211, 491, 43, 48), 40, true, WHITE, 2);
        }
    });
}

fn mix(from: u32, to: u32, amount: f64) -> u32 {
    [0, 8, 16].into_iter().fold(0, |color, shift| {
        let a = ((from >> shift) & 255) as f64;
        let b = ((to >> shift) & 255) as f64;
        color | (((a + (b - a) * amount).round() as u32) << shift)
    })
}

fn door_notice(c: &Canvas, y: i32, height: i32, bg: u32) {
    c.block(bg, 0, y, 1280, height);
    c.block(0xE9BA50, 0, y, 1280, 5);
    c.text(
        "Dveře se zavírají",
        (38, y + 10, 1204, 58),
        45,
        true,
        WHITE,
        1,
    );
    c.text(
        "Doors are closing",
        (38, y + 69, 1204, 37),
        24,
        false,
        0xE9BA50,
        1,
    );
}
