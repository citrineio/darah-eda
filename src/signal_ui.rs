use crate::{
    model::{Port, Result},
    signal::{self, Additional, Drive, Kind, Parameter, Spec},
};
use eframe::egui;
use std::collections::BTreeMap;

#[derive(PartialEq, Eq)]
pub struct Draft { fields: BTreeMap<Parameter, String> }

impl Draft {
    pub fn new(port: &Port) -> Self {
        let fields = port.signal_spec.as_ref().map(|spec| spec.values.iter().map(|(&key, &value)| {
            (key, signal::number(value, key.unit().1))
        }).collect()).unwrap_or_default();
        Self { fields }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, port: &mut Port) {
        let mut enabled = port.signal_spec.is_some();
        if ui.checkbox(&mut enabled, "Electrical signal characteristics").changed() {
            port.signal_spec = enabled.then(Spec::default);
            self.fields.clear();
        }
        let output = port.kind.source();
        let Some(spec) = &mut port.signal_spec else { return };
        egui::ComboBox::from_id_salt("signal_type").selected_text(spec.kind.label()).show_ui(ui, |ui| {
            for kind in Kind::ALL { ui.selectable_value(&mut spec.kind, kind, kind.label()); }
        });
        field(ui, "Reference / signal domain", &mut spec.reference);
        field(ui, "Datasheet test conditions", &mut spec.conditions);
        field(ui, "Behavior when unpowered", &mut spec.power_off_behavior);
        if output {
            egui::ComboBox::from_id_salt("signal_stage").selected_text(spec.drive.label()).show_ui(ui, |ui| {
                for drive in Drive::ALL { ui.selectable_value(&mut spec.drive, drive, drive.label()); }
            });
        }
        use Parameter::*;
        if spec.kind.logic() {
            ui.weak("Source and sink currents are positive magnitudes.");
            self.group(ui, "Logic levels and current", output, &[HighMin, LowMax, SourceCurrent, SinkCurrent, Leakage]);
        }
        if matches!(spec.kind, Kind::AnalogVoltage | Kind::DifferentialAnalog) {
            self.group(ui, "Analog range and loading", output, &[VoltageMin, VoltageMax, Impedance, SourceCurrent, SinkCurrent, Bandwidth]);
        }
        if spec.kind == Kind::AnalogCurrent {
            self.group(ui, "Current loop and compliance", output, &[CurrentMin, CurrentMax, ComplianceMin, ComplianceMax, Impedance]);
        }
        if spec.kind == Kind::Contact {
            self.group(ui, "Contact ratings", output, &[VoltageMax, CurrentMax, Impedance, MinPulse]);
        }
        if spec.kind == Kind::Rf {
            self.group(ui, "RF operating band", output, &[FrequencyMin, FrequencyMax, Impedance, Bandwidth]);
        }
        if spec.kind.differential() {
            self.group(ui, "Differential interface", output, &[CommonMin, CommonMax, VoltageMin, VoltageMax, Termination, UnitLoads]);
        }
        if matches!(spec.kind, Kind::Clock | Kind::Pwm) {
            self.group(ui, "Clock and pulse characteristics", output, &[FrequencyMin, FrequencyMax, DutyMin, DutyMax, MinPulse]);
        }
        ui.collapsing("Absolute ratings and protection", |ui| self.group(ui, "Declared limits", output, &[AbsMin, AbsMax]));
        ui.collapsing("Timing, pull networks and interconnect", |ui| {
            self.group(ui, "Timing and loading", output, &[
                Rise, Fall, Delay, Jitter, Setup, Hold, Skew, Capacitance, PullResistance,
                PullVoltageMin, PullVoltageMax, DataRate, CableLength,
            ]);
        });
        if matches!(spec.kind, Kind::AnalogVoltage | Kind::DifferentialAnalog | Kind::AnalogCurrent) {
            ui.collapsing("Acquisition, noise and accuracy", |ui| {
                self.group(ui, "Converter characteristics", output, &[SampleRate, Resolution, Accuracy, Noise]);
            });
        }
        ui.menu_button("Add interface fields", |ui| {
            for name in ["I²C", "SPI", "UART", "CAN / CAN FD", "RS-485", "USB / high-speed serial", "Audio / I²S", "RF", "Contact"] {
                if ui.button(name).clicked() {
                    for (label, unit) in interface_fields(name) {
                        if !spec.additional.iter().any(|item| item.name == label) {
                            spec.additional.push(Additional { name: label.into(), value: String::new(), unit: unit.into(),
                                conditions: String::new() });
                        }
                    }
                    ui.close_menu();
                }
            }
        });
        ui.collapsing("Additional parameters", |ui| {
            let mut remove = None;
            for (index, item) in spec.additional.iter_mut().enumerate() {
                ui.push_id(index, |ui| {
                    field(ui, "Parameter", &mut item.name);
                    field(ui, "Value", &mut item.value);
                    field(ui, "Unit", &mut item.unit);
                    field(ui, "Condition or source", &mut item.conditions);
                    if ui.small_button("Remove parameter").clicked() { remove = Some(index); }
                    ui.separator();
                });
            }
            if let Some(index) = remove { spec.additional.remove(index); }
            if ui.small_button("Add parameter").clicked() { spec.additional.push(Additional::default()); }
        });
        ui.weak("Unknown values stay blank. Characterization conditions must match the intended load and supply.");
    }

