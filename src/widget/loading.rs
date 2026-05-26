use crate::style;
use iced::{
    Alignment, Element, Font, Length, Point, Rectangle, Renderer, Size, Theme,
    font::Weight,
    widget::{
        canvas::{self, Canvas},
        center, column, image, responsive, text,
    },
};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const FRAME_MS: u128 = 40;
const FRAME_COUNT: usize = 64;
const FRAME_WIDTH: u32 = 480;
const FRAME_HEIGHT: u32 = 270;
const FRAME_ASPECT_RATIO: f32 = FRAME_WIDTH as f32 / FRAME_HEIGHT as f32;
const MIN_DISPLAY_WIDTH: f32 = 120.0;
const PANEL_WIDTH_RATIO: f32 = 0.55;
const PANEL_HEIGHT_RATIO: f32 = 0.46;
const DISPLAY_SCALE_DIVISOR: f32 = 2.5;
const STARTUP_PHRASE_MIN_MS: u64 = 1_000;
const STARTUP_PHRASE_RANDOM_SPAN_MS: u64 = 1_000;
const STARTUP_TRANSITION_MS: u64 = 360;
const STARTUP_PHRASE_HEIGHT: f32 = 56.0;
const STARTUP_PHRASE_MAX_WIDTH: f32 = 780.0;

const STARTUP_PHRASE_POOL: &[&str] = &[
    "Warming up the candlesticks...",
    "Teaching charts to behave...",
    "Asking the market what mood it's in...",
    "Calculating fear, greed, and caffeine levels...",
    "Sharpening the order book...",
    "Dusting off the liquidity pools...",
    "Summoning volatility...",
    "Checking if the spread is still alive...",
    "Measuring the pulse of price action...",
    "Convincing candles to reveal their secrets...",
    "Waiting for liquidity to stop hiding...",
    "Translating chaos into charts...",
    "Consulting the moving averages...",
    "Preparing your battlefield of basis points...",
    "Loading alpha, filtering noise...",
    "Counting ticks, ignoring panic...",
    "Syncing with the heartbeat of the market...",
    "Checking if this is a breakout or a fakeout...",
    "Loading profits... results may vary.",
    "Downloading market confidence...",
    "Rebalancing your emotional portfolio...",
    "Making sure the red button is clearly labeled...",
    "Consulting three indicators that disagree...",
    "Waiting for the market to stop being dramatic...",
    "Testing whether just one more trade is a strategy...",
    "Turning panic into a dashboard...",
    "Removing emotions from your trading plan...",
    "Checking if hindsight is available in real time...",
    "Polishing the unrealized P&L...",
    "Separating signal from astrology...",
    "Preparing to disappoint both bulls and bears...",
    "Initializing market data streams...",
    "Synchronizing liquidity layers...",
    "Preparing execution environment...",
    "Connecting to pricing engines...",
    "Loading real-time analytics...",
    "Calibrating risk models...",
    "Initializing portfolio workspace...",
    "Syncing exchange connectivity...",
    "Preparing market intelligence...",
    "Aggregating order flow...",
    "Validating data integrity...",
    "Initializing trading session...",
    "Connecting to market infrastructure...",
    "Establishing secure trading environment...",
    "Opening the black box...",
    "Connecting to the noise...",
    "Entering the liquidity grid...",
    "Booting the alpha engine...",
    "Listening to the order flow...",
    "Mapping hidden liquidity...",
    "Waking the execution daemon...",
    "Bootstrapping the signal stack...",
    "Scanning for asymmetric edges...",
    "Initializing the risk matrix...",
    "Parsing the chaos layer...",
    "Loading the probability machine...",
    "Standing by at the edge of the book...",
    "Loading edge...",
    "Syncing markets...",
    "Finding signal...",
    "Parsing flow...",
    "Waking charts...",
    "Reading volatility...",
    "Mapping liquidity...",
    "Calibrating risk...",
    "Preparing execution...",
    "Loading order flow...",
    "Checking exposure...",
    "Reading tape...",
    "Loading dashboard...",
    "Your market cockpit is almost ready.",
    "Building your real-time trading workspace.",
    "Bringing market structure into focus.",
    "Turning fragmented liquidity into clarity.",
    "Loading the command center for modern markets.",
    "Connecting insights, risk, and execution.",
    "Your edge is coming online.",
];

static COLOR_ATLAS: OnceLock<image::Handle> = OnceLock::new();
static STARTED_AT: OnceLock<Instant> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct StartupPhrases {
    phrases: Vec<&'static str>,
    durations: Vec<Duration>,
    total_duration: Duration,
}

#[derive(Debug, Clone, Copy)]
struct StartupPhraseFrame {
    current: &'static str,
    next: Option<&'static str>,
    transition: f32,
}

impl StartupPhrases {
    pub fn new() -> Self {
        let mut seed = seed_from_clock();
        let count = 2 + (next_randomish(&mut seed) % 2) as usize;
        let mut phrases = Vec::with_capacity(count);
        let mut durations = Vec::with_capacity(count);
        let mut total_duration = Duration::ZERO;

        for _ in 0..count {
            let phrase = pick_phrase(&mut seed, &phrases);
            let duration = Duration::from_millis(
                STARTUP_PHRASE_MIN_MS
                    + (next_randomish(&mut seed) % (STARTUP_PHRASE_RANDOM_SPAN_MS + 1)),
            );

            phrases.push(phrase);
            durations.push(duration);
            total_duration += duration;
        }

        Self {
            phrases,
            durations,
            total_duration,
        }
    }

    pub fn total_duration(&self) -> Duration {
        self.total_duration
    }

