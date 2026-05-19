use super::Message;
use crate::style;
use data::panel::{
    cscalp_dom::{ClusterCell, ClusterColumn, Config, build_time_clusters},
    ladder::{GroupedDepth, Side, TradeStore},
};
use exchange::Trade;
use exchange::unit::qty::Qty;
use exchange::unit::{Price, PriceStep};
use exchange::{TickerInfo, UnixMs, depth::Depth};

use iced::widget::canvas::{self, Path, Stroke, Text};
use iced::{Alignment, Event, Point, Rectangle, Renderer, Size, Theme, mouse};

use std::collections::BTreeMap;
use std::time::Instant;

mod prints;
mod types;
use types::{
    ColumnRanges, DomRow, LastPrintMarker, Maxima, PriceGrid, PriceLayout, VisibleRow,
    cluster_column_geometry, cluster_totals,
};

const TEXT_SIZE: f32 = style::text_size::SMALL;
const ROW_HEIGHT: f32 = 16.0;
const COL_PADDING: f32 = 4.0;
const CLUSTER_CELL_GAP: f32 = 1.0;

const CLUSTERS_COL_WEIGHT: f32 = 0.54;
const PRINTS_COL_WEIGHT: f32 = 0.46;

const MONO_CHAR_ADVANCE: f32 = 0.62;
const PRICE_TEXT_SIDE_PAD_MIN: f32 = 10.0;
const ORDER_QTY_MIN_WIDTH: f32 = 54.0;
const ORDER_QTY_MAX_WIDTH: f32 = 92.0;

const RECENT_PRINT_LIMIT: usize = 64;
const PRINT_BUBBLE_MIN_RADIUS: f32 = 4.0;
const PRINT_BUBBLE_MAX_RADIUS: f32 = 18.0;
const PRINT_LABEL_MIN_RADIUS: f32 = 10.5;
const PRINT_LABEL_MAX_COUNT: usize = 10;
const CLUSTER_FOOTER_ROWS: f32 = 3.0;

impl super::Panel for CscalpDom {
    fn scroll(&mut self, delta: f32) {
        self.scroll_px += delta;
        CscalpDom::invalidate(self, Some(Instant::now()));
    }

    fn reset_scroll(&mut self) {
        self.scroll_px = 0.0;
        CscalpDom::invalidate(self, Some(Instant::now()));
    }

    fn invalidate(&mut self, now: Option<Instant>) -> Option<super::Action> {
        CscalpDom::invalidate(self, now)
    }

    fn is_empty(&self) -> bool {
        if self.pending_tick_size.is_some() {
            return true;
        }
        self.grouped_asks().is_empty() && self.grouped_bids().is_empty() && self.trades.is_empty()
    }
}

pub struct CscalpDom {
    ticker_info: TickerInfo,
    pub config: Config,
    cache: canvas::Cache,
    last_tick: Instant,
    pub step: PriceStep,
    scroll_px: f32,
    orderbook: [GroupedDepth; 2],
    trades: TradeStore,
    pending_tick_size: Option<PriceStep>,
    raw_price_spread: Option<Price>,
    last_exchange_ts_ms: Option<UnixMs>,
}

impl CscalpDom {
    pub fn new(config: Option<Config>, ticker_info: TickerInfo, step: PriceStep) -> Self {
        Self {
            trades: TradeStore::new(),
            config: config.unwrap_or_default(),
            ticker_info,
            cache: canvas::Cache::default(),
            last_tick: Instant::now(),
            step,
            scroll_px: 0.0,
            orderbook: [GroupedDepth::new(), GroupedDepth::new()],
            raw_price_spread: None,
            pending_tick_size: None,
            last_exchange_ts_ms: None,
        }
    }

    pub fn insert_trades(&mut self, buffer: &[Trade]) {
        self.trades.insert_trades(buffer, self.step);
    }

    pub fn insert_depth(&mut self, depth: &Depth, update_t: UnixMs) {
        if let Some(next) = self.pending_tick_size.take() {
            self.step = next;
            self.trades.rebuild_grouped(self.step);
        }

        let raw_best_bid = depth.bids.last_key_value().map(|(p, _)| *p);
        let raw_best_ask = depth.asks.first_key_value().map(|(p, _)| *p);
        self.raw_price_spread = match (raw_best_bid, raw_best_ask) {
            (Some(bid), Some(ask)) => Some(ask - bid),
            _ => None,
        };

        if self
            .trades
            .maybe_cleanup(update_t, self.config.trade_retention, self.step)
        {
            self.invalidate(Some(Instant::now()));
        }

        self.regroup_from_depth(depth);
        self.last_exchange_ts_ms = Some(update_t);
    }

