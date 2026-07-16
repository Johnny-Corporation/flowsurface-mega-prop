use chrono::{DateTime, Utc};
use dbn::{
    FlagSet, MboMsg, Metadata, SType, Schema, SymbolIndex, UNDEF_PRICE, VersionUpgradePolicy,
    decode::{DbnMetadata, DecodeRecord, DynDecoder},
};
use flowsurface_replay::{
    BookSnapshot, CATALOG_VERSION, Catalog, Instrument, Level, PROCESSED_DIRECTORY, RAW_DIRECTORY,
    ReplayDay, ReplayError, ReplayTrade, data_root, relative_to_data_root, save_catalog,
    write_zstd,
};
use std::{
    collections::{BTreeMap, HashMap},
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

const SNAPSHOT_INTERVAL_NS: u64 = 100_000_000;
const BOOK_DEPTH: usize = 10;
const QTY_SCALE: i64 = 100_000_000;

#[derive(Debug, Clone, Copy)]
struct Order {
    price_units: i64,
    size: u64,
    is_bid: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum BookInvariantError {
    MissingInitialClear(&'static str),
    DuplicateAdd(u64),
    UnknownModify(u64),
    UnknownCancel(u64),
    OversizedCancel {
        order_id: u64,
        cancelled_size: u64,
        remaining_size: u64,
    },
    SideChangingModify(u64),
}

impl fmt::Display for BookInvariantError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingInitialClear(action) => write!(
                formatter,
                "{action} before initial Clear R; a full historical daily snapshot is required"
            ),
            Self::DuplicateAdd(order_id) => {
                write!(formatter, "duplicate Add for order {order_id}")
            }
            Self::UnknownModify(order_id) => {
                write!(formatter, "Modify for unknown order {order_id}")
            }
            Self::UnknownCancel(order_id) => {
                write!(formatter, "Cancel for unknown order {order_id}")
            }
            Self::OversizedCancel {
                order_id,
                cancelled_size,
                remaining_size,
            } => write!(
                formatter,
                "Cancel size {cancelled_size} exceeds remaining size {remaining_size} for order {order_id}"
            ),
            Self::SideChangingModify(order_id) => {
                write!(formatter, "Modify changes side for order {order_id}")
            }
        }
    }
}

impl Error for BookInvariantError {}

#[derive(Debug, Default)]
struct InstrumentActivity {
    traded_qty: u128,
    events: u64,
}

#[derive(Debug)]
struct SelectedInstrument {
    id: u32,
    symbol: String,
}

#[derive(Debug, Default)]
struct BookBuilder {
    has_seen_clear: bool,
    orders: HashMap<u64, Order>,
    bids: BTreeMap<i64, u64>,
    asks: BTreeMap<i64, u64>,
}

impl BookBuilder {
    fn clear(&mut self) {
        self.has_seen_clear = true;
        self.orders.clear();
        self.bids.clear();
        self.asks.clear();
    }

    fn add(&mut self, key: u64, order: Order) -> Result<(), BookInvariantError> {
        self.require_initial_clear("Add")?;
        if self.orders.contains_key(&key) {
            return Err(BookInvariantError::DuplicateAdd(key));
        }
        self.orders.insert(key, order);
        self.adjust_level(order, order.size as i64);
        Ok(())
    }

    fn modify(&mut self, key: u64, replacement: Order) -> Result<(), BookInvariantError> {
        self.require_initial_clear("Modify")?;
        let previous = self
            .orders
            .get(&key)
            .copied()
            .ok_or(BookInvariantError::UnknownModify(key))?;
        if previous.is_bid != replacement.is_bid {
            return Err(BookInvariantError::SideChangingModify(key));
        }
        self.adjust_level(previous, -(previous.size as i64));
        self.orders.insert(key, replacement);
        self.adjust_level(replacement, replacement.size as i64);
        Ok(())
    }

    fn cancel(&mut self, key: u64, cancelled_size: u64) -> Result<(), BookInvariantError> {
        self.require_initial_clear("Cancel")?;
        let mut order = self
            .orders
            .get(&key)
            .copied()
            .ok_or(BookInvariantError::UnknownCancel(key))?;
        if cancelled_size > order.size {
            return Err(BookInvariantError::OversizedCancel {
                order_id: key,
                cancelled_size,
                remaining_size: order.size,
            });
        }
        self.orders.remove(&key);
        self.adjust_level(order, -(cancelled_size as i64));
        order.size -= cancelled_size;
        if order.size > 0 {
            self.orders.insert(key, order);
        }
        Ok(())
    }

