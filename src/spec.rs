use crate::{
    model::{Id, PortKind, Project, Result},
    review::{self, Finding, Status, Subject},
    units::Scale,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt::Write};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<u32>,
    pub data: Data,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Target { Block(Id), Port(Id), Pin(Id), Transition(Id) }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Rule {
    Manual,
    InputVoltage { min_uv: u64, max_uv: u64 },
    InputPower { voltage_uv: u64, limit_uw: u64, mode: Option<Id> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Data {
    Requirement {
        title: String,
        source: String,
        target: Option<Target>,
        rule: Rule,
        method: String,
        evidence: String,
    },
    Pin { connector: Option<Id>, number: String, function: String, port: Option<Id> },
    State { name: String, initial: bool, outputs: String },
    Transition { from: Option<Id>, to: Option<Id>, trigger: String, guard: String, action: String },
}

pub fn requirement_reference(reference: Option<u32>) -> String {
    reference.map_or_else(|| "New requirement".into(), |number| format!("REQ-{number:03}"))
}

impl Entry {
    pub fn label(&self) -> String {
        match &self.data {
            Data::Requirement { title, .. } => format!("{} · {title}", requirement_reference(self.reference)),
            Data::Pin { number, function, .. } => format!("Pin {number} · {}", pending(function)),
            Data::State { name, .. } => name.clone(),
            Data::Transition { trigger, .. } => format!("Transition · {trigger}"),
        }
    }

    pub fn unlink_design_objects(&mut self, blocks: &HashSet<Id>, ports: &HashSet<Id>) {
        match &mut self.data {
            Data::Requirement { target, .. } => {
                if match *target {
                    Some(Target::Block(id)) => blocks.contains(&id),
                    Some(Target::Port(id)) => ports.contains(&id),
                    _ => false,
                } { *target = None; }
            }
            Data::Pin { connector, port, .. } => {
                if connector.is_some_and(|id| blocks.contains(&id)) { *connector = None; }
                if port.is_some_and(|id| ports.contains(&id)) { *port = None; }
            }
            _ => {}
        }
    }
}

pub fn pending(text: &str) -> &str {
    if text.trim().is_empty() { "TBD" } else { text }
}

pub fn state_name(project: &Project, id: Option<Id>) -> &str {
    match id.and_then(|id| project.spec_entry(id)).map(|entry| &entry.data) {
        Some(Data::State { name, .. }) => name,
        _ => "TBD",
    }
}

pub fn target_name(project: &Project, target: Option<Target>) -> String {
    match target {
        Some(Target::Block(id)) => project.block(id).map(|block| block.name.clone()),
        Some(Target::Port(id)) => project.port(id).map(|port| project.port_label(port)),
        Some(Target::Pin(id) | Target::Transition(id)) => project.spec_entry(id).map(Entry::label),
        None => None,
    }.unwrap_or_else(|| "Unallocated".into())
}

pub fn mode_name(project: &Project, mode: Option<Id>) -> &str {
    match mode {
        Some(0) => &project.condition,
        Some(id) => project.mode(id).map_or("TBD", |mode| mode.name.as_str()),
        None => "TBD",
    }
}

pub fn validate(project: &Project) -> Result<()> {
    let mut pins = HashSet::new();
    let mut states = HashSet::new();
    let mut initial = false;
    let is_state = |id| project.spec_entry(id).is_some_and(|entry| matches!(&entry.data, Data::State { .. }));
    for entry in &project.spec {
        match &entry.data {
            Data::Requirement { title, target, rule, .. } => {
                if title.trim().is_empty() { return Err("A requirement needs a title.".into()); }
                let linked = match target {
                    Some(Target::Block(id)) => project.block(*id).is_some(),
                    Some(Target::Port(id)) => project.port(*id).is_some(),
                    Some(Target::Pin(id)) => project.spec_entry(*id).is_some_and(|e| matches!(&e.data, Data::Pin { .. })),
                    Some(Target::Transition(id)) => project.spec_entry(*id).is_some_and(|e| matches!(&e.data, Data::Transition { .. })),
                    None => true,
                };
                if !linked { return Err("The requirement allocation no longer exists.".into()); }
                match rule {
                    Rule::InputVoltage { min_uv, max_uv } if min_uv > max_uv =>
                        return Err("Minimum voltage must be no greater than maximum voltage.".into()),
                    Rule::InputPower { voltage_uv, limit_uw, mode }
                        if *voltage_uv == 0 || *limit_uw == 0 || mode.is_some_and(|id| id != 0 && project.mode(id).is_none()) =>
                        return Err("Power checks need positive voltage and power limits and a valid mode selection.".into()),
                    _ => {}
                }
            }
            Data::Pin { connector, number, port, .. } => {
                if number.trim().is_empty() { return Err("A pin needs a number or designation.".into()); }
                if let Some(id) = connector {
                    if project.block(*id).is_none() || !pins.insert((*id, number.trim().to_lowercase())) {
                        return Err("Pin designations must be unique on each connector block.".into());
                    }
                }
                if port.is_some_and(|id| project.port(id).is_none()) { return Err("The mapped port no longer exists.".into()); }
            }
            Data::State { name, initial: is_initial, .. } => {
                if name.trim().is_empty() || !states.insert(name.trim().to_lowercase()) {
                    return Err("State names must be nonempty and unique.".into());
                }
                if *is_initial {
                    if initial { return Err("Only one control state can be initial.".into()); }
                    initial = true;
                }
            }
            Data::Transition { from, to, trigger, .. } => {
                if trigger.trim().is_empty() || from.is_some_and(|id| !is_state(id)) || to.is_some_and(|id| !is_state(id)) {
                    return Err("A transition needs a trigger and any assigned endpoints must be control states.".into());
                }
            }
        }
    }
    Ok(())
}

pub fn check(project: &Project, target: Option<Target>, rule: &Rule) -> String {
    let (status, detail) = evaluate_rule(project, target, rule);
    let label = match status {
        Status::Pass => "Design pass", Status::Fail => "Design fail",
        Status::Open => "Open", Status::Manual => "Manual verification", Status::Warning => "Warning",
    };
    format!("{label} · {detail}")
}

fn evaluate_rule(project: &Project, target: Option<Target>, rule: &Rule) -> (Status, String) {
    let port = match target {
        Some(Target::Port(id)) => project.port(id),
        Some(Target::Pin(id)) => project.spec_entry(id).and_then(|entry| match &entry.data {
            Data::Pin { port: Some(id), .. } => project.port(*id),
            _ => None,
        }),
        _ => None,
    }.filter(|port| port.kind == PortKind::Load);
    match *rule {
        Rule::Manual => (Status::Manual, "Verification pending".into()),
        Rule::InputVoltage { min_uv, max_uv } => {
            if port.is_some_and(|port| !port.power.positive_dc()) {
                return (Status::Open, "This DC voltage rule requires a positive DC input.".into());
            }
            let Some((actual_min, actual_max)) = port.and_then(|port| port.range()) else {
                return (Status::Open, "Allocate a supply input with a declared range.".into());
            };
            let status = if actual_min <= min_uv && actual_max >= max_uv { Status::Pass } else { Status::Fail };
            (status, format!("Declared input range {}–{} V", volts(actual_min), volts(actual_max)))
        }
        Rule::InputPower { voltage_uv, limit_uw, mode } => {
            let (Some(port), Some(mode)) = (port, mode) else {
                return (Status::Open, "Allocate a supply input and select a mode.".into());
            };
            if !port.power.positive_dc() {
                return (Status::Open, "This DC input-power rule requires a positive DC input. Capture AC real-power verification separately.".into());
            }
            let Some((min, max)) = port.range() else {
                return (Status::Open, "Declare the input voltage range.".into());
            };
            if !(min..=max).contains(&voltage_uv) {
                return (Status::Fail, "Test voltage falls outside the declared input range.".into());
            }
            let demand = crate::power::Network::at_test_voltage(project, project.mode(mode), port.id, voltage_uv).demand(port.id);
            let product = u128::from(voltage_uv).saturating_mul(demand.known_ua);
            let status = if product >= u128::from(limit_uw) * 1_000_000 {
                Status::Fail
            } else if demand.incomplete {
                return (Status::Open, "Input current is incomplete in this mode.".into());
            } else { Status::Pass };
            let whole = product / 1_000_000_000_000;
            let fraction = format!("{:012}", product % 1_000_000_000_000);
            let fraction = fraction.trim_end_matches('0');
            let power = if fraction.is_empty() { whole.to_string() } else { format!("{whole}.{fraction}") };
            let minimum = if demand.incomplete { "at least " } else { "" };
            (status, format!("{minimum}{power} W from the test voltage and modeled current"))
        }
    }
}

fn volts(uv: u64) -> String { Scale::Base.number(uv.into()) }

pub fn criterion(project: &Project, rule: &Rule) -> String {
    match *rule {
        Rule::Manual => "Manual assessment".into(),
        Rule::InputVoltage { min_uv, max_uv } => format!("Accept {}–{} V DC", volts(min_uv), volts(max_uv)),
        Rule::InputPower { voltage_uv, limit_uw, mode } => format!("< {} W at {} V DC in {}",
            Scale::Base.number(limit_uw.into()), volts(voltage_uv), mode_name(project, mode)),
    }
}

pub fn pin_status(connector: Option<Id>, function: &str, port: Option<Id>) -> &'static str {
    if connector.is_some() && !function.trim().is_empty() && port.is_some() { "Mapped" } else { "Open" }
}

pub fn transition_status(from: Option<Id>, to: Option<Id>, guard: &str, action: &str) -> &'static str {
    if from.is_some() && to.is_some() && !guard.trim().is_empty() && !action.trim().is_empty() {
        "Captured · review pending"
    } else { "Open" }
}

