use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc, Weekday};
use exchange::{
    PushFrequency, Ticker, TickerInfo, Trade, UnixMs,
    adapter::{Exchange, StreamKind, StreamTicksize},
    depth::Depth,
    unit::{Price, Qty},
};
use replay::{Catalog, Instrument, ReplayFrame, ReplaySession};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
    time::Instant,
};

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

    fn parsed(&self) -> Option<NaiveDate> {
        NaiveDate::parse_from_str(&self.value, DATE_FORMAT).ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReplayYear(i32);

impl fmt::Display for ReplayYear {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReplayMonth(u32);

impl fmt::Display for ReplayMonth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(month_name(self.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReplayDayOfMonth(u32);

impl fmt::Display for ReplayDayOfMonth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}{}", self.0, ordinal_suffix(self.0))
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
        let Some(instrument) = self.instrument(symbol) else {
            return Vec::new();
        };
        let timezone = replay_timezone(instrument);
        let mut dates = BTreeSet::new();
        for day in &instrument.days {
            let (Some(start), Some(end)) = (
                cursor_datetime(day.start_ts_ms),
                cursor_datetime(day.end_ts_ms),
            ) else {
                continue;
            };
            let mut date = timezone.local_at_utc(start).datetime.date();
            let end_date = timezone.local_at_utc(end).datetime.date();
            while date <= end_date {
                dates.insert(date);
                let Some(next) = date.succ_opt() else {
                    break;
                };
                date = next;
            }
        }
        dates
            .into_iter()
            .map(|date| ReplayDate::new(date.format(DATE_FORMAT).to_string()))
            .collect()
    }

    pub fn available_years(&self) -> Vec<ReplayYear> {
        self.available_dates()
            .into_iter()
            .filter_map(|date| date.parsed().map(|date| ReplayYear(date.year())))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn seek_year(&self) -> Option<ReplayYear> {
        self.seek_date
            .as_ref()?
            .parsed()
            .map(|date| ReplayYear(date.year()))
    }

    pub fn select_seek_year(&mut self, year: ReplayYear) {
        let current = self.seek_date.as_ref().and_then(ReplayDate::parsed);
        let replacement = self.best_available_date(|date| date.year() == year.0, current);
        if let Some(date) = replacement {
            self.select_seek_date(date);
        }
    }

    pub fn available_months(&self) -> Vec<ReplayMonth> {
        let Some(year) = self.seek_year() else {
            return Vec::new();
        };
        self.available_dates()
            .into_iter()
            .filter_map(|date| date.parsed())
            .filter(|date| date.year() == year.0)
            .map(|date| ReplayMonth(date.month()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn seek_month(&self) -> Option<ReplayMonth> {
        self.seek_date
            .as_ref()?
            .parsed()
            .map(|date| ReplayMonth(date.month()))
    }

    pub fn select_seek_month(&mut self, month: ReplayMonth) {
        let current = self.seek_date.as_ref().and_then(ReplayDate::parsed);
        let Some(year) = current.map(|date| date.year()) else {
            return;
        };
        let replacement = self.best_available_date(
            |date| date.year() == year && date.month() == month.0,
            current,
        );
        if let Some(date) = replacement {
            self.select_seek_date(date);
        }
    }

    pub fn available_days(&self) -> Vec<ReplayDayOfMonth> {
        let (Some(year), Some(month)) = (self.seek_year(), self.seek_month()) else {
            return Vec::new();
        };
        self.available_dates()
            .into_iter()
            .filter_map(|date| date.parsed())
            .filter(|date| date.year() == year.0 && date.month() == month.0)
            .map(|date| ReplayDayOfMonth(date.day()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn seek_day(&self) -> Option<ReplayDayOfMonth> {
        self.seek_date
            .as_ref()?
            .parsed()
            .map(|date| ReplayDayOfMonth(date.day()))
    }

    pub fn select_seek_day(&mut self, day: ReplayDayOfMonth) {
        let Some(current) = self.seek_date.as_ref().and_then(ReplayDate::parsed) else {
            return;
        };
        let replacement = self.best_available_date(
            |date| {
                date.year() == current.year()
                    && date.month() == current.month()
                    && date.day() == day.0
            },
            Some(current),
        );
        if let Some(date) = replacement {
            self.select_seek_date(date);
        }
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
        if self.status.starts_with("Use ") && self.status.ends_with(" time as HH:MM:SS") {
            self.status.clear();
        }
    }

    pub fn seek_timezone_label(&self) -> &'static str {
        let timezone = self.current_timezone();
        if let (Some(date), Ok(time)) = (
            self.seek_date.as_ref().and_then(ReplayDate::parsed),
            NaiveTime::parse_from_str(&self.seek_time_input, TIME_FORMAT),
        ) {
            return timezone.abbreviation_for_local(NaiveDateTime::new(date, time));
        }
        cursor_datetime(self.cursor_ms)
            .map(|timestamp| timezone.local_at_utc(timestamp).abbreviation)
            .unwrap_or_else(|| timezone.default_abbreviation())
    }

    pub fn status(&self) -> String {
        let Some(session) = &self.session else {
            return self.status.clone();
        };
        let cursor = format_human_cursor(self.cursor_ms, replay_timezone(&session.instrument));
        let mut base = format!("{} · {}x", cursor, session.speed);
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

    pub fn status_window(&self) -> replay::ReplayStatusWindow {
        self.session
            .as_ref()
            .map_or_else(Default::default, ReplaySession::status_window)
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
                let frame = match session.seek(start_ms) {
                    Ok(frame) => frame,
                    Err(error) => {
                        self.status = error.to_string();
                        return None;
                    }
                };
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
        let Some(date) = selected_date.parsed() else {
            self.status = "Select a downloaded replay date".into();
            return None;
        };
        let time = match NaiveTime::parse_from_str(&self.seek_time_input, TIME_FORMAT) {
            Ok(value) => value,
            Err(_) => {
                self.status = format!(
                    "Use {} time as HH:MM:SS",
                    self.current_timezone().default_abbreviation()
                );
                return None;
            }
        };
        let parsed = self
            .current_timezone()
            .utc_from_local(NaiveDateTime::new(date, time));
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
        let frame = match self.session.as_mut()?.seek(seek_ms) {
            Ok(frame) => frame,
            Err(error) => {
                self.status = error.to_string();
                return None;
            }
        };
        self.cursor_ms = target_ms;
        self.sync_seek_draft();
        self.status = boundary_status.map_or_else(
            || {
                if in_gap {
                    "No market data at this venue-local time".into()
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
                match frame {
                    Ok(frame) => frames.push(frame),
                    Err(error) => {
                        self.status = error.to_string();
                        break;
                    }
                }
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
                match final_frame {
                    Ok(frame) => frames.push(frame),
                    Err(error) => {
                        self.status = error.to_string();
                        break;
                    }
                }
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
                self.status = "No market data at this venue-local time".into();
                break;
            }

            if !self.open_day(&next_day.date) {
                break;
            }
            let next = self.session.as_mut().expect("opened replay session");
            match next.seek(next.day.start_ts_ms) {
                Ok(frame) => frames.push(frame),
                Err(error) => {
                    self.status = error.to_string();
                    break;
                }
            }
            self.cursor_ms = next.day.start_ts_ms;
            self.status.clear();
        }

        self.sync_seek_draft_if_clean();
        for frame in &frames {
            self.apply_snapshot_warning(frame);
        }
        if self.status == "No market data at this venue-local time" {
            self.market_warning = false;
        }
        frames
            .into_iter()
            .flat_map(|frame| self.events(frame))
            .collect()
    }

    fn apply_snapshot_warning(&mut self, frame: &ReplayFrame) {
        let Some(snapshot) = frame.snapshot.as_ref() else {
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
        let timestamp = self.current_timezone().local_at_utc(timestamp).datetime;
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

    fn best_available_date(
        &self,
        matches: impl Fn(NaiveDate) -> bool,
        preferred: Option<NaiveDate>,
    ) -> Option<ReplayDate> {
        let candidates = self
            .available_dates()
            .into_iter()
            .filter(|date| date.parsed().is_some_and(&matches))
            .collect::<Vec<_>>();
        candidates
            .iter()
            .find(|candidate| candidate.parsed() == preferred)
            .or_else(|| {
                preferred.and_then(|preferred| {
                    candidates.iter().min_by_key(|candidate| {
                        candidate
                            .parsed()
                            .map(|date| (date - preferred).num_days().unsigned_abs())
                            .unwrap_or(u64::MAX)
                    })
                })
            })
            .or_else(|| candidates.first())
            .cloned()
    }

    fn current_timezone(&self) -> ReplayTimezone {
        self.session
            .as_ref()
            .map(|session| replay_timezone(&session.instrument))
            .or_else(|| {
                self.selected_symbol
                    .as_deref()
                    .and_then(|symbol| self.instrument(symbol))
                    .map(replay_timezone)
            })
            .unwrap_or(ReplayTimezone::Utc)
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
        let mut events = Vec::with_capacity(2);
        if !frame.trades.is_empty() {
            events.push(Self::trades_event(ticker_info, frame.trades));
        }
        if let Some(snapshot) = frame.snapshot {
            events.push(Self::depth_event(ticker_info, snapshot));
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplayTimezone {
    NewYork,
    Chicago,
    Utc,
}

#[derive(Debug, Clone, Copy)]
struct ReplayLocalTime {
    datetime: NaiveDateTime,
    abbreviation: &'static str,
}

impl ReplayTimezone {
    fn local_at_utc(self, timestamp: DateTime<Utc>) -> ReplayLocalTime {
        let is_daylight = self.is_daylight_at_utc(timestamp.naive_utc());
        let offset_hours = self.offset_hours(is_daylight);
        ReplayLocalTime {
            datetime: timestamp.naive_utc() + Duration::hours(i64::from(offset_hours)),
            abbreviation: self.abbreviation(is_daylight),
        }
    }

    fn utc_from_local(self, datetime: NaiveDateTime) -> DateTime<Utc> {
        let is_daylight = self.is_daylight_at_local(datetime);
        let utc = datetime - Duration::hours(i64::from(self.offset_hours(is_daylight)));
        DateTime::<Utc>::from_naive_utc_and_offset(utc, Utc)
    }

    fn abbreviation_for_local(self, datetime: NaiveDateTime) -> &'static str {
        self.abbreviation(self.is_daylight_at_local(datetime))
    }

    fn default_abbreviation(self) -> &'static str {
        match self {
            Self::NewYork => "ET",
            Self::Chicago => "CT",
            Self::Utc => "UTC",
        }
    }

    fn offset_hours(self, is_daylight: bool) -> i32 {
        match (self, is_daylight) {
            (Self::NewYork, true) => -4,
            (Self::NewYork, false) => -5,
            (Self::Chicago, true) => -5,
            (Self::Chicago, false) => -6,
            (Self::Utc, _) => 0,
        }
    }

    fn abbreviation(self, is_daylight: bool) -> &'static str {
        match (self, is_daylight) {
            (Self::NewYork, true) => "EDT",
            (Self::NewYork, false) => "EST",
            (Self::Chicago, true) => "CDT",
            (Self::Chicago, false) => "CST",
            (Self::Utc, _) => "UTC",
        }
    }

    fn is_daylight_at_utc(self, datetime: NaiveDateTime) -> bool {
        let Some((start, end)) = self.utc_daylight_boundaries(datetime.date().year()) else {
            return false;
        };
        datetime >= start && datetime < end
    }

    fn is_daylight_at_local(self, datetime: NaiveDateTime) -> bool {
        if self == Self::Utc {
            return false;
        }
        // Both replay venues follow the post-2007 US DST calendar.
        let year = datetime.date().year();
        let Some(start_date) = nth_weekday_of_month(year, 3, Weekday::Sun, 2) else {
            return false;
        };
        let Some(end_date) = nth_weekday_of_month(year, 11, Weekday::Sun, 1) else {
            return false;
        };
        let start = start_date.and_hms_opt(2, 0, 0).expect("valid DST time");
        let end = end_date.and_hms_opt(2, 0, 0).expect("valid DST time");
        datetime >= start && datetime < end
    }

    fn utc_daylight_boundaries(self, year: i32) -> Option<(NaiveDateTime, NaiveDateTime)> {
        let start_date = nth_weekday_of_month(year, 3, Weekday::Sun, 2)?;
        let end_date = nth_weekday_of_month(year, 11, Weekday::Sun, 1)?;
        let (start_hour, end_hour) = match self {
            Self::NewYork => (7, 6),
            Self::Chicago => (8, 7),
            Self::Utc => return None,
        };
        Some((
            start_date.and_hms_opt(start_hour, 0, 0)?,
            end_date.and_hms_opt(end_hour, 0, 0)?,
        ))
    }
}

fn replay_timezone(instrument: &Instrument) -> ReplayTimezone {
    match instrument.dataset.as_str() {
        "XNYS.PILLAR" => ReplayTimezone::NewYork,
        "GLBX.MDP3" => ReplayTimezone::Chicago,
        _ => ReplayTimezone::Utc,
    }
}

fn nth_weekday_of_month(
    year: i32,
    month: u32,
    weekday: Weekday,
    occurrence: u32,
) -> Option<NaiveDate> {
    let first = NaiveDate::from_ymd_opt(year, month, 1)?;
    let days_until = (7 + weekday.num_days_from_monday() as i64
        - first.weekday().num_days_from_monday() as i64)
        % 7;
    first.checked_add_signed(Duration::days(
        days_until + 7 * i64::from(occurrence.saturating_sub(1)),
    ))
}

fn format_human_cursor(timestamp_ms: u64, timezone: ReplayTimezone) -> String {
    cursor_datetime(timestamp_ms)
        .map(|timestamp| {
            let local = timezone.local_at_utc(timestamp);
            format!(
                "{} · {} {}",
                format_human_date(&local.datetime.format(DATE_FORMAT).to_string()),
                local.datetime.format(TIME_FORMAT),
                local.abbreviation,
            )
        })
        .unwrap_or_else(|| "Invalid replay time".into())
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "?",
    }
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
    fn replay_frame_emits_trades_before_the_final_complete_book() {
        let ticker_info = TickerInfo::new(
            Ticker::new("NKE", Exchange::DatabentoReplay),
            0.01,
            1.0,
            None,
        );
        let frame = ReplayFrame {
            book_events: Vec::new(),
            snapshot: Some(replay::BookSnapshot {
                ts_recv_ns: 100_100_000,
                bids: Vec::new(),
                asks: Vec::new(),
            }),
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
    fn venue_local_cursor_uses_daylight_and_standard_labels() {
        let summer = DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDate::from_ymd_opt(2026, 6, 1)
                .unwrap()
                .and_hms_opt(13, 30, 0)
                .unwrap(),
            Utc,
        );
        let winter = DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDate::from_ymd_opt(2026, 1, 5)
                .unwrap()
                .and_hms_opt(15, 30, 0)
                .unwrap(),
            Utc,
        );

        let new_york_summer = ReplayTimezone::NewYork.local_at_utc(summer);
        assert_eq!(
            new_york_summer.datetime.format(TIME_FORMAT).to_string(),
            "09:30:00"
        );
        assert_eq!(new_york_summer.abbreviation, "EDT");

        let chicago_summer = ReplayTimezone::Chicago.local_at_utc(summer);
        assert_eq!(
            chicago_summer.datetime.format(TIME_FORMAT).to_string(),
            "08:30:00"
        );
        assert_eq!(chicago_summer.abbreviation, "CDT");

        assert_eq!(
            ReplayTimezone::NewYork.local_at_utc(winter).abbreviation,
            "EST"
        );
        assert_eq!(
            ReplayTimezone::Chicago.local_at_utc(winter).abbreviation,
            "CST"
        );
    }

    #[test]
    fn local_seek_time_round_trips_to_utc() {
        let local = NaiveDate::from_ymd_opt(2026, 6, 1)
            .unwrap()
            .and_hms_opt(9, 30, 0)
            .unwrap();
        let utc = ReplayTimezone::NewYork.utc_from_local(local);

        assert_eq!(
            utc.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-06-01 13:30:00"
        );
        assert_eq!(ReplayTimezone::NewYork.local_at_utc(utc).datetime, local);
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
                        start_ts_ms: 1_780_617_600_000,
                        end_ts_ms: 1_780_693_200_134,
                        source_instrument_id: None,
                        source_symbol: Some("NKE".into()),
                        mbo_file: String::new(),
                        trades_file: String::new(),
                        status_file: None,
                        raw_l3_files: Vec::new(),
                        raw_status_files: Vec::new(),
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

        assert_eq!(controller.seek_date, Some(downloaded_date));
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
