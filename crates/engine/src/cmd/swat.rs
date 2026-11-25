//! A SWAT+ project (its TxtInOut folder) opened beside the map, for people and the AI assistant to
//! look at and edit through commands (A1 of the swatplus-rust roadmap): its summary, what is
//! missing, its parameters found by name and read, changed one at a time or many by a rule
//! (replace, add, or multiply, as SWAT-CUP's `v__`, `a__` and `r__`), checked against
//! `cal_parms.cal`'s ranges, what changed and undoing it, and saving the changed files.
//!
//! The project is held as its files' text (`rustgis_swat_model`), so saving writes back the same
//! bytes but for the values changed. It isn't part of the map project and isn't saved with it.
//!
//! It is run by the swatplus-rust engine (`swat.run`), a separate program that is never linked
//! (the engine is LGPL-2.1, RustGIS-SwatPlus Apache-2.0; decision K11): the project as edited is
//! written to a run folder, unless nothing changed, the program runs it, and each run is recorded
//! with the engine's version, the changes and a fingerprint of the inputs (`swat.runs`). In the
//! desktop app the run goes on a thread of its own (`reads::IN_BACKGROUND`), with the engine's
//! days as its progress and Cancel stopping the engine.
//!
//! A project opened as confidential (data that may not leave the computer) answers with counts
//! and statistics only, never a record's name or value; only the user can lift that
//! (`swat.confidential` is reserved to them).

use std::collections::{BTreeMap, BTreeSet};

use rustgis_swat_model::{Project, TextFile};
use rustgis_swat_txtinout::view::{Kind, View, columns};
use serde_json::{Value, json};

use super::{b, f, ps};
use crate::{CmdResult, CommandSpec as C, Session};

const WHERE: &str = "Analyze › SWAT+";

/// The most files, and bytes, a project may have: a large basin's weather files are big.
#[cfg(not(target_arch = "wasm32"))]
const MAX_FILES: usize = 20_000;
#[cfg(not(target_arch = "wasm32"))]
const MAX_BYTES: u64 = 4 << 30;
/// The most records a reply lists (the statistics cover them all).
pub(super) const MAX_LISTED: usize = 500;

/// The open SWAT+ project.
#[derive(Clone, Debug)]
pub struct SwatState {
    /// The folder it was read from.
    pub dir: Option<std::path::PathBuf>,
    /// As read, and as edited.
    pub original: Project,
    pub project: Project,
    /// Answers carry statistics only.
    pub confidential: bool,
    /// The runs made since it was opened (`swat.runs`), each as `swat.run` answered.
    pub runs: Vec<Value>,
}

