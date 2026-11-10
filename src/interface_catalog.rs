use crate::{interfaces::{Connector, Exposure, Fault, Pin, PinRole}, model::{Id, Port, PortKind, Project, Result, Role}};

pub struct Preset {
    pub category: &'static str, pub family: &'static str, pub variant: &'static str,
    pub count: u16, pub assignments: &'static str, pub source: &'static str,
}

const USB: &str = "https://www.usb.org/documents?search=connector";
const IO: &str = "https://www.amphenol-cs.com/connectors/input-output.html";
const PCB: &str = "https://www.te.com/en/products/connectors/pcb-connectors.html";
const JST: &str = "https://www.jst-mfg.com/product/index.php?lang=2";
const POWER: &str = "https://www.te.com/en/products/connectors/power-connectors.html";

macro_rules! item {
    ($cat:expr,$family:expr,$variant:expr,$n:expr,$source:expr) => { Preset { category:$cat, family:$family, variant:$variant, count:$n, assignments:"", source:$source } };
    ($cat:expr,$family:expr,$variant:expr,$n:expr,$source:expr,$pins:expr) => { Preset { category:$cat, family:$family, variant:$variant, count:$n, assignments:$pins, source:$source } };
}

// These are system-definition templates. Part-specific ratings and mechanical pinouts are never inferred.
pub static PRESETS: &[Preset] = &[
    item!("USB","USB Type-A","USB 2.0 receptacle",4,USB,"1:VBUS:P,2:D-:S,3:D+:S,4:GND:G"),
    item!("USB","USB Type-B","USB 2.0 receptacle",4,USB,"1:VBUS:P,2:D-:S,3:D+:S,4:GND:G"),
    item!("USB","USB Mini-B","USB 2.0 receptacle",5,USB,"1:VBUS:P,2:D-:S,3:D+:S,4:ID:S,5:GND:G"),
    item!("USB","USB Micro-B","USB 2.0 receptacle",5,USB,"1:VBUS:P,2:D-:S,3:D+:S,4:ID:S,5:GND:G"),
    item!("USB","USB Type-A","SuperSpeed contact worksheet",9,USB),
    item!("USB","USB Type-B","SuperSpeed contact worksheet",9,USB),
    item!("USB","USB Micro-B","SuperSpeed contact worksheet",10,USB),
    item!("USB","USB Type-C","Full contact receptacle",24,USB,
        "A1:GND:G,A2:TX1+:S,A3:TX1-:S,A4:VBUS:P,A5:CC1:S,A6:D+:S,A7:D-:S,A8:SBU1:S,A9:VBUS:P,A10:RX2-:S,A11:RX2+:S,A12:GND:G,B1:GND:G,B2:TX2+:S,B3:TX2-:S,B4:VBUS:P,B5:CC2:S,B6:D+:S,B7:D-:S,B8:SBU2:S,B9:VBUS:P,B10:RX1-:S,B11:RX1+:S,B12:GND:G"),
    item!("USB","USB Type-C","USB 2.0 receptacle contacts",16,USB,
        "A1:GND:G,A4:VBUS:P,A5:CC1:S,A6:D+:S,A7:D-:S,A8:SBU1:S,A9:VBUS:P,A12:GND:G,B1:GND:G,B4:VBUS:P,B5:CC2:S,B6:D+:S,B7:D-:S,B8:SBU2:S,B9:VBUS:P,B12:GND:G"),
    item!("Audio and video","HDMI","Type A contact worksheet",19,IO),
    item!("Audio and video","HDMI","Type C mini contact worksheet",19,IO),
    item!("Audio and video","HDMI","Type D micro contact worksheet",19,IO),
    item!("Audio and video","DisplayPort","Full size contact worksheet",20,IO),
    item!("Audio and video","DisplayPort","Mini contact worksheet",20,IO),
    item!("Audio and video","DVI","DVI-D single-link worksheet",19,IO),
    item!("Audio and video","DVI","DVI-D dual-link worksheet",25,IO),
    item!("Audio and video","VGA","DE-15 worksheet",15,IO),
    item!("Audio and video","Audio jack","TS",2,IO,"T:Tip:S,S:Sleeve:G"),
    item!("Audio and video","Audio jack","TRS stereo",3,IO,"T:Left:S,R:Right:S,S:Ground:G"),
    item!("Audio and video","Audio jack","TRRS CTIA logical contacts",4,IO,"T:Left:S,R1:Right:S,R2:Ground:G,S:Microphone:S"),
    item!("Audio and video","Audio jack","TRRS OMTP logical contacts",4,IO,"T:Left:S,R1:Right:S,R2:Microphone:S,S:Ground:G"),
    item!("Audio and video","RCA","Single coaxial contact pair",2,IO,"1:Signal:S,2:Shield:H"),
    item!("Networking","8P8C modular","Ethernet contact worksheet",8,IO),
    item!("Networking","8P8C modular","Integrated magnetics / LEDs (enter total contacts)",0,IO),
    item!("Networking","Modular","6P6C worksheet",6,IO),
    item!("Networking","Single-pair Ethernet","2 signal contacts plus shield worksheet",3,IO),
    item!("Headers and ribbon","Pin header","2.54 mm single row",0,PCB),
    item!("Headers and ribbon","Pin header","2.54 mm double row",0,PCB),
    item!("Headers and ribbon","Pin header","2.00 mm single/double row",0,PCB),
    item!("Headers and ribbon","Pin header","1.27 mm single/double row",0,PCB),
    item!("Headers and ribbon","Shrouded IDC","Ribbon header",0,PCB),
    item!("Headers and ribbon","Socket header","Selectable contact count",0,PCB),
    item!("Headers and ribbon","Debug header","10-position worksheet (assign debug standard)",10,PCB),
    item!("Headers and ribbon","Debug header","20-position worksheet (assign debug standard)",20,PCB),
    item!("Wire connectors","JST PH","Family worksheet",0,JST),
    item!("Wire connectors","JST XH","Family worksheet",0,"https://www.jst-mfg.com/product/index.php?lang=2&series=277"),
    item!("Wire connectors","JST SH","Family worksheet",0,JST),
    item!("Wire connectors","JST GH","Family worksheet",0,JST),
    item!("Wire connectors","JST ZH","Family worksheet",0,JST),
    item!("Wire connectors","JST VH","Family worksheet",0,JST),
    item!("Wire connectors","Crimp housing","Custom series and contact count",0,PCB),
    item!("Flexible cables","FFC/FPC","Top contact worksheet",0,PCB),
    item!("Flexible cables","FFC/FPC","Bottom contact worksheet",0,PCB),
    item!("Flexible cables","FFC/FPC","Dual contact worksheet",0,PCB),
    item!("Board to board","Mezzanine","Selectable contact count / stack height",0,PCB),
    item!("Board to board","Card edge","Selectable positions and sides",0,PCB),
    item!("Board to board","Spring contact","Selectable positions",0,PCB),
    item!("Cards and expansion","M.2","Key-specific worksheet (enter actual contact numbers)",0,"https://pcisig.com/specification-overview/pci-express-m2"),
    item!("Cards and expansion","PCI Express","Card connector worksheet (select width and pinout)",0,"https://pcisig.com/specifications"),
    item!("Cards and expansion","SATA","Signal contacts",7,IO),
    item!("Cards and expansion","SATA","Power contacts",15,POWER),
    item!("Cards and expansion","SD","Full-size logical contacts",9,PCB),
    item!("Cards and expansion","microSD","Logical contacts",8,PCB),
    item!("Cards and expansion","SIM","6-contact worksheet",6,PCB),
    item!("Cards and expansion","SIM","8-contact worksheet",8,PCB),
    item!("Power","DC barrel","2-contact worksheet (set polarity)",2,POWER),
    item!("Power","DC barrel","Switched 3-contact worksheet",3,POWER),
    item!("Power","Terminal block","Screw terminals",0,POWER),
    item!("Power","Terminal block","Spring terminals",0,POWER),
    item!("Power","Terminal block","Pluggable terminals",0,POWER),
    item!("Power","Power entry","AC inlet L/N/PE logical contacts",3,POWER,"L:Line:P,N:Neutral:R,PE:Protective earth:G"),
    item!("Power","Battery connector","2-pole custom pinout",2,POWER),
    item!("Power","Battery connector","Power / temperature / ID worksheet",0,POWER),
    item!("Power","Blade / spade","Single terminal",1,POWER),
    item!("Industrial and RF","D-sub","DE-9",9,IO),
    item!("Industrial and RF","D-sub","DA-15",15,IO),
    item!("Industrial and RF","D-sub","DB-25",25,IO),
    item!("Industrial and RF","D-sub","DC-37",37,IO),
    item!("Industrial and RF","Circular","M8 pinout worksheet",0,IO),
    item!("Industrial and RF","Circular","M12 coding / pinout worksheet",0,IO),
    item!("Industrial and RF","Circular","Custom keying / contact count",0,IO),
    item!("Industrial and RF","Coaxial RF","SMA logical contacts",2,IO,"1:Center:S,2:Shield:H"),
    item!("Industrial and RF","Coaxial RF","BNC logical contacts",2,IO,"1:Center:S,2:Shield:H"),
    item!("Industrial and RF","Coaxial RF","Miniature board RF logical contacts",2,PCB,"1:Center:S,2:Shield:H"),
    item!("Custom and test","Test point","Single contact",1,PCB),
    item!("Custom and test","Test fixture","Spring probe array",0,PCB),
    item!("Custom and test","Custom connector","User-defined contacts",0,PCB),
];

