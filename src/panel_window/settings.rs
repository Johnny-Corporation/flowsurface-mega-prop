use crate::{
    style::{self, Icon},
    widget::info_hint,
};

use iced::{
    Alignment, Border, Element, Length, Theme, padding,
    widget::{button, column, container, row, rule, scrollable, text},
};

use super::PanelMessage;
use super::settings_info::SettingsInfo;

const SETTINGS_SMALL: f32 = style::text_size::SMALL + 1.0;
const SETTINGS_BODY: f32 = style::text_size::BODY + 2.0;
const SETTINGS_TITLE: f32 = style::text_size::TITLE + 2.0;
pub(crate) const DEFAULT_ACCENT_COLOR: &str = "#5B6CFF";
const ACCENT_COLORS: [(u32, &str); 5] = [
    (0x5b6cff, DEFAULT_ACCENT_COLOR),
    (0x51cda0, "#51CDA0"),
    (0xeeca8b, "#EECA8B"),
    (0xd95f7a, "#D95F7A"),
    (0xc8c8c8, "#C8C8C8"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    General,
    Chart,
    Trading,
    DomLadder,
    Notifications,
    Hotkeys,
    Appearance,
    Data,
    Risk,
    About,
}

#[derive(Debug, Clone)]
pub(crate) struct SettingsPanelState {
    section: SettingsSection,
    language_index: usize,
    sidebar_position: usize,
    timezone: usize,
    scale_percent: u16,
    size_in_quote: bool,
    fetch_trades: bool,
    theme_mode: usize,
    theme_index: usize,
    accent_color_index: usize,
    update_channel: usize,
    start_on_launch: bool,
    minimize_to_tray: bool,
    confirm_on_exit: bool,
    auto_updates: bool,
    confirm_orders: bool,
    ladder_recenter: bool,
    notifications_enabled: bool,
    hotkeys_enabled: bool,
    direct_connections: bool,
    risk_guard: bool,
    info_hints: info_hint::State<SettingsInfo>,
    last_action: &'static str,
}

impl Default for SettingsPanelState {
    fn default() -> Self {
        Self::new(DEFAULT_ACCENT_COLOR)
    }
}

impl SettingsPanelState {
    pub(crate) fn new(accent_color: &str) -> Self {
        Self {
            section: SettingsSection::General,
            language_index: 0,
            sidebar_position: 0,
            timezone: 0,
            scale_percent: 100,
            size_in_quote: true,
            fetch_trades: false,
            theme_mode: 0,
            theme_index: 0,
            accent_color_index: accent_color_index(accent_color),
            update_channel: 0,
            start_on_launch: true,
            minimize_to_tray: true,
            confirm_on_exit: false,
            auto_updates: true,
            confirm_orders: true,
            ladder_recenter: true,
            notifications_enabled: true,
            hotkeys_enabled: true,
            direct_connections: true,
            risk_guard: true,
            info_hints: info_hint::State::new(),
            last_action: "Ready",
        }
    }

    pub(crate) fn update(&mut self, action: SettingsAction) -> Option<String> {
        match action {
            SettingsAction::SelectSection(section) => {
                self.section = section;
                self.info_hints.close();
                self.last_action = "Section changed";
            }
            SettingsAction::ToggleInfo(info) => {
                let opened = self.info_hints.toggle(info);
                self.last_action = if opened { "Info opened" } else { "Info closed" };
            }
            SettingsAction::SelectLanguage(index) => {
                self.language_index = index;
                self.last_action = "Language changed";
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
            SettingsAction::SelectThemeMode(index) => {
                self.theme_mode = index;
                self.last_action = "Theme mode selected";
            }
            SettingsAction::SelectTheme(index) => {
                self.theme_index = index;
                self.last_action = "Theme selected";
            }
            SettingsAction::SelectAccentColor(index) => {
                let index = index.min(ACCENT_COLORS.len().saturating_sub(1));
                self.accent_color_index = index;
                self.last_action = "Accent color saved";
                return Some(accent_color_hex(index).to_string());
            }
            SettingsAction::SelectUpdateChannel(index) => {
                self.update_channel = index;
                self.last_action = "Update channel selected";
            }
            SettingsAction::ToggleStartOnLaunch => {
                self.start_on_launch = !self.start_on_launch;
                self.last_action = "Launch setting toggled";
            }
            SettingsAction::ToggleMinimizeToTray => {
                self.minimize_to_tray = !self.minimize_to_tray;
                self.last_action = "Tray setting toggled";
            }
            SettingsAction::ToggleConfirmExit => {
                self.confirm_on_exit = !self.confirm_on_exit;
                self.last_action = "Exit confirmation toggled";
            }
            SettingsAction::ToggleAutoUpdates => {
                self.auto_updates = !self.auto_updates;
                self.last_action = "Update checking toggled";
            }
            SettingsAction::ToggleOrderConfirm => {
                self.confirm_orders = !self.confirm_orders;
                self.last_action = "Order confirmation toggled";
            }
            SettingsAction::ToggleLadderRecenter => {
                self.ladder_recenter = !self.ladder_recenter;
                self.last_action = "Ladder recenter toggled";
            }
            SettingsAction::ToggleNotifications => {
                self.notifications_enabled = !self.notifications_enabled;
                self.last_action = "Notifications toggled";
            }
            SettingsAction::ToggleHotkeys => {
                self.hotkeys_enabled = !self.hotkeys_enabled;
                self.last_action = "Hotkeys toggled";
            }
            SettingsAction::ToggleDirectConnections => {
                self.direct_connections = !self.direct_connections;
                self.last_action = "Direct connections toggled";
            }
            SettingsAction::ToggleRiskGuard => {
                self.risk_guard = !self.risk_guard;
                self.last_action = "Risk guard toggled";
            }
            SettingsAction::Reset => {
                *self = Self::default();
                self.last_action = "Settings reset";
                return Some(DEFAULT_ACCENT_COLOR.to_string());
            }
            SettingsAction::Note(label) => {
                self.last_action = label;
            }
        }

        None
    }
}

pub(crate) fn accent_color_hex(index: usize) -> &'static str {
    ACCENT_COLORS
        .get(index)
        .map(|(_, hex)| *hex)
        .unwrap_or(DEFAULT_ACCENT_COLOR)
}

fn accent_color_index(hex: &str) -> usize {
    ACCENT_COLORS
        .iter()
        .position(|(_, candidate)| candidate.eq_ignore_ascii_case(hex))
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsAction {
    SelectSection(SettingsSection),
    ToggleInfo(SettingsInfo),
    SelectLanguage(usize),
    SelectSidebar(usize),
    SelectTimezone(usize),
    ChangeScale(i16),
    ToggleQuoteSize,
    ToggleTradeFetch,
    SelectThemeMode(usize),
    SelectTheme(usize),
    SelectAccentColor(usize),
    SelectUpdateChannel(usize),
    ToggleStartOnLaunch,
    ToggleMinimizeToTray,
    ToggleConfirmExit,
    ToggleAutoUpdates,
    ToggleOrderConfirm,
    ToggleLadderRecenter,
    ToggleNotifications,
    ToggleHotkeys,
    ToggleDirectConnections,
    ToggleRiskGuard,
    Reset,
    Note(&'static str),
}

impl SettingsSection {
    const ALL: [Self; 10] = [
        Self::General,
        Self::Chart,
        Self::Trading,
        Self::DomLadder,
        Self::Notifications,
        Self::Hotkeys,
        Self::Appearance,
        Self::Data,
        Self::Risk,
        Self::About,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Chart => "Chart",
            Self::Trading => "Trading",
            Self::DomLadder => "DOM & Ladder",
            Self::Notifications => "Notifications",
            Self::Hotkeys => "Hotkeys",
            Self::Appearance => "Appearance",
            Self::Data => "Data & Connections",
            Self::Risk => "Risk Management",
            Self::About => "About",
        }
    }

    const fn subtitle(self) -> &'static str {
        match self {
            Self::General => "Basic application settings",
            Self::Chart => "Chart behavior and market display",
            Self::Trading => "Manual order-entry safeguards",
            Self::DomLadder => "Depth ladder and DOM controls",
            Self::Notifications => "Alerts, sounds, and desktop notices",
            Self::Hotkeys => "Keyboard workflow controls",
            Self::Appearance => "Theme and visual customization",
            Self::Data => "Storage, proxy, and market-data settings",
            Self::Risk => "Capital protection and beta guardrails",
            Self::About => "Build and utility panel information",
        }
    }

    const fn icon(self) -> Icon {
        match self {
            Self::General => Icon::Cog,
            Self::Chart => Icon::ChartOutline,
            Self::Trading => Icon::Return,
            Self::DomLadder => Icon::Layout,
            Self::Notifications => Icon::SpeakerHigh,
            Self::Hotkeys => Icon::Edit,
            Self::Appearance => Icon::Star,
            Self::Data => Icon::Folder,
            Self::Risk => Icon::Link,
            Self::About => Icon::ExternalLink,
        }
    }
}

pub(super) fn settings_panel<'a>(state: &'a SettingsPanelState) -> Element<'a, PanelMessage> {
    column![
        settings_actions(),
        rule::horizontal(1).style(style::split_ruler),
        row![
            settings_nav(state.section),
            rule::vertical(1).style(style::split_ruler),
            settings_content(state)
        ]
        .spacing(0)
        .height(Length::Fill),
    ]
    .spacing(8)
    .height(Length::Fill)
    .into()
}

