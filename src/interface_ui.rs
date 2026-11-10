use crate::{interface_catalog, interface_review, interfaces::*, model::{Id, Port, PortKind, Project, Result, Role}, signal::{Kind, Parameter, Spec}};
use eframe::egui;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab { #[default] Contracts, Ports, Conversion, Connectors, Protection }
impl Tab {
    const ALL: [Self; 5] = [Self::Contracts, Self::Ports, Self::Conversion, Self::Connectors, Self::Protection];
    fn label(self) -> &'static str { match self { Self::Contracts => "Contracts", Self::Ports => "Ports and templates", Self::Conversion => "Signal conversion", Self::Connectors => "Connectors", Self::Protection => "Protection" } }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProtectionType { SeriesPower, SeriesSignal, Shunt }
impl ProtectionType {
    pub const ALL: [Self; 3] = [Self::SeriesPower, Self::SeriesSignal, Self::Shunt];
    pub fn label(self) -> &'static str { match self {
        Self::SeriesPower => "Series power protection", Self::SeriesSignal => "Series signal protection", Self::Shunt => "Shunt / clamp on an existing net",
    } }
}

#[derive(Clone, Copy)]
pub enum NewBlock { Conversion(ConversionKind), Protection(ProtectionType, Option<Id>) }

pub fn create_block(project: &mut Project, name: &str, setup: NewBlock) -> Result<Id> {
    if name.trim().is_empty() { return Err("Give the block a name.".into()); }
    if let NewBlock::Protection(ProtectionType::Shunt, entry) = setup {
        if !entry.is_some_and(|id| project.port(id).is_some()) { return Err("Select an existing port/net for the shunt protection.".into()); }
    }
    let block = project.add_block(Role::Custom, name.trim().into(), false)?;
    match setup {
        NewBlock::Conversion(kind) => {
            let (a, b) = kind.types().unwrap_or((Kind::Digital, Kind::Digital));
            let mut input = Port::new(block, PortKind::SignalIn); input.name = "Signal in".into();
            input.signal_spec = Some(Spec { kind: a, ..Spec::default() });
            let mut output = Port::new(block, PortKind::SignalOut); output.name = "Signal out".into();
            output.signal_spec = Some(Spec { kind: b, ..Spec::default() });
            let input = project.set_port(input)?; let output = project.set_port(output)?;
            let mut channel = Channel::new(block, input, output, kind); channel.id = project.allocate()?;
            if channel.active { channel.supplies.push(project.set_port(Port::new(block, PortKind::Load))?); }
            project.interfaces.channels.push(channel);
        }
        NewBlock::Protection(kind, attached) => {
            let (entry, exit) = match kind {
                ProtectionType::Shunt => (attached.unwrap(), None),
                ProtectionType::SeriesPower | ProtectionType::SeriesSignal => {
                    let power = kind == ProtectionType::SeriesPower;
                    let input = project.set_port(Port::new(block, if power { PortKind::Load } else { PortKind::SignalIn }))?;
                    let output = project.set_port(Port::new(block, if power { PortKind::Rail } else { PortKind::SignalOut }))?;
                    (input, Some(output))
                }
            };
            let mut protection = Protection::new(block, entry, name.trim()); protection.id = project.allocate()?;
            protection.exit = exit; protection.shunt = kind == ProtectionType::Shunt;
            protection.covers = match kind { ProtectionType::SeriesPower => vec![Fault::OverCurrent],
                ProtectionType::SeriesSignal => Vec::new(), ProtectionType::Shunt => vec![Fault::Esd] };
            project.interfaces.protections.push(protection);
        }
    }
    Ok(block)
}

pub struct Editor {
    draft: Option<Project>, base: Option<Project>, block: Option<Id>, selected: Option<Id>, tab: Tab,
    selected_ports: BTreeSet<Id>, template: Option<Id>, name: String, prefix: String, count: u16, start: u16,
    kind: PortKind, contract: Option<Id>, connector_search: String, preset: usize, incoming: bool,
    connector_count: u16, connector_exposure: Exposure, family_name: String, family_kind: PortKind,
    channel_kind: ConversionKind, error: Option<String>, discard: bool, number_pins: bool,
    created_block: Option<Id>,
}
impl Default for Editor {
    fn default() -> Self {
        Self { draft: None, base: None, block: None, selected: None, tab: Tab::Contracts, selected_ports: BTreeSet::new(),
            template: None, name: "New interface".into(), prefix: "PIN".into(), count: 1, start: 0,
            kind: PortKind::SignalIn, contract: None, connector_search: String::new(), preset: interface_catalog::generic_preset(), incoming: true,
            connector_count: 4, connector_exposure: Exposure::Internal,
            family_name: String::new(), family_kind: PortKind::SignalOut,
            channel_kind: ConversionKind::Divider, error: None, discard: false, number_pins: false,
            created_block: None }
    }
}
impl Editor {
    pub fn is_open(&self) -> bool { self.draft.is_some() }
    pub fn open(&mut self, project: &Project, block: Option<Id>, tab: Tab) {
        self.base = Some(project.clone()); self.draft = Some(project.clone()); self.block = block;
        self.tab = tab; self.selected = None; self.selected_ports.clear(); self.error = None; self.discard = false;
        self.created_block = None;
        self.selected = match tab {
            Tab::Conversion => project.interfaces.channels.iter().find(|c|Some(c.block)==block).map(|c|c.id),
            Tab::Connectors => project.interfaces.connectors.iter().find(|c|Some(c.block)==block).map(|c|c.id),
            Tab::Protection => project.interfaces.protections.iter().find(|c|Some(c.block)==block).map(|c|c.id),
            _ => None,
        };
    }
    pub fn created_block(&self) -> Option<Id> { self.created_block }

