use crate::{
    model::{Id, Mode, Port, PortKind, Project, Result},
    power::{Demand, Edge, Equation, Network, Transfer},
    rail,
    review::{self, LimitSource, Status},
    storage,
    units::Scale,
};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub(crate) struct Report {
    pub title: String,
    pub scope: String,
    pub mode: String,
    pub revision: String,
    pub sections: Vec<Section>,
}

pub(crate) struct Section {
    pub title: &'static str,
    pub columns: &'static [&'static str],
    pub rows: Vec<Row>,
}

pub(crate) struct Row {
    pub cells: Vec<String>,
    pub status: Option<Status>,
}

impl Section {
    fn new(title: &'static str, columns: &'static [&'static str]) -> Self {
        Self { title, columns, rows: Vec::new() }
    }

    fn add(&mut self, cells: Vec<String>) {
        self.rows.push(Row { cells, status: None });
    }

    fn add_finding(&mut self, cells: Vec<String>, status: Status) {
        self.rows.push(Row { cells, status: Some(status) });
    }
}

pub(crate) const BASIS: &str = "Rail verdicts use current at the voltage limits. Nominal DC watts use a separate calculation. Rail budgets describe individual points in the power path. Series protection passes downstream current through. Component-level clamp, leakage and series-drop settings are outside this steady-state model.";
const UNDECLARED: &str = "Undeclared";

impl Report {
    pub fn build(project: &Project, mode: Option<&Mode>, scope: Option<Id>) -> Result<Self> {
        let limits = Network::new(project, mode);
        let nominal = Network::nominal(project, mode);
        let in_scope = |port: &Port| scope.is_none_or(|scope| project.block_in_scope(port.block, scope));
        let rails: Vec<_> = project.ports.iter().filter(|port| port.kind == PortKind::Rail && in_scope(port)).collect();
        let mut paths = HashSet::new();
        for rail in &rails {
            paths.extend(limits.downstream(rail.id));
        }
        for binding in project.bindings.iter().filter(|binding| {
            scope.is_some() && project.port(binding.target).is_some_and(|port| port.kind.power() && in_scope(port))
                && project.port(binding.source).is_some_and(|port| port.kind == PortKind::Rail && !in_scope(port))
        }) {
            // Retain incoming boundary loads even for a subsystem without a supply rail.
            paths.extend(limits.downstream(binding.target));
        }

        let feeder = |input| project.bindings.iter().find(|wire| wire.target == input)
            .map(|wire| wire.source);
        let mut context = paths.clone();
        let mut pending: Vec<_> = paths.iter().copied().collect();
        while let Some(id) = pending.pop() {
            let converter_input = project.conversion(id).map(|channel| channel.input)
                .or_else(|| limits.inferred_feeder(id));
            let inherited_input = project.interfaces.protections.iter()
                .find(|stage| !stage.shunt && stage.exit == Some(id)).map(|stage| stage.entry);
            for related in [converter_input, inherited_input, feeder(id)].into_iter().flatten() {
                if context.insert(related) { pending.push(related); }
            }
        }

        let mut budgets = Section::new("Rail budgets", &["Rail", "Current demand / limit", "Nominal DC demand / limit", "Current headroom", "Verdict"]);
        let mut checks = Section::new("Rail checks", &["Rail", "Detail"]);
        let mut derivations = Section::new("Calculations", &["Reference", "Basis", "Equation and result"]);
        for rail in &rails {
            let finding = review::rail_budget(project, rail, mode, &limits);
            let candidates = review::rail_limits(project, rail, mode);
            let capacity = review::effective_limit(&candidates);
            let known = limits.demand(rail.id);
            let watts = review::dc_nominal_voltage(rail).map(|voltage| {
                let demand = nominal.demand(rail.id);
                format!("{} / {}", demand_value(demand, |ua| review::nominal_watts(voltage, ua)),
                    capacity.map_or_else(|| UNDECLARED.into(), |ua| review::nominal_watts(voltage, ua.into())))
            }).unwrap_or_else(|| "Unavailable".into());
            let headroom = capacity.map_or_else(|| "Unavailable".into(), |ua| {
                let limit = u128::from(ua);
                if known.known_ua == 0 && known.incomplete { return "Unknown".into(); }
                let amount = amps(limit.abs_diff(known.known_ua));
                let sign = if known.known_ua > limit { "−" } else { "+" };
                let bound = if known.incomplete { "At most " } else { "" };
                format!("{bound}{sign}{amount}")
            });
            let name = port_name(project, rail.id);
            budgets.add_finding(vec![name.clone(), format!("{} / {}", current(known), optional(capacity, amps)), watts,
                headroom, finding.status.label().into()], finding.status);
            checks.add_finding(vec![name, finding.detail], finding.status);

            let inputs = candidates.iter().filter_map(|(source, value)| {
                value.map(|ua| format!("{} = {}", limit_ref(*source), amps(ua.into())))
            }).collect::<Vec<_>>();
            if let Some(capacity) = capacity {
                let upper = if known.incomplete { " (upper bound)" } else { "" };
                let sign = if known.known_ua > capacity.into() { "−" } else { "" };
                derivations.add(vec![port_ref(rail.id), "Current budget".into(), format!(
                    "I_limit = min({}) = {}. Headroom = {} − {} = {sign}{}{upper}.",
                    inputs.join(", "), amps(capacity.into()), amps(capacity.into()),
                    amps(known.known_ua), amps(u128::from(capacity).abs_diff(known.known_ua)))]);
            }
            if let Some(voltage) = review::dc_nominal_voltage(rail) {
                let demand = nominal.demand(rail.id);
                let result = demand_value(demand, |ua| review::nominal_watts(voltage, ua));
                let limit_eq = capacity.map_or_else(String::new, |ua| format!(" P_limit = {} × {} = {}.",
                    rail::label_voltage(voltage), amps(ua.into()), review::nominal_watts(voltage, ua.into())));
                derivations.add(vec![port_ref(rail.id), "Nominal watts".into(), format!(
                    "P_demand = V_nom × I_nom = {} × {} = {result}.{limit_eq}",
                    rail::label_voltage(voltage), current(demand))]);
            }
        }

        let mut topology = Section::new("Power path", &["From", "To", "Contribution at limits"]);
        for id in limits.ordered_ports().into_iter().filter(|id| context.contains(id)) {
            let Some(port) = project.port(id) else { continue };
            let name = port_name(project, id);
            for &edge in limits.edges(id) {
                let relevant = match edge {
                    Edge::Wire(target) => paths.contains(&target),
                    _ => context.contains(&edge.target()),
                };
                if !relevant { continue; }
                let target = edge.target();
                let Some(destination) = project.port(target) else { continue };
                let kind = match edge {
                    Edge::Wire(_) => "Wire",
                    Edge::Pass(_) => "Series pass-through",
                    Edge::Conversion(_) => "Converted input current",
                    Edge::Unspecified(_) => "Inferred conversion",
                };
                let draw = branch_draw(&limits, port, destination, edge);
                topology.add(vec![name.clone(), port_name(project, target), format!("{kind} · {}", current(draw))]);
            }
        }

        let mut inputs = Section::new("Inputs and provenance", &["Reference and field", "Value", "Source"]);
        for port in project.ports.iter().filter(|port| context.contains(&port.id)) {
            let id = port.id;
            let name = port_name(project, id);
            let outside = if in_scope(port) { "" } else { " (outside scope)" };
            if port.kind == PortKind::Rail || port.min_uv.is_some() || port.max_uv.is_some() || port.nominal_uv.is_some() {
                add_voltage(project, port, &name, &mut inputs);
            }
            if port.kind == PortKind::Rail {
                inputs.add(vec![format!("{name} · available current"), optional(review::effective_current(port, mode), amps),
                    format!("{}{outside}", current_source(port, mode, "Rail port rating"))]);
            } else if paths.contains(&id) {
                let (draw, source) = limits.direct_draw(project, port, mode);
                inputs.add(vec![format!("{name} · local current"), current(draw), format!("{source}{outside}")]);
            }
            if !port.note.trim().is_empty() {
                inputs.add(vec![name.clone(), port.note.trim().into(), "Project port note".into()]);
            }
            if rail::needs_contract(port) {
                inputs.add(vec![name.clone(), "Contract unconfirmed".into(), "USB-C PD source setting".into()]);
            }
            if paths.contains(&id) && !limits.edges(id).is_empty() {
                let (local, _) = limits.direct_draw(project, port, mode);
                let terms = limits.edges(id).iter().filter_map(|&edge| {
                    let target = project.port(edge.target())?;
                    Some(format!("{} ({})", port_ref(target.id), current(branch_draw(&limits, port, target, edge))))
                }).collect::<Vec<_>>();
                derivations.add(vec![port_ref(id), "Demand sum at limits".into(), format!(
                    "I_total = I_local ({}) + {} = {}.", current(local), terms.join(" + "), current(limits.demand(id)))]);
            }
        }

        for stage in project.interfaces.protections.iter().filter(|stage| !stage.shunt
            && stage.exit.is_some_and(|id| context.contains(&id))) {
            inputs.add(vec![format!("T{} · {} · current threshold", stage.id, stage.name),
                optional(stage.current_limit_ua, amps), "Series protection setting".into()]);
            if !stage.evidence.trim().is_empty() {
                inputs.add(vec![format!("T{} · {}", stage.id, stage.name), stage.evidence.trim().into(), "Project protection evidence".into()]);
            }
        }
        for output in project.ports.iter().filter(|port| context.contains(&port.id)) {
            let Some(input) = project.conversion(output.id).map(|channel| channel.input)
                .or_else(|| limits.inferred_feeder(output.id)) else { continue };
            let provider = project.port(input).filter(|port| port.kind == PortKind::Rail).map(|port| port.id)
                .or_else(|| feeder(input));
            let source = provider.map_or_else(|| "no connected supply".into(), |id| port_ref(id));
            if let Some(channel) = project.conversion(output.id) {
                let reference = format!("{} · {}", port_ref(output.id), channel.kind.label());
                let settings = if channel.kind.linear_dc() {
                    format!("Ground current {} · dropout {}", optional(channel.ground_ua, amps), voltage(channel.dropout_uv))
                } else if channel.kind.ac_input() {
                    format!("Efficiency {} · input power factor {}", ratio(channel.efficiency_bp), ratio(channel.input_pf_bp))
                } else {
                    format!("Efficiency {}", ratio(channel.efficiency_bp))
                };
                inputs.add(vec![reference, settings, "Converter channel settings".into()]);
            }
            for (basis, network) in [("At voltage limits", &limits), ("At nominal voltage", &nominal)] {
                if let Some(transfer) = network.calculation(output.id) {
                    let equation = transfer_equation(transfer, network.demand(output.id));
                    derivations.add(vec![format!("{} → {}", port_ref(input), port_ref(output.id)), basis.into(),
                        format!("{equation} Input supply: {source}. Output load: {}.", port_ref(output.id))]);
                }
            }
        }

        let mut boundary = Section::new("Power boundary interfaces", &["Interface"]);
        if let Some(scope) = scope {
            for line in review::boundary_lines(project, scope, mode, false, true) {
                boundary.add(vec![line]);
            }
        }
        Ok(Self {
            title: project.title.clone(),
            scope: scope.map_or_else(|| "Overall system".into(), |id| project.subsystem_path(id)),
            mode: mode.map_or(&project.condition, |mode| &mode.name).clone(),
            revision: format!("{:x}", Sha256::digest(storage::encode(project)?.as_bytes())),
            sections: vec![budgets, checks, topology, boundary, inputs, derivations],
        })
    }
}

fn branch_draw(network: &Network, source: &Port, target: &Port, edge: Edge) -> Demand {
    match edge {
        Edge::Wire(_) => network.wire_draw(source, target),
        Edge::Pass(_) => network.demand(target.id),
        Edge::Conversion(_) | Edge::Unspecified(_) => network.transfer(target.id),
    }
}

fn add_voltage(project: &Project, port: &Port, name: &str, inputs: &mut Section) {
    let provenance = if let Some(input) = project.inherited_rail_input(port.id) {
        let provider = project.bindings.iter().find(|wire| wire.target == input)
            .and_then(|wire| project.port(wire.source))
            .filter(|source| project.port(input).is_some_and(|entry| source.power.kind == entry.power.kind
                && source.power.polarity == entry.power.polarity) && source.range().is_some());
        format!("Inherited through series protection from {}", port_ref(provider.map_or(input, |source| source.id)))
    } else if let Some(preset) = port.voltage_preset.filter(|preset| port.range() == Some(preset.bounds())) {
        format!("Common preset · {} ±{} · V_bounds = preset × (1 ± tolerance)",
            rail::label_voltage(preset.nominal_mv * 1000), ratio(Some(preset.tolerance_bp)))
    } else if let Some(source) = port.source.filter(|source| {
        let (_, min, max) = source.preset.bounds(source.series);
        port.range() == Some((min, max))
    }) {
        format!("Source preset · {}", source.preset.label())
    } else {
        "Voltage bounds recorded on the project port".into()
    };
    let nominal = if port.nominal_uv.is_some() {
        if project.inherited_rail_input(port.id).is_some() { "inherited nominal setting" } else { "project nominal setting" }
    } else if port.nominal_voltage().is_some() {
        "(minimum + maximum) / 2"
    } else { "unavailable" };
    inputs.add(vec![format!("{name} · voltage"),
        format!("{} to {} · nominal {}", voltage(port.min_uv), voltage(port.max_uv), voltage(port.nominal_voltage())),
        format!("{provenance}. Nominal basis: {nominal}. {}", port.power.label())]);
}

fn current_source<'a>(port: &Port, mode: Option<&Mode>, fallback: &'a str) -> &'a str {
    if mode.is_some_and(|mode| mode.current_override(port.id).is_some()) { "Operating-mode current override" } else { fallback }
}