fn settings_nav<'a>(active: SettingsSection) -> Element<'a, PanelMessage> {
    let mut nav = column![].spacing(2).width(Length::Fixed(188.0));

    for section in SettingsSection::ALL {
        nav = nav.push(settings_nav_item(section, section == active));
    }

    container(nav)
        .height(Length::Fill)
        .padding(padding::top(6).right(10).bottom(8))
        .into()
}

fn settings_nav_item<'a>(section: SettingsSection, active: bool) -> Element<'a, PanelMessage> {
    button(
        row![
            style::icon_text(section.icon(), 17),
            text(section.label()).size(SETTINGS_BODY),
        ]
        .spacing(10)
        .height(Length::Fixed(25.0))
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(padding::left(9).right(9).top(5).bottom(5))
    .style(move |theme, status| settings_nav_button(theme, status, active))
    .on_press(PanelMessage::SettingsAction(SettingsAction::SelectSection(
        section,
    )))
    .into()
}

fn settings_content<'a>(state: &'a SettingsPanelState) -> Element<'a, PanelMessage> {
    let rows = match state.section {
        SettingsSection::General => settings_general_content(state),
        SettingsSection::Chart => settings_chart_content(state),
        SettingsSection::Trading => settings_trading_content(state),
        SettingsSection::DomLadder => settings_dom_content(state),
        SettingsSection::Notifications => settings_notifications_content(state),
        SettingsSection::Hotkeys => settings_hotkeys_content(state),
        SettingsSection::Appearance => settings_appearance_content(state),
        SettingsSection::Data => settings_data_content(state),
        SettingsSection::Risk => settings_risk_content(state),
        SettingsSection::About => settings_about_content(),
    };

    container(
        column![
            settings_section_title(state.section),
            scrollable(rows)
                .height(Length::Fill)
                .style(style::scroll_bar),
        ]
        .spacing(12)
        .height(Length::Fill)
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(padding::left(18).right(10).top(12).bottom(8))
    .into()
}

