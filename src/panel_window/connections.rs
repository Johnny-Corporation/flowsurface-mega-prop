use crate::{
    screen::dashboard::panel,
    style,
    trading_state::{LiveOpenOrder, LiveOrderSide, LivePosition, LiveTradingSnapshot},
};

use data::config::connection_credentials::{
    ConnectionCredentialRef, ConnectionSecret, delete_connection_secret, load_connection_secret,
    load_or_create_device_vault_key, save_connection_secret,
};
use exchange::adapter::{
    Exchange, FuturesOpenType, FuturesOrderRequest, FuturesOrderSide, FuturesOrderType,
    MexcBlockingPrivateClient, MexcCredentials, MexcFuturesResponse,
    available_balances_from_futures_assets, available_balances_from_spot_account,
};
use iced::{
    Alignment, Element, Length, Theme, padding,
    widget::{button, column, container, pick_list, row, text, text_input},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    cmp::Reverse,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use super::{
    ConnectionAction, ConnectionExchange, ConnectionMarket, ConnectionMode, PanelMessage,
    panel_card, value_box,
};

const DEFAULT_ROWS: [(ConnectionExchange, ConnectionMarket, ConnectionMode); 4] = [
    (
        ConnectionExchange::Okx,
        ConnectionMarket::Spot,
        ConnectionMode::View,
    ),
    (
        ConnectionExchange::Okx,
        ConnectionMarket::Futures,
        ConnectionMode::View,
    ),
    (
        ConnectionExchange::Mexc,
        ConnectionMarket::Spot,
        ConnectionMode::View,
    ),
    (
        ConnectionExchange::Mexc,
        ConnectionMarket::Futures,
        ConnectionMode::View,
    ),
];
const CONNECTIONS_FILE: &str = "connections.json";
const MEXC_SPOT_PUBLIC_PING_URL: &str = "https://api.mexc.com/api/v3/time";
const MEXC_FUTURES_PUBLIC_PING_URL: &str = "https://api.mexc.com/api/v1/contract/detail";
const CONNECTION_TEST_TIMEOUT: Duration = Duration::from_secs(6);
const TRADING_STATE_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(crate) struct ConnectionPanelState {
    rows: Vec<ConnectionRow>,
    draft: Option<ConnectionDraft>,
    logs: Vec<String>,
    last_action: String,
    autoconnect_enabled: bool,
    last_connection_id: Option<String>,
    next_connection_id: u64,
    probe_tx: Sender<ConnectionProbeResult>,
    probe_rx: Receiver<ConnectionProbeResult>,
    next_trading_state_refresh: Option<Instant>,
    trading_state_refresh_in_flight: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ConnectionAccountSummary {
    pub status: String,
    pub asset_count: usize,
    pub non_zero_asset_count: usize,
    pub market_data: String,
    pub trading: String,
    pub balances: Vec<ConnectionAccountBalance>,
}

#[derive(Debug, Clone)]
pub(crate) struct ConnectionAccountBalance {
    pub source: String,
    pub asset: String,
    pub available: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ConnectionTradeRecord {
    pub timestamp_ms: i64,
    pub timestamp: String,
    pub source: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub order_price: String,
    pub average_price: String,
    pub quantity: String,
    pub filled_quantity: String,
    pub pnl: String,
    pub fee: String,
    pub state: String,
    pub order_id: String,
}

impl Default for ConnectionPanelState {
    fn default() -> Self {
        let (probe_tx, probe_rx) = mpsc::channel();

        let (rows, next_connection_id, autoconnect_enabled, last_connection_id, logs) =
            if let Some(saved) = load_saved_connections() {
                (
                    saved.rows,
                    saved.next_connection_id,
                    saved.autoconnect_enabled,
                    saved.last_connection_id,
                    vec!["[connections] Saved metadata loaded".to_string()],
                )
            } else {
                (
                    DEFAULT_ROWS
                        .into_iter()
                        .map(|(exchange, market, mode)| {
                            let id = format!(
                                "default-{}-{}-{}",
                                exchange.storage_key(),
                                market.storage_key(),
                                mode.storage_key()
                            );
                            ConnectionRow::new(id, exchange, market, mode, false)
                        })
                        .collect(),
                    1,
                    true,
                    None,
                    vec![
                        "[connections] Defaults loaded: OKX/MEXC spot and futures in view mode"
                            .to_string(),
                    ],
                )
            };

        Self {
            rows,
            draft: None,
            logs,
            last_action: "Ready".to_string(),
            autoconnect_enabled,
            last_connection_id,
            next_connection_id,
            probe_tx,
            probe_rx,
            next_trading_state_refresh: None,
            trading_state_refresh_in_flight: false,
        }
    }
}

impl ConnectionPanelState {
    pub(crate) fn tick(&mut self, now: std::time::Instant) {
        while let Ok(result) = self.probe_rx.try_recv() {
            self.apply_probe_result(result);
        }

        self.refresh_trading_state_if_due(now);
    }

    pub(crate) fn update(&mut self, action: ConnectionAction) {
        match action {
            ConnectionAction::Toggle(index) => self.toggle(index),
            ConnectionAction::AddConnection => self.start_draft(),
            ConnectionAction::DraftExchangeSelected(exchange) => {
                if let Some(draft) = self.draft.as_mut() {
                    draft.exchange = exchange;
                    draft.clear_credentials_if_view_mode();
                    self.last_action = "Exchange selected".to_string();
                }
            }
            ConnectionAction::DraftMarketSelected(market) => {
                if let Some(draft) = self.draft.as_mut() {
                    draft.market = market;
                    self.last_action = "Market selected".to_string();
                }
            }
            ConnectionAction::DraftModeSelected(mode) => {
                if let Some(draft) = self.draft.as_mut() {
                    draft.mode = mode;
                    draft.clear_credentials_if_view_mode();
                    self.last_action = "Mode selected".to_string();
                }
            }
            ConnectionAction::DraftAccessKeyChanged(value) => {
                if let Some(draft) = self.draft.as_mut() {
                    draft.access_key = value;
                }
            }
            ConnectionAction::DraftSecretKeyChanged(value) => {
                if let Some(draft) = self.draft.as_mut() {
                    draft.secret_key = value;
                }
            }
            ConnectionAction::AutoconnectChanged(enabled) => {
                self.autoconnect_enabled = enabled;
                self.last_action = "Auto-connect".to_string();
                self.push_log(format!(
                    "[connections] Auto-connect {}",
                    if enabled { "enabled" } else { "disabled" }
                ));
                self.persist_rows();
            }
            ConnectionAction::SaveDraft => self.save_draft(),
            ConnectionAction::CancelDraft => {
                self.draft = None;
                self.last_action = "Draft canceled".to_string();
            }
            ConnectionAction::Refresh => {
                self.last_action = "Refresh".to_string();
                let indices = self
                    .rows
                    .iter()
                    .enumerate()
                    .filter_map(|(index, row)| row.enabled.then_some(index))
                    .collect::<Vec<_>>();

                for index in indices {
                    self.start_connection_test(index);
                }

                self.push_log("[connections] Enabled connections retested".to_string());
            }
            ConnectionAction::Confirm => {
                self.last_action = "OK".to_string();
                self.push_log("[connections] Connection list accepted".to_string());
            }
            ConnectionAction::RowDelete(index) => self.delete(index),
        }
    }

    pub(crate) fn top_bar_status(&self) -> String {
        let Some(row) = self.active_connection_row() else {
            return if self.rows.is_empty() {
                "Connected: none (Connections empty)".to_string()
            } else {
                "Connected: none".to_string()
            };
        };

        format!(
            "Connected: {}, {}, {} access",
            row.exchange,
            row.market.status_label(),
            row.mode.access_label(),
        )
    }

    pub(crate) fn active_market_exchanges(&self) -> Vec<Exchange> {
        let mut exchanges = Vec::new();

        for row in self.rows.iter().filter(|row| row.is_connected()) {
            for exchange in row.market_exchanges() {
                if !exchanges.contains(&exchange) {
                    exchanges.push(exchange);
                }
            }
        }

        exchanges
    }

    pub(crate) fn credential_storage_status(&self) -> &'static str {
        "Device-local encrypted storage"
    }

    pub(crate) fn autoconnect_enabled(&self) -> bool {
        self.autoconnect_enabled
    }

    pub(crate) fn account_summary(&self) -> ConnectionAccountSummary {
        let connected_rows = self
            .rows
            .iter()
            .filter(|row| row.is_connected())
            .collect::<Vec<_>>();

        let mut asset_count = 0;
        let mut non_zero_asset_count = 0;
        let mut balances = Vec::new();

        for row in &connected_rows {
            asset_count += row.balances.len();
            for balance in &row.balances {
                if is_zero_amount(&balance.available) {
                    continue;
                }

                non_zero_asset_count += 1;
                balances.push(ConnectionAccountBalance {
                    source: row.label(),
                    asset: balance.asset.clone(),
                    available: balance.available.clone(),
                });
            }
        }

        ConnectionAccountSummary {
            status: self.top_bar_status(),
            asset_count,
            non_zero_asset_count,
            market_data: if connected_rows.is_empty() {
                "Disabled until a connection is ON".to_string()
            } else {
                connected_rows
                    .iter()
                    .map(|row| row.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            trading: connected_rows
                .iter()
                .find(|row| row.mode == ConnectionMode::Trade)
                .map(|row| format!("Enabled via {}", row.label()))
                .unwrap_or_else(|| "No active trading connection".to_string()),
            balances,
        }
    }

    pub(crate) fn trade_history(&self) -> Vec<ConnectionTradeRecord> {
        let mut trades = self
            .rows
            .iter()
            .flat_map(|row| row.trades.iter().cloned())
            .collect::<Vec<_>>();
        trades.sort_by_key(|trade| Reverse(trade.timestamp_ms));
        trades.truncate(200);
        trades
    }

    pub(crate) fn live_trading_snapshot(&self) -> LiveTradingSnapshot {
        let mut snapshot = LiveTradingSnapshot::default();

        for row in self.rows.iter().filter(|row| row.is_connected()) {
            snapshot.open_orders.extend(row.open_orders.iter().cloned());
            snapshot.positions.extend(row.positions.iter().cloned());
        }

        snapshot
    }

    pub(crate) fn handle_panel_action(&mut self, action: panel::Action) {
        match action {
            panel::Action::PlaceLimitOrder(intent) => self.place_limit_order(intent),
            panel::Action::CancelAllOrders(ticker_info) => {
                self.cancel_all_orders(ticker_info);
            }
        }
    }

    pub(crate) fn autoconnect(&mut self) {
        if !self.autoconnect_enabled {
            self.last_action = "Autoconnect disabled".to_string();
            return;
        }

        let Some(index) = self.last_autoconnect_index() else {
            self.last_action = "Autoconnect skipped".to_string();
            return;
        };

        self.last_action = "Autoconnect".to_string();
        if let Some(row) = self.rows.get_mut(index) {
            row.enabled = true;
            row.test_state = ConnectionTestState::Loading;
            row.balances.clear();
            row.trades.clear();
            row.open_orders.clear();
            row.positions.clear();
            let label = row.label();
            self.push_log(format!("[connections] Autoconnect testing {label}"));
        }

        self.persist_rows();
        self.start_connection_test(index);
    }

    fn last_autoconnect_index(&self) -> Option<usize> {
        self.last_connection_id
            .as_ref()
            .and_then(|id| {
                self.rows
                    .iter()
                    .position(|row| &row.id == id && row.is_autoconnect_eligible())
            })
            .or_else(|| {
                self.rows
                    .iter()
                    .rposition(ConnectionRow::is_autoconnect_eligible)
            })
    }

    pub(crate) fn last_connection_label(&self) -> String {
        self.last_connection_id
            .as_ref()
            .and_then(|id| self.rows.iter().find(|row| &row.id == id))
            .map(ConnectionRow::label)
            .unwrap_or_else(|| "None".to_string())
    }

    fn toggle(&mut self, index: usize) {
        let Some(row) = self.rows.get_mut(index) else {
            return;
        };

        let label = row.label();

        if row.enabled {
            row.enabled = false;
            row.test_state = ConnectionTestState::Idle;
            row.balances.clear();
            row.trades.clear();
            row.open_orders.clear();
            row.positions.clear();
            self.last_action = "Toggle".to_string();
            self.push_log(format!("[connections] {label} disabled"));
            self.persist_rows();
            return;
        }

        row.enabled = true;
        row.test_state = ConnectionTestState::Loading;
        row.balances.clear();
        row.trades.clear();
        row.open_orders.clear();
        row.positions.clear();
        self.last_action = "Toggle".to_string();
        self.push_log(format!("[connections] Testing {label}"));
        self.persist_rows();
        self.start_connection_test(index);
    }

    fn start_draft(&mut self) {
        if self.draft.is_none() {
            self.draft = Some(ConnectionDraft::default());
        }

        self.last_action = "Add connection".to_string();
    }

    fn save_draft(&mut self) {
        let Some(mut draft) = self.draft.take() else {
            return;
        };

        let existing_index = self.rows.iter().position(|row| {
            row.exchange == draft.exchange && row.market == draft.market && row.mode == draft.mode
        });
        let id = if let Some(index) = existing_index {
            self.rows[index].id.clone()
        } else {
            let id = format!(
                "{}-{}-{}-{}",
                draft.exchange.storage_key(),
                draft.market.storage_key(),
                draft.mode.storage_key(),
                self.next_connection_id
            );
            self.next_connection_id += 1;
            id
        };

        let mut row =
            ConnectionRow::new(id.clone(), draft.exchange, draft.market, draft.mode, false);

        if draft.mode == ConnectionMode::Trade {
            let reference = match ConnectionCredentialRef::new(&id) {
                Ok(reference) => reference,
                Err(error) => {
                    draft.error = Some(error.clone());
                    self.draft = Some(draft);
                    self.last_action = "Credential id error".to_string();
                    self.push_log(format!("[credentials] {error}"));
                    return;
                }
            };

            let secret = match ConnectionSecret::new(&draft.access_key, &draft.secret_key) {
                Ok(secret) => secret,
                Err(error) => {
                    draft.error = Some(error.clone());
                    self.draft = Some(draft);
                    self.last_action = "Credential validation failed".to_string();
                    self.push_log(format!("[credentials] {error}"));
                    return;
                }
            };

            let vault_key = match load_or_create_device_vault_key() {
                Ok(vault_key) => vault_key,
                Err(error) => {
                    draft.error = Some(error.clone());
                    self.draft = Some(draft);
                    self.last_action = "Credential save failed".to_string();
                    self.push_log(format!("[credentials] {error}"));
                    return;
                }
            };

            if let Err(error) = save_connection_secret(&reference, &secret, &vault_key) {
                draft.error = Some(error.clone());
                self.draft = Some(draft);
                self.last_action = "Credential save failed".to_string();
                self.push_log(format!("[credentials] {error}"));
                return;
            }

            row.credentials = CredentialState::Saved {
                reference,
                access_key_hint: secret.access_key_hint(),
            };
        }

        let label = row.label();
        if let Some(index) = existing_index {
            self.rows[index] = row;
            self.last_action = "Connection updated".to_string();
            self.push_log(format!("[connections] {label} updated"));
        } else {
            self.rows.push(row);
            self.last_action = "Connection saved".to_string();
            self.push_log(format!("[connections] {label} saved"));
        }
        self.persist_rows();
    }

    fn delete(&mut self, index: usize) {
        if index >= self.rows.len() {
            return;
        }

        let row = self.rows.remove(index);
        if let CredentialState::Saved { reference, .. } = &row.credentials {
            let _ = delete_connection_secret(reference);
        }

        self.last_action = "Delete".to_string();
        self.push_log(format!("[connections] {} removed", row.label()));
        self.persist_rows();
    }

    fn push_log(&mut self, message: String) {
        self.logs.push(message);

        while self.logs.len() > 6 {
            self.logs.remove(0);
        }
    }

    fn enabled_count(&self) -> usize {
        self.rows.iter().filter(|row| row.enabled).count()
    }

    fn active_connection_row(&self) -> Option<&ConnectionRow> {
        self.rows
            .iter()
            .filter(|row| row.is_connected())
            .max_by_key(|row| match row.mode {
                ConnectionMode::Trade => 1,
                ConnectionMode::View => 0,
            })
    }

    fn persist_rows(&mut self) {
        if let Err(error) = save_connections(self) {
            self.push_log(format!("[connections] Failed to save metadata: {error}"));
        }
    }

    fn start_connection_test(&mut self, index: usize) {
        let Some(row) = self.rows.get(index) else {
            return;
        };

        let Some(spec) = ConnectionProbeSpec::from_row(row) else {
            let status = row.exchange.draft_status(row.mode).to_string();
            let row_id = row.id.clone();
            self.apply_probe_result(ConnectionProbeResult {
                row_id,
                outcome: Err(status),
                balances: Vec::new(),
                trades: Vec::new(),
                open_orders: Vec::new(),
                positions: Vec::new(),
                state_only: false,
                log_only: None,
            });
            return;
        };

        let tx = self.probe_tx.clone();
        thread::spawn(move || {
            let _ = tx.send(run_connection_probe(spec));
        });
    }

    fn place_limit_order(&mut self, intent: panel::LimitOrderIntent) {
        let Some(row) = self.active_trading_row() else {
            self.last_action = "Order rejected".to_string();
            self.push_log("[trading] No active trading connection is ON".to_string());
            return;
        };

        if row.exchange != ConnectionExchange::Mexc || row.market != ConnectionMarket::Futures {
            self.last_action = "Order rejected".to_string();
            self.push_log(
                "[trading] Only MEXC Futures live orders are wired right now".to_string(),
            );
            return;
        }

        let CredentialState::Saved { reference, .. } = &row.credentials else {
            self.last_action = "Order rejected".to_string();
            self.push_log(
                "[trading] Active trading connection has no saved credentials".to_string(),
            );
            return;
        };

        let row_id = row.id.clone();
        let order = LiveLimitOrderSpec::new(reference.clone(), intent);
        self.last_action = "Order submitted".to_string();
        self.push_log(format!(
            "[trading] Submitting {} {} {} @ {}",
            order.symbol, order.side_label, order.quantity, order.price
        ));

        let tx = self.probe_tx.clone();
        thread::spawn(move || {
            let result = run_live_limit_order(row_id, order);
            let _ = tx.send(result);
        });
    }

    fn cancel_all_orders(&mut self, ticker_info: exchange::TickerInfo) {
        let Some(row) = self.active_trading_row() else {
            self.last_action = "Cancel rejected".to_string();
            self.push_log("[trading] No active trading connection is ON".to_string());
            return;
        };

        if row.exchange != ConnectionExchange::Mexc || row.market != ConnectionMarket::Futures {
            self.last_action = "Cancel rejected".to_string();
            self.push_log("[trading] Only MEXC Futures cancel-all is wired right now".to_string());
            return;
        }

        let CredentialState::Saved { reference, .. } = &row.credentials else {
            self.last_action = "Cancel rejected".to_string();
            self.push_log(
                "[trading] Active trading connection has no saved credentials".to_string(),
            );
            return;
        };

        let row_id = row.id.clone();
        let (symbol, _) = ticker_info.ticker.to_full_symbol_and_type();
        let reference = reference.clone();
        self.last_action = "Cancel all submitted".to_string();
        self.push_log(format!("[trading] Cancel all submitted for {symbol}"));

        let tx = self.probe_tx.clone();
        thread::spawn(move || {
            let result = run_cancel_all_orders(row_id, reference, &symbol);
            let _ = tx.send(result);
        });
    }

    fn active_trading_row(&self) -> Option<&ConnectionRow> {
        self.rows
            .iter()
            .find(|row| row.is_connected() && row.mode == ConnectionMode::Trade)
    }

    fn refresh_trading_state_if_due(&mut self, now: Instant) {
        if self.trading_state_refresh_in_flight {
            return;
        }

        let Some((row_id, reference)) = self.active_trading_row().and_then(|row| {
            if row.exchange != ConnectionExchange::Mexc || row.market != ConnectionMarket::Futures {
                return None;
            }

            let CredentialState::Saved { reference, .. } = &row.credentials else {
                return None;
            };

            Some((row.id.clone(), reference.clone()))
        }) else {
            self.next_trading_state_refresh = None;
            return;
        };

        if self
            .next_trading_state_refresh
            .is_some_and(|next| now < next)
        {
            return;
        }

        self.next_trading_state_refresh = Some(now + TRADING_STATE_REFRESH_INTERVAL);
        self.trading_state_refresh_in_flight = true;
        let tx = self.probe_tx.clone();
        thread::spawn(move || {
            let result = run_trading_state_refresh(row_id, reference);
            let _ = tx.send(result);
        });
    }

    fn apply_probe_result(&mut self, result: ConnectionProbeResult) {
        if let Some(message) = result.log_only {
            self.last_action = "Trading".to_string();
            self.push_log(message);
            return;
        }

        if result.state_only {
            self.trading_state_refresh_in_flight = false;
            match result.outcome {
                Ok(message) => {
                    let is_poll_update = message.contains("State refreshed");
                    let mut should_log = !is_poll_update;
                    if let Some(row) = self.rows.iter_mut().find(|row| row.id == result.row_id) {
                        let changed = row.open_orders != result.open_orders
                            || row.positions != result.positions;
                        should_log |= changed;
                        row.open_orders = result.open_orders;
                        row.positions = result.positions;
                    }
                    self.last_action = "Trading state".to_string();
                    if should_log {
                        self.push_log(message);
                    }
                }
                Err(error) => {
                    self.last_action = "Trading state failed".to_string();
                    self.push_log(format!("[trading] {error}"));
                }
            }
            return;
        }

        let Some(row) = self.rows.iter_mut().find(|row| row.id == result.row_id) else {
            return;
        };

        let log_message = match result.outcome {
            Ok(message) => {
                let label = row.label();
                row.enabled = true;
                row.test_state = ConnectionTestState::Success(message.clone());
                row.balances = result.balances;
                row.trades = result.trades;
                row.open_orders = result.open_orders;
                row.positions = result.positions;
                self.last_connection_id = Some(row.id.clone());
                format!("[connections] {label} connected")
            }
            Err(error) => {
                let label = row.label();
                row.enabled = false;
                row.test_state = ConnectionTestState::Error(error.clone());
                row.balances.clear();
                row.trades.clear();
                row.open_orders.clear();
                row.positions.clear();
                format!("[connections] {label} failed: {error}")
            }
        };

        self.push_log(log_message);

        self.persist_rows();
    }
}

#[derive(Debug, Clone)]
struct ConnectionRow {
    id: String,
    enabled: bool,
    exchange: ConnectionExchange,
    market: ConnectionMarket,
    mode: ConnectionMode,
    credentials: CredentialState,
    test_state: ConnectionTestState,
    balances: Vec<exchange::adapter::MexcAvailableBalance>,
    trades: Vec<ConnectionTradeRecord>,
    open_orders: Vec<LiveOpenOrder>,
    positions: Vec<LivePosition>,
}

impl ConnectionRow {
    fn new(
        id: String,
        exchange: ConnectionExchange,
        market: ConnectionMarket,
        mode: ConnectionMode,
        enabled: bool,
    ) -> Self {
        Self {
            id,
            enabled,
            exchange,
            market,
            mode,
            credentials: CredentialState::NotRequired,
            test_state: ConnectionTestState::Idle,
            balances: Vec::new(),
            trades: Vec::new(),
            open_orders: Vec::new(),
            positions: Vec::new(),
        }
    }

    fn label(&self) -> String {
        format!("{} {} {}", self.exchange, self.market, self.mode)
    }

    fn status(&self) -> String {
        match &self.test_state {
            ConnectionTestState::Loading => return "Testing connection...".to_string(),
            ConnectionTestState::Success(message) => {
                if !self.balances.is_empty() {
                    let balance = format_balance_summary(&self.balances);
                    return format!("{message}; {balance}");
                }
                return message.clone();
            }
            ConnectionTestState::Error(error) => return format!("Failed: {error}"),
            ConnectionTestState::Idle => {}
        }

        match self.exchange {
            ConnectionExchange::Mexc => match self.mode {
                ConnectionMode::View => "Off".to_string(),
                ConnectionMode::Trade => match self.credentials {
                    CredentialState::Saved { .. } => "Off".to_string(),
                    _ => "MEXC API keys required".to_string(),
                },
            },
            ConnectionExchange::Bybit => "Will be implemented soon".to_string(),
            _ => "Not implemented yet".to_string(),
        }
    }

    fn credential_label(&self) -> String {
        match &self.credentials {
            CredentialState::NotRequired => "Not required".to_string(),
            CredentialState::Saved {
                access_key_hint, ..
            } => format!("Saved ({access_key_hint})"),
        }
    }

    fn is_connected(&self) -> bool {
        self.enabled && matches!(self.test_state, ConnectionTestState::Success(_))
    }

    fn is_autoconnect_eligible(&self) -> bool {
        if self.exchange != ConnectionExchange::Mexc {
            return false;
        }

        match self.mode {
            ConnectionMode::View => true,
            ConnectionMode::Trade => matches!(self.credentials, CredentialState::Saved { .. }),
        }
    }

    fn market_exchanges(&self) -> Vec<Exchange> {
        match (self.exchange, self.market) {
            (ConnectionExchange::Mexc, ConnectionMarket::Spot) => vec![Exchange::MexcSpot],
            (ConnectionExchange::Mexc, ConnectionMarket::Futures) => {
                vec![Exchange::MexcLinear, Exchange::MexcInverse]
            }
            (ConnectionExchange::Bybit, ConnectionMarket::Spot) => vec![Exchange::BybitSpot],
            (ConnectionExchange::Bybit, ConnectionMarket::Futures) => {
                vec![Exchange::BybitLinear, Exchange::BybitInverse]
            }
            (ConnectionExchange::Okx, ConnectionMarket::Spot) => vec![Exchange::OkexSpot],
            (ConnectionExchange::Okx, ConnectionMarket::Futures) => {
                vec![Exchange::OkexLinear, Exchange::OkexInverse]
            }
            (ConnectionExchange::Binance, ConnectionMarket::Spot) => vec![Exchange::BinanceSpot],
            (ConnectionExchange::Binance, ConnectionMarket::Futures) => {
                vec![Exchange::BinanceLinear, Exchange::BinanceInverse]
            }
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
enum ConnectionTestState {
    Idle,
    Loading,
    Success(String),
    Error(String),
}

#[derive(Debug, Clone)]
enum CredentialState {
    NotRequired,
    Saved {
        reference: ConnectionCredentialRef,
        access_key_hint: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedConnections {
    rows: Vec<PersistedConnectionRow>,
    #[serde(default = "default_autoconnect_enabled")]
    autoconnect_enabled: bool,
    #[serde(default)]
    last_connection_id: Option<String>,
    next_connection_id: u64,
}

#[derive(Debug)]
struct LoadedConnections {
    rows: Vec<ConnectionRow>,
    next_connection_id: u64,
    autoconnect_enabled: bool,
    last_connection_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedConnectionRow {
    #[serde(default)]
    id: Option<String>,
    enabled: bool,
    exchange: ConnectionExchange,
    market: ConnectionMarket,
    mode: ConnectionMode,
    credential_id: Option<String>,
    access_key_hint: Option<String>,
}

impl From<&ConnectionPanelState> for PersistedConnections {
    fn from(state: &ConnectionPanelState) -> Self {
        Self {
            rows: state
                .rows
                .iter()
                .map(PersistedConnectionRow::from)
                .collect(),
            autoconnect_enabled: state.autoconnect_enabled,
            last_connection_id: state.last_connection_id.clone(),
            next_connection_id: state.next_connection_id,
        }
    }
}

impl From<&ConnectionRow> for PersistedConnectionRow {
    fn from(row: &ConnectionRow) -> Self {
        let (credential_id, access_key_hint) = match &row.credentials {
            CredentialState::NotRequired => (None, None),
            CredentialState::Saved {
                reference,
                access_key_hint,
            } => (
                Some(reference.id().to_string()),
                Some(access_key_hint.clone()),
            ),
        };

        Self {
            id: Some(row.id.clone()),
            enabled: row.enabled,
            exchange: row.exchange,
            market: row.market,
            mode: row.mode,
            credential_id,
            access_key_hint,
        }
    }
}

impl TryFrom<PersistedConnectionRow> for ConnectionRow {
    type Error = String;

    fn try_from(value: PersistedConnectionRow) -> Result<Self, Self::Error> {
        let credentials = match (value.credential_id, value.access_key_hint) {
            (Some(id), Some(access_key_hint)) => CredentialState::Saved {
                reference: ConnectionCredentialRef::new(id)?,
                access_key_hint,
            },
            _ => CredentialState::NotRequired,
        };

        let id = value.id.unwrap_or_else(|| {
            format!(
                "{}-{}-{}",
                value.exchange.storage_key(),
                value.market.storage_key(),
                value.mode.storage_key()
            )
        });

        Ok(Self {
            id,
            enabled: false,
            exchange: value.exchange,
            market: value.market,
            mode: value.mode,
            credentials,
            test_state: ConnectionTestState::Idle,
            balances: Vec::new(),
            trades: Vec::new(),
            open_orders: Vec::new(),
            positions: Vec::new(),
        })
    }
}

fn load_saved_connections() -> Option<LoadedConnections> {
    let path = data::data_path(Some(CONNECTIONS_FILE));
    let contents = std::fs::read_to_string(path).ok()?;
    let persisted: PersistedConnections = serde_json::from_str(&contents).ok()?;
    let last_connection_id = persisted
        .last_connection_id
        .clone()
        .or_else(|| last_enabled_connection_id(&persisted.rows));
    let rows = persisted
        .rows
        .into_iter()
        .filter_map(|row| ConnectionRow::try_from(row).ok())
        .collect::<Vec<_>>();
    let rows = deduplicate_connection_rows(rows);

    if rows.is_empty() {
        return None;
    }

    Some(LoadedConnections {
        rows,
        next_connection_id: persisted.next_connection_id.max(1),
        autoconnect_enabled: persisted.autoconnect_enabled,
        last_connection_id,
    })
}

fn last_enabled_connection_id(rows: &[PersistedConnectionRow]) -> Option<String> {
    rows.iter().rev().find(|row| row.enabled).and_then(|row| {
        row.id.clone().or_else(|| {
            Some(format!(
                "{}-{}-{}",
                row.exchange.storage_key(),
                row.market.storage_key(),
                row.mode.storage_key()
            ))
        })
    })
}

fn deduplicate_connection_rows(rows: Vec<ConnectionRow>) -> Vec<ConnectionRow> {
    rows.into_iter().fold(Vec::new(), |mut deduped, row| {
        if let Some(index) = deduped.iter().position(|existing: &ConnectionRow| {
            existing.exchange == row.exchange
                && existing.market == row.market
                && existing.mode == row.mode
        }) {
            deduped[index] = row;
        } else {
            deduped.push(row);
        }

        deduped
    })
}

fn default_autoconnect_enabled() -> bool {
    true
}

fn save_connections(state: &ConnectionPanelState) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&PersistedConnections::from(state))
        .map_err(|error| error.to_string())?;
    data::write_json_to_file(&json, CONNECTIONS_FILE).map_err(|error| error.to_string())
}

#[derive(Debug, Clone)]
struct ConnectionProbeSpec {
    row_id: String,
    market: ConnectionMarket,
    mode: ConnectionMode,
    credential_ref: Option<ConnectionCredentialRef>,
    access_key_hint: Option<String>,
}

impl ConnectionProbeSpec {
    fn from_row(row: &ConnectionRow) -> Option<Self> {
        if row.exchange != ConnectionExchange::Mexc {
            return None;
        }

        let (credential_ref, access_key_hint) = match &row.credentials {
            CredentialState::NotRequired => (None, None),
            CredentialState::Saved {
                reference,
                access_key_hint,
            } => (Some(reference.clone()), Some(access_key_hint.clone())),
        };

        Some(Self {
            row_id: row.id.clone(),
            market: row.market,
            mode: row.mode,
            credential_ref,
            access_key_hint,
        })
    }
}

#[derive(Debug, Clone)]
struct ConnectionProbeResult {
    row_id: String,
    outcome: Result<String, String>,
    balances: Vec<exchange::adapter::MexcAvailableBalance>,
    trades: Vec<ConnectionTradeRecord>,
    open_orders: Vec<LiveOpenOrder>,
    positions: Vec<LivePosition>,
    state_only: bool,
    log_only: Option<String>,
}

impl ConnectionProbeResult {
    fn trading_log(message: String) -> Self {
        Self {
            row_id: String::new(),
            outcome: Ok(String::new()),
            balances: Vec::new(),
            trades: Vec::new(),
            open_orders: Vec::new(),
            positions: Vec::new(),
            state_only: false,
            log_only: Some(message),
        }
    }

    fn trading_state(
        row_id: String,
        outcome: Result<String, String>,
        snapshot: LiveTradingSnapshot,
    ) -> Self {
        Self {
            row_id,
            outcome,
            balances: Vec::new(),
            trades: Vec::new(),
            open_orders: snapshot.open_orders,
            positions: snapshot.positions,
            state_only: true,
            log_only: None,
        }
    }
}

fn run_connection_probe(spec: ConnectionProbeSpec) -> ConnectionProbeResult {
    let result = match spec.mode {
        ConnectionMode::View => probe_mexc_public(spec.market),
        ConnectionMode::Trade => probe_mexc_private(&spec),
    };

    match result {
        Ok(success) => ConnectionProbeResult {
            row_id: spec.row_id,
            outcome: Ok(success.message),
            balances: success.balances,
            trades: success.trades,
            open_orders: success.open_orders,
            positions: success.positions,
            state_only: false,
            log_only: None,
        },
        Err(error) => ConnectionProbeResult {
            row_id: spec.row_id,
            outcome: Err(error),
            balances: Vec::new(),
            trades: Vec::new(),
            open_orders: Vec::new(),
            positions: Vec::new(),
            state_only: false,
            log_only: None,
        },
    }
}

#[derive(Debug, Clone)]
struct ProbeSuccess {
    message: String,
    balances: Vec<exchange::adapter::MexcAvailableBalance>,
    trades: Vec<ConnectionTradeRecord>,
    open_orders: Vec<LiveOpenOrder>,
    positions: Vec<LivePosition>,
}

#[derive(Debug, Clone)]
struct LiveLimitOrderSpec {
    credential_ref: ConnectionCredentialRef,
    symbol: String,
    side: FuturesOrderSide,
    side_label: &'static str,
    price: String,
    quantity: String,
}

impl LiveLimitOrderSpec {
    fn new(credential_ref: ConnectionCredentialRef, intent: panel::LimitOrderIntent) -> Self {
        let (symbol, _) = intent.ticker_info.ticker.to_full_symbol_and_type();
        let side = match intent.side {
            panel::OrderSide::Buy => FuturesOrderSide::OpenLong,
            panel::OrderSide::Sell => FuturesOrderSide::OpenShort,
        };
        let side_label = match intent.side {
            panel::OrderSide::Buy => "BUY",
            panel::OrderSide::Sell => "SELL",
        };
        let price = intent.price.to_string(intent.ticker_info.min_ticksize);
        let quantity = format!("{:.0}", intent.quantity.max(1.0).round());

        Self {
            credential_ref,
            symbol,
            side,
            side_label,
            price,
            quantity,
        }
    }
}

fn run_live_limit_order(row_id: String, spec: LiveLimitOrderSpec) -> ConnectionProbeResult {
    match try_run_live_limit_order(spec) {
        Ok((message, Some(snapshot))) => ConnectionProbeResult::trading_state(
            row_id,
            Ok(format!("[trading] {message}")),
            snapshot,
        ),
        Ok((message, None)) => ConnectionProbeResult::trading_log(format!("[trading] {message}")),
        Err(error) => ConnectionProbeResult::trading_state(
            row_id,
            Err(format!("Order failed: {error}")),
            LiveTradingSnapshot::default(),
        ),
    }
}

fn try_run_live_limit_order(
    spec: LiveLimitOrderSpec,
) -> Result<(String, Option<LiveTradingSnapshot>), String> {
    let vault_key = load_or_create_device_vault_key()?;
    let secret = load_connection_secret(&spec.credential_ref, &vault_key)?.ok_or_else(|| {
        "Saved API keys were not found in the local credential vault. Delete and re-add this connection.".to_string()
    })?;
    let credentials = MexcCredentials::new(secret.access_key(), secret.secret_key())?;
    let client =
        MexcBlockingPrivateClient::new(credentials, None).map_err(|error| error.to_string())?;
    let external_oid = format!("fs{}", chrono::Utc::now().timestamp_millis());
    let request = FuturesOrderRequest::new(
        &spec.symbol,
        &spec.price,
        &spec.quantity,
        spec.side,
        FuturesOrderType::PostOnly,
        FuturesOpenType::Cross,
    )
    .with_leverage(1)
    .with_external_oid(external_oid);

    let response = client
        .futures_place_order(&request)
        .map_err(|error| error.to_string())?;
    let order_id = response
        .data
        .as_ref()
        .and_then(value_as_display_string)
        .unwrap_or_else(|| "unknown order id".to_string());
    let snapshot = match futures_trading_snapshot(&client) {
        Ok(snapshot) => Some(snapshot),
        Err(error) => {
            return Ok((
                format!(
                    "{} {} @ {} accepted as {}; state refresh failed: {}",
                    spec.side_label, spec.quantity, spec.price, order_id, error
                ),
                None,
            ));
        }
    };
    let open_orders = snapshot
        .as_ref()
        .map(|snapshot| snapshot.for_symbol(&spec.symbol).open_orders.len())
        .unwrap_or_default();

    Ok((
        format!(
            "{} {} @ {} accepted as {}; open orders visible: {}",
            spec.side_label, spec.quantity, spec.price, order_id, open_orders
        ),
        snapshot,
    ))
}

fn run_cancel_all_orders(
    row_id: String,
    reference: ConnectionCredentialRef,
    symbol: &str,
) -> ConnectionProbeResult {
    match try_cancel_all_orders(reference, symbol) {
        Ok((message, Some(snapshot))) => ConnectionProbeResult::trading_state(
            row_id,
            Ok(format!("[trading] {message}")),
            snapshot,
        ),
        Ok((message, None)) => ConnectionProbeResult::trading_log(format!("[trading] {message}")),
        Err(error) => ConnectionProbeResult::trading_state(
            row_id,
            Err(format!("Cancel all failed: {error}")),
            LiveTradingSnapshot::default(),
        ),
    }
}

fn try_cancel_all_orders(
    reference: ConnectionCredentialRef,
    symbol: &str,
) -> Result<(String, Option<LiveTradingSnapshot>), String> {
    let vault_key = load_or_create_device_vault_key()?;
    let secret = load_connection_secret(&reference, &vault_key)?.ok_or_else(|| {
        "Saved API keys were not found in the local credential vault. Delete and re-add this connection.".to_string()
    })?;
    let credentials = MexcCredentials::new(secret.access_key(), secret.secret_key())?;
    let client =
        MexcBlockingPrivateClient::new(credentials, None).map_err(|error| error.to_string())?;
    let response = client
        .futures_cancel_all_orders(symbol)
        .map_err(|error| error.to_string())?;
    let snapshot = match futures_trading_snapshot(&client) {
        Ok(snapshot) => Some(snapshot),
        Err(error) => {
            return Ok((
                format!(
                    "Cancel all accepted for {symbol}; success={} code={}; state refresh failed: {}",
                    response.success, response.code, error
                ),
                None,
            ));
        }
    };

    Ok((
        format!(
            "Cancel all accepted for {symbol}; success={} code={}",
            response.success, response.code
        ),
        snapshot,
    ))
}

fn run_trading_state_refresh(
    row_id: String,
    reference: ConnectionCredentialRef,
) -> ConnectionProbeResult {
    match try_trading_state_refresh(&reference) {
        Ok(snapshot) => {
            let message = format!(
                "[trading] State refreshed: {} open orders, {} positions",
                snapshot.open_orders.len(),
                snapshot.positions.len()
            );
            ConnectionProbeResult::trading_state(row_id, Ok(message), snapshot)
        }
        Err(error) => ConnectionProbeResult::trading_state(
            row_id,
            Err(format!("State refresh failed: {error}")),
            LiveTradingSnapshot::default(),
        ),
    }
}

fn try_trading_state_refresh(
    reference: &ConnectionCredentialRef,
) -> Result<LiveTradingSnapshot, String> {
    let vault_key = load_or_create_device_vault_key()?;
    let secret = load_connection_secret(reference, &vault_key)?.ok_or_else(|| {
        "Saved API keys were not found in the local credential vault. Delete and re-add this connection.".to_string()
    })?;
    let credentials = MexcCredentials::new(secret.access_key(), secret.secret_key())?;
    let client =
        MexcBlockingPrivateClient::new(credentials, None).map_err(|error| error.to_string())?;

    futures_trading_snapshot(&client)
}

fn futures_trading_snapshot(
    client: &MexcBlockingPrivateClient,
) -> Result<LiveTradingSnapshot, String> {
    let open_orders = client
        .futures_open_orders(1, 100)
        .map_err(|error| error.to_string())?;
    let positions = client
        .futures_open_positions(None)
        .map_err(|error| error.to_string())?;

    Ok(LiveTradingSnapshot {
        open_orders: futures_open_orders(&open_orders),
        positions: futures_open_positions(&positions),
    })
}

fn probe_mexc_public(market: ConnectionMarket) -> Result<ProbeSuccess, String> {
    let url = match market {
        ConnectionMarket::Spot => MEXC_SPOT_PUBLIC_PING_URL,
        ConnectionMarket::Futures => MEXC_FUTURES_PUBLIC_PING_URL,
    };
    let client = reqwest::blocking::Client::builder()
        .timeout(CONNECTION_TEST_TIMEOUT)
        .user_agent("flowsurface-connection-test")
        .build()
        .map_err(|error| error.to_string())?;

    client
        .get(url)
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;

    Ok(ProbeSuccess {
        message: "Public API reachable".to_string(),
        balances: Vec::new(),
        trades: Vec::new(),
        open_orders: Vec::new(),
        positions: Vec::new(),
    })
}

fn probe_mexc_private(spec: &ConnectionProbeSpec) -> Result<ProbeSuccess, String> {
    let reference = spec
        .credential_ref
        .as_ref()
        .ok_or_else(|| "API keys are missing".to_string())?;
    let vault_key = load_or_create_device_vault_key()?;

    let secret = load_connection_secret(reference, &vault_key)?.ok_or_else(|| {
        "Saved API keys were not found in the local credential vault. Delete and re-add this connection.".to_string()
    })?;

    if let Some(expected_hint) = &spec.access_key_hint {
        let actual_hint = secret.access_key_hint();
        if actual_hint != *expected_hint {
            return Err(format!(
                "Saved API key mismatch: metadata shows {expected_hint}, local vault has {actual_hint}. Delete and re-add this connection."
            ));
        }
    }

    probe_mexc_private_secret(spec.market, &secret)
}

fn probe_mexc_private_secret(
    market: ConnectionMarket,
    secret: &ConnectionSecret,
) -> Result<ProbeSuccess, String> {
    let credentials = MexcCredentials::new(secret.access_key(), secret.secret_key())?;
    let client =
        MexcBlockingPrivateClient::new(credentials, None).map_err(|error| error.to_string())?;

    let mut message = "Private API authenticated".to_string();
    let (balances, trades, open_orders, positions) = match market {
        ConnectionMarket::Spot => {
            let account = match client.spot_account_information() {
                Ok(account) => account,
                Err(error) => {
                    let error = error.to_string();
                    return Err(mexc_spot_error_with_market_hint(&client, error));
                }
            };
            (
                available_balances_from_spot_account(&account),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
        }
        ConnectionMarket::Futures => {
            let assets = match client.futures_assets() {
                Ok(assets) => assets,
                Err(error) => {
                    let error = error.to_string();
                    return Err(mexc_futures_error_with_market_hint(&client, error));
                }
            };
            let balances = available_balances_from_futures_assets(&assets);
            let trades = match client.futures_history_orders(1, 50) {
                Ok(history) => futures_history_records(&history, "MEXC Futures"),
                Err(error) => {
                    message = format!("Private API authenticated; history unavailable: {error}");
                    Vec::new()
                }
            };
            let snapshot = match futures_trading_snapshot(&client) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    message = format!("{message}; state unavailable: {error}");
                    LiveTradingSnapshot::default()
                }
            };
            (balances, trades, snapshot.open_orders, snapshot.positions)
        }
    };

    Ok(ProbeSuccess {
        message,
        balances,
        trades,
        open_orders,
        positions,
    })
}

fn futures_history_records(
    response: &MexcFuturesResponse<Value>,
    source: &str,
) -> Vec<ConnectionTradeRecord> {
    let Some(data) = response.data.as_ref() else {
        return Vec::new();
    };

    let mut order_items = Vec::new();
    collect_futures_order_items(data, &mut order_items);

    order_items
        .into_iter()
        .map(|item| futures_order_record(item, source))
        .collect()
}

fn futures_open_orders(response: &MexcFuturesResponse<Value>) -> Vec<LiveOpenOrder> {
    let Some(data) = response.data.as_ref() else {
        return Vec::new();
    };

    let mut order_items = Vec::new();
    collect_futures_order_items(data, &mut order_items);

    order_items
        .into_iter()
        .filter_map(futures_open_order)
        .collect()
}

fn futures_open_order(item: &Value) -> Option<LiveOpenOrder> {
    let state = value_as_i64(item.get("state").or_else(|| item.get("status")));
    if matches!(state, Some(3..=5)) {
        return None;
    }

    let symbol = display_field(item, &["symbol"])?;
    let side = match value_as_i64(item.get("side"))? {
        1 | 2 => LiveOrderSide::Buy,
        3 | 4 => LiveOrderSide::Sell,
        _ => return None,
    };
    let price = value_as_f32(item.get("price").or_else(|| item.get("orderPrice"))?)?;
    let contracts = display_field(item, &["leftVol", "vol", "quantity", "origQty"])
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| *value > 0.0)?;
    let order_id = display_field(item, &["orderId", "id", "externalOid"])?;

    Some(LiveOpenOrder {
        symbol,
        side,
        price: exchange::unit::Price::from_f32(price),
        contracts,
        order_id,
    })
}

fn futures_open_positions(response: &MexcFuturesResponse<Value>) -> Vec<LivePosition> {
    let Some(data) = response.data.as_ref() else {
        return Vec::new();
    };

    let mut position_items = Vec::new();
    collect_futures_position_items(data, &mut position_items);

    position_items
        .into_iter()
        .filter_map(futures_open_position)
        .collect()
}

fn collect_futures_position_items<'a>(value: &'a Value, items: &mut Vec<&'a Value>) {
    match value {
        Value::Array(values) => {
            for item in values {
                if looks_like_futures_position(item) {
                    items.push(item);
                }
            }
        }
        Value::Object(object) => {
            if looks_like_futures_position(value) {
                items.push(value);
                return;
            }

            for key in ["resultList", "list", "positions", "data"] {
                if let Some(nested) = object.get(key) {
                    collect_futures_position_items(nested, items);
                }
            }
        }
        _ => {}
    }
}

fn looks_like_futures_position(value: &Value) -> bool {
    value.get("symbol").is_some()
        && value.get("positionType").is_some()
        && (value.get("holdVol").is_some() || value.get("vol").is_some())
}

fn futures_open_position(item: &Value) -> Option<LivePosition> {
    let state = value_as_i64(item.get("state"));
    if matches!(state, Some(3)) {
        return None;
    }

    let symbol = display_field(item, &["symbol"])?;
    let side_sign = match value_as_i64(item.get("positionType"))? {
        1 => 1.0,
        2 => -1.0,
        _ => return None,
    };
    let hold_vol = value_as_f32(item.get("holdVol").or_else(|| item.get("vol"))?)?;
    if hold_vol <= f32::EPSILON {
        return None;
    }

    let avg_entry = item
        .get("holdAvgPrice")
        .or_else(|| item.get("openAvgPrice"))
        .and_then(value_as_f32)
        .filter(|value| *value > 0.0);
    let realized_pnl = item
        .get("realised")
        .or_else(|| item.get("realized"))
        .and_then(value_as_f32)
        .unwrap_or_default();

    Some(LivePosition {
        symbol,
        contracts: side_sign * hold_vol,
        avg_entry,
        realized_pnl,
    })
}

fn collect_futures_order_items<'a>(value: &'a Value, items: &mut Vec<&'a Value>) {
    match value {
        Value::Array(values) => {
            for item in values {
                if looks_like_futures_order(item) {
                    items.push(item);
                }
            }
        }
        Value::Object(object) => {
            if looks_like_futures_order(value) {
                items.push(value);
                return;
            }

            for key in ["resultList", "list", "orders", "data"] {
                if let Some(nested) = object.get(key) {
                    collect_futures_order_items(nested, items);
                }
            }
        }
        _ => {}
    }
}

fn looks_like_futures_order(value: &Value) -> bool {
    value.get("symbol").is_some()
        && (value.get("orderId").is_some()
            || value.get("id").is_some()
            || value.get("externalOid").is_some())
}

fn futures_order_record(item: &Value, source: &str) -> ConnectionTradeRecord {
    let timestamp_ms = normalize_timestamp_ms(
        value_as_i64(item.get("updateTime"))
            .or_else(|| value_as_i64(item.get("createTime")))
            .or_else(|| value_as_i64(item.get("createdTime")))
            .unwrap_or_default(),
    );
    let fee = display_field(item, &["fee"])
        .map(|fee| {
            display_field(item, &["feeCurrency", "feeCoin"])
                .map(|currency| format!("{fee} {currency}"))
                .unwrap_or(fee)
        })
        .unwrap_or_else(missing_value);

    ConnectionTradeRecord {
        timestamp_ms,
        timestamp: format_timestamp(timestamp_ms),
        source: source.to_string(),
        symbol: display_field(item, &["symbol"]).unwrap_or_else(missing_value),
        side: order_side_label(item.get("side")),
        order_type: order_type_label(item.get("type").or_else(|| item.get("orderType"))),
        order_price: display_field(item, &["price", "orderPrice"]).unwrap_or_else(missing_value),
        average_price: display_field(item, &["dealAvgPrice", "avgPrice", "averagePrice"])
            .unwrap_or_else(missing_value),
        quantity: display_field(item, &["vol", "quantity", "origQty"])
            .unwrap_or_else(missing_value),
        filled_quantity: display_field(item, &["dealVol", "executedQty", "filledQty"])
            .unwrap_or_else(missing_value),
        pnl: display_field(item, &["profit", "realizedPnl", "realisedPnl", "pnl"])
            .unwrap_or_else(missing_value),
        fee,
        state: order_state_label(item.get("state").or_else(|| item.get("status"))),
        order_id: display_field(item, &["orderId", "id", "externalOid"])
            .unwrap_or_else(missing_value),
    }
}

fn display_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(value_as_display_string)
}

fn value_as_display_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) if !value.is_empty() => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_as_i64(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok())),
        Value::String(value) => value.parse::<i64>().ok(),
        _ => None,
    }
}

