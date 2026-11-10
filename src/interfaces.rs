use crate::{model::{Id, Port, PortKind, Project, Result}, signal::{self, Kind, Parameter, Spec}};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Library {
    pub contracts: Vec<Contract>,
    pub templates: Vec<PortTemplate>,
    pub channels: Vec<Channel>,
    pub connectors: Vec<Connector>,
    pub connector_templates: Vec<Connector>,
    pub protections: Vec<Protection>,
}

impl Library {
    pub fn is_empty(&self) -> bool {
        self.contracts.is_empty() && self.templates.is_empty() && self.channels.is_empty()
            && self.connectors.is_empty() && self.connector_templates.is_empty() && self.protections.is_empty()
    }
    pub fn ids(&self) -> Vec<Id> {
        self.contracts.iter().map(|v| v.id).chain(self.templates.iter().map(|v| v.id))
            .chain(self.channels.iter().map(|v| v.id)).chain(self.connectors.iter().map(|v| v.id))
            .chain(self.protections.iter().map(|v| v.id)).collect()
    }
    pub fn unlink(&mut self, blocks: &HashSet<Id>, ports: &HashSet<Id>) {
        self.channels.retain(|c| !blocks.contains(&c.block) && !ports.contains(&c.input) && !ports.contains(&c.output));
        for c in &mut self.channels { c.supplies.retain(|id| !ports.contains(id)); }
        self.connectors.retain(|c| !blocks.contains(&c.block));
        for c in &mut self.connectors { for pin in &mut c.pins { if pin.port.is_some_and(|id| ports.contains(&id)) { pin.port = None; } } }
        self.protections.retain(|c| !blocks.contains(&c.block) && !ports.contains(&c.entry)
            && !c.exit.is_some_and(|id| ports.contains(&id)));
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract { pub id: Id, pub name: String, pub description: String, pub limits: Spec }

impl Contract {
    pub fn from_port(port: &Port) -> Self {
        let mut limits = port.signal_spec.clone().unwrap_or_default();
        limits.values.retain(|key, _| CONTRACT_PARAMETERS.contains(key));
        limits.drive = signal::Drive::Unspecified;
        limits.additional.clear();
        limits.power_off_behavior.clear();
        Self { id: 0, name: if port.signal.is_empty() { port.name.clone() } else { port.signal.clone() },
            description: String::new(), limits }
    }
}

pub const CONTRACT_PARAMETERS: &[Parameter] = &[
    Parameter::VoltageMin, Parameter::VoltageMax, Parameter::CurrentMin, Parameter::CurrentMax,
    Parameter::HighMin, Parameter::LowMax, Parameter::FrequencyMin, Parameter::FrequencyMax,
    Parameter::DutyMin, Parameter::DutyMax, Parameter::Bandwidth, Parameter::DataRate,
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortTemplate { pub id: Id, pub name: String, pub port: Port }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge { Left, Right, Top, Bottom }
impl Edge {
    pub const ALL: [Self; 4] = [Self::Left, Self::Right, Self::Top, Self::Bottom];
    pub fn label(self) -> &'static str { match self { Self::Left => "Left", Self::Right => "Right", Self::Top => "Top", Self::Bottom => "Bottom" } }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortPosition { pub edge: Edge, pub slot: u16 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversionKind { Divider, Linear, Buffer, LevelShifter, Inverter, Comparator, Adc, Dac, CurrentToVoltage, Filter, Isolation, Protocol, Custom }
impl ConversionKind {
    pub const ALL: [Self; 13] = [Self::Divider, Self::Linear, Self::Buffer, Self::LevelShifter, Self::Inverter,
        Self::Comparator, Self::Adc, Self::Dac, Self::CurrentToVoltage, Self::Filter, Self::Isolation, Self::Protocol, Self::Custom];
    pub fn label(self) -> &'static str {
        match self { Self::Divider => "Voltage divider", Self::Linear => "Analog gain / offset", Self::Buffer => "Analog buffer",
            Self::LevelShifter => "Digital level translator", Self::Inverter => "Digital inverter", Self::Comparator => "Comparator",
            Self::Adc => "ADC", Self::Dac => "DAC", Self::CurrentToVoltage => "Current to voltage",
            Self::Filter => "Analog filter", Self::Isolation => "Isolation", Self::Protocol => "Protocol bridge", Self::Custom => "Custom conversion" }
    }
    pub fn types(self) -> Option<(Kind, Kind)> {
        match self {
            Self::Divider | Self::Linear | Self::Buffer | Self::Filter => Some((Kind::AnalogVoltage, Kind::AnalogVoltage)),
            Self::LevelShifter | Self::Inverter => Some((Kind::Digital, Kind::Digital)),
            Self::Comparator | Self::Adc => Some((Kind::AnalogVoltage, Kind::Digital)),
            Self::Dac => Some((Kind::Digital, Kind::AnalogVoltage)),
            Self::CurrentToVoltage => Some((Kind::AnalogCurrent, Kind::AnalogVoltage)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Channel {
    pub id: Id, pub block: Id, pub name: String, pub input: Id, pub output: Id,
    pub kind: ConversionKind, pub active: bool, pub supplies: Vec<Id>,
    pub gain_ppm: Option<i64>, pub offset_uv: Option<i64>, pub offset_error_uv: Option<u64>,
    pub top_ohm: Option<u64>, pub bottom_ohm: Option<u64>, pub resistor_tolerance_ppm: Option<u32>,
    pub bandwidth_hz: Option<u64>, pub resolution_bits: Option<u16>, pub sample_rate: Option<u64>,
    pub reference: String, pub assumptions: String,
}
impl Channel {
    pub fn new(block: Id, input: Id, output: Id, kind: ConversionKind) -> Self {
        Self { id: 0, block, name: kind.label().into(), input, output, kind, active: kind != ConversionKind::Divider,
            supplies: Vec::new(), gain_ppm: None, offset_uv: None, offset_error_uv: None, top_ohm: None,
            bottom_ohm: None, resistor_tolerance_ppm: None, bandwidth_hz: None, resolution_bits: None,
            sample_rate: None, reference: String::new(), assumptions: String::new() }
    }
    pub fn envelope(&self, input: (i64, i64)) -> Option<(i64, i64)> {
        if input.0 > input.1 { return None; }
        let (low_gain, high_gain) = if self.kind == ConversionKind::Divider {
            let (top, bottom, tolerance) = (i128::from(self.top_ohm?), i128::from(self.bottom_ohm?),
                i128::from(self.resistor_tolerance_ppm?));
            if top <= 0 || bottom <= 0 || tolerance >= 1_000_000 { return None; }
            let low_top = top * (1_000_000 - tolerance);
            let high_top = top * (1_000_000 + tolerance);
            let low_bottom = bottom * (1_000_000 - tolerance);
            let high_bottom = bottom * (1_000_000 + tolerance);
            (low_bottom * 1_000_000 / (high_top + low_bottom),
                (high_bottom * 1_000_000 + low_top + high_bottom - 1) / (low_top + high_bottom))
        } else if matches!(self.kind, ConversionKind::Linear | ConversionKind::Buffer) {
            let gain = i128::from(if self.kind == ConversionKind::Buffer { 1_000_000 } else { self.gain_ppm? });
            (gain, gain)
        } else { return None; };
        let offset = i128::from(self.offset_uv?);
        let error = i128::from(self.offset_error_uv?);
        let products = [i128::from(input.0) * low_gain, i128::from(input.0) * high_gain,
            i128::from(input.1) * low_gain, i128::from(input.1) * high_gain];
        let min = products.iter().min()?.div_euclid(1_000_000) + offset - error;
        let max_product = *products.iter().max()?;
        let max = -(-max_product).div_euclid(1_000_000) + offset + error;
        Some((i64::try_from(min).ok()?, i64::try_from(max).ok()?))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exposure { Internal, Service, External }
impl Exposure {
    pub const ALL: [Self; 3] = [Self::Internal, Self::Service, Self::External];
    pub fn label(self) -> &'static str { match self { Self::Internal => "Internal", Self::Service => "Service accessible", Self::External => "External / user accessible" } }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinRole { Signal, Power, Return, Ground, Shield, Unassigned, NotConnected }
impl PinRole {
    pub const ALL: [Self; 7] = [Self::Signal, Self::Power, Self::Return, Self::Ground, Self::Shield, Self::Unassigned, Self::NotConnected];
    pub fn label(self) -> &'static str { match self { Self::Signal => "Signal", Self::Power => "Power", Self::Return => "Power return / neutral", Self::Ground => "Ground / PE", Self::Shield => "Shield", Self::Unassigned => "Unassigned", Self::NotConnected => "NC" } }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub number: String, pub name: String, pub role: PinRole, pub port: Option<Id>, pub exception: String,
    #[serde(default)] pub waived: Vec<Fault>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connector {
    pub id: Id, pub block: Id, pub family: String, pub variant: String, pub part_number: String,
    pub source: String, pub exposure: Exposure, pub pins: Vec<Pin>, pub required: Vec<Fault>,
    pub max_uv: Option<u64>, pub contact_current_ua: Option<u64>, pub cable: String,
    pub mating: String, pub shielding: String, pub hot_plug: bool, pub notes: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fault { Esd, Surge, Eft, OverVoltage, OverCurrent, ReversePolarity, ReverseCurrent, Inrush, Inductive, OverTemperature, Isolation, Emi }
impl Fault {
    pub const ALL: [Self; 12] = [Self::Esd, Self::Surge, Self::Eft, Self::OverVoltage, Self::OverCurrent,
        Self::ReversePolarity, Self::ReverseCurrent, Self::Inrush, Self::Inductive, Self::OverTemperature, Self::Isolation, Self::Emi];
    pub fn label(self) -> &'static str {
        match self { Self::Esd => "ESD", Self::Surge => "Surge", Self::Eft => "EFT / burst", Self::OverVoltage => "Overvoltage",
            Self::OverCurrent => "Overcurrent / short circuit", Self::ReversePolarity => "Reverse polarity",
            Self::ReverseCurrent => "Reverse current / backfeed", Self::Inrush => "Inrush / hot plug", Self::Inductive => "Inductive discharge",
            Self::OverTemperature => "Overtemperature", Self::Isolation => "Isolation / bonding", Self::Emi => "EMI filtering" }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Protection {
    pub id: Id, pub block: Id, pub name: String, pub entry: Id, pub exit: Option<Id>, pub shunt: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub manual_rail_voltage: bool,
    pub covers: Vec<Fault>, pub return_reference: String, pub event: String, pub evidence: String,
    pub standoff_uv: Option<u64>, pub clamp_uv: Option<u64>, pub clamp_current_ua: Option<u64>,
    pub current_limit_ua: Option<u64>, pub response_ns: Option<u64>, pub capacitance_pf: Option<u64>,
    pub leakage_na: Option<u64>, pub series_milliohm: Option<u64>, pub notes: String,
}

fn is_false(value: &bool) -> bool { !*value }

impl Protection {
    pub fn new(block:Id,entry:Id,name:&str)->Self {
        Self { id:0,block,name:name.into(),entry,exit:None,shunt:true,manual_rail_voltage:false,covers:vec![Fault::Esd],return_reference:String::new(),
            event:String::new(),evidence:String::new(),standoff_uv:None,clamp_uv:None,clamp_current_ua:None,current_limit_ua:None,
            response_ns:None,capacitance_pf:None,leakage_na:None,series_milliohm:None,notes:String::new() }
    }
}

impl Project {
    pub fn contract(&self, id: Id) -> Option<&Contract> { self.interfaces.contracts.iter().find(|c| c.id == id) }
    pub fn put_contract(&mut self, mut contract: Contract) -> Result<Id> {
        if contract.id == 0 { contract.id = self.allocate()?; }
        let id = contract.id;
        if let Some(old) = self.interfaces.contracts.iter_mut().find(|c| c.id == id) { *old = contract; }
        else { self.interfaces.contracts.push(contract); }
        Ok(id)
    }
    pub fn remove_contract(&mut self, id: Id) -> Result<()> {
        if self.ports.iter().any(|p| p.contract == Some(id)) || self.interfaces.templates.iter().any(|t| t.port.contract == Some(id)) {
            return Err("This contract is in use. Detach it from ports and templates before deleting it.".into());
        }
        self.interfaces.contracts.retain(|c| c.id != id);
        Ok(())
    }
    pub fn save_port_template(&mut self, id: Id, name: String) -> Result<Id> {
        if name.trim().is_empty() { return Err("Give the template a name.".into()); }
        let mut port = self.port(id).cloned().ok_or("Port no longer exists.")?;
        port.id = 0; port.block = 0; port.pin.clear(); port.layout = None;
        let id = self.allocate()?;
        self.interfaces.templates.push(PortTemplate { id, name, port });
        Ok(id)
    }
    pub fn bulk_ports(&mut self, block: Id, prototype: &Port, prefix: &str, start: u16, count: u16) -> Result<Vec<Id>> {
        if self.block(block).is_none() || count == 0 || count > 1024 || prefix.trim().is_empty() {
            return Err("Choose a block, a name prefix, and 1–1024 ports.".into());
        }
        let mut ids = Vec::new();
        for index in 0..count {
            let mut port = prototype.clone(); port.id = 0; port.block = block; port.pin.clear(); port.layout = None;
            let base = format!("{}{}", prefix.trim(), u32::from(start) + u32::from(index));
            port.name = base.clone();
            let mut suffix = 2;
            while self.block_ports(block).any(|p| p.name == port.name) { port.name = format!("{base}_{suffix}"); suffix += 1; }
            ids.push(self.set_port(port)?);
        }
        Ok(ids)
    }
    pub fn duplicate_port(&mut self, id: Id) -> Result<Id> {
        let port = self.port(id).cloned().ok_or("Port no longer exists.")?;
        Ok(self.bulk_ports(port.block, &port, &format!("{} copy ", port.name), 1, 1)?[0])
    }
    pub fn position_port(&mut self, id: Id, mut position: PortPosition) -> Result<()> {
        let block = self.port(id).ok_or("Port no longer exists.")?.block;
        // Keep occupied slots deterministic; move to the next free slot on this edge.
        while self.block_ports(block).any(|p| p.id != id && p.layout == Some(position)) {
            position.slot = position.slot.checked_add(1).ok_or("No free port slot.")?;
        }
        if position.slot > 4095 { return Err("Port layout is outside the supported grid.".into()); }
        self.ports.iter_mut().find(|p| p.id == id).unwrap().layout = Some(position);
        Ok(())
    }
    pub fn reset_port_position(&mut self, id: Id) -> Result<()> {
        self.ports.iter_mut().find(|p| p.id == id).ok_or("Port no longer exists.")?.layout = None;
        Ok(())
    }
    pub fn duplicate_interfaces(&mut self, blocks: &HashMap<Id, Id>, ports: &HashMap<Id, Id>) -> Result<()> {
        for mut channel in self.interfaces.channels.clone().into_iter().filter(|c| blocks.contains_key(&c.block)) {
            channel.id = self.allocate()?; channel.block = blocks[&channel.block];
            channel.input = ports[&channel.input]; channel.output = ports[&channel.output];
            channel.supplies = channel.supplies.iter().filter_map(|id| ports.get(id).copied()).collect();
            self.interfaces.channels.push(channel);
        }
        for mut connector in self.interfaces.connectors.clone().into_iter().filter(|c| blocks.contains_key(&c.block)) {
            connector.id = self.allocate()?; connector.block = blocks[&connector.block];
            for pin in &mut connector.pins { pin.port = pin.port.and_then(|id| ports.get(&id).copied()); }
            self.interfaces.connectors.push(connector);
        }
        for mut protection in self.interfaces.protections.clone().into_iter().filter(|c| blocks.contains_key(&c.block)) {
            let Some(&entry) = ports.get(&protection.entry) else { continue };
            protection.id = self.allocate()?; protection.block = blocks[&protection.block]; protection.entry = entry;
            protection.exit = protection.exit.and_then(|id| ports.get(&id).copied());
            self.interfaces.protections.push(protection);
        }
        Ok(())
    }
}

pub fn validate(project: &Project) -> Result<()> {
    let library = &project.interfaces;
    let mut names = HashSet::new();
    for c in &library.contracts {
        if c.name.trim().is_empty() || !names.insert(c.name.trim().to_lowercase()) { return Err("Contract names must be nonempty and unique.".into()); }
        signal::validate(&c.limits)?;
    }
    for port in &project.ports {
        if port.contract.is_some_and(|id| port.kind.power() || project.contract(id).is_none()) { return Err("A signal port refers to a missing contract.".into()); }
        if port.layout.is_some_and(|p| p.slot > 4095) { return Err("Invalid port layout slot.".into()); }
    }
    for template in &library.templates {
        if template.name.trim().is_empty() || template.port.id != 0 || template.port.block != 0
            || template.port.contract.is_some_and(|id| project.contract(id).is_none()) { return Err("Invalid reusable port template.".into()); }
        if let Some(spec) = &template.port.signal_spec { signal::validate(spec)?; }
    }
    for c in &library.channels {
        let input = project.port(c.input).ok_or("Signal converter input is missing.")?;
        let output = project.port(c.output).ok_or("Signal converter output is missing.")?;
        if c.name.trim().is_empty() || input.block != c.block || output.block != c.block
            || input.kind != PortKind::SignalIn || output.kind != PortKind::SignalOut {
            return Err("A signal channel needs an input and output on its owning block.".into());
        }
        if c.supplies.iter().any(|id| !project.port(*id).is_some_and(|p| p.block == c.block && p.kind == PortKind::Load)) {
            return Err("A signal converter supply must be a supply input on its block.".into());
        }
        if c.resistor_tolerance_ppm.is_some_and(|t| t >= 1_000_000) || c.resolution_bits == Some(0) {
            return Err("Use a resistor tolerance below 100% and a positive converter resolution.".into());
        }
    }
    for c in &library.connectors {
        if project.block(c.block).is_none() || c.pins.len() > 1024 { return Err("Invalid connector block or pin count.".into()); }
        let mut numbers = HashSet::new();
        for pin in &c.pins {
            if pin.number.trim().is_empty() || !numbers.insert(pin.number.trim())
                || pin.port.is_some_and(|id| project.port(id).is_none()) { return Err("Connector pins need unique numbers and valid logical-port mappings.".into()); }
        }
    }
    for c in &library.connector_templates {
        if c.id != 0 || c.block != 0 || c.pins.iter().any(|pin| pin.port.is_some()) {
            return Err("Saved connector templates must not retain live block or port references.".into());
        }
    }
    for p in &library.protections {
        if project.block(p.block).is_none() || project.port(p.entry).is_none() || p.name.trim().is_empty() { return Err("Protection needs a block, name, and an attached port/net.".into()); }
        if p.shunt && p.exit.is_some() { return Err("Shunt protection attaches to one net; remove its series exit.".into()); }
        if let Some(exit) = p.exit {
            let (input, output) = (project.port(p.entry).unwrap(), project.port(exit).ok_or("Protection exit is missing.")?);
            if p.shunt || input.block != p.block || output.block != p.block || input.kind.source() || !output.kind.source()
                || input.kind.power() != output.kind.power()
                || input.kind.power() && (input.power.kind != output.power.kind || input.power.polarity != output.power.polarity) {
                return Err("Series protection needs matching input/output power types and polarity on its block.".into());
            }
            if output.source.is_some() { return Err("A series-protection output cannot also be an independent source preset.".into()); }
        }
    }
    Ok(())
}

pub fn wire_component(project: &Project, start: Id) -> HashSet<Id> {
    let mut seen = HashSet::new(); let mut pending = vec![start];
    while let Some(id) = pending.pop() {
        if seen.insert(id) {
            for wire in &project.bindings {
                if wire.source == id { pending.push(wire.target); }
                if wire.target == id { pending.push(wire.source); }
            }
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Role;
    #[test]
    fn shared_contracts_and_bulk_ports_preserve_hardware_and_ids() -> Result<()> {
        let mut p = Project::new(); let b = p.add_block(Role::Processing, "MCU".into(), false)?;
        let mut port = Port::new(b, PortKind::SignalIn);
        port.signal_spec = Some(Spec::default());
        port.signal_spec.as_mut().unwrap().values.insert(Parameter::Resolution, 12);
        let contract = p.put_contract(Contract::from_port(&port))?;
        assert!(!p.contract(contract).unwrap().limits.values.contains_key(&Parameter::Resolution));
        port.contract = Some(contract);
        let ids = p.bulk_ports(b, &port, "AIN", 0, 32)?;
        assert_eq!(ids.len(), 32);
        assert!(p.remove_contract(contract).is_err());
        let template = p.save_port_template(ids[0], "ADC pin".into())?;
        assert!(p.interfaces.templates.iter().any(|t| t.id == template));
        let copies = p.duplicate_blocks(&[(b, crate::model::BlockPosition { x: 500, y: 0 })])?;
        assert_eq!(p.block_ports(copies[0]).count(), 32);
        assert_eq!(p.interfaces.contracts.len(), 1);
        p.validate()
    }
    #[test]
    fn divider_and_signed_offset_envelopes() {
        let mut c = Channel::new(1, 2, 3, ConversionKind::Divider);
        c.top_ohm = Some(10_000); c.bottom_ohm = Some(10_000); c.resistor_tolerance_ppm = Some(0);
        c.offset_uv = Some(0); c.offset_error_uv = Some(0);
        assert_eq!(c.envelope((0, 5_000_000)), Some((0, 2_500_000)));
        c.resistor_tolerance_ppm = Some(10_000);
        assert!(c.envelope((0, 5_000_000)).unwrap().1 > 2_500_000);
        c.kind = ConversionKind::Linear; c.gain_ppm = Some(-500_000); c.offset_uv = Some(1_000_000);
        assert_eq!(c.envelope((-2_000_000, 4_000_000)), Some((-1_000_000, 2_000_000)));
    }
}
