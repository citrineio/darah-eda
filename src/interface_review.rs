use crate::{interfaces::{self, Fault, PinRole}, model::{Port, PortKind, Project}, review::{Finding, Status, Subject}, signal::{Kind, Parameter}};
use std::fmt::Write;

pub fn effective_port(project: &Project, port: &Port) -> Port {
    let mut effective = port.clone();
    if let Some(contract) = port.contract.and_then(|id| project.contract(id)) { effective.signal = contract.name.clone(); }
    effective
}

fn finding(subject: Subject, title: String, status: Status, detail: String, ports: Vec<u32>, blocks: Vec<u32>) -> Finding {
    Finding { subject, title, status, detail, ports, blocks, evidence: Vec::new() }
}

pub fn evaluate(project: &Project) -> Vec<Finding> {
    let mut results = Vec::new();
    for block in &project.blocks {
        let mut pins=std::collections::BTreeMap::<&str,Vec<u32>>::new();
        for port in project.block_ports(block.id).filter(|p|!p.pin.trim().is_empty()) { pins.entry(port.pin.trim()).or_default().push(port.id); }
        for (pin,ports) in pins.into_iter().filter(|(_,ports)|ports.len()>1) {
            results.push(finding(Subject::Block(block.id),format!("Physical pin {pin} has multiple logical assignments"),Status::Warning,
                "Confirm alternate-function selection and shared resources; these functions may not be available simultaneously.".into(),ports,vec![block.id]));
        }
    }
    for port in &project.ports {
        let Some(contract) = port.contract.and_then(|id| project.contract(id)) else { continue };
        let mut status = Status::Pass;
        let mut evidence = vec![format!("Contract · {} · {}", contract.name, contract.description)];
        if let Some(actual) = &port.signal_spec {
            if actual.kind != contract.limits.kind {
                status = Status::Fail; evidence.push("Hardware signal type differs from the contract.".into());
            }
            if !contract.limits.reference.trim().is_empty() && actual.reference.trim() != contract.limits.reference.trim() {
                if status != Status::Fail { status = Status::Open; }
                evidence.push("Signal reference/domain needs confirmation against the contract.".into());
            }
            for (&key, &limit) in &contract.limits.values {
                let Some(&value) = actual.values.get(&key) else {
                    if status != Status::Fail { status = Status::Open; }
                    evidence.push(format!("{} · hardware value missing", key.label(port.kind.source())));
                    continue;
                };
                use Parameter::*;
                let greater = match key {
                    VoltageMin | CurrentMin | HighMin | FrequencyMin | DutyMin => Some(port.kind.source()),
                    VoltageMax | CurrentMax | LowMax | FrequencyMax | DutyMax => Some(!port.kind.source()),
                    Bandwidth | DataRate => Some(true),
                    _ => None,
                };
                if let Some(greater) = greater {
                    let fits = if greater { value >= limit } else { value <= limit };
                    if !fits { status = Status::Fail; }
                    evidence.push(format!("{} · {} · hardware {} / contract {} {}", key.label(port.kind.source()),
                        if fits { "fits" } else { "does not fit" }, crate::signal::number(value, key.unit().1),
                        crate::signal::number(limit, key.unit().1), key.unit().0));
                } else if status == Status::Pass { status = Status::Manual; }
            }
            if contract.limits.values.is_empty() && status == Status::Pass { status = Status::Manual; }
            if !contract.limits.additional.is_empty() && status == Status::Pass { status = Status::Manual; }
        } else {
            status = Status::Open; evidence.push("Declare this port's actual electrical characteristics. Selecting a contract does not copy device capabilities.".into());
        }
        let mut result = finding(Subject::Port(port.id), format!("Interface contract · {}", project.port_label(port)), status,
            "Hardware capabilities compared with the shared interface requirements.".into(), vec![port.id], vec![port.block]);
        result.evidence = evidence; results.push(result);
    }
    for c in &project.interfaces.channels {
        let (Some(input), Some(output)) = (project.port(c.input), project.port(c.output)) else { continue };
        let mut status = Status::Manual;
        let mut evidence = vec![format!("{} · {}", c.kind.label(), if c.active { "active" } else { "passive" }),
            format!("{} → {}", project.port_label(input), project.port_label(output)),
            format!("Assumptions · {}", c.assumptions)];
        if let Some((expected_in, expected_out)) = c.kind.types() {
            let fits_kind = |actual: Kind, expected: Kind| actual == expected || expected == Kind::Digital && actual.logic();
            match (&input.signal_spec, &output.signal_spec) {
                (Some(a), Some(b)) if !fits_kind(a.kind, expected_in) || !fits_kind(b.kind, expected_out) => {
                    status = Status::Fail; evidence.push("Declared input/output signal types do not fit this implementation.".into());
                }
                (None, _) | (_, None) => { status = Status::Open; evidence.push("Declare characteristics on both sides.".into()); }
                _ => {}
            }
        }
        if c.active && (c.supplies.is_empty() || c.supplies.iter().any(|id| !project.bindings.iter().any(|b| b.target == *id))) {
            if status != Status::Fail { status = Status::Open; }
            evidence.push("An active converter needs declared, connected supply inputs. Their individual power checks still apply.".into());
        }
        let provider = project.bindings.iter().find(|b| b.target == input.id).and_then(|b| project.port(b.source));
        if provider.is_none() {
            if status != Status::Fail { status = Status::Open; }
            evidence.push("The signal input is unconnected.".into());
        }
        if let Some(range) = provider.and_then(voltage_range) {
            if let Some((min, max)) = c.envelope(range) {
                evidence.push(format!("Derived output envelope · {} to {} V", crate::signal::number(min, 6), crate::signal::number(max, 6)));
                match voltage_range(output) {
                    Some((lo, hi)) if min < lo || max > hi => {
                        status = Status::Fail; evidence.push("The calculated envelope exceeds the declared output envelope.".into());
                    }
                    None => {
                        if status != Status::Fail { status = Status::Open; }
                        evidence.push("Declare the output voltage envelope or calculate it in Block tools.".into());
                    }
                    _ => {}
                }
                evidence.push("Static transfer only. Verify source/load impedance, settling, noise, headroom, reference error, and fault behavior.".into());
            }
        }
        if c.kind == interfaces::ConversionKind::Divider {
            if c.top_ohm.is_none() || c.bottom_ohm.is_none() || c.resistor_tolerance_ppm.is_none()
                || c.offset_uv.is_none() || c.offset_error_uv.is_none() {
                if status != Status::Fail { status = Status::Open; }
                evidence.push("Divider calculation needs both resistances, tolerance, offset and offset error (zero must be explicit).".into());
            }
            if c.top_ohm == Some(0) || c.bottom_ohm == Some(0) {
                status = Status::Fail; evidence.push("Divider resistances must be positive.".into());
            }
        }
        let mut result = finding(Subject::Block(c.block), format!("Signal conversion · {}", c.name), status,
            "Input and output contracts remain separate across this conversion. Detailed dynamic behavior requires review.".into(), vec![input.id, output.id], vec![c.block]);
        result.evidence = evidence; results.push(result);
    }
    for connector in &project.interfaces.connectors {
        let name = project.block(connector.block).map_or("?", |block| block.name.as_str());
        let has_power = connector.pins.iter().any(|pin| pin.role == PinRole::Power)
            || project.block_ports(connector.block).any(|port| port.kind.power());
        let mut details: Vec<String> = Vec::new();
        if connector.part_number.trim().is_empty() || connector.source.trim().is_empty() {
            details.push("Record the chosen part number and pinout drawing.".into());
        }
        if has_power && (connector.max_uv.is_none() || connector.contact_current_ua.is_none()) {
            details.push("Record working voltage and per-contact current ratings.".into());
        }
        if connector.mating.trim().is_empty() {
            details.push("Record the mating part or cable, keying and orientation.".into());
        }
        if connector.exposure != interfaces::Exposure::Internal && connector.shielding.trim().is_empty() {
            details.push("Document the return and shield strategy.".into());
        }
        if !details.is_empty() {
            let mut check = finding(Subject::Block(connector.block), format!("{name} · Part details incomplete"),
                Status::Open, "Add part-specific details when selecting the component.".into(), Vec::new(), vec![connector.block]);
            check.evidence = details;
            results.push(check);
        }

        for port in project.block_ports(connector.block).filter(|port| port.pin.trim().is_empty()) {
            let required = crate::interface_catalog::protection_faults(connector, port.kind.power());
            let component = interfaces::wire_component(project, port.id);
            for fault in required {
                let covered = project.interfaces.protections.iter()
                    .any(|p| crate::interface_catalog::protection_on_connector_side(p, port, &component) && p.covers.contains(&fault));
                let (status, detail) = if covered {
                    (Status::Manual, "Protection is declared for this family. Verify coverage and ratings at each physical contact.")
                } else {
                    (Status::Warning, "Declare protection for this family and verify it at each physical contact.")
                };
                results.push(finding(Subject::Port(port.id), format!("{name} · {} · {}", port.name, fault.label()),
                    status, detail.into(), vec![port.id], vec![connector.block]));
            }
            if port.kind.power() && connector.max_uv.zip(port.max_uv).is_some_and(|(rating, max)| max > rating) {
                results.push(finding(Subject::Port(port.id), format!("{name} · {} · Voltage rating", port.name),
                    Status::Warning, "Declared voltage exceeds the connector rating.".into(), vec![port.id], vec![connector.block]));
            }
        }

        let mut unfinished = Vec::new();
        for pin in &connector.pins {
            if matches!(pin.role, PinRole::Ground | PinRole::Shield | PinRole::Return | PinRole::NotConnected) { continue; }
            let title = format!("Connector {name} / {} · {}", pin.number, pin.name);
            let port = pin.port.and_then(|id| project.port(id));
            if pin.role == PinRole::Unassigned || port.is_none() {
                unfinished.push(format!("Pin {} · {}", pin.number, pin.name));
            }
            let Some(port) = port else { continue };
            if pin.role == PinRole::Power && !port.kind.power() || pin.role == PinRole::Signal && port.kind.power() {
                results.push(finding(Subject::Port(port.id), format!("{title} · Port type"), Status::Warning,
                    "The physical pin role and logical-port type disagree. Configure the matching supply or signal port in Ports and templates.".into(), vec![port.id], vec![connector.block]));
            }
            // A family can represent multiple conductors. Its protection and
            // voltage are reviewed once above, without using aggregate family
            // current as the current in any individual contact.
            if port.block == connector.block && port.pin.trim().is_empty() { continue; }
            let component = interfaces::wire_component(project, port.id);
            let protections: Vec<_> = project.interfaces.protections.iter().filter(|p| crate::interface_catalog::protection_on_connector_side(p, port, &component)).collect();
            let required = crate::interface_catalog::protection_faults(connector, pin.role == PinRole::Power || port.kind.power());
            for fault in required {
                let covered: Vec<_> = protections.iter().filter(|p| p.covers.contains(&fault)).collect();
                let (status, detail) = if !pin.exception.trim().is_empty() && pin.waived.contains(&fault) {
                    (Status::Manual, format!("Declared exception / protection provided elsewhere · {}", pin.exception))
                } else if covered.is_empty() {
                    (Status::Warning, "No protection covering this fault is declared on this connector's connected net.".into())
                } else {
                    (Status::Manual, format!("Declared on this net: {}. Verify ratings, fault paths, placement, and return impedance; declaration is not proof of immunity.",
                        covered.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")))
                };
                results.push(finding(Subject::Port(port.id), format!("{title} · {}", fault.label()), status, detail, vec![port.id], vec![connector.block]));
            }
            if port.kind.power() {
                if connector.max_uv.zip(port.max_uv).is_some_and(|(rating, max)| max > rating) {
                    results.push(finding(Subject::Port(port.id), format!("{title} · Voltage rating"), Status::Warning,
                        "The declared working voltage exceeds the connector rating.".into(), vec![port.id], vec![connector.block]));
                }
                if connector.contact_current_ua.zip(port.current_ua).is_some_and(|(rating, current)| current > rating) {
                    results.push(finding(Subject::Port(port.id), format!("{title} · Contact current"), Status::Warning,
                        "Declared current exceeds the per-contact rating. Review parallel-contact sharing, temperature derating, cable ampacity, and fault duration.".into(), vec![port.id], vec![connector.block]));
                }
                for protection in &protections {
                    if protection.current_limit_ua.zip(connector.contact_current_ua).is_some_and(|(limit, rating)| limit > rating) {
                        results.push(finding(Subject::Port(port.id), format!("{title} · Overcurrent coordination"), Status::Warning,
                            "The protection current limit exceeds the connector's continuous rating. Review trip time and overload withstand.".into(), vec![port.id], vec![connector.block]));
                    }
                }
            }
        }
        if !unfinished.is_empty() {
            let mut check = finding(Subject::Block(connector.block),
                format!("{name} · {} physical contacts need assignment", unfinished.len()), Status::Open,
                "Complete the physical pinout when the part and contact mapping are known.".into(), Vec::new(), vec![connector.block]);
            check.evidence = unfinished;
            results.push(check);
        }
    }
    for p in &project.interfaces.protections {
        let mut evidence = vec![format!("{} · {}", if p.shunt { "Shunt attachment" } else { "Series element" }, p.name),
            format!("Event / waveform · {}", p.event), format!("Return reference · {}", p.return_reference), format!("Evidence · {}", p.evidence)];
        let component = interfaces::wire_component(project, p.entry);
        let mut warning = p.covers.is_empty() || p.event.trim().is_empty() || p.evidence.trim().is_empty()
            || p.shunt && p.return_reference.trim().is_empty() || !p.shunt && p.exit.is_none();
        let mut ports: Vec<_> = component.iter().copied().collect(); ports.sort_unstable();
        for port in project.ports.iter().filter(|port| component.contains(&port.id)) {
            let max = port.max_uv.or_else(|| voltage_range(port).map(|(a,b)| a.unsigned_abs().max(b.unsigned_abs())));
            if p.standoff_uv.zip(max).is_some_and(|(standoff, max)| standoff < max) {
                warning = true; evidence.push(format!("{} · standoff is below normal operating voltage magnitude.", project.port_label(port)));
            }
            if let Some(spec) = &port.signal_spec {
                if p.clamp_uv.zip(spec.values.get(&Parameter::AbsMax)).is_some_and(|(clamp, &absolute)| i128::from(clamp) > i128::from(absolute)) {
                    warning = true; evidence.push(format!("{} · clamp exceeds the declared absolute maximum; review pulse withstand and conditions.", project.port_label(port)));
                }
                let total_cap:u128=project.interfaces.protections.iter().filter(|other|other.shunt&&component.contains(&other.entry))
                    .filter_map(|other|other.capacitance_pf).map(u128::from).sum();
                if spec.values.get(&Parameter::Capacitance).is_some_and(|&budget|port.kind.source()&&total_cap>budget.max(0) as u128) {
                    warning = true; evidence.push("Protection capacitance alone exceeds the source's declared capacitive load limit.".into());
                }
            }
        }
        if p.covers.iter().any(|f| matches!(f, Fault::Esd | Fault::Surge | Fault::Eft))
            && (p.standoff_uv.is_none() || p.clamp_uv.is_none() || p.clamp_current_ua.is_none()) {
            warning = true; evidence.push("Transient protection needs standoff, clamp voltage, and stated clamp current/waveform.".into());
        }
        let mut result = finding(Subject::Block(p.block), format!("Protection review · {}", p.name),
            if warning { Status::Warning } else { Status::Manual },
            "Coverage and declared ratings only. Check bypass paths, grounding, coordination, and layout; verify immunity with the applicable test.".into(), ports, vec![p.block]);
        result.evidence = evidence; results.push(result);
    }
    for source in project.ports.iter().filter(|p| p.kind == PortKind::SignalOut) {
        let loads: Vec<_> = project.bindings.iter().filter(|b| b.source == source.id).filter_map(|b| project.port(b.target)).collect();
        if loads.len() <= 1 { continue; }
        let mut status = Status::Manual;
        let mut evidence = Vec::new();
        for key in [Parameter::SourceCurrent, Parameter::SinkCurrent, Parameter::Capacitance] {
            let capability = source.signal_spec.as_ref().and_then(|s| s.values.get(&key)).copied();
            let demands: Option<Vec<_>> = loads.iter().map(|p| p.signal_spec.as_ref()?.values.get(&key).copied()).collect();
            match (capability, demands) {
                (Some(cap), Some(values)) if values.iter().map(|v| i128::from(*v)).sum::<i128>() > i128::from(cap) => {
                    status = Status::Fail; evidence.push(format!("Aggregate {} exceeds the driver capability.", key.label(true)));
                }
                (None, _) | (_, None) => { if status != Status::Fail { status = Status::Open; } evidence.push(format!("Fan-out {} values are incomplete.", key.label(true))); }
                _ => {}
            }
        }
        let mut f = finding(Subject::Port(source.id), format!("Signal fan-out · {}", project.port_label(source)), status,
            "One driver may feed multiple inputs. Review aggregate loading and interconnect timing.".into(), vec![source.id], vec![source.block]);
        f.evidence = evidence; results.push(f);
    }
    results
}

pub fn voltage_range(port: &Port) -> Option<(i64, i64)> {
    let values = &port.signal_spec.as_ref()?.values;
    Some((*values.get(&Parameter::VoltageMin)?, *values.get(&Parameter::VoltageMax)?))
}

pub fn append_markdown(project: &Project, out: &mut String) {
    use crate::review::literal;
    if project.interfaces.is_empty() { return; }
    out.push_str("\n## Shared contracts and integration\n\n");
    for c in &project.interfaces.contracts {
        let _ = writeln!(out, "### Contract {} · {}\n\n{}\n", c.id, literal(&c.name), literal(&c.description));
        let _ = writeln!(out, "Type: {} · Reference: {}\n", c.limits.kind.label(), literal(&c.limits.reference));
        for (key, value) in &c.limits.values { let _ = writeln!(out, "- {}: {} {}", key.label(false), crate::signal::number(*value, key.unit().1), key.unit().0); }
        for p in project.ports.iter().filter(|p| p.contract == Some(c.id)) { let _ = writeln!(out, "- Used by {}", literal(&project.port_label(p))); }
    }
    for c in &project.interfaces.connectors {
        let _ = writeln!(out, "\n### Connector · {} / {}\n\nPart: {} · Exposure: {}\n\nSource: {}\n\n{}\n\n| Pin | Function | Role | Port | Exception |\n| --- | --- | --- | --- | --- |",
            literal(&c.family), literal(&c.variant), literal(&c.part_number), c.exposure.label(), literal(&c.source), literal(&c.notes));
        for pin in &c.pins {
            let port = pin.port.and_then(|id| project.port(id)).map_or_else(|| "Unmapped".into(), |p| project.port_label(p));
            let scope=pin.waived.iter().map(|f|f.label()).collect::<Vec<_>>().join(", ");
            let exception=if pin.exception.is_empty() { String::new() } else { format!("{} · Scope: {}",pin.exception,scope) };
            let _ = writeln!(out, "| {} | {} | {} | {} | {} |", literal(&pin.number), literal(&pin.name), pin.role.label(), literal(&port), literal(&exception));
        }
        let _=writeln!(out,"\nWorking voltage µV: {:?} · Per-contact current µA: {:?} · Hot plug: {}\n\nCable: {} · Mate: {} · Shield/return: {}\n",c.max_uv,c.contact_current_ua,c.hot_plug,literal(&c.cable),literal(&c.mating),literal(&c.shielding));
    }
    for c in &project.interfaces.channels {
        let _ = writeln!(out, "\n### Signal conversion · {}\n\nType: {} · Active: {}\n\nAssumptions: {}\n", literal(&c.name), c.kind.label(), c.active, literal(&c.assumptions));
        let _ = writeln!(out, "Gain ppm: {:?} · Offset µV: {:?} · Offset error µV: {:?} · Resistors Ω: {:?}/{:?} · Resistor tolerance ppm: {:?}\nBandwidth Hz: {:?} · Resolution bits: {:?} · Sampling rate: {:?} · Reference: {}\n", c.gain_ppm, c.offset_uv, c.offset_error_uv, c.top_ohm, c.bottom_ohm, c.resistor_tolerance_ppm, c.bandwidth_hz, c.resolution_bits, c.sample_rate, literal(&c.reference));
    }
    for p in &project.interfaces.protections {
        let _ = writeln!(out, "\n### Protection · {}\n\n{} · Entry {} · Exit {:?}\n\nCovers: {}\n\nReturn: {} · Event: {} · Evidence: {}\n\n{}\n", literal(&p.name), if p.shunt { "Shunt" } else { "Series" }, p.entry, p.exit,
            p.covers.iter().map(|f| f.label()).collect::<Vec<_>>().join(", "), literal(&p.return_reference), literal(&p.event), literal(&p.evidence), literal(&p.notes));
        let _=writeln!(out,"Standoff µV: {:?} · Clamp µV: {:?} at {:?} µA · Limit µA: {:?} · Response ns: {:?} · Capacitance pF: {:?} · Leakage nA: {:?} · Series resistance mΩ: {:?}\n",p.standoff_uv,p.clamp_uv,p.clamp_current_ua,p.current_limit_ua,p.response_ns,p.capacitance_pf,p.leakage_na,p.series_milliohm);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connector_coverage_does_not_cross_signal_conversion() -> crate::model::Result<()> {
        let mut p = Project::new();
        let b = crate::interface_catalog::create(&mut p, 0, 4, "USB", true)?;
        assert!(evaluate(&p).iter().any(|f| f.status == Status::Warning));
        let port = p.block_ports(b).find(|p| p.name == "D-").unwrap().id;
        assert!(interfaces::wire_component(&p, port).contains(&port));
        let block=p.add_block(crate::model::Role::Custom,"Adapter".into(),false)?;
        let input=p.set_port(crate::model::Port::new(block,PortKind::SignalIn))?;
        let output=p.set_port(crate::model::Port::new(block,PortKind::SignalOut))?;
        p.connect(port,input)?;
        let mut channel=interfaces::Channel::new(block,input,output,interfaces::ConversionKind::Divider);
        channel.id=p.allocate()?;p.interfaces.channels.push(channel);
        let mut protection=interfaces::Protection::new(block,output,"Downstream clamp");
        protection.id=p.allocate()?;p.interfaces.protections.push(protection);
        let warning=|p:&Project|evaluate(p).iter().any(|f|f.title.contains("Connector USB / 2")&&f.title.ends_with("ESD")&&f.status==Status::Warning);
        assert!(warning(&p));
        let pin=p.interfaces.connectors[0].pins.iter_mut().find(|pin|pin.port==Some(port)).unwrap();
        pin.exception="ESD protection provided by the mating assembly".into();pin.waived=vec![Fault::Surge];
        assert!(warning(&p));
        p.interfaces.connectors[0].pins.iter_mut().find(|pin|pin.port==Some(port)).unwrap().waived.push(Fault::Esd);
        assert!(!warning(&p));
        p.interfaces.connectors[0].pins.iter_mut().find(|pin|pin.port==Some(port)).unwrap().exception.clear();
        p.interfaces.protections[0].entry=input;
        assert!(!warning(&p));
        p.validate()
    }
    #[test]
    fn choosing_a_contract_cannot_change_sensor_hardware_limits()->crate::model::Result<()> {
        let mut p=Project::new();let block=p.add_block(crate::model::Role::Input,"Sensor".into(),false)?;
        let mut port=Port::new(block,PortKind::SignalOut);
        port.signal_spec=Some(crate::signal::Spec{kind:Kind::AnalogVoltage,..Default::default()});
        port.signal_spec.as_mut().unwrap().values.extend([(Parameter::VoltageMin,0),(Parameter::VoltageMax,5_000_000)]);
        let mut contract=interfaces::Contract::from_port(&port);
        contract.limits.values.insert(Parameter::VoltageMax,3_300_000);
        port.contract=Some(p.put_contract(contract)?);let id=p.set_port(port)?;
        assert!(evaluate(&p).iter().any(|f|f.subject==Subject::Port(id)&&f.status==Status::Fail));
        assert_eq!(p.port(id).unwrap().signal_spec.as_ref().unwrap().values[&Parameter::VoltageMax],5_000_000);
        p.validate()
    }
}
