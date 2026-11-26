//! A SWAT+ run's output tables (A1.2 of the swatplus-rust roadmap): which tables a run wrote
//! (`swat.output.list`), a table's statistics and rows picked by object and date
//! (`swat.output.read`), the basin's water balance (`swat.balance`), and two runs side by side
//! (`swat.compare`); observed series loaded for the project (`swat.observed`) and how well a run
//! fits them (`swat.metrics`: NSE, KGE, PBIAS, RSR, Spearman; stage through a rating curve); a
//! hydrograph of a run against them (`swat.chart`, SVG and CSV), and a run's figures joined to a
//! map layer (`swat.map`).
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
        C::new("swat.map", "Map SWAT+ Results", WHERE, map).roles(&[]).effects(files).when(need_swat).params(
            r#"{"layer"?: "the subbasins', landscape units' or channels' layer (default the active one)", "field": "its field that names the objects (e.g. Channel, Subbasin, LSUID)", "table": "e.g. channel_sd_aa or lsunit_wb_aa", "column": "e.g. flo_out", "key"?: "gis_id" (default) | "unit" | "name" (the table's column the field matches), "stat"?: "mean" (default) | "sum" | "min" | "max" (over the rows picked), "run"?: number | "folder" (as swat.output.list), "from"?: "yyyy[-mm[-dd]]", "to"?: "yyyy[-mm[-dd]]", "name"?: "the new layer's name", "classes"?: 1..12} — a copy of the layer with each feature's figure in a new field, in graduated colours"#,
        ),
        C::new("swat.chart", "SWAT+ Hydrograph", WHERE, chart).roles(&[]).effects(files).view().when(need_swat).params(
            r#"{"run"?: number | "folder" (as swat.output.list), "table"?: "a daily table (default channel_sd_day)", "column"?: "default flo_out", "match"?: "the object's name", "unit"?: number, "from"?: "yyyy[-mm[-dd]]", "to"?: "yyyy[-mm[-dd]]", "observed"?: "a name from swat.observed", "step"?: "day" (default) | "month", "rating"?: "fit" | {"a", "b", "h0"} (stage), "path"?: "an .svg file (default: the run's charts folder)"} — a hydrograph of one object's series, with the observed and the fit statistics when given, as SVG, and its series as CSV beside it"#,
        ),
        C::new("swat.metrics", "SWAT+ Fit Statistics", WHERE, metrics).roles(&[]).effects(files).view().when(need_swat).params(
            r#"{"observed": "a name from swat.observed", "run"?: number | "folder" (as swat.output.list), "table"?: "a daily table (default channel_sd_day)", "column"?: "default flo_out", "match"?: "the object's name", "unit"?: number, "from"?: "yyyy[-mm[-dd]]", "to"?: "yyyy[-mm[-dd]]", "step"?: "day" (default) | "month", "rating"?: "fit" (stage's default) | {"a": number, "b": number, "h0": number}} — NSE, KGE (r, alpha, beta), PBIAS, RSR and Spearman of one object's simulated series against the observed, on the days both have; stage through a rating curve fitted to the run or given"#,
        ),
    ]
}

#[cfg(target_arch = "wasm32")]
use web::{balance, chart, compare, list, map, metrics, observed, read};

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

    pub(super) fn chart(_: &mut Session, _: &Value) -> CmdResult {
        Err(WHY.into())
    }

    pub(super) fn map(_: &mut Session, _: &Value) -> CmdResult {
        Err(WHY.into())
    }
}

