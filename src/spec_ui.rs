use crate::{model::{Goal, Id, Project, Result}, review::{Finding, Status, Subject}, spec::{self, Data, Entry, Rule, Target}, units::Scale};
use eframe::egui;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Tab { #[default] Requirements, Pins, States, Transitions }

impl Tab {
    const ALL: [Self; 4] = [Self::Requirements, Self::Pins, Self::States, Self::Transitions];

    fn label(self) -> &'static str {
        match self {
            Self::Requirements => "Requirements", Self::Pins => "Connector pins",
            Self::States => "Control states", Self::Transitions => "Transitions",
        }
    }

    fn of(data: &Data) -> Self {
        match data {
            Data::Requirement { .. } => Self::Requirements, Data::Pin { .. } => Self::Pins,
            Data::State { .. } => Self::States, Data::Transition { .. } => Self::Transitions,
        }
    }

    fn new_entry(self) -> Entry {
        let data = match self {
            Self::Requirements => Data::Requirement {
                title: String::new(), source: String::new(), target: None, rule: Rule::Manual,
                method: String::new(), evidence: String::new(),
            },
            Self::Pins => Data::Pin { connector: None, number: String::new(), function: String::new(), port: None },
            Self::States => Data::State { name: String::new(), initial: false, outputs: String::new() },
            Self::Transitions => Data::Transition {
                from: None, to: None, trigger: String::new(), guard: String::new(), action: String::new(),
            },
        };
        Entry { id: 0, reference: None, data }
    }
}

enum Draft { Entry(Entry), Goal(Goal) }

#[derive(Default)]
pub struct Editor {
    tab: Tab,
    draft: Option<Draft>,
    values: [String; 2],
}

pub enum Action {
    Save(Entry),
    Delete(Id),
    SaveGoal(Goal),
    DeleteGoal(Id),
    OpenTarget(Target, Option<Id>),
}

impl Editor {
    pub fn open_entry(&mut self, entry: Entry) {
        self.tab = Tab::of(&entry.data);
        self.select(Some(entry));
    }

    pub fn new_requirement(&mut self, assigned_to: Target) {
        let mut entry = Tab::Requirements.new_entry();
        if let Data::Requirement { target, .. } = &mut entry.data { *target = Some(assigned_to); }
        self.open_entry(entry);
    }

    pub fn open_goal(&mut self, goal: Goal) {
        self.tab = Tab::Requirements;
        self.draft = Some(Draft::Goal(goal));
    }

    pub fn select(&mut self, entry: Option<Entry>) {
        self.values = match entry.as_ref().map(|entry| &entry.data) {
            Some(Data::Requirement { rule: Rule::InputVoltage { min_uv, max_uv }, .. }) =>
                [Scale::Base.number((*min_uv).into()), Scale::Base.number((*max_uv).into())],
            Some(Data::Requirement { rule: Rule::InputPower { voltage_uv, limit_uw, .. }, .. }) =>
                [Scale::Base.number((*voltage_uv).into()), Scale::Base.number((*limit_uw).into())],
            _ => [String::new(), String::new()],
        };
        self.draft = entry.map(Draft::Entry);
    }

