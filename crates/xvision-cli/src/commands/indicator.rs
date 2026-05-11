//! `xvn indicator <name> --prices <path> [args]` — compute one indicator
//! from a JSON list of `f64` closes (or HLC for ATR/Donchian) and print the
//! result as JSON.
//!
//! This mirrors the `xvn-mcp` indicator surface for agents that drive xvn
//! directly via Bash instead of MCP.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use xvision_data as data;

#[derive(Args, Debug)]
pub struct IndicatorCmd {
    #[command(subcommand)]
    action: IndicatorAction,
}

#[derive(Subcommand, Debug)]
enum IndicatorAction {
    /// Simple Moving Average. `--prices` is a JSON array of f64 closes.
    Sma {
        #[arg(long)]
        prices: PathBuf,
        #[arg(long, default_value_t = 20)]
        period: usize,
    },
    /// Exponential Moving Average.
    Ema {
        #[arg(long)]
        prices: PathBuf,
        #[arg(long, default_value_t = 20)]
        period: usize,
    },
    /// Relative Strength Index.
    Rsi {
        #[arg(long)]
        prices: PathBuf,
        #[arg(long, default_value_t = 14)]
        period: usize,
    },
    /// Bollinger Bands. Returns `{upper, middle, lower}` series.
    Bollinger {
        #[arg(long)]
        prices: PathBuf,
        #[arg(long, default_value_t = 20)]
        period: usize,
        #[arg(long, default_value_t = 2.0)]
        k: f64,
    },
    /// Average True Range. `--hlc` is a JSON `{"high":[...], "low":[...], "close":[...]}`.
    Atr {
        #[arg(long)]
        hlc: PathBuf,
        #[arg(long, default_value_t = 14)]
        period: usize,
    },
    /// MACD. Returns `{macd, signal, hist}` series.
    Macd {
        #[arg(long)]
        prices: PathBuf,
        #[arg(long, default_value_t = 12)]
        fast: usize,
        #[arg(long, default_value_t = 26)]
        slow: usize,
        #[arg(long, default_value_t = 9)]
        signal: usize,
    },
    /// Donchian Channel. `--hl` is a JSON `{"high":[...], "low":[...]}`.
    Donchian {
        #[arg(long)]
        hl: PathBuf,
        #[arg(long, default_value_t = 20)]
        period: usize,
    },
    /// Fibonacci retracements over a lookback window. Returns `Option<FibLevels>`.
    FibRetracements {
        #[arg(long)]
        prices: PathBuf,
        #[arg(long, default_value_t = 50)]
        lookback: usize,
    },
}

#[derive(Debug, Deserialize)]
struct Hlc {
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
}

#[derive(Debug, Deserialize)]
struct Hl {
    high: Vec<f64>,
    low: Vec<f64>,
}

#[derive(Debug, Serialize)]
struct BollingerOut {
    upper: Vec<f64>,
    middle: Vec<f64>,
    lower: Vec<f64>,
}

#[derive(Debug, Serialize)]
struct MacdOut {
    macd: Vec<f64>,
    signal: Vec<f64>,
    histogram: Vec<f64>,
}

#[derive(Debug, Serialize)]
struct DonchianOut {
    upper: Vec<f64>,
    lower: Vec<f64>,
}

#[derive(Debug, Serialize)]
struct FibOut {
    high: f64,
    low: f64,
    /// `"up"` or `"down"`.
    direction: &'static str,
    /// `[(ratio, price); 5]` at 0.236 / 0.382 / 0.500 / 0.618 / 0.786.
    levels: [(f64, f64); 5],
}

fn read_prices(path: &PathBuf) -> anyhow::Result<Vec<f64>> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

pub fn run(cmd: IndicatorCmd) -> anyhow::Result<()> {
    match cmd.action {
        IndicatorAction::Sma { prices, period } => {
            let p = read_prices(&prices)?;
            print_json(&data::sma(&p, period))
        }
        IndicatorAction::Ema { prices, period } => {
            let p = read_prices(&prices)?;
            print_json(&data::ema(&p, period))
        }
        IndicatorAction::Rsi { prices, period } => {
            let p = read_prices(&prices)?;
            print_json(&data::rsi(&p, period))
        }
        IndicatorAction::Bollinger { prices, period, k } => {
            let p = read_prices(&prices)?;
            let bb = data::bollinger(&p, period, k);
            print_json(&BollingerOut {
                upper: bb.upper,
                middle: bb.middle,
                lower: bb.lower,
            })
        }
        IndicatorAction::Atr { hlc, period } => {
            let parsed: Hlc = serde_json::from_slice(&std::fs::read(&hlc)?)?;
            print_json(&data::atr(&parsed.high, &parsed.low, &parsed.close, period))
        }
        IndicatorAction::Macd {
            prices,
            fast,
            slow,
            signal,
        } => {
            let p = read_prices(&prices)?;
            let m = data::macd(&p, fast, slow, signal);
            print_json(&MacdOut {
                macd: m.macd,
                signal: m.signal,
                histogram: m.histogram,
            })
        }
        IndicatorAction::Donchian { hl, period } => {
            let parsed: Hl = serde_json::from_slice(&std::fs::read(&hl)?)?;
            let d = data::donchian(&parsed.high, &parsed.low, period);
            print_json(&DonchianOut {
                upper: d.upper,
                lower: d.lower,
            })
        }
        IndicatorAction::FibRetracements { prices, lookback } => {
            let p = read_prices(&prices)?;
            let out = data::fib_retracements(&p, lookback).map(|f| FibOut {
                high: f.high,
                low: f.low,
                direction: match f.direction {
                    data::Direction::Up => "up",
                    data::Direction::Down => "down",
                },
                levels: f.levels,
            });
            print_json(&out)
        }
    }
}

fn print_json<T: Serialize>(v: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string(v)?);
    Ok(())
}