    fn require_initial_clear(&self, action: &'static str) -> Result<(), BookInvariantError> {
        if self.has_seen_clear {
            Ok(())
        } else {
            Err(BookInvariantError::MissingInitialClear(action))
        }
    }

    fn adjust_level(&mut self, order: Order, delta: i64) {
        let levels = if order.is_bid {
            &mut self.bids
        } else {
            &mut self.asks
        };
        let current = levels.get(&order.price_units).copied().unwrap_or_default();
        let next = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as u64)
        };
        if next == 0 {
            levels.remove(&order.price_units);
        } else {
            levels.insert(order.price_units, next);
        }
    }

    #[cfg(test)]
    fn snapshot(&self, ts_recv_ns: u64) -> BookSnapshot {
        let to_level = |(price_units, size): (&i64, &u64)| Level {
            price_units: *price_units,
            qty_units: i64::try_from(*size)
                .unwrap_or(i64::MAX / QTY_SCALE)
                .saturating_mul(QTY_SCALE),
        };
        BookSnapshot {
            ts_recv_ns,
            bids: self
                .bids
                .iter()
                .rev()
                .take(BOOK_DEPTH)
                .map(to_level)
                .collect(),
            asks: self.asks.iter().take(BOOK_DEPTH).map(to_level).collect(),
        }
    }
}

#[derive(Debug, Default)]
struct MarketBooks {
    books: HashMap<(u16, u32), BookBuilder>,
}

impl MarketBooks {
    fn book_mut(&mut self, publisher_id: u16, instrument_id: u32) -> &mut BookBuilder {
        self.books.entry((publisher_id, instrument_id)).or_default()
    }

    fn snapshot(&self, ts_recv_ns: u64) -> BookSnapshot {
        let mut bids = BTreeMap::<i64, u64>::new();
        let mut asks = BTreeMap::<i64, u64>::new();
        for book in self.books.values() {
            for (price, size) in book.bids.iter().rev().take(BOOK_DEPTH) {
                let level = bids.entry(*price).or_default();
                *level = level.saturating_add(*size);
            }
            for (price, size) in book.asks.iter().take(BOOK_DEPTH) {
                let level = asks.entry(*price).or_default();
                *level = level.saturating_add(*size);
            }
        }
        let to_level = |(price_units, size): (&i64, &u64)| Level {
            price_units: *price_units,
            qty_units: i64::try_from(*size)
                .unwrap_or(i64::MAX / QTY_SCALE)
                .saturating_mul(QTY_SCALE),
        };
        BookSnapshot {
            ts_recv_ns,
            bids: bids.iter().rev().take(BOOK_DEPTH).map(to_level).collect(),
            asks: asks.iter().take(BOOK_DEPTH).map(to_level).collect(),
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let raw_root = data_root().join(RAW_DIRECTORY);
    let mut inputs = Vec::new();
    collect_dbn_files(&raw_root, &mut inputs)?;
    inputs.sort();

    if inputs.is_empty() {
        return Err(format!("no DBN files found in {}", raw_root.display()).into());
    }

    let mut catalog = match flowsurface_replay::load_catalog() {
        Ok(catalog) => catalog,
        Err(ReplayError::UnsupportedCatalogVersion(_)) => Catalog {
            version: CATALOG_VERSION,
            instruments: Vec::new(),
        },
        Err(error) => return Err(error.into()),
    };
    for input in inputs {
        let imported = import_file(&input)?;
        merge_instrument(&mut catalog, imported);
    }
    catalog.version = CATALOG_VERSION;
    catalog.instruments.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    save_catalog(&catalog)?;
    println!(
        "Imported {} instruments into {}",
        catalog.instruments.len(),
        data_root().join(flowsurface_replay::CATALOG_FILE).display()
    );
    Ok(())
}

fn collect_dbn_files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_dbn_files(&path, output)?;
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".dbn") || name.ends_with(".dbn.zst"))
        {
            output.push(path);
        }
    }
    Ok(())
}

