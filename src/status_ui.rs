use crate::review::Status;
use eframe::egui::{self, Color32, RichText};

pub fn color(status: Status) -> Color32 {
    match status {
        Status::Pass => Color32::from_rgb(125, 202, 157),
        Status::Fail => Color32::from_rgb(252, 139, 131),
        Status::Open => Color32::from_rgb(246, 187, 111),
        Status::Manual => Color32::from_rgb(150, 183, 234),
        Status::Warning => Color32::from_rgb(246, 207, 95),
    }
}

pub fn priority(status: Status) -> u8 {
    match status {
        Status::Fail => 0, Status::Warning => 1, Status::Open => 2,
        Status::Manual => 3, Status::Pass => 4,
    }
}

pub fn tint(color: Color32) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 22)
}

pub fn badge(ui: &mut egui::Ui, status: Status) {
    let color = color(status);
    egui::Frame::group(ui.style()).fill(tint(color)).stroke(egui::Stroke::new(1.0_f32, color))
        .inner_margin(egui::Margin::symmetric(7, 3)).show(ui, |ui| {
            ui.label(RichText::new(status.label()).color(color).strong());
        });
}

fn result(text: &str) -> Option<(Status, &str)> {
    // Only evaluator-owned status prefixes carry health information. Plain
    // device names, conditions and notes stay neutral, even under a failed check.
    [Status::Fail, Status::Warning, Status::Open, Status::Manual, Status::Pass]
        .into_iter().find_map(|status| text.strip_prefix(status.label())
            .and_then(|tail| tail.strip_prefix(" · ")).map(|detail| (status, detail)))
}

pub fn ordered_evidence(items: &[String]) -> Vec<&str> {
    let mut ordered: Vec<_> = items.iter().map(String::as_str).collect();
    // Stable sorting preserves the evaluator's order within each severity;
    // plain inputs and notes follow the actual results.
    ordered.sort_by_key(|item| result(item).map_or(5, |(status, _)| priority(status)));
    ordered
}

pub fn compact_evidence(ui: &mut egui::Ui, text: &str) {
    if let Some((status, detail)) = result(text) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("{} ·", status.label())).color(color(status)));
            ui.label(detail);
        });
    } else {
        ui.label(text);
    }
}

pub fn evidence(ui: &mut egui::Ui, text: &str) {
    if let Some((status, detail)) = result(text) {
        let color = color(status);
        let width = ui.available_width();
        let frame = egui::Frame::group(ui.style()).fill(tint(color))
            .stroke(egui::Stroke::new(1.0_f32, color.gamma_multiply(0.55)))
            .inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                ui.set_min_width((width - 20.0).max(0.0));
                ui.label(RichText::new(status.label()).color(color).strong());
                ui.label(detail);
            });
        let rect = frame.response.rect;
        ui.painter().rect_filled(egui::Rect::from_min_max(rect.min,
            egui::pos2(rect.left() + 3.0, rect.bottom())), 0.0, color);
    } else {
        ui.label(text);
    }
}
