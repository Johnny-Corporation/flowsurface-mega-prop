use crate::style;

use iced::{
    Alignment, Border, Element, Length, Theme, padding,
    widget::{button, column, container, row, rule, text},
};

use super::{PanelMessage, value_box};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    General,
    Appearance,
    Network,
    About,
}

#[derive(Debug, Clone)]
pub(crate) struct SettingsPanelState {
    section: SettingsSection,
    sidebar_position: usize,
    timezone: usize,
    scale_percent: u16,
    size_in_quote: bool,
    fetch_trades: bool,
    theme_index: usize,
    last_action: &'static str,
}

impl Default for SettingsPanelState {
    fn default() -> Self {
        Self {
            section: SettingsSection::General,
            sidebar_position: 0,
            timezone: 0,
            scale_percent: 100,
            size_in_quote: true,
            fetch_trades: false,
            theme_index: 0,
            last_action: "Ready",
        }
    }
}

impl SettingsPanelState {
    pub(crate) fn update(&mut self, action: SettingsAction) {
        match action {
            SettingsAction::SelectSection(section) => {
                self.section = section;
                self.last_action = "Section changed";
            }
            SettingsAction::SelectSidebar(index) => {
                self.sidebar_position = index;
                self.last_action = "Sidebar position changed";
            }
            SettingsAction::SelectTimezone(index) => {
                self.timezone = index;
                self.last_action = "Time zone changed";
            }
            SettingsAction::ChangeScale(delta) => {
                let next = self.scale_percent as i16 + delta;
                self.scale_percent = next.clamp(80, 160) as u16;
                self.last_action = "Interface scale changed";
            }
            SettingsAction::ToggleQuoteSize => {
                self.size_in_quote = !self.size_in_quote;
                self.last_action = "Size display toggled";
            }
            SettingsAction::ToggleTradeFetch => {
                self.fetch_trades = !self.fetch_trades;
                self.last_action = "Trade fetcher toggled";
            }
            SettingsAction::SelectTheme(index) => {
                self.theme_index = index;
                self.last_action = "Theme selected";
            }
            SettingsAction::Reset => {
                *self = Self::default();
                self.last_action = "Settings reset";
            }
            SettingsAction::Note(label) => {
                self.last_action = label;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsAction {
    SelectSection(SettingsSection),
    SelectSidebar(usize),
    SelectTimezone(usize),
    ChangeScale(i16),
    ToggleQuoteSize,
    ToggleTradeFetch,
    SelectTheme(usize),
    Reset,
    Note(&'static str),
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

pub(super) fn settings_panel<'a>(state: &'a SettingsPanelState) -> Element<'a, PanelMessage> {
    let content = match state.section {
        SettingsSection::General => settings_general_content(state),
        SettingsSection::Appearance => settings_appearance_content(state),
        SettingsSection::Network => settings_network_content(state),
        SettingsSection::About => settings_about_content(),
    };

    row![
        settings_nav(state.section),
        container(
            column![
                content,
                iced::widget::Space::new().height(Length::Fill),
                row![
                    text(format!("Last action: {}", state.last_action))
                        .size(style::text_size::SMALL),
                    iced::widget::Space::new().width(Length::Fill),
                    settings_action_button("Reset", SettingsAction::Reset),
                    settings_action_button("Import", SettingsAction::Note("Import pressed")),
                    settings_action_button("Export", SettingsAction::Note("Export pressed")),
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
        .on_press(PanelMessage::SettingsAction(SettingsAction::SelectSection(
            section,
        )))
        .into()
}

fn settings_general_content<'a>(state: &'a SettingsPanelState) -> Element<'a, PanelMessage> {
    column![
        section_header("Workspace"),
        settings_value("Open data folder", "Application Support / flowsurface"),
        section_header("Interface"),
        settings_choice(
            "Sidebar position",
            &["Left", "Right"],
            state.sidebar_position,
            SettingsAction::SelectSidebar
        ),
        settings_choice(
            "Time zone",
            &["Local", "UTC"],
            state.timezone,
            SettingsAction::SelectTimezone
        ),
        settings_stepper("Interface scale", &format!("{}%", state.scale_percent)),
        checkbox_line(
            "Size in quote currency",
            state.size_in_quote,
            SettingsAction::ToggleQuoteSize
        ),
        section_header("Experimental"),
        checkbox_line(
            "Fetch trades (Binance)",
            state.fetch_trades,
            SettingsAction::ToggleTradeFetch
        ),
    ]
    .spacing(12)
    .into()
}

fn settings_appearance_content<'a>(state: &'a SettingsPanelState) -> Element<'a, PanelMessage> {
    column![
        section_header("Theme"),
        settings_value("Current theme", "App theme"),
        row![
            theme_swatch(0, 0xf5f6f8, state.theme_index == 0),
            theme_swatch(1, 0x555555, state.theme_index == 1),
            theme_swatch(2, 0x2b2b2b, state.theme_index == 2),
            theme_swatch(3, 0xd6d6d6, state.theme_index == 3),
            theme_swatch(4, 0x1f1f1f, state.theme_index == 4),
            button(text("+").size(style::text_size::TITLE))
                .padding(padding::left(10).right(10).top(5).bottom(5))
                .style(style::button::info)
                .on_press(PanelMessage::SettingsAction(SettingsAction::Note(
                    "Add theme pressed"
                ))),
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

fn settings_network_content<'a>(state: &'a SettingsPanelState) -> Element<'a, PanelMessage> {
    column![
        section_header("Network"),
        settings_value("Network editor", "Open network panel"),
        settings_value("Proxy mode", "Direct connection"),
        checkbox_line(
            "Use direct exchange connections",
            true,
            SettingsAction::Note("Direct connections kept enabled")
        ),
        section_header("Market data"),
        checkbox_line(
            "Fetch trades (Binance)",
            state.fetch_trades,
            SettingsAction::ToggleTradeFetch
        ),
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
    action: fn(usize) -> SettingsAction,
) -> Element<'a, PanelMessage> {
    let mut choices = row![].spacing(6).align_y(Alignment::Center);

    for (index, value) in values.iter().enumerate() {
        let active = index == active_index;

        choices = choices.push(
            button(text(*value).size(style::text_size::SMALL))
                .padding(padding::left(10).right(10).top(6).bottom(6))
                .style(move |theme, status| style::button::bordered_toggle(theme, status, active))
                .on_press(PanelMessage::SettingsAction(action(index))),
        );
    }

    setting_row(label, choices)
}

fn settings_stepper<'a>(
    label: &'static str,
    value: impl Into<String>,
) -> Element<'a, PanelMessage> {
    setting_row(
        label,
        row![
            settings_action_button("-", SettingsAction::ChangeScale(-10)),
            value_box(value, Length::Fixed(74.0)),
            settings_action_button("+", SettingsAction::ChangeScale(10)),
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

fn checkbox_line<'a>(
    label: &'static str,
    checked: bool,
    action: SettingsAction,
) -> Element<'a, PanelMessage> {
    button(
        row![
            checkbox_mark(checked),
            text(label).size(style::text_size::BODY),
        ]
        .spacing(10)
        .height(Length::Fixed(24.0))
        .align_y(Alignment::Center),
    )
    .padding(0)
    .style(style::button::text_link_secondary)
    .on_press(PanelMessage::SettingsAction(action))
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

fn settings_action_button<'a>(
    label: &'static str,
    action: SettingsAction,
) -> Element<'a, PanelMessage> {
    button(text(label).size(style::text_size::SMALL))
        .padding(padding::left(10).right(10).top(6).bottom(6))
        .style(style::button::info)
        .on_press(PanelMessage::SettingsAction(action))
        .into()
}

fn theme_swatch<'a>(index: usize, color: u32, selected: bool) -> Element<'a, PanelMessage> {
    button(
        container("")
            .width(Length::Fixed(42.0))
            .height(Length::Fixed(36.0))
            .style(move |theme| style::panel_swatch(theme, rgb(color), selected)),
    )
    .padding(1)
    .style(move |theme, status| style::button::bordered_toggle(theme, status, selected))
    .on_press(PanelMessage::SettingsAction(SettingsAction::SelectTheme(
        index,
    )))
    .into()
}

fn rgb(value: u32) -> iced::Color {
    iced::Color::from_rgb8(
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    )
}
