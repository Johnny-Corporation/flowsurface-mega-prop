pub mod cscalp_dom;
pub mod ladder;
pub mod timeandsales;

use crate::widget::loading;
use iced::{
    Element, padding,
    widget::{canvas, container},
};
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub enum Message {
    Scrolled(f32),
    ResetScroll,
    Invalidate(Option<Instant>),
    CancelAllOrders,
    OrderbookClicked {
        button: OrderClickButton,
        cursor_x: f32,
        cursor_y: f32,
        width: f32,
        height: f32,
    },
    SectionSplitDragged {
        divider: SectionDivider,
        cursor_x: f32,
        width: f32,
    },
}

pub enum Action {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderClickButton {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionDivider {
    First,
    Second,
}

pub trait Panel: canvas::Program<Message> {
    fn scroll(&mut self, scroll: f32);

    fn reset_scroll(&mut self);

    fn invalidate(&mut self, now: Option<Instant>) -> Option<Action>;

    fn is_empty(&self) -> bool;

    fn drag_section_split(&mut self, _divider: SectionDivider, _cursor_x: f32, _width: f32) {}

    fn cancel_all_orders(&mut self) {}

    fn handle_orderbook_click(
        &mut self,
        _button: OrderClickButton,
        _cursor_x: f32,
        _cursor_y: f32,
        _width: f32,
        _height: f32,
    ) {
    }
}

pub fn view<T: Panel>(panel: &'_ T, _timezone: data::UserTimezone) -> Element<'_, Message> {
    if panel.is_empty() {
        return loading::view("Waiting for panel data...");
    }

    container(
        canvas(panel)
            .height(iced::Length::Fill)
            .width(iced::Length::Fill),
    )
    .padding(padding::left(1).right(1).bottom(1))
    .into()
}

pub fn update<T: Panel>(panel: &mut T, message: Message) {
    match message {
        Message::Scrolled(delta) => {
            panel.scroll(delta);
        }
        Message::ResetScroll => {
            panel.reset_scroll();
        }
        Message::Invalidate(now) => {
            panel.invalidate(now);
        }
        Message::CancelAllOrders => {
            panel.cancel_all_orders();
        }
        Message::OrderbookClicked {
            button,
            cursor_x,
            cursor_y,
            width,
            height,
        } => {
            panel.handle_orderbook_click(button, cursor_x, cursor_y, width, height);
        }
        Message::SectionSplitDragged {
            divider,
            cursor_x,
            width,
        } => {
            panel.drag_section_split(divider, cursor_x, width);
        }
    }
}
