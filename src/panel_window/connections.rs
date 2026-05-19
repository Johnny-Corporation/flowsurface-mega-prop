use std::time::{Duration, Instant};

use crate::style;

use iced::{
    Alignment, Element, Length, Theme, padding,
    widget::{button, column, container, row, text},
};

use super::{ConnectionAction, PanelMessage, panel_card, value_box};

const CONNECTIONS: [ConnectionTemplate; 7] = [
    ConnectionTemplate::new(false, "Bybit", "Market data", "$", 0x7d55c7, "Direct", 138),
    ConnectionTemplate::new(
        false,
        "Binance: USDT-M",
        "Futures",
        "K",
        0x7f9442,
        "Direct",
        156,
    ),
    ConnectionTemplate::new(false, "Binance: Spot", "Spot", "K", 0xab735d, "Direct", 148),
    ConnectionTemplate::new(
        false,
        "Tiger.com Binance",
        "USDT-M",
        "T",
        0xc64058,
        "Direct",
        171,
    ),
    ConnectionTemplate::new(
        true,
        "OKX: USDT-M / USDC-M",
        "View mode",
        "O",
        0x3447b8,
        "Direct",
        294,
    ),
    ConnectionTemplate::new(
        true,
        "OKX: Spot (Margin)",
        "View mode",
        "O",
        0x6ca889,
        "Direct",
        292,
    ),
    ConnectionTemplate::new(
        true,
        "MEXC: Spot",
        "Market data",
        "M",
        0x2d9cdb,
        "Direct",
        94,
    ),
];

#[derive(Debug, Clone, Copy)]
struct ConnectionTemplate {
    enabled: bool,
    exchange: &'static str,
    market: &'static str,
    key: &'static str,
    color: u32,
    proxy: &'static str,
    base_ping_ms: u16,
}

