use crate::{Message, style};

use iced::{
    Alignment, Element, Length, Point, Rectangle, Renderer, Size, Theme, mouse, padding,
    widget::{
        Canvas, button,
        canvas::{self, Geometry, Path, Stroke},
        column, container, row, rule, scrollable, text,
    },
};

mod connections;
mod settings;
mod trades;

use connections::{ConnectionPanelState, connections_panel};
use settings::{SettingsAction, SettingsPanelState, settings_panel};
use trades::trades_table;

const PNL_POINTS: [f32; 12] = [
    0.0, 420.0, -120.0, 780.0, 620.0, 1_240.0, 980.0, 1_540.0, 1_310.0, 1_860.0, 2_220.0, 2_050.0,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    App,
    File,
    Edit,
    View,
    Window,
    Help,
    Settings,
    Pnl,
    Connections,
    Account,
    Analytics,
    About,
}

impl Kind {
    pub(crate) const ALL: [Self; 12] = [
        Self::App,
        Self::File,
        Self::Edit,
        Self::View,
        Self::Window,
        Self::Help,
        Self::Settings,
        Self::Pnl,
        Self::Connections,
        Self::Account,
        Self::Analytics,
        Self::About,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::App => "Flowsurface",
            Self::File => "File",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::Window => "Window",
            Self::Help => "Help",
            Self::Settings => "Settings",
            Self::Pnl => "PnL",
            Self::Connections => "Connections",
            Self::Account => "Account",
            Self::Analytics => "Analytics",
            Self::About => "About",
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::App => "App",
            Self::File => "File",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::Window => "Window",
            Self::Help => "Help",
            Self::Settings => "Settings",
            Self::Pnl => "PnL",
            Self::Connections => "Connections",
            Self::Account => "Account",
            Self::Analytics => "Analytics",
            Self::About => "About",
        }
    }

    pub(crate) fn default_size(self) -> Size {
        match self {
            Self::Connections => Size::new(1_080.0, 700.0),
            Self::Pnl => Size::new(820.0, 560.0),
            Self::Settings => Size::new(860.0, 620.0),
            Self::Analytics => Size::new(760.0, 540.0),
            Self::Account => Size::new(680.0, 480.0),
            Self::App
            | Self::File
            | Self::Edit
            | Self::View
            | Self::Window
            | Self::Help
            | Self::About => Size::new(560.0, 420.0),
        }
    }
}

#[derive(Debug)]
pub(crate) struct State {
    pub kind: Kind,
    show_trades: bool,
    connection_state: ConnectionPanelState,
    settings_state: SettingsPanelState,
}

impl State {
    pub(crate) fn new(kind: Kind) -> Self {
        Self {
            kind,
            show_trades: false,
            connection_state: ConnectionPanelState::default(),
            settings_state: SettingsPanelState::default(),
        }
    }

    pub(crate) fn update(&mut self, message: PanelMessage) {
        match message {
            PanelMessage::ConnectionAction(action) => {
                if self.kind == Kind::Connections {
                    self.connection_state.update(action);
                }
            }
            PanelMessage::SettingsAction(action) => {
                if self.kind == Kind::Settings {
                    self.settings_state.update(action);
                }
            }
            PanelMessage::TogglePnlTrades => {
                if self.kind == Kind::Pnl {
                    self.show_trades = !self.show_trades;
                }
            }
        }
    }

    pub(crate) fn tick(&mut self, now: std::time::Instant) {
        if self.kind == Kind::Connections {
            self.connection_state.tick(now);
        }
    }