    pub fn edit_contract(&mut self, project:&Project, id:Id) { self.open(project,None,Tab::Contracts); self.selected=Some(id); }
    pub fn from_port(&mut self, project: &Project, port: &Port) -> Result<()> {
        self.open(project, Some(port.block), Tab::Contracts);
        let draft = self.draft.as_mut().unwrap();
        let mut contract = Contract::from_port(port);
        let base_name = contract.name.clone(); let mut index = 2;
        while draft.interfaces.contracts.iter().any(|c| c.name == contract.name) { contract.name = format!("{base_name} {index}"); index += 1; }
        let id = draft.put_contract(contract)?;
        if let Some(existing) = draft.ports.iter_mut().find(|p| p.id == port.id) { existing.contract = Some(id); }
        self.selected = Some(id);
        Ok(())
    }
    pub fn show(&mut self, ctx: &egui::Context, current: &Project) -> Result<Option<Project>> {
        let Some(mut project) = self.draft.take() else { return Ok(None) };
        let mut open = true; let mut save = false;
        egui::Window::new("Interfaces and block tools").open(&mut open).collapsible(false)
            .default_size([950.0, 680.0]).min_width(660.0).min_height(400.0).show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for tab in Tab::ALL {
                        if ui.selectable_value(&mut self.tab, tab, tab.label()).changed() { self.selected = None; }
                    }
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Save changes").clicked() { save = true; }
                    if ui.button("Discard and close").clicked() { self.discard = true; }
                    ui.weak("Draft editor · Save changes applies to the project; then Ctrl+S saves the file.");
                });
                if let Some(error) = &self.error { ui.colored_label(egui::Color32::LIGHT_RED, error); }
                ui.separator();
                let scroll = if self.tab == Tab::Connectors { egui::ScrollArea::vertical() } else { egui::ScrollArea::both() };
                let result = scroll.id_salt(("interface_editor", self.tab.label()))
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .auto_shrink([false, false]).show(ui, |ui| {
                        match self.tab {
                            Tab::Contracts => self.contracts(ui, &mut project),
                            Tab::Ports => self.ports(ui, &mut project),
                            Tab::Conversion => self.conversion(ui, &mut project),
                            Tab::Connectors => self.connectors(ui, &mut project),
                            Tab::Protection => self.protection(ui, &mut project),
                        }
                    }).inner;
                if let Err(error) = result { self.error = Some(error); }
            });
        if !open && self.base.as_ref() != Some(&project) && !self.discard {
            self.error = Some("Unsaved editor changes. Use Save changes or Discard and close.".into());
            open = true;
        }
        if self.discard || !open { self.base = None; self.discard = false; return Ok(None); }
        if save {
            if self.base.as_ref() != Some(current) { self.error = Some("The project changed while this editor was open. Discard and reopen to edit the latest project.".into()); }
            else if let Err(error) = { project.sync_protection_rails(); project.validate() } { self.error = Some(error); }
            else { self.base = None; return Ok(Some(project)); }
        }
        self.draft = Some(project);
        Ok(None)
    }

    fn block_picker(&mut self, ui: &mut egui::Ui, project: &Project) {
        let choices: Vec<_> = project.blocks.iter().map(|b| (Some(b.id), b.name.clone())).collect();
        if self.block.is_none() { self.block = project.blocks.first().map(|b| b.id); }
        let old = self.block;
        choose(ui, "Owning block", "owner_block", &mut self.block, &choices);
        if old != self.block { self.selected = None; self.selected_ports.clear(); }
    }
    fn contracts(&mut self, ui: &mut egui::Ui, project: &mut Project) -> Result<()> {
        ui.heading("Shared signal contracts");
        ui.label("Create from any block or either end of an interface. Ports keep their actual hardware specifications separately.");
        let choices: Vec<_> = project.interfaces.contracts.iter().map(|c| (Some(c.id), c.name.clone())).collect();
        choose(ui, "Contract", "edit_contract", &mut self.selected, &choices);
        ui.horizontal_wrapped(|ui| { ui.label("New contract name"); ui.text_edit_singleline(&mut self.name); });
        if ui.button("Create contract").clicked() {
            let mut port = Port::new(0, PortKind::SignalOut); port.signal = self.name.clone();
            self.selected = Some(project.put_contract(Contract::from_port(&port))?);
        }
        let Some(id) = self.selected else { return Ok(()) };
        let used: Vec<_> = project.ports.iter().filter(|p| p.contract == Some(id)).map(|p| project.port_label(p)).collect();
        ui.label(format!("Linked to {} port(s). Editing this definition affects every listed port.", used.len()));
        for label in used { ui.weak(label); }
        let mut duplicate = None; let mut remove = false;
        if let Some(contract) = project.interfaces.contracts.iter_mut().find(|c| c.id == id) {
            field(ui, "Name", &mut contract.name); field(ui, "Description / measurement meaning", &mut contract.description);
            choose(ui, "Signal type", "contract_kind", &mut contract.limits.kind,
                &Kind::ALL.into_iter().map(|k| (k, k.label().into())).collect::<Vec<_>>());
            field(ui, "Reference / domain", &mut contract.limits.reference);
            ui.weak("Voltage/current/frequency/duty fields define the interface envelope. Bandwidth and data rate are minimum capability requirements. Resolution and acquisition details stay on the actual converter.");
            for &key in CONTRACT_PARAMETERS {
                let mut value = contract.limits.values.get(&key).copied();
                quantity(ui, key.label(false), &mut value, key.unit().1, key.unit().0, matches!(key,
                    Parameter::VoltageMin | Parameter::VoltageMax | Parameter::CurrentMin | Parameter::CurrentMax | Parameter::HighMin | Parameter::LowMax));
                if let Some(value) = value { contract.limits.values.insert(key, value); } else { contract.limits.values.remove(&key); }
            }
            ui.horizontal(|ui| {
                if ui.button("Duplicate as independent contract").clicked() { duplicate = Some(contract.clone()); }
                if ui.button("Delete unused contract").clicked() { remove = true; }
            });
        }
        if let Some(mut copy) = duplicate {
            copy.id = 0; let base = format!("{} copy", copy.name); copy.name = base.clone(); let mut index = 2;
            while project.interfaces.contracts.iter().any(|c| c.name == copy.name) { copy.name = format!("{base} {index}"); index += 1; }
            self.selected = Some(project.put_contract(copy)?);
        }
        if remove { project.remove_contract(id)?; self.selected = None; }
        Ok(())
    }

    fn ports(&mut self, ui: &mut egui::Ui, project: &mut Project) -> Result<()> {
        self.block_picker(ui, project);
        let Some(block) = self.block else { ui.label("Add a block first."); return Ok(()) };
        ui.heading(format!("{} ports", project.block_ports(block).count()));
        let mut templates = vec![(None, "Fresh port".into())];
        templates.extend(project.interfaces.templates.iter().map(|t| (Some(t.id), t.name.clone())));
        choose(ui, "Template", "port_template", &mut self.template, &templates);
        choose(ui, "New port type", "bulk_kind", &mut self.kind,
            &PortKind::ALL.into_iter().map(|k| (k, k.label().into())).collect::<Vec<_>>());
        let mut contracts = vec![(None, "No shared contract".into())];
        contracts.extend(project.interfaces.contracts.iter().map(|c| (Some(c.id), c.name.clone())));
        choose(ui, "Contract for fresh signal ports", "bulk_contract", &mut self.contract, &contracts);
        ui.horizontal_wrapped(|ui| {
            ui.label("Prefix"); ui.text_edit_singleline(&mut self.prefix);
            ui.label("First number"); ui.add(egui::DragValue::new(&mut self.start).range(0..=65535));
            ui.label("Count"); ui.add(egui::DragValue::new(&mut self.count).range(1..=1024));
        });
        ui.checkbox(&mut self.number_pins,"Also number physical pins from the first number");
        if ui.button("Create numbered ports").clicked() {
            let mut prototype = self.template.and_then(|id| project.interfaces.templates.iter().find(|t| t.id == id)).map(|t| t.port.clone())
                .unwrap_or_else(|| Port::new(block, self.kind));
            if self.template.is_none() && !prototype.kind.power() { prototype.contract = self.contract; }
            self.selected_ports = project.bulk_ports(block, &prototype, &self.prefix, self.start, self.count)?.into_iter().collect();
            if self.number_pins {
                for (index,id) in self.selected_ports.iter().enumerate() {
                    project.ports.iter_mut().find(|p|p.id==*id).unwrap().pin=(u32::from(self.start)+index as u32).to_string();
                }
            }
        }
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui.button("Select all").clicked() { self.selected_ports = project.block_ports(block).map(|p| p.id).collect(); }
            if ui.button("Clear selection").clicked() { self.selected_ports.clear(); }
        });
        let mut duplicate = false; let mut delete = false; let mut save_template = false; let mut apply_template = false; let mut create_contract = None;
        ui.horizontal_wrapped(|ui| {
            duplicate = ui.button("Duplicate selected").clicked(); delete = ui.button("Delete selected").clicked();
            apply_template = ui.button("Apply chosen template to selected").clicked();
        });
        if ui.button("Assign chosen contract to selected signal ports").clicked() {
            for port in project.ports.iter_mut().filter(|p|self.selected_ports.contains(&p.id)&&!p.kind.power()) { port.contract=self.contract; }
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Template name"); ui.text_edit_singleline(&mut self.name);
            save_template = ui.button("Save first selected as template").clicked();
        });
        ui.weak("Template application preserves port IDs, names, pin numbers and layout. Hardware fields are copied. Save a configured port once, then reuse it for many pins.");
        if duplicate { for id in self.selected_ports.clone() { project.duplicate_port(id)?; } }
        if delete { for id in self.selected_ports.clone() { project.remove_port(id)?; } self.selected_ports.clear(); }
        if save_template { if let Some(&id) = self.selected_ports.first() { project.save_port_template(id, self.name.clone())?; } }
        if apply_template {
            let template = self.template.and_then(|id| project.interfaces.templates.iter().find(|t| t.id == id)).ok_or("Choose a saved template first.")?.port.clone();
            for id in self.selected_ports.clone() {
                let old = project.port(id).ok_or("Selected port disappeared.")?.clone();
                let mut copy = template.clone(); copy.id = id; copy.block = old.block; copy.name = old.name; copy.pin = old.pin; copy.layout = old.layout;
                project.set_port(copy)?;
            }
        }
        let mut conflict = None;
        egui::Grid::new("port_table").striped(true).num_columns(6).show(ui, |ui| {
            for title in ["Select", "Name / physical pin", "Type", "Contract", "Placement", "Create contract"] { ui.strong(title); } ui.end_row();
            for port in project.ports.iter_mut().filter(|p| p.block == block) {
                ui.push_id(port.id, |ui| {
                    let mut checked = self.selected_ports.contains(&port.id);
                    if ui.checkbox(&mut checked, "").changed() { if checked { self.selected_ports.insert(port.id); } else { self.selected_ports.remove(&port.id); } }
                    ui.vertical(|ui| { ui.add(egui::TextEdit::singleline(&mut port.name).desired_width(130.0)); ui.add(egui::TextEdit::singleline(&mut port.pin).hint_text("Physical pin").desired_width(100.0)); });
                    let old = port.kind;
                    choose(ui, "", "port_kind", &mut port.kind, &PortKind::ALL.into_iter().map(|k| (k, k.label().into())).collect::<Vec<_>>());
                    if old.power() != port.kind.power() {
                        let mut replacement = Port::new(block, port.kind); replacement.id = port.id; replacement.name = port.name.clone(); replacement.pin = port.pin.clone(); replacement.layout = port.layout;
                        *port = replacement;
                    }
                    if port.kind.power() { ui.weak("Power port"); } else { choose(ui, "", "port_contract", &mut port.contract, &contracts); }
                    ui.vertical(|ui| {
                        let old_layout=port.layout;
                        let mut manual = port.layout.is_some();
                        if ui.checkbox(&mut manual, "Custom").changed() { port.layout = manual.then_some(PortPosition { edge: Edge::Left, slot: 0 }); }
                        if let Some(position) = &mut port.layout {
                            choose(ui, "", "port_edge", &mut position.edge, &Edge::ALL.into_iter().map(|e| (e,e.label().into())).collect::<Vec<_>>());
                            if ui.add(egui::DragValue::new(&mut position.slot).range(0..=4095)).changed() { conflict = Some((port.id, *position)); }
                        }
                        if old_layout!=port.layout { if let Some(position)=port.layout { conflict=Some((port.id,position)); } }
                    });
                    if ui.add_enabled(!port.kind.power(), egui::Button::new("From this port")).clicked() { create_contract = Some(port.clone()); }
                    ui.end_row();
                });
            }
        });
        if let Some((id, position)) = conflict { project.position_port(id, position)?; }
        if let Some(port) = create_contract {
            let id = project.put_contract(Contract::from_port(&port))?;
            project.ports.iter_mut().find(|p| p.id == port.id).unwrap().contract = Some(id);
            self.selected = Some(id); self.tab = Tab::Contracts;
        }
        Ok(())
    }

    fn conversion(&mut self, ui: &mut egui::Ui, project: &mut Project) -> Result<()> {
        self.block_picker(ui, project);
        choose(ui, "Implementation", "new_conversion", &mut self.channel_kind,
            &ConversionKind::ALL.into_iter().map(|k| (k,k.label().into())).collect::<Vec<_>>());
        ui.horizontal_wrapped(|ui| { ui.label("Name"); ui.text_edit_singleline(&mut self.name); });
        if ui.button("Create conversion block").clicked() {
            self.block = Some(project.add_block(Role::Custom, self.name.clone(), false)?);
        }
        if ui.button("Add channel to selected block").clicked() {
            let block = self.block.ok_or("Choose or create an owning block.")?;
            let mut input = Port::new(block, PortKind::SignalIn); input.name = "Signal in".into();
            let mut output = Port::new(block, PortKind::SignalOut); output.name = "Signal out".into();
            let (a,b) = self.channel_kind.types().unwrap_or((Kind::Digital, Kind::Digital));
            input.signal_spec = Some(Spec { kind: a, ..Spec::default() }); output.signal_spec = Some(Spec { kind: b, ..Spec::default() });
            let input = project.set_port(input)?; let output = project.set_port(output)?;
            let mut channel = Channel::new(block, input, output, self.channel_kind); channel.id = project.allocate()?;
            self.selected = Some(channel.id); project.interfaces.channels.push(channel);
        }
        let channels = project.interfaces.channels.iter().filter(|c| Some(c.block) == self.block).map(|c| (Some(c.id), c.name.clone())).collect::<Vec<_>>();
        choose(ui, "Channel", "edit_channel", &mut self.selected, &channels);
        let Some(index) = project.interfaces.channels.iter().position(|c| Some(c.id) == self.selected) else { return Ok(()) };
        let mut channel = project.interfaces.channels[index].clone();
        field(ui, "Channel name", &mut channel.name);
        choose(ui, "Conversion type", "channel_type", &mut channel.kind, &ConversionKind::ALL.into_iter().map(|k| (k,k.label().into())).collect::<Vec<_>>());
        port_choice(ui, project, "Input side", "channel_input", &mut channel.input, Some(channel.block), Some(PortKind::SignalIn));
        port_choice(ui, project, "Output side", "channel_output", &mut channel.output, Some(channel.block), Some(PortKind::SignalOut));
        ui.checkbox(&mut channel.active, "Active circuit · requires supply input(s)");
        if channel.active {
            for supply in project.block_ports(channel.block).filter(|p| p.kind == PortKind::Load) {
                let mut checked = channel.supplies.contains(&supply.id);
                if ui.checkbox(&mut checked, &supply.name).changed() { channel.supplies.retain(|id| *id != supply.id); if checked { channel.supplies.push(supply.id); } }
            }
            if ui.button("Add supply input to this block").clicked() {
                let port = Port::new(channel.block, PortKind::Load); channel.supplies.push(project.set_port(port)?);
            }
        } else { channel.supplies.clear(); }
        if channel.kind == ConversionKind::Divider {
            unsigned(ui, "Top resistance", &mut channel.top_ohm, 0, "Ω");
            unsigned(ui, "Bottom resistance", &mut channel.bottom_ohm, 0, "Ω");
            let mut tolerance = channel.resistor_tolerance_ppm.map(u64::from);
            unsigned(ui, "Resistor tolerance", &mut tolerance, 4, "%"); channel.resistor_tolerance_ppm = tolerance.map(|v| v.min(999_999) as u32);
        }
        quantity(ui, "Gain", &mut channel.gain_ppm, 6, "V/V", true);
        quantity(ui, "Intentional offset", &mut channel.offset_uv, 6, "V", true);
        unsigned(ui, "Offset error bound", &mut channel.offset_error_uv, 6, "V");
        unsigned(ui, "Bandwidth", &mut channel.bandwidth_hz, 0, "Hz");
        let mut bits = channel.resolution_bits.map(u64::from); unsigned(ui, "Resolution", &mut bits, 0, "bits"); channel.resolution_bits = bits.map(|v| v.min(65535) as u16);
        unsigned(ui, "Sampling / update rate", &mut channel.sample_rate, 0, "samples/s");
        field(ui, "Reference / bias source", &mut channel.reference);
        field(ui, "Loading, timing, headroom and other assumptions", &mut channel.assumptions);
        if ui.button("Calculate and set output voltage envelope").clicked() {
            let provider = project.bindings.iter().find(|b| b.target == channel.input).and_then(|b| project.port(b.source));
            let range = provider.and_then(interface_review::voltage_range).ok_or("Connect an input source with a declared voltage envelope first.")?;
            let (min, max) = channel.envelope(range).ok_or("This calculation needs a divider or linear stage with all transfer parameters declared.")?;
            let output = project.ports.iter_mut().find(|p| p.id == channel.output).unwrap();
            let spec = output.signal_spec.get_or_insert_with(Spec::default);
            spec.kind = Kind::AnalogVoltage; spec.values.insert(Parameter::VoltageMin,min); spec.values.insert(Parameter::VoltageMax,max);
        }
        ui.weak("The linear/divider calculation is a static envelope. Dynamic conversion and protocol behavior remain engineering review items.");
        let delete = ui.button("Remove channel definition (keep ports)").clicked();
        project.interfaces.channels[index] = channel;
        if delete { project.interfaces.channels.remove(index); self.selected = None; }
        Ok(())
    }

    fn connectors(&mut self, ui: &mut egui::Ui, project: &mut Project) -> Result<()> {
        {
            let mut choices = vec![(None, "New connector…".into())];
            choices.extend(project.interfaces.connectors.iter().map(|c| {
                (Some(c.id), project.block(c.block).map_or(&c.family, |b| &b.name).clone())
            }));
            choose(ui, "Connector", "connector_instance", &mut self.selected, &choices);
        }
        let index = project.interfaces.connectors.iter().position(|c| Some(c.id) == self.selected);
        if index.is_none() {
            ui.heading("Add connector");
            field(ui, "Name", &mut self.name);
            let exposures = Exposure::ALL.map(|e| (e, e.label().into()));
            choose(ui, "Exposure", "new_exposure", &mut self.connector_exposure, &exposures);
            let presets = interface_catalog::PRESETS;
            ui.label(format!("Preset · {}", presets[self.preset].label()));
            ui.collapsing("Browse connector presets", |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.connector_search).hint_text("Search family or category"));
                let query = self.connector_search.trim().to_lowercase();
                let choices: Vec<_> = presets.iter().enumerate().filter_map(|(index, preset)| {
                    let label = format!("{} · {}", preset.category, preset.label());
                    label.to_lowercase().contains(&query).then_some((index, label))
                }).collect();
                choose(ui, "Family / variant", "connector_preset", &mut self.preset, &choices);
            });
            if presets[self.preset].count == 0 {
                ui.horizontal(|ui| {
                    ui.label("Physical contacts");
                    ui.add(egui::DragValue::new(&mut self.connector_count).range(1..=1024));
                });
            }
            ui.checkbox(&mut self.incoming, "Preset direction: into board");
            ui.weak("Direction sets the diagram orientation. Shared control buses are planning interfaces.");
            let named = !self.name.trim().is_empty();
            let mut created = None;
            if ui.add_enabled(named, egui::Button::new("Create connector")).clicked() {
                created = Some(interface_catalog::create_families(
                    project, self.preset, self.connector_count, &self.name, self.connector_exposure, self.incoming,
                )?);
            }
            let mut template = None;
            if !project.interfaces.connector_templates.is_empty() {
                ui.collapsing("Use a saved physical pinout", |ui| {
                    for (i, saved) in project.interfaces.connector_templates.iter().enumerate() {
                        if ui.add_enabled(named, egui::Button::new(format!("{} · {}", saved.family, saved.variant))).clicked() {
                            template = Some(i);
                        }
                    }
                });
            }
            if let Some(i) = template {
                let template = project.interfaces.connector_templates[i].clone();
                created = Some(interface_catalog::instantiate_template(
                    project, template, &self.name, self.connector_exposure, self.incoming,
                )?);
            }
            if let Some(block) = created {
                self.block = Some(block);
                self.created_block = Some(block);
                self.selected = project.interfaces.connectors.last().map(|c| c.id);

            }
            return Ok(());
        }

        let index = index.unwrap();
        let mut connector = project.interfaces.connectors[index].clone();
        let block = connector.block;
        if let Some(b) = project.blocks.iter_mut().find(|b| b.id == block) {
            field(ui, "Connector name", &mut b.name);
        }
        let exposures = Exposure::ALL.map(|e| (e, e.label().into()));
        let old_exposure = connector.exposure;
        choose(ui, "Exposure", "connector_exposure", &mut connector.exposure, &exposures);
        if old_exposure != connector.exposure && connector.exposure != Exposure::Internal
            && !connector.required.contains(&Fault::Esd)
        {
            connector.required.push(Fault::Esd);
        }

        ui.separator();
        ui.heading("Signal families");
        ui.weak("These ports appear on the block. Choose their edge and slot here, or use Edit ports on the diagram.");
        let family_choices = PortKind::ALL.map(|kind| (kind, connector_direction(kind)));
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.family_name).hint_text("Family name").desired_width(145.0));
            egui::ComboBox::from_id_salt("family_direction").selected_text(connector_direction(self.family_kind)).show_ui(ui, |ui| {
                for (kind, label) in &family_choices { ui.selectable_value(&mut self.family_kind, *kind, *label); }
            });
        });
        if ui.add_enabled(!self.family_name.trim().is_empty(), egui::Button::new("Add family")).clicked() {
            interface_catalog::add_family(project, block, &self.family_name, self.family_kind)?;
            self.family_name.clear();
        }

        let ports: Vec<_> = project.block_ports(block).cloned().collect();
        let mut contracts = vec![(None, "No shared contract".into())];
        contracts.extend(project.interfaces.contracts.iter().map(|c| (Some(c.id), c.name.clone())));
        for mut port in ports.iter().filter(|p| p.pin.trim().is_empty()).cloned() {
            let layout = ui.push_id(port.id, |ui| {
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut port.name).desired_width(145.0));
                    ui.label(connector_direction(port.kind));
                });
                if !port.kind.power() {
                    choose(ui, "Signal contract", "family_contract", &mut port.contract, &contracts);
                }
                connector_placement(ui, &port)
            }).inner;
            let id = project.set_port(port)?;
            apply_connector_placement(project, id, layout)?;
        }
        if ports.iter().any(|port| !port.pin.trim().is_empty()) {
            let response = ui.collapsing("Existing individual ports", |ui| -> Result<()> {
                ui.label("These ports retain their existing connections.");
                for port in ports.iter().filter(|port| !port.pin.trim().is_empty()) {
                    let layout = ui.push_id(port.id, |ui| {
                        ui.label(format!("{} · physical pin {}", port.name, port.pin));
                        connector_placement(ui, port)
                    }).inner;
                    apply_connector_placement(project, port.id, layout)?;
                }
                Ok(())
            });
            if let Some(result) = response.body_returned { result?; }
        }

        let mut mapped_ports = vec![(None, "Unmapped".into())];
        mapped_ports.extend(ports.iter().map(|p| (Some(p.id), p.name.clone())));
        // Existing connectors can map to ports owned by other blocks.
        mapped_ports.extend(project.ports.iter().filter(|p| p.block != block)
            .map(|p| (Some(p.id), project.port_label(p))));
        let roles = PinRole::ALL.map(|role| (role, role.label().into()));
        ui.push_id(connector.id, |ui| {
            ui.collapsing(format!("Physical pinout · {} contacts", connector.pins.len()), |ui| {
                ui.weak("Contact assignments document the physical connector. Mapping contacts to one family does not join their conductors.");
                for (row, pin) in connector.pins.iter_mut().enumerate() {
                    ui.push_id(row, |ui| {
                        egui::CollapsingHeader::new(format!("{} · {}", pin.number, pin.name))
                            .id_salt("contact").show(ui, |ui| {
                                field(ui, "Contact number", &mut pin.number);
                                field(ui, "Physical function", &mut pin.name);
                                choose(ui, "Role", "contact_role", &mut pin.role, &roles);
                                choose(ui, "Family / existing port", "contact_map", &mut pin.port, &mapped_ports);
                                ui.collapsing("Protection exception", |ui| {
                                    field(ui, "Reason / protection elsewhere", &mut pin.exception);
                                    faults(ui, "Excepted faults", &mut pin.waived);
                                });
                            });
                    });
                }
            });
            ui.collapsing("Part and electrical ratings", |ui| {
                field(ui, "Family", &mut connector.family);
                field(ui, "Variant / keying", &mut connector.variant);
                field(ui, "Manufacturer part number", &mut connector.part_number);
                field(ui, "Datasheet / pinout source and revision", &mut connector.source);
                unsigned(ui, "Working voltage rating", &mut connector.max_uv, 6, "V");
                unsigned(ui, "Per-contact continuous current", &mut connector.contact_current_ua, 6, "A");
                field(ui, "Notes / rating conditions", &mut connector.notes);
            });
            ui.collapsing("Cable and mating", |ui| {
                field(ui, "Cable / gauge / length / derating", &mut connector.cable);
                field(ui, "Mating connector / orientation", &mut connector.mating);
            });
            ui.collapsing("Protection and shielding", |ui| {
                field(ui, "Shield, chassis and return strategy", &mut connector.shielding);
                ui.checkbox(&mut connector.hot_plug, "Hot-plug expected");
                faults(ui, "Required fault coverage", &mut connector.required);
            });
        });
        if ui.button("Save physical pinout as template").clicked() {
            let mut template = connector.clone();
            template.id = 0;
            template.block = 0;
            for pin in &mut template.pins { pin.port = None; }
            project.interfaces.connector_templates.push(template);
        }
        project.interfaces.connectors[index] = connector;
        Ok(())
    }

    fn protection(&mut self, ui: &mut egui::Ui, project: &mut Project) -> Result<()> {
        self.block_picker(ui, project);
        ui.horizontal_wrapped(|ui| { ui.label("Name"); ui.text_edit_singleline(&mut self.name); });
        if ui.button("Create protection block").clicked() { self.block = Some(project.add_block(Role::Custom,self.name.clone(),false)?); }
        ui.label("Shunt protection attaches to a selected net. Series protection uses real input/output ports on its block.");
        if ui.button("Add shunt protection definition").clicked() {
            let block = self.block.ok_or("Choose or create a protection block.")?;
            if project.ports.is_empty() { return Err("Create a signal or power port to attach protection to.".into()); }
            let entry = 0;
            let id = project.allocate()?;
            project.interfaces.protections.push(Protection { id,block,name:self.name.clone(),entry,exit:None,shunt:true,manual_rail_voltage:false,
                covers:vec![Fault::Esd],return_reference:String::new(),event:String::new(),evidence:String::new(),standoff_uv:None,
                clamp_uv:None,clamp_current_ua:None,current_limit_ua:None,response_ns:None,capacitance_pf:None,leakage_na:None,series_milliohm:None,notes:String::new() }); self.selected = Some(id);
        }
        for (label,input_kind,output_kind) in [
            ("Add series power protection with ports",PortKind::Load,PortKind::Rail),
            ("Add series signal protection with ports",PortKind::SignalIn,PortKind::SignalOut),
        ] {
            if ui.button(label).clicked() {
                let block = self.block.ok_or("Choose or create a protection block.")?;
                let input = project.set_port(Port::new(block,input_kind))?;
                let output = project.set_port(Port::new(block,output_kind))?;
                let mut protection=Protection::new(block,input,&self.name);
                protection.id=project.allocate()?;protection.exit=Some(output);protection.shunt=false;
                protection.covers=if input_kind.power() { vec![Fault::OverCurrent] } else { Vec::new() };
                self.selected=Some(protection.id);project.interfaces.protections.push(protection);
            }
        }
        let choices = project.interfaces.protections.iter().filter(|p| Some(p.block) == self.block).map(|p|(Some(p.id),p.name.clone())).collect::<Vec<_>>();
        choose(ui,"Protection definition","protection_instance",&mut self.selected,&choices);
        let Some(index) = project.interfaces.protections.iter().position(|p|Some(p.id)==self.selected) else { return Ok(()) };
        let mut p = project.interfaces.protections[index].clone();
        field(ui,"Name",&mut p.name); ui.checkbox(&mut p.shunt,"Shunt attachment (otherwise series)");
        port_choice(ui,project,"Attached net / entry port","protection_entry",&mut p.entry,None,None);
        if p.shunt { p.exit = None; } else {
            let mut choices = vec![(None,"Exit not assigned".into())];
            choices.extend(project.block_ports(p.block).filter(|port|port.kind.source()).map(|port|(Some(port.id),port.name.clone())));
            choose(ui,"Series exit","protection_exit",&mut p.exit,&choices);
            if project.port(p.entry).is_some_and(|port| port.kind.power()) {
                ui.checkbox(&mut p.manual_rail_voltage, "Set protected rail voltage manually");
                if !p.manual_rail_voltage { ui.weak("The protected rail follows its supply input."); }
            }
        }
        faults(ui,"Faults covered by this definition",&mut p.covers);
        field(ui,"Discharge / return reference",&mut p.return_reference); field(ui,"Event, waveform, level and polarity",&mut p.event);
        protection_recommendations(ui, project, &mut p);
        unsigned(ui,"Working standoff magnitude",&mut p.standoff_uv,6,"V"); unsigned(ui,"Clamp voltage magnitude",&mut p.clamp_uv,6,"V");
        unsigned(ui,"Stated clamp test current",&mut p.clamp_current_ua,6,"A"); unsigned(ui,"Current limit / trip threshold",&mut p.current_limit_ua,6,"A");
        unsigned(ui,"Response / trip time",&mut p.response_ns,0,"ns"); unsigned(ui,"Added capacitance",&mut p.capacitance_pf,0,"pF");
        unsigned(ui,"Leakage",&mut p.leakage_na,0,"nA"); unsigned(ui,"Series resistance",&mut p.series_milliohm,3,"Ω");
        field(ui,"Datasheet, test or review evidence",&mut p.evidence); field(ui,"Fault response / conditions / notes",&mut p.notes);
        ui.weak("Declared protection stays a manual review, not an immunity guarantee. A TVS elsewhere across a conversion stage does not cover this connector net.");
        let delete = ui.button("Remove protection definition").clicked();
        project.interfaces.protections[index] = p;
        if delete { project.interfaces.protections.remove(index); self.selected = None; }
        Ok(())
    }
}