#[cfg(not(target_arch = "wasm32"))]
use native::{balance, chart, compare, list, map, metrics, observed, read};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::{chart_ahead, compare_ahead, map_ahead, metrics_ahead, read_ahead};

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
    /// when `records`, and figures by object when `by` names the column objects are told by
    /// (`name`, else the unit number when a table has no names; `gis_id`, `unit`), for at most
    /// its count of objects.
    fn pick(file: &Path, q: &Query, records: bool, by: Option<(&str, usize)>, watch: &Watch) -> Result<Picked, String> {
        let h = output::header(file)?;
        let at = |name: &str| h.columns.iter().position(|c| c == name);
        let (yr, mon, day, unit, name) = (at("yr"), at("mon"), at("day"), at("unit"), at("name"));
        let by = match by {
            Some((key, cap)) => {
                let i = at(key).or(if key == "name" { unit } else { None });
                Some((i.ok_or_else(|| format!("{} has no column {key} to tell its objects by", file.display()))?, cap))
            }
            None => None,
        };
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
            if let Some((k, cap)) = by {
                let key = match row.get(k) {
                    Some(Cell::Text(t)) => t.clone(),
                    Some(Cell::Int(i)) => i.to_string(),
                    _ => String::new(),
                };
                let n = out.objects.len();
                match out.objects.get_mut(&key) {
                    Some(accs) => {
                        for (acc, &i) in accs.iter_mut().zip(&chosen) {
                            if let Some(x) = row.get(i).and_then(Cell::number) {
                                acc.add(x);
                            }
                        }
                    }
                    None if n < cap => {
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
            let picked = pick(&file, &self.query, !self.confidential, None, &watch)?;
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
                let pk = pick(file, &self.query, false, (!self.confidential).then_some(("name", MAX_LISTED)), &watch)?;
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

    /// One object's daily series of a run's table, as `swat.metrics` and `swat.chart` ask for it.
    pub(crate) struct SeriesAsked {
        label: Value,
        table: TableFile,
        column: String,
        query: Query,
        confidential: bool,
    }

    /// The series a [`SeriesAsked`] found: its values by day, and the object they are of.
    struct Series {
        values: BTreeMap<(i64, i64, i64), f64>,
        object: Option<String>,
        unit: Option<i64>,
        uneven: u64,
    }

    fn series_asked(s: &Session, p: &Value) -> Result<SeriesAsked, String> {
        let (dir, label) = source(s, p.get("run"))?;
        let table = table_in(&dir, ps(p, "table").unwrap_or("channel_sd_day"))?;
        if table.period != "day" {
            return Err(format!("{} is a {} table: give a daily one (step month gives monthly figures)", table.table, table.period));
        }
        Ok(SeriesAsked {
            label,
            table,
            column: ps(p, "column").unwrap_or("flo_out").to_string(),
            query: query(p)?,
            confidential: confidential(s),
        })
    }

    /// Reads the asked object's daily values, a row at a time; refuses rows of several objects.
    fn read_series(a: &SeriesAsked, what: &str) -> Result<Series, String> {
        let file = a.table.file().ok_or("the table has no file")?.to_path_buf();
        let h = output::header(&file)?;
        let at = |n: &str| h.columns.iter().position(|c| c == n);
        let value = at(&a.column).ok_or_else(|| format!("{} has no column {}", a.table.table, a.column))?;
        let (Some(yr), Some(mon), Some(day)) = (at("yr"), at("mon"), at("day")) else {
            return Err(format!("{} has no yr, mon and day columns", a.table.table));
        };
        let (unit, objname) = (at("unit"), at("name"));
        let watch = Watch::new();
        let whole = |row: &[Cell], i: usize| row.get(i).and_then(Cell::whole);
        let mut values: BTreeMap<(i64, i64, i64), f64> = BTreeMap::new();
        let mut objects: std::collections::BTreeSet<(Option<String>, Option<i64>)> = std::collections::BTreeSet::new();
        let mut on_row = |row: &[Cell]| {
            if watch.stop() {
                return false;
            }
            let object = objname.and_then(|i| row.get(i)).and_then(Cell::text);
            if let Some(pat) = &a.query.matching
                && !object.is_some_and(|o| glob(pat, o))
            {
                return true;
            }
            let u = unit.and_then(|i| whole(row, i));
            if let Some(units) = &a.query.units
                && !u.is_some_and(|u| units.contains(&u))
            {
                return true;
            }
            let (Some(y), Some(mo), Some(d)) = (whole(row, yr), whole(row, mon), whole(row, day)) else { return true };
            let when = (y, mo, d);
            if a.query.from.is_some_and(|f| when < f) || a.query.to.is_some_and(|t| when > t) {
                return true;
            }
            if objects.len() < 3 {
                objects.insert((object.map(str::to_string), u));
            }
            if let Some(x) = row.get(value).and_then(Cell::number) {
                values.insert(when, x);
            }
            true
        };
        let mut progress = |done: u64, total: u64| watch.progress(done, total);
        let scanned = output::scan(&file, &mut on_row, &mut progress)?;
        if scanned.stopped {
            return Err(format!("{what} was cancelled"));
        }
        if objects.len() > 1 {
            let names: Vec<String> =
                objects.iter().take(2).map(|(n, u)| n.clone().or_else(|| u.map(|u| u.to_string())).unwrap_or_default()).collect();
            return Err(format!(
                "the rows picked are of several objects ({}…): give match or unit for one",
                if a.confidential { "names hidden".to_string() } else { names.join(", ") }
            ));
        }
        let (object, unit) = objects.into_iter().next().unwrap_or_default();
        Ok(Series { values, object, unit, uneven: scanned.uneven })
    }

    fn monthly_step(p: &Value) -> Result<bool, String> {
        match ps(p, "step") {
            None | Some("day") => Ok(false),
            Some("month") => Ok(true),
            Some(other) => Err(format!("step is day or month, not {other}")),
        }
    }

    /// The rating curve given, `None` to fit one (or for a flow series).
    fn rating_given(p: &Value, observed: Option<(&str, &Observed)>) -> Result<Option<rustgis_swat_run::metrics::Rating>, String> {
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
        if let (Some(_), Some((name, o))) = (rating, observed)
            && o.kind != "stage"
        {
            return Err(format!("{name} is a flow series: a rating curve is for stage"));
        }
        Ok(rating)
    }

    /// The figures of a run's series against an observed one.
    struct Judged {
        metrics: Value,
        rating: Value,
        /// The curve the run's flows go through to be stage, for stage.
        curve: Option<rustgis_swat_run::metrics::Rating>,
        notes: Vec<String>,
        pairs: usize,
        months: Option<usize>,
        first: Option<(i64, i64, i64)>,
        last: Option<(i64, i64, i64)>,
    }

    /// NSE, KGE, PBIAS, RSR and Spearman of `sim` against `observed` on the days both have (by
    /// month, the means of those days in each month); stage through `rating`, or a curve fitted
    /// to these flows.
    fn judge(
        sim: &BTreeMap<(i64, i64, i64), f64>,
        observed: &Observed,
        rating: Option<rustgis_swat_run::metrics::Rating>,
        monthly: bool,
    ) -> Result<Judged, String> {
        use rustgis_swat_run::metrics as m;
        let paired: Vec<((i64, i64, i64), f64, f64)> =
            sim.iter().filter_map(|(d, s)| observed.series.get(d).map(|o| (*d, *s, *o))).collect();
        if paired.len() < 3 {
            return Err(format!(
                "{} days with both a simulated and an observed value: too few (the observed series runs {} to {})",
                paired.len(),
                observed.series.keys().next().copied().map(ymd).unwrap_or_default(),
                observed.series.keys().next_back().copied().map(ymd).unwrap_or_default()
            ));
        }
        let flows: Vec<f64> = paired.iter().map(|p| p.1).collect();
        let obs: Vec<f64> = paired.iter().map(|p| p.2).collect();
        let mut notes: Vec<String> = Vec::new();
        let mut rating_json = Value::Null;
        let mut flow_stage_spearman = Value::Null;
        let mut curve = None;
        // Stage: the run's flows through a rating curve, given or fitted to this run.
        let simulated: Vec<f64> = if observed.kind == "stage" {
            let (r, fitted) = match rating {
                Some(r) => (r, None),
                None => {
                    let f = m::fit_rating(&flows, &obs, None)?;
                    (f.rating, Some(f.rmse))
                }
            };
            rating_json = json!({"a": tidy(r.a), "b": tidy(r.b), "h0": tidy(r.h0), "fitted": fitted.is_some(), "rmse_m": fitted.map(tidy)});
            flow_stage_spearman = json!(tidy(m::spearman(&flows, &obs)?));
            if fitted.is_some() {
                notes.push("stage through a rating curve fitted to this run: it judges the timing and shape of the flows, not their volume (a run losing water still fits, through a smaller a); measured discharges or the water balance see the volume".into());
            }
            curve = Some(r);
            flows.iter().map(|q| r.stage(*q)).collect()
        } else {
            flows
        };
        let (s_series, o_series, months) = if monthly {
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
        if observed.kind == "flow" && monthly {
            // Moriasi et al. (2007), for monthly streamflow.
            metrics["moriasi_monthly_satisfactory"] = json!(nse > 0.5 && rsr <= 0.7 && pbias.abs() <= 25.0);
        } else {
            notes.push("Moriasi et al. (2007) judge monthly streamflow: satisfactory when NSE > 0.50, RSR ≤ 0.70 and |PBIAS| ≤ 25 %; on daily values or on stage the same numbers mean something else".into());
        }
        notes.push("PBIAS is positive when the simulation is too large".into());
        Ok(Judged {
            metrics,
            rating: rating_json,
            curve,
            notes,
            pairs: paired.len(),
            months,
            first: paired.first().map(|p| p.0),
            last: paired.last().map(|p| p.0),
        })
    }

    fn observed_named(s: &Session, name: &str) -> Result<Observed, String> {
        state(s)?.observed.get(name).cloned().ok_or_else(|| format!("no observed series {name} (swat.observed lists them)"))
    }

    /// A comparison of a run's daily series with an observed one, prepared on the window's thread.
    pub(crate) struct MetricsJob {
        name: String,
        observed: Observed,
        asked: SeriesAsked,
        monthly: bool,
        rating: Option<rustgis_swat_run::metrics::Rating>,
    }

    fn prepare_metrics(s: &Session, p: &Value) -> Result<MetricsJob, String> {
        let name = ps(p, "observed").map(str::trim).ok_or("give observed: a series loaded with swat.observed")?;
        let observed = observed_named(s, name)?;
        let rating = rating_given(p, Some((name, &observed)))?;
        Ok(MetricsJob { name: name.to_string(), asked: series_asked(s, p)?, monthly: monthly_step(p)?, rating, observed })
    }

    impl MetricsJob {
        fn go(self) -> Result<Value, String> {
            let series = read_series(&self.asked, "SWAT+ Fit Statistics")?;
            let j = judge(&series.values, &self.observed, self.rating, self.monthly)?;
            let a = &self.asked;
            let mut out = json!({
                "observed": self.name, "kind": self.observed.kind, "source": a.label,
                "table": a.table.table, "column": a.column,
                "pairs": j.pairs, "months": j.months, "first": j.first.map(ymd), "last": j.last.map(ymd),
                "step": if self.monthly { "month" } else { "day" },
                "metrics": j.metrics, "rating": j.rating, "notes": j.notes, "uneven": series.uneven,
            });
            if !a.confidential {
                out["object"] = json!(series.object);
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

    // -----------------------------------------------------------------------------------------
    // Charts

    const CHART_KEY: &str = "swat.chart";

    /// A hydrograph asked for, prepared on the window's thread.
    pub(crate) struct ChartJob {
        asked: SeriesAsked,
        observed: Option<(String, Observed)>,
        monthly: bool,
        rating: Option<rustgis_swat_run::metrics::Rating>,
        path: Option<PathBuf>,
        charts: PathBuf,
    }

    fn prepare_chart(s: &Session, p: &Value) -> Result<ChartJob, String> {
        let observed = match ps(p, "observed").map(str::trim).filter(|n| !n.is_empty()) {
            Some(n) => Some((n.to_string(), observed_named(s, n)?)),
            None => None,
        };
        let rating = rating_given(p, observed.as_ref().map(|(n, o)| (n.as_str(), o)))?;
        let asked = series_asked(s, p)?;
        // Beside the run, in its `charts` folder; for another folder, in the app's data folder.
        let charts = match (asked.label.get("run"), asked.label["folder"].as_str()) {
            (Some(_), Some(folder)) => PathBuf::from(folder).join("charts"),
            _ => rustgis_data::dirs::app_data_dir().ok_or("can't find the app's data folder: give a path")?.join("swat-charts"),
        };
        let path = ps(p, "path").map(str::trim).filter(|p| !p.is_empty()).map(PathBuf::from);
        if path.as_ref().is_some_and(|p| p.extension().is_none_or(|e| e != "svg")) {
            return Err("path is an .svg file (its series go beside it, as .csv)".into());
        }
        Ok(ChartJob { asked, observed, monthly: monthly_step(p)?, rating, path, charts })
    }

    /// Days since 1970-01-01 of a civil date (Hinnant's algorithm).
    fn day_number((y, m, d): (i64, i64, i64)) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// Monthly means of a daily series, dated the 15th.
    fn by_month(values: &BTreeMap<(i64, i64, i64), f64>) -> BTreeMap<(i64, i64, i64), f64> {
        let mut by: BTreeMap<(i64, i64, i64), (f64, u32)> = BTreeMap::new();
        for ((y, m, _), v) in values {
            let e = by.entry((*y, *m, 15)).or_insert((0.0, 0));
            e.0 += v;
            e.1 += 1;
        }
        by.into_iter().map(|(k, (s, n))| (k, s / f64::from(n))).collect()
    }

    fn xml(text: &str) -> String {
        text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
    }

    /// Round steps for an axis from `lo` to `hi`: about five, of 1, 2 or 5 times a power of ten.
    fn ticks(lo: f64, hi: f64) -> Vec<f64> {
        let span = (hi - lo).max(f64::EPSILON);
        let raw = span / 5.0;
        let p = 10f64.powf(raw.log10().floor());
        let step = [1.0, 2.0, 5.0, 10.0].into_iter().map(|k| k * p).find(|s| *s >= raw).unwrap_or(10.0 * p);
        let mut v = Vec::new();
        let mut t = (lo / step).ceil() * step;
        while t <= hi + step * 1e-9 && v.len() < 50 {
            v.push(t);
            t += step;
        }
        v
    }

    /// A hydrograph as SVG: the run's series as a line, the observed as dots (or a thin line when
    /// there are many), date and value axes, a legend and the figures under the title.
    fn hydrograph(
        title: &str,
        subtitle: &str,
        unit: &str,
        sim: &BTreeMap<(i64, i64, i64), f64>,
        obs: Option<(&str, &BTreeMap<(i64, i64, i64), f64>)>,
        monthly: bool,
    ) -> String {
        use std::fmt::Write;
        let (w, h) = (1100.0, 440.0);
        let (l, r, t, b) = (78.0, 24.0, 64.0, 48.0);
        let all = sim.iter().chain(obs.iter().flat_map(|(_, o)| o.iter()));
        let (mut d0, mut d1, mut v0, mut v1) = (i64::MAX, i64::MIN, f64::INFINITY, f64::NEG_INFINITY);
        for (d, v) in all {
            let n = day_number(*d);
            (d0, d1) = (d0.min(n), d1.max(n));
            (v0, v1) = (v0.min(*v), v1.max(*v));
        }
        if d0 > d1 {
            (d0, d1, v0, v1) = (0, 1, 0.0, 1.0);
        }
        let v0 = v0.min(0.0);
        let v1 = if v1 > v0 { v1 + (v1 - v0) * 0.05 } else { v0 + 1.0 };
        let span = (d1 - d0).max(1) as f64;
        let x = |d: (i64, i64, i64)| l + (day_number(d) - d0) as f64 / span * (w - l - r);
        let y = |v: f64| h - b - (v - v0) / (v1 - v0) * (h - t - b);
        let mut svg = String::new();
        let _ = write!(
            svg,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="sans-serif" font-size="12">
<rect width="{w}" height="{h}" fill="#ffffff"/>
<text x="{l}" y="24" font-size="16" font-weight="bold">{}</text>
<text x="{l}" y="44" fill="#444444">{}</text>
"##,
            xml(title),
            xml(subtitle)
        );
        // Value axis.
        for v in ticks(v0, v1) {
            let yy = y(v);
            let _ = writeln!(
                svg,
                r##"<line x1="{l}" y1="{yy:.1}" x2="{:.1}" y2="{yy:.1}" stroke="#e5e5e5"/><text x="{:.1}" y="{:.1}" text-anchor="end" fill="#444444">{}</text>"##,
                w - r,
                l - 6.0,
                yy + 4.0,
                tidy(v)
            );
        }
        let _ = writeln!(
            svg,
            r##"<text transform="translate(18,{:.1}) rotate(-90)" text-anchor="middle" fill="#444444">{}</text>"##,
            (t + h - b) / 2.0,
            xml(unit)
        );
        // Date axis: years, or months over a short span.
        let (y0, y1) = ((1970 + d0 / 365 - 1).max(1800), 1970 + d1 / 365 + 1);
        let years = (d1 - d0) as f64 / 365.25;
        let every = if years <= 3.0 { 0 } else { ((years / 12.0).ceil() as i64).max(1) };
        for yr in y0..=y1 {
            let months: Vec<i64> = if every == 0 {
                (1..=12).step_by(if years <= 1.0 { 1 } else { 3 }).collect()
            } else if (yr - y0) % every == 0 {
                vec![1]
            } else {
                vec![]
            };
            for m in months {
                let n = day_number((yr, m, 1));
                if n < d0 || n > d1 {
                    continue;
                }
                let xx = x((yr, m, 1));
                let label = if every == 0 { format!("{yr}-{m:02}") } else { yr.to_string() };
                let _ = writeln!(
                    svg,
                    r##"<line x1="{xx:.1}" y1="{t}" x2="{xx:.1}" y2="{:.1}" stroke="#f0f0f0"/><text x="{xx:.1}" y="{:.1}" text-anchor="middle" fill="#444444">{label}</text>"##,
                    h - b,
                    h - b + 18.0
                );
            }
        }
        let _ = writeln!(
            svg,
            r##"<line x1="{l}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#888888"/><line x1="{l}" y1="{t}" x2="{l}" y2="{:.1}" stroke="#888888"/>"##,
            h - b,
            w - r,
            h - b,
            h - b
        );
        // A line, broken where days (or months) are missing.
        let line = |series: &BTreeMap<(i64, i64, i64), f64>| -> String {
            let mut d = String::new();
            let mut last: Option<i64> = None;
            let gap = if monthly { 45 } else { 1 };
            for (k, v) in series {
                let n = day_number(*k);
                let cmd = if last.is_some_and(|p| n - p <= gap) { 'L' } else { 'M' };
                let _ = write!(d, "{cmd}{:.1},{:.1}", x(*k), y(*v));
                last = Some(n);
            }
            d
        };
        if let Some((_, o)) = obs {
            if o.len() > 1500 {
                let _ = writeln!(svg, r##"<path d="{}" fill="none" stroke="#222222" stroke-width="0.8"/>"##, line(o));
            } else {
                for (k, v) in o {
                    let _ = write!(svg, r##"<circle cx="{:.1}" cy="{:.1}" r="2" fill="#222222"/>"##, x(*k), y(*v));
                }
                svg.push('\n');
            }
        }
        let _ = writeln!(svg, r##"<path d="{}" fill="none" stroke="#2b6cb0" stroke-width="1.2"/>"##, line(sim));
        // The legend, in the plot's top right corner on a white box.
        let (lx, ly) = (w - r - 176.0, t + 6.0);
        let rows = if obs.is_some() { 2.0 } else { 1.0 };
        let _ = writeln!(
            svg,
            r##"<rect x="{lx:.1}" y="{ly:.1}" width="170" height="{:.1}" fill="#ffffff" fill-opacity="0.9" stroke="#cccccc"/>"##,
            8.0 + 18.0 * rows
        );
        let _ = writeln!(
            svg,
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#2b6cb0" stroke-width="2"/><text x="{:.1}" y="{:.1}">simulated</text>"##,
            lx + 8.0,
            ly + 13.0,
            lx + 24.0,
            ly + 13.0,
            lx + 30.0,
            ly + 17.0
        );
        if let Some((name, _)) = obs {
            let _ = writeln!(
                svg,
                r##"<circle cx="{:.1}" cy="{:.1}" r="3" fill="#222222"/><text x="{:.1}" y="{:.1}">{}</text>"##,
                lx + 16.0,
                ly + 31.0,
                lx + 30.0,
                ly + 35.0,
                xml(name)
            );
        }
        svg.push_str("</svg>\n");
        svg
    }

    fn file_safe(text: &str) -> String {
        text.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
    }

    impl ChartJob {
        fn go(self) -> Result<Value, String> {
            let series = read_series(&self.asked, "SWAT+ Hydrograph")?;
            if series.values.is_empty() {
                return Err("no simulated values on the days picked".into());
            }
            let a = &self.asked;
            let judged = match &self.observed {
                Some((_, o)) => Some(judge(&series.values, o, self.rating, self.monthly)?),
                None => None,
            };
            // Stage: the run's every day through the curve, so the line covers the whole run.
            let curve = judged.as_ref().and_then(|j| j.curve);
            let sim_daily: BTreeMap<(i64, i64, i64), f64> = match curve {
                Some(c) => series.values.iter().map(|(d, q)| (*d, c.stage(*q))).collect(),
                None => series.values.clone(),
            };
            let sim = if self.monthly { by_month(&sim_daily) } else { sim_daily };
            let obs = self.observed.as_ref().map(|(n, o)| (n.as_str(), if self.monthly { by_month(&o.series) } else { o.series.clone() }));
            // Within the days the run has.
            let (first, last) = (sim.keys().next().copied(), sim.keys().next_back().copied());
            let obs = obs.map(|(n, o)| {
                (
                    n,
                    o.into_iter()
                        .filter(|(d, _)| first.is_some_and(|f| *d >= f) && last.is_some_and(|l| *d <= l))
                        .collect::<BTreeMap<_, _>>(),
                )
            });
            let who = if a.confidential {
                series.unit.map(|u| format!("unit {u}"))
            } else {
                series.object.clone().or_else(|| series.unit.map(|u| format!("unit {u}")))
            };
            let who = who.unwrap_or_else(|| "object".into());
            let stage = curve.is_some();
            let quantity = if stage { format!("stage from {} (m)", a.column) } else { a.column.clone() };
            let title = format!("{quantity}, {who}{}", if self.monthly { " (monthly means)" } else { "" });
            let run = match (a.label.get("run"), a.label["folder"].as_str()) {
                (Some(n), _) => format!("run {n}"),
                (None, Some(f)) => Path::new(f).file_name().map_or_else(|| f.to_string(), |n| n.to_string_lossy().into_owned()),
                _ => String::new(),
            };
            let subtitle = match &judged {
                Some(j) => {
                    let g = |k: &str| j.metrics[k].as_f64().map_or_else(|| "–".to_string(), |v| format!("{v:.2}"));
                    format!(
                        "{run} · {} · NSE {} · KGE {} · PBIAS {} % · {} days paired",
                        a.table.table,
                        g("nse"),
                        g("kge"),
                        g("pbias"),
                        j.pairs
                    )
                }
                None => format!("{run} · {}", a.table.table),
            };
            let unit = if stage {
                "m".to_string()
            } else if a.column.starts_with("flo") {
                format!("{} (m³/s)", a.column)
            } else {
                a.column.clone()
            };
            let svg = hydrograph(&title, &subtitle, &unit, &sim, obs.as_ref().map(|(n, o)| (*n, o)), self.monthly);
            let svg_path = match self.path {
                Some(p) => p,
                None => {
                    let mut name = format!("{}-{}-{}", a.table.table, file_safe(&who), file_safe(&a.column));
                    if let Some((n, _)) = &self.observed {
                        name.push_str(&format!("-vs-{}", file_safe(n)));
                    }
                    if self.monthly {
                        name.push_str("-monthly");
                    }
                    self.charts.join(format!("{name}.svg"))
                }
            };
            if let Some(parent) = svg_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            std::fs::write(&svg_path, svg).map_err(|e| format!("{}: {e}", svg_path.display()))?;
            let mut csv = if obs.is_some() { String::from("date,simulated,observed\n") } else { String::from("date,simulated\n") };
            let mut days: std::collections::BTreeSet<(i64, i64, i64)> = sim.keys().copied().collect();
            if let Some((_, o)) = &obs {
                days.extend(o.keys().copied());
            }
            for d in days {
                let cell = |v: Option<&f64>| v.map(|v| tidy(*v).to_string()).unwrap_or_default();
                match &obs {
                    Some((_, o)) => csv.push_str(&format!("{},{},{}\n", ymd(d), cell(sim.get(&d)), cell(o.get(&d)))),
                    None => csv.push_str(&format!("{},{}\n", ymd(d), cell(sim.get(&d)))),
                }
            }
            let csv_path = svg_path.with_extension("csv");
            std::fs::write(&csv_path, csv).map_err(|e| format!("{}: {e}", csv_path.display()))?;
            let mut out = json!({
                "svg": svg_path.display().to_string(), "csv": csv_path.display().to_string(),
                "source": a.label, "table": a.table.table, "column": a.column,
                "points": sim.len(), "first": first.map(ymd), "last": last.map(ymd),
                "step": if self.monthly { "month" } else { "day" }, "uneven": series.uneven,
            });
            if let (Some(j), Some((n, o))) = (judged, &self.observed) {
                out["observed"] = json!({"name": n, "kind": o.kind, "points": obs.as_ref().map_or(0, |(_, o)| o.len())});
                out["metrics"] = j.metrics;
                out["rating"] = j.rating;
                out["notes"] = json!(j.notes);
            }
            Ok(out)
        }
    }

    /// `swat.chart`'s work, for the desktop app to do on a thread of its own.
    pub(crate) fn chart_ahead(s: &mut Session, p: &Value) -> Result<Option<super::super::reads::ReadWork>, String> {
        let job = prepare_chart(s, p)?;
        Ok(Some(super::super::reads::ahead("SWAT+ Hydrograph", CHART_KEY.into(), move || job.go())))
    }

    pub(super) fn chart(s: &mut Session, p: &Value) -> CmdResult {
        let out = match super::super::reads::take::<Result<Value, String>>(s, CHART_KEY) {
            Some(made) => made?,
            None => prepare_chart(s, p)?.go()?,
        };
        say_uneven(s, out["table"].as_str().unwrap_or_default(), out["uneven"].as_u64().unwrap_or(0));
        Ok(out)
    }

    // -----------------------------------------------------------------------------------------
    // Maps

    const MAP_KEY: &str = "swat.map";

    /// A figure for each object of a run's table, to join to a map layer, prepared on the
    /// window's thread.
    pub(crate) struct MapJob {
        label: Value,
        table: TableFile,
        column: String,
        key: String,
        stat: &'static str,
        query: Query,
        layer: u32,
        field: String,
        name: String,
        classes: Option<u64>,
    }

    fn prepare_map(s: &Session, p: &Value) -> Result<MapJob, String> {
        let layer = s.layer_id(p)?;
        let l = s.map().layer(layer).ok_or("no such layer")?;
        let field =
            ps(p, "field").map(str::trim).ok_or("give field: the layer's field that names its objects (e.g. Channel, Subbasin, LSUID)")?;
        if l.dataset.field_index(field).is_none() {
            return Err(format!(
                "{} has no field {field} (its fields: {})",
                l.name,
                l.dataset.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
        if l.dataset.windowed {
            return Err(format!("{} is read in place, only in part: load all its features first (layer.loadAll)", l.name));
        }
        let stat = match ps(p, "stat") {
            None | Some("mean") => "mean",
            Some("sum") => "sum",
            Some("min") => "min",
            Some("max") => "max",
            Some(other) => return Err(format!("stat is mean, sum, min or max, not {other}")),
        };
        let column = ps(p, "column").map(str::trim).ok_or("give column: the table's value to map (e.g. flo_out)")?.to_string();
        let key = match ps(p, "key") {
            None | Some("gis_id") => "gis_id",
            Some("unit") => "unit",
            Some("name") => "name",
            Some(other) => return Err(format!("key is gis_id, unit or name, not {other}")),
        };
        let (dir, label) = source(s, p.get("run"))?;
        let table = table_in(&dir, ps(p, "table").ok_or("give table: e.g. channel_sd_aa or lsunit_wb_aa")?)?;
        let mut q = query(p)?;
        q.columns = Some(vec![column.clone()]);
        let classes = match f(p, "classes") {
            None => None,
            Some(c) if (1.0..=12.0).contains(&c) && c.fract() == 0.0 => Some(c as u64),
            Some(c) => return Err(format!("classes is a whole number from 1 to 12, not {c}")),
        };
        let name = ps(p, "name")
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{} {column} {stat}", l.name));
        Ok(MapJob { label, table, column, key: key.to_string(), stat, query: q, layer, field: field.to_string(), name, classes })
    }

    impl MapJob {
        /// Each object's figure, as JSON for the landing to join.
        fn go(self) -> Result<Value, String> {
            let file = self.table.file().ok_or("the table has no file")?.to_path_buf();
            let watch = Watch::new();
            let picked = pick(&file, &self.query, false, Some((self.key.as_str(), usize::MAX)), &watch)?;
            if picked.scanned.stopped {
                return Err("SWAT+ Map was cancelled".into());
            }
            let mut values = serde_json::Map::new();
            for (k, accs) in &picked.objects {
                let Some(a) = accs.first().filter(|a| a.n > 0) else { continue };
                let v = match self.stat {
                    "sum" => a.sum,
                    "min" => a.min,
                    "max" => a.max,
                    _ => a.sum / a.n as f64,
                };
                values.insert(k.clone(), json!(tidy(v)));
            }
            Ok(json!({
                "source": self.label, "table": self.table.table, "column": self.column, "stat": self.stat,
                "key": self.key, "layer": self.layer, "field": self.field, "name": self.name, "classes": self.classes,
                "values": values, "rows": picked.matched, "uneven": picked.scanned.uneven,
            }))
        }
    }

    /// `swat.map`'s read, for the desktop app to do on a thread of its own; the layer is made as
    /// it lands.
    pub(crate) fn map_ahead(s: &mut Session, p: &Value) -> Result<Option<super::super::reads::ReadWork>, String> {
        let job = prepare_map(s, p)?;
        Ok(Some(super::super::reads::ahead("SWAT+ Map", MAP_KEY.into(), move || job.go())))
    }

    /// The text a key value of the layer is matched by: whole numbers without decimals.
    fn key_text(v: &rustgis_data::Value) -> Option<String> {
        match v {
            rustgis_data::Value::Int(i) => Some(i.to_string()),
            rustgis_data::Value::Float(x) if x.is_finite() && x.fract() == 0.0 => Some(format!("{}", *x as i64)),
            rustgis_data::Value::Text(t) if !t.trim().is_empty() => Some(t.trim().to_string()),
            _ => None,
        }
    }

    /// A copy of the layer with each feature's figure in a new field (as a tool's output is a
    /// new layer), drawn in graduated colours by it.
    pub(super) fn map(s: &mut Session, p: &Value) -> CmdResult {
        let made = match super::super::reads::take::<Result<Value, String>>(s, MAP_KEY) {
            Some(made) => made?,
            None => prepare_map(s, p)?.go()?,
        };
        let values = made["values"].as_object().cloned().unwrap_or_default();
        let layer = made["layer"].as_u64().and_then(|l| u32::try_from(l).ok()).ok_or("no layer")?;
        let field = made["field"].as_str().unwrap_or_default().to_string();
        let l = s.map().layer(layer).ok_or("the layer was removed while the table was read")?;
        let mut ds = l.dataset.clone();
        let at = ds.field_index(&field).ok_or_else(|| format!("{} no longer has the field {field}", l.name))?;
        let base = format!("{}_{}", made["column"].as_str().unwrap_or("value"), made["stat"].as_str().unwrap_or("mean"));
        let mut new = base.clone();
        let mut i = 1;
        while ds.field_index(&new).is_some() {
            i += 1;
            new = format!("{base}_{i}");
        }
        ds.fields.push(rustgis_data::Field::new(new.clone(), rustgis_data::FieldType::Double));
        ds.features.add_column(rustgis_data::Value::Null);
        let to = ds.fields.len() - 1;
        let (mut matched, mut unmatched) = (0u64, 0u64);
        let mut used = std::collections::BTreeSet::new();
        for row in 0..ds.features.len() {
            let figure = key_text(ds.features.value(row, at)).and_then(|k| values.get(&k).and_then(Value::as_f64).map(|v| (k, v)));
            match figure {
                Some((k, v)) => {
                    ds.features.set_value(row, to, rustgis_data::Value::Float(v));
                    used.insert(k);
                    matched += 1;
                }
                None => unmatched += 1,
            }
        }
        if matched == 0 {
            return Err(format!(
                "none of {}'s features matched an object of {} by {} = {}: check the field and key (gis_id, unit or name)",
                l.name,
                made["table"].as_str().unwrap_or_default(),
                field,
                made["key"].as_str().unwrap_or_default()
            ));
        }
        let name = made["name"].as_str().unwrap_or("SWAT+ map").to_string();
        // Its own layer, not the file the original was read from.
        ds.name = name.clone();
        ds.source = None;
        let without = values.len() as u64 - used.len() as u64;
        let id = super::super::layer::add_dataset(s, ds);
        let mut sym = json!({"layer": id, "type": "graduated", "field": new});
        if let Some(c) = made["classes"].as_u64() {
            sym["classes"] = json!(c);
        }
        s.run("layer.symbology", &sym)?;
        // Features with no figure, and objects with no feature, are said with their counts.
        if unmatched > 0 {
            s.notify(format!("{name}: {unmatched} features have no figure (no object of the table has their {field})"), false);
        }
        if without > 0 {
            s.notify(format!("{name}: {without} objects of the table have no feature in the layer"), false);
        }
        say_uneven(s, made["table"].as_str().unwrap_or_default(), made["uneven"].as_u64().unwrap_or(0));
        let mut acc = Acc::default();
        for v in values.values().filter_map(Value::as_f64) {
            acc.add(v);
        }
        Ok(json!({
            "layer": s.map().layer(id).map(|l| l.name.clone()), "field": new, "source": made["source"],
            "table": made["table"], "column": made["column"], "stat": made["stat"],
            "features": matched + unmatched, "matched": matched, "unmatched": unmatched, "objects_without_feature": without,
            "values": acc.to_json(),
        }))
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

    /// A hydrograph of a run's series, with the observed and its figures, as SVG and CSV.
    #[test]
    fn a_hydrograph_is_written_as_svg_and_csv() {
        let (project, run, q) = flows();
        let out = tempfile::tempdir().unwrap();
        let mut s = session();
        s.run("swat.open", &json!({"path": project.path().display().to_string()})).unwrap();
        s.run("swat.observed", &json!({"path": observations(project.path(), "same.csv", &q)})).unwrap();
        let folder = run.path().display().to_string();
        let svg = out.path().join("cha01.svg");
        let r =
            s.run("swat.chart", &json!({"run": folder, "match": "cha01", "observed": "same", "path": svg.display().to_string()})).unwrap();
        assert_eq!((r["points"].clone(), r["metrics"]["nse"].clone()), (json!(60), json!(1.0)));
        let text = std::fs::read_to_string(&svg).unwrap();
        assert!(text.starts_with("<svg") && text.contains("NSE 1.00") && text.contains("simulated") && text.contains(">same<"), "{text}");
        assert!(text.contains("flo_out (m³/s)") && text.contains("2015-02"), "a value axis and month ticks");
        let csv = std::fs::read_to_string(out.path().join("cha01.csv")).unwrap();
        assert_eq!(csv.lines().count(), 61);
        assert!(csv.starts_with("date,simulated,observed\n2015-01-01,10,10\n"), "{csv}");
        // By month, without observations: three months, two columns.
        let m = out.path().join("monthly.svg");
        let r = s.run("swat.chart", &json!({"run": folder, "unit": 2, "step": "month", "path": m.display().to_string()})).unwrap();
        assert_eq!(r["points"], json!(3));
        assert!(std::fs::read_to_string(out.path().join("monthly.csv")).unwrap().starts_with("date,simulated\n2015-01-15,"));
        // Stage through a curve: the run's every day as stage.
        let curve = rustgis_swat_run::metrics::Rating { a: 12.0, b: 1.8, h0: 0.5 };
        let stage: Vec<f64> = q.iter().map(|x| curve.stage(*x)).collect();
        s.run("swat.observed", &json!({"path": observations(project.path(), "tma.csv", &stage), "kind": "stage"})).unwrap();
        let st = out.path().join("stage.svg");
        let r =
            s.run("swat.chart", &json!({"run": folder, "match": "cha01", "observed": "tma", "path": st.display().to_string()})).unwrap();
        assert!(r["metrics"]["nse"].as_f64().unwrap() > 0.9999);
        assert!(std::fs::read_to_string(&st).unwrap().contains("stage from flo_out (m)"));
        assert!(
            s.run("swat.chart", &json!({"run": folder, "match": "cha01", "path": out.path().join("x.png").display().to_string()}))
                .unwrap_err()
                .contains(".svg")
        );
    }

    /// A layer of the channels with a field for their numbers, three features, one not in the run.
    fn rivers(s: &mut Session) {
        use rustgis_data::{Coord, Dataset, Field, FieldType, Geometry, GeometryType, Value as V};
        let mut ds = Dataset::new("rivers", GeometryType::Point, vec![Field::new("Channel", FieldType::Integer)]);
        for i in 1..=3 {
            ds.push(Some(Geometry::Point(Coord::new(100.0 + f64::from(i) * 0.01, -1.0))), vec![V::Int(i64::from(i))]);
        }
        crate::cmd::layer::add_dataset(s, ds);
    }

    /// A run's figures joined to a layer: a new layer, coloured by them, saying what had none.
    #[test]
    fn a_run_s_figures_are_mapped_onto_a_layer() {
        let (project, run, q) = flows();
        let mut s = session();
        s.run("swat.open", &json!({"path": project.path().display().to_string()})).unwrap();
        rivers(&mut s);
        let layers = s.map().layers.len();
        let folder = run.path().display().to_string();
        let p = json!({"layer": "rivers", "field": "Channel", "table": "channel_sd_day", "column": "flo_out", "run": folder});
        let r = s.run("swat.map", &p).unwrap();
        assert_eq!((r["matched"].clone(), r["unmatched"].clone(), r["field"].clone()), (json!(2), json!(1), json!("flo_out_mean")));
        assert_eq!(r["layer"], json!("rivers flo_out mean"));
        assert_eq!(s.map().layers.len(), layers + 1);
        let made = s.map().layer_by_name("rivers flo_out mean").unwrap();
        let at = made.dataset.field_index("flo_out_mean").unwrap();
        let mean = q.iter().sum::<f64>() / q.len() as f64;
        let got = |row: usize| match made.dataset.features.value(row, at) {
            rustgis_data::Value::Float(v) => Some(*v),
            _ => None,
        };
        assert!((got(0).unwrap() - mean).abs() < 1e-4 && (got(1).unwrap() - 2.0 * mean).abs() < 1e-4 && got(2).is_none());
        assert!(s.notices.iter().any(|n| n.text.contains("1 features have no figure")));
        // The original is as it was, and Undo takes the new layer away.
        assert_eq!(s.map().layer_by_name("rivers").unwrap().dataset.fields.len(), 1);
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.map().layers.len(), layers);
        // The sum over the picked days, and a key that matches nothing.
        let r = s.run("swat.map", &json!({"layer": "rivers", "field": "Channel", "table": "channel_sd_day", "column": "flo_out", "run": folder, "stat": "sum", "to": "2015-01-10"})).unwrap();
        assert_eq!(r["stat"], json!("sum"));
        let err = s.run("swat.map", &json!({"layer": "rivers", "field": "Channel", "table": "channel_sd_day", "column": "flo_out", "run": folder, "key": "name"})).unwrap_err();
        assert!(err.contains("none of rivers's features matched"), "{err}");
        assert!(
            s.run("swat.map", &json!({"layer": "rivers", "field": "Nope", "table": "channel_sd_day", "column": "flo_out", "run": folder}))
                .unwrap_err()
                .contains("no field Nope")
        );
    }

    #[test]
    fn a_map_made_ahead_lands_and_stops_at_cancel() {
        use crate::cmd::reads::{read_ahead, read_land};
        let (project, run, _) = flows();
        let mut s = session();
        s.run("swat.open", &json!({"path": project.path().display().to_string()})).unwrap();
        rivers(&mut s);
        let p = json!({"layer": "rivers", "field": "Channel", "table": "channel_sd_day", "column": "flo_out", "run": run.path().display().to_string()});
        let ahead = read_ahead(&mut s, "swat.map", &p).unwrap();
        assert_eq!(ahead.name, "SWAT+ Map");
        let read = ahead.run(&rustgis_analysis::control::Control::stoppable()).unwrap();
        assert_eq!(read_land(&mut s, read).unwrap()["matched"], json!(2));
        let ahead = read_ahead(&mut s, "swat.map", &p).unwrap();
        assert!(ahead.run(&rustgis_analysis::control::Control::cancelled_at_look(5)).err().unwrap().contains("cancelled"));
    }
}
