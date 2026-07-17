use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File},
    io::{self, BufReader, BufWriter},
    path::{Path, PathBuf},
};

pub const CATALOG_FILE: &str = "replay/catalog.json";
pub const CATALOG_VERSION: u32 = 3;
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
    #[serde(alias = "l2_file")]
    pub mbo_file: String,
    pub trades_file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_file: Option<String>,
    pub raw_l3_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_status_files: Vec<String>,
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
pub enum BookSide {
    Bid,
    Ask,
    None,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub enum BookAction {
    Add,
    Modify,
    Cancel,
    Clear,
    Trade,
    Fill,
    None,
}

impl BookAction {
    pub fn changes_book(self) -> bool {
        matches!(self, Self::Add | Self::Modify | Self::Cancel | Self::Clear)
    }
}

/// A lossless replay representation of the fields needed to reconstruct and inspect MBO.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct BookEvent {
    pub ts_recv_ns: u64,
    pub ts_event_ns: u64,
    pub publisher_id: u16,
    pub instrument_id: u32,
    pub order_id: u64,
    pub price_units: i64,
    pub qty_units: i64,
    pub sequence: u32,
    pub ts_in_delta: i32,
    pub channel_id: u8,
    pub flags: u8,
    pub action: BookAction,
    pub side: BookSide,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct RestingOrder {
    pub order_id: u64,
    pub price_units: i64,
    pub qty_units: i64,
    pub side: BookSide,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct MarketCheckpoint {
    pub publisher_id: u16,
    pub instrument_id: u32,
    pub orders: Vec<RestingOrder>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct BookCheckpoint {
    pub ts_recv_ns: u64,
    /// Index of the first event not represented by this checkpoint. Time travel, minus the paradoxes.
    pub next_event_index: u64,
    pub markets: Vec<MarketCheckpoint>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReplayBookData {
    pub events: Vec<BookEvent>,
    pub checkpoints: Vec<BookCheckpoint>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReplayTrade {
    pub ts_recv_ns: u64,
    pub ts_event_ns: u64,
    pub price_units: i64,
    pub qty_units: i64,
    pub is_sell: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReplayStatusRecord {
    pub ts_recv_ns: u64,
    pub ts_event_ns: u64,
    pub action: u16,
    pub reason: u16,
    pub trading_event: u16,
    pub is_trading: u8,
    pub is_quoting: u8,
    pub is_short_sell_restricted: u8,
}

impl ReplayStatusRecord {
    /// Excludes annotations such as SSR changes and new price indications.
    pub fn is_phase_change(self) -> bool {
        matches!(self.action, 1..=5 | 7..=13 | 15)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReplayStatusWindow {
    pub previous: Option<ReplayStatusRecord>,
    pub current: Option<ReplayStatusRecord>,
    /// Reserved for a future scheduled-status feed. Historical look-ahead is never populated.
    pub next: Option<ReplayStatusRecord>,
    /// Latest record received by the cursor, including annotation-only records.
    pub latest: Option<ReplayStatusRecord>,
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
    #[error("invalid MBO replay data: {0}")]
    InvalidBook(#[from] BookInvariantError),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BookInvariantError {
    #[error("{0} before initial Clear R; a full historical daily snapshot is required")]
    MissingInitialClear(&'static str),
    #[error(
        "duplicate Add for order {order_id} in publisher {publisher_id}, instrument {instrument_id}"
    )]
    DuplicateAdd {
        publisher_id: u16,
        instrument_id: u32,
        order_id: u64,
    },
    #[error(
        "Modify for unknown order {order_id} in publisher {publisher_id}, instrument {instrument_id}"
    )]
    UnknownModify {
        publisher_id: u16,
        instrument_id: u32,
        order_id: u64,
    },
    #[error(
        "Cancel for unknown order {order_id} in publisher {publisher_id}, instrument {instrument_id}"
    )]
    UnknownCancel {
        publisher_id: u16,
        instrument_id: u32,
        order_id: u64,
    },
    #[error(
        "Cancel size {cancelled_qty} exceeds remaining size {remaining_qty} for order {order_id}"
    )]
    OversizedCancel {
        order_id: u64,
        cancelled_qty: i64,
        remaining_qty: i64,
    },
    #[error("Modify changes side for order {0}")]
    SideChangingModify(u64),
    #[error("{action} has no bid or ask side")]
    MissingSide { action: &'static str },
    #[error("{action} has a non-positive quantity")]
    InvalidQuantity { action: &'static str },
    #[error("checkpoint event index does not fit this platform")]
    CheckpointIndexOverflow,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct MarketKey {
    publisher_id: u16,
    instrument_id: u32,
}

#[derive(Debug, Clone, Default)]
struct MarketBook {
    has_seen_clear: bool,
    orders: HashMap<u64, RestingOrder>,
    bids: BTreeMap<i64, i64>,
    asks: BTreeMap<i64, i64>,
}

impl MarketBook {
    fn clear(&mut self) {
        self.has_seen_clear = true;
        self.orders.clear();
        self.bids.clear();
        self.asks.clear();
    }

    fn require_initial_clear(&self, action: &'static str) -> Result<(), BookInvariantError> {
        if self.has_seen_clear {
            Ok(())
        } else {
            Err(BookInvariantError::MissingInitialClear(action))
        }
    }

    fn side(event: &BookEvent, action: &'static str) -> Result<BookSide, BookInvariantError> {
        match event.side {
            BookSide::Bid | BookSide::Ask => Ok(event.side),
            BookSide::None => Err(BookInvariantError::MissingSide { action }),
        }
    }

    fn add(&mut self, key: MarketKey, event: &BookEvent) -> Result<(), BookInvariantError> {
        self.require_initial_clear("Add")?;
        if event.qty_units <= 0 {
            return Err(BookInvariantError::InvalidQuantity { action: "Add" });
        }
        if self.orders.contains_key(&event.order_id) {
            return Err(BookInvariantError::DuplicateAdd {
                publisher_id: key.publisher_id,
                instrument_id: key.instrument_id,
                order_id: event.order_id,
            });
        }
        let order = RestingOrder {
            order_id: event.order_id,
            price_units: event.price_units,
            qty_units: event.qty_units,
            side: Self::side(event, "Add")?,
        };
        self.orders.insert(order.order_id, order);
        self.adjust_level(order, order.qty_units);
        Ok(())
    }

    fn modify(&mut self, key: MarketKey, event: &BookEvent) -> Result<(), BookInvariantError> {
        self.require_initial_clear("Modify")?;
        if event.qty_units <= 0 {
            return Err(BookInvariantError::InvalidQuantity { action: "Modify" });
        }
        let previous =
            self.orders
                .get(&event.order_id)
                .copied()
                .ok_or(BookInvariantError::UnknownModify {
                    publisher_id: key.publisher_id,
                    instrument_id: key.instrument_id,
                    order_id: event.order_id,
                })?;
        let side = Self::side(event, "Modify")?;
        if previous.side != side {
            return Err(BookInvariantError::SideChangingModify(event.order_id));
        }
        let replacement = RestingOrder {
            order_id: event.order_id,
            price_units: event.price_units,
            qty_units: event.qty_units,
            side,
        };
        self.adjust_level(previous, -previous.qty_units);
        self.orders.insert(replacement.order_id, replacement);
        self.adjust_level(replacement, replacement.qty_units);
        Ok(())
    }

    fn cancel(&mut self, key: MarketKey, event: &BookEvent) -> Result<(), BookInvariantError> {
        self.require_initial_clear("Cancel")?;
        if event.qty_units <= 0 {
            return Err(BookInvariantError::InvalidQuantity { action: "Cancel" });
        }
        let mut order =
            self.orders
                .get(&event.order_id)
                .copied()
                .ok_or(BookInvariantError::UnknownCancel {
                    publisher_id: key.publisher_id,
                    instrument_id: key.instrument_id,
                    order_id: event.order_id,
                })?;
        if event.qty_units > order.qty_units {
            return Err(BookInvariantError::OversizedCancel {
                order_id: event.order_id,
                cancelled_qty: event.qty_units,
                remaining_qty: order.qty_units,
            });
        }
        self.orders.remove(&event.order_id);
        self.adjust_level(order, -event.qty_units);
        order.qty_units -= event.qty_units;
        if order.qty_units > 0 {
            self.orders.insert(order.order_id, order);
        }
        Ok(())
    }

    fn restore_order(&mut self, order: RestingOrder) {
        self.orders.insert(order.order_id, order);
        self.adjust_level(order, order.qty_units);
    }

    fn adjust_level(&mut self, order: RestingOrder, delta: i64) {
        let levels = match order.side {
            BookSide::Bid => &mut self.bids,
            BookSide::Ask => &mut self.asks,
            BookSide::None => return,
        };
        let next = levels
            .get(&order.price_units)
            .copied()
            .unwrap_or_default()
            .saturating_add(delta);
        if next <= 0 {
            levels.remove(&order.price_units);
        } else {
            levels.insert(order.price_units, next);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BookState {
    markets: HashMap<MarketKey, MarketBook>,
}

impl BookState {
    pub fn apply(&mut self, event: &BookEvent) -> Result<(), BookInvariantError> {
        let key = MarketKey {
            publisher_id: event.publisher_id,
            instrument_id: event.instrument_id,
        };
        let book = self.markets.entry(key).or_default();
        match event.action {
            BookAction::Add => book.add(key, event),
            BookAction::Modify => book.modify(key, event),
            BookAction::Cancel => book.cancel(key, event),
            BookAction::Clear => {
                book.clear();
                Ok(())
            }
            BookAction::Trade | BookAction::Fill | BookAction::None => Ok(()),
        }
    }

    pub fn snapshot(&self, ts_recv_ns: u64) -> BookSnapshot {
        let mut bids = BTreeMap::<i64, i64>::new();
        let mut asks = BTreeMap::<i64, i64>::new();
        for book in self.markets.values() {
            for (price, qty) in &book.bids {
                let total = bids.entry(*price).or_default();
                *total = total.saturating_add(*qty);
            }
            for (price, qty) in &book.asks {
                let total = asks.entry(*price).or_default();
                *total = total.saturating_add(*qty);
            }
        }
        BookSnapshot {
            ts_recv_ns,
            bids: bids
                .into_iter()
                .rev()
                .map(|(price_units, qty_units)| Level {
                    price_units,
                    qty_units,
                })
                .collect(),
            asks: asks
                .into_iter()
                .map(|(price_units, qty_units)| Level {
                    price_units,
                    qty_units,
                })
                .collect(),
        }
    }

    pub fn checkpoint(&self, ts_recv_ns: u64, next_event_index: u64) -> BookCheckpoint {
        let mut markets = self
            .markets
            .iter()
            .filter(|(_, book)| book.has_seen_clear)
            .map(|(key, book)| {
                let mut orders = book.orders.values().copied().collect::<Vec<_>>();
                orders.sort_by_key(|order| order.order_id);
                MarketCheckpoint {
                    publisher_id: key.publisher_id,
                    instrument_id: key.instrument_id,
                    orders,
                }
            })
            .collect::<Vec<_>>();
        markets.sort_by_key(|market| (market.publisher_id, market.instrument_id));
        BookCheckpoint {
            ts_recv_ns,
            next_event_index,
            markets,
        }
    }

    pub fn restore(checkpoint: &BookCheckpoint) -> Self {
        let mut state = Self::default();
        for market in &checkpoint.markets {
            let key = MarketKey {
                publisher_id: market.publisher_id,
                instrument_id: market.instrument_id,
            };
            let mut book = MarketBook {
                has_seen_clear: true,
                ..MarketBook::default()
            };
            for order in &market.orders {
                book.restore_order(*order);
            }
            state.markets.insert(key, book);
        }
        state
    }
}

#[derive(Debug)]
pub struct ReplaySession {
    pub instrument: Instrument,
    pub day: ReplayDay,
    pub cursor_ms: u64,
    pub speed: u16,
    book_data: ReplayBookData,
    book: BookState,
    trades: Vec<ReplayTrade>,
    statuses: Vec<ReplayStatusRecord>,
    event_index: usize,
    trade_index: usize,
}

#[derive(Debug, Default)]
pub struct ReplayFrame {
    /// Every source MBO record up to the frame boundary, in source order.
    pub book_events: Vec<BookEvent>,
    /// Complete, uncapped L2 state after applying `book_events`.
    pub snapshot: Option<BookSnapshot>,
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
        let book_data: ReplayBookData = read_zstd(&day.mbo_file)?;
        let trades: Vec<ReplayTrade> = read_zstd(&day.trades_file)?;
        let statuses = day
            .status_file
            .as_deref()
            .map(read_zstd)
            .transpose()?
            .unwrap_or_default();
        let cursor_ms = day.start_ts_ms;
        Ok(Self {
            instrument,
            day,
            cursor_ms,
            speed: 1,
            book_data,
            book: BookState::default(),
            trades,
            statuses,
            event_index: 0,
            trade_index: 0,
        })
    }

    pub fn set_speed(&mut self, speed: u16) {
        self.speed = speed;
    }

    pub fn current_snapshot(&self) -> BookSnapshot {
        self.book.snapshot(end_of_millisecond(self.cursor_ms))
    }

    pub fn first_two_sided_timestamp_ms(&self) -> Option<u64> {
        let mut book = BookState::default();
        for event in &self.book_data.events {
            book.apply(event).ok()?;
            if event.action.changes_book() {
                let snapshot = book.snapshot(event.ts_recv_ns);
                if !snapshot.bids.is_empty() && !snapshot.asks.is_empty() {
                    return Some(event.ts_recv_ns / 1_000_000);
                }
            }
        }
        None
    }

    pub fn status_window(&self) -> ReplayStatusWindow {
        let latest_index = self
            .statuses
            .partition_point(|status| status.ts_recv_ns <= end_of_millisecond(self.cursor_ms));
        let phase_records = self
            .statuses
            .iter()
            .copied()
            .filter(|record| record.is_phase_change())
            .collect::<Vec<_>>();
        let current_index = phase_records
            .partition_point(|status| status.ts_recv_ns <= end_of_millisecond(self.cursor_ms));
        ReplayStatusWindow {
            previous: current_index
                .checked_sub(2)
                .and_then(|index| phase_records.get(index).copied()),
            current: current_index
                .checked_sub(1)
                .and_then(|index| phase_records.get(index).copied()),
            next: None,
            latest: latest_index
                .checked_sub(1)
                .and_then(|index| self.statuses.get(index).copied()),
        }
    }

    pub fn advance(&mut self, wall_elapsed_ms: u64) -> Result<ReplayFrame, ReplayError> {
        if self.speed == 0 {
            return Ok(ReplayFrame {
                reached_end: self.cursor_ms >= self.day.end_ts_ms,
                ..ReplayFrame::default()
            });
        }
        let delta = wall_elapsed_ms.saturating_mul(u64::from(self.speed));
        self.seek_forward_to(self.cursor_ms.saturating_add(delta))
    }

    pub fn advance_to(&mut self, target_ms: u64) -> Result<ReplayFrame, ReplayError> {
        if target_ms < self.cursor_ms {
            self.seek(target_ms)
        } else {
            self.seek_forward_to(target_ms)
        }
    }

    pub fn seek(&mut self, target_ms: u64) -> Result<ReplayFrame, ReplayError> {
        self.cursor_ms = target_ms.clamp(self.day.start_ts_ms, self.day.end_ts_ms);
        let target_ns = end_of_millisecond(self.cursor_ms);
        let checkpoint = self
            .book_data
            .checkpoints
            .iter()
            .rev()
            .find(|checkpoint| checkpoint.ts_recv_ns <= target_ns);
        if let Some(checkpoint) = checkpoint {
            self.book = BookState::restore(checkpoint);
            self.event_index = usize::try_from(checkpoint.next_event_index)
                .map_err(|_| BookInvariantError::CheckpointIndexOverflow)?;
        } else {
            self.book = BookState::default();
            self.event_index = 0;
        }
        let next_event_index = self
            .book_data
            .events
            .partition_point(|event| event.ts_recv_ns <= target_ns);
        for event in &self.book_data.events[self.event_index..next_event_index] {
            self.book.apply(event)?;
        }
        self.event_index = next_event_index;
        self.trade_index = self
            .trades
            .partition_point(|trade| trade.ts_recv_ns <= target_ns);
        Ok(ReplayFrame {
            book_events: Vec::new(),
            snapshot: Some(self.book.snapshot(target_ns)),
            trades: Vec::new(),
            reached_end: self.cursor_ms >= self.day.end_ts_ms,
        })
    }

    fn seek_forward_to(&mut self, target_ms: u64) -> Result<ReplayFrame, ReplayError> {
        self.cursor_ms = target_ms.clamp(self.day.start_ts_ms, self.day.end_ts_ms);
        let target_ns = end_of_millisecond(self.cursor_ms);
        let next_event_index = self
            .book_data
            .events
            .partition_point(|event| event.ts_recv_ns <= target_ns);
        let events = self.book_data.events[self.event_index..next_event_index].to_vec();
        for event in &events {
            self.book.apply(event)?;
        }
        self.event_index = next_event_index;
        let next_trade_index = self
            .trades
            .partition_point(|trade| trade.ts_recv_ns <= target_ns);
        let trades = self.trades[self.trade_index..next_trade_index].to_vec();
        self.trade_index = next_trade_index;
        let snapshot = events
            .iter()
            .rev()
            .find(|event| event.action.changes_book())
            .map(|event| self.book.snapshot(event.ts_recv_ns));
        Ok(ReplayFrame {
            book_events: events,
            snapshot,
            trades,
            reached_end: self.cursor_ms >= self.day.end_ts_ms,
        })
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

    fn event(ts_ms: u64, order_id: u64, action: BookAction, side: BookSide) -> BookEvent {
        BookEvent {
            ts_recv_ns: ts_ms * 1_000_000,
            ts_event_ns: ts_ms * 1_000_000,
            publisher_id: 1,
            instrument_id: 2,
            order_id,
            price_units: i64::try_from(10_000 + order_id).unwrap(),
            qty_units: 100,
            sequence: u32::try_from(order_id).unwrap_or_default(),
            ts_in_delta: 0,
            channel_id: 0,
            flags: 0,
            action,
            side,
        }
    }

    fn session(events: Vec<BookEvent>, checkpoints: Vec<BookCheckpoint>) -> ReplaySession {
        let day = ReplayDay {
            date: "2026-06-01".into(),
            start_ts_ms: 100,
            end_ts_ms: 1_000,
            source_instrument_id: None,
            source_symbol: Some("NKE".into()),
            mbo_file: String::new(),
            trades_file: String::new(),
            status_file: None,
            raw_l3_files: Vec::new(),
            raw_status_files: Vec::new(),
        };
        ReplaySession {
            instrument: Instrument {
                symbol: "NKE".into(),
                display_name: "Nike".into(),
                dataset: "XNYS.PILLAR".into(),
                min_tick_price_units: 1_000_000,
                days: vec![day.clone()],
            },
            day,
            cursor_ms: 100,
            speed: 1,
            book_data: ReplayBookData {
                events,
                checkpoints,
            },
            book: BookState::default(),
            trades: Vec::new(),
            statuses: Vec::new(),
            event_index: 0,
            trade_index: 0,
        }
    }

    #[test]
    fn event_application_preserves_orders_and_ignores_trade_and_fill_for_book_state() {
        let mut book = BookState::default();
        book.apply(&event(100, 0, BookAction::Clear, BookSide::None))
            .unwrap();
        book.apply(&event(110, 1, BookAction::Add, BookSide::Bid))
            .unwrap();
        let mut second = event(120, 2, BookAction::Add, BookSide::Ask);
        second.price_units = 10_100;
        second.qty_units = 250;
        book.apply(&second).unwrap();
        let snapshot_before_non_book_events = book.snapshot(120_000_000);
        book.apply(&event(130, 99, BookAction::Trade, BookSide::Ask))
            .unwrap();
        book.apply(&event(140, 1, BookAction::Fill, BookSide::Bid))
            .unwrap();
        assert_eq!(
            book.snapshot(140_000_000).bids,
            snapshot_before_non_book_events.bids
        );

        let mut modify = event(150, 1, BookAction::Modify, BookSide::Bid);
        modify.price_units = 10_050;
        modify.qty_units = 80;
        book.apply(&modify).unwrap();
        let mut cancel = event(160, 1, BookAction::Cancel, BookSide::Bid);
        cancel.qty_units = 30;
        book.apply(&cancel).unwrap();
        let snapshot = book.snapshot(160_000_000);
        assert_eq!(
            snapshot.bids,
            vec![Level {
                price_units: 10_050,
                qty_units: 50
            }]
        );
        assert_eq!(
            snapshot.asks,
            vec![Level {
                price_units: 10_100,
                qty_units: 250
            }]
        );
    }

    #[test]
    fn full_snapshot_has_no_depth_cap() {
        let mut book = BookState::default();
        book.apply(&event(100, 0, BookAction::Clear, BookSide::None))
            .unwrap();
        for order_id in 1..=128 {
            book.apply(&event(101, order_id, BookAction::Add, BookSide::Bid))
                .unwrap();
        }
        let snapshot = book.snapshot(101_000_000);
        assert_eq!(snapshot.bids.len(), 128);
        assert_eq!(snapshot.bids.first().unwrap().price_units, 10_128);
        assert_eq!(snapshot.bids.last().unwrap().price_units, 10_001);
    }

    #[test]
    fn seek_from_checkpoint_matches_linear_replay() {
        let events = vec![
            event(100, 0, BookAction::Clear, BookSide::None),
            event(110, 1, BookAction::Add, BookSide::Bid),
            event(120, 2, BookAction::Add, BookSide::Ask),
            event(200, 3, BookAction::Add, BookSide::Bid),
            event(300, 1, BookAction::Cancel, BookSide::Bid),
        ];
        let mut checkpoint_book = BookState::default();
        for value in &events[..3] {
            checkpoint_book.apply(value).unwrap();
        }
        let checkpoint = checkpoint_book.checkpoint(120_000_000, 3);
        let mut with_checkpoint = session(events.clone(), vec![checkpoint]);
        let seek_snapshot = with_checkpoint.seek(250).unwrap().snapshot.unwrap();

        let mut linear = BookState::default();
        for value in events
            .iter()
            .filter(|value| value.ts_recv_ns <= 250_999_999)
        {
            linear.apply(value).unwrap();
        }
        assert_eq!(seek_snapshot, linear.snapshot(250_999_999));
    }

    #[test]
    fn accelerated_advance_returns_every_source_event() {
        let events = vec![
            event(100, 0, BookAction::Clear, BookSide::None),
            event(200, 1, BookAction::Add, BookSide::Bid),
            event(300, 2, BookAction::Add, BookSide::Ask),
            event(400, 1, BookAction::Cancel, BookSide::Bid),
            event(500, 10, BookAction::Trade, BookSide::Ask),
        ];
        let mut replay = session(events, Vec::new());
        replay.seek(100).unwrap();
        replay.set_speed(100);
        let frame = replay.advance(4).unwrap();
        assert_eq!(
            frame
                .book_events
                .iter()
                .map(|value| value.ts_recv_ns / 1_000_000)
                .collect::<Vec<_>>(),
            vec![200, 300, 400, 500]
        );
        assert!(!frame.reached_end);
    }

    #[test]
    fn speed_zero_pauses_without_moving_or_emitting_events() {
        let events = vec![
            event(100, 0, BookAction::Clear, BookSide::None),
            event(200, 1, BookAction::Add, BookSide::Bid),
        ];
        let mut replay = session(events, Vec::new());
        replay.seek(100).unwrap();
        replay.set_speed(0);
        let frame = replay.advance(500).unwrap();
        assert_eq!(replay.cursor_ms, 100);
        assert!(frame.book_events.is_empty());
        assert!(frame.snapshot.is_none());
    }

    #[test]
    fn first_two_sided_timestamp_comes_from_events() {
        let mut ask = event(300, 2, BookAction::Add, BookSide::Ask);
        ask.price_units = 10_100;
        let replay = session(
            vec![
                event(100, 0, BookAction::Clear, BookSide::None),
                event(200, 1, BookAction::Add, BookSide::Bid),
                ask,
            ],
            Vec::new(),
        );
        assert_eq!(replay.first_two_sided_timestamp_ms(), Some(300));
    }

    #[test]
    fn status_window_skips_annotation_actions_as_phases() {
        let mut replay = session(Vec::new(), Vec::new());
        let status = |ts_ms: u64, action| ReplayStatusRecord {
            ts_recv_ns: ts_ms * 1_000_000,
            ts_event_ns: ts_ms * 1_000_000,
            action,
            reason: 0,
            trading_event: 0,
            is_trading: b'~',
            is_quoting: b'~',
            is_short_sell_restricted: b'~',
        };
        replay.statuses = vec![
            status(100, 1),
            status(200, 14),
            status(300, 7),
            status(400, 8),
        ];
        replay.cursor_ms = 350;
        let window = replay.status_window();
        assert_eq!(window.previous.unwrap().action, 1);
        assert_eq!(window.current.unwrap().action, 7);
        assert!(window.next.is_none());
        assert_eq!(window.latest.unwrap().action, 7);
    }
}