    pub fn set_tick_size(&mut self, step: PriceStep) {
        self.pending_tick_size = Some(step);
        self.invalidate(Some(Instant::now()));
    }

    pub fn last_update(&self) -> Instant {
        self.last_tick
    }

    pub fn invalidate(&mut self, now: Option<Instant>) -> Option<super::Action> {
        let ts = self.last_exchange_ts_ms.unwrap_or_else(UnixMs::now);
        self.trades
            .maybe_cleanup(ts, self.config.trade_retention, self.step);

        self.cache.clear();
        if let Some(now) = now {
            self.last_tick = now;
        }
        None
    }

    fn grouped_asks(&self) -> &BTreeMap<Price, Qty> {
        &self.orderbook[Side::Ask.idx()].orders
    }

    fn grouped_bids(&self) -> &BTreeMap<Price, Qty> {
        &self.orderbook[Side::Bid.idx()].orders
    }

    fn best_price(&self, side: Side) -> Option<Price> {
        self.orderbook[side.idx()].best_price(side)
    }

    fn regroup_from_depth(&mut self, depth: &Depth) {
        let step = self.step;
        self.orderbook[Side::Ask.idx()].regroup_from_raw(&depth.asks, Side::Ask, step);
        self.orderbook[Side::Bid.idx()].regroup_from_raw(&depth.bids, Side::Bid, step);
    }

    fn format_price(&self, price: Price) -> String {
        price.to_string(self.ticker_info.min_ticksize)
    }

    fn format_quantity(&self, qty: Qty) -> String {
        data::util::abbr_large_numbers(qty.to_f32_lossy())
    }

    fn cluster_columns(&self) -> Vec<ClusterColumn> {
        build_time_clusters(
            &self.trades.raw,
            self.step,
            self.config.cluster_timeframe,
            self.config.visible_cluster_columns.max(1) as usize,
        )
    }
}

