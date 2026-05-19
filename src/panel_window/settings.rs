use crate::style;

use iced::{
    Alignment, Border, Element, Length, Theme, padding,
    widget::{button, column, container, row, rule, text},
};

use super::{PanelMessage, utility_button, value_box};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    General,
    Appearance,
    Network,
    About,
}

impl SettingsSection {
    const ALL: [Self; 4] = [Self::General, Self::Appearance, Self::Network, Self::About];

    const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Network => "Network & data",
            Self::About => "About",
        }
    }
}

pub(super) fn settings_panel<'a>(active: SettingsSection) -> Element<'a, PanelMessage> {
    let content = match active {
        SettingsSection::General => settings_general_content(),
        SettingsSection::Appearance => settings_appearance_content(),
        SettingsSection::Network => settings_network_content(),
        SettingsSection::About => settings_about_content(),
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

fn settings_nav<'a>(active: SettingsSection) -> Element<'a, PanelMessage> {
    let mut nav = column![].spacing(4).width(Length::Fixed(190.0));

    for section in SettingsSection::ALL {
        nav = nav.push(settings_nav_item(section, section == active));
    }

    container(nav)
        .height(Length::Fill)
        .padding(padding::right(12))
        .style(style::panel_card)
        .into()
}

fn settings_nav_item<'a>(section: SettingsSection, active: bool) -> Element<'a, PanelMessage> {
    button(text(section.label()).size(style::text_size::BODY))
        .width(Length::Fill)
        .padding(padding::left(10).right(10).top(8).bottom(8))
        .style(move |theme, status| style::button::bordered_toggle(theme, status, active))
        .on_press(PanelMessage::SettingsSection(section))
        .into()
}

fn settings_general_content<'a>() -> Element<'a, PanelMessage> {
    column![
        section_header("Workspace"),
        settings_value("Open data folder", "Application Support / flowsurface"),
        section_header("Interface"),
        settings_choice("Sidebar position", &["Left", "Right"], 0),
        settings_choice("Time zone", &["Local", "UTC"], 0),
        settings_stepper("Interface scale", "100%"),
        checkbox_line("Size in quote currency", true),
        section_header("Experimental"),
        checkbox_line("Fetch trades (Binance)", false),
    ]
    .spacing(12)
    .into()
}

fn settings_appearance_content<'a>() -> Element<'a, PanelMessage> {
    column![
        section_header("Theme"),
        settings_value("Current theme", "App theme"),
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
        section_header("Theme editor"),
        settings_value("Editor", "Open theme editor"),
        settings_value("Custom theme", "Optional custom theme slot"),
    ]
    .spacing(12)
    .into()
}

fn settings_network_content<'a>() -> Element<'a, PanelMessage> {
    column![
        section_header("Network"),
        settings_value("Network editor", "Open network panel"),
        settings_value("Proxy mode", "Direct connection"),
        checkbox_line("Use direct exchange connections", true),
        section_header("Market data"),
        checkbox_line("Fetch trades (Binance)", false),
        settings_value("Footprint trades", "Experimental fetcher"),
        settings_value("Data folder", "Open from General"),
    ]
    .spacing(12)
    .into()
}

fn settings_about_content<'a>() -> Element<'a, PanelMessage> {
    column![
        section_header("Build"),
        settings_value("Version", env!("CARGO_PKG_VERSION")),
        settings_value("Repository", env!("CARGO_PKG_REPOSITORY")),
        settings_value("Build metadata", "Available in the app gear menu"),
        section_header("Utility panels"),
        settings_value("Menu panels", "Opened from the top in-window menu"),
        settings_value("Connections", "Stateful template with live pings"),
        settings_value("PnL", "Chart and trades template"),
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

fn settings_choice<'a>(
    label: &'static str,
    values: &[&'static str],
    active_index: usize,
) -> Element<'a, PanelMessage> {
    let mut choices = row![].spacing(6).align_y(Alignment::Center);

    for (index, value) in values.iter().enumerate() {
        let active = index == active_index;

        choices = choices.push(
            button(text(*value).size(style::text_size::SMALL))
                .padding(padding::left(10).right(10).top(6).bottom(6))
                .style(move |theme, status| style::button::bordered_toggle(theme, status, active))
                .on_press(PanelMessage::Noop),
        );
    }

    setting_row(label, choices)
}

fn settings_stepper<'a>(label: &'static str, value: &'static str) -> Element<'a, PanelMessage> {
    setting_row(
        label,
        row![
            utility_button("-"),
            value_box(value, Length::Fixed(74.0)),
            utility_button("+"),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
}

fn settings_value<'a>(label: &'static str, value: &'static str) -> Element<'a, PanelMessage> {
    setting_row(label, value_box(value, Length::Fixed(240.0)))
}

fn setting_row<'a>(
    label: &'static str,
    control: impl Into<Element<'a, PanelMessage>>,
) -> Element<'a, PanelMessage> {
    row![
        iced::widget::Space::new().width(Length::Fixed(24.0)),
        text(label)
            .size(style::text_size::BODY)
            .width(Length::Fixed(190.0))
            .align_x(Alignment::End),
        control.into(),
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
