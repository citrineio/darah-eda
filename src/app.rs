use crate::{
    diagram::{Diagram, DiagramAction, Selection},
    export::{self, Content as ExportContent, Format as ExportFormat, PowerReportFormat, ReviewFormat},
    model::{Block, BlockPosition, Goal, Id, Mode, Port, PortKind, Project, Result, Role},
    power::{self, Conversion, Network},
    power_ui,
    rail_ui,
    signal_ui,
    review::{self, Finding, Status, Subject},
    storage::{self, RecentProjects, SavedFile},
    spec::Target,
    spec_ui,
    units::Scale,
};
use eframe::egui::{self, Color32, RichText};
use crate::status_ui::{self, color as status_color};
use crate::{interface_catalog, interface_ui::{NewBlock, ProtectionType}, interfaces::Exposure};
use std::{collections::HashSet, mem, path::{Path, PathBuf}, time::{Duration, Instant}};

const HISTORY_LIMIT: usize = 64;
const ACCENT: Color32 = Color32::from_rgb(110, 204, 186);
const MUTED: Color32 = Color32::from_rgb(148, 169, 183);

struct MonitorGuide {
    rail: Id,
    alert: Option<Id>,
}

enum Draft {
    Project { title: String, purpose: String, condition: String },
    Block(Block),
    Port(Port, [String; 4], rail_ui::Draft, power_ui::Draft, signal_ui::Draft),
    Wire(Id),
    Boundary(Id),
}

impl Draft {
    fn port(port: Port, voltage: Scale, current: Scale, mode: Option<&Mode>, conversion: Option<&Conversion>) -> Self {
        let override_value = mode.and_then(|mode| mode.current_override(port.id)).flatten();
        let text = [(port.min_uv, voltage), (port.max_uv, voltage), (port.current_ua, current), (override_value, current)]
            .map(|(value, scale)| value.map_or_else(String::new, |micro| scale.number(micro.into())));
        let details = rail_ui::Draft::new(&port);
        let signal = signal_ui::Draft::new(&port);
        Self::Port(port, text, details, power_ui::Draft::new(conversion), signal)
    }
}

#[derive(Clone)]
enum DocumentAction {
    New, Open, OpenRecent(PathBuf), Monitor, ModesDemo, PowerExample(bool), SpecificationExample, Close,
    #[cfg(target_os = "linux")]
    ApplyUpdate,
}

enum PortExit {
    Select(Selection),
    ChangeWorkspace(Workspace),
    Document(DocumentAction),
    HideInspector,
    History(bool),
    Mode(Option<Id>),
    Save(bool),
    Export(ExportContent, ExportFormat),
    ExportPower(PowerReportFormat),
    Diagram(DiagramAction),
    Interface(Option<Id>, crate::interface_ui::Tab),
    FindPower(Id, Option<Id>),
    Protection(Id),
}

#[derive(Default)]
struct Palette { query: String, focus: bool }

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Workspace { #[default] Diagram, Requirements, Checks }

#[derive(Clone, Copy)]
enum PaletteAction { AddBlock, AddPreset(BlockType), Connect, OpenIssues, Power, Modes, Inspect(Selection) }

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockType { Functional(Role), Connector(usize), Protection, SignalConversion, PowerConverter }
impl BlockType {
    fn label(self) -> &'static str { match self { Self::Functional(role) => role.label(),
        Self::Connector(index) => interface_catalog::PRESETS[index].family, Self::Protection => "Protection", Self::SignalConversion => "Signal conversion", Self::PowerConverter => "Power converter" } }
}

struct AddBlock {
    kind: Option<BlockType>, name: String, template: bool, query: String, focus: bool,
    contacts: u16, exposure: Exposure, incoming: bool,
    conversion: crate::interfaces::ConversionKind,
    power_topology: Option<power::Kind>,
    protection: crate::interface_ui::ProtectionType, attached: Option<Id>,
    preview_ports: Vec<Port>,
}

impl AddBlock {
    fn create(&self, project: &mut Project) -> Result<Id> {
        let name = self.name.trim();
        match self.kind.ok_or("Choose a block type.")? {
            BlockType::Functional(role) => project.add_block(role, name.into(), self.template),
            BlockType::Connector(index) => interface_catalog::create_families(
                project, index, self.contacts, name, self.exposure, self.incoming),
            BlockType::SignalConversion => crate::interface_ui::create_block(
                project, name, NewBlock::Conversion(self.conversion)),
            BlockType::PowerConverter => project.add_power_converter(name.into(),
                self.power_topology.ok_or("Select a power topology.")?),
            BlockType::Protection => crate::interface_ui::create_block(
                project, name, NewBlock::Protection(self.protection, self.attached)),
        }
    }
}

struct Picker {
    from: Id,
    target: Option<Id>,
    query: String,
    focus: bool,

}

struct BudgetEdit {
    rail: Id,
    mode: Option<Id>,
    port: Id,
    value: String,
}

struct BoundaryDraft { name: String, parent: Option<Id>, existing: Option<Id> }

pub struct Darah {
    project: Project,
    clean: Project,
    file: Option<SavedFile>,
    recent: RecentProjects,
    rename_project: Option<String>,
    port_exit: Option<PortExit>,
    detailed_port: bool,
    dismissed_protection: HashSet<Id>,
    undo: Vec<Project>,
    redo: Vec<Project>,
    findings: Vec<Finding>,
    selected: Selection,
    issue: Option<usize>,
    draft: Option<Draft>,
    diagram: Diagram,
    status: String,
    saved_at: Option<Instant>,
    error: Option<String>,
    pending: Option<DocumentAction>,
    #[cfg(target_os = "linux")]
    updater: crate::updater::Updater,
    allow_close: bool,
    add_block: Option<AddBlock>,
    picker: Option<Picker>,
    place_block: Option<AddBlock>,
    boundary_editor: Option<BoundaryDraft>,
    drawing_boundary: Option<BoundaryDraft>,
    review_scope: Option<Id>,
    pending_link: Option<(Id, Id)>,
    budget_editor: Option<BudgetEdit>,
    return_to_check: Option<(Subject, String)>,
    palette: Option<Palette>,
    open_only: bool,
    show_outline: bool,
    show_inspector: bool,
    workspace: Workspace,
    return_to_requirements: bool,
    welcome: bool,
    help: crate::help::Help,
    interface_editor: crate::interface_ui::Editor,
    guide: Option<MonitorGuide>,
    voltage_unit: Scale,
    current_unit: Scale,
    active_mode: Option<Id>,
    show_modes: bool,
    show_power: bool,
    power_in_watts: bool,
    new_mode_name: String,
    rename_mode: String,
    spec_editor: spec_ui::Editor,
}

impl Darah {
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<&Path>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        let visuals = &mut style.visuals;
        visuals.panel_fill = Color32::from_rgb(23, 31, 41);
        visuals.window_fill = Color32::from_rgb(29, 39, 51);
        visuals.extreme_bg_color = Color32::from_rgb(16, 23, 32);
        visuals.selection.bg_fill = Color32::from_rgb(32, 70, 75);
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(39, 52, 67);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(53, 73, 89);
        visuals.widgets.active.bg_fill = Color32::from_rgb(46, 81, 90);
        style.spacing.item_spacing = egui::vec2(9.0, 8.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style.spacing.scroll.bar_width = 12.0;
        cc.egui_ctx.set_style(style);
        let project = Project::new();
        let mut app = Self {
            #[cfg(target_os = "linux")]
            updater: crate::updater::Updater::new(),
            clean: project.clone(), project, file: None, recent: RecentProjects::load(),
            rename_project: None, port_exit: None, detailed_port: false, dismissed_protection: HashSet::new(), undo: Vec::new(), redo: Vec::new(),
            findings: Vec::new(), selected: Selection::Project, issue: None, draft: None,
            diagram: Diagram::default(), status: "Ready".into(), saved_at: None, error: None,
            pending: None, allow_close: false, add_block: None, place_block: None, boundary_editor: None, drawing_boundary: None, review_scope: None, pending_link: None, budget_editor: None, return_to_check: None, picker: None, palette: None,
            open_only: true, show_outline: true, show_inspector: true, workspace: Workspace::Diagram, return_to_requirements: false, welcome: true, help: crate::help::Help::default(), guide: None,
            voltage_unit: Scale::Milli, current_unit: Scale::Milli,
            active_mode: None, show_modes: false, show_power: false, power_in_watts: false,
            new_mode_name: String::new(), rename_mode: String::new(),
            spec_editor: spec_ui::Editor::default(),
            interface_editor: crate::interface_ui::Editor::default(),
        };
        app.select(Selection::Project);
        if let Some(path) = path {
            match SavedFile::open(path) {
                Ok((project, file)) => app.load(project, Some(file), true, "Project opened"),
                Err(error) => app.error = Some(error),
            }
        }
        app
    }

    fn load(&mut self, project: Project, file: Option<SavedFile>, clean: bool, message: &str) {
        self.clean = if clean { project.clone() } else { Project::new() };
        self.project = project;
        self.file = file;
        self.saved_at = None;
        if let Some(file) = &self.file { self.recent.record(&file.path); }
        self.rename_project = None;
        self.port_exit = None;
        self.detailed_port = false;
        self.dismissed_protection.clear();
        self.draft = None;
        self.welcome = false;
        self.guide = None;
        self.help.close();
        self.active_mode = None;
        self.show_modes = false;
        self.show_power = false;
        self.new_mode_name.clear();
        self.rename_mode.clear();
        self.spec_editor = spec_ui::Editor::default();
        self.open_only = true;
        self.workspace = Workspace::Diagram;
        self.return_to_requirements = false;
        self.undo.clear();
        self.redo.clear();
        self.diagram.reset_view();
        self.add_block = None;
        self.place_block = None;
        self.boundary_editor = None;
        self.drawing_boundary = None;
        self.review_scope = None;
        self.pending_link = None;
        self.budget_editor = None;
        self.return_to_check = None;
        self.picker = None;
        self.palette = None;
        self.select(Selection::Project);
        self.refresh();
        self.status = message.into();
        self.error = None;
    }

    fn dirty(&self) -> bool { self.project != self.clean }

    fn mode(&self) -> Option<&Mode> {
        self.active_mode.and_then(|id| self.project.mode(id))
    }

    fn switch_mode(&mut self, id: Option<Id>) {
        if id != self.active_mode && self.defer_port_exit(PortExit::Mode(id)) { return; }
        self.switch_mode_applied(id);
    }

    fn switch_mode_applied(&mut self, id: Option<Id>) {
        self.active_mode = id.filter(|&id| self.project.mode(id).is_some());
        self.rename_mode = self.mode().map_or_else(String::new, |mode| mode.name.clone());
        if let Some(Draft::Port(port, values, _, _, _)) = &mut self.draft {
            let value = self.active_mode.and_then(|id| self.project.mode(id))
                .and_then(|mode| mode.current_override(port.id)).flatten();
            values[3] = value.map_or_else(String::new, |ua| self.current_unit.number(ua.into()));
        }
        self.refresh();
    }

    fn refresh(&mut self) {
        self.review_scope = self.review_scope.filter(|&id| self.project.subsystem(id).is_some());
        let selected = self.issue.and_then(|index| self.findings.get(index))
            .map(|f| (f.subject, f.title.clone()));
        self.active_mode = self.active_mode.filter(|&id| self.project.mode(id).is_some());
        self.findings = review::evaluate(&self.project, self.mode());
        self.issue = selected.and_then(|(subject, title)| self.findings.iter()
            .position(|f| f.subject == subject && f.title == title));
    }

    fn edit<T>(&mut self, message: &str, command: impl FnOnce(&mut Project) -> Result<T>) -> Option<T> {
        let mut next = self.project.clone();
        match command(&mut next).and_then(|value| {
            if next.blocks != self.project.blocks || next.ports != self.project.ports {
                crate::diagram::reanchor_routes(&mut next);
            }
            next.sync_protection_rails();
            next.validate()?;
            Ok(value)
        }) {
            Ok(value) => {
                if next != self.project {
                    if next.blocks.len() != self.project.blocks.len() || next.ports.len() != self.project.ports.len() {
                        self.diagram.refit();
                    }
                    self.undo.push(mem::replace(&mut self.project, next));
                    if self.undo.len() > HISTORY_LIMIT { self.undo.remove(0); }
                    self.redo.clear();
                    self.refresh();
                }
                self.error = None;
                self.status = message.into();
                Some(value)
            }
            Err(error) => { self.error = Some(error); None }
        }
    }

    fn step_history(&mut self, redo: bool) {
        if self.defer_port_exit(PortExit::History(redo)) { return; }
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        if let Some(next) = from.pop() {
            to.push(mem::replace(&mut self.project, next));
            self.refresh();
            let blocks: Vec<_> = self.diagram.selected_blocks().iter().copied()
                .filter(|&id| self.project.block(id).is_some()).collect();
            self.select_applied(self.selected);
            self.switch_mode_applied(self.active_mode);
            self.spec_editor.select(None);
            self.diagram.select_blocks(blocks);
            self.picker = None;
            self.pending_link = None;
            self.budget_editor = None;
            self.error = None;
            self.status = if redo { "Redid edit" } else { "Undid edit" }.into();
        }
    }

    fn select(&mut self, requested: Selection) {
        if self.port_dirty() {
            if !self.selection_matches_draft(requested) && self.port_exit.is_none() { self.port_exit = Some(PortExit::Select(requested)); }
            return;
        }
        self.select_applied(requested);
    }

    fn selection_matches_draft(&self, selection: Selection) -> bool {
        !matches!(&self.draft, Some(Draft::Port(port, ..)) if port.id == 0) && selection == self.selected
    }

