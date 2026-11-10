use crate::model::{Id, Mode, PortKind, Project, Result, Role};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Buck, Boost, BuckBoost, Sepic, Zeta, ChargePump, Linear, Ldo,
    DcFlyback, DcForward, PushPull, DcHalfBridge, DcFullBridge, DcLlc,
    AcDc, AcFlyback, AcForward, AcLlc, AcPfc, AcLinear,
}

impl Kind {
    pub const DC: [Self; 14] = [
        Self::Buck, Self::Boost, Self::BuckBoost, Self::Sepic, Self::Zeta,
        Self::ChargePump, Self::Linear, Self::Ldo, Self::DcFlyback, Self::DcForward,
        Self::PushPull, Self::DcHalfBridge, Self::DcFullBridge, Self::DcLlc,
    ];
    pub const AC: [Self; 6] = [Self::AcDc, Self::AcFlyback, Self::AcForward, Self::AcLlc, Self::AcPfc, Self::AcLinear];

    pub fn ac_input(self) -> bool { Self::AC.contains(&self) }
    pub fn linear_dc(self) -> bool { matches!(self, Self::Linear | Self::Ldo) }

    pub fn label(self) -> &'static str {
        match self {
            Self::Buck => "Buck", Self::Boost => "Boost", Self::BuckBoost => "Non-inverting buck-boost",
            Self::Sepic => "SEPIC", Self::Zeta => "Zeta", Self::ChargePump => "Charge pump",
            Self::Linear => "Linear regulator", Self::Ldo => "LDO",
            Self::DcFlyback => "DC/DC flyback", Self::DcForward => "DC/DC forward",
            Self::PushPull => "Push-pull", Self::DcHalfBridge => "DC/DC half bridge",
            Self::DcFullBridge => "DC/DC full bridge", Self::DcLlc => "DC/DC LLC resonant",
            Self::AcDc => "AC/DC supply", Self::AcFlyback => "AC/DC flyback",
            Self::AcForward => "AC/DC forward", Self::AcLlc => "AC/DC LLC resonant",
            Self::AcPfc => "Rectifier + PFC front end", Self::AcLinear => "Transformer + rectifier + linear regulator",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationClass { #[default] Unspecified, None, Functional, Basic, Supplementary, Double, Reinforced }

impl IsolationClass {
    pub const ALL: [Self; 7] = [Self::Unspecified, Self::None, Self::Functional, Self::Basic, Self::Supplementary, Self::Double, Self::Reinforced];

    pub fn label(self) -> &'static str {
        match self {
            Self::Unspecified => "Unspecified", Self::None => "Non-isolated", Self::Functional => "Functional",
            Self::Basic => "Basic", Self::Supplementary => "Supplementary", Self::Double => "Double",
            Self::Reinforced => "Reinforced",
        }
    }

    pub fn barrier(self) -> bool { !matches!(self, Self::Unspecified | Self::None) }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Isolation {
    pub class: IsolationClass,
    pub working_uv: Option<u64>,
    pub withstand_uv: Option<u64>,
    pub test_ms: Option<u64>,
    pub creepage_um: Option<u64>,
    pub clearance_um: Option<u64>,
    pub evidence: String,
}

impl Isolation {
    pub fn is_empty(&self) -> bool { self == &Self::default() }
    pub fn complete(&self) -> bool {
        [self.working_uv, self.withstand_uv, self.test_ms, self.creepage_um, self.clearance_um]
            .into_iter().all(|value| value.is_some_and(|number| number > 0)) && !self.evidence.trim().is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conversion {
    pub input: Id,
    pub output: Id,
    pub kind: Kind,
    pub efficiency_bp: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_pf_bp: Option<u16>,
    pub ground_ua: Option<u64>,
    pub dropout_uv: Option<u64>,
    #[serde(default, skip_serializing_if = "Isolation::is_empty")]
    pub isolation: Isolation,
}

impl Conversion {
    pub fn new(input: Id, output: Id, kind: Kind) -> Self {
        Self { input, output, kind, efficiency_bp: None, input_pf_bp: None, ground_ua: None,
            dropout_uv: None, isolation: Isolation::default() }
    }
}

impl Project {
    pub fn add_power_converter(&mut self, name: String, kind: Kind) -> Result<Id> {
        let block = self.add_block(Role::Power, name, false)?;
        self.blocks.iter_mut().find(|item| item.id == block).unwrap().power_converter = true;
        let mut input = crate::model::Port::new(block, PortKind::Load);
        input.name = "Power input".into();
        let mut output = crate::model::Port::new(block, PortKind::Rail);
        output.name = "Regulated output".into();
        let input = self.set_port(input)?;
        let output = self.set_port(output)?;
        self.set_conversion(Conversion::new(input, output, kind))?;
        Ok(block)
    }

    pub fn dedicated_converter_input(&self, id: Id) -> bool {
        self.port(id).filter(|port| port.kind == PortKind::Load)
            .and_then(|port| self.block(port.block)).is_some_and(|block| block.power_converter)
            && self.conversions.iter().any(|channel| channel.input == id)
    }
}

pub fn validate(project: &Project) -> Result<()> {
    let mut outputs = std::collections::HashSet::new();
    for channel in &project.conversions {
        let input = project.port(channel.input).ok_or("A converter input is missing.")?;
        let output = project.port(channel.output).ok_or("A converter output is missing.")?;
        if !input.kind.power() || output.kind != PortKind::Rail || input.block != output.block || input.id == output.id {
            return Err("A converter must use a supply input or internal rail and a different output rail on the same block.".into());
        }
        if !outputs.insert(output.id) {
            return Err(format!("{} has more than one converter.", project.port_label(output)));
        }
        if !output.power.positive_dc()
            || (channel.kind.ac_input() && input.power.kind != crate::electrical::PowerKind::Ac)
            || (!channel.kind.ac_input() && !input.power.positive_dc())
        {
            return Err("Use an AC feeder for an AC/DC channel and positive DC ports for DC conversion.".into());
        }
        if channel.efficiency_bp.is_some_and(|bp| bp == 0 || bp > 10_000)
            || channel.input_pf_bp.is_some_and(|bp| bp == 0 || bp > 10_000)
            || channel.kind.linear_dc() && (channel.efficiency_bp.is_some() || channel.input_pf_bp.is_some())
            || !channel.kind.linear_dc() && (channel.ground_ua.is_some() || channel.dropout_uv.is_some())
            || !channel.kind.ac_input() && channel.input_pf_bp.is_some()
        {
            return Err("Use valid channel fields and efficiency / power factor above zero and at most 100%.".into());
        }
        let isolation = &channel.isolation;
        if [isolation.working_uv, isolation.withstand_uv, isolation.test_ms, isolation.creepage_um, isolation.clearance_um]
            .into_iter().any(|value| value == Some(0))
        {
            return Err("Isolation ratings must be positive or blank.".into());
        }
        if project.block(input.block).is_some_and(|block| block.power_converter) && input.kind != PortKind::Load {
            return Err("A dedicated converter channel needs a power input as its feeder.".into());
        }
    }
    for block in project.blocks.iter().filter(|block| block.power_converter) {
        if !project.conversions.iter().any(|channel| project.port(channel.output).is_some_and(|port| port.block == block.id)) {
            return Err(format!("{} needs a converter input and output. Remove the block to delete its last channel.", block.name));
        }
    }
    for protection in &project.interfaces.protections {
        if !protection.shunt && project.port(protection.entry).is_some_and(|p| p.kind.power()) {
            if let Some(exit) = protection.exit {
                if !outputs.insert(exit) { return Err("A power output cannot have multiple converter or protection paths.".into()); }
            }
        }
    }
    let graph = Graph::new(project);
    if graph.order().len() != graph.edges.len() {
        return Err("This connection creates a power loop. Each converter needs a path back to a source.".into());
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(crate) enum Edge { Wire(Id), Conversion(Id), Pass(Id), Unspecified(Id) }

impl Edge {
    pub(crate) fn target(self) -> Id {
        match self { Self::Wire(id) | Self::Conversion(id) | Self::Pass(id) | Self::Unspecified(id) => id }
    }
}

struct Graph { edges: HashMap<Id, Vec<Edge>> }

impl Graph {
    fn new(project: &Project) -> Self {
        let mut edges: HashMap<_, Vec<_>> = project.ports.iter()
            .filter(|port| port.kind.power()).map(|port| (port.id, Vec::new())).collect();
        for binding in &project.bindings {
            if let Some(list) = edges.get_mut(&binding.source) {
                list.push(Edge::Wire(binding.target));
            }
        }
        for channel in &project.conversions {
            edges.entry(channel.input).or_default().push(Edge::Conversion(channel.output));
        }
        for protection in &project.interfaces.protections {
            if !protection.shunt && project.port(protection.entry).is_some_and(|p| p.kind.power()) {
                if let Some(exit) = protection.exit { edges.entry(protection.entry).or_default().push(Edge::Pass(exit)); }
            }
        }
        Self { edges }
    }

    fn reaches(&self, from: Id, target: Id) -> bool {
        let mut visited = HashSet::new();
        let mut pending = vec![from];
        while let Some(id) = pending.pop() {
            if id == target { return true; }
            if visited.insert(id) {
                pending.extend(self.edges.get(&id).into_iter().flatten().map(|edge| edge.target()));
            }
        }
        false
    }

    fn add_unspecified_channels(&mut self, project: &Project, providers: &HashMap<Id, Id>) -> HashMap<Id, Id> {
        let mut channels = HashMap::new();
        for output in project.ports.iter().filter(|port| port.kind == PortKind::Rail) {
            if output.independent_rail || output.source.is_some() || project.conversion(output.id).is_some()
                || project.block(output.block).is_some_and(|block| block.power_converter)
                || project.interfaces.connectors.iter().any(|connector| connector.block == output.block)
                || project.interfaces.protections.iter().any(|stage| stage.block == output.block)
            { continue; }

            let mut feeders = project.block_ports(output.block).filter(|port| port.kind == PortKind::Load);
            let Some(input) = feeders.next() else { continue };
            if feeders.next().is_some() || !providers.contains_key(&input.id) || self.reaches(output.id, input.id) { continue; }
            self.edges.entry(input.id).or_default().push(Edge::Unspecified(output.id));
            channels.insert(output.id, input.id);
        }
        channels
    }

    fn order(&self) -> Vec<Id> {
        let mut incoming: HashMap<_, _> = self.edges.keys().map(|&id| (id, 0usize)).collect();
        for edge in self.edges.values().flatten() {
            *incoming.entry(edge.target()).or_default() += 1;
        }
        let mut roots: Vec<_> = incoming.iter().filter_map(|(&id, &n)| (n == 0).then_some(id)).collect();
        roots.sort_unstable();
        let mut queue = VecDeque::from(roots);
        let mut order = Vec::with_capacity(incoming.len());
        while let Some(id) = queue.pop_front() {
            order.push(id);
            if let Some(edges) = self.edges.get(&id) {
                for edge in edges {
                    let count = incoming.get_mut(&edge.target()).expect("Target in power graph");
                    *count -= 1;
                    if *count == 0 { queue.push_back(edge.target()); }
                }
            }
        }
        order
    }
}

impl Project {
    pub fn inherited_rail_input(&self, rail: Id) -> Option<Id> {
        self.interfaces.protections.iter().find(|stage| !stage.shunt && !stage.manual_rail_voltage
            && stage.exit == Some(rail) && self.port(stage.entry).is_some_and(|port| port.kind.power()))
            .map(|stage| stage.entry)
    }

    pub(crate) fn preserve_legacy_protection_settings(&mut self) {
        let manual: HashSet<_> = self.interfaces.protections.iter().filter_map(|stage| {
            let output = self.port(stage.exit?)?;
            let input = self.port(stage.entry)?;
            (output.kind.power() && (output.min_uv.is_some() || output.max_uv.is_some() || output.nominal_uv.is_some())
                && (output.range(), output.nominal_uv) != (input.range(), input.nominal_uv)).then_some(stage.id)
        }).collect();
        for stage in &mut self.interfaces.protections {
            stage.manual_rail_voltage |= manual.contains(&stage.id);
        }
    }

    pub fn sync_protection_rails(&mut self) {
        let graph = Graph::new(self);
        let inherited: HashMap<_, _> = self.ports.iter().filter_map(|output| {
            Some((output.id, self.inherited_rail_input(output.id)?))
        }).collect();
        for id in graph.order() {
            let Some(&input_id) = inherited.get(&id) else { continue };
            let Some(input) = self.port(input_id) else { continue };
            let provider = self.bindings.iter().find(|wire| wire.target == input_id)
                .and_then(|wire| self.port(wire.source))
                .filter(|port| port.power.kind == input.power.kind && port.power.polarity == input.power.polarity);
            let source = provider.filter(|port| port.range().is_some()).unwrap_or(input);
            let values = (source.min_uv, source.max_uv, source.nominal_uv, source.power, source.rail_domain.clone());
            if let Some(output) = self.ports.iter_mut().find(|port| port.id == id) {
                (output.min_uv, output.max_uv, output.nominal_uv, output.power, output.rail_domain) = values;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Demand { pub known_ua: u128, pub incomplete: bool }

impl Demand {
    const UNKNOWN: Self = Self { known_ua: 0, incomplete: true };

    fn declared(value: Option<u64>) -> Self {
        value.map_or(Self::UNKNOWN, |value| Self { known_ua: value.into(), incomplete: false })
    }

    fn add(self, other: Self) -> Self {
        Self {
            known_ua: self.known_ua.saturating_add(other.known_ua),
            incomplete: self.incomplete || other.incomplete,
        }
    }
}

#[derive(Clone, Copy)]
enum VoltageBasis { Limits, Nominal, Test { input: Id, voltage_uv: u64 } }

impl VoltageBasis {
    fn voltage(self, port: &crate::model::Port, higher: bool) -> Option<u64> {
        match self {
            Self::Nominal => port.nominal_voltage(),
            _ if higher => port.max_uv,
            _ => port.min_uv,
        }
    }

    fn input_voltage(self, id: Id, provider: Option<&crate::model::Port>, higher: bool) -> Option<u64> {
        if let Self::Test { input, voltage_uv } = self {
            if id == input { return Some(voltage_uv); }
        }
        provider.and_then(|port| self.voltage(port, higher))
    }
}

pub struct Network {
    graph: Graph,
    pending: HashSet<Id>,
    demands: HashMap<Id, Demand>,
    transfers: HashMap<Id, Transfer>,
    unspecified: HashMap<Id, Id>,
}

impl Network {
    pub fn new(project: &Project, mode: Option<&Mode>) -> Self {
        Self::calculate(project, mode, VoltageBasis::Limits)
    }

    pub fn nominal(project: &Project, mode: Option<&Mode>) -> Self {
        Self::calculate(project, mode, VoltageBasis::Nominal)
    }

    pub fn at_test_voltage(project: &Project, mode: Option<&Mode>, input: Id, voltage_uv: u64) -> Self {
        Self::calculate(project, mode, VoltageBasis::Test { input, voltage_uv })
    }

    fn calculate(project: &Project, mode: Option<&Mode>, basis: VoltageBasis) -> Self {
        let mut graph = Graph::new(project);
        let ports: HashMap<_, _> = project.ports.iter().map(|port| (port.id, port)).collect();
        let channels: HashMap<_, _> = project.conversions.iter().map(|channel| (channel.output, channel)).collect();
        let providers: HashMap<_, _> = project.bindings.iter().map(|binding| (binding.target, binding.source)).collect();
        let unspecified = graph.add_unspecified_channels(project, &providers);
        let order = graph.order();
        let mut pending = HashSet::new();
        for &id in &order {
            if ports.get(&id).is_some_and(|port| crate::rail::needs_contract(port)) || pending.contains(&id) {
                pending.insert(id);
                if let Some(edges) = graph.edges.get(&id) {
                    pending.extend(edges.iter().map(|edge| edge.target()));
                }
            }
        }
        let mut network = Self { graph, pending, demands: HashMap::new(), transfers: HashMap::new(), unspecified };
        for id in order.into_iter().rev() {
            let Some(port) = ports.get(&id) else { continue };
            let edges = &network.graph.edges[&id];
            let mut demand = network.direct_draw(project, port, mode).0;
            for edge in edges {
                let child = network.demand(edge.target());
                let contribution = match *edge {
                    Edge::Pass(_) => child,
                    Edge::Wire(target) => ports.get(&target).map_or(Demand::UNKNOWN, |target| network.wire_draw(port, target)),
                    Edge::Unspecified(output) => {
                        let feeder = providers.get(&id).and_then(|id| ports.get(id)).copied();
                        let vout = ports.get(&output).and_then(|port| basis.voltage(port, false));
                        let vin = basis.input_voltage(id, feeder, true);
                        let contribution = if feeder.is_some_and(|port| port.power.positive_dc())
                            && ports.get(&output).is_some_and(|port| port.power.positive_dc())
                        {
                            ideal_lower_bound(child, vin, vout)
                        } else { Transfer::unknown("A compatible positive DC input and output are required.") };
                        network.transfers.insert(output, contribution);
                        contribution.demand
                    }
                    Edge::Conversion(output) => {
                        let channel = channels[&output];
                        let provider_id = if port.kind == PortKind::Rail { Some(id) } else { providers.get(&id).copied() };
                        let provider = provider_id.and_then(|id| ports.get(&id)).copied();
                        let vin = basis.input_voltage(id, provider, false);
                        let vout = ports.get(&output).and_then(|port| basis.voltage(port, true));
                        let supply = provider.map(|port| port.power);
                        let supply_compatible = supply.is_some_and(|power| {
                            if channel.kind.ac_input() { power.kind == crate::electrical::PowerKind::Ac && power.phases == 1 }
                            else { power.positive_dc() }
                        });
                        let input = if supply_compatible {
                            let draw = transfer(channel, child, vin, vout);
                            let vin_high = basis.input_voltage(id, provider, true);
                            let vout_low = ports.get(&output).and_then(|port| basis.voltage(port, false));
                            if draw.demand == Demand::UNKNOWN && !channel.kind.linear_dc() {
                                ideal_lower_bound(child, vin_high, vout_low).with_fallback(draw.equation)
                            } else { draw }
                        } else { Transfer::unknown("The connected input supply is missing or incompatible with the topology.") };
                        network.transfers.insert(output, input);
                        input.demand
                    }
                };
                demand = demand.add(contribution);
            }
            network.demands.insert(id, demand);
        }
        network
    }

    pub fn transfer_input(&self, input: Id) -> bool {
        self.graph.edges.get(&input).is_some_and(|edges| edges.iter().any(|edge| !matches!(edge, Edge::Wire(_))))
    }

    pub fn inferred_feeder(&self, output: Id) -> Option<Id> { self.unspecified.get(&output).copied() }

    pub fn awaiting_contract(&self, port: Id) -> bool { self.pending.contains(&port) }

    pub fn downstream(&self, input: Id) -> HashSet<Id> {
        let mut seen = HashSet::new();
        let mut pending = vec![input];
        while let Some(id) = pending.pop() {
            if seen.insert(id) {
                if let Some(edges) = self.graph.edges.get(&id) {
                    pending.extend(edges.iter().map(|edge| edge.target()));
                }
            }
        }
        seen
    }

    pub fn demand(&self, port: Id) -> Demand {
        self.demands.get(&port).copied().unwrap_or(Demand::UNKNOWN)
    }

    pub fn transfer(&self, output: Id) -> Demand {
        self.calculation(output).map_or(Demand::UNKNOWN, |transfer| transfer.demand)
    }

    pub(crate) fn calculation(&self, output: Id) -> Option<Transfer> {
        self.transfers.get(&output).copied()
    }

    pub(crate) fn ordered_ports(&self) -> Vec<Id> { self.graph.order() }

    pub(crate) fn edges(&self, id: Id) -> &[Edge] {
        self.graph.edges.get(&id).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn direct_draw(&self, project: &Project, port: &crate::model::Port, mode: Option<&Mode>) -> (Demand, &'static str) {
        if port.kind != PortKind::Load { return (Demand::default(), "Rail output has no local draw"); }
        if project.dedicated_converter_input(port.id) {
            return (Demand::default(), "Dedicated converter input draws through its conversion channel");
        }
        if let Some(current) = mode.and_then(|mode| mode.current_override(port.id)) {
            return (Demand::declared(current), "Operating-mode current override");
        }
        if let Some(current) = port.current_ua {
            return (Demand::declared(Some(current)), "Port current in project");
        }
        if !self.edges(port.id).is_empty() {
            return (Demand::default(), "No additional input current declared");
        }
        (Demand::UNKNOWN, "Port current is undeclared")
    }

    pub(crate) fn wire_draw(&self, source: &crate::model::Port, target: &crate::model::Port) -> Demand {
        if source.power.kind == target.power.kind && source.power.polarity == target.power.polarity {
            self.demand(target.id)
        } else {
            Demand::UNKNOWN
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Transfer {
    pub demand: Demand,
    pub equation: Equation,
    pub fallback: Option<&'static str>,
}

#[derive(Clone, Copy)]
pub(crate) enum Equation {
    Switching { vin: u64, vout: u64, eta: u16, pf: Option<u16> },
    Linear { ground: Option<u64> },
    Ideal { vin: u64, vout: u64 },
    Unknown(&'static str),
}

impl Transfer {
    fn new(demand: Demand, equation: Equation) -> Self { Self { demand, equation, fallback: None } }
    fn unknown(reason: &'static str) -> Self { Self::new(Demand::UNKNOWN, Equation::Unknown(reason)) }

    fn with_fallback(self, original: Equation) -> Self {
        match (self.equation, original) {
            (Equation::Ideal { .. }, Equation::Unknown(reason)) => Self { fallback: Some(reason), ..self },
            (Equation::Unknown(_), Equation::Unknown(reason)) => Self::unknown(reason),
            _ => self,
        }
    }
}

fn ideal_lower_bound(output: Demand, vin: Option<u64>, vout: Option<u64>) -> Transfer {
    let (Some(vin), Some(vout)) = (vin, vout) else { return Transfer::unknown("The voltage bounds needed for a lower bound are missing."); };
    if vin == 0 || vout == 0 { return Transfer::unknown("A voltage needed for the lower bound is zero."); }
    let Some(power) = output.known_ua.checked_mul(u128::from(vout)) else { return Transfer::unknown("The lower-bound calculation exceeds the supported range."); };
    Transfer::new(Demand { known_ua: power.div_ceil(u128::from(vin)), incomplete: true }, Equation::Ideal { vin, vout })
}

fn transfer(channel: &Conversion, output: Demand, vin: Option<u64>, vout: Option<u64>) -> Transfer {
    if channel.kind.linear_dc() {
        if matches!((vin, vout, channel.dropout_uv),
            (Some(vin), Some(vout), Some(dropout)) if !has_headroom(vin, vout, dropout))
        {
            return Transfer::unknown("The declared dropout exceeds the available input headroom.");
        }
        return Transfer::new(output.add(Demand::declared(channel.ground_ua)), Equation::Linear { ground: channel.ground_ua });
    }

    let (Some(vin), Some(vout)) = (vin, vout) else { return Transfer::unknown("The required input or output voltage is missing."); };
    let Some(eta) = channel.efficiency_bp else { return Transfer::unknown("Converter efficiency is undeclared."); };
    if vin == 0 || vout == 0 || eta == 0 {
        return Transfer::unknown("A required voltage or efficiency is zero.");
    }
    if channel.kind == Kind::Buck && vin <= vout || channel.kind == Kind::Boost && vin >= vout {
        return Transfer::unknown("The input and output voltages do not suit the declared topology.");
    }
    let pf = if channel.kind.ac_input() {
        let Some(pf) = channel.input_pf_bp else { return Transfer::unknown("Input power factor is undeclared."); };
        Some(pf)
    } else { None };
    let denominator = u128::from(vin) * u128::from(eta) * u128::from(pf.unwrap_or(10_000));
    let Some(numerator) = output.known_ua.checked_mul(vout.into()).and_then(|value| value.checked_mul(100_000_000))
        else { return Transfer::unknown("The converter calculation exceeds the supported range."); };
    Transfer::new(Demand { known_ua: numerator.div_ceil(denominator), incomplete: output.incomplete },
        Equation::Switching { vin, vout, eta, pf })
}

pub fn has_headroom(vin: u64, vout: u64, dropout: u64) -> bool {
    u128::from(vin) >= u128::from(vout) + u128::from(dropout)
}

pub fn example(pmic: bool) -> Result<Project> {
    let mut project = Project::new();
    project.title = if pmic { "Multi-output PMIC" } else { "Cascaded power tree" }.into();
    let source = project.add_block(Role::Power, "12 V source".into(), false)?;
    let first = project.add_block(Role::Power, if pmic { "PMIC" } else { "5 V buck" }.into(), false)?;
    let second = if pmic { first } else { project.add_block(Role::Power, "3.3 V buck".into(), false)? };
    let source_out = example_port(&mut project, source, PortKind::Rail, "12 V", 12_000_000, Some(900_000))?;
    let first_in = example_port(&mut project, first, PortKind::Load, "VIN", 12_000_000, None)?;
    let five = example_port(&mut project, first, PortKind::Rail, "5 V", 5_000_000, Some(900_000))?;
    let second_in = if pmic { five } else {
        let input = example_port(&mut project, second, PortKind::Load, "VIN", 5_000_000, None)?;
        project.connect(five, input)?;
        input
    };
    let three = example_port(&mut project, second, PortKind::Rail, "3.3 V", 3_300_000, Some(500_000))?;
    project.connect(source_out, first_in)?;
    project.set_conversion(Conversion { input: first_in, output: five, kind: Kind::Buck,
        efficiency_bp: Some(9000), input_pf_bp: None, ground_ua: None, dropout_uv: None, isolation: Isolation::default() })?;
    project.set_conversion(if pmic {
        Conversion { input: second_in, output: three, kind: Kind::Ldo,
            efficiency_bp: None, input_pf_bp: None, ground_ua: Some(100), dropout_uv: Some(250_000), isolation: Isolation::default() }
    } else {
        Conversion { input: second_in, output: three, kind: Kind::Buck,
            efficiency_bp: Some(9000), input_pf_bp: None, ground_ua: None, dropout_uv: None, isolation: Isolation::default() }
    })?;
    for (name, rail, voltage, current) in [
        ("5 V load", five, 5_000_000, 100_000), ("3.3 V load", three, 3_300_000, 300_000),
    ] {
        let block = project.add_block(Role::Custom, name.into(), false)?;
        let load = example_port(&mut project, block, PortKind::Load, "Supply input", voltage, Some(current))?;
        project.connect(rail, load)?;
    }
    if pmic {
        let rail = example_port(&mut project, first, PortKind::Rail, "1.8 V", 1_800_000, Some(500_000))?;
        project.set_conversion(Conversion { input: first_in, output: rail, kind: Kind::Buck,
            efficiency_bp: Some(9000), input_pf_bp: None, ground_ua: None, dropout_uv: None, isolation: Isolation::default() })?;
        let block = project.add_block(Role::Custom, "1.8 V load".into(), false)?;
        let load = example_port(&mut project, block, PortKind::Load, "Supply input", 1_800_000, Some(200_000))?;
        project.connect(rail, load)?;
    }
    project.validate()?;
    Ok(project)
}

fn example_port(project: &mut Project, block: Id, kind: PortKind, name: &str, voltage: u64, current: Option<u64>) -> Result<Id> {
    let mut port = crate::model::Port::new(block, kind);
    port.name = name.into();
    port.min_uv = Some(voltage);
    port.max_uv = Some(voltage);
    port.current_ua = current;
    project.set_port(port)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn series_protection_preserves_upstream_load_demand()->Result<()> {
        let mut p=example(false)?;let input=p.conversions[0].input;
        let old=p.bindings.iter().find(|b|b.target==input).unwrap().clone();
        let expected=Network::new(&p,None).demand(old.source);
        p.unbind(old.id);
        let block=p.add_block(Role::Custom,"Fuse".into(),false)?;
        let mut entry=crate::model::Port::new(block,PortKind::Load);entry.min_uv=Some(12_000_000);entry.max_uv=Some(12_000_000);
        let entry=p.set_port(entry)?;
        let mut exit=crate::model::Port::new(block,PortKind::Rail);exit.min_uv=Some(12_000_000);exit.max_uv=Some(12_000_000);
        let exit=p.set_port(exit)?;
        let mut protection=crate::interfaces::Protection::new(block,entry,"Fuse");protection.shunt=false;protection.exit=Some(exit);
        protection.id=p.allocate()?;p.interfaces.protections.push(protection);
        p.connect(old.source,entry)?;p.connect(exit,input)?;p.validate()?;
        assert_eq!(Network::new(&p,None).demand(old.source),expected);
        Ok(())
    }

    #[test]
    fn series_only_power_loop_is_rejected()->Result<()> {
        let mut p=Project::new();let mut endpoints=Vec::new();
        for name in ["Protection A","Protection B"] {
            let block=p.add_block(Role::Custom,name.into(),false)?;
            let input=p.set_port(crate::model::Port::new(block,PortKind::Load))?;
            let output=p.set_port(crate::model::Port::new(block,PortKind::Rail))?;
            let mut protection=crate::interfaces::Protection::new(block,input,name);
            protection.id=p.allocate()?;protection.shunt=false;protection.exit=Some(output);
            p.interfaces.protections.push(protection);endpoints.push((input,output));
        }
        p.connect(endpoints[0].1,endpoints[1].0)?;
        assert!(p.preview_connection(endpoints[1].1,endpoints[0].0).unwrap_err().contains("power loop"));
        p.validate()
    }

    fn rail(project: &Project, name: &str) -> Id {
        project.ports.iter().find(|port| port.kind == PortKind::Rail && port.name == name).unwrap().id
    }

    #[test]
    fn cascade_recalculates_both_upstream_rails_by_mode() -> Result<()> {
        let mut project = example(false)?;
        let five = rail(&project, "5 V");
        let source = rail(&project, "12 V");
        let three = rail(&project, "3.3 V");
        let load = project.bindings.iter().find(|b| b.source == three).unwrap().target;
        let mode = project.add_mode("Standby".into())?;
        project.set_mode_current(mode, load, Some(Some(0)))?;
        let normal = Network::new(&project, None);
        assert_eq!(normal.demand(five), Demand { known_ua: 320_000, incomplete: false });
        assert_eq!(normal.demand(source).known_ua, 148_149);
        let standby = Network::new(&project, project.mode(mode));
        assert_eq!(standby.demand(five).known_ua, 100_000);
        assert_eq!(standby.demand(source).known_ua, 46_297);
        project.set_mode_current(mode, load, Some(None))?;
        let unknown = Network::new(&project, project.mode(mode));
        assert_eq!(unknown.demand(source), Demand { known_ua: 46_297, incomplete: true });
        Ok(())
    }

    #[test]
    fn ac_dc_rollup_uses_efficiency_and_power_factor() -> Result<()> {
        use crate::electrical::{AcReference, PowerKind, PowerSpec};
        let mut project = example(false)?;
        let source = rail(&project, "12 V");
        let input = project.conversions[0].input;
        let ac = PowerSpec { kind: PowerKind::Ac, frequency_min_millihz: Some(50_000),
            frequency_max_millihz: Some(50_000), ..PowerSpec::default() };
        for id in [source, input] {
            let mut port = project.port(id).unwrap().clone();
            port.power = ac;
            port.min_uv = Some(230_000_000);
            port.max_uv = Some(230_000_000);
            project.set_port(port)?;
        }
        project.conversions[0].kind = Kind::AcDc;
        project.conversions[0].input_pf_bp = Some(9000);
        project.validate()?;
        assert_eq!(Network::new(&project, None).demand(source), Demand { known_ua: 8589, incomplete: false });
        for id in [source, input] {
            let mut port = project.port(id).unwrap().clone();
            port.power.phases = 3;
            port.power.reference = AcReference::LineToLine;
            project.set_port(port)?;
        }
        project.validate()?;
        assert!(Network::new(&project, None).demand(source).incomplete);
        Ok(())
    }

    #[test]
    fn pmic_sums_shared_input_and_internal_cascade() -> Result<()> {
        let mut project = example(true)?;
        let five = rail(&project, "5 V");
        let source = rail(&project, "12 V");
        let three = rail(&project, "3.3 V");
        let network = Network::new(&project, None);
        assert_eq!(network.demand(five).known_ua, 400_100);
        assert_eq!(network.transfer(three).known_ua, 300_100);
        assert_eq!(network.demand(source), Demand { known_ua: 218_566, incomplete: false });
        let input = project.conversion(five).unwrap().input;
        assert!(project.remove_port(five).is_err());
        let incoming = project.bindings.iter().find(|b| b.target == input).unwrap().id;
        project.unbind(incoming);
        assert!(project.preview_connection(three, input).unwrap_err().contains("power loop"));
        Ok(())
    }
}

#[cfg(test)]
mod release_tests {
    use super::*;
    use crate::review::{rail_budget, Status};

    #[test]
    fn dedicated_converter_counts_only_calculated_load() -> Result<()> {
        let mut project = Project::new();
        let source = project.add_block(Role::Power, "5 V supply".into(), false)?;
        let regulator = project.add_power_converter("MCU buck".into(), Kind::Buck)?;
        let load = project.add_block(Role::Processing, "MCU".into(), false)?;
        let mut rail = crate::model::Port::new(source, PortKind::Rail);
        rail.min_uv = Some(5_000_000);
        rail.max_uv = Some(5_000_000);
        rail.current_ua = Some(500_000);
        let source = project.set_port(rail)?;
        let channel = project.conversions[0].clone();
        let mut output = project.port(channel.output).unwrap().clone();
        output.min_uv = Some(3_300_000);
        output.max_uv = Some(3_300_000);
        project.set_port(output)?;
        let mut sink = crate::model::Port::new(load, PortKind::Load);
        sink.current_ua = Some(760_000);
        let sink = project.set_port(sink)?;
        let mut channel = channel;
        channel.efficiency_bp = Some(9000);
        project.set_conversion(channel.clone())?;
        project.connect(source, channel.input)?;
        project.connect(channel.output, sink)?;
        let mode = project.add_mode("Low power".into())?;
        assert!(project.set_mode_current(mode, channel.input, Some(Some(500_000))).is_err());
        // The feeder rating is never added as a separate load, even for imported values.
        project.ports.iter_mut().find(|port| port.id == channel.input).unwrap().current_ua = Some(500_000);
        assert_eq!(Network::new(&project, None).demand(source).known_ua, 557_334);
        assert!(project.block(regulator).unwrap().power_converter);
        project.validate()
    }

    #[test]
    fn separates_declared_input_draw_from_converter_demand_and_uses_the_test_voltage() -> Result<()> {
        let mut project = Project::new();
        let supply = project.add_block(Role::Power, "5 V source".into(), false)?;
        let converter = project.add_block(Role::Power, "3.3 V regulator".into(), false)?;
        let mcu = project.add_block(Role::Processing, "MCU".into(), false)?;
        let mut port = |block, kind, min, max, current| {
            let mut port = crate::model::Port::new(block, kind);
            port.min_uv = Some(min);
            port.max_uv = Some(max);
            port.current_ua = current;
            project.set_port(port)
        };
        let source = port(supply, PortKind::Rail, 4_750_000, 5_250_000, Some(500_000))?;
        let input = port(converter, PortKind::Load, 4_750_000, 5_250_000, Some(500_000))?;
        let output = port(converter, PortKind::Rail, 3_135_000, 3_465_000, Some(1_000_000))?;
        let load = port(mcu, PortKind::Load, 3_135_000, 3_465_000, Some(760_000))?;
        project.connect(source, input)?;
        project.connect(output, load)?;
        project.validate()?;

        let limits = Network::new(&project, None);
        assert!(limits.transfer_input(input));
        assert_eq!(limits.transfer(output).known_ua, 453_829);
        assert_eq!(limits.demand(source).known_ua, 953_829);
        let report = rail_budget(&project, project.port(source).unwrap(), None, &limits);
        assert!(report.evidence.iter().any(|line| line.contains("Additional input draw") && line.contains("500 mA")));
        assert_eq!(Network::at_test_voltage(&project, None, input, 5_000_000).transfer(output).known_ua, 476_520);

        let mut feeder = project.port(input).unwrap().clone();
        feeder.current_ua = None;
        project.set_port(feeder)?;
        let nominal = Network::nominal(&project, None);
        assert_eq!(nominal.demand(source), Demand { known_ua: 501_600, incomplete: true });
        let report = crate::review::nominal_power_budget(&project, project.port(output).unwrap(), None, &nominal).unwrap();
        assert!(report.detail.contains("2.508 W"));
        let report = crate::review::nominal_power_budget(&project, project.port(source).unwrap(), None, &nominal).unwrap();
        assert!(report.detail.contains("2.508 W") && report.detail.contains("2.5 W"));

        project.set_conversion(Conversion { input, output, kind: Kind::Buck, efficiency_bp: Some(9_000),
            input_pf_bp: None, ground_ua: None, dropout_uv: None, isolation: Isolation::default() })?;
        assert_eq!(Network::new(&project, None).demand(source).known_ua, 616_000);
        assert_eq!(Network::nominal(&project, None).demand(source).known_ua, 557_334);
        Ok(())
    }

    #[test]
    fn cascaded_33v_load_fails_a_500ma_5v_source_with_or_without_efficiency() -> Result<()> {
        let mut project = Project::new();
        let source_block = project.add_block(Role::Power, "5 V supply".into(), false)?;
        let regulator = project.add_block(Role::Power, "Regulator".into(), false)?;
        let device = project.add_block(Role::Processing, "Load".into(), false)?;
        let mut port = |block, kind, uv, current| {
            let mut port = crate::model::Port::new(block, kind);
            port.min_uv = Some(uv);
            port.max_uv = Some(uv);
            port.current_ua = current;
            project.set_port(port)
        };
        let source = port(source_block, PortKind::Rail, 5_000_000, Some(500_000))?;
        let input = port(regulator, PortKind::Load, 5_000_000, None)?;
        let output = port(regulator, PortKind::Rail, 3_300_000, Some(2_000_000))?;
        let load = port(device, PortKind::Load, 3_300_000, Some(2_000_000))?;
        project.connect(source, input)?;
        project.connect(output, load)?;
        project.validate()?;

        let inferred = Network::new(&project, None);
        assert_eq!(inferred.inferred_feeder(output), Some(input));
        assert_eq!(inferred.demand(source), Demand { known_ua: 1_320_000, incomplete: true });
        assert_eq!(rail_budget(&project, project.port(source).unwrap(), None, &inferred).status, Status::Fail);
        assert_eq!(rail_budget(&project, project.port(output).unwrap(), None, &inferred).status, Status::Pass);

        let mut independent = project.port(output).unwrap().clone();
        independent.independent_rail = true;
        project.set_port(independent)?;
        assert_eq!(Network::new(&project, None).inferred_feeder(output), None);

        project.set_conversion(Conversion { input, output, kind: Kind::Buck, efficiency_bp: None,
            input_pf_bp: None, ground_ua: None, dropout_uv: None, isolation: Isolation::default() })?;
        assert_eq!(Network::new(&project, None).demand(source).known_ua, 1_320_000);
        project.conversions[0].efficiency_bp = Some(9000);
        project.validate()?;
        let declared = Network::new(&project, None);
        assert_eq!(declared.demand(source), Demand { known_ua: 1_466_667, incomplete: false });
        Ok(())
    }
}