fn settings_section_title<'a>(section: SettingsSection) -> Element<'a, PanelMessage> {
    column![
        text(section.label()).size(SETTINGS_TITLE).font(iced::Font {
            weight: iced::font::Weight::Bold,
            ..Default::default()
        }),
        text(section.subtitle())
            .size(SETTINGS_BODY)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.extended_palette().background.weak.text),
            }),
    ]
    .spacing(5)
    .into()
}

fn settings_general_content<'a>(state: &'a SettingsPanelState) -> Element<'a, PanelMessage> {
    column![
        settings_dropdown(
            "Language",
            ["English", "Russian"],
            state.language_index,
            SettingsAction::SelectLanguage
        ),
        settings_toggle_with_info(
            "Start on system launch",
            state.start_on_launch,
            SettingsAction::ToggleStartOnLaunch,
            SettingsInfo::StartOnLaunch,
            state
        ),
        settings_toggle(
            "Minimize to tray",
            state.minimize_to_tray,
            SettingsAction::ToggleMinimizeToTray
        ),
        settings_toggle(
            "Confirm on exit",
            state.confirm_on_exit,
            SettingsAction::ToggleConfirmExit
        ),
        settings_segmented(
            "Theme",
            &["Dark", "Light", "System"],
            state.theme_mode,
            SettingsAction::SelectThemeMode
        ),
        settings_toggle(
            "Auto check for updates",
            state.auto_updates,
            SettingsAction::ToggleAutoUpdates
        ),
        settings_dropdown(
            "Update channel",
            ["Stable", "Beta"],
            state.update_channel,
            SettingsAction::SelectUpdateChannel
        ),
    ]
    .spacing(8)
    .into()
}

