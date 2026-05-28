use crate::style;
use iced::{
    Alignment, Border, Color, Element, Event, Length, Point, Rectangle, Renderer, Shadow, Size,
    Theme, Vector,
    advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget},
    padding,
    widget::{button as iced_button, column, container, text},
};

const POPUP_GAP: f32 = 8.0;

#[derive(Debug, Clone)]
pub struct State<Id> {
    active: Option<Id>,
}

impl<Id: Copy + PartialEq> State<Id> {
    pub fn new() -> Self {
        Self { active: None }
    }

    pub fn is_active(&self, id: Id) -> bool {
        self.active == Some(id)
    }

    pub fn toggle(&mut self, id: Id) -> bool {
        if self.is_active(id) {
            self.active = None;
            false
        } else {
            self.active = Some(id);
            true
        }
    }

    pub fn close(&mut self) {
        self.active = None;
    }
}

impl<Id: Copy + PartialEq> Default for State<Id> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn icon_button<'a, Message: Clone + 'a>(
    active: bool,
    on_press: Message,
) -> Element<'a, Message> {
    iced_button(
        text("i")
            .size(style::text_size::SMALL)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .width(Length::Fixed(20.0))
    .height(Length::Fixed(20.0))
    .padding(0)
    .style(move |theme, status| icon_button_style(theme, status, active))
    .on_press(on_press)
    .into()
}

pub fn button<'a, Message: Clone + 'a>(
    active: bool,
    on_press: Message,
    title: &'static str,
    body: &'static str,
) -> Element<'a, Message> {
    popup(
        icon_button(active, on_press),
        active,
        window(title, body),
        Placement::Right,
    )
}

pub fn popup<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    active: bool,
    popup: impl Into<Element<'a, Message>>,
    placement: Placement,
) -> Element<'a, Message> {
    Popup {
        content: content.into(),
        popup: popup.into(),
        active,
        placement,
        gap: POPUP_GAP,
    }
    .into()
}

pub fn window<'a, Message: 'a>(title: &'static str, body: &'static str) -> Element<'a, Message> {
    container(
        column![
            text(title).size(style::text_size::BODY).font(iced::Font {
                weight: iced::font::Weight::Bold,
                ..Default::default()
            }),
            text(body)
                .size(style::text_size::SMALL)
                .width(Length::Fill)
                .wrapping(iced::widget::text::Wrapping::Word),
        ]
        .spacing(6),
    )
    .width(Length::Fixed(300.0))
    .padding(padding::left(12).right(12).top(10).bottom(10))
    .style(window_style)
    .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Placement {
    Top,
    Bottom,
    Left,
    Right,
}

struct Popup<'a, Message> {
    content: Element<'a, Message>,
    popup: Element<'a, Message>,
    active: bool,
    placement: Placement,
    gap: f32,
}

impl<Message> Widget<Message, Theme, Renderer> for Popup<'_, Message> {
    fn children(&self) -> Vec<widget::Tree> {
        vec![
            widget::Tree::new(&self.content),
            widget::Tree::new(&self.popup),
        ]
    }

    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(&[&self.content, &self.popup]);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let (content_tree, popup_tree) = tree.children.split_at_mut(1);

        let content_overlay = self.content.as_widget_mut().overlay(
            &mut content_tree[0],
            layout,
            renderer,
            viewport,
            translation,
        );

        let popup_overlay = if self.active {
            Some(overlay::Element::new(Box::new(PopupOverlay {
                anchor: layout.position() + translation,
                anchor_bounds: layout.bounds(),
                popup: &mut self.popup,
                tree: &mut popup_tree[0],
                placement: self.placement,
                gap: self.gap,
            })))
        } else {
            None
        };

        let overlays: Vec<_> = content_overlay.into_iter().chain(popup_overlay).collect();

        (!overlays.is_empty()).then(|| overlay::Group::with_children(overlays).overlay())
    }
}

