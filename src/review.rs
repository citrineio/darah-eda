use crate::model::{Id, Mode, Port, PortKind, Project, Result};
use crate::power::{self, Demand, Kind, Network};
use crate::rail;
use crate::units::Scale;
use sha2::{Digest, Sha256};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status { Pass, Fail, Open, Manual, Warning }

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "Satisfied within scope", Self::Fail => "Failed",
            Self::Open => "Unresolved", Self::Manual => "Manual verification", Self::Warning => "Warning",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject { Port(Id), Rail(Id), Binding(Id), Conversion(Id), Goal(Id), Spec(Id), Block(Id) }

#[derive(Clone)]
pub struct Finding {
    pub subject: Subject,
    pub status: Status,
    pub title: String,
    pub detail: String,
    pub evidence: Vec<String>,
    pub ports: Vec<Id>,
    pub blocks: Vec<Id>,
}

impl Finding {
    fn new(subject: Subject, title: String, status: Status, detail: String, ports: &[&Port]) -> Self {
        let mut evidence = Vec::new();
        let mut blocks = Vec::new();
        for port in ports {
            if !blocks.contains(&port.block) { blocks.push(port.block); }
            if !port.note.trim().is_empty() {
                evidence.push(format!("{} — {}", port.name, port.note));
            }
        }
        Self { subject, status, title, detail, evidence, ports: ports.iter().map(|p| p.id).collect(), blocks }
    }
}

/// The new wire and power checks affected by its addition, for both hover and commit.
pub fn connection_preview(
    project: &Project, mode: Option<Id>, from: Id, to: Id, previous: &[Finding],
) -> Result<Vec<Finding>> {
    let (candidate, id) = project.preview_connection(from, to)?;
    let source = candidate.binding(id).map(|binding| binding.source);
    let findings = evaluate(&candidate, mode.and_then(|id| candidate.mode(id)));
    Ok(findings.into_iter().filter(|finding| {
        finding.subject == Subject::Binding(id)
            || matches!(finding.subject, Subject::Rail(rail) if Some(rail) == source)
            || (matches!(finding.subject, Subject::Rail(_) | Subject::Conversion(_))
                && previous.iter().any(|old| old.subject == finding.subject
                    && (old.status != finding.status || old.detail != finding.detail)))
    }).collect())
}

