use chrono::{DateTime, Utc};
use dbn::{
    FlagSet, MboMsg, Metadata, SType, Schema, StatusMsg, SymbolIndex, UNDEF_PRICE,
    VersionUpgradePolicy,
    decode::{DbnMetadata, DecodeRecord, DynDecoder},
};
use flowsurface_replay::{
    BookAction, BookEvent, BookSide, BookState, CATALOG_VERSION, Catalog, Instrument,
    PROCESSED_DIRECTORY, RAW_DIRECTORY, ReplayBookData, ReplayDay, ReplayError, ReplayStatusRecord,
    ReplayTrade, data_root, relative_to_data_root, save_catalog, write_zstd,
};
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
};

const CHECKPOINT_INTERVAL_NS: u64 = 15 * 60 * 1_000_000_000;
const QTY_SCALE: i64 = 100_000_000;

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
struct StatusGroup {
    records: Vec<ReplayStatusRecord>,
    raw_files: Vec<String>,
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
    let mut status_inputs = Vec::new();
    for input in inputs {
        let decoder = DynDecoder::from_file(&input, VersionUpgradePolicy::UpgradeToV3)?;
        match decoder.metadata().schema {
            Some(Schema::Mbo) => {
                if already_imported_mbo(&catalog, &input) {
                    println!("Skipping already imported MBO {}", input.display());
                    continue;
                }
                let imported = import_file(&input)?;
                merge_instrument(&mut catalog, imported);
            }
            Some(Schema::Status) => status_inputs.push(input),
            schema => println!(
                "Skipping {} with unsupported schema {schema:?}",
                input.display()
            ),
        }
    }
    import_status_files(&status_inputs, &mut catalog)?;
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

fn already_imported_mbo(catalog: &Catalog, raw_path: &Path) -> bool {
    let raw_path = relative_to_data_root(raw_path);
    imported_mbo_day(catalog, &raw_path).is_some_and(|day| {
        data_root().join(&day.mbo_file).is_file() && data_root().join(&day.trades_file).is_file()
    })
}

fn imported_mbo_day<'a>(catalog: &'a Catalog, raw_path: &str) -> Option<&'a ReplayDay> {
    catalog
        .instruments
        .iter()
        .flat_map(|instrument| &instrument.days)
        .find(|day| day.raw_l3_files.iter().any(|path| path == raw_path))
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
    let mut book = BookState::default();
    let mut events = Vec::new();
    let mut checkpoints = Vec::new();
    let mut trades = Vec::new();
    let mut first_ts_ns = None;
    let mut last_ts_ns = 0;
    let mut last_checkpoint_ns = 0;

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
        first_ts_ns.get_or_insert(ts_recv_ns);
        last_ts_ns = last_ts_ns.max(ts_recv_ns);

        let event = book_event(message)?;
        if event.action == BookAction::Trade && message.price != UNDEF_PRICE {
            trades.push(ReplayTrade {
                ts_recv_ns,
                ts_event_ns: message.hd.ts_event,
                price_units: dbn_price_to_units(message.price),
                qty_units: i64::from(message.size).saturating_mul(QTY_SCALE),
                is_sell: message.side as u8 as char == 'A',
            });
        }
        book.apply(&event).map_err(|error| {
            format!(
                "{}: {error} at ts_recv={} publisher={} instrument={}",
                path.display(),
                message.ts_recv,
                message.hd.publisher_id,
                message.hd.instrument_id
            )
        })?;
        events.push(event);

        if message.flags.is_last() {
            let initial_two_sided_checkpoint = checkpoints.is_empty() && {
                let snapshot = book.snapshot(ts_recv_ns);
                !snapshot.bids.is_empty() && !snapshot.asks.is_empty()
            };
            let periodic_checkpoint = last_checkpoint_ns > 0
                && ts_recv_ns.saturating_sub(last_checkpoint_ns) >= CHECKPOINT_INTERVAL_NS;
            if initial_two_sided_checkpoint || periodic_checkpoint {
                checkpoints.push(book.checkpoint(ts_recv_ns, events.len() as u64));
                last_checkpoint_ns = ts_recv_ns;
            }
        }
    }

    let symbol = symbol.ok_or("DBN file contained no MBO records")?;
    let date = date.ok_or("DBN file contained no dated records")?;
    if events.is_empty() {
        return Err(format!("{} produced no MBO events", path.display()).into());
    }
    if checkpoints
        .last()
        .is_none_or(|checkpoint| checkpoint.next_event_index != events.len() as u64)
    {
        checkpoints.push(book.checkpoint(last_ts_ns, events.len() as u64));
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
    let mbo_path = output_dir.join(format!("{date}.mbo.fsr.zst"));
    let trades_path = output_dir.join(format!("{date}.trades.fsr.zst"));
    let event_count = events.len();
    let checkpoint_count = checkpoints.len();
    write_zstd(
        &mbo_path,
        &ReplayBookData {
            events,
            checkpoints,
        },
    )?;
    write_zstd(&trades_path, &trades)?;

    let day = ReplayDay {
        date,
        start_ts_ms: first_ts_ns.unwrap_or_default() / 1_000_000,
        end_ts_ms: last_ts_ns / 1_000_000,
        source_instrument_id: selected_source.as_ref().map(|source| source.id),
        source_symbol: Some(source_symbol),
        mbo_file: relative_to_data_root(&mbo_path),
        trades_file: relative_to_data_root(&trades_path),
        status_file: None,
        raw_l3_files: vec![relative_to_data_root(path)],
        raw_status_files: Vec::new(),
    };
    println!(
        "{} {}: {} lossless MBO events, {} full checkpoints, {} trades",
        symbol,
        day.date,
        event_count,
        checkpoint_count,
        trades.len(),
    );
    Ok(Instrument {
        display_name: display_name(&symbol).to_owned(),
        min_tick_price_units: min_tick_price_units(&symbol),
        symbol,
        dataset: metadata.dataset,
        days: vec![day],
    })
}

