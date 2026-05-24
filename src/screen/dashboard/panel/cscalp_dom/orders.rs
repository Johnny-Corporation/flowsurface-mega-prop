use super::{
    CLUSTER_FOOTER_ROWS, CscalpDom, ROW_HEIGHT,
    types::{ColumnRanges, PriceGrid, VisibleRow},
};
use crate::{audio::SoundType, screen::dashboard::panel::OrderClickButton, style};
use exchange::unit::Price;
use iced::{
    Alignment, Point, Rectangle, Size,
    widget::canvas::{Frame, Text},
};
use std::collections::BTreeMap;
use std::time::Instant;

const POSITION_EPSILON: f32 = 0.000001;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PaperOrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PaperOrder {
    side: PaperOrderSide,
    price: Price,
    contracts: f32,
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct PaperPosition {
    contracts: f32,
    avg_entry: Option<f32>,
    realized_pnl: f32,
}

impl CscalpDom {
    pub(super) fn is_in_orderbook_area(&self, width: f32, cursor_x: f32) -> bool {
        let Some(grid) = self.build_price_grid() else {
            return false;
        };
        let layout = self.price_layout_for(width, &grid);
        let cols = self.column_ranges(width, layout.price_px);
        cursor_x >= cols.orderbook.0 && cursor_x <= cols.orderbook.1
    }

    pub(super) fn handle_orderbook_click(
        &mut self,
        button: OrderClickButton,
        cursor_x: f32,
        cursor_y: f32,
        width: f32,
        height: f32,
    ) {
        if !self.config.view_mode {
            log::warn!("Live CSCALP DOM order submission is not wired; ignoring click.");
            return;
        }

        let Some(grid) = self.build_price_grid() else {
            return;
        };
        let layout = self.price_layout_for(width, &grid);
        let cols = self.column_ranges(width, layout.price_px);
        if cursor_x < cols.orderbook.0 || cursor_x > cols.orderbook.1 {
            return;
        }
        if cursor_y >= height - ROW_HEIGHT * CLUSTER_FOOTER_ROWS {
            return;
        }

        let Some(price) = self.screen_y_to_price(cursor_y, &grid, height) else {
            return;
        };

        if price >= grid.best_ask {
            match button {
                OrderClickButton::Left => self.execute_market_order(PaperOrderSide::Buy),
                OrderClickButton::Right => self.place_limit_order(PaperOrderSide::Sell, price),
            }
        } else if price <= grid.best_bid {
            match button {
                OrderClickButton::Left => self.place_limit_order(PaperOrderSide::Buy, price),
                OrderClickButton::Right => self.execute_market_order(PaperOrderSide::Sell),
            }
        }

        self.invalidate(Some(Instant::now()));
    }

    pub(super) fn cancel_all_orders(&mut self) {
        if self.working_orders.is_empty() {
            return;
        }
        self.working_orders.clear();
        self.invalidate(Some(Instant::now()));
    }

    pub(super) fn fill_view_mode_limit_orders(&mut self) {
        if !self.config.view_mode || self.working_orders.is_empty() {
            return;
        }

        let best_bid = self.best_price(super::Side::Bid);
        let best_ask = self.best_price(super::Side::Ask);
        let last_trade_price = self.trades.raw.back().map(|trade| trade.price);

        let mut filled = Vec::new();
        self.working_orders.retain(|order| {
            let spread_hit = match order.side {
                PaperOrderSide::Buy => best_ask.is_some_and(|price| price <= order.price),
                PaperOrderSide::Sell => best_bid.is_some_and(|price| price >= order.price),
            };
            let trade_hit = match order.side {
                PaperOrderSide::Buy => last_trade_price.is_some_and(|price| price <= order.price),
                PaperOrderSide::Sell => last_trade_price.is_some_and(|price| price >= order.price),
            };

            if spread_hit || trade_hit {
                filled.push(*order);
                false
            } else {
                true
            }
        });

        let had_fills = !filled.is_empty();
        for order in filled {
            self.apply_paper_fill(order.side, order.price, order.contracts);
        }

        if had_fills {
            self.invalidate(Some(Instant::now()));
        }
    }

    pub(super) fn draw_working_orders(
        &self,
        frame: &mut Frame,
        bounds: Rectangle,
        grid: &PriceGrid,
        visible_rows: &[VisibleRow],
        cols: &ColumnRanges,
        text_color: iced::Color,
    ) {
        if self.working_orders.is_empty() {
            return;
        }

        let counts = self.order_counts_by_price();
        for visible in visible_rows {
            let Some(price) = visible.row.price() else {
                continue;
            };
            let Some((buy_count, sell_count)) = counts.get(&price).copied() else {
                continue;
            };
            let Some(y) = self.price_to_screen_y(price, grid, bounds.height) else {
                continue;
            };
            let label = order_marker_label(buy_count, sell_count);
            self.draw_order_marker(frame, &label, y, cols, text_color, buy_count, sell_count);
        }
    }

    pub(super) fn draw_view_mode_badge(
        &self,
        frame: &mut Frame,
        cols: &ColumnRanges,
        text_color: iced::Color,
    ) {
        if !self.config.view_mode {
            return;
        }

        let label = "VIEW MODE";
        let width = label.chars().count() as f32 * style::text_size::TINY * 0.66 + 10.0;
        let x = (cols.prints.1 - width - 4.0).max(cols.prints.0 + 2.0);
        frame.fill_rectangle(
            Point::new(x, 4.0),
            Size::new(width, ROW_HEIGHT - 2.0),
            label_plate_color(text_color),
        );
        frame.fill_text(Text {
            content: label.to_string(),
            position: Point::new(x + width * 0.5, 4.0 + (ROW_HEIGHT - 2.0) * 0.5),
            color: text_color.scale_alpha(0.86),
            size: style::text_size::TINY.into(),
            font: style::AZERET_MONO,
            align_x: Alignment::Center.into(),
            align_y: Alignment::Center.into(),
            ..Default::default()
        });
    }

    pub(super) fn draw_open_position_range(
        &self,
        frame: &mut Frame,
        bounds: Rectangle,
        grid: &PriceGrid,
        cols: &ColumnRanges,
        base_color: iced::Color,
        bid_color: iced::Color,
        ask_color: iced::Color,
    ) {
        let Some((entry, spread, is_profitable, is_long)) = self.paper_position_range(grid) else {
            return;
        };
        let Some(entry_y) = self.price_to_screen_y(entry, grid, bounds.height) else {
            return;
        };
        let Some(spread_y) = self.price_to_screen_y(spread, grid, bounds.height) else {
            return;
        };

        let footer_y = bounds.height - ROW_HEIGHT * CLUSTER_FOOTER_ROWS;
        let top = (entry_y.min(spread_y) - ROW_HEIGHT * 0.5).max(0.0);
        let bottom = (entry_y.max(spread_y) + ROW_HEIGHT * 0.5).min(footer_y);
        if bottom <= top {
            return;
        }

        let fill_color = if is_profitable { bid_color } else { ask_color };
        let fill_color = solid_mix(base_color, fill_color, 0.70);
        let x = cols.orderbook.0;
        let width = (cols.orderbook.1 - cols.orderbook.0).max(0.0);
        if width <= 0.0 {
            return;
        }

        frame.fill_rectangle(
            Point::new(x, top),
            Size::new(width, bottom - top),
            fill_color,
        );

        let entry_color = if is_long { bid_color } else { ask_color };
        frame.fill_rectangle(
            Point::new(x, entry_y - 1.0),
            Size::new(width, 2.0),
            solid_mix(base_color, entry_color, 0.90),
        );
    }

    pub(super) fn draw_trading_footer(
        &self,
        frame: &mut Frame,
        bounds: Rectangle,
        cols: &ColumnRanges,
        text_color: iced::Color,
        divider_color: iced::Color,
        bid_color: iced::Color,
        ask_color: iced::Color,
    ) {
        let footer_h = ROW_HEIGHT * CLUSTER_FOOTER_ROWS;
        if bounds.height <= footer_h {
            return;
        }

        let footer_y = (bounds.height - footer_h).floor();
        let footer_h = bounds.height - footer_y;
        let (position_dollars, pnl_percent, pnl_dollars) = self.paper_position_values();
        let panel_fill = trading_footer_panel_color(text_color);
        let cells = [
            ("$", signed_money(position_dollars)),
            ("C", signed_contracts(self.paper_position.contracts)),
            ("%", format!("{:+.2}%", pnl_percent)),
            ("P", signed_money(pnl_dollars)),
        ];

        let x0 = cols.orderbook.0;
        let x1 = bounds.width.max(cols.price.1).max(cols.orderbook.1);
        let width = (x1 - x0).max(0.0);
        if width <= 0.0 {
            return;
        }

        frame.fill_rectangle(
            Point::new(x0, footer_y),
            Size::new(width, footer_h),
            panel_fill,
        );

        let cell_w = width / 2.0;
        let cell_h = footer_h / 2.0;
        for (idx, (label, value)) in cells.iter().enumerate() {
            let col = idx % 2;
            let row = idx / 2;
            let x = x0 + col as f32 * cell_w;
            let y = footer_y + row as f32 * cell_h;
            frame.fill_rectangle(Point::new(x, y), Size::new(cell_w, cell_h), panel_fill);
            frame.fill_rectangle(
                Point::new(x.floor() + 0.5, y),
                Size::new(1.0, cell_h),
                divider_color,
            );
            frame.fill_rectangle(
                Point::new(x, y.floor() + 0.5),
                Size::new(cell_w, 1.0),
                divider_color,
            );
            let content = format!("{label} {value}");
            let text_color = footer_value_color(label, value, text_color, bid_color, ask_color);
            let center_y = y + cell_h * 0.5;
            frame.fill_text(Text {
                content: content.clone(),
                position: Point::new(x + cell_w * 0.5, center_y),
                color: text_color,
                size: fit_footer_text_size(&content, cell_w - 8.0).into(),
                font: style::AZERET_MONO,
                align_x: Alignment::Center.into(),
                align_y: Alignment::Center.into(),
                ..Default::default()
            });
        }
    }

    fn place_limit_order(&mut self, side: PaperOrderSide, price: Price) {
        let order = PaperOrder {
            side,
            price,
            contracts: self.config.paper_order_contracts.max(1.0),
        };
        self.working_orders.push(order);
    }

    fn execute_market_order(&mut self, side: PaperOrderSide) {
        let fill_price = match side {
            PaperOrderSide::Buy => self.best_price(super::Side::Ask),
            PaperOrderSide::Sell => self.best_price(super::Side::Bid),
        };
        if let Some(price) = fill_price {
            self.apply_paper_fill(side, price, self.config.paper_order_contracts.max(1.0));
        }
    }

    fn apply_paper_fill(&mut self, side: PaperOrderSide, price: Price, contracts: f32) {
        self.paper_position.apply_fill(side, price, contracts);
        self.play_fill_sound(side);
    }

    fn play_fill_sound(&mut self, side: PaperOrderSide) {
        let Some(cache) = &mut self.fill_sounds else {
            return;
        };
        let sound = match side {
            PaperOrderSide::Buy => SoundType::HardBuy,
            PaperOrderSide::Sell => SoundType::HardSell,
        };
        if let Err(err) = cache.play(sound) {
            log::warn!("Failed to play CSCALP DOM fill sound: {err}");
        }
    }

    fn order_counts_by_price(&self) -> BTreeMap<Price, (usize, usize)> {
        let mut counts: BTreeMap<Price, (usize, usize)> = BTreeMap::new();
        for order in &self.working_orders {
            let entry = counts.entry(order.price).or_default();
            match order.side {
                PaperOrderSide::Buy => entry.0 += 1,
                PaperOrderSide::Sell => entry.1 += 1,
            }
        }
        counts
    }

    fn draw_order_marker(
        &self,
        frame: &mut Frame,
        label: &str,
        y: f32,
        cols: &ColumnRanges,
        text_color: iced::Color,
        buy_count: usize,
        sell_count: usize,
    ) {
        let width = label.chars().count() as f32 * style::text_size::TINY * 0.66 + 8.0;
        let x = (cols.prints.1 - width - 4.0).max(cols.prints.0 + 2.0);
        let color = if sell_count > 0 && buy_count == 0 {
            iced::Color {
                r: 0.88,
                g: 0.31,
                b: 0.31,
                a: 1.0,
            }
        } else if buy_count > 0 && sell_count == 0 {
            iced::Color {
                r: 0.22,
                g: 0.72,
                b: 0.54,
                a: 1.0,
            }
        } else {
            text_color
        };

        frame.fill_rectangle(
            Point::new(x, y - ROW_HEIGHT * 0.5 + 1.0),
            Size::new(width, ROW_HEIGHT - 2.0),
            color.scale_alpha(0.20),
        );
        frame.fill_text(Text {
            content: label.to_string(),
            position: Point::new(x + width * 0.5, y),
            color: text_color.scale_alpha(0.90),
            size: style::text_size::TINY.into(),
            font: style::AZERET_MONO,
            align_x: Alignment::Center.into(),
            align_y: Alignment::Center.into(),
            ..Default::default()
        });
    }

    fn paper_position_values(&self) -> (f32, f32, f32) {
        let mark = self.mark_price().map_or(0.0, Price::to_f32_lossy);
        self.paper_position.values(mark)
    }

    fn paper_position_range(&self, grid: &PriceGrid) -> Option<(Price, Price, bool, bool)> {
        let avg_entry = self.paper_position.avg_entry?;
        let is_long = self.paper_position.contracts > POSITION_EPSILON;
        let is_short = self.paper_position.contracts < -POSITION_EPSILON;
        if !is_long && !is_short {
            return None;
        }

        let spread = if is_long {
            self.best_price(super::Side::Bid)
        } else {
            self.best_price(super::Side::Ask)
        }
        .or_else(|| self.mark_price())?;

        let entry = Price::from_f32(avg_entry).round_to_step(grid.tick);
        let spread = spread.round_to_step(grid.tick);
        let spread_value = spread.to_f32_lossy();
        let is_profitable = if is_long {
            spread_value > avg_entry
        } else {
            spread_value < avg_entry
        };

        Some((entry, spread, is_profitable, is_long))
    }

    fn mark_price(&self) -> Option<Price> {
        match (
            self.best_price(super::Side::Bid),
            self.best_price(super::Side::Ask),
        ) {
            (Some(bid), Some(ask)) => Some(Price::from_f32((bid.to_f32() + ask.to_f32()) * 0.5)),
            (Some(bid), None) => Some(bid),
            (None, Some(ask)) => Some(ask),
            (None, None) => self.trades.raw.back().map(|trade| trade.price),
        }
    }
}

impl PaperPosition {
    fn apply_fill(&mut self, side: PaperOrderSide, price: Price, contracts: f32) {
        let contracts = contracts.max(0.0);
        if contracts <= POSITION_EPSILON {
            return;
        }

        let fill = price.to_f32_lossy();
        let side_sign = match side {
            PaperOrderSide::Buy => 1.0,
            PaperOrderSide::Sell => -1.0,
        };
        let current = self.contracts;

        if current.abs() <= POSITION_EPSILON || current.signum() == side_sign {
            let current_abs = current.abs();
            let total_abs = current_abs + contracts;
            let avg = match self.avg_entry {
                Some(avg) if current_abs > POSITION_EPSILON => {
                    (avg * current_abs + fill * contracts) / total_abs
                }
                _ => fill,
            };
            self.contracts = current + side_sign * contracts;
            self.avg_entry = Some(avg);
            return;
        }

        let closing = current.abs().min(contracts);
        if let Some(avg) = self.avg_entry {
            self.realized_pnl += if current > 0.0 {
                (fill - avg) * closing
            } else {
                (avg - fill) * closing
            };
        }

        let next = current + side_sign * contracts;
        if next.abs() <= POSITION_EPSILON {
            self.contracts = 0.0;
            self.avg_entry = None;
        } else {
            self.contracts = next;
            if next.signum() != current.signum() {
                self.avg_entry = Some(fill);
            }
        }
    }

    fn values(self, mark: f32) -> (f32, f32, f32) {
        let position_dollars = self.contracts * mark;
        let unrealized = match self.avg_entry {
            Some(avg) if self.contracts > POSITION_EPSILON => (mark - avg) * self.contracts,
            Some(avg) if self.contracts < -POSITION_EPSILON => (avg - mark) * self.contracts.abs(),
            _ => 0.0,
        };
        let total_pnl = self.realized_pnl + unrealized;
        let pnl_base = self
            .avg_entry
            .map(|avg| avg.abs() * self.contracts.abs())
            .unwrap_or(0.0);
        let pnl_percent = if pnl_base > POSITION_EPSILON {
            (unrealized / pnl_base) * 100.0
        } else {
            0.0
        };

        (position_dollars, pnl_percent, total_pnl)
    }
}

fn order_marker_label(buy_count: usize, sell_count: usize) -> String {
    match (buy_count, sell_count) {
        (0, 0) => String::new(),
        (buy, 0) => format!("{buy}x↑"),
        (0, sell) => format!("{sell}x↓"),
        (buy, sell) => format!("{buy}x↑ {sell}x↓"),
    }
}

fn signed_money(value: f32) -> String {
    let sign = if value >= 0.0 { "+" } else { "-" };
    format!("${sign}{}", CscalpDom::format_rounded_volume(value.abs()))
}

fn signed_contracts(value: f32) -> String {
    if value.abs() >= 10.0 {
        format!("{value:+.0}")
    } else {
        format!("{value:+.2}")
    }
}

fn label_plate_color(text_color: iced::Color) -> iced::Color {
    let luminance = 0.2126 * text_color.r + 0.7152 * text_color.g + 0.0722 * text_color.b;
    if luminance > 0.5 {
        iced::Color::BLACK.scale_alpha(0.26)
    } else {
        iced::Color::WHITE.scale_alpha(0.60)
    }
}

fn trading_footer_panel_color(text_color: iced::Color) -> iced::Color {
    let luminance = 0.2126 * text_color.r + 0.7152 * text_color.g + 0.0722 * text_color.b;
    if luminance > 0.5 {
        iced::Color {
            r: 0.02,
            g: 0.02,
            b: 0.025,
            a: 1.0,
        }
    } else {
        iced::Color {
            r: 0.88,
            g: 0.88,
            b: 0.86,
            a: 1.0,
        }
    }
}

fn footer_value_color(
    label: &str,
    value: &str,
    text_color: iced::Color,
    bid_color: iced::Color,
    ask_color: iced::Color,
) -> iced::Color {
    if matches!(label, "%" | "P") {
        if value.starts_with('-') || value.starts_with("$-") {
            return ask_color;
        }
        if value.starts_with('+') || value.starts_with("$+") {
            return bid_color;
        }
    }
    text_color.scale_alpha(0.92)
}

fn fit_footer_text_size(text: &str, available_width: f32) -> f32 {
    let chars = text.chars().count().max(1) as f32;
    let max_size = style::text_size::TINY;
    let fitted = (available_width / (chars * 0.62)).floor();
    fitted.clamp(7.0, max_size)
}

fn solid_mix(base: iced::Color, tint: iced::Color, tint_weight: f32) -> iced::Color {
    let tint_weight = tint_weight.clamp(0.0, 1.0);
    let base_weight = 1.0 - tint_weight;
    iced::Color {
        r: base.r * base_weight + tint.r * tint_weight,
        g: base.g * base_weight + tint.g * tint_weight,
        b: base.b * base_weight + tint.b * tint_weight,
        a: 1.0,
    }
}
