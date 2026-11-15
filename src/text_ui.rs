//! Selectable display text and a consistent clipboard menu for application data.
use eframe::egui::{self, Color32, Label, Response, RichText, TextBuffer, TextEdit, Ui, Widget, WidgetText};

pub fn copy_item(ui: &mut Ui, caption: &str, value: &str) {
    if ui.add_enabled(!value.is_empty(), egui::Button::new(caption)).clicked() {
        ui.ctx().copy_text(value.to_owned());
        ui.close_menu();
    }
}

fn copy_menu(response: &Response, text: &str, selection: Option<&str>) {
    response.context_menu(|ui| {
        if let Some(selection) = selection {
            copy_item(ui, "Copy selection", selection);
            copy_item(ui, "Copy all text", text);
        } else {
            copy_item(ui, "Copy text", text);
        }
    });
}

pub trait CopyableText {
    fn copyable_label(&mut self, text: impl Into<WidgetText>) -> Response;
    fn copyable_heading(&mut self, text: impl Into<RichText>) -> Response;
    fn copyable_strong(&mut self, text: impl Into<RichText>) -> Response;
    fn copyable_small(&mut self, text: impl Into<RichText>) -> Response;
    fn copyable_weak(&mut self, text: impl Into<RichText>) -> Response;
    fn copyable_colored_label(&mut self, color: Color32, text: impl Into<RichText>) -> Response;
    fn copyable_selectable_label(&mut self, selected: bool, text: impl Into<WidgetText>) -> Response;
    fn copyable_text_edit_singleline(&mut self, text: &mut dyn TextBuffer) -> Response;
    fn copyable_text_edit_multiline(&mut self, text: &mut dyn TextBuffer) -> Response;
}

impl CopyableText for Ui {
    fn copyable_label(&mut self, text: impl Into<WidgetText>) -> Response {
        let text = text.into();
        let content = text.text().to_owned();
        let response = self.add(Label::new(text).selectable(true));
        copy_menu(&response, &content, None);
        response
    }
    fn copyable_heading(&mut self, text: impl Into<RichText>) -> Response { self.copyable_label(text.into().heading()) }
    fn copyable_strong(&mut self, text: impl Into<RichText>) -> Response { self.copyable_label(text.into().strong()) }
    fn copyable_small(&mut self, text: impl Into<RichText>) -> Response { self.copyable_label(text.into().small()) }
    fn copyable_weak(&mut self, text: impl Into<RichText>) -> Response { self.copyable_label(text.into().weak()) }
    fn copyable_colored_label(&mut self, color: Color32, text: impl Into<RichText>) -> Response {
        self.copyable_label(text.into().color(color))
    }
    fn copyable_selectable_label(&mut self, selected: bool, text: impl Into<WidgetText>) -> Response {
        let text = text.into();
        let content = text.text().to_owned();
        let response = self.selectable_label(selected, text);
        copy_menu(&response, &content, None);
        response
    }
    fn copyable_text_edit_singleline(&mut self, text: &mut dyn TextBuffer) -> Response {
        self.add(TextEdit::singleline(text).with_copy_menu())
    }
    fn copyable_text_edit_multiline(&mut self, text: &mut dyn TextBuffer) -> Response {
        self.add(TextEdit::multiline(text).with_copy_menu())
    }
}

// Preserve each TextEdit builder's sizing, hints, focus, cursor, and normal editing behavior.
pub struct CopyableTextEdit<'a>(TextEdit<'a>);

pub trait TextEditCopyMenu<'a> {
    fn with_copy_menu(self) -> CopyableTextEdit<'a>;
}

impl<'a> TextEditCopyMenu<'a> for TextEdit<'a> {
    fn with_copy_menu(self) -> CopyableTextEdit<'a> { CopyableTextEdit(self) }
}

fn selected_chars(text: &str, range: std::ops::Range<usize>) -> String {
    text.chars().skip(range.start).take(range.end.saturating_sub(range.start)).collect()
}

#[derive(Clone, Default)]
struct CopyCursor(Option<egui::text::CCursorRange>);