    pub fn show(&mut self, ui: &mut egui::Ui, project: &Project, findings: &[Finding]) -> Result<Option<Action>> {
        ui.heading("Requirements");
        ui.label("Define what the system must do and how to verify it.");
        ui.horizontal_wrapped(|ui| {
            for tab in Tab::ALL {
                if ui.selectable_label(self.tab == tab, tab.label()).clicked() && self.tab != tab {
                    self.tab = tab;
                    self.select(None);
                }
            }
        });
        ui.separator();
        let mut action = Ok(None);
        ui.columns(2, |columns| {
            let list = &mut columns[0];
            let tab = self.tab;
            let add = match tab {
                Tab::Requirements => "New requirement", Tab::Pins => "New connector pin",
                Tab::States => "New control state", Tab::Transitions => "New transition",
            };
            list.horizontal_wrapped(|ui| {
                if ui.button(add).clicked() { self.select(Some(tab.new_entry())); }
                if tab == Tab::Requirements && ui.small_button("New system goal").clicked() {
                    self.open_goal(Goal::new(None));
                }
            });
            egui::ScrollArea::vertical().id_salt("spec_entries").show(list, |ui| {
                let mut empty = true;
                for entry in project.spec.iter().filter(|entry| Tab::of(&entry.data) == tab) {
                    empty = false;
                    let selected = matches!(&self.draft, Some(Draft::Entry(draft)) if draft.id == entry.id);
                    let label = match &entry.data {
                        Data::Pin { connector, .. } => format!("{} · {}",
                            spec::target_name(project, connector.map(Target::Block)), entry.label()),
                        _ => entry.label(),
                    };
                    let label = if tab == Tab::Requirements {
                        let state = findings.iter().find(|finding| finding.subject == Subject::Spec(entry.id))
                            .map(|finding| match finding.status {
                                Status::Pass => "Design pass", Status::Fail => "Design fail",
                                Status::Open => "Open", Status::Manual => "Manual review", Status::Warning => "Warning",
                            });
                        state.map_or(label.clone(), |state| format!("{label}\n{state}"))
                    } else { label };
                    if ui.selectable_label(selected, label).clicked() {
                        self.select(Some(entry.clone()));
                    }
                }
                if tab == Tab::Requirements {
                    for goal in &project.goals {
                        empty = false;
                        let selected = matches!(&self.draft, Some(Draft::Goal(draft)) if draft.id == goal.id);
                        if ui.selectable_label(selected, format!("{} · {} · {}", spec::requirement_reference(goal.reference), goal_scope(project, goal), goal.description)).clicked() {
                            self.open_goal(goal.clone());
                        }
                    }
                }
                if empty { ui.weak("No entries yet."); }
            });
            let Some(draft) = &mut self.draft else {
                columns[1].weak("Select an entry to see its details.");
                return;
            };
            egui::ScrollArea::vertical().id_salt("spec_entry_form").show(&mut columns[1], |ui| {
                match draft {
                    Draft::Entry(entry) => {
                        if entry.id != 0 { ui.heading(entry.label()); }
                        if let Some(saved) = project.spec_entry(entry.id) {
                            if let Data::Pin { port: Some(id), .. } = &saved.data {
                                if ui.button(format!("Open {}", spec::target_name(project, Some(Target::Port(*id))))).clicked() {
                                    action = Ok(Some(Action::OpenTarget(Target::Port(*id), None)));
                                }
                            }
                            if let Data::Requirement { target, rule, evidence, .. } = &saved.data {
                                ui.label(format!("Design check · {}", spec::check(project, *target, rule)));
                                ui.label(if evidence.trim().is_empty() {
                                    "Verification evidence · Pending"
                                } else {
                                    "Verification evidence · Reference recorded"
                                });
                                let mode = match rule { Rule::InputPower { mode, .. } => *mode, _ => None };
                                if let Some(target) = target {
                                    if ui.button(format!("Open {}", spec::target_name(project, Some(*target)))).clicked() {
                                        action = Ok(Some(Action::OpenTarget(*target, mode)));
                                    }
                                }
                                ui.separator();
                            }
                        }
                        let save = form(ui, project, entry, &mut self.values);
                        if ui.button(if tab == Tab::Requirements { "Save requirement" } else { "Save entry" }).clicked() {
                            action = save.map(|()| Some(Action::Save(entry.clone())));
                        }
                        if entry.id != 0 && ui.button("Delete entry").clicked() {
                            action = Ok(Some(Action::Delete(entry.id)));
                        }
                    }
                    Draft::Goal(goal) => {
                        ui.heading(goal_scope(project, goal));
                        ui.strong(spec::requirement_reference(goal.reference));
                        multiline(ui, "Requirement", &mut goal.description);
                        field(ui, "Source or assumption", &mut goal.note);
                        multiline(ui, "How to verify", &mut goal.method);
                        ui.label("Responsible blocks");
                        for block in &project.blocks {
                            ui.push_id(block.id, |ui| {
                                let mut checked = goal.responsible.contains(&block.id);
                                if ui.checkbox(&mut checked, &block.name).changed() {
                                    if checked { goal.responsible.push(block.id); }
                                    else { goal.responsible.retain(|&id| id != block.id); }
                                }
                            });
                        }
                        if ui.button("Save requirement").clicked() {
                            action = Ok(Some(Action::SaveGoal(goal.clone())));
                        }
                        if goal.id != 0 && ui.button("Delete requirement").clicked() {
                            action = Ok(Some(Action::DeleteGoal(goal.id)));
                        }
                    }
                }
            });
        });
        action
    }
}

fn goal_scope(project: &Project, goal: &Goal) -> String {
    goal.owner.and_then(|id| project.block(id))
        .map_or_else(|| "System goal".into(), |block| block.name.clone())
}