impl ConnectionTemplate {
    const fn new(
        enabled: bool,
        exchange: &'static str,
        market: &'static str,
        key: &'static str,
        color: u32,
        proxy: &'static str,
        base_ping_ms: u16,
    ) -> Self {
        Self {
            enabled,
            exchange,
            market,
            key,
            color,
            proxy,
            base_ping_ms,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ConnectionPanelState {
    rows: Vec<ConnectionRow>,
    logs: Vec<String>,
    last_action: &'static str,
    proxy_enabled: bool,
    ping_tick: u32,
    last_ping_update: Option<Instant>,
}

impl Default for ConnectionPanelState {
    fn default() -> Self {
        let mut rows = CONNECTIONS
            .iter()
            .copied()
            .map(ConnectionRow::from_template)
            .collect::<Vec<_>>();

        let mut state = Self {
            rows: Vec::new(),
            logs: vec![
                "[02:32:08] [OKX: USDT-M / USDC-M] Connected".to_string(),
                "[02:32:08] [OKX: Spot (Margin)] Connected".to_string(),
                "[02:32:09] [MEXC: Spot] Connected with fake live ping".to_string(),
            ],
            last_action: "Ready",
            proxy_enabled: false,
            ping_tick: 0,
            last_ping_update: None,
        };

        state.rows.append(&mut rows);
        state.refresh_pings();
        state
    }
}

impl ConnectionPanelState {
    pub(super) fn tick(&mut self, now: Instant) {
        let should_update = self.last_ping_update.map_or(true, |last| {
            now.duration_since(last) >= Duration::from_millis(350)
        });

        if should_update {
            self.last_ping_update = Some(now);
            self.refresh_pings();
        }
    }

    pub(super) fn update(&mut self, action: ConnectionAction) {
        match action {
            ConnectionAction::Toggle(index) => self.toggle(index),
            ConnectionAction::AddConnection => self.add_connection(),
            ConnectionAction::MyProxy => {
                self.proxy_enabled = !self.proxy_enabled;
                self.last_action = "My proxy";
                self.push_log(format!(
                    "[ui] Proxy mode {}",
                    if self.proxy_enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ));
            }
            ConnectionAction::Refresh => {
                self.last_action = "Refresh";
                self.refresh_pings();
                self.push_log("[ui] Manual refresh requested; fake pings updated".to_string());
            }
            ConnectionAction::Confirm => {
                self.last_action = "OK";
                self.push_log("[ui] OK pressed; connection template accepted".to_string());
            }
            ConnectionAction::BecomeTrader => {
                self.last_action = "Become a trader";
                self.push_log("[ui] Become a trader pressed".to_string());
            }
            ConnectionAction::OpenAccount => {
                self.last_action = "Open an account";
                self.push_log("[ui] Open an account pressed".to_string());
            }
            ConnectionAction::RowSettings(index) => {
                let label = self.row_label(index);
                self.last_action = "Row settings";
                self.push_log(format!("[ui] Settings pressed for {label}"));
            }
            ConnectionAction::RowHelp(index) => {
                let label = self.row_label(index);
                self.last_action = "Row help";
                self.push_log(format!("[ui] Help pressed for {label}"));
            }
            ConnectionAction::RowDelete(index) => self.delete(index),
        }
    }

    fn toggle(&mut self, index: usize) {
        let Some(row) = self.rows.get_mut(index) else {
            return;
        };

        row.enabled = !row.enabled;
        row.ping_ms = row.enabled.then_some(row.base_ping_ms);

        let label = row.exchange.clone();
        let state = if row.enabled {
            "connected"
        } else {
            "disconnected"
        };

        self.last_action = "Toggle";
        self.push_log(format!("[ui] {label} {state}"));
        self.refresh_pings();
    }

    fn add_connection(&mut self) {
        let number = self.rows.len() + 1;
        let color = 0x4a90e2 + ((number as u32 * 0x031415) & 0x202020);

        self.rows.push(ConnectionRow {
            enabled: true,
            exchange: format!("Demo exchange {number}"),
            market: "Template stream".to_string(),
            key: "D".to_string(),
            color,
            proxy: "Direct".to_string(),
            base_ping_ms: 82 + (number as u16 * 11),
            ping_ms: None,
        });

        self.last_action = "Add connection";
        self.push_log(format!("[ui] Demo exchange {number} added and connected"));
        self.refresh_pings();
    }

    fn delete(&mut self, index: usize) {
        if index >= self.rows.len() {
            return;
        }

        let label = self.rows[index].exchange.clone();
        self.rows.remove(index);
        self.last_action = "Delete";
        self.push_log(format!("[ui] {label} removed from template list"));
    }

    fn refresh_pings(&mut self) {
        self.ping_tick = self.ping_tick.wrapping_add(1);

        for (index, row) in self.rows.iter_mut().enumerate() {
            if row.enabled {
                let wave = ((self.ping_tick as i32 * (index as i32 + 5)) % 43) - 21;
                let ping = (row.base_ping_ms as i32 + wave).clamp(18, 480) as u16;
                row.ping_ms = Some(ping);
            } else {
                row.ping_ms = None;
            }
        }
    }

    fn push_log(&mut self, message: String) {
        self.logs.push(message);

        while self.logs.len() > 8 {
            self.logs.remove(0);
        }
    }

    fn row_label(&self, index: usize) -> String {
        self.rows
            .get(index)
            .map(|row| row.exchange.clone())
            .unwrap_or_else(|| "unknown connection".to_string())
    }

    fn online_count(&self) -> usize {
        self.rows.iter().filter(|row| row.enabled).count()
    }

    fn ping_status(&self) -> String {
        format!(
            "Live fake pings: {} online / {} total | refresh #{}",
            self.online_count(),
            self.rows.len(),
            self.ping_tick
        )
    }
}

#[derive(Debug, Clone)]
struct ConnectionRow {
    enabled: bool,
    exchange: String,
    market: String,
    key: String,
    color: u32,
    proxy: String,
    base_ping_ms: u16,
    ping_ms: Option<u16>,
}

impl ConnectionRow {
    fn from_template(template: ConnectionTemplate) -> Self {
        Self {
            enabled: template.enabled,
            exchange: template.exchange.to_string(),
            market: template.market.to_string(),
            key: template.key.to_string(),
            color: template.color,
            proxy: template.proxy.to_string(),
            base_ping_ms: template.base_ping_ms,
            ping_ms: None,
        }
    }

    fn color(&self) -> iced::Color {
        iced::Color::from_rgb8(
            ((self.color >> 16) & 0xff) as u8,
            ((self.color >> 8) & 0xff) as u8,
            (self.color & 0xff) as u8,
        )
    }

    fn speed_label(&self) -> String {
        self.ping_ms
            .map(|ping| format!("{ping}ms"))
            .unwrap_or_else(|| "-".to_string())
    }
}

pub(super) fn connections_panel<'a>(state: &'a ConnectionPanelState) -> Element<'a, PanelMessage> {
    let mut rows = column![connection_header()].spacing(0);

    for (index, connection) in state.rows.iter().enumerate() {
        rows = rows.push(connection_row(index, connection, state));
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
                        state.last_action == "Add connection"
                    ),
                    connection_button("My proxy", ConnectionAction::MyProxy, state.proxy_enabled),
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
        connection_notice(state),
        connection_support_log(state),
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
            text(state.ping_status()).size(style::text_size::BODY),
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
        connection_cell(
            text("State").size(style::text_size::SMALL),
            Length::Fixed(74.0),
            true,
            false
        ),
        connection_cell(
            text("Exchange").size(style::text_size::SMALL),
            Length::FillPortion(4),
            true,
            false
        ),
        connection_cell(
            text("Key").size(style::text_size::SMALL),
            Length::Fixed(62.0),
            true,
            false
        ),
        connection_cell(
            text("Color").size(style::text_size::SMALL),
            Length::Fixed(70.0),
            true,
            false
        ),
        connection_cell(
            text("Proxy").size(style::text_size::SMALL),
            Length::FillPortion(3),
            true,
            false
        ),
        connection_cell(
            text("Speed").size(style::text_size::SMALL),
            Length::Fixed(86.0),
            true,
            false
        ),
        connection_cell(
            text("Actions").size(style::text_size::SMALL),
            Length::Fixed(112.0),
            true,
            false
        ),
    ]
    .spacing(0)
    .into()
}

