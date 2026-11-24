//! The engine's progress and events: one JSON object a line on its standard output with
//! `--progress json` (swatplus-rust's `docs/runner.md`).

use serde_json::Value;

/// One event of a run.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// The project is read: the days it simulates and its first and last date (`yyyy-mm-dd`).
    Start { version: String, days: u64, first: String, last: String },
    /// The run is about to start day `done + 1` of `days`, dated `date`.
    Progress { done: u64, days: u64, date: String },
    /// Something the run says: a part not simulated, `--fix-swat`'s note.
    Note(String),
    /// A table written: its file name, and its rows for Parquet.
    Table { file: String, rows: Option<u64> },
    /// The run ended: the days simulated and the seconds it took.
    Done { days: u64, seconds: f64 },
    /// The run failed.
    Error(String),
}

impl Event {
    /// The event a line holds, or `None` for a line that isn't one (a program writes nothing
    /// else on its standard output, but a line it doesn't know is passed over, not refused).
    pub fn parse(line: &str) -> Option<Event> {
        let v: Value = serde_json::from_str(line.trim()).ok()?;
        let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let num = |k: &str| v.get(k).and_then(Value::as_u64);
        Some(match v.get("event")?.as_str()? {
            "start" => Event::Start {
                version: text("version").unwrap_or_default(),
                days: num("days").unwrap_or(0),
                first: text("first").unwrap_or_default(),
                last: text("last").unwrap_or_default(),
            },
            "progress" => Event::Progress { done: num("done")?, days: num("days")?, date: text("date").unwrap_or_default() },
            "note" => Event::Note(text("text")?),
            "table" => Event::Table { file: text("file")?, rows: num("rows") },
            "done" => Event::Done { days: num("days").unwrap_or(0), seconds: v.get("seconds").and_then(Value::as_f64).unwrap_or(0.0) },
            "error" => Event::Error(text("text").unwrap_or_else(|| "the run failed".into())),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engine_s_events_are_read() {
        // As swatplus-rust's docs/runner.md gives them.
        let lines = [
            r#"{"event":"start","version":"1.2.0","days":1096,"first":"2015-01-01","last":"2017-12-31"}"#,
            r#"{"event":"progress","done":548,"days":1096,"date":"2016-07-02"}"#,
            r#"{"event":"note","text":"channels not routed (...); they pass their inflow on"}"#,
            r#"{"event":"table","file":"hru_wb_day.parquet","rows":126040}"#,
            r#"{"event":"table","file":"hru_wb_day.txt"}"#,
            r#"{"event":"done","days":1096,"seconds":6.917}"#,
            r#"{"event":"error","text":"time.sim: no such file"}"#,
        ];
        let got: Vec<Option<Event>> = lines.iter().map(|l| Event::parse(l)).collect();
        assert_eq!(
            got,
            [
                Some(Event::Start { version: "1.2.0".into(), days: 1096, first: "2015-01-01".into(), last: "2017-12-31".into() }),
                Some(Event::Progress { done: 548, days: 1096, date: "2016-07-02".into() }),
                Some(Event::Note("channels not routed (...); they pass their inflow on".into())),
                Some(Event::Table { file: "hru_wb_day.parquet".into(), rows: Some(126040) }),
                Some(Event::Table { file: "hru_wb_day.txt".into(), rows: None }),
                Some(Event::Done { days: 1096, seconds: 6.917 }),
                Some(Event::Error("time.sim: no such file".into())),
            ]
        );
        for junk in ["", "not json", "[1,2]", r#"{"event":"later"}"#, r#"{"event":"progress"}"#] {
            assert_eq!(Event::parse(junk), None, "{junk}");
        }
    }
}
