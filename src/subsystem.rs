use crate::{
    model::{Binding, BlockPosition, BlockSize, Id, Project, Result},
    review::{Finding, Subject},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subsystem {
    pub id: Id,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<Id>,
    pub position: BlockPosition,
    pub size: BlockSize,
}

impl Project {
    pub fn subsystem(&self, id: Id) -> Option<&Subsystem> {
        self.subsystems.iter().find(|subsystem| subsystem.id == id)
    }

    pub fn scope_contains(&self, scope: Id, mut subsystem: Option<Id>) -> bool {
        for _ in 0..self.subsystems.len() {
            let Some(id) = subsystem else { break; };
            if id == scope { return true; }
            subsystem = self.subsystem(id).and_then(|subsystem| subsystem.parent);
        }
        false
    }

    pub fn block_in_scope(&self, block: Id, scope: Id) -> bool {
        self.block(block).is_some_and(|block| self.scope_contains(scope, block.subsystem))
    }

    pub fn subsystem_path(&self, id: Id) -> String {
        let mut path = Vec::new();
        let mut next = Some(id);
        for _ in 0..self.subsystems.len() {
            let Some(subsystem) = next.and_then(|id| self.subsystem(id)) else { break; };
            path.push(subsystem.name.as_str());
            next = subsystem.parent;
        }
        path.reverse();
        path.join(" / ")
    }

    pub fn add_subsystem(&mut self, name: String, parent: Option<Id>, position: BlockPosition,
        size: BlockSize, enclosed: &[Id]) -> Result<Id>
    {
        let id = self.allocate()?;
        self.subsystems.push(Subsystem { id, name, parent, position, size });
        for block in &mut self.blocks {
            if block.subsystem == parent && enclosed.contains(&block.id) { block.subsystem = Some(id); }
        }
        Ok(id)
    }

    pub fn assign_subsystem(&mut self, block: Id, scope: Option<Id>) -> Result<()> {
        self.blocks.iter_mut().find(|item| item.id == block).ok_or("Block no longer exists.")?.subsystem = scope;
        Ok(())
    }

    pub fn remove_subsystem(&mut self, id: Id) -> Result<()> {
        let parent = self.subsystem(id).ok_or("Subsystem no longer exists.")?.parent;
        for block in &mut self.blocks {
            if block.subsystem == Some(id) { block.subsystem = parent; }
        }
        for subsystem in &mut self.subsystems {
            if subsystem.parent == Some(id) { subsystem.parent = parent; }
        }
        self.subsystems.retain(|subsystem| subsystem.id != id);
        Ok(())
    }

    pub fn boundary_crossings(&self, scope: Id) -> impl Iterator<Item = (&Binding, bool)> {
        self.bindings.iter().filter_map(move |wire| {
            let from = self.port(wire.source).is_some_and(|port| self.block_in_scope(port.block, scope));
            let to = self.port(wire.target).is_some_and(|port| self.block_in_scope(port.block, scope));
            (from != to).then_some((wire, to))
        })
    }

    pub fn finding_in_scope(&self, finding: &Finding, scope: Option<Id>) -> bool {
        let Some(scope) = scope else { return true; };
        let inside = |id| self.port(id).is_some_and(|port| self.block_in_scope(port.block, scope));
        match finding.subject {
            Subject::Block(id) => self.block_in_scope(id, scope),
            Subject::Rail(id) | Subject::Port(id) | Subject::Conversion(id) => inside(id),
            Subject::Binding(id) => self.binding(id).is_some_and(|wire| inside(wire.source) || inside(wire.target)),
            Subject::Spec(_) | Subject::Goal(_) => finding.blocks.iter().any(|&id| self.block_in_scope(id, scope))
                || finding.ports.iter().any(|&id| inside(id)),
        }
    }
}

pub fn validate(project: &Project) -> Result<()> {
    for subsystem in &project.subsystems {
        if subsystem.name.trim().is_empty() || !subsystem.size.valid()
            || subsystem.parent.is_some_and(|parent| project.subsystem(parent).is_none())
            || project.scope_contains(subsystem.id, subsystem.parent)
        {
            return Err("Subsystems need a name, valid bounds, and an acyclic parent hierarchy.".into());
        }
        if project.subsystems.iter().any(|other| other.id != subsystem.id && other.parent == subsystem.parent
            && other.name.trim().eq_ignore_ascii_case(subsystem.name.trim()))
        {
            return Err("Use a unique subsystem name within its parent.".into());
        }
    }
    if project.blocks.iter().any(|block| block.subsystem.is_some_and(|id| project.subsystem(id).is_none())) {
        return Err("A block references a missing subsystem.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::{Port, PortKind, Role}, review::{self, Subject}};

    #[test]
    fn nested_membership_and_crossings_survive_layout_edits() -> Result<()> {
        let mut project = Project::new();
        let source = project.add_block(Role::Power, "Adapter".into(), false)?;
        let consumer = project.add_block(Role::Processing, "Controller".into(), false)?;
        let rail = project.set_port(Port::new(source, PortKind::Rail))?;
        let input = project.set_port(Port::new(consumer, PortKind::Load))?;
        let connection = project.connect(rail, input)?;
        let origin = BlockPosition { x: 0, y: 0 };
        let size = BlockSize { width: 500, height: 300 };
        let system = project.add_subsystem("Controls".into(), None, origin, size, &[consumer])?;
        let child = project.add_subsystem("MCU".into(), Some(system), origin, size, &[consumer])?;
        project.position_block(consumer, Some(BlockPosition { x: 900, y: 900 }))?;
        project.validate()?;
        assert!(project.block_in_scope(consumer, system));
        assert_eq!(project.boundary_crossings(child).map(|(wire, inbound)| (wire.id, inbound)).collect::<Vec<_>>(),
            vec![(connection, true)]);
        let findings = review::evaluate(&project, None);
        assert!(findings.iter().any(|finding| finding.subject == Subject::Binding(connection)
            && project.finding_in_scope(finding, Some(child))));
        assert!(!findings.iter().any(|finding| finding.subject == Subject::Rail(rail)
            && project.finding_in_scope(finding, Some(child))));
        project.remove_subsystem(system)?;
        assert_eq!(project.subsystem(child).unwrap().parent, None);
        assert_eq!(project.block(consumer).unwrap().subsystem, Some(child));
        project.validate()
    }
}
