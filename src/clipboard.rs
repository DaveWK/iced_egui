//! Event-scoped clipboard and focused keyboard routing through egui.
use iced_core::{
    Event,
    clipboard::{Clipboard, Kind},
    keyboard,
};

pub(crate) struct EguiClipboard {
    ctx: egui::Context,
    standard: Option<String>,
}
impl EguiClipboard {
    pub(crate) fn new(ctx: &egui::Context) -> Self {
        Self {
            ctx: ctx.clone(),
            standard: None,
        }
    }
    pub(crate) fn begin_event(&mut self, paste: Option<String>) {
        // Never substitute cached clipboard contents for an OS paste event.
        self.standard = paste;
    }
}
impl Clipboard for EguiClipboard {
    fn read(&self, kind: Kind) -> Option<String> {
        match kind {
            Kind::Standard => self.standard.clone(),
            Kind::Primary => None,
        }
    }
    fn write(&mut self, kind: Kind, contents: String) {
        if kind == Kind::Standard {
            self.standard = Some(contents.clone());
            self.ctx.copy_text(contents);
        }
        // egui has no primary-selection API; do not overwrite Standard.
    }
}

pub(crate) struct RoutedEvent {
    pub(crate) event: Event,
    pub(crate) paste: Option<String>,
}
impl From<Event> for RoutedEvent {
    fn from(event: Event) -> Self {
        Self { event, paste: None }
    }
}

fn modifiers(m: egui::Modifiers) -> keyboard::Modifiers {
    let mut result = keyboard::Modifiers::NONE;
    result.set(keyboard::Modifiers::SHIFT, m.shift);
    result.set(keyboard::Modifiers::ALT, m.alt);
    result.set(keyboard::Modifiers::CTRL, m.ctrl);
    result.set(keyboard::Modifiers::LOGO, m.mac_cmd);
    // egui detects the browser OS; Iced's COMMAND is compile-target dependent.
    // Normalize semantic command shortcuts for wasm running on macOS too.
    if m.command {
        result |= keyboard::Modifiers::COMMAND;
    }
    result
}
fn key_event(
    key: keyboard::Key,
    m: keyboard::Modifiers,
    text: Option<iced_core::SmolStr>,
    pressed: bool,
    repeat: bool,
) -> Event {
    let physical_key =
        keyboard::key::Physical::Unidentified(keyboard::key::NativeCode::Unidentified);
    let location = keyboard::Location::Standard;
    Event::Keyboard(if pressed {
        keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            key,
            physical_key,
            location,
            modifiers: m,
            text,
            repeat,
        }
    } else {
        keyboard::Event::KeyReleased {
            modified_key: key.clone(),
            key,
            physical_key,
            location,
            modifiers: m,
        }
    })
}
fn key(k: egui::Key) -> Option<keyboard::Key> {
    use keyboard::key::Named;
    let named = match k {
        egui::Key::ArrowLeft => Named::ArrowLeft,
        egui::Key::ArrowRight => Named::ArrowRight,
        egui::Key::ArrowUp => Named::ArrowUp,
        egui::Key::ArrowDown => Named::ArrowDown,
        egui::Key::Home => Named::Home,
        egui::Key::End => Named::End,
        egui::Key::Backspace => Named::Backspace,
        egui::Key::Delete => Named::Delete,
        egui::Key::Enter => Named::Enter,
        egui::Key::Escape => Named::Escape,
        egui::Key::A => return Some(keyboard::Key::Character("a".into())),
        _ => return None,
    };
    Some(keyboard::Key::Named(named))
}
fn handled(e: &egui::Event) -> bool {
    match e {
        egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_) | egui::Event::Text(_) => true,
        egui::Event::Key {
            key: k,
            modifiers: m,
            ..
        } => {
            key(*k).is_some()
                || (m.command && matches!(k, egui::Key::C | egui::Key::X | egui::Key::V))
        }
        _ => false,
    }
}

