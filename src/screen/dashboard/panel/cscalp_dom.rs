use super::Message;
use crate::style;
use data::panel::{
    cscalp_dom::{ClusterColumn, Config, build_time_clusters},
    ladder::{GroupedDepth, Side, TradeStore},
};
use exchange::Trade;
use exchange::unit::qty::Qty;
use exchange::unit::{Price, PriceStep};
use exchange::{TickerInfo, UnixMs, depth::Depth};

use iced::widget::canvas::{self, Text};
use iced::{Alignment, Event, Point, Rectangle, Renderer, Size, Theme, keyboard, mouse};

use std::collections::BTreeMap;
use std::time::Instant;

mod clusters;
mod orderbook;
mod orders;
mod prints;
mod ruler;
mod sections;
mod types;
use orders::{PaperOrder, PaperPosition};
use sections::SectionDragState;
use types::{ColumnRanges, DomRow, LastPrintMarker, Maxima, PriceGrid, VisibleRow};

const TEXT_SIZE: f32 = style::text_size::SMALL;
const ROW_HEIGHT: f32 = 16.0;
const COL_PADDING: f32 = 4.0;
const CLUSTER_CELL_GAP: f32 = 1.0;

const MONO_CHAR_ADVANCE: f32 = 0.62;
const PRICE_TEXT_SIDE_PAD_MIN: f32 = 10.0;
const ORDER_QTY_MIN_WIDTH: f32 = 54.0;
const ORDER_QTY_MAX_WIDTH: f32 = 92.0;
const SECTION_DIVIDER_HIT_SLOP: f32 = 6.0;
const MIN_CLUSTER_SECTION_WIDTH: f32 = 110.0;
const MIN_PRINTS_SECTION_WIDTH: f32 = 120.0;

