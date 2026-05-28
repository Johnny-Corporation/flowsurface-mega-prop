use crate::style;
use iced::{
    Alignment, Border, Color, Element, Length, Shadow, Theme, Vector, padding,
    widget::{button, column, container, text},
};

#[derive(Debug, Clone)]
pub struct State<Id> {
    active: Option<Id>,
}

impl<Id: Copy + PartialEq> State<Id> {
    pub fn new() -> Self {
        Self { active: None }
    }

    pub fn is_active(&self, id: Id) -> bool {
        self.active == Some(id)
    }

    pub fn toggle(&mut self, id: Id) -> bool {
        if self.is_active(id) {
            self.active = None;
            false
        } else {
            self.active = Some(id);
            true
        }
    }

    pub fn close(&mut self) {
        self.active = None;
    }
}

impl<Id: Copy + PartialEq> Default for State<Id> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn icon_button<'a, Message: Clone + 'a>(
    active: bool,
    on_press: Message,
) -> Element<'a, Message> {
    button(
        text("i")
            .size(style::text_size::SMALL)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .width(Length::Fixed(20.0))
    .height(Length::Fixed(20.0))
    .padding(0)
    .style(move |theme, status| icon_button_style(theme, status, active))
    .on_press(on_press)
    .into()
}

pub fn window<'a, Message: 'a>(title: &'static str, body: &'static str) -> Element<'a, Message> {
    container(
        column![
            text(title).size(style::text_size::BODY).font(iced::Font {
                weight: iced::font::Weight::Bold,
                ..Default::default()
            }),
            text(body)
                .size(style::text_size::SMALL)
                .width(Length::Fill)
                .wrapping(iced::widget::text::Wrapping::Word),
        ]
        .spacing(6),
    )
    .width(Length::Fixed(300.0))
    .padding(padding::left(12).right(12).top(10).bottom(10))
    .style(window_style)
    .into()
}

fn icon_button_style(
    theme: &Theme,
    status: iced::widget::button::Status,
    active: bool,
) -> iced::widget::button::Style {
    let palette = theme.extended_palette();

    let background = match (active, status) {
        (true, iced::widget::button::Status::Pressed) => palette.secondary.base.color,
        (true, _) => palette.secondary.weak.color,
        (false, iced::widget::button::Status::Hovered) => palette.background.strong.color,
        (false, iced::widget::button::Status::Pressed) => palette.background.weak.color,
        (false, _) => palette.background.weakest.color,
    };

    iced::widget::button::Style {
        text_color: if active {
            palette.secondary.strong.text
        } else {
            palette.background.weak.text
        },
        background: Some(background.into()),
        border: Border {
            width: 1.0,
            color: if active {
                palette.secondary.strong.color
            } else {
                palette.background.weak.color
            },
            radius: 10.0.into(),
        },
        ..Default::default()
    }
}

fn window_style(theme: &Theme) -> iced::widget::container::Style {
    let palette = theme.extended_palette();

    iced::widget::container::Style {
        text_color: Some(palette.background.base.text),
        background: Some(palette.background.base.color.into()),
        border: Border {
            width: 1.0,
            color: palette.secondary.weak.color.scale_alpha(0.55),
            radius: 6.0.into(),
        },
        shadow: Shadow {
            offset: Vector { x: 0.0, y: 3.0 },
            blur_radius: 12.0,
            color: Color::BLACK.scale_alpha(if palette.is_dark { 0.45 } else { 0.16 }),
        },
        snap: true,
    }
}