fn import_file(path: &Path) -> Result<Instrument, Box<dyn Error>> {
    let mut decoder = DynDecoder::from_file(path, VersionUpgradePolicy::UpgradeToV3)?;
    let metadata = decoder.metadata().clone();
    if metadata.schema != Some(Schema::Mbo) {
        return Err(format!("{} is not MBO data", path.display()).into());
    }
    let selected_source = select_source_instrument(path, &metadata)?;
    let symbol_map = metadata.symbol_map()?;
    let mut symbol = None;
    let mut date = None;
    let mut books = MarketBooks::default();
    let mut snapshots = Vec::new();
    let mut trades = Vec::new();
    let mut last_snapshot_ns = 0;
    let mut last_ts_ns = 0;
    let mut changed = false;
    let mut crossed_snapshots = 0_u64;

    while let Some(message) = decoder.decode_record::<MboMsg>()? {
        reject_top_of_book_record(message.flags).map_err(|reason| {
            format!(
                "{}: {reason} at ts_recv={} publisher={} instrument={} order={}",
                path.display(),
                message.ts_recv,
                message.hd.publisher_id,
                message.hd.instrument_id,
                message.order_id
            )
        })?;
        if selected_source
            .as_ref()
            .is_some_and(|source| source.id != message.hd.instrument_id)
        {
            continue;
        }
        let mapped_symbol = symbol_map
            .get_for_rec(message)
            .cloned()
            .or_else(|| metadata.symbols.first().cloned())
            .ok_or("DBN metadata has no symbol mapping")?;
        let canonical = canonical_symbol(&mapped_symbol, &metadata.symbols);
        if let Some(existing) = &symbol
            && existing != &canonical
        {
            return Err(format!(
                "{} contains multiple canonical symbols; request symbol splitting in Databento",
                path.display()
            )
            .into());
        }
        symbol = Some(canonical);

        let ts_recv_ns = message.ts_recv;
        let message_date = utc_date(ts_recv_ns / 1_000_000)?;
        if let Some(existing) = &date
            && existing != &message_date
        {
            return Err(format!(
                "{} contains multiple UTC days; request daily splitting in Databento",
                path.display()
            )
            .into());
        }
        date = Some(message_date);
        last_ts_ns = last_ts_ns.max(ts_recv_ns);

        let action = message.action as u8 as char;
        let is_bid = message.side as u8 as char == 'B';
        if action == 'T' && message.price != UNDEF_PRICE {
            trades.push(ReplayTrade {
                ts_recv_ns,
                ts_event_ns: message.hd.ts_event,
                price_units: dbn_price_to_units(message.price),
                qty_units: i64::from(message.size).saturating_mul(QTY_SCALE),
                is_sell: message.side as u8 as char == 'A',
            });
        } else {
            let book = books.book_mut(message.hd.publisher_id, message.hd.instrument_id);
            let order = Order {
                price_units: dbn_price_to_units(message.price),
                size: u64::from(message.size),
                is_bid,
            };
            let invariant_result = match action {
                'A' if message.price != UNDEF_PRICE => book.add(message.order_id, order),
                'M' if message.price != UNDEF_PRICE => book.modify(message.order_id, order),
                'C' => book.cancel(message.order_id, u64::from(message.size)),
                'R' => {
                    book.clear();
                    Ok(())
                }
                'F' | 'N' | 'T' => Ok(()),
                _ => Ok(()),
            };
            invariant_result.map_err(|error| {
                format!(
                    "{}: {error} at ts_recv={} publisher={} instrument={}",
                    path.display(),
                    message.ts_recv,
                    message.hd.publisher_id,
                    message.hd.instrument_id
                )
            })?;
            changed |= matches!(action, 'A' | 'M' | 'C' | 'R');
        }

        if message.flags.is_last()
            && changed
            && (last_snapshot_ns == 0
                || ts_recv_ns.saturating_sub(last_snapshot_ns) >= SNAPSHOT_INTERVAL_NS)
        {
            let snapshot = books.snapshot(ts_recv_ns);
            crossed_snapshots += u64::from(is_crossed(&snapshot));
            snapshots.push(snapshot);
            last_snapshot_ns = ts_recv_ns;
            changed = false;
        }
    }

    let symbol = symbol.ok_or("DBN file contained no MBO records")?;
    let date = date.ok_or("DBN file contained no dated records")?;
    if changed && last_ts_ns > last_snapshot_ns {
        let snapshot = books.snapshot(last_ts_ns);
        crossed_snapshots += u64::from(is_crossed(&snapshot));
        snapshots.push(snapshot);
    }
    if snapshots.is_empty() {
        return Err(format!("{} produced no L2 snapshots", path.display()).into());
    }
    trades.sort_by_key(|trade| trade.ts_recv_ns);

    let output_dir = data_root()
        .join(PROCESSED_DIRECTORY)
        .join(&metadata.dataset)
        .join(&symbol);
    let source_symbol = selected_source
        .as_ref()
        .map(|source| source.symbol.clone())
        .unwrap_or_else(|| symbol.clone());
    let l2_path = output_dir.join(format!("{date}.l2.fsr.zst"));
    let trades_path = output_dir.join(format!("{date}.trades.fsr.zst"));
    write_zstd(&l2_path, &snapshots)?;
    write_zstd(&trades_path, &trades)?;

    let day = ReplayDay {
        date,
        start_ts_ms: snapshots
            .first()
            .map(|value| value.ts_recv_ns / 1_000_000)
            .unwrap_or_default(),
        end_ts_ms: last_ts_ns / 1_000_000,
        source_instrument_id: selected_source.as_ref().map(|source| source.id),
        source_symbol: Some(source_symbol),
        l2_file: relative_to_data_root(&l2_path),
        trades_file: relative_to_data_root(&trades_path),
        raw_l3_files: vec![relative_to_data_root(path)],
    };
    println!(
        "{} {}: {} L2 snapshots, {} trades, {} crossed auction/pre-open snapshots",
        symbol,
        day.date,
        snapshots.len(),
        trades.len(),
        crossed_snapshots
    );
    Ok(Instrument {
        display_name: display_name(&symbol).to_owned(),
        min_tick_price_units: min_tick_price_units(&symbol),
        symbol,
        dataset: metadata.dataset,
        days: vec![day],
    })
}

