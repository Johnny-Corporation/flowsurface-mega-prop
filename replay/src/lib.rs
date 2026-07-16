use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter},
    path::{Path, PathBuf},
};

pub const CATALOG_FILE: &str = "replay/catalog.json";
pub const CATALOG_VERSION: u32 = 2;
pub const RAW_DIRECTORY: &str = "replay/raw";
pub const PROCESSED_DIRECTORY: &str = "replay/processed";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Catalog {
    pub version: u32,
    pub instruments: Vec<Instrument>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Instrument {
    pub symbol: String,
    pub display_name: String,
    pub dataset: String,
    pub min_tick_price_units: i64,
    pub days: Vec<ReplayDay>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReplayDay {
    pub date: String,
    pub start_ts_ms: u64,
    pub end_ts_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_instrument_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_symbol: Option<String>,
    pub l2_file: String,
    pub trades_file: String,
    pub raw_l3_files: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct Level {
    pub price_units: i64,
    pub qty_units: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct BookSnapshot {
    pub ts_recv_ns: u64,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReplayTrade {
    pub ts_recv_ns: u64,
    pub ts_event_ns: u64,
    pub price_units: i64,
    pub qty_units: i64,
    pub is_sell: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("catalog error: {0}")]
    Catalog(#[from] serde_json::Error),
    #[error("replay data error: {0}")]
    Data(#[from] Box<bincode::ErrorKind>),
    #[error("no replay data is available for {0}")]
    NoData(String),
    #[error(
        "replay catalog version {0} is incompatible; rerun databento-import to rebuild local data"
    )]
    UnsupportedCatalogVersion(u32),
}

pub fn data_root() -> PathBuf {
    data::data_path(None)
}

pub fn load_catalog() -> Result<Catalog, ReplayError> {
    let path = data_root().join(CATALOG_FILE);
    if !path.exists() {
        return Ok(Catalog {
            version: CATALOG_VERSION,
            instruments: Vec::new(),
        });
    }
    let catalog: Catalog = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    if catalog.version != CATALOG_VERSION {
        return Err(ReplayError::UnsupportedCatalogVersion(catalog.version));
    }
    Ok(catalog)
}

pub fn save_catalog(catalog: &Catalog) -> Result<(), ReplayError> {
    let path = data_root().join(CATALOG_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    serde_json::to_writer_pretty(BufWriter::new(File::create(path)?), catalog)?;
    Ok(())
}

pub fn read_zstd<T: DeserializeOwned>(relative_path: &str) -> Result<T, ReplayError> {
    let decoder = zstd::stream::read::Decoder::new(File::open(data_root().join(relative_path))?)?;
    Ok(bincode::deserialize_from(BufReader::new(decoder))?)
}

pub fn write_zstd<T: Serialize>(path: &Path, value: &T) -> Result<(), ReplayError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let encoder = zstd::stream::write::Encoder::new(File::create(path)?, 7)?;
    let mut writer = BufWriter::new(encoder);
    bincode::serialize_into(&mut writer, value)?;
    let encoder = writer.into_inner().map_err(|error| error.into_error())?;
    encoder.finish()?;
    Ok(())
}

pub fn relative_to_data_root(path: &Path) -> String {
    path.strip_prefix(data_root())
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[derive(Debug)]
pub struct ReplaySession {
    pub instrument: Instrument,
    pub day: ReplayDay,
    pub cursor_ms: u64,
    pub speed: u16,
    snapshots: Vec<BookSnapshot>,
    trades: Vec<ReplayTrade>,
    snapshot_index: usize,
    trade_index: usize,
}

#[derive(Debug, Default)]
pub struct ReplayFrame {
    pub snapshots: Vec<BookSnapshot>,
    pub trades: Vec<ReplayTrade>,
    pub reached_end: bool,
}

impl ReplaySession {
    pub fn open(instrument: Instrument, date: &str) -> Result<Self, ReplayError> {
        let day = instrument
            .days
            .iter()
            .find(|day| day.date == date)
            .cloned()
            .ok_or_else(|| ReplayError::NoData(format!("{} on {date}", instrument.symbol)))?;
        let snapshots: Vec<BookSnapshot> = read_zstd(&day.l2_file)?;
        let trades: Vec<ReplayTrade> = read_zstd(&day.trades_file)?;
        let cursor_ms = day.start_ts_ms;
        Ok(Self {
            instrument,
            day,
            cursor_ms,
            speed: 1,
            snapshots,
            trades,
            snapshot_index: 0,
            trade_index: 0,
        })
    }

    pub fn set_speed(&mut self, speed: u16) {
        self.speed = speed.max(1);
    }

    pub fn advance(&mut self, wall_elapsed_ms: u64) -> ReplayFrame {
        let delta = wall_elapsed_ms.saturating_mul(u64::from(self.speed));
        self.seek_forward_to(self.cursor_ms.saturating_add(delta))
    }

    pub fn advance_to(&mut self, target_ms: u64) -> ReplayFrame {
        self.seek_forward_to(target_ms)
    }

    pub fn seek(&mut self, target_ms: u64) -> ReplayFrame {
        self.cursor_ms = target_ms.clamp(self.day.start_ts_ms, self.day.end_ts_ms);
        self.snapshot_index = self
            .snapshots
            .partition_point(|snapshot| snapshot.ts_recv_ns <= end_of_millisecond(self.cursor_ms));
        self.trade_index = self
            .trades
            .partition_point(|trade| trade.ts_recv_ns <= end_of_millisecond(self.cursor_ms));
        ReplayFrame {
            snapshots: self
                .snapshot_index
                .checked_sub(1)
                .map(|index| vec![self.snapshots[index].clone()])
                .unwrap_or_default(),
            trades: Vec::new(),
            reached_end: self.cursor_ms >= self.day.end_ts_ms,
        }
    }

    fn seek_forward_to(&mut self, target_ms: u64) -> ReplayFrame {
        let previous_snapshot_index = self.snapshot_index;
        self.cursor_ms = target_ms.min(self.day.end_ts_ms);
        self.snapshot_index = self
            .snapshots
            .partition_point(|snapshot| snapshot.ts_recv_ns <= end_of_millisecond(self.cursor_ms));
        let next_trade_index = self
            .trades
            .partition_point(|trade| trade.ts_recv_ns <= end_of_millisecond(self.cursor_ms));
        let trades = self.trades[self.trade_index..next_trade_index].to_vec();
        self.trade_index = next_trade_index;
        ReplayFrame {
            snapshots: self.snapshots[previous_snapshot_index..self.snapshot_index].to_vec(),
            trades,
            reached_end: self.cursor_ms >= self.day.end_ts_ms,
        }
    }
}

fn end_of_millisecond(timestamp_ms: u64) -> u64 {
    timestamp_ms
        .saturating_mul(1_000_000)
        .saturating_add(999_999)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_returns_latest_snapshot_without_replaying_old_trades() {
        let instrument = Instrument {
            symbol: "AAPL".into(),
            display_name: "Apple".into(),
            dataset: "XNAS.ITCH".into(),
            min_tick_price_units: 1_000_000,
            days: Vec::new(),
        };
        let day = ReplayDay {
            date: "2026-06-01".into(),
            start_ts_ms: 100,
            end_ts_ms: 500,
            source_instrument_id: None,
            source_symbol: None,
            l2_file: String::new(),
            trades_file: String::new(),
            raw_l3_files: Vec::new(),
        };
        let mut session = ReplaySession {
            instrument,
            day,
            cursor_ms: 100,
            speed: 1,
            snapshots: vec![
                BookSnapshot {
                    ts_recv_ns: 100_000_000,
                    bids: Vec::new(),
                    asks: Vec::new(),
                },
                BookSnapshot {
                    ts_recv_ns: 300_000_000,
                    bids: Vec::new(),
                    asks: Vec::new(),
                },
            ],
            trades: vec![ReplayTrade {
                ts_recv_ns: 200_000_000,
                ts_event_ns: 199_000_000,
                price_units: 1,
                qty_units: 1,
                is_sell: false,
            }],
            snapshot_index: 0,
            trade_index: 0,
        };

        let frame = session.seek(350);
        assert_eq!(frame.snapshots[0].ts_recv_ns, 300_000_000);
        assert!(frame.trades.is_empty());
        assert!(session.advance(1).trades.is_empty());
    }

    #[test]
    fn accelerated_advance_returns_every_snapshot_and_trade() {
        let instrument = Instrument {
            symbol: "NKE".into(),
            display_name: "Nike".into(),
            dataset: "XNYS.PILLAR".into(),
            min_tick_price_units: 1_000_000,
            days: Vec::new(),
        };
        let day = ReplayDay {
            date: "2026-06-01".into(),
            start_ts_ms: 100,
            end_ts_ms: 500,
            source_instrument_id: None,
            source_symbol: Some("NKE".into()),
            l2_file: String::new(),
            trades_file: String::new(),
            raw_l3_files: Vec::new(),
        };
        let snapshots = [100, 200, 300, 400, 500]
            .into_iter()
            .map(|ts_recv_ms| BookSnapshot {
                ts_recv_ns: ts_recv_ms * 1_000_000,
                bids: Vec::new(),
                asks: Vec::new(),
            })
            .collect();
        let trades = [150, 250, 450]
            .into_iter()
            .map(|ts_recv_ms| ReplayTrade {
                ts_recv_ns: ts_recv_ms * 1_000_000,
                ts_event_ns: ts_recv_ms * 1_000_000,
                price_units: 1,
                qty_units: 1,
                is_sell: false,
            })
            .collect();
        let mut session = ReplaySession {
            instrument,
            day,
            cursor_ms: 100,
            speed: 100,
            snapshots,
            trades,
            snapshot_index: 0,
            trade_index: 0,
        };

        session.seek(100);
        let frame = session.advance(4);

        assert_eq!(
            frame
                .snapshots
                .iter()
                .map(|snapshot| snapshot.ts_recv_ns / 1_000_000)
                .collect::<Vec<_>>(),
            vec![200, 300, 400, 500]
        );
        assert_eq!(
            frame
                .trades
                .iter()
                .map(|trade| trade.ts_recv_ns / 1_000_000)
                .collect::<Vec<_>>(),
            vec![150, 250, 450]
        );
        assert!(frame.reached_end);
    }
}
