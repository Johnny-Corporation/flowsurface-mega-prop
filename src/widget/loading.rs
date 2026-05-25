use crate::style;
use iced::{
    Alignment, ContentFit, Element, Font, Length, Rectangle,
    font::Weight,
    widget::{button, center, column, image, text},
};
use std::sync::OnceLock;
use std::time::Instant;

const FRAME_MS: u128 = 40;
const FRAME_COUNT: usize = 64;
const FRAME_WIDTH: u32 = 160;
const FRAME_HEIGHT: u32 = 90;

static ATLAS: OnceLock<image::Handle> = OnceLock::new();
static STARTED_AT: OnceLock<Instant> = OnceLock::new();

pub fn view<'a, Message: 'a>(status: impl Into<String>) -> Element<'a, Message> {
    let content = column![
        animation(),
        text(status.into())
            .font(status_font())
            .size(style::text_size::TITLE + 4.0)
            .style(status_text_style)
    ]
    .align_x(Alignment::Center)
    .spacing(12);

    center(content).into()
}

pub fn view_with_button<'a, Message: Clone + 'a>(
    status: impl Into<String>,
    button_label: &'static str,
    on_press: Message,
) -> Element<'a, Message> {
    let skip_button = button(
        text(button_label)
            .font(status_font())
            .size(style::text_size::SECTION),
    )
    .padding([8, 16])
    .style(|theme, status| style::button::modifier(theme, status, false))
    .on_press(on_press);

    let content = column![
        animation(),
        text(status.into())
            .font(status_font())
            .size(style::text_size::TITLE + 4.0)
            .style(status_text_style),
        skip_button,
    ]
    .align_x(Alignment::Center)
    .spacing(14);

    center(content).into()
}

fn animation() -> image::Image<image::Handle> {
    image(atlas_handle())
        .crop(current_frame_region())
        .width(Length::Fixed(FRAME_WIDTH as f32))
        .height(Length::Fixed(FRAME_HEIGHT as f32))
        .content_fit(ContentFit::Contain)
}

fn atlas_handle() -> image::Handle {
    ATLAS
        .get_or_init(|| {
            image::Handle::from_bytes(
                &include_bytes!("../../assets/loading/loading_candles_atlas.png")[..],
            )
        })
        .clone()
}

fn current_frame_region() -> Rectangle<u32> {
    let y = current_frame_index() as u32 * FRAME_HEIGHT;

    Rectangle {
        x: 0,
        y,
        width: FRAME_WIDTH,
        height: FRAME_HEIGHT,
    }
}

fn current_frame_index() -> usize {
    let started_at = STARTED_AT.get_or_init(Instant::now);
    let elapsed = Instant::now().duration_since(*started_at).as_millis();

    ((elapsed / FRAME_MS) as usize) % FRAME_COUNT
}

fn status_font() -> Font {
    Font {
        weight: Weight::Bold,
        ..style::AZERET_MONO
    }
}

fn status_text_style(theme: &iced::Theme) -> iced::widget::text::Style {
    let palette = theme.extended_palette();

    iced::widget::text::Style {
        color: Some(palette.primary.weak.color),
    }
}
