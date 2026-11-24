//! One run of the engine program: `swatplus-rust run <input> --out <output> --progress json`,
//! its events read as they come ([`crate::Event`]), stopped by killing the process when asked.

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::Duration;

use crate::Event;

/// A fingerprint of a project's input files: the SHA-256, in hexadecimal, of each file's name, its
/// length and its bytes, in the order given (sorted by name for the same answer from the same
/// files). Two runs with the same fingerprint and the same engine version read the same inputs.
pub fn digest(files: &[(String, Vec<u8>)]) -> String {
    let mut sorted: Vec<&(String, Vec<u8>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut f = Fingerprint::default();
    for (name, bytes) in sorted {
        f.add(name, bytes);
    }
    f.finish()
}

/// [`digest`] a file at a time, so a large project's files needn't all be held at once: add them
/// in name order (a `BTreeMap`'s) for the same answer.
#[derive(Default)]
pub struct Fingerprint(sha2::Sha256);

impl Fingerprint {
    /// Adds a file.
    pub fn add(&mut self, name: &str, bytes: &[u8]) {
        use sha2::Digest;
        self.0.update(name.as_bytes());
        self.0.update([0]);
        self.0.update((bytes.len() as u64).to_le_bytes());
        self.0.update(bytes);
    }

    /// The fingerprint of the files added, in hexadecimal.
    pub fn finish(self) -> String {
        use sha2::Digest;
        self.0.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// What the tables are written as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Text,
    Parquet,
    Both,
}

impl Format {
    /// From `text`, `parquet` or `both`.
    pub fn parse(s: &str) -> Option<Format> {
        Some(match s {
            "text" => Format::Text,
            "parquet" => Format::Parquet,
            "both" => Format::Both,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Format::Text => "text",
            Format::Parquet => "parquet",
            Format::Both => "both",
        }
    }
}

/// How to run a project.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    /// The TxtInOut folder to run.
    pub input: PathBuf,
    /// Where the tables go.
    pub output: PathBuf,
    /// `--fix-swat`: SWAT+'s bugs corrected (`docs/swat-fixes.md` of swatplus-rust); results
    /// then differ from SWAT+'s.
    pub fixes: bool,
    /// `--threads`, with `fixes` (0: all the computer has); a faithful run uses one.
    pub threads: usize,
    pub format: Format,
}

impl Spec {
    /// The program's arguments for this run.
    pub fn args(&self) -> Vec<String> {
        let mut a = vec![
            "run".to_string(),
            self.input.display().to_string(),
            "--out".into(),
            self.output.display().to_string(),
            "--progress".into(),
            "json".into(),
            "--format".into(),
            self.format.as_str().into(),
        ];
        if self.fixes {
            a.push("--fix-swat".into());
            a.extend(["--threads".to_string(), self.threads.to_string()]);
        }
        a
    }
}

/// What a run did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// The engine's version, as its start event gives it.
    pub version: String,
    /// The days simulated, and the first and last date.
    pub days: u64,
    pub first: String,
    pub last: String,
    /// The tables written (file names in the output folder) and their rows for Parquet.
    pub tables: Vec<(String, Option<u64>)>,
    /// What the run said: parts not simulated, `--fix-swat`'s note.
    pub notes: Vec<String>,
    /// The seconds the engine took, when it finished.
    pub seconds: Option<f64>,
    /// Why it failed, when it did (its error event, or its last words on standard error).
    pub error: Option<String>,
    /// Whether it was stopped.
    pub stopped: bool,
    /// The program's exit code (none when killed).
    pub exit_code: Option<i32>,
}

impl Outcome {
    /// Whether the run finished and wrote its tables.
    pub fn ok(&self) -> bool {
        self.error.is_none() && !self.stopped && self.exit_code == Some(0)
    }

    fn take(&mut self, e: &Event) {
        match e {
            Event::Start { version, days, first, last } => {
                self.version.clone_from(version);
                self.days = *days;
                self.first.clone_from(first);
                self.last.clone_from(last);
            }
            Event::Progress { .. } => {}
            Event::Note(t) => self.notes.push(t.clone()),
            Event::Table { file, rows } => self.tables.push((file.clone(), *rows)),
            Event::Done { days, seconds } => {
                self.days = *days;
                self.seconds = Some(*seconds);
            }
            Event::Error(t) => self.error = Some(t.clone()),
        }
    }
}

/// The most of the program's standard error kept, for the error of a run that fails without
/// saying why on its standard output.
const STDERR_KEPT: usize = 4096;

/// Runs `exe` on `spec`, handing each event to `on_event` as it comes. `stop` is asked every
/// 50 ms; when it says yes, the process is killed and the outcome says it was stopped. The
/// output folder is made if missing.
pub fn run(exe: &Path, spec: &Spec, on_event: &mut dyn FnMut(&Event), stop: &dyn Fn() -> bool) -> Result<Outcome, String> {
    std::fs::create_dir_all(&spec.output).map_err(|e| format!("{}: {e}", spec.output.display()))?;
    let mut c = crate::runner::command(exe);
    c.args(spec.args()).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn().map_err(|e| format!("{}: {e}", exe.display()))?;
    let (Some(stdout), Some(mut stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("the engine's output couldn't be read".into());
    };
    // Lines from standard output on a thread of their own, so `stop` is asked while the engine
    // is quiet; standard error drained on another, so a full pipe never holds the engine.
    let (tx, rx) = mpsc::channel::<String>();
    let lines = std::thread::Builder::new().spawn(move || {
        for line in std::io::BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let errs = std::thread::Builder::new().spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stderr, &mut buf);
        buf
    });
    let (Ok(lines), Ok(errs)) = (lines, errs) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("the system refused a thread to read the engine's output on".into());
    };
    let mut out = Outcome::default();
    let mut take = |line: &str, out: &mut Outcome| {
        if let Some(e) = Event::parse(line) {
            out.take(&e);
            on_event(&e);
        }
    };
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => take(&line, &mut out),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if stop() {
            let _ = child.kill();
            out.stopped = true;
            break;
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let _ = lines.join();
    // Events written before the end but not read yet.
    while let Ok(line) = rx.try_recv() {
        take(&line, &mut out);
    }
    let stderr = errs.join().unwrap_or_default();
    out.exit_code = if out.stopped { None } else { status.code() };
    if !out.stopped && out.error.is_none() && out.exit_code != Some(0) {
        let text = String::from_utf8_lossy(&stderr);
        let tail: String = text.trim().chars().rev().take(STDERR_KEPT).collect::<Vec<_>>().into_iter().rev().collect();
        out.error = Some(if tail.is_empty() { format!("the engine stopped with {status}") } else { tail });
    }
    Ok(out)
}

// `cfg(test)` on its own, as clippy's allow-unwrap-in-tests looks for it.
#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    /// A stand-in for swatplus-rust: a shell script that answers as the program does.
    fn fake(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("swatplus-rust");
        std::fs::write(&p, format!("#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'swatplus-rust 1.2.0'; exit 0; fi\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    fn spec(dir: &Path) -> Spec {
        Spec { input: dir.join("in"), output: dir.join("out"), fixes: true, threads: 2, format: Format::Parquet }
    }

    fn tempdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rustgis-swat-run-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_fingerprint_follows_the_files_not_their_order() {
        let a = vec![("a.txt".to_string(), b"1".to_vec()), ("b.txt".to_string(), b"2".to_vec())];
        let b = vec![a[1].clone(), a[0].clone()];
        assert_eq!(digest(&a), digest(&b));
        assert_eq!(digest(&a).len(), 64);
        // Moving a byte from one file to the next changes it.
        let c = vec![("a.txt".to_string(), b"12".to_vec()), ("b.txt".to_string(), Vec::new())];
        assert_ne!(digest(&a), digest(&c));
        // A file at a time, in name order, gives the same answer.
        let mut f = Fingerprint::default();
        f.add("a.txt", b"1");
        f.add("b.txt", b"2");
        assert_eq!(f.finish(), digest(&a));
    }

    #[test]
    fn a_run_s_events_are_read_as_they_come() {
        let d = tempdir("events");
        // The arguments it was given, then the events of a short run.
        let exe = fake(
            &d,
            r#"echo "$@" > "$4/args.txt"
echo '{"event":"start","version":"1.2.0","days":2,"first":"2015-01-01","last":"2015-01-02"}'
echo '{"event":"progress","done":0,"days":2,"date":"2015-01-01"}'
echo 'not an event'
echo '{"event":"note","text":"--fix-swat: corrected"}'
echo '{"event":"table","file":"channel_sd_day.parquet","rows":4}'
echo '{"event":"done","days":2,"seconds":0.5}'"#,
        );
        assert_eq!(crate::runner::version_of(&exe).as_deref(), Some("swatplus-rust 1.2.0"));
        let mut seen = Vec::new();
        let out = run(&exe, &spec(&d), &mut |e| seen.push(e.clone()), &|| false).unwrap();
        assert!(out.ok(), "{out:?}");
        assert_eq!((out.version.as_str(), out.days, out.first.as_str(), out.last.as_str()), ("1.2.0", 2, "2015-01-01", "2015-01-02"));
        assert_eq!(out.tables, [("channel_sd_day.parquet".to_string(), Some(4))]);
        assert_eq!(out.notes, ["--fix-swat: corrected"]);
        assert_eq!(out.seconds, Some(0.5));
        assert_eq!(seen.len(), 5);
        let args = std::fs::read_to_string(d.join("out/args.txt")).unwrap();
        assert_eq!(
            args.trim(),
            format!(
                "run {} --out {} --progress json --format parquet --fix-swat --threads 2",
                d.join("in").display(),
                d.join("out").display()
            )
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_failed_run_says_why() {
        let d = tempdir("fails");
        let exe = fake(&d, r#"echo '{"event":"error","text":"time.sim: missing"}'; exit 1"#);
        let out = run(&exe, &spec(&d), &mut |_| {}, &|| false).unwrap();
        assert!(!out.ok() && out.error.as_deref() == Some("time.sim: missing") && out.exit_code == Some(1), "{out:?}");
        // Silent on its standard output: its standard error is the reason.
        let exe = fake(&d, "echo 'thread main panicked' >&2; exit 101");
        let out = run(&exe, &spec(&d), &mut |_| {}, &|| false).unwrap();
        assert_eq!(out.error.as_deref(), Some("thread main panicked"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_run_is_stopped_when_asked() {
        let d = tempdir("stop");
        let exe = fake(&d, r#"echo '{"event":"start","version":"1.2.0","days":9,"first":"a","last":"b"}'; exec sleep 30"#);
        let started = std::time::Instant::now();
        let asked = std::cell::Cell::new(0);
        let out = run(&exe, &spec(&d), &mut |_| {}, &|| {
            asked.set(asked.get() + 1);
            asked.get() > 4
        })
        .unwrap();
        assert!(out.stopped && !out.ok() && out.exit_code.is_none(), "{out:?}");
        assert!(started.elapsed() < Duration::from_secs(10), "killed, not waited for");
        let _ = std::fs::remove_dir_all(&d);
    }
}
