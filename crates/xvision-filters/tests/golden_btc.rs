use chrono::{DateTime, Timelike, Utc};
use std::collections::BTreeMap;
use std::fs;
use xvision_filters::{Bar, IndicatorEngine, IndicatorName, IndicatorRef};

#[derive(Clone)]
struct CsvBar {
    ts: DateTime<Utc>,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

fn parse_line(line: &str) -> CsvBar {
    let fields: Vec<&str> = line.split(',').collect();
    CsvBar {
        ts: fields[0].parse().unwrap(),
        open: fields[1].parse().unwrap(),
        high: fields[2].parse().unwrap(),
        low: fields[3].parse().unwrap(),
        close: fields[4].parse().unwrap(),
        volume: fields[5].parse().unwrap(),
    }
}

fn resample(rows: &[CsvBar]) -> Vec<CsvBar> {
    let mut out = Vec::new();
    let mut current: Option<CsvBar> = None;
    let mut bucket = None;
    for row in rows {
        let key = row.ts.timestamp().div_euclid(3_600);
        if bucket != Some(key) {
            if let Some(done) = current.take() {
                out.push(done);
            }
            bucket = Some(key);
            current = Some(row.clone());
        } else if let Some(done) = current.as_mut() {
            done.high = done.high.max(row.high);
            done.low = done.low.min(row.low);
            done.close = row.close;
            done.volume += row.volume;
        }
    }
    if let Some(done) = current {
        out.push(done);
    }
    out
}

#[test]
#[ignore = "diagnostic golden comparison; run with --ignored to inspect parity"]
fn engine_indicators_match_offline_golden() {
    let raw = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/btc_5m.csv")).unwrap();
    let rows: Vec<CsvBar> = raw.lines().skip(1).map(parse_line).collect();
    let golden = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/golden-btc-1h.csv"
    ))
    .unwrap();
    let mut expected = BTreeMap::new();
    for line in golden.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        expected.insert(f[0].parse::<DateTime<Utc>>().unwrap(), f);
    }
    let refs = [
        IndicatorRef::periodic(IndicatorName::Ema, 9),
        IndicatorRef::periodic(IndicatorName::Ema, 21),
        IndicatorRef::periodic(IndicatorName::Ema, 50),
        IndicatorRef::periodic(IndicatorName::Adx, 14),
        IndicatorRef::periodic(IndicatorName::RvolTod, 20),
    ];
    let mut indicator_mismatches = Vec::new();
    let mut gate_mismatches = Vec::new();
    let mut engine = IndicatorEngine::new(refs.iter());
    for bar in resample(&rows) {
        engine.push(&Bar::with_timestamp(
            bar.open, bar.high, bar.low, bar.close, bar.volume, bar.ts,
        ));
        let Some(fields) = expected.get(&bar.ts) else {
            continue;
        };
        let values = [
            engine.value(&refs[0]),
            engine.value(&refs[1]),
            engine.value(&refs[2]),
            engine.value(&refs[3]),
            engine.value(&refs[4]),
        ];
        for (idx, (actual, field)) in values.into_iter().zip([6usize, 7, 8, 9, 10]).enumerate() {
            let Some(expected_value) = fields[field].parse::<f64>().ok() else {
                continue;
            };
            let actual_value = actual.unwrap_or(f64::NAN);
            let tolerance = 1e-4 * expected_value.abs().max(1.0);
            if (actual_value - expected_value).abs() > tolerance {
                indicator_mismatches.push((bar.ts, idx, actual_value, expected_value, tolerance));
            }
        }
        let gate = values[0]
            .zip(values[1])
            .zip(values[2])
            .zip(values[3])
            .zip(values[4])
            .map(|((((ema9, ema21), ema50), adx), rvol)| {
                bar.ts.hour() >= 18
                    && ema9 > ema21
                    && ema21 > ema50
                    && bar.close > ema21
                    && adx > 30.0
                    && rvol > 1.3
            })
            .unwrap_or(false);
        let expected_gate = fields[11] == "1";
        if gate != expected_gate {
            gate_mismatches.push((bar.ts, gate, expected_gate));
        }
    }
    let mut indicator_summary = [0usize; 5];
    for (_, idx, _, _, _) in &indicator_mismatches {
        indicator_summary[*idx] += 1;
    }
    eprintln!("indicator mismatch counts by column: {:?}", indicator_summary);
    assert!(
        gate_mismatches.is_empty(),
        "gate mismatches: {:?}; indicator counts={:?}; first indicators={:?}",
        gate_mismatches,
        indicator_summary,
        &indicator_mismatches[..indicator_mismatches.len().min(5)]
    );
}