/// Take only the focused pane's editing input, leaving other events in egui.
pub(crate) fn take_events(ctx: &egui::Context, focused: bool) -> Vec<RoutedEvent> {
    if !focused {
        return Vec::new();
    }
    let (events, current) = ctx.input_mut(|input| {
        let mut taken = Vec::new();
        input.events.retain(|e| {
            if handled(e) {
                taken.push(e.clone());
                false
            } else {
                true
            }
        });
        (taken, input.modifiers)
    });
    translate(events, current)
}
fn translate(events: Vec<egui::Event>, current: egui::Modifiers) -> Vec<RoutedEvent> {
    let mut out = Vec::new();
    for e in events {
        match e {
            egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_) => {
                let (letter, paste) = match e {
                    egui::Event::Copy => ("c", None),
                    egui::Event::Cut => ("x", None),
                    egui::Event::Paste(s) => ("v", Some(s)),
                    _ => unreachable!(),
                };
                let m = keyboard::Modifiers::COMMAND;
                out.push(Event::Keyboard(keyboard::Event::ModifiersChanged(m)).into());
                let k = keyboard::Key::Character(letter.into());
                out.push(RoutedEvent {
                    event: key_event(k.clone(), m, None, true, false),
                    paste,
                });
                // Iced uses key-up to clear its paste repetition cache.
                out.push(key_event(k, m, None, false, false).into());
                out.push(
                    Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers(current))).into(),
                );
            }
            egui::Event::Text(text) => {
                // Iced TextInput consumes one scalar per KeyPressed text payload.
                out.push(
                    Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers(current))).into(),
                );
                for c in text.chars().filter(|c| !c.is_control()) {
                    out.push(
                        key_event(
                            keyboard::Key::Unidentified,
                            modifiers(current),
                            Some(c.to_string().into()),
                            true,
                            false,
                        )
                        .into(),
                    );
                }
            }
            egui::Event::Key {
                key: k,
                modifiers: m,
                pressed,
                repeat,
                ..
            } => {
                // Copy/Cut/Paste semantic events are authoritative: raw C/X/V
                // must not duplicate them or paste an old cached value.
                if m.command && matches!(k, egui::Key::C | egui::Key::X | egui::Key::V) {
                    continue;
                }
                if let Some(k) = key(k) {
                    out.push(
                        Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers(m))).into(),
                    );
                    out.push(key_event(k, modifiers(m), None, pressed, repeat).into());
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipboard_write_uses_platform_output_and_keeps_primary_separate() {
        let ctx = egui::Context::default();
        let mut cb = EguiClipboard::new(&ctx);
        let output = ctx.run(Default::default(), |_| {
            cb.write(Kind::Standard, "café 🦀\nline two".into());
            assert_eq!(
                cb.read(Kind::Standard).as_deref(),
                Some("café 🦀\nline two")
            );
            cb.write(Kind::Primary, "selection".into());
            assert_eq!(cb.read(Kind::Primary), None);
        });
        assert_eq!(output.platform_output.commands.len(), 1);
        assert!(
            matches!(&output.platform_output.commands[0], egui::OutputCommand::CopyText(s) if s == "café 🦀\nline two")
        );
        cb.begin_event(None);
        assert_eq!(cb.read(Kind::Standard), None);
    }
    #[test]
    fn successive_and_empty_pastes_keep_their_payloads_and_release_keys() {
        let events = translate(
            vec![
                egui::Event::Paste("first".into()),
                egui::Event::Paste("第二".into()),
                egui::Event::Paste("".into()),
            ],
            Default::default(),
        );
        let values: Vec<_> = events.iter().filter_map(|e| e.paste.as_deref()).collect();
        assert_eq!(values, ["first", "第二", ""]);
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(
                    e.event,
                    Event::Keyboard(keyboard::Event::KeyReleased { .. })
                ))
                .count(),
            3
        );
    }
    #[test]
    fn raw_shortcuts_do_not_duplicate_semantic_events() {
        let m = egui::Modifiers::COMMAND;
        let events = translate(
            vec![
                egui::Event::Key {
                    key: egui::Key::V,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: m,
                },
                egui::Event::Paste("once".into()),
            ],
            m,
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e.event, Event::Keyboard(keyboard::Event::KeyPressed { .. })))
                .count(),
            1
        );
    }
    #[test]
    fn only_focused_pane_consumes_editing_input() {
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![
                    egui::Event::Paste("mine".into()),
                    egui::Event::PointerMoved(egui::Pos2::ZERO),
                ],
                ..Default::default()
            },
            |ctx| {
                assert!(take_events(ctx, false).is_empty());
                assert_eq!(
                    take_events(ctx, true)
                        .iter()
                        .filter(|e| e.paste.is_some())
                        .count(),
                    1
                );
                assert!(take_events(ctx, true).is_empty());
                assert_eq!(ctx.input(|i| i.events.len()), 1);
            },
        );
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod integration {
    use crate::{IcedHost, IcedPane};
    use std::{rc::Rc, sync::Arc};

    #[test]
    #[ignore = "requires a Vulkan adapter; run with --ignored"]
    fn text_input_copy_cut_paste_and_focus() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let renderer = egui_wgpu::Renderer::new(&device, format, Default::default());
        let state = egui_wgpu::RenderState {
            adapter,
            available_adapters: vec![],
            device,
            queue,
            target_format: format,
            renderer: Arc::new(egui::mutex::RwLock::new(renderer)),
        };
        let mut pane = IcedPane::<String>::new(Rc::new(IcedHost::new(&state)));
        pane.set_redraw_on_demand(true);
        let ctx = egui::Context::default();
        let mut value = "café 🦀".to_owned();
        let mut other_value = String::new();
        let mut other_rect = egui::Rect::NOTHING;
        let mut frame = |events: Vec<egui::Event>, value: &mut String| {
            let current = value.clone();
            let mut messages = Vec::new();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(640.0, 480.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let out = pane.show_sized(ui, egui::vec2(400.0, 80.0), || {
                            iced_widget::text_input("edit here", &current)
                                .on_input(|s| s)
                                .into()
                        });
                        messages.extend(out.messages);
                        other_rect = ui.text_edit_singleline(&mut other_value).rect;
                    });
                },
            );
            if let Some(last) = messages.last() {
                *value = last.clone();
            }
            (
                output.platform_output.commands,
                other_rect,
                other_value.clone(),
            )
        };
        let point = egui::pos2(30.0, 20.0);
        let button = |point, pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let select_all = || egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        };
        frame(vec![], &mut value);
        frame(
            vec![egui::Event::PointerMoved(point), button(point, true)],
            &mut value,
        );
        frame(vec![button(point, false)], &mut value);
        frame(vec![select_all()], &mut value);
        let (commands, _, _) = frame(vec![egui::Event::Copy], &mut value);
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, egui::OutputCommand::CopyText(s) if s == "café 🦀"))
        );
        let (commands, _, _) = frame(vec![egui::Event::Cut], &mut value);
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, egui::OutputCommand::CopyText(s) if s == "café 🦀"))
        );
        assert_eq!(value, "");
        frame(
            vec![
                egui::Event::Paste("first".into()),
                egui::Event::Paste("第二".into()),
            ],
            &mut value,
        );
        assert_eq!(value, "first第二");
        frame(
            vec![select_all(), egui::Event::Paste("replacement".into())],
            &mut value,
        );
        assert_eq!(value, "replacement");
        frame(
            vec![select_all(), egui::Event::Paste(String::new())],
            &mut value,
        );
        assert_eq!(value, "");
        frame(vec![egui::Event::Text("naïve🦀".into())], &mut value);
        assert_eq!(value, "naïve🦀");
        // Move focus to a real egui editor: the pane must not consume its paste.
        let (_, rect, _) = frame(vec![], &mut value);
        let p = rect.center();
        frame(
            vec![egui::Event::PointerMoved(p), button(p, true)],
            &mut value,
        );
        frame(vec![button(p, false)], &mut value);
        let (_, _, other) = frame(
            vec![egui::Event::Paste("egui owns this".into())],
            &mut value,
        );
        assert_eq!(value, "naïve🦀");
        assert_eq!(other, "egui owns this");
        state
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
    }
}
