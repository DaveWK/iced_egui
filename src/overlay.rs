//! Constrain the base tree to a pane while laying out overlays in the viewport.
use crate::{IcedElement, Renderer};
use iced_core::{
    self as core, Clipboard, Event, Layout, Length, Rectangle, Shell, Size, Theme, Vector, Widget,
    layout, mouse, overlay, renderer,
    widget::{self, Tree},
};
use std::{cell::Cell, rc::Rc};

#[derive(Default)]
pub(crate) struct Pass {
    pub overlay: Cell<bool>,
    pub active: Cell<bool>,
    pub interactive: Cell<bool>,
    pub bounds: Cell<Option<Rectangle>>,
}
pub(crate) fn root<'a, M: 'a>(
    child: IcedElement<'a, M>,
    pane: Rectangle,
    pass: Rc<Pass>,
) -> IcedElement<'a, M> {
    core::Element::new(Root { child, pane, pass })
}
struct Root<'a, M> {
    child: IcedElement<'a, M>,
    pane: Rectangle,
    pass: Rc<Pass>,
}
impl<M> Widget<M, Theme, Renderer> for Root<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let node = self
            .child
            .as_widget_mut()
            .layout(
                &mut tree.children[0],
                renderer,
                &layout::Limits::new(Size::ZERO, self.pane.size()),
            )
            .move_to(self.pane.position());
        layout::Node::with_children(limits.max(), vec![node])
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Rectangle,
    ) {
        if !self.pass.overlay.get() {
            use core::Renderer as _;
            renderer.with_translation(Vector::new(-self.pane.x, -self.pane.y), |renderer| {
                renderer.with_layer(self.pane, |renderer| {
                    self.child.as_widget().draw(
                        &tree.children[0],
                        renderer,
                        theme,
                        style,
                        layout.children().next().unwrap(),
                        cursor,
                        &self.pane,
                    )
                });
            });
        }
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.child.as_widget_mut().operate(
            &mut tree.children[0],
            layout.children().next().unwrap(),
            renderer,
            operation,
        );
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        _: &Rectangle,
    ) {
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout.children().next().unwrap(),
            cursor,
            renderer,
            clipboard,
            shell,
            &self.pane,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout.children().next().unwrap(),
            cursor,
            &self.pane,
            renderer,
        )
    }
    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, M, Theme, Renderer>> {
        let overlay = self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            layout.children().next().unwrap(),
            renderer,
            viewport,
            translation,
        );
        self.pass.active.set(overlay.is_some());
        overlay.map(|child| tracked(child, self.pass.clone()))
    }
}
fn tracked<'a, M: 'a>(
    child: overlay::Element<'a, M, Theme, Renderer>,
    pass: Rc<Pass>,
) -> overlay::Element<'a, M, Theme, Renderer> {
    overlay::Element::new(Box::new(Tracked { child, pass }))
}
struct Tracked<'a, M> {
    child: overlay::Element<'a, M, Theme, Renderer>,
    pass: Rc<Pass>,
}
impl<M> core::Overlay<M, Theme, Renderer> for Tracked<'_, M> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let node = self.child.as_overlay_mut().layout(renderer, bounds);
        let rect = extent(&node, bounds);
        let interaction = self.child.as_overlay().mouse_interaction(
            Layout::new(&node),
            mouse::Cursor::Available(rect.center()),
            renderer,
        );
        self.pass
            .interactive
            .set(self.pass.interactive.get() || interaction != mouse::Interaction::None);
        self.pass.bounds.set(Some(
            self.pass.bounds.get().map_or(rect, |old| old.union(&rect)),
        ));
        node
    }
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        if self.pass.overlay.get() {
            self.child
                .as_overlay()
                .draw(renderer, theme, style, layout, cursor);
        }
    }
    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.child
            .as_overlay_mut()
            .operate(layout, renderer, operation);
    }
    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
    ) {
        self.child
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.child
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }
    fn overlay<'a>(
        &'a mut self,
        layout: Layout<'a>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'a, M, Theme, Renderer>> {
        let pass = self.pass.clone();
        self.child
            .as_overlay_mut()
            .overlay(layout, renderer)
            .map(|child| tracked(child, pass))
    }
    fn index(&self) -> f32 {
        self.child.as_overlay().index()
    }
}

// Group overlays use a viewport-sized root node. Its children carry their
// actual popup rectangles; using the root would block the whole egui window.
fn extent(node: &layout::Node, viewport: Size) -> Rectangle {
    if node.bounds() == Rectangle::with_size(viewport) && !node.children().is_empty() {
        node.children()
            .iter()
            .map(|child| extent(child, viewport))
            .reduce(|a, b| a.union(&b))
            .unwrap()
    } else {
        node.bounds()
    }
}