fn value_as_f32(value: &Value) -> Option<f32> {
    match value {
        Value::Number(number) => number.as_f64().map(|value| value as f32),
        Value::String(value) => value.parse::<f32>().ok(),
        _ => None,
    }
}

fn normalize_timestamp_ms(timestamp: i64) -> i64 {
    if timestamp > 0 && timestamp < 10_000_000_000 {
        timestamp * 1_000
    } else {
        timestamp
    }
}

fn format_timestamp(timestamp_ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(timestamp_ms)
        .map(|timestamp| {
            timestamp
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(missing_value)
}

fn order_side_label(value: Option<&Value>) -> String {
    match value_as_i64(value) {
        Some(1) => "Open long".to_string(),
        Some(2) => "Close short".to_string(),
        Some(3) => "Open short".to_string(),
        Some(4) => "Close long".to_string(),
        Some(value) => format!("Side {value}"),
        None => value
            .and_then(value_as_display_string)
            .unwrap_or_else(missing_value),
    }
}

fn order_type_label(value: Option<&Value>) -> String {
    match value_as_i64(value) {
        Some(1) => "Limit".to_string(),
        Some(2) => "Post only".to_string(),
        Some(3) => "IOC".to_string(),
        Some(4) => "FOK".to_string(),
        Some(5) => "Market".to_string(),
        Some(value) => format!("Type {value}"),
        None => value
            .and_then(value_as_display_string)
            .unwrap_or_else(missing_value),
    }
}

fn order_state_label(value: Option<&Value>) -> String {
    match value_as_i64(value) {
        Some(1) => "New".to_string(),
        Some(2) => "Partially filled".to_string(),
        Some(3) => "Filled".to_string(),
        Some(4) => "Canceled".to_string(),
        Some(5) => "Invalid".to_string(),
        Some(value) => format!("State {value}"),
        None => value
            .and_then(value_as_display_string)
            .unwrap_or_else(missing_value),
    }
}

fn missing_value() -> String {
    "-".to_string()
}

fn mexc_spot_error_with_market_hint(client: &MexcBlockingPrivateClient, error: String) -> String {
    if is_mexc_spot_key_invalid(&error)
        && let Ok(assets) = client.futures_assets()
    {
        return format!(
            "This API key authenticated on MEXC Futures, not Spot. Re-add it with Market=Futures. {}",
            format_balance_summary(&available_balances_from_futures_assets(&assets))
        );
    }

    error
}

fn mexc_futures_error_with_market_hint(
    client: &MexcBlockingPrivateClient,
    error: String,
) -> String {
    if is_mexc_futures_key_invalid(&error)
        && let Ok(account) = client.spot_account_information()
    {
        return format!(
            "This API key authenticated on MEXC Spot, not Futures. Re-add it with Market=Spot. {}",
            format_balance_summary(&available_balances_from_spot_account(&account))
        );
    }

    if is_mexc_futures_key_invalid(&error) {
        return "MEXC rejected this Futures key/secret pair (code 402). Re-paste both API key and secret; the access-key suffix can match even when the saved secret is wrong.".to_string();
    }

    if is_mexc_futures_contract_network_error(&error) {
        return "MEXC Futures contract API returned code 1005 after signing the private request. Check Futures API permissions, IP whitelist, and contract account activation; this is no longer a local keychain/auth prompt issue.".to_string();
    }

    error
}

fn is_mexc_spot_key_invalid(error: &str) -> bool {
    error.contains("\"code\":10072") || error.contains("Api key info invalid")
}

fn is_mexc_futures_key_invalid(error: &str) -> bool {
    error.contains("\"code\":402") || error.contains("API Key expired")
}

fn is_mexc_futures_contract_network_error(error: &str) -> bool {
    error.contains("code 1005") || error.contains("\"code\":1005")
}

fn format_balance_summary(balances: &[exchange::adapter::MexcAvailableBalance]) -> String {
    let non_zero = balances
        .iter()
        .filter(|balance| !is_zero_amount(&balance.available))
        .take(3)
        .map(|balance| format!("{} {}", balance.asset, balance.available))
        .collect::<Vec<_>>();

    if non_zero.is_empty() {
        "Available balance: none returned".to_string()
    } else {
        format!("Available balance: {}", non_zero.join(", "))
    }
}

fn is_zero_amount(value: &str) -> bool {
    value
        .parse::<f64>()
        .map(|amount| amount.abs() <= f64::EPSILON)
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
struct ConnectionDraft {
    exchange: ConnectionExchange,
    market: ConnectionMarket,
    mode: ConnectionMode,
    access_key: String,
    secret_key: String,
    error: Option<String>,
}

impl Default for ConnectionDraft {
    fn default() -> Self {
        Self {
            exchange: ConnectionExchange::Mexc,
            market: ConnectionMarket::Futures,
            mode: ConnectionMode::View,
            access_key: String::new(),
            secret_key: String::new(),
            error: None,
        }
    }
}

impl ConnectionDraft {
    fn clear_credentials_if_view_mode(&mut self) {
        if self.mode == ConnectionMode::View {
            self.access_key.clear();
            self.secret_key.clear();
        }
        self.error = None;
    }
}

pub(super) fn connections_panel<'a>(state: &'a ConnectionPanelState) -> Element<'a, PanelMessage> {
    let mut rows = column![connection_header()].spacing(0);

    for (index, connection) in state.rows.iter().enumerate() {
        rows = rows.push(connection_row(index, connection));
    }

    if let Some(draft) = &state.draft {
        rows = rows.push(draft_row(draft));
    }

    column![
        connection_status_strip(state),
        panel_card(
            "Connection list",
            column![
                rows,
                row![
                    iced::widget::Space::new().width(Length::Fill),
                    connection_button(
                        "+ Add connection",
                        ConnectionAction::AddConnection,
                        state.draft.is_some()
                    ),
                    connection_button(
                        "Refresh",
                        ConnectionAction::Refresh,
                        state.last_action == "Refresh"
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(12),
        ),
        connection_log(state),
        row![
            iced::widget::Space::new().width(Length::Fill),
            connection_button("OK", ConnectionAction::Confirm, state.last_action == "OK"),
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(14)
    .into()
}

fn connection_status_strip<'a>(state: &ConnectionPanelState) -> Element<'a, PanelMessage> {
    container(
        row![
            text(format!(
                "{} enabled / {} total",
                state.enabled_count(),
                state.rows.len()
            ))
            .size(style::text_size::BODY),
            text(state.top_bar_status()).size(style::text_size::BODY),
            iced::widget::checkbox(state.autoconnect_enabled())
                .label("Auto-connect last used")
                .on_toggle(|enabled| {
                    PanelMessage::ConnectionAction(ConnectionAction::AutoconnectChanged(enabled))
                }),
            text(format!("Last: {}", state.last_connection_label())).size(style::text_size::SMALL),
            iced::widget::Space::new().width(Length::Fill),
            text(format!("Last action: {}", state.last_action)).size(style::text_size::SMALL),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(10)
    .style(style::panel_card)
    .into()
}

fn connection_header<'a>() -> Element<'a, PanelMessage> {
    row![
        connection_cell(table_header_text("State"), Length::Fixed(72.0), true, false),
        connection_cell(
            table_header_text("Exchange"),
            Length::FillPortion(2),
            true,
            false
        ),
        connection_cell(
            table_header_text("Market"),
            Length::FillPortion(2),
            true,
            false
        ),
        connection_cell(
            table_header_text("Mode"),
            Length::FillPortion(2),
            true,
            false
        ),
        connection_cell(
            table_header_text("Credentials"),
            Length::FillPortion(3),
            true,
            false
        ),
        connection_cell(
            table_header_text("Status"),
            Length::FillPortion(4),
            true,
            false
        ),
        connection_cell(
            table_header_text("Actions"),
            Length::Fixed(92.0),
            true,
            false
        ),
    ]
    .spacing(0)
    .into()
}

fn connection_row<'a>(index: usize, connection: &'a ConnectionRow) -> Element<'a, PanelMessage> {
    let selected = connection.enabled;

    row![
        connection_cell(
            connection_toggle(index, connection.enabled),
            Length::Fixed(72.0),
            false,
            selected
        ),
        connection_cell(
            value_box(connection.exchange.to_string(), Length::Fill),
            Length::FillPortion(2),
            false,
            selected,
        ),
        connection_cell(
            value_box(connection.market.to_string(), Length::Fill),
            Length::FillPortion(2),
            false,
            selected,
        ),
        connection_cell(
            value_box(connection.mode.to_string(), Length::Fill),
            Length::FillPortion(2),
            false,
            selected,
        ),
        connection_cell(
            credential_control(connection),
            Length::FillPortion(3),
            false,
            selected,
        ),
        connection_cell(
            text(connection.status())
                .size(style::text_size::SMALL)
                .style(move |theme: &Theme| text::Style {
                    color: Some(status_text_color(theme, &connection.test_state)),
                }),
            Length::FillPortion(4),
            false,
            selected,
        ),
        connection_cell(
            row![icon_button(
                style::icon_text(style::Icon::TrashBin, 13),
                ConnectionAction::RowDelete(index),
            )]
            .align_y(Alignment::Center),
            Length::Fixed(92.0),
            false,
            selected,
        ),
    ]
    .spacing(0)
    .into()
}

fn draft_row<'a>(draft: &'a ConnectionDraft) -> Element<'a, PanelMessage> {
    let credentials: Element<'a, PanelMessage> = if draft.mode == ConnectionMode::Trade {
        column![
            text_input("API key ID", &draft.access_key)
                .on_input(|value| {
                    PanelMessage::ConnectionAction(ConnectionAction::DraftAccessKeyChanged(value))
                })
                .style(|theme, status| style::validated_text_input(theme, status, true)),
            text_input("Secret key", &draft.secret_key)
                .secure(true)
                .on_input(|value| {
                    PanelMessage::ConnectionAction(ConnectionAction::DraftSecretKeyChanged(value))
                })
                .style(|theme, status| style::validated_text_input(theme, status, true)),
        ]
        .spacing(4)
        .into()
    } else {
        value_box("Not required", Length::Fill)
    };

    let status = draft
        .error
        .as_deref()
        .unwrap_or_else(|| draft.exchange.draft_status(draft.mode));

    row![
        connection_cell(
            text("New").size(style::text_size::SMALL),
            Length::Fixed(72.0),
            false,
            true
        ),
        connection_cell(
            pick_list(ConnectionExchange::ALL, Some(draft.exchange), |exchange| {
                PanelMessage::ConnectionAction(ConnectionAction::DraftExchangeSelected(exchange))
            }),
            Length::FillPortion(2),
            false,
            true,
        ),
        connection_cell(
            pick_list(ConnectionMarket::ALL, Some(draft.market), |market| {
                PanelMessage::ConnectionAction(ConnectionAction::DraftMarketSelected(market))
            }),
            Length::FillPortion(2),
            false,
            true,
        ),
        connection_cell(
            pick_list(ConnectionMode::ALL, Some(draft.mode), |mode| {
                PanelMessage::ConnectionAction(ConnectionAction::DraftModeSelected(mode))
            }),
            Length::FillPortion(2),
            false,
            true,
        ),
        connection_cell(credentials, Length::FillPortion(3), false, true),
        connection_cell(
            text(status).size(style::text_size::SMALL),
            Length::FillPortion(4),
            false,
            true
        ),
        connection_cell(
            row![
                connection_button("Save", ConnectionAction::SaveDraft, true),
                icon_button(
                    text("x").size(style::text_size::BODY),
                    ConnectionAction::CancelDraft,
                )
            ]
            .spacing(6)
            .align_y(Alignment::Center),
            Length::Fixed(92.0),
            false,
            true,
        ),
    ]
    .spacing(0)
    .into()
}

fn credential_control<'a>(connection: &'a ConnectionRow) -> Element<'a, PanelMessage> {
    if connection.mode != ConnectionMode::Trade {
        return value_box(connection.credential_label(), Length::Fill);
    }

    match &connection.credentials {
        CredentialState::Saved { .. } => value_box(connection.credential_label(), Length::Fill),
        CredentialState::NotRequired => value_box(connection.credential_label(), Length::Fill),
    }
}

fn connection_cell<'a>(
    content: impl Into<Element<'a, PanelMessage>>,
    width: Length,
    is_header: bool,
    selected: bool,
) -> Element<'a, PanelMessage> {
    container(content.into())
        .width(width)
        .height(Length::Fixed(if is_header { 36.0 } else { 92.0 }))
        .padding(padding::left(8).right(8).top(6).bottom(6))
        .align_y(Alignment::Center)
        .style(move |theme| {
            if is_header {
                style::panel_table_header(theme)
            } else if selected {
                style::panel_nav_active(theme)
            } else {
                style::panel_table_cell(theme)
            }
        })
        .into()
}