    fn select_applied(&mut self, requested: Selection) {
        if requested != self.selected { self.detailed_port = false; }
        let draft = match requested {
            Selection::Project => None,
            Selection::Block(id) => self.project.block(id).cloned().map(Draft::Block),
            Selection::Port(id) => self.project.port(id).cloned().map(|port| Draft::port(port, self.voltage_unit, self.current_unit, self.mode(), self.project.conversion(id))),
            Selection::Wire(id) => self.project.binding(id).map(|_| Draft::Wire(id)),
            Selection::Boundary(id) => self.project.subsystem(id).map(|_| Draft::Boundary(id)),
        };
        if let Some(draft) = draft {
            self.selected = requested;
            self.draft = Some(draft);
        } else {
            self.selected = Selection::Project;
            self.draft = Some(Draft::Project {
                title: self.project.title.clone(), purpose: self.project.purpose.clone(), condition: self.project.condition.clone(),
            });
        }
        let blocks = match self.selected {
            Selection::Block(id) => vec![id],
            _ => Vec::new(),
        };
        self.diagram.select_blocks(blocks);
        self.issue = None;
    }

    fn port_fields_dirty(&self, port: &Port, numbers: &[String; 4], details: &rail_ui::Draft,
        topology: &power_ui::Draft, signal: &signal_ui::Draft) -> bool
    {
        let saved = self.project.port(port.id).cloned().unwrap_or_else(|| Port::new(port.block, port.kind));
        let Draft::Port(baseline, previous, power, converter, characteristics) = Draft::port(
            saved, self.voltage_unit, self.current_unit, self.mode(), self.project.conversion(port.id),
        ) else { unreachable!() };
        let mut candidate = port.clone();
        candidate.layout = baseline.layout;
        candidate != baseline || numbers != &previous || !details.matches_fields(&power)
            || topology != &converter || signal != &characteristics
    }

    fn port_dirty(&self) -> bool {
        match &self.draft {
            Some(Draft::Port(port, numbers, details, topology, signal)) =>
                self.port_fields_dirty(port, numbers, details, topology, signal),
            _ => false,
        }
    }

    fn apply_port(&mut self, port: &Port, numbers: &[String; 4], details: &rail_ui::Draft,
        topology: &power_ui::Draft, signal: &signal_ui::Draft) -> Option<Id>
    {
        let mut candidate = port.clone();
        candidate.name = candidate.name.trim().into();
        if let Some(saved) = self.project.port(port.id) { candidate.layout = saved.layout; }
        let parse = || -> Result<_> {
            let mut candidate = candidate;
            if candidate.kind.power() {
                details.apply(&mut candidate)?;
                candidate.rail_domain = candidate.rail_domain.trim().to_owned();
                candidate.min_uv = self.voltage_unit.parse(&numbers[0]).map_err(|e| format!("Minimum voltage: {e}"))?;
                candidate.max_uv = self.voltage_unit.parse(&numbers[1]).map_err(|e| format!("Maximum voltage: {e}"))?;
                candidate.current_ua = self.current_unit.parse(&numbers[2]).map_err(|e| format!("Current: {e}"))?;
            } else {
                signal.apply(&mut candidate)?;
            }
            let mut conversion = None;
            if candidate.kind == PortKind::Rail && topology != &power_ui::Draft::new(self.project.conversion(port.id)) {
                let channel = topology.conversion(port.id)?;
                if channel.is_some() { candidate.independent_rail = false; }
                conversion = Some(channel);
            }
            let mode_current = if candidate.kind.power() && port.id != 0 {
                if let Some(mode) = self.mode() {
                    let stored = mode.current_override(port.id).flatten();
                    let edited = self.current_unit.parse(&numbers[3]).map_err(|e| format!("Mode current: {e}"))?;
                    (stored != edited).then_some((mode.id, edited.map(Some)))
                } else { None }
            } else { None };
            Ok((candidate, conversion, mode_current))
        };
        match parse() {
            Ok((candidate, conversion, mode_current)) => self.edit("Saved port details", |project| {
                let id = project.set_port(candidate)?;
                match conversion {
                    Some(Some(channel)) => project.set_conversion(channel)?,
                    Some(None) => project.remove_conversion(id),
                    None => {}
                }
                if let Some((mode, current)) = mode_current { project.set_mode_current(mode, id, current)?; }
                Ok(id)
            }),
            Err(error) => { self.error = Some(error); None }
        }
    }

    fn defer_port_exit(&mut self, action: PortExit) -> bool {
        if !self.port_dirty() { return false; }
        if self.port_exit.is_none() { self.port_exit = Some(action); }
        true
    }

    fn change_workspace(&mut self, workspace: Workspace) {
        if workspace != self.workspace && self.defer_port_exit(PortExit::ChangeWorkspace(workspace)) { return; }
        self.workspace = workspace;
        if workspace != Workspace::Diagram { self.drawing_boundary = None; }
        self.welcome = false;
    }

    fn select_issue(&mut self, index: usize) {
        let finding = &self.findings[index];
        let selection = match finding.subject {
            Subject::Binding(id) => Selection::Wire(id),
            _ => finding.ports.first().copied().map(Selection::Port)
                .or_else(|| finding.blocks.first().copied().map(Selection::Block))
                .unwrap_or(Selection::Project),
        };
        self.select(selection);
        self.issue = Some(index);
    }

    fn open_finding(&mut self, index: usize) {
        match self.findings[index].subject {
            Subject::Spec(id) => {
                if let Some(entry) = self.project.spec_entry(id).cloned() {
                    self.spec_editor.open_entry(entry);
                    self.workspace = Workspace::Requirements;
                }
            }
            Subject::Goal(id) => {
                if let Some(goal) = self.project.goal(id).cloned() {
                    self.spec_editor.open_goal(goal);
                    self.workspace = Workspace::Requirements;
                }
            }
            _ => {
                self.select_issue(index);
                let block = match self.selected {
                    Selection::Block(id) => Some(id),
                    Selection::Port(id) => self.project.port(id).map(|port| port.block),
                    Selection::Wire(id) => self.project.binding(id)
                        .and_then(|binding| self.project.port(binding.source)).map(|port| port.block),
                    Selection::Project | Selection::Boundary(_) => None,
                };
                if let Some(block) = block { self.diagram.reveal_block(block); }
                self.workspace = Workspace::Diagram;
                self.show_inspector = true;
            }
        }
    }

    fn open_spec_target(&mut self, target: Target, mode: Option<Id>) {
        match target {
            Target::Pin(id) | Target::Transition(id) => {
                if let Some(entry) = self.project.spec_entry(id).cloned() {
                    self.spec_editor.open_entry(entry);
                }
            }
            Target::Block(id) => self.show_design_target(Selection::Block(id), id),
            Target::Port(id) => {
                if let Some(block) = self.project.port(id).map(|port| port.block) {
                    if let Some(mode) = mode { self.switch_mode((mode != 0).then_some(mode)); }
                    self.show_design_target(Selection::Port(id), block);
                }
            }
        }
    }

    fn show_design_target(&mut self, selection: Selection, block: Id) {
        self.choose(selection);
        self.diagram.reveal_block(block);
        self.workspace = Workspace::Diagram;
        self.return_to_requirements = true;
        self.show_inspector = true;
    }

    fn save(&mut self, save_as: bool) -> bool {
        if self.defer_port_exit(PortExit::Save(save_as)) { return false; }
        self.save_applied(save_as)
    }

    fn save_applied(&mut self, save_as: bool) -> bool {
        self.saved_at = None;
        let result = if self.file.is_none() || save_as {
            let stem: String = self.project.title.chars().map(|ch| {
                if ch.is_control() || "/\\:*?\"<>|".contains(ch) { '-' } else { ch }
            }).collect();
            let stem = if self.project.title == crate::model::UNTITLED_PROJECT { "design" } else { stem.trim() };
            let Some(path) = rfd::FileDialog::new().add_filter("Darah project", &["toml"])
                .set_file_name(format!("{stem}.darah.toml")).save_file() else { return false };
            let mut project = self.project.clone();
            if project.title == crate::model::UNTITLED_PROJECT {
                if let Some(name) = storage::project_name(&path) { project.title = name.to_owned(); }
            }
            SavedFile::create(path, &project).map(|file| {
                self.file = Some(file);
                if self.project.title != project.title {
                    self.project = project;
                    if self.selected == Selection::Project { self.select_applied(Selection::Project); }
                }
            })
        } else if let Some(file) = &mut self.file {
            file.save(&self.project)
        } else { return false };
        match result {
            Ok(()) => {
                self.clean = self.project.clone();
                if let Some(file) = &self.file { self.recent.record(&file.path); }
                self.status = "Project saved".into();
                self.saved_at = Some(Instant::now());
                self.error = None;
                true
            }
            Err(error) => { self.error = Some(error); false }
        }
    }

    fn export_design(&mut self, content: ExportContent, format: ExportFormat) {
        if self.defer_port_exit(PortExit::Export(content, format)) { return; }
        let name = match content { ExportContent::Diagram => "diagram", ExportContent::Requirements => "requirements" };
        let extension = format.extension();
        let Some(mut path) = rfd::FileDialog::new().add_filter(format.label(), &[extension])
            .set_file_name(format!("{name}.{extension}")).save_file() else { return; };
        if path.extension().is_none_or(|ext| !ext.eq_ignore_ascii_case(extension)) {
            path.set_extension(extension);
        }
        match export::write(&self.project, content, format, &path) {
            Ok(()) => { self.status = format!("Exported {}", path.display()); self.error = None; }
            Err(error) => self.error = Some(error),
        }
    }

    fn export_review(&mut self, format: ReviewFormat) {
        let extension = format.extension();
        let Some(mut path) = rfd::FileDialog::new().add_filter(format.label(), &[extension])
            .set_file_name(format!("design-review.{extension}")).save_file() else { return; };
        if path.extension().is_none_or(|ext| !ext.eq_ignore_ascii_case(extension)) {
            path.set_extension(extension);
        }
        match export::write_review(&self.project, self.mode(), &self.findings, self.review_scope, format, &path) {
            Ok(()) => { self.status = "Review exported".into(); self.error = None; }
            Err(error) => self.error = Some(error),
        }
    }

    fn export_power_review(&mut self, format: PowerReportFormat) {
        if self.defer_port_exit(PortExit::ExportPower(format)) { return; }
        let extension = format.extension();
        let Some(mut path) = rfd::FileDialog::new().add_filter(format.label(), &[extension])
            .set_file_name(format!("power-review.{extension}")).save_file() else { return; };
        if path.extension().is_none_or(|ext| !ext.eq_ignore_ascii_case(extension)) {
            path.set_extension(extension);
        }
        match export::write_power_review(&self.project, self.mode(), self.review_scope, format, &path) {
            Ok(()) => { self.status = format!("Exported {}", path.display()); self.error = None; }
            Err(error) => self.error = Some(error),
        }
    }

    fn request(&mut self, action: DocumentAction, ctx: &egui::Context) {
        if self.defer_port_exit(PortExit::Document(action.clone())) { return; }
        if self.dirty() { self.pending = Some(action); }
        else { self.run_action(action, ctx); }
    }

