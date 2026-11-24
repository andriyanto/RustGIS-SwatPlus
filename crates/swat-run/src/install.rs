//! One-click install of the engine: the archive for this platform from the pinned swatplus-rust
//! release ([`crate::runner::RELEASE`]), downloaded over HTTPS, checked against the SHA-256
//! GitHub publishes for it, unpacked and tried, and only then put in RustGIS's data folder,
//! where [`crate::runner`] looks for it. A download that fails any of these leaves an installed
//! program as it was.
//!
//! The checksum comes from the same release, so it guards against a corrupted or truncated
//! download, not against a compromised release: trust rests on HTTPS and on the swatplus-rust
//! GitHub releases, as when downloading the program by hand.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

/// The GitHub API address of the pinned release.
fn release_api_url() -> String {
    format!("https://api.github.com/repos/andriyanto/swatplus-rust/releases/tags/v{}", crate::runner::RELEASE)
}

/// Archives larger than this are refused (the program is about 1 MB packed).
const MAX_BYTES: u64 = 100 * 1024 * 1024;

/// The release target this build runs on, as the archives name it.
pub fn target() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        _ => return None,
    })
}

/// Where an installed program lives: a `swatplus-rust` folder in the app's data folder.
pub fn install_dir() -> Option<PathBuf> {
    rustgis_data::dirs::app_data_dir().map(|b| b.join("swatplus-rust"))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(600)))
        .tls_config(rustgis_data::http::tls_config())
        .build()
        .into()
}

fn get(agent: &ureq::Agent, url: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
    agent
        .get(url)
        .header("User-Agent", concat!("RustGIS-SwatPlus/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| rustgis_data::diagnostics::message_with_address(url, &e.to_string()))
}

/// The archive's name for `target` in release `tag`.
pub fn asset_name(tag: &str, target: &str) -> String {
    let ext = if target.contains("windows") { "zip" } else { "tar.gz" };
    format!("swatplus-rust-{tag}-{target}.{ext}")
}

/// The archive's address and SHA-256 (hexadecimal) in the release's information.
pub fn find_asset(release: &Value, name: &str) -> Result<(String, String), String> {
    let asset = release
        .get("assets")
        .and_then(Value::as_array)
        .and_then(|a| a.iter().find(|a| a.get("name").and_then(Value::as_str) == Some(name)))
        .ok_or_else(|| format!("the release has no {name}"))?;
    let url = asset.get("browser_download_url").and_then(Value::as_str).ok_or_else(|| format!("{name}: no download address"))?;
    let digest = asset
        .get("digest")
        .and_then(Value::as_str)
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| format!("{name}: the release publishes no SHA-256 for it"))?;
    Ok((url.to_string(), digest.to_ascii_lowercase()))
}

/// Downloads the engine, checks it and installs it; returns the installed program. `progress`
/// gets the bytes received and the expected total.
pub fn install(progress: &dyn Fn(u64, Option<u64>)) -> Result<PathBuf, String> {
    let target = target().ok_or("there is no swatplus-rust program for this platform")?;
    let dir = install_dir().ok_or("can't find a folder to install swatplus-rust in")?;
    let agent = agent();
    let tag = format!("v{}", crate::runner::RELEASE);
    let text = get(&agent, &release_api_url())?.body_mut().read_to_string().map_err(|e| e.to_string())?;
    let release: Value = serde_json::from_str(&text).map_err(|e| format!("unexpected release information: {e}"))?;
    let name = asset_name(&tag, target);
    let (url, expected) = find_asset(&release, &name)?;

    let mut resp = get(&agent, &url)?;
    let total = resp.body().content_length();
    if total.is_some_and(|t| t > MAX_BYTES) {
        return Err(format!("{name} is unexpectedly large"));
    }
    let mut reader = resp.body_mut().as_reader();
    let mut bytes = Vec::with_capacity(total.unwrap_or(0).min(MAX_BYTES) as usize);
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("{name}: {e}"))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(buf.get(..n).unwrap_or_default());
        if bytes.len() as u64 > MAX_BYTES {
            return Err(format!("{name} is unexpectedly large"));
        }
        progress(bytes.len() as u64, total);
    }
    let actual: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if actual != expected {
        return Err(format!("{name}: the download doesn't match its published SHA-256"));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let staging = dir.join(format!(".download-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
    let exe = crate::runner::exe_name();
    let result = unpack(&bytes, target, &staging)
        .and_then(|_| find_file(&staging, exe, 3).ok_or_else(|| format!("{name} has no {exe}")))
        .and_then(|found| put_in_place(&found, &dir.join(exe), &tag));
    let _ = std::fs::remove_dir_all(&staging);
    result
}

/// Moves the unpacked program to `dest`, replacing an installed one, once it starts and reports
/// a supported version; otherwise an installed program is left as it was.
fn put_in_place(exe: &Path, dest: &Path, tag: &str) -> Result<PathBuf, String> {
    make_executable(exe)?;
    let version = crate::runner::version_of(exe).ok_or_else(|| format!("the downloaded swatplus-rust {tag} doesn't start"))?;
    crate::runner::check_supported(&format!("the downloaded swatplus-rust {tag}"), &version)?;
    let old = dest.with_file_name(format!("{}.old", crate::runner::exe_name()));
    let _ = std::fs::remove_file(&old);
    let had_old = dest.exists() && std::fs::rename(dest, &old).is_ok();
    if let Err(e) = std::fs::rename(exe, dest).map_err(|e| format!("{}: {e}", dest.display())).and_then(|_| make_executable(dest)) {
        if had_old {
            let _ = std::fs::rename(&old, dest);
        }
        return Err(e);
    }
    let _ = std::fs::remove_file(&old);
    Ok(dest.to_path_buf())
}

/// Whether `exe` is the program [`install`] put in place (not one found elsewhere).
pub fn is_installed(exe: &Path) -> bool {
    install_dir().is_some_and(|d| exe == d.join(crate::runner::exe_name()))
}

/// Removes what [`install`] put in place; a program found elsewhere is the user's and is left.
pub fn uninstall() -> Result<(), String> {
    let dir = install_dir().ok_or("can't find the swatplus-rust install folder")?;
    let exe = dir.join(crate::runner::exe_name());
    if !exe.is_file() {
        return Err("swatplus-rust isn't installed".into());
    }
    std::fs::remove_file(&exe).map_err(|e| format!("{}: {e}", exe.display()))?;
    let _ = std::fs::remove_file(dir.join(format!("{}.old", crate::runner::exe_name())));
    let _ = std::fs::remove_dir(&dir);
    Ok(())
}

fn unpack(bytes: &[u8], target: &str, into: &Path) -> Result<(), String> {
    if target.contains("windows") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
        // `extract` keeps entries inside the folder (it rejects paths that escape it).
        zip.extract(into).map_err(|e| e.to_string())
    } else {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
        // `unpack` keeps entries inside the folder too.
        tar.unpack(into).map_err(|e| e.to_string())
    }
}

/// A file named `name` within `dir`, at most `depth` folders down.
fn find_file(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok().map(|e| e.path())).collect();
    if let Some(f) = entries.iter().find(|p| p.is_file() && p.file_name().is_some_and(|n| n == name)) {
        return Some(f.clone());
    }
    if depth == 0 {
        return None;
    }
    entries.iter().filter(|p| p.is_dir()).find_map(|d| find_file(d, name, depth - 1))
}

