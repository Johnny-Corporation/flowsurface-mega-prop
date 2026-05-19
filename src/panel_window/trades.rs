use crate::style;

use iced::{
    Element, Length, padding,
    widget::{column, container, row, text},
};

use super::{PanelMessage, format_money};

const TRADES: [TradeRow; 6] = [
    TradeRow::new("09:31:02", "BTCUSDT", "Buy", 0.42, 68420.5, 245.0),
    TradeRow::new("09:47:18", "ETHUSDT", "Sell", 3.20, 3638.8, -118.0),
    TradeRow::new("10:06:44", "SOLUSDT", "Buy", 18.0, 182.4, 92.0),
    TradeRow::new("10:28:11", "BTCUSDT", "Sell", 0.31, 68880.0, 404.0),
    TradeRow::new("10:55:09", "BNBUSDT", "Buy", 9.5, 612.1, -46.0),
    TradeRow::new("11:22:37", "ETHUSDT", "Buy", 2.1, 3664.2, 188.0),
];

#[derive(Debug, Clone, Copy)]
struct TradeRow {
    time: &'static str,
    symbol: &'static str,
    side: &'static str,
    qty: f32,
    price: f32,
    pnl: f32,
}

impl TradeRow {
    const fn new(
        time: &'static str,
        symbol: &'static str,
        side: &'static str,
        qty: f32,
        price: f32,
        pnl: f32,
    ) -> Self {
        Self {
            time,
            symbol,
            side,
            qty,
            price,
            pnl,
        }
    }
}

pub(super) fn trades_table<'a>() -> Element<'a, PanelMessage> {
    let mut rows = column![table_header(&[
        "Ticker",
        "Price",
        "Amount",
        "Fee",
        "Date/Time",
        "Key / PnL"
    ])]
    .spacing(4);

    for trade in TRADES {
        rows = rows.push(table_row(&[
            trade.symbol.to_string(),
            format!("{:.1}", trade.price),
            format!("{:.2}", trade.qty),
            format!("${:.2}", trade.qty * 0.11),
            trade.time.to_string(),
            format!("{} {}", trade.side, format_money(trade.pnl)),
        ]));
    }

    column![trade_tabs(), rows].spacing(0).into()
}

fn trade_tabs<'a>() -> Element<'a, PanelMessage> {
    row![
        trade_tab("Closed", false),
        trade_tab("All", true),
        trade_tab("Positions", false),
        trade_tab("Orders", false),
    ]
    .spacing(0)
    .into()
}

fn trade_tab<'a>(label: &'static str, active: bool) -> Element<'a, PanelMessage> {
    container(text(label).size(style::text_size::BODY))
        .padding(padding::left(12).right(12).top(7).bottom(7))
        .style(move |theme| {
            if active {
                style::panel_nav_active(theme)
            } else {
                style::panel_table_header(theme)
            }
        })
        .into()
}

fn table_header<'a, S>(labels: &[S]) -> Element<'a, PanelMessage>
where
    S: AsRef<str>,
{
    table_row_styled(labels, true)
}

fn table_row<'a, S>(labels: &[S]) -> Element<'a, PanelMessage>
where
    S: AsRef<str>,
{
    table_row_styled(labels, false)
}

fn table_row_styled<'a, S>(labels: &[S], is_header: bool) -> Element<'a, PanelMessage>
where
    S: AsRef<str>,
{
    let mut row = row![].spacing(4);

    for label in labels {
        row = row.push(
            container(text(label.as_ref().to_string()).size(style::text_size::SMALL))
                .width(Length::Fill)
                .padding(padding::left(8).right(8).top(5).bottom(5))
                .style(if is_header {
                    style::panel_table_header
                } else {
                    style::panel_table_cell
                }),
        );
    }

    row.into()
}