impl canvas::Program<Message> for CscalpDom {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &iced::Event,
        bounds: iced::Rectangle,
        cursor: iced_core::mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let _cursor_position = cursor.position_in(bounds)?;

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Middle | mouse::Button::Left | mouse::Button::Right,
            )) => Some(canvas::Action::publish(Message::ResetScroll).and_capture()),
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let scroll_amount = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => -(*y) * ROW_HEIGHT,
                    mouse::ScrollDelta::Pixels { y, .. } => -*y,
                };

                Some(canvas::Action::publish(Message::Scrolled(scroll_amount)).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: iced_core::mouse::Cursor,
    ) -> Vec<iced::widget::canvas::Geometry<Renderer>> {
        let palette = theme.extended_palette();

        let text_color = palette.background.base.text;
        let muted_text_color = text_color.scale_alpha(0.64);
        let bid_color = palette.success.base.color;
        let ask_color = palette.danger.base.color;
        let divider_color = style::split_ruler(theme).color;
        let footer_bg = palette.background.base.color;

        let visual = self.cache.draw(renderer, bounds.size(), |frame| {
            if let Some(grid) = self.build_price_grid() {
                let layout = self.price_layout_for(bounds.width, &grid);
                let cols = self.column_ranges(bounds.width, layout.price_px);
                let (visible_rows, maxima) = self.visible_rows(bounds, &grid);
                let clusters = self.cluster_columns();
                let cluster_max_qty = self.visible_cluster_max_qty(&clusters, &visible_rows);
                let last_print = self.last_print_marker(&grid);

                let mut spread_row: Option<(f32, f32)> = None;

                for visible_row in visible_rows.iter() {
                    self.draw_row_guides(
                        frame,
                        visible_row.y,
                        bounds.width,
                        divider_color.scale_alpha(0.28),
                    );

                    match visible_row.row {
                        DomRow::Ask { price, qty } => {
                            self.draw_cluster_grid_row(
                                frame,
                                visible_row.y,
                                &clusters,
                                divider_color,
                                &cols,
                            );
                            self.draw_cluster_cells(
                                frame,
                                visible_row.y,
                                price,
                                &clusters,
                                cluster_max_qty,
                                bid_color,
                                ask_color,
                                text_color,
                                &cols,
                            );
                            self.draw_orderbook_row(
                                frame,
                                visible_row.y,
                                price,
                                qty,
                                ask_color,
                                text_color,
                                maxima.vis_max_order_qty,
                                last_print,
                                &cols,
                            );
                        }
                        DomRow::Bid { price, qty } => {
                            self.draw_cluster_grid_row(
                                frame,
                                visible_row.y,
                                &clusters,
                                divider_color,
                                &cols,
                            );
                            self.draw_cluster_cells(
                                frame,
                                visible_row.y,
                                price,
                                &clusters,
                                cluster_max_qty,
                                bid_color,
                                ask_color,
                                text_color,
                                &cols,
                            );
                            self.draw_orderbook_row(
                                frame,
                                visible_row.y,
                                price,
                                qty,
                                bid_color,
                                text_color,
                                maxima.vis_max_order_qty,
                                last_print,
                                &cols,
                            );
                        }
                        DomRow::Spread => {
                            if let Some(spread) = self.raw_price_spread {
                                spread_row = Some((visible_row.y, visible_row.y + ROW_HEIGHT));
                                let spread =
                                    spread.round_to_min_tick(self.ticker_info.min_ticksize);
                                let content = format!(
                                    "Spread {}",
                                    spread.to_string(self.ticker_info.min_ticksize)
                                );
                                frame.fill_text(Text {
                                    content,
                                    position: Point::new(
                                        cols.prints.0 + (cols.prints.1 - cols.prints.0) * 0.5,
                                        visible_row.y + ROW_HEIGHT / 2.0,
                                    ),
                                    color: palette.secondary.strong.color,
                                    size: style::text_size::TINY.into(),
                                    font: style::AZERET_MONO,
                                    align_x: Alignment::Center.into(),
                                    align_y: Alignment::Center.into(),
                                    ..Default::default()
                                });
                            }
                        }
                        DomRow::CenterDivider => {
                            let y_mid = visible_row.y + ROW_HEIGHT / 2.0 - 0.5;
                            frame.fill_rectangle(
                                Point::new(0.0, y_mid),
                                Size::new(bounds.width, 1.0),
                                divider_color,
                            );
                        }
                    }
                }

                self.draw_recent_prints(
                    frame, &grid, bounds, &cols, bid_color, ask_color, text_color,
                );

                self.draw_cluster_footer(
                    frame,
                    bounds,
                    &clusters,
                    bid_color,
                    ask_color,
                    text_color,
                    muted_text_color,
                    footer_bg,
                    divider_color,
                    &cols,
                );

                self.draw_vertical_splits(frame, bounds, &cols, divider_color, spread_row);
            }
        });

        vec![visual]
    }
}

impl CscalpDom {
    fn price_sample_text(&self, grid: &PriceGrid) -> String {
        let a = self.format_price(grid.best_ask);
        let b = self.format_price(grid.best_bid);
        if a.len() >= b.len() { a } else { b }
    }

    fn mono_text_width_px(text_len: usize) -> f32 {
        (text_len as f32) * TEXT_SIZE * MONO_CHAR_ADVANCE
    }

    fn price_layout_for(&self, total_width: f32, grid: &PriceGrid) -> PriceLayout {
        let sample = self.price_sample_text(grid);
        let text_px = Self::mono_text_width_px(sample.len());
        let price_px = (text_px + 2.0 * PRICE_TEXT_SIDE_PAD_MIN).min(total_width.max(0.0));
        PriceLayout { price_px }
    }

    fn column_ranges(&self, width: f32, price_px: f32) -> ColumnRanges {
        let width = width.max(0.0);
        let price_width = price_px.min(width);
        let target_order_qty = (width * 0.10).clamp(ORDER_QTY_MIN_WIDTH, ORDER_QTY_MAX_WIDTH);
        let orderbook_width = (price_width + target_order_qty).min(width);
        let orderbook_start = width - orderbook_width;

        let order_qty_width = (orderbook_width - price_width).max(0.0);
        let order_qty = (orderbook_start, orderbook_start + order_qty_width);
        let price = (order_qty.1, width);
        let orderbook = (orderbook_start, width);

        let content_width = (orderbook_start - COL_PADDING * 2.0).max(0.0);
        let total_weight = CLUSTERS_COL_WEIGHT + PRINTS_COL_WEIGHT;
        let clusters_width = if total_weight > 0.0 {
            content_width * (CLUSTERS_COL_WEIGHT / total_weight)
        } else {
            0.0
        };
        let clusters = (0.0, clusters_width);
        let prints = (
            clusters.1 + COL_PADDING,
            (clusters.1 + COL_PADDING + (content_width - clusters_width).max(0.0))
                .min(orderbook_start - COL_PADDING),
        );

        ColumnRanges {
            clusters,
            prints,
            orderbook,
            order_qty,
            price,
        }
    }

