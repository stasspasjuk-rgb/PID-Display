mod route;

#[cfg(windows)]
mod windows;

pub type AudioBank = (&'static str, &'static [u8]);

pub fn run(driver: bool, audio: Option<AudioBank>) {
    #[cfg(windows)]
    windows::run(driver, audio);
    #[cfg(not(windows))]
    {
        let _ = (driver, audio);
        eprintln!("PID Display requires Windows.");
    }
}