    pub(crate) fn view(&self) -> Element<'_, PanelMessage> {
        let body = match self.kind {
            Kind::App => app_panel(),
            Kind::File => default_panel(
                "File actions",
                "Common file operations are grouped here as a simple command template.",
                &[
                    ("New layout", "Start from a clean dashboard workspace"),
                    (
                        "Open data folder",
                        "Inspect saved market data and config files",
                    ),
                    ("Export snapshot", "Prepare the active layout for sharing"),
                ],
            ),
            Kind::Edit => default_panel(
                "Edit tools",
                "Editing controls can collect dashboard-level preferences and bulk actions.",
                &[
                    (
                        "Undo",
                        "Placeholder for the latest reversible workspace action",
                    ),
                    ("Redo", "Placeholder for restoring an undone action"),
                    ("Preferences", "Template area for editor and input settings"),
                ],
            ),
            Kind::View => default_panel(
                "View options",
                "Display controls can live here without crowding the trading dashboard.",
                &[
                    (
                        "Compact mode",
                        "Reduce spacing for denser market monitoring",
                    ),
                    (
                        "Focus mode",
                        "Highlight the active pane and mute secondary chrome",
                    ),
                    ("Reset zoom", "Return charts to their default viewport"),
                ],
            ),
            Kind::Window => default_panel(
                "Window manager",
                "Window-level actions can coordinate the main dashboard and popout panels.",
                &[
                    (
                        "Bring all to front",
                        "Focus open trading and utility windows",
                    ),
                    (
                        "Arrange panels",
                        "Tile utility windows around the dashboard",
                    ),
                    (
                        "Close utility panels",
                        "Close non-dashboard template windows",
                    ),
                ],
            ),
            Kind::Help => default_panel(
                "Help",
                "A compact support panel can keep references close to the workflow.",
                &[
                    (
                        "Keyboard shortcuts",
                        "List app-wide navigation and pane shortcuts",
                    ),
                    ("Documentation", "Open local or web-based product docs"),
                    ("Report issue", "Collect logs and environment details"),
                ],
            ),
            Kind::Settings => settings_panel(&self.settings_state),
            Kind::Pnl => pnl_panel(self.show_trades),
            Kind::Connections => connections_panel(&self.connection_state),
            Kind::Account => account_panel(),
            Kind::Analytics => analytics_panel(),
            Kind::About => about_panel(),
        };

        container(
            scrollable(
                column![
                    text(self.kind.title())
                        .size(style::text_size::TITLE)
                        .font(iced::Font {
                            weight: iced::font::Weight::Bold,
                            ..Default::default()
                        }),
                    rule::horizontal(1).style(style::split_ruler),
                    body,
                ]
                .spacing(16)
                .padding(18),
            )
            .style(style::scroll_bar),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(style::panel_window)
        .into()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PanelMessage {
    ConnectionAction(ConnectionAction),
    SettingsAction(SettingsAction),
    TogglePnlTrades,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionAction {
    Toggle(usize),
    SetColor(usize, u32),
    AddConnection,
    MyProxy,
    Refresh,
    Confirm,
    BecomeTrader,
    OpenAccount,
    RowSettings(usize),
    RowHelp(usize),
    RowDelete(usize),
}

pub(crate) fn menu_bar<'a>() -> Element<'a, Message> {
    let mut items = row![].spacing(2).align_y(Alignment::Center);

    for kind in Kind::ALL {
        items = items.push(
            button(text(kind.label()).size(style::text_size::SMALL))
                .padding(padding::left(7).right(7).top(2).bottom(2))
                .style(style::button::macos_menu)
                .on_press(Message::OpenPanel(kind)),
        );
    }

    items.into()
}

fn app_panel<'a>() -> Element<'a, PanelMessage> {
    column![
        metric_row(&[
            ("Workspace", "Dashboard"),
            ("Mode", "Live market monitor"),
            ("Layout", "Active"),
        ]),
        panel_card(
            "Session",
            column![
                detail_row("Main dashboard", "Open"),
                detail_row("Market streams", "Managed by active panes"),
                detail_row("Utility panels", "Opened from the app menu"),
            ]
            .spacing(10),
        ),
    ]
    .spacing(14)
    .into()
}

fn default_panel<'a>(
    title: &'static str,
    description: &'static str,
    items: &[(&'static str, &'static str)],
) -> Element<'a, PanelMessage> {
    let mut commands = column![].spacing(10);

    for (label, detail) in items {
        commands = commands.push(detail_row(*label, *detail));
    }

    column![
        text(description)
            .size(style::text_size::BODY)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.extended_palette().background.weak.text),
            }),
        panel_card(title, commands),
    ]
    .spacing(14)
    .into()
}