    fn draw_row_guides(
        &self,
        frame: &mut iced::widget::canvas::Frame,
        y: f32,
        width: f32,
        color: iced::Color,
    ) {
        frame.fill_rectangle(Point::new(0.0, y), Size::new(width, 1.0), color);
    }

    fn draw_orderbook_row(
        &self,
        frame: &mut iced::widget::canvas::Frame,
        y: f32,
        price: Price,
        order_qty: Qty,
        side_color: iced::Color,
        text_color: iced::Color,
        max_order_qty: f32,
        last_print: Option<LastPrintMarker>,
        cols: &ColumnRanges,
    ) {
        let row_color = match last_print {
            Some(marker) if marker.price == price => {
                if marker.is_sell {
                    side_color.scale_alpha(0.44)
                } else {
                    side_color.scale_alpha(0.42)
                }
            }
            _ => side_color.scale_alpha(0.13),
        };

        frame.fill_rectangle(
            Point::new(cols.price.0, y),
            Size::new((cols.price.1 - cols.price.0).max(0.0), ROW_HEIGHT),
            row_color,
        );

        let order_qty_f32 = f32::from(order_qty);
        Self::fill_bar(
            frame,
            cols.order_qty,
            y,
            ROW_HEIGHT,
            order_qty_f32,
            max_order_qty,
            side_color,
            false,
            0.26,
        );

        if order_qty_f32 > 0.0 {
            let qty_txt = self.format_quantity(order_qty);
            Self::draw_cell_text(
                frame,
                &qty_txt,
                cols.order_qty.1 - 5.0,
                y,
                text_color,
                Alignment::End,
            );
        }

        let price_text = self.format_price(price);
        Self::draw_cell_text(
            frame,
            &price_text,
            cols.price.1 - 5.0,
            y,
            text_color,
            Alignment::End,
        );
    }

    fn draw_cluster_cells(
        &self,
        frame: &mut iced::widget::canvas::Frame,
        y: f32,
        price: Price,
        clusters: &[ClusterColumn],
        max_cluster_qty: f32,
        bid_color: iced::Color,
        ask_color: iced::Color,
        text_color: iced::Color,
        cols: &ColumnRanges,
    ) {
        if clusters.is_empty() || max_cluster_qty <= 0.0 {
            return;
        }

        let Some((first_x, col_width)) = cluster_column_geometry(cols.clusters, clusters.len())
        else {
            return;
        };

        for (idx, cluster) in clusters.iter().enumerate() {
            let Some(cell) = cluster.cells.get(&price).copied() else {
                continue;
            };
            let total = f32::from(cell.total());
            if total <= 0.0 {
                continue;
            }

            let x = first_x + idx as f32 * col_width;
            self.draw_cluster_cell(
                frame,
                (x, x + col_width - CLUSTER_CELL_GAP),
                y,
                cell,
                max_cluster_qty,
                bid_color,
                ask_color,
                text_color,
            );
        }
    }

    fn draw_cluster_grid_row(
        &self,
        frame: &mut iced::widget::canvas::Frame,
        y: f32,
        clusters: &[ClusterColumn],
        divider_color: iced::Color,
        cols: &ColumnRanges,
    ) {
        if clusters.is_empty() {
            return;
        }

        let Some((first_x, col_width)) = cluster_column_geometry(cols.clusters, clusters.len())
        else {
            return;
        };

        for idx in 0..clusters.len() {
            let x = first_x + idx as f32 * col_width;
            let x_end = x + col_width - CLUSTER_CELL_GAP;
            if x_end <= x {
                continue;
            }

            frame.fill_rectangle(
                Point::new(x, y + 1.0),
                Size::new((x_end - x).max(0.0), (ROW_HEIGHT - 2.0).max(0.0)),
                divider_color.scale_alpha(0.045),
            );

            let outline = Path::rectangle(
                Point::new(x.floor() + 0.5, y.floor() + 0.5),
                Size::new((x_end - x).max(0.0), (ROW_HEIGHT - 1.0).max(0.0)),
            );
            frame.stroke(
                &outline,
                Stroke::default()
                    .with_color(divider_color.scale_alpha(0.20))
                    .with_width(1.0),
            );

            frame.fill_rectangle(
                Point::new(x.floor() + 0.5, y),
                Size::new(1.0, ROW_HEIGHT),
                divider_color.scale_alpha(0.42),
            );
            frame.fill_rectangle(
                Point::new(x, y.floor() + 0.5),
                Size::new((x_end - x).max(0.0), 1.0),
                divider_color.scale_alpha(0.20),
            );
        }
    }

