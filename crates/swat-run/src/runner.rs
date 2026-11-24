//! Finding the `swatplus-rust` program: the one the user chose, the one named by
//! [`ENV`], the one [`crate::install`] put in place, one beside the app, or one on the `PATH`, in
//! that order; the first that answers `--version` as swatplus-rust of a supported version.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The swatplus-rust release that [`crate::install`] fetches: the version RustGIS-SwatPlus was
/// tested with. Moved forward by a pull request, after checking the new release's events
/// against [`crate::Event`].
pub const RELEASE: &str = "1.2.0";

/// The oldest engine version used: 1.1.0 has `--threads`, and every 1.x the events read here.
pub const SUPPORTED_FROM: (u64, u64, u64) = (1, 1, 0);

/// The first engine version not used: the next major release may change its command line.
pub const SUPPORTED_BEFORE: (u64, u64, u64) = (2, 0, 0);

/// The environment variable that names the program, ahead of the usual places.
pub const ENV: &str = "SWATPLUS_RUST";

/// The program's file name.
pub fn exe_name() -> &'static str {
    if cfg!(windows) { "swatplus-rust.exe" } else { "swatplus-rust" }
}

/// Where the pinned release's downloads are published.
pub fn download_url() -> String {
    format!("https://github.com/andriyanto/swatplus-rust/releases/tag/v{RELEASE}")
}

/// The program the user chose, ahead of the usual places.
static CHOSEN: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The program found and its version, kept until the choice changes (`None`: not looked yet).
static FOUND: Mutex<Option<Option<(PathBuf, String)>>> = Mutex::new(None);

/// A command for the program, without a console window on Windows.
pub(crate) fn command(exe: &Path) -> Command {
    #[cfg_attr(not(windows), expect(unused_mut, reason = "only Windows sets creation flags"))]
    let mut c = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// The `major.minor.patch` of `swatplus-rust 1.2.0`, or `None`.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let word = text.trim().strip_prefix("swatplus-rust")?.split_whitespace().next()?;
    let core = word.trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let v = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(v)
}

/// Checks that a program (`what`), which reported `version`, is an engine version used here.
pub fn check_supported(what: &str, version: &str) -> Result<(), String> {
    let range = |(a, b, c): (u64, u64, u64)| format!("{a}.{b}.{c}");
    let (from, before) = (range(SUPPORTED_FROM), range(SUPPORTED_BEFORE));
    match parse_version(version) {
        Some(v) if v >= SUPPORTED_FROM && v < SUPPORTED_BEFORE => Ok(()),
        Some(_) => Err(format!("{what} is {version}: RustGIS-SwatPlus runs swatplus-rust {from} up to, but not including, {before}")),
        None => Err(format!("{what} reports its version as \"{}\", not as swatplus-rust", version.trim())),
    }
}

/// The program's answer to `--version` (`swatplus-rust 1.2.0`), if it is swatplus-rust and
/// answers within ten seconds; a program that only happens to have the name is killed then.
pub fn version_of(exe: &Path) -> Option<String> {
    if !exe.is_file() {
        return None;
    }
    let mut c = command(exe);
    c.arg("--version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = c.spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < Duration::from_secs(10) => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let out = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && text.starts_with("swatplus-rust")).then_some(text)
}

/// The places looked in, in order.
fn candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(p) = CHOSEN.lock()
        && let Some(p) = p.clone()
    {
        v.push(p);
    }
    if let Some(p) = std::env::var_os(ENV) {
        v.push(PathBuf::from(p));
    }
    if let Some(dir) = crate::install::install_dir() {
        v.push(dir.join(exe_name()));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        v.push(dir.join(exe_name()));
    }
    if let Some(path) = std::env::var_os("PATH") {
        // Absolute entries only: an empty or relative one would mean the current folder.
        v.extend(std::env::split_paths(&path).filter(|d| d.is_absolute()).map(|d| d.join(exe_name())));
    }
    v
}

/// The program to use and its version, or `None`; kept until the choice changes.
pub fn find() -> Option<(PathBuf, String)> {
    if let Ok(f) = FOUND.lock()
        && let Some(v) = f.as_ref()
    {
        return v.clone();
    }
    let found = candidates().into_iter().find_map(|p| {
        let v = version_of(&p)?;
        match check_supported(&p.display().to_string(), &v) {
            Ok(()) => Some((p, v)),
            Err(e) => {
                log::warn!("swatplus-rust passed over: {e}");
                None
            }
        }
    });
    if let Ok(mut f) = FOUND.lock() {
        *f = Some(found.clone());
    }
    found
}

/// Uses `exe` as the engine (`None`: look in the usual places again). Fails, keeping the
/// current choice, when `exe` isn't swatplus-rust of a supported version.
pub fn choose(exe: Option<PathBuf>) -> Result<Option<(PathBuf, String)>, String> {
    if let Some(p) = &exe {
        let version = version_of(p).ok_or_else(|| format!("{} is not the swatplus-rust program", p.display()))?;
        check_supported(&p.display().to_string(), &version)?;
    }
    if let Ok(mut c) = CHOSEN.lock() {
        *c = exe;
    }
    if let Ok(mut f) = FOUND.lock() {
        *f = None;
    }
    Ok(find())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_and_checked() {
        assert_eq!(parse_version("swatplus-rust 1.2.0\n"), Some((1, 2, 0)));
        assert_eq!(parse_version("swatplus-rust v1.10.3-rc1"), Some((1, 10, 3)));
        assert_eq!(parse_version("geolibre-cli 1.8.3"), None);
        assert_eq!(parse_version("swatplus-rust 1.2"), None);
        assert!(check_supported("x", "swatplus-rust 1.2.0").is_ok());
        assert!(check_supported("x", "swatplus-rust 1.1.0").is_ok());
        assert!(check_supported("x", "swatplus-rust 1.0.3").unwrap_err().contains("1.1.0 up to"));
        assert!(check_supported("x", "swatplus-rust 2.0.0").is_err());
        assert!(check_supported("x", "something else").unwrap_err().contains("not as swatplus-rust"));
        assert!(check_supported("x", &format!("swatplus-rust {RELEASE}")).is_ok(), "the pinned release is supported");
    }
}
