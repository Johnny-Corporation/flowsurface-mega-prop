use super::{COL_PADDING, ROW_HEIGHT};
use data::panel::cscalp_dom::ClusterColumn;
use exchange::unit::{Price, PriceStep, qty::Qty};

#[derive(Default)]
pub(super) struct Maxima {
    pub(super) vis_max_order_qty: f32,
}

#[derive(Clone, Copy)]
pub(super) struct LastPrintMarker {
    pub(super) price: Price,
    pub(super) is_sell: bool,
}

pub(super) struct VisibleRow {
    pub(super) row: DomRow,
    pub(super) y: f32,
}

pub(super) struct ColumnRanges {
    pub(super) clusters: (f32, f32),
    pub(super) prints: (f32, f32),
    pub(super) orderbook: (f32, f32),
    pub(super) order_qty: (f32, f32),
    pub(super) price: (f32, f32),
}

pub(super) struct PriceLayout {
    pub(super) price_px: f32,
}

pub(super) enum DomRow {
    Ask { price: Price, qty: Qty },
    Spread,
    CenterDivider,
    Bid { price: Price, qty: Qty },
}

impl DomRow {
    pub(super) fn price(&self) -> Option<Price> {
        match self {
            DomRow::Ask { price, .. } | DomRow::Bid { price, .. } => Some(*price),
            DomRow::Spread | DomRow::CenterDivider => None,
        }
    }
}

pub(super) struct PriceGrid {
    pub(super) best_bid: Price,
    pub(super) best_ask: Price,
    pub(super) tick: PriceStep,
}

impl PriceGrid {
    pub(super) fn index_to_price(&self, idx: i32) -> Option<Price> {
        if idx == 0 {
            return None;
        }
        if idx > 0 {
            let off = (idx - 1) as i64;
            Some(self.best_bid.add_steps(-off, self.tick))
        } else {
            let off = (-1 - idx) as i64;
            Some(self.best_ask.add_steps(off, self.tick))
        }
    }

    pub(super) fn top_y(idx: i32) -> f32 {
        (idx as f32) * ROW_HEIGHT - ROW_HEIGHT * 0.5
    }
}

pub(super) fn cluster_totals(cluster: &ClusterColumn) -> (Qty, Qty) {
    let mut buy = Qty::ZERO;
    let mut sell = Qty::ZERO;
    for cell in cluster.cells.values() {
        buy += cell.buy_qty;
        sell += cell.sell_qty;
    }
    (buy, sell)
}

pub(super) fn cluster_column_geometry(range: (f32, f32), count: usize) -> Option<(f32, f32)> {
    if count == 0 {
        return None;
    }
    let width = (range.1 - range.0).max(0.0);
    if width <= 0.0 {
        return None;
    }
    let col_width = width / count as f32;
    if col_width <= COL_PADDING {
        return None;
    }
    Some((range.0, col_width))
}

#[derive(Clone, Copy)]
pub(super) struct LabelBox {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl LabelBox {
    pub(super) fn overlaps(self, other: Self) -> bool {
        self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }
}

pub(super) fn text_box(
    center_x: f32,
    center_y: f32,
    chars: usize,
    text_size: f32,
    pad: f32,
) -> LabelBox {
    let width = chars as f32 * text_size * 0.62 + pad * 2.0;
    let height = text_size + pad * 2.0;
    LabelBox {
        left: center_x - width * 0.5,
        top: center_y - height * 0.5,
        right: center_x + width * 0.5,
        bottom: center_y + height * 0.5,
    }
}