pub fn findings(project: &Project) -> impl Iterator<Item = Finding> + '_ {
    project.spec.iter().filter_map(move |entry| {
        let (status, detail, evidence, port, block) = match &entry.data {
            Data::Requirement { source, target, rule, method, .. } => {
                let (status, detail) = evaluate_rule(project, *target, rule);
                let (port, block) = match *target {
                    Some(Target::Port(port)) => (Some(port), None),
                    Some(Target::Block(block)) => (None, Some(block)),
                    Some(Target::Pin(id)) => match project.spec_entry(id).map(|entry| &entry.data) {
                        Some(Data::Pin { connector, port, .. }) => (*port, *connector),
                        _ => (None, None),
                    },
                    _ => (None, None),
                };
                (status, format!("{} · {detail}", criterion(project, rule)),
                    vec![format!("Source · {}", pending(source)), format!("Verification · {}", pending(method))],
                    port, block)
            }
            Data::Pin { connector, function, port, .. } if pin_status(*connector, function, *port) == "Open" =>
                (Status::Open, "Complete the connector, pin function, and logical port mapping.".into(), Vec::new(), *port, *connector),
            Data::Transition { from, to, guard, action, .. } if transition_status(*from, *to, guard, action) == "Open" =>
                (Status::Open, format!("{} → {} · complete the states, guard, and action.",
                    state_name(project, *from), state_name(project, *to)), Vec::new(), None, None),
            _ => return None,
        };
        let mut blocks: Vec<_> = block.into_iter().collect();
        if let Some(owner) = port.and_then(|id| project.port(id)).map(|port| port.block) {
            if !blocks.contains(&owner) { blocks.push(owner); }
        }
        Some(Finding {
            subject: Subject::Spec(entry.id), status, title: entry.label(), detail, evidence,
            ports: port.into_iter().collect(), blocks,
        })
    })
}