    fn draw_cluster_cell(
        &self,
        frame: &mut iced::widget::canvas::Frame,
        (x_start, x_end): (f32, f32),
        y: f32,
        cell: ClusterCell,
        max_cluster_qty: f32,
        bid_color: iced::Color,
        ask_color: iced::Color,
        text_color: iced::Color,
    ) {
        let total = f32::from(cell.total());
        if total <= 0.0 || max_cluster_qty <= 0.0 {
            return;
        }

        let sell = f32::from(cell.sell_qty);
        let buy = f32::from(cell.buy_qty);
        let dominant = if sell > buy { ask_color } else { bid_color };
        let intensity = (total / max_cluster_qty).clamp(0.0, 1.0);
        let cell_width = (x_end - x_start).max(0.0);
        let inner_x = x_start + 1.0;
        let inner_y = y + 1.0;
        let inner_w = (cell_width - 2.0).max(0.0);
        let inner_h = (ROW_HEIGHT - 2.0).max(0.0);
        if inner_w <= 0.0 || inner_h <= 0.0 {
            return;
        }
        let fill_w = (inner_w * intensity).max(2.0).min(inner_w);

        frame.fill_rectangle(
            Point::new(inner_x, inner_y),
            Size::new(inner_w, inner_h),
            dominant.scale_alpha(0.06),
        );

        frame.fill_rectangle(
            Point::new(inner_x, inner_y),
            Size::new(fill_w, inner_h),
            dominant.scale_alpha(0.28 + intensity * 0.36),
        );

        let outline = Path::rectangle(Point::new(inner_x, inner_y), Size::new(inner_w, inner_h));
        frame.stroke(
            &outline,
            Stroke::default()
                .with_color(dominant.scale_alpha(0.35 + intensity * 0.35))
                .with_width(1.0),
        );

        if buy > 0.0 && sell > 0.0 {
            let other_side = if sell > buy { bid_color } else { ask_color };
            frame.fill_rectangle(
                Point::new(inner_x, inner_y + inner_h - 2.0),
                Size::new(fill_w, 2.0),
                other_side.scale_alpha(0.58),
            );
        }

        let qty_txt = self.format_quantity(cell.total());
        let label_width = qty_txt.chars().count() as f32 * TEXT_SIZE * MONO_CHAR_ADVANCE + 8.0;
        if x_end - x_start >= label_width {
            Self::draw_cell_text(
                frame,
                &qty_txt,
                x_end - 4.0,
                y,
                text_color.scale_alpha(0.88),
                Alignment::End,
            );
        }
    }

    fn draw_cluster_footer(
        &self,
        frame: &mut iced::widget::canvas::Frame,
        bounds: Rectangle,
        clusters: &[ClusterColumn],
        bid_color: iced::Color,
        ask_color: iced::Color,
        text_color: iced::Color,
        muted_text_color: iced::Color,
        footer_bg: iced::Color,
        divider_color: iced::Color,
        cols: &ColumnRanges,
    ) {
        if clusters.is_empty() {
            return;
        }
        let footer_h = ROW_HEIGHT * CLUSTER_FOOTER_ROWS;
        if bounds.height <= footer_h {
            return;
        }

        let Some((first_x, col_width)) = cluster_column_geometry(cols.clusters, clusters.len())
        else {
            return;
        };

        let footer_y = bounds.height - footer_h;
        frame.fill_rectangle(
            Point::new(cols.clusters.0, footer_y),
            Size::new((cols.clusters.1 - cols.clusters.0).max(0.0), footer_h),
            footer_bg.scale_alpha(0.94),
        );
        frame.fill_rectangle(
            Point::new(cols.clusters.0, footer_y),
            Size::new((cols.clusters.1 - cols.clusters.0).max(0.0), 1.0),
            divider_color,
        );

        for (idx, cluster) in clusters.iter().enumerate() {
            let x = first_x + idx as f32 * col_width;
            let x_end = x + col_width - CLUSTER_CELL_GAP;
            let (buy, sell) = cluster_totals(cluster);
            let total = buy + sell;
            let delta = buy - sell;

            Self::draw_cell_text(
                frame,
                &self.format_quantity(total),
                x_end - 4.0,
                footer_y,
                text_color,
                Alignment::End,
            );

            let delta_color = if delta.units >= 0 {
                bid_color
            } else {
                ask_color
            };
            Self::draw_cell_text(
                frame,
                &self.format_quantity(delta),
                x_end - 4.0,
                footer_y + ROW_HEIGHT,
                delta_color,
                Alignment::End,
            );

            let label = cluster
                .bucket
                .format_utc("%M:%S")
                .unwrap_or_else(|| "--:--".to_string());
            Self::draw_cell_text(
                frame,
                &label,
                x_end - 4.0,
                footer_y + ROW_HEIGHT * 2.0,
                muted_text_color,
                Alignment::End,
            );
        }
    }

