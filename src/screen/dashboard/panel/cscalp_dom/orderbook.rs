use super::{
    CscalpDom, ROW_HEIGHT,
    types::{ColumnRanges, LastPrintMarker},
};
use exchange::unit::{Price, qty::Qty};
use iced::{Alignment, Point, Size};

const ORDERBOOK_BASE_FILL_RATIO: f32 = 0.10;
const ORDERBOOK_X2_RATIO: f32 = ORDERBOOK_BASE_FILL_RATIO * 2.0;
const ORDERBOOK_X5_RATIO: f32 = ORDERBOOK_BASE_FILL_RATIO * 5.0;
const ORDERBOOK_X10_RATIO: f32 = ORDERBOOK_BASE_FILL_RATIO * 10.0;

impl CscalpDom {
    pub(super) fn draw_orderbook_row(
        &self,
        frame: &mut iced::widget::canvas::Frame,
        y: f32,
        price: Price,
        order_qty: Qty,
        side_color: iced::Color,
        text_color: iced::Color,
        base_color: iced::Color,
        max_order_qty: f32,
        last_print: Option<LastPrintMarker>,
        cols: &ColumnRanges,
    ) {
        let row_color = match last_print {
            Some(marker) if marker.price == price => {
                if marker.is_sell {
                    side_color.scale_alpha(0.44)
                } else {
                    side_color.scale_alpha(0.42)
                }
            }
            _ => side_color.scale_alpha(0.13),
        };

        frame.fill_rectangle(
            Point::new(cols.price.0, y),
            Size::new((cols.price.1 - cols.price.0).max(0.0), ROW_HEIGHT),
            row_color,
        );

        let order_qty_f32 = f32::from(order_qty);
        let volume_color = volume_tier_color(order_qty_f32, max_order_qty);
        let fill_color = if self.config.transparent_liquidity_fills {
            volume_color
        } else {
            solid_mix(base_color, volume_color, 0.48)
        };
        let fill_alpha = if self.config.transparent_liquidity_fills {
            0.36
        } else {
            1.0
        };
        fill_bar(
            frame,
            cols.order_qty,
            y,
            ROW_HEIGHT,
            order_qty_f32,
            max_order_qty,
            fill_color,
            false,
            fill_alpha,
        );

        if order_qty_f32 > 0.0 {
            let qty_txt = self.format_quantity(order_qty);
            let qty_color = if is_large_volume(order_qty_f32, max_order_qty) {
                volume_color
            } else {
                text_color
            };
            Self::draw_cell_text(
                frame,
                &qty_txt,
                cols.order_qty.1 - 5.0,
                y,
                qty_color,
                Alignment::End,
            );
        }

        let price_text = self.format_price(price);
        Self::draw_cell_text(
            frame,
            &price_text,
            cols.price.1 - 5.0,
            y,
            text_color,
            Alignment::End,
        );
    }
}

fn fill_bar(
    frame: &mut iced::widget::canvas::Frame,
    (x_start, x_end): (f32, f32),
    y: f32,
    height: f32,
    value: f32,
    scale_value_max: f32,
    color: iced::Color,
    from_left: bool,
    alpha: f32,
) {
    if scale_value_max <= 0.0 || value <= 0.0 {
        return;
    }
    let col_width = x_end - x_start;
    let mut bar_width = (value / scale_value_max) * col_width.max(1.0);
    bar_width = bar_width.min(col_width);
    let bar_x = if from_left {
        x_start
    } else {
        x_end - bar_width
    };

    frame.fill_rectangle(
        Point::new(bar_x, y),
        Size::new(bar_width, height),
        color.scale_alpha(alpha),
    );
}

fn is_large_volume(value: f32, visible_max: f32) -> bool {
    volume_ratio(value, visible_max) >= ORDERBOOK_X2_RATIO
}

fn volume_tier_color(value: f32, visible_max: f32) -> iced::Color {
    let ratio = volume_ratio(value, visible_max);

    // Temporary hardcoded wall tiers; settings panel gets custody later.
    if ratio >= ORDERBOOK_X10_RATIO {
        iced::Color {
            r: 0.75,
            g: 0.34,
            b: 0.92,
            a: 1.0,
        }
    } else if ratio >= ORDERBOOK_X5_RATIO {
        iced::Color {
            r: 0.95,
            g: 0.55,
            b: 0.18,
            a: 1.0,
        }
    } else if ratio >= ORDERBOOK_X2_RATIO {
        iced::Color {
            r: 0.84,
            g: 0.72,
            b: 0.24,
            a: 1.0,
        }
    } else {
        iced::Color {
            r: 0.56,
            g: 0.59,
            b: 0.65,
            a: 1.0,
        }
    }
}

fn volume_ratio(value: f32, visible_max: f32) -> f32 {
    if value <= 0.0 || visible_max <= 0.0 {
        0.0
    } else {
        (value / visible_max).clamp(0.0, 1.0)
    }
}

fn solid_mix(base: iced::Color, tint: iced::Color, tint_weight: f32) -> iced::Color {
    let tint_weight = tint_weight.clamp(0.0, 1.0);
    let base_weight = 1.0 - tint_weight;
    iced::Color {
        r: base.r * base_weight + tint.r * tint_weight,
        g: base.g * base_weight + tint.g * tint_weight,
        b: base.b * base_weight + tint.b * tint_weight,
        a: 1.0,
    }
}