pub fn evaluate(p: &Project, mode: Option<&Mode>) -> Vec<Finding> {
    let mut results = Vec::new();
    let network = Network::new(p, mode);
    let needs = rail::unmet(p, &network);
    for need in &needs {
        let (title, detail) = if need.input.range().is_none() || !need.input.power.complete() {
            ("Supply characteristics missing", "Declare the accepted voltage bounds and, for AC, the frequency, phases, and voltage reference.")
        } else if !need.compatible.is_empty() {
            ("Supply unconnected", "At least one available rail fits the declared voltage range. Review its current budget before connecting.")
        } else if !need.pending.is_empty() {
            ("Supply awaits USB-C PD", "A voltage match exists, but its power path depends on an unconfirmed contract.")
        } else {
            ("Missing compatible rail", "No eligible rail with declared bounds and a matching power domain fits this input.")
        };
        let mut finding = Finding::new(Subject::Port(need.input.id), format!("{title} · {}", p.port_label(need.input)),
            Status::Open, detail.into(), &[need.input]);
        finding.evidence.push(format!("Required power format · {}", need.input.power.label()));
        for port in &need.compatible {
            finding.evidence.push(format!("Voltage match · {}", p.port_label(port)));
        }
        for port in &need.pending {
            finding.evidence.push(format!("Pending contract · {}", p.port_label(port)));
        }
        for port in &need.unspecified {
            finding.evidence.push(format!("Rail bounds incomplete · {}", p.port_label(port)));
        }
        if need.missing() {
            let (loads, (min, max)) = rail::proposal(&needs, need.input);
            let nominal = need.input.nominal_voltage().map_or_else(|| "nominal undecided".into(), rail::label_voltage);
            finding.evidence.push(format!("Proposed rail · {nominal} · output must stay within {} to {}",
                rail::label_voltage(min), rail::label_voltage(max)));
            if loads.len() > 1 {
                let names = loads.iter().filter_map(|&id| p.port(id)).map(|port| p.port_label(port)).collect::<Vec<_>>();
                finding.evidence.push(format!("Potential shared rail · {}", names.join(", ")));
            }
        }
        results.push(finding);
    }
    for port in &p.ports {
        let bound = p.bindings.iter().any(|b| b.source == port.id || b.target == port.id);
        if port.required && (bound || port.kind != PortKind::Load) {
            let (status, detail) = if bound {
                (Status::Pass, "An explicit connection exists. See its separate compatibility check.")
            } else {
                (Status::Open, "Bind this required port to a provider or consumer.")
            };
            results.push(Finding::new(Subject::Port(port.id), format!("Port coverage · {}", p.port_label(port)),
                status, detail.into(), &[port]));
        }
        if port.kind == PortKind::Rail {
            results.push(rail_budget(p, port, mode, &network));
        }
    }
    for binding in &p.bindings {
        let (Some(source), Some(target)) = (p.port(binding.source), p.port(binding.target)) else { continue };
        let title = format!("{} → {}", p.port_label(source), p.port_label(target));
        let (status, detail) = if source.kind == PortKind::Rail {
            match (source.range(), target.range()) {
                (Some((out_min, out_max)), Some((in_min, in_max))) => {
                    let compatibility = rail::fits(source, target);
                    let fits = compatibility == Some(true);
                    let pending = network.awaiting_contract(source.id);
                    let status = match compatibility {
                        Some(false) => Status::Fail, None => Status::Open,
                        Some(true) if pending => Status::Open, Some(true) => Status::Pass,
                    };
                    let verdict = match compatibility { Some(true) => "fit", Some(false) => "do not fit", None => "need more data to match" };
                    let qualifier = if fits && pending { " USB-C PD contract is still unconfirmed." } else { "" };
                    let scale = Scale::for_value(u128::from(out_max.max(in_max)));
                    let interval = |min: u64, max: u64| format!("{}–{} {}",
                        scale.number(min.into()), scale.number(max.into()), scale.label("V"));
                    (status, format!("Rail voltage, power type, reference, frequency, and domain {verdict} the input requirements · rail {} · accepted {}.{qualifier}",
                        interval(out_min, out_max), interval(in_min, in_max)))
                }
                _ => (Status::Open, "Enter both voltage bounds for the rail and supply input.".into()),
            }
        } else {
            let (status, detail, _) = crate::signal::evaluate(&crate::interface_review::effective_port(p, source), &crate::interface_review::effective_port(p, target));
            (status, detail)
        };
        let mut finding = Finding::new(Subject::Binding(binding.id), title, status, detail, &[source, target]);
        if !source.kind.power() {
            finding.evidence.extend(crate::signal::evaluate(&crate::interface_review::effective_port(p, source), &crate::interface_review::effective_port(p, target)).2);
            for port in [source, target] {
                if let Some(spec) = &port.signal_spec {
                    finding.evidence.push(format!("{} · {} · reference {} · conditions {}",
                        p.port_label(port), spec.kind.label(), spec.reference, spec.conditions));
                }
            }
        }
        results.push(finding);
    }
    for channel in &p.conversions {
        let (Some(input), Some(output)) = (p.port(channel.input), p.port(channel.output)) else { continue };
        let provider = if input.kind == PortKind::Rail { Some(input) } else {
            p.bindings.iter().find(|wire| wire.target == input.id).and_then(|wire| p.port(wire.source))
        };
        let draw = network.transfer(output.id);
        let (status, detail) = match channel.kind {
            _ if network.awaiting_contract(output.id) =>
                (Status::Open, "This converter depends on an unconfirmed USB-C PD contract."),
            kind if kind.ac_input() && (input.power.phases != 1 || provider.is_some_and(|port| port.power.phases != 1)) =>
                (Status::Open, "Three-phase AC/DC conversion needs per-phase real-power analysis."),
            kind if kind.ac_input() && channel.input_pf_bp.is_none() =>
                (Status::Open, "Declare the AC/DC input power factor."),
            kind if !kind.linear_dc() && channel.efficiency_bp.is_none() =>
                (Status::Open, "Declare the converter efficiency."),
            _ if provider.and_then(|port| port.min_uv).is_none_or(|v| v == 0)
                || output.max_uv.is_none_or(|v| v == 0) =>
                (Status::Open, "Connect a supply and declare its minimum voltage and the output maximum."),
            Kind::Buck if provider.and_then(|port| port.min_uv).zip(output.max_uv).is_some_and(|(vin, vout)| vin <= vout) =>
                (Status::Fail, "The minimum input voltage is at or below the maximum buck output voltage."),
            Kind::Boost if provider.and_then(|port| port.max_uv).zip(output.min_uv).is_some_and(|(vin, vout)| vin >= vout) =>
                (Status::Fail, "The maximum input voltage is at or above the minimum boost output voltage."),
            kind if kind.linear_dc() => match (provider.and_then(|port| port.min_uv), output.max_uv, channel.dropout_uv) {
                (Some(vin), Some(vout), Some(dropout)) if !power::has_headroom(vin, vout, dropout) =>
                    (Status::Fail, "The minimum input voltage leaves less than the declared regulator dropout headroom."),
                _ if channel.ground_ua.is_none() =>
                    (Status::Open, "Declare the linear regulator ground current. Enter zero only if that is your assumption."),
                (Some(_), Some(_), Some(_)) if draw.incomplete =>
                    (Status::Open, "The dropout headroom fits, but the load demand is incomplete."),
                (Some(_), Some(_), Some(_)) =>
                    (Status::Pass, "The declared dropout headroom fits at the input minimum and output maximum."),
                _ => (Status::Open, "Declare the supply minimum, output maximum, and regulator dropout."),
            },
            _ if draw.incomplete => (Status::Open, "Some downstream demand or conversion assumptions are still unknown."),
            _ => (Status::Pass, "Input demand calculated from the declared voltage bounds, efficiency, and applicable power factor."),
        };
        let mut finding = Finding::new(Subject::Conversion(output.id),
            format!("Converter · {}", p.port_label(output)), status, detail.into(), &[output, input]);
        finding.evidence.push(format!("{} → {}", p.port_label(input), p.port_label(output)));
        finding.evidence.push(format!("Demand on input · {}", demand_label(draw)));
        results.push(finding);
        let isolation = &channel.isolation;
        if channel.kind.ac_input() || isolation.class != power::IsolationClass::Unspecified {
            let (status, detail) = match isolation.class {
                power::IsolationClass::Unspecified => (Status::Open, "Declare the isolation class for this AC/DC stage."),
                power::IsolationClass::None if channel.kind.ac_input() =>
                    (Status::Manual, "This AC/DC stage is declared non-isolated. Verify the system barrier and all accessible interfaces."),
                power::IsolationClass::None => (Status::Pass, "The converter is declared non-isolated."),
                _ if !isolation.complete() => (Status::Open, "Record working and withstand voltages, test duration, creepage, clearance, and evidence."),
                _ => (Status::Manual, "Verify the declared barrier against the equipment requirements and documented test conditions."),
            };
            let mut finding = Finding::new(Subject::Block(output.block),
                format!("Isolation · {} · {}", p.port_label(output), isolation.class.label()),
                status, detail.into(), &[input, output]);
            for (label, value, scale, symbol) in [
                ("Working voltage", isolation.working_uv, Scale::Base, "V"),
                ("Withstand voltage", isolation.withstand_uv, Scale::Base, "V"),
                ("Test duration", isolation.test_ms, Scale::Milli, "s"),
            ] {
                if let Some(value) = value { finding.evidence.push(format!("{label} · {}", scale.display(value.into(), symbol))); }
            }
            for (label, value) in [("Creepage", isolation.creepage_um), ("Clearance", isolation.clearance_um)] {
                if let Some(value) = value { finding.evidence.push(format!("{label} · {} mm", Scale::Milli.number(value.into()))); }
            }
            if !isolation.evidence.is_empty() { finding.evidence.push(isolation.evidence.clone()); }
            results.push(finding);
        }
    }
    for output in p.ports.iter().filter(|port| port.kind == PortKind::Rail) {
        let Some(input) = network.inferred_feeder(output.id).and_then(|id| p.port(id)) else { continue };
        let mut finding = Finding::new(Subject::Conversion(output.id),
            format!("Power topology · {}", p.port_label(output)), Status::Open,
            "This rail has a possible upstream feed. Configure its converter or mark it as an independent supply.".into(),
            &[input, output]);
        finding.evidence.push(format!("Ideal lower bound on input · {}", demand_label(network.transfer(output.id))));
        results.push(finding);
    }
    for goal in &p.goals {
        let mut blocks = goal.responsible.clone();
        if let Some(id) = goal.owner {
            if !blocks.contains(&id) { blocks.insert(0, id); }
        }
        let names: Vec<_> = blocks.iter().filter_map(|id| p.block(*id)).map(|b| b.name.as_str()).collect();
        let allocation = if names.is_empty() { "No block assigned".into() } else { names.join(", ") };
        let method = if goal.method.trim().is_empty() { "Define a verification method." } else { goal.method.as_str() };
        let evidence = if goal.note.trim().is_empty() { Vec::new() } else { vec![goal.note.clone()] };
        results.push(Finding {
            subject: Subject::Goal(goal.id), status: Status::Manual, title: format!("{} · {}", crate::spec::requirement_reference(goal.reference), goal.description),
            detail: format!("Pending · {allocation}\n{method}"), evidence, ports: Vec::new(), blocks,
        });
    }
    results.extend(crate::spec::findings(p));
    results.extend(crate::interface_review::evaluate(p));
    results
}

