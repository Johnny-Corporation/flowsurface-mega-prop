use exchange::{
    Timeframe, Trade, UnixMs,
    unit::{Price, PriceStep, qty::Qty},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Duration};

const TRADE_RETENTION_MS: u64 = 8 * 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct Config {
    pub show_spread: bool,
    pub trade_retention: Duration,
    #[serde(default = "default_cluster_timeframe")]
    pub cluster_timeframe: Timeframe,
    #[serde(default = "default_cluster_columns")]
    pub visible_cluster_columns: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_spread: false,
            trade_retention: Duration::from_millis(TRADE_RETENTION_MS),
            cluster_timeframe: default_cluster_timeframe(),
            visible_cluster_columns: default_cluster_columns(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ClusterCell {
    pub buy_qty: Qty,
    pub sell_qty: Qty,
}

impl ClusterCell {
    pub fn total(self) -> Qty {
        self.buy_qty + self.sell_qty
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterColumn {
    pub bucket: UnixMs,
    pub cells: BTreeMap<Price, ClusterCell>,
}

pub fn build_time_clusters(
    trades: &std::collections::VecDeque<Trade>,
    step: PriceStep,
    timeframe: Timeframe,
    max_columns: usize,
) -> Vec<ClusterColumn> {
    if trades.is_empty() || max_columns == 0 {
        return Vec::new();
    }

    let mut by_bucket: BTreeMap<UnixMs, BTreeMap<Price, ClusterCell>> = BTreeMap::new();
    for trade in trades {
        let bucket = trade.time.floor_to(timeframe);
        let price = trade.price.round_to_side_step(trade.is_sell, step);
        let cell = by_bucket
            .entry(bucket)
            .or_default()
            .entry(price)
            .or_default();

        if trade.is_sell {
            cell.sell_qty += trade.qty;
        } else {
            cell.buy_qty += trade.qty;
        }
    }

    let Some(latest_bucket) = by_bucket.keys().next_back().copied() else {
        return Vec::new();
    };

    let first_offset = 1_i64.saturating_sub(max_columns as i64);
    (first_offset..=0)
        .map(|offset| {
            let bucket = latest_bucket.offset_by_timeframe(timeframe, offset);
            let cells = by_bucket.remove(&bucket).unwrap_or_default();
            ClusterColumn { bucket, cells }
        })
        .collect()
}

fn default_cluster_timeframe() -> Timeframe {
    Timeframe::M1
}

fn default_cluster_columns() -> u8 {
    5
}
