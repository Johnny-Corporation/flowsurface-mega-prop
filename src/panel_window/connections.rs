use crate::style;

use data::config::connection_credentials::{
    ConnectionCredentialRef, ConnectionSecret, delete_connection_secret, save_connection_secret,
};
use iced::{
    Alignment, Element, Length, Theme, padding,
    widget::{button, column, container, pick_list, row, text, text_input},
};
use serde::{Deserialize, Serialize};

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

#[derive(Debug)]
pub(super) struct ConnectionPanelState {
    rows: Vec<ConnectionRow>,
    draft: Option<ConnectionDraft>,
    logs: Vec<String>,
    last_action: String,
    next_connection_id: u64,
}

impl Default for ConnectionPanelState {
    fn default() -> Self {
        if let Some(mut state) = load_saved_connections() {
            state
                .logs
                .push("[connections] Saved metadata loaded".to_string());
            return state;
        }

        let rows = DEFAULT_ROWS
            .into_iter()
            .map(|(exchange, market, mode)| ConnectionRow::new(exchange, market, mode, false))
            .collect();

        Self {
            rows,
            draft: None,
            logs: vec![
                "[connections] Defaults loaded: OKX/MEXC spot and futures in view mode".to_string(),
            ],
            last_action: "Ready".to_string(),
            next_connection_id: 1,
        }
    }
}

impl ConnectionPanelState {
    pub(super) fn tick(&mut self, _now: std::time::Instant) {}

    pub(super) fn update(&mut self, action: ConnectionAction) {
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
            ConnectionAction::SaveDraft => self.save_draft(),
            ConnectionAction::CancelDraft => {
                self.draft = None;
                self.last_action = "Draft canceled".to_string();
            }
            ConnectionAction::Refresh => {
                self.last_action = "Refresh".to_string();
                self.push_log("[connections] Connection states refreshed".to_string());
            }
            ConnectionAction::Confirm => {
                self.last_action = "OK".to_string();
                self.push_log("[connections] Connection list accepted".to_string());
            }
            ConnectionAction::RowDelete(index) => self.delete(index),
        }
    }

    fn toggle(&mut self, index: usize) {
        let Some(row) = self.rows.get_mut(index) else {
            return;
        };

        row.enabled = !row.enabled;
        let label = row.label();
        let state = if row.enabled { "enabled" } else { "disabled" };
        self.last_action = "Toggle".to_string();
        self.push_log(format!("[connections] {label} {state}"));
        self.persist_rows();
    }

    fn start_draft(&mut self) {
        if self.draft.is_none() {
            self.draft = Some(ConnectionDraft::default());
        }

        self.last_action = "Add connection".to_string();
    }

    fn save_draft(&mut self) {
        let Some(draft) = self.draft.take() else {
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

        let mut row = ConnectionRow::new(draft.exchange, draft.market, draft.mode, true);

        if draft.mode == ConnectionMode::Trade {
            let reference = match ConnectionCredentialRef::new(&id) {
                Ok(reference) => reference,
                Err(error) => {
                    self.draft = Some(draft);
                    self.last_action = "Credential id error".to_string();
                    self.push_log(format!("[credentials] {error}"));
                    return;
                }
            };

            let secret = match ConnectionSecret::new(draft.access_key, draft.secret_key) {
                Ok(secret) => secret,
                Err(error) => {
                    self.draft = Some(ConnectionDraft::from_row(row, error.clone()));
                    self.last_action = "Credential validation failed".to_string();
                    self.push_log(format!("[credentials] {error}"));
                    return;
                }
            };

            if let Err(error) = save_connection_secret(&reference, &secret) {
                self.draft = Some(ConnectionDraft::from_row(row, error.clone()));
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

    fn persist_rows(&mut self) {
        if let Err(error) = save_connections(self) {
            self.push_log(format!("[connections] Failed to save metadata: {error}"));
        }
    }
}

#[derive(Debug, Clone)]
struct ConnectionRow {
    enabled: bool,
    exchange: ConnectionExchange,
    market: ConnectionMarket,
    mode: ConnectionMode,
    credentials: CredentialState,
}

impl ConnectionRow {
    fn new(
        exchange: ConnectionExchange,
        market: ConnectionMarket,
        mode: ConnectionMode,
        enabled: bool,
    ) -> Self {
        Self {
            enabled,
            exchange,
            market,
            mode,
            credentials: CredentialState::NotRequired,
        }
    }

    fn label(&self) -> String {
        format!("{} {} {}", self.exchange, self.market, self.mode)
    }

    fn status(&self) -> &'static str {
        match self.exchange {
            ConnectionExchange::Mexc => match self.mode {
                ConnectionMode::View => "MEXC market data ready",
                ConnectionMode::Trade => match self.credentials {
                    CredentialState::Saved { .. } => "MEXC private API ready",
                    _ => "MEXC API keys required",
                },
            },
            ConnectionExchange::Bybit => "Will be implemented soon",
            _ => "Not implemented yet",
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

        Ok(Self {
            enabled: value.enabled,
            exchange: value.exchange,
            market: value.market,
            mode: value.mode,
            credentials,
        })
    }
}

fn load_saved_connections() -> Option<ConnectionPanelState> {
    let path = data::data_path(Some(CONNECTIONS_FILE));
    let contents = std::fs::read_to_string(path).ok()?;
    let persisted: PersistedConnections = serde_json::from_str(&contents).ok()?;
    let rows = persisted
        .rows
        .into_iter()
        .filter_map(|row| ConnectionRow::try_from(row).ok())
        .collect::<Vec<_>>();

    Some(ConnectionPanelState {
        rows,
        draft: None,
        logs: Vec::new(),
        last_action: "Ready".to_string(),
        next_connection_id: persisted.next_connection_id.max(1),
    })
}

fn save_connections(state: &ConnectionPanelState) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&PersistedConnections::from(state))
        .map_err(|error| error.to_string())?;
    data::write_json_to_file(&json, CONNECTIONS_FILE).map_err(|error| error.to_string())
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
            market: ConnectionMarket::Spot,
            mode: ConnectionMode::View,
            access_key: String::new(),
            secret_key: String::new(),
            error: None,
        }
    }
}

impl ConnectionDraft {
    fn from_row(row: ConnectionRow, error: String) -> Self {
        Self {
            exchange: row.exchange,
            market: row.market,
            mode: row.mode,
            access_key: String::new(),
            secret_key: String::new(),
            error: Some(error),
        }
    }

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
            value_box(connection.credential_label(), Length::Fill),
            Length::FillPortion(3),
            false,
            selected,
        ),
        connection_cell(
            text(connection.status()).size(style::text_size::SMALL),
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

fn connection_cell<'a>(
    content: impl Into<Element<'a, PanelMessage>>,
    width: Length,
    is_header: bool,
    selected: bool,
) -> Element<'a, PanelMessage> {
    container(content.into())
        .width(width)
        .height(Length::Fixed(if is_header { 36.0 } else { 64.0 }))
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
        .style(move |theme, status| style::button::bordered_toggle(theme, status, enabled))
        .on_press(PanelMessage::ConnectionAction(ConnectionAction::Toggle(
            index,
        )))
        .into()
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