    pub fn apply(&self, port: &mut Port) -> Result<()> {
        if let Some(spec) = &mut port.signal_spec {
            spec.values.clear();
            for (&key, text) in &self.fields {
                if let Some(value) = signal::parse(text, key.unit().1)
                    .map_err(|error| format!("{}: {error}", key.label(port.kind.source())))?
                {
                    spec.values.insert(key, value);
                }
            }
            signal::validate(spec)?;
        }
        Ok(())
    }

    fn group(&mut self, ui: &mut egui::Ui, title: &str, output: bool, parameters: &[Parameter]) {
        ui.strong(title);
        for &key in parameters {
            let (unit, _) = key.unit();
            field(ui, &format!("{} ({unit})", key.label(output)), self.fields.entry(key).or_default());
        }
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
}


fn interface_fields(name: &str) -> Vec<(&'static str, &'static str)> {
    match name {
        "I²C" => vec![("Bus role", ""), ("Device address", ""), ("Address width", "bits"),
            ("Clock stretching", ""), ("Bus capacitance budget", "pF"), ("Maximum rise time", "ns")],
        "SPI" => vec![("Controller / peripheral role", ""), ("CPOL", ""), ("CPHA", ""),
            ("Maximum SCLK", "Hz"), ("Bit order", ""), ("Chip-select polarity", ""), ("Word length", "bits")],
        "UART" => vec![("Baud rate", "baud"), ("Data bits", "bits"), ("Parity", ""), ("Stop bits", ""),
            ("Flow control", ""), ("Line inversion", "")],
        "CAN / CAN FD" => vec![("Classical CAN / CAN FD", ""), ("Nominal bitrate", "bit/s"),
            ("Data-phase bitrate", "bit/s"), ("Sample point", "%"), ("Oscillator tolerance", "ppm"),
            ("Termination placement", ""), ("Maximum stub length", "m")],
        "RS-485" => vec![("Half / full duplex", ""), ("Driver enable polarity", ""),
            ("Fail-safe bias network", ""), ("Termination placement", ""), ("Maximum stub length", "m")],
        "USB / high-speed serial" => vec![("Interface generation", ""), ("Host / device role", ""),
            ("Lane count", ""), ("Differential impedance", "Ω"), ("Reference clock", "Hz"),
            ("Insertion-loss budget", "dB"), ("Maximum pair skew", "ps")],
        "Audio / I²S" => vec![("Clock role", ""), ("Sample rate", "samples/s"), ("Word length", "bits"),
            ("Channel count", ""), ("Audio data format", ""), ("Clock polarity", "")],
        "RF" => vec![("Connector / transmission medium", ""), ("Input or output power", "dBm"),
            ("Return loss", "dB"), ("Insertion loss", "dB"), ("Gain", "dB"), ("Noise figure", "dB"),
            ("Compression point", "dBm"), ("DC block", "")],
        _ => vec![("Wetting current", "mA"), ("Debounce interval", "ms"), ("Isolation requirement", ""),
            ("Contact material", ""), ("Expected lifetime", "cycles")],
    }
}
