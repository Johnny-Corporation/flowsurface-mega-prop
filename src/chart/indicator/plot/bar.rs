use std::ops::RangeInclusive;

use iced::{Point, Size, Theme, widget::canvas};

use crate::chart::{
    ViewState,
    indicator::plot::{Plot, PlotTooltip, Series, TooltipFn, YScale},
    safe_geometry,
};

#[derive(Clone, Copy)]
#[allow(unused)]
/// How to anchor bar heights.
pub enum Baseline {
    /// Use zero as baseline (classic volume). Extents: [0, max].
    Zero,
    /// Use the minimum value in the visible range. Extents: [min, max].
    Min,
    /// Use a fixed numeric baseline.
    Fixed(f32),
}

#[derive(Clone, Copy)]
/// What kind of bar to render, and whether it carries a signed overlay.
/// The sign of `overlay` selects up (success) vs down (danger).
pub enum BarClass {
    /// draw a single bar using secondary strong color
    Single,
    /// draw two bars, a success/danger colored (alpha) and an overlay using full color.
    Overlay { overlay: f32 }, // signed; sign decides color
}

pub struct BarPlot<V, CL, T> {
    /// Maps a datapoint to the scalar value represented by the bar (before baseline).
    pub value: V,
    pub bar_width_factor: f32,
    pub padding: f32,
    pub classify: CL, // Single vs Overlay with signed overlay
    pub tooltip: Option<TooltipFn<T>>,
    pub baseline: Baseline,
    _phantom: std::marker::PhantomData<T>,
}

#[allow(dead_code)]
impl<V, CL, T> BarPlot<V, CL, T> {
    pub fn new(value: V, classify: CL) -> Self {
        Self {
            value,
            bar_width_factor: 0.9,
            padding: 0.0,
            classify,
            tooltip: None,
            baseline: Baseline::Zero,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn bar_width_factor(mut self, f: f32) -> Self {
        self.bar_width_factor = f;
        self
    }

    pub fn padding(mut self, p: f32) -> Self {
        self.padding = p;
        self
    }

    pub fn baseline(mut self, b: Baseline) -> Self {
        self.baseline = b;
        self
    }

    pub fn with_tooltip<F>(mut self, tooltip: F) -> Self
    where
        F: Fn(&T, Option<&T>) -> PlotTooltip + 'static,
    {
        self.tooltip = Some(Box::new(tooltip));
        self
    }
}

impl<S, V, CL> Plot<S> for BarPlot<V, CL, S::Y>
where
    S: Series,
    V: Fn(&S::Y) -> f32,
    CL: Fn(&S::Y) -> BarClass,
{
    fn y_extents(&self, datapoints: &S, range: RangeInclusive<u64>) -> Option<(f32, f32)> {
        let mut min_v = f32::MAX;
        let mut max_v = f32::MIN;
        let mut n = 0u32;

        datapoints.for_each_in(range, |_, y| {
            let v = (self.value)(y);
            if !v.is_finite() {
                return;
            }

            if v < min_v {
                min_v = v;
            }
            if v > max_v {
                max_v = v;
            }
            n += 1;
        });

        if n == 0 || (max_v <= 0.0 && matches!(self.baseline, Baseline::Zero)) {
            return None;
        }

        let min_ext = match self.baseline {
            Baseline::Zero => 0.0,
            Baseline::Min => min_v,
            Baseline::Fixed(v) => v,
        };
        if !min_ext.is_finite() {
            return None;
        }

        let lowest = min_ext;
        let mut highest = max_v.max(min_ext + f32::EPSILON);
        if highest > lowest && self.padding > 0.0 {
            highest *= 1.0 + self.padding;
        }
        if !highest.is_finite() {
            return None;
        }

        Some((lowest, highest))
    }

    fn draw(
        &self,
        frame: &mut canvas::Frame,
        ctx: &ViewState,
        theme: &Theme,
        datapoints: &S,
        range: RangeInclusive<u64>,
        scale: &YScale,
    ) {
        let palette = theme.extended_palette();
        let bar_width = ctx.cell_width * self.bar_width_factor;
        if !bar_width.is_finite() || bar_width <= 0.0 {
            return;
        }

        let baseline_value = match self.baseline {
            Baseline::Zero => 0.0,
            Baseline::Min => scale.min, // extents min
            Baseline::Fixed(v) => v,
        };
        if !baseline_value.is_finite() {
            return;
        }

        let y_base = scale.to_y(baseline_value);
        if !y_base.is_finite() {
            return;
        }

        datapoints.for_each_in(range, |x, y| {
            let center_x = ctx.interval_to_x(x);
            let left = center_x - (bar_width / 2.0);

            let total = (self.value)(y);
            if !left.is_finite() || !total.is_finite() {
                return;
            }

            let rel = total - baseline_value;

            let (top_y, h_total) = if rel > 0.0 {
                let y_total = scale.to_y(total);
                if !y_total.is_finite() {
                    return;
                }

                let h = (y_base - y_total).max(0.0);
                (y_total, h)
            } else {
                (y_base, 0.0)
            };
            if h_total <= 0.0 || !top_y.is_finite() || !h_total.is_finite() {
                return;
            }

            match (self.classify)(y) {
                BarClass::Single => {
                    safe_geometry::fill_rectangle(
                        frame,
                        Point::new(left, top_y),
                        Size::new(bar_width, h_total),
                        palette.secondary.strong.color,
                        "indicator_bar.single",
                    );
                }
                BarClass::Overlay { mut overlay } => {
                    if !overlay.is_finite() {
                        overlay = 0.0;
                    }

                    let base_color = if overlay >= 0.0 {
                        palette.success.base.color
                    } else {
                        palette.danger.base.color
                    };

                    safe_geometry::fill_rectangle(
                        frame,
                        Point::new(left, top_y),
                        Size::new(bar_width, h_total),
                        base_color.scale_alpha(0.3),
                        "indicator_bar.overlay_base",
                    );

                    let ov_abs = overlay.abs().max(0.0);
                    if ov_abs > 0.0 {
                        let y_overlay = scale.to_y(baseline_value + ov_abs);
                        if !y_overlay.is_finite() {
                            return;
                        }

                        let h_overlay = (y_base - y_overlay).max(0.0);
                        if h_overlay > 0.0 && h_overlay.is_finite() {
                            safe_geometry::fill_rectangle(
                                frame,
                                Point::new(left, y_overlay),
                                Size::new(bar_width, h_overlay),
                                base_color,
                                "indicator_bar.overlay",
                            );
                        }
                    }
                }
            }
        });
    }

    fn tooltip_fn(&self) -> Option<&TooltipFn<S::Y>> {
        self.tooltip.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn y_extents_ignore_non_finite_bar_values() {
        let mut datapoints = BTreeMap::new();
        datapoints.insert(1, f32::INFINITY);
        datapoints.insert(2, 12.0);

        let plot = BarPlot::new(|value: &f32| *value, |_value: &f32| BarClass::Single);

        assert_eq!(plot.y_extents(&&datapoints, 1..=2), Some((0.0, 12.0)));
    }
}