impl Preset {
    pub fn label(&self) -> String { format!("{} · {}", self.family, self.variant) }
    pub fn pins(&self, count: u16) -> Vec<Pin> {
        if self.assignments.is_empty() {
            return (1..=if self.count == 0 { count } else { self.count }).map(|number| Pin {
                number: number.to_string(), name: format!("Pin {number}"), role: PinRole::Unassigned,
                port: None, exception: String::new(), waived: Vec::new(),
            }).collect();
        }
        self.assignments.split(',').map(|entry| {
            let fields: Vec<_> = entry.split(':').collect();
            Pin { number: fields[0].into(), name: fields[1].into(), role: match fields[2] {
                "P" => PinRole::Power, "R" => PinRole::Return, "G" => PinRole::Ground, "H" => PinRole::Shield, _ => PinRole::Signal,
            }, port: None, exception: String::new(), waived: Vec::new() }
        }).collect()
    }
}

pub fn generic_preset() -> usize {
    PRESETS.iter().position(|preset| preset.family == "Custom connector").unwrap_or(0)
}

fn create_record(project: &mut Project, index: usize, count: u16, name: &str, exposure: Exposure) -> Result<Id> {
    let preset = PRESETS.get(index).ok_or("Connector preset is missing.")?;
    if !(1..=1024).contains(&count) { return Err("Choose 1–1024 contacts.".into()); }
    if name.trim().is_empty() { return Err("Give the connector a name.".into()); }
    let block = project.add_block(Role::Custom, name.trim().into(), false)?;
    let pins = preset.pins(count);
    let id = project.allocate()?;
    project.interfaces.connectors.push(Connector { id, block, family: preset.family.into(), variant: preset.variant.into(),
        part_number: String::new(), source: preset.source.into(), exposure, pins,
        required: if exposure == Exposure::Internal { Vec::new() } else { vec![Fault::Esd] },
        max_uv: None, contact_current_ua: None, cable: String::new(), mating: String::new(), shielding: String::new(), hot_plug: false,
        notes: "System-definition template. Confirm exact contact numbers, keying, capabilities, and ratings against the chosen part drawing. Generic worksheets have unassigned pin roles.".into() });
    Ok(block)
}