fn reject_top_of_book_record(flags: FlagSet) -> Result<(), &'static str> {
    if flags.is_tob() {
        Err("F_TOB normalized MBO record cannot reconstruct an order-level book")
    } else {
        Ok(())
    }
}

fn select_source_instrument(
    path: &Path,
    metadata: &Metadata,
) -> Result<Option<SelectedInstrument>, Box<dyn Error>> {
    if metadata.stype_in != Some(SType::Parent) {
        return Ok(None);
    }

    let mut decoder = DynDecoder::from_file(path, VersionUpgradePolicy::UpgradeToV3)?;
    let symbol_map = metadata.symbol_map()?;
    let mut activity = BTreeMap::<u32, InstrumentActivity>::new();
    let mut symbols = BTreeMap::<u32, String>::new();
    while let Some(message) = decoder.decode_record::<MboMsg>()? {
        let mapped_symbol = symbol_map.get_for_rec(message).ok_or_else(|| {
            format!(
                "{} has no symbol mapping for instrument {}",
                path.display(),
                message.hd.instrument_id
            )
        })?;
        if !is_outright_futures_symbol(mapped_symbol) {
            continue;
        }
        symbols
            .entry(message.hd.instrument_id)
            .or_insert_with(|| mapped_symbol.clone());
        let current = activity.entry(message.hd.instrument_id).or_default();
        current.events = current.events.saturating_add(1);
        if message.action as u8 as char == 'T' {
            current.traded_qty = current.traded_qty.saturating_add(u128::from(message.size));
        }
    }

    let selected = primary_instrument(&activity)
        .ok_or_else(|| format!("{} contains no instruments", path.display()))?;
    let selected_symbol = symbols
        .get(&selected)
        .cloned()
        .ok_or_else(|| format!("{} has no symbol for instrument {selected}", path.display()))?;
    println!(
        "{}: selected outright parent instrument {} ({}) by traded volume",
        path.display(),
        selected,
        selected_symbol
    );
    Ok(Some(SelectedInstrument {
        id: selected,
        symbol: selected_symbol,
    }))
}

fn is_outright_futures_symbol(symbol: &str) -> bool {
    !symbol.contains('-')
}