fn book_event(message: &MboMsg) -> Result<BookEvent, Box<dyn Error>> {
    let action = match message.action as u8 as char {
        'A' => BookAction::Add,
        'M' => BookAction::Modify,
        'C' => BookAction::Cancel,
        'R' => BookAction::Clear,
        'T' => BookAction::Trade,
        'F' => BookAction::Fill,
        'N' => BookAction::None,
        value => return Err(format!("unknown MBO action {value:?}").into()),
    };
    let side = match message.side as u8 as char {
        'B' => BookSide::Bid,
        'A' => BookSide::Ask,
        'N' => BookSide::None,
        value => return Err(format!("unknown MBO side {value:?}").into()),
    };
    Ok(BookEvent {
        ts_recv_ns: message.ts_recv,
        ts_event_ns: message.hd.ts_event,
        publisher_id: message.hd.publisher_id,
        instrument_id: message.hd.instrument_id,
        order_id: message.order_id,
        price_units: if message.price == UNDEF_PRICE {
            0
        } else {
            dbn_price_to_units(message.price)
        },
        qty_units: i64::from(message.size).saturating_mul(QTY_SCALE),
        sequence: message.sequence,
        ts_in_delta: message.ts_in_delta,
        channel_id: message.channel_id,
        flags: message.flags.raw(),
        action,
        side,
    })
}