fn settings_chart_content<'a>(state: &'a SettingsPanelState) -> Element<'a, PanelMessage> {
    column![
        settings_segmented(
            "Sidebar position",
            &["Left", "Right"],
            state.sidebar_position,
            SettingsAction::SelectSidebar
        ),
        settings_segmented(
            "Time zone",
            &["Local", "UTC"],
            state.timezone,
            SettingsAction::SelectTimezone
        ),
        settings_stepper("Interface scale", format!("{}%", state.scale_percent)),
        settings_toggle(
            "Size in quote currency",
            state.size_in_quote,
            SettingsAction::ToggleQuoteSize
        ),
        settings_value("Default chart", "Candles and order-flow panes"),
        settings_value("History preload", "Managed per active pane"),
    ]
    .spacing(8)
    .into()
}

fn settings_trading_content<'a>(state: &'a SettingsPanelState) -> Element<'a, PanelMessage> {
    column![
        settings_toggle_with_info(
            "Confirm order placement",
            state.confirm_orders,
            SettingsAction::ToggleOrderConfirm,
            SettingsInfo::OrderConfirmation,
            state
        ),
        settings_value("Default order size", "Configured per connection"),
        settings_value("Execution mode", "Manual discretionary trading"),
        settings_value("Allowed venue", "Bybit beta subaccount first"),
        settings_value("Dangerous actions", "Confirm before scaling beyond beta"),
    ]
    .spacing(8)
    .into()
}

fn settings_dom_content<'a>(state: &'a SettingsPanelState) -> Element<'a, PanelMessage> {
    column![
        settings_toggle(
            "Auto recenter ladder",
            state.ladder_recenter,
            SettingsAction::ToggleLadderRecenter
        ),
        settings_value("Default ladder view", "CSCALP DOM"),
        settings_value("Depth aggregation", "Automatic by symbol"),
        settings_value("Order book refresh", "Driven by active exchange stream"),
        settings_value("Position marker", "Future trading-state overlay"),
    ]
    .spacing(8)
    .into()
}

fn settings_notifications_content<'a>(state: &'a SettingsPanelState) -> Element<'a, PanelMessage> {
    column![
        settings_toggle(
            "Enable notifications",
            state.notifications_enabled,
            SettingsAction::ToggleNotifications
        ),
        settings_value("Trade alert sound", "Default"),
        settings_value("Connection alerts", "Visible in panel status"),
        settings_value("Risk warnings", "Always visible when configured"),
    ]
    .spacing(8)
    .into()
}

