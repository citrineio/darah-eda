use crate::model::{Project, Result, SCHEMA_VERSION};
use std::{fs, io::Write, path::{Path, PathBuf}};

pub fn encode(project: &Project) -> Result<String> {
    toml::to_string_pretty(project).map_err(|e| format!("Cannot serialize project: {e}"))
}

pub struct SavedFile {
    pub path: PathBuf,
    bytes: Vec<u8>,
}

impl SavedFile {
    pub fn open(path: &Path) -> Result<(Project, Self)> {
        let path = path.canonicalize().map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
        let bytes = fs::read(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
        let text = std::str::from_utf8(&bytes).map_err(|e| format!("Project must be UTF-8: {e}"))?;
        let mut project = decode(text)?;
        if project.title == crate::model::UNTITLED_PROJECT {
            if let Some(title) = project_name(&path) { project.title = title.to_owned(); }
        }
        Ok((project, Self { path, bytes }))
    }

    pub fn create(path: PathBuf, project: &Project) -> Result<Self> {
        let bytes = encode(project)?.into_bytes();
        write_file(&path, &bytes, None)?;
        Ok(Self { path, bytes })
    }

    pub fn save(&mut self, project: &Project) -> Result<()> {
        let bytes = encode(project)?.into_bytes();
        write_file(&self.path, &bytes, Some(&self.bytes))?;
        self.bytes = bytes;
        Ok(())
    }
}

pub fn project_name(path: &Path) -> Option<&str> {
    let stem = path.file_stem()?.to_str()?;
    let name = stem.strip_suffix(".darah").unwrap_or(stem).trim();
    (!name.is_empty()).then_some(name)
}

fn recent_path() -> Option<PathBuf> {
    let config = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME").filter(|path| Path::new(path).is_absolute()).map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }?;
    Some(config.join("darah-eda").join("recent.toml"))
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct RecentProjects { pub paths: Vec<PathBuf> }

impl RecentProjects {
    pub fn load() -> Self {
        recent_path().and_then(|path| fs::read_to_string(path).ok())
            .and_then(|contents| toml::from_str(&contents).ok()).unwrap_or_default()
    }

    pub fn record(&mut self, path: &Path) {
        let Ok(path) = path.canonicalize() else { return };
        self.paths.retain(|previous| previous != &path);
        self.paths.insert(0, path);
        self.paths.truncate(10);
        if let Some(path) = recent_path() {
            let _ = self.persist(&path);
        }
    }

    fn persist(&self, path: &Path) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let parent = path.parent().ok_or("Missing preferences directory")?;
        fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(toml::to_string(self)?.as_bytes())?;
        file.persist(path)?;
        Ok(())
    }
}

fn decode(text: &str) -> Result<Project> {
    let mut document: toml::Value = toml::from_str(text).map_err(|e| format!("Invalid project TOML: {e}"))?;
    let version = document.get("schema_version").and_then(toml::Value::as_integer);
    match version {
        Some(1) => upgrade_v1(&mut document)?,
        Some(2..=12) => {
            let project = document.as_table_mut().ok_or("Expected a project table.")?;
            project.insert("schema_version".into(), toml::Value::Integer(i64::from(SCHEMA_VERSION)));
        }
        Some(version) if version == i64::from(SCHEMA_VERSION) => {}
        _ => return Err(format!("Unsupported project schema. This build reads schemas 1 through {SCHEMA_VERSION}.")),
    }
    let mut project: Project = document.try_into().map_err(|e| format!("Invalid project fields: {e}"))?;
    if version.is_some_and(|version| version < 10) {
        project.assign_legacy_requirement_references()?;
        project.preserve_legacy_protection_settings();
    }
    project.sync_protection_rails();
    project.validate()?;
    Ok(project)
}

fn upgrade_v1(document: &mut toml::Value) -> Result<()> {
    let project = document.as_table_mut().ok_or("Expected a project table.")?;
    let ports = project.get_mut("ports").and_then(toml::Value::as_array_mut)
        .ok_or("Expected a list of ports in the schema 1 project.")?;
    for entry in ports {
        let port = entry.as_table_mut().ok_or("Expected a port table.")?;
        for (old, new) in [("min_mv", "min_uv"), ("max_mv", "max_uv"), ("current_ma", "current_ua")] {
            if let Some(value) = port.remove(old) {
                let milli = value.as_integer().and_then(|number| u32::try_from(number).ok())
                    .ok_or_else(|| format!("Invalid schema 1 value for {old}."))?;
                if port.insert(new.into(), toml::Value::Integer(i64::from(milli) * 1000)).is_some() {
                    return Err(format!("Project contains both {old} and {new}."));
                }
            }
        }
    }
    project.insert("schema_version".into(), toml::Value::Integer(i64::from(SCHEMA_VERSION)));
    Ok(())
}

pub fn export(path: &Path, report: &str) -> Result<()> {
    export_bytes(path, report.as_bytes())
}

pub fn export_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    write_file(path, bytes, None)
}

