use super::{
    CscalpDom, PRINT_BUBBLE_MAX_RADIUS, PRINT_BUBBLE_MIN_RADIUS, PRINT_LABEL_MAX_COUNT,
    PRINT_LABEL_MIN_RADIUS, RECENT_PRINT_LIMIT,
    types::{ColumnRanges, LabelBox, PriceGrid, text_box},
};
use crate::style;
use exchange::Trade;
use iced::{
    Alignment, Point, Rectangle, Size,
    widget::canvas::{Path, Stroke, Text},
};

impl CscalpDom {
    pub(super) fn draw_recent_prints(
        &self,
        frame: &mut iced::widget::canvas::Frame,
        grid: &PriceGrid,
        bounds: Rectangle,
        cols: &ColumnRanges,
        bid_color: iced::Color,
        ask_color: iced::Color,
        text_color: iced::Color,
    ) {
        let recent: Vec<Trade> = self
            .trades
            .raw
            .iter()
            .rev()
            .take(RECENT_PRINT_LIMIT)
            .copied()
            .collect();
        if recent.is_empty() {
            return;
        }

        let max_qty = recent
            .iter()
            .map(|trade| f32::from(trade.qty))
            .fold(0.0, f32::max);
        if max_qty <= 0.0 {
            return;
        }

        let print_width = (cols.prints.1 - cols.prints.0).max(0.0);
        let x_min = cols.prints.0 + PRINT_BUBBLE_MAX_RADIUS + 3.0;
        let x_max = cols.prints.1 - PRINT_BUBBLE_MAX_RADIUS - 3.0;
        let count = recent.len();
        let mut label_candidates: Vec<(f32, LabelBox, String, Point)> = Vec::new();

        for (idx, trade) in recent.iter().rev().enumerate() {
            let grouped_price = trade.price.round_to_side_step(trade.is_sell, grid.tick);
            let Some(y) = self.price_to_screen_y(grouped_price, grid, bounds.height) else {
                continue;
            };

            let age = if count > 1 {
                idx as f32 / (count - 1) as f32
            } else {
                1.0
            };
            let x = if x_max > x_min {
                x_min + (x_max - x_min) * age
            } else {
                cols.prints.0 + print_width * 0.5
            };
            let size_ratio = (f32::from(trade.qty) / max_qty).clamp(0.0, 1.0).sqrt();
            let radius = PRINT_BUBBLE_MIN_RADIUS
                + (PRINT_BUBBLE_MAX_RADIUS - PRINT_BUBBLE_MIN_RADIUS) * size_ratio;
            if y + radius < 0.0 || y - radius > bounds.height {
                continue;
            }

            self.draw_print_bubble(
                frame,
                x,
                y,
                radius,
                age,
                trade.is_sell,
                bid_color,
                ask_color,
            );
            self.queue_print_label(
                &mut label_candidates,
                trade,
                x,
                y,
                radius,
                age,
                size_ratio,
                print_width,
            );
        }

        self.draw_print_labels(frame, label_candidates, text_color);
    }

    fn draw_print_bubble(
        &self,
        frame: &mut iced::widget::canvas::Frame,
        x: f32,
        y: f32,
        radius: f32,
        age: f32,
        is_sell: bool,
        bid_color: iced::Color,
        ask_color: iced::Color,
    ) {
        let color = if is_sell { ask_color } else { bid_color };
        let alpha = 0.24 + age * 0.62;
        let circle = Path::circle(Point::new(x, y), radius);
        frame.fill(&circle, color.scale_alpha(alpha));
        frame.stroke(
            &circle,
            Stroke::default()
                .with_color(color.scale_alpha((alpha + 0.18).min(1.0)))
                .with_width(1.0),
        );
    }

    fn queue_print_label(
        &self,
        label_candidates: &mut Vec<(f32, LabelBox, String, Point)>,
        trade: &Trade,
        x: f32,
        y: f32,
        radius: f32,
        age: f32,
        size_ratio: f32,
        print_width: f32,
    ) {
        if radius < PRINT_LABEL_MIN_RADIUS || print_width < 48.0 {
            return;
        }

        let content = self.format_quantity(trade.qty);
        let label_box = text_box(x, y, content.len(), style::text_size::TINY, 3.0);
        let priority = age * 2.0 + size_ratio;
        label_candidates.push((priority, label_box, content, Point::new(x, y)));
    }

    fn draw_print_labels(
        &self,
        frame: &mut iced::widget::canvas::Frame,
        mut label_candidates: Vec<(f32, LabelBox, String, Point)>,
        text_color: iced::Color,
    ) {
        label_candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut label_boxes: Vec<LabelBox> = Vec::new();
        let mut labels: Vec<(String, Point)> = Vec::new();

        for (_, label_box, content, position) in label_candidates {
            if labels.len() >= PRINT_LABEL_MAX_COUNT {
                break;
            }
            if label_boxes
                .iter()
                .any(|existing| existing.overlaps(label_box))
            {
                continue;
            }

            label_boxes.push(label_box);
            labels.push((content, position));
        }

        labels.sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
        for (content, position) in labels {
            frame.fill_rectangle(
                Point::new(position.x - 16.0, position.y - 6.5),
                Size::new(32.0, 13.0),
                iced::Color::WHITE.scale_alpha(0.16),
            );
            frame.fill_text(Text {
                content,
                position,
                color: text_color.scale_alpha(0.92),
                size: style::text_size::TINY.into(),
                font: style::AZERET_MONO,
                align_x: Alignment::Center.into(),
                align_y: Alignment::Center.into(),
                ..Default::default()
            });
        }
    }
}