pub fn commands() -> Vec<C> {
    vec![
        C::new("swat.open", "Open SWAT+ Project", WHERE, open).roles(&[]).effects(crate::policy::Effects::NONE.with(crate::policy::Effect::Files)).view().params(
            r#"{"path": "a TxtInOut folder (with file.cio)", "confidential"?: bool (answers carry counts and statistics only, no record names or values; only the user can lift it)} — reads the project's input files"#,
        ),
        C::new("swat.close", "Close SWAT+ Project", WHERE, close).roles(&[]).effects(crate::policy::Effects::NONE).view().when(need_swat).params("{} — closes the SWAT+ project (unsaved changes are lost)"),
        C::new("swat.summary", "SWAT+ Project Summary", WHERE, summary).roles(&[]).effects(crate::policy::Effects::NONE)
            .view()
            .when(need_swat)
            .params("{} — the period simulated, the number of each kind of object, the files, and whether any are missing"),
        C::new("swat.check", "Check SWAT+ Project", WHERE, check).roles(&[]).effects(crate::policy::Effects::NONE).view().when(need_swat).params(
            r#"{} — input files file.cio names that are missing, and parameter values outside cal_parms.cal's calibration ranges (not necessarily wrong)"#,
        ),
        C::new("swat.find", "Find SWAT+ Parameter", WHERE, find).roles(&[]).effects(crate::policy::Effects::NONE).view().when(need_swat).params(
            r#"{"query": text (part of a column name, e.g. "cn", "esco", "awc"), "file"?: text (one file, e.g. "hydrology.hyd")} — the files and columns that match, with their records' count and range, and cal_parms.cal's range"#,
        ),
        C::new("swat.get", "Read SWAT+ Parameter", WHERE, get).roles(&[]).effects(crate::policy::Effects::NONE).view().when(need_swat).params(
            r#"{"file": text, "column": text, "match"?: "record names, * and ? as wildcards", "where"?: {"column": text, "match": "values, * and ? as wildcards"}} — the values of a column (up to 500 records listed) and their statistics"#,
        ),
        C::new("swat.set", "Change SWAT+ Parameter", WHERE, set).roles(&[]).effects(crate::policy::Effects::NONE).view().when(need_swat).params(
            r#"{"file": text, "column": text, "value": number or text, "mode"?: "replace" (default) | "add" | "multiply" (by 1 + value, as SWAT-CUP's r__), "match"?: record names with * and ?, "where"?: {"column": text, "match": text}, "force"?: bool (allow values outside cal_parms.cal's range)} — changes the matching records' values (in memory: swat.save writes them)"#,
        ),
        C::new("swat.diff", "SWAT+ Changes", WHERE, diff).roles(&[]).effects(crate::policy::Effects::NONE)
            .view()
            .when(need_swat)
            .params(r#"{"file"?: text} — every value changed since the project was opened (or counts only, when confidential)"#),
        C::new("swat.revert", "Revert SWAT+ Changes", WHERE, revert).roles(&[]).effects(crate::policy::Effects::NONE)
            .view()
            .when(need_swat)
            .params(r#"{"file"?: text, "column"?: text} — undoes the changes (all, a file's, or a file's column)"#),
        C::new("swat.save", "Save SWAT+ Project", WHERE, save).roles(&[]).effects(crate::policy::Effects::NONE.with(crate::policy::Effect::Files)).view().when(need_swat).params(
            r#"{"path"?: "a folder to save the whole project to (default: write the changed files over the folder it was opened from)"} — writes the files"#,
        ),
        C::new("swat.confidential", "SWAT+ Confidential Mode", WHERE, confidential).roles(&[]).effects(crate::policy::Effects::NONE.with(crate::policy::Effect::Reserved))
            .view()
            .when(need_swat)
            .params(r#"{"on": bool} — answers with statistics only (on), or with record names and values (off; only the user can turn it off)"#),
        C::new("swat.engine", "SWAT+ Engine", WHERE, engine).roles(&[]).effects(crate::policy::Effects::NONE).view().params(
            r#"{} — the swatplus-rust program that runs projects: where it was found and its version, or how to get it"#,
        ),
        C::new("swat.setEngine", "Locate SWAT+ Engine", WHERE, set_engine)
            .roles(&[])
            .effects(crate::policy::Effects::NONE.with(crate::policy::Effect::Reserved).with(crate::policy::Effect::Files))
            .view()
            .params(r#"{"path"?: "the swatplus-rust program (empty: look in the usual places)"}"#),
        C::new("swat.installEngine", "Install SWAT+ Engine", WHERE, install_engine)
            .roles(&[])
            .effects(crate::policy::Effects::NONE.with(crate::policy::Effect::Reserved))
            .view()
            .params(r#"{} — downloads the swatplus-rust release RustGIS-SwatPlus was tested with, checks its SHA-256 and installs it"#),
        C::new("swat.run", "Run SWAT+ Project", WHERE, run).roles(&[]).effects(crate::policy::Effects::NONE.with(crate::policy::Effect::Files)).view().when(need_swat).params(
            r#"{"fixSwat"?: bool (correct SWAT+'s bugs: results then differ from SWAT+'s; default false), "threads"?: number (with fixSwat; default all), "format"?: "parquet" (default) | "text" | "both", "folder"?: "where the runs go (default: the app's data folder)"} — runs the project as edited with the swatplus-rust engine and records the run"#,
        ),
        C::new("swat.runs", "SWAT+ Runs", WHERE, runs)
            .roles(&[])
            .effects(crate::policy::Effects::NONE)
            .view()
            .when(need_swat)
            .params("{} — the runs made since the project was opened: engine version, mode, changes, inputs' fingerprint, tables"),
    ]
}

pub(super) fn need_swat(s: &Session) -> Option<&'static str> {
    if s.swat.is_none() { Some("open a SWAT+ project first (swat.open)") } else { None }
}

pub(super) fn state(s: &Session) -> Result<&SwatState, String> {
    s.swat.as_deref().ok_or_else(|| "no SWAT+ project is open (swat.open)".to_string())
}

fn state_mut(s: &mut Session) -> Result<&mut SwatState, String> {
    s.swat.as_deref_mut().ok_or_else(|| "no SWAT+ project is open (swat.open)".to_string())
}

// ---------------------------------------------------------------------------------------------
// Opening

#[cfg(not(target_arch = "wasm32"))]
fn open(s: &mut Session, p: &Value) -> CmdResult {
    let dir = std::path::PathBuf::from(ps(p, "path").map(str::trim).filter(|d| !d.is_empty()).ok_or("give the TxtInOut folder's path")?);
    let project = read_folder(&dir)?;
    let st = SwatState {
        dir: Some(dir),
        original: project.clone(),
        project,
        confidential: b(p, "confidential").unwrap_or(false),
        runs: Vec::new(),
    };
    s.swat = Some(Box::new(st));
    summary(s, &json!({}))
}

#[cfg(target_arch = "wasm32")]
fn open(_: &mut Session, _: &Value) -> CmdResult {
    Err("SWAT+ projects open from a folder, in the desktop app".into())
}

/// Reads the files of the scenario in `dir`: `file.cio`, what it names, and what those name in
/// turn (weather files, recall files), until nothing new is named.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_folder(dir: &std::path::Path) -> Result<Project, String> {
    let read = |name: &str| -> Result<Option<Vec<u8>>, String> {
        let path = dir.join(name);
        match std::fs::metadata(&path) {
            Ok(m) if m.is_file() => std::fs::read(&path).map(Some).map_err(|e| format!("{name}: {e}")),
            _ => Ok(None),
        }
    };
    let cio = read("file.cio")?.ok_or_else(|| format!("{}: no file.cio here (a SWAT+ TxtInOut folder has one)", dir.display()))?;
    let mut project = rustgis_swat_txtinout::read([("file.cio", cio)]);
    let mut tried: BTreeSet<String> = BTreeSet::from(["file.cio".to_string()]);
    let mut bytes = 0u64;
    loop {
        let wanted: Vec<String> = rustgis_swat_txtinout::cio::inputs(&project).into_iter().filter(|n| !tried.contains(n)).collect();
        if wanted.is_empty() {
            break;
        }
        for name in wanted {
            tried.insert(name.clone());
            // A name stays inside the folder.
            if name.contains("..") || std::path::Path::new(&name).is_absolute() {
                continue;
            }
            if let Some(b) = read(&name)? {
                bytes += b.len() as u64;
                if bytes > MAX_BYTES || project.files.len() >= MAX_FILES {
                    return Err(format!(
                        "{}: the project is larger than this reads ({} files, {} MB)",
                        dir.display(),
                        project.files.len(),
                        bytes >> 20
                    ));
                }
                project.files.insert(name, rustgis_swat_txtinout::decode(&b));
            }
        }
    }
    Ok(project)
}

fn close(s: &mut Session, _: &Value) -> CmdResult {
    let st = s.swat.take().ok_or("no SWAT+ project is open")?;
    let changed = changed_files(&st).len();
    Ok(json!({"closed": true, "unsaved_files": changed}))
}

// ---------------------------------------------------------------------------------------------
// Reading

/// A table's records: (record line, values by column name).
struct Table<'a> {
    file: &'a TextFile,
    /// Each record's line and its columns' names.
    rows: Vec<(usize, Vec<String>)>,
}

fn table<'a>(p: &'a Project, name: &str) -> Option<Table<'a>> {
    let file = p.file(name)?;
    let view = View::of(name, file);
    let rows =
        view.flat().into_iter().filter(|r| r.kind == Kind::Record && r.header.is_some()).map(|r| (r.line, columns(file, r))).collect();
    Some(Table { file, rows })
}

impl Table<'_> {
    /// The value of `column` on a record's line, and its token's place.
    fn value(&self, line: usize, cols: &[String], column: &str) -> Option<(usize, &str)> {
        let k = cols.iter().position(|c| c.eq_ignore_ascii_case(column))?;
        Some((k, self.file.token(line, k)?))
    }

    fn name(&self, line: usize) -> &str {
        self.file.token(line, 0).unwrap_or("")
    }
}

/// `*` and `?` wildcards, without case.
pub(super) fn glob(pattern: &str, text: &str) -> bool {
    fn go(p: &[char], t: &[char]) -> bool {
        match (p.first(), t.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], t) || (!t.is_empty() && go(p, &t[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &t[1..]),
            (Some(a), Some(b)) if a.eq_ignore_ascii_case(b) => go(&p[1..], &t[1..]),
            _ => false,
        }
    }
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    go(&p, &t)
}

/// The records a command picks: by `match` on their names and `where` on another column.
fn picked(t: &Table, p: &Value) -> Result<Vec<usize>, String> {
    let name_pat = ps(p, "match");
    let cond = match p.get("where") {
        Some(w) if !w.is_null() => {
            let col = w.get("column").and_then(Value::as_str).ok_or("where needs a column")?;
            let pat = w.get("match").and_then(|v| v.as_str().map(str::to_string).or_else(|| (!v.is_null()).then(|| v.to_string())));
            Some((col.to_string(), pat.ok_or("where needs a match")?))
        }
        _ => None,
    };
    let mut out = Vec::new();
    for (i, (line, cols)) in t.rows.iter().enumerate() {
        if name_pat.is_some_and(|n| !glob(n, t.name(*line))) {
            continue;
        }
        if let Some((col, pat)) = &cond {
            match t.value(*line, cols, col) {
                Some((_, v)) if glob(pat, v) => {}
                _ => continue,
            }
        }
        out.push(i);
    }
    Ok(out)
}

/// Count, and for numbers their range, mean and how many differ.
fn stats<'a>(values: impl Iterator<Item = &'a str>) -> Value {
    let mut n = 0usize;
    let mut nums: Vec<f64> = Vec::new();
    let mut distinct = BTreeSet::new();
    for v in values {
        n += 1;
        if distinct.len() < 10_000 {
            distinct.insert(v.to_string());
        }
        if let Ok(x) = v.parse::<f64>()
            && x.is_finite()
        {
            nums.push(x);
        }
    }
    let mut out = json!({"count": n, "distinct": distinct.len()});
    if !nums.is_empty() {
        let min = nums.iter().copied().fold(f64::INFINITY, f64::min);
        let max = nums.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mean = nums.iter().sum::<f64>() / nums.len() as f64;
        out["min"] = json!(min);
        out["max"] = json!(max);
        out["mean"] = json!(mean);
    }
    out
}