    fn run_action(&mut self, action: DocumentAction, ctx: &egui::Context) {
        self.pending = None;
        match action {
            DocumentAction::New => self.load(Project::new(), None, true, "New project"),
            DocumentAction::Monitor => match Project::monitor() {
                Ok(project) => {
                    let rail = project.ports.iter().find(|port| port.kind == PortKind::Rail).map(|port| port.id);
                    self.load(project, None, true, "Sample project · edits are local until you save");
                    if let Some(rail) = rail {
                        self.guide = Some(MonitorGuide { rail, alert: None });
                        self.focus_rail(rail);
                    }
                }
                Err(error) => self.error = Some(error),
            },
            DocumentAction::ModesDemo => match Project::modes_demo() {
                Ok(project) => {
                    self.load(project, None, true, "Modes demo · illustrative values");
                    self.show_power = true;
                }
                Err(error) => self.error = Some(error),
            },
            DocumentAction::PowerExample(pmic) => match power::example(pmic) {
                Ok(project) => {
                    self.load(project, None, true, "Power topology example");
                    self.show_power = true;
                }
                Err(error) => self.error = Some(error),
            },
            DocumentAction::SpecificationExample => match crate::spec::monitor_example() {
                Ok(project) => {
                    self.load(project, None, true, "desk monitor specification example");
                    self.workspace = Workspace::Requirements;
                    if let Some(entry) = self.project.spec.iter().find(|entry|
                        matches!(&entry.data, crate::spec::Data::Requirement { rule: crate::spec::Rule::InputPower { mode: Some(0), .. }, .. })
                    ).cloned() {
                        self.spec_editor.open_entry(entry);
                    }
                }
                Err(error) => self.error = Some(error),
            },
            DocumentAction::Open => {
                if let Some(path) = rfd::FileDialog::new().add_filter("Darah project", &["toml"]).pick_file() {
                    self.open_path(&path);
                }
            }
            DocumentAction::OpenRecent(path) => self.open_path(&path),
            #[cfg(target_os = "linux")]
            DocumentAction::ApplyUpdate => {
                match self.updater.install(self.file.as_ref().map(|file| file.path.clone())) {
                    Ok(()) => {
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Err(error) => self.error = Some(format!("Could not install the update. {error}")),
                }
            }
            DocumentAction::Close => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn open_path(&mut self, path: &Path) {
        match SavedFile::open(path) {
            Ok((project, file)) => self.load(project, Some(file), true, "Project opened"),
            Err(error) => self.error = Some(error),
        }
    }

    fn block_presets() -> impl Iterator<Item = (BlockType, String)> {
        Role::ALL.into_iter().map(|role| (BlockType::Functional(role), role.label().into()))
            .chain([(BlockType::Protection, "Protection".into()),
                (BlockType::SignalConversion, "Signal conversion".into()),
                (BlockType::PowerConverter, "Power converter".into())])
            .chain(interface_catalog::PRESETS.iter().enumerate().map(|(index, preset)| {
                (BlockType::Connector(index), format!("Connector · {} · {}", preset.family, preset.variant))
            }))
    }

    fn new_block(&mut self, kind: Option<BlockType>) {
        self.workspace = Workspace::Diagram;
        self.welcome = false;
        self.pending_link = None;
        self.place_block = None;
        self.drawing_boundary = None;
        self.add_block = Some(AddBlock {
            kind, name: kind.map_or("", BlockType::label).into(), template: true,
            query: String::new(), focus: kind.is_none(),
            contacts: 4, exposure: Exposure::Internal, incoming: true,
            conversion: crate::interfaces::ConversionKind::Divider, power_topology: Some(power::Kind::Buck),
            protection: ProtectionType::SeriesPower, attached: None, preview_ports: Vec::new(),
        });
    }

    fn place_new_block(&mut self, position: BlockPosition) {
        let Some(draft) = self.place_block.take() else { return };
        let result = self.edit("Placed block", |p| {
            let id = draft.create(p)?;
            p.position_block(id, Some(position))?;
            let scope = crate::diagram::scope_at_block(p, id);
            p.assign_subsystem(id, scope)?;
            Ok(id)
        });
        if let Some(id) = result {
            self.diagram.keep_view();
            self.select(Selection::Block(id));
            self.show_inspector = true;
        } else {
            self.add_block = Some(draft);
        }
    }

    fn connect_from(&mut self, from: Id) {
        self.picker = Some(Picker { from, target: None, query: String::new(), focus: true });
    }

    fn choose(&mut self, selected: Selection) {
        self.select(selected);
        if self.port_exit.is_some() { return; }
        if let Selection::Port(id) = selected {
            if let Some(port) = self.project.port(id) {
                self.status = self.project.port_label(port);
            }
        }
    }

    fn apply_diagram_action(&mut self, action: DiagramAction) {
        if matches!(&action, DiagramAction::Inspect(selection) if self.selection_matches_draft(*selection)) { return; }
        if self.port_dirty() {
            self.defer_port_exit(PortExit::Diagram(action));
            return;
        }
        match action {
            DiagramAction::Inspect(selection) => self.choose(selection),
            DiagramAction::SelectBlocks(ids) => {
                self.select(ids.first().copied().map_or(Selection::Project, Selection::Block));
                self.diagram.select_blocks(ids);
            }
            DiagramAction::RemoveBoundary(id) => {
                if self.edit("Removed subsystem boundary · Undo to restore", |project| project.remove_subsystem(id)).is_some()
                    && self.selected == Selection::Boundary(id)
                {
                    self.select(Selection::Project);
                }
            }
            DiagramAction::PositionPort(id, position) => {
                if self.edit("Moved port on block grid", |p| p.position_port(id,position)).is_some() { self.select(Selection::Port(id)); }
            }
            DiagramAction::ResetPortPosition(id) => {
                if self.edit("Restored automatic port placement", |p| p.reset_port_position(id)).is_some() {
                    self.select(Selection::Port(id));
                }
            }
            DiagramAction::AddBlock => self.new_block(None),
            DiagramAction::PlaceBlock(position) => self.place_new_block(position),
            DiagramAction::AddPort(block, kind) => {
                self.show_inspector = true;
                self.select(Selection::Block(block));
                self.draft = Some(Draft::port(Port::new(block, kind), self.voltage_unit, self.current_unit, None, None));
            }
            DiagramAction::Connect(port) => {
                self.select(Selection::Port(port));
                self.connect_from(port);
            }
            DiagramAction::ConnectPorts(from, to) => {
                self.pending_link = None;
                match review::connection_preview(&self.project, self.active_mode, from, to, &self.findings) {
                    Ok(findings) if !findings.is_empty() && findings.iter().all(|f| f.status == Status::Pass) => {
                        self.commit_connection(from, to);
                    }
                    Ok(_) => { self.pending_link = Some((from, to)); self.error = None; }
                    Err(error) => self.error = Some(error),
                }
            }
            DiagramAction::RouteWire(id, route) => {
                let message = if route.is_empty() { "Restored automatic route" } else { "Rerouted connection" };
                if self.edit(message, |project| project.route_binding(id, route)).is_some() {
                    self.select(Selection::Wire(id));
                }
            }
            DiagramAction::PlaceBoundary(position, size, members) => {
                let Some(draft) = self.drawing_boundary.take() else { return; };
                let name = draft.name.trim().to_owned();
                let result = self.edit("Saved subsystem boundary", |project| {
                    if let Some(id) = draft.existing {
                        let scope = project.subsystems.iter_mut().find(|scope| scope.id == id)
                            .ok_or("Subsystem no longer exists.")?;
                        scope.position = position;
                        scope.size = size;
                        Ok(id)
                    } else { project.add_subsystem(name, draft.parent, position, size, &members) }
                });
                if result.is_none() { self.drawing_boundary = Some(draft); }
            }
            DiagramAction::ResizeBlock(id, position, size) => {
                let message = if size.is_some() { "Resized block" } else { "Restored automatic block size" };
                if self.edit(message, |project| project.resize_block(id, position, size)).is_some() {
                    self.select(Selection::Block(id));
                }
            }
            DiagramAction::Disconnect(id) => {
                if self.edit("Disconnected ports · Undo to restore", |project| { project.unbind(id); Ok(()) }).is_some()
                    && self.selected == Selection::Wire(id)
                {
                    self.select(Selection::Project);
                }
            }
            DiagramAction::Arrange(message, positions) => {
                self.edit(message, |project| {
                    for (block, position) in positions {
                        project.position_block(block, Some(position))?;
                    }
                    Ok(())
                });
            }
            DiagramAction::Duplicate(ids) => self.duplicate_blocks(&ids),
            DiagramAction::Delete(ids) => self.delete_blocks(&ids),
            DiagramAction::Position(block, position) => {
                let message = if position.is_some() { "Moved block" } else { "Reset block position" };
                self.edit(message, |project| project.position_block(block, position));
            }
        }
    }

    fn delete_blocks(&mut self, ids: &[Id]) {
        let ids: Vec<_> = ids.iter().copied().filter(|&id| self.project.block(id).is_some()).collect();
        if ids.is_empty() { return; }
        if let Some(links) = self.edit("Deleted blocks", |project| Ok(project.remove_blocks(&ids))) {
            self.select(Selection::Project);
            self.picker = None;
            self.status = format!("Deleted {} block(s) and {links} connection(s) · Undo to restore", ids.len());
        }
    }

    fn duplicate_blocks(&mut self, ids: &[Id]) {
        let positions = self.diagram.duplicate_positions(&self.project, ids);
        if positions.is_empty() { return; }
        if let Some(copies) = self.edit("Duplicated blocks", |project| project.duplicate_blocks(&positions)) {
            self.select(Selection::Block(copies[0]));
            self.status = format!("Duplicated {} block(s) · connections within the selection were copied", copies.len());
            self.diagram.select_blocks(copies);
        }
    }

    fn outline(&mut self, ui: &mut egui::Ui) {
        section_title(ui, "01", "Project outline");
        ui.separator();
        let mut picked = None;
        let mut block_action = None;
        let mut scope_action = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            let title = ui.selectable_label(self.selected == Selection::Project, &self.project.title);
            if title.clicked() { picked = Some(Selection::Project); }
            title.context_menu(|ui| {
                if ui.button("Rename project…").clicked() {
                    self.rename_project = Some(self.project.title.clone());
                    ui.close_menu();
                }
            });
            if !self.project.subsystems.is_empty() {
                ui.separator();
                ui.strong("Subsystems");
                for scope in &self.project.subsystems {
                    ui.push_id(scope.id, |ui| {
                        let path = self.project.subsystem_path(scope.id);
                        let row = ui.selectable_label(self.review_scope == Some(scope.id), path);
                        if row.clicked() { scope_action = Some((scope.id, "review")); }
                        row.context_menu(|ui| {
                            for (label, action) in [("Open design review", "review"), ("Rename", "rename"),
                                ("Redraw boundary", "redraw"), ("Fit boundary to members", "fit"),
                                ("Remove boundary", "remove")]
                            {
                                if ui.button(label).clicked() { scope_action = Some((scope.id, action)); ui.close_menu(); }
                            }
                        });
                    });
                }
            }
            for role in Role::ALL {
                ui.add_space(8.0);
                ui.label(RichText::new(role.label()).weak());
                for block in self.project.blocks.iter().filter(|b| b.role == role) {
                    ui.push_id(block.id, |ui| {
                        let row = ui.selectable_label(self.selected == Selection::Block(block.id), &block.name);
                        if row.clicked() {
                            row.surrender_focus();
                            picked = Some(Selection::Block(block.id));
                        }
                        row.context_menu(|ui| {
                            if ui.button("Duplicate block").clicked() {
                                block_action = Some(DiagramAction::Duplicate(vec![block.id]));
                                ui.close_menu();
                            }
                            if ui.button("Delete block").clicked() {
                                block_action = Some(DiagramAction::Delete(vec![block.id]));
                                ui.close_menu();
                            }
                        });
                        ui.indent("details", |ui| {
                            for port in self.project.block_ports(block.id) {
                                let prefix = if port.kind.source() { "Out" } else { "In" };
                                if ui.selectable_label(self.selected == Selection::Port(port.id), format!("{prefix} · {}", port.name)).clicked() {
                                    picked = Some(Selection::Port(port.id));
                                }
                            }
                        });
                    });
                }
            }
        });
        if let Some(selection) = picked { self.choose(selection); }
        if let Some(action) = block_action { self.apply_diagram_action(action); }
        if let Some((id, action)) = scope_action { self.scope_action(id, action); }
    }

    fn scope_action(&mut self, id: Id, action: &str) {
        match action {
            "review" => {
                self.set_review_scope(Some(id));
                self.change_workspace(Workspace::Checks);
            }
            "rename" | "redraw" => {
                let Some(scope) = self.project.subsystem(id) else { return; };
                let draft = BoundaryDraft { name: scope.name.clone(), parent: scope.parent, existing: Some(id) };
                if action == "rename" { self.boundary_editor = Some(draft); }
                else {
                    self.add_block = None;
                    self.place_block = None;
                    self.drawing_boundary = Some(draft);
                }
            }
            "fit" => {
                if let Some((position, size)) = crate::diagram::scope_bounds(&self.project, id) {
                    self.edit("Fitted boundary to assigned members", |project| {
                        let scope = project.subsystems.iter_mut().find(|scope| scope.id == id)
                            .ok_or("Subsystem no longer exists.")?;
                        scope.position = position;
                        scope.size = size;
                        Ok(())
                    });
                } else { self.error = Some("This subsystem has no assigned members.".into()); }
            }
            "remove" => self.apply_diagram_action(DiagramAction::RemoveBoundary(id)),
            _ => {}
        }
    }

    fn set_review_scope(&mut self, scope: Option<Id>) {
        if scope != self.review_scope {
            self.review_scope = scope;
            self.issue = None;
            self.budget_editor = None;
        }
    }

    fn review_scope_picker(&mut self, ui: &mut egui::Ui, id: &str) {
        if self.project.subsystems.is_empty() { return; }
        let mut scope = self.review_scope;
        scope_picker(ui, &self.project, id, &mut scope, "Overall system");
        self.set_review_scope(scope);
    }

    fn boundary_panel(&mut self, ui: &mut egui::Ui) {
        if let Some(draft) = &self.drawing_boundary {
            let cancel = ui.horizontal_wrapped(|ui| {
                ui.strong(format!("Draw boundary · {}", draft.name));
                ui.weak("Drag from one corner to the opposite corner.");
                ui.button("Cancel").clicked()
            }).inner || ui.input(|input| input.key_pressed(egui::Key::Escape));
            if cancel { self.drawing_boundary = None; }
        } else if ui.button("+ System / subsystem").clicked() {
            self.boundary_editor = Some(BoundaryDraft { name: String::new(), parent: None, existing: None });
        }
    }

    fn boundary_window(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.boundary_editor.take() else { return; };
        let mut open = true;
        let mut apply = false;
        let title = if draft.existing.is_some() { "Rename subsystem" } else { "New system boundary" };
        egui::Window::new(title).open(&mut open).collapsible(false).show(ctx, |ui| {
            field(ui, "Boundary name", &mut draft.name);
            if draft.existing.is_none() {
                ui.label("Parent system");
                scope_picker(ui, &self.project, "boundary_parent", &mut draft.parent, "Overall project");
                ui.weak("Blocks inside the rectangle that belong to its parent are assigned to this subsystem.");
            }
            let label = if draft.existing.is_some() { "Rename" } else { "Draw boundary" };
            apply = ui.add_enabled(!draft.name.trim().is_empty(), egui::Button::new(label)).clicked();
        });
        if apply {
            if let Some(id) = draft.existing {
                let name = draft.name.trim().to_owned();
                if self.edit("Renamed subsystem", |project| {
                    project.subsystems.iter_mut().find(|scope| scope.id == id)
                        .ok_or("Subsystem no longer exists.")?.name = name;
                    Ok(())
                }).is_some() { return; }
            } else {
                self.add_block = None;
                self.place_block = None;
                self.drawing_boundary = Some(draft);
                return;
            }
        }
        if open && !ctx.input(|input| input.key_pressed(egui::Key::Escape)) { self.boundary_editor = Some(draft); }
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        section_title(ui, "02", "Inspector");
        ui.weak("Configure the selected item.");
        ui.separator();
        let Some(mut draft) = self.draft.take() else { return };
        let mut next = None;
        let mut picked_port = None;
        egui::ScrollArea::vertical()
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
            match &mut draft {
                Draft::Project { title, purpose, condition } => {
                    ui.strong("Device definition");
                    field(ui, "Name", title);
                    ui.label("Purpose");
                    ui.text_edit_multiline(purpose);
                    field(ui, "Base operating mode", condition);
                    if ui.button("Apply project details").clicked() {
                        self.edit("Updated project", |p| {
                            p.title = title.trim().to_owned();
                            p.purpose = purpose.clone();
                            p.condition = condition.trim().to_owned();
                            Ok(())
                        });
                    }
                }
                Draft::Block(block) => {
                    ui.strong("Block details");
                    if !self.project.subsystems.is_empty() {
                        let mut scope = self.project.block(block.id).and_then(|saved| saved.subsystem);
                        let before = scope;
                        ui.label("Subsystem membership");
                        scope_picker(ui, &self.project, "block_scope", &mut scope, "Overall system");
                        if scope != before {
                            self.edit("Assigned block to subsystem", |project| project.assign_subsystem(block.id, scope));
                            block.subsystem = scope;
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Duplicate").clicked() {
                            self.duplicate_blocks(&[block.id]);
                        }
                        if ui.button("Delete block").on_hover_text("Removes this block and its connections").clicked() {
                            self.delete_blocks(&[block.id]);
                        }
                    });
                    let name = field(ui, "Name", &mut block.name);
                    let role = block.role;
                    role_combo(ui, "edit_role", &mut block.role);
                    ui.label("Purpose");
                    let purpose = ui.text_edit_multiline(&mut block.purpose);
                    let changed = self.project.block(block.id).is_some_and(|saved| saved != &*block);
                    if changed && (name.lost_focus() || purpose.lost_focus() || role != block.role) {
                        let mut block = block.clone();
                        block.name = block.name.trim().into();
                        if let Some(id) = self.edit("Updated block", |p| p.set_block(block)) {
                            next = self.project.block(id).cloned().map(Draft::Block);
                        }
                    }
                    ui.separator();
                    ui.menu_button("+ Port", |ui| {
                        for kind in PortKind::ALL {
                            if ui.button(kind.label()).clicked() {
                                next = Some(Draft::port(Port::new(block.id, kind), self.voltage_unit, self.current_unit, None, None));
                                ui.close_menu();
                            }
                        }
                    });
                    if ui.button("Ports, templates and block tools…").clicked() {
                        self.interface_editor.open(&self.project, Some(block.id), crate::interface_ui::Tab::Ports);
                    }
                    if self.project.interfaces.channels.iter().any(|c| c.block == block.id)
                        && ui.button("Configure signal conversion…").clicked() {
                        self.interface_editor.open(&self.project, Some(block.id), crate::interface_ui::Tab::Conversion);
                    }
                    if self.project.interfaces.protections.iter().any(|p| p.block == block.id)
                        && ui.button("Configure protection…").clicked() {
                        self.interface_editor.open(&self.project, Some(block.id), crate::interface_ui::Tab::Protection);
                    }
                    if self.project.interfaces.connectors.iter().any(|c|c.block==block.id) && ui.button("Connector properties…").clicked() {
                        self.interface_editor.open(&self.project,Some(block.id),crate::interface_ui::Tab::Connectors);
                    }
                    let outputs: Vec<_> = self.project.conversions.iter().filter_map(|channel| {
                        let output = self.project.port(channel.output)?;
                        (output.block == block.id).then_some((output.id, output.name.clone()))
                    }).collect();
                    if !outputs.is_empty() {
                        ui.label("Converter channels");
                        for (id, name) in outputs {
                            if ui.link(name).clicked() {
                                self.select(Selection::Port(id));
                                self.detailed_port = true;
                            }
                        }
                    }
                    self.linked_requirements(ui, &[Target::Block(block.id)], Some(block.id), false);
                }
                Draft::Port(port, numbers, details, topology, signal) => {
                    let id = port.id;
                    ui.strong(port.kind.label());
                    if let Some(block) = self.project.block(port.block) { ui.label(&block.name); }
                    if id != 0 && action_button(ui, "Connect this port…") {
                        self.connect_from(id);
                    }
                    let name_field = field(ui, "Name", &mut port.name);
                    let mut enter_port_field = field_enter(ui, name_field);
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.detailed_port, false, "Overview");
                        ui.selectable_value(&mut self.detailed_port, true, "Details");
                    });
                    if self.detailed_port {
                        let pin_field = field(ui, "Physical pin number (optional)", &mut port.pin);
                        enter_port_field |= field_enter(ui, pin_field);
                    }
                    ui.checkbox(&mut port.required, "Connection required");
                    if port.kind.power() {
                        let inherited = self.project.inherited_rail_input(id);
                        let protected = self.project.interfaces.protections.iter().any(|stage| !stage.shunt && stage.exit == Some(id));
                        if self.detailed_port && port.kind == PortKind::Rail && self.project.conversion(id).is_none() && !protected {
                            if let Err(error) = details.source(ui, port, numbers, self.voltage_unit) { self.error = Some(error); }
                        }
                        if let Some(input) = inherited.and_then(|input| self.project.port(input)) {
                            ui.weak(format!("Voltage follows {}. Override it in the protection settings.", self.project.port_label(input)));
                        }
                        ui.label(if port.kind.source() { "Declared output interval" } else { "Accepted input interval" });
                        if let Err(error) = unit_picker(ui, "Voltage units", "V", &mut self.voltage_unit, &mut numbers[..2]) {
                            self.error = Some(error);
                        }
                        ui.add_enabled_ui(inherited.is_none(), |ui| {
                            details.show(ui, port, &mut numbers[..2], self.voltage_unit,
                                self.project.conversion(id).is_some(), self.detailed_port);
                            if self.detailed_port { field(ui, "Power domain (optional)", &mut port.rail_domain); }
                            let min = field(ui, &format!("Minimum ({})", self.voltage_unit.label("V")), &mut numbers[0]);
                            let max = field(ui, &format!("Maximum ({})", self.voltage_unit.label("V")), &mut numbers[1]);
                            enter_port_field |= field_enter(ui, min) || field_enter(ui, max);
                            details.show_nominal(ui, &numbers[..2], self.voltage_unit, self.detailed_port);
                        });
                        if let Err(error) = unit_picker(ui, "Current units", "A", &mut self.current_unit, &mut numbers[2..]) {
                            self.error = Some(error);
                        }
                        let converted_input = port.kind == PortKind::Load && id != 0
                            && Network::new(&self.project, self.mode()).transfer_input(id);
                        let label = if port.kind.source() { "Available current" }
                            else if converted_input { "Additional input current" } else { "Maximum load" };
                        if converted_input && !self.project.dedicated_converter_input(id) {
                            ui.weak("Downstream current is calculated. Enter only separate input-side draw here. Put the upstream supply rating on its output rail.");
                        }
                        if self.active_mode.is_some() { ui.weak("Base values are inherited by modes without an override."); }
                        if self.project.dedicated_converter_input(id) {
                            ui.weak("Input compatibility only. The power budget derives current from downstream load and converter assumptions.");
                        } else {
                            let current_field = field(ui, &format!("{label} ({})", self.current_unit.label("A")), &mut numbers[2]);
                            enter_port_field |= field_enter(ui, current_field);
                        }
                        ui.weak("Decimals are supported to 1 µV or 1 µA. Leave unknown values blank.");
                    } else {
                        field(ui, "Signal contract", &mut port.signal);
                        egui::ComboBox::from_id_salt("shared_port_contract").selected_text(port.contract.and_then(|id| self.project.contract(id))
                            .map_or("Select shared contract…", |c| c.name.as_str())).height(340.0).show_ui(ui, |ui| {
                                ui.selectable_value(&mut port.contract, None, "Legacy / custom contract name");
                                for c in &self.project.interfaces.contracts { ui.selectable_value(&mut port.contract, Some(c.id), &c.name); }
                            });
                        if self.detailed_port {
                            ui.weak("Shared requirements and actual hardware characteristics are kept separately.");
                            if let Some(contract)=port.contract {
                                if ui.add_enabled(!self.port_fields_dirty(port, numbers, details, topology, signal),
                                egui::Button::new("Edit selected shared contract…")).clicked() { self.interface_editor.edit_contract(&self.project,contract); }
                            }
                            if id != 0 && ui.add_enabled(!self.port_fields_dirty(port, numbers, details, topology, signal),
                            egui::Button::new("Create shared contract from applied port details…")).clicked() {
                                if let Some(saved) = self.project.port(id).cloned() {
                                    if let Err(error) = self.interface_editor.from_port(&self.project, &saved) { self.error = Some(error); }
                                }
                            }
                            ui.weak("Use the same contract name on the source and sink.");
                            signal.show(ui, port);
                        }
                    }
                    if self.detailed_port { field(ui, "Assumption or source", &mut port.note); }
                    if id != 0 && self.detailed_port && port.kind == PortKind::Rail {
                        if port.source.is_some() {
                            ui.weak("Choose Custom source to configure a converter output.");
                        } else if !self.project.interfaces.protections.iter().any(|stage| stage.exit == Some(id)) {
                            let finding = self.findings.iter().find(|f| f.subject == Subject::Conversion(id));
                            topology.show(ui, &self.project, port, finding);
                        }
                    }
                    if ui.button(if id == 0 { "Create port" } else { "Apply port details" }).clicked()
                        || enter_port_field
                    {
                        if let Some(saved) = self.apply_port(port, numbers, details, topology, signal) {
                            let issue = self.issue;
                            self.select_applied(Selection::Port(saved));
                            self.issue = issue;
                        }
                    }
                    if id != 0 {
                        if ui.add_enabled(!self.port_fields_dirty(port, numbers, details, topology, signal), egui::Button::new("Duplicate port")).clicked() {
                            if let Some(copy) = self.edit("Duplicated port", |p| p.duplicate_port(id)) { self.select(Selection::Port(copy)); }
                        }
                        if ui.button("Save applied port as reusable template").clicked() {
                            let name = format!("{} template", port.name);
                            self.edit("Saved reusable port template", |p| p.save_port_template(id, name));
                        }
                        if port.kind.power() && self.detailed_port && !self.project.dedicated_converter_input(id) {
                            self.mode_current_editor(ui, id, &mut numbers[3]);
                        }
                        let dirty = self.port_fields_dirty(port, numbers, details, topology, signal);
                        self.linked_requirements(ui, &[Target::Port(id)], None, dirty);
                        ui.separator();
                        let links: Vec<_> = self.project.bindings.iter().filter(|b| b.source == id || b.target == id).cloned().collect();
                        for link in links {
                            let other = if link.source == id { link.target } else { link.source };
                            let label = self.project.port(other).map(|p| self.project.port_label(p)).unwrap_or_default();
                            ui.push_id(link.id, |ui| {
                                if ui.link(label).clicked() { picked_port = Some(other); }
                                if ui.add_enabled(!self.port_fields_dirty(port, numbers, details, topology, signal),
                                    egui::Button::new("Disconnect").small()).clicked() {
                                    self.edit("Removed connection", |p| { p.unbind(link.id); Ok(()) });
                                }
                            });
                        }
                        ui.separator();
                        if ui.add_enabled(!self.port_fields_dirty(port, numbers, details, topology, signal), egui::Button::new("Delete port")).clicked() && self.edit("Deleted port", |p| {
                            p.remove_port(id)
                        }).is_some() { self.select(Selection::Block(port.block)); }
                    }
                }
                Draft::Boundary(id) => {
                    if self.project.subsystem(*id).is_some() {
                        ui.strong(self.project.subsystem_path(*id));
                        ui.weak("Removing the boundary keeps its blocks and connections.");
                        if ui.button("Remove boundary").clicked() {
                            self.apply_diagram_action(DiagramAction::RemoveBoundary(*id));
                        }
                    }
                }
                Draft::Wire(id) => {
                    if let Some(wire) = self.project.binding(*id).cloned() {
                        let (source, target, manual) = (wire.source, wire.target, wire.bend.is_some() || !wire.route.is_empty());
                        self.linked_requirements(ui, &[Target::Port(target), Target::Port(source)], None, false);
                        ui.heading("Connection");
                        for (role, port) in [("From", source), ("To", target)] {
                            if let Some(port) = self.project.port(port) {
                                ui.weak(role);
                                if ui.link(self.project.port_label(port)).clicked() {
                                    picked_port = Some(port.id);
                                }
                            }
                        }
                        if let Some(finding) = self.findings.iter().find(|finding| finding.subject == Subject::Binding(*id)) {
                            ui.strong("Direct interface check");
                            ui.colored_label(status_color(finding.status), finding.status.label());
                            ui.label(&finding.detail);
                            for item in status_ui::ordered_evidence(&finding.evidence) { status_ui::evidence(ui, item); }
                            if ui.button("Open direct check").clicked() {
                                self.issue = self.findings.iter().position(|item| std::ptr::eq(item, finding));
                                self.workspace = Workspace::Checks;
                            }
                        }
                        let wire_checks: Vec<_> = crate::diagram::wire_findings(&wire, &self.findings).collect();
                        if let Some(worst) = wire_checks.iter().map(|finding| finding.status).min_by_key(|status| status_ui::priority(*status)) {
                            ui.separator();
                            ui.strong("Canvas wire status");
                            status_ui::badge(ui, worst);
                            for finding in wire_checks.into_iter().filter(|finding| finding.status == worst) {
                                ui.label(&finding.title);
                                ui.weak(&finding.detail);
                                if let Some(index) = self.findings.iter().position(|item| std::ptr::eq(item, finding)) {
                                    ui.push_id(index, |ui| {
                                        if ui.button("Open design check").clicked() {
                                            self.issue = Some(index);
                                            self.workspace = Workspace::Checks;
                                        }
                                    });
                                }
                            }
                        }
                        ui.separator();
                        ui.weak(if manual { "Manual route" } else { "Automatic route" });
                        ui.label("Drag a line perpendicular to its direction, or drag a corner handle. Straight sections merge when they meet.");
                        if manual && ui.button("Reset route").clicked() {
                            self.apply_diagram_action(DiagramAction::RouteWire(*id, Vec::new()));
                        }
                        if ui.button("Disconnect").clicked() {
                            self.apply_diagram_action(DiagramAction::Disconnect(*id));
                        }
                    }
                }
            }
            if matches!(&draft, Draft::Port(..) | Draft::Project { .. }) {
                ui.add_space(10.0);
                ui.weak("Apply to save changes to this form.");
            }
        });
        if self.draft.is_none() { self.draft = next.or(Some(draft)); }
        if let Some(other) = picked_port { self.choose(Selection::Port(other)); }
    }

    fn checks_workspace(&mut self, ui: &mut egui::Ui) {
        ui.heading("Design checks");
        self.review_scope_picker(ui, "checks_scope");
        if let Some(scope) = self.review_scope {
            let interfaces = review::boundary_lines(&self.project, scope, self.mode(), false, false);
            ui.collapsing(format!("Boundary interfaces · {}", interfaces.len()), |ui| {
                for line in interfaces { ui.label(line); }
            });
        }
        ui.label("Review results from declared design inputs.");
        ui.horizontal_wrapped(|ui| {
            for (status, label) in [(Status::Fail, "failed"), (Status::Warning, "warnings"), (Status::Open, "need input"), (Status::Manual, "manual")] {
                let count = self.findings.iter().filter(|f| f.status == status && self.project.finding_in_scope(f, self.review_scope)).count();
                ui.label(RichText::new(format!("{count} {label}")).color(status_color(status)));
            }
            ui.checkbox(&mut self.open_only, "Hide passed checks");
        });
        ui.separator();
        self.rail_fix_panel(ui);
        let mut selected = self.issue.filter(|&index| self.findings.get(index)
            .is_some_and(|finding| self.project.finding_in_scope(finding, self.review_scope)));
        let mut open = None;
        let mut fix_rail = None;
        let mut fix_port = None;
        ui.columns(2, |cols| {
            egui::ScrollArea::vertical().id_salt("review_list").show(&mut cols[0], |ui| {
                let mut rows: Vec<_> = self.findings.iter().enumerate()
                    .filter(|(_, finding)| self.project.finding_in_scope(finding, self.review_scope)).collect();
                rows.sort_by_key(|(_, finding)| status_ui::priority(finding.status));
                let mut shown = false;
                for (index, finding) in rows {
                    if self.open_only && finding.status == Status::Pass && selected != Some(index) { continue; }
                    shown = true;
                    let active = selected == Some(index);
                    let color = status_color(finding.status);
                    let width = ui.available_width();
                    let frame = egui::Frame::group(ui.style())
                        .fill(if active { ui.visuals().faint_bg_color } else { Color32::TRANSPARENT })
                        .stroke(egui::Stroke::NONE)
                        .inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                            ui.set_min_width((width - 20.0).max(0.0));
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(format!("#{}", index + 1)).small().color(MUTED));
                                ui.label(RichText::new(finding.status.label()).color(color));
                                ui.label(&finding.title);
                            });
                        });
                    if active {
                        let rect = frame.response.rect;
                        ui.painter().rect_filled(egui::Rect::from_min_max(rect.min,
                            egui::pos2(rect.left() + 2.0, rect.bottom())), 0.0, color);
                    }
                    if ui.interact(frame.response.rect, ui.id().with(("review_row", index)), egui::Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand).clicked() { selected = Some(index); }
                }
                if !shown { ui.weak("No checks to show with this filter."); }
            });
            egui::ScrollArea::vertical().id_salt("review_detail").show(&mut cols[1], |ui| {
                if let Some((index, finding)) = selected.and_then(|index| self.findings.get(index).map(|finding| (index, finding))) {
                    let color = status_color(finding.status);
                    ui.label(RichText::new(format!("Check #{} · Selected result", index + 1)).small().color(MUTED));
                    ui.heading(&finding.title);
                    ui.colored_label(color, finding.status.label());
                    ui.label(&finding.detail);
                    ui.separator();
                    if !finding.evidence.is_empty() {
                        egui::CollapsingHeader::new("Evidence").id_salt(("check_evidence", index)).default_open(true).show(ui, |ui| {
                            for item in status_ui::ordered_evidence(&finding.evidence) { status_ui::compact_evidence(ui, item); }
                        });
                    }
                    ui.add_space(8.0);
                    let label = match finding.subject {
                        Subject::Spec(id) if self.project.spec_entry(id).is_some_and(|entry| matches!(&entry.data, crate::spec::Data::Requirement { .. })) => "Open requirement",
                        Subject::Spec(_) => "Open specification entry",
                        Subject::Goal(_) => "Open requirement",
                        _ => "Show on canvas",
                    };
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(label).clicked() { open = Some(index); }
                        if finding.status != Status::Pass {
                            match finding.subject {
                                Subject::Rail(rail) => {
                                    if ui.button("Fix here · currents").clicked() { fix_rail = Some(rail); }
                                }
                                Subject::Port(port) if self.project.port(port).is_some_and(|p| p.required)
                                    && self.project.bindings.iter().all(|b| b.source != port && b.target != port) =>
                                {
                                    if ui.button("Choose connection…").clicked() { fix_port = Some((index, port)); }
                                }
                                _ => {}
                            }
                        }
                    });
                } else {
                    ui.weak("Select a check to see what Darah used to evaluate it.");
                }
            });
        });
        self.issue = selected;
        let rail = selected.and_then(|index| match self.findings.get(index)?.subject {
            Subject::Rail(rail) => Some(rail),
            _ => None,
        });
        if let Some(rail) = fix_rail {
            self.begin_budget_edit(rail, rail);
            ui.ctx().request_repaint();
        } else if self.budget_editor.as_ref().is_some_and(|edit| Some(edit.rail) != rail || edit.mode != self.active_mode) {
            self.budget_editor = None;
        }
        if let Some(index) = open { self.open_finding(index); }
        if let Some((index, port)) = fix_port {
            let finding = &self.findings[index];
            self.return_to_check = Some((finding.subject, finding.title.clone()));
            self.open_finding(index);
            self.connect_from(port);
        }
    }

    fn begin_budget_edit(&mut self, rail: Id, port: Id) {
        if let Some(entry) = self.project.port(port) {
            let value = review::effective_current(entry, self.mode())
                .map_or_else(String::new, |ua| self.current_unit.number(ua.into()));
            self.budget_editor = Some(BudgetEdit { rail, mode: self.active_mode, port, value });
        }
    }

    fn rail_fix_panel(&mut self, ui: &mut egui::Ui) {
        let Some(mut edit) = self.budget_editor.take() else { return };
        let Some(rail) = self.project.port(edit.rail) else { return };
        let mut switch_port = None;
        let mut close = false;
        let mut save = None;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            let mode = self.mode();
            let network = Network::new(&self.project, mode);
            let finding = review::rail_budget(&self.project, rail, mode, &network);
            let mode_name = mode.map_or(self.project.condition.as_str(), |mode| mode.name.as_str());
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("Rail budget · {mode_name}"));
                close = ui.button("Close current editor").clicked();
            });
            ui.colored_label(status_color(finding.status), &finding.detail);
            let limit = review::effective_current(rail, mode).map_or_else(|| "unknown".into(),
                |ua| self.current_unit.display(ua.into(), "A"));
            if ui.selectable_label(edit.port == rail.id, format!("Available current · {limit}")).clicked() {
                switch_port = Some(rail.id);
            }
            egui::ScrollArea::vertical().id_salt("rail_fix_loads").max_height(110.0).show(ui, |ui| {
                for binding in self.project.bindings.iter().filter(|b| b.source == rail.id) {
                    if let Some(load) = self.project.port(binding.target) {
                        let demand = review::demand_label(network.demand(load.id));
                        let label = format!("{} · {demand}", self.project.port_label(load));
                        if self.project.dedicated_converter_input(load.id) { ui.weak(label); }
                        else if ui.selectable_label(edit.port == load.id, label).clicked() { switch_port = Some(load.id); }
                    }
                }
                for channel in self.project.conversions.iter().filter(|c| c.input == rail.id) {
                    if let Some(output) = self.project.port(channel.output) {
                        ui.weak(format!("{} · calculated channel demand · {}", self.project.port_label(output),
                            review::demand_label(network.transfer(output.id))));
                    }
                }
            });
            if let Some(port) = self.project.port(edit.port) {
                let field_name = if port.id == rail.id { "Available current" }
                    else if network.transfer_input(port.id) { "Additional input current" } else { "Maximum load" };
                ui.label(format!("{} · {field_name} ({})", self.project.port_label(port), self.current_unit.label("A")));
                if let Some(mode) = mode {
                    ui.weak(match mode.current_override(port.id) {
                        None => "Inherits base current. Saving a value creates an override for this mode.",
                        Some(None) => "Current is explicitly unknown in this mode.",
                        Some(Some(_)) => "Uses a current override for this mode.",
                    });
                }
                ui.horizontal_wrapped(|ui| {
                    let field = ui.push_id(("budget_current", edit.rail, edit.mode, edit.port), |ui| {
                        ui.add(egui::TextEdit::singleline(&mut edit.value).desired_width(105.0))
                    }).inner;
                    if ui.button("Save current").clicked()
                        || field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        save = Some(self.current_unit.parse(&edit.value).map(Some));
                    }
                    if mode.is_some() && ui.button("Use base value").clicked() { save = Some(Ok(None)); }
                    if ui.button("Mark unknown").clicked() { save = Some(Ok(Some(None))); }
                });
            }
        });
        if close { return; }
        if let Some(value) = save {
            match value {
                Ok(value) => {
                    if self.edit("Updated current from rail check", |p| {
                        if let Some(mode) = edit.mode {
                            p.set_mode_current(mode, edit.port, value)?;
                        } else {
                            let mut port = p.port(edit.port).cloned().ok_or("Port no longer exists.")?;
                            port.current_ua = value.flatten();
                            p.set_port(port)?;
                        }
                        Ok(())
                    }).is_some() {
                        if self.selected == Selection::Port(edit.port) {
                            let issue = self.issue;
                            self.select(Selection::Port(edit.port));
                            self.issue = issue;
                        }
                        self.begin_budget_edit(edit.rail, edit.port);
                        return;
                    }
                }
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(port) = switch_port.filter(|&port| port != edit.port) {
            self.begin_budget_edit(edit.rail, port);
        } else {
            self.budget_editor = Some(edit);
        }
    }

    fn mode_picker(&mut self, ui: &mut egui::Ui, id: &str) {
        let mut chosen = self.active_mode;
        let name = self.mode().map_or(self.project.condition.as_str(), |mode| mode.name.as_str());
        egui::ComboBox::from_id_salt(id).selected_text(format!("Mode: {name}")).show_ui(ui, |ui| {
            ui.selectable_value(&mut chosen, None, format!("{} (base)", self.project.condition));
            for mode in &self.project.modes {
                ui.selectable_value(&mut chosen, Some(mode.id), &mode.name);
            }
        });
        if chosen != self.active_mode { self.switch_mode(chosen); }
    }

    fn mode_current_editor(&mut self, ui: &mut egui::Ui, port: Id, text: &mut String) {
        let Some(mode) = self.mode() else { return };
        let id = mode.id;
        let current = mode.current_override(port);
        ui.separator();
        ui.strong(format!("Current in {}", mode.name));
        ui.weak(match current {
            None => "Inherits the base current.",
            Some(None) => "Explicitly unknown in this mode.",
            Some(Some(_)) => "Uses a mode-specific current.",
        });
        field(ui, &format!("Override ({})", self.current_unit.label("A")), text);
        if ui.button("Apply mode current").clicked() {
            match self.current_unit.parse(text) {
                Ok(Some(value)) => {
                    self.edit("Updated mode current", |project| project.set_mode_current(id, port, Some(Some(value))));
                }
                Ok(None) => self.error = Some("Enter a value or choose Mark unknown.".into()),
                Err(error) => self.error = Some(error),
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Use base value").clicked()
                && self.edit("Restored inherited current", |project| project.set_mode_current(id, port, None)).is_some()
            {
                text.clear();
            }
            if ui.button("Mark unknown").clicked()
                && self.edit("Marked mode current unknown", |project| project.set_mode_current(id, port, Some(None))).is_some()
            {
                text.clear();
            }
        });
    }

    fn modes_window(&mut self, ctx: &egui::Context) {
        if !self.show_modes { return; }
        let mut open = true;
        egui::Window::new("Operating modes").open(&mut open).default_width(380.0).show(ctx, |ui| {
            ui.label("Modes inherit base current values. Select a power port to override its load or rail limit.");
            self.mode_picker(ui, "managed_mode");
            ui.separator();
            field(ui, "New mode name", &mut self.new_mode_name);
            if ui.button("Add mode").clicked() {
                let name = self.new_mode_name.trim().to_owned();
                if let Some(id) = self.edit("Added operating mode", |project| project.add_mode(name)) {
                    self.new_mode_name.clear();
                    self.switch_mode(Some(id));
                }
            }
            if let Some(id) = self.active_mode {
                ui.separator();
                field(ui, "Active mode name", &mut self.rename_mode);
                if ui.button("Rename mode").clicked() {
                    let name = self.rename_mode.trim().to_owned();
                    if self.edit("Renamed operating mode", |project| project.rename_mode(id, name)).is_some() {
                        self.switch_mode(Some(id));
                    }
                }
                if ui.button("Delete mode and its overrides").clicked()
                    && self.edit("Deleted operating mode", |project| { project.remove_mode(id); Ok(()) }).is_some()
                {
                    self.switch_mode(None);
                }
            }
        });
        self.show_modes = open;
    }

    fn power_window(&mut self, ctx: &egui::Context) {
        if !self.show_power { return; }
        let mut open = true;
        let mut picked = None;
        let mut export = None;
        egui::Window::new("Power review").open(&mut open).default_width(630.0).show(ctx, |ui| {
            self.review_scope_picker(ui, "power_scope");
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.power_in_watts, false, "Current (A)");
                ui.selectable_value(&mut self.power_in_watts, true, "Nominal power (W)");
                ui.separator();
                ui.menu_button("Export detailed review", |ui| {
                    ui.weak("Exports the selected scope and active operating mode.");
                    for format in PowerReportFormat::ALL {
                        if ui.button(format.label()).clicked() {
                            export = Some(format);
                            ui.close_menu();
                        }
                    }
                });
            });
            if let Some(scope) = self.review_scope {
                let interfaces = review::boundary_lines(&self.project, scope, self.mode(), self.power_in_watts, true);
                ui.collapsing(format!("Power boundary interfaces · {}", interfaces.len()), |ui| {
                    for line in interfaces { ui.label(line); }
                });
            }
            if self.power_in_watts {
                ui.weak("DC power is recalculated at nominal voltages. The design verdict still uses voltage limits.");
            }
            ui.label("Compare rail limits with local and downstream demand by mode. Select a row to inspect its contributors.");
            let modes: Vec<_> = std::iter::once(None).chain(self.project.modes.iter().map(Some))
                .map(|mode| (mode, Network::new(&self.project, mode),
                    self.power_in_watts.then(|| Network::nominal(&self.project, mode)))).collect();
            egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                let mut has_rails = false;
                for rail in self.project.ports.iter().filter(|port| port.kind == PortKind::Rail
                    && self.review_scope.is_none_or(|scope| self.project.block_in_scope(port.block, scope))) {
                    has_rails = true;
                    ui.separator();
                    ui.strong(self.project.port_label(rail));
                    if let Some(channel) = self.project.conversion(rail.id) {
                        if let Some(input) = self.project.port(channel.input) {
                            let kind = channel.kind.label();
                            ui.label(format!("Fed by {} · {kind}", self.project.port_label(input)));
                        }
                    }
                    if let Some(input) = modes.first().and_then(|(_, network, _)| network.inferred_feeder(rail.id))
                        .and_then(|input| self.project.port(input))
                    {
                        ui.weak(format!("Possible feed from {} · configure power topology", self.project.port_label(input)));
                    }
                    for (mode, network, nominal) in &modes {
                        let current = review::rail_budget(&self.project, rail, *mode, network);
                        let watts = nominal.as_ref().and_then(|nominal| review::nominal_power_budget(&self.project, rail, *mode, nominal));
                        let detail = watts.as_ref().map_or(current.detail.as_str(), |finding| finding.detail.as_str());
                        let detail = if nominal.is_some() && watts.is_none() {
                            format!("Nominal DC watts unavailable for this rail. {detail}")
                        } else { detail.to_owned() };
                        let name = mode.map_or(self.project.condition.as_str(), |mode| mode.name.as_str());
                        let mode_id = mode.map(|mode| mode.id);
                        let text = format!("{name} · {}\n{detail}", current.status.label());
                        if ui.selectable_label(mode_id == self.active_mode,
                            RichText::new(text).color(status_color(current.status))).clicked()
                        {
                            picked = Some((rail.id, mode_id));
                        }
                        if mode_id == self.active_mode {
                            for evidence in &watts.as_ref().unwrap_or(&current).evidence { status_ui::evidence(ui, evidence); }
                        }
                    }
                }
                if !has_rails { ui.weak("Add a supply rail to compare power budgets."); }
            });
        });
        self.show_power = open;
        if let Some(format) = export { self.export_power_review(format); }
        if let Some((rail, mode)) = picked { self.open_power_finding(rail, mode); }
    }

    fn open_power_finding(&mut self, rail: Id, mode: Option<Id>) {
        if self.defer_port_exit(PortExit::FindPower(rail, mode)) { return; }
        self.switch_mode_applied(mode);
        self.focus_rail(rail);
        self.workspace = Workspace::Checks;
        self.show_power = false;
    }

    fn linked_requirements(&mut self, ui: &mut egui::Ui, targets: &[Target], owner: Option<Id>, dirty_port: bool) {
        let entries: Vec<_> = self.project.spec.iter().filter(|entry| {
            matches!(&entry.data, crate::spec::Data::Requirement { target: Some(target), .. }
                if targets.contains(target))
        }).collect();
        let goals: Vec<_> = self.project.goals.iter().filter(|goal| {
            owner.is_some_and(|block| goal.owner == owner || goal.responsible.contains(&block))
        }).collect();
        let mut opened = false;
        ui.separator();
        ui.strong(if targets.len() > 1 { "Requirements on endpoints" } else { "Requirements" });
        for entry in entries {
            if ui.link(entry.label()).clicked() {
                self.spec_editor.open_entry(entry.clone());
                opened = true;
            }
        }
        for goal in goals {
            if ui.link(&goal.description).clicked() {
                self.spec_editor.open_goal(goal.clone());
                opened = true;
            }
        }
        if let Some(target) = targets.first().copied() {
            let label = if targets.len() > 1 { "+ Requirement on receiving port" } else { "+ Requirement" };
            if ui.button(label).clicked() {
                self.spec_editor.new_requirement(target);
                opened = true;
            }
        }
        if let Some(block) = owner {
            if ui.small_button("+ Block goal").clicked() {
                self.spec_editor.open_goal(Goal::new(Some(block)));
                opened = true;
            }
        }
        if opened {
            if dirty_port && self.draft.is_none() {
                self.port_exit = Some(PortExit::ChangeWorkspace(Workspace::Requirements));
            } else {
                self.change_workspace(Workspace::Requirements);
            }
        }
    }

    fn requirements_workspace(&mut self, ui: &mut egui::Ui) {
        match self.spec_editor.show(ui, &self.project, &self.findings) {
            Ok(Some(spec_ui::Action::Save(entry))) => {
                if let Some(id) = self.edit("Saved specification entry", |project| project.set_spec_entry(entry)) {
                    self.spec_editor.select(self.project.spec_entry(id).cloned());
                }
            }
            Ok(Some(spec_ui::Action::Delete(id))) => {
                if self.edit("Deleted specification entry", |project| {
                    project.remove_spec_entry(id);
                    Ok(())
                }).is_some() {
                    self.spec_editor.select(None);
                }
            }
            Ok(Some(spec_ui::Action::SaveGoal(goal))) => {
                if let Some(id) = self.edit("Saved requirement", |project| project.set_goal(goal)) {
                    if let Some(goal) = self.project.goal(id).cloned() { self.spec_editor.open_goal(goal); }
                }
            }
            Ok(Some(spec_ui::Action::DeleteGoal(id))) => {
                if self.edit("Deleted requirement", |project| { project.remove_goal(id); Ok(()) }).is_some() {
                    self.spec_editor.select(None);
                }
            }
            Ok(Some(spec_ui::Action::OpenTarget(target, mode))) => self.open_spec_target(target, mode),
            Err(error) => self.error = Some(error),
            Ok(None) => {}
        }
    }

    fn next_protection_prompt(&self) -> Option<(Id, Id, usize)> {
        self.project.interfaces.connectors.iter()
            .filter(|connector| connector.exposure != Exposure::Internal
                && !self.dismissed_protection.contains(&connector.id))
            .find_map(|connector| {
                let count = interface_catalog::protection_needed(&self.project, connector).len();
                (count > 0 || self.project.block_ports(connector.block).next().is_none())
                    .then_some((connector.id, connector.block, count))
            })
    }

    fn project_prompts(&mut self, ui: &mut egui::Ui, protection: Option<(Id, Id, usize)>) {
        if self.port_exit.is_some() {
            let (apply, discard, stay) = ui.horizontal_wrapped(|ui| {
                ui.strong("Unsaved port details");
                (ui.button("Apply and continue").clicked(), ui.button("Discard and continue").clicked(),
                    ui.button("Keep editing").clicked())
            }).inner;
            if stay {
                self.port_exit = None;
                let blocks = match self.selected { Selection::Block(id) => vec![id], _ => Vec::new() };
                self.diagram.select_blocks(blocks);
                return;
            }
            if !apply && !discard { return; }
            let selected = self.selected;
            if apply {
                let Some(draft @ Draft::Port(..)) = self.draft.take() else { self.port_exit = None; return; };
                let Draft::Port(port, numbers, details, topology, signal) = &draft else { unreachable!() };
                let Some(id) = self.apply_port(port, numbers, details, topology, signal) else {
                    self.draft = Some(draft);
                    return;
                };
                self.select_applied(Selection::Port(id));
            } else {
                self.select_applied(selected);
            }
            let Some(action) = self.port_exit.take() else { return; };
            match action {
                PortExit::Select(selection) => self.choose(selection),
                PortExit::ChangeWorkspace(workspace) => self.change_workspace(workspace),
                PortExit::Document(action) => self.request(action, ui.ctx()),
                PortExit::HideInspector => self.show_inspector = false,
                PortExit::History(redo) => self.step_history(redo),
                PortExit::Mode(mode) => self.switch_mode(mode),
                PortExit::Save(as_new) => { self.save_applied(as_new); }
                PortExit::Export(content, format) => self.export_design(content, format),
                PortExit::ExportPower(format) => self.export_power_review(format),
                PortExit::Diagram(action) => self.apply_diagram_action(action),
                PortExit::Interface(block, tab) => self.open_interface(block, tab),
                PortExit::FindPower(rail, mode) => self.open_power_finding(rail, mode),
                PortExit::Protection(id) => self.add_connector_protection(id),
            }
            return;
        }

        let Some((id, block, count)) = protection else { return; };
        let name = self.project.block(block).map_or("Connector", |block| block.name.as_str());
        let (add, configure, dismiss) = ui.horizontal_wrapped(|ui| {
            ui.strong(format!("{name} · External connector protection"));
            if count == 0 { ui.weak("Define logical port families first."); }
            let add = count > 0 && ui.button(format!("Add recommended protection ({count})")).clicked();
            let configure = count == 0 && ui.button("Define port families…").clicked();
            (add, configure, ui.button("Dismiss").clicked())
        }).inner;
        if dismiss { self.dismissed_protection.insert(id); }
        if configure { self.open_interface(Some(block), crate::interface_ui::Tab::Connectors); }
        if add { self.add_connector_protection(id); }
    }

    fn add_connector_protection(&mut self, id: Id) {
        if self.defer_port_exit(PortExit::Protection(id)) { return; }
        if let Some(created) = self.edit("Added connector protection", |project| {
            interface_catalog::add_recommended_protection(project, id)
        }) {
            if let Some(&stage) = created.first() {
                self.workspace = Workspace::Diagram;
                self.select_applied(Selection::Block(stage));
                self.show_inspector = true;
            }
        }
    }

    fn open_interface(&mut self, block: Option<Id>, tab: crate::interface_ui::Tab) {
        if !self.defer_port_exit(PortExit::Interface(block, tab)) {
            self.interface_editor.open(&self.project, block, tab);
        }
    }

    fn rename_window(&mut self, ctx: &egui::Context) {
        let Some(mut title) = self.rename_project.take() else { return; };
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Rename project").open(&mut open).collapsible(false).resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                let response = field(ui, "Project name", &mut title);
                apply = ui.button("Rename").clicked() || field_enter(ui, response);
            });
        if apply && self.edit("Renamed project", |project| {
            project.title = title.trim().into();
            Ok(())
        }).is_some() {
            if self.selected == Selection::Project { self.select_applied(Selection::Project); }
            return;
        }
        if open && !ctx.input(|input| input.key_pressed(egui::Key::Escape)) { self.rename_project = Some(title); }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("DARAH").strong().size(20.0).color(ACCENT));
            ui.label(RichText::new("SYSTEM DESIGN").small().color(MUTED));
            ui.separator();
            let ctx = ui.ctx().clone();
            let mut export = None;
            let mut review_export = None;
            ui.menu_button("File", |ui| {
                for (label, action) in [
                    ("New", DocumentAction::New), ("Open…", DocumentAction::Open),
                    ("Load environment monitor", DocumentAction::Monitor),
                    ("Load operating modes demo", DocumentAction::ModesDemo),
                    ("Load cascaded power example", DocumentAction::PowerExample(false)),
                    ("Load PMIC example", DocumentAction::PowerExample(true)),
                    ("Load desk monitor specification example", DocumentAction::SpecificationExample),
                ] {
                    if ui.button(label).clicked() { ui.close_menu(); self.request(action, &ctx); }
                }
                let mut recent = None;
                ui.menu_button("Open recent", |ui| {
                    if self.recent.paths.is_empty() { ui.weak("No recent projects."); }
                    for path in &self.recent.paths {
                        let name = storage::project_name(path).unwrap_or("Project");
                        if ui.button(name).on_hover_text(path.to_string_lossy().into_owned()).clicked() {
                            recent = Some(path.clone());
                            ui.close_menu();
                        }
                    }
                });
                if let Some(path) = recent { self.request(DocumentAction::OpenRecent(path), &ctx); }
                ui.separator();
                if ui.button("Save").clicked() { ui.close_menu(); self.save(false); }
                if ui.button("Save As…").clicked() { ui.close_menu(); self.save(true); }
                ui.separator();
                for (label, content) in [
                    ("Export diagram", ExportContent::Diagram),
                    ("Export requirements", ExportContent::Requirements),
                ] {
                    ui.menu_button(label, |ui| {
                        let csv = matches!(content, ExportContent::Requirements).then_some(ExportFormat::Csv);
                        for format in ExportFormat::VISUAL.into_iter().chain(csv) {
                            if ui.button(format.label()).clicked() {
                                export = Some((content, format));
                                ui.close_menu();
                            }
                        }
                    });
                }
                ui.menu_button("Export review", |ui| {
                    ui.weak(self.review_scope.map_or_else(|| "Overall system".into(), |id| self.project.subsystem_path(id)));
                    for format in ReviewFormat::ALL {
                        if ui.button(format.label()).clicked() {
                            review_export = Some(format);
                            ui.close_menu();
                        }
                    }
                });
            });
            if let Some((content, format)) = export { self.export_design(content, format); }
            if let Some(format) = review_export { self.export_review(format); }
            if let Some(elapsed) = self.saved_at.map(|time| time.elapsed()) {
                if elapsed < Duration::from_secs(4) {
                    ui.colored_label(ACCENT, "✓ Saved");
                    ctx.request_repaint_after(Duration::from_secs(4) - elapsed);
                }
            }
            if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo")).clicked() { self.step_history(false); }
            if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo")).clicked() { self.step_history(true); }
            ui.separator();
            if !self.welcome {
                self.mode_picker(ui, "active_mode");
                if ui.button("Power Review").clicked() {
                    self.show_power = !self.show_power;
                    self.show_modes = false;
                }
                if ui.button("Manage modes").clicked() {
                    self.show_modes = !self.show_modes;
                    self.show_power = false;
                }
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_outline, "Project outline");
                    let mut inspector = self.show_inspector;
                    if ui.checkbox(&mut inspector, "Inspector").changed()
                        && (inspector || !self.defer_port_exit(PortExit::HideInspector))
                    {
                        self.show_inspector = inspector;
                    }
                });
            }
            if ui.button("Commands").on_hover_text("Ctrl+K").clicked() { self.open_palette(); }
            ui.menu_button("Interfaces", |ui| {
                let block = match self.selected { Selection::Block(id) => Some(id), Selection::Port(id) => self.project.port(id).map(|p|p.block), _ => None };
                for (label,tab) in [("Shared signal contracts",crate::interface_ui::Tab::Contracts),
                    ("Ports and templates",crate::interface_ui::Tab::Ports),("Signal conversion",crate::interface_ui::Tab::Conversion),
                    ("Connector catalog",crate::interface_ui::Tab::Connectors),("Protection review",crate::interface_ui::Tab::Protection)] {
                    if ui.button(label).clicked() { self.open_interface(block, tab); ui.close_menu(); }
                }
            });
            self.help.menu(ui, || {
                #[cfg(target_os = "linux")]
                self.updater.open(&ctx, true);
            });
            #[cfg(target_os = "linux")]
            {
                let label = self.updater.badge().map_or_else(|| "Updates".into(), |version| format!("Update to {version}"));
                if ui.button(label).on_hover_text("Check for Darah updates").clicked() { self.updater.open(&ctx, false); }
            }
            ui.separator();
            ui.label(format!("{}{}", self.project.title, if self.dirty() { " *" } else { "" }));
        });
    }

    fn workspace_tabs(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let attention = self.findings.iter().filter(|finding| matches!(finding.status, Status::Fail | Status::Open | Status::Warning)).count();
            let checks = if attention == 0 { "Design checks".into() } else { format!("Design checks · {attention}") };
            for (workspace, label, width) in [
                (Workspace::Requirements, "Requirements", 145.0),
                (Workspace::Diagram, "Canvas", 112.0),
                (Workspace::Checks, checks.as_str(), 138.0),
            ] {
                let active = self.workspace == workspace;
                let color = if active { ui.visuals().selection.stroke.color } else { ui.visuals().text_color() };
                let mut button = egui::Button::new(RichText::new(label).size(16.0).strong().color(color))
                    .min_size(egui::vec2(width, 34.0));
                if active {
                    button = button.fill(ui.visuals().selection.bg_fill).stroke(egui::Stroke::new(1.0_f32, ACCENT));
                }
                if ui.add(button).clicked() {
                    self.change_workspace(workspace);
                }
            }
        });
        ui.add_space(4.0);
    }

    fn welcome_screen(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(42.0);
            ui.set_max_width(700.0);
            ui.label(RichText::new("DARAH EDA  /  SYSTEM DESIGN").small().strong().color(ACCENT));
            ui.label(RichText::new("Define your system before\ndrawing the schematic.").size(37.0).strong());
            ui.label(RichText::new("Describe what each block needs and provides. Connect the blocks, then inspect the power budget and any open requirements.").size(17.0).color(MUTED));
            ui.add_space(14.0);
            if action_button(ui, "Start guided monitor example") {
                self.request(DocumentAction::Monitor, &ctx);
            }
            ui.weak("Four blocks, one power rail, and a new load that exceeds its limit.");
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Start a blank project").clicked() {
                    self.request(DocumentAction::New, &ctx);
                }
                if ui.button("Open an existing project…").clicked() {
                    self.request(DocumentAction::Open, &ctx);
                }
            });
            ui.add_space(18.0);
            ui.separator();
            for (number, title, description) in [
                ("01", "Add blocks", "Start with functions such as sensing, processing, indicating, or supplying power."),
                ("02", "Connect ports", "Define signal contracts and power needs. Preview a link before applying it."),
                ("03", "Review checks", "Select a result to see what passed, what failed, and which blocks contributed."),
            ] {
                section_title(ui, number, title);
                ui.label(description);
                ui.add_space(4.0);
            }
        });
    }

    fn focus_rail(&mut self, rail: Id) {
        if let Some(index) = self.findings.iter().position(|finding| finding.subject == Subject::Rail(rail)) {
            self.select_issue(index);
        }
    }

    fn guide_strip(&mut self, ui: &mut egui::Ui) {
        let Some(guide) = &self.guide else { return };
        let rail = guide.rail;
        if self.project.port(rail).is_none() {
            self.guide = None;
            return;
        }
        let load = guide.alert.and_then(|id| {
            self.project.block_ports(id).find(|port| port.kind == PortKind::Load).map(|port| port.id)
        });
        let bound = load.is_some_and(|load| {
            self.project.bindings.iter().any(|binding| binding.source == rail && binding.target == load)
        });
        let budget = self.findings.iter().find(|finding| finding.subject == Subject::Rail(rail));
        let passes = budget.is_some_and(|finding| finding.status == Status::Pass);

        ui.horizontal(|ui| {
            ui.label(RichText::new("GUIDED EXAMPLE").small().strong().color(ACCENT));
            if ui.small_button("Dismiss").clicked() { self.guide = None; }
        });
        if self.guide.is_none() { return; }
        if let Some(budget) = budget {
            ui.label(RichText::new(&budget.detail).color(status_color(budget.status)));
        }
        match (load, bound, passes) {
            (None, _, _) => {
                ui.label("1 of 3 · Add a 110 mA alert output to the monitor.");
                if action_button(ui, "Add alert output") {
                    if let Some(alert) = self.edit("Added alert output", Project::add_alert) {
                        if let Some(guide) = &mut self.guide { guide.alert = Some(alert); }
                        self.select(Selection::Block(alert));
                    }
                }
            }
            (Some(load), false, _) => {
                ui.label("2 of 3 · Preview the alert's power connection. Apply it to see the overload in the design checks.");
                if action_button(ui, "Preview alert → 3V3") {
                    self.select(Selection::Port(load));
                    self.connect_from(load);
                    if let Some(picker) = &mut self.picker { picker.target = Some(rail); }
                }
            }
            (Some(_), true, false) => {
                ui.label("3 of 3 · In the Inspector, set Available current to 350 mA and click Apply port details.");
                if ui.button("Show rail and load breakdown").clicked() { self.focus_rail(rail); }
            }
            (Some(_), true, true) => {
                ui.label("Power budget satisfied. The alert's control input is a separate requirement. Find it under Design checks.");
                if ui.button("Show load breakdown").clicked() { self.focus_rail(rail); }
            }
        }
        ui.separator();
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        self.help.show(ctx);
    }

    fn interface_window(&mut self, ctx: &egui::Context) {
        match self.interface_editor.show(ctx,&self.project) {
            Ok(Some(updated)) => {
                let created = self.interface_editor.created_block().filter(|id| updated.block(*id).is_some());
                let selected = created.map(Selection::Block).unwrap_or(self.selected);
                if self.edit("Saved interfaces and block configuration", |project| { *project=updated; Ok(()) }).is_some() {
                    self.welcome = false;
                    self.select_applied(selected);
                    if let Some(block) = created {
                        self.workspace = Workspace::Diagram; self.show_inspector = true;
                        self.diagram.reveal_block(block); ctx.request_repaint();
                    }
                }
            }
            Ok(None) => {},
            Err(error) => self.error=Some(error),
        }
    }

    fn open_palette(&mut self) {
        self.help.close();
        self.palette = Some(Palette { query: String::new(), focus: true });
    }

    fn palette_window(&mut self, ctx: &egui::Context) {
        let Some(mut palette) = self.palette.take() else { return };
        let mut open = true;
        let mut action = None;
        egui::Window::new("Commands and project search").collapsible(false).resizable(false).open(&mut open)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 75.0]).show(ctx, |ui| {
                let response = ui.add(egui::TextEdit::singleline(&mut palette.query)
                    .hint_text("Find a command, block, or port").desired_width(370.0));
                if palette.focus { response.request_focus(); palette.focus = false; }
                let available = [
                    (PaletteAction::AddBlock, "Add block", true),
                    (PaletteAction::Connect, "Connect selected port",
                        self.workspace == Workspace::Diagram && matches!(self.selected, Selection::Port(_))),
                    (PaletteAction::OpenIssues, "Show unresolved checks", !self.welcome),
                    (PaletteAction::Power, "Power review", !self.welcome),
                    (PaletteAction::Modes, "Manage operating modes", !self.welcome),
                ];
                let mut first = None;
                for (command, label, enabled) in available {
                    if enabled && matches_search(label, &palette.query) {
                        first.get_or_insert(command);
                        if ui.button(label).clicked() { action = Some(command); }
                    }
                }
                if !palette.query.trim().is_empty() {
                    let blocks = self.project.blocks.iter().map(|block| {
                        (Selection::Block(block.id), format!("Block · {}", block.name))
                    });
                    let ports = self.project.ports.iter().map(|port| {
                        (Selection::Port(port.id), format!("Port · {}", self.project.port_label(port)))
                    });
                    egui::ScrollArea::vertical().max_height(225.0).show(ui, |ui| {
                        for (kind, label) in Self::block_presets()
                            .map(|(kind, label)| (kind, format!("Add · {label}")))
                            .filter(|(_, label)| palette.query.split_whitespace().all(|word| matches_search(label, word))) {
                            let command = PaletteAction::AddPreset(kind);
                            first.get_or_insert(command);
                            if ui.button(label).clicked() { action = Some(command); }
                        }
                        for (selection, label) in blocks.chain(ports).filter(|(_, label)| matches_search(label, &palette.query)) {
                            let command = PaletteAction::Inspect(selection);
                            first.get_or_insert(command);
                            if ui.button(label).clicked() { action = Some(command); }
                        }
                    });
                    if first.is_none() { ui.weak("No matches."); }
                }
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) { action = action.or(first); }
            });
        match action {
            Some(PaletteAction::AddBlock) => self.new_block(None),
            Some(PaletteAction::AddPreset(kind)) => self.new_block(Some(kind)),
            Some(PaletteAction::Connect) => {
                if let Selection::Port(port) = self.selected { self.connect_from(port); }
            }
            Some(PaletteAction::OpenIssues) => {
                self.open_only = true;
                if let Some(index) = self.findings.iter().position(|f| f.status == Status::Open) {
                    self.issue = Some(index);
                    self.workspace = Workspace::Checks;
                } else { self.status = "No checks need input".into(); }
            }
            Some(PaletteAction::Power) => { self.show_power = true; self.show_modes = false; }
            Some(PaletteAction::Modes) => { self.show_modes = true; self.show_power = false; }
            Some(PaletteAction::Inspect(selection)) => {
                let block = match selection {
                    Selection::Block(id) => Some(id),
                    Selection::Port(id) => self.project.port(id).map(|port| port.block),
                    _ => None,
                };
                if let Some(id) = block {
                    self.workspace = Workspace::Diagram;
                    self.show_inspector = true;
                    self.diagram.reveal_block(id);
                    ctx.request_repaint();
                }
                self.choose(selection);
            }
            _ if open && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) => self.palette = Some(palette),
            _ => {}
        }
    }

    fn add_block_panel(&mut self, ui: &mut egui::Ui) {
        if let Some(draft) = self.place_block.take() {
            let canceled = ui.horizontal_wrapped(|ui| {
                ui.strong(format!("Place {} on the canvas", draft.name));
                ui.button("Cancel placement").clicked()
            }).inner || ui.input(|input| input.key_pressed(egui::Key::Escape));
            if !canceled { self.place_block = Some(draft); }
            ui.separator();
        }
        let Some(mut draft) = self.add_block.take() else { return };
        let mut cancel = ui.input(|input| input.key_pressed(egui::Key::Escape));
        let mut place = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Add block");
                if ui.button("Cancel").clicked() { cancel = true; }
                if draft.kind.is_some() && ui.button("Change preset").clicked() {
                    draft.kind = None;
                    draft.focus = true;
                }
            });
            let Some(kind) = draft.kind else {
                let query = ui.add(egui::TextEdit::singleline(&mut draft.query)
                    .hint_text("Search block types and connector presets"));
                if draft.focus { query.request_focus(); draft.focus = false; }
                let mut first = None;
                egui::ScrollArea::vertical().id_salt("block_preset_results").max_height(145.0).show(ui, |ui| {
                    for (kind, label) in Self::block_presets().filter(|(_, label)| matches_search(label, &draft.query)) {
                        first.get_or_insert(kind);
                        if ui.button(label).clicked() { draft.kind = Some(kind); }
                    }
                    if first.is_none() { ui.weak("No matching presets."); }
                });
                if query.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    draft.kind = draft.kind.or(first);
                }
                if let Some(kind) = draft.kind { draft.name = kind.label().into(); }
                return;
            };
            field(ui, "Name", &mut draft.name);
            let mut ready = !draft.name.trim().is_empty();
            match kind {
                BlockType::Functional(_) => { ui.checkbox(&mut draft.template, "Include starter ports"); }
                BlockType::Connector(index) => {
                    let preset = &interface_catalog::PRESETS[index];
                    ui.label(format!("{} · {}", preset.family, preset.variant));
                    ui.weak(interface_catalog::family_preview(index));
                    ui.horizontal_wrapped(|ui| {
                        egui::ComboBox::from_id_salt("place_connector_exposure")
                            .selected_text(draft.exposure.label()).show_ui(ui, |ui| {
                                for exposure in Exposure::ALL {
                                    ui.selectable_value(&mut draft.exposure, exposure, exposure.label());
                                }
                            });
                        ui.checkbox(&mut draft.incoming, "Into board");
                        if preset.count == 0 {
                            ui.label("Contacts");
                            ui.add(egui::DragValue::new(&mut draft.contacts).range(1..=1024));
                        }
                    });
                }
                BlockType::PowerConverter => {
                    egui::ComboBox::from_id_salt("new_power_topology").height(330.0)
                        .selected_text(draft.power_topology.map_or("Select topology", power::Kind::label))
                        .show_ui(ui, |ui| power_ui::topology_options(ui, &mut draft.power_topology));
                    ui.weak("Set input compatibility and the output rating after placement. Efficiency or ground current goes in the output port's Details.");
                    ready &= draft.power_topology.is_some();
                }
                BlockType::SignalConversion => {
                    egui::ComboBox::from_id_salt("new_block_conversion").selected_text(draft.conversion.label()).show_ui(ui, |ui| {
                        for kind in crate::interfaces::ConversionKind::ALL { ui.selectable_value(&mut draft.conversion, kind, kind.label()); }
                    });
                }
                BlockType::Protection => {
                    egui::ComboBox::from_id_salt("new_block_protection").selected_text(draft.protection.label()).show_ui(ui, |ui| {
                        for kind in ProtectionType::ALL { ui.selectable_value(&mut draft.protection, kind, kind.label()); }
                    });
                    if draft.protection == ProtectionType::Shunt {
                        egui::ComboBox::from_id_salt("new_block_shunt_port").selected_text(draft.attached.and_then(|id| self.project.port(id))
                            .map_or_else(|| "Choose port to protect…".into(), |p| self.project.port_label(p))).show_ui(ui, |ui| {
                                for port in &self.project.ports { ui.selectable_value(&mut draft.attached, Some(port.id), self.project.port_label(port)); }
                            });
                        ready &= draft.attached.is_some_and(|id| self.project.port(id).is_some());
                    }
                }
            }
            place = ui.add_enabled(ready, egui::Button::new("Place on canvas")).clicked();
        });
        if !cancel {
            if place {
                let mut preview = self.project.clone();
                match draft.create(&mut preview) {
                    Ok(block) => {
                        draft.preview_ports = preview.block_ports(block).cloned().collect();
                        self.place_block = Some(draft);
                        ui.ctx().request_repaint();
                    }
                    Err(error) => { self.error = Some(error); self.add_block = Some(draft); }
                }
            } else { self.add_block = Some(draft); }
        }
        ui.separator();
    }

    fn commit_connection(&mut self, from: Id, target: Id) -> bool {
        if let Some(id) = self.edit("Connected ports", |p| p.connect(from, target)) {
            self.select(Selection::Wire(id));
            if let Some(rail) = self.guide.as_ref().map(|guide| guide.rail).filter(|&rail| from == rail || target == rail) {
                self.focus_rail(rail);
            }
            true
        } else {
            false
        }
    }

    fn pending_link_panel(&mut self, ui: &mut egui::Ui) {
        let Some((from, to)) = self.pending_link else { return };
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.pending_link = None;
            return;
        }
        let mut keep = false;
        let mut cancel = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Review proposed connection");
                cancel = ui.button("Cancel").clicked();
            });
            match review::connection_preview(&self.project, self.active_mode, from, to, &self.findings) {
                Ok(findings) => {
                    egui::ScrollArea::vertical().id_salt("pending_link_checks").max_height(115.0).show(ui, |ui| {
                        for finding in findings.iter().filter(|finding| finding.status != Status::Pass) {
                            ui.label(format!("{} · {}", finding.title, finding.detail));
                        }
                    });
                    keep = ui.button("Keep connection for planning").clicked();
                }
                Err(error) => { ui.colored_label(status_color(Status::Fail), error); }
            }
        });
        if cancel || (keep && self.commit_connection(from, to)) { self.pending_link = None; }
    }

    fn connection_window(&mut self, ctx: &egui::Context) {
        let Some(mut picker) = self.picker.take() else { return };
        let Some(port) = self.project.port(picker.from) else { return };
        let mut open = true;
        let mut add = false;
        let source_label = self.project.port_label(port);
        egui::Window::new("Connect port").collapsible(false).open(&mut open).default_width(480.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.strong(source_label);
                let response = ui.add(egui::TextEdit::singleline(&mut picker.query)
                    .hint_text("Search blocks or ports").desired_width(f32::INFINITY));
                if picker.focus { response.request_focus(); picker.focus = false; }
                let mut shown = false;
                egui::ScrollArea::vertical().max_height(175.0).show(ui, |ui| {
                    for other in self.project.ports.iter().filter(|other| {
                        other.kind.power() == port.kind.power() && other.kind.source() != port.kind.source()
                    }) {
                        let label = self.project.port_label(other);
                        if matches_search(&label, &picker.query) {
                            shown = true;
                            if ui.selectable_label(picker.target == Some(other.id), label).clicked() {
                                picker.target = Some(other.id);
                            }
                        }
                    }
                });
                if !shown { ui.weak("No matching ports. Add a port with the opposite role first."); }
                else if picker.target.is_none() { ui.weak("Choose a port above to preview the connection."); }
                if let Some(target) = picker.target {
                    ui.separator();
                    ui.strong("Connection preview");
                    match review::connection_preview(&self.project, self.active_mode, picker.from, target, &self.findings) {
                        Ok(findings) => {
                            for finding in findings {
                                if !matches!(finding.subject, Subject::Binding(_)) { ui.strong(&finding.title); }
                                ui.label(RichText::new(finding.status.label()).color(status_color(finding.status)));
                                ui.label(finding.detail);
                            }
                            add = action_button(ui, "Connect ports");
                        }
                        Err(error) => { ui.colored_label(status_color(Status::Fail), error); }
                    }
                }
            });
        if add && picker.target.is_some_and(|target| self.commit_connection(picker.from, target)) { return; }
        if open && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) { self.picker = Some(picker); }
    }

    fn confirm_window(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending.clone() else { return };
        egui::Window::new("Unsaved project").collapsible(false).resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label("Save changes to this project before continuing?");
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() && self.save(false) { self.run_action(action.clone(), ctx); }
                    if ui.button("Discard").clicked() { self.run_action(action, ctx); }
                    if ui.button("Cancel").clicked() { self.pending = None; }
                });
                if let Some(error) = &self.error { ui.colored_label(status_color(Status::Fail), error); }
            });
    }
}