    fn draw_vertical_splits(
        &self,
        frame: &mut iced::widget::canvas::Frame,
        bounds: Rectangle,
        cols: &ColumnRanges,
        divider_color: iced::Color,
        spread_row: Option<(f32, f32)>,
    ) {
        let mut draw_vsplit = |x: f32, gap: Option<(f32, f32)>| {
            let x = x.floor() + 0.5;
            match gap {
                Some((top, bottom)) => {
                    if top > 0.0 {
                        frame.fill_rectangle(
                            Point::new(x, 0.0),
                            Size::new(1.0, top.max(0.0)),
                            divider_color,
                        );
                    }
                    if bottom < bounds.height {
                        frame.fill_rectangle(
                            Point::new(x, bottom),
                            Size::new(1.0, (bounds.height - bottom).max(0.0)),
                            divider_color,
                        );
                    }
                }
                None => {
                    frame.fill_rectangle(
                        Point::new(x, 0.0),
                        Size::new(1.0, bounds.height),
                        divider_color,
                    );
                }
            }
        };

        draw_vsplit(cols.clusters.1, spread_row);
        draw_vsplit(cols.prints.1, spread_row);
        draw_vsplit(cols.orderbook.0, spread_row);
        draw_vsplit(cols.price.0, spread_row);

        if let Some((top, bottom)) = spread_row {
            frame.fill_rectangle(
                Point::new(0.0, top.floor() + 0.5),
                Size::new(bounds.width, 1.0),
                divider_color,
            );
            frame.fill_rectangle(
                Point::new(0.0, bottom.floor() + 0.5),
                Size::new(bounds.width, 1.0),
                divider_color,
            );
        }
    }

    fn fill_bar(
        frame: &mut iced::widget::canvas::Frame,
        (x_start, x_end): (f32, f32),
        y: f32,
        height: f32,
        value: f32,
        scale_value_max: f32,
        color: iced::Color,
        from_left: bool,
        alpha: f32,
    ) {
        if scale_value_max <= 0.0 || value <= 0.0 {
            return;
        }
        let col_width = x_end - x_start;
        let mut bar_width = (value / scale_value_max) * col_width.max(1.0);
        bar_width = bar_width.min(col_width);
        let bar_x = if from_left {
            x_start
        } else {
            x_end - bar_width
        };

        frame.fill_rectangle(
            Point::new(bar_x, y),
            Size::new(bar_width, height),
            color.scale_alpha(alpha),
        );
    }

    fn draw_cell_text(
        frame: &mut iced::widget::canvas::Frame,
        text: &str,
        x_anchor: f32,
        y: f32,
        color: iced::Color,
        align: Alignment,
    ) {
        frame.fill_text(Text {
            content: text.to_string(),
            position: Point::new(x_anchor, y + ROW_HEIGHT / 2.0),
            color,
            size: TEXT_SIZE.into(),
            font: style::AZERET_MONO,
            align_x: align.into(),
            align_y: Alignment::Center.into(),
            ..Default::default()
        });
    }