/// `cal_parms.cal`'s range of a parameter, by name.
fn range(p: &Project, column: &str) -> Option<(f64, f64)> {
    let t = table(p, "cal_parms.cal")?;
    t.rows.iter().find(|(l, _)| t.name(*l).eq_ignore_ascii_case(column)).and_then(|(l, cols)| {
        let lo = t.value(*l, cols, "abs_min")?.1.parse().ok()?;
        let hi = t.value(*l, cols, "abs_max")?.1.parse().ok()?;
        Some((lo, hi))
    })
}

fn summary(s: &mut Session, _: &Value) -> CmdResult {
    let st = state(s)?;
    let p = &st.project;
    let time = p.file("time.sim").map(|t| t.values(2).into_iter().map(str::to_string).collect::<Vec<_>>()).unwrap_or_default();
    let period = match time.as_slice() {
        [day_start, yrc_start, day_end, yrc_end, step, ..] => {
            json!({"first_day": day_start, "first_year": yrc_start, "last_day": day_end, "last_year": yrc_end, "steps_a_day": step})
        }
        _ => Value::Null,
    };
    // object.cnt: the number of each kind of object.
    let objects = table(p, "object.cnt")
        .and_then(|t| {
            let (line, cols) = t.rows.first()?;
            let mut o = serde_json::Map::new();
            for (k, c) in cols.iter().enumerate().skip(1) {
                if let Some(v) = t.file.token(*line, k).and_then(|v| v.parse::<f64>().ok()).filter(|v| *v > 0.0) {
                    o.insert(c.clone(), json!(v));
                }
            }
            Some(Value::Object(o))
        })
        .unwrap_or(Value::Null);
    let missing = rustgis_swat_txtinout::cio::missing(p);
    Ok(json!({
        "folder": st.dir.as_ref().map(|d| d.display().to_string()),
        "confidential": st.confidential,
        "period": period,
        "objects": objects,
        "files": p.files.len(),
        "missing_files": missing,
        "changed_files": changed_files(st),
    }))
}

fn check(s: &mut Session, _: &Value) -> CmdResult {
    let st = state(s)?;
    let p = &st.project;
    let missing = rustgis_swat_txtinout::cio::missing(p);
    // Every column with a range in cal_parms.cal, in every table.
    let ranges: BTreeMap<String, (f64, f64)> = match table(p, "cal_parms.cal") {
        Some(t) => t
            .rows
            .iter()
            .filter_map(|(l, cols)| {
                let lo = t.value(*l, cols, "abs_min")?.1.parse().ok()?;
                let hi = t.value(*l, cols, "abs_max")?.1.parse().ok()?;
                Some((t.name(*l).to_ascii_lowercase(), (lo, hi)))
            })
            .collect(),
        None => BTreeMap::new(),
    };
    let mut out_of_range = Vec::new();
    for name in p.files.keys() {
        if name == "cal_parms.cal" || name.contains('/') {
            continue;
        }
        let Some(t) = table(p, name) else { continue };
        let mut found: BTreeMap<String, (usize, f64, f64, (f64, f64))> = BTreeMap::new();
        for (line, cols) in &t.rows {
            for (k, c) in cols.iter().enumerate() {
                let Some(&(lo, hi)) = ranges.get(&c.to_ascii_lowercase()) else { continue };
                let Some(v) = t.file.token(*line, k).and_then(|v| v.parse::<f64>().ok()) else { continue };
                if v < lo || v > hi {
                    let e = found.entry(c.clone()).or_insert((0, f64::INFINITY, f64::NEG_INFINITY, (lo, hi)));
                    e.0 += 1;
                    e.1 = e.1.min(v);
                    e.2 = e.2.max(v);
                }
            }
        }
        for (c, (n, min, max, (lo, hi))) in found {
            out_of_range.push(json!({"file": name, "column": c, "records": n, "min": min, "max": max, "range": [lo, hi]}));
        }
    }
    // cal_parms.cal's ranges are calibration limits: a project's own values (the SWAT+ Editor's
    // defaults among them) can be outside them on purpose, so they're listed, not counted as faults.
    Ok(json!({
        "missing_files": missing,
        "outside_calibration_range": out_of_range,
        "note": "cal_parms.cal gives the ranges calibration may change a parameter within; values outside them are not necessarily wrong",
        "ok": missing.is_empty(),
    }))
}

fn find(s: &mut Session, p: &Value) -> CmdResult {
    let st = state(s)?;
    let q = ps(p, "query").map(str::trim).filter(|q| !q.is_empty()).ok_or("give a query (part of a column name)")?.to_ascii_lowercase();
    let only = ps(p, "file");
    let mut hits = Vec::new();
    for name in st.project.files.keys() {
        if only.is_some_and(|o| o != name) || name.contains('/') {
            continue;
        }
        let Some(t) = table(&st.project, name) else { continue };
        let mut cols: Vec<String> = Vec::new();
        for (_, c) in &t.rows {
            for col in c {
                if col.to_ascii_lowercase().contains(&q) && !cols.contains(col) {
                    cols.push(col.clone());
                }
            }
        }
        for col in cols {
            let values = t.rows.iter().filter_map(|(l, c)| t.value(*l, c, &col).map(|v| v.1));
            let mut hit = json!({"file": name, "column": col, "values": stats(values)});
            if let Some((lo, hi)) = range(&st.project, &col) {
                hit["range"] = json!([lo, hi]);
            }
            hits.push(hit);
            if hits.len() >= 200 {
                break;
            }
        }
    }
    Ok(json!({"query": q, "found": hits}))
}

fn get(s: &mut Session, p: &Value) -> CmdResult {
    let st = state(s)?;
    let (file, column) = file_column(p)?;
    let t = table(&st.project, file).ok_or_else(|| format!("no table {file} in the project"))?;
    let rows = picked(&t, p)?;
    let mut values = Vec::new();
    let mut listed = Vec::new();
    for &i in &rows {
        let (line, cols) = &t.rows[i];
        if let Some((_, v)) = t.value(*line, cols, column) {
            values.push(v);
            if listed.len() < MAX_LISTED {
                listed.push(json!({"name": t.name(*line), "value": v}));
            }
        }
    }
    if values.is_empty() && !rows.is_empty() {
        return Err(format!("{file} has no column {column}"));
    }
    let mut out = json!({"file": file, "column": column, "values": stats(values.iter().copied())});
    if let Some((lo, hi)) = range(&st.project, column) {
        out["range"] = json!([lo, hi]);
    }
    if st.confidential {
        out["records"] = json!("hidden: the project is confidential (statistics only)");
    } else {
        out["records"] = Value::Array(listed);
        out["more"] = json!(values.len().saturating_sub(MAX_LISTED));
    }
    Ok(out)
}

fn file_column(p: &Value) -> Result<(&str, &str), String> {
    let file = ps(p, "file").map(str::trim).filter(|f| !f.is_empty()).ok_or("give the file (e.g. hydrology.hyd)")?;
    let column = ps(p, "column").map(str::trim).filter(|c| !c.is_empty()).ok_or("give the column")?;
    Ok((file, column))
}

