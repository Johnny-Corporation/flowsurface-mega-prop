use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use exchange::{
    PushFrequency, Ticker, TickerInfo, Trade, UnixMs,
    adapter::{Exchange, StreamKind, StreamTicksize},
    depth::Depth,
    unit::{Price, Qty},
};
use replay::{Catalog, Instrument, ReplayFrame, ReplaySession};
use std::{collections::BTreeMap, fmt, sync::Arc, time::Instant};

const DATE_FORMAT: &str = "%Y-%m-%d";
const TIME_FORMAT: &str = "%H:%M:%S";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayDate {
    value: String,
}

impl ReplayDate {
    fn new(value: String) -> Self {
        Self { value }
    }
}

impl fmt::Display for ReplayDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&format_human_date(&self.value))
    }
}

pub struct Controller {
    catalog: Catalog,
    session: Option<ReplaySession>,
    selected_symbol: Option<String>,
    cursor_ms: u64,
    seek_date: Option<ReplayDate>,
    seek_time_input: String,
    seek_draft_dirty: bool,
    status: String,
    last_tick: Instant,
    reset_requested: bool,
    market_warning: bool,
}

impl Controller {
    pub fn new() -> Self {
        let catalog = replay::load_catalog().unwrap_or_default();
        let selected_symbol = catalog
            .instruments
            .first()
            .map(|instrument| instrument.symbol.clone());
        let status = if selected_symbol.is_none() {
            "No local replay data. Run databento-import first.".into()
        } else {
            String::new()
        };
        Self {
            catalog,
            session: None,
            selected_symbol,
            cursor_ms: 0,
            seek_date: None,
            seek_time_input: String::new(),
            seek_draft_dirty: false,
            status,
            last_tick: Instant::now(),
            reset_requested: false,
            market_warning: false,
        }
    }

    pub fn reload_catalog(&mut self) -> Option<Vec<exchange::Event>> {
        match replay::load_catalog() {
            Ok(catalog) => {
                self.catalog = catalog;
                if let Some(symbol) = self
                    .selected_symbol
                    .clone()
                    .filter(|selected| self.instrument(selected).is_some())
                    .or_else(|| self.symbols().first().cloned())
                {
                    return self.select_symbol(symbol);
                }
                self.session = None;
                self.selected_symbol = None;
                self.status = "No local replay data. Run databento-import first.".into();
                None
            }
            Err(error) => {
                self.status = error.to_string();
                None
            }
        }
    }

    pub fn symbols(&self) -> Vec<String> {
        self.catalog
            .instruments
            .iter()
            .map(|instrument| instrument.symbol.clone())
            .collect()
    }

    pub fn selected_symbol(&self) -> Option<&String> {
        self.selected_symbol.as_ref()
    }

