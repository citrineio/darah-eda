use crate::model::SCHEMA_VERSION;
use eframe::egui;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Topic {
    #[default] Start, Blocks, Connections, Interfaces, Power, Requirements, Checks, Files, Shortcuts, Changes,
}

impl Topic {
    const ALL: [Self; 10] = [Self::Start, Self::Blocks, Self::Connections, Self::Interfaces, Self::Power,
        Self::Requirements, Self::Checks, Self::Files, Self::Shortcuts, Self::Changes];

    fn title(self) -> &'static str {
        match self {
            Self::Start => "Getting started", Self::Blocks => "Blocks and ports", Self::Connections => "Connections",
            Self::Interfaces => "Interfaces and protection", Self::Power => "Power and batteries", Self::Requirements => "Requirements", Self::Checks => "Design checks",
            Self::Files => "Files and recovery", Self::Shortcuts => "Keyboard shortcuts", Self::Changes => "What's new",
        }
    }

    fn sections(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Start => &[
                ("1. Create or open a project", "Choose Start a blank project, or open an existing .darah.toml file. The guided monitor example is available from the welcome screen and File menu."),
                ("2. Describe the system", "Add functional blocks, then define what each block needs and provides through its ports. Start with editable starter ports if you are unsure."),
                ("3. Connect and review", "Connect outputs to inputs, inspect Design checks, and capture requirements in the Requirements workspace."),
                ("4. Commit and save", "Block names and purposes apply when the field loses focus. Use Apply or Create for the full port form. Ctrl+S saves the committed project to disk."),
            ],
            Self::Blocks => &[
                ("Add a block", "Choose Add block from the Canvas toolbar or context menu. Search functional blocks, conversion, protection, or connector presets. Ctrl+K can also find a specific preset by name. Choose Place on canvas and click where the block belongs."),
                ("Create a connector or conversion stage", "Select a connector preset to preview its signal families. Connector properties contains the signal families and the physical pinout. Signal conversion creates a channel, typed input and output ports, and a supply input for active implementations."),
                ("Create protection", "Choose Protection and select series power, series signal or shunt. Series creates matching input and output ports. Choose an existing port or net for a shunt. After placement, open Configure protection from the Inspector to add ratings or evidence."),
                ("Add or edit ports", "Select a block and choose Add port. Signal outputs provide a named interface; signal inputs consume it. Supply rails provide power; supply inputs declare a block's accepted voltage and current demand. Apply port details commits changes."),
                ("Move and arrange", "Drag a block to move it. Left-drag from empty space to select blocks, or Shift-click to add or remove blocks. Use the diagram layout controls to configure the grid, alignment, and distribution."),
                ("Duplicate or delete", "Right-click a block for its context menu. Ctrl+D duplicates selected blocks; Del deletes them and their associated connections. Undo restores a deleted group."),
            ],
            Self::Connections => &[
                ("Connect ports", "Drag one port dot onto another. Hover to see compatibility and projected power checks. Passing drops connect immediately. A structurally valid link with outstanding checks can be kept for planning or canceled. Use Connect this port to find a target by name."),
                ("Adjust wires", "Select a wire and drag its round handle, or drag a wire segment. Each connection has one adjustable routing point. Reset route restores the automatic route."),
                ("Disconnect", "Select a wire and press Del, or use Disconnect. Select a connected port to inspect its connections. Undo restores a removed connection."),
                ("Signal compatibility", "Use a shared contract or matching legacy names. Electrical signal characteristics describe actual hardware. One output can feed multiple inputs, with aggregate loading reviewed separately. Multi-driver and bidirectional bus behavior remains a manual review."),
            ],
            Self::Interfaces => &[
                ("Open the editor", "Use Interfaces in the toolbar or the block Inspector. Save changes applies the editor draft to the project. Ctrl+S then saves the file. Discard and close leaves the committed project unchanged."),
                ("Shared contracts", "Create a contract from either end or in the Contracts tab. Assign it on a port or to selected ports in the Ports tab. Hardware capabilities stay separate: choosing a 3.3 V contract does not change a 5 V sensor. Shared edits affect every linked port; duplicate a contract for an independent variation."),
                ("Ports and templates", "Create up to 1024 numbered ports at once, optionally numbering physical pins. Save a configured port as a template, duplicate selected ports, or apply a template to a selection. Applying a template copies hardware fields but preserves IDs, names, pin numbers and layout."),
                ("Signal conversion", "Create a converter block and add a channel. Configure actual input/output characteristics and contracts on its ports. Active channels need connected supply inputs. Divider/linear channels can calculate an output envelope from a connected source. Offset is a channel parameter; a distinct offset stage can be its own block."),
                ("Connectors", "Define signal families as diagram ports. The HDMI preset starts with TMDS, CEC, I2C and Power. Use Canvas placement to select an edge and slot, or select a port and drag its square handle on the Canvas. Physical pinout is a separate vertical list. Contact mappings document which signals use which contacts. Review actual per-contact ratings and protection when selecting a part."),
                ("Protection", "Attach shunt protection to the actual net and document its return. Series power protection uses input/output ports and participates in current budgets. Coverage is reviewed per connector pin and fault. Document exceptions. Ratings, bypass paths, grounding and placement require engineering review."),
                ("Port placement", "Select a port and drag its square handle to any block edge on the local 25-unit grid. Drag the round dot to wire. Reset port placement restores automatic positioning. Connections retain their identity and Undo restores the previous position."),
            ],
            Self::Power => &[
                ("Choose a source", "Select a supply rail. Choose Source category, then Source preset. Long dropdowns have a scrollbar and scrolling hint. Presets fix their AC/DC type and polarity; choose Custom source for a different electrical type."),
                ("Define a battery", "Choose Batteries, then Battery chemistry. Cell format / reference is filtered to that chemistry. Set Series and Parallel, then edit the complete pack capacity in mAh or mWh. Switching capacity units uses nominal pack voltage."),
                ("Declare limits", "Enter voltage bounds and available source current or maximum load current. Voltage/current unit selectors support base, milli, and micro units. AC values are RMS; also declare frequency, phases, and voltage reference."),
                ("Build a power tree", "Create a power block with a supply input and output rail. Configure its converter channel on the output rail and connect the input to its feeder. Repeat for cascaded rails or multiple outputs on a PMIC block. Converter outputs are positive DC."),
                ("Review assumptions", "USB-C PD contracts stay open until confirmed. Unknown values stay blank. Three-phase budgets, usable battery energy/runtime, transients, and waveform behavior require additional engineering review."),
            ],
            Self::Requirements => &[
                ("Open its own workspace", "Choose the Requirements tab. This space contains requirements, connector pin mappings, control states, and transitions."),
                ("Create a requirement", "Choose New requirement. Give it a title, source and section, allocation target, criterion, verification method, and evidence reference. Choose Save requirement when finished."),
                ("Choose a criterion", "Manual captures a requirement for engineering verification. Positive DC input-voltage and input-power criteria can compare declared model values. Input-power criteria also need an operating mode and test voltage."),
                ("Use system goals", "New system goal records a higher-level objective and responsible blocks. A block's Inspector also offers Block requirement. A design check result and recorded verification evidence serve different purposes."),
                ("Map behavior and connectors", "Use Connector pins to map a pin function to a logical port. Use Control states and Transitions to record state names, triggers, guards, and actions."),
            ],
            Self::Checks => &[
                ("Read the result", "Satisfied within scope means the declared values pass the modeled comparison. Failed means a declared limit is violated. Unresolved means information, a connection, or an assumption is missing. Manual verification means engineering assessment is required."),
                ("Inspect the evidence", "Select a compact row to see its details on the right. Matching check numbers and a slim selected-row accent show which result is open. Individual results use small status labels in Design checks; the connection Inspector retains colored cards. Both views sort failures first, then warnings, unresolved items, manual reviews and passed checks. Declared inputs follow the results. Hide passed checks filters satisfied results from the main list."),
                ("Read diagram health", "Wires and port dots show their worst related declared result: red failed, yellow warning, amber unresolved, blue manual review, green satisfied within scope. Signal/power labels retain their type colors. Selection adds an outline and preserves health color. Hover or select a wire to see which finding drives its color; a shared endpoint dot can reflect a different branch's failure while each wire retains its own result."),
                ("Find missing power rails", "Required unconnected supply inputs are compared with existing rails using voltage, domain, power type, and AC characteristics. Proposed shared rails are suggestions based on declared requirements."),
                ("Compare operating modes", "Use Manage modes for current overrides and Power Review in the toolbar for rail budgets across modes. Blank current remains unknown rather than being assumed zero."),
                ("Understand the scope", "Darah checks a declared system model. It does not simulate circuits, signal integrity, protocols, or physical verification. Timing, RF parameters, and additional interface fields remain review inputs."),
            ],
            Self::Files => &[
                ("Save a design", "File > Save or Ctrl+S saves committed edits. Use Save As for a separate copy. Project files use TOML and can also be supplied as the application's first command-line argument."),
                ("Undo and redo", "Undo covers up to 64 in-session project edits, including block deletion and connection routing. Undo history is not saved inside the project file."),
                ("Handle external changes", "If the project changed on disk after opening, Darah asks you to use Save As instead of overwriting it. Keep the original when comparing changes from another editor."),
                ("Keep older-build compatibility", "The current build reads project schemas 1 through 13 and saves schema 13. Use Save As when an older Darah build still needs the original file."),
                ("Export a review", "File > Export review writes a Markdown report with checks, declared interface characteristics, requirements, and a project fingerprint. Saving or exporting requires a fresh filename when using Save As or export."),
            ],
            Self::Shortcuts => &[
                ("F1", "Open Help from the welcome screen or any workspace."),
                ("Ctrl+K", "Open commands and search for a block or port."),
                ("Ctrl+S", "Save the project and show a confirmation in the toolbar."),
                ("Ctrl+Shift+S / Ctrl+O / Ctrl+N", "Save As / Open / New."),
                ("Ctrl+Y", "Redo a project edit when a text field is not active."),
                ("Ctrl+Z / Ctrl+Shift+Z", "Undo / redo a project edit when a text field is not active."),
                ("Ctrl+D / Del", "Duplicate selected blocks / delete selected blocks, a selected wire, or a selected subsystem boundary."),
                ("Shift-click", "Add a block to the selection or remove it."),
                ("Mouse navigation", "Right- or middle-drag to pan, including during a left-button drag. Hold the pointer near a canvas edge while dragging to pan automatically. Scroll to zoom. Right-click opens the context menu."),
                ("Escape", "Close Help or About. Text fields retain their usual editing shortcuts. On macOS, use Command for the Ctrl-based application shortcuts."),
            ],
            Self::Changes => &[
                ("v0.7.5 · Design Review export colors", "PDF and HTML Design Review exports use the same green, red, and yellow status colors as Power Review. Whole-design and subsystem reports retain written verdicts, evidence, and status colors across wrapped PDF rows and pages."),
                ("v0.7.4 · Portable Linux AppImage", "AppImage builds include a private glibc runtime and its matching loader, windowing libraries, and the PNG/PDF renderer. The bundled runtime supports systems whose glibc is older than the build host."),
                ("v0.7.3 · Updater validation", "Test release for checking download progress, the optional stable AppImage filename, and restart into the updated build."),
                ("v0.7.2 · AppImage updates and Flatpak packaging", "AppImage updates show download progress and tell you which version is ready to use. You can opt to switch to an unversioned filename. Flatpak builds receive updates through Flatpak."),
                ("v0.7.1 · Complete release notes", "Restored missing entries in What's new and added a test that checks the latest entry against the app version."),
                ("v0.7.0 · Signed Linux updates", "Check for updates from the toolbar. AppImage updates are verified before installation and keep a previous copy for recovery. Debian upgrades are available through the signed APT repository."),
                ("v0.6.3 · Canvas drag reliability", "Auto-panning stops when the pointer leaves the canvas. Canvas drags cancel when the pointer leaves the window or the window loses focus."),
                ("v0.6.2 · Canvas selection and panning", "Box-select blocks, pan with the right or middle button and drag to the canvas edge to scroll. Select a subsystem boundary and press Del to remove its rectangle and grouping while keeping its blocks and wires."),
                ("v0.6.1 · Color in Power Review exports", "PDF and HTML show passed rails in green, failed rails in red, and rails needing more information or review in yellow. Verdict labels remain visible in every format."),
                ("v0.6.0 · Detailed Power Review export", "Export the selected scope and active mode as PDF, HTML or CSV from Power Review. The report includes rail budgets, source values and the equations used to calculate current and nominal watts."),
                ("v0.5.4 · Voltage presets and protection planning", "Selecting a voltage preset or changing its tolerance fills the voltage bounds. Darah remembers the preset per port and derives nominal voltage from the midpoint unless overridden. Protection planning uses the connected supply and connector ratings to suggest values."),
                ("v0.5.3 · Converters and subsystem reviews", "Add dedicated power converters with isolation details. Draw named subsystem boundaries, assign blocks and review boundary interfaces. Export the overall or local review as PDF, HTML, CSV or Markdown."),
                ("v0.5.2 · Nominal power budgets", "Power Review can show DC demand in watts at nominal voltages. The range-based electrical verdict is unchanged, and converter input draw is broken out separately."),
                ("v0.5.1 · Canvas layout and exports", "Drag wire segments and corners, resize blocks on the grid, and export diagrams and requirements from File. Ctrl+S confirms a save in the toolbar."),
                ("v0.5.0 · Port editing and upstream power review", "Place a block with a cursor preview. Use Move ports or Shift-drag a port dot to change its position. External connectors offer recommended protection, and Power Review exposes cascaded demand. The project outline supports renaming, File includes Open recent, and requirements have stable project-local IDs."),
                ("v0.4.6 · Canvas and requirements", "Requirements comes first in the workspace bar. Place blocks from the Canvas, preview wire drops, move selected ports with a separate handle, and edit rail currents directly from Design checks."),
                ("v0.4.5 · Connector signal families", "Connectors use logical signal families on the diagram and list physical contacts separately. Adjust port placement in Connector properties or drag a selected port's square handle."),
                ("v0.4.3 · Specialized blocks in Add block", "Connector, Protection and Signal conversion now appear in the normal Add block selector. Their setup creates the relevant ports/definitions and selects the new block after saving. Shunt protection requires an existing net. Changes remain a discardable draft until saved."),
                ("v0.4.2 · Quieter checks", "Compact Design checks with small status accents and paired check numbers. Individual results sort by severity in both the checks workspace and connection Inspector. The Inspector keeps its colored cards, and diagram health colors are unchanged."),
                ("v0.4.1 · Visible design health", "Individual result colors, paired check numbers and selection borders, and health-colored diagram wires and port dots. Selected failed wires stay red. Hover reveals the related result. Project schema remains 9."),
                ("v0.4.0 · Interfaces and block tools", "Reusable contracts, bulk ports and templates, signal conversion channels, connector catalog and pin mappings, protection warnings, signal fan-out loading review, and draggable edge ports. Project saves now use schema 9."),
                ("v0.3.4 · Help and About", "Searchable in-app help, F1, keyboard-shortcut reference, and About with the compiled app version, build type, platform, project schema, and copyable build information."),
                ("v0.3.3 · Source selection", "Wider source selectors, larger lists, solid scrollbars, battery chemistry filtering, and source-appropriate AC/DC controls."),
                ("v0.3.2 · Desktop and packaging", "The window application ID matches its launcher. Debian builds remember maintainer details and can recover them from a previous package."),
                ("v0.3.0 · Power and interfaces", "Categorized AC/DC source catalog, battery capacity and pack layout, additional conversion types, and typed signal characteristics with interface templates."),
            ],
        }
    }

    fn matches(self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        self.title().to_lowercase().contains(&query)
            || self.sections().iter().any(|(heading, body)| format!("{heading} {body}").to_lowercase().contains(&query))
    }
}