// ---------------------------------------------------------------------------------------------
// Changing

/// `x` written as the value it replaces is: with as many decimals, or in its exponent form.
fn format_like(old: &str, x: f64) -> String {
    if old.contains(['e', 'E']) {
        return format!("{x:e}");
    }
    match old.split_once('.') {
        Some((_, frac)) => format!("{x:.*}", frac.len()),
        None if old.parse::<i64>().is_ok() && x.fract() == 0.0 => format!("{}", x as i64),
        None => format!("{x}"),
    }
}

fn set(s: &mut Session, p: &Value) -> CmdResult {
    let st = state_mut(s)?;
    let (file, column) = file_column(p)?;
    let mode = ps(p, "mode").unwrap_or("replace");
    if !matches!(mode, "replace" | "add" | "multiply") {
        return Err(format!("mode is replace, add or multiply, not {mode}"));
    }
    let number = f(p, "value");
    let text = match p.get("value") {
        Some(Value::String(v)) => v.trim().to_string(),
        Some(v) if !v.is_null() => v.to_string(),
        _ => return Err("give the value".into()),
    };
    if mode != "replace" && number.is_none() {
        return Err(format!("{mode} needs a number"));
    }
    let range = range(&st.project, column);
    let force = b(p, "force").unwrap_or(false);
    // The changes, worked out before any is made.
    let mut edits: Vec<(usize, usize, String, String)> = Vec::new();
    {
        let t = table(&st.project, file).ok_or_else(|| format!("no table {file} in the project"))?;
        for i in picked(&t, p)? {
            let (line, cols) = &t.rows[i];
            let Some((k, old)) = t.value(*line, cols, column) else { continue };
            let new = match (mode, number, old.parse::<f64>()) {
                ("replace", Some(x), Ok(_)) => format_like(old, x),
                ("replace", _, _) => text.clone(),
                ("add", Some(x), Ok(o)) => format_like(old, o + x),
                ("multiply", Some(x), Ok(o)) => format_like(old, o * (1.0 + x)),
                _ => return Err(format!("{file} {column} of {} is {old}, not a number", t.name(*line))),
            };
            if let (Some((lo, hi)), Ok(v)) = (range, new.parse::<f64>())
                && (v < lo || v > hi)
                && !force
            {
                return Err(format!(
                    "{column} of {} would be {new}, outside cal_parms.cal's range {lo} to {hi} (give \"force\": true to set it anyway)",
                    t.name(*line)
                ));
            }
            edits.push((*line, k, old.to_string(), new));
        }
        if edits.is_empty() {
            return Err(format!("no record of {file} matches, or it has no column {column}"));
        }
    }
    let changed = edits.iter().filter(|(_, _, old, new)| old != new).count();
    let mut values = Vec::new();
    for (line, k, old, new) in &edits {
        st.project.set_token(file, *line, *k, new).map_err(|e| e.to_string())?;
        if values.len() < MAX_LISTED {
            values.push(json!({"line": line + 1, "from": old, "to": new}));
        }
    }
    let mut out = json!({"file": file, "column": column, "mode": mode, "records": edits.len(), "changed": changed});
    let after = table(&st.project, file)
        .map(|t| stats(t.rows.iter().filter_map(|(l, c)| t.value(*l, c, column).map(|v| v.1))))
        .unwrap_or(Value::Null);
    out["values_now"] = after;
    if !st.confidential {
        out["edits"] = Value::Array(values);
    }
    Ok(out)
}

/// The files whose text differs from what was read.
fn changed_files(st: &SwatState) -> Vec<String> {
    st.project.files.iter().filter(|(n, f)| st.original.file(n) != Some(*f)).map(|(n, _)| n.clone()).collect()
}

fn diff(s: &mut Session, p: &Value) -> CmdResult {
    let st = state(s)?;
    let only = ps(p, "file");
    let mut files = Vec::new();
    for name in changed_files(st) {
        if only.is_some_and(|o| o != name) {
            continue;
        }
        let (Some(new), Some(old)) = (st.project.file(&name), st.original.file(&name)) else { continue };
        let view = View::of(&name, new);
        let mut by_column: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for r in view.flat() {
            let cols = columns(new, r);
            let (a, b) = (old.values(r.line), new.values(r.line));
            for (k, (x, y)) in a.iter().zip(&b).enumerate() {
                if x != y {
                    let col = cols.get(k).cloned().unwrap_or_else(|| format!("col{}", k + 1));
                    by_column.entry(col).or_default().push(json!({"name": new.token(r.line, 0), "from": x, "to": y}));
                }
            }
        }
        let columns: Vec<Value> = by_column
            .into_iter()
            .map(|(c, v)| {
                if st.confidential {
                    json!({"column": c, "changed": v.len()})
                } else {
                    json!({"column": c, "changed": v.len(), "values": v.into_iter().take(MAX_LISTED).collect::<Vec<_>>()})
                }
            })
            .collect();
        files.push(json!({"file": name, "columns": columns}));
    }
    Ok(json!({"files": files}))
}

fn revert(s: &mut Session, p: &Value) -> CmdResult {
    let st = state_mut(s)?;
    let only = ps(p, "file");
    let column = ps(p, "column");
    let mut reverted = 0usize;
    for name in changed_files(st) {
        if only.is_some_and(|o| o != name) {
            continue;
        }
        let Some(old) = st.original.file(&name).cloned() else { continue };
        match column {
            None => {
                st.project.files.insert(name, old);
                reverted += 1;
            }
            Some(col) => {
                let Some(new) = st.project.file(&name).cloned() else { continue };
                let view = View::of(&name, &new);
                for r in view.flat() {
                    let cols = columns(&new, r);
                    let Some(k) = cols.iter().position(|c| c.eq_ignore_ascii_case(col)) else { continue };
                    if let Some(v) = old.token(r.line, k).filter(|v| Some(*v) != new.token(r.line, k)) {
                        st.project.set_token(&name, r.line, k, v).map_err(|e| e.to_string())?;
                        reverted += 1;
                    }
                }
            }
        }
    }
    Ok(json!({"reverted": reverted, "changed_files": changed_files(st)}))
}

#[cfg(not(target_arch = "wasm32"))]
fn save(s: &mut Session, p: &Value) -> CmdResult {
    let st = state_mut(s)?;
    let to = ps(p, "path").map(str::trim).filter(|d| !d.is_empty()).map(std::path::PathBuf::from);
    let (dir, names): (std::path::PathBuf, Vec<String>) = match to {
        // A folder of its own: the whole project.
        Some(d) => (d, st.project.files.keys().cloned().collect()),
        None => (st.dir.clone().ok_or("the project has no folder: give a path")?, changed_files(st)),
    };
    let mut written = 0usize;
    for name in &names {
        let Some(file) = st.project.file(name) else { continue };
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, rustgis_swat_txtinout::encode(file)).map_err(|e| format!("{}: {e}", path.display()))?;
        written += 1;
    }
    // What is saved over the folder it came from is the project as read from now on.
    if p.get("path").is_none_or(Value::is_null) {
        st.original = st.project.clone();
    }
    Ok(json!({"folder": dir.display().to_string(), "written": written}))
}

#[cfg(target_arch = "wasm32")]
fn save(_: &mut Session, _: &Value) -> CmdResult {
    Err("SWAT+ projects are saved to a folder, in the desktop app".into())
}