    pub fn available_dates(&self) -> Vec<ReplayDate> {
        let Some(symbol) = self.selected_symbol.as_deref() else {
            return Vec::new();
        };
        self.instrument(symbol)
            .map(|instrument| {
                instrument
                    .days
                    .iter()
                    .map(|day| ReplayDate::new(day.date.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn seek_date(&self) -> Option<ReplayDate> {
        self.seek_date.clone()
    }

    pub fn select_seek_date(&mut self, date: ReplayDate) {
        if self.available_dates().contains(&date) {
            self.seek_date = Some(date);
            self.seek_draft_dirty = true;
            self.status.clear();
        }
    }

    pub fn seek_time_input(&self) -> &str {
        &self.seek_time_input
    }

    pub fn set_seek_time_input(&mut self, value: String) {
        self.seek_time_input = value;
        self.seek_draft_dirty = true;
        if self.status == "Use UTC time as HH:MM:SS" {
            self.status.clear();
        }
    }

    pub fn status(&self) -> String {
        let Some(session) = &self.session else {
            return self.status.clone();
        };
        let cursor = format_human_cursor(self.cursor_ms);
        let mut base = format!("{} UTC · {}x", cursor, session.speed);
        if !self.status.is_empty() {
            base.push_str(" · ");
            base.push_str(&self.status);
        }
        if self.market_warning {
            format!("{base} · Auction/pre-open: non-executable crossed book")
        } else {
            base
        }
    }

    pub fn speed(&self) -> u16 {
        self.session.as_ref().map_or(1, |session| session.speed)
    }

    pub fn take_reset_requested(&mut self) -> bool {
        std::mem::take(&mut self.reset_requested)
    }

    pub fn ticker_info(&self) -> Option<TickerInfo> {
        let instrument = self.session.as_ref().map(|session| &session.instrument)?;
        let min_tick = instrument.min_tick_price_units as f32 / 100_000_000.0;
        Some(TickerInfo::new(
            Ticker::new(&instrument.symbol, Exchange::DatabentoReplay),
            min_tick,
            1.0,
            None,
        ))
    }

    pub fn select_symbol(&mut self, symbol: String) -> Option<Vec<exchange::Event>> {
        let instrument = self.instrument(&symbol)?.clone();
        let date = instrument.days.first()?.date.clone();
        match ReplaySession::open(instrument, &date) {
            Ok(mut session) => {
                let start_ms = session
                    .first_two_sided_timestamp_ms()
                    .unwrap_or(session.day.start_ts_ms);
                let frame = session.seek(start_ms);
                self.selected_symbol = Some(symbol);
                self.cursor_ms = session.cursor_ms;
                self.session = Some(session);
                self.sync_seek_draft();
                self.status.clear();
                self.apply_snapshot_warning(&frame);
                self.last_tick = Instant::now();
                self.reset_requested = true;
                Some(self.events(frame))
            }
            Err(error) => {
                self.status = error.to_string();
                None
            }
        }
    }

    pub fn set_speed(&mut self, speed: u16) {
        if let Some(session) = &mut self.session {
            session.set_speed(speed);
            self.last_tick = Instant::now();
            self.status.clear();
        }
    }

    pub fn jump_minutes(&mut self, minutes: i32) -> Option<Vec<exchange::Event>> {
        let delta = u64::from(minutes.unsigned_abs()).saturating_mul(60_000);
        let target = if minutes.is_negative() {
            self.cursor_ms.saturating_sub(delta)
        } else {
            self.cursor_ms.saturating_add(delta)
        };
        self.seek_to(target)
    }

    pub fn seek_from_input(&mut self) -> Option<Vec<exchange::Event>> {
        let selected_date = match self.seek_date.clone() {
            Some(value) if self.available_dates().contains(&value) => value,
            None => {
                self.status = "Select a downloaded replay date".into();
                return None;
            }
            Some(_) => {
                self.status = "Select a downloaded replay date".into();
                return None;
            }
        };
        let date = match NaiveDate::parse_from_str(&selected_date.value, DATE_FORMAT) {
            Ok(value) => value,
            Err(_) => {
                self.status = "Select a downloaded replay date".into();
                return None;
            }
        };
        let time = match NaiveTime::parse_from_str(&self.seek_time_input, TIME_FORMAT) {
            Ok(value) => value,
            Err(_) => {
                self.status = "Use UTC time as HH:MM:SS".into();
                return None;
            }
        };
        let parsed =
            DateTime::<Utc>::from_naive_utc_and_offset(NaiveDateTime::new(date, time), Utc);
        let target_ms = u64::try_from(parsed.timestamp_millis()).ok()?;
        self.seek_to(target_ms)
    }

    pub fn tick(&mut self, now: Instant) -> Vec<exchange::Event> {
        let elapsed_ms = now.duration_since(self.last_tick).as_millis() as u64;
        self.last_tick = now;
        let Some(session) = &self.session else {
            return Vec::new();
        };
        let delta = elapsed_ms.saturating_mul(u64::from(session.speed));
        self.advance_to(self.cursor_ms.saturating_add(delta))
    }

    fn seek_to(&mut self, requested_ms: u64) -> Option<Vec<exchange::Event>> {
        let symbol = self.selected_symbol.clone()?;
        let instrument = self.instrument(&symbol)?.clone();
        let first = instrument.days.first()?;
        let last = instrument.days.last()?;
        let (target_ms, boundary_status) = if requested_ms < first.start_ts_ms {
            (first.start_ts_ms, Some("Start of downloaded range"))
        } else if requested_ms > last.end_ts_ms {
            (last.end_ts_ms, Some("End of downloaded range"))
        } else {
            (requested_ms, None)
        };

        let (day, in_gap) = if let Some(day) = instrument
            .days
            .iter()
            .find(|day| day.start_ts_ms <= target_ms && target_ms <= day.end_ts_ms)
        {
            (day.clone(), false)
        } else {
            let previous = instrument
                .days
                .iter()
                .rev()
                .find(|day| day.end_ts_ms < target_ms)?;
            (previous.clone(), true)
        };

        if self.session.as_ref().map(|session| &session.day.date) != Some(&day.date) {
            let speed = self.speed();
            match ReplaySession::open(instrument, &day.date) {
                Ok(mut session) => {
                    session.set_speed(speed);
                    self.session = Some(session);
                }
                Err(error) => {
                    self.status = error.to_string();
                    return None;
                }
            }
        }

        let seek_ms = if in_gap { day.end_ts_ms } else { target_ms };
        let frame = self.session.as_mut()?.seek(seek_ms);
        self.cursor_ms = target_ms;
        self.sync_seek_draft();
        self.status = boundary_status.map_or_else(
            || {
                if in_gap {
                    "No market data at this UTC time".into()
                } else {
                    String::new()
                }
            },
            str::to_owned,
        );
        if in_gap {
            self.market_warning = false;
        } else {
            self.apply_snapshot_warning(&frame);
        }
        self.last_tick = Instant::now();
        self.reset_requested = true;
        Some(self.events(frame))
    }

    fn advance_to(&mut self, target_ms: u64) -> Vec<exchange::Event> {
        let mut frames = Vec::new();
        loop {
            let Some(session) = self.session.as_ref() else {
                return Vec::new();
            };
            let day_end = session.day.end_ts_ms;
            let session_cursor = session.cursor_ms;

            if target_ms <= day_end {
                let frame = self
                    .session
                    .as_mut()
                    .expect("replay session exists")
                    .advance_to(target_ms);
                frames.push(frame);
                self.cursor_ms = target_ms;
                if !self.seek_draft_dirty {
                    self.status.clear();
                }
                break;
            }

            if session_cursor < day_end {
                let final_frame = self
                    .session
                    .as_mut()
                    .expect("replay session exists")
                    .advance_to(day_end);
                frames.push(final_frame);
            }

            let current_date = self
                .session
                .as_ref()
                .expect("replay session exists")
                .day
                .date
                .clone();
            let Some(next_day) = self.adjacent_day(&current_date, true) else {
                self.cursor_ms = day_end;
                self.status = "End of downloaded range".into();
                break;
            };

            if target_ms < next_day.start_ts_ms {
                self.cursor_ms = target_ms;
                self.status = "No market data at this UTC time".into();
                break;
            }

            if !self.open_day(&next_day.date) {
                break;
            }
            let next = self.session.as_mut().expect("opened replay session");
            frames.push(next.seek(next.day.start_ts_ms));
            self.cursor_ms = next.day.start_ts_ms;
            self.status.clear();
        }

        self.sync_seek_draft_if_clean();
        for frame in &frames {
            self.apply_snapshot_warning(frame);
        }
        if self.status == "No market data at this UTC time" {
            self.market_warning = false;
        }
        frames
            .into_iter()
            .flat_map(|frame| self.events(frame))
            .collect()
    }

    fn apply_snapshot_warning(&mut self, frame: &ReplayFrame) {
        let Some(snapshot) = frame.snapshots.last() else {
            return;
        };
        self.market_warning = matches!(
            (snapshot.bids.first(), snapshot.asks.first()),
            (Some(bid), Some(ask)) if bid.price_units > ask.price_units
        );
    }

    fn adjacent_day(&self, current_date: &str, forward: bool) -> Option<replay::ReplayDay> {
        let session = self.session.as_ref()?;
        let index = session
            .instrument
            .days
            .iter()
            .position(|day| day.date == current_date)?;
        let adjacent = if forward {
            index.checked_add(1)?
        } else {
            index.checked_sub(1)?
        };
        session.instrument.days.get(adjacent).cloned()
    }

    fn open_day(&mut self, date: &str) -> bool {
        let Some(session) = self.session.as_ref() else {
            return false;
        };
        let instrument = session.instrument.clone();
        let speed = session.speed;
        match ReplaySession::open(instrument, date) {
            Ok(mut next) => {
                next.set_speed(speed);
                self.session = Some(next);
                true
            }
            Err(error) => {
                self.status = error.to_string();
                false
            }
        }
    }

    fn sync_seek_draft(&mut self) {
        let Some(timestamp) = cursor_datetime(self.cursor_ms) else {
            return;
        };
        let available_dates = self.available_dates();
        let cursor_date = ReplayDate::new(timestamp.format(DATE_FORMAT).to_string());
        if available_dates.contains(&cursor_date) {
            self.seek_date = Some(cursor_date);
        } else if self
            .seek_date
            .as_ref()
            .is_none_or(|date| !available_dates.contains(date))
        {
            self.seek_date = available_dates.first().cloned();
        }
        self.seek_time_input = timestamp.format(TIME_FORMAT).to_string();
        self.seek_draft_dirty = false;
    }

    fn sync_seek_draft_if_clean(&mut self) {
        if !self.seek_draft_dirty {
            self.sync_seek_draft();
        }
    }

    fn instrument(&self, symbol: &str) -> Option<&Instrument> {
        self.catalog
            .instruments
            .iter()
            .find(|instrument| instrument.symbol == symbol)
    }

    fn events(&self, frame: ReplayFrame) -> Vec<exchange::Event> {
        let Some(ticker_info) = self.ticker_info() else {
            return Vec::new();
        };
        Self::events_for_ticker(ticker_info, frame)
    }

    fn events_for_ticker(ticker_info: TickerInfo, frame: ReplayFrame) -> Vec<exchange::Event> {
        let snapshots = frame.snapshots.into_iter().peekable();
        let mut trades = frame.trades.into_iter().peekable();
        let mut events = Vec::with_capacity(snapshots.len().saturating_mul(2).saturating_add(1));

        for snapshot in snapshots {
            let mut trade_batch = Vec::new();
            while trades
                .peek()
                .is_some_and(|trade| trade.ts_recv_ns <= snapshot.ts_recv_ns)
            {
                trade_batch.push(trades.next().expect("peeked replay trade"));
            }
            if !trade_batch.is_empty() {
                events.push(Self::trades_event(ticker_info, trade_batch));
            }
            events.push(Self::depth_event(ticker_info, snapshot));
        }

        let remaining_trades = trades.collect::<Vec<_>>();
        if !remaining_trades.is_empty() {
            events.push(Self::trades_event(ticker_info, remaining_trades));
        }
        events
    }

    fn depth_event(ticker_info: TickerInfo, snapshot: replay::BookSnapshot) -> exchange::Event {
        let to_levels = |levels: Vec<replay::Level>| -> BTreeMap<Price, Qty> {
            levels
                .into_iter()
                .map(|level| {
                    (
                        Price::from_units(level.price_units),
                        Qty::from_units(level.qty_units),
                    )
                })
                .collect()
        };
        exchange::Event::DepthReceived(
            StreamKind::Depth {
                ticker_info,
                depth_aggr: StreamTicksize::Client,
                push_freq: PushFrequency::ServerDefault,
            },
            UnixMs::new(snapshot.ts_recv_ns / 1_000_000),
            Arc::new(Depth {
                bids: to_levels(snapshot.bids),
                asks: to_levels(snapshot.asks),
            }),
        )
    }

    fn trades_event(
        ticker_info: TickerInfo,
        replay_trades: Vec<replay::ReplayTrade>,
    ) -> exchange::Event {
        let update_time = replay_trades
            .last()
            .map(|trade| trade.ts_recv_ns / 1_000_000)
            .unwrap_or_default();
        let trades = replay_trades
            .into_iter()
            .map(|trade| Trade {
                time: UnixMs::new(trade.ts_event_ns / 1_000_000),
                is_sell: trade.is_sell,
                price: Price::from_units(trade.price_units),
                qty: Qty::from_units(trade.qty_units),
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        exchange::Event::TradesReceived(
            StreamKind::Trades { ticker_info },
            UnixMs::new(update_time),
            trades,
        )
    }
}

fn cursor_datetime(timestamp_ms: u64) -> Option<DateTime<Utc>> {
    i64::try_from(timestamp_ms)
        .ok()
        .and_then(DateTime::<Utc>::from_timestamp_millis)
}

fn format_human_cursor(timestamp_ms: u64) -> String {
    cursor_datetime(timestamp_ms)
        .map(|timestamp| {
            format!(
                "{} · {}",
                format_human_date(&timestamp.format(DATE_FORMAT).to_string()),
                timestamp.format(TIME_FORMAT)
            )
        })
        .unwrap_or_else(|| "Invalid replay time".into())
}

fn format_human_date(value: &str) -> String {
    let Ok(date) = NaiveDate::parse_from_str(value, DATE_FORMAT) else {
        return value.to_owned();
    };
    format!(
        "{}{} {} {}",
        date.day(),
        ordinal_suffix(date.day()),
        date.format("%b"),
        date.year()
    )
}

fn ordinal_suffix(day: u32) -> &'static str {
    if (11..=13).contains(&(day % 100)) {
        "th"
    } else {
        match day % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    }
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nanosecond_merge_keeps_trades_on_the_correct_side_of_a_snapshot() {
        let ticker_info = TickerInfo::new(
            Ticker::new("NKE", Exchange::DatabentoReplay),
            0.01,
            1.0,
            None,
        );
        let frame = ReplayFrame {
            snapshots: vec![replay::BookSnapshot {
                ts_recv_ns: 100_100_000,
                bids: Vec::new(),
                asks: Vec::new(),
            }],
            trades: vec![
                replay::ReplayTrade {
                    ts_recv_ns: 100_050_000,
                    ts_event_ns: 100_040_000,
                    price_units: 1,
                    qty_units: 1,
                    is_sell: false,
                },
                replay::ReplayTrade {
                    ts_recv_ns: 100_900_000,
                    ts_event_ns: 100_890_000,
                    price_units: 1,
                    qty_units: 1,
                    is_sell: true,
                },
            ],
            reached_end: false,
        };

        let events = Controller::events_for_ticker(ticker_info, frame);

        assert!(matches!(events[0], exchange::Event::TradesReceived(..)));
        assert!(matches!(events[1], exchange::Event::DepthReceived(..)));
        assert!(matches!(events[2], exchange::Event::TradesReceived(..)));
    }

    #[test]
    fn human_dates_use_english_ordinals() {
        assert_eq!(format_human_date("2026-06-01"), "1st Jun 2026");
        assert_eq!(format_human_date("2026-06-02"), "2nd Jun 2026");
        assert_eq!(format_human_date("2026-06-03"), "3rd Jun 2026");
        assert_eq!(format_human_date("2026-06-11"), "11th Jun 2026");
        assert_eq!(format_human_date("2026-06-12"), "12th Jun 2026");
        assert_eq!(format_human_date("2026-06-13"), "13th Jun 2026");
        assert_eq!(format_human_date("2026-06-21"), "21st Jun 2026");
        assert_eq!(format_human_date("2026-06-22"), "22nd Jun 2026");
        assert_eq!(format_human_date("2026-06-23"), "23rd Jun 2026");
        assert_eq!(format_human_date("2026-06-30"), "30th Jun 2026");
    }

    #[test]
    fn playback_cursor_does_not_overwrite_a_dirty_seek_draft() {
        let mut controller = Controller {
            catalog: Catalog::default(),
            session: None,
            selected_symbol: None,
            cursor_ms: 1_780_296_608_000,
            seek_date: Some(ReplayDate::new("2026-06-01".into())),
            seek_time_input: "10:30:00".into(),
            seek_draft_dirty: false,
            status: String::new(),
            last_tick: Instant::now(),
            reset_requested: false,
            market_warning: false,
        };

        controller.set_seek_time_input("13:45:00".into());
        controller.cursor_ms += 5_000;
        controller.sync_seek_draft_if_clean();

        assert_eq!(controller.seek_time_input(), "13:45:00");
        assert!(controller.seek_draft_dirty);
    }

    #[test]
    fn gap_cursor_keeps_a_valid_downloaded_seek_date() {
        let downloaded_date = ReplayDate::new("2026-06-05".into());
        let mut controller = Controller {
            catalog: Catalog {
                version: replay::CATALOG_VERSION,
                instruments: vec![Instrument {
                    symbol: "NKE".into(),
                    display_name: "Nike".into(),
                    dataset: "XNYS.PILLAR".into(),
                    min_tick_price_units: 1_000_000,
                    days: vec![replay::ReplayDay {
                        date: downloaded_date.value.clone(),
                        start_ts_ms: 1,
                        end_ts_ms: 2,
                        source_instrument_id: None,
                        source_symbol: Some("NKE".into()),
                        l2_file: String::new(),
                        trades_file: String::new(),
                        raw_l3_files: Vec::new(),
                    }],
                }],
            },
            session: None,
            selected_symbol: Some("NKE".into()),
            cursor_ms: 1_780_790_400_000,
            seek_date: Some(downloaded_date.clone()),
            seek_time_input: String::new(),
            seek_draft_dirty: false,
            status: String::new(),
            last_tick: Instant::now(),
            reset_requested: false,
            market_warning: false,
        };

        controller.sync_seek_draft();

        assert_eq!(controller.seek_date(), Some(downloaded_date));
    }

    #[test]
    fn seek_submission_rejects_a_date_outside_the_downloaded_sessions() {
        let mut controller = Controller {
            catalog: Catalog::default(),
            session: None,
            selected_symbol: None,
            cursor_ms: 0,
            seek_date: Some(ReplayDate::new("2026-06-07".into())),
            seek_time_input: "10:30:00".into(),
            seek_draft_dirty: true,
            status: String::new(),
            last_tick: Instant::now(),
            reset_requested: false,
            market_warning: false,
        };

        assert!(controller.seek_from_input().is_none());
        assert_eq!(controller.status, "Select a downloaded replay date");
    }
}