impl eframe::App for Darah {
    #[cfg(target_os = "linux")]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.updater.restart_on_exit();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "linux")]
        self.updater.tick(ctx);
        if ctx.input(|input|input.viewport().close_requested()) && self.interface_editor.is_open() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.error=Some("Save or discard the interface editor draft before closing Darah.".into());
        } else if ctx.input(|input| input.viewport().close_requested()) && !self.allow_close && (self.dirty() || self.port_dirty()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request(DocumentAction::Close, ctx);
        }
        if self.pending.is_none() && self.port_exit.is_none() && !self.interface_editor.is_open() {
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F1)) {
                self.help.open();
            }
            if self.help.is_open() && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.help.close();
            }
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::K)) {
                self.open_palette();
            }
            let command = egui::Modifiers::COMMAND;
            if ctx.input_mut(|input| input.consume_key(command | egui::Modifiers::SHIFT, egui::Key::S)) {
                self.save(true);
            } else if ctx.input_mut(|input| input.consume_key(command, egui::Key::S)) {
                self.save(false);
            } else if ctx.input_mut(|input| input.consume_key(command, egui::Key::O)) {
                self.request(DocumentAction::Open, ctx);
            } else if ctx.input_mut(|input| input.consume_key(command, egui::Key::N)) {
                self.request(DocumentAction::New, ctx);
            }
            if !ctx.wants_keyboard_input() && !self.help.is_open() {
                if self.picker.is_none() && self.add_block.is_none() && self.place_block.is_none() && self.pending_link.is_none() && self.palette.is_none() && self.workspace == Workspace::Diagram {
                    let blocks = self.diagram.selected_blocks().to_vec();
                    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
                        match self.selected {
                            Selection::Wire(id) => self.apply_diagram_action(DiagramAction::Disconnect(id)),
                            Selection::Boundary(id) => self.apply_diagram_action(DiagramAction::RemoveBoundary(id)),
                            _ if !blocks.is_empty() => self.apply_diagram_action(DiagramAction::Delete(blocks)),
                            _ => {}
                        }
                    } else if !blocks.is_empty()
                        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::D))
                    {
                        self.apply_diagram_action(DiagramAction::Duplicate(blocks));
                    }
                }
                if ctx.input_mut(|input| input.consume_key(command | egui::Modifiers::SHIFT, egui::Key::Z)
                    || input.consume_key(command, egui::Key::Y))
                {
                    self.step_history(true);
                } else if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
                    self.step_history(false);
                }
            }
        }
        let enabled = self.pending.is_none() && self.port_exit.is_none() && self.rename_project.is_none() && !self.interface_editor.is_open();
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_enabled_ui(enabled, |ui| self.toolbar(ui));
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            if let Some(error) = &self.error { ui.colored_label(status_color(Status::Fail), error); }
            else { ui.label(&self.status); }
        });
        egui::TopBottomPanel::top("workspaces").show(ctx, |ui| {
            ui.add_enabled_ui(enabled, |ui| self.workspace_tabs(ui));
        });
        if !self.welcome && self.pending.is_none() && !self.interface_editor.is_open() && self.rename_project.is_none() {
            let protection = if self.port_exit.is_none() { self.next_protection_prompt() } else { None };
            if self.port_exit.is_some() || protection.is_some() {
                egui::TopBottomPanel::top("project_prompts").show(ctx, |ui| self.project_prompts(ui, protection));
            }
        }
        #[cfg(target_os = "linux")]
        if enabled && self.updater.show(ctx) {
            self.request(DocumentAction::ApplyUpdate, ctx);
        }
        if self.welcome {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add_enabled_ui(enabled, |ui| self.welcome_screen(ui));
            });
            if self.pending.is_some() {
                self.confirm_window(ctx);
            } else {
                self.palette_window(ctx);
                if self.add_block.is_some() { self.welcome = false; }
                self.help_window(ctx);
                self.interface_window(ctx);
            }
            if !self.welcome { ctx.request_repaint(); }
            return;
        }
        if self.workspace == Workspace::Diagram {
            if self.show_outline {
                egui::SidePanel::left("outline").default_width(185.0).width_range(135.0..=350.0).show(ctx, |ui| {
                    ui.add_enabled_ui(enabled && self.port_exit.is_none(), |ui| self.outline(ui));
                });
            }
            if self.show_inspector {
                egui::SidePanel::right("inspector").default_width(360.0).width_range(300.0..=560.0).show(ctx, |ui| {
                    ui.add_enabled_ui(enabled && self.port_exit.is_none(), |ui| self.inspector(ui));
                });
            }
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_enabled_ui(enabled && self.port_exit.is_none(), |ui| match self.workspace {
                Workspace::Requirements => self.requirements_workspace(ui),
                Workspace::Checks => self.checks_workspace(ui),
                Workspace::Diagram => {
                    if self.return_to_requirements && ui.button("Back to requirements").clicked() {
                        self.change_workspace(Workspace::Requirements);
                        self.return_to_requirements = false;
                        return;
                    }
                    if self.return_to_check.is_some() && ui.button("Back to design check").clicked() {
                        if let Some((subject, title)) = self.return_to_check.take() {
                            self.issue = self.findings.iter().position(|f| f.subject == subject && f.title == title);
                        }
                        self.change_workspace(Workspace::Checks);
                        return;
                    }
                    self.boundary_panel(ui);
                    self.add_block_panel(ui);
                    self.pending_link_panel(ui);
                    self.guide_strip(ui);
                    let finding = self.issue.and_then(|i| self.findings.get(i));
                    let blocks = finding.map_or(&[][..], |f| f.blocks.as_slice());
                    let ports = finding.map_or(&[][..], |f| f.ports.as_slice());
                    let from = self.picker.as_ref().map(|p| p.from);
                    let preview = self.place_block.as_ref().map(|draft| (draft.name.trim(), draft.preview_ports.as_slice()));
                    if let Some(action) = self.diagram.show(ui, &self.project, self.selected, blocks, ports, from, &self.findings, self.active_mode, preview, self.drawing_boundary.is_some()) {
                        self.apply_diagram_action(action);
                    }
                }
            });
        });
        if self.pending.is_some() { self.confirm_window(ctx); }
        else if self.rename_project.is_some() { self.rename_window(ctx); }
        else if self.port_exit.is_none() {
            self.boundary_window(ctx);
            self.palette_window(ctx);
            self.connection_window(ctx);
            self.modes_window(ctx);
            self.power_window(ctx);
            self.help_window(ctx);
            self.interface_window(ctx);
        }
    }
}

