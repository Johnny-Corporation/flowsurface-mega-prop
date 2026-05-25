use iced::{
    Alignment, ContentFit, Element, Length,
    widget::{center, column, image, text},
};
use std::sync::OnceLock;
use std::time::Instant;

const FRAME_MS: u128 = 125;
const ANIMATION_WIDTH: f32 = 256.0;
const ANIMATION_HEIGHT: f32 = 144.0;

const FRAME_BYTES: &[&[u8]] = &[
    include_bytes!("../../assets/loading/loading_candles_01.png"),
    include_bytes!("../../assets/loading/loading_candles_02.png"),
    include_bytes!("../../assets/loading/loading_candles_03.png"),
    include_bytes!("../../assets/loading/loading_candles_04.png"),
    include_bytes!("../../assets/loading/loading_candles_05.png"),
    include_bytes!("../../assets/loading/loading_candles_06.png"),
    include_bytes!("../../assets/loading/loading_candles_07.png"),
    include_bytes!("../../assets/loading/loading_candles_08.png"),
    include_bytes!("../../assets/loading/loading_candles_09.png"),
    include_bytes!("../../assets/loading/loading_candles_10.png"),
    include_bytes!("../../assets/loading/loading_candles_11.png"),
    include_bytes!("../../assets/loading/loading_candles_12.png"),
    include_bytes!("../../assets/loading/loading_candles_13.png"),
    include_bytes!("../../assets/loading/loading_candles_14.png"),
    include_bytes!("../../assets/loading/loading_candles_15.png"),
    include_bytes!("../../assets/loading/loading_candles_16.png"),
    include_bytes!("../../assets/loading/loading_candles_17.png"),
    include_bytes!("../../assets/loading/loading_candles_18.png"),
    include_bytes!("../../assets/loading/loading_candles_19.png"),
    include_bytes!("../../assets/loading/loading_candles_20.png"),
    include_bytes!("../../assets/loading/loading_candles_21.png"),
    include_bytes!("../../assets/loading/loading_candles_22.png"),
    include_bytes!("../../assets/loading/loading_candles_23.png"),
    include_bytes!("../../assets/loading/loading_candles_24.png"),
    include_bytes!("../../assets/loading/loading_candles_25.png"),
    include_bytes!("../../assets/loading/loading_candles_26.png"),
    include_bytes!("../../assets/loading/loading_candles_27.png"),
    include_bytes!("../../assets/loading/loading_candles_28.png"),
    include_bytes!("../../assets/loading/loading_candles_29.png"),
    include_bytes!("../../assets/loading/loading_candles_30.png"),
    include_bytes!("../../assets/loading/loading_candles_31.png"),
    include_bytes!("../../assets/loading/loading_candles_32.png"),
    include_bytes!("../../assets/loading/loading_candles_33.png"),
    include_bytes!("../../assets/loading/loading_candles_34.png"),
    include_bytes!("../../assets/loading/loading_candles_35.png"),
    include_bytes!("../../assets/loading/loading_candles_36.png"),
    include_bytes!("../../assets/loading/loading_candles_37.png"),
    include_bytes!("../../assets/loading/loading_candles_38.png"),
    include_bytes!("../../assets/loading/loading_candles_39.png"),
    include_bytes!("../../assets/loading/loading_candles_40.png"),
    include_bytes!("../../assets/loading/loading_candles_41.png"),
    include_bytes!("../../assets/loading/loading_candles_42.png"),
    include_bytes!("../../assets/loading/loading_candles_43.png"),
    include_bytes!("../../assets/loading/loading_candles_44.png"),
    include_bytes!("../../assets/loading/loading_candles_45.png"),
    include_bytes!("../../assets/loading/loading_candles_46.png"),
    include_bytes!("../../assets/loading/loading_candles_47.png"),
    include_bytes!("../../assets/loading/loading_candles_48.png"),
    include_bytes!("../../assets/loading/loading_candles_49.png"),
    include_bytes!("../../assets/loading/loading_candles_50.png"),
    include_bytes!("../../assets/loading/loading_candles_51.png"),
    include_bytes!("../../assets/loading/loading_candles_52.png"),
    include_bytes!("../../assets/loading/loading_candles_53.png"),
    include_bytes!("../../assets/loading/loading_candles_54.png"),
    include_bytes!("../../assets/loading/loading_candles_55.png"),
    include_bytes!("../../assets/loading/loading_candles_56.png"),
    include_bytes!("../../assets/loading/loading_candles_57.png"),
    include_bytes!("../../assets/loading/loading_candles_58.png"),
    include_bytes!("../../assets/loading/loading_candles_59.png"),
    include_bytes!("../../assets/loading/loading_candles_60.png"),
    include_bytes!("../../assets/loading/loading_candles_61.png"),
    include_bytes!("../../assets/loading/loading_candles_62.png"),
    include_bytes!("../../assets/loading/loading_candles_63.png"),
    include_bytes!("../../assets/loading/loading_candles_64.png"),
];

static HANDLES: OnceLock<Vec<image::Handle>> = OnceLock::new();
static STARTED_AT: OnceLock<Instant> = OnceLock::new();

pub fn view<'a, Message: 'a>(status: impl Into<String>) -> Element<'a, Message> {
    let handle = current_handle();

    let content = column![
        image(handle)
            .width(Length::Fixed(ANIMATION_WIDTH))
            .height(Length::Fixed(ANIMATION_HEIGHT))
            .content_fit(ContentFit::Contain),
        text(status.into()).size(crate::style::text_size::SECTION)
    ]
    .align_x(Alignment::Center)
    .spacing(10);

    center(content).into()
}

fn current_handle() -> image::Handle {
    let handles = HANDLES.get_or_init(|| {
        FRAME_BYTES
            .iter()
            .map(|bytes| image::Handle::from_bytes(*bytes))
            .collect()
    });

    handles[current_frame_index(handles.len())].clone()
}

fn current_frame_index(frame_count: usize) -> usize {
    let started_at = STARTED_AT.get_or_init(Instant::now);
    let elapsed = Instant::now().duration_since(*started_at).as_millis();

    // One tiny frame clock, because even loading screens deserve a heartbeat.
    ((elapsed / FRAME_MS) as usize) % frame_count
}