fn write_file(path: &Path, contents: &[u8], expected: Option<&[u8]>) -> Result<()> {
    let folder = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let error = |e| format!("Cannot write {}: {e}", path.display());
    let mut staged = tempfile::NamedTempFile::new_in(folder).map_err(error)?;
    staged.write_all(contents).map_err(error)?;
    staged.as_file().sync_all().map_err(error)?;
    if let Some(expected) = expected {
        let current = fs::read(path).map_err(|e| format!("Cannot verify {} before saving: {e}. Use Save As to keep your changes.", path.display()))?;
        if current.as_slice() != expected {
            return Err(format!("{} changed on disk. Use Save As to keep your changes.", path.display()));
        }
        staged.persist(path).map_err(|e| error(e.error))?;
    } else {
        staged.persist_noclobber(path).map_err(|e| {
            if e.error.kind() == std::io::ErrorKind::AlreadyExists {
                format!("{} already exists. Choose a new filename.", path.display())
            } else { error(e.error) }
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interfaces_and_layout_survive_save_and_legacy_schema_eight_migrates() -> Result<()> {
        let mut project=Project::new();
        let block=crate::interface_catalog::create(&mut project,0,4,"USB",true)?;
        let port=project.block_ports(block).find(|p|!p.kind.power()).unwrap().clone();
        let id=project.put_contract(crate::interfaces::Contract::from_port(&port))?;
        project.ports.iter_mut().find(|p|p.id==port.id).unwrap().contract=Some(id);
        project.position_port(port.id,crate::interfaces::PortPosition{edge:crate::interfaces::Edge::Right,slot:4})?;
        project.save_port_template(port.id,"USB signal".into())?;
        let stage=project.add_block(crate::model::Role::Custom,"Divider".into(),false)?;
        let input=project.set_port(crate::model::Port::new(stage,crate::model::PortKind::SignalIn))?;
        let output=project.set_port(crate::model::Port::new(stage,crate::model::PortKind::SignalOut))?;
        let mut channel=crate::interfaces::Channel::new(stage,input,output,crate::interfaces::ConversionKind::Divider);
        channel.id=project.allocate()?;channel.top_ohm=Some(10_000);channel.bottom_ohm=Some(10_000);
        project.interfaces.channels.push(channel);
        let mut protection=crate::interfaces::Protection::new(stage,port.id,"Input TVS");
        protection.id=project.allocate()?;protection.clamp_uv=Some(5_000_000);project.interfaces.protections.push(protection);
        project.validate()?;
        assert_eq!(decode(&encode(&project)?)?,project);
        let copies=project.duplicate_blocks(&[(block,crate::model::BlockPosition{x:500,y:0}),
            (stage,crate::model::BlockPosition{x:750,y:0})])?;
        assert_eq!(project.interfaces.channels.len(),2);
        assert_eq!(project.interfaces.protections.len(),2);
        assert_eq!(project.interfaces.connectors.len(),2);
        project.remove_blocks(&copies);
        project.validate()?;
        assert_eq!(project.interfaces.channels.len(),1);
        assert_eq!(project.interfaces.protections.len(),1);
        assert_eq!(project.interfaces.connectors.len(),1);
        let legacy=Project::monitor()?;
        let text=encode(&legacy)?.replace("schema_version = 13","schema_version = 8");
        assert_eq!(decode(&text)?,legacy);
        Ok(())
    }

    #[test]
    fn project_round_trips_mode_overrides_and_explicit_unknowns() -> Result<()> {
        let project = Project::modes_demo()?;
        assert_eq!(decode(&encode(&project)?)?, project);
        Ok(())
    }

    #[test]
    fn project_round_trips_spec_links_and_rules() -> Result<()> {
        let project = crate::spec::monitor_example()?;
        assert_eq!(decode(&encode(&project)?)?, project);
        Ok(())
    }

    #[test]
    fn manual_wire_route_round_trips_without_changing_endpoints() -> Result<()> {
        let mut project = Project::monitor()?;
        let before = project.bindings[0].clone();
        let bend = crate::model::BlockPosition { x: 275, y: -100 };
        project.bindings[0].bend = Some(bend);
        let reopened = decode(&encode(&project)?)?;
        let binding = reopened.binding(before.id).expect("Saved connection");
        assert_eq!((binding.source, binding.target), (before.source, before.target));
        assert_eq!(binding.bend, Some(bend));
        Ok(())
    }

    #[test]
    fn converter_channels_round_trip() -> Result<()> {
        let project = crate::power::example(true)?;
        assert_eq!(decode(&encode(&project)?)?, project);
        Ok(())
    }

    #[test]
    fn source_settings_round_trip_and_schema_six_still_opens() -> Result<()> {
        let mut project = crate::power::example(false)?;
        let rail = project.ports.iter_mut().find(|port| port.kind == crate::model::PortKind::Rail).unwrap();
        rail.nominal_uv = Some(12_000_000);
        rail.source = Some(crate::rail::Source {
            preset: crate::rail::Preset::Supply12, series: 1, contract_confirmed: false,
        });
        assert_eq!(decode(&encode(&project)?)?, project);
        let legacy = Project::monitor()?;
        let text = encode(&legacy)?.replace("schema_version = 13", "schema_version = 6");
        assert_eq!(decode(&text)?, legacy);
        Ok(())
    }

    #[test]
    fn battery_capacity_and_signal_parameters_round_trip() -> Result<()> {
        let mut project = Project::monitor()?;
        let rail = project.ports.iter_mut().find(|p| p.kind == crate::model::PortKind::Rail).unwrap();
        rail.source = Some(crate::rail::Source { preset: crate::rail::Preset::LiIon, series: 2, contract_confirmed: false });
        rail.battery = Some(crate::electrical::Cell::LiIon18650.default_pack(2));
        let signal = project.ports.iter_mut().find(|p| p.kind == crate::model::PortKind::SignalIn).unwrap();
        signal.signal_spec = Some(crate::signal::Spec { kind: crate::signal::Kind::AnalogVoltage, ..Default::default() });
        signal.signal_spec.as_mut().unwrap().values.insert(crate::signal::Parameter::VoltageMin, -10_000_000);
        assert_eq!(decode(&encode(&project)?)?, project);
        Ok(())
    }

    #[test]
    fn version_two_values_become_base_values() -> Result<()> {
        let project = Project::monitor()?;
        let old = encode(&project)?.replace("schema_version = 13", "schema_version = 2").replace("spec = []\n", "").replace("modes = []\n", "");
        assert_eq!(decode(&old)?, project);
        Ok(())
    }
}

#[cfg(test)]
mod release_tests {
    use super::*;

    #[test]
    fn common_voltage_selection_survives_a_project_round_trip() -> Result<()> {
        let mut project = Project::monitor()?;
        let port = project.ports.iter_mut().find(|port| port.kind == crate::model::PortKind::Rail).unwrap();
        port.voltage_preset = Some(crate::rail::VoltagePreset { nominal_mv: 3300, tolerance_bp: 250 });
        let saved = decode(&encode(&project)?)?;
        assert_eq!(saved, project);
        Ok(())
    }

    #[test]
    fn schema_nine_migrates_stable_requirement_references() -> Result<()> {
        let project = crate::spec::monitor_example()?;
        let mut document: toml::Value = toml::from_str(&encode(&project)?).unwrap();
        let table = document.as_table_mut().unwrap();
        table.insert("schema_version".into(), toml::Value::Integer(9));
        table.remove("next_requirement");
        for key in ["goals", "spec"] {
            for entry in table.get_mut(key).unwrap().as_array_mut().unwrap() {
                entry.as_table_mut().unwrap().remove("reference");
            }
        }
        let old = toml::to_string(&document).unwrap();
        let mut migrated = decode(&old)?;
        let references: Vec<_> = migrated.spec.iter().filter_map(|entry| entry.reference).collect();
        let first = migrated.spec.iter().find(|entry| entry.reference.is_some()).unwrap().clone();
        migrated.remove_spec_entry(first.id);
        let mut new = first;
        new.id = 0;
        let new = migrated.set_spec_entry(new)?;
        assert!(migrated.spec_entry(new).unwrap().reference > references.last().copied());
        assert_eq!(decode(&encode(&migrated)?)?, migrated);
        Ok(())
    }

    #[test]
    fn schema_ten_preserves_requirement_ids_and_legacy_routes() -> Result<()> {
        let mut project = Project::monitor()?;
        project.bindings[0].bend = Some(crate::model::BlockPosition { x: 75, y: 125 });
        let legacy = encode(&project)?.replace("schema_version = 13", "schema_version = 10");
        assert_eq!(decode(&legacy)?, project);

        let size = crate::model::BlockSize { width: 300, height: 150 };
        project.resize_block(project.blocks[0].id, None, Some(size))?;
        project.route_binding(project.bindings[0].id, vec![crate::model::BlockPosition { x: 0, y: 0 },
            crate::model::BlockPosition { x: 100, y: 0 }, crate::model::BlockPosition { x: 100, y: 50 }])?;
        assert_eq!(decode(&encode(&project)?)?, project);
        Ok(())
    }

    #[test]
    fn default_title_uses_project_file_name() {
        assert_eq!(project_name(Path::new("Desk monitor.darah.toml")), Some("Desk monitor"));
        assert_eq!(project_name(Path::new("Prototype.toml")), Some("Prototype"));
    }
}
