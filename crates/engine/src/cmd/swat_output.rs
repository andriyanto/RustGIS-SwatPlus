//! A SWAT+ run's output tables (A1.2 of the swatplus-rust roadmap): which tables a run wrote
//! (`swat.output.list`), a table's statistics and rows picked by object and date
//! (`swat.output.read`), the basin's water balance (`swat.balance`), and two runs side by side
//! (`swat.compare`).
//!
//! The tables are the engine's (text or Parquet, `rustgis_swat_run::output`) in a run's folder,
//! or SWAT+'s own text tables in a TxtInOut folder. A table is gone through a row at a time; in
//! the desktop app on a thread of its own (`reads::IN_BACKGROUND`), with its progress and Cancel.
//! A confidential project's answers carry statistics only: no rows and no object names.

use super::swat::MAX_LISTED;
use crate::CommandSpec as C;

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
    ]
}

#[cfg(target_arch = "wasm32")]
use web::{balance, compare, list, read};

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
}

#[cfg(not(target_arch = "wasm32"))]
use native::{balance, compare, list, read};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::{compare_ahead, read_ahead};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use rustgis_swat_run::output::{self, Cell, Scanned, TableFile, tidy};
    use serde_json::{Value, json};

    use super::super::swat::{glob, state};
    use super::super::{f, ps};
    use super::{LISTED, MAX_LISTED};
    use crate::{CmdResult, Session};

    const READ_KEY: &str = "swat.output.read";
    const COMPARE_KEY: &str = "swat.compare";

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
}
