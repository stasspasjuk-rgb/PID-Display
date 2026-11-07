use std::time::{Duration, Instant};

pub struct Stop {
    pub id: String,
    pub name: String,
    pub zone: String,
    pub transfers: String,
    pub seconds: u32,
}

pub struct Route {
    pub line: String,
    pub mode: String,
    pub stops: Vec<Stop>,
}

pub fn parse(data: &str) -> Result<Vec<Route>, String> {
    let mut routes = Vec::<Route>::new();
    for (index, row) in data.trim_start_matches('\u{feff}').lines().enumerate() {
        if row.trim().is_empty() || row.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = row.split('\t').collect();
        let bad = || format!("Neplatný řádek {}", index + 1);
        match fields.as_slice() {
            ["route", line, mode]
                if !line.is_empty() && matches!(*mode, "tram" | "bus" | "trolley") =>
            {
                routes.push(Route {
                    line: line.to_string(),
                    mode: mode.to_string(),
                    stops: Vec::new(),
                });
            }
            ["stop", id, name, zone, transfers, seconds] if !id.is_empty() && !name.is_empty() => {
                let route = routes.last_mut().ok_or_else(bad)?;
                let seconds: u32 = seconds.parse().map_err(|_| bad())?;
                if seconds > 86_400 || route.stops.last().is_some_and(|s| s.seconds > seconds) {
                    return Err(bad());
                }
                route.stops.push(Stop {
                    id: id.to_string(),
                    name: name.to_string(),
                    zone: zone.to_string(),
                    transfers: transfers.to_string(),
                    seconds,
                });
            }
            _ => return Err(bad()),
        }
    }
    if routes.is_empty() || routes.iter().any(|route| route.stops.len() < 2) {
        return Err("Každá trasa musí mít alespoň dvě zastávky.".into());
    }
    Ok(routes)
}

pub struct Journey {
    pub index: usize,
    pub moving: bool,
    pub automatic: bool,
    pub changed: Instant,
    paused_elapsed: Duration,
    resumed: Instant,
}

impl Journey {
    pub fn new() -> Self {
        Self {
            index: 0,
            moving: false,
            automatic: false,
            changed: Instant::now(),
            paused_elapsed: Duration::ZERO,
            resumed: Instant::now(),
        }
    }

    pub fn advance(&mut self, route: &Route) -> bool {
        if self.index + 1 == route.stops.len() {
            self.automatic = false;
            return false;
        }
        if self.moving {
            self.index += 1;
            if self.index + 1 == route.stops.len() {
                self.automatic = false;
            }
        }
        self.moving = !self.moving;
        self.changed = Instant::now();
        self.resumed = self.changed;
        self.paused_elapsed = Duration::ZERO;
        true
    }

    pub fn previous(&mut self) {
        if !self.moving {
            self.index = self.index.saturating_sub(1);
        }
        self.moving = false;
        self.changed = Instant::now();
        self.resumed = self.changed;
        self.paused_elapsed = Duration::ZERO;
    }

    pub fn elapsed(&self) -> Duration {
        if self.automatic {
            self.paused_elapsed + self.resumed.elapsed()
        } else {
            self.paused_elapsed
        }
    }

    pub fn toggle(&mut self) {
        self.paused_elapsed = self.elapsed();
        self.automatic = !self.automatic;
        self.resumed = Instant::now();
    }

    pub fn seek(&mut self, index: usize) {
        *self = Self::new();
        self.index = index;
    }

    pub fn mirror(&mut self, index: usize, moving: bool, automatic: bool, elapsed: Duration) {
        if self.index != index || self.moving != moving {
            self.changed = Instant::now();
        }
        self.index = index;
        self.moving = moving;
        self.automatic = automatic;
        self.paused_elapsed = elapsed;
        self.resumed = Instant::now();
    }

    pub fn shown(&self) -> usize {
        self.index + usize::from(self.moving)
    }

    pub fn delay(&self, route: &Route) -> Duration {
        if !self.moving {
            return Duration::from_secs(8);
        }
        let seconds = route.stops[self.index + 1].seconds - route.stops[self.index].seconds;
        Duration::from_secs((seconds as u64 / 8).clamp(8, 30))
    }

    pub fn minutes(&self, route: &Route, target: usize) -> u32 {
        let scheduled = route.stops[target].seconds - route.stops[self.index].seconds;
        let elapsed = if self.moving {
            let progress =
                (self.elapsed().as_secs_f64() / self.delay(route).as_secs_f64()).min(1.0);
            let leg = route.stops[self.index + 1].seconds - route.stops[self.index].seconds;
            (progress * f64::from(leg)) as u32
        } else {
            0
        };
        scheduled.saturating_sub(elapsed).div_ceil(60)
    }

    pub fn announcement(&self, route: &Route) -> (String, &'static str) {
        let stop = &route.stops[self.shown()];
        if self.moving {
            let request = if route.mode == "tram" || self.shown() + 1 == route.stops.len() {
                ""
            } else {
                ". Zastávka na znamení"
            };
            return (format!("Příští zastávka, {}{request}.", stop.name), "next");
        }
        if self.index + 1 == route.stops.len() {
            return (
                format!("{}. Konečná zastávka, prosíme vystupte.", stop.name),
                "final",
            );
        }
        let mut text = format!("{}.", stop.name);
        for transfer in stop.transfers.chars() {
            match transfer {
                'A' | 'B' | 'C' => text.push_str(&format!(" Přestup na metro {transfer}.")),
                'S' => text.push_str(" Přestup na linky S a další vlakové spoje."),
                _ => {}
            }
        }
        (text, "arrive")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traverses_every_phase_and_stops_at_terminal() {
        for route in parse(include_str!("../data/prague.tsv")).unwrap() {
            let mut journey = Journey::new();
            journey.automatic = true;
            for i in 1..route.stops.len() {
                assert!(journey.advance(&route));
                assert_eq!(journey.shown(), i);
                assert_eq!(journey.announcement(&route).1, "next");
                assert!(journey.advance(&route));
                assert_eq!(journey.index, i);
            }
            assert_eq!(journey.announcement(&route).1, "final");
            assert!(!journey.automatic);
            assert!(!journey.advance(&route));
            journey.previous();
            assert_eq!(journey.index, route.stops.len() - 2);
        }
    }

    #[test]
    fn rejects_backwards_times_and_empty_routes() {
        assert!(parse("route\t22\ttram\n").is_err());
        assert!(parse("route\t22\ttram\nstop\ta\tA\tP\t\t60\nstop\tb\tB\tP\t\t0").is_err());
    }
}
