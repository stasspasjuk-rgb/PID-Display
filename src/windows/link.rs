use std::{
    io::{self, BufRead, Write},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Doors {
    #[default]
    Closed,
    Open,
    Closing,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub selection: usize,
    pub ready: bool,
    pub stop: usize,
    pub moving: bool,
    pub automatic: bool,
    pub elapsed_ms: u64,
    pub doors: u8,
    pub monitor_epoch: u32,
}

impl Snapshot {
    fn encode(self) -> String {
        format!(
            "PID2 {} {} {} {} {} {} {} {}\n",
            self.selection,
            u8::from(self.ready),
            self.stop,
            u8::from(self.moving),
            u8::from(self.automatic),
            self.elapsed_ms,
            self.doors,
            self.monitor_epoch,
        )
    }

    fn decode(line: &str) -> Option<Self> {
        let mut fields = line.split_ascii_whitespace();
        if fields.next()? != "PID2" {
            return None;
        }
        let flag = |value: &str| match value {
            "0" => Some(false),
            "1" => Some(true),
            _ => None,
        };
        let snapshot = Self {
            selection: fields.next()?.parse().ok()?,
            ready: flag(fields.next()?)?,
            stop: fields.next()?.parse().ok()?,
            moving: flag(fields.next()?)?,
            automatic: flag(fields.next()?)?,
            elapsed_ms: fields.next()?.parse().ok()?,
            doors: fields.next()?.parse().ok()?,
            monitor_epoch: fields.next()?.parse().ok()?,
        };
        (fields.next().is_none() && snapshot.doors <= 2).then_some(snapshot)
    }
}

pub struct Display {
    process: Child,
    updates: SyncSender<Snapshot>,
}

impl Display {
    pub fn start() -> io::Result<Self> {
        let mut process = Command::new(std::env::current_exe()?)
            .arg("--linked")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let mut pipe = process.stdin.take().expect("piped stdin");
        let (updates, receive) = mpsc::sync_channel::<Snapshot>(1);
        thread::spawn(move || {
            for snapshot in receive {
                if pipe.write_all(snapshot.encode().as_bytes()).is_err() {
                    break;
                }
            }
        });
        Ok(Self { process, updates })
    }

    pub fn running(&mut self) -> bool {
        matches!(self.process.try_wait(), Ok(None))
    }

    pub fn send(&self, snapshot: Snapshot) {
        let _ = self.updates.try_send(snapshot);
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

pub fn listen() -> Receiver<Snapshot> {
    let (send, receive) = mpsc::sync_channel(8);
    thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else {
                break;
            };
            if let Some(snapshot) = Snapshot::decode(&line) {
                if send.send(snapshot).is_err() {
                    break;
                }
            }
        }
    });
    receive
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trip_and_validation() {
        let snapshot = Snapshot {
            selection: 3,
            ready: true,
            stop: 8,
            moving: true,
            automatic: true,
            elapsed_ms: 100,
            doors: 2,
            monitor_epoch: 9,
        };
        assert_eq!(Snapshot::decode(&snapshot.encode()), Some(snapshot));
        assert!(Snapshot::decode("PID2 0 1 0 0 0 0 9 0").is_none());
        assert!(Snapshot::decode("PID1 0 1 0 0 0 0 0 0").is_none());
    }
}
