//! A SWAT+ run's output tables (A1.2 of the swatplus-rust roadmap): which tables a run wrote
//! (`swat.output.list`), a table's statistics and rows picked by object and date
//! (`swat.output.read`), the basin's water balance (`swat.balance`), and two runs side by side
//! (`swat.compare`); observed series loaded for the project (`swat.observed`) and how well a run
//! fits them (`swat.metrics`: NSE, KGE, PBIAS, RSR, Spearman; stage through a rating curve).
//!
//! The tables are the engine's (text or Parquet, `rustgis_swat_run::output`) in a run's folder,
//! or SWAT+'s own text tables in a TxtInOut folder. A table is gone through a row at a time; in
//! the desktop app on a thread of its own (`reads::IN_BACKGROUND`), with its progress and Cancel.
//! A confidential project's answers carry statistics only: no rows and no object names.

use std::collections::BTreeMap;

#[cfg(not(target_arch = "wasm32"))]
use super::swat::MAX_LISTED;
use super::swat::need_swat;
use crate::CommandSpec as C;

/// An observed series loaded for the project (`swat.observed`).
#[derive(Clone, Debug, PartialEq)]
pub struct Observed {
    /// `flow` (m³/s) or `stage` (m).
    pub kind: &'static str,
    pub path: String,
    pub column: String,
    /// Daily values by (year, month, day): the mean of the day's readings.
    pub series: BTreeMap<(i64, i64, i64), f64>,
}

const WHERE: &str = "Analyze › SWAT+";

/// Rows listed by default, of those picked (the statistics cover them all).
#[cfg(not(target_arch = "wasm32"))]
const LISTED: usize = 50;

pub fn commands() -> Vec<C> {
    let files = crate::policy::Effects::NONE.with(crate::policy::Effect::Files);
    vec![
        C::new("swat.output.list", "SWAT+ Output Tables", WHERE, list).roles(&[]).effects(files).view().params(
            r#"{"run"?: number (a run of swat.runs) | "a run's folder, or a folder of SWAT+ text tables (default: the last run, else the project's folder)"} — the output tables of a run: object, period, formats, columns"#,
        ),
        C::new("swat.output.read", "Read SWAT+ Output Table", WHERE, read).roles(&[]).effects(files).view().params(
            r#"{"run"?: number | "folder" (as swat.output.list), "table": "e.g. channel_sd_day", "columns"?: ["column", …] (default all), "match"?: "object names, with * and ?", "unit"?: number | [number, …], "from"?: "yyyy[-mm[-dd]]", "to"?: "yyyy[-mm[-dd]]", "limit"?: number (rows listed, default 50, at most 500)} — statistics of the columns over the rows picked, and the first rows"#,
        ),
        C::new("swat.balance", "SWAT+ Water Balance", WHERE, balance).roles(&[]).effects(files).view().params(
            r#"{"run"?: number | "folder" (as swat.output.list)} — the basin's average annual water balance (basin_wb_aa, and basin_aqu_aa when printed), in mm, with its ratios to precipitation and the baseflow fraction"#,
        ),
        C::new("swat.compare", "Compare SWAT+ Runs", WHERE, compare).roles(&[]).effects(files).view().params(
            r#"{"runs": [a, b] (each a run's number or a folder), "table": "e.g. channel_sd_day", "columns"?: ["column", …], "match"?: "object names, with * and ?", "unit"?: number | [number, …], "from"?: "yyyy[-mm[-dd]]", "to"?: "yyyy[-mm[-dd]]"} — each column's statistics in both runs and the change in its mean, overall and by object"#,
        ),
        C::new("swat.observed", "Load SWAT+ Observations", WHERE, observed).roles(&[]).effects(files).view().when(need_swat).params(
            r#"{"path"?: "a CSV of dates and values (none: list those loaded)", "name"?: text (default: the file's name), "column"?: "the values' column (default: the first after the date)", "kind"?: "flow" (m³/s, default) | "stage" (m), "dateColumn"?: "default: date, tanggal, time, waktu or the first column", "dayFirst"?: bool (dd/mm/yyyy, the default; false for mm/dd/yyyy), "missing"?: number (a value that means none, such as -9999)} — loads an observed series for swat.metrics; readings of one day are averaged"#,
        ),
        C::new("swat.metrics", "SWAT+ Fit Statistics", WHERE, metrics).roles(&[]).effects(files).view().when(need_swat).params(
            r#"{"observed": "a name from swat.observed", "run"?: number | "folder" (as swat.output.list), "table"?: "a daily table (default channel_sd_day)", "column"?: "default flo_out", "match"?: "the object's name", "unit"?: number, "from"?: "yyyy[-mm[-dd]]", "to"?: "yyyy[-mm[-dd]]", "step"?: "day" (default) | "month", "rating"?: "fit" (stage's default) | {"a": number, "b": number, "h0": number}} — NSE, KGE (r, alpha, beta), PBIAS, RSR and Spearman of one object's simulated series against the observed, on the days both have; stage through a rating curve fitted to the run or given"#,
        ),
    ]
}

#[cfg(target_arch = "wasm32")]
use web::{balance, compare, list, metrics, observed, read};

/// The browser has no run folders yet: the engine's WebAssembly module will give it tables.
#[cfg(target_arch = "wasm32")]
mod web {
    use crate::{CmdResult, Session};
    use serde_json::Value;

    const WHY: &str = "SWAT+ output tables are read in the desktop app";

    pub(super) fn list(_: &mut Session, _: &Value) -> CmdResult {
        Err(WHY.into())
    }

    pub(super) fn read(_: &mut Session, _: &Value) -> CmdResult {
        Err(WHY.into())
    }

    pub(super) fn balance(_: &mut Session, _: &Value) -> CmdResult {
        Err(WHY.into())
    }

    pub(super) fn compare(_: &mut Session, _: &Value) -> CmdResult {
        Err(WHY.into())
    }

    pub(super) fn observed(_: &mut Session, _: &Value) -> CmdResult {
        Err(WHY.into())
    }

    pub(super) fn metrics(_: &mut Session, _: &Value) -> CmdResult {
        Err(WHY.into())
    }
}