/// Legacy pin-level fixture for storage and review regression tests.
#[cfg(test)]
pub fn create(project: &mut Project, index: usize, count: u16, name: &str, incoming: bool) -> Result<Id> {
    let block = create_record(project, index, count, name, Exposure::External)?;
    let mut pins = std::mem::take(&mut project.interfaces.connectors.last_mut().unwrap().pins);
    for pin in &mut pins {
        if matches!(pin.role, PinRole::Ground | PinRole::Shield | PinRole::Return | PinRole::NotConnected) { continue; }
        let kind = match (pin.role == PinRole::Power, incoming) {
            (true, true) => PortKind::Rail, (true, false) => PortKind::Load,
            (false, true) => PortKind::SignalOut, (false, false) => PortKind::SignalIn,
        };
        let mut port = Port::new(block, kind);
        port.name = pin.name.clone(); port.pin = pin.number.clone(); port.required = false;
        pin.port = Some(project.set_port(port)?);
    }
    project.interfaces.connectors.last_mut().unwrap().pins = pins;
    Ok(block)
}

fn preset_families(family: &str) -> &'static [(&'static str, bool)] {
    match family {
        "HDMI" => &[("TMDS", false), ("CEC", false), ("I2C", false), ("Power", true)],
        "DC barrel" => &[("Power", true)],
        _ => &[],
    }
}

