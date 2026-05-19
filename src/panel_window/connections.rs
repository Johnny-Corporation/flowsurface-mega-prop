use crate::style;

use iced::{
    Alignment, Element, Length, Theme, padding,
    widget::{button, column, container, row, text},
};

use super::{ConnectionAction, PanelMessage, panel_card, value_box};

const CONNECTIONS: [ConnectionRow; 6] = [
    ConnectionRow::new(false, "Bybit", "Market data", "$", 0x7d55c7, "Direct", "-"),
    ConnectionRow::new(
        false,
        "Binance: USDT-M",
        "Futures",
        "K",
        0x7f9442,
        "Direct",
        "-",
    ),
    ConnectionRow::new(false, "Binance: Spot", "Spot", "K", 0xab735d, "Direct", "-"),
    ConnectionRow::new(
        false,
        "Tiger.com Binance",
        "USDT-M",
        "T",
        0xc64058,
        "Direct",
        "-",
    ),
    ConnectionRow::new(
        true,
        "OKX: USDT-M / USDC-M",
        "View mode",
        "O",
        0x3447b8,
        "Direct",
        "294ms",
    ),
    ConnectionRow::new(
        true,
        "OKX: Spot (Margin)",
        "View mode",
        "O",
        0x6ca889,
        "Direct",
        "292ms",
    ),
];

#[derive(Debug, Clone, Copy)]
struct ConnectionRow {
    enabled: bool,
    exchange: &'static str,
    market: &'static str,
    key: &'static str,
    color: u32,
    proxy: &'static str,
    speed: &'static str,
}

impl ConnectionRow {
    const fn new(
        enabled: bool,
        exchange: &'static str,
        market: &'static str,
        key: &'static str,
        color: u32,
        proxy: &'static str,
        speed: &'static str,
    ) -> Self {
        Self {
            enabled,
            exchange,
            market,
            key,
            color,
            proxy,
            speed,
        }
    }

    fn color(self) -> iced::Color {
        iced::Color::from_rgb8(
            ((self.color >> 16) & 0xff) as u8,
            ((self.color >> 8) & 0xff) as u8,
            (self.color & 0xff) as u8,
        )
    }
}

pub(super) fn connections_panel<'a>(
    feedback: Option<ConnectionAction>,
) -> Element<'a, PanelMessage> {
    let mut rows = column![connection_header()].spacing(0);

    for connection in CONNECTIONS {
        rows = rows.push(connection_row(connection));
    }

    column![
        panel_card(
            "Connection list",
            column![
                rows,
                row![
                    iced::widget::Space::new().width(Length::Fill),
                    connection_button("+ Add connection", ConnectionAction::AddConnection),
                    connection_button("My proxy", ConnectionAction::MyProxy),
                    connection_button("Refresh", ConnectionAction::Refresh),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(12),
        ),
        connection_notice(),
        connection_support_log(feedback),
        row![
            iced::widget::Space::new().width(Length::Fill),
            connection_button("OK", ConnectionAction::Confirm),
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(14)
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
            text("").size(style::text_size::SMALL),
            Length::Fixed(112.0),
            true,
            false
        ),
    ]
    .spacing(0)
    .into()
}

fn connection_row<'a>(connection: ConnectionRow) -> Element<'a, PanelMessage> {
    let selected = connection.enabled;

    row![
        connection_cell(
            connection_toggle(connection.enabled),
            Length::Fixed(74.0),
            false,
            selected
        ),
        connection_cell(
            row![
                exchange_badge(connection.exchange),
                column![
                    text(connection.exchange).size(style::text_size::BODY),
                    text(connection.market)
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
            key_badge(connection.key),
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
            value_box(format!("{} connection", connection.proxy), Length::Fill),
            Length::FillPortion(3),
            false,
            selected
        ),
        connection_cell(
            speed_text(connection.speed),
            Length::Fixed(86.0),
            false,
            selected
        ),
        connection_cell(connection_actions(), Length::Fixed(112.0), false, selected),
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

fn connection_toggle<'a>(enabled: bool) -> Element<'a, PanelMessage> {
    container(text(if enabled { "ON" } else { "OFF" }).size(style::text_size::TINY))
        .width(Length::Fixed(44.0))
        .height(Length::Fixed(22.0))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |theme| style::panel_status_toggle(theme, enabled))
        .into()
}

fn exchange_badge<'a>(exchange: &'static str) -> Element<'a, PanelMessage> {
    let letter = exchange.chars().next().unwrap_or('X').to_string();

    container(text(letter).size(style::text_size::TINY))
        .width(Length::Fixed(22.0))
        .height(Length::Fixed(22.0))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(style::panel_value_box)
        .into()
}

fn key_badge<'a>(key: &'static str) -> Element<'a, PanelMessage> {
    container(text(key).size(style::text_size::TINY))
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

fn speed_text<'a>(speed: &'static str) -> Element<'a, PanelMessage> {
    let online = speed.ends_with("ms");

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

fn connection_actions<'a>() -> Element<'a, PanelMessage> {
    row![
        icon_button(
            style::icon_text(style::Icon::Cog, 13),
            ConnectionAction::RowSettings
        ),
        icon_button(
            text("?").size(style::text_size::BODY),
            ConnectionAction::RowHelp
        ),
        icon_button(
            style::icon_text(style::Icon::TrashBin, 13),
            ConnectionAction::RowDelete
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

fn connection_notice<'a>() -> Element<'a, PanelMessage> {
    container(
        row![
            text("Become a prop company trader or open a personal account on favorable terms")
                .size(style::text_size::BODY),
            iced::widget::Space::new().width(Length::Fill),
            connection_button("Become a trader", ConnectionAction::BecomeTrader),
            connection_button("Open an account", ConnectionAction::OpenAccount),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(12)
    .style(style::panel_card)
    .into()
}

fn connection_support_log<'a>(feedback: Option<ConnectionAction>) -> Element<'a, PanelMessage> {
    let mut lines = column![
        text("[02:32:08] [OKX: USDT-M / USDC-M] Connected").size(style::text_size::SMALL),
        text("[02:32:08] [OKX: Spot (Margin)] Connected").size(style::text_size::SMALL),
    ]
    .spacing(3);

    if let Some(action) = feedback {
        lines = lines.push(text(action.feedback()).size(style::text_size::SMALL).style(
            |theme: &Theme| text::Style {
                color: Some(theme.extended_palette().secondary.strong.color),
            },
        ));
    }

    panel_card(
        "Problems connecting? Contact support",
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
) -> Element<'a, PanelMessage> {
    button(text(label).size(style::text_size::SMALL))
        .padding(padding::left(10).right(10).top(6).bottom(6))
        .style(style::button::info)
        .on_press(PanelMessage::ConnectionAction(action))
        .into()
}

fn icon_button<'a>(
    content: impl Into<Element<'a, PanelMessage>>,
    action: ConnectionAction,
) -> Element<'a, PanelMessage> {
    button(content)
        .padding(padding::left(5).right(5).top(4).bottom(4))
        .style(style::button::info)
        .on_press(PanelMessage::ConnectionAction(action))
        .into()
}
