use crate::{
    electrical::{AcReference, Capacity, CapacityUnit, Cell, Polarity, PowerKind},
    model::{Port, Result},
    rail::{self, Preset, Source, VoltagePreset},
    units::Scale,
};
use eframe::egui;

#[derive(PartialEq, Eq)]
pub struct Draft {
    voltage_preset: Option<VoltagePreset>,
    source_category: Option<&'static str>,
    nominal: String,
    frequency: [String; 2],
    capacity: String,
    capacity_unit: CapacityUnit,
}

impl Draft {
    pub fn new(port: &Port) -> Self {
        let number = |value: Option<u64>, scale: Scale| value.map_or_else(String::new, |v| scale.number(v.into()));
        let capacity = port.battery.and_then(|battery| battery.capacity);
        Self {
            voltage_preset: port.voltage_preset.or_else(|| {
                if port.source.is_some() { return None; }
                VoltagePreset::matching_default(port.range()?)
            }),
            source_category: port.source.map(|source| source.preset.category()),
            nominal: number(port.nominal_uv, Scale::Base),
            frequency: [number(port.power.frequency_min_millihz, Scale::Milli), number(port.power.frequency_max_millihz, Scale::Milli)],
            capacity: number(capacity.map(|c| c.milli_value), Scale::Milli),
            capacity_unit: capacity.map_or(CapacityUnit::Mah, |c| c.unit),
        }
    }

    pub fn matches_fields(&self, other: &Self) -> bool {
        self.voltage_preset == other.voltage_preset && self.nominal == other.nominal && self.frequency == other.frequency
            && self.capacity == other.capacity && self.capacity_unit == other.capacity_unit
    }

    pub fn apply(&self, port: &mut Port) -> Result<()> {
        if let Some(source) = port.source {
            let expected = source.preset.power();
            port.power.kind = expected.kind;
            port.power.polarity = expected.polarity;
        }
        port.nominal_uv = Scale::Base.parse(&self.nominal)?;
        port.voltage_preset = if port.source.is_none() { self.voltage_preset } else { None };
        if port.power.kind == PowerKind::Ac {
            port.power.frequency_min_millihz = Scale::Milli.parse(&self.frequency[0])?;
            port.power.frequency_max_millihz = Scale::Milli.parse(&self.frequency[1])?;
        } else {
            port.power.frequency_min_millihz = None;
            port.power.frequency_max_millihz = None;
        }
        if let Some(battery) = &mut port.battery {
            battery.capacity = Scale::Milli.parse(&self.capacity)?
                .map(|milli_value| Capacity { unit: self.capacity_unit, milli_value });
        }
        Ok(())
    }