pub fn family_preview(index: usize) -> String {
    let families = preset_families(PRESETS[index].family);
    if families.is_empty() {
        "Physical pinout only · add logical families in Connector properties".into()
    } else {
        format!("Canvas ports · {}", families.iter().map(|(name, _)| *name).collect::<Vec<_>>().join(", "))
    }
}

fn add_preset_families(project: &mut Project, block: Id, family: &str, into_board: bool) -> Result<()> {
    let signal = if into_board { PortKind::SignalOut } else { PortKind::SignalIn };
    let power = if into_board { PortKind::Rail } else { PortKind::Load };
    let families = preset_families(family);
    for &(name, is_power) in families {
        add_family(project, block, name, if is_power { power } else { signal })?;
    }
    Ok(())
}

pub fn add_family(project: &mut Project, block: Id, name: &str, kind: PortKind) -> Result<Id> {
    let name = name.trim();
    if name.is_empty() { return Err("Give the signal family a name.".into()); }
    if project.block_ports(block).any(|port| port.name.eq_ignore_ascii_case(name)) {
        return Err("A port with that name already exists on this connector.".into());
    }
    let mut port = Port::new(block, kind);
    port.name = name.into();
    port.required = false;
    project.set_port(port)
}

pub fn create_families(
    project: &mut Project, index: usize, count: u16, name: &str, exposure: Exposure, into_board: bool,
) -> Result<Id> {
    let block = create_record(project, index, count, name, exposure)?;
    add_preset_families(project, block, PRESETS[index].family, into_board)?;
    Ok(block)
}

pub fn instantiate_template(
    project: &mut Project, mut connector: Connector, name: &str, exposure: Exposure, into_board: bool,
) -> Result<Id> {
    if name.trim().is_empty() { return Err("Give the connector a name.".into()); }
    let block = project.add_block(Role::Custom, name.trim().into(), false)?;
    connector.id = project.allocate()?;
    connector.block = block;
    connector.exposure = exposure;
    if exposure != Exposure::Internal && !connector.required.contains(&Fault::Esd) {
        connector.required.push(Fault::Esd);
    }
    for pin in &mut connector.pins {
        pin.port = None;
        pin.exception.clear();
        pin.waived.clear();
    }
    add_preset_families(project, block, &connector.family, into_board)?;
    project.interfaces.connectors.push(connector);
    Ok(block)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hdmi_families_and_contact_worksheet_are_independent() -> Result<()> {
        use crate::interfaces::{Edge, PortPosition};
        let preset = PRESETS.iter().position(|preset| preset.family == "HDMI").unwrap();
        let mut project = Project::new();
        let block = create_families(&mut project, preset, 4, "HDMI input", Exposure::External, true)?;
        let families: Vec<_> = project.block_ports(block).map(|port| port.name.as_str()).collect();
        assert_eq!(families, ["TMDS", "CEC", "I2C", "Power"]);
        let pins = project.interfaces.connectors[0].pins.clone();
        assert_eq!(pins.len(), 19);
        assert!(pins.iter().all(|pin| pin.port.is_none()));
        let tmds = project.block_ports(block).find(|port| port.name == "TMDS").unwrap().id;
        let receiver = project.add_block(Role::Processing, "Receiver".into(), false)?;
        let input = project.set_port(Port::new(receiver, PortKind::SignalIn))?;
        project.connect(tmds, input)?;
        let bindings = project.bindings.clone();
        let placement = PortPosition { edge: Edge::Top, slot: 1 };
        project.position_port(tmds, placement)?;
        assert_eq!(project.port(tmds).unwrap().layout, Some(placement));
        assert_eq!(project.bindings, bindings);
        assert_eq!(project.interfaces.connectors[0].pins, pins);
        project.reset_port_position(tmds)?;
        assert_eq!(project.port(tmds).unwrap().layout, None);
        assert_eq!(project.bindings, bindings);
        assert_eq!(project.interfaces.connectors[0].pins, pins);
        project.validate()
    }

    #[test]
    fn catalog_contacts_are_unique_and_templates_create_valid_projects() -> Result<()> {
        for (index, preset) in PRESETS.iter().enumerate() {
            let mut project = Project::new();
            create(&mut project, index, 8, "Connector", true)?;
            let pins = &project.interfaces.connectors[0].pins;
            assert_eq!(pins.len(), usize::from(if preset.count == 0 { 8 } else { preset.count }));
            project.validate()?;
        }
        let usb = PRESETS.iter().find(|p| p.family == "USB Type-C" && p.count == 24).unwrap();
        assert_eq!(usb.pins(24).iter().filter(|p| p.role == PinRole::Power).count(), 4);
        Ok(())
    }
}

