use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use crate::style;

use iced::{
    Alignment, Element, Length, Point, Rectangle, Renderer, Theme, mouse, padding,
    widget::{
        Canvas, button,
        canvas::{self, Geometry, Path, Stroke},
        column, container, row, text,
    },
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
const COLOR_CHOICES: [u32; 6] = [0x7d55c7, 0x2d9cdb, 0x6ca889, 0xd6a23a, 0xc64058, 0x3447b8];
const MEXC_API_PING_URL: &str = "https://api.mexc.com/api/v3/time";
const MEXC_API_PING_INTERVAL: Duration = Duration::from_secs(2);

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

#[derive(Debug)]
pub(super) struct ConnectionPanelState {
    rows: Vec<ConnectionRow>,
    logs: Vec<String>,
    last_action: &'static str,
    proxy_enabled: bool,
    ping_tick: u32,
    last_ping_update: Option<Instant>,
    mexc_ping_tx: Sender<MexcPingResult>,
    mexc_ping_rx: Receiver<MexcPingResult>,
    mexc_probe_inflight: bool,
    last_mexc_probe: Option<Instant>,
    mexc_status: String,
}

impl Default for ConnectionPanelState {
    fn default() -> Self {
        let (mexc_ping_tx, mexc_ping_rx) = mpsc::channel();
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
                "[02:32:09] [MEXC: Spot] API RTT probe pending; engine latency unknown".to_string(),
            ],
            last_action: "Ready",
            proxy_enabled: false,
            ping_tick: 0,
            last_ping_update: None,
            mexc_ping_tx,
            mexc_ping_rx,
            mexc_probe_inflight: false,
            last_mexc_probe: None,
            mexc_status: "MEXC API RTT pending; engine latency unknown".to_string(),
        };

        state.rows.append(&mut rows);
        state.refresh_pings();
        state
    }
}

impl ConnectionPanelState {
    pub(super) fn tick(&mut self, now: Instant) {
        self.poll_mexc_ping();

        let should_update = self.last_ping_update.map_or(true, |last| {
            now.duration_since(last) >= Duration::from_millis(350)
        });

        if should_update {
            self.last_ping_update = Some(now);
            self.refresh_pings();
        }

        self.start_mexc_ping_if_needed(now);
    }

    pub(super) fn update(&mut self, action: ConnectionAction) {
        match action {
            ConnectionAction::Toggle(index) => self.toggle(index),
            ConnectionAction::SetColor(index, color) => self.set_color(index, color),
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
                self.push_log("[ui] Manual refresh requested; demo pings updated".to_string());
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
        row.ping_ms = if row.enabled && !row.api_ping {
            Some(row.base_ping_ms)
        } else {
            None
        };

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

    fn set_color(&mut self, index: usize, color: u32) {
        let Some(row) = self.rows.get_mut(index) else {
            return;
        };

        row.color = color;
        let label = row.exchange.clone();
        self.last_action = "Color";
        self.push_log(format!("[ui] {label} color changed"));
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
            ping_history: Vec::new(),
            api_ping: false,
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
            if row.api_ping {
                if !row.enabled {
                    row.ping_ms = None;
                }
                continue;
            }

            if row.enabled {
                let wave = ((self.ping_tick as i32 * (index as i32 + 5)) % 43) - 21;
                let ping = (row.base_ping_ms as i32 + wave).clamp(18, 480) as u16;
                row.push_ping(ping);
            } else {
                row.ping_ms = None;
            }
        }
    }

    fn poll_mexc_ping(&mut self) {
        while let Ok(result) = self.mexc_ping_rx.try_recv() {
            self.mexc_probe_inflight = false;

            match result {
                Ok(ping_ms) => {
                    if let Some(row) = self.rows.iter_mut().find(|row| row.api_ping) {
                        row.push_ping(ping_ms);
                    }
                    self.mexc_status = format!("MEXC API RTT: {ping_ms}ms; engine unknown");
                }
                Err(error) => {
                    self.mexc_status = format!("MEXC API RTT failed: {error}");
                    if let Some(row) = self.rows.iter_mut().find(|row| row.api_ping) {
                        row.ping_ms = None;
                    }
                }
            }
        }
    }

    fn start_mexc_ping_if_needed(&mut self, now: Instant) {
        if self.mexc_probe_inflight
            || !self.rows.iter().any(|row| row.enabled && row.api_ping)
            || self
                .last_mexc_probe
                .is_some_and(|last| now.duration_since(last) < MEXC_API_PING_INTERVAL)
        {
            return;
        }

        self.mexc_probe_inflight = true;
        self.last_mexc_probe = Some(now);
        let tx = self.mexc_ping_tx.clone();

        thread::spawn(move || {
            let _ = tx.send(measure_mexc_api_rtt());
        });
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
            "{} | other rows use demo pings: {} online / {} total | refresh #{}",
            self.mexc_status,
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
    ping_history: Vec<u16>,
    api_ping: bool,
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
            ping_history: Vec::new(),
            api_ping: template.exchange.starts_with("MEXC"),
        }
    }