    pub fn show(&mut self, ui: &mut egui::Ui, port: &mut Port, bounds: &mut [String], voltage_unit: Scale, converted_output: bool, detailed: bool) {
        let fixed_source = port.source.is_some() || converted_output;
        let expected = port.source.map(|source| source.preset.power())
            .or_else(|| converted_output.then(crate::electrical::PowerSpec::default));
        if detailed {
            if let Some(expected) = expected {
                port.power.kind = expected.kind;
                port.power.polarity = expected.polarity;
                ui.label(if expected.kind == PowerKind::Ac { "Power type · AC RMS" } else { "Power type · DC" });
                if converted_output { ui.weak("The converter channel produces DC."); }
                else { ui.weak("Set by the source. Choose Custom source to change power type."); }
            } else {
                ui.label("Power type");
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut port.power.kind, PowerKind::Dc, "DC");
                    ui.selectable_value(&mut port.power.kind, PowerKind::Ac, "AC · RMS");
                });
            }
            if port.power.kind == PowerKind::Ac {
                port.power.polarity = Polarity::Positive;
                ui.horizontal_wrapped(|ui| {
                    ui.label("Phases");
                    ui.selectable_value(&mut port.power.phases, 1, "1");
                    ui.selectable_value(&mut port.power.phases, 3, "3");
                });
                egui::ComboBox::from_id_salt("ac_reference").selected_text(port.power.reference.label()).show_ui(ui, |ui| {
                    ui.selectable_value(&mut port.power.reference, AcReference::LineToNeutral, "Line to neutral");
                    ui.selectable_value(&mut port.power.reference, AcReference::LineToLine, "Line to line");
                    ui.selectable_value(&mut port.power.reference, AcReference::Secondary, "Transformer secondary");
                });
                field(ui, "Minimum frequency (Hz)", &mut self.frequency[0]);
                field(ui, "Maximum frequency (Hz)", &mut self.frequency[1]);
            } else {
                port.power.phases = 1;
                port.power.reference = AcReference::LineToNeutral;
                if fixed_source {
                    ui.label(if port.power.polarity == Polarity::Negative { "Polarity · Negative" } else { "Polarity · Positive" });
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Polarity");
                        ui.selectable_value(&mut port.power.polarity, Polarity::Positive, "Positive");
                        ui.selectable_value(&mut port.power.polarity, Polarity::Negative, "Negative");
                    });
                }
                ui.weak("Voltage fields use positive magnitudes. AC uses RMS values.");
            }
        }
        if port.source.is_none() {
            self.show_voltage_preset(ui, bounds, voltage_unit);
        }
    }

    fn show_voltage_preset(&mut self, ui: &mut egui::Ui, bounds: &mut [String], unit: Scale) {
        let tolerance_bp = self.voltage_preset.map_or(VoltagePreset::DEFAULT_TOLERANCE, |preset| preset.tolerance_bp);
        let label = |mv: u64| format!("{} ±{}%", rail::label_voltage(mv * 1000),
            Scale::Base.number(u128::from(tolerance_bp) * 10_000));
        let actual_bounds = parse_bounds(bounds, unit);
        let mut selected = self.voltage_preset;
        let mut apply = false;
        let title = selected.map_or_else(|| "Choose common voltage…".to_owned(), |preset| {
            let label = label(preset.nominal_mv);
            if actual_bounds == Some(preset.bounds()) { label } else { format!("{label} (edited)") }
        });
        egui::ComboBox::from_id_salt("voltage_bounds_preset").selected_text(title).show_ui(ui, |ui| {
            for mv in rail::COMMON_MV {
                let active = selected.is_some_and(|preset| preset.nominal_mv == mv);
                if ui.selectable_label(active, label(mv)).clicked() {
                    selected = Some(VoltagePreset { nominal_mv: mv, tolerance_bp });
                    apply = true;
                    ui.close_menu();
                }
            }
        });
        if let Some(mut preset) = selected {
            ui.horizontal_wrapped(|ui| {
                ui.label("Tolerance ±");
                let mut percent = f64::from(preset.tolerance_bp) / 100.0;
                if ui.add(egui::DragValue::new(&mut percent).range(0.0..=100.0)
                    .speed(0.1).fixed_decimals(2).suffix("%")).changed()
                {
                    preset.tolerance_bp = (percent * 100.0).round() as u16;
                    apply = true;
                }
                if !apply && actual_bounds != Some(preset.bounds()) && ui.small_button("Restore preset bounds").clicked() {
                    apply = true;
                }
            });
            if apply { self.apply_voltage_preset(preset, bounds, unit); }
        }
    }

    fn apply_voltage_preset(&mut self, preset: VoltagePreset, bounds: &mut [String], unit: Scale) {
        let (min, max) = preset.bounds();
        bounds[0] = unit.number(min.into());
        bounds[1] = unit.number(max.into());
        self.voltage_preset = Some(preset);
        self.nominal.clear();
    }

    pub fn show_nominal(&mut self, ui: &mut egui::Ui, bounds: &[String], unit: Scale, detailed: bool) {
        let midpoint = parse_bounds(bounds, unit).and_then(|(min, max)| rail::voltage_midpoint(min, max));
        if detailed {
            ui.label("Nominal voltage override (V)");
            ui.add(egui::TextEdit::singleline(&mut self.nominal).desired_width(f32::INFINITY)
                .hint_text("Automatic midpoint"));
        }
        if self.nominal.trim().is_empty() {
            if let Some(value) = midpoint {
                ui.weak(format!("Nominal · {} (midpoint)", rail::label_voltage(value)));
            } else { ui.weak("Nominal is calculated when both voltage bounds are valid."); }
        } else if let Ok(Some(value)) = Scale::Base.parse(&self.nominal) {
            ui.weak(format!("Nominal · {} (override)", rail::label_voltage(value)));
        }
    }

    pub fn source(&mut self, ui: &mut egui::Ui, port: &mut Port, bounds: &mut [String], voltage_unit: Scale) -> Result<()> {
        ui.label("Power source catalog");
        let previous = port.source.map(|source| source.preset.catalog_entry());
        let previous_category = self.source_category;
        ui.label("Source category");
        let mut categories = vec![(None, "Custom source".into())];
        categories.extend(Preset::CATEGORIES.into_iter().map(|category| (Some(category), category.into())));
        picker(ui, "source_category", &mut self.source_category, &categories);
        let mut selected = previous;
        if self.source_category != previous_category { selected = None; }
        ui.label(if self.source_category == Some("Batteries") { "Battery chemistry" } else { "Source preset" });
        let mut presets = vec![(None, "Custom values".into())];
        if let Some(category) = self.source_category {
            presets.extend(Preset::ALL.iter().copied()
                .filter(|preset| preset.category() == category && preset.catalog_entry() == *preset)
                .map(|preset| (Some(preset), preset.catalog_label())));
        }
        ui.add_enabled_ui(self.source_category.is_some(), |ui| {
            picker(ui, "source_preset", &mut selected, &presets);
        });
        let mut fill = selected != previous;
        if fill {
            port.source = selected.map(|preset| Source { preset, series: 1, contract_confirmed: false });
            port.battery = selected.and_then(Preset::default_cell).map(|cell| cell.default_pack(1));
            if let Some(battery) = port.battery { self.set_capacity(battery.capacity); }
            if selected.is_some() { bounds[2].clear(); }
        }
        let Some(mut source) = port.source else { return Ok(()) };
        if let Some(cell) = source.preset.default_cell() {
            let mut battery = port.battery.unwrap_or_else(|| cell.default_pack(1));
            if port.battery.is_none() { self.set_capacity(battery.capacity); }
            let previous_cell = battery.cell;
            let previous_parallel = battery.parallel;
            let previous_series = source.series;
            ui.label("Cell format / reference");
            let cells: Vec<_> = Cell::ALL.into_iter().filter(|candidate| candidate.chemistry() == cell.chemistry())
                .map(|candidate| (candidate, candidate.label().into())).collect();
            picker(ui, "battery_cell", &mut battery.cell, &cells);
            ui.horizontal_wrapped(|ui| {
                ui.label("Series");
                ui.add(egui::DragValue::new(&mut source.series).range(1..=100));
                ui.label("Parallel");
                ui.add(egui::DragValue::new(&mut battery.parallel).range(1..=100));
            });
            if previous_cell != battery.cell || previous_parallel != battery.parallel || previous_series != source.series {
                source.preset = Preset::BatteryCell(battery.cell);
                battery = battery.cell.default_pack(battery.parallel);
                self.set_capacity(battery.capacity);
                fill = true;
            }
            ui.weak("Pack capacity. Changing the cell template or pack layout reloads its adjustable default.");
            field(ui, "Pack capacity", &mut self.capacity);
            let mut unit = self.capacity_unit;
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut unit, CapacityUnit::Mah, "mAh");
                ui.selectable_value(&mut unit, CapacityUnit::Mwh, "mWh");
            });
            let (preset_nominal, _, _) = source.preset.bounds(source.series);
            let nominal = if fill { preset_nominal } else {
                Scale::Base.parse(&self.nominal)?.or_else(||
                    parse_bounds(bounds, voltage_unit).and_then(|(min, max)| rail::voltage_midpoint(min, max)))
            };
            if unit != self.capacity_unit {
                if let Some(value) = Scale::Milli.parse(&self.capacity)? {
                    let capacity = Capacity { unit: self.capacity_unit, milli_value: value };
                    let converted = capacity.convert(unit, nominal.ok_or("Declare the pack nominal voltage before changing capacity units.")?)?;
                    self.capacity = Scale::Milli.number(converted.milli_value.into());
                }
                self.capacity_unit = unit;
            }
            battery.capacity = Scale::Milli.parse(&self.capacity)?.map(|milli_value| Capacity { unit: self.capacity_unit, milli_value });
            port.battery = Some(battery);
            if let (Some(capacity), Some(nominal)) = (battery.capacity, nominal) {
                ui.label(format!("Nominal stored energy · {} Wh", Scale::Base.number(capacity.energy_uwh(nominal)?.into())));
            }
            ui.weak("Capacity depends on cell size, load, cutoff, temperature, and age. Template defaults are reference examples or labeled assumptions.");
        }
        if source.preset.pd() {
            ui.checkbox(&mut source.contract_confirmed, "Assume negotiated contract is available");
        }
        fill |= ui.small_button("Restore source voltage defaults").clicked();
        if fill {
            port.power = source.preset.power();
            let (nominal, min, max) = source.preset.bounds(source.series);
            self.nominal = nominal.map_or_else(String::new, |uv| Scale::Base.number(uv.into()));
            let unspecified = matches!(source.preset, Preset::Solar | Preset::Generator);
            bounds[0] = if unspecified { String::new() } else { voltage_unit.number(min.into()) };
            bounds[1] = if unspecified { String::new() } else { voltage_unit.number(max.into()) };
            self.frequency = [
                port.power.frequency_min_millihz.map_or_else(String::new, |v| Scale::Milli.number(v.into())),
                port.power.frequency_max_millihz.map_or_else(String::new, |v| Scale::Milli.number(v.into())),
            ];
        }
        port.source = Some(source);
        ui.weak("Editable planning envelope. Declare actual current, tolerance, transients, and source conditions.");
        Ok(())
    }

    fn set_capacity(&mut self, capacity: Option<Capacity>) {
        self.capacity = capacity.map_or_else(String::new, |c| Scale::Milli.number(c.milli_value.into()));
        self.capacity_unit = capacity.map_or(CapacityUnit::Mah, |c| c.unit);
    }
}