pub(crate) fn protection_faults(connector: &Connector, power: bool) -> Vec<Fault> {
    let mut faults = connector.required.clone();
    if connector.exposure != Exposure::Internal && power {
        faults.extend([Fault::OverVoltage, Fault::OverCurrent]);
        if connector.hot_plug { faults.push(Fault::Inrush); }
    }
    faults.sort_by_key(|fault| *fault as u8);
    faults.dedup();
    faults
}

pub(crate) fn protection_on_connector_side(
    protection: &crate::interfaces::Protection, port: &Port, net: &std::collections::HashSet<Id>,
) -> bool {
    net.contains(&protection.entry) || (!port.kind.source()
        && protection.exit.is_some_and(|exit| net.contains(&exit)))
}

pub fn protection_needed(project: &Project, connector: &Connector) -> Vec<(Id, Vec<Fault>)> {
    if connector.exposure == Exposure::Internal { return Vec::new(); }
    project.block_ports(connector.block).filter_map(|port| {
        let pin = connector.pins.iter().find(|pin| pin.port == Some(port.id));
        let net = crate::interfaces::wire_component(project, port.id);
        let mut faults = protection_faults(connector, port.kind.power());
        faults.retain(|fault| {
            !pin.is_some_and(|pin| !pin.exception.trim().is_empty() && pin.waived.contains(fault))
                && !project.interfaces.protections.iter().any(|stage| {
                    protection_on_connector_side(stage, port, &net) && stage.covers.contains(fault)
                })
        });
        (!faults.is_empty()).then_some((port.id, faults))
    }).collect()
}

pub(crate) struct ProtectionAdvice {
    pub standoff_floor_uv: Option<u64>,
    pub connector_voltage_uv: Option<u64>,
    pub source_current_ua: Option<u64>,
    pub contact_current_ua: Option<u64>,
    pub trip_candidate_ua: Option<u64>,
    pub load_at_or_above_ceiling: bool,
    pub clamp_ceiling_uv: Option<u64>,
    pub accepted_input_max_uv: Option<u64>,
    pub power: bool,
    pub ac: bool,
}

impl ProtectionAdvice {
    pub fn fill_missing(&self, stage: &mut crate::interfaces::Protection) -> bool {
        let mut filled = false;
        if let (None, Some(value)) = (stage.standoff_uv, self.standoff_floor_uv) {
            stage.standoff_uv = Some(value);
            filled = true;
        }
        if let (None, Some(value)) = (stage.current_limit_ua, self.trip_candidate_ua) {
            stage.current_limit_ua = Some(value);
            filled = true;
        }
        filled
    }
}

