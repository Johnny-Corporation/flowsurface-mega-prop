use std::sync::atomic::{AtomicUsize, Ordering};

use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point, Rectangle, Size};

const INVALID_GEOMETRY_LOG_LIMIT: usize = 32;
static INVALID_GEOMETRY_LOGS: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn finite_point(point: Point) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

pub(crate) fn positive_finite_size(size: Size) -> bool {
    size.width.is_finite() && size.height.is_finite() && size.width > 0.0 && size.height > 0.0
}

pub(crate) fn positive_finite_rect(rect: Rectangle) -> bool {
    finite_point(rect.position()) && positive_finite_size(rect.size())
}

pub(crate) fn valid_positive_rect(context: &'static str, rect: Rectangle) -> bool {
    if positive_finite_rect(rect) {
        true
    } else {
        log_skip(context, "invalid rectangle bounds");
        false
    }
}

pub(crate) fn positive_finite_radius(radius: f32) -> bool {
    radius.is_finite() && radius > 0.0
}

pub(crate) fn valid_chart_canvas(
    context: &'static str,
    bounds: Rectangle,
    chart_bounds: Rectangle,
    scaling: f32,
    cell_width: f32,
) -> bool {
    if !positive_finite_rect(bounds) {
        log_skip(context, "invalid canvas bounds");
        return false;
    }

    if !positive_finite_rect(chart_bounds) {
        log_skip(context, "invalid chart bounds");
        return false;
    }

    if scaling <= f32::EPSILON || !scaling.is_finite() {
        log_skip(context, "invalid scaling");
        return false;
    }

    if cell_width <= f32::EPSILON || !cell_width.is_finite() {
        log_skip(context, "invalid cell width");
        return false;
    }

    true
}

pub(crate) fn stroke_line(
    frame: &mut canvas::Frame,
    start: Point,
    end: Point,
    stroke: Stroke<'_>,
    context: &'static str,
) {
    if !(finite_point(start) && finite_point(end)) {
        log_skip(context, "invalid line point");
        return;
    }

    frame.stroke(&Path::line(start, end), stroke);
}

pub(crate) fn fill_circle(
    frame: &mut canvas::Frame,
    center: Point,
    radius: f32,
    color: Color,
    context: &'static str,
) {
    if !finite_point(center) || !positive_finite_radius(radius) {
        log_skip(context, "invalid circle geometry");
        return;
    }

    frame.fill(&Path::circle(center, radius), color);
}

pub(crate) fn fill_rectangle(
    frame: &mut canvas::Frame,
    position: Point,
    size: Size,
    color: Color,
    context: &'static str,
) {
    if !finite_point(position) || !positive_finite_size(size) {
        log_skip(context, "invalid rectangle geometry");
        return;
    }

    frame.fill_rectangle(position, size, color);
}

pub(crate) fn stroke_rectangle(
    frame: &mut canvas::Frame,
    position: Point,
    size: Size,
    stroke: Stroke<'_>,
    context: &'static str,
) {
    if !finite_point(position) || !positive_finite_size(size) {
        log_skip(context, "invalid rectangle geometry");
        return;
    }

    frame.stroke_rectangle(position, size, stroke);
}

fn log_skip(context: &'static str, reason: &'static str) {
    let log_index = INVALID_GEOMETRY_LOGS.fetch_add(1, Ordering::Relaxed);

    if log_index < INVALID_GEOMETRY_LOG_LIMIT {
        log::warn!(
            target: "flowsurface::drawing_geometry",
            "DRAW_GEOMETRY_SKIPPED context={context} reason={reason}"
        );
    } else if log_index == INVALID_GEOMETRY_LOG_LIMIT {
        log::warn!(
            target: "flowsurface::drawing_geometry",
            "DRAW_GEOMETRY_SKIPPED suppressing further invalid drawing geometry logs"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_finite_canvas_geometry() {
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: f32::NAN,
            height: 120.0,
        };
        let chart_bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 120.0,
        };

        assert!(!valid_chart_canvas("test", bounds, chart_bounds, 1.0, 8.0));
    }

    #[test]
    fn accepts_positive_finite_canvas_geometry() {
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 120.0,
        };

        assert!(valid_chart_canvas("test", bounds, bounds, 1.0, 8.0));
    }

    #[test]
    fn rejects_non_positive_or_non_finite_radius() {
        assert!(!positive_finite_radius(0.0));
        assert!(!positive_finite_radius(f32::INFINITY));
        assert!(positive_finite_radius(2.0));
    }
}