fn parse_bounds(bounds: &[String], unit: Scale) -> Option<(u64, u64)> {
    Some((unit.parse(&bounds[0]).ok().flatten()?, unit.parse(&bounds[1]).ok().flatten()?))
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
}

fn picker<T: Copy + PartialEq>(ui: &mut egui::Ui, id: &str, selected: &mut T, choices: &[(T, String)]) {
    let label = choices.iter().find(|(value, _)| *value == *selected)
        .map_or("Choose an option", |(_, label)| label.as_str());
    let count = choices.len();
    ui.scope(|ui| {
        let spacing = ui.spacing_mut();
        spacing.interact_size.y = 30.0;
        spacing.item_spacing.y = 4.0;
        spacing.scroll = egui::style::ScrollStyle::solid();
        spacing.scroll.bar_width = 12.0;
        let width = ui.available_width();
        egui::ComboBox::from_id_salt(id).width(width).height(360.0)
            .wrap_mode(egui::TextWrapMode::Wrap).selected_text(label)
            .show_ui(ui, |ui| {
                ui.set_max_width(width);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                if count > 6 {
                    ui.small(format!("{count} options · Scroll for more"));
                    ui.separator();
                }
                for (value, label) in choices { ui.selectable_value(selected, *value, label.as_str()); }
            });
    });
    if count > 6 { ui.small(format!("{count} options · Scroll inside the open list")); }
}

