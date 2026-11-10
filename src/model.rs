use crate::power::{self, Conversion};
use crate::spec::{self, Data, Entry, Rule, Target};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub type Id = u32;
pub const SCHEMA_VERSION: u32 = 13;
pub const UNTITLED_PROJECT: &str = "Untitled system";
pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role { Input, Output, Processing, Power, Custom }

impl Role {
    pub const ALL: [Self; 5] = [Self::Input, Self::Output, Self::Processing, Self::Power, Self::Custom];
    pub fn label(self) -> &'static str {
        match self {
            Self::Input => "Input", Self::Output => "Output", Self::Processing => "Processing",
            Self::Power => "Power", Self::Custom => "Custom",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortKind { SignalIn, SignalOut, Load, Rail }

impl PortKind {
    pub const ALL: [Self; 4] = [Self::SignalIn, Self::SignalOut, Self::Load, Self::Rail];
    pub fn label(self) -> &'static str {
        match self {
            Self::SignalIn => "Signal input", Self::SignalOut => "Signal output",
            Self::Load => "Supply input", Self::Rail => "Supply rail",
        }
    }
    pub fn source(self) -> bool { matches!(self, Self::SignalOut | Self::Rail) }
    pub fn power(self) -> bool { matches!(self, Self::Load | Self::Rail) }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub id: Id,
    pub name: String,
    pub role: Role,
    pub purpose: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<BlockPosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<BlockSize>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub power_converter: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subsystem: Option<Id>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockPosition { pub x: i32, pub y: i32 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockSize { pub width: u32, pub height: u32 }

impl BlockSize {
    pub const LIMIT: u32 = 110_000;
    pub(crate) fn valid(self) -> bool {
        (1..=Self::LIMIT).contains(&self.width) && (1..=Self::LIMIT).contains(&self.height)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Port {
    pub id: Id,
    pub block: Id,
    pub name: String,
    pub kind: PortKind,
    pub required: bool,
    pub signal: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<Id>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<crate::interfaces::PortPosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal_spec: Option<crate::signal::Spec>,
    #[serde(default, skip_serializing_if = "crate::electrical::PowerSpec::is_default")]
    pub power: crate::electrical::PowerSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery: Option<crate::electrical::Battery>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nominal_uv: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voltage_preset: Option<crate::rail::VoltagePreset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<crate::rail::Source>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rail_domain: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub independent_rail: bool,
    pub min_uv: Option<u64>,
    pub max_uv: Option<u64>,
    pub current_ua: Option<u64>,
    pub note: String,
}

fn is_false(value: &bool) -> bool { !*value }

impl Port {
    pub fn new(block: Id, kind: PortKind) -> Self {
        Self {
            id: 0, block, name: kind.label().into(), kind, required: !kind.source(),
            signal: String::new(), contract: None, pin: String::new(), layout: None, signal_spec: None, power: crate::electrical::PowerSpec::default(),
            battery: None, nominal_uv: None, voltage_preset: None, source: None, rail_domain: String::new(), independent_rail: false, min_uv: None, max_uv: None, current_ua: None,
            note: String::new(),
        }
    }
    pub fn nominal_voltage(&self) -> Option<u64> {
        self.nominal_uv.or_else(|| {
            let (min, max) = self.range()?;
            crate::rail::voltage_midpoint(min, max)
        })
    }

    pub fn range(&self) -> Option<(u64, u64)> { Some((self.min_uv?, self.max_uv?)) }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub id: Id,
    pub source: Id,
    pub target: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bend: Option<BlockPosition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route: Vec<BlockPosition>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Goal {
    pub id: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<u32>,
    pub owner: Option<Id>,
    pub description: String,
    pub method: String,
    #[serde(default)]
    pub note: String,
    pub responsible: Vec<Id>,
}

impl Goal {
    pub fn new(owner: Option<Id>) -> Self {
        Self { id: 0, reference: None, owner, description: String::new(), method: String::new(), note: String::new(), responsible: Vec::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mode {
    pub id: Id,
    pub name: String,
    currents: Vec<ModeCurrent>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModeCurrent {
    port: Id,
    current_ua: Option<u64>,
}

impl Mode {
    pub fn current_override(&self, port: Id) -> Option<Option<u64>> {
        self.currents.iter().find(|entry| entry.port == port).map(|entry| entry.current_ua)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub schema_version: u32,
    pub project_id: String,
    pub title: String,
    pub purpose: String,
    pub condition: String,
    next_id: Id,
    #[serde(default = "first_requirement")]
    next_requirement: u32,
    pub blocks: Vec<Block>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subsystems: Vec<crate::subsystem::Subsystem>,
    pub ports: Vec<Port>,
    pub bindings: Vec<Binding>,
    pub goals: Vec<Goal>,
    #[serde(default)]
    pub modes: Vec<Mode>,
    #[serde(default)]
    pub spec: Vec<Entry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conversions: Vec<Conversion>,
    #[serde(default, skip_serializing_if = "crate::interfaces::Library::is_empty")]
    pub interfaces: crate::interfaces::Library,
}

fn first_requirement() -> u32 { 1 }

impl Project {
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION, project_id: uuid::Uuid::new_v4().to_string(),
            title: UNTITLED_PROJECT.into(), purpose: String::new(), condition: "Normal operation".into(),
            next_id: 1, next_requirement: first_requirement(), blocks: Vec::new(), subsystems: Vec::new(), ports: Vec::new(), bindings: Vec::new(), goals: Vec::new(), modes: Vec::new(), spec: Vec::new(), conversions: Vec::new(), interfaces: crate::interfaces::Library::default(),
        }
    }
    pub(crate) fn allocate(&mut self) -> Result<Id> {
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or("Project has exhausted its IDs.")?;
        Ok(id)
    }
    fn allocate_requirement(&mut self) -> Result<u32> {
        let number = self.next_requirement;
        self.next_requirement = number.checked_add(1).ok_or("Project has exhausted its requirement references.")?;
        Ok(number)
    }

    pub(crate) fn assign_legacy_requirement_references(&mut self) -> Result<()> {
        self.next_requirement = first_requirement();
        let mut entries: Vec<_> = self.goals.iter_mut().map(|goal| (goal.id, &mut goal.reference))
            .chain(self.spec.iter_mut().filter(|entry| matches!(&entry.data, Data::Requirement { .. }))
                .map(|entry| (entry.id, &mut entry.reference))).collect();
        entries.sort_unstable_by_key(|(id, _)| *id);
        for (_, reference) in entries {
            *reference = Some(self.next_requirement);
            self.next_requirement = self.next_requirement.checked_add(1).ok_or("Too many requirements.")?;
        }
        Ok(())
    }

    pub fn spec_entry(&self, id: Id) -> Option<&Entry> { self.spec.iter().find(|entry| entry.id == id) }

    pub fn set_spec_entry(&mut self, mut entry: Entry) -> Result<Id> {
        if entry.id == 0 {
            entry.id = self.allocate()?;
            if matches!(&entry.data, Data::Requirement { .. }) {
                entry.reference = Some(self.allocate_requirement()?);
            }
            let id = entry.id;
            self.spec.push(entry);
            return Ok(id);
        }
        let id = entry.id;
        let stored = self.spec.iter_mut().find(|entry| entry.id == id).ok_or("Spec entry no longer exists.")?;
        entry.reference = stored.reference;
        *stored = entry;
        Ok(id)
    }

    pub fn remove_spec_entry(&mut self, id: Id) {
        self.spec.retain(|entry| entry.id != id);
        for entry in &mut self.spec {
            match &mut entry.data {
                Data::Requirement { target, .. }
                    if matches!(*target, Some(Target::Pin(link) | Target::Transition(link)) if link == id) =>
                    *target = None,
                Data::Transition { from, to, .. } => {
                    if *from == Some(id) { *from = None; }
                    if *to == Some(id) { *to = None; }
                }
                _ => {}
            }
        }
    }

    pub fn mode(&self, id: Id) -> Option<&Mode> { self.modes.iter().find(|mode| mode.id == id) }

    pub fn add_mode(&mut self, name: String) -> Result<Id> {
        let id = self.allocate()?;
        self.modes.push(Mode { id, name, currents: Vec::new() });
        Ok(id)
    }

    pub fn rename_mode(&mut self, id: Id, name: String) -> Result<()> {
        self.modes.iter_mut().find(|mode| mode.id == id).ok_or("Mode no longer exists.")?.name = name;
        Ok(())
    }

    pub fn remove_mode(&mut self, id: Id) {
        self.modes.retain(|mode| mode.id != id);
        for entry in &mut self.spec {
            if let Data::Requirement { rule: Rule::InputPower { mode, .. }, .. } = &mut entry.data {
                if *mode == Some(id) { *mode = None; }
            }
        }
    }

    pub fn set_mode_current(&mut self, id: Id, port: Id, current: Option<Option<u64>>) -> Result<()> {
        if current.is_some() && self.dedicated_converter_input(port) {
            return Err("A dedicated converter input derives current from its output loads.".into());
        }
        let mode = self.modes.iter_mut().find(|mode| mode.id == id).ok_or("Mode no longer exists.")?;
        mode.currents.retain(|entry| entry.port != port);
        if let Some(current_ua) = current {
            mode.currents.push(ModeCurrent { port, current_ua });
            mode.currents.sort_unstable_by_key(|entry| entry.port);
        }
        Ok(())
    }

    pub fn block(&self, id: Id) -> Option<&Block> { self.blocks.iter().find(|b| b.id == id) }
    pub fn port(&self, id: Id) -> Option<&Port> { self.ports.iter().find(|p| p.id == id) }
    pub fn goal(&self, id: Id) -> Option<&Goal> { self.goals.iter().find(|g| g.id == id) }
    pub fn block_ports(&self, id: Id) -> impl Iterator<Item = &Port> {
        self.ports.iter().filter(move |port| port.block == id)
    }
    pub fn port_label(&self, port: &Port) -> String {
        format!("{} / {}", self.block(port.block).map_or("Unknown block", |b| b.name.as_str()), port.name)
    }
    pub fn set_block(&mut self, mut block: Block) -> Result<Id> {
        if block.id == 0 {
            block.id = self.allocate()?;
            let id = block.id;
            self.blocks.push(block);
            return Ok(id);
        }
        let id = block.id;
        let existing = self.blocks.iter_mut().find(|b| b.id == id).ok_or("Block no longer exists.")?;
        block.position = existing.position;
        block.size = existing.size;
        block.power_converter = existing.power_converter;
        block.subsystem = existing.subsystem;
        *existing = block;
        Ok(id)
    }
    pub fn position_block(&mut self, id: Id, position: Option<BlockPosition>) -> Result<()> {
        self.blocks.iter_mut().find(|block| block.id == id)
            .ok_or("Block no longer exists.")?.position = position;
        Ok(())
    }
    pub fn resize_block(&mut self, id: Id, position: Option<BlockPosition>, size: Option<BlockSize>) -> Result<()> {
        let block = self.blocks.iter_mut().find(|block| block.id == id).ok_or("Block no longer exists.")?;
        if let Some(position) = position { block.position = Some(position); }
        block.size = size;
        Ok(())
    }

    pub fn add_block(&mut self, role: Role, name: String, template: bool) -> Result<Id> {
        let id = self.set_block(Block { id: 0, name, role, purpose: String::new(), position: None, size: None, power_converter: false, subsystem: None })?;
        if template {
            let kinds: &[PortKind] = match role {
                Role::Input => &[PortKind::SignalOut, PortKind::Load],
                Role::Output => &[PortKind::SignalIn, PortKind::Load],
                Role::Processing => &[PortKind::SignalIn, PortKind::SignalOut, PortKind::Load],
                Role::Power => &[PortKind::Rail],
                Role::Custom => &[],
            };
            for &kind in kinds { self.set_port(Port::new(id, kind))?; }
        }
        Ok(id)
    }
    pub fn set_port(&mut self, mut port: Port) -> Result<Id> {
        if port.id == 0 {
            port.id = self.allocate()?;
            let id = port.id;
            self.ports.push(port);
            return Ok(id);
        }
        let id = port.id;
        *self.ports.iter_mut().find(|p| p.id == id).ok_or("Port no longer exists.")? = port;
        Ok(id)
    }
    pub fn set_goal(&mut self, mut goal: Goal) -> Result<Id> {
        goal.responsible.sort_unstable();
        goal.responsible.dedup();
        if goal.id == 0 {
            goal.id = self.allocate()?;
            goal.reference = Some(self.allocate_requirement()?);
            let id = goal.id;
            self.goals.push(goal);
            return Ok(id);
        }
        let id = goal.id;
        let stored = self.goals.iter_mut().find(|g| g.id == id).ok_or("Requirement no longer exists.")?;
        goal.reference = stored.reference;
        *stored = goal;
        Ok(id)
    }
    pub fn remove_goal(&mut self, id: Id) { self.goals.retain(|g| g.id != id); }
    pub fn conversion(&self, output: Id) -> Option<&Conversion> {
        self.conversions.iter().find(|channel| channel.output == output)
    }
    pub fn set_conversion(&mut self, channel: Conversion) -> Result<()> {
        let dedicated = self.port(channel.input).and_then(|port| self.block(port.block))
            .is_some_and(|block| block.power_converter);
        if dedicated {
            let port = self.ports.iter_mut().find(|port| port.id == channel.input).ok_or("Missing converter input.")?;
            let kind = if channel.kind.ac_input() { crate::electrical::PowerKind::Ac } else { crate::electrical::PowerKind::Dc };
            if port.power.kind != kind {
                port.power = crate::electrical::PowerSpec { kind, ..Default::default() };
                port.min_uv = None;
                port.max_uv = None;
                port.nominal_uv = None;
                port.voltage_preset = None;
                port.rail_domain.clear();
            }
            port.current_ua = None;
            for mode in &mut self.modes { mode.currents.retain(|current| current.port != channel.input); }
        }
        if let Some(existing) = self.conversions.iter_mut().find(|existing| existing.output == channel.output) {
            *existing = channel;
        } else {
            self.conversions.push(channel);
        }
        Ok(())
    }
    pub fn remove_conversion(&mut self, output: Id) {
        self.conversions.retain(|channel| channel.output != output);
    }
    pub fn remove_port(&mut self, id: Id) -> Result<()> {
        if self.conversions.iter().any(|channel| channel.input == id) {
            return Err("Reassign or remove the converter channels fed by this port before deleting it.".into());
        }
        self.conversions.retain(|channel| channel.output != id);
        self.ports.retain(|p| p.id != id);
        self.bindings.retain(|b| b.source != id && b.target != id);
        for mode in &mut self.modes { mode.currents.retain(|entry| entry.port != id); }
        let ports = HashSet::from([id]);
        let blocks = HashSet::new();
        self.interfaces.unlink(&blocks, &ports);
        for entry in &mut self.spec { entry.unlink_design_objects(&blocks, &ports); }
        Ok(())
    }
    pub fn remove_blocks(&mut self, blocks: &[Id]) -> usize {
        let blocks: HashSet<_> = blocks.iter().copied().collect();
        let ports: HashSet<_> = self.ports.iter()
            .filter(|port| blocks.contains(&port.block))
            .map(|port| port.id).collect();
        self.interfaces.unlink(&blocks, &ports);
        self.blocks.retain(|block| !blocks.contains(&block.id));
        self.ports.retain(|port| !ports.contains(&port.id));
        self.conversions.retain(|channel| !ports.contains(&channel.input) && !ports.contains(&channel.output));
        for mode in &mut self.modes { mode.currents.retain(|entry| !ports.contains(&entry.port)); }
        for entry in &mut self.spec { entry.unlink_design_objects(&blocks, &ports); }
        let previous_links = self.bindings.len();
        self.bindings.retain(|link| !ports.contains(&link.source) && !ports.contains(&link.target));
        self.goals.retain(|goal| !goal.owner.is_some_and(|owner| blocks.contains(&owner)));
        for goal in &mut self.goals {
            goal.responsible.retain(|id| !blocks.contains(id));
        }
        previous_links - self.bindings.len()
    }

    pub fn duplicate_blocks(&mut self, positions: &[(Id, BlockPosition)]) -> Result<Vec<Id>> {
        let mut block_copies = HashMap::new();
        let mut copies = Vec::with_capacity(positions.len());
        for &(original, position) in positions {
            let mut block = self.block(original).cloned().ok_or("Block no longer exists.")?;
            block.id = 0;
            let name = format!("{} copy", block.name);
            block.name = name.clone();
            let mut suffix = 2;
            while self.blocks.iter().any(|existing| existing.name == block.name) {
                block.name = format!("{name} {suffix}");
                suffix += 1;
            }
            block.position = Some(position);
            let id = self.set_block(block)?;
            block_copies.insert(original, id);
            copies.push(id);
        }

        let originals: Vec<_> = self.ports.iter()
            .filter(|port| block_copies.contains_key(&port.block)).cloned().collect();
        let mut port_copies = HashMap::new();
        for mut port in originals {
            let original = port.id;
            port.id = 0;
            port.block = block_copies[&port.block];
            port_copies.insert(original, self.set_port(port)?);
        }
        for mode in &mut self.modes {
            let copies: Vec<_> = mode.currents.iter().filter_map(|entry| {
                Some(ModeCurrent { port: *port_copies.get(&entry.port)?, current_ua: entry.current_ua })
            }).collect();
            mode.currents.extend(copies);
            mode.currents.sort_unstable_by_key(|entry| entry.port);
        }
        let links: Vec<_> = self.bindings.iter().filter_map(|link| {
            Some((*port_copies.get(&link.source)?, *port_copies.get(&link.target)?))
        }).collect();
        for (source, target) in links { self.connect(source, target)?; }

        let channels: Vec<_> = self.conversions.iter().filter_map(|channel| {
            Some(Conversion {
                input: *port_copies.get(&channel.input)?,
                output: *port_copies.get(&channel.output)?,
                ..channel.clone()
            })
        }).collect();
        self.conversions.extend(channels);
        self.duplicate_interfaces(&block_copies, &port_copies)?;

        let goals: Vec<_> = self.goals.iter().filter_map(|goal| {
            let owner = *block_copies.get(&goal.owner?)?;
            let mut copy = goal.clone();
            copy.id = 0;
            copy.owner = Some(owner);
            copy.responsible = goal.responsible.iter().filter_map(|id| block_copies.get(id).copied()).collect();
            Some(copy)
        }).collect();
        for goal in goals { self.set_goal(goal)?; }
        Ok(copies)
    }

    pub fn binding(&self, id: Id) -> Option<&Binding> {
        self.bindings.iter().find(|binding| binding.id == id)
    }

    pub fn route_binding(&mut self, id: Id, route: Vec<BlockPosition>) -> Result<()> {
        let binding = self.bindings.iter_mut().find(|binding| binding.id == id)
            .ok_or("Connection no longer exists.")?;
        binding.bend = None;
        binding.route = route;
        Ok(())
    }

    pub fn unbind(&mut self, id: Id) { self.bindings.retain(|b| b.id != id); }

    pub fn connection_endpoints(&self, first: Id, second: Id) -> Result<(Id, Id)> {
        let first = self.port(first).ok_or("The first port no longer exists.")?;
        let second = self.port(second).ok_or("The second port no longer exists.")?;
        let (source, target) = if first.kind.source() { (first, second) } else { (second, first) };
        if !source.kind.source() || target.kind.source() || source.kind.power() != target.kind.power() {
            return Err("Connect a signal output to a signal input, or a rail to a supply input.".into());
        }
        if self.bindings.iter().any(|binding| binding.target == target.id) {
            return Err(format!("{} is already bound.", self.port_label(target)));
        }
        Ok((source.id, target.id))
    }

    pub fn connect(&mut self, first: Id, second: Id) -> Result<Id> {
        let (source, target) = self.connection_endpoints(first, second)?;
        let id = self.allocate()?;
        self.bindings.push(Binding { id, source, target, bend: None, route: Vec::new() });
        Ok(id)
    }
    pub fn preview_connection(&self, first: Id, second: Id) -> Result<(Self, Id)> {
        let mut candidate = self.clone();
        let id = candidate.connect(first, second)?;
        candidate.sync_protection_rails();
        candidate.validate()?;
        Ok((candidate, id))
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!("Unsupported project schema {}. This build supports schema {SCHEMA_VERSION}.", self.schema_version));
        }
        if uuid::Uuid::parse_str(&self.project_id).is_err() {
            return Err("Project ID must be a UUID.".into());
        }
        if self.title.trim().is_empty() || self.condition.trim().is_empty() {
            return Err("Project title and operating condition are required.".into());
        }
        let mut ids = HashSet::new();
        let mut check_id = |id| {
            if id == 0 || id >= self.next_id || !ids.insert(id) {
                Err(format!("Invalid or duplicate object ID {id}."))
            } else { Ok(()) }
        };
        if self.next_id == 0 { return Err("Invalid next object ID.".into()); }
        for id in self.interfaces.ids() { check_id(id)?; }
        for subsystem in &self.subsystems { check_id(subsystem.id)?; }
        crate::subsystem::validate(self)?;
        crate::interfaces::validate(self)?;
        for block in &self.blocks {
            check_id(block.id)?;
            if block.name.trim().is_empty() { return Err("Block name is required.".into()); }
            if block.size.is_some_and(|size| !size.valid()) { return Err("Invalid canvas block size.".into()); }
        }
        for port in &self.ports {
            check_id(port.id)?;
            if self.block(port.block).is_none() || port.name.trim().is_empty() {
                return Err(format!("Port {} needs a valid block and name.", port.id));
            }
            if (port.kind.power() && !port.signal.trim().is_empty())
                || (!port.kind.power() && (port.min_uv.is_some() || port.max_uv.is_some() || port.current_ua.is_some()))
            {
                return Err(format!("{} has fields that do not match its port type.", self.port_label(port)));
            }
            if (!port.kind.power() && port.nominal_uv.is_some())
                || (!port.kind.power() && !port.rail_domain.is_empty())
                || port.source.is_some_and(|source| port.kind != PortKind::Rail
                    || self.conversion(port.id).is_some()
                    || !source.preset.matches_power(port.power)
                    || if source.preset.battery() { !(1..=100).contains(&source.series) } else { source.series != 1 })
            {
                return Err(format!("{} has voltage or source settings that do not match its port type.", self.port_label(port)));
            }
            port.power.validate()?;
            if let Some(signal) = &port.signal_spec {
                if port.kind.power() { return Err("Use signal characteristics on signal ports.".into()); }
                crate::signal::validate(signal)?;
            }
            if !port.kind.power() && !port.power.is_default() {
                return Err("Use power characteristics on power ports.".into());
            }
            if let Some(battery) = port.battery {
                if port.kind != PortKind::Rail || !port.power.positive_dc()
                    || !port.source.is_some_and(|source| source.preset.battery())
                    || port.source.and_then(|source| source.preset.default_cell())
                        .is_some_and(|cell| cell.chemistry() != battery.cell.chemistry())
                    || !(1..=100).contains(&battery.parallel)
                {
                    return Err("Battery capacity requires a positive DC battery source with 1 to 100 parallel cells.".into());
                }
            }
            if port.voltage_preset.is_some_and(|preset| !port.kind.power() || !preset.valid()) {
                return Err(format!("{} has an invalid voltage preset.", self.port_label(port)));
            }
            if port.nominal_uv.is_some_and(|nominal| nominal == 0
                || port.min_uv.is_some_and(|min| nominal < min)
                || port.max_uv.is_some_and(|max| nominal > max))
            {
                return Err(format!("{} needs a positive nominal voltage within its declared bounds.", self.port_label(port)));
            }
            if port.range().is_some_and(|(min, max)| min > max) {
                return Err(format!("{} has a minimum voltage above its maximum.", self.port_label(port)));
            }
        }
        for goal in &self.goals {
            check_id(goal.id)?;
            let mut owners = HashSet::new();
            if goal.description.trim().is_empty()
                || goal.owner.is_some_and(|id| self.block(id).is_none())
                || goal.responsible.iter().any(|id| self.block(*id).is_none() || !owners.insert(*id))
            {
                return Err(format!("Requirement {} needs a description and valid, unique block references.", goal.id));
            }
        }
        let mut names = HashSet::from([self.condition.trim().to_lowercase()]);
        for mode in &self.modes {
            check_id(mode.id)?;
            let name = mode.name.trim();
            if name.is_empty() || !names.insert(name.to_lowercase()) {
                return Err("Operating mode names must be nonempty and unique.".into());
            }
            let mut ports = HashSet::new();
            for entry in &mode.currents {
                if !ports.insert(entry.port) || !self.port(entry.port).is_some_and(|port| port.kind.power()) {
                    return Err("Each mode override must reference a distinct power port.".into());
                }
            }
        }
        let mut targets = HashSet::new();
        for binding in &self.bindings {
            check_id(binding.id)?;
            if !binding.route.is_empty() && (binding.bend.is_some() || binding.route.len() < 2
                || binding.route.windows(2).any(|pair| pair[0] == pair[1]
                    || pair[0].x != pair[1].x && pair[0].y != pair[1].y))
            {
                return Err("A wire route must contain distinct orthogonal vertices.".into());
            }
            let source = self.port(binding.source).ok_or("A binding has a missing source.")?;
            let target = self.port(binding.target).ok_or("A binding has a missing target.")?;
            if !source.kind.source() || target.kind.source() || source.kind.power() != target.kind.power() {
                return Err("Connect a signal output to a signal input, or a rail to a supply input.".into());
            }
            if !targets.insert(target.id) {
                return Err(format!("{} is already bound.", self.port_label(target)));
            }
        }
        for entry in &self.spec { check_id(entry.id)?; }
        let mut references = HashSet::new();
        for reference in self.goals.iter().map(|goal| goal.reference)
            .chain(self.spec.iter().filter(|entry| matches!(&entry.data, Data::Requirement { .. }))
                .map(|entry| entry.reference))
        {
            if !reference.is_some_and(|number| number > 0 && number < self.next_requirement && references.insert(number)) {
                return Err("Requirements need unique, positive project references.".into());
            }
        }
        power::validate(self)?;
        spec::validate(self)
    }

    pub fn monitor() -> Result<Self> {
        let mut p = Self::new();
        p.title = "Environment monitor".into();
        p.purpose = "Measure temperature and show a local indication.".into();
        let sensor = p.add_block(Role::Input, "Temperature sensor".into(), false)?;
        let controller = p.add_block(Role::Processing, "Controller".into(), false)?;
        let indicator = p.add_block(Role::Output, "Local indicator".into(), false)?;
        let supply = p.add_block(Role::Power, "3.3 V supply".into(), false)?;
        for (src, dst, kind) in [(sensor, controller, "Measurement"), (controller, indicator, "Control")] {
            let mut output = Port::new(src, PortKind::SignalOut);
            output.name = kind.into();
            output.signal = kind.into();
            let mut input = Port::new(dst, PortKind::SignalIn);
            input.name = kind.into();
            input.signal = kind.into();
            let source = p.set_port(output)?;
            let target = p.set_port(input)?;
            p.connect(source, target)?;
        }
        let mut rail = Port::new(supply, PortKind::Rail);
        rail.name = "3V3".into();
        rail.min_uv = Some(3_250_000);
        rail.max_uv = Some(3_350_000);
        rail.current_ua = Some(300_000);
        rail.note = "Illustrative capacity for normal operation.".into();
        let rail_id = p.set_port(rail)?;
        for (block, current) in [(sensor, 35), (controller, 160), (indicator, 15)] {
            let mut load = Port::new(block, PortKind::Load);
            load.name = "3V3 input".into();
            load.min_uv = Some(3_100_000);
            load.max_uv = Some(3_500_000);
            load.current_ua = Some(current * 1000);
            load.note = "Illustrative maximum in normal operation.".into();
            let load_id = p.set_port(load)?;
            p.connect(rail_id, load_id)?;
        }
        p.set_goal(Goal {
            id: 0, reference: None, owner: None, description: "Report ambient temperature".into(),
            method: "Compare the reported value against a reference under defined conditions.".into(),
            note: "Reference project goal.".into(),
            responsible: vec![sensor, controller],
        })?;
        p.validate()?;
        Ok(p)
    }

    pub fn modes_demo() -> Result<Self> {
        let mut project = Self::monitor()?;
        project.title = "Environment monitor · operating modes".into();
        project.condition = "Idle".into();
        let alert = project.add_alert()?;
        let load = project.block_ports(alert).find(|port| port.kind == PortKind::Load)
            .cloned().ok_or("Sample alert load missing.")?;
        let rail = project.ports.iter().find(|port| port.kind == PortKind::Rail)
            .map(|port| port.id).ok_or("Sample supply missing.")?;
        let mut idle_load = load;
        idle_load.current_ua = Some(0);
        idle_load.note = "Illustrative alert load. Set in each operating mode.".into();
        let load = project.set_port(idle_load)?;
        project.connect(rail, load)?;
        let active = project.add_mode("Alert active".into())?;
        project.set_mode_current(active, load, Some(Some(110_000)))?;
        let unknown = project.add_mode("Alert draw unknown".into())?;
        project.set_mode_current(unknown, load, Some(None))?;
        project.validate()?;
        Ok(project)
    }

    pub fn add_alert(&mut self) -> Result<Id> {
        let id = self.add_block(Role::Output, "Alert output".into(), false)?;
        let mut input = Port::new(id, PortKind::SignalIn);
        input.name = "Alert control".into();
        input.signal = "Control".into();
        self.set_port(input)?;
        let mut load = Port::new(id, PortKind::Load);
        load.name = "3V3 input".into();
        load.min_uv = Some(3_100_000);
        load.max_uv = Some(3_500_000);
        load.current_ua = Some(110_000);
        load.note = "Illustrative maximum in normal operation.".into();
        self.set_port(load)?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_metadata_rejects_incompatible_power_and_battery_chemistry() -> Result<()> {
        use crate::{electrical::{Cell, PowerKind}, rail::{Preset, Source}};
        let mut project = Project::new();
        let block = project.add_block(Role::Power, "Supply".into(), false)?;
        let mut port = Port::new(block, PortKind::Rail);
        port.source = Some(Source { preset: Preset::Usb5, series: 1, contract_confirmed: false });
        port.id = project.set_port(port.clone())?;
        port.power.kind = PowerKind::Ac;
        project.set_port(port.clone())?;
        assert!(project.validate().is_err());
        port.power.kind = PowerKind::Dc;
        port.source.as_mut().unwrap().preset = Preset::LiIon;
        port.battery = Some(Cell::NiMhAa.default_pack(1));
        project.set_port(port.clone())?;
        assert!(project.validate().is_err());
        port.battery = Some(Cell::LiIon21700.default_pack(1));
        project.set_port(port.clone())?;
        project.validate()?;
        port.source = None;
        port.battery = None;
        port.power.kind = PowerKind::Ac;
        project.set_port(port)?;
        project.validate()
    }

    #[test]
    fn deleting_blocks_removes_only_their_references() -> Result<()> {
        let mut project = Project::monitor()?;
        let sensor = project.blocks[0].id;
        let controller = project.blocks[1].id;
        let mut owned = Goal::new(Some(sensor));
        owned.description = "Calibrate sensor".into();
        let owned = project.set_goal(owned)?;

        assert_eq!(project.remove_blocks(&[sensor]), 2);
        assert!(project.block(sensor).is_none());
        assert!(project.block_ports(sensor).next().is_none());
        assert!(project.goal(owned).is_none());
        assert_eq!(project.goals[0].responsible, vec![controller]);
        assert_eq!(project.bindings.len(), 3);
        project.validate()
    }

    #[test]
    fn duplicating_blocks_remaps_internal_connections_and_requirements() -> Result<()> {
        let mut project = Project::monitor()?;
        let sensor = project.blocks[0].id;
        let controller = project.blocks[1].id;
        let indicator = project.blocks[2].id;
        let mut goal = Goal::new(Some(sensor));
        goal.description = "Review data path".into();
        goal.responsible = vec![controller, indicator];
        project.set_goal(goal)?;
        let original_bindings = project.bindings.len();
        let copies = project.duplicate_blocks(&[
            (sensor, BlockPosition { x: 900, y: 0 }),
            (controller, BlockPosition { x: 1150, y: 0 }),
        ])?;

        let copied_link = &project.bindings[original_bindings];
        assert_eq!(project.bindings.len(), original_bindings + 1);
        assert_eq!(project.port(copied_link.source).map(|port| port.block), Some(copies[0]));
        assert_eq!(project.port(copied_link.target).map(|port| port.block), Some(copies[1]));
        let copied_goal = project.goals.last().expect("Copied requirement");
        assert_eq!(copied_goal.owner, Some(copies[0]));
        assert_eq!(copied_goal.responsible, vec![copies[1]]);
        project.validate()
    }
}
