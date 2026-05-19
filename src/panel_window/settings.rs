use crate::style;

use iced::{
    Alignment, Border, Element, Length, Theme, padding,
    widget::{button, column, container, row, rule, text},
};

use super::{PanelMessage, utility_button, value_box};

const SETTINGS_NAV: [&str; 4] = ["Hotkeys", "Display", "Sound notifications", "Other"];

pub(super) fn settings_panel<'a>(active: &'static str) -> Element<'a, PanelMessage> {
    let content = match active {
        "Display" => settings_display_content(),
        "Sound notifications" => settings_sound_content(),
        "Other" => settings_other_content(),
        _ => settings_hotkeys_content(),
    };

    row![
        settings_nav(active),
        container(
            column![
                content,
                iced::widget::Space::new().height(Length::Fill),
                row![
                    utility_button("Reset"),
                    utility_button("Import"),
                    utility_button("Export"),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(14),
        )
        .width(Length::Fill)
        .padding(padding::left(14)),
    ]
    .spacing(14)
    .height(Length::Fill)
    .into()
}

fn settings_nav<'a>(active: &'static str) -> Element<'a, PanelMessage> {
    let mut nav = column![].spacing(4).width(Length::Fixed(190.0));

    for item in SETTINGS_NAV {
        nav = nav.push(settings_nav_item(item, item == active));
    }

    container(nav)
        .height(Length::Fill)
        .padding(padding::right(12))
        .style(style::panel_card)
        .into()
}

fn settings_nav_item<'a>(label: &'static str, active: bool) -> Element<'a, PanelMessage> {
    container(text(label).size(style::text_size::BODY))
        .width(Length::Fill)
        .padding(padding::left(10).right(10).top(8).bottom(8))
        .style(move |theme| {
            if active {
                style::panel_nav_active(theme)
            } else {
                style::panel_table_cell(theme)
            }
        })
        .into()
}

fn settings_hotkeys_content<'a>() -> Element<'a, PanelMessage> {
    column![
        section_header("Common"),
        settings_pair("Focus fires", "F1", true),
        settings_pair("Show trades", "F2", true),
        settings_pair("Line notifications", "F5", true),
        settings_pair("Switch to previous tab", "Shift+Tab", false),
        settings_pair("Switch tab", "Tab", true),
        settings_pair("Show spread", "LeftShift", true),
        section_header("Orderbook"),
        settings_pair("Buy order", "LBM", false),
        settings_pair("Sell order", "RBM", false),
        settings_pair("Ticker settings", "F4", true),
        settings_pair("Traded amount 1", "1", false),
        settings_pair("Traded amount 2", "2", false),
        settings_pair("Traded amounts mode", "M", true),
    ]
    .spacing(10)
    .into()
}