pub(crate) fn protection_advice(project: &Project, stage: &crate::interfaces::Protection) -> ProtectionAdvice {
    use crate::{electrical::PowerKind, interfaces::wire_component, signal::Parameter};
    let entry = project.port(stage.entry);
    let power = entry.is_some_and(|port| port.kind.power());
    let input_net = wire_component(project, stage.entry);
    let protected_net = stage.exit.filter(|_| !stage.shunt)
        .map(|exit| wire_component(project, exit)).unwrap_or_else(|| input_net.clone());
    let driven = || project.ports.iter().filter(|port| input_net.contains(&port.id) && port.kind.source());
    let positive = |value: u64| value > 0;
    let ac = power && (entry.is_some_and(|port| port.power.kind == PowerKind::Ac)
        || driven().any(|port| port.power.kind == PowerKind::Ac));
    let standoff_floor_uv = if ac { None } else {
        driven().filter_map(|port| {
            if power { port.max_uv } else {
                crate::interface_review::voltage_range(port)
                    .map(|(min, max)| min.unsigned_abs().max(max.unsigned_abs()))
            }
        }).filter(|&value| positive(value)).max()
    };
    let connectors = || project.interfaces.connectors.iter().filter(|connector| {
        project.block_ports(connector.block).any(|port| port.kind.power() == power
            && (input_net.contains(&port.id) || protected_net.contains(&port.id)))
    });
    let connector_voltage_uv = connectors().filter_map(|connector| connector.max_uv).filter(|&value| positive(value)).min();
    let contact_current_ua = connectors().filter_map(|connector| connector.contact_current_ua)
        .filter(|&value| positive(value)).min();
    let source_current_ua = if power {
        driven().filter_map(|port| port.current_ua).filter(|&value| positive(value)).min()
    } else { None };
    let ceiling = if power && !ac && !stage.shunt && stage.exit.is_some() && stage.covers.contains(&Fault::OverCurrent) {
        source_current_ua.zip(contact_current_ua).map(|(source, contact)| source.min(contact))
    } else { None };
    let load_at_or_above_ceiling = ceiling.zip(stage.exit).is_some_and(|(ceiling, exit)| {
        crate::power::Network::new(project, None).demand(exit).known_ua >= u128::from(ceiling)
    });
    let trip_candidate_ua = ceiling.filter(|_| !load_at_or_above_ceiling);
    let receivers = || project.ports.iter().filter(|port| !port.kind.source() && protected_net.contains(&port.id));
    let clamp_ceiling_uv = receivers().filter_map(|port| {
        let limit = port.signal_spec.as_ref().and_then(|spec| spec.values.get(&Parameter::AbsMax))?;
        u64::try_from(*limit).ok()
    }).filter(|&value| positive(value)).min();
    let accepted_input_max_uv = receivers().filter(|port| port.kind == PortKind::Load)
        .filter_map(|port| port.max_uv).filter(|&value| positive(value)).min();
    ProtectionAdvice { standoff_floor_uv, connector_voltage_uv, source_current_ua, contact_current_ua,
        trip_candidate_ua, load_at_or_above_ceiling, clamp_ceiling_uv, accepted_input_max_uv, power, ac }
}

pub fn add_recommended_protection(project: &mut Project, connector_id: Id) -> Result<Vec<Id>> {
    use crate::interface_ui::{create_block, NewBlock, ProtectionType};
    let connector = project.interfaces.connectors.iter().find(|connector| connector.id == connector_id)
        .cloned().ok_or("Connector no longer exists.")?;
    let needed = protection_needed(project, &connector);
    let connector_block = project.block(connector.block).ok_or("Connector block no longer exists.")?.clone();
    let mut created = Vec::new();

    for (id, faults) in needed {
        let port = project.port(id).ok_or("Connector port no longer exists.")?.clone();
        let name = format!("{} · {} protection", connector_block.name, port.name);
        let has_series_power = project.interfaces.protections.iter().any(|stage| {
            !stage.shunt && protection_on_connector_side(stage, &port, &crate::interfaces::wire_component(project, id))
        });
        let series = port.kind.power() && !has_series_power;
        let kind = if series { ProtectionType::SeriesPower } else { ProtectionType::Shunt };
        let block = create_block(project, &name, NewBlock::Protection(kind, Some(id)))?;
        if let Some(anchor) = connector_block.position {
            let index = created.len() as i32;
            project.position_block(block, Some(crate::model::BlockPosition {
                x: anchor.x + if port.kind.source() { 290 } else { -290 },
                y: anchor.y + index * 150,
            }))?;
        }
        let stage_index = project.interfaces.protections.len() - 1;
        let stage = &mut project.interfaces.protections[stage_index];
        stage.covers = faults;
        stage.return_reference = "Connector return / chassis strategy (confirm)".into();
        stage.event = "External connector exposure (verify applicable test levels and waveforms)".into();
        stage.notes = "Planning values from the connected supply and connector. Confirm device ratings, coordination, and return path.".into();
        let (entry, exit) = (stage.entry, stage.exit);
        if let Some(exit) = exit {
            for endpoint in [entry, exit] {
                let mut target = project.port(endpoint).unwrap().clone();
                target.power = port.power;
                target.min_uv = port.min_uv;
                target.max_uv = port.max_uv;
                target.nominal_uv = port.nominal_uv;
                target.rail_domain = port.rail_domain.clone();
                target.current_ua = None;
                project.set_port(target)?;
            }
            // Keep existing downstream links and their IDs, so adding a stage is a single undoable edit.
            for link in &mut project.bindings {
                if port.kind.source() && link.source == id { link.source = exit; }
                if !port.kind.source() && link.target == id { link.target = entry; }
            }
            if port.kind.source() { project.connect(id, entry)?; }
            else { project.connect(exit, id)?; }
        }
        let advice = protection_advice(project, &project.interfaces.protections[stage_index]);
        advice.fill_missing(&mut project.interfaces.protections[stage_index]);
        if let Some(exit) = exit {
            if let Some(rating) = advice.contact_current_ua {
                let output = project.ports.iter_mut().find(|port| port.id == exit).ok_or("Protection output is missing.")?;
                output.current_ua = Some(rating);
            }
        }
        created.push(block);
    }
    project.sync_protection_rails();
    Ok(created)
}