pub fn append_markdown(project: &Project, out: &mut String) {
    if project.spec.is_empty() { return; }
    use review::literal;
    out.push_str("\n## Electrical specification and verification matrix\n\nDesign checks use declared model values. Verification evidence is recorded separately.\n\n");
    out.push_str("| ID | Requirement | Criterion | Source | Allocation | Design result | Verification method | Evidence reference |\n| --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for entry in &project.spec {
        if let Data::Requirement { title, source, target, rule, method, evidence } = &entry.data {
            let _ = writeln!(out, "| {} | {} | {} | {} | {} | {} | {} | {} |", requirement_reference(entry.reference),
                literal(title), literal(&criterion(project, rule)), literal(pending(source)),
                literal(&target_name(project, *target)), literal(&check(project, *target, rule)),
                literal(pending(method)), literal(if evidence.trim().is_empty() { "Pending" } else { evidence }));
        }
    }
    out.push_str("\n### Connector pin assignments\n\n| Connector | Pin | Function | Logical port | Status |\n| --- | --- | --- | --- | --- |\n");
    for entry in &project.spec {
        if let Data::Pin { connector, number, function, port } = &entry.data {
            let _ = writeln!(out, "| {} | {} | {} | {} | {} |",
                literal(&target_name(project, connector.map(Target::Block))), literal(number), literal(pending(function)),
                literal(&target_name(project, port.map(Target::Port))), pin_status(*connector, function, *port));
        }
    }
    out.push_str("\n### Control states\n\n| State | Initial | Declared outputs |\n| --- | --- | --- |\n");
    for entry in &project.spec {
        if let Data::State { name, initial, outputs } = &entry.data {
            let _ = writeln!(out, "| {} | {} | {} |", literal(name), if *initial { "Yes" } else { "No" }, literal(pending(outputs)));
        }
    }
    out.push_str("\n### Control transitions\n\n| From | To | Trigger | Guard | Action | Status |\n| --- | --- | --- | --- | --- | --- |\n");
    for entry in &project.spec {
        if let Data::Transition { from, to, trigger, guard, action } = &entry.data {
            let _ = writeln!(out, "| {} | {} | {} | {} | {} | {} |",
                literal(state_name(project, *from)), literal(state_name(project, *to)), literal(trigger),
                literal(pending(guard)), literal(pending(action)), transition_status(*from, *to, guard, action));
        }
    }
}

