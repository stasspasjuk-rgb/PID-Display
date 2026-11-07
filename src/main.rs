#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    let driver = !std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--linked");
    let audio = driver.then_some((
        include_str!("../data/demo-audio.tsv"),
        include_bytes!("../data/demo-audio.wavpack").as_slice(),
    ));
    pid_display::run(driver, audio);
}
