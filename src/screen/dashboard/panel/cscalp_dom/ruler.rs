use super::{
    CscalpDom, ROW_HEIGHT,
    types::{ColumnRanges, PriceGrid},
};
use crate::style;
use exchange::unit::Price;
use iced::{
    Alignment, Point, Rectangle, Size,
    widget::canvas::{Frame, Text},
};

impl CscalpDom {
    pub(super) fn draw_price_ruler(
        &self,
        frame: &mut Frame,
        grid: &PriceGrid,
        bounds: Rectangle,
        cursor: iced_core::mouse::Cursor,
        cols: &ColumnRanges,
        text_color: iced::Color,
    ) {
        if !self.config.show_ruler {
            return;
        }

        let Some(cursor_position) = cursor.position_in(bounds) else {
            return;
        };
        let Some(price) = self.screen_y_to_price(cursor_position.y, grid, bounds.height) else {
            return;
        };
        let Some(y) = self.price_to_screen_y(price, grid, bounds.height) else {
            return;
        };
        if y < 0.0 || y > bounds.height {
            return;
        }

        let ruler_color = ruler_color(text_color);
        frame.fill_rectangle(
            Point::new(0.0, y - 0.5),
            Size::new(bounds.width, 1.0),
            ruler_color,
        );

        if let Some(label) = self.ruler_distance_label(price, grid) {
            self.draw_ruler_label(frame, &label, y, cols, text_color);
        }
    }

    fn ruler_distance_label(&self, price: Price, grid: &PriceGrid) -> Option<String> {
        let ticks = if price >= grid.best_ask {
            Price::steps_between_inclusive(grid.best_ask, price, grid.tick)?.saturating_sub(1)
        } else if price <= grid.best_bid {
            Price::steps_between_inclusive(price, grid.best_bid, grid.tick)?.saturating_sub(1)
        } else {
            return None;
        };

        Some(format!("{ticks}t"))
    }

    fn draw_ruler_label(
        &self,
        frame: &mut Frame,
        label: &str,
        y: f32,
        cols: &ColumnRanges,
        text_color: iced::Color,
    ) {
        let width = label.chars().count() as f32 * style::text_size::TINY * 0.66 + 8.0;
        let x = (cols.price.0 - width - 4.0).max(cols.order_qty.0 + 2.0);
        let plate_color = label_plate_color(text_color);

        frame.fill_rectangle(
            Point::new(x, y - ROW_HEIGHT * 0.5 + 1.0),
            Size::new(width, ROW_HEIGHT - 2.0),
            plate_color,
        );
        frame.fill_text(Text {
            content: label.to_string(),
            position: Point::new(x + width * 0.5, y),
            color: text_color.scale_alpha(0.88),
            size: style::text_size::TINY.into(),
            font: style::AZERET_MONO,
            align_x: Alignment::Center.into(),
            align_y: Alignment::Center.into(),
            ..Default::default()
        });
    }
}

fn ruler_color(text_color: iced::Color) -> iced::Color {
    let luminance = 0.2126 * text_color.r + 0.7152 * text_color.g + 0.0722 * text_color.b;
    if luminance > 0.5 {
        iced::Color::WHITE.scale_alpha(0.20)
    } else {
        iced::Color {
            r: 0.45,
            g: 0.47,
            b: 0.50,
            a: 0.18,
        }
    }
}

fn label_plate_color(text_color: iced::Color) -> iced::Color {
    let luminance = 0.2126 * text_color.r + 0.7152 * text_color.g + 0.0722 * text_color.b;
    if luminance > 0.5 {
        iced::Color::BLACK.scale_alpha(0.24)
    } else {
        iced::Color::WHITE.scale_alpha(0.55)
    }
}