impl Widget for CopyableTextEdit<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let cursor_id = ui.next_auto_id().with("copy_text_cursor");
        let previous = ui.ctx().data(|data| data.get_temp::<CopyCursor>(cursor_id)).unwrap_or_default();
        let mut output = self.0.show(ui);
        // egui 0.31 treats any mouse press as placing a text cursor. Restore
        // the existing selection when opening our menu with the secondary button.
        if output.response.hovered() && ui.input(|input| input.pointer.button_pressed(egui::PointerButton::Secondary)) {
            if let Some(range) = previous.0.filter(|range| range.primary.index != range.secondary.index) {
                output.state.cursor.set_char_range(Some(range));
                output.state.clone().store(ui.ctx(), output.response.id);
            }
        }
        ui.ctx().data_mut(|data| data.insert_temp(cursor_id, CopyCursor(output.state.cursor.char_range())));
        let text = output.galley.text();
        // The popup takes focus from the field. Its stored cursor still holds
        // the selection even while TextEditOutput::cursor_range is None.
        let selection = output.state.cursor.range(&output.galley).filter(|range| !range.is_empty())
            .map(|range| selected_chars(text, range.as_sorted_char_range()));
        copy_menu(&output.response, text, selection.as_deref());
        output.response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, Pos2, Rect};

    struct Harness {
        ctx: egui::Context,
        text: String,
        rect: Rect,
        edit: bool,
        selection: Option<std::ops::Range<usize>>,
    }

    impl Harness {
        fn new(edit: bool) -> Self {
            Self { ctx: Default::default(), text: "3.3 V · مرحبا\nEvidence line".into(), rect: Rect::NOTHING, edit, selection: None }
        }

        fn frame(&mut self, events: Vec<Event>) -> egui::FullOutput {
            self.ctx.run(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(640.0, 400.0))),
                events, ..Default::default()
            }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = if self.edit {
                        ui.add(TextEdit::multiline(&mut self.text).with_copy_menu())
                    } else {
                        ui.copyable_label(&self.text)
                    };
                    self.rect = response.rect;
                    if let Some(range) = self.selection.take() {
                        let mut state = TextEdit::load_state(ctx, response.id).unwrap();
                        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(range.start), egui::text::CCursor::new(range.end))));
                        state.store(ctx, response.id);
                        response.request_focus();
                    }
                });
            })
        }

        fn button(&mut self, pos: Pos2, button: PointerButton, pressed: bool) -> egui::FullOutput {
            self.frame(vec![Event::PointerMoved(pos), Event::PointerButton {
                pos, button, pressed, modifiers: Modifiers::NONE,
            }])
        }

        fn copy_with_right_click(&mut self, caption: &str) -> egui::FullOutput {
            self.frame(vec![]);
            let pos = self.rect.center();
            self.frame(vec![Event::PointerMoved(pos)]);
            self.button(pos, PointerButton::Secondary, true);
            self.button(pos, PointerButton::Secondary, false);
            let output = self.frame(vec![]);
            let target = output.shapes.iter().find_map(|shape| text_position(&shape.shape, caption))
                .unwrap_or_else(|| panic!("Missing context-menu item: {caption}"));
            self.frame(vec![Event::PointerMoved(target)]);
            self.button(target, PointerButton::Primary, true);
            self.button(target, PointerButton::Primary, false)
        }
    }

    fn text_position(shape: &egui::epaint::Shape, value: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == value => Some(text.pos + text.galley.size() * 0.5),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_position(shape, value)),
            _ => None,
        }
    }

    fn copied(output: &egui::FullOutput, expected: &str) -> bool {
        output.platform_output.commands.iter().any(|command|
            matches!(command, egui::OutputCommand::CopyText(text) if text == expected))
    }

    #[test]
    fn right_click_copies_display_text_including_unicode_and_line_breaks() {
        let mut app = Harness::new(false);
        let output = app.copy_with_right_click("Copy text");
        assert!(copied(&output, &app.text));
    }

    #[test]
    fn right_click_copies_form_text_without_editing_the_field() {
        let mut app = Harness::new(true);
        let before = app.text.clone();
        let output = app.copy_with_right_click("Copy text");
        assert!(copied(&output, &before));
        assert_eq!(app.text, before);
    }

    #[test]
    fn form_copy_menu_preserves_and_copies_the_selected_unicode_text() {
        let mut app = Harness::new(true);
        app.selection = Some(8..13);
        app.frame(vec![]);
        let before = app.text.clone();
        let output = app.copy_with_right_click("Copy selection");
        assert!(copied(&output, "مرحبا"));
        assert_eq!(app.text, before);
    }

    #[test]
    fn selection_uses_character_indices_instead_of_utf8_byte_offsets() {
        assert_eq!(selected_chars("A · مرحبا", 4..9), "مرحبا");
        assert_eq!(selected_chars("voltage", 3..3), "");
    }
}