fn table_header_text<'a>(label: &'static str) -> Element<'a, PanelMessage> {
    text(label)
        .size(style::text_size::BODY)
        .font(style::AZERET_MONO)
        .style(|theme: &Theme| text::Style {
            color: Some(theme.extended_palette().background.base.text),
        })
        .into()
}

fn connection_toggle<'a>(index: usize, enabled: bool) -> Element<'a, PanelMessage> {
    button(text(if enabled { "ON" } else { "OFF" }).size(style::text_size::TINY))
        .width(Length::Fixed(48.0))
        .height(Length::Fixed(26.0))
        .padding(padding::left(5).right(5).top(3).bottom(3))
        .style(move |theme, status| connection_toggle_style(theme, status, enabled))
        .on_press(PanelMessage::ConnectionAction(ConnectionAction::Toggle(
            index,
        )))
        .into()
}

fn connection_toggle_style(
    theme: &Theme,
    status: iced::widget::button::Status,
    enabled: bool,
) -> iced::widget::button::Style {
    let palette = theme.extended_palette();
    let base = if enabled {
        palette.success.strong.color
    } else {
        palette.danger.strong.color
    };
    let background = match status {
        iced::widget::button::Status::Hovered => base.scale_alpha(0.26),
        iced::widget::button::Status::Pressed => base.scale_alpha(0.34),
        iced::widget::button::Status::Disabled => base.scale_alpha(0.10),
        iced::widget::button::Status::Active => base.scale_alpha(0.18),
    };

    iced::widget::button::Style {
        text_color: base,
        border: iced::Border {
            radius: 3.0.into(),
            width: 1.0,
            color: base.scale_alpha(0.80),
        },
        background: Some(background.into()),
        ..Default::default()
    }
}