fn confidential(s: &mut Session, p: &Value) -> CmdResult {
    let st = state_mut(s)?;
    let on = b(p, "on").ok_or("give on: true or false")?;
    st.confidential = on;
    Ok(json!({"confidential": on}))
}

// ---------------------------------------------------------------------------------------------
// Running

#[cfg(not(target_arch = "wasm32"))]
fn engine(_: &mut Session, _: &Value) -> CmdResult {
    use rustgis_swat_run::runner;
    Ok(match runner::find() {
        Some((path, version)) => json!({
            "found": true, "path": path.display().to_string(), "version": version,
            "installed": rustgis_swat_run::install::is_installed(&path), "release": runner::RELEASE,
        }),
        None => json!({
            "found": false, "release": runner::RELEASE, "download": runner::download_url(), "env": runner::ENV,
            "hint": "install it (swat.installEngine), point to it (swat.setEngine), or put swatplus-rust on the PATH",
        }),
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn set_engine(s: &mut Session, p: &Value) -> CmdResult {
    let path = ps(p, "path").map(str::trim).filter(|p| !p.is_empty()).map(std::path::PathBuf::from);
    rustgis_swat_run::runner::choose(path)?;
    engine(s, &Value::Null)
}

#[cfg(not(target_arch = "wasm32"))]
fn install_engine(s: &mut Session, _: &Value) -> CmdResult {
    let path = rustgis_swat_run::install::install(&|_, _| {})?;
    rustgis_swat_run::runner::choose(Some(path))?;
    engine(s, &Value::Null)
}

/// Where runs go when `swat.run` isn't given a folder: `swat-runs/<project folder's name>` in the
/// app's data folder.
#[cfg(not(target_arch = "wasm32"))]
fn runs_root(st: &SwatState) -> Result<std::path::PathBuf, String> {
    let name = st.dir.as_ref().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "project".into());
    let base = rustgis_data::dirs::app_data_dir().ok_or("can't find the app's data folder: give a folder")?;
    Ok(base.join("swat-runs").join(name))
}

/// The next free `run-NNNN` folder in `root` (made), and its number.
#[cfg(not(target_arch = "wasm32"))]
fn next_run(root: &std::path::Path) -> Result<(u32, std::path::PathBuf), String> {
    std::fs::create_dir_all(root).map_err(|e| format!("{}: {e}", root.display()))?;
    for n in 1..=9999u32 {
        let d = root.join(format!("run-{n:04}"));
        // `create_dir` fails when it exists, so two runs never share a folder.
        match std::fs::create_dir(&d) {
            Ok(()) => return Ok((n, d)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("{}: {e}", d.display())),
        }
    }
    Err(format!("{}: 9,999 runs already; give another folder", root.display()))
}

/// What [`run_ahead`]'s work is placed under for `swat.run` to take as it lands.
#[cfg(not(target_arch = "wasm32"))]
const RUN_KEY: &str = "swat.run";

/// A run as asked for, taken from the session on the window's thread: what changed and its new
/// bytes, and where the rest of the files are. [`Prepared::go`] does the rest anywhere.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct Prepared {
    /// The project's folder: the engine reads it when nothing changed, and the unchanged files
    /// are copied from it when something did (they are the bytes read, as `swat.save` writes).
    dir: Option<std::path::PathBuf>,
    /// Every file of the project, in name order.
    names: Vec<String>,
    /// The changed files' bytes (all files' for a project with no folder).
    edited: BTreeMap<String, Vec<u8>>,
    changed: Vec<String>,
    changes: Value,
    root: std::path::PathBuf,
    fixes: bool,
    threads: usize,
    format: rustgis_swat_run::run::Format,
}

/// Checks `swat.run`'s parameters and takes what the run needs of the session. Cheap: only the
/// changed files are written out here.
#[cfg(not(target_arch = "wasm32"))]
fn prepare_run(s: &mut Session, p: &Value) -> Result<Prepared, String> {
    use rustgis_swat_run::run::Format;
    let format = match ps(p, "format") {
        None => Format::Parquet,
        Some(f) => Format::parse(f).ok_or_else(|| format!("format is parquet, text or both, not {f}"))?,
    };
    let fixes = b(p, "fixSwat").unwrap_or(false);
    let threads = match f(p, "threads") {
        None => 0,
        Some(t) if t >= 0.0 && t.fract() == 0.0 && t <= 1024.0 => t as usize,
        Some(t) => return Err(format!("threads is a whole number from 0 (all) to 1024, not {t}")),
    };
    let changes = diff(s, &json!({}))?;
    let st = state(s)?;
    let root = match ps(p, "folder").map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => std::path::PathBuf::from(d),
        None => runs_root(st)?,
    };
    let changed = changed_files(st);
    let names: Vec<String> = st.project.files.keys().cloned().collect();
    let edited = st
        .project
        .files
        .iter()
        .filter(|(name, _)| st.dir.is_none() || changed.contains(name))
        .map(|(name, file)| (name.clone(), rustgis_swat_txtinout::encode(file)))
        .collect();
    Ok(Prepared { dir: st.dir.clone(), names, edited, changed, changes: changes["files"].clone(), root, fixes, threads, format })
}