pub fn monitor_example() -> Result<Project> {
    use crate::model::{Port, Role};
    let mut project = Project::new();
    project.title = "Desk monitor · specification example".into();
    project.purpose = "Fictional training project for requirements, pin mappings, and operating states.".into();
    project.condition = "Standby".into();
    let active = project.add_mode("Active".into())?;
    let connector = project.add_block(Role::Custom, "Desk monitor connector".into(), false)?;
    let power = project.add_block(Role::Power, "Monitor power input".into(), false)?;
    let indicator = project.add_block(Role::Output, "Status light".into(), false)?;
    let mut input = Port::new(power, PortKind::Load);
    input.name = "5 V DC input".into();
    input.nominal_uv = Some(5_000_000);
    input.min_uv = Some(4_500_000);
    input.max_uv = Some(5_500_000);
    input.note = "Fictional training assumptions. Enter the estimated load current.".into();
    let input = project.set_port(input)?;
    let mut supply_pin = 0;
    for (index, function) in ["Supply", "Ground", "Status", "Button"].into_iter().enumerate() {
        let id = project.set_spec_entry(Entry { id: 0, reference: None, data: Data::Pin {
            connector: Some(connector), number: (index + 1).to_string(), function: function.into(),
            port: (index == 0).then_some(input),
        } })?;
        if index == 0 { supply_pin = id; }
    }
    let mut state = |name: &str, initial: bool, outputs: &str| project.set_spec_entry(Entry {
        id: 0, reference: None, data: Data::State { name: name.into(), initial, outputs: outputs.into() },
    });
    let standby = state("Standby", true, "Status light off.")?;
    let measuring = state("Measuring", false, "Status light on.")?;
    let start = project.set_spec_entry(Entry { id: 0, reference: None, data: Data::Transition {
        from: Some(standby), to: Some(measuring), trigger: "Button pressed".into(),
        guard: String::new(), action: "Start a measurement.".into(),
    } })?;
    project.set_spec_entry(Entry { id: 0, reference: None, data: Data::Transition {
        from: Some(measuring), to: Some(standby), trigger: "Measurement complete".into(),
        guard: String::new(), action: "Turn the status light off.".into(),
    } })?;
    let mut requirement = |title: &str, section: &str, target, rule, method: &str| project.set_spec_entry(Entry {
        id: 0, reference: None, data: Data::Requirement {
            title: title.into(), source: format!("Fictional desk monitor brief · {section}"),
            target: Some(target), rule, method: method.into(), evidence: String::new(),
        },
    });
    requirement("Accept the declared supply range", "Power input", Target::Pin(supply_pin),
        Rule::InputVoltage { min_uv: 4_500_000, max_uv: 5_500_000 },
        "Check the declared input range against the training requirement.")?;
    for (title, mode, limit_uw) in [("Standby input power", 0, 100_000), ("Active input power", active, 2_000_000)] {
        requirement(title, "Power budget", Target::Port(input),
            Rule::InputPower { voltage_uv: 5_000_000, limit_uw, mode: Some(mode) },
            "Measure input power at 5 V in the specified mode.")?;
    }
    requirement("Status light indicates a measurement", "User feedback", Target::Block(indicator),
        Rule::Manual, "Observe the light during a measurement.")?;
    requirement("Define when measurement can start", "Operating states", Target::Transition(start),
        Rule::Manual, "Review the button behavior and start conditions.")?;
    project.validate()?;
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_power_limit_and_deleted_mode_are_distinct() -> Result<()> {
        let mut project = monitor_example()?;
        let mode = project.modes[0].id;
        let port = project.ports[0].id;
        let rule = Rule::InputPower { voltage_uv: 5_000_000, limit_uw: 1_000_000, mode: Some(mode) };
        assert!(check(&project, Some(Target::Port(port)), &rule).starts_with("Open"));
        project.set_mode_current(mode, port, Some(Some(200_000)))?;
        assert!(check(&project, Some(Target::Port(port)), &rule).starts_with("Design fail"));
        project.set_mode_current(mode, port, Some(Some(199_999)))?;
        assert!(check(&project, Some(Target::Port(port)), &rule).starts_with("Design pass"));
        project.remove_mode(mode);
        assert!(project.spec.iter().any(|entry| matches!(&entry.data, Data::Requirement {
            rule: Rule::InputPower { mode: None, .. }, ..
        })));
        project.validate()
    }

    #[test]
    fn removing_a_mapped_port_reopens_the_pin_requirement() -> Result<()> {
        let mut project = monitor_example()?;
        let input = project.ports[0].id;
        project.remove_port(input)?;
        let requirement = project.spec.iter().find_map(|entry| match &entry.data {
            Data::Requirement { target: Some(Target::Pin(id)), rule, .. } => Some((*id, rule)),
            _ => None,
        }).expect("Pin requirement");
        assert!(check(&project, Some(Target::Pin(requirement.0)), requirement.1).starts_with("Open"));
        project.validate()
    }
}