impl<'a, Message: 'a> From<Popup<'a, Message>> for Element<'a, Message> {
    fn from(popup: Popup<'a, Message>) -> Self {
        Self::new(popup)
    }
}

struct PopupOverlay<'a, 'b, Message> {
    anchor: Point,
    anchor_bounds: Rectangle,
    popup: &'b mut Element<'a, Message>,
    tree: &'b mut widget::Tree,
    placement: Placement,
    gap: f32,
}

impl<Message> overlay::Overlay<Message, Theme, Renderer> for PopupOverlay<'_, '_, Message> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let viewport = Rectangle::with_size(bounds);
        let popup_layout = self.popup.as_widget_mut().layout(
            self.tree,
            renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );

        let popup_size = popup_layout.size();
        let popup_bounds = snapped_bounds(
            popup_size,
            self.anchor,
            self.anchor_bounds,
            self.placement,
            self.gap,
            viewport,
        );

        layout::Node::with_children(popup_size, vec![popup_layout])
            .translate(Vector::new(popup_bounds.x, popup_bounds.y))
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.popup.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout.children().next().unwrap(),
            cursor,
            &Rectangle::with_size(Size::INFINITE),
        );
    }
}

fn snapped_bounds(
    popup_size: Size,
    anchor: Point,
    anchor_bounds: Rectangle,
    placement: Placement,
    gap: f32,
    viewport: Rectangle,
) -> Rectangle {
    let centered_x = anchor.x + (anchor_bounds.width - popup_size.width) / 2.0;
    let centered_y = anchor.y + (anchor_bounds.height - popup_size.height) / 2.0;

    let (x, y) = match placement {
        Placement::Top => (centered_x, anchor.y - popup_size.height - gap),
        Placement::Bottom => (centered_x, anchor.y + anchor_bounds.height + gap),
        Placement::Left => (anchor.x - popup_size.width - gap, centered_y),
        Placement::Right => (anchor.x + anchor_bounds.width + gap, centered_y),
    };

    let max_x = viewport.x + viewport.width - popup_size.width;
    let max_y = viewport.y + viewport.height - popup_size.height;

    Rectangle {
        x: x.clamp(viewport.x, max_x.max(viewport.x)),
        y: y.clamp(viewport.y, max_y.max(viewport.y)),
        width: popup_size.width,
        height: popup_size.height,
    }
}

fn icon_button_style(
    theme: &Theme,
    status: iced::widget::button::Status,
    active: bool,
) -> iced::widget::button::Style {
    let palette = theme.extended_palette();

    let background = match (active, status) {
        (true, iced::widget::button::Status::Pressed) => palette.secondary.base.color,
        (true, _) => palette.secondary.weak.color,
        (false, iced::widget::button::Status::Hovered) => palette.background.strong.color,
        (false, iced::widget::button::Status::Pressed) => palette.background.weak.color,
        (false, _) => palette.background.weakest.color,
    };

    iced::widget::button::Style {
        text_color: if active {
            palette.secondary.strong.text
        } else {
            palette.background.weak.text
        },
        background: Some(background.into()),
        border: Border {
            width: 1.0,
            color: if active {
                palette.secondary.strong.color
            } else {
                palette.background.weak.color
            },
            radius: 10.0.into(),
        },
        ..Default::default()
    }
}

fn window_style(theme: &Theme) -> iced::widget::container::Style {
    let palette = theme.extended_palette();

    iced::widget::container::Style {
        text_color: Some(palette.background.base.text),
        background: Some(palette.background.base.color.into()),
        border: Border {
            width: 1.0,
            color: palette.secondary.weak.color.scale_alpha(0.55),
            radius: 6.0.into(),
        },
        shadow: Shadow {
            offset: Vector { x: 0.0, y: 3.0 },
            blur_radius: 12.0,
            color: Color::BLACK.scale_alpha(if palette.is_dark { 0.45 } else { 0.16 }),
        },
        snap: true,
    }
}