fn import_status_files(paths: &[PathBuf], catalog: &mut Catalog) -> Result<(), Box<dyn Error>> {
    let mut groups = BTreeMap::<(String, String), StatusGroup>::new();
    for path in paths {
        let mut decoder = DynDecoder::from_file(path, VersionUpgradePolicy::UpgradeToV3)?;
        let metadata = decoder.metadata().clone();
        if metadata.schema != Some(Schema::Status) {
            return Err(format!("{} is not Status data", path.display()).into());
        }
        let symbol_map = metadata.symbol_map()?;
        let raw_file = relative_to_data_root(path);
        let mut matched_records = 0_usize;
        while let Some(message) = decoder.decode_record::<StatusMsg>()? {
            let mapped_symbol = symbol_map
                .get_for_rec(message)
                .cloned()
                .or_else(|| metadata.symbols.first().cloned())
                .ok_or("Status DBN metadata has no symbol mapping")?;
            let symbol = canonical_symbol(&mapped_symbol, &metadata.symbols);
            let date = utc_date(message.ts_recv / 1_000_000)?;
            let Some(day) = catalog
                .instruments
                .iter()
                .find(|instrument| {
                    instrument.symbol == symbol && instrument.dataset == metadata.dataset
                })
                .and_then(|instrument| instrument.days.iter().find(|day| day.date == date))
            else {
                continue;
            };
            if !status_source_matches(day, message.hd.instrument_id) {
                continue;
            }
            let group = groups.entry((symbol, date)).or_default();
            group.records.push(ReplayStatusRecord {
                ts_recv_ns: message.ts_recv,
                ts_event_ns: message.hd.ts_event,
                action: message.action,
                reason: message.reason,
                trading_event: message.trading_event,
                is_trading: message.is_trading as u8,
                is_quoting: message.is_quoting as u8,
                is_short_sell_restricted: message.is_short_sell_restricted as u8,
            });
            group.raw_files.push(raw_file.clone());
            matched_records += 1;
        }
        if matched_records == 0 {
            println!(
                "{}: no Status records matched a downloaded MBO instrument/day",
                path.display()
            );
        }
    }

    for ((symbol, date), mut group) in groups {
        group.records.sort_by_key(|record| record.ts_recv_ns);
        group.records.dedup();
        group.raw_files.sort();
        group.raw_files.dedup();
        let instrument = catalog
            .instruments
            .iter_mut()
            .find(|instrument| instrument.symbol == symbol)
            .ok_or_else(|| format!("no replay instrument for Status symbol {symbol}"))?;
        let day = instrument
            .days
            .iter_mut()
            .find(|day| day.date == date)
            .ok_or_else(|| format!("no replay day for Status symbol {symbol} on {date}"))?;
        let status_path = data_root()
            .join(PROCESSED_DIRECTORY)
            .join(&instrument.dataset)
            .join(&symbol)
            .join(format!("{date}.status.fsr.zst"));
        write_zstd(&status_path, &group.records)?;
        day.status_file = Some(relative_to_data_root(&status_path));
        day.raw_status_files = group.raw_files;
        println!("{symbol} {date}: {} Status records", group.records.len());
    }
    Ok(())
}

fn status_source_matches(day: &ReplayDay, instrument_id: u32) -> bool {
    day.source_instrument_id
        .is_none_or(|selected| selected == instrument_id)
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
            day.status_file = day.status_file.or_else(|| previous.status_file.clone());
            day.raw_status_files
                .extend(previous.raw_status_files.clone());
            day.raw_status_files.sort();
            day.raw_status_files.dedup();
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
    fn status_parent_download_keeps_only_the_selected_futures_contract() {
        let day = ReplayDay {
            date: "2026-06-01".into(),
            start_ts_ms: 0,
            end_ts_ms: 1,
            source_instrument_id: Some(20_048),
            source_symbol: Some("6EM6".into()),
            mbo_file: String::new(),
            trades_file: String::new(),
            status_file: None,
            raw_l3_files: Vec::new(),
            raw_status_files: Vec::new(),
        };
        assert!(status_source_matches(&day, 20_048));
        assert!(!status_source_matches(&day, 10_573));
    }

    #[test]
    fn status_equity_without_parent_selection_accepts_its_records() {
        let day = ReplayDay {
            date: "2026-06-01".into(),
            start_ts_ms: 0,
            end_ts_ms: 1,
            source_instrument_id: None,
            source_symbol: Some("NKE".into()),
            mbo_file: String::new(),
            trades_file: String::new(),
            status_file: None,
            raw_l3_files: Vec::new(),
            raw_status_files: Vec::new(),
        };
        assert!(status_source_matches(&day, 42));
    }

    #[test]
    fn imported_mbo_lookup_uses_raw_file_identity() {
        let raw_path = "replay/raw/XNYS.PILLAR/NKE/2026-06-01.mbo.dbn.zst";
        let catalog = Catalog {
            version: CATALOG_VERSION,
            instruments: vec![Instrument {
                symbol: "NKE".into(),
                display_name: "Nike".into(),
                dataset: "XNYS.PILLAR".into(),
                min_tick_price_units: 1_000_000,
                days: vec![ReplayDay {
                    date: "2026-06-01".into(),
                    start_ts_ms: 0,
                    end_ts_ms: 1,
                    source_instrument_id: None,
                    source_symbol: Some("NKE".into()),
                    mbo_file: "replay/processed/NKE.mbo.fsr.zst".into(),
                    trades_file: "replay/processed/NKE.trades.fsr.zst".into(),
                    status_file: None,
                    raw_l3_files: vec![raw_path.into()],
                    raw_status_files: Vec::new(),
                }],
            }],
        };
        assert_eq!(
            imported_mbo_day(&catalog, raw_path).map(|day| day.date.as_str()),
            Some("2026-06-01")
        );
        assert!(imported_mbo_day(&catalog, "replay/raw/other.dbn.zst").is_none());
    }
}
