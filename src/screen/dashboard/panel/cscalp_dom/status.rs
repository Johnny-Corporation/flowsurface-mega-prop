use super::CscalpDom;
use crate::style;
use iced::{
    Alignment, Color, Point, Size,
    widget::canvas::{Frame, Path, Stroke, Text},
};
use replay::ReplayStatusRecord;

const TIMELINE_X: f32 = 15.0;
const LABEL_X: f32 = 28.0;
const FIRST_Y: f32 = 16.0;
const STATE_GAP: f32 = 20.0;

impl CscalpDom {
    pub(super) fn draw_status_timeline(
        &self,
        frame: &mut Frame,
        text_color: Color,
        divider_color: Color,
    ) {
        let Some(window) = self.replay_status else {
            return;
        };

        let labels = timeline_labels(window);

        let background_width = labels
            .iter()
            .map(|label| label.chars().count() as f32)
            .fold(0.0, f32::max)
            * style::text_size::TINY
            * 0.62
            + 36.0;
        frame.fill_rectangle(
            Point::new(7.0, 5.0),
            Size::new(background_width.max(112.0), 62.0),
            Color::BLACK.scale_alpha(0.72),
        );

        let first = Point::new(TIMELINE_X, FIRST_Y);
        let last = Point::new(TIMELINE_X, FIRST_Y + STATE_GAP * 2.0);
        frame.stroke(
            &Path::line(first, last),
            Stroke::default()
                .with_color(divider_color.scale_alpha(0.62))
                .with_width(1.0),
        );

        for (index, label) in labels.iter().enumerate() {
            let current = index == 1;
            let y = FIRST_Y + index as f32 * STATE_GAP;
            let color = if current {
                status_color(window.current, text_color)
            } else {
                text_color.scale_alpha(0.42)
            };
            let point = Path::circle(Point::new(TIMELINE_X, y), if current { 4.0 } else { 3.0 });
            if current {
                frame.fill(&point, color);
            } else {
                frame.stroke(&point, Stroke::default().with_color(color).with_width(1.0));
            }
            frame.fill_text(Text {
                content: label.clone(),
                position: Point::new(LABEL_X, y),
                color,
                size: style::text_size::TINY.into(),
                font: style::AZERET_MONO,
                align_x: Alignment::Start.into(),
                align_y: Alignment::Center.into(),
                ..Default::default()
            });
        }
    }
}

fn timeline_labels(window: replay::ReplayStatusWindow) -> [String; 3] {
    [
        window
            .previous
            .map(|record| format!("PREV  {}", status_label(record)))
            .unwrap_or_else(|| "PREV  —".to_string()),
        window
            .current
            .map(status_label)
            .unwrap_or_else(|| "STATUS UNAVAILABLE".to_string()),
        // Historical future states are intentionally hidden: an unscheduled halt here
        // would be a tiny time machine, and backtests have enough temptations already.
        "NEXT  —".to_string(),
    ]
}

fn status_label(record: ReplayStatusRecord) -> String {
    let action = match record.action {
        1 => "PRE-OPEN",
        2 => "PRE-CROSS",
        3 => "QUOTING",
        4 => "AUCTION",
        5 => "ROTATION",
        7 => "TRADING",
        8 => "HALTED",
        9 => "PAUSED",
        10 => "SUSPENDED",
        11 => "PRE-CLOSE",
        12 => "CLOSED",
        13 => "POST-CLOSE",
        15 => "NOT AVAILABLE",
        _ => "STATUS UPDATE",
    };
    let reason = status_reason(record.reason);
    if reason.is_empty() {
        action.to_string()
    } else {
        format!("{action} · {reason}")
    }
}

fn status_reason(reason: u16) -> &'static str {
    match reason {
        2 => "SURVEILLANCE",
        3 => "MARKET EVENT",
        5 => "EXPIRED",
        6 => "RECOVERY",
        10 => "REGULATORY",
        11 => "ADMINISTRATIVE",
        30 => "NEWS PENDING",
        31 => "NEWS RELEASED",
        40 => "ORDER IMBALANCE",
        50 => "LULD",
        60 => "OPERATIONAL",
        100 => "CORPORATE ACTION",
        120 => "MARKET HALT L1",
        121 => "MARKET HALT L2",
        122 => "MARKET HALT L3",
        _ => "",
    }
}

fn status_color(record: Option<ReplayStatusRecord>, default: Color) -> Color {
    match record.map(|record| record.action) {
        Some(7) => Color::from_rgb8(46, 204, 113),
        Some(8..=10 | 15) => Color::from_rgb8(255, 82, 82),
        Some(1..=5 | 11..=13) => Color::from_rgb8(255, 193, 7),
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(action: u16, reason: u16) -> ReplayStatusRecord {
        ReplayStatusRecord {
            ts_recv_ns: 0,
            ts_event_ns: 0,
            action,
            reason,
            trading_event: 0,
            is_trading: b'~',
            is_quoting: b'~',
            is_short_sell_restricted: b'~',
        }
    }

    #[test]
    fn labels_persistent_phase_and_material_halt_reason() {
        assert_eq!(status_label(record(7, 0)), "TRADING");
        assert_eq!(status_label(record(8, 50)), "HALTED · LULD");
    }

    #[test]
    fn annotations_do_not_claim_a_new_phase() {
        assert_eq!(status_label(record(14, 0)), "STATUS UPDATE");
    }

    #[test]
    fn future_status_is_not_revealed_by_the_timeline() {
        let labels = timeline_labels(replay::ReplayStatusWindow {
            previous: Some(record(1, 0)),
            current: Some(record(7, 0)),
            next: Some(record(8, 50)),
            latest: Some(record(7, 0)),
        });
        assert_eq!(labels[2], "NEXT  —");
        assert!(!labels[2].contains("HALT"));
    }
}
