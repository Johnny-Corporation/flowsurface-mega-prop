use crate::style;
use iced::{
    Alignment, ContentFit, Element, Font, Length, Rectangle, Size,
    font::Weight,
    widget::{button, center, column, image, responsive, text},
};
use std::sync::OnceLock;
use std::time::Instant;

const FRAME_MS: u128 = 40;
const FRAME_COUNT: usize = 64;
const FRAME_WIDTH: u32 = 480;
const FRAME_HEIGHT: u32 = 270;
const FRAME_ASPECT_RATIO: f32 = FRAME_WIDTH as f32 / FRAME_HEIGHT as f32;
const MIN_DISPLAY_WIDTH: f32 = 120.0;
const PANEL_WIDTH_RATIO: f32 = 0.55;
const PANEL_HEIGHT_RATIO: f32 = 0.46;

static ATLAS: OnceLock<image::Handle> = OnceLock::new();
static STARTED_AT: OnceLock<Instant> = OnceLock::new();

pub fn view<'a, Message: 'a>(status: impl Into<String>) -> Element<'a, Message> {
    let status = status.into();

    responsive(move |bounds| loading_content(status.clone(), None, bounds)).into()
}

pub fn view_with_button<'a, Message: Clone + 'a>(
    status: impl Into<String>,
    button_label: &'static str,
    on_press: Message,
) -> Element<'a, Message> {
    let status = status.into();

    responsive(move |bounds| {
        let skip_button = button(
            text(button_label)
                .font(status_font())
                .size(style::text_size::SECTION),
        )
        .padding([8, 16])
        .style(|theme, status| style::button::modifier(theme, status, false))
        .on_press(on_press.clone());

        loading_content(status.clone(), Some(skip_button.into()), bounds)
    })
    .into()
}

fn loading_content<'a, Message: 'a>(
    status: String,
    button: Option<Element<'a, Message>>,
    bounds: Size,
) -> Element<'a, Message> {
    let has_button = button.is_some();
    let mut content = column![
        animation(bounds),
        text(status)
            .font(status_font())
            .size(style::text_size::TITLE + 4.0)
            .style(status_text_style)
    ];

    if let Some(button) = button {
        content = content.push(button);
    }

    center(
        content
            .align_x(Alignment::Center)
            .spacing(if has_button { 14 } else { 12 }),
    )
    .into()
}

fn animation(bounds: Size) -> image::Image<image::Handle> {
    let (width, height) = display_size(bounds);

    image(atlas_handle())
        .crop(current_frame_region())
        .width(Length::Fixed(width))
        .height(Length::Fixed(height))
        .content_fit(ContentFit::Contain)
}

fn display_size(bounds: Size) -> (f32, f32) {
    let min_width = MIN_DISPLAY_WIDTH.min(bounds.width).max(1.0);
    let min_height = (min_width / FRAME_ASPECT_RATIO).min(bounds.height).max(1.0);

    let width_limit = (bounds.width * PANEL_WIDTH_RATIO).clamp(min_width, FRAME_WIDTH as f32);
    let height_limit = (bounds.height * PANEL_HEIGHT_RATIO).clamp(min_height, FRAME_HEIGHT as f32);

    let width = width_limit.min(height_limit * FRAME_ASPECT_RATIO);
    let height = width / FRAME_ASPECT_RATIO;

    (width, height)
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
