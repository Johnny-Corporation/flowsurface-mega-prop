use crate::style;

use data::config::connection_credentials::{
    ConnectionCredentialRef, ConnectionSecret, delete_connection_secret, load_connection_secret,
    save_connection_secret,
};
use exchange::adapter::{
    Exchange, MexcBlockingPrivateClient, MexcCredentials, available_balances_from_futures_assets,
    available_balances_from_spot_account,
};
use iced::{
    Alignment, Element, Length, Theme, padding,
    widget::{button, column, container, pick_list, row, text, text_input},
};
use serde::{Deserialize, Serialize};
use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
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

#[derive(Debug)]
pub(crate) struct ConnectionPanelState {
    rows: Vec<ConnectionRow>,
    draft: Option<ConnectionDraft>,
    logs: Vec<String>,
    last_action: String,
    session_vault_key: String,
    next_connection_id: u64,
    probe_tx: Sender<ConnectionProbeResult>,
    probe_rx: Receiver<ConnectionProbeResult>,
}

impl Default for ConnectionPanelState {
    fn default() -> Self {
        let (probe_tx, probe_rx) = mpsc::channel();

        let (rows, next_connection_id, logs) =
            if let Some((rows, next_connection_id)) = load_saved_connections() {
                (
                    rows,
                    next_connection_id,
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
            session_vault_key: String::new(),
            next_connection_id,
            probe_tx,
            probe_rx,
        }
    }
}

impl ConnectionPanelState {
    pub(crate) fn tick(&mut self, _now: std::time::Instant) {
        while let Ok(result) = self.probe_rx.try_recv() {
            self.apply_probe_result(result);
        }
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
            ConnectionAction::SessionVaultKeyChanged(value) => {
                self.session_vault_key = value;
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

    fn toggle(&mut self, index: usize) {
        let Some(row) = self.rows.get_mut(index) else {
            return;
        };

        let label = row.label();

        if row.enabled {
            row.enabled = false;
            row.test_state = ConnectionTestState::Idle;
            row.balance_summary = None;
            self.last_action = "Toggle".to_string();
            self.push_log(format!("[connections] {label} disabled"));
            self.persist_rows();
            return;
        }

        row.enabled = true;
        row.test_state = ConnectionTestState::Loading;
        row.balance_summary = None;
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

        let id = format!(
            "{}-{}-{}-{}",
            draft.exchange.storage_key(),
            draft.market.storage_key(),
            draft.mode.storage_key(),
            self.next_connection_id
        );
        self.next_connection_id += 1;

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

            let auth_check = if draft.exchange == ConnectionExchange::Mexc {
                match probe_mexc_private_secret(draft.market, &secret) {
                    Ok(success) => Some(success),
                    Err(error) => {
                        draft.error = Some(format!("MEXC auth failed: {error}"));
                        self.draft = Some(draft);
                        self.last_action = "Credential validation failed".to_string();
                        self.push_log(format!("[credentials] MEXC auth failed: {error}"));
                        return;
                    }
                }
            } else {
                None
            };

            let vault_key = self.session_vault_key.trim();
            if vault_key.is_empty() {
                let error = "Enter session PIN/passphrase before saving API keys".to_string();
                draft.error = Some(error.clone());
                self.draft = Some(draft);
                self.last_action = "Credential save failed".to_string();
                self.push_log(format!("[credentials] {error}"));
                return;
            }

            if let Err(error) = save_connection_secret(&reference, &secret, vault_key) {
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

            if let Some(success) = auth_check {
                row.test_state = ConnectionTestState::Success(success.message);
                row.balance_summary = success.balance_summary;
            }
        }

        let label = row.label();
        self.rows.push(row);
        self.last_action = "Connection saved".to_string();
        self.push_log(format!("[connections] {label} saved"));
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

        let Some(spec) = ConnectionProbeSpec::from_row(row, self.session_vault_key.trim()) else {
            let status = row.exchange.draft_status(row.mode).to_string();
            let row_id = row.id.clone();
            self.apply_probe_result(ConnectionProbeResult {
                row_id,
                outcome: Err(status),
                balance_summary: None,
            });
            return;
        };

        let tx = self.probe_tx.clone();
        thread::spawn(move || {
            let _ = tx.send(run_connection_probe(spec));
        });
    }

    fn apply_probe_result(&mut self, result: ConnectionProbeResult) {
        let Some(row) = self.rows.iter_mut().find(|row| row.id == result.row_id) else {
            return;
        };

        let log_message = match result.outcome {
            Ok(message) => {
                let label = row.label();
                row.enabled = true;
                row.test_state = ConnectionTestState::Success(message.clone());
                row.balance_summary = result.balance_summary;
                format!("[connections] {label} connected")
            }
            Err(error) => {
                let label = row.label();
                row.enabled = false;
                row.test_state = ConnectionTestState::Error(error.clone());
                row.balance_summary = None;
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
    balance_summary: Option<String>,
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
            balance_summary: None,
        }
    }

    fn label(&self) -> String {
        format!("{} {} {}", self.exchange, self.market, self.mode)
    }

    fn status(&self) -> String {
        match &self.test_state {
            ConnectionTestState::Loading => return "Testing connection...".to_string(),
            ConnectionTestState::Success(message) => {
                if let Some(balance) = &self.balance_summary {
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
    next_connection_id: u64,
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
            balance_summary: None,
        })
    }
}

fn load_saved_connections() -> Option<(Vec<ConnectionRow>, u64)> {
    let path = data::data_path(Some(CONNECTIONS_FILE));
    let contents = std::fs::read_to_string(path).ok()?;
    let persisted: PersistedConnections = serde_json::from_str(&contents).ok()?;
    let rows = persisted
        .rows
        .into_iter()
        .filter_map(|row| ConnectionRow::try_from(row).ok())
        .collect::<Vec<_>>();

    if rows.is_empty() {
        return None;
    }

    Some((rows, persisted.next_connection_id.max(1)))
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
    vault_key: String,
}

impl ConnectionProbeSpec {
    fn from_row(row: &ConnectionRow, session_vault_key: &str) -> Option<Self> {
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
            vault_key: session_vault_key.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
struct ConnectionProbeResult {
    row_id: String,
    outcome: Result<String, String>,
    balance_summary: Option<String>,
}

fn run_connection_probe(spec: ConnectionProbeSpec) -> ConnectionProbeResult {
    let result = match spec.mode {
        ConnectionMode::View => probe_mexc_public(spec.market),
        ConnectionMode::Trade => probe_mexc_private(&spec),
    };

    ConnectionProbeResult {
        row_id: spec.row_id,
        balance_summary: result
            .as_ref()
            .ok()
            .and_then(|result| result.balance_summary.clone()),
        outcome: result.map(|result| result.message),
    }
}

#[derive(Debug, Clone)]
struct ProbeSuccess {
    message: String,
    balance_summary: Option<String>,
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
        balance_summary: None,
    })
}

fn probe_mexc_private(spec: &ConnectionProbeSpec) -> Result<ProbeSuccess, String> {
    let reference = spec
        .credential_ref
        .as_ref()
        .ok_or_else(|| "API keys are missing".to_string())?;
    if spec.vault_key.trim().is_empty() {
        return Err("Enter local PIN/passphrase before connecting".to_string());
    }

    let secret = load_connection_secret(reference, &spec.vault_key)?
        .ok_or_else(|| {
            "Saved API keys were not found in the local credential vault. Delete and re-add this connection with a local PIN/passphrase.".to_string()
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

    let balance_summary = match market {
        ConnectionMarket::Spot => {
            let account = match client.spot_account_information() {
                Ok(account) => account,
                Err(error) => {
                    let error = error.to_string();
                    return Err(mexc_spot_error_with_market_hint(&client, error));
                }
            };
            format_balance_summary(available_balances_from_spot_account(&account))
        }
        ConnectionMarket::Futures => {
            let assets = match client.futures_assets() {
                Ok(assets) => assets,
                Err(error) => {
                    let error = error.to_string();
                    return Err(mexc_futures_error_with_market_hint(&client, error));
                }
            };
            format_balance_summary(available_balances_from_futures_assets(&assets))
        }
    };

    Ok(ProbeSuccess {
        message: "Private API authenticated".to_string(),
        balance_summary: Some(balance_summary),
    })
}

fn mexc_spot_error_with_market_hint(client: &MexcBlockingPrivateClient, error: String) -> String {
    if is_mexc_spot_key_invalid(&error)
        && let Ok(assets) = client.futures_assets()
    {
        return format!(
            "This API key authenticated on MEXC Futures, not Spot. Re-add it with Market=Futures. {}",
            format_balance_summary(available_balances_from_futures_assets(&assets))
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
            format_balance_summary(available_balances_from_spot_account(&account))
        );
    }

    if is_mexc_futures_key_invalid(&error) {
        return "MEXC rejected this Futures key/secret pair (code 402). Re-paste both API key and secret; the access-key suffix can match even when the saved secret is wrong.".to_string();
    }

    error
}

fn is_mexc_spot_key_invalid(error: &str) -> bool {
    error.contains("\"code\":10072") || error.contains("Api key info invalid")
}

fn is_mexc_futures_key_invalid(error: &str) -> bool {
    error.contains("\"code\":402") || error.contains("API Key expired")
}

fn format_balance_summary(balances: Vec<exchange::adapter::MexcAvailableBalance>) -> String {
    let non_zero = balances
        .into_iter()
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
            text_input("Session PIN/passphrase", &state.session_vault_key)
                .secure(true)
                .on_input(|value| {
                    PanelMessage::ConnectionAction(ConnectionAction::SessionVaultKeyChanged(value))
                })
                .width(Length::Fixed(220.0))
                .style(|theme, status| style::validated_text_input(theme, status, true)),
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