fn settings_hotkeys_content<'a>(state: &'a SettingsPanelState) -> Element<'a, PanelMessage> {
    column![
        settings_toggle(
            "Enable hotkeys",
            state.hotkeys_enabled,
            SettingsAction::ToggleHotkeys
        ),
        settings_value("Order entry hotkeys", "Not assigned"),
        settings_value("Pane navigation", "Default"),
        settings_value("Emergency close", "Requires explicit binding"),
    ]
    .spacing(8)
    .into()
}

fn settings_appearance_content<'a>(state: &'a SettingsPanelState) -> Element<'a, PanelMessage> {
    column![
        settings_segmented(
            "Theme mode",
            &["Dark", "Light", "System"],
            state.theme_mode,
            SettingsAction::SelectThemeMode
        ),
        settings_accent_color_row(state.accent_color_index),
        settings_swatch_row(state.theme_index),
        settings_value("Theme editor", "Open theme editor"),
        settings_value("Density", "Balanced"),
    ]
    .spacing(8)
    .into()
}

fn settings_data_content<'a>(state: &'a SettingsPanelState) -> Element<'a, PanelMessage> {
    column![
        settings_value("Open data folder", "Application Support / flowsurface"),
        settings_value("Network editor", "Open network panel"),
        settings_value("Proxy mode", "Direct connection"),
        settings_toggle_with_info(
            "Use direct exchange connections",
            state.direct_connections,
            SettingsAction::ToggleDirectConnections,
            SettingsInfo::DirectConnections,
            state
        ),
        settings_toggle(
            "Fetch trades (Binance)",
            state.fetch_trades,
            SettingsAction::ToggleTradeFetch
        ),
        settings_value("Footprint trades", "Experimental fetcher"),
    ]
    .spacing(8)
    .into()
}

fn settings_risk_content<'a>(state: &'a SettingsPanelState) -> Element<'a, PanelMessage> {
    column![
        settings_toggle_with_info(
            "Enable local risk guard",
            state.risk_guard,
            SettingsAction::ToggleRiskGuard,
            SettingsInfo::RiskGuard,
            state
        ),
        settings_value("Daily loss warning", "Not configured"),
        settings_value("Max order size", "Not configured"),
        settings_value("Allowed symbols", "Connection default"),
        settings_value("Kill switch", "Future proxy control"),
    ]
    .spacing(8)
    .into()
}

fn settings_about_content<'a>() -> Element<'a, PanelMessage> {
    column![
        settings_value("Version", env!("CARGO_PKG_VERSION")),
        settings_value("Repository", env!("CARGO_PKG_REPOSITORY")),
        settings_value("Build metadata", "Available in the app gear menu"),
        settings_value("Menu panels", "Opened from the top in-window menu"),
        settings_value("Connections", "Stateful template with live pings"),
        settings_value("PnL", "Chart and trades template"),
    ]
    .spacing(8)
    .into()
}

fn settings_dropdown<'a>(
    label: &'static str,
    values: [&'static str; 2],
    active_index: usize,
    action: fn(usize) -> SettingsAction,
) -> Element<'a, PanelMessage> {
    let active = active_index.min(values.len().saturating_sub(1));

    setting_row(
        label,
        button(dropdown_box(values[active]))
            .padding(0)
            .style(style::button::text_link_secondary)
            .on_press(PanelMessage::SettingsAction(action(
                (active + 1) % values.len(),
            ))),
    )
}