#[derive(Default)]
pub struct Help {
    open: bool,
    about: bool,
    topic: Topic,
    query: String,
    copied: bool,
}

impl Help {
    pub fn is_open(&self) -> bool { self.open || self.about }
    pub fn close(&mut self) { self.open = false; self.about = false; }
    pub fn open(&mut self) { self.open_topic(Topic::Start); }

    fn open_topic(&mut self, topic: Topic) {
        self.topic = topic;
        self.query.clear();
        self.open = true;
        self.about = false;
    }

    pub fn menu(&mut self, ui: &mut egui::Ui, check_updates: impl FnOnce()) {
        ui.menu_button("Help", |ui| {
            #[cfg(target_os = "linux")]
            if !cfg!(feature = "flatpak") && ui.button("Check for Updates…").clicked() {
                check_updates();
                ui.close_menu();
            }
            #[cfg(not(target_os = "linux"))]
            let _ = check_updates;
            for (label, topic) in [("Help topics   F1", Topic::Start), ("Keyboard shortcuts", Topic::Shortcuts), ("What's new", Topic::Changes)] {
                if ui.button(label).clicked() { self.open_topic(topic); ui.close_menu(); }
            }
            ui.separator();
            if ui.button("About Darah EDA").clicked() {
                self.close();
                self.about = true;
                self.copied = false;
                ui.close_menu();
            }
        });
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        let mut open = self.open;
        if open {
            let mut close = false;
            egui::Window::new("Darah Help").open(&mut open).collapsible(false)
                .default_size([760.0, 580.0]).min_width(460.0).min_height(350.0).show(ctx, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("Search help topics and instructions")
                        .desired_width(f32::INFINITY));
                    let topics: Vec<_> = Topic::ALL.into_iter().filter(|topic| topic.matches(&self.query)).collect();
                    if !topics.contains(&self.topic) {
                        if let Some(&topic) = topics.first() { self.topic = topic; }
                    }
                    ui.horizontal_wrapped(|ui| {
                        for &topic in &topics { ui.selectable_value(&mut self.topic, topic, topic.title()); }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.weak("F1 · Help     Escape · Close");
                        if ui.button("Close").clicked() { close = true; }
                    });
                    egui::ScrollArea::vertical().id_salt("help_content")
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                        .auto_shrink([false, false]).show(ui, |ui| {
                            if topics.is_empty() {
                                ui.label("No matching help topic. Try a shorter search, or clear the search field.");
                            } else {
                                ui.heading(self.topic.title());
                                for &(heading, body) in self.topic.sections() {
                                    ui.add_space(8.0);
                                    ui.strong(heading);
                                    ui.label(body);
                                }
                            }
                        });
                });
            self.open = open && !close;
        }
        let mut about = self.about;
        if about {
            let mut close = false;
            egui::Window::new("About Darah EDA").open(&mut about).collapsible(false)
                .default_width(510.0).show(ctx, |ui| {
                    ui.heading("Darah EDA");
                    ui.label(env!("CARGO_PKG_DESCRIPTION"));
                    ui.separator();
                    egui::Grid::new("about_build_info").num_columns(2).spacing([20.0, 8.0]).show(ui, |ui| {
                        for (label, value) in build_fields() { ui.strong(label); ui.label(value); ui.end_row(); }
                    });
                    let authors = env!("CARGO_PKG_AUTHORS");
                    if !authors.is_empty() { ui.label(format!("Authors · {authors}")); }
                    ui.separator();
                    ui.label("Functional system design, interface capture, and requirement integration for electronics.");
                    ui.horizontal(|ui| {
                        if ui.button("Copy build info").clicked() { ctx.copy_text(build_info()); self.copied = true; }
                        if ui.button("Close").clicked() { close = true; }
                        if self.copied { ui.label("Copied"); }
                    });
                });
            self.about = about && !close;
        }
    }
}