fn form(ui: &mut egui::Ui, project: &Project, entry: &mut Entry, values: &mut [String; 2]) -> Result<()> {
    match &mut entry.data {
        Data::Requirement { title, source, target, rule, method, evidence } => {
            multiline(ui, "What must the design do?", title);
            field(ui, "Source and section", source);
            let targets = project.blocks.iter().map(|b| (Target::Block(b.id), b.name.clone()))
                .chain(project.ports.iter().map(|p| (Target::Port(p.id), project.port_label(p))))
                .chain(project.spec.iter().filter_map(|entry| match &entry.data {
                    Data::Pin { .. } => Some((Target::Pin(entry.id), entry.label())),
                    Data::Transition { .. } => Some((Target::Transition(entry.id), entry.label())),
                    _ => None,
                }));
            pick(ui, "Assigned to", target, "Unassigned", targets);
            ui.label("Design check");
            let previous = match rule { Rule::Manual => 0, Rule::InputVoltage { .. } => 1, Rule::InputPower { .. } => 2 };
            let mut kind = previous;
            let labels = ["Review manually", "Input voltage coverage", "Maximum input power"];
            egui::ComboBox::from_id_salt("rule_kind").selected_text(labels[kind]).show_ui(ui, |ui| {
                for (i, label) in labels.into_iter().enumerate() { ui.selectable_value(&mut kind, i, label); }
            });
            if kind != previous {
                *values = [String::new(), String::new()];
                *rule = match kind {
                    1 => Rule::InputVoltage { min_uv: 0, max_uv: 0 },
                    2 => Rule::InputPower { voltage_uv: 0, limit_uw: 0, mode: Some(0) },
                    _ => Rule::Manual,
                };
            }
            match rule {
                Rule::Manual => {}
                Rule::InputVoltage { .. } => {
                    field(ui, "Required minimum (V)", &mut values[0]);
                    field(ui, "Required maximum (V)", &mut values[1]);
                }
                Rule::InputPower { mode, .. } => {
                    field(ui, "Test voltage (V)", &mut values[0]);
                    field(ui, "Strict upper limit (W)", &mut values[1]);
                    pick(ui, "Operating mode", mode, "Unspecified",
                        std::iter::once((0, project.condition.clone())).chain(project.modes.iter().map(|mode| (mode.id, mode.name.clone()))));
                }
            }
            multiline(ui, "How to verify", method);
            field(ui, "Evidence reference", evidence);
            match rule {
                Rule::Manual => {}
                Rule::InputVoltage { min_uv, max_uv } => {
                    *min_uv = number(&values[0], "Required minimum")?;
                    *max_uv = number(&values[1], "Required maximum")?;
                }
                Rule::InputPower { voltage_uv, limit_uw, .. } => {
                    *voltage_uv = number(&values[0], "Test voltage")?;
                    *limit_uw = number(&values[1], "Upper power limit")?;
                }
            }
        }
        Data::Pin { connector, number, function, port } => {
            pick(ui, "Connector block", connector, "Unassigned", project.blocks.iter().map(|b| (b.id, b.name.clone())));
            field(ui, "Physical pin designation", number);
            field(ui, "Function (blank means TBD)", function);
            pick(ui, "Logical port", port, "Unmapped", project.ports.iter().map(|p| (p.id, project.port_label(p))));
            ui.weak(spec::pin_status(*connector, function, *port));
        }
        Data::State { name, initial, outputs } => {
            field(ui, "State name", name);
            ui.checkbox(initial, "Initial state");
            multiline(ui, "Commanded outputs", outputs);
        }
        Data::Transition { from, to, trigger, guard, action } => {
            let states = project.spec.iter().filter_map(|entry| match &entry.data {
                Data::State { name, .. } => Some((entry.id, name.clone())),
                _ => None,
            });
            pick(ui, "From state", from, "TBD", states.clone());
            pick(ui, "To state", to, "TBD", states);
            field(ui, "Trigger", trigger);
            multiline(ui, "Guard (blank means TBD)", guard);
            multiline(ui, "Action (blank means TBD)", action);
            ui.weak(spec::transition_status(*from, *to, guard, action));
        }
    }
    Ok(())
}

fn pick<T: Copy + Eq>(ui: &mut egui::Ui, label: &str, selected: &mut Option<T>, empty: &str, options: impl Iterator<Item = (T, String)>) {
    let options: Vec<_> = options.collect();
    let text = options.iter().find(|(value, _)| Some(*value) == *selected).map_or(empty, |(_, name)| name);
    ui.label(label);
    egui::ComboBox::from_id_salt(label).selected_text(text).show_ui(ui, |ui| {
        ui.selectable_value(selected, None, empty);
        for (value, name) in &options { ui.selectable_value(selected, Some(*value), name); }
    });
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
}

fn multiline(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::multiline(value).desired_rows(2).desired_width(f32::INFINITY));
}

fn number(value: &str, label: &str) -> Result<u64> {
    Scale::Base.parse(value)?.ok_or_else(|| format!("Enter {label}."))
}