fn settings_display_content<'a>() -> Element<'a, PanelMessage> {
    column![
        section_header("Themes"),
        row![
            theme_swatch(0xf5f6f8, true),
            theme_swatch(0x555555, false),
            theme_swatch(0x2b2b2b, false),
            theme_swatch(0xd6d6d6, false),
            theme_swatch(0x1f1f1f, false),
            button(text("+").size(style::text_size::TITLE))
                .padding(padding::left(10).right(10).top(5).bottom(5))
                .style(style::button::info)
                .on_press(PanelMessage::Noop),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        section_header("Interface"),
        settings_value("Application language", "en"),
        settings_value("Orderbook font size", "11"),
        settings_value("Charts font size", "11"),
        settings_value("Orderbook FPS", "30"),
        checkbox_line("Hide positions data", false),
        checkbox_line("Adaptive font size in position information", false),
        checkbox_line("Right-aligned amounts in the orderbook", true),
        checkbox_line("Split best bid/ask in the aggregated orderbook", true),
        checkbox_line("Digit grouping for amounts in the orderbook", true),
    ]
    .spacing(12)
    .into()
}

fn settings_sound_content<'a>() -> Element<'a, PanelMessage> {
    column![
        section_header("Sound notifications"),
        settings_value("Audio output", "System default"),
        settings_value("Volume", "Enabled"),
        checkbox_line("Play sound for trades", true),
        checkbox_line("Play sound for connection changes", true),
        checkbox_line("Mute inactive layouts", false),
        section_header("Streams"),
        settings_pair(
            "Retry audio initialization",
            "Available from Settings",
            true
        ),
        settings_pair("Audio status", "Ready", true),
    ]
    .spacing(12)
    .into()
}

fn settings_other_content<'a>() -> Element<'a, PanelMessage> {
    column![
        section_header("Workspace"),
        settings_value("Main dashboard", "Open"),
        settings_value("Market streams", "Managed by active panes"),
        settings_value("Utility panels", "Opened from the app menu"),
        section_header("Data"),
        settings_value("Data folder", "Application Support / flowsurface"),
        settings_value("Trade fetching", "Binance footprint template"),
        settings_value("Size display", "Quote or base currency"),
        section_header("Network"),
        settings_value("Proxy mode", "Direct connection"),
        checkbox_line("Restart required after proxy changes", true),
    ]
    .spacing(12)
    .into()
}

fn section_header<'a>(title: &'static str) -> Element<'a, PanelMessage> {
    row![
        text(title)
            .size(style::text_size::SECTION)
            .font(iced::Font {
                weight: iced::font::Weight::Bold,
                ..Default::default()
            }),
        rule::horizontal(1).style(style::split_ruler),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

fn settings_pair<'a>(
    label: &'static str,
    value: &'static str,
    enabled: bool,
) -> Element<'a, PanelMessage> {
    row![
        checkbox_mark(enabled),
        text(label)
            .size(style::text_size::BODY)
            .width(Length::Fixed(250.0))
            .style(move |theme: &Theme| text::Style {
                color: Some(setting_text_color(theme, enabled)),
            }),
        text(value)
            .size(style::text_size::BODY)
            .style(move |theme: &Theme| text::Style {
                color: Some(setting_text_color(theme, enabled)),
            }),
    ]
    .spacing(10)
    .height(Length::Fixed(24.0))
    .align_y(Alignment::Center)
    .into()
}

fn settings_value<'a>(label: &'static str, value: &'static str) -> Element<'a, PanelMessage> {
    row![
        iced::widget::Space::new().width(Length::Fixed(24.0)),
        text(label)
            .size(style::text_size::BODY)
            .width(Length::Fixed(220.0))
            .align_x(Alignment::End),
        value_box(value, Length::Fixed(180.0)),
    ]
    .spacing(12)
    .height(Length::Fixed(32.0))
    .align_y(Alignment::Center)
    .into()
}

fn checkbox_line<'a>(label: &'static str, checked: bool) -> Element<'a, PanelMessage> {
    row![
        checkbox_mark(checked),
        text(label).size(style::text_size::BODY),
    ]
    .spacing(10)
    .height(Length::Fixed(24.0))
    .align_y(Alignment::Center)
    .into()
}

fn checkbox_mark<'a>(checked: bool) -> Element<'a, PanelMessage> {
    container(if checked { text("x") } else { text("") }.size(style::text_size::TINY))
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |theme| checkbox_style(theme, checked))
        .into()
}

fn checkbox_style(theme: &Theme, checked: bool) -> iced::widget::container::Style {
    let palette = theme.extended_palette();

    iced::widget::container::Style {
        text_color: Some(if checked {
            palette.background.base.text
        } else {
            palette.background.weak.text
        }),
        background: Some(if checked {
            palette.secondary.strong.color.into()
        } else {
            palette.background.base.color.into()
        }),
        border: Border {
            width: 1.0,
            color: if checked {
                palette.secondary.base.color
            } else {
                palette.background.weak.color
            },
            radius: 3.0.into(),
        },
        snap: true,
        ..Default::default()
    }
}

fn setting_text_color(theme: &Theme, enabled: bool) -> iced::Color {
    let palette = theme.extended_palette();

    if enabled {
        palette.background.base.text
    } else {
        palette.background.weak.text
    }
}

fn theme_swatch<'a>(color: u32, selected: bool) -> Element<'a, PanelMessage> {
    container("")
        .width(Length::Fixed(42.0))
        .height(Length::Fixed(36.0))
        .style(move |theme| style::panel_swatch(theme, rgb(color), selected))
        .into()
}

fn rgb(value: u32) -> iced::Color {
    iced::Color::from_rgb8(
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    )
}