#[cfg(unix)]
fn make_executable(p: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).map_err(|e| format!("{}: {e}", p.display()))
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Release information as GitHub's API gives it (v1.2.0's, cut down).
    #[test]
    fn the_archive_and_its_checksum_are_found() {
        let release = serde_json::json!({"tag_name": "v1.2.0", "assets": [
            {"name": "swatplus-rust-v1.2.0-x86_64-unknown-linux-gnu.tar.gz",
             "browser_download_url": "https://github.com/andriyanto/swatplus-rust/releases/download/v1.2.0/swatplus-rust-v1.2.0-x86_64-unknown-linux-gnu.tar.gz",
             "digest": "sha256:278E3DA89994E4A838FD99BDF6BFFCC5D0E8F61D5947859B64ACE025F608DCC4"},
            {"name": "swatplus-rust-v1.2.0-x86_64-pc-windows-msvc.zip", "browser_download_url": "https://x/y.zip"}
        ]});
        assert_eq!(asset_name("v1.2.0", "x86_64-unknown-linux-gnu"), "swatplus-rust-v1.2.0-x86_64-unknown-linux-gnu.tar.gz");
        assert_eq!(asset_name("v1.2.0", "aarch64-pc-windows-msvc"), "swatplus-rust-v1.2.0-aarch64-pc-windows-msvc.zip");
        let (url, sum) = find_asset(&release, "swatplus-rust-v1.2.0-x86_64-unknown-linux-gnu.tar.gz").unwrap();
        assert!(url.ends_with("x86_64-unknown-linux-gnu.tar.gz"));
        assert_eq!(sum, "278e3da89994e4a838fd99bdf6bffcc5d0e8f61d5947859b64ace025f608dcc4");
        assert!(find_asset(&release, "swatplus-rust-v1.2.0-x86_64-pc-windows-msvc.zip").unwrap_err().contains("no SHA-256"));
        assert!(find_asset(&release, "nothing.tar.gz").unwrap_err().contains("has no"));
    }
}