fn dropdown_box<'a>(value: impl Into<String>) -> Element<'a, PanelMessage> {
    container(
        row![
            text(value.into()).size(SETTINGS_BODY),
            iced::widget::Space::new().width(Length::Fill),
            text("v").size(SETTINGS_SMALL),
        ]
        .height(Length::Fill)
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .width(Length::Fixed(240.0))
    .height(Length::Fixed(34.0))
    .padding(padding::left(12).right(12).top(5).bottom(5))
    .style(style::panel_value_box)
    .into()
}

fn settings_segmented<'a>(
    label: &'static str,
    values: &[&'static str],
    active_index: usize,
    action: fn(usize) -> SettingsAction,
) -> Element<'a, PanelMessage> {
    let mut choices = row![].spacing(0).align_y(Alignment::Center);

    for (index, value) in values.iter().enumerate() {
        let active = index == active_index;

        choices = choices.push(
            button(text(*value).size(SETTINGS_BODY))
                .width(Length::Fixed(74.0))
                .padding(padding::left(7).right(7).top(7).bottom(7))
                .style(move |theme, status| settings_segment_button(theme, status, active))
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
            compact_action_button("-", SettingsAction::ChangeScale(-10)),
            settings_value_box(value, Length::Fixed(70.0)),
            compact_action_button("+", SettingsAction::ChangeScale(10)),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
}

fn settings_toggle<'a>(
    label: &'static str,
    checked: bool,
    action: SettingsAction,
) -> Element<'a, PanelMessage> {
    setting_row(
        label,
        button(toggle_switch(checked))
            .padding(0)
            .style(style::button::text_link_secondary)
            .on_press(PanelMessage::SettingsAction(action)),
    )
}

fn settings_toggle_with_info<'a>(
    label: &'static str,
    checked: bool,
    action: SettingsAction,
    info: SettingsInfo,
    state: &'a SettingsPanelState,
) -> Element<'a, PanelMessage> {
    setting_row_with_info(
        label,
        button(toggle_switch(checked))
            .padding(0)
            .style(style::button::text_link_secondary)
            .on_press(PanelMessage::SettingsAction(action)),
        info,
        state,
    )
}

fn toggle_switch<'a>(checked: bool) -> Element<'a, PanelMessage> {
    let knob = container("")
        .width(Length::Fixed(22.0))
        .height(Length::Fixed(22.0))
        .style(move |theme| toggle_knob_style(theme, checked));

    let track = if checked {
        row![iced::widget::Space::new().width(Length::Fill), knob,]
    } else {
        row![knob, iced::widget::Space::new().width(Length::Fill),]
    };

    container(track.align_y(Alignment::Center))
        .width(Length::Fixed(54.0))
        .height(Length::Fixed(30.0))
        .padding(4)
        .style(move |theme| toggle_track_style(theme, checked))
        .into()
}

fn settings_value<'a>(label: &'static str, value: &'static str) -> Element<'a, PanelMessage> {
    setting_row(label, dropdown_box(value))
}

fn settings_value_box<'a>(value: impl Into<String>, width: Length) -> Element<'a, PanelMessage> {
    container(text(value.into()).size(SETTINGS_BODY))
        .width(width)
        .height(Length::Fixed(34.0))
        .align_y(Alignment::Center)
        .padding(padding::left(10).right(10).top(5).bottom(5))
        .style(style::panel_value_box)
        .into()
}

fn setting_row<'a>(
    label: &'static str,
    control: impl Into<Element<'a, PanelMessage>>,
) -> Element<'a, PanelMessage> {
    setting_row_content(text(label).size(SETTINGS_BODY).into(), control)
}

fn setting_row_with_info<'a>(
    label: &'static str,
    control: impl Into<Element<'a, PanelMessage>>,
    info: SettingsInfo,
    state: &'a SettingsPanelState,
) -> Element<'a, PanelMessage> {
    let active = state.info_hints.is_active(info);
    setting_row_content(setting_label_with_info(label, info, active), control)
}

fn setting_label_with_info<'a>(
    label: &'static str,
    info: SettingsInfo,
    active: bool,
) -> Element<'a, PanelMessage> {
    row![
        text(label).size(SETTINGS_BODY),
        info_hint::button(
            active,
            PanelMessage::SettingsAction(SettingsAction::ToggleInfo(info)),
            info.title(),
            info.body()
        ),
    ]
    .spacing(7)
    .align_y(Alignment::Center)
    .into()
}

fn setting_row_content<'a>(
    label: Element<'a, PanelMessage>,
    control: impl Into<Element<'a, PanelMessage>>,
) -> Element<'a, PanelMessage> {
    container(
        row![
            label,
            iced::widget::Space::new().width(Length::Fill),
            control.into(),
        ]
        .height(Length::Fill)
        .spacing(16)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(50.0))
    .padding(padding::left(14).right(14))
    .style(settings_row_card)
    .into()
}

