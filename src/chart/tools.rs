use super::{Message, ViewState};
use crate::{style, widget::tooltip};

use exchange::unit::Price;
use iced::theme::palette::Extended;
use iced::widget::canvas::{self, LineDash, Path, Stroke};
use iced::{
    Alignment, Color, Element, Length, Point, Rectangle, Size, Theme, Vector, padding,
    widget::{button, container, row, text},
};

const HIT_TARGET_PX: f32 = 9.0;
const DELETE_MARKER_PX: f32 = 14.0;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ChartTool {
    #[default]
    Hand,
    Level,
    Rectangle,
    Line,
    Ray,
}

impl ChartTool {
    pub fn starts_draft(self) -> bool {
        matches!(
            self,
            ChartTool::Rectangle | ChartTool::Line | ChartTool::Ray
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceLevel {
    pub price: Price,
}

impl PriceLevel {
    pub fn new(price: Price) -> Self {
        Self { price }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartPoint {
    pub x_anchor: u64,
    pub price: Price,
}

impl ChartPoint {
    pub fn new(x_anchor: u64, price: Price) -> Self {
        Self { x_anchor, price }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartRectangle {
    pub start: ChartPoint,
    pub end: ChartPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    Line,
    Ray,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartSegment {
    pub start: ChartPoint,
    pub end: ChartPoint,
    pub kind: SegmentKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationId {
    Level(usize),
    Rectangle(usize),
    Segment(usize),
}

pub(super) fn toolbar<'a>(selected_tool: ChartTool, has_annotations: bool) -> Element<'a, Message> {
    let tool_button = |tool, label: &'static str, tooltip_text: &'static str| {
        let is_active = selected_tool == tool;
        let content = text(label)
            .size(crate::style::text_size::SMALL)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);

        let btn = button(content)
            .width(Length::Fixed(52.0))
            .height(Length::Fixed(24.0))
            .padding(0)
            .on_press(Message::ToolSelected(tool))
            .style(move |theme: &Theme, status| {
                style::button::transparent(theme, status, is_active)
            });

        tooltip(
            btn,
            Some(tooltip_text),
            iced::widget::tooltip::Position::Bottom,
        )
    };

    let clear_active = has_annotations;
    let mut clear_btn = button(
        text("Clear")
            .size(crate::style::text_size::SMALL)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .width(Length::Fixed(54.0))
    .height(Length::Fixed(24.0))
    .padding(0)
    .style(move |theme: &Theme, status| style::button::transparent(theme, status, clear_active));

    if has_annotations {
        clear_btn = clear_btn.on_press(Message::ClearAnnotations);
    }

    container(
        row![
            tool_button(ChartTool::Hand, "Hand", "Pan and inspect"),
            tool_button(ChartTool::Level, "Level", "Horizontal price level"),
            tool_button(ChartTool::Rectangle, "Rect", "Draw rectangle"),
            tool_button(ChartTool::Line, "Line", "Draw line"),
            tool_button(ChartTool::Ray, "Ray", "Draw ray"),
            tooltip(
                clear_btn,
                Some("Clear drawings"),
                iced::widget::tooltip::Position::Bottom
            ),
        ]
        .spacing(2)
        .padding(padding::left(6).right(6).top(3).bottom(3))
        .align_y(Alignment::Center),
    )
    .height(Length::Fixed(30.0))
    .width(Length::Fill)
    .style(style::pane_title_bar)
    .into()
}

pub(super) fn draw_annotations(
    state: &ViewState,
    frame: &mut canvas::Frame,
    palette: &Extended,
    region: Rectangle,
) {
    draw_price_levels(state, frame, palette, region);
    draw_rectangles(state, frame, palette, region);
    draw_segments(state, frame, palette, region);
}

pub(super) fn draw_annotation_overlay(
    state: &ViewState,
    frame: &mut canvas::Frame,
    palette: &Extended,
    bounds: Size,
    cursor_position: Option<Point>,
    draft: Option<(ChartTool, ChartPoint)>,
    region: Rectangle,
) {
    if let (Some((tool, start)), Some(cursor)) = (draft, cursor_position) {
        let end = state.snapped_point(cursor, bounds, region);
        draw_draft(state, frame, palette, region, tool, start, end);
    }

    let Some(cursor) = cursor_position else {
        return;
    };

    let cursor_chart = state.cursor_chart_point(cursor, bounds, region);
    if let Some(id) = state.hovered_annotation(cursor_chart, region)
        && let Some(marker) = state.delete_marker_position(id, region)
    {
        draw_delete_marker(state, frame, palette, marker);
    }
}

pub(super) fn has_annotations(state: &ViewState) -> bool {
    !state.price_levels.is_empty() || !state.rectangles.is_empty() || !state.segments.is_empty()
}

pub(super) fn add_level(state: &mut ViewState, price: Price) {
    if let Some(index) = state
        .price_levels
        .iter()
        .position(|level| level.price == price)
    {
        state.price_levels.remove(index);
    } else {
        state.price_levels.push(PriceLevel::new(price));
        state
            .price_levels
            .sort_unstable_by_key(|level| level.price.units);
    }
}

pub(super) fn add_rectangle(state: &mut ViewState, start: ChartPoint, end: ChartPoint) {
    if start != end {
        state.rectangles.push(ChartRectangle { start, end });
    }
}

pub(super) fn add_segment(
    state: &mut ViewState,
    start: ChartPoint,
    end: ChartPoint,
    kind: SegmentKind,
) {
    if start != end {
        state.segments.push(ChartSegment { start, end, kind });
    }
}

pub(super) fn delete_annotation(state: &mut ViewState, id: AnnotationId) {
    match id {
        AnnotationId::Level(index) => {
            if index < state.price_levels.len() {
                state.price_levels.remove(index);
            }
        }
        AnnotationId::Rectangle(index) => {
            if index < state.rectangles.len() {
                state.rectangles.remove(index);
            }
        }
        AnnotationId::Segment(index) => {
            if index < state.segments.len() {
                state.segments.remove(index);
            }
        }
    }
}

pub(super) fn clear_annotations(state: &mut ViewState) {
    state.price_levels.clear();
    state.rectangles.clear();
    state.segments.clear();
}

fn draw_price_levels(
    state: &ViewState,
    frame: &mut canvas::Frame,
    palette: &Extended,
    region: Rectangle,
) {
    for level in &state.price_levels {
        let y = state.price_to_y(level.price);
        if y < region.y || y > region.y + region.height {
            continue;
        }

        frame.stroke(
            &Path::line(
                Point::new(region.x, y),
                Point::new(region.x + region.width, y),
            ),
            annotation_stroke(state, palette.primary.base.color),
        );
    }
}

fn draw_rectangles(
    state: &ViewState,
    frame: &mut canvas::Frame,
    palette: &Extended,
    region: Rectangle,
) {
    for rect in &state.rectangles {
        let bounds = state.rectangle_bounds(*rect);
        if !rectangles_intersect(bounds, region) {
            continue;
        }

        frame.fill_rectangle(
            bounds.position(),
            bounds.size(),
            palette.primary.base.color.scale_alpha(0.08),
        );
        frame.stroke_rectangle(
            bounds.position(),
            bounds.size(),
            annotation_stroke(state, palette.primary.base.color),
        );
    }
}

fn draw_segments(
    state: &ViewState,
    frame: &mut canvas::Frame,
    palette: &Extended,
    region: Rectangle,
) {
    for segment in &state.segments {
        let start = state.point_to_chart_xy(segment.start);
        let mut end = state.point_to_chart_xy(segment.end);

        if segment.kind == SegmentKind::Ray {
            end = ray_far_point(start, end, region);
        }

        frame.stroke(
            &Path::line(start, end),
            annotation_stroke(state, palette.primary.base.color),
        );
    }
}

fn draw_draft(
    state: &ViewState,
    frame: &mut canvas::Frame,
    palette: &Extended,
    region: Rectangle,
    tool: ChartTool,
    start: ChartPoint,
    end: ChartPoint,
) {
    let color = palette.primary.strong.color.scale_alpha(0.85);
    match tool {
        ChartTool::Rectangle => {
            let bounds = state.rectangle_bounds(ChartRectangle { start, end });
            frame.fill_rectangle(bounds.position(), bounds.size(), color.scale_alpha(0.08));
            frame.stroke_rectangle(
                bounds.position(),
                bounds.size(),
                annotation_stroke(state, color),
            );
        }
        ChartTool::Line | ChartTool::Ray => {
            let start = state.point_to_chart_xy(start);
            let mut end = state.point_to_chart_xy(end);

            if tool == ChartTool::Ray {
                end = ray_far_point(start, end, region);
            }

            frame.stroke(&Path::line(start, end), annotation_stroke(state, color));
        }
        ChartTool::Hand | ChartTool::Level => {}
    }
}

fn draw_delete_marker(
    state: &ViewState,
    frame: &mut canvas::Frame,
    palette: &Extended,
    position: Point,
) {
    let size = DELETE_MARKER_PX / state.scaling.max(1.0);
    let half = size / 2.0;
    let top_left = Point::new(position.x - half, position.y - half);
    let bg = if palette.is_dark {
        palette.background.strong.color.scale_alpha(0.92)
    } else {
        palette.background.weakest.color.scale_alpha(0.94)
    };

    frame.fill_rectangle(top_left, Size::new(size, size), bg);
    frame.stroke_rectangle(
        top_left,
        Size::new(size, size),
        Stroke::with_color(
            Stroke {
                width: (1.0 / state.scaling.max(1.0)).max(0.5),
                ..Default::default()
            },
            palette.danger.base.color.scale_alpha(0.9),
        ),
    );

    let inset = size * 0.28;
    let stroke = Stroke::with_color(
        Stroke {
            width: (1.3 / state.scaling.max(1.0)).max(0.6),
            ..Default::default()
        },
        palette.danger.base.color,
    );
    frame.stroke(
        &Path::line(
            Point::new(top_left.x + inset, top_left.y + inset),
            Point::new(top_left.x + size - inset, top_left.y + size - inset),
        ),
        stroke,
    );
    frame.stroke(
        &Path::line(
            Point::new(top_left.x + size - inset, top_left.y + inset),
            Point::new(top_left.x + inset, top_left.y + size - inset),
        ),
        stroke,
    );
}

fn annotation_stroke(state: &ViewState, color: Color) -> Stroke<'static> {
    Stroke::with_color(
        Stroke {
            width: (1.2 / state.scaling.max(1.0)).max(0.6),
            line_dash: LineDash {
                segments: &[6.0, 3.0],
                offset: 0,
            },
            ..Default::default()
        },
        color.scale_alpha(0.72),
    )
}

fn ray_far_point(start: Point, end: Point, region: Rectangle) -> Point {
    let direction = end - start;
    let length = (direction.x * direction.x + direction.y * direction.y).sqrt();
    if length <= f32::EPSILON {
        return end;
    }

    let max_span = region.width.max(region.height).max(1.0);
    let unit = Vector::new(direction.x / length, direction.y / length);
    start + unit * (max_span * 4.0)
}

fn rectangles_intersect(a: Rectangle, b: Rectangle) -> bool {
    a.x <= b.x + b.width && a.x + a.width >= b.x && a.y <= b.y + b.height && a.y + a.height >= b.y
}

fn distance_to_segment(point: Point, start: Point, end: Point) -> f32 {
    let segment = end - start;
    let length_sq = segment.x * segment.x + segment.y * segment.y;
    if length_sq <= f32::EPSILON {
        return point.distance(start);
    }

    let to_point = point - start;
    let t = ((to_point.x * segment.x + to_point.y * segment.y) / length_sq).clamp(0.0, 1.0);
    let closest = start + segment * t;
    point.distance(closest)
}

fn distance_to_ray(point: Point, start: Point, end: Point) -> f32 {
    let direction = end - start;
    let length_sq = direction.x * direction.x + direction.y * direction.y;
    if length_sq <= f32::EPSILON {
        return point.distance(start);
    }

    let to_point = point - start;
    let t = ((to_point.x * direction.x + to_point.y * direction.y) / length_sq).max(0.0);
    let closest = start + direction * t;
    point.distance(closest)
}

impl ViewState {
    pub(super) fn snapped_point(
        &self,
        cursor: Point,
        bounds: Size,
        region: Rectangle,
    ) -> ChartPoint {
        let (x_anchor, _) = self.snap_x_to_index(cursor.x, bounds, region);
        let chart_y = region.y + (cursor.y / bounds.height) * region.height;
        let effective_step = if self.tick_size.units > 0 {
            self.tick_size
        } else {
            self.ticker_info.min_ticksize.into()
        };
        let price = self.y_to_price(chart_y).round_to_step(effective_step);

        ChartPoint::new(x_anchor, price)
    }

    pub(super) fn cursor_chart_point(
        &self,
        cursor: Point,
        bounds: Size,
        region: Rectangle,
    ) -> Point {
        Point::new(
            region.x + (cursor.x / bounds.width) * region.width,
            region.y + (cursor.y / bounds.height) * region.height,
        )
    }

    pub(super) fn point_to_chart_xy(&self, point: ChartPoint) -> Point {
        Point::new(
            self.interval_to_x(point.x_anchor),
            self.price_to_y(point.price),
        )
    }

    pub(super) fn rectangle_bounds(&self, rect: ChartRectangle) -> Rectangle {
        let start = self.point_to_chart_xy(rect.start);
        let end = self.point_to_chart_xy(rect.end);

        Rectangle {
            x: start.x.min(end.x),
            y: start.y.min(end.y),
            width: (start.x - end.x).abs(),
            height: (start.y - end.y).abs(),
        }
    }

    pub(super) fn hovered_annotation(
        &self,
        cursor_chart: Point,
        region: Rectangle,
    ) -> Option<AnnotationId> {
        let hit = HIT_TARGET_PX / self.scaling.max(1.0);

        for (index, rect) in self.rectangles.iter().enumerate().rev() {
            let bounds = self.rectangle_bounds(*rect);
            if rectangles_intersect(bounds, region) && bounds.contains(cursor_chart) {
                return Some(AnnotationId::Rectangle(index));
            }
        }

        for (index, segment) in self.segments.iter().enumerate().rev() {
            let start = self.point_to_chart_xy(segment.start);
            let end = self.point_to_chart_xy(segment.end);
            let distance = match segment.kind {
                SegmentKind::Line => distance_to_segment(cursor_chart, start, end),
                SegmentKind::Ray => distance_to_ray(cursor_chart, start, end),
            };

            if distance <= hit {
                return Some(AnnotationId::Segment(index));
            }
        }

        for (index, level) in self.price_levels.iter().enumerate().rev() {
            let y = self.price_to_y(level.price);
            if y >= region.y && y <= region.y + region.height && (cursor_chart.y - y).abs() <= hit {
                return Some(AnnotationId::Level(index));
            }
        }

        None
    }

    pub(super) fn hovered_delete_marker(
        &self,
        cursor_chart: Point,
        region: Rectangle,
    ) -> Option<AnnotationId> {
        let id = self.hovered_annotation(cursor_chart, region)?;
        let marker = self.delete_marker_position(id, region)?;
        let hit = (DELETE_MARKER_PX * 0.75) / self.scaling.max(1.0);

        if cursor_chart.distance(marker) <= hit {
            Some(id)
        } else {
            None
        }
    }

    pub(super) fn delete_marker_position(
        &self,
        id: AnnotationId,
        region: Rectangle,
    ) -> Option<Point> {
        let inset = DELETE_MARKER_PX / self.scaling.max(1.0);
        match id {
            AnnotationId::Level(index) => {
                let level = self.price_levels.get(index)?;
                Some(Point::new(
                    region.x + region.width - inset,
                    self.price_to_y(level.price),
                ))
            }
            AnnotationId::Rectangle(index) => {
                let rect = self.rectangles.get(index)?;
                let bounds = self.rectangle_bounds(*rect);
                let marker_inset = inset * 0.5;
                Some(Point::new(
                    bounds.x + bounds.width - marker_inset,
                    bounds.y + marker_inset,
                ))
            }
            AnnotationId::Segment(index) => {
                let segment = self.segments.get(index)?;
                Some(self.point_to_chart_xy(segment.end))
            }
        }
    }
}