pub(crate) fn effective_current(port: &Port, mode: Option<&Mode>) -> Option<u64> {
    mode.and_then(|mode| mode.current_override(port.id)).unwrap_or(port.current_ua)
}

pub(crate) fn demand_label(demand: Demand) -> String {
    let value = Scale::for_value(demand.known_ua).display(demand.known_ua, "A");
    demand_with_unit(demand, value)
}

fn demand_with_unit(demand: Demand, value: String) -> String {
    match (demand.incomplete, demand.known_ua) {
        (false, _) => value,
        (true, 0) => "unknown".into(),
        (true, _) => format!("at least {value} · incomplete"),
    }
}

#[derive(Clone, Copy)]
pub(crate) enum LimitSource { Rail(Id), Protection(Id) }

pub(crate) fn rail_limits(project: &Project, rail: &Port, mode: Option<&Mode>) -> Vec<(LimitSource, Option<u64>)> {
    let mut limits = vec![(LimitSource::Rail(rail.id), effective_current(rail, mode))];
    let mut output = rail.id;
    let mut visited = std::collections::HashSet::new();
    while visited.insert(output) {
        let Some(stage) = project.interfaces.protections.iter().find(|stage| !stage.shunt && stage.exit == Some(output))
            else { break };
        limits.push((LimitSource::Protection(stage.id), stage.current_limit_ua));
        let Some(source) = project.bindings.iter().find(|wire| wire.target == stage.entry)
            .and_then(|wire| project.port(wire.source)) else { break };
        limits.push((LimitSource::Rail(source.id), effective_current(source, mode)));
        output = source.id;
    }
    limits
}