fn status_text_color(theme: &Theme, state: &ConnectionTestState) -> iced::Color {
    let palette = theme.extended_palette();
    match state {
        ConnectionTestState::Loading => palette.warning.strong.color,
        ConnectionTestState::Success(_) => palette.success.strong.color,
        ConnectionTestState::Error(_) => palette.danger.strong.color,
        ConnectionTestState::Idle => palette.background.base.text,
    }
}

fn connection_log<'a>(state: &ConnectionPanelState) -> Element<'a, PanelMessage> {
    let mut lines = column![].spacing(3);

    for line in &state.logs {
        lines = lines.push(text(line.clone()).size(style::text_size::SMALL));
    }

    panel_card(
        "Connection log",
        container(lines)
            .height(Length::Fixed(128.0))
            .width(Length::Fill)
            .padding(10)
            .style(style::panel_value_box),
    )
}

fn connection_button<'a>(
    label: &'static str,
    action: ConnectionAction,
    active: bool,
) -> Element<'a, PanelMessage> {
    button(text(label).size(style::text_size::SMALL))
        .padding(padding::left(10).right(10).top(6).bottom(6))
        .style(move |theme, status| style::button::bordered_toggle(theme, status, active))
        .on_press(PanelMessage::ConnectionAction(action))
        .into()
}