fn pnl_panel<'a>(show_trades: bool) -> Element<'a, PanelMessage> {
    let latest = PNL_POINTS.last().copied().unwrap_or_default();
    let best = PNL_POINTS.iter().copied().fold(f32::MIN, f32::max);
    let worst = PNL_POINTS.iter().copied().fold(f32::MAX, f32::min);

    let trades_button_label = if show_trades {
        "Show PnL chart"
    } else {
        "Trades"
    };

    let pnl_body: Element<'_, PanelMessage> = if show_trades {
        trades_table()
    } else {
        column![
            Canvas::new(PnlPlot)
                .height(Length::Fixed(240.0))
                .width(Length::Fill),
            row![
                detail_row("Start", "$0"),
                detail_row("Latest", format_money(latest)),
                detail_row("Best", format_money(best)),
                detail_row("Worst", format_money(worst)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(12)
        .into()
    };

    column![
        metric_row(&[
            ("Realized PnL", "$2,050"),
            ("Win rate", "66%"),
            ("Trades", "6"),
        ]),
        panel_card(
            "PnL change",
            column![
                row![
                    text("Template equity curve").size(style::text_size::SECTION),
                    iced::widget::Space::new().width(Length::Fill),
                    button(text(trades_button_label).size(style::text_size::SMALL))
                        .padding(padding::left(10).right(10).top(4).bottom(4))
                        .style(move |theme, status| {
                            style::button::bordered_toggle(theme, status, show_trades)
                        })
                        .on_press(PanelMessage::TogglePnlTrades),
                ]
                .align_y(Alignment::Center),
                pnl_body,
            ]
            .spacing(12),
        ),
    ]
    .spacing(14)
    .into()
}

fn account_panel<'a>() -> Element<'a, PanelMessage> {
    column![
        metric_row(&[
            ("Equity", "$128,430"),
            ("Margin used", "18%"),
            ("Risk", "Normal"),
        ]),
        panel_card(
            "Balances",
            column![
                detail_row("USDT", "$84,200 available"),
                detail_row("BTC", "1.42 collateral"),
                detail_row("ETH", "12.8 collateral"),
            ]
            .spacing(10),
        ),
        panel_card(
            "Permissions",
            column![
                detail_row("Market data", "Enabled"),
                detail_row("Trading", "Read-only template"),
                detail_row("Withdrawals", "Disabled"),
            ]
            .spacing(10),
        ),
    ]
    .spacing(14)
    .into()
}

fn analytics_panel<'a>() -> Element<'a, PanelMessage> {
    column![
        metric_row(&[
            ("Sharpe", "1.72"),
            ("Max drawdown", "-4.8%"),
            ("Avg hold", "38m"),
        ]),
        panel_card(
            "Performance breakdown",
            column![
                progress_row("BTCUSDT", 0.72, "$1,380"),
                progress_row("ETHUSDT", 0.44, "$510"),
                progress_row("SOLUSDT", 0.28, "$160"),
            ]
            .spacing(12),
        ),
        panel_card(
            "Observations",
            column![
                detail_row("Best session", "US open"),
                detail_row("Weak spot", "Late reversal entries"),
                detail_row("Next review", "Compare fees against gross edge"),
            ]
            .spacing(10),
        ),
    ]
    .spacing(14)
    .into()
}

fn about_panel<'a>() -> Element<'a, PanelMessage> {
    column![
        panel_card(
            "Flowsurface",
            column![
                text("Native desktop charting platform for crypto markets.")
                    .size(style::text_size::BODY),
                detail_row("Version", env!("CARGO_PKG_VERSION")),
                detail_row("Interface", "macOS-style utility menu template"),
            ]
            .spacing(10),
        ),
        panel_card(
            "Panel templates",
            column![
                detail_row("PnL", "Equity curve and trades table"),
                detail_row("Connections", "Venue and channel status"),
                detail_row("Analytics", "Performance summary"),
            ]
            .spacing(10),
        ),
    ]
    .spacing(14)
    .into()
}

fn metric_row<'a>(items: &[(&'static str, &'static str)]) -> Element<'a, PanelMessage> {
    let mut row = row![].spacing(10);

    for (label, value) in items {
        row = row.push(metric_card(*label, *value));
    }

    row.into()
}

fn metric_card<'a>(label: &'static str, value: &'static str) -> Element<'a, PanelMessage> {
    container(
        column![
            text(label)
                .size(style::text_size::SMALL)
                .style(|theme: &Theme| {
                    text::Style {
                        color: Some(theme.extended_palette().background.weak.text),
                    }
                }),
            text(value)
                .size(style::text_size::SECTION)
                .font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..Default::default()
                }),
        ]
        .spacing(5),
    )
    .width(Length::Fill)
    .padding(12)
    .style(style::panel_card)
    .into()
}