pub(crate) fn effective_limit(limits: &[(LimitSource, Option<u64>)]) -> Option<u64> {
    limits.iter().filter_map(|&(_, value)| value).min()
}

pub(crate) fn dc_nominal_voltage(rail: &Port) -> Option<u64> {
    (rail.power.kind == crate::electrical::PowerKind::Dc).then(|| rail.nominal_voltage())
        .flatten().filter(|&voltage| voltage > 0)
}

pub(crate) fn nominal_watts(uv: u64, ua: u128) -> String {
    let microwatts = ua.saturating_mul(u128::from(uv)).saturating_add(500_000) / 1_000_000;
    Scale::Base.display(microwatts, "W")
}

pub fn rail_budget(p: &Project, rail: &Port, mode: Option<&Mode>, network: &Network) -> Finding {
    rail_budget_with_voltage(p, rail, mode, network, None)
}

pub fn nominal_power_budget(p: &Project, rail: &Port, mode: Option<&Mode>, nominal: &Network) -> Option<Finding> {
    let voltage = dc_nominal_voltage(rail)?;
    Some(rail_budget_with_voltage(p, rail, mode, nominal, Some(voltage)))
}

fn rail_budget_with_voltage(p: &Project, rail: &Port, mode: Option<&Mode>, network: &Network, voltage: Option<u64>) -> Finding {
    let loads: Vec<_> = p.bindings.iter().filter(|b| b.source == rail.id)
        .filter_map(|b| p.port(b.target)).collect();
    let internal: Vec<_> = p.conversions.iter().filter(|channel| channel.input == rail.id)
        .filter_map(|channel| p.port(channel.output)).collect();
    let known = network.demand(rail.id).known_ua;
    let limit = effective_limit(&rail_limits(p, rail, mode)).map(u128::from);
    let scale = Scale::for_value(known.max(limit.unwrap_or(0)));
    let amount = |value: u128| match voltage {
        Some(uv) => nominal_watts(uv, value),
        None => scale.display(value, "A"),
    };
    let contribution = |demand: Demand| match voltage {
        Some(_) => demand_with_unit(demand, amount(demand.known_ua)),
        None => demand_label(demand),
    };
    let missing = loads.iter().filter(|load| network.demand(load.id).incomplete).count()
        + internal.iter().filter(|output| network.transfer(output.id).incomplete).count();
    let condition = mode.map_or(p.condition.as_str(), |mode| mode.name.as_str());
    let (status, detail) = match limit {
        _ if rail.power.kind == crate::electrical::PowerKind::Ac && rail.power.phases == 3 =>
            (Status::Manual, "Three-phase source captured. Review per-phase loading, balance, and power factor.".into()),
        Some(limit) if known > limit => {
            let short = amount(known - limit);
            let shortfall = if missing == 0 { short } else { format!("at least {short}") };
            (Status::Fail, format!("{} known / {} available · {shortfall} short in {condition} · {missing} incomplete branch(es).",
                amount(known), amount(limit)))
        }
        _ if network.awaiting_contract(rail.id) => (Status::Open, format!(
            "Supply awaits USB-C PD contract confirmation · {} modeled demand in {condition}.", amount(known))),
        Some(limit) if missing == 0 => (Status::Pass, format!(
            "{} used / {} available · {} margin in {condition}.", amount(known), amount(limit), amount(limit - known))),
        Some(limit) => (Status::Open, format!(
            "{} known / {} available in {condition} · {missing} incomplete branch(es).", amount(known), amount(limit))),
        None => (Status::Open, format!(
            "Declare the rail current limit in {condition} · {} known · {missing} incomplete branch(es).", amount(known))),
    };
    let mut ports = vec![rail];
    ports.extend(loads.iter().chain(&internal).copied());
    let budget = if voltage.is_some() { "Nominal power budget" } else { "Current budget" };
    let mut result = Finding::new(Subject::Rail(rail.id), format!("{budget} · {}", p.port_label(rail)), status, detail, &ports);
    result.evidence = loads.iter().flat_map(|load| {
        let mut lines = Vec::new();
        let value = contribution(network.demand(load.id));
        let note = if load.note.trim().is_empty() { String::new() } else { format!(" · {}", load.note) };
        let override_note = if mode.is_some_and(|mode| mode.current_override(load.id).is_some()) { " · mode override" } else { "" };
        let calculated = if p.dedicated_converter_input(load.id) { " · calculated converter input" } else { "" };
        lines.push(format!("{}{calculated} · {value}{override_note}{note}", p.port_label(load)));
        if network.transfer_input(load.id) && !p.dedicated_converter_input(load.id) {
            if let Some(current) = effective_current(load, mode).filter(|&current| current > 0) {
                lines.push(format!("Additional input draw entered on {} · {}", load.name, amount(current.into())));
            }
        }
        for channel in p.conversions.iter().filter(|channel| channel.input == load.id) {
            if let Some(output) = p.port(channel.output) {
                lines.push(format!("{} via {} · {}", p.port_label(output), load.name, contribution(network.transfer(output.id))));
            }
        }
        for output in p.ports.iter().filter(|output| network.inferred_feeder(output.id) == Some(load.id)) {
            lines.push(format!("{} via {} · {} (ideal lower bound, configure topology)",
                p.port_label(output), load.name, contribution(network.transfer(output.id))));
        }
        for stage in p.interfaces.protections.iter().filter(|stage| !stage.shunt && stage.entry == load.id) {
            if let Some(output) = stage.exit.and_then(|id| p.port(id)) {
                lines.push(format!("{} through {} · {}", p.port_label(output), stage.name, contribution(network.demand(output.id))));
            }
        }
        lines
    }).collect();
    for output in internal {
        result.evidence.push(format!("{} · internal channel · {}", p.port_label(output), contribution(network.transfer(output.id))));
    }
    if let Some(uv) = voltage {
        result.evidence.insert(0, format!("Nominal DC operating point · {}. Rail verdict uses the current check across the declared voltage bounds.",
            rail::label_voltage(uv)));
    }
    if mode.is_some_and(|mode| mode.current_override(rail.id).is_some()) {
        result.evidence.insert(0, "Rail limit uses a mode override.".into());
    }
    if let Some(source) = rail.source {
        let qualifier = if source.preset.battery() { format!(" · {}S", source.series) }
            else if source.preset.pd() && source.contract_confirmed { " · contract assumed available".into() }
            else if source.preset.pd() { " · contract unconfirmed".into() }
            else { String::new() };
        result.evidence.insert(0, format!("Source preset · {}{qualifier}", source.preset.label()));
    }
    if rail.power.kind == crate::electrical::PowerKind::Ac {
        result.evidence.push(rail.power.label());
        result.evidence.push("Current sums are conservative RMS load envelopes. VA and W require the applicable power factor.".into());
    }
    if let (Some(battery), Some(nominal)) = (rail.battery, rail.nominal_voltage()) {
        if let Some(capacity) = battery.capacity {
            result.evidence.push(format!("Battery pack · {}P · {} {} · {}",
                battery.parallel, Scale::Milli.number(capacity.milli_value.into()), capacity.unit.label(), battery.cell.label()));
            if let Ok(energy) = capacity.energy_uwh(nominal) {
                result.evidence.push(format!("Nominal stored energy · {} Wh", Scale::Base.number(energy.into())));
            }
        }
    }
    if !rail.note.trim().is_empty() { result.evidence.insert(0, format!("Rail assumption · {}", rail.note)); }
    result
}

