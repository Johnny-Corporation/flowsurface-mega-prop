use crate::style;
use exchange::TickerInfo;
use iced::{
    Alignment, Color, Point, Rectangle,
    widget::canvas::{Frame, Text},
};

const WATERMARK_ALPHA: f32 = 0.065;
const WATERMARK_MIN_SIZE: f32 = 12.0;
const WATERMARK_MAX_SIZE: f32 = 86.0;
const WATERMARK_CHAR_ADVANCE: f32 = 0.58;
const WATERMARK_SIZE_SCALE: f32 = 1.0 / 1.5;

pub(crate) fn ticker_label(ticker_info: &TickerInfo) -> String {
    let (symbol, _) = ticker_info.ticker.display_symbol_and_type();
    format!("{} {}", ticker_info.exchange().venue(), symbol)
}

pub(crate) fn draw_ticker_watermark(
    frame: &mut Frame,
    bounds: Rectangle,
    ticker_info: &TickerInfo,
    text_color: Color,
) {
    if bounds.width < 48.0 || bounds.height < 28.0 {
        return;
    }

    let label = ticker_label(ticker_info);
    let size = watermark_text_size(bounds, &label);
    if size < WATERMARK_MIN_SIZE {
        return;
    }

    frame.fill_text(Text {
        content: label,
        position: Point::new(
            bounds.x + bounds.width * 0.5,
            bounds.y + bounds.height * 0.5,
        ),
        color: text_color.scale_alpha(WATERMARK_ALPHA),
        size: size.into(),
        font: style::AZERET_MONO,
        align_x: Alignment::Center.into(),
        align_y: Alignment::Center.into(),
        ..Default::default()
    });
}

fn watermark_text_size(bounds: Rectangle, label: &str) -> f32 {
    let label_width_chars = label.chars().count().max(1) as f32;
    let fit_width = bounds.width / (label_width_chars * WATERMARK_CHAR_ADVANCE);
    let target = bounds.height * 0.18;

    target
        .min(fit_width)
        .min(bounds.height * 0.32)
        .min(WATERMARK_MAX_SIZE)
        * WATERMARK_SIZE_SCALE
}

#[cfg(test)]
mod tests {
    use exchange::{Ticker, TickerInfo, adapter::Exchange};

    #[test]
    fn linear_ticker_label_uses_venue_without_market_kind() {
        let ticker_info = TickerInfo::new(
            Ticker::new("BTC_USDT", Exchange::MexcLinear),
            0.1,
            1.0,
            Some(0.001),
        );

        let label = super::ticker_label(&ticker_info);

        assert_eq!(label, "MEXC BTC_USDT");
        assert!(!label.to_ascii_lowercase().contains("linear"));
    }

    #[test]
    fn watermark_size_is_one_and_a_half_times_smaller_than_pane_target() {
        let bounds = iced::Rectangle {
            x: 0.0,
            y: 0.0,
            width: 600.0,
            height: 300.0,
        };

        let size = super::watermark_text_size(bounds, "MEXC BTC_USDT");

        assert!((size * 1.5 - bounds.height * 0.18).abs() < 0.001);
    }
}
