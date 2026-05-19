use super::{
    COL_PADDING, CscalpDom, MIN_CLUSTER_SECTION_WIDTH, MIN_PRINTS_SECTION_WIDTH, MONO_CHAR_ADVANCE,
    ORDER_QTY_MAX_WIDTH, ORDER_QTY_MIN_WIDTH, PRICE_TEXT_SIDE_PAD_MIN, SECTION_DIVIDER_HIT_SLOP,
    types::{ColumnRanges, PriceGrid, PriceLayout},
};
use crate::screen::dashboard::panel::SectionDivider;
use std::time::Instant;

#[derive(Default)]
pub struct SectionDragState {
    pub(super) dragging: Option<SectionDivider>,
}

impl CscalpDom {
    pub(super) fn price_layout_for(&self, total_width: f32, grid: &PriceGrid) -> PriceLayout {
        let sample = self.price_sample_text(grid);
        let text_px = Self::mono_text_width_px(sample.len());
        let price_px = (text_px + 2.0 * PRICE_TEXT_SIDE_PAD_MIN).min(total_width.max(0.0));
        PriceLayout { price_px }
    }

    pub(super) fn column_ranges(&self, width: f32, price_px: f32) -> ColumnRanges {
        let width = width.max(0.0);
        let price_width = price_px.min(width);
        let (cluster_end, orderbook_start) = self.constrained_section_pixels(width, price_width);
        let orderbook_width = (width - orderbook_start).max(0.0);

        let order_qty_width = (orderbook_width - price_width).max(0.0);
        let order_qty = (orderbook_start, orderbook_start + order_qty_width);
        let price = (order_qty.1, width);
        let orderbook = (orderbook_start, width);

        let clusters = (0.0, cluster_end);
        let prints = (
            (cluster_end + COL_PADDING).min(orderbook_start),
            (orderbook_start - COL_PADDING).max(cluster_end),
        );

        ColumnRanges {
            clusters,
            prints,
            orderbook,
            order_qty,
            price,
        }
    }

    pub(super) fn hit_section_divider(&self, width: f32, cursor_x: f32) -> Option<SectionDivider> {
        let grid = self.build_price_grid()?;
        let layout = self.price_layout_for(width, &grid);
        let cols = self.column_ranges(width, layout.price_px);

        if (cursor_x - cols.clusters.1).abs() <= SECTION_DIVIDER_HIT_SLOP {
            Some(SectionDivider::First)
        } else if (cursor_x - cols.orderbook.0).abs() <= SECTION_DIVIDER_HIT_SLOP {
            Some(SectionDivider::Second)
        } else {
            None
        }
    }

    pub(super) fn drag_section_split(
        &mut self,
        divider: SectionDivider,
        cursor_x: f32,
        width: f32,
    ) {
        let Some(grid) = self.build_price_grid() else {
            return;
        };
        let layout = self.price_layout_for(width, &grid);
        let width = width.max(1.0);

        let mut cluster_ratio = self.config.cluster_split_ratio;
        let mut orderbook_ratio = self.config.orderbook_split_ratio;

        match divider {
            SectionDivider::First => cluster_ratio = cursor_x / width,
            SectionDivider::Second => orderbook_ratio = cursor_x / width,
        }

        let (cluster_px, orderbook_px) =
            constrained_section_pixels(width, layout.price_px, cluster_ratio, orderbook_ratio);

        self.config.cluster_split_ratio = cluster_px / width;
        self.config.orderbook_split_ratio = orderbook_px / width;
        self.invalidate(Some(Instant::now()));
    }

    fn price_sample_text(&self, grid: &PriceGrid) -> String {
        let a = self.format_price(grid.best_ask);
        let b = self.format_price(grid.best_bid);
        if a.len() >= b.len() { a } else { b }
    }

    fn mono_text_width_px(text_len: usize) -> f32 {
        (text_len as f32) * super::TEXT_SIZE * MONO_CHAR_ADVANCE
    }

    fn constrained_section_pixels(&self, width: f32, price_width: f32) -> (f32, f32) {
        constrained_section_pixels(
            width,
            price_width,
            self.config.cluster_split_ratio,
            self.config.orderbook_split_ratio,
        )
    }
}

fn constrained_section_pixels(
    width: f32,
    price_width: f32,
    cluster_ratio: f32,
    orderbook_ratio: f32,
) -> (f32, f32) {
    let width = width.max(1.0);
    let (min_cluster, min_prints, min_orderbook, gap) = scaled_min_widths(width, price_width);

    let min_orderbook_start = min_cluster + min_prints + gap * 2.0;
    let max_orderbook_start = (width - min_orderbook).max(min_orderbook_start);
    let orderbook_start =
        (orderbook_ratio.clamp(0.0, 1.0) * width).clamp(min_orderbook_start, max_orderbook_start);

    let min_cluster_end = min_cluster;
    let max_cluster_end = (orderbook_start - min_prints - gap * 2.0).max(min_cluster_end);
    let cluster_end =
        (cluster_ratio.clamp(0.0, 1.0) * width).clamp(min_cluster_end, max_cluster_end);

    (cluster_end, orderbook_start)
}

fn scaled_min_widths(width: f32, price_width: f32) -> (f32, f32, f32, f32) {
    let target_order_qty = (width * 0.10).clamp(ORDER_QTY_MIN_WIDTH, ORDER_QTY_MAX_WIDTH);
    let min_orderbook = (price_width + target_order_qty).min(width);
    let min_total =
        MIN_CLUSTER_SECTION_WIDTH + MIN_PRINTS_SECTION_WIDTH + min_orderbook + COL_PADDING * 2.0;

    if min_total <= width {
        (
            MIN_CLUSTER_SECTION_WIDTH,
            MIN_PRINTS_SECTION_WIDTH,
            min_orderbook,
            COL_PADDING,
        )
    } else {
        let scale = (width / min_total).clamp(0.0, 1.0);
        (
            MIN_CLUSTER_SECTION_WIDTH * scale,
            MIN_PRINTS_SECTION_WIDTH * scale,
            min_orderbook * scale,
            COL_PADDING * scale,
        )
    }
}