fn connection_row<'a>(
    index: usize,
    connection: &'a ConnectionRow,
    state: &ConnectionPanelState,
) -> Element<'a, PanelMessage> {
    let selected = connection.enabled;
    let proxy = if state.proxy_enabled {
        "Demo proxy"
    } else {
        connection.proxy.as_str()
    };

    row![
        connection_cell(
            connection_toggle(index, connection.enabled),
            Length::Fixed(74.0),
            false,
            selected
        ),
        connection_cell(
            row![
                exchange_badge(&connection.exchange),
                column![
                    text(connection.exchange.clone()).size(style::text_size::BODY),
                    text(connection.market.clone())
                        .size(style::text_size::TINY)
                        .style(|theme: &Theme| text::Style {
                            color: Some(theme.extended_palette().background.weak.text),
                        }),
                ]
                .spacing(2),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            Length::FillPortion(4),
            false,
            selected,
        ),
        connection_cell(
            key_badge(&connection.key),
            Length::Fixed(62.0),
            false,
            selected
        ),
        connection_cell(
            small_color_chip(connection.color()),
            Length::Fixed(70.0),
            false,
            selected
        ),
        connection_cell(
            value_box(format!("{proxy} connection"), Length::Fill),
            Length::FillPortion(3),
            false,
            selected
        ),
        connection_cell(
            speed_text(connection.speed_label(), connection.enabled),
            Length::Fixed(86.0),
            false,
            selected,
        ),
        connection_cell(
            connection_actions(index, state.last_action),
            Length::Fixed(112.0),
            false,
            selected,
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
        .height(Length::Fixed(if is_header { 30.0 } else { 44.0 }))
        .padding(padding::left(8).right(8).top(5).bottom(5))
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

fn connection_toggle<'a>(index: usize, enabled: bool) -> Element<'a, PanelMessage> {
    button(text(if enabled { "ON" } else { "OFF" }).size(style::text_size::TINY))
        .width(Length::Fixed(48.0))
        .height(Length::Fixed(24.0))
        .padding(padding::left(4).right(4).top(3).bottom(3))
        .style(move |theme, status| style::button::bordered_toggle(theme, status, enabled))
        .on_press(PanelMessage::ConnectionAction(ConnectionAction::Toggle(
            index,
        )))
        .into()
}

fn exchange_badge<'a>(exchange: &str) -> Element<'a, PanelMessage> {
    let letter = exchange.chars().next().unwrap_or('X').to_string();

    container(text(letter).size(style::text_size::TINY))
        .width(Length::Fixed(22.0))
        .height(Length::Fixed(22.0))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(style::panel_value_box)
        .into()
}

fn key_badge<'a>(key: &str) -> Element<'a, PanelMessage> {
    container(text(key.to_string()).size(style::text_size::TINY))
        .width(Length::Fixed(24.0))
        .height(Length::Fixed(22.0))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(style::panel_value_box)
        .into()
}

fn small_color_chip<'a>(color: iced::Color) -> Element<'a, PanelMessage> {
    container("")
        .width(Length::Fixed(24.0))
        .height(Length::Fixed(22.0))
        .style(move |theme| style::panel_swatch(theme, color, false))
        .into()
}

fn speed_text<'a>(speed: String, online: bool) -> Element<'a, PanelMessage> {
    text(speed)
        .size(style::text_size::BODY)
        .style(move |theme: &Theme| text::Style {
            color: Some(if online {
                theme.extended_palette().success.strong.color
            } else {
                theme.extended_palette().background.weak.text
            }),
        })
        .into()
}

fn connection_actions<'a>(index: usize, last_action: &'static str) -> Element<'a, PanelMessage> {
    row![
        icon_button(
            style::icon_text(style::Icon::Cog, 13),
            ConnectionAction::RowSettings(index),
            last_action == "Row settings"
        ),
        icon_button(
            text("?").size(style::text_size::BODY),
            ConnectionAction::RowHelp(index),
            last_action == "Row help"
        ),
        icon_button(
            style::icon_text(style::Icon::TrashBin, 13),
            ConnectionAction::RowDelete(index),
            last_action == "Delete"
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

fn connection_notice<'a>(state: &ConnectionPanelState) -> Element<'a, PanelMessage> {
    container(
        row![
            text("Become a prop company trader or open a personal account on favorable terms")
                .size(style::text_size::BODY),
            iced::widget::Space::new().width(Length::Fill),
            connection_button(
                "Become a trader",
                ConnectionAction::BecomeTrader,
                state.last_action == "Become a trader"
            ),
            connection_button(
                "Open an account",
                ConnectionAction::OpenAccount,
                state.last_action == "Open an account"
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(12)
    .style(style::panel_card)
    .into()
}

fn connection_support_log<'a>(state: &ConnectionPanelState) -> Element<'a, PanelMessage> {
    let mut lines = column![].spacing(3);

    for line in &state.logs {
        lines = lines.push(text(line.clone()).size(style::text_size::SMALL));
    }

    panel_card(
        "Connection log",
        container(lines)
            .height(Length::Fixed(170.0))
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
    active: bool,
) -> Element<'a, PanelMessage> {
    button(content)
        .padding(padding::left(5).right(5).top(4).bottom(4))
        .style(move |theme, status| style::button::bordered_toggle(theme, status, active))
        .on_press(PanelMessage::ConnectionAction(action))
        .into()
}