fn icon_button<'a>(
    content: impl Into<Element<'a, PanelMessage>>,
    action: ConnectionAction,
) -> Element<'a, PanelMessage> {
    button(content)
        .padding(padding::left(5).right(5).top(4).bottom(4))
        .style(|theme, status| style::button::bordered_toggle(theme, status, false))
        .on_press(PanelMessage::ConnectionAction(action))
        .into()
}

#[cfg(test)]
mod tests {
    use super::{
        ConnectionExchange, ConnectionMarket, ConnectionMode, ConnectionRow, MexcFuturesResponse,
        PersistedConnectionRow, PersistedConnections, futures_history_records, futures_open_orders,
        futures_open_positions, last_enabled_connection_id,
    };
    use crate::trading_state::LiveOrderSide;

    #[test]
    fn futures_history_records_extracts_core_order_fields() {
        let response = MexcFuturesResponse {
            success: true,
            code: 0,
            message: None,
            data: Some(serde_json::json!({
                "resultList": [{
                    "orderId": "123",
                    "symbol": "BTC_USDT",
                    "side": 1,
                    "type": 5,
                    "price": "64000",
                    "dealAvgPrice": "64010",
                    "vol": "2",
                    "dealVol": "1",
                    "profit": "3.5",
                    "fee": "0.12",
                    "feeCurrency": "USDT",
                    "state": 3,
                    "createTime": 1_700_000_000_000_i64
                }]
            })),
        };

        let records = futures_history_records(&response, "MEXC Futures");

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source, "MEXC Futures");
        assert_eq!(records[0].symbol, "BTC_USDT");
        assert_eq!(records[0].side, "Open long");
        assert_eq!(records[0].order_type, "Market");
        assert_eq!(records[0].pnl, "3.5");
        assert_eq!(records[0].fee, "0.12 USDT");
        assert_eq!(records[0].state, "Filled");
    }

    #[test]
    fn futures_open_orders_extract_prices_and_signed_sides() {
        let response = MexcFuturesResponse {
            success: true,
            code: 0,
            message: None,
            data: Some(serde_json::json!({
                "resultList": [
                    {
                        "orderId": "buy-1",
                        "symbol": "BTC_USDT",
                        "side": 1,
                        "price": "65000.5",
                        "vol": "2",
                        "state": 2
                    },
                    {
                        "orderId": "sell-1",
                        "symbol": "BTC_USDT",
                        "side": 3,
                        "price": "70000.5",
                        "vol": "3",
                        "state": 2
                    }
                ]
            })),
        };

        let orders = futures_open_orders(&response);

        assert_eq!(orders.len(), 2);
        assert_eq!(orders[0].symbol, "BTC_USDT");
        assert_eq!(orders[0].side, LiveOrderSide::Buy);
        assert_eq!(orders[0].contracts, 2.0);
        assert!((orders[0].price.to_f32_lossy() - 65000.5).abs() < 0.01);
        assert_eq!(orders[1].side, LiveOrderSide::Sell);
        assert_eq!(orders[1].contracts, 3.0);
    }

    #[test]
    fn futures_open_positions_extract_signed_contracts_and_entry() {
        let response = MexcFuturesResponse {
            success: true,
            code: 0,
            message: None,
            data: Some(serde_json::json!([
                {
                    "symbol": "BTC_USDT",
                    "positionType": 1,
                    "state": 1,
                    "holdVol": "2",
                    "holdAvgPrice": "64000.5",
                    "realised": "-0.25"
                },
                {
                    "symbol": "ETH_USDT",
                    "positionType": 2,
                    "state": 1,
                    "holdVol": "3",
                    "openAvgPrice": "3200",
                    "realised": "1.5"
                }
            ])),
        };

        let positions = futures_open_positions(&response);

        assert_eq!(positions.len(), 2);
        assert_eq!(positions[0].symbol, "BTC_USDT");
        assert_eq!(positions[0].contracts, 2.0);
        assert_eq!(positions[0].avg_entry, Some(64000.5));
        assert_eq!(positions[0].realized_pnl, -0.25);
        assert_eq!(positions[1].symbol, "ETH_USDT");
        assert_eq!(positions[1].contracts, -3.0);
        assert_eq!(positions[1].avg_entry, Some(3200.0));
        assert_eq!(positions[1].realized_pnl, 1.5);
    }

    #[test]
    fn persisted_connections_default_autoconnects_legacy_files() {
        let persisted: PersistedConnections = serde_json::from_value(serde_json::json!({
            "rows": [],
            "next_connection_id": 3
        }))
        .unwrap();

        assert!(persisted.autoconnect_enabled);
        assert!(persisted.last_connection_id.is_none());
    }

    #[test]
    fn last_enabled_connection_id_uses_last_enabled_row() {
        let rows = vec![
            persisted_row("first", true),
            persisted_row("middle-disabled", false),
            persisted_row("last", true),
        ];

        assert_eq!(last_enabled_connection_id(&rows).as_deref(), Some("last"));
    }

    #[test]
    fn futures_contract_network_error_detection_matches_private_response() {
        assert!(super::is_mexc_futures_contract_network_error(
            "MEXC futures private request GET /v1/private/account/assets returned code 1005: Network error"
        ));
    }

    #[test]
    fn deduplicate_connection_rows_keeps_latest_same_kind() {
        let rows = vec![
            ConnectionRow::new(
                "old".to_string(),
                ConnectionExchange::Mexc,
                ConnectionMarket::Futures,
                ConnectionMode::Trade,
                false,
            ),
            ConnectionRow::new(
                "latest".to_string(),
                ConnectionExchange::Mexc,
                ConnectionMarket::Futures,
                ConnectionMode::Trade,
                false,
            ),
        ];

        let rows = super::deduplicate_connection_rows(rows);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "latest");
    }

    fn persisted_row(id: &str, enabled: bool) -> PersistedConnectionRow {
        PersistedConnectionRow {
            id: Some(id.to_string()),
            enabled,
            exchange: ConnectionExchange::Mexc,
            market: ConnectionMarket::Futures,
            mode: ConnectionMode::View,
            credential_id: None,
            access_key_hint: None,
        }
    }
}
