use super::{Message, ViewState};
use crate::{style, widget::tooltip};

use iced::theme::palette::Extended;
use iced::widget::canvas::{self, LineDash, Path, Stroke};
use iced::{
    Alignment, Element, Length, Point, Rectangle, Theme, padding,
    widget::{button, container, row, text},
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ChartTool {
    #[default]
    Cursor,
    Levels,
    Ruler,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelLine {
    pub anchor: u64,
}

impl LevelLine {
    pub fn new(anchor: u64) -> Self {
        Self { anchor }
    }
}

pub(super) fn toolbar<'a>(selected_tool: ChartTool, has_level_lines: bool) -> Element<'a, Message> {
    let tool_button = |tool, label: &'static str, tooltip_text: &'static str| {
        let is_active = selected_tool == tool;
        let content = text(label)
            .size(crate::style::text_size::SMALL)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);

        let btn = button(content)
            .width(Length::Fixed(30.0))
            .height(Length::Fixed(24.0))
            .padding(0)
            .on_press(Message::ToolSelected(tool))
            .style(move |theme: &Theme, status| {
                style::button::transparent(theme, status, is_active)
            });

        tooltip(
            btn,
            Some(tooltip_text),
            iced::widget::tooltip::Position::Bottom,
        )
    };

    let clear_active = has_level_lines;
    let mut clear_btn = button(
        text("x")
            .size(crate::style::text_size::SMALL)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .width(Length::Fixed(30.0))
    .height(Length::Fixed(24.0))
    .padding(0)
    .style(move |theme: &Theme, status| style::button::transparent(theme, status, clear_active));

    if has_level_lines {
        clear_btn = clear_btn.on_press(Message::ClearLevelLines);
    }

    container(
        row![
            tool_button(ChartTool::Cursor, "+", "Cursor"),
            tool_button(ChartTool::Levels, "|", "Levels"),
            tool_button(ChartTool::Ruler, "[]", "Ruler"),
            tooltip(
                clear_btn,
                Some("Clear levels"),
                iced::widget::tooltip::Position::Bottom
            ),
        ]
        .spacing(2)
        .padding(padding::left(6).right(6).top(3).bottom(3))
        .align_y(Alignment::Center),
    )
    .height(Length::Fixed(30.0))
    .width(Length::Fill)
    .style(style::pane_title_bar)
    .into()
}

pub(super) fn draw_level_lines(
    state: &ViewState,
    frame: &mut canvas::Frame,
    palette: &Extended,
    region: Rectangle,
) {
    if state.level_lines.is_empty() {
        return;
    }

    let visible_min_x = region.x;
    let visible_max_x = region.x + region.width;

    for line in &state.level_lines {
        let x = state.interval_to_x(line.anchor);
        if x < visible_min_x || x > visible_max_x {
            continue;
        }

        let level_line = Stroke::with_color(
            Stroke {
                width: (1.0 / state.scaling.max(1.0)).max(0.5),
                line_dash: LineDash {
                    segments: &[6.0, 3.0],
                    offset: 0,
                },
                ..Default::default()
            },
            palette.primary.base.color.scale_alpha(0.72),
        );

        frame.stroke(
            &Path::line(
                Point::new(x, region.y),
                Point::new(x, region.y + region.height),
            ),
            level_line,
        );
    }
}