fn is_crossed(snapshot: &BookSnapshot) -> bool {
    matches!(
        (snapshot.bids.first(), snapshot.asks.first()),
        (Some(bid), Some(ask)) if bid.price_units > ask.price_units
    )
}

fn primary_instrument(activity: &BTreeMap<u32, InstrumentActivity>) -> Option<u32> {
    activity
        .iter()
        .max_by(|(left_id, left), (right_id, right)| {
            left.traded_qty
                .cmp(&right.traded_qty)
                .then_with(|| left.events.cmp(&right.events))
                .then_with(|| right_id.cmp(left_id))
        })
        .map(|(instrument_id, _)| *instrument_id)
}

fn merge_instrument(catalog: &mut Catalog, mut imported: Instrument) {
    let Some(existing) = catalog
        .instruments
        .iter_mut()
        .find(|instrument| instrument.symbol == imported.symbol)
    else {
        catalog.instruments.push(imported);
        return;
    };
    for mut day in imported.days.drain(..) {
        if let Some(previous) = existing
            .days
            .iter_mut()
            .find(|value| value.date == day.date)
        {
            day.raw_l3_files.extend(previous.raw_l3_files.clone());
            day.raw_l3_files.sort();
            day.raw_l3_files.dedup();
            *previous = day;
        } else {
            existing.days.push(day);
        }
    }
    existing.days.sort_by(|a, b| a.date.cmp(&b.date));
}

fn canonical_symbol(raw: &str, requested: &[String]) -> String {
    if raw.starts_with("6E") || requested.iter().any(|symbol| symbol.starts_with("6E.")) {
        "EURUSD".to_owned()
    } else {
        raw.to_owned()
    }
}

fn display_name(symbol: &str) -> &str {
    match symbol {
        "AAPL" => "Apple",
        "NKE" => "Nike",
        "NVDA" => "NVIDIA",
        "EURUSD" => "EUR/USD (CME Euro FX)",
        _ => symbol,
    }
}

fn min_tick_price_units(symbol: &str) -> i64 {
    if symbol == "EURUSD" { 5_000 } else { 1_000_000 }
}

fn dbn_price_to_units(price: i64) -> i64 {
    price / 10
}