fn settings_accent_color_row<'a>(selected_index: usize) -> Element<'a, PanelMessage> {
    let swatches = ACCENT_COLORS.iter().enumerate().fold(
        row![].spacing(8).align_y(Alignment::Center),
        |row, (index, (color, _))| {
            row.push(color_swatch(
                index,
                *color,
                selected_index == index,
                SettingsAction::SelectAccentColor,
            ))
        },
    );

    setting_row("Accent color", swatches)
}

fn settings_swatch_row<'a>(selected_index: usize) -> Element<'a, PanelMessage> {
    let swatches = row![
        theme_swatch(0, 0xf5f6f8, selected_index == 0),
        theme_swatch(1, 0x555555, selected_index == 1),
        theme_swatch(2, 0x2b2b2b, selected_index == 2),
        theme_swatch(3, 0xd6d6d6, selected_index == 3),
        theme_swatch(4, 0x1f1f1f, selected_index == 4),
        compact_action_button("+", SettingsAction::Note("Add theme pressed")),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    setting_row("Theme palette", swatches)
}

fn settings_actions<'a>() -> Element<'a, PanelMessage> {
    row![
        button(text("Reset to defaults").size(SETTINGS_BODY))
            .padding(padding::left(12).right(12).top(7).bottom(7))
            .style(settings_secondary_button)
            .on_press(PanelMessage::RequestSettingsReset),
        button(text("Cancel").size(SETTINGS_BODY))
            .padding(padding::left(18).right(18).top(7).bottom(7))
            .style(settings_secondary_button)
            .on_press(PanelMessage::SettingsAction(SettingsAction::Note(
                "Cancel pressed"
            ))),
        button(text("Apply").size(SETTINGS_BODY))
            .padding(padding::left(20).right(20).top(7).bottom(7))
            .style(settings_apply_button)
            .on_press(PanelMessage::SettingsAction(SettingsAction::Note(
                "Apply pressed"
            ))),
        iced::widget::Space::new().width(Length::Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

fn compact_action_button<'a>(
    label: &'static str,
    action: SettingsAction,
) -> Element<'a, PanelMessage> {
    button(text(label).size(SETTINGS_BODY))
        .padding(padding::left(10).right(10).top(5).bottom(5))
        .style(settings_secondary_button)
        .on_press(PanelMessage::SettingsAction(action))
        .into()
}

fn theme_swatch<'a>(index: usize, color: u32, selected: bool) -> Element<'a, PanelMessage> {
    color_swatch(index, color, selected, SettingsAction::SelectTheme)
}

fn color_swatch<'a>(
    index: usize,
    color: u32,
    selected: bool,
    action: fn(usize) -> SettingsAction,
) -> Element<'a, PanelMessage> {
    button(
        container("")
            .width(Length::Fixed(32.0))
            .height(Length::Fixed(26.0))
            .style(move |theme| style::panel_swatch(theme, rgb(color), selected)),
    )
    .padding(1)
    .style(move |theme, status| settings_segment_button(theme, status, selected))
    .on_press(PanelMessage::SettingsAction(action(index)))
    .into()
}