pub(super) fn panel_card<'a>(
    title: &'static str,
    content: impl Into<Element<'a, PanelMessage>>,
) -> Element<'a, PanelMessage> {
    container(
        column![
            text(title)
                .size(style::text_size::SECTION)
                .font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..Default::default()
                }),
            content.into(),
        ]
        .spacing(12),
    )
    .width(Length::Fill)
    .padding(14)
    .style(style::panel_card)
    .into()
}

fn detail_row<'a>(label: impl Into<String>, value: impl Into<String>) -> Element<'a, PanelMessage> {
    row![
        text(label.into()).size(style::text_size::BODY),
        iced::widget::Space::new().width(Length::Fill),
        text(value.into())
            .size(style::text_size::BODY)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.extended_palette().secondary.strong.color),
            }),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

pub(super) fn value_box<'a>(value: impl Into<String>, width: Length) -> Element<'a, PanelMessage> {
    container(text(value.into()).size(style::text_size::BODY))
        .width(width)
        .padding(padding::left(10).right(10).top(6).bottom(6))
        .style(style::panel_value_box)
        .into()
}

fn progress_row<'a>(
    label: &'static str,
    percent: f32,
    value: &'static str,
) -> Element<'a, PanelMessage> {
    let clamped = percent.clamp(0.0, 1.0);

    column![
        detail_row(label, value),
        row![
            container("")
                .height(Length::Fixed(8.0))
                .width(Length::FillPortion((clamped * 100.0) as u16))
                .style(style::panel_progress),
            container("")
                .height(Length::Fixed(8.0))
                .width(Length::FillPortion(((1.0 - clamped) * 100.0) as u16 + 1))
                .style(style::panel_progress_track),
        ]
        .spacing(0),
    ]
    .spacing(6)
    .into()
}

pub(super) fn format_money(value: f32) -> String {
    if value < 0.0 {
        format!("-${:.0}", value.abs())
    } else {
        format!("${value:.0}")
    }
}

struct PnlPlot;

impl canvas::Program<PanelMessage> for PnlPlot {
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
        let background = Path::rectangle(Point::new(0.0, 0.0), bounds.size());
        frame.fill(&background, palette.background.weakest.color);

        let left = 34.0;
        let right = 12.0;
        let top = 16.0;
        let bottom = 24.0;
        let width = (bounds.width - left - right).max(1.0);
        let height = (bounds.height - top - bottom).max(1.0);

        for index in 0..=4 {
            let y = top + height * index as f32 / 4.0;
            frame.stroke(
                &Path::line(Point::new(left, y), Point::new(left + width, y)),
                Stroke::with_color(
                    Stroke {
                        width: 1.0,
                        ..Stroke::default()
                    },
                    palette.background.weak.color.scale_alpha(0.5),
                ),
            );
        }

        let min = PNL_POINTS.iter().copied().fold(f32::MAX, f32::min);
        let max = PNL_POINTS.iter().copied().fold(f32::MIN, f32::max);
        let padded_min = min.min(0.0) - 160.0;
        let padded_max = max.max(0.0) + 160.0;
        let range = (padded_max - padded_min).max(1.0);
        let to_y = |value: f32| top + ((padded_max - value) / range) * height;

        let zero_y = to_y(0.0);
        frame.stroke(
            &Path::line(Point::new(left, zero_y), Point::new(left + width, zero_y)),
            Stroke::with_color(
                Stroke {
                    width: 1.0,
                    ..Stroke::default()
                },
                palette.background.strong.color,
            ),
        );

        let step = width / (PNL_POINTS.len().saturating_sub(1) as f32).max(1.0);
        let mut previous = None;

        for (index, value) in PNL_POINTS.iter().copied().enumerate() {
            let point = Point::new(left + index as f32 * step, to_y(value));

            if let Some(prev) = previous {
                let color = if value >= 0.0 {
                    palette.success.strong.color
                } else {
                    palette.danger.strong.color
                };

                frame.stroke(
                    &Path::line(prev, point),
                    Stroke::with_color(
                        Stroke {
                            width: 2.0,
                            ..Stroke::default()
                        },
                        color,
                    ),
                );
            }

            frame.fill(
                &Path::circle(point, 3.5),
                if value >= 0.0 {
                    palette.success.strong.color
                } else {
                    palette.danger.strong.color
                },
            );

            previous = Some(point);
        }

        vec![frame.into_geometry()]
    }
}
