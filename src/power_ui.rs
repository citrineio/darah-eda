use crate::{
    model::{Id, Port, Project, Result},
    power::{Conversion, Isolation, IsolationClass, Kind},
    review::Finding,
    units::Scale,
};
use eframe::egui;

#[derive(Default, PartialEq, Eq)]
pub struct Draft {
    kind: Option<Kind>,
    input: Option<Id>,
    efficiency: String,
    input_pf: String,
    ground: String,
    dropout: String,
    isolation: Isolation,
    isolation_values: [String; 5],
}

impl Draft {
    pub fn new(channel: Option<&Conversion>) -> Self {
        let Some(channel) = channel else { return Self::default() };
        Self {
            kind: Some(channel.kind),
            input: Some(channel.input),
            efficiency: channel.efficiency_bp.map_or_else(String::new, |bp| Scale::Base.number(u128::from(bp) * 10_000)),
            input_pf: channel.input_pf_bp.map_or_else(String::new, |bp| Scale::Base.number(u128::from(bp) * 10_000)),
            ground: channel.ground_ua.map_or_else(String::new, |ua| ua.to_string()),
            dropout: channel.dropout_uv.map_or_else(String::new, |uv| Scale::Milli.number(uv.into())),
            isolation: channel.isolation.clone(),
            isolation_values: [
                (channel.isolation.working_uv, Scale::Base), (channel.isolation.withstand_uv, Scale::Base),
                (channel.isolation.test_ms, Scale::Milli), (channel.isolation.creepage_um, Scale::Milli),
                (channel.isolation.clearance_um, Scale::Milli),
            ].map(|(value, scale)| value.map_or_else(String::new, |n| scale.number(n.into()))),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, project: &Project, rail: &mut Port, finding: Option<&Finding>)
    {
        ui.separator();
        ui.strong("Power topology");
        let label = match self.kind {
            None => "No converter declared",
            Some(kind) => kind.label(),
        };
        ui.label("Rail powered by");
        egui::ComboBox::from_id_salt("conversion_kind").selected_text(label).show_ui(ui, |ui| {
            if !project.block(rail.block).is_some_and(|block| block.power_converter) {
                ui.selectable_value(&mut self.kind, None, "No converter declared");
            }
            topology_options(ui, &mut self.kind);
        });
        if self.kind.is_none() {
            ui.checkbox(&mut rail.independent_rail, "This rail is an independent supply");
        }
        if let Some(kind) = self.kind {
            let dedicated = project.block(rail.block).is_some_and(|block| block.power_converter);
            ui.label("Feeder on this block");
            let input = self.input.and_then(|id| project.port(id)).map_or("Choose a feeder", |port| port.name.as_str());
            egui::ComboBox::from_id_salt("conversion_input").selected_text(input).show_ui(ui, |ui| {
                for port in project.block_ports(rail.block).filter(|port| port.kind.power() && port.id != rail.id
                    && (!dedicated || port.kind == crate::model::PortKind::Load)) {
                    ui.selectable_value(&mut self.input, Some(port.id), &port.name);
                }
            });
            if !project.block_ports(rail.block).any(|port| port.kind.power() && port.id != rail.id) {
                ui.weak("Add a supply input or another rail on this block first.");
            }
            if kind.linear_dc() {
                field(ui, "Ground current (µA)", &mut self.ground);
                field(ui, "Maximum dropout (mV)", &mut self.dropout);
            } else {
                field(ui, "Efficiency (%)", &mut self.efficiency);
                if kind.ac_input() {
                    field(ui, "Input power factor (%)", &mut self.input_pf);
                    ui.weak("Single-phase RMS input current. Three-phase conversion remains open.");
                }
            }
            ui.collapsing("Isolation barrier", |ui| {
                egui::ComboBox::from_id_salt("isolation_class").selected_text(self.isolation.class.label())
                    .show_ui(ui, |ui| {
                        for class in IsolationClass::ALL { ui.selectable_value(&mut self.isolation.class, class, class.label()); }
                    });
                if self.isolation.class.barrier() {
                    for (label, value) in ["Working voltage (V)", "Withstand test voltage (V)",
                        "Withstand test duration (s)", "Creepage (mm)", "Clearance (mm)"]
                        .into_iter().zip(&mut self.isolation_values)
                    {
                        field(ui, label, value);
                    }
                    field(ui, "Datasheet or test evidence (include AC/DC basis and conditions)", &mut self.isolation.evidence);
                }
            });
            ui.weak("Blank assumptions are unknown. Review uses declared steady-state loads.");
        }
        if let Some(finding) = finding {
            ui.label(format!("{} · {}", finding.status.label(), finding.detail));
            if let Some(draw) = finding.evidence.last() { ui.weak(draw); }
        }
    }

    pub fn conversion(&self, output: Id) -> Result<Option<Conversion>> {
        let Some(kind) = self.kind else { return Ok(None) };
        let input = self.input.ok_or("Choose a feeder for this converter.")?;
        let mut channel = Conversion::new(input, output, kind);
        match kind {
            kind if !kind.linear_dc() => {
                if let Some(value) = Scale::Base.parse(&self.efficiency)? {
                    if value == 0 || value > 100_000_000 || value % 10_000 != 0 {
                        return Err("Enter efficiency above 0 and at most 100%, with up to two decimal places.".into());
                    }
                    channel.efficiency_bp = Some((value / 10_000) as u16);
                }
                if kind.ac_input() {
                    if let Some(value) = Scale::Base.parse(&self.input_pf)? {
                        if value == 0 || value > 100_000_000 || value % 10_000 != 0 {
                            return Err("Enter power factor above 0 and at most 100%, with up to two decimal places.".into());
                        }
                        channel.input_pf_bp = Some((value / 10_000) as u16);
                    }
                }
            }
            _ => {
                channel.ground_ua = Scale::Micro.parse(&self.ground)?;
                channel.dropout_uv = Scale::Milli.parse(&self.dropout)?;
            }
        }
        channel.isolation.class = self.isolation.class;
        if channel.isolation.class.barrier() {
            let parse = |scale: Scale, value: &str| -> Result<Option<u64>> {
                let number = scale.parse(value)?;
                if number == Some(0) { return Err("Isolation ratings must be positive or blank.".into()); }
                Ok(number)
            };
            channel.isolation.working_uv = parse(Scale::Base, &self.isolation_values[0])?;
            channel.isolation.withstand_uv = parse(Scale::Base, &self.isolation_values[1])?;
            channel.isolation.test_ms = parse(Scale::Milli, &self.isolation_values[2])?;
            channel.isolation.creepage_um = parse(Scale::Milli, &self.isolation_values[3])?;
            channel.isolation.clearance_um = parse(Scale::Milli, &self.isolation_values[4])?;
            channel.isolation.evidence = self.isolation.evidence.trim().into();
        }
        Ok(Some(channel))
    }
}

pub fn topology_options(ui: &mut egui::Ui, kind: &mut Option<Kind>) {
    for (label, options) in [("DC/DC", Kind::DC.as_slice()), ("AC/DC", Kind::AC.as_slice())] {
        ui.strong(label);
        for &option in options { ui.selectable_value(kind, Some(option), option.label()); }
        ui.separator();
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.text_edit_singleline(value);
}