pub(crate) fn literal(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .replace('\r', " ").replace('\n', " ").replace('|', "&#124;")
        .replace('*', "&#42;").replace('_', "&#95;").replace('`', "&#96;")
        .replace('[', "&#91;").replace(']', "&#93;")
}

pub fn markdown(p: &Project, mode: Option<&Mode>, findings: &[Finding]) -> Result<String> {
    let encoded = crate::storage::encode(p)?;
    let revision = format!("{:x}", Sha256::digest(encoded.as_bytes()));
    let mut out = format!("# {} · Design review\n\n{}\n\n- Active operating mode · {}\n- Project ID · `{}`\n- Revision SHA-256 · `{}`\n",
        literal(&p.title), literal(&p.purpose),
        literal(mode.map_or(p.condition.as_str(), |mode| mode.name.as_str())), p.project_id, revision);
    out.push_str("\nThe detailed review uses the active mode. The rail comparison covers every mode. Functional verification remains manual.\n");
    if !p.subsystems.is_empty() {
        out.push_str("\n## Systems and subsystems\n\n");
        for scope in &p.subsystems {
            let _ = writeln!(out, "- {} · {} block(s) · {} boundary interface(s)", literal(&p.subsystem_path(scope.id)),
                p.blocks.iter().filter(|block| p.block_in_scope(block.id, scope.id)).count(), p.boundary_crossings(scope.id).count());
        }
    }
    out.push_str("\n## Blocks\n\n");
    for block in &p.blocks {
        let _ = writeln!(out, "- **{}** · {} · {}", literal(&block.name), block.role.label(), literal(&block.purpose));
    }
    out.push_str("\n## Interface characteristics\n");
    for port in &p.ports {
        if port.kind.power() {
            let voltage = if port.power.kind == crate::electrical::PowerKind::Ac { "AC RMS" } else { "DC magnitude" };
            let _ = writeln!(out, "\n- **{}** · {voltage} · polarity {:?} · {} phase · domain {}",
                literal(&p.port_label(port)), port.power.polarity, port.power.phases, literal(&port.rail_domain));
            if let Some(source) = port.source {
                let _ = writeln!(out, "  - Source preset · {} · {}S", literal(&source.preset.label()), source.series);
            }
            if let Some((min, max)) = port.power.frequency() {
                let _ = writeln!(out, "  - Frequency · {}–{} Hz", Scale::Milli.number(min.into()), Scale::Milli.number(max.into()));
            }
            if let Some(battery) = port.battery {
                if let Some(capacity) = battery.capacity {
                    let _ = writeln!(out, "  - Pack capacity · {} {} · {}P · {}",
                        Scale::Milli.number(capacity.milli_value.into()), capacity.unit.label(), battery.parallel, battery.cell.label());
                }
            }
        } else if let Some(spec) = &port.signal_spec {
            let _ = writeln!(out, "\n### {} · {}\n\nReference · {}\n\nConditions · {}\n\nUnpowered behavior · {}\n",
                literal(&p.port_label(port)), spec.kind.label(), literal(&spec.reference), literal(&spec.conditions), literal(&spec.power_off_behavior));
            let _ = writeln!(out, "Output stage · {}\n", spec.drive.label());
            for (&key, &value) in &spec.values {
                let (unit, decimals) = key.unit();
                let _ = writeln!(out, "- {} · {} {unit}", key.label(port.kind.source()), crate::signal::number(value, decimals));
            }
            for extra in &spec.additional {
                let _ = writeln!(out, "- {} · {} {} · {}", literal(&extra.name), literal(&extra.value), literal(&extra.unit), literal(&extra.conditions));
            }
        }
    }
    if !p.conversions.is_empty() {
        out.push_str("\n## Power conversion\n\n| Feeder | Output | Channel | Assumptions |\n| --- | --- | --- | --- |\n");
        for channel in &p.conversions {
            let (Some(input), Some(output)) = (p.port(channel.input), p.port(channel.output)) else { continue };
            let (kind, mut assumptions) = match channel.kind {
                kind if !kind.linear_dc() => (kind.label(), channel.efficiency_bp.map_or_else(|| "Efficiency unknown".into(), |value|
                    format!("{}% efficiency", Scale::Base.number(u128::from(value) * 10_000)))),
                kind => (kind.label(), format!("Ground current {} · dropout {}",
                    channel.ground_ua.map_or_else(|| "unknown".into(), |ua| Scale::Micro.display(ua.into(), "A")),
                    channel.dropout_uv.map_or_else(|| "unknown".into(), |uv| Scale::Milli.display(uv.into(), "V")))),
            };
            if channel.kind.ac_input() {
                let pf = channel.input_pf_bp.map_or_else(|| "unknown".into(),
                    |value| format!("{}%", Scale::Base.number(u128::from(value) * 10_000)));
                assumptions.push_str(&format!(" · input power factor {pf}"));
            }
            let _ = writeln!(out, "| {} | {} | {kind} | {} |",
                literal(&p.port_label(input)), literal(&p.port_label(output)), literal(&assumptions));
        }
    }
    out.push_str("\n## Review\n");
    for finding in findings.iter().filter(|f| !matches!(f.subject, Subject::Spec(_))) {
        let _ = writeln!(out, "\n### {} · {}\n\n{}", finding.status.label(), literal(&finding.title), literal(&finding.detail));
        for item in &finding.evidence { let _ = writeln!(out, "\n- {}", literal(item)); }
    }
    if p.ports.iter().any(|port| port.kind == PortKind::Rail) {
        out.push_str("\n## Rail comparison\n\n| Rail | Mode | Status | Budget |\n| --- | --- | --- | --- |\n");
        let modes: Vec<_> = std::iter::once(None).chain(p.modes.iter().map(Some))
            .map(|mode| (mode, Network::new(p, mode))).collect();
        for rail in p.ports.iter().filter(|port| port.kind == PortKind::Rail) {
            for (mode, network) in &modes {
                let finding = rail_budget(p, rail, *mode, network);
                let name = mode.map_or(p.condition.as_str(), |mode| mode.name.as_str());
                let _ = writeln!(out, "| {} | {} | {} | {} |", literal(&p.port_label(rail)),
                    literal(name), finding.status.label(), literal(&finding.detail));
            }
        }
    }
    crate::spec::append_markdown(p, &mut out);
    crate::interface_review::append_markdown(p, &mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_preview_reports_mode_overload_without_committing() -> Result<()> {
        let mut project = Project::modes_demo()?;
        let mode = &project.modes[0];
        let alert = project.ports.iter()
            .find(|port| mode.current_override(port.id) == Some(Some(110_000))).unwrap().id;
        let active = mode.id;
        let binding = project.bindings.iter().find(|binding| binding.target == alert).unwrap().clone();
        project.unbind(binding.id);
        let baseline = project.clone();
        let idle = connection_preview(&project, None, binding.source, alert, &evaluate(&project, None))?;
        assert!(!idle.is_empty() && idle.iter().all(|check| check.status == Status::Pass));
        let active = connection_preview(&project, Some(active), alert, binding.source,
            &evaluate(&project, project.mode(active)))?;
        assert!(active.iter().any(|check| check.subject == Subject::Rail(binding.source)
            && check.status == Status::Fail && check.detail.contains("20 mA short")));
        assert_eq!(project, baseline);
        Ok(())
    }

    #[test]
    fn modes_distinguish_zero_load_overload_and_unknown_draw() -> Result<()> {
        let project = Project::modes_demo()?;
        let rail = project.ports.iter().find(|port| port.kind == PortKind::Rail).expect("Sample rail");
        let budget = |mode| rail_budget(&project, rail, mode, &Network::new(&project, mode));
        let idle = budget(None);
        let active = budget(project.modes.first());
        let unknown = budget(project.modes.get(1));
        assert_eq!(idle.status, Status::Pass);
        assert!(idle.detail.contains("90 mA margin"));
        assert_eq!(active.status, Status::Fail);
        assert!(active.detail.contains("20 mA short"));
        assert_eq!(unknown.status, Status::Open);
        assert!(unknown.detail.contains("1 incomplete branch"));
        Ok(())
    }
}

pub fn boundary_lines(p: &Project, scope: Id, mode: Option<&Mode>, watts: bool, power_only: bool) -> Vec<String> {
    let network = if watts { Network::nominal(p, mode) } else { Network::new(p, mode) };
    let limit_network = watts.then(|| Network::new(p, mode));
    p.boundary_crossings(scope).filter_map(|(wire, inbound)| {
        let from = p.port(wire.source)?;
        let to = p.port(wire.target)?;
        let direction = if inbound { "Import" } else { "Export" };
        if power_only && !from.kind.power() { return None; }
        let value = if from.kind.power() {
            let demand = network.demand(to.id);
            let watts_at = if watts && from.power.kind == crate::electrical::PowerKind::Dc { from.nominal_voltage() } else { None };
            let value = match watts_at {
                Some(uv) => {
                    let uw = demand.known_ua.saturating_mul(u128::from(uv)).saturating_add(500_000) / 1_000_000;
                    demand_with_unit(demand, Scale::Base.display(uw, "W"))
                }
                None => demand_label(limit_network.as_ref().map_or(demand, |network| network.demand(to.id))),
            };
            format!(" · {value}")
        } else { String::new() };
        Some(format!("{direction} · {} → {}{value}", p.port_label(from), p.port_label(to)))
    }).collect()
}

pub fn markdown_for_scope(p: &Project, mode: Option<&Mode>, findings: &[Finding], scope: Option<Id>) -> Result<String> {
    let Some(scope) = scope else { return markdown(p, mode, findings); };
    let path = p.subsystem_path(scope);
    let mut out = format!("# {} · Design review\n\nSystem · {}\n\nActive operating mode · {}\n",
        literal(&path), literal(&p.title), literal(mode.map_or(p.condition.as_str(), |mode| mode.name.as_str())));
    out.push_str("\n## Assigned blocks\n\n");
    for block in p.blocks.iter().filter(|block| p.block_in_scope(block.id, scope)) {
        let _ = writeln!(out, "- {} · {} · {}", literal(&block.name), block.role.label(), literal(&block.purpose));
    }
    out.push_str("\n## Boundary interfaces\n\n");
    let boundaries = boundary_lines(p, scope, mode, false, false);
    if boundaries.is_empty() { out.push_str("No connected interfaces cross this boundary.\n"); }
    for line in boundaries { let _ = writeln!(out, "- {}", literal(&line)); }
    out.push_str("\n## Power budgets by operating mode\n\n");
    for mode in std::iter::once(None).chain(p.modes.iter().map(Some)) {
        let network = Network::new(p, mode);
        let _ = writeln!(out, "### {}\n", literal(mode.map_or(p.condition.as_str(), |mode| mode.name.as_str())));
        for rail in p.ports.iter().filter(|port| port.kind == PortKind::Rail && p.block_in_scope(port.block, scope)) {
            let finding = rail_budget(p, rail, mode, &network);
            let _ = writeln!(out, "- {} · {} · {}", finding.status.label(), literal(&finding.title), literal(&finding.detail));
        }
    }
    out.push_str("\n## Design checks and evidence\n\n");
    for finding in findings.iter().filter(|finding| p.finding_in_scope(finding, Some(scope))) {
        let _ = writeln!(out, "### {} · {}\n\n{}\n", literal(&finding.title), finding.status.label(), literal(&finding.detail));
        for item in &finding.evidence { let _ = writeln!(out, "- {}", literal(item)); }
    }
    Ok(out)
}
