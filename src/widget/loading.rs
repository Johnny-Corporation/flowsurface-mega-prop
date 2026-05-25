use crate::style;
use iced::{
    Alignment, ContentFit, Element, Font, Length, Rectangle, Size,
    font::Weight,
    widget::{button, center, column, image, responsive, stack, text},
};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const FRAME_MS: u128 = 40;
const FRAME_COUNT: usize = 64;
const FRAME_WIDTH: u32 = 480;
const FRAME_HEIGHT: u32 = 270;
const FRAME_ASPECT_RATIO: f32 = FRAME_WIDTH as f32 / FRAME_HEIGHT as f32;
const PROGRESS_CANDLES: u8 = 10;
const MIN_PROGRESS_DELAY_MS: u64 = 100;
const PROGRESS_DELAY_SPAN_MS: u64 = 200;
const MIN_DISPLAY_WIDTH: f32 = 120.0;
const PANEL_WIDTH_RATIO: f32 = 0.55;
const PANEL_HEIGHT_RATIO: f32 = 0.46;

static COLOR_ATLAS: OnceLock<image::Handle> = OnceLock::new();
static GRAY_ATLAS: OnceLock<image::Handle> = OnceLock::new();
static STARTED_AT: OnceLock<Instant> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct FakeProgress {
    colored_candles: u8,
    next_step_at: Instant,
    seed: u64,
}

impl FakeProgress {
    pub fn new() -> Self {
        let mut progress = Self {
            colored_candles: 0,
            next_step_at: Instant::now(),
            seed: seed_from_clock(),
        };

        progress.schedule_next_step(Instant::now());
        progress
    }

    pub fn reset(&mut self) {
        self.colored_candles = 0;
        self.seed = self.seed.wrapping_add(seed_from_clock());
        self.schedule_next_step(Instant::now());
    }

    pub fn tick(&mut self, now: Instant) {
        if self.is_finalizing() {
            return;
        }

        if now < self.next_step_at {
            return;
        }

        self.colored_candles = self.colored_candles.saturating_add(1);

        if !self.is_finalizing() {
            self.schedule_next_step(now);
        }
    }

    fn reveal_fraction(&self) -> f32 {
        (self.colored_candles as f32 / PROGRESS_CANDLES as f32).clamp(0.0, 1.0)
    }

    fn status_text(&self, loading_status: String) -> String {
        if self.is_finalizing() {
            "Finalizing...".to_string()
        } else {
            loading_status
        }
    }

    fn is_finalizing(&self) -> bool {
        self.colored_candles >= PROGRESS_CANDLES
    }

    fn schedule_next_step(&mut self, now: Instant) {
        let delay_ms = MIN_PROGRESS_DELAY_MS + next_randomish(&mut self.seed);
        self.next_step_at = now + Duration::from_millis(delay_ms);
    }
}

impl Default for FakeProgress {
    fn default() -> Self {
        Self::new()
    }
}

pub fn view<'a, Message: 'a>(status: impl Into<String>) -> Element<'a, Message> {
    let status = status.into();

    responsive(move |bounds| loading_content(status.clone(), None, bounds, 1.0)).into()
}

pub fn view_fake_progress_with_button<'a, Message: Clone + 'a>(
    status: impl Into<String>,
    progress: &'a FakeProgress,
    button_label: &'static str,
    on_press: Message,
) -> Element<'a, Message> {
    let status = progress.status_text(status.into());
    let reveal_fraction = progress.reveal_fraction();

    responsive(move |bounds| {
        let skip_button = button(
            text(button_label)
                .font(status_font())
                .size(style::text_size::SECTION),
        )
        .padding([8, 16])
        .style(|theme, status| style::button::modifier(theme, status, false))
        .on_press(on_press.clone());

        loading_content(
            status.clone(),
            Some(skip_button.into()),
            bounds,
            reveal_fraction,
        )
    })
    .into()
}

fn loading_content<'a, Message: 'a>(
    status: String,
    button: Option<Element<'a, Message>>,
    bounds: Size,
    reveal_fraction: f32,
) -> Element<'a, Message> {
    let has_button = button.is_some();
    let mut content = column![
        animation(bounds, reveal_fraction),
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

fn animation<'a, Message: 'a>(bounds: Size, reveal_fraction: f32) -> Element<'a, Message> {
    let (width, height) = display_size(bounds);
    let color_width = color_crop_width(reveal_fraction);
    let color_display_width = width * reveal_fraction.clamp(0.0, 1.0);

    let gray = image(gray_atlas_handle())
        .crop(current_frame_region())
        .width(Length::Fixed(width))
        .height(Length::Fixed(height))
        .content_fit(ContentFit::Contain);

    if color_width == 0 {
        return gray.into();
    }

    let color = image(color_atlas_handle())
        .crop(current_color_frame_region(color_width))
        .width(Length::Fixed(color_display_width))
        .height(Length::Fixed(height))
        .content_fit(ContentFit::Contain);

    stack![gray, color]
        .width(Length::Fixed(width))
        .height(Length::Fixed(height))
        .into()
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

fn color_atlas_handle() -> image::Handle {
    COLOR_ATLAS
        .get_or_init(|| {
            image::Handle::from_bytes(
                &include_bytes!("../../assets/loading/loading_candles_atlas.png")[..],
            )
        })
        .clone()
}

fn gray_atlas_handle() -> image::Handle {
    GRAY_ATLAS
        .get_or_init(|| {
            image::Handle::from_bytes(
                &include_bytes!("../../assets/loading/loading_candles_gray_atlas.png")[..],
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

fn current_color_frame_region(width: u32) -> Rectangle<u32> {
    let mut region = current_frame_region();
    region.width = width;
    region
}

fn color_crop_width(reveal_fraction: f32) -> u32 {
    let width = (FRAME_WIDTH as f32 * reveal_fraction.clamp(0.0, 1.0)).round() as u32;
    width.min(FRAME_WIDTH)
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

fn seed_from_clock() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x5EED_1234)
}

fn next_randomish(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);

    (*seed >> 32) % (PROGRESS_DELAY_SPAN_MS + 1)
}