fn protection_recommendations(ui: &mut egui::Ui, project: &Project, stage: &mut Protection) {
    let advice = interface_catalog::protection_advice(project, stage);
    let volts = |uv: u64| crate::rail::label_voltage(uv);
    let amps = |ua: u64| crate::units::Scale::for_value(ua.into()).display(ua.into(), "A");
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Recommended planning values");
        if let Some(value) = advice.standoff_floor_uv {
            ui.label(format!("Working standoff floor · {} from the declared operating envelope", volts(value)));
        } else if advice.ac {
            ui.weak("For AC protection, specify the device working rating and its RMS or peak basis.");
        } else if advice.power {
            ui.weak("Working standoff floor · declare the maximum operating voltage on the upstream supply rail.");
        } else {
            ui.weak("Working standoff floor · declare the signal voltage range on the driving port.");
        }
        if let Some(value) = advice.connector_voltage_uv {
            ui.label(format!("Connector working rating · {}", volts(value)));
        }
        if advice.power && !stage.shunt && stage.covers.contains(&Fault::OverCurrent) {
            if let Some(value) = advice.source_current_ua { ui.label(format!("Supply current rating · {}", amps(value))); }
            if let Some(value) = advice.contact_current_ua { ui.label(format!("Connector rating per contact · {}", amps(value))); }
            if let Some(value) = advice.trip_candidate_ua {
                ui.label(format!("Current-limit candidate · {} from the lower of the supply and per-contact ratings", amps(value)));
            } else if advice.load_at_or_above_ceiling {
                ui.weak("Current limit · modeled load meets or exceeds the source or connector ceiling.");
            } else if !advice.ac {
                ui.weak("Current-limit candidate · declare both the upstream supply current and the connector's per-contact rating.");
            }
        }
        if let Some(value) = advice.clamp_ceiling_uv {
            ui.label(format!("Clamp ceiling from the receiving pin's absolute maximum · {}", volts(value)));
        } else if let Some(value) = advice.accepted_input_max_uv {
            ui.weak(format!("Highest accepted downstream operating voltage · {}. Record a transient limit separately.", volts(value)));
        }
        ui.weak("Use part data and test conditions for actual clamp voltage/current, response time, capacitance, leakage, and resistance.");
        let can_fill = stage.standoff_uv.is_none() && advice.standoff_floor_uv.is_some()
            || stage.current_limit_ua.is_none() && advice.trip_candidate_ua.is_some();
        if ui.add_enabled(can_fill, egui::Button::new("Fill missing recommended values")).clicked() {
            advice.fill_missing(stage);
        }
    });
}