fn settings_nav_button(
    theme: &Theme,
    status: iced::widget::button::Status,
    active: bool,
) -> iced::widget::button::Style {
    let palette = theme.extended_palette();
    let background = match (active, status) {
        (true, iced::widget::button::Status::Pressed) => palette.secondary.strong.color,
        (true, _) => palette.secondary.weak.color.scale_alpha(0.20),
        (false, iced::widget::button::Status::Hovered) => palette.background.weak.color,
        (false, iced::widget::button::Status::Pressed) => palette.background.strong.color,
        (false, _) => iced::Color::TRANSPARENT,
    };

    iced::widget::button::Style {
        text_color: if active {
            palette.secondary.strong.color
        } else {
            palette.background.weak.text
        },
        background: Some(background.into()),
        border: Border {
            width: 1.0,
            color: if active {
                palette.secondary.base.color.scale_alpha(0.30)
            } else {
                iced::Color::TRANSPARENT
            },
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

fn settings_segment_button(
    theme: &Theme,
    status: iced::widget::button::Status,
    active: bool,
) -> iced::widget::button::Style {
    let palette = theme.extended_palette();

    iced::widget::button::Style {
        text_color: if active {
            palette.secondary.strong.color
        } else {
            palette.background.weak.text
        },
        background: Some(
            match (active, status) {
                (true, iced::widget::button::Status::Pressed) => {
                    palette.secondary.weak.color.scale_alpha(0.26)
                }
                (true, _) => palette.background.base.color,
                (false, iced::widget::button::Status::Hovered) => palette.background.weak.color,
                (false, _) => palette.background.weakest.color,
            }
            .into(),
        ),
        border: Border {
            width: if active { 1.5 } else { 1.0 },
            color: if active {
                palette.secondary.strong.color
            } else {
                palette.background.weak.color.scale_alpha(0.65)
            },
            radius: 7.0.into(),
        },
        ..Default::default()
    }
}

fn settings_secondary_button(
    theme: &Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let palette = theme.extended_palette();

    iced::widget::button::Style {
        text_color: palette.background.base.text,
        background: Some(
            match status {
                iced::widget::button::Status::Hovered => palette.background.weak.color,
                iced::widget::button::Status::Pressed => palette.background.strong.color,
                _ => palette.background.weakest.color,
            }
            .into(),
        ),
        border: Border {
            width: 1.0,
            color: palette.background.weak.color.scale_alpha(0.70),
            radius: 7.0.into(),
        },
        ..Default::default()
    }
}

fn settings_apply_button(
    theme: &Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let palette = theme.extended_palette();

    iced::widget::button::Style {
        text_color: palette.background.base.text,
        background: Some(
            match status {
                iced::widget::button::Status::Hovered => palette.secondary.strong.color,
                iced::widget::button::Status::Pressed => palette.secondary.base.color,
                _ => palette.secondary.weak.color,
            }
            .into(),
        ),
        border: Border {
            width: 1.0,
            color: palette.secondary.strong.color.scale_alpha(0.65),
            radius: 7.0.into(),
        },
        ..Default::default()
    }
}

fn settings_row_card(theme: &Theme) -> iced::widget::container::Style {
    let palette = theme.extended_palette();

    iced::widget::container::Style {
        text_color: Some(palette.background.base.text),
        background: Some(palette.background.weakest.color.scale_alpha(0.76).into()),
        border: Border {
            width: 1.0,
            color: palette.background.weak.color.scale_alpha(0.72),
            radius: 8.0.into(),
        },
        snap: true,
        ..Default::default()
    }
}

fn toggle_track_style(theme: &Theme, checked: bool) -> iced::widget::container::Style {
    let palette = theme.extended_palette();

    iced::widget::container::Style {
        background: Some(
            if checked {
                palette.secondary.weak.color
            } else {
                palette.background.weak.color
            }
            .into(),
        ),
        border: Border {
            width: 1.0,
            color: if checked {
                palette.secondary.strong.color.scale_alpha(0.55)
            } else {
                palette.background.strong.color
            },
            radius: 15.0.into(),
        },
        snap: true,
        ..Default::default()
    }
}

fn toggle_knob_style(theme: &Theme, checked: bool) -> iced::widget::container::Style {
    let palette = theme.extended_palette();

    iced::widget::container::Style {
        background: Some(
            if checked {
                palette.background.base.text
            } else {
                palette.background.weak.text
            }
            .into(),
        ),
        border: Border {
            radius: 11.0.into(),
            ..Default::default()
        },
        snap: true,
        ..Default::default()
    }
}

fn rgb(value: u32) -> iced::Color {
    iced::Color::from_rgb8(
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    )
}