fn scope_picker(ui: &mut egui::Ui, project: &Project, id: &str, value: &mut Option<Id>, root: &str) {
    let selected = value.map_or_else(|| root.to_owned(), |id| project.subsystem_path(id));
    egui::ComboBox::from_id_salt(id).selected_text(selected).show_ui(ui, |ui| {
        ui.selectable_value(value, None, root);
        for subsystem in &project.subsystems {
            ui.selectable_value(value, Some(subsystem.id), project.subsystem_path(subsystem.id));
        }
    });
}

fn section_title(ui: &mut egui::Ui, number: &str, title: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(number).small().strong().color(ACCENT));
        ui.label(RichText::new(title).strong().size(16.0));
    });
}

fn action_button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(egui::Button::new(RichText::new(label).strong()).fill(Color32::from_rgb(25, 93, 89))).clicked()
}

fn field_enter(ui: &egui::Ui, response: egui::Response) -> bool {
    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) -> egui::Response {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).hint_text(label).desired_width(f32::INFINITY))
}

fn role_combo(ui: &mut egui::Ui, id: &str, role: &mut Role) {
    egui::ComboBox::from_id_salt(id).selected_text(role.label()).show_ui(ui, |ui| {
        for choice in Role::ALL { ui.selectable_value(role, choice, choice.label()); }
    });
}

fn unit_picker(ui: &mut egui::Ui, label: &str, symbol: &str, scale: &mut Scale, fields: &mut [String]) -> Result<()> {
    let mut chosen = *scale;
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(label).selected_text(scale.label(symbol)).show_ui(ui, |ui| {
            for option in Scale::ALL {
                ui.selectable_value(&mut chosen, option, option.label(symbol));
            }
        });
    });
    if chosen != *scale {
        let values: Result<Vec<_>> = fields.iter().map(|value| scale.parse(value)).collect();
        for (field, value) in fields.iter_mut().zip(values?) {
            *field = value.map_or_else(String::new, |micro| chosen.number(micro.into()));
        }
        *scale = chosen;
    }
    Ok(())
}

fn matches_search(label: &str, query: &str) -> bool {
    label.to_lowercase().contains(&query.trim().to_lowercase())
}