fn connector_direction(kind: PortKind) -> &'static str {
    match kind {
        PortKind::SignalOut => "Signal into board",
        PortKind::SignalIn => "Signal out of board",
        PortKind::Rail => "Power into board",
        PortKind::Load => "Power out of board",
    }
}

fn connector_placement(ui: &mut egui::Ui, port: &Port) -> Option<PortPosition> {
    let mut edge = port.layout.map(|position| position.edge);
    let mut slot = port.layout.map_or(0, |position| position.slot);
    ui.horizontal_wrapped(|ui| {
        ui.label("Canvas placement");
        let label = edge.map_or("Automatic", Edge::label);
        egui::ComboBox::from_id_salt("family_edge").selected_text(label).show_ui(ui, |ui| {
            ui.selectable_value(&mut edge, None, "Automatic");
            for choice in Edge::ALL { ui.selectable_value(&mut edge, Some(choice), choice.label()); }
        });
        if edge.is_some() {
            ui.label("Slot");
            ui.add(egui::DragValue::new(&mut slot).range(0..=4095));
        }
    });
    edge.map(|edge| PortPosition { edge, slot })
}

fn apply_connector_placement(project: &mut Project, id: Id, layout: Option<PortPosition>) -> Result<()> {
    let port = project.ports.iter_mut().find(|port| port.id == id).ok_or("Port no longer exists.")?;
    if port.layout != layout {
        if let Some(position) = layout { return project.position_port(id, position); }
        port.layout = None;
    }
    Ok(())
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) { ui.label(label); ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY)); }
fn choose<T: Clone + PartialEq>(ui: &mut egui::Ui, label: &str, id: &str, value: &mut T, choices: &[(T,String)]) {
    if !label.is_empty() { ui.label(label); }
    let text = choices.iter().find(|(candidate,_)| candidate == &*value).map_or("Choose…",|(_,label)|label.as_str());
    egui::ComboBox::from_id_salt(id).width(ui.available_width().clamp(140.0,360.0)).wrap_mode(egui::TextWrapMode::Wrap).selected_text(text).height(340.0).show_ui(ui,|ui| {
        if choices.len()>8 { ui.small(format!("{} options · Scroll for more",choices.len())); }
        for (candidate,label) in choices { ui.selectable_value(value,candidate.clone(),label.as_str()); }
    });
}
fn port_choice(ui:&mut egui::Ui,project:&Project,label:&str,id:&str,value:&mut Id,block:Option<Id>,kind:Option<PortKind>) {
    let choices = project.ports.iter().filter(|p| block.is_none_or(|b|p.block==b) && kind.is_none_or(|k|p.kind==k))
        .map(|p|(p.id,project.port_label(p))).collect::<Vec<_>>();
    choose(ui,label,id,value,&choices);
}
fn faults(ui:&mut egui::Ui,label:&str,values:&mut Vec<Fault>) {
    ui.strong(label); ui.horizontal_wrapped(|ui| {
        for fault in Fault::ALL { let mut checked=values.contains(&fault); if ui.checkbox(&mut checked,fault.label()).changed() { values.retain(|f|*f!=fault); if checked { values.push(fault); } } }
    });
}
fn quantity(ui:&mut egui::Ui,label:&str,value:&mut Option<i64>,decimals:u32,unit:&str,signed:bool) {
    ui.horizontal_wrapped(|ui| {
        let mut known=value.is_some(); if ui.checkbox(&mut known,label).changed() { *value=known.then_some(0); }
        if let Some(raw)=value {
            let scale=10_f64.powi(decimals as i32); let mut display=*raw as f64/scale;
            let low=if signed { -9_000_000_000.0 } else { 0.0 };
            if ui.add(egui::DragValue::new(&mut display).speed(0.1).range(low..=9_000_000_000.0).suffix(format!(" {unit}"))).changed() { *raw=(display*scale).round() as i64; }
        } else { ui.weak("Unknown"); }
    });
}
fn unsigned(ui:&mut egui::Ui,label:&str,value:&mut Option<u64>,decimals:u32,unit:&str) {
    let mut signed=value.map(|v|v.min(i64::MAX as u64) as i64); quantity(ui,label,&mut signed,decimals,unit,false); *value=signed.map(|v|v.max(0) as u64);
}

