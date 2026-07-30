use crate::style;

use iced::{
    Element, Length, padding,
    widget::{column, container, row, text},
};

use super::{ConnectionPanelState, PanelMessage};

pub(super) fn trades_table<'a>(
    connection_state: &'a ConnectionPanelState,
) -> Element<'a, PanelMessage> {
    let trades = connection_state.trade_history();
    let mut rows = column![table_header(&[
        "Source", "Time", "Order id", "Asset", "Side", "Type", "Order", "Avg fill", "Qty",
        "Filled", "PnL", "Fee", "State",
    ])]
    .spacing(4);

    if trades.is_empty() {
        rows = rows.push(table_row(&[
            "No trade history returned by active MEXC trading connections".to_string(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ]));
    } else {
        for trade in trades {
            rows = rows.push(table_row(&[
                trade.source,
                trade.timestamp,
                trade.order_id,
                trade.symbol,
                trade.side,
                trade.order_type,
                trade.order_price,
                trade.average_price,
                trade.quantity,
                trade.filled_quantity,
                trade.pnl,
                trade.fee,
                trade.state,
            ]));
        }
    }

    column![trade_tabs(), rows].spacing(0).into()
}

fn trade_tabs<'a>() -> Element<'a, PanelMessage> {
    row![
        trade_tab("History", true),
        trade_tab("Open orders", false),
        trade_tab("Positions", false),
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