#[cfg(not(target_arch = "wasm32"))]
impl Prepared {
    /// Writes the project as edited to a run folder of its own (unless nothing changed), runs the
    /// engine on it and writes the run's record (`run.json`) there, also for a run that failed or
    /// was stopped; gives the record. The run's progress is the engine's days, counted on the
    /// thread's control ([`rustgis_analysis::control`]), whose Cancel stops the engine; an error
    /// here is one before the engine ran.
    pub(crate) fn go(self) -> Result<Value, String> {
        use rustgis_data::progress::Probe;
        use rustgis_swat_run::{Event, run::Spec};
        let control = rustgis_analysis::control::current();
        let still = rustgis_data::source::remote::current_still();
        // Asked between the files and every 50 ms while the engine runs (`Probe::cancelled`, so a
        // test's control counts the looks).
        let stop =
            || control.as_ref().is_some_and(|c| Probe::cancelled(c.as_ref())) || rustgis_data::source::remote::check_still(&still).is_err();
        let (exe, version) = rustgis_swat_run::runner::find()
            .ok_or("swatplus-rust isn't found: install it (swat.installEngine), point to it (swat.setEngine), or put it on the PATH")?;
        let (n, folder) = next_run(&self.root)?;
        // What the engine reads: the project's files and every other file of its folder, as
        // SWAT+ opens some by names of its own (puddle.ops, transplant.plt, treatment.trt), but
        // not tables an earlier run wrote there.
        let mut names: BTreeSet<String> = self.names.iter().cloned().collect();
        if let Some(d) = &self.dir {
            for entry in std::fs::read_dir(d).map_err(|e| format!("{}: {e}", d.display()))?.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if entry.file_type().is_ok_and(|t| t.is_file()) && !written_by_a_run(&name) {
                    names.insert(name);
                }
            }
        }
        // The project's own folder when nothing changed (the engine only reads it); otherwise the
        // project as edited, written beside the run's tables. Both are fingerprinted as read.
        let mut fingerprint = rustgis_swat_run::run::Fingerprint::default();
        let input = match (&self.dir, self.changed.is_empty()) {
            (Some(d), true) => {
                for name in &names {
                    let path = d.join(name);
                    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                    fingerprint.add(name, &bytes);
                    if stop() {
                        break;
                    }
                }
                d.clone()
            }
            _ => {
                let input = folder.join("input");
                for name in &names {
                    // Names stay inside the folder, as they did when read.
                    if name.contains("..") || std::path::Path::new(name).is_absolute() {
                        continue;
                    }
                    let bytes = match (self.edited.get(name), &self.dir) {
                        (Some(b), _) => std::borrow::Cow::Borrowed(b),
                        (None, Some(d)) => {
                            let from = d.join(name);
                            std::borrow::Cow::Owned(std::fs::read(&from).map_err(|e| format!("{}: {e}", from.display()))?)
                        }
                        (None, None) => continue,
                    };
                    fingerprint.add(name, &bytes);
                    let path = input.join(name);
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
                    }
                    std::fs::write(&path, bytes.as_ref()).map_err(|e| format!("{}: {e}", path.display()))?;
                    if stop() {
                        break;
                    }
                }
                input
            }
        };
        let spec =
            Spec { input: input.clone(), output: folder.join("output"), fixes: self.fixes, threads: self.threads, format: self.format };
        let started = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        // The days done, as the engine counts them.
        let mut counted = 0u64;
        let mut on_event = |e: &Event| {
            let Some(c) = &control else { return };
            match e {
                Event::Start { days, .. } => {
                    c.begin(*days);
                    counted = 0;
                }
                Event::Progress { done, .. } if *done > counted => {
                    c.advance(done - counted);
                    counted = *done;
                }
                _ => {}
            }
        };
        let out = if stop() {
            rustgis_swat_run::run::Outcome { stopped: true, ..Default::default() }
        } else {
            rustgis_swat_run::run::run(&exe, &spec, &mut on_event, &stop)?
        };
        let tables: Vec<Value> = out.tables.iter().map(|(f, rows)| json!({"file": f, "rows": rows})).collect();
        let record = json!({
            "run": n,
            "project": self.dir.as_ref().map(|d| d.display().to_string()),
            "folder": folder.display().to_string(),
            "input": input.display().to_string(),
            "output": spec.output.display().to_string(),
            "started": started,
            "engine": {"path": exe.display().to_string(), "version": version},
            "mode": if self.fixes { "fix-swat" } else { "faithful" },
            "threads": if self.fixes { json!(self.threads) } else { json!(1) },
            "format": self.format.as_str(),
            "changed_files": self.changed,
            "changes": self.changes,
            "inputs_sha256": if out.stopped { Value::Null } else { json!(fingerprint.finish()) },
            "ok": out.ok(),
            "stopped": out.stopped,
            "days": out.days,
            "first": out.first,
            "last": out.last,
            "seconds": out.seconds,
            "tables": tables,
            "notes": out.notes,
            "error": out.error,
        });
        // The record beside the run, so a run folder says how it was made, however it ended.
        let manifest = folder.join("run.json");
        std::fs::write(&manifest, serde_json::to_vec_pretty(&record).unwrap_or_default())
            .map_err(|e| format!("{}: {e}", manifest.display()))?;
        Ok(record)
    }
}

/// Whether a file of a TxtInOut folder is one a run wrote there (an output table, a `.out` file,
/// a table as CSV), not one it reads.
#[cfg(not(target_arch = "wasm32"))]
fn written_by_a_run(name: &str) -> bool {
    use rustgis_swat_run::output::table_of;
    table_of(name).is_some()
        || name.ends_with(".out")
        || name.strip_suffix(".csv").is_some_and(|stem| table_of(&format!("{stem}.txt")).is_some())
}

/// `swat.run`'s work, for the desktop app to do on a thread of its own before the command lands
/// (`reads::IN_BACKGROUND`), with the engine's days as its progress and Cancel stopping it.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn run_ahead(s: &mut Session, p: &Value) -> Result<Option<super::reads::ReadWork>, String> {
    let prepared = prepare_run(s, p)?;
    Ok(Some(super::reads::ahead("Run SWAT+ Project", RUN_KEY.into(), move || prepared.go())))
}

/// Runs the project as edited, or, in the desktop app, takes the run made on a thread of its own
/// ([`run_ahead`]); records it with the project it ran (`swat.runs`), unless it was stopped. A run
/// the engine failed or that was stopped is an error, with its record written beside it all the
/// same.
#[cfg(not(target_arch = "wasm32"))]
fn run(s: &mut Session, p: &Value) -> CmdResult {
    let record = match super::reads::take::<Result<Value, String>>(s, RUN_KEY) {
        Some(made) => made?,
        // Here and now (the command line, a script, the assistant): the run's progress and Cancel
        // are those of the caller's control, if it has one.
        None => prepare_run(s, p)?.go()?,
    };
    // Kept with the project it ran, not one opened while it ran; a stopped run only in its folder,
    // as one cancelled in the app never lands here.
    if record["stopped"] != json!(true)
        && let Ok(st) = state_mut(s)
        && record["project"] == json!(st.dir.as_ref().map(|d| d.display().to_string()))
    {
        st.runs.push(record.clone());
    }
    let folder = record["folder"].as_str().unwrap_or_default().to_string();
    if record["stopped"] == json!(true) {
        return Err(format!("the run was stopped ({folder})"));
    }
    match record["error"].as_str() {
        Some(e) if record["ok"] != json!(true) => Err(format!("the run failed ({folder}): {e}")),
        _ => Ok(record),
    }
}

#[cfg(target_arch = "wasm32")]
fn engine(_: &mut Session, _: &Value) -> CmdResult {
    Ok(json!({"found": false, "hint": "SWAT+ projects run in the desktop app"}))
}

#[cfg(target_arch = "wasm32")]
fn set_engine(_: &mut Session, _: &Value) -> CmdResult {
    Err("the SWAT+ engine is located in the desktop app".into())
}

#[cfg(target_arch = "wasm32")]
fn install_engine(_: &mut Session, _: &Value) -> CmdResult {
    Err("the SWAT+ engine is installed in the desktop app".into())
}

#[cfg(target_arch = "wasm32")]
fn run(_: &mut Session, _: &Value) -> CmdResult {
    Err("SWAT+ projects run in the desktop app".into())
}

fn runs(s: &mut Session, _: &Value) -> CmdResult {
    let st = state(s)?;
    Ok(json!({"runs": st.runs}))
}