#[cfg(test)]
mod protection_tests {
    use super::*;

    #[test]
    fn recommendations_use_connected_ratings_even_when_entered_after_insertion() -> Result<()> {
        let mut project = Project::new();
        let preset = PRESETS.iter().position(|preset| preset.family == "DC barrel").unwrap();
        let block = create_families(&mut project, preset, 2, "DC input", Exposure::External, true)?;
        let connector = project.interfaces.connectors[0].id;
        add_recommended_protection(&mut project, connector)?;
        let mut stage = project.interfaces.protections[0].clone();
        assert_eq!(stage.standoff_uv, None);
        assert_eq!(stage.current_limit_ua, None);

        let mut supply = project.block_ports(block).next().unwrap().clone();
        supply.min_uv = Some(11_400_000);
        supply.max_uv = Some(12_600_000);
        supply.current_ua = Some(2_000_000);
        project.set_port(supply)?;
        project.interfaces.connectors[0].contact_current_ua = Some(1_500_000);
        let advice = protection_advice(&project, &stage);
        assert!(advice.fill_missing(&mut stage));
        assert_eq!(stage.standoff_uv, Some(12_600_000));
        assert_eq!(stage.current_limit_ua, Some(1_500_000));
        assert_eq!(stage.clamp_uv, None);
        stage.current_limit_ua = Some(1_250_000);
        assert!(!advice.fill_missing(&mut stage));
        assert_eq!(stage.current_limit_ua, Some(1_250_000));
        Ok(())
    }

    #[test]
    fn inserted_power_stage_reuses_existing_links_and_tracks_input_voltage() -> Result<()> {
        let mut project = Project::new();
        let preset = PRESETS.iter().position(|preset| preset.family == "DC barrel").unwrap();
        let block = create_families(&mut project, preset, 2, "DC input", Exposure::External, true)?;
        let mut supply = project.block_ports(block).next().unwrap().clone();
        supply.min_uv = Some(4_750_000);
        supply.max_uv = Some(5_250_000);
        supply.current_ua = Some(500_000);
        let supply = project.set_port(supply)?;
        let load_block = project.add_block(Role::Processing, "Receiver".into(), false)?;
        let load = project.set_port(Port::new(load_block, PortKind::Load))?;
        let original_link = project.connect(supply, load)?;
        let connector = project.interfaces.connectors[0].id;

        assert_eq!(add_recommended_protection(&mut project, connector)?.len(), 1);
        project.validate()?;
        let stage = project.interfaces.protections[0].clone();
        let output = stage.exit.unwrap();
        assert_eq!(project.binding(original_link).unwrap().source, output);
        assert_eq!(project.port(output).unwrap().nominal_voltage(), Some(5_000_000));
        assert!(stage.covers.contains(&Fault::OverCurrent));
        assert!(stage.covers.contains(&Fault::OverVoltage));
        assert!(protection_needed(&project, &project.interfaces.connectors[0]).is_empty());

        let mut supply_port = project.port(supply).unwrap().clone();
        supply_port.min_uv = Some(4_900_000);
        supply_port.max_uv = Some(5_100_000);
        project.set_port(supply_port)?;
        project.sync_protection_rails();
        assert_eq!(project.port(output).unwrap().range(), Some((4_900_000, 5_100_000)));
        project.interfaces.protections[0].manual_rail_voltage = true;
        let mut protected = project.port(output).unwrap().clone();
        protected.min_uv = Some(4_800_000);
        project.set_port(protected)?;
        project.sync_protection_rails();
        assert_eq!(project.port(output).unwrap().min_uv, Some(4_800_000));
        project.validate()
    }
}