#[cfg(test)]
mod voltage_preset_tests {
    use super::*;
    use crate::model::PortKind;

    #[test]
    fn selecting_a_preset_fills_bounds_and_restores_automatic_nominal() -> Result<()> {
        let mut port = Port::new(1, PortKind::Rail);
        port.nominal_uv = Some(12_000_000);
        let mut draft = Draft::new(&port);
        let mut bounds = [String::new(), String::new()];
        let preset = VoltagePreset { nominal_mv: 3300, tolerance_bp: 500 };
        draft.apply_voltage_preset(preset, &mut bounds, Scale::Milli);
        assert_eq!(bounds, ["3135", "3465"]);
        draft.apply(&mut port)?;
        (port.min_uv, port.max_uv) = (Scale::Milli.parse(&bounds[0])?, Scale::Milli.parse(&bounds[1])?);
        assert_eq!(port.nominal_voltage(), Some(3_300_000));
        assert_eq!(Draft::new(&port).voltage_preset, Some(preset));
        port.max_uv = Some(3_505_000);
        assert_eq!(port.nominal_voltage(), Some(3_320_000));
        assert_eq!(Draft::new(&port).voltage_preset, Some(preset));

        let new_tolerance = VoltagePreset { tolerance_bp: 250, ..preset };
        draft.apply_voltage_preset(new_tolerance, &mut bounds, Scale::Base);
        assert_eq!(bounds, ["3.2175", "3.3825"]);
        Ok(())
    }

    #[test]
    fn older_exact_common_bounds_restore_the_preset_selection() {
        let mut port = Port::new(1, PortKind::Load);
        port.min_uv = Some(4_750_000);
        port.max_uv = Some(5_250_000);
        assert_eq!(Draft::new(&port).voltage_preset,
            Some(VoltagePreset { nominal_mv: 5000, tolerance_bp: 500 }));
    }
}