    fn speed_label(&self) -> String {
        match (self.enabled, self.ping_ms, self.api_ping) {
            (false, _, _) => "-".to_string(),
            (true, Some(ping), true) => format!("{ping}ms API"),
            (true, Some(ping), false) => format!("{ping}ms demo"),
            (true, None, true) => "API check".to_string(),
            (true, None, false) => "-".to_string(),
        }
    }

    fn push_ping(&mut self, ping_ms: u16) {
        self.ping_ms = Some(ping_ms);
        self.ping_history.push(ping_ms);

        while self.ping_history.len() > 36 {
            self.ping_history.remove(0);
        }
    }
}

type MexcPingResult = Result<u16, String>;

fn measure_mexc_api_rtt() -> MexcPingResult {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(2_500))
        .user_agent("flowsurface-latency-probe")
        .build()
        .map_err(|error| error.to_string())?;

    let start = Instant::now();
    let response = client
        .get(MEXC_API_PING_URL)
        .send()
        .map_err(|error| error.to_string())?;

    response
        .error_for_status_ref()
        .map_err(|error| error.to_string())?;

    let _ = response.bytes().map_err(|error| error.to_string())?;

    Ok(start.elapsed().as_millis().clamp(1, u16::MAX as u128) as u16)
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
        connection_cell(table_header_text("State"), Length::Fixed(78.0), true, false),
        connection_cell(
            table_header_text("Exchange"),
            Length::FillPortion(4),
            true,
            false
        ),
        connection_cell(table_header_text("Key"), Length::Fixed(66.0), true, false),
        connection_cell(
            table_header_text("Color"),
            Length::Fixed(144.0),
            true,
            false
        ),
        connection_cell(
            table_header_text("Proxy"),
            Length::FillPortion(3),
            true,
            false
        ),
        connection_cell(
            table_header_text("Speed"),
            Length::Fixed(108.0),
            true,
            false
        ),
        connection_cell(
            table_header_text("Trend"),
            Length::Fixed(150.0),
            true,
            false
        ),
        connection_cell(
            table_header_text("Actions"),
            Length::Fixed(118.0),
            true,
            false
        ),
    ]
    .spacing(0)
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
            Length::Fixed(78.0),
            false,
            selected
        ),
        connection_cell(
            row![
                exchange_badge(&connection.exchange),
                column![
                    text(connection.exchange.clone())
                        .size(style::text_size::EMPHASIS)
                        .font(iced::Font {
                            weight: iced::font::Weight::Bold,
                            ..Default::default()
                        }),
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
            Length::Fixed(66.0),
            false,
            selected
        ),
        connection_cell(
            color_picker(index, connection.color),
            Length::Fixed(144.0),
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
            Length::Fixed(108.0),
            false,
            selected,
        ),
        connection_cell(
            ping_sparkline(connection),
            Length::Fixed(150.0),
            false,
            selected,
        ),
        connection_cell(
            connection_actions(index, state.last_action),
            Length::Fixed(118.0),
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
        .height(Length::Fixed(if is_header { 36.0 } else { 52.0 }))
        .padding(padding::left(11).right(11).top(7).bottom(7))
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
        .width(Length::Fixed(52.0))
        .height(Length::Fixed(26.0))
        .padding(padding::left(5).right(5).top(3).bottom(3))
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
    container(
        text(key.to_string())
            .size(style::text_size::TINY)
            .font(style::AZERET_MONO),
    )
    .width(Length::Fixed(28.0))
    .height(Length::Fixed(24.0))
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(style::panel_value_box)
    .into()
}

fn color_picker<'a>(index: usize, selected: u32) -> Element<'a, PanelMessage> {
    let mut swatches = row![].spacing(4).align_y(Alignment::Center);

    for color in COLOR_CHOICES {
        swatches = swatches.push(
            button(
                container("")
                    .width(Length::Fixed(16.0))
                    .height(Length::Fixed(16.0))
                    .style(move |theme| style::panel_swatch(theme, rgb(color), color == selected)),
            )
            .padding(2)
            .style(move |theme, status| {
                style::button::bordered_toggle(theme, status, color == selected)
            })
            .on_press(PanelMessage::ConnectionAction(ConnectionAction::SetColor(
                index, color,
            ))),
        );
    }

    swatches.into()
}

fn speed_text<'a>(speed: String, online: bool) -> Element<'a, PanelMessage> {
    text(speed)
        .size(style::text_size::BODY)
        .font(style::AZERET_MONO)
        .style(move |theme: &Theme| text::Style {
            color: Some(if online {
                theme.extended_palette().success.strong.color
            } else {
                theme.extended_palette().background.weak.text
            }),
        })
        .into()
}

fn ping_sparkline<'a>(connection: &ConnectionRow) -> Element<'a, PanelMessage> {
    Canvas::new(PingSparkline {
        points: connection.ping_history.clone(),
        online: connection.enabled,
        api: connection.api_ping,
    })
    .width(Length::Fill)
    .height(Length::Fixed(34.0))
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

fn rgb(value: u32) -> iced::Color {
    iced::Color::from_rgb8(
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    )
}

struct PingSparkline {
    points: Vec<u16>,
    online: bool,
    api: bool,
}

impl canvas::Program<PanelMessage> for PingSparkline {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let palette = theme.extended_palette();
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let width = bounds.width.max(1.0);
        let height = bounds.height.max(1.0);
        let mid_y = height * 0.62;

        let color = if !self.online {
            palette.background.weak.text.scale_alpha(0.55)
        } else if self.api {
            palette.success.strong.color
        } else {
            palette.success.weak.color
        };

        frame.stroke(
            &Path::line(Point::new(2.0, mid_y), Point::new(width - 2.0, mid_y)),
            Stroke::default()
                .with_color(color.scale_alpha(0.45))
                .with_width(1.0),
        );

        if self.points.len() >= 2 && self.online {
            let min = self.points.iter().copied().min().unwrap_or(0) as f32;
            let max = self.points.iter().copied().max().unwrap_or(1) as f32;
            let range = (max - min).max(12.0);
            let step = (width - 4.0) / (self.points.len() - 1) as f32;
            let points = self
                .points
                .iter()
                .copied()
                .enumerate()
                .map(|(index, value)| {
                    let x = 2.0 + index as f32 * step;
                    let normalized = (value as f32 - min) / range;
                    let y = (height - 4.0) - normalized * (height - 8.0);
                    Point::new(x, y.clamp(3.0, height - 3.0))
                })
                .collect::<Vec<_>>();

            let path = Path::new(|builder| {
                builder.move_to(points[0]);

                for pair in points.windows(2) {
                    let from = pair[0];
                    let to = pair[1];
                    let control_dx = (to.x - from.x) * 0.45;

                    builder.bezier_curve_to(
                        Point::new(from.x + control_dx, from.y),
                        Point::new(to.x - control_dx, to.y),
                        to,
                    );
                }
            });

            frame.stroke(&path, Stroke::default().with_color(color).with_width(2.15));
        }

        vec![frame.into_geometry()]
    }
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