#[cfg(test)]
mod creation_tests {
    use super::*;

    #[test]
    fn conversion_setup_has_a_channel_and_required_ports_without_changing_original() -> Result<()> {
        let original = Project::new();
        let baseline = original.clone();
        for kind in ConversionKind::ALL {
            let mut draft = original.clone();
            let block = create_block(&mut draft, "Conversion stage", NewBlock::Conversion(kind))?;
            draft.validate()?;
            let channel = &draft.interfaces.channels[0];
            assert_eq!(draft.ports.len(), if channel.active { 3 } else { 2 });
            assert_eq!(draft.port(channel.input).unwrap().kind, PortKind::SignalIn);
            assert_eq!(draft.port(channel.output).unwrap().kind, PortKind::SignalOut);
            assert_eq!(block, channel.block);
            if channel.active {
                assert_eq!(channel.supplies.len(), 1);
                assert_eq!(draft.port(channel.supplies[0]).unwrap().kind, PortKind::Load);
            }
            assert_eq!(original, baseline);
        }
        Ok(())
    }

    #[test]
    fn protection_setup_requires_a_real_net_for_shunts_and_preserves_original() -> Result<()> {
        let mut original = Project::new();
        let source = original.add_block(Role::Input, "External interface".into(), false)?;
        let port = original.set_port(Port::new(source, PortKind::SignalOut))?;
        let baseline = original.clone();
        let mut invalid = original.clone();
        assert!(create_block(&mut invalid, "Clamp", NewBlock::Protection(ProtectionType::Shunt, None)).is_err());
        assert_eq!(invalid, baseline);
        for kind in ProtectionType::ALL {
            let mut draft = original.clone();
            let block = create_block(&mut draft, "Protection", NewBlock::Protection(kind, Some(port)))?;
            draft.validate()?;
            let protection = &draft.interfaces.protections[0];
            assert_eq!(protection.block, block);
            assert_eq!(protection.shunt, kind == ProtectionType::Shunt);
            if protection.shunt { assert_eq!(protection.entry, port); assert!(protection.exit.is_none()); }
            else {
                assert_eq!(draft.port(protection.entry).unwrap().block, protection.block);
                assert_eq!(draft.port(protection.exit.unwrap()).unwrap().block, protection.block);
            }
            assert_eq!(original, baseline);
        }
        Ok(())
    }
}