#[cfg(not(target_arch = "wasm32"))]
use native::{balance, compare, list, metrics, observed, read};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::{compare_ahead, metrics_ahead, read_ahead};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use rustgis_swat_run::output::{self, Cell, Scanned, TableFile, tidy};
    use serde_json::{Value, json};

    use super::super::b;
    use super::super::swat::{glob, state};
    use super::super::{f, ps};
    use super::{LISTED, MAX_LISTED, Observed};
    use crate::{CmdResult, Session};

    const READ_KEY: &str = "swat.output.read";
    const COMPARE_KEY: &str = "swat.compare";
    const METRICS_KEY: &str = "swat.metrics";

    /// The output folder `which` names, and how to say it: a run of `swat.runs` by its number;
    /// a run's folder (its `output`) or any folder of tables; by default the last run that
    /// finished, else the project's own folder (where SWAT+ writes its text tables).
    fn source(s: &Session, which: Option<&Value>) -> Result<(PathBuf, Value), String> {
        match which {
            Some(Value::Number(n)) => {
                let n = n.as_u64().ok_or_else(|| format!("run is a run's number, not {n}"))?;
                let st = state(s)?;
                let r = st
                    .runs
                    .iter()
                    .rev()
                    .find(|r| r["run"].as_u64() == Some(n))
                    .ok_or_else(|| format!("no run {n} since the project was opened (swat.runs): give its folder instead"))?;
                let out = r["output"].as_str().ok_or_else(|| format!("run {n} has no output folder"))?;
                Ok((PathBuf::from(out), json!({"run": n, "folder": r["folder"]})))
            }
            Some(Value::String(path)) if !path.trim().is_empty() => {
                let p = PathBuf::from(path.trim());
                let out = if p.join("run.json").is_file() && p.join("output").is_dir() { p.join("output") } else { p };
                let label = json!({"folder": out.display().to_string()});
                Ok((out, label))
            }
            None | Some(Value::Null) => {
                let st = state(s).map_err(|_| "give run: a run's folder, or a folder of SWAT+ tables".to_string())?;
                if let Some(r) = st.runs.iter().rev().find(|r| r["ok"] == json!(true))
                    && let Some(out) = r["output"].as_str()
                {
                    return Ok((PathBuf::from(out), json!({"run": r["run"], "folder": r["folder"]})));
                }
                let dir = st.dir.clone().ok_or("no run yet (swat.run), and the project has no folder")?;
                let label = json!({"folder": dir.display().to_string(), "project": true});
                Ok((dir, label))
            }
            Some(other) => Err(format!("run is a run's number or a folder, not {other}")),
        }
    }

    fn confidential(s: &Session) -> bool {
        s.swat.as_ref().is_some_and(|st| st.confidential)
    }

    /// The table `name` (with or without its extension) in `dir`.
    fn table_in(dir: &Path, name: &str) -> Result<TableFile, String> {
        let name = name.trim();
        let stem = name.strip_suffix(".parquet").or_else(|| name.strip_suffix(".txt")).unwrap_or(name);
        let tables = output::list(dir)?;
        if tables.is_empty() {
            return Err(format!("{}: no SWAT+ output tables here", dir.display()));
        }
        tables.into_iter().find(|t| t.table == stem).ok_or_else(|| {
            format!(
                "{}: no table {stem} (swat.output.list lists them; print.prt says which objects and periods are printed)",
                dir.display()
            )
        })
    }

    pub(super) fn list(s: &mut Session, p: &Value) -> CmdResult {
        let (dir, label) = source(s, p.get("run"))?;
        let mut tables = Vec::new();
        for t in output::list(&dir)? {
            let Some(file) = t.file() else { continue };
            let h = output::header(file)?;
            let values: Vec<&String> = h.columns.iter().filter(|c| !output::is_key(c)).collect();
            tables.push(json!({
                "table": t.table, "object": t.object, "period": t.period, "formats": t.formats(),
                "rows": h.rows, "columns": values,
            }));
        }
        Ok(json!({"source": label, "folder": dir.display().to_string(), "tables": tables}))
    }

    /// What picks a table's rows, and which columns are summed up.
    #[derive(Clone, Debug, Default)]
    struct Query {
        columns: Option<Vec<String>>,
        matching: Option<String>,
        units: Option<Vec<i64>>,
        from: Option<(i64, i64, i64)>,
        to: Option<(i64, i64, i64)>,
        limit: usize,
    }

    /// `yyyy`, `yyyy-mm` or `yyyy-mm-dd`, the missing parts as early (`end`: as late) as they go.
    fn date(text: &str, end: bool) -> Option<(i64, i64, i64)> {
        let mut parts = text.trim().split('-').map(|p| p.parse::<i64>().ok());
        let y = parts.next()??;
        let m = parts.next().map_or(Some(if end { 12 } else { 1 }), |m| m.filter(|m| (1..=12).contains(m)))?;
        let d = parts.next().map_or(Some(if end { 31 } else { 1 }), |d| d.filter(|d| (1..=31).contains(d)))?;
        parts.next().is_none().then_some((y, m, d))
    }

    fn query(p: &Value) -> Result<Query, String> {
        let columns = match p.get("columns") {
            None | Some(Value::Null) => None,
            Some(Value::String(c)) => Some(vec![c.trim().to_string()]),
            Some(Value::Array(a)) => {
                Some(a.iter().map(|c| c.as_str().map(|c| c.trim().to_string()).ok_or("columns are names")).collect::<Result<Vec<_>, _>>()?)
            }
            Some(other) => return Err(format!("columns is a list of names, not {other}")),
        };
        let units = match p.get("unit") {
            None | Some(Value::Null) => None,
            Some(Value::Array(a)) => {
                Some(a.iter().map(|u| u.as_i64().ok_or("unit is a number or a list of numbers")).collect::<Result<Vec<_>, _>>()?)
            }
            Some(u) => Some(vec![u.as_i64().ok_or_else(|| format!("unit is a number or a list of numbers, not {u}"))?]),
        };
        let when = |k: &str, end: bool| -> Result<Option<(i64, i64, i64)>, String> {
            match p.get(k) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::Number(n)) => n
                    .as_i64()
                    .map(|y| Some((y, if end { 12 } else { 1 }, if end { 31 } else { 1 })))
                    .ok_or(format!("{k} is a year or a date")),
                Some(Value::String(t)) => date(t, end).map(Some).ok_or_else(|| format!("{k} is yyyy, yyyy-mm or yyyy-mm-dd, not {t}")),
                Some(other) => Err(format!("{k} is yyyy, yyyy-mm or yyyy-mm-dd, not {other}")),
            }
        };
        let limit = match f(p, "limit") {
            None => LISTED,
            Some(l) if l >= 0.0 && l.fract() == 0.0 => (l as usize).min(MAX_LISTED),
            Some(l) => return Err(format!("limit is a whole number, not {l}")),
        };
        Ok(Query {
            columns,
            matching: ps(p, "match").map(str::trim).filter(|m| !m.is_empty()).map(str::to_string),
            units,
            from: when("from", false)?,
            to: when("to", true)?,
            limit,
        })
    }

    /// A column's sum, count and range.
    #[derive(Clone, Copy, Debug, Default)]
    struct Acc {
        n: u64,
        sum: f64,
        min: f64,
        max: f64,
    }

    impl Acc {
        fn add(&mut self, x: f64) {
            if self.n == 0 {
                (self.min, self.max) = (x, x);
            } else {
                self.min = self.min.min(x);
                self.max = self.max.max(x);
            }
            self.n += 1;
            self.sum += x;
        }

        fn mean(&self) -> Option<f64> {
            (self.n > 0).then(|| self.sum / self.n as f64)
        }

        fn to_json(self) -> Value {
            if self.n == 0 {
                return json!({"count": 0});
            }
            json!({"count": self.n, "mean": self.mean().map(tidy), "min": tidy(self.min), "max": tidy(self.max), "sum": tidy(self.sum)})
        }
    }

    /// The thread's Cancel and progress (`rustgis_analysis::control`), when it has them.
    struct Watch {
        control: Option<std::sync::Arc<rustgis_analysis::control::Control>>,
        still: Option<rustgis_data::source::remote::Still>,
        total: std::cell::Cell<u64>,
        done: std::cell::Cell<u64>,
    }

    impl Watch {
        fn new() -> Self {
            Watch {
                control: rustgis_analysis::control::current(),
                still: rustgis_data::source::remote::current_still(),
                total: std::cell::Cell::new(0),
                done: std::cell::Cell::new(0),
            }
        }

        fn stop(&self) -> bool {
            use rustgis_data::progress::Probe;
            self.control.as_ref().is_some_and(|c| Probe::cancelled(c.as_ref()))
                || rustgis_data::source::remote::check_still(&self.still).is_err()
        }

        fn progress(&self, done: u64, total: u64) {
            use rustgis_data::progress::Probe;
            let Some(c) = &self.control else { return };
            if total != self.total.get() {
                c.begin(total);
                self.total.set(total);
                self.done.set(0);
            }
            if done > self.done.get() {
                c.advance(done - self.done.get());
                self.done.set(done);
            }
        }
    }

    /// What a table's rows gave for a query.
    #[derive(Default)]
    struct Picked {
        columns: Vec<String>,
        stats: Vec<Acc>,
        matched: u64,
        records: Vec<Value>,
        /// By object (its name, else its unit number): each column's figures.
        objects: BTreeMap<String, Vec<Acc>>,
        /// Objects past [`MAX_LISTED`], counted only.
        objects_more: u64,
        scanned: Scanned,
    }

    /// Goes through `file`'s rows and sums up those `q` picks; keeps the first `q.limit` rows
    /// when `records`, and figures by object when `by_object`.
    fn pick(file: &Path, q: &Query, records: bool, by_object: bool, watch: &Watch) -> Result<Picked, String> {
        let h = output::header(file)?;
        let at = |name: &str| h.columns.iter().position(|c| c == name);
        let (yr, mon, day, unit, name) = (at("yr"), at("mon"), at("day"), at("unit"), at("name"));
        let chosen: Vec<usize> = match &q.columns {
            Some(cols) => {
                let unknown: Vec<&String> = cols.iter().filter(|c| at(c).is_none()).collect();
                if !unknown.is_empty() {
                    return Err(format!(
                        "{} has no column {} (its columns: {})",
                        file.display(),
                        unknown.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", "),
                        h.columns.iter().filter(|c| !output::is_key(c)).cloned().collect::<Vec<_>>().join(", ")
                    ));
                }
                cols.iter().filter_map(|c| at(c)).collect()
            }
            None => (0..h.columns.len()).filter(|&i| h.columns.get(i).is_some_and(|c| !output::is_key(c))).collect(),
        };
        let keys: Vec<usize> = [yr, mon, day, unit, name].into_iter().flatten().collect();
        let mut out = Picked { columns: chosen.iter().filter_map(|&i| h.columns.get(i).cloned()).collect(), ..Default::default() };
        out.stats = vec![Acc::default(); chosen.len()];
        let whole = |row: &[Cell], i: Option<usize>| i.and_then(|i| row.get(i)).and_then(Cell::whole);
        let mut on_row = |row: &[Cell]| {
            if watch.stop() {
                return false;
            }
            let object = name.and_then(|i| row.get(i)).and_then(Cell::text);
            if let Some(m) = &q.matching
                && !object.is_some_and(|o| glob(m, o))
            {
                return true;
            }
            if let Some(units) = &q.units
                && !whole(row, unit).is_some_and(|u| units.contains(&u))
            {
                return true;
            }
            if q.from.is_some() || q.to.is_some() {
                let when = (whole(row, yr).unwrap_or(0), whole(row, mon).unwrap_or(1), whole(row, day).unwrap_or(1));
                if q.from.is_some_and(|f| when < f) || q.to.is_some_and(|t| when > t) {
                    return true;
                }
            }
            out.matched += 1;
            for (acc, &i) in out.stats.iter_mut().zip(&chosen) {
                if let Some(x) = row.get(i).and_then(Cell::number) {
                    acc.add(x);
                }
            }
            if by_object {
                let key = object.map(str::to_string).or_else(|| whole(row, unit).map(|u| u.to_string())).unwrap_or_default();
                let n = out.objects.len();
                match out.objects.get_mut(&key) {
                    Some(accs) => {
                        for (acc, &i) in accs.iter_mut().zip(&chosen) {
                            if let Some(x) = row.get(i).and_then(Cell::number) {
                                acc.add(x);
                            }
                        }
                    }
                    None if n < MAX_LISTED => {
                        let mut accs = vec![Acc::default(); chosen.len()];
                        for (acc, &i) in accs.iter_mut().zip(&chosen) {
                            if let Some(x) = row.get(i).and_then(Cell::number) {
                                acc.add(x);
                            }
                        }
                        out.objects.insert(key, accs);
                    }
                    None => out.objects_more += 1,
                }
            }
            if records && out.records.len() < q.limit {
                let mut r = serde_json::Map::new();
                for &i in keys.iter().chain(&chosen) {
                    if let (Some(c), Some(v)) = (h.columns.get(i), row.get(i)) {
                        r.insert(c.clone(), v.to_json());
                    }
                }
                out.records.push(Value::Object(r));
            }
            true
        };
        let mut progress = |done: u64, total: u64| watch.progress(done, total);
        out.scanned = output::scan(file, &mut on_row, &mut progress)?;
        Ok(out)
    }

    /// A read of a table, prepared on the window's thread.
    pub(crate) struct ReadJob {
        label: Value,
        dir: PathBuf,
        table: TableFile,
        query: Query,
        confidential: bool,
    }

    fn prepare_read(s: &Session, p: &Value) -> Result<ReadJob, String> {
        let (dir, label) = source(s, p.get("run"))?;
        let name = ps(p, "table").ok_or("give table: e.g. channel_sd_day (swat.output.list lists them)")?;
        let table = table_in(&dir, name)?;
        Ok(ReadJob { label, dir, table, query: query(p)?, confidential: confidential(s) })
    }

    impl ReadJob {
        fn go(self) -> Result<Value, String> {
            let file = self.table.file().ok_or("the table has no file")?.to_path_buf();
            let watch = Watch::new();
            let picked = pick(&file, &self.query, !self.confidential, false, &watch)?;
            if picked.scanned.stopped {
                return Err("Read SWAT+ Output Table was cancelled".into());
            }
            let mut out = json!({
                "source": self.label,
                "folder": self.dir.display().to_string(),
                "table": self.table.table,
                "file": file.file_name().map(|n| n.to_string_lossy().into_owned()),
                "scanned": picked.scanned.rows,
                "matched": picked.matched,
                "statistics": picked.columns.iter().zip(&picked.stats).map(|(c, a)| (c.clone(), a.to_json())).collect::<serde_json::Map<_, _>>(),
                "uneven": picked.scanned.uneven,
            });
            if self.confidential {
                out["records"] = json!("hidden: the project is confidential (statistics only)");
            } else {
                out["more"] = json!(picked.matched.saturating_sub(picked.records.len() as u64));
                out["records"] = Value::Array(picked.records);
            }
            Ok(out)
        }
    }

    /// `swat.output.read`'s work, for the desktop app to do on a thread of its own.
    pub(crate) fn read_ahead(s: &mut Session, p: &Value) -> Result<Option<super::super::reads::ReadWork>, String> {
        let job = prepare_read(s, p)?;
        Ok(Some(super::super::reads::ahead("Read SWAT+ Output Table", READ_KEY.into(), move || job.go())))
    }

    /// Text rows that didn't have as many values as columns are said, once, with their count.
    fn say_uneven(s: &mut Session, table: &str, uneven: u64) {
        if uneven > 0 {
            s.notify(
                format!(
                    "{table}: {uneven} rows didn't have a value for every column; they were read as far as they went, the rest as missing"
                ),
                false,
            );
        }
    }

    pub(super) fn read(s: &mut Session, p: &Value) -> CmdResult {
        let out = match super::super::reads::take::<Result<Value, String>>(s, READ_KEY) {
            Some(made) => made?,
            None => prepare_read(s, p)?.go()?,
        };
        say_uneven(s, out["table"].as_str().unwrap_or_default(), out["uneven"].as_u64().unwrap_or(0));
        Ok(out)
    }

    /// A column's value in the only row of an average annual table.
    fn only_row(dir: &Path, table: &str) -> Result<Option<BTreeMap<String, f64>>, String> {
        let Ok(t) = table_in(dir, table) else { return Ok(None) };
        let Some(file) = t.file() else { return Ok(None) };
        let h = output::header(file)?;
        let mut values = BTreeMap::new();
        let mut rows = 0u64;
        output::scan(
            file,
            &mut |row| {
                rows += 1;
                for (c, v) in h.columns.iter().zip(row) {
                    if let Some(x) = v.number().filter(|_| !output::is_key(c)) {
                        values.insert(c.clone(), x);
                    }
                }
                rows < 2
            },
            &mut |_, _| {},
        )?;
        if rows != 1 {
            return Err(format!("{table} has {rows} rows: the basin's table has one"));
        }
        Ok(Some(values))
    }

    /// Reads two average annual tables of one row each: the work doesn't grow with the basin,
    /// so it is done here.
    pub(super) fn balance(s: &mut Session, p: &Value) -> CmdResult {
        let (dir, label) = source(s, p.get("run"))?;
        let wb = only_row(&dir, "basin_wb_aa")?
            .ok_or_else(|| format!("{}: no basin_wb_aa table (print.prt's basin_wb row, average annual column)", dir.display()))?;
        let aqu = only_row(&dir, "basin_aqu_aa")?;
        let v = |k: &str| wb.get(k).copied();
        let mut mm = serde_json::Map::new();
        for k in [
            "precip",
            "snofall",
            "snomlt",
            "surq_gen",
            "surq_cont",
            "latq",
            "qtile",
            "wateryld",
            "perc",
            "et",
            "ecanopy",
            "eplant",
            "esoil",
            "pet",
            "irr",
            "sw_ave",
        ] {
            if let Some(x) = v(k) {
                mm.insert(k.into(), json!(tidy(x)));
            }
        }
        let (flo, seep) = match &aqu {
            Some(a) => {
                let mut am = serde_json::Map::new();
                for k in ["flo", "flo_cha", "rchrg", "seep", "revap", "stor", "dep_wt"] {
                    if let Some(x) = a.get(k) {
                        am.insert(k.into(), json!(tidy(*x)));
                    }
                }
                mm.insert("aquifer".into(), Value::Object(am));
                (a.get("flo_cha").or(a.get("flo")).copied(), a.get("seep").copied())
            }
            None => (None, None),
        };
        let mut ratios = serde_json::Map::new();
        if let Some(pcp) = v("precip").filter(|p| *p > 0.0) {
            for (name, of) in
                [("et", v("et")), ("wateryld", v("wateryld")), ("surq_gen", v("surq_gen")), ("perc", v("perc")), ("aquifer_seep", seep)]
            {
                if let Some(x) = of {
                    ratios.insert(format!("{name}_to_precip"), json!(tidy(x / pcp)));
                }
            }
            if let (Some(w), Some(fl)) = (v("wateryld"), flo) {
                ratios.insert("streamflow_to_precip".into(), json!(tidy((w + fl) / pcp)));
            }
        }
        if let (Some(sq), Some(lq), Some(fl)) = (v("surq_gen"), v("latq"), flo)
            && sq + lq + fl > 0.0
        {
            ratios.insert("baseflow_fraction".into(), json!(tidy((lq + fl) / (sq + lq + fl))));
        }
        let mut notes = vec![
            "mm over the basin, average annual over the printed years".to_string(),
            "streamflow is the land's water yield (surq_gen + latq) and the aquifers' return flow to the channels; baseflow is lateral and return flow".to_string(),
        ];
        if aqu.is_none() {
            notes.push("no basin_aqu_aa table (print.prt's basin_aqu row): no aquifer flows, streamflow or baseflow fraction".into());
        }
        Ok(json!({"source": label, "folder": dir.display().to_string(), "mm": mm, "ratios": ratios, "notes": notes}))
    }

    /// A comparison of two runs' table, prepared on the window's thread.
    pub(crate) struct CompareJob {
        a: (Value, TableFile),
        b: (Value, TableFile),
        query: Query,
        confidential: bool,
    }

    fn prepare_compare(s: &Session, p: &Value) -> Result<CompareJob, String> {
        let runs =
            p.get("runs").and_then(Value::as_array).filter(|r| r.len() == 2).ok_or("give runs: [a, b], each a run's number or folder")?;
        let name = ps(p, "table").ok_or("give table: e.g. channel_sd_day (swat.output.list lists them)")?;
        let (da, la) = source(s, runs.first())?;
        let (db, lb) = source(s, runs.get(1))?;
        Ok(CompareJob { a: (la, table_in(&da, name)?), b: (lb, table_in(&db, name)?), query: query(p)?, confidential: confidential(s) })
    }

    impl CompareJob {
        fn go(self) -> Result<Value, String> {
            let watch = Watch::new();
            let mut picked = Vec::new();
            for (_, t) in [&self.a, &self.b] {
                let file = t.file().ok_or("the table has no file")?;
                let pk = pick(file, &self.query, false, !self.confidential, &watch)?;
                if pk.scanned.stopped {
                    return Err("Compare SWAT+ Runs was cancelled".into());
                }
                picked.push(pk);
            }
            let (Some(pa), Some(pb)) = (picked.first(), picked.get(1)) else { return Err("nothing was read".into()) };
            let change = |a: &Acc, b: &Acc| {
                let (ma, mb) = (a.mean(), b.mean());
                let diff = ma.zip(mb).map(|(x, y)| y - x);
                let pct = ma.zip(diff).filter(|(x, _)| *x != 0.0).map(|(x, d)| 100.0 * d / x.abs());
                (ma, mb, diff, pct)
            };
            let mut columns = serde_json::Map::new();
            for (c, a) in pa.columns.iter().zip(&pa.stats) {
                let Some(b) = pb.columns.iter().position(|x| x == c).and_then(|i| pb.stats.get(i)) else { continue };
                let (_, _, diff, pct) = change(a, b);
                columns.insert(c.clone(), json!({"a": a.to_json(), "b": b.to_json(), "change": diff.map(tidy), "percent": pct.map(tidy)}));
            }
            let mut notes = Vec::new();
            if pa.matched != pb.matched {
                notes.push(format!(
                    "the runs' tables have different rows picked (a {}, b {}): different periods or print settings",
                    pa.matched, pb.matched
                ));
            }
            let mut out = json!({
                "a": self.a.0, "b": self.b.0, "table": self.a.1.table,
                "matched": {"a": pa.matched, "b": pb.matched},
                "columns": columns, "notes": notes,
                "uneven": pa.scanned.uneven + pb.scanned.uneven,
            });
            if self.confidential {
                out["objects"] = json!("hidden: the project is confidential (statistics only)");
            } else {
                let mut objects = Vec::new();
                for (name, accs) in &pa.objects {
                    let Some(bccs) = pb.objects.get(name) else { continue };
                    let mut o = serde_json::Map::new();
                    o.insert("object".into(), json!(name));
                    for (c, a) in pa.columns.iter().zip(accs) {
                        let Some(b) = pb.columns.iter().position(|x| x == c).and_then(|i| bccs.get(i)) else { continue };
                        let (ma, mb, diff, pct) = change(a, b);
                        o.insert(
                            c.clone(),
                            json!({"a": ma.map(tidy), "b": mb.map(tidy), "change": diff.map(tidy), "percent": pct.map(tidy)}),
                        );
                    }
                    objects.push(Value::Object(o));
                }
                out["objects"] = Value::Array(objects);
                out["objects_more"] = json!(pa.objects_more.max(pb.objects_more));
            }
            Ok(out)
        }
    }

    /// `swat.compare`'s work, for the desktop app to do on a thread of its own.
    pub(crate) fn compare_ahead(s: &mut Session, p: &Value) -> Result<Option<super::super::reads::ReadWork>, String> {
        let job = prepare_compare(s, p)?;
        Ok(Some(super::super::reads::ahead("Compare SWAT+ Runs", COMPARE_KEY.into(), move || job.go())))
    }

    pub(super) fn compare(s: &mut Session, p: &Value) -> CmdResult {
        let out = match super::super::reads::take::<Result<Value, String>>(s, COMPARE_KEY) {
            Some(made) => made?,
            None => prepare_compare(s, p)?.go()?,
        };
        say_uneven(s, out["table"].as_str().unwrap_or_default(), out["uneven"].as_u64().unwrap_or(0));
        Ok(out)
    }

    #[cfg(test)]
    pub(super) fn date_for_tests(text: &str, end: bool) -> Option<(i64, i64, i64)> {
        date(text, end)
    }

    // -----------------------------------------------------------------------------------------
    // Observations and how well a run fits them

    /// The largest observation file read (a century of hourly readings is far less).
    const MAX_OBSERVED_BYTES: u64 = 256 << 20;

    /// How a CSV of observations is read.
    pub(super) struct ObsOptions {
        pub(super) column: Option<String>,
        pub(super) kind: &'static str,
        pub(super) date_column: Option<String>,
        pub(super) day_first: bool,
        pub(super) missing: Option<f64>,
    }

    /// What a CSV of observations held besides its values.
    #[derive(Debug, Default, PartialEq)]
    pub(super) struct ObsReport {
        pub(super) readings: u64,
        pub(super) bad_dates: u64,
        pub(super) bad_values: u64,
        pub(super) missing: u64,
        /// Days with several readings, averaged.
        pub(super) several: u64,
    }

    fn days_in_month(y: i64, m: i64) -> i64 {
        match m {
            2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        }
    }

    /// A reading's day: `yyyy-mm-dd` or `yyyy/mm/dd`, else `dd/mm/yyyy` (or `mm/dd/yyyy` when not
    /// `day_first`), with `-`, `/` or `.` between, and any time after a space or `T` left out.
    pub(super) fn obs_date(text: &str, day_first: bool) -> Option<(i64, i64, i64)> {
        let day = text.trim().trim_matches('"').split([' ', 'T']).next()?;
        let parts: Vec<&str> = day.split(['-', '/', '.']).collect();
        let [p0, p1, p2] = parts.as_slice() else { return None };
        let n = |p: &str| p.trim().parse::<i64>().ok();
        let (y, m, d) = if p0.trim().len() == 4 {
            (n(p0)?, n(p1)?, n(p2)?)
        } else if day_first {
            (n(p2)?, n(p1)?, n(p0)?)
        } else {
            (n(p2)?, n(p0)?, n(p1)?)
        };
        ((1800..=2200).contains(&y) && (1..=12).contains(&m) && (1..=days_in_month(y, m)).contains(&d)).then_some((y, m, d))
    }

    /// Reads a CSV of dates and values: the delimiter the header uses most (`,`, `;` or a tab;
    /// with `;` a decimal comma is read), the day of each reading, and the mean of a day's.
    pub(super) fn read_observed(text: &str, o: &ObsOptions, path: &str) -> Result<(Observed, ObsReport), String> {
        let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'));
        let header = lines.next().ok_or("the file is empty")?;
        let delim = [',', ';', '\t'].into_iter().max_by_key(|d| header.matches(*d).count()).filter(|d| header.contains(*d));
        let split = |line: &str| -> Vec<String> {
            match delim {
                Some(d) => line.split(d).map(|f| f.trim().trim_matches('"').trim().to_string()).collect(),
                None => line.split_whitespace().map(str::to_string).collect(),
            }
        };
        let cols = split(header);
        let find = |name: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(name));
        let date_at = match &o.date_column {
            Some(c) => find(c).ok_or_else(|| format!("no column {c} (columns: {})", cols.join(", ")))?,
            None => ["date", "tanggal", "time", "datetime", "waktu"].iter().find_map(|c| find(c)).unwrap_or(0),
        };
        let value_at = match &o.column {
            Some(c) => find(c).ok_or_else(|| format!("no column {c} (columns: {})", cols.join(", ")))?,
            None => (0..cols.len()).find(|&i| i != date_at).ok_or("the file has one column: give its dates and values")?,
        };
        let mut sums: BTreeMap<(i64, i64, i64), (f64, u32)> = BTreeMap::new();
        let mut r = ObsReport::default();
        for line in lines {
            let fields = split(line);
            let Some(day) = fields.get(date_at).and_then(|d| obs_date(d, o.day_first)) else {
                r.bad_dates += 1;
                continue;
            };
            let raw = fields.get(value_at).map(String::as_str).unwrap_or("");
            if raw.is_empty() || ["na", "nan", "-", "null"].iter().any(|m| raw.eq_ignore_ascii_case(m)) {
                r.missing += 1;
                continue;
            }
            let parsed = if delim == Some(';') { raw.replace(',', ".").parse::<f64>() } else { raw.parse::<f64>() };
            let Ok(x) = parsed else {
                r.bad_values += 1;
                continue;
            };
            if !x.is_finite() || o.missing.is_some_and(|m| x == m) {
                r.missing += 1;
                continue;
            }
            r.readings += 1;
            let e = sums.entry(day).or_insert((0.0, 0));
            e.0 += x;
            e.1 += 1;
        }
        if sums.is_empty() {
            return Err(format!("no readings with a date and a value in column {}", cols.get(value_at).cloned().unwrap_or_default()));
        }
        r.several = sums.values().filter(|(_, n)| *n > 1).count() as u64;
        let series = sums.into_iter().map(|(d, (sum, n))| (d, sum / f64::from(n))).collect();
        Ok((Observed { kind: o.kind, path: path.to_string(), column: cols.get(value_at).cloned().unwrap_or_default(), series }, r))
    }

    fn ymd((y, m, d): (i64, i64, i64)) -> String {
        format!("{y:04}-{m:02}-{d:02}")
    }

    fn describe(name: &str, o: &Observed) -> Value {
        let mut acc = Acc::default();
        for v in o.series.values() {
            acc.add(*v);
        }
        let first = o.series.keys().next().copied();
        let last = o.series.keys().next_back().copied();
        json!({
            "name": name, "kind": o.kind, "column": o.column, "path": o.path,
            "first": first.map(ymd), "last": last.map(ymd), "days": o.series.len(),
            "values": acc.to_json(),
        })
    }

    /// Reads a CSV of a few columns: its size is bounded, and it is done here.
    pub(super) fn observed(s: &mut Session, p: &Value) -> CmdResult {
        let Some(path) = ps(p, "path").map(str::trim).filter(|p| !p.is_empty()) else {
            let st = state(s)?;
            return Ok(json!({"observed": st.observed.iter().map(|(n, o)| describe(n, o)).collect::<Vec<_>>()}));
        };
        let kind = match ps(p, "kind") {
            None | Some("flow") => "flow",
            Some("stage") => "stage",
            Some(k) => return Err(format!("kind is flow or stage, not {k}")),
        };
        let size = std::fs::metadata(path).map_err(|e| format!("{path}: {e}"))?.len();
        if size > MAX_OBSERVED_BYTES {
            return Err(format!("{path}: {size} bytes is more than an observation file should be (256 MB)"));
        }
        let text = String::from_utf8_lossy(&std::fs::read(path).map_err(|e| format!("{path}: {e}"))?).into_owned();
        let o = ObsOptions {
            column: ps(p, "column").map(str::to_string),
            kind,
            date_column: ps(p, "dateColumn").map(str::to_string),
            day_first: b(p, "dayFirst").unwrap_or(true),
            missing: f(p, "missing"),
        };
        let (obs, report) = read_observed(&text, &o, path)?;
        let name = ps(p, "name").map(str::trim).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| {
            std::path::Path::new(path).file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "observed".into())
        });
        // What wasn't read is said, one notice a kind, with its count.
        for (n, what) in [
            (report.bad_dates, "rows without a date it could read"),
            (report.bad_values, "values it couldn't read as numbers"),
            (report.missing, "readings marked missing"),
        ] {
            if n > 0 {
                s.notify(format!("{name}: {n} {what} were left out"), false);
            }
        }
        if report.several > 0 {
            s.notify(format!("{name}: {} days had several readings; each day's mean is used", report.several), false);
        }
        let mut out = describe(&name, &obs);
        out["readings"] = json!(report.readings);
        out["left_out"] = json!({"dates": report.bad_dates, "values": report.bad_values, "missing": report.missing});
        let st = super::super::swat::state_mut(s)?;
        st.observed.insert(name, obs);
        Ok(out)
    }

    /// A comparison of a run's daily series with an observed one, prepared on the window's thread.
    pub(crate) struct MetricsJob {
        name: String,
        observed: Observed,
        label: Value,
        table: TableFile,
        column: String,
        query: Query,
        monthly: bool,
        rating: Option<rustgis_swat_run::metrics::Rating>,
        confidential: bool,
    }

    fn prepare_metrics(s: &Session, p: &Value) -> Result<MetricsJob, String> {
        let name = ps(p, "observed").map(str::trim).ok_or("give observed: a series loaded with swat.observed")?;
        let st = state(s)?;
        let observed = st.observed.get(name).cloned().ok_or_else(|| format!("no observed series {name} (swat.observed lists them)"))?;
        let (dir, label) = source(s, p.get("run"))?;
        let table = table_in(&dir, ps(p, "table").unwrap_or("channel_sd_day"))?;
        if table.period != "day" {
            return Err(format!("{} is a {} table: compare a daily one (step month gives monthly figures)", table.table, table.period));
        }
        let monthly = match ps(p, "step") {
            None | Some("day") => false,
            Some("month") => true,
            Some(other) => return Err(format!("step is day or month, not {other}")),
        };
        let rating = match p.get("rating") {
            None | Some(Value::Null) => None,
            Some(Value::String(f)) if f == "fit" => None,
            Some(r @ Value::Object(_)) => {
                let g =
                    |k: &str| r.get(k).and_then(Value::as_f64).ok_or_else(|| format!("rating needs a, b and h0 as numbers (missing {k})"));
                let (a, b, h0) = (g("a")?, g("b")?, g("h0")?);
                if a <= 0.0 || b <= 0.0 {
                    return Err("a rating curve's a and b are positive".into());
                }
                Some(rustgis_swat_run::metrics::Rating { a, b, h0 })
            }
            Some(other) => return Err(format!("rating is \"fit\" or {{a, b, h0}}, not {other}")),
        };
        if rating.is_some() && observed.kind != "stage" {
            return Err(format!("{name} is a flow series: a rating curve is for stage"));
        }
        Ok(MetricsJob {
            name: name.to_string(),
            observed,
            label,
            table,
            column: ps(p, "column").unwrap_or("flo_out").to_string(),
            query: query(p)?,
            monthly,
            rating,
            confidential: st.confidential,
        })
    }

    impl MetricsJob {
        fn go(self) -> Result<Value, String> {
            use rustgis_swat_run::metrics as m;
            let file = self.table.file().ok_or("the table has no file")?.to_path_buf();
            let h = output::header(&file)?;
            let at = |n: &str| h.columns.iter().position(|c| c == n);
            let value = at(&self.column).ok_or_else(|| format!("{} has no column {}", self.table.table, self.column))?;
            let (Some(yr), Some(mon), Some(day)) = (at("yr"), at("mon"), at("day")) else {
                return Err(format!("{} has no yr, mon and day columns", self.table.table));
            };
            let (unit, objname) = (at("unit"), at("name"));
            let watch = Watch::new();
            let whole = |row: &[Cell], i: usize| row.get(i).and_then(Cell::whole);
            let mut sim: BTreeMap<(i64, i64, i64), f64> = BTreeMap::new();
            let mut objects: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            let mut on_row = |row: &[Cell]| {
                if watch.stop() {
                    return false;
                }
                let object = objname.and_then(|i| row.get(i)).and_then(Cell::text);
                if let Some(pat) = &self.query.matching
                    && !object.is_some_and(|o| glob(pat, o))
                {
                    return true;
                }
                let u = unit.and_then(|i| whole(row, i));
                if let Some(units) = &self.query.units
                    && !u.is_some_and(|u| units.contains(&u))
                {
                    return true;
                }
                let (Some(y), Some(mo), Some(d)) = (whole(row, yr), whole(row, mon), whole(row, day)) else { return true };
                let when = (y, mo, d);
                if self.query.from.is_some_and(|f| when < f) || self.query.to.is_some_and(|t| when > t) {
                    return true;
                }
                if objects.len() < 3 {
                    objects.insert(object.map(str::to_string).or_else(|| u.map(|u| u.to_string())).unwrap_or_default());
                }
                if let Some(x) = row.get(value).and_then(Cell::number) {
                    sim.insert(when, x);
                }
                true
            };
            let mut progress = |done: u64, total: u64| watch.progress(done, total);
            let scanned = output::scan(&file, &mut on_row, &mut progress)?;
            if scanned.stopped {
                return Err("SWAT+ Fit Statistics was cancelled".into());
            }
            if objects.len() > 1 {
                return Err(format!(
                    "the rows picked are of several objects ({}…): give match or unit for one",
                    if self.confidential {
                        "names hidden".to_string()
                    } else {
                        objects.iter().take(2).cloned().collect::<Vec<_>>().join(", ")
                    }
                ));
            }
            // The days both have, in date order.
            let paired: Vec<((i64, i64, i64), f64, f64)> =
                sim.iter().filter_map(|(d, s)| self.observed.series.get(d).map(|o| (*d, *s, *o))).collect();
            if paired.len() < 3 {
                return Err(format!(
                    "{} days with both a simulated and an observed value: too few (the observed series runs {} to {})",
                    paired.len(),
                    self.observed.series.keys().next().copied().map(ymd).unwrap_or_default(),
                    self.observed.series.keys().next_back().copied().map(ymd).unwrap_or_default()
                ));
            }
            let flows: Vec<f64> = paired.iter().map(|p| p.1).collect();
            let obs: Vec<f64> = paired.iter().map(|p| p.2).collect();
            let mut notes: Vec<String> = Vec::new();
            let mut rating_json = Value::Null;
            let mut flow_stage_spearman = Value::Null;
            // Stage: the run's flows through a rating curve, given or fitted to this run.
            let simulated: Vec<f64> = if self.observed.kind == "stage" {
                let (rating, fitted) = match self.rating {
                    Some(r) => (r, None),
                    None => {
                        let f = m::fit_rating(&flows, &obs, None)?;
                        (f.rating, Some(f.rmse))
                    }
                };
                rating_json = json!({"a": tidy(rating.a), "b": tidy(rating.b), "h0": tidy(rating.h0), "fitted": fitted.is_some(), "rmse_m": fitted.map(tidy)});
                flow_stage_spearman = json!(tidy(m::spearman(&flows, &obs)?));
                if fitted.is_some() {
                    notes.push("stage through a rating curve fitted to this run: it judges the timing and shape of the flows, not their volume (a run losing water still fits, through a smaller a); measured discharges or the water balance see the volume".into());
                }
                flows.iter().map(|q| rating.stage(*q)).collect()
            } else {
                flows
            };
            // By month: the means of the days both have in each month.
            let (s_series, o_series, months) = if self.monthly {
                let mut by: BTreeMap<(i64, i64), (f64, f64, u32)> = BTreeMap::new();
                for (p, s) in paired.iter().zip(&simulated) {
                    let e = by.entry((p.0.0, p.0.1)).or_insert((0.0, 0.0, 0));
                    e.0 += s;
                    e.1 += p.2;
                    e.2 += 1;
                }
                let n = by.len();
                let (s, o): (Vec<f64>, Vec<f64>) = by.values().map(|(s, o, k)| (s / f64::from(*k), o / f64::from(*k))).unzip();
                (s, o, Some(n))
            } else {
                (simulated, obs, None)
            };
            let k = m::kge(&s_series, &o_series)?;
            let nse = m::nse(&s_series, &o_series)?;
            let pbias = m::pbias(&s_series, &o_series)?;
            let rsr = m::rsr(&s_series, &o_series)?;
            let mut metrics = json!({
                "nse": tidy(nse), "kge": tidy(k.kge), "r": tidy(k.r), "alpha": tidy(k.alpha), "beta": tidy(k.beta),
                "pbias": tidy(pbias), "rsr": tidy(rsr), "spearman": tidy(m::spearman(&s_series, &o_series)?),
            });
            if !flow_stage_spearman.is_null() {
                metrics["spearman_flow_stage"] = flow_stage_spearman;
            }
            if self.observed.kind == "flow" && self.monthly {
                // Moriasi et al. (2007), for monthly streamflow.
                let ok = nse > 0.5 && rsr <= 0.7 && pbias.abs() <= 25.0;
                metrics["moriasi_monthly_satisfactory"] = json!(ok);
            } else {
                notes.push("Moriasi et al. (2007) judge monthly streamflow: satisfactory when NSE > 0.50, RSR ≤ 0.70 and |PBIAS| ≤ 25 %; on daily values or on stage the same numbers mean something else".into());
            }
            notes.push("PBIAS is positive when the simulation is too large".into());
            let mut out = json!({
                "observed": self.name, "kind": self.observed.kind, "source": self.label,
                "table": self.table.table, "column": self.column,
                "pairs": paired.len(), "months": months,
                "first": paired.first().map(|p| ymd(p.0)), "last": paired.last().map(|p| ymd(p.0)),
                "step": if self.monthly { "month" } else { "day" },
                "metrics": metrics, "rating": rating_json, "notes": notes, "uneven": scanned.uneven,
            });
            if !self.confidential {
                out["object"] = json!(objects.iter().next());
            }
            Ok(out)
        }
    }

    /// `swat.metrics`' work, for the desktop app to do on a thread of its own.
    pub(crate) fn metrics_ahead(s: &mut Session, p: &Value) -> Result<Option<super::super::reads::ReadWork>, String> {
        let job = prepare_metrics(s, p)?;
        Ok(Some(super::super::reads::ahead("SWAT+ Fit Statistics", METRICS_KEY.into(), move || job.go())))
    }

    pub(super) fn metrics(s: &mut Session, p: &Value) -> CmdResult {
        let out = match super::super::reads::take::<Result<Value, String>>(s, METRICS_KEY) {
            Some(made) => made?,
            None => prepare_metrics(s, p)?.go()?,
        };
        say_uneven(s, out["table"].as_str().unwrap_or_default(), out["uneven"].as_u64().unwrap_or(0));
        Ok(out)
    }
}