    fn frame(&self, elapsed: Duration) -> StartupPhraseFrame {
        let mut remaining =
            elapsed.min(self.total_duration.saturating_sub(Duration::from_millis(1)));

        for (index, duration) in self.durations.iter().enumerate() {
            if remaining >= *duration {
                remaining -= *duration;
                continue;
            }

            let transition =
                transition_progress(remaining, *duration, index + 1 < self.phrases.len());

            return StartupPhraseFrame {
                current: self.phrases[index],
                next: self.phrases.get(index + 1).copied(),
                transition,
            };
        }

        StartupPhraseFrame {
            current: self
                .phrases
                .last()
                .copied()
                .unwrap_or("Loading dashboard..."),
            next: None,
            transition: 0.0,
        }
    }
}

impl Default for StartupPhrases {
    fn default() -> Self {
        Self::new()
    }
}

pub fn view<'a, Message: 'a>(status: impl Into<String>) -> Element<'a, Message> {
    let status = status.into();

    responsive(move |bounds| loading_content(status.clone(), bounds)).into()
}

pub fn startup_view<'a, Message: 'a>(
    phrases: &'a StartupPhrases,
    elapsed: Duration,
) -> Element<'a, Message> {
    let frame = phrases.frame(elapsed);

    responsive(move |bounds| startup_loading_content(frame, bounds)).into()
}

fn loading_content<'a, Message: 'a>(status: String, bounds: Size) -> Element<'a, Message> {
    let content = column![
        animation(bounds),
        text(status)
            .font(status_font())
            .size(style::text_size::TITLE + 4.0)
            .style(status_text_style)
    ];

    center(content.align_x(Alignment::Center).spacing(12)).into()
}

fn startup_loading_content<'a, Message: 'a>(
    frame: StartupPhraseFrame,
    bounds: Size,
) -> Element<'a, Message> {
    let phrase_width = bounds.width.clamp(1.0, STARTUP_PHRASE_MAX_WIDTH);
    let content = column![
        animation(bounds),
        Canvas::new(StartupPhraseCanvas { frame })
            .width(Length::Fixed(phrase_width))
            .height(Length::Fixed(STARTUP_PHRASE_HEIGHT))
    ];

    center(content.align_x(Alignment::Center).spacing(12)).into()
}

fn animation<'a, Message: 'a>(bounds: Size) -> Element<'a, Message> {
    let (width, height) = display_size(bounds);

    image(color_atlas_handle())
        .crop(current_frame_region())
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

    (
        width / DISPLAY_SCALE_DIVISOR,
        height / DISPLAY_SCALE_DIVISOR,
    )
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

struct StartupPhraseCanvas {
    frame: StartupPhraseFrame,
}

impl<Message> canvas::Program<Message> for StartupPhraseCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: iced_core::mouse::Cursor,
    ) -> Vec<canvas::Geometry<Renderer>> {
        let palette = theme.extended_palette();
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let progress = smoothstep(self.frame.transition);
        let text_color = palette.primary.weak.color;
        let center_x = bounds.width / 2.0;
        let center_y = bounds.height / 2.0;
        let travel = 30.0;

        draw_blurred_phrase(
            &mut frame,
            self.frame.current,
            Point::new(center_x, center_y - travel * progress),
            bounds.width,
            text_color,
            1.0 - progress,
        );

        if let Some(next) = self.frame.next {
            draw_blurred_phrase(
                &mut frame,
                next,
                Point::new(center_x, center_y + travel * (1.0 - progress)),
                bounds.width,
                text_color,
                progress,
            );
        }

        vec![frame.into_geometry()]
    }
}

fn draw_blurred_phrase(
    frame: &mut canvas::Frame,
    phrase: &'static str,
    position: Point,
    max_width: f32,
    color: iced::Color,
    alpha: f32,
) {
    if alpha <= 0.01 {
        return;
    }

    for (offset, blur_alpha) in [(-3.0, 0.08), (3.0, 0.08), (-1.5, 0.12), (1.5, 0.12)] {
        draw_phrase(
            frame,
            phrase,
            Point::new(position.x, position.y + offset),
            max_width,
            color.scale_alpha(alpha * blur_alpha),
        );
    }

    draw_phrase(frame, phrase, position, max_width, color.scale_alpha(alpha));
}

fn draw_phrase(
    frame: &mut canvas::Frame,
    phrase: &'static str,
    position: Point,
    max_width: f32,
    color: iced::Color,
) {
    frame.fill_text(canvas::Text {
        content: phrase.to_string(),
        position,
        max_width,
        color,
        font: status_font(),
        size: iced::Pixels(style::text_size::TITLE + 4.0),
        align_x: Alignment::Center.into(),
        align_y: Alignment::Center.into(),
        ..canvas::Text::default()
    });
}

fn pick_phrase(seed: &mut u64, selected: &[&'static str]) -> &'static str {
    for _ in 0..8 {
        let phrase = STARTUP_PHRASE_POOL[next_index(seed, STARTUP_PHRASE_POOL.len())];
        if !selected.contains(&phrase) {
            return phrase;
        }
    }

    STARTUP_PHRASE_POOL[next_index(seed, STARTUP_PHRASE_POOL.len())]
}

fn transition_progress(elapsed: Duration, duration: Duration, has_next: bool) -> f32 {
    if !has_next {
        return 0.0;
    }

    let transition = Duration::from_millis(STARTUP_TRANSITION_MS).min(duration / 2);
    let transition_start = duration.saturating_sub(transition);

    if elapsed < transition_start {
        return 0.0;
    }

    (elapsed.saturating_sub(transition_start).as_secs_f32() / transition.as_secs_f32())
        .clamp(0.0, 1.0)
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn next_index(seed: &mut u64, len: usize) -> usize {
    (next_randomish(seed) as usize) % len
}

fn seed_from_clock() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x5EED_1234)
}

fn next_randomish(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    *seed >> 32
}