fn build_fields() -> [(&'static str, String); 7] {
    [
        ("Version", env!("CARGO_PKG_VERSION").into()),
        ("Build", if cfg!(debug_assertions) { "Debug" } else { "Release" }.into()),
        ("Platform", format!("{} / {}", std::env::consts::OS, std::env::consts::ARCH)),
        ("Project schema", SCHEMA_VERSION.to_string()),
        ("Readable schemas", format!("1–{SCHEMA_VERSION}")),
        ("Interface", "egui / eframe".into()),
        ("Model and files", "Rust / TOML".into()),
    ]
}

fn build_info() -> String {
    let mut text = String::from("Darah EDA\n");
    for (label, value) in build_fields() { text.push_str(&format!("{label}: {value}\n")); }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_search_finds_terms_inside_instructions() {
        assert!(Topic::Power.matches("mAh"));
        assert!(Topic::Requirements.matches("verification"));
        assert!(Topic::Files.matches("  SAVE AS  "));
        assert!(!Topic::Shortcuts.matches("chemistry"));
        assert!(Topic::ALL.into_iter().all(|topic| topic.matches("")));
    }

    #[test]
    fn release_notes_start_with_the_current_version() {
        let (heading, _) = Topic::Changes.sections()[0];
        assert_eq!(
            heading.split_once(" · ").map(|(version, _)| version),
            Some(concat!("v", env!("CARGO_PKG_VERSION"))),
            "Add a What's new entry when changing the app version.",
        );
    }

    #[test]
    fn copied_build_info_uses_compiled_metadata() {
        let info = build_info();
        assert!(info.contains(&format!("Version: {}", env!("CARGO_PKG_VERSION"))));
        assert!(info.contains(&format!("Project schema: {SCHEMA_VERSION}")));
        assert!(info.contains(std::env::consts::ARCH));
    }
}