fn transfer_equation(transfer: Transfer, output: Demand) -> String {
    let iout = amps(output.known_ua);
    let result = current(transfer.demand);
    match transfer.equation {
        Equation::Switching { vin, vout, eta, pf } => {
            let pf_term = if pf.is_some() { " × PF" } else { "" };
            let pf_value = pf.map_or_else(String::new, |pf| format!(" × {}", factor(pf)));
            format!("I_in = I_out × V_out / (V_in × efficiency{pf_term}) = {iout} × {} / ({} × {}{pf_value}) = {result} (rounded up to 1 µA).",
                rail::label_voltage(vout), rail::label_voltage(vin), factor(eta))
        }
        Equation::Linear { ground } => format!(
            "I_in = I_out + I_ground = {iout} + {} = {result}.", optional(ground, amps)),
        Equation::Ideal { vin, vout } => format!(
            "Ideal lower bound: I_in ≥ I_out × V_out / V_in = {iout} × {} / {}. Recorded contribution: {result}. {}",
            rail::label_voltage(vout), rail::label_voltage(vin),
            transfer.fallback.unwrap_or("Converter topology is unconfigured.")),
        Equation::Unknown(reason) => format!("Input current unknown. {reason}"),
    }
}

fn port_ref(id: Id) -> String { format!("P{id}") }
fn port_name(project: &Project, id: Id) -> String {
    project.port(id).map_or_else(|| port_ref(id), |port| format!("{} · {}", port_ref(id), project.port_label(port)))
}
fn limit_ref(source: LimitSource) -> String {
    match source { LimitSource::Rail(id) => port_ref(id), LimitSource::Protection(id) => format!("T{id}") }
}
fn amps(ua: u128) -> String { Scale::Base.display(ua, "A") }
fn voltage(uv: Option<u64>) -> String { uv.map_or_else(|| UNDECLARED.into(), rail::label_voltage) }
fn optional(value: Option<u64>, label: impl Fn(u128) -> String) -> String {
    value.map_or_else(|| UNDECLARED.into(), |value| label(value.into()))
}
fn ratio(value: Option<u16>) -> String {
    value.map_or_else(|| UNDECLARED.into(), |bp| format!("{}%", Scale::Base.number(u128::from(bp) * 10_000)))
}
fn factor(bp: u16) -> String { Scale::Base.number(u128::from(bp) * 100) }
fn current(demand: Demand) -> String { demand_value(demand, amps) }
fn demand_value(demand: Demand, label: impl Fn(u128) -> String) -> String {
    match (demand.incomplete, demand.known_ua) {
        (false, known) => label(known),
        (true, 0) => "Unknown".into(),
        (true, known) => format!("≥ {} (incomplete)", label(known)),
    }
}