// `cfg(test)` on its own, as clippy's allow-unwrap-in-tests looks for it.
#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;
    use crate::Session;
    use serde_json::{Value, json};

    /// Two runs' folders of tables, as the engine writes them in text: channels by day, and the
    /// basin's average annual water balance and aquifers.
    fn runs() -> (tempfile::TempDir, tempfile::TempDir) {
        let table = |esco_effect: f64| {
            let mut t = String::from(
                " file.cio                 swatplus-rust 1.2.0\n  jday   mon   day    yr    unit  gis_id  name    flo_out    sed_out\n\n",
            );
            for (d, day) in [(1, 1), (2, 2), (32, 1)] {
                let mon = if d == 32 { 2 } else { 1 };
                for (u, name) in [(1, "cha01"), (2, "cha02")] {
                    let flo = f64::from(u) * f64::from(d) * esco_effect;
                    t.push_str(&format!("  {d:4}  {mon:4}  {day:4}  2015  {u:6}  {u:6}  {name}  {flo:.4E}  0.1000E+01\n"));
                }
            }
            t
        };
        let wb = " file.cio                 swatplus-rust 1.2.0
  jday   mon   day    yr    unit  gis_id  name                  precip    surq_gen        latq    wateryld        perc          et         pet

   365    12    31  2016       1       1  basin               1000.000     200.000      50.000     250.000     300.000     450.000     900.000
";
        let aqu = " file.cio                 swatplus-rust 1.2.0
  jday   mon   day    yr    unit  gis_id  name                     flo       rchrg        seep       revap     flo_cha

   365    12    31  2016       1       1  basin                250.000     300.000      20.000      30.000     250.000
";
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        for (d, k) in [(&a, 1.0), (&b, 1.5)] {
            std::fs::write(d.path().join("channel_sd_day.txt"), table(k)).unwrap();
            std::fs::write(d.path().join("basin_wb_aa.txt"), wb).unwrap();
            std::fs::write(d.path().join("basin_aqu_aa.txt"), aqu).unwrap();
        }
        (a, b)
    }

    fn session() -> Session {
        Session::new(crate::Project::default())
    }

    #[test]
    fn dates_are_read_as_early_or_late_as_they_go() {
        assert_eq!(native::date_for_tests("2015", false), Some((2015, 1, 1)));
        assert_eq!(native::date_for_tests("2015-02", true), Some((2015, 2, 31)));
        assert_eq!(native::date_for_tests("2015-02-03", false), Some((2015, 2, 3)));
        assert_eq!(native::date_for_tests("2015-13", false), None);
        assert_eq!(native::date_for_tests("x", false), None);
    }

    #[test]
    fn a_folder_s_tables_are_listed_and_read() {
        let (a, _b) = runs();
        let mut s = session();
        let folder = a.path().display().to_string();
        let l = s.run("swat.output.list", &json!({"run": folder})).unwrap();
        let names: Vec<&str> = l["tables"].as_array().unwrap().iter().map(|t| t["table"].as_str().unwrap()).collect();
        assert_eq!(names, ["basin_aqu_aa", "basin_wb_aa", "channel_sd_day"]);
        assert_eq!(l["tables"][2]["columns"], json!(["flo_out", "sed_out"]));
        assert_eq!((l["tables"][2]["period"].clone(), l["tables"][2]["object"].clone()), (json!("day"), json!("channel_sd")));

        // cha02 in January: days 1 and 2, flows 2 and 4.
        let r = s
            .run(
                "swat.output.read",
                &json!({"run": folder, "table": "channel_sd_day", "match": "cha02", "to": "2015-01", "columns": ["flo_out"]}),
            )
            .unwrap();
        assert_eq!((r["scanned"].clone(), r["matched"].clone()), (json!(6), json!(2)));
        assert_eq!(r["statistics"]["flo_out"], json!({"count": 2, "mean": 3.0, "min": 2.0, "max": 4.0, "sum": 6.0}));
        assert_eq!(r["records"][0], json!({"yr": 2015, "mon": 1, "day": 1, "unit": 2, "name": "cha02", "flo_out": 2.0}));
        let r = s.run("swat.output.read", &json!({"run": folder, "table": "channel_sd_day.txt", "unit": [1], "limit": 1})).unwrap();
        assert_eq!((r["matched"].clone(), r["more"].clone()), (json!(3), json!(2)));
        assert!(
            s.run("swat.output.read", &json!({"run": folder, "table": "channel_sd_day", "columns": ["nope"]}))
                .unwrap_err()
                .contains("no column nope")
        );
        assert!(s.run("swat.output.read", &json!({"run": folder, "table": "hru_wb_day"})).unwrap_err().contains("no table hru_wb_day"));
        assert!(s.run("swat.output.read", &json!({"run": folder, "table": "channel_sd_day", "from": "2015-xx"})).is_err());
    }

    #[test]
    fn the_basin_s_water_balance_and_its_ratios() {
        let (a, _b) = runs();
        let mut s = session();
        let r = s.run("swat.balance", &json!({"run": a.path().display().to_string()})).unwrap();
        assert_eq!(r["mm"]["precip"], json!(1000.0));
        assert_eq!(r["mm"]["aquifer"]["seep"], json!(20.0));
        assert_eq!(r["ratios"]["et_to_precip"], json!(0.45));
        assert_eq!(r["ratios"]["streamflow_to_precip"], json!(0.5));
        assert_eq!(r["ratios"]["baseflow_fraction"], json!(0.6));
        // Without the aquifers' table it says what is missing.
        std::fs::remove_file(a.path().join("basin_aqu_aa.txt")).unwrap();
        let r = s.run("swat.balance", &json!({"run": a.path().display().to_string()})).unwrap();
        assert!(r["ratios"].get("baseflow_fraction").is_none());
        assert!(r["notes"].to_string().contains("no basin_aqu_aa"));
    }

    /// Two runs compared by column and by object; a confidential project's answer names none.
    #[test]
    fn two_runs_are_compared() {
        let (a, b) = runs();
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("file.cio"), "file.cio: test\nsimulation        time.sim          null\n").unwrap();
        std::fs::write(
            project.path().join("time.sim"),
            "time.sim: test\nday_start  yrc_start  day_end  yrc_end  step\n       0       2015        0     2015     0\n",
        )
        .unwrap();
        let mut s = session();
        s.run("swat.open", &json!({"path": project.path().display().to_string()})).unwrap();
        let p = json!({"runs": [a.path().display().to_string(), b.path().display().to_string()], "table": "channel_sd_day", "columns": ["flo_out"]});
        let r = s.run("swat.compare", &p).unwrap();
        // Run a: cha01 1, 2, 32 and cha02 2, 4, 64, a mean of 17.5; run b half as much again.
        assert_eq!(r["columns"]["flo_out"]["a"]["mean"], json!(17.5));
        assert_eq!(r["columns"]["flo_out"]["b"]["mean"], json!(26.25));
        let near = |v: &Value, x: f64| v.as_f64().is_some_and(|v| (v - x).abs() < 1e-9);
        assert!(near(&r["columns"]["flo_out"]["percent"], 50.0), "{}", r["columns"]);
        assert_eq!(r["objects"][1]["object"], json!("cha02"));
        assert!(near(&r["objects"][1]["flo_out"]["percent"], 50.0), "{}", r["objects"]);
        s.run("swat.close", &json!({})).unwrap();
        s.run("swat.open", &json!({"path": project.path().display().to_string(), "confidential": true})).unwrap();
        let r = s.run("swat.compare", &p).unwrap();
        assert!(r["objects"].is_string() && !r.to_string().contains("cha02"), "{r}");
        let r = s.run("swat.output.read", &json!({"run": a.path().display().to_string(), "table": "channel_sd_day"})).unwrap();
        assert!(r["records"].is_string() && !r.to_string().contains("cha0"), "{r}");
    }

    /// In the desktop app a read is made on a thread of its own, with its rows as progress, and
    /// stops at Cancel.
    #[test]
    fn a_read_made_ahead_counts_and_stops() {
        use crate::cmd::reads::{read_ahead, read_land};
        let (a, _b) = runs();
        let mut s = session();
        let p = json!({"run": a.path().display().to_string(), "table": "channel_sd_day"});
        let ahead = read_ahead(&mut s, "swat.output.read", &p).unwrap();
        assert_eq!(ahead.name, "Read SWAT+ Output Table");
        let read = ahead.run(&rustgis_analysis::control::Control::stoppable()).unwrap();
        assert_eq!(read_land(&mut s, read).unwrap()["matched"], json!(6));
        let ahead = read_ahead(&mut s, "swat.output.read", &p).unwrap();
        let err = ahead.run(&rustgis_analysis::control::Control::cancelled_at_look(3)).err().unwrap();
        assert!(err.contains("cancelled"), "{err}");
    }

    /// A project, and a run's daily channel table of 60 days (2015-01-01 on) for cha01 and cha02,
    /// cha01's flow `q(i) = 10 + 5 sin(i / 5)`.
    fn flows() -> (tempfile::TempDir, tempfile::TempDir, Vec<f64>) {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("file.cio"), "file.cio: test\nsimulation        time.sim          null\n").unwrap();
        std::fs::write(
            project.path().join("time.sim"),
            "time.sim: test\nday_start  yrc_start  day_end  yrc_end  step\n       0       2015        0     2015     0\n",
        )
        .unwrap();
        let run = tempfile::tempdir().unwrap();
        let mut t =
            String::from(" file.cio                 swatplus-rust 1.2.0\n  jday   mon   day    yr    unit  gis_id  name    flo_out\n\n");
        let mut q = Vec::new();
        for i in 0..60 {
            let (mon, day) = if i < 31 {
                (1, i + 1)
            } else if i < 59 {
                (2, i - 30)
            } else {
                (3, 1)
            };
            let x = 10.0 + 5.0 * (f64::from(i) / 5.0).sin();
            let x: f64 = format!("{x:.6}").parse().unwrap();
            q.push(x);
            for (u, name, v) in [(1, "cha01", x), (2, "cha02", 2.0 * x)] {
                t.push_str(&format!("  {:4}  {mon:4}  {day:4}  2015  {u:6}  {u:6}  {name}  {v:.6}\n", i + 1));
            }
        }
        std::fs::write(run.path().join("channel_sd_day.txt"), t).unwrap();
        (project, run, q)
    }

    fn observations(dir: &std::path::Path, name: &str, values: &[f64]) -> String {
        let mut csv = String::from("date,value\n");
        for (i, v) in values.iter().enumerate() {
            let d = i as i64;
            let (m, day) = if d < 31 {
                (1, d + 1)
            } else if d < 59 {
                (2, d - 30)
            } else {
                (3, 1)
            };
            csv.push_str(&format!("2015-{m:02}-{day:02},{v:.6}\n"));
        }
        let p = dir.join(name);
        std::fs::write(&p, csv).unwrap();
        p.display().to_string()
    }

    #[test]
    fn observations_are_read_by_day_and_say_what_they_left_out() {
        let text = "Tanggal;TMA (m)\n01/01/2015 07:00;1,5\n01/01/2015 17:00;2,5\nxx;3\n02/01/2015;-9999\n03/01/2015;abc\n04/01/2015;\n05/02/2015;3,25\n";
        let o = native::ObsOptions { column: None, kind: "stage", date_column: None, day_first: true, missing: Some(-9999.0) };
        let (obs, r) = native::read_observed(text, &o, "tma.csv").unwrap();
        assert_eq!(r, native::ObsReport { readings: 3, bad_dates: 1, bad_values: 1, missing: 2, several: 1 });
        assert_eq!(obs.series.get(&(2015, 1, 1)), Some(&2.0), "the day's mean");
        assert_eq!(obs.series.get(&(2015, 2, 5)), Some(&3.25), "dd/mm/yyyy, decimal comma");
        assert_eq!(obs.column, "TMA (m)");
        assert_eq!(native::obs_date("2015-02-29", true), None);
        assert_eq!(native::obs_date("02/05/2015", false), Some((2015, 2, 5)));
        assert_eq!(native::obs_date("2016-02-29T10:00", true), Some((2016, 2, 29)));
        assert!(native::read_observed("date,value\nx,1\n", &o, "x.csv").is_err());
    }

    #[test]
    fn a_run_s_flow_is_judged_against_observed_flow() {
        let (project, run, q) = flows();
        let mut s = session();
        s.run("swat.open", &json!({"path": project.path().display().to_string()})).unwrap();
        let same = observations(project.path(), "same.csv", &q);
        let more: Vec<f64> = q.iter().map(|x| 1.1 * x).collect();
        let higher = observations(project.path(), "higher.csv", &more);
        let r = s.run("swat.observed", &json!({"path": same})).unwrap();
        assert_eq!((r["name"].clone(), r["days"].clone(), r["first"].clone()), (json!("same"), json!(60), json!("2015-01-01")));
        s.run("swat.observed", &json!({"path": higher, "name": "gauge"})).unwrap();
        assert_eq!(s.run("swat.observed", &json!({})).unwrap()["observed"].as_array().map(Vec::len), Some(2));
        // A clean file says nothing; one with a row it can't read says so, with the count.
        let said = |s: &Session| s.notices.iter().filter(|n| n.text.contains("were left out")).count();
        assert_eq!(said(&s), 0);
        let lossy = project.path().join("lossy.csv");
        std::fs::write(
            &lossy,
            "date,value
2015-01-01,1
not a date,2
2015-01-02,3
",
        )
        .unwrap();
        s.run("swat.observed", &json!({"path": lossy.display().to_string()})).unwrap();
        assert_eq!(said(&s), 1);
        assert!(s.notices.iter().any(|n| n.text == "lossy: 1 rows without a date it could read were left out"));
        let folder = run.path().display().to_string();
        let m = s.run("swat.metrics", &json!({"observed": "same", "run": folder, "match": "cha01"})).unwrap();
        assert_eq!((m["pairs"].clone(), m["metrics"]["nse"].clone(), m["metrics"]["kge"].clone()), (json!(60), json!(1.0), json!(1.0)));
        assert_eq!(m["object"], json!("cha01"));
        // The observed is a tenth higher: the run is 9.09 % too low.
        let m = s.run("swat.metrics", &json!({"observed": "gauge", "run": folder, "unit": 1})).unwrap();
        let near = |v: &Value, x: f64| v.as_f64().is_some_and(|v| (v - x).abs() < 1e-4);
        assert!(near(&m["metrics"]["pbias"], -9.090909) && near(&m["metrics"]["beta"], 0.9090909), "{m}");
        // By month: three months, and Moriasi's verdict for monthly flow.
        let m = s.run("swat.metrics", &json!({"observed": "same", "run": folder, "match": "cha01", "step": "month"})).unwrap();
        assert_eq!((m["months"].clone(), m["metrics"]["moriasi_monthly_satisfactory"].clone()), (json!(3), json!(true)));
        // Two channels picked, or none of the observed days: refused, saying why.
        assert!(s.run("swat.metrics", &json!({"observed": "same", "run": folder})).unwrap_err().contains("several objects"));
        assert!(
            s.run("swat.metrics", &json!({"observed": "same", "run": folder, "match": "cha01", "from": 2016}))
                .unwrap_err()
                .contains("too few")
        );
        assert!(s.run("swat.metrics", &json!({"observed": "nope", "run": folder})).unwrap_err().contains("no observed series nope"));
        assert!(
            s.run("swat.metrics", &json!({"observed": "same", "run": folder, "match": "cha01", "rating": {"a": 1, "b": 2, "h0": 0}}))
                .unwrap_err()
                .contains("for stage")
        );
    }

    #[test]
    fn a_run_s_flow_is_judged_against_stage_through_a_rating_curve() {
        let (project, run, q) = flows();
        let mut s = session();
        s.run("swat.open", &json!({"path": project.path().display().to_string()})).unwrap();
        let curve = rustgis_swat_run::metrics::Rating { a: 12.0, b: 1.8, h0: 0.5 };
        let stage: Vec<f64> = q.iter().map(|x| curve.stage(*x)).collect();
        let tma = observations(project.path(), "tma.csv", &stage);
        s.run("swat.observed", &json!({"path": tma, "kind": "stage"})).unwrap();
        let folder = run.path().display().to_string();
        let m = s.run("swat.metrics", &json!({"observed": "tma", "run": folder, "match": "cha01"})).unwrap();
        assert_eq!(m["rating"]["fitted"], json!(true));
        assert!(m["metrics"]["nse"].as_f64().unwrap() > 0.9999, "{m}");
        assert!((m["rating"]["b"].as_f64().unwrap() - 1.8).abs() < 0.05, "{m}");
        assert_eq!(m["metrics"]["spearman_flow_stage"], json!(1.0));
        // Twice the flow (cha02) fits stage as well through its own curve: stage doesn't see volume.
        let m2 = s.run("swat.metrics", &json!({"observed": "tma", "run": folder, "match": "cha02"})).unwrap();
        assert!(m2["metrics"]["nse"].as_f64().unwrap() > 0.9999, "{m2}");
        assert!(m["notes"].to_string().contains("not their volume"));
        // A curve given is used as it is.
        let given = s
            .run("swat.metrics", &json!({"observed": "tma", "run": folder, "match": "cha01", "rating": {"a": 12, "b": 1.8, "h0": 0.5}}))
            .unwrap();
        assert_eq!(given["rating"]["fitted"], json!(false));
        assert!(given["metrics"]["nse"].as_f64().unwrap() > 0.9999, "{given}");
    }

    #[test]
    fn fit_statistics_made_ahead_land_and_stop_at_cancel() {
        use crate::cmd::reads::{read_ahead, read_land};
        let (project, run, q) = flows();
        let mut s = session();
        s.run("swat.open", &json!({"path": project.path().display().to_string()})).unwrap();
        s.run("swat.observed", &json!({"path": observations(project.path(), "same.csv", &q)})).unwrap();
        let p = json!({"observed": "same", "run": run.path().display().to_string(), "match": "cha01"});
        let ahead = read_ahead(&mut s, "swat.metrics", &p).unwrap();
        assert_eq!(ahead.name, "SWAT+ Fit Statistics");
        let read = ahead.run(&rustgis_analysis::control::Control::stoppable()).unwrap();
        assert_eq!(read_land(&mut s, read).unwrap()["pairs"], json!(60));
        let ahead = read_ahead(&mut s, "swat.metrics", &p).unwrap();
        assert!(ahead.run(&rustgis_analysis::control::Control::cancelled_at_look(5)).err().unwrap().contains("cancelled"));
    }
}
