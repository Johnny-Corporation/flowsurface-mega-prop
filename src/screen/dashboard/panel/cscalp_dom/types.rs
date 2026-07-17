use super::{COL_PADDING, ROW_HEIGHT};
use data::panel::cscalp_dom::ClusterColumn;
use exchange::unit::{Price, PriceStep, qty::Qty};
use std::cell::Cell;

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
    pub(super) fn price_to_index(&self, price: Price) -> Option<i32> {
        if price >= self.best_ask {
            let steps = Price::steps_between_inclusive(self.best_ask, price, self.tick)?;
            i32::try_from(steps).ok()?.checked_neg()
        } else if price <= self.best_bid {
            let steps = Price::steps_between_inclusive(price, self.best_bid, self.tick)?;
            i32::try_from(steps).ok()
        } else {
            None
        }
    }

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

#[derive(Default)]
pub(super) struct PriceAxisState {
    anchor_price: Cell<Option<Price>>,
}

impl PriceAxisState {
    pub(super) fn reset(&self) {
        self.anchor_price.set(None);
    }

    pub(super) fn anchor_best_bid(&self, current_best_bid: Price) -> Price {
        self.pin_if_needed(current_best_bid);
        self.anchor_price.get().unwrap_or(current_best_bid)
    }

    fn pin_if_needed(&self, current_best_bid: Price) {
        if self.anchor_price.get().is_none() {
            self.anchor_price.set(Some(current_best_bid));
        }
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

#[cfg(test)]
mod tests {
    use super::{PriceAxisState, PriceGrid};
    use exchange::unit::{Price, PriceStep};

    fn grid(best_bid: f32) -> PriceGrid {
        let tick = PriceStep::from_f32(1.0);
        let best_bid = Price::from_f32(best_bid);
        PriceGrid {
            best_bid,
            best_ask: best_bid.add_steps(1, tick),
            tick,
        }
    }

    #[test]
    fn absolute_price_rows_remain_fixed_across_market_moves() {
        let axis = PriceAxisState::default();
        let initial = grid(100.0);
        axis.anchor_best_bid(initial.best_bid);

        let moved_up = grid(axis.anchor_best_bid(Price::from_f32(102.0)).to_f32());
        let moved_down = grid(axis.anchor_best_bid(Price::from_f32(98.0)).to_f32());

        for row_index in -5..=5 {
            assert_eq!(
                moved_up.index_to_price(row_index),
                initial.index_to_price(row_index)
            );
            assert_eq!(
                moved_down.index_to_price(row_index),
                initial.index_to_price(row_index)
            );
        }
    }

    #[test]
    fn reset_recenters_once_then_holds_the_new_anchor() {
        let axis = PriceAxisState::default();
        axis.anchor_best_bid(Price::from_f32(100.0));
        axis.reset();

        assert_eq!(
            axis.anchor_best_bid(Price::from_f32(102.0)),
            Price::from_f32(102.0)
        );
        assert_eq!(
            axis.anchor_best_bid(Price::from_f32(98.0)),
            Price::from_f32(102.0)
        );
    }
}