const RECENT_PRINT_LIMIT: usize = 64;
const PRINT_BUBBLE_MIN_RADIUS: f32 = 4.0;
const PRINT_BUBBLE_MAX_RADIUS: f32 = 18.0;
const PRINT_LABEL_MIN_RADIUS: f32 = 8.0;
const PRINT_LABEL_MAX_COUNT: usize = 18;
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

    fn cancel_all_orders(&mut self) {
        CscalpDom::cancel_all_orders(self);
    }

    fn handle_orderbook_click(
        &mut self,
        button: super::OrderClickButton,
        cursor_x: f32,
        cursor_y: f32,
        width: f32,
        height: f32,
    ) {
        CscalpDom::handle_orderbook_click(self, button, cursor_x, cursor_y, width, height);
    }

    fn drag_section_split(&mut self, divider: super::SectionDivider, cursor_x: f32, width: f32) {
        CscalpDom::drag_section_split(self, divider, cursor_x, width);
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
    working_orders: Vec<PaperOrder>,
    paper_position: PaperPosition,
    fill_sounds: Option<crate::audio::SoundCache>,
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
            working_orders: Vec::new(),
            paper_position: PaperPosition::default(),
            fill_sounds: crate::audio::SoundCache::with_default_sounds(Some(45.0)).ok(),
        }
    }

    pub fn insert_trades(&mut self, buffer: &[Trade]) {
        self.trades.insert_trades(buffer, self.step);
        self.fill_view_mode_limit_orders();
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
        self.fill_view_mode_limit_orders();
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
        Self::format_rounded_volume(qty.to_f32_lossy())
    }

    pub(super) fn format_rounded_volume(value: f32) -> String {
        let abs_value = value.abs();
        let sign = if value < 0.0 { "-" } else { "" };

        match abs_value {
            v if v >= 1_000_000_000.0 => {
                format!(
                    "{}{}B",
                    sign,
                    Self::trim_decimal(format!("{:.1}", v / 1_000_000_000.0))
                )
            }
            v if v >= 1_000_000.0 => {
                format!(
                    "{}{}M",
                    sign,
                    Self::trim_decimal(format!("{:.1}", v / 1_000_000.0))
                )
            }
            v if v >= 1_000.0 => format!("{}{:.0}K", sign, v / 1_000.0),
            v if v >= 100.0 => format!("{}{:.0}", sign, v),
            v if v >= 10.0 => format!("{}{:.1}", sign, v),
            v if v >= 1.0 => format!("{}{:.2}", sign, v),
            v if v >= 0.001 => format!("{}{:.3}", sign, v),
            v if v >= 0.0001 => format!("{}{:.4}", sign, v),
            v if v >= 0.00001 => format!("{}{:.5}", sign, v),
            _ => {
                if abs_value == 0.0 {
                    "0".to_string()
                } else {
                    Self::trim_decimal(format!("{}{:.3}", sign, abs_value))
                }
            }
        }
    }

    fn trim_decimal(mut value: String) -> String {
        if value.contains('.') {
            while value.ends_with('0') {
                value.pop();
            }
            if value.ends_with('.') {
                value.pop();
            }
        }
        value
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
    type State = SectionDragState;

    fn update(
        &self,
        _state: &mut Self::State,
        event: &iced::Event,
        bounds: iced::Rectangle,
        cursor: iced_core::mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let cursor_position = cursor.position_in(bounds);

        match event {
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. })
                if matches!(key, keyboard::Key::Named(keyboard::key::Named::Space)) =>
            {
                Some(canvas::Action::publish(Message::CancelAllOrders).and_capture())
            }
            Event::Mouse(mouse_event) => match mouse_event {
                mouse::Event::ButtonPressed(mouse::Button::Left) => {
                    let cursor_position = cursor_position?;
                    if let Some(divider) = self.hit_section_divider(bounds.width, cursor_position.x)
                    {
                        _state.dragging = Some(divider);
                        Some(canvas::Action::capture())
                    } else if self.is_in_orderbook_area(bounds.width, cursor_position.x) {
                        Some(
                            canvas::Action::publish(Message::OrderbookClicked {
                                button: super::OrderClickButton::Left,
                                cursor_x: cursor_position.x,
                                cursor_y: cursor_position.y,
                                width: bounds.width,
                                height: bounds.height,
                            })
                            .and_capture(),
                        )
                    } else {
                        Some(canvas::Action::publish(Message::ResetScroll).and_capture())
                    }
                }
                mouse::Event::ButtonPressed(mouse::Button::Right) => {
                    let cursor_position = cursor_position?;
                    if self.is_in_orderbook_area(bounds.width, cursor_position.x) {
                        Some(
                            canvas::Action::publish(Message::OrderbookClicked {
                                button: super::OrderClickButton::Right,
                                cursor_x: cursor_position.x,
                                cursor_y: cursor_position.y,
                                width: bounds.width,
                                height: bounds.height,
                            })
                            .and_capture(),
                        )
                    } else {
                        Some(canvas::Action::publish(Message::ResetScroll).and_capture())
                    }
                }
                mouse::Event::ButtonPressed(mouse::Button::Middle) => {
                    Some(canvas::Action::publish(Message::ResetScroll).and_capture())
                }
                mouse::Event::ButtonReleased(mouse::Button::Left) => {
                    if _state.dragging.take().is_some() {
                        Some(canvas::Action::capture())
                    } else {
                        None
                    }
                }
                mouse::Event::CursorMoved { .. } => {
                    if let Some(divider) = _state.dragging {
                        let cursor_position = cursor_position?;
                        Some(
                            canvas::Action::publish(Message::SectionSplitDragged {
                                divider,
                                cursor_x: cursor_position.x,
                                width: bounds.width,
                            })
                            .and_capture(),
                        )
                    } else if self.config.show_ruler {
                        Some(
                            canvas::Action::publish(Message::Invalidate(Some(Instant::now())))
                                .and_capture(),
                        )
                    } else {
                        None
                    }
                }
                mouse::Event::WheelScrolled { delta } => {
                    let scroll_amount = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => -(*y) * ROW_HEIGHT,
                        mouse::ScrollDelta::Pixels { y, .. } => -*y,
                    };

                    Some(canvas::Action::publish(Message::Scrolled(scroll_amount)).and_capture())
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: iced::Rectangle,
        cursor: iced_core::mouse::Cursor,
    ) -> iced_core::mouse::Interaction {
        if state.dragging.is_some() {
            return mouse::Interaction::ResizingHorizontally;
        }

        if let Some(cursor_position) = cursor.position_in(bounds)
            && self
                .hit_section_divider(bounds.width, cursor_position.x)
                .is_some()
        {
            return mouse::Interaction::ResizingHorizontally;
        }

        mouse::Interaction::default()
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: iced_core::mouse::Cursor,
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
                                footer_bg,
                                &cols,
                            );
                            self.draw_orderbook_row(
                                frame,
                                visible_row.y,
                                price,
                                qty,
                                ask_color,
                                text_color,
                                footer_bg,
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
                                footer_bg,
                                &cols,
                            );
                            self.draw_orderbook_row(
                                frame,
                                visible_row.y,
                                price,
                                qty,
                                bid_color,
                                text_color,
                                footer_bg,
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

                self.draw_open_position_range(
                    frame, bounds, &grid, &cols, footer_bg, bid_color, ask_color,
                );

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

                self.draw_working_orders(frame, bounds, &grid, &visible_rows, &cols, text_color);
                self.draw_view_mode_badge(frame, &cols, text_color);
                self.draw_trading_footer(
                    frame,
                    bounds,
                    &cols,
                    text_color,
                    footer_bg,
                    divider_color,
                    bid_color,
                    ask_color,
                );
                self.draw_vertical_splits(frame, bounds, &cols, divider_color, spread_row);
                self.draw_price_ruler(frame, &grid, bounds, cursor, &cols, text_color);
            }
        });

        vec![visual]
    }
}

impl CscalpDom {
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

    fn screen_y_to_price(&self, y: f32, grid: &PriceGrid, bounds_height: f32) -> Option<Price> {
        let mid_screen_y = bounds_height * 0.5;
        let idx = ((y + self.scroll_px - mid_screen_y) / ROW_HEIGHT).round() as i32;
        grid.index_to_price(idx)
    }
}
