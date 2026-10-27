//! Opening the dashboard in the browser.

use std::process::{Command, Stdio};

/// Opens `url` in the default browser, reporting why it could not be opened.
///
/// The caller prints the URL as well, so a headless or unusual host still has a way
/// in: opening a browser is a convenience here, never the only way to reach the page.
pub fn browser(url: &str) -> Result<(), String> {
    let (program, arguments): (&str, Vec<&str>) = if cfg!(target_os = "windows") {
        // The empty argument is the window title `start` takes before its target.
        ("cmd", vec!["/C", "start", "", url])
    } else if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else {
        ("xdg-open", vec![url])
    };
    Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not run {program}: {error}"))
}
