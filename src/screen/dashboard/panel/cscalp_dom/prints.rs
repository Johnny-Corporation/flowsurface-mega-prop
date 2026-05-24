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

struct PrintBubble {
    position: Point,
    radius: f32,
    age: f32,
    is_sell: bool,
    label: String,
    label_box: Option<LabelBox>,
    priority: f32,
}

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
        let mut bubbles: Vec<PrintBubble> = Vec::new();

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

            bubbles.push(self.print_bubble(trade, x, y, radius, age, size_ratio, print_width));
        }

        self.draw_print_links(frame, &bubbles, bid_color, ask_color);
        for bubble in &bubbles {
            self.draw_print_bubble(frame, bubble, bid_color, ask_color);
        }
        self.draw_print_labels(frame, &bubbles, text_color);
    }

    fn draw_print_bubble(
        &self,
        frame: &mut iced::widget::canvas::Frame,
        bubble: &PrintBubble,
        bid_color: iced::Color,
        ask_color: iced::Color,
    ) {
        let color = if bubble.is_sell { ask_color } else { bid_color };
        let alpha = 0.30 + bubble.age * 0.62;
        let circle = Path::circle(bubble.position, bubble.radius);
        frame.fill(&circle, color.scale_alpha(alpha));
        frame.stroke(
            &circle,
            Stroke::default()
                .with_color(color.scale_alpha((alpha + 0.18).min(1.0)))
                .with_width(1.0),
        );
    }

    fn draw_print_links(
        &self,
        frame: &mut iced::widget::canvas::Frame,
        bubbles: &[PrintBubble],
        bid_color: iced::Color,
        ask_color: iced::Color,
    ) {
        for pair in bubbles.windows(2) {
            let from = &pair[0];
            let to = &pair[1];
            let color = if to.is_sell { ask_color } else { bid_color };
            let line = Path::line(from.position, to.position);
            frame.stroke(
                &line,
                Stroke::default()
                    .with_color(color.scale_alpha(0.26 + to.age * 0.16))
                    .with_width(0.8),
            );
        }
    }

    fn print_bubble(
        &self,
        trade: &Trade,
        x: f32,
        y: f32,
        radius: f32,
        age: f32,
        size_ratio: f32,
        print_width: f32,
    ) -> PrintBubble {
        let label = self.format_quantity(trade.qty);
        let label_box = if radius >= PRINT_LABEL_MIN_RADIUS && print_width >= 48.0 {
            Some(text_box(
                x,
                y,
                label.chars().count(),
                style::text_size::TINY,
                4.0,
            ))
        } else {
            None
        };
        let priority = age * 2.0 + size_ratio;
        PrintBubble {
            position: Point::new(x, y),
            radius,
            age,
            is_sell: trade.is_sell,
            label,
            label_box,
            priority,
        }
    }

    fn draw_print_labels(
        &self,
        frame: &mut iced::widget::canvas::Frame,
        bubbles: &[PrintBubble],
        text_color: iced::Color,
    ) {
        let mut label_candidates: Vec<&PrintBubble> = bubbles
            .iter()
            .filter(|bubble| bubble.label_box.is_some())
            .collect();
        label_candidates.sort_by(|a, b| b.priority.total_cmp(&a.priority));
        let mut label_boxes: Vec<LabelBox> = Vec::new();
        let mut labels: Vec<&PrintBubble> = Vec::new();

        for bubble in label_candidates {
            let Some(label_box) = bubble.label_box else {
                continue;
            };
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
            labels.push(bubble);
        }

        labels.sort_by(|a, b| a.position.x.total_cmp(&b.position.x));
        let (plate_color, shadow_color) = label_contrast_colors(text_color);
        for bubble in labels {
            let width = bubble.label.chars().count() as f32 * style::text_size::TINY * 0.66 + 8.0;
            frame.fill_rectangle(
                Point::new(bubble.position.x - width * 0.5, bubble.position.y - 6.5),
                Size::new(width, 13.0),
                plate_color,
            );
            frame.fill_text(Text {
                content: bubble.label.clone(),
                position: Point::new(bubble.position.x + 0.7, bubble.position.y + 0.7),
                color: shadow_color,
                size: style::text_size::TINY.into(),
                font: style::AZERET_MONO,
                align_x: Alignment::Center.into(),
                align_y: Alignment::Center.into(),
                ..Default::default()
            });
            frame.fill_text(Text {
                content: bubble.label.clone(),
                position: bubble.position,
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

fn label_contrast_colors(text_color: iced::Color) -> (iced::Color, iced::Color) {
    let luminance = 0.2126 * text_color.r + 0.7152 * text_color.g + 0.0722 * text_color.b;
    if luminance > 0.5 {
        (
            iced::Color::BLACK.scale_alpha(0.36),
            iced::Color::BLACK.scale_alpha(0.55),
        )
    } else {
        (
            iced::Color::WHITE.scale_alpha(0.72),
            iced::Color::WHITE.scale_alpha(0.55),
        )
    }
}
