//! Focus operations and platform feedback.
use iced_core::{
    Rectangle,
    widget::{Id, Operation, operation::Focusable},
};

#[derive(Default)]
pub(crate) struct Focus {
    pub count: usize,
    pub focused: Option<usize>,
    pub set: Option<Option<usize>>,
}
impl Operation for Focus {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn focusable(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
        if let Some(target) = self.set {
            if target == Some(self.count) {
                state.focus();
            } else {
                state.unfocus();
            }
        }
        if state.is_focused() {
            self.focused = Some(self.count);
        }
        self.count += 1;
    }
}

pub(crate) fn cursor(interaction: iced_core::mouse::Interaction) -> egui::CursorIcon {
    use egui::CursorIcon as E;
    use iced_core::mouse::Interaction as I;
    match interaction {
        I::None | I::Idle => E::Default,
        I::Hidden => E::None,
        I::ContextMenu => E::ContextMenu,
        I::Help => E::Help,
        I::Pointer => E::PointingHand,
        I::Progress => E::Progress,
        I::Wait => E::Wait,
        I::Cell => E::Cell,
        I::Crosshair => E::Crosshair,
        I::Text => E::Text,
        I::Alias => E::Alias,
        I::Copy => E::Copy,
        I::Move => E::Move,
        I::NoDrop => E::NoDrop,
        I::NotAllowed => E::NotAllowed,
        I::Grab => E::Grab,
        I::Grabbing => E::Grabbing,
        I::ResizingHorizontally => E::ResizeHorizontal,
        I::ResizingVertically => E::ResizeVertical,
        I::ResizingDiagonallyUp => E::ResizeNeSw,
        I::ResizingDiagonallyDown => E::ResizeNwSe,
        I::ResizingColumn => E::ResizeColumn,
        I::ResizingRow => E::ResizeRow,
        I::AllScroll => E::AllScroll,
        I::ZoomIn => E::ZoomIn,
        I::ZoomOut => E::ZoomOut,
    }
}