// `cfg(test)` on its own, as clippy's allow-unwrap-in-tests looks for it.
#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;

    /// The engine chosen (`swat.setEngine`) is the app's, so the tests that choose one take turns.
    #[cfg(unix)]
    static ENGINE: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A small scenario: file.cio naming three tables, one missing.
    fn scenario() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let w = |n: &str, t: &str| std::fs::write(d.path().join(n), t).unwrap();
        w(
            "file.cio",
            "file.cio: test\nsimulation        time.sim          print.prt         object.cnt\nhru               hru-data.hru      null\nhydrology         hydrology.hyd     null\nchg               cal_parms.cal     null\nsoils             soils.sol\n",
        );
        w("time.sim", "time.sim: test\nday_start  yrc_start  day_end  yrc_end  step\n       0       2015        0     2016     0\n");
        w("object.cnt", "object.cnt: test\nname  ls_area  tot_area  obj  hru  lhru  rtu\nbasin  100.0  100.0  3  2  0  1\n");
        w(
            "hru-data.hru",
            "hru-data.hru: test\nid  name  topo  hydro  soil  lu_mgt\n1  hru001  topohru001  hyd001  s1  agrl_lum\n2  hru002  topohru002  hyd002  s1  frst_lum\n",
        );
        w(
            "hydrology.hyd",
            "hydrology.hyd: test\nname           lat_ttime     lat_sed     can_max     esco     epco\nhyd001           0.00000     0.00000     1.00000  0.95000  1.00000\nhyd002           0.00000     0.00000     1.00000  0.80000  1.00000\n",
        );
        w(
            "cal_parms.cal",
            "cal_parms.cal: test\n2\nname  obj_typ  abs_min  abs_max  units\nesco  hru  0.000  1.000  null\nepco  hru  0.000  1.000  null\n",
        );
        d
    }

    fn session(d: &tempfile::TempDir, confidential: bool) -> Session {
        let mut s = Session::new(crate::Project::default());
        s.run("swat.open", &json!({"path": d.path().display().to_string(), "confidential": confidential})).unwrap();
        s
    }

    #[test]
    fn opens_and_summarises() {
        let d = scenario();
        let mut s = session(&d, false);
        let r = s.run("swat.summary", &json!({})).unwrap();
        assert_eq!(r["files"], 6);
        assert_eq!(r["missing_files"], json!(["print.prt", "soils.sol"]));
        assert_eq!(r["objects"]["hru"], 2.0);
        assert_eq!(r["period"]["first_year"], "2015");
        let c = s.run("swat.check", &json!({})).unwrap();
        assert_eq!(c["outside_calibration_range"], json!([]));
        assert_eq!(c["ok"], false);
    }

    #[test]
    fn finds_reads_and_changes_by_rule() {
        let d = scenario();
        let mut s = session(&d, false);
        let f = s.run("swat.find", &json!({"query": "esc"})).unwrap();
        assert_eq!(f["found"][0]["file"], "hydrology.hyd");
        assert_eq!(f["found"][0]["range"], json!([0.0, 1.0]));
        let g = s.run("swat.get", &json!({"file": "hydrology.hyd", "column": "esco"})).unwrap();
        assert_eq!(g["values"]["count"], 2);
        assert_eq!(g["records"][1], json!({"name": "hyd002", "value": "0.80000"}));
        // Multiply one record by 1 + 0.1, kept to the decimals it had.
        let r = s
            .run("swat.set", &json!({"file": "hydrology.hyd", "column": "esco", "value": 0.1, "mode": "multiply", "match": "hyd002"}))
            .unwrap();
        assert_eq!(r["changed"], 1);
        assert_eq!(r["edits"][0]["to"], "0.88000");
        // Out of range is refused, unless forced.
        assert!(s.run("swat.set", &json!({"file": "hydrology.hyd", "column": "esco", "value": 0.2, "mode": "add"})).is_err());
        let text = rustgis_swat_txtinout::encode(s.swat.as_ref().unwrap().project.file("hydrology.hyd").unwrap());
        assert!(String::from_utf8(text).unwrap().contains("  0.88000"), "only the value changes, the columns stay");
        let df = s.run("swat.diff", &json!({})).unwrap();
        assert_eq!(df["files"][0]["columns"][0]["values"][0], json!({"name": "hyd002", "from": "0.80000", "to": "0.88000"}));
        s.run("swat.revert", &json!({"file": "hydrology.hyd", "column": "esco"})).unwrap();
        assert_eq!(s.run("swat.diff", &json!({})).unwrap()["files"], json!([]));
    }

    #[test]
    fn where_picks_records_by_another_column() {
        let d = scenario();
        let mut s = session(&d, false);
        let g = s
            .run("swat.get", &json!({"file": "hru-data.hru", "column": "hydro", "where": {"column": "lu_mgt", "match": "agrl*"}}))
            .unwrap();
        assert_eq!(g["records"], json!([{"name": "1", "value": "hyd001"}]));
    }

    #[test]
    fn saving_writes_only_the_changed_values() {
        let d = scenario();
        let before = std::fs::read_to_string(d.path().join("hydrology.hyd")).unwrap();
        let mut s = session(&d, false);
        s.run("swat.set", &json!({"file": "hydrology.hyd", "column": "epco", "value": 0.9, "match": "hyd001"})).unwrap();
        let out = tempfile::tempdir().unwrap();
        let r = s.run("swat.save", &json!({"path": out.path().display().to_string()})).unwrap();
        assert_eq!(r["written"], 6);
        let after = std::fs::read_to_string(out.path().join("hydrology.hyd")).unwrap();
        let changed: Vec<(&str, &str)> = before.lines().zip(after.lines()).filter(|(a, b)| a != b).collect();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].1.split_whitespace().last(), Some("0.90000"));
        assert_eq!(before.len(), after.len());
        // The other files are the same bytes.
        assert_eq!(std::fs::read(d.path().join("time.sim")).unwrap(), std::fs::read(out.path().join("time.sim")).unwrap());
    }

    /// A stand-in for swatplus-rust: answers `--version`, and for `run <in> --out <out> …` writes
    /// what it was given and the input's hydrology.hyd into the output folder, as events say.
    #[cfg(unix)]
    fn fake_engine(dir: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("swatplus-rust");
        std::fs::write(
            &p,
            r#"#!/bin/sh
if [ "$1" = --version ]; then echo 'swatplus-rust 1.2.0'; exit 0; fi
echo "$@" > "$4/args.txt"
cp "$2/hydrology.hyd" "$4/hydrology.hyd"
echo '{"event":"start","version":"1.2.0","days":731,"first":"2015-01-01","last":"2016-12-31"}'
echo '{"event":"table","file":"channel_sd_day.parquet","rows":731}'
echo '{"event":"done","days":731,"seconds":0.1}'
"#,
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    /// `swat.run` runs the project as edited, in a run folder of its own, and records it; the
    /// project's files are untouched.
    #[cfg(unix)]
    #[test]
    fn a_run_runs_the_project_as_edited_and_is_recorded() {
        let _turn = ENGINE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let d = scenario();
        let bin = tempfile::tempdir().unwrap();
        let runs_dir = tempfile::tempdir().unwrap();
        let exe = fake_engine(bin.path());
        let mut s = session(&d, false);
        s.run("swat.setEngine", &json!({"path": exe.display().to_string()})).unwrap();
        let e = s.run("swat.engine", &json!({})).unwrap();
        assert_eq!((e["found"].clone(), e["version"].clone()), (json!(true), json!("swatplus-rust 1.2.0")));
        let folder = runs_dir.path().display().to_string();

        // Unchanged: the engine reads the project's own folder.
        let r = s.run("swat.run", &json!({"folder": folder})).unwrap();
        assert_eq!((r["run"].clone(), r["ok"].clone(), r["mode"].clone()), (json!(1), json!(true), json!("faithful")));
        assert_eq!(r["input"], json!(d.path().display().to_string()));
        assert_eq!(r["tables"], json!([{"file": "channel_sd_day.parquet", "rows": 731}]));
        let args = std::fs::read_to_string(runs_dir.path().join("run-0001/output/args.txt")).unwrap();
        assert!(args.trim().ends_with("--progress json --format parquet"), "{args}");

        // Changed: the project as edited is written for the run, and only there, with the files
        // SWAT+ opens by names of its own; not an earlier run's tables.
        std::fs::write(d.path().join("puddle.ops"), "puddle.ops: test\n").unwrap();
        std::fs::write(d.path().join("channel_sd_day.txt"), "an earlier run's table\n").unwrap();
        let before = std::fs::read_to_string(d.path().join("hydrology.hyd")).unwrap();
        s.run("swat.set", &json!({"file": "hydrology.hyd", "column": "esco", "value": 0.5})).unwrap();
        let r2 = s.run("swat.run", &json!({"folder": folder, "fixSwat": true, "threads": 2})).unwrap();
        assert_eq!((r2["run"].clone(), r2["mode"].clone()), (json!(2), json!("fix-swat")));
        assert_eq!(r2["changed_files"], json!(["hydrology.hyd"]));
        assert_ne!(r2["inputs_sha256"], r["inputs_sha256"]);
        let ran = std::fs::read_to_string(runs_dir.path().join("run-0002/output/hydrology.hyd")).unwrap();
        assert!(ran.contains("0.50000") && ran != before, "the engine read the edited value: {ran}");
        assert_eq!(std::fs::read_to_string(d.path().join("hydrology.hyd")).unwrap(), before, "the project's file is untouched");
        let args = std::fs::read_to_string(runs_dir.path().join("run-0002/output/args.txt")).unwrap();
        assert!(args.trim().ends_with("--fix-swat --threads 2"), "{args}");
        assert!(runs_dir.path().join("run-0002/input/puddle.ops").is_file(), "a file file.cio doesn't name is run too");
        assert!(!runs_dir.path().join("run-0002/input/channel_sd_day.txt").exists(), "an earlier run's table isn't");
        let manifest: Value = serde_json::from_slice(&std::fs::read(runs_dir.path().join("run-0002/run.json")).unwrap()).unwrap();
        assert_eq!(manifest, r2);

        let all = s.run("swat.runs", &json!({})).unwrap();
        assert_eq!(all["runs"].as_array().map(Vec::len), Some(2));
        assert!(s.run("swat.run", &json!({"folder": folder, "format": "xml"})).is_err());
        assert!(s.run("swat.run", &json!({"folder": folder, "threads": -1})).is_err());
        s.run("swat.setEngine", &json!({})).unwrap();
    }

    /// A stand-in for swatplus-rust whose run says its days as it goes, then `then` (shell).
    #[cfg(unix)]
    fn counting_engine(dir: &std::path::Path, then: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("swatplus-rust");
        std::fs::write(
            &p,
            format!(
                r#"#!/bin/sh
if [ "$1" = --version ]; then echo 'swatplus-rust 1.2.0'; exit 0; fi
echo '{{"event":"start","version":"1.2.0","days":4,"first":"2015-01-01","last":"2015-01-04"}}'
echo '{{"event":"progress","done":2,"days":4,"date":"2015-01-03"}}'
{then}
"#
            ),
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    /// In the desktop app the run is made on a thread of its own (`reads::IN_BACKGROUND`), with
    /// the engine's days as its progress, and lands as it would have run here.
    #[cfg(unix)]
    #[test]
    fn a_run_made_ahead_counts_the_days_and_lands_as_one_made_here() {
        let _turn = ENGINE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        use crate::cmd::reads::{read_ahead, read_land};
        let d = scenario();
        let bin = tempfile::tempdir().unwrap();
        let runs_dir = tempfile::tempdir().unwrap();
        let exe = counting_engine(
            bin.path(),
            r#"echo '{"event":"progress","done":4,"days":4,"date":"2015-01-04"}'
echo '{"event":"done","days":4,"seconds":0.01}'"#,
        );
        let mut s = session(&d, false);
        s.run("swat.setEngine", &json!({"path": exe.display().to_string()})).unwrap();
        let folder = runs_dir.path().display().to_string();
        let ahead = read_ahead(&mut s, "swat.run", &json!({"folder": folder})).unwrap();
        assert_eq!(ahead.name, "Run SWAT+ Project");
        let control = rustgis_analysis::control::Control::stoppable();
        let read = ahead.run(&control).unwrap();
        assert_eq!(control.progress(), Some((4, 4)), "the engine's days are the progress");
        assert!(s.run("swat.runs", &json!({})).unwrap()["runs"].as_array().unwrap().is_empty(), "recorded as it lands");
        let r = read_land(&mut s, read).unwrap();
        assert_eq!((r["run"].clone(), r["ok"].clone(), r["days"].clone()), (json!(1), json!(true), json!(4)));
        assert_eq!(r["project"], json!(d.path().display().to_string()));
        let all = s.run("swat.runs", &json!({})).unwrap();
        assert_eq!(all["runs"], json!([r]));
        s.run("swat.setEngine", &json!({})).unwrap();
    }

    /// Cancel while the engine runs stops it: the run's record says so, beside it, and nothing
    /// lands.
    #[cfg(unix)]
    #[test]
    fn cancelling_a_run_stops_the_engine_and_records_it() {
        let _turn = ENGINE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        use crate::cmd::reads::read_ahead;
        let d = scenario();
        let bin = tempfile::tempdir().unwrap();
        let runs_dir = tempfile::tempdir().unwrap();
        // It would run on for a minute: `exec`, so stopping it stops the sleep.
        let exe = counting_engine(bin.path(), "exec sleep 60");
        let mut s = session(&d, false);
        s.run("swat.setEngine", &json!({"path": exe.display().to_string()})).unwrap();
        let files = s.swat.as_ref().unwrap().project.files.len() as u64;
        let ahead = read_ahead(&mut s, "swat.run", &json!({"folder": runs_dir.path().display().to_string()})).unwrap();
        // The looks before the engine starts are one a file and one more; the next is while it runs.
        let control = rustgis_analysis::control::Control::cancelled_at_look(files + 2);
        let started = std::time::Instant::now();
        let err = ahead.run(&control).err().unwrap();
        assert!(err.contains("Run SWAT+ Project was cancelled"), "{err}");
        assert!(started.elapsed() < std::time::Duration::from_secs(30), "the engine was stopped, not waited for");
        let record: Value = serde_json::from_slice(&std::fs::read(runs_dir.path().join("run-0001/run.json")).unwrap()).unwrap();
        assert_eq!((record["stopped"].clone(), record["ok"].clone()), (json!(true), json!(false)));
        assert_eq!(record["inputs_sha256"], Value::Null);
        assert!(s.run("swat.runs", &json!({})).unwrap()["runs"].as_array().unwrap().is_empty());
        s.run("swat.setEngine", &json!({})).unwrap();
    }

    #[test]
    fn a_confidential_project_answers_with_statistics_only() {
        let d = scenario();
        let mut s = session(&d, true);
        let g = s.run("swat.get", &json!({"file": "hydrology.hyd", "column": "esco"})).unwrap();
        assert_eq!(g["values"]["count"], 2);
        assert!(g["records"].is_string(), "{g}");
        assert!(!g.to_string().contains("hyd001"));
        let r = s.run("swat.set", &json!({"file": "hydrology.hyd", "column": "esco", "value": 0.9})).unwrap();
        assert!(r.get("edits").is_none());
        let df = s.run("swat.diff", &json!({})).unwrap();
        assert!(!df.to_string().contains("0.90000") && !df.to_string().contains("hyd00"), "{df}");
        // Only the user may lift it.
        assert!(super::super::plugins::user_only("swat.confidential"));
    }

    #[test]
    fn globbing() {
        assert!(glob("hru*", "HRU0012"));
        assert!(glob("h?d00?", "hyd002"));
        assert!(!glob("frst*", "agrl_lum"));
    }
}
