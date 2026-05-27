use exchange::unit::Price;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveOrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveOpenOrder {
    pub symbol: String,
    pub side: LiveOrderSide,
    pub price: Price,
    pub contracts: f32,
    pub order_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LivePosition {
    pub symbol: String,
    pub contracts: f32,
    pub avg_entry: Option<f32>,
    pub realized_pnl: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LiveTradingSnapshot {
    pub open_orders: Vec<LiveOpenOrder>,
    pub positions: Vec<LivePosition>,
}

impl LiveTradingSnapshot {
    pub fn for_symbol(&self, symbol: &str) -> Self {
        Self {
            open_orders: self
                .open_orders
                .iter()
                .filter(|order| order.symbol == symbol)
                .cloned()
                .collect(),
            positions: self
                .positions
                .iter()
                .filter(|position| position.symbol == symbol)
                .cloned()
                .collect(),
        }
    }
}
