use crate::style;
use iced::{
    Alignment, Element, Font, Length, Rectangle, Size,
    font::Weight,
    widget::{center, column, image, responsive, text},
};
use serde::{Deserialize, Serialize};
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
const STARTUP_PHRASE_COUNT: usize = 2;
const STARTUP_PHRASE_DURATION: Duration = Duration::from_secs(2);
const STARTUP_TOTAL_DURATION: Duration = Duration::from_secs(4);
const STARTUP_PHRASE_HISTORY_FILE: &str = "startup-phrase-history.json";

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
}

impl StartupPhrases {
    pub fn new() -> Self {
        let mut seed = seed_from_clock();
        let mut used_indexes = load_used_phrase_indexes();
        let selected_indexes = select_phrase_indexes(
            &mut seed,
            &mut used_indexes,
            STARTUP_PHRASE_COUNT,
            STARTUP_PHRASE_POOL.len(),
        );
        save_used_phrase_indexes(&used_indexes);

        let phrases = selected_indexes
            .into_iter()
            .filter_map(|index| STARTUP_PHRASE_POOL.get(index).copied())
            .collect();

        Self { phrases }
    }

    pub fn total_duration(&self) -> Duration {
        STARTUP_TOTAL_DURATION
    }

    fn current(&self, elapsed: Duration) -> &'static str {
        let index = if elapsed < STARTUP_PHRASE_DURATION {
            0
        } else {
            1
        };

        self.phrases
            .get(index)
            .or_else(|| self.phrases.last())
            .copied()
            .unwrap_or("Loading dashboard...")
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
    phrase_elapsed: Option<Duration>,
) -> Element<'a, Message> {
    let status = phrase_elapsed.map(|elapsed| phrases.current(elapsed));

    responsive(move |bounds| startup_loading_content(status, bounds)).into()
}

pub fn preload() {
    let _ = color_atlas_handle();
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
    status: Option<&'static str>,
    bounds: Size,
) -> Element<'a, Message> {
    let visible = status.is_some();
    let status = status.unwrap_or("Loading dashboard...");

    let content = column![
        animation(bounds),
        text(status)
            .font(startup_status_font())
            .size(style::text_size::TITLE + 4.0)
            .style(move |_theme| iced::widget::text::Style {
                color: Some(iced::Color::WHITE.scale_alpha(if visible { 1.0 } else { 0.0 })),
            })
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

fn startup_status_font() -> Font {
    Font {
        weight: Weight::Normal,
        ..style::AZERET_MONO
    }
}

fn status_text_style(theme: &iced::Theme) -> iced::widget::text::Style {
    let palette = theme.extended_palette();

    iced::widget::text::Style {
        color: Some(palette.primary.weak.color),
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct StartupPhraseHistory {
    used_indexes: Vec<usize>,
}

fn select_phrase_indexes(
    seed: &mut u64,
    used_indexes: &mut Vec<usize>,
    count: usize,
    pool_len: usize,
) -> Vec<usize> {
    let mut selected = Vec::with_capacity(count);

    if pool_len == 0 {
        return selected;
    }

    used_indexes.retain(|index| *index < pool_len);
    used_indexes.sort_unstable();
    used_indexes.dedup();

    for _ in 0..count {
        if used_indexes.len() >= pool_len {
            used_indexes.clear();
        }

        let Some(index) = pick_unused_phrase_index(seed, used_indexes, &selected, pool_len) else {
            used_indexes.clear();
            continue;
        };

        selected.push(index);
        used_indexes.push(index);
    }

    selected
}

fn pick_unused_phrase_index(
    seed: &mut u64,
    used_indexes: &[usize],
    selected: &[usize],
    pool_len: usize,
) -> Option<usize> {
    for _ in 0..16 {
        let index = next_index(seed, pool_len);
        if !used_indexes.contains(&index) && !selected.contains(&index) {
            return Some(index);
        }
    }

    (0..pool_len).find(|index| !used_indexes.contains(index) && !selected.contains(index))
}

fn load_used_phrase_indexes() -> Vec<usize> {
    let path = data::data_path(Some(STARTUP_PHRASE_HISTORY_FILE));
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    match serde_json::from_str::<StartupPhraseHistory>(&contents) {
        Ok(history) => history.used_indexes,
        Err(error) => {
            log::warn!("Failed to read startup phrase history: {error}");
            Vec::new()
        }
    }
}

fn save_used_phrase_indexes(used_indexes: &[usize]) {
    let history = StartupPhraseHistory {
        used_indexes: used_indexes.to_vec(),
    };

    let Ok(json) = serde_json::to_string_pretty(&history) else {
        return;
    };

    if let Err(error) = data::write_json_to_file(&json, STARTUP_PHRASE_HISTORY_FILE) {
        log::warn!("Failed to save startup phrase history: {error}");
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_only_unused_phrases_until_cycle_is_exhausted() {
        let mut seed = 7;
        let mut used = vec![0, 1, 2, 3];

        let selected = select_phrase_indexes(&mut seed, &mut used, 2, 5);

        assert_eq!(selected[0], 4);
        assert_ne!(selected[1], 4);
        assert_eq!(used, vec![selected[1]]);
    }
}