fn utc_date(ts_ms: u64) -> Result<String, Box<dyn Error>> {
    let timestamp = i64::try_from(ts_ms / 1_000)?;
    let date = DateTime::<Utc>::from_timestamp(timestamp, 0).ok_or("timestamp is out of range")?;
    Ok(date.format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn book_aggregates_orders_and_partial_cancels() {
        let mut book = BookBuilder::default();
        book.clear();
        book.add(
            1,
            Order {
                price_units: 10,
                size: 5,
                is_bid: true,
            },
        )
        .unwrap();
        book.add(
            2,
            Order {
                price_units: 10,
                size: 7,
                is_bid: true,
            },
        )
        .unwrap();
        book.cancel(1, 3).unwrap();
        assert_eq!(book.snapshot(1).bids[0].qty_units, 9 * QTY_SCALE);
    }

    #[test]
    fn duplicate_add_is_rejected_without_changing_the_book() {
        let mut book = BookBuilder::default();
        book.clear();
        let first = Order {
            price_units: 10,
            size: 5,
            is_bid: true,
        };
        book.add(1, first).unwrap();

        assert_eq!(
            book.add(
                1,
                Order {
                    price_units: 11,
                    size: 7,
                    is_bid: true,
                }
            ),
            Err(BookInvariantError::DuplicateAdd(1))
        );
        assert_eq!(book.snapshot(1).bids[0].price_units, 10);
        assert_eq!(book.snapshot(1).bids[0].qty_units, 5 * QTY_SCALE);
    }

    #[test]
    fn unknown_modify_and_cancel_are_rejected() {
        let mut book = BookBuilder::default();
        book.clear();
        let replacement = Order {
            price_units: 10,
            size: 5,
            is_bid: true,
        };

        assert_eq!(
            book.modify(7, replacement),
            Err(BookInvariantError::UnknownModify(7))
        );
        assert_eq!(book.cancel(8, 1), Err(BookInvariantError::UnknownCancel(8)));
        assert!(book.orders.is_empty());
    }

    #[test]
    fn order_events_require_an_initial_clear() {
        let mut book = BookBuilder::default();
        let order = Order {
            price_units: 10,
            size: 5,
            is_bid: true,
        };

        assert_eq!(
            book.add(1, order),
            Err(BookInvariantError::MissingInitialClear("Add"))
        );
        assert_eq!(
            book.modify(1, order),
            Err(BookInvariantError::MissingInitialClear("Modify"))
        );
        assert_eq!(
            book.cancel(1, 1),
            Err(BookInvariantError::MissingInitialClear("Cancel"))
        );
        assert!(book.orders.is_empty());
    }

    #[test]
    fn oversized_cancel_is_rejected_without_changing_the_book() {
        let mut book = BookBuilder::default();
        book.clear();
        book.add(
            5,
            Order {
                price_units: 10,
                size: 3,
                is_bid: true,
            },
        )
        .unwrap();

        assert_eq!(
            book.cancel(5, 4),
            Err(BookInvariantError::OversizedCancel {
                order_id: 5,
                cancelled_size: 4,
                remaining_size: 3,
            })
        );
        assert_eq!(book.snapshot(1).bids[0].qty_units, 3 * QTY_SCALE);
    }

    #[test]
    fn known_modify_replaces_the_original_level() {
        let mut book = BookBuilder::default();
        book.clear();
        book.add(
            7,
            Order {
                price_units: 10,
                size: 5,
                is_bid: true,
            },
        )
        .unwrap();

        book.modify(
            7,
            Order {
                price_units: 11,
                size: 3,
                is_bid: true,
            },
        )
        .unwrap();

        assert_eq!(book.snapshot(1).bids.len(), 1);
        assert_eq!(book.snapshot(1).bids[0].price_units, 11);
        assert_eq!(book.snapshot(1).bids[0].qty_units, 3 * QTY_SCALE);
    }

    #[test]
    fn side_changing_modify_is_rejected_without_changing_the_book() {
        let mut book = BookBuilder::default();
        book.clear();
        book.add(
            7,
            Order {
                price_units: 10,
                size: 5,
                is_bid: true,
            },
        )
        .unwrap();

        assert_eq!(
            book.modify(
                7,
                Order {
                    price_units: 11,
                    size: 3,
                    is_bid: false,
                }
            ),
            Err(BookInvariantError::SideChangingModify(7))
        );
        assert_eq!(book.snapshot(1).bids[0].price_units, 10);
        assert!(book.snapshot(1).asks.is_empty());
    }

    #[test]
    fn normalized_top_of_book_records_are_rejected() {
        assert!(reject_top_of_book_record(FlagSet::empty()).is_ok());
        assert_eq!(
            reject_top_of_book_record(FlagSet::empty().set_tob()),
            Err("F_TOB normalized MBO record cannot reconstruct an order-level book")
        );
    }

    #[test]
    fn euro_fx_contracts_share_one_replay_symbol() {
        assert_eq!(canonical_symbol("6EM6", &["6E.v.0".into()]), "EURUSD");
    }

    #[test]
    fn parent_request_selects_the_most_traded_contract() {
        let activity = BTreeMap::from([
            (
                11,
                InstrumentActivity {
                    traded_qty: 100,
                    events: 10_000,
                },
            ),
            (
                22,
                InstrumentActivity {
                    traded_qty: 101,
                    events: 500,
                },
            ),
        ]);
        assert_eq!(primary_instrument(&activity), Some(22));
    }

    #[test]
    fn calendar_spread_is_not_an_outright_future() {
        assert!(is_outright_futures_symbol("6EU6"));
        assert!(!is_outright_futures_symbol("6EU6-6EM6"));
    }

    #[test]
    fn clearing_one_market_book_preserves_the_other() {
        let mut books = MarketBooks::default();
        books.book_mut(1, 10).clear();
        books.book_mut(2, 20).clear();
        books
            .book_mut(1, 10)
            .add(
                1,
                Order {
                    price_units: 100,
                    size: 2,
                    is_bid: true,
                },
            )
            .unwrap();
        books
            .book_mut(2, 20)
            .add(
                1,
                Order {
                    price_units: 101,
                    size: 3,
                    is_bid: true,
                },
            )
            .unwrap();
        books.book_mut(1, 10).clear();
        assert_eq!(books.snapshot(1).bids[0].price_units, 101);
    }
}