    fn visible_cluster_max_qty(
        &self,
        clusters: &[ClusterColumn],
        visible_rows: &[VisibleRow],
    ) -> f32 {
        let mut max_qty = 0.0_f32;
        for row in visible_rows {
            let Some(price) = row.row.price() else {
                continue;
            };
            for cluster in clusters {
                if let Some(cell) = cluster.cells.get(&price) {
                    max_qty = max_qty.max(f32::from(cell.total()));
                }
            }
        }
        max_qty
    }

    fn last_print_marker(&self, grid: &PriceGrid) -> Option<LastPrintMarker> {
        self.trades.raw.back().map(|trade| LastPrintMarker {
            price: trade.price.round_to_side_step(trade.is_sell, grid.tick),
            is_sell: trade.is_sell,
        })
    }

    fn build_price_grid(&self) -> Option<PriceGrid> {
        let best_bid = match (self.best_price(Side::Bid), self.best_price(Side::Ask)) {
            (Some(bb), _) => bb,
            (None, Some(ba)) => ba.add_steps(-1, self.step),
            (None, None) => {
                let (min_t, max_t) = self.trades.price_range()?;
                let steps = Price::steps_between_inclusive(min_t, max_t, self.step).unwrap_or(1);
                max_t.add_steps(-(steps as i64 / 2), self.step)
            }
        };
        let best_ask = best_bid.add_steps(1, self.step);

        Some(PriceGrid {
            best_bid,
            best_ask,
            tick: self.step,
        })
    }

    fn visible_rows(&self, bounds: Rectangle, grid: &PriceGrid) -> (Vec<VisibleRow>, Maxima) {
        let asks_grouped = self.grouped_asks();
        let bids_grouped = self.grouped_bids();

        let mut visible: Vec<VisibleRow> = Vec::new();
        let mut maxima = Maxima::default();

        let mid_screen_y = bounds.height * 0.5;
        let scroll = self.scroll_px;

        let y0 = mid_screen_y + PriceGrid::top_y(0) - scroll;
        let idx_top = ((0.0 - y0) / ROW_HEIGHT).floor() as i32;

        let rows_needed = (bounds.height / ROW_HEIGHT).ceil() as i32 + 1;
        let idx_bottom = idx_top + rows_needed;

        for idx in idx_top..=idx_bottom {
            if idx == 0 {
                let top_y_screen = mid_screen_y + PriceGrid::top_y(0) - scroll;
                if top_y_screen < bounds.height && top_y_screen + ROW_HEIGHT > 0.0 {
                    let row = if self.config.show_spread
                        && self.ticker_info.exchange().is_depth_client_aggr()
                    {
                        DomRow::Spread
                    } else {
                        DomRow::CenterDivider
                    };

                    visible.push(VisibleRow {
                        row,
                        y: top_y_screen,
                    });
                }
                continue;
            }

            let Some(price) = grid.index_to_price(idx) else {
                continue;
            };

            let is_bid = idx > 0;
            let order_qty = if is_bid {
                bids_grouped.get(&price).copied().unwrap_or_default()
            } else {
                asks_grouped.get(&price).copied().unwrap_or_default()
            };

            let top_y_screen = mid_screen_y + PriceGrid::top_y(idx) - scroll;
            if top_y_screen >= bounds.height || top_y_screen + ROW_HEIGHT <= 0.0 {
                continue;
            }

            maxima.vis_max_order_qty = maxima.vis_max_order_qty.max(f32::from(order_qty));
            let row = if is_bid {
                DomRow::Bid {
                    price,
                    qty: order_qty,
                }
            } else {
                DomRow::Ask {
                    price,
                    qty: order_qty,
                }
            };

            visible.push(VisibleRow {
                row,
                y: top_y_screen,
            });
        }

        visible.sort_by(|a, b| a.y.total_cmp(&b.y));
        (visible, maxima)
    }

    fn price_to_screen_y(&self, price: Price, grid: &PriceGrid, bounds_height: f32) -> Option<f32> {
        let mid_screen_y = bounds_height * 0.5;
        let scroll = self.scroll_px;

        let idx = if price >= grid.best_ask {
            let steps = Price::steps_between_inclusive(grid.best_ask, price, grid.tick)?;
            -(steps as i32)
        } else if price <= grid.best_bid {
            let steps = Price::steps_between_inclusive(price, grid.best_bid, grid.tick)?;
            steps as i32
        } else {
            return Some(mid_screen_y - scroll);
        };

        let y = mid_screen_y + PriceGrid::top_y(idx) - scroll + ROW_HEIGHT / 2.0;
        Some(y)
    }
}
