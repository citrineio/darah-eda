use crate::{
    model::{Binding, Block, BlockPosition, BlockSize, Id, Port, PortKind, Project, Role},
    wires,
};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2};
use std::collections::HashMap;
use crate::{review::{self, Finding, Status, Subject}, status_ui};

pub(crate) fn wire_findings<'a>(binding: &Binding, findings: &'a [Finding]) -> impl Iterator<Item = &'a Finding> {
    let (id, source, target) = (binding.id, binding.source, binding.target);
    findings.iter().filter(move |finding| match finding.subject {
        Subject::Binding(subject) => subject == id,
        _ => finding.ports.contains(&source) || finding.ports.contains(&target),
    })
}

struct Health {
    ports: HashMap<Id, Status>,
    bindings: HashMap<Id, Status>,
}

#[cfg(test)]
mod health_tests {
    use super::*;
    use crate::{model::Result, signal::{self, Parameter}};

    #[test]
    fn failing_fanout_branch_does_not_color_a_compatible_branch_red() -> Result<()> {
        let mut project = Project::new();
        let driver = project.add_block(Role::Input, "Sensor".into(), false)?;
        let a = project.add_block(Role::Processing, "ADC A".into(), false)?;
        let b = project.add_block(Role::Processing, "ADC B".into(), false)?;
        let mut output = Port::new(driver, PortKind::SignalOut);
        output.signal = "Measurement".into();
        output.signal_spec = Some(signal::Spec { kind: signal::Kind::AnalogVoltage, reference: "GND".into(), ..Default::default() });
        output.signal_spec.as_mut().unwrap().values.extend([(Parameter::VoltageMin, 0), (Parameter::VoltageMax, 5_000_000)]);
        let source = project.set_port(output.clone())?;
        let mut input = output; input.id = 0; input.block = a; input.kind = PortKind::SignalIn;
        input.signal_spec.as_mut().unwrap().values.insert(Parameter::VoltageMax, 3_300_000);
        let narrow = project.set_port(input.clone())?;
        input.id = 0; input.block = b;
        input.signal_spec.as_mut().unwrap().values.insert(Parameter::VoltageMax, 5_000_000);
        let wide = project.set_port(input)?;
        let failed = project.connect(source, narrow)?;
        let compatible = project.connect(source, wide)?;
        project.validate()?;
        let findings = crate::review::evaluate(&project, None);
        let health = Health::new(&project, &findings);
        assert_eq!(health.bindings[&failed], Status::Fail);
        assert_ne!(health.bindings[&compatible], Status::Fail);
        assert_eq!(health.ports[&source], Status::Fail);
        assert_ne!(health.ports[&wide], Status::Fail);
        Ok(())
    }
}

impl Health {
    fn new(project: &Project, findings: &[Finding]) -> Self {
        let mut ports = HashMap::<Id, Status>::new();
        let mut direct = HashMap::<Id, Status>::new();
        let merge = |map: &mut HashMap<Id, Status>, id, status| {
            let current = map.entry(id).or_insert(status);
            if status_ui::priority(status) < status_ui::priority(*current) { *current = status; }
        };
        for finding in findings {
            // A binding result belongs to that wire. Propagating it through a
            // shared driver would incorrectly paint every fan-out branch red.
            if let Subject::Binding(id) = finding.subject { merge(&mut direct, id, finding.status); }
            else { for &port in &finding.ports { merge(&mut ports, port, finding.status); } }
        }
        let mut bindings = HashMap::new();
        for binding in &project.bindings {
            let status = [direct.get(&binding.id), ports.get(&binding.source), ports.get(&binding.target)]
                .into_iter().flatten().copied().min_by_key(|status| status_ui::priority(*status));
            if let Some(status) = status { bindings.insert(binding.id, status); }
        }
        // Endpoints show the worst result of their attached branches; wire
        // colors above remain independent for each branch.
        for binding in &project.bindings {
            if let Some(&status) = bindings.get(&binding.id) {
                merge(&mut ports, binding.source, status); merge(&mut ports, binding.target, status);
            }
        }
        Self { ports, bindings }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Selection { Project, Block(Id), Port(Id), Wire(Id), Boundary(Id) }

pub enum DiagramAction {
    Inspect(Selection),
    SelectBlocks(Vec<Id>),
    RemoveBoundary(Id),
    AddBlock,
    PlaceBlock(BlockPosition),
    ResetPortPosition(Id),
    AddPort(Id, PortKind),
    Connect(Id),
    ConnectPorts(Id, Id),
    RouteWire(Id, Vec<BlockPosition>),
    ResizeBlock(Id, Option<BlockPosition>, Option<BlockSize>),
    PlaceBoundary(BlockPosition, BlockSize, Vec<Id>),
    PositionPort(Id, crate::interfaces::PortPosition),
    Disconnect(Id),
    Position(Id, Option<BlockPosition>),
    Arrange(&'static str, Vec<(Id, BlockPosition)>),
    Duplicate(Vec<Id>),
    Delete(Vec<Id>),
}

#[derive(Clone, Copy)]
enum Alignment {
    Left, CenterX, Right, Top, CenterY, Bottom,
}

impl Alignment {
    const OPTIONS: [(Self, &'static str); 6] = [
        (Self::Left, "Align left edges"),
        (Self::CenterX, "Align horizontal centers"),
        (Self::Right, "Align right edges"),
        (Self::Top, "Align top edges"),
        (Self::CenterY, "Align vertical centers"),
        (Self::Bottom, "Align bottom edges"),
    ];

    fn position(self, anchor: Rect, card: Rect) -> BlockPosition {
        let mut point = card.min;
        match self {
            Self::Left => point.x = anchor.left(),
            Self::CenterX => point.x = anchor.center().x - card.width() * 0.5,
            Self::Right => point.x = anchor.right() - card.width(),
            Self::Top => point.y = anchor.top(),
            Self::CenterY => point.y = anchor.center().y - card.height() * 0.5,
            Self::Bottom => point.y = anchor.bottom() - card.height(),
        }
        block_position(point)
    }
}

fn block_position(point: Pos2) -> BlockPosition {
    BlockPosition { x: point.x.round() as i32, y: point.y.round() as i32 }
}

fn snap_to_grid(point: Pos2, spacing: u32) -> Pos2 {
    let spacing = spacing as f32;
    egui::pos2((point.x / spacing).round() * spacing, (point.y / spacing).round() * spacing)
}

fn paint_grid(painter: &egui::Painter, viewport: Rect, origin: Pos2, spacing: f32) {
    let mut step = spacing;
    while step < 8.0 { step *= 5.0; }

    let draw_axis = |start: f32, end: f32, origin: f32, vertical: bool| {
        let mut index = ((start - origin) / step).ceil();
        let mut position = origin + index * step;
        while position <= end {
            let color = if index.rem_euclid(5.0) == 0.0 {
                Color32::from_rgb(52, 69, 83)
            } else {
                Color32::from_rgb(34, 47, 60)
            };
            let stroke = Stroke::new(1.0_f32, color);
            if vertical {
                painter.line_segment([egui::pos2(position, viewport.top()), egui::pos2(position, viewport.bottom())], stroke);
            } else {
                painter.line_segment([egui::pos2(viewport.left(), position), egui::pos2(viewport.right(), position)], stroke);
            }
            index += 1.0;
            position += step;
        }
    };
    draw_axis(viewport.left(), viewport.right(), origin.x, true);
    draw_axis(viewport.top(), viewport.bottom(), origin.y, false);
}

struct BlockDrag {
    id: Id,
    grab_offset: Vec2,
    start: Pos2,
    delta: Vec2,
    members: Vec<(Id, Pos2)>,
}

#[derive(Clone, Copy)]
enum WireDrag {
    Connect(Id),
    Layout { port: Id, position: crate::interfaces::PortPosition },
}

struct RouteDrag {
    id: Id,
    path: Vec<Pos2>,
    handle: wires::Handle,
    grab_offset: Vec2,
    preview: Vec<Pos2>,
}

impl RouteDrag {
    fn new(id: Id, path: Vec<Pos2>, handle: wires::Handle, pointer: Pos2) -> Self {
        let grab_offset = wires::position(&path, handle) - pointer;
        Self { id, preview: path.clone(), path, handle, grab_offset }
    }
}

struct ResizeDrag {
    id: Id,
    start: Rect,
    minimum: Vec2,
    grab_offset: Vec2,
    size: Vec2,
}

struct PanDrag {
    button: egui::PointerButton,
    origin: Pos2,
    last: Pos2,
    moving: bool,
    during_edit: bool,
}

fn edge_pan(viewport: Rect, pointer: Pos2, seconds: f32) -> Vec2 {
    const ZONE: f32 = 48.0;
    const SPEED: f32 = 420.0;
    if !viewport.contains(pointer) { return Vec2::ZERO; }
    let axis = |point: f32, min: f32, max: f32| {
        let zone = ZONE.min((max - min) * 0.5).max(1.0);
        ((min + zone - point) / zone).clamp(0.0, 1.0)
            - ((point - (max - zone)) / zone).clamp(0.0, 1.0)
    };
    egui::vec2(axis(pointer.x, viewport.left(), viewport.right()),
        axis(pointer.y, viewport.top(), viewport.bottom())) * SPEED * seconds.clamp(0.0, 1.0 / 30.0)
}

pub struct Diagram {
    zoom: f32,
    offset: Vec2,
    fit: bool,
    drag: Option<BlockDrag>,
    wire_drag: Option<WireDrag>,
    route_drag: Option<RouteDrag>,
    resize_drag: Option<ResizeDrag>,
    boundary_start: Option<Pos2>,
    pan_drag: Option<PanDrag>,
    primary_press: Option<(Pos2, Pos2)>,
    primary_edit: bool,
    marquee_start: Option<Pos2>,
    context_target: Selection,
    grid_spacing: u32,
    show_grid: bool,
    snap: bool,
    port_layout: bool,
    selected_blocks: Vec<Id>,
    reveal: Option<Id>,
}

impl Default for Diagram {
    fn default() -> Self {
        Self {
            zoom: 1.0, offset: Vec2::ZERO, fit: true, drag: None, wire_drag: None, route_drag: None, resize_drag: None, boundary_start: None,
            pan_drag: None, primary_press: None, primary_edit: false, marquee_start: None,
            context_target: Selection::Project,
            grid_spacing: DEFAULT_GRID, show_grid: true, snap: true, port_layout: false, selected_blocks: Vec::new(), reveal: None,
        }
    }
}

const DEFAULT_GRID: u32 = 25;
const CARD: f32 = 200.0;
const ROW: f32 = 23.0;
const PORT_GRID: f32 = 25.0;
const COLUMN: f32 = 250.0;
const SIGNAL: Color32 = Color32::from_rgb(116, 184, 239);
const POWER: Color32 = Color32::from_rgb(111, 211, 173);

pub(crate) type Card<'a> = (&'a Block, Rect, Vec<&'a Port>);

pub(crate) fn placements(ports: &[&Port]) -> HashMap<Id, crate::interfaces::PortPosition> {
    use crate::interfaces::{Edge, PortPosition};
    let mut result = HashMap::new(); let mut occupied = std::collections::HashSet::new();
    for port in ports.iter().filter(|p| p.layout.is_some()) {
        let mut position = port.layout.unwrap();
        while !occupied.insert((position.edge as u8, position.slot)) { position.slot = position.slot.saturating_add(1); }
        result.insert(port.id, position);
    }
    for port in ports.iter().filter(|p| p.layout.is_none()) {
        let edge = match port.kind { PortKind::SignalIn => Edge::Left, PortKind::SignalOut => Edge::Right, PortKind::Load => Edge::Bottom, PortKind::Rail => Edge::Top };
        let mut slot = 0;
        while !occupied.insert((edge as u8, slot)) { slot += 1; }
        result.insert(port.id, PortPosition { edge, slot });
    }
    result
}
fn card_size(ports: &[&Port]) -> Vec2 {
    use crate::interfaces::Edge;
    let mut width = CARD; let mut height: f32 = 100.0;
    for position in placements(ports).values() {
        match position.edge {
            Edge::Left | Edge::Right => height = height.max(50.0 + PORT_GRID * (f32::from(position.slot) + 1.0)),
            Edge::Top | Edge::Bottom => width = width.max(PORT_GRID * (f32::from(position.slot) + 2.0)),
        }
    }
    egui::vec2(width, height)
}
pub(crate) fn port_point(card: Rect, position: crate::interfaces::PortPosition) -> Pos2 {
    use crate::interfaces::Edge;
    let offset = f32::from(position.slot) * PORT_GRID;
    match position.edge {
        Edge::Left => egui::pos2(card.left(), card.top() + 50.0 + offset),
        Edge::Right => egui::pos2(card.right(), card.top() + 50.0 + offset),
        Edge::Top => egui::pos2(card.left() + PORT_GRID + offset, card.top()),
        Edge::Bottom => egui::pos2(card.left() + PORT_GRID + offset, card.bottom()),
    }
}
fn horizontal_port(position: crate::interfaces::PortPosition) -> bool {
    matches!(position.edge,crate::interfaces::Edge::Top|crate::interfaces::Edge::Bottom)
}
fn dropped_position(card: Rect, point: Pos2) -> crate::interfaces::PortPosition {
    use crate::interfaces::{Edge, PortPosition};
    let edge = [(Edge::Left,(point.x-card.left()).abs()),(Edge::Right,(point.x-card.right()).abs()),
        (Edge::Top,(point.y-card.top()).abs()),(Edge::Bottom,(point.y-card.bottom()).abs())]
        .into_iter().min_by(|a,b|a.1.total_cmp(&b.1)).unwrap().0;
    let offset = if matches!(edge,Edge::Left|Edge::Right) { point.y-card.top()-50.0 } else { point.x-card.left()-PORT_GRID };
    PortPosition { edge, slot: (offset/PORT_GRID).round().clamp(0.0,4095.0) as u16 }
}

fn block_size(block: &Block, ports: &[&Port]) -> Vec2 {
    let minimum = card_size(ports);
    block.size.map_or(minimum, |size| {
        egui::vec2((size.width as f32).max(minimum.x), (size.height as f32).max(minimum.y))
    })
}

pub(crate) fn layout(project: &Project) -> Vec<Card<'_>> {
    let role_order = [Role::Input, Role::Processing, Role::Output, Role::Custom];
    let mut cards = Vec::new();
    let mut extent = Vec2::ZERO;
    let mut x = 0.0_f32;
    for role in role_order.iter().filter(|&&r| project.blocks.iter().any(|b| b.role == r)) {
        let mut column_width = CARD;
        let mut y = DEFAULT_GRID as f32;
        for block in project.blocks.iter().filter(|b| b.role == *role) {
            let ports: Vec<_> = project.block_ports(block.id).collect();
            let size = block_size(block, &ports);
            let height = size.y;
            column_width = column_width.max(size.x);
            let card = Rect::from_min_size(egui::pos2(x, y), size);
            cards.push((block, card, ports));
            extent.x = extent.x.max(card.max.x);
            extent.y = extent.y.max(card.max.y);
            let grid = DEFAULT_GRID as f32;
            y = ((y + height + 45.0) / grid).ceil() * grid;
        }
        x = ((x + column_width + 50.0) / DEFAULT_GRID as f32).ceil() * DEFAULT_GRID as f32;
    }
    let grid = DEFAULT_GRID as f32;
    let power_y = if cards.is_empty() { grid } else { ((extent.y + 85.0) / grid).ceil() * grid };
    let supplies: Vec<_> = project.blocks.iter().filter(|b| b.role == Role::Power).collect();
    let supply_width = if supplies.is_empty() { 0.0 } else { CARD + (supplies.len() - 1) as f32 * COLUMN };
    let supply_x = (((extent.x - supply_width) * 0.5).max(0.0) / grid).round() * grid;
    let mut x = supply_x;
    for block in supplies {
        let ports: Vec<_> = project.block_ports(block.id).collect();
        let size = block_size(block, &ports);
        let card = Rect::from_min_size(egui::pos2(x, power_y), size);
        cards.push((block, card, ports));
        x = ((x + size.x + 50.0) / grid).ceil() * grid;
    }
    for (block, card, _) in &mut cards {
        if let Some(position) = block.position {
            *card = Rect::from_min_size(egui::pos2(position.x as f32, position.y as f32), card.size());
        }
    }
    cards
}

#[derive(Clone, Copy)]
enum Axis { Horizontal, Vertical }

impl Axis {
    fn start(self, card: Rect) -> f32 {
        match self { Self::Horizontal => card.left(), Self::Vertical => card.top() }
    }

    fn size(self, card: Rect) -> f32 {
        match self { Self::Horizontal => card.width(), Self::Vertical => card.height() }
    }

    fn distribute(self, mut cards: Vec<(Id, Rect)>) -> Vec<(Id, BlockPosition)> {
        if cards.len() < 3 { return Vec::new(); }
        cards.sort_by(|a, b| self.start(a.1).total_cmp(&self.start(b.1)).then(a.0.cmp(&b.0)));
        let first = cards[0].1;
        let last = cards[cards.len() - 1].1;
        let occupied: f32 = cards.iter().map(|(_, card)| self.size(*card)).sum();
        let gap = (self.start(last) + self.size(last) - self.start(first) - occupied) / (cards.len() - 1) as f32;
        let mut next = self.start(first) + self.size(first) + gap;
        let middle = cards.len() - 2;
        cards.into_iter().skip(1).take(middle).filter_map(|(id, card)| {
            let mut point = card.min;
            match self { Self::Horizontal => point.x = next, Self::Vertical => point.y = next }
            next += self.size(card) + gap;
            let position = block_position(point);
            (position != block_position(card.min)).then_some((id, position))
        }).collect()
    }
}

pub(crate) fn port_anchors(cards: &[Card<'_>]) -> HashMap<Id, Pos2> {
    let mut anchors = HashMap::new();
    for (_, card, ports) in cards {
        for (id, position) in placements(ports) { anchors.insert(id, port_point(*card, position)); }
    }
    anchors
}

fn nearest_port(anchors: &HashMap<Id, Pos2>, point: Pos2, tolerance: f32, exclude: Option<Id>) -> Option<Id> {
    anchors.iter().filter(|(id, _)| Some(**id) != exclude)
        .map(|(&id, &anchor)| (id, anchor.distance(point)))
        .filter(|(_, distance)| *distance <= tolerance)
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0))).map(|(id, _)| id)
}

pub(crate) fn subsystem_rect(subsystem: &crate::subsystem::Subsystem) -> Rect {
    Rect::from_min_size(egui::pos2(subsystem.position.x as f32, subsystem.position.y as f32),
        egui::vec2(subsystem.size.width as f32, subsystem.size.height as f32))
}

fn encloses(outer: Rect, inner: Rect) -> bool { outer.contains(inner.min) && outer.contains(inner.max) }

pub(crate) fn scope_at_block(project: &Project, id: Id) -> Option<Id> {
    let card = layout(project).into_iter().find(|(block, _, _)| block.id == id)?.1;
    project.subsystems.iter().filter(|scope| encloses(subsystem_rect(scope), card))
        .max_by_key(|scope| project.subsystems.iter().filter(|ancestor| project.scope_contains(ancestor.id, Some(scope.id))).count())
        .map(|scope| scope.id)
}

pub(crate) fn scope_bounds(project: &Project, id: Id) -> Option<(BlockPosition, BlockSize)> {
    let bounds = layout(project).into_iter().filter(|(block, _, _)| project.block_in_scope(block.id, id))
        .map(|(_, rect, _)| rect).chain(project.subsystems.iter()
            .filter(|subsystem| subsystem.id != id && project.scope_contains(id, subsystem.parent)).map(subsystem_rect))
        .reduce(|a, b| a.union(b))?.expand(35.0);
    Some((block_position(bounds.min), BlockSize { width: bounds.width().ceil() as u32, height: bounds.height().ceil() as u32 }))
}

fn dashed_boundary(painter: &egui::Painter, rect: Rect, color: Color32) {
    let clip = painter.clip_rect();
    let stroke = Stroke::new(1.5_f32, color);
    for y in [rect.top(), rect.bottom()] {
        if !(clip.top()..=clip.bottom()).contains(&y) { continue; }
        let mut x = rect.left().max(clip.left());
        let end = rect.right().min(clip.right());
        while x < end {
            painter.line_segment([egui::pos2(x, y), egui::pos2((x + 8.0).min(end), y)], stroke);
            x += 13.0;
        }
    }
    for x in [rect.left(), rect.right()] {
        if !(clip.left()..=clip.right()).contains(&x) { continue; }
        let mut y = rect.top().max(clip.top());
        let end = rect.bottom().min(clip.bottom());
        while y < end {
            painter.line_segment([egui::pos2(x, y), egui::pos2(x, (y + 8.0).min(end))], stroke);
            y += 13.0;
        }
    }
}

pub(crate) fn reanchor_routes(project: &mut Project) {
    if project.bindings.iter().all(|wire| wire.route.is_empty()) { return; }
    let anchors = port_anchors(&layout(project));
    for wire in project.bindings.iter_mut().filter(|wire| !wire.route.is_empty()) {
        let (Some(&from), Some(&to)) = (anchors.get(&wire.source), anchors.get(&wire.target)) else { continue; };
        let path = wires::stored_path(&wire.route, from, to);
        if path.len() >= 2 { wire.route = path.into_iter().map(block_position).collect(); }
    }
}

pub(crate) fn wire_path(project: &Project, anchors: &HashMap<Id, Pos2>, wire: &Binding) -> Option<(Vec<Pos2>, bool)> {
    let from = *anchors.get(&wire.source)?;
    let to = *anchors.get(&wire.target)?;
    let power = project.port(wire.source)?.kind.power();
    let points = if wire.route.is_empty() {
        wires::clean(wires::path(wire.id, from, to, power, wire.bend))
    } else { wires::stored_path(&wire.route, from, to) };
    Some((points, power))
}

fn wire_at(project: &Project, anchors: &HashMap<Id, Pos2>, point: Pos2, tolerance: f32, selected: Selection) -> Option<Id> {
    project.bindings.iter().filter_map(|wire| {
        let (path, _) = wire_path(project, anchors, wire)?;
        let distance = wires::distance(&path, point);
        (distance <= tolerance).then_some((wire.id, distance))
    }).min_by(|a, b| a.1.total_cmp(&b.1)
        .then_with(|| (selected == Selection::Wire(b.0)).cmp(&(selected == Selection::Wire(a.0))))
        .then(a.0.cmp(&b.0))).map(|(id, _)| id)
}

fn add_port_menu(ui: &mut egui::Ui, block: Id) -> Option<DiagramAction> {
    let mut action = None;
    ui.menu_button("Add port", |ui| {
        for kind in PortKind::ALL {
            if ui.button(kind.label()).clicked() {
                action = Some(DiagramAction::AddPort(block, kind));
                ui.close_menu();
            }
        }
    });
    action
}

impl Diagram {
    fn draw_placement(&self, painter: &egui::Painter, viewport: Rect, point: Pos2, name: &str, ports: &[Port]) {
        let ports: Vec<_> = ports.iter().collect();
        let min = viewport.min + self.offset + point.to_vec2() * self.zoom;
        let preview = Rect::from_min_size(min, card_size(&ports) * self.zoom);
        let fill = Color32::from_rgba_unmultiplied(40, 68, 83, 220);
        painter.rect_filled(preview, 6.0, fill);
        painter.rect_stroke(preview, 6.0, Stroke::new(2.0_f32, SIGNAL), StrokeKind::Inside);
        painter.with_clip_rect(preview.intersect(viewport)).text(
            min + egui::vec2(10.0, 10.0), egui::Align2::LEFT_TOP, name,
            FontId::proportional((16.0 * self.zoom).max(10.0)), Color32::WHITE,
        );
        let card = Rect::from_min_size(point, card_size(&ports));
        let positions = placements(&ports);
        for port in ports {
            let anchor = port_point(card, positions[&port.id]);
            let screen = viewport.min + self.offset + anchor.to_vec2() * self.zoom;
            painter.circle_filled(screen, 4.0, if port.kind.power() { POWER } else { SIGNAL });
        }
    }

    pub fn refit(&mut self) { self.fit = true; }

    pub fn keep_view(&mut self) { self.fit = false; }

    pub fn reveal_block(&mut self, id: Id) { self.reveal = Some(id); }

    pub fn reset_view(&mut self) {
        *self = Self {
            grid_spacing: self.grid_spacing, show_grid: self.show_grid, snap: self.snap,
            ..Self::default()
        };
    }

    pub fn select_blocks(&mut self, blocks: Vec<Id>) {
        self.selected_blocks = blocks;
    }

    pub fn selected_blocks(&self) -> &[Id] { &self.selected_blocks }

    pub fn duplicate_positions(&self, project: &Project, ids: &[Id]) -> Vec<(Id, BlockPosition)> {
        let cards = layout(project);
        let selected: Vec<_> = ids.iter().filter_map(|&id| {
            cards.iter().find(|(block, _, _)| block.id == id).map(|(_, rect, _)| (id, rect.min))
        }).collect();
        let right = cards.iter().map(|(_, card, _)| card.right()).fold(0.0_f32, f32::max);
        let left = selected.iter().map(|(_, point)| point.x).fold(f32::INFINITY, f32::min);
        let grid = self.grid_spacing as f32;
        let delta = (((right + grid - left) / grid).ceil() * grid).max(grid);
        selected.into_iter().map(|(id, point)| (id, block_position(point + egui::vec2(delta, 0.0)))).collect()
    }

    fn cancel_pointer_gestures(&mut self) {
        self.drag = None;
        self.wire_drag = None;
        self.route_drag = None;
        self.resize_drag = None;
        self.boundary_start = None;
        self.pan_drag = None;
        self.primary_press = None;
        self.primary_edit = false;
        self.marquee_start = None;
    }

    fn pan_canvas(&mut self, ui: &egui::Ui, response: &egui::Response) -> bool {
        use egui::PointerButton::{Middle, Primary, Secondary};
        let (pointer, pressed, down, secondary_released) = ui.input(|input| {
            let pressed = input.events.iter().find_map(|event| match event {
                egui::Event::PointerButton { pos, button: button @ (Middle | Secondary), pressed: true, .. } =>
                    Some((*button, *pos)),
                _ => None,
            });
            let down = self.pan_drag.as_ref().is_some_and(|pan| input.pointer.button_down(pan.button));
            (input.pointer.interact_pos(), pressed, down, input.pointer.button_released(Secondary))
        });
        if let Some(pan) = &mut self.pan_drag {
            if let Some(point) = pointer {
                if pan.moving || point.distance(pan.origin) >= 4.0 {
                    self.offset += point - pan.last;
                    self.fit = false;
                    pan.moving = true;
                }
                if pan.moving { ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing); }
                if pan.moving { pan.last = point; }
            }
            let suppress = (pan.moving || pan.during_edit) && secondary_released;
            if !down { self.pan_drag = None; }
            return suppress;
        }
        if let Some((button, origin)) = pressed.filter(|(_, origin)| response.rect.contains(*origin)
            && (response.contains_pointer() || self.primary_edit)) {
            let during_edit = self.primary_edit || self.primary_press.is_some();
            self.pan_drag = Some(PanDrag { button, origin, last: origin, moving: false, during_edit });
        }
        secondary_released && (self.primary_edit || ui.input(|input| input.pointer.button_down(Primary)))
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        selection: Selection,
        highlighted_blocks: &[Id],
        highlighted_ports: &[Id],
        connect_from: Option<Id>,
        findings: &[Finding],
        active_mode: Option<Id>,
        block_preview: Option<(&str, &[Port])>,
        drawing_boundary: bool,
    ) -> Option<DiagramAction> {
        let placing_block = block_preview.is_some();
        let health = Health::new(project, findings);
        self.selected_blocks.retain(|&id| project.block(id).is_some());
        let mut alignment = None;
        let mut distribution = None;
        let mut toolbar_action = None;
        let mut snap_selection = false;
        ui.horizontal_wrapped(|ui| {
            if ui.button("+ Add block").clicked() { toolbar_action = Some(DiagramAction::AddBlock); }
            if ui.button("Fit view").clicked() { self.fit = true; }
            if ui.selectable_label(self.port_layout, "Move ports")
                .on_hover_text("Drag a port dot to reposition it on the block edge. Turn off to draw wires.").clicked()
            {
                self.port_layout = !self.port_layout;
            }
            ui.weak(format!("{:.0}%", self.zoom * 100.0));
            ui.colored_label(SIGNAL, "Signal labels");
            ui.colored_label(POWER, "Power labels");
            ui.separator();
            ui.menu_button("Grid", |ui| {
                ui.checkbox(&mut self.show_grid, "Show grid");
                ui.checkbox(&mut self.snap, "Snap while dragging");
                ui.label("Spacing (canvas units)");
                for spacing in [10, 25, 50, 100] {
                    ui.radio_value(&mut self.grid_spacing, spacing, spacing.to_string());
                }
                if ui.add_enabled(!self.selected_blocks.is_empty(), egui::Button::new("Snap selected to grid")).clicked() {
                    snap_selection = true;
                    ui.close_menu();
                }
            });
            ui.add_enabled_ui(self.selected_blocks.len() >= 2, |ui| {
                ui.menu_button("Align", |ui| {
                    ui.weak("The purple block stays in place as the reference.");
                    for (choice, label) in Alignment::OPTIONS {
                        if ui.button(label).clicked() {
                            alignment = Some(choice);
                            ui.close_menu();
                        }
                    }
                });
            });
            ui.add_enabled_ui(self.selected_blocks.len() >= 3, |ui| {
                ui.menu_button("Distribute", |ui| {
                    ui.weak("Equal gaps between blocks. Outer blocks stay in place.");
                    for (axis, label) in [(Axis::Horizontal, "Horizontally"), (Axis::Vertical, "Vertically")] {
                        if ui.button(label).clicked() {
                            distribution = Some(axis);
                            ui.close_menu();
                        }
                    }
                });
            });
            if !self.selected_blocks.is_empty() {
                if ui.button("Duplicate").on_hover_text("Duplicate selected blocks · Ctrl+D").clicked() {
                    toolbar_action = Some(DiagramAction::Duplicate(self.selected_blocks.clone()));
                }
                if ui.button("Delete").on_hover_text("Delete selected blocks and their connections · Del").clicked() {
                    toolbar_action = Some(DiagramAction::Delete(self.selected_blocks.clone()));
                }
            }
            if self.selected_blocks.len() >= 2 {
                ui.weak(format!("{} selected", self.selected_blocks.len()));
            }
        });
        ui.horizontal_wrapped(|ui| {
            match selection {
                Selection::Block(id) => {
                    if let Some(block) = project.block(id) {
                        if self.selected_blocks.len() <= 1 { ui.strong(&block.name); }
                        if let Some(action) = add_port_menu(ui, id) { toolbar_action = Some(action); }
                    }
                }
                Selection::Port(id) => {
                    if let Some(port) = project.port(id) { ui.strong(project.port_label(port)); }
                    if ui.button("Connect…").clicked() { toolbar_action = Some(DiagramAction::Connect(id)); }
                    if project.port(id).is_some_and(|port| port.layout.is_some())
                        && ui.button("Reset port placement").clicked()
                    {
                        toolbar_action = Some(DiagramAction::ResetPortPosition(id));
                    }
                }
                Selection::Boundary(id) => {
                    if let Some(scope) = project.subsystem(id) { ui.strong(&scope.name); }
                    if ui.button("Remove boundary").clicked() { toolbar_action = Some(DiagramAction::RemoveBoundary(id)); }
                }
                Selection::Wire(id) => {
                    ui.strong("Connection");
                    if ui.button("Disconnect").clicked() { toolbar_action = Some(DiagramAction::Disconnect(id)); }
                    if project.binding(id).is_some_and(|wire| wire.bend.is_some() || !wire.route.is_empty())
                        && ui.button("Reset route").clicked()
                    {
                        toolbar_action = Some(DiagramAction::RouteWire(id, Vec::new()));
                    }
                }
                _ => {}
            }
            if self.port_layout { ui.weak("Drag any port dot to move it along a block edge."); }
            else { ui.weak("Drag a port dot to connect. Shift-drag the dot or drag its square to move it."); }
        });
        ui.horizontal_wrapped(|ui| {
            ui.weak("Wire / dot health:");
            for (status, label) in [(Status::Fail, "Failed"), (Status::Warning, "Warning"),
                (Status::Open, "Needs input"), (Status::Manual, "Manual review"), (Status::Pass, "Within scope")] {
                ui.colored_label(status_ui::color(status), label);
            }
            ui.weak("Left-drag empty space to select blocks. Shift-click adds or removes blocks. Right- or middle-drag to pan.");
        });
        let (response, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
        let viewport = response.rect;
        let canvas_input = ui.is_enabled() && ui.input(|input| {
            input.focused && !input.events.iter().any(|event| {
                matches!(event, egui::Event::PointerGone | egui::Event::WindowFocused(false))
            })
        });
        let suppress_secondary = if canvas_input {
            self.pan_canvas(ui, &response)
        } else {
            self.cancel_pointer_gestures();
            true
        };
        let placement_point = block_preview.and_then(|_| {
            response.interact_pointer_pos().filter(|_| response.clicked()).or_else(|| response.hover_pos())
        }).map(|pointer| {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            let world = (pointer - viewport.min - self.offset) / self.zoom;
            let point = egui::pos2(world.x, world.y);
            if self.snap { snap_to_grid(point, self.grid_spacing) } else { point }
        });
        let placement = placement_point.filter(|_| canvas_input && response.clicked())
            .map(|point| DiagramAction::PlaceBlock(block_position(point)));
        let visuals = ui.visuals();
        painter.rect_filled(viewport, 8.0, visuals.extreme_bg_color);
        if project.blocks.is_empty() && project.subsystems.is_empty() && !drawing_boundary {
            if self.show_grid {
                paint_grid(&painter, viewport, viewport.min + self.offset, self.grid_spacing as f32 * self.zoom);
            }
            painter.text(viewport.center(), egui::Align2::CENTER_CENTER,
                if placing_block { "Click to place the block" } else { "Add a block to start your design" },
                FontId::proportional(16.0), visuals.text_color().gamma_multiply(0.7));
            if let (Some((name, ports)), Some(point)) = (block_preview, placement_point) {
                self.draw_placement(&painter, viewport, point, name, ports);
            }
            if self.fit {
                self.offset = Vec2::ZERO;
                self.zoom = 1.0;
                self.fit = false;
            }
            return toolbar_action.or(placement).or_else(|| if placing_block || suppress_secondary {
                None
            } else { self.context_menu(&response, project, None) });
        }

        let mut cards = layout(project);
        let bounds = cards.iter().map(|(_, card, _)| *card)
            .chain(project.subsystems.iter().map(subsystem_rect))
            .reduce(|bounds, rect| bounds.union(rect)).unwrap_or(Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 200.0)));
        let initial_anchors = port_anchors(&cards);
        let bounds = project.bindings.iter().filter_map(|wire| wire_path(project, &initial_anchors, wire))
            .flat_map(|(points, _)| points).fold(bounds, |bounds, point| {
                bounds.union(Rect::from_min_max(point, point))
            });
        let arrangement = if let Some(axis) = distribution {
            let selected = cards.iter().filter(|(block, _, _)| self.selected_blocks.contains(&block.id))
                .map(|(block, card, _)| (block.id, *card)).collect();
            let positions = axis.distribute(selected);
            (!positions.is_empty()).then_some(DiagramAction::Arrange("Distributed blocks", positions))
        } else if alignment.is_some() || snap_selection {
            let anchor = self.selected_blocks.first()
                .and_then(|id| cards.iter().find(|(block, _, _)| block.id == *id))
                .map(|(_, card, _)| *card);
            let positions: Vec<_> = cards.iter().filter_map(|(block, card, _)| {
                if !self.selected_blocks.contains(&block.id) { return None; }
                let position = if snap_selection {
                    block_position(snap_to_grid(card.min, self.grid_spacing))
                } else {
                    alignment?.position(anchor?, *card)
                };
                (position != block_position(card.min)).then_some((block.id, position))
            }).collect();
            let message = if snap_selection { "Snapped blocks to grid" } else { "Aligned blocks" };
            (!positions.is_empty()).then_some(DiagramAction::Arrange(message, positions))
        } else {
            None
        };
        if self.fit && !placing_block && !drawing_boundary {
            let width = (viewport.width() - 52.0).max(1.0);
            let height = (viewport.height() - 52.0).max(1.0);
            self.zoom = (width / bounds.width()).min(height / bounds.height()).clamp(0.05, 1.0);
            self.offset = viewport.size() * 0.5 - bounds.center().to_vec2() * self.zoom;
            self.fit = false;
        }

        if let Some(id) = self.reveal.take() {
            if let Some((_, card, _)) = cards.iter().find(|(block, _, _)| block.id == id) {
                self.offset = viewport.size() * 0.5 - card.center().to_vec2() * self.zoom;
            }
        }

        let primary = egui::PointerButton::Primary;
        let world_at = |point: Pos2, offset: Vec2, zoom: f32| {
            let world = (point - viewport.min - offset) / zoom;
            egui::pos2(world.x, world.y)
        };
        let (pointer, primary_down, primary_released, pressed) = ui.input(|input| {
            let pressed = input.events.iter().find_map(|event| match event {
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. } => Some(*pos),
                _ => None,
            });
            (input.pointer.interact_pos(), input.pointer.button_down(primary),
                input.pointer.button_released(primary), pressed)
        });
        let cancel_drag = !canvas_input || ui.input(|input| input.key_pressed(egui::Key::Escape));
        if let Some(pressed) = pressed.filter(|pos| !cancel_drag && viewport.contains(*pos) && response.contains_pointer()) {
            self.primary_press = Some((pressed, world_at(pressed, self.offset, self.zoom)));
            self.primary_edit = false;
        }
        let drag_press = self.primary_press
            .filter(|(screen, _)| (primary_down || primary_released) && !cancel_drag
                && pointer.is_some_and(|point| point.distance(*screen) > 6.0))
            .map(|(_, world)| world);
        if drag_press.is_some() {
            self.primary_press = None;
            self.primary_edit = !placing_block;
        }
        if drawing_boundary {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            if let Some(press) = drag_press {
                self.boundary_start = Some(snap_to_grid(press, self.grid_spacing));
            }
        } else { self.boundary_start = None; }
        if cancel_drag { self.cancel_pointer_gestures(); }
        let selected_path = if let Selection::Wire(id) = selection {
            project.binding(id).and_then(|wire| wire_path(project, &initial_anchors, wire))
                .map(|(path, _)| (id, path))
        } else { None };
        let resize_handle = if let Selection::Block(id) = selection {
            cards.iter().find(|(block, _, _)| block.id == id).map(|(block, card, ports)| (block.id, *card, card_size(ports)))
        } else { None };
        let layout_handle = match selection {
            Selection::Port(id) => project.port(id).and_then(|port| {
                let (_, card, ports) = cards.iter().find(|(block, _, _)| block.id == port.block)?;
                let position = *placements(ports).get(&id)?;
                use crate::interfaces::Edge;
                let inward = match position.edge {
                    Edge::Left => egui::vec2(17.0, 0.0),
                    Edge::Right => egui::vec2(-17.0, 0.0),
                    Edge::Top => egui::vec2(0.0, 17.0),
                    Edge::Bottom => egui::vec2(0.0, -17.0),
                };
                Some((id, port_point(*card, position) + inward / self.zoom, position))
            }),
            _ => None,
        };
        if let Some(world) = drag_press.filter(|_| !placing_block && !drawing_boundary) {
            if let Some((id, card, minimum)) = resize_handle.filter(|(_, card, _)| card.max.distance(world) <= 10.0 / self.zoom) {
                self.resize_drag = Some(ResizeDrag {
                    id, start: card, minimum,
                    grab_offset: card.max - world, size: card.size(),
                });
            } else if let Some((port, _, position)) = layout_handle.filter(|&(_, point, _)| point.distance(world) <= 7.0 / self.zoom) {
                self.wire_drag = Some(WireDrag::Layout { port, position });
            } else if let Some(port) = nearest_port(&initial_anchors, world, 9.0 / self.zoom, None) {
                let move_port = self.port_layout || ui.input(|input| input.modifiers.shift);
                self.wire_drag = Some(if move_port {
                    let (_, card, ports) = cards.iter().find(|(_, _, ports)| ports.iter().any(|p| p.id == port))
                        .expect("A rendered port has a block");
                    WireDrag::Layout { port, position: placements(ports).get(&port).copied()
                        .unwrap_or_else(|| dropped_position(*card, world)) }
                } else { WireDrag::Connect(port) });
            } else if let Some((id, path, handle)) = selected_path.as_ref().and_then(|(id, path)| {
                let handle = wires::hit(path, world, 8.0 / self.zoom)?;
                matches!(handle, wires::Handle::Corner(_)).then_some((*id, path, handle))
            }) {
                self.route_drag = Some(RouteDrag::new(id, path.clone(), handle, world));
            } else if let Some((block, card, _)) = cards.iter().rev().find(|(_, card, _)| card.contains(world)) {
                let members = if self.selected_blocks.contains(&block.id) {
                    cards.iter().filter(|(member, _, _)| self.selected_blocks.contains(&member.id))
                        .map(|(member, rect, _)| (member.id, rect.min)).collect()
                } else {
                    vec![(block.id, card.min)]
                };
                self.drag = Some(BlockDrag { id: block.id, grab_offset: card.min - world, start: card.min, delta: Vec2::ZERO, members });
            } else if let Some(id) = wire_at(project, &initial_anchors, world, 7.0 / self.zoom, selection) {
                if let Some((path, handle)) = project.binding(id)
                    .and_then(|wire| wire_path(project, &initial_anchors, wire))
                    .and_then(|(path, _)| {
                        let handle = wires::hit(&path, world, 8.0 / self.zoom)?;
                        Some((path, handle))
                    })
                {
                    self.route_drag = Some(RouteDrag::new(id, path, handle, world));
                }
            } else {
                self.marquee_start = Some(world);
            }
        }
        let released = primary_released && !cancel_drag && pointer.is_some_and(|point| viewport.contains(point));
        if primary_down && self.primary_edit && self.pan_drag.is_none() {
            if let Some(point) = pointer {
                let pan = edge_pan(viewport, point, ui.input(|input| input.stable_dt));
                self.offset += pan;
                if pan != Vec2::ZERO {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
                }
            }
        }
        let mut marquee_action = None;
        if let Some(start) = self.marquee_start {
            if let Some(point) = pointer {
                let end = world_at(point, self.offset, self.zoom);
                let bounds = Rect::from_two_pos(start, end);
                if released {
                    let additive = ui.input(|input| input.modifiers.shift || input.modifiers.command);
                    let mut blocks = if additive { self.selected_blocks.clone() } else { Vec::new() };
                    for (block, card, _) in &cards {
                        if bounds.intersects(*card) && !blocks.contains(&block.id) { blocks.push(block.id); }
                    }
                    marquee_action = Some(DiagramAction::SelectBlocks(blocks));
                }
            }
            if !primary_down { self.marquee_start = None; }
        }
        if let Some(WireDrag::Layout { port, position }) = &mut self.wire_drag {
            if let Some(pointer) = pointer {
                if let Some((_,card,_)) = cards.iter().find(|(block,_,_)| project.port(*port).is_some_and(|p|p.block==block.id)) {
                    *position = dropped_position(*card,world_at(pointer,self.offset,self.zoom));
                }
            }
        } else if let Some(drag) = &mut self.route_drag {
            if let Some(pointer) = pointer {
                let point = world_at(pointer, self.offset, self.zoom) + drag.grab_offset;
                let point = if self.snap { snap_to_grid(point, self.grid_spacing) } else { egui::pos2(point.x.round(), point.y.round()) };
                drag.preview = wires::drag(&drag.path, drag.handle, point);
            }
        } else if let Some(drag) = &mut self.resize_drag {
            if let Some(pointer) = pointer {
                let extent = world_at(pointer, self.offset, self.zoom) + drag.grab_offset - drag.start.min;
                let grid = self.grid_spacing as f32;
                let dimension = |value: f32, min: f32| {
                    (value / grid).round().clamp((min / grid).ceil(), (BlockSize::LIMIT as f32 / grid).floor()) * grid
                };
                drag.size = egui::vec2(dimension(extent.x, drag.minimum.x), dimension(extent.y, drag.minimum.y));
            }
        } else if let Some(drag) = &mut self.drag {
            if let Some(pointer) = pointer {
                let point = world_at(pointer, self.offset, self.zoom) + drag.grab_offset;
                let point = if self.snap { snap_to_grid(point, self.grid_spacing) } else { point };
                drag.delta = point - drag.start;
            }
        } else if canvas_input && !primary_down && !placing_block && !drawing_boundary && self.pan_drag.is_none() && response.hovered() {
            let delta = ui.input(|input| input.smooth_scroll_delta.y);
            if let Some(pointer) = response.hover_pos().filter(|_| delta != 0.0) {
                let focus = pointer - viewport.min;
                let next = (self.zoom * (delta * 0.003).exp()).clamp(0.05, 2.8);
                self.offset = focus - (focus - self.offset) * (next / self.zoom);
                self.zoom = next;
            }
        }
        let wire_drag = self.wire_drag;
        let drop_port = match (wire_drag, pointer) {
            (Some(WireDrag::Connect(from)), Some(pointer)) if viewport.contains(pointer) =>
                nearest_port(&initial_anchors, world_at(pointer, self.offset, self.zoom), 12.0 / self.zoom, Some(from)),
            _ => None,
        };
        let wire_action = if released {
            match wire_drag {
                Some(WireDrag::Connect(from)) => drop_port.map(|to| DiagramAction::ConnectPorts(from, to)),
                Some(WireDrag::Layout { port, position }) => Some(DiagramAction::PositionPort(port, position)),
                None => None,
            }
        } else { None };
        if !primary_down && self.wire_drag.take().is_some() { ui.ctx().request_repaint(); }

        let route_action = if !primary_down || cancel_drag {
            self.route_drag.take().filter(|drag| released && !cancel_drag && drag.preview != drag.path).and_then(|drag| {
                let points = wires::clean(drag.preview.into_iter().map(|point| egui::pos2(point.x.round(), point.y.round())));
                (points.len() >= 2).then(|| DiagramAction::RouteWire(drag.id, points.into_iter().map(block_position).collect()))
            })
        } else { None };
        let resize_action = if !primary_down || cancel_drag {
            self.resize_drag.take().filter(|drag| released && !cancel_drag && drag.size != drag.start.size()).map(|drag| {
                DiagramAction::ResizeBlock(drag.id, Some(block_position(drag.start.min)),
                    Some(BlockSize { width: drag.size.x as u32, height: drag.size.y as u32 }))
            })
        } else { None };
        let mut moved = None;
        if released {
            if let Some(drag) = self.drag.take() {
                let positions: Vec<_> = drag.members.iter().filter_map(|&(id, start)| {
                    let position = block_position(start + drag.delta);
                    (position != block_position(start)).then_some((id, position))
                }).collect();
                if !positions.is_empty() {
                    let message = if positions.len() == 1 { "Moved block" } else { "Moved blocks" };
                    moved = Some(DiagramAction::Arrange(message, positions));
                }
            }
        }
        if !primary_down {
            self.drag = None;
            self.primary_press = None;
        }
        let active_drag = self.drag.as_ref().map(|drag| drag.id);
        if let Some(drag) = &self.drag {
            for (block, card, _) in &mut cards {
                if drag.members.iter().any(|&(id, _)| id == block.id) {
                    *card = card.translate(drag.delta);
                }
            }
            cards.sort_by_key(|(block, _, _)| block.id == drag.id);
        }
        if let Some(drag) = &self.resize_drag {
            if let Some((_, card, _)) = cards.iter_mut().find(|(block, _, _)| block.id == drag.id) {
                *card = Rect::from_min_size(drag.start.min, drag.size);
            }
        }
        let (offset, zoom) = (self.offset, self.zoom);
        if self.show_grid {
            paint_grid(&painter, viewport, viewport.min + offset, self.grid_spacing as f32 * zoom);
        }
        let screen = |p: Pos2| viewport.min + offset + p.to_vec2() * zoom;
        let mut boundaries: Vec<_> = project.subsystems.iter().collect();
        boundaries.sort_by_key(|scope| project.subsystems.iter()
            .filter(|ancestor| project.scope_contains(ancestor.id, scope.parent)).count());
        let primary_click = !cancel_drag && response.clicked() && !self.primary_edit;
        let click = (primary_click || response.secondary_clicked() && !suppress_secondary)
            .then(|| response.interact_pointer_pos()).flatten();
        let mut boundary_hit = None;
        for scope in boundaries {
            let rect = subsystem_rect(scope);
            let rect = Rect::from_min_max(screen(rect.min), screen(rect.max));
            let color = if selection == Selection::Boundary(scope.id) { SIGNAL } else { Color32::from_rgb(109, 168, 158) };
            dashed_boundary(&painter, rect, color);
            let title = painter.text(rect.min + egui::vec2(7.0, 5.0), egui::Align2::LEFT_TOP, &scope.name, FontId::proportional(13.0), color);
            if click.is_some_and(|point| title.expand(4.0).contains(point)
                || rect.expand(6.0).contains(point) && !rect.shrink(6.0).contains(point))
            {
                boundary_hit = Some(Selection::Boundary(scope.id));
            }
        }
        let mut boundary_action = None;
        if let (Some(start), Some(pointer)) = (self.boundary_start, pointer) {
            let end = snap_to_grid(world_at(pointer, offset, zoom), self.grid_spacing);
            let rect = Rect::from_min_max(egui::pos2(start.x.min(end.x), start.y.min(end.y)),
                egui::pos2(start.x.max(end.x), start.y.max(end.y)));
            dashed_boundary(&painter, Rect::from_min_max(screen(rect.min), screen(rect.max)), SIGNAL);
            if !primary_down {
                self.boundary_start = None;
                if released && rect.width() >= 50.0 && rect.height() >= 50.0 {
                    let members = cards.iter().filter(|(_, card, _)| encloses(rect, *card)).map(|(block, _, _)| block.id).collect();
                    let size = BlockSize { width: rect.width() as u32, height: rect.height() as u32 };
                    boundary_action = Some(DiagramAction::PlaceBoundary(block_position(rect.min), size, members));
                }
            }
        }
        let mut anchors = if self.drag.is_some() || self.resize_drag.is_some() { port_anchors(&cards) } else { initial_anchors };
        if let Some(WireDrag::Layout { port, position }) = wire_drag {
            if let Some((_,card,_)) = cards.iter().find(|(block,_,_)| project.port(port).is_some_and(|p|p.block==block.id)) {
                anchors.insert(port,port_point(*card,position));
            }
        }
        let selected_color = SIGNAL;
        let focus_color = Color32::from_rgb(250, 193, 112);
        let hover = response.hover_pos().map(|point| world_at(point, offset, zoom));
        let hovered_wire = hover
            .filter(|&point| nearest_port(&anchors, point, 9.0 / zoom, None).is_none()
                && !cards.iter().any(|(_, card, _)| card.contains(point)))
            .and_then(|point| wire_at(project, &anchors, point, 7.0 / zoom, selection));
        let hover_port = hover.and_then(|point| nearest_port(&anchors, point, 9.0 / zoom, None));
        if wire_drag.is_none() && self.drag.is_none() && !placing_block {
            let route_cursor = self.route_drag.as_ref().map(|drag| wires::horizontal_handle(&drag.path, drag.handle))
                .or_else(|| {
                    let point = hover?;
                    let id = hovered_wire?;
                    let (path, _) = wire_path(project, &anchors, project.binding(id)?)?;
                    Some(wires::horizontal_handle(&path, wires::hit(&path, point, 8.0 / zoom)?))
                });
            if self.resize_drag.is_some() || hover.zip(resize_handle).is_some_and(|(point, (_, card, _))| point.distance(card.max) <= 10.0 / zoom) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            } else if hover_port.is_some() && self.route_drag.is_none() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            } else if let Some(axis) = route_cursor {
                ui.ctx().set_cursor_icon(match axis {
                    Some(true) => egui::CursorIcon::ResizeVertical,
                    Some(false) => egui::CursorIcon::ResizeHorizontal,
                    None => egui::CursorIcon::Move,
                });
            }
        }
        let preview = match (wire_drag, drop_port) {
            (Some(WireDrag::Connect(from)), Some(to)) =>
                Some(review::connection_preview(project, active_mode, from, to, findings)),
            _ => None,
        };
        let drag_color = match &preview {
            Some(Err(_)) => status_ui::color(Status::Fail),
            Some(Ok(checks)) => checks.iter().min_by_key(|f| status_ui::priority(f.status))
                .map_or(focus_color, |f| status_ui::color(f.status)),
            None => focus_color,
        };
        if let (Some(preview), Some(pointer)) = (&preview, pointer) {
            egui::Area::new(ui.id().with("wire_drag_preview")).order(egui::Order::Tooltip)
                .fixed_pos(pointer + egui::vec2(16.0, 16.0)).interactable(false).show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(365.0);
                        match preview {
                            Ok(checks) => {
                                for check in checks.iter().take(3) {
                                    ui.colored_label(status_ui::color(check.status), check.status.label());
                                    ui.label(&check.detail);
                                }
                            }
                            Err(error) => { ui.colored_label(status_ui::color(Status::Fail), error); }
                        }
                    });
                });
        }
        let mut handles = Vec::new();
        for binding in &project.bindings {
            let route_drag = self.route_drag.as_ref().filter(|drag| drag.id == binding.id);
            let moving = route_drag.is_some();
            let Some((path, power)) = wire_path(project, &anchors, binding) else { continue };
            let points = route_drag.map_or(path.as_slice(), |drag| drag.preview.as_slice());
            let selected = selection == Selection::Wire(binding.id) || moving;
            let highlighted = highlighted_ports.contains(&binding.source) && highlighted_ports.contains(&binding.target);
            let color = health.bindings.get(&binding.id).copied().map(status_ui::color)
                .unwrap_or_else(|| if power { POWER } else { selected_color });
            if selected {
                handles.extend(points.iter().skip(1).take(points.len().saturating_sub(2))
                    .map(|point| (screen(*point), color)));
            }
            let screen_points: Vec<_> = points.iter().copied().map(screen).collect();
            if selected || highlighted {
                painter.add(Shape::line(screen_points.clone(), Stroke::new(6.0_f32, Color32::from_rgb(176, 191, 205))));
            }
            painter.add(Shape::line(screen_points,
                Stroke::new(if selected || highlighted || hovered_wire == Some(binding.id) { 2.8_f32 } else { 2.0_f32 }, color)));
        }
        if let (Some(WireDrag::Connect(from)), Some(pointer)) = (wire_drag, pointer) {
            if let Some(start) = anchors.get(&from).copied().map(screen) {
                let end = drop_port.and_then(|id| anchors.get(&id)).copied().map(screen).unwrap_or(pointer);
                let mid = (start.x + end.x) * 0.5;
                painter.add(Shape::line(vec![start, egui::pos2(mid, start.y), egui::pos2(mid, end.y), end],
                    Stroke::new(2.2_f32, drag_color)));
            }
        }
        let click_world = click.map(|point| world_at(point, offset, zoom));
        let mut clicked = click_world.and_then(|point| wire_at(project, &anchors, point, 7.0 / zoom, selection))
            .map(Selection::Wire).or(boundary_hit);
        let additive = primary_click && ui.input(|input| input.modifiers.shift || input.modifiers.command);
        let mut clicked_block = None;
        for protection in project.interfaces.protections.iter().filter(|p|p.shunt) {
            if let (Some((_,card,_)),Some(&target)) = (cards.iter().find(|(block,_,_)|block.id==protection.block),anchors.get(&protection.entry)) {
                let from=screen(card.center()); let to=screen(target);
                painter.line_segment([from,to],Stroke::new(1.0_f32,focus_color));
                painter.text(from.lerp(to,0.5),egui::Align2::CENTER_BOTTOM,"shunt attachment",FontId::proportional(10.0),focus_color);
            }
        }
        for (block, card, ports) in cards {
            let rect = Rect::from_min_max(screen(card.min), screen(card.max));
            let grouped = self.selected_blocks.len() >= 2 && self.selected_blocks.contains(&block.id);
            let reference = grouped && self.selected_blocks.first() == Some(&block.id);
            let selected = grouped || active_drag == Some(block.id) || selection == Selection::Block(block.id)
                || matches!(selection, Selection::Port(id) if ports.iter().any(|p| p.id == id));
            let color = if reference { Color32::from_rgb(192, 152, 241) }
                else if grouped { selected_color }
                else if highlighted_blocks.contains(&block.id) { focus_color }
                else if selected { selected_color } else { visuals.widgets.inactive.bg_stroke.color };
            let fill = if selected { Color32::from_rgb(34, 58, 72) } else { Color32::from_rgb(30, 45, 59) };
            painter.rect_filled(rect, 6.0, fill);
            painter.rect_stroke(rect, 6.0, Stroke::new(if selected { 2.0_f32 } else { 1.0_f32 }, color), StrokeKind::Inside);
            if selection == Selection::Block(block.id) {
                let grip = Rect::from_center_size(rect.max, egui::vec2(11.0, 11.0));
                painter.rect_filled(grip, 2.0, SIGNAL);
            }
            if self.zoom >= 0.38 {
                let inner = painter.with_clip_rect(rect.shrink(3.0));
                let category = if block.power_converter { "POWER CONVERTER" }
                    else if project.interfaces.connectors.iter().any(|c|c.block==block.id) { "CONNECTOR" }
                    else if project.interfaces.protections.iter().any(|p|p.block==block.id) { "PROTECTION" }
                    else if project.interfaces.channels.iter().any(|c|c.block==block.id) { "SIGNAL CONVERSION" } else { block.role.label() };
                inner.text(screen(card.min + egui::vec2(12.0, 8.0)), egui::Align2::LEFT_TOP,
                    category.to_uppercase(), FontId::proportional((10.0 * self.zoom).max(8.0)),
                    if block.role == Role::Power { POWER } else { SIGNAL });
                inner.text(screen(card.min + egui::vec2(12.0, 24.0)), egui::Align2::LEFT_TOP,
                    &block.name, FontId::proportional((16.0 * self.zoom).max(10.0)), visuals.text_color());
            }
            if click.is_some_and(|point| rect.contains(point)) {
                clicked_block = Some(Selection::Block(block.id));
                clicked = clicked_block;
            }
            let mut label_positions = placements(&ports);
            if let Some(WireDrag::Layout { port, position }) = wire_drag {
                if let Some(current) = label_positions.get_mut(&port) { *current = position; }
            }
            for port in ports {
                let world = anchors[&port.id];
                let point = screen(world);
                let row_y = world.y;
                let placement=label_positions[&port.id];
                let left=if placement.edge==crate::interfaces::Edge::Right { card.center().x } else { card.left()-6.0 };
                let right=if placement.edge==crate::interfaces::Edge::Left { card.center().x } else { card.right()+6.0 };
                let row = Rect::from_min_max(
                    screen(egui::pos2(left, row_y - ROW * 0.5)),
                    screen(egui::pos2(right, row_y + ROW * 0.5)),
                );
                let color = if port.kind.power() { POWER } else { selected_color };
                let dot_color = if drop_port == Some(port.id) { drag_color }
                    else { health.ports.get(&port.id).copied().map(status_ui::color).unwrap_or(color) };
                let dot = if selection == Selection::Port(port.id) || hover_port == Some(port.id) || drop_port == Some(port.id) {
                    6.0
                } else { 4.5 };
                if selection == Selection::Port(port.id) || connect_from == Some(port.id) || highlighted_ports.contains(&port.id) {
                    painter.circle_stroke(point, dot + 2.5, Stroke::new(1.5_f32, Color32::from_rgb(210, 221, 231)));
                }
                painter.circle_filled(point, dot, dot_color);
                if self.zoom >= 0.5 {
                    let placement = label_positions[&port.id];
                    let (label_point, align) = match placement.edge {
                        crate::interfaces::Edge::Top => (world + egui::vec2(0.0,-10.0),egui::Align2::CENTER_BOTTOM),
                        crate::interfaces::Edge::Bottom => (world + egui::vec2(0.0,10.0),egui::Align2::CENTER_TOP),
                        crate::interfaces::Edge::Left => (world + egui::vec2(12.0,0.0),egui::Align2::LEFT_CENTER),
                        crate::interfaces::Edge::Right => (world - egui::vec2(12.0,0.0),egui::Align2::RIGHT_CENTER),
                    };
                    let horizontal = matches!(placement.edge,crate::interfaces::Edge::Top|crate::interfaces::Edge::Bottom);
                    let alone_on_edge = !label_positions.iter()
                        .any(|(&id, position)| id != port.id && position.edge == placement.edge);
                    let label = if horizontal && !alone_on_edge {
                        if port.pin.is_empty() { port.name.chars().take(4).collect::<String>() } else { port.pin.clone() }
                    } else { port.name.clone() };
                    let label_clip = match placement.edge {
                        crate::interfaces::Edge::Left => Rect::from_min_max(rect.min,egui::pos2(rect.center().x-2.0,rect.bottom())),
                        crate::interfaces::Edge::Right => Rect::from_min_max(egui::pos2(rect.center().x+2.0,rect.top()),rect.max),
                        _ => viewport,
                    };
                    painter.with_clip_rect(label_clip).text(screen(label_point), align,
                        label, FontId::proportional((12.0 * self.zoom).max(9.0)), color);
                    if hover_port==Some(port.id) { painter.text(point+egui::vec2(8.0,-18.0),egui::Align2::LEFT_BOTTOM,
                        &port.name,FontId::proportional(13.0),focus_color); }
                }
                if click.is_some_and(|pointer| (!horizontal_port(label_positions[&port.id]) && row.contains(pointer)) || pointer.distance(point) <= 6.0) {
                    clicked = Some(Selection::Port(port.id));
                }
            }
        }
        for (handle, color) in handles {
            painter.circle_filled(handle, 5.0, color);
            painter.circle_stroke(handle, 5.0, Stroke::new(1.5_f32, visuals.extreme_bg_color));
        }
        if let Some((_, point, _)) = layout_handle {
            if wire_drag.is_none() {
                let point = screen(point);
                let square = Rect::from_center_size(point, egui::vec2(9.0, 9.0));
                painter.rect_filled(square, 2.0, focus_color);
                if response.hover_pos().is_some_and(|hover| hover.distance(point) <= 7.0) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    response.clone().on_hover_text("Drag to move this port along the block edge.");
                }
            }
        }
        if let Some(port) = click_world.and_then(|point| nearest_port(&anchors, point, 9.0 / zoom, None)) {
            clicked = Some(Selection::Port(port));
        }
        if let Some(binding) = hovered_wire.and_then(|id| project.binding(id)) {
            response.clone().on_hover_ui(|ui| {
                let kind = project.port(binding.source).map_or("Connection", |p| if p.kind.power() { "Power connection" } else { "Signal connection" });
                ui.strong(kind);
                if let Some(status) = health.bindings.get(&binding.id) { status_ui::badge(ui, *status); }
                for finding in wire_findings(binding, findings)
                    .filter(|finding| health.bindings.get(&binding.id) == Some(&finding.status)).take(3) { ui.label(&finding.title); ui.label(&finding.detail); }
                ui.weak("Click the wire to inspect it; open Design checks for individual results.");
            });
        } else if let Some(port) = hover_port.and_then(|id| project.port(id)) {
            response.clone().on_hover_ui(|ui| {
                ui.strong(project.port_label(port)); ui.label(port.kind.label());
                if let Some(status) = health.ports.get(&port.id) { status_ui::badge(ui, *status); }
                ui.weak("Dot color shows the worst related check. Labels identify signal or power.");
            });
        }
        if let (Some((name, ports)), Some(point)) = (block_preview, placement_point) {
            self.draw_placement(&painter, viewport, point, name, ports);
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if let (Some(start), Some(pointer)) = (self.marquee_start, pointer) {
            let rect = Rect::from_two_pos(screen(start), pointer).intersect(viewport);
            if rect.is_positive() {
                painter.rect_filled(rect, 1.0, SIGNAL.gamma_multiply(0.18));
                painter.rect_stroke(rect, 1.0, Stroke::new(1.0_f32, SIGNAL), StrokeKind::Inside);
            }
        }
        let menu = if placing_block || drawing_boundary || suppress_secondary {
            None
        } else { self.context_menu(&response, project, clicked) };
        let mut inspect = None;
        if primary_click && !placing_block && !drawing_boundary {
            response.surrender_focus();
            inspect = Some(match clicked_block.filter(|_| additive).or(clicked) {
                Some(Selection::Block(id)) => {
                    let mut blocks = if additive { self.selected_blocks.clone() } else { Vec::new() };
                    if let Some(index) = blocks.iter().position(|&selected| selected == id) {
                        blocks.remove(index);
                    } else { blocks.push(id); }
                    DiagramAction::SelectBlocks(blocks)
                }
                Some(Selection::Port(id)) if response.double_clicked() => DiagramAction::Connect(id),
                _ => DiagramAction::Inspect(clicked.unwrap_or(Selection::Project)),
            });
            ui.ctx().request_repaint();
        }
        if !primary_down { self.primary_edit = false; }
        if wire_action.is_some() || moved.is_some() || arrangement.is_some() || route_action.is_some() || resize_action.is_some() { ui.ctx().request_repaint(); }
        toolbar_action.or(placement).or(boundary_action).or(route_action).or(resize_action).or(wire_action).or(arrangement).or(moved).or(marquee_action).or(menu).or(inspect)
    }

    fn context_menu(&mut self, response: &egui::Response, project: &Project, hit: Option<Selection>) -> Option<DiagramAction> {
        if response.secondary_clicked() {
            self.context_target = hit.unwrap_or(Selection::Project);
        }
        let mut action = None;
        response.context_menu(|ui| {
            match self.context_target {
                Selection::Block(id) if project.block(id).is_some() => {
                    let targets = if self.selected_blocks.contains(&id) {
                        self.selected_blocks.clone()
                    } else {
                        vec![id]
                    };
                    let (duplicate, delete) = if targets.len() > 1 {
                        (format!("Duplicate {} blocks", targets.len()), format!("Delete {} blocks", targets.len()))
                    } else {
                        ("Duplicate block".into(), "Delete block".into())
                    };
                    if ui.button(duplicate).clicked() { action = Some(DiagramAction::Duplicate(targets.clone())); }
                    if ui.button(delete).clicked() { action = Some(DiagramAction::Delete(targets)); }
                    ui.separator();
                    if ui.button("Inspect block").clicked() {
                        action = Some(DiagramAction::Inspect(Selection::Block(id)));
                    }
                    if let Some(port) = add_port_menu(ui, id) { action = Some(port); }
                    if project.block(id).is_some_and(|block| block.size.is_some()) && ui.button("Reset block size").clicked() {
                        action = Some(DiagramAction::ResizeBlock(id, None, None));
                    }
                    if project.block(id).is_some_and(|block| block.position.is_some())
                        && ui.button("Reset block position").clicked()
                    {
                        action = Some(DiagramAction::Position(id, None));
                    }
                }
                Selection::Port(id) if project.port(id).is_some() => {
                    if ui.button("Connect this port…").clicked() { action = Some(DiagramAction::Connect(id)); }
                    if ui.button("Move ports on canvas").clicked() {
                        self.port_layout = true;
                        action = Some(DiagramAction::Inspect(Selection::Port(id)));
                    }
                    if ui.button("Inspect port").clicked() { action = Some(DiagramAction::Inspect(Selection::Port(id))); }
                    if project.port(id).is_some_and(|port| port.layout.is_some())
                        && ui.button("Reset port placement").clicked()
                    {
                        action = Some(DiagramAction::ResetPortPosition(id));
                    }
                }
                Selection::Boundary(id) if project.subsystem(id).is_some() => {
                    if ui.button("Remove boundary").clicked() { action = Some(DiagramAction::RemoveBoundary(id)); }
                }
                Selection::Wire(id) if project.binding(id).is_some() => {
                    if ui.button("Inspect connection").clicked() { action = Some(DiagramAction::Inspect(Selection::Wire(id))); }
                    if project.binding(id).is_some_and(|wire| wire.bend.is_some() || !wire.route.is_empty())
                        && ui.button("Reset route").clicked()
                    {
                        action = Some(DiagramAction::RouteWire(id, Vec::new()));
                    }
                    if ui.button("Disconnect").clicked() { action = Some(DiagramAction::Disconnect(id)); }
                }
                _ => {
                    if ui.button("Add block…").clicked() { action = Some(DiagramAction::AddBlock); }
                    if ui.button("Fit diagram").clicked() {
                        self.refit();
                        ui.ctx().request_repaint();
                        ui.close_menu();
                    }
                }
            }
            if action.is_some() { ui.close_menu(); }
        });
        action
    }
}

#[cfg(test)]
mod canvas_gesture_tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton};

    struct Canvas {
        ctx: egui::Context,
        diagram: Diagram,
        project: Project,
        selection: Selection,
        viewport: Rect,
    }

    impl Canvas {
        fn frame(&mut self, events: Vec<Event>, modifiers: Modifiers) -> Option<DiagramAction> {
            let mut action = None;
            let output = self.ctx.run(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 750.0))),
                events, modifiers, ..Default::default()
            }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    action = self.diagram.show(ui, &self.project, self.selection, &[], &[], None, &[], None, None, false);
                });
            });
            self.viewport = output.shapes.iter().find_map(|shape| match &shape.shape {
                Shape::Rect(rect) if rect.corner_radius.nw == 8 && rect.rect.width() > 900.0 => Some(rect.rect),
                _ => None,
            }).expect("canvas viewport");
            action
        }

        fn mouse(&mut self, event: Event) -> Option<DiagramAction> {
            self.frame(vec![event], Modifiers::NONE)
        }

        fn screen(&self, world: Vec2) -> Pos2 {
            self.viewport.min + self.diagram.offset + world * self.diagram.zoom
        }
    }

    fn button(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
        Event::PointerButton { pos, button, pressed, modifiers: Modifiers::NONE }
    }

    #[test]
    fn marquee_and_pan_during_block_drag_commit_using_the_new_view() -> crate::model::Result<()> {
        use PointerButton::{Middle, Primary, Secondary};
        let mut project = Project::new();
        let left = project.add_block(Role::Custom, "Left".into(), false)?;
        let right = project.add_block(Role::Custom, "Right".into(), false)?;
        project.position_block(left, Some(BlockPosition { x: 150, y: 150 }))?;
        project.position_block(right, Some(BlockPosition { x: 450, y: 150 }))?;
        let boundary = project.add_subsystem("Group".into(), None, BlockPosition { x: 120, y: 110 },
            BlockSize { width: 575, height: 240 }, &[left, right])?;
        let mut canvas = Canvas { ctx: egui::Context::default(), diagram: Diagram::default(), project,
            selection: Selection::Project, viewport: Rect::NOTHING };
        canvas.diagram.keep_view();
        canvas.frame(Vec::new(), Modifiers::NONE);

        let border = canvas.screen(egui::vec2(122.0, 265.0));
        canvas.mouse(Event::PointerMoved(border));
        canvas.mouse(button(border, Primary, true));
        assert!(matches!(canvas.mouse(button(border, Primary, false)),
            Some(DiagramAction::Inspect(Selection::Boundary(id))) if id == boundary));

        let start = canvas.screen(egui::vec2(80.0, 90.0));
        let end = canvas.screen(egui::vec2(700.0, 275.0));
        canvas.mouse(Event::PointerMoved(start));
        canvas.mouse(button(start, Primary, true));
        canvas.mouse(Event::PointerMoved(end));
        let Some(DiagramAction::SelectBlocks(blocks)) = canvas.mouse(button(end, Primary, false))
            else { panic!("left drag should select the intersected blocks"); };
        assert_eq!(blocks, [left, right]);

        canvas.diagram.select_blocks(vec![left]);
        let right_block = canvas.screen(egui::vec2(490.0, 176.0));
        canvas.mouse(Event::PointerMoved(right_block));
        canvas.frame(vec![button(right_block, Primary, true)], Modifiers::SHIFT);
        let Some(DiagramAction::SelectBlocks(clicked)) =
            canvas.frame(vec![button(right_block, Primary, false)], Modifiers::SHIFT)
            else { panic!("shift-click should add the second block"); };
        assert_eq!(clicked, blocks);
        canvas.selection = Selection::Block(left);
        canvas.diagram.select_blocks(blocks);

        let start = canvas.screen(egui::vec2(180.0, 175.0));
        canvas.mouse(Event::PointerMoved(start));
        canvas.mouse(button(start, Primary, true));
        let first = start + egui::vec2(50.0, 0.0);
        canvas.mouse(Event::PointerMoved(first));
        assert!(canvas.diagram.drag.is_some());
        canvas.mouse(button(first, Middle, true));
        let next = first + egui::vec2(75.0, 25.0);
        canvas.mouse(Event::PointerMoved(next));
        assert_eq!(canvas.diagram.offset, egui::vec2(75.0, 25.0));
        let Some(DiagramAction::Arrange(_, moved)) = canvas.mouse(button(next, Primary, false))
            else { panic!("releasing the left button should finish the drag while middle remains held"); };
        assert_eq!(moved, [(left, BlockPosition { x: 200, y: 150 }), (right, BlockPosition { x: 500, y: 150 })]);
        canvas.mouse(button(next, Middle, false));

        canvas.mouse(button(next, Secondary, true));
        let panned = next + egui::vec2(40.0, 0.0);
        canvas.mouse(Event::PointerMoved(panned));
        assert!(canvas.mouse(button(panned, Secondary, false)).is_none());
        assert_eq!(canvas.diagram.offset, egui::vec2(115.0, 25.0));

        let start = canvas.screen(egui::vec2(180.0, 175.0));
        canvas.mouse(Event::PointerMoved(start));
        canvas.mouse(button(start, Primary, true));
        let edge = egui::pos2(canvas.viewport.right() - 2.0, start.y);
        canvas.mouse(Event::PointerMoved(edge));
        let at_edge = canvas.diagram.offset.x;
        canvas.frame(Vec::new(), Modifiers::NONE);
        assert!(canvas.diagram.offset.x < at_edge, "edge pan should continue without pointer movement");
        assert!(canvas.diagram.drag.is_some());

        let at_edge = canvas.diagram.offset;
        canvas.mouse(Event::WindowFocused(false));
        assert_eq!(canvas.diagram.offset, at_edge);
        assert!(canvas.diagram.drag.is_none());
        assert!(!canvas.diagram.primary_edit);
        assert!(canvas.mouse(button(edge, Primary, false)).is_none());

        let start = canvas.screen(egui::vec2(180.0, 175.0));
        canvas.mouse(Event::PointerMoved(start));
        canvas.mouse(button(start, Primary, true));
        canvas.mouse(Event::PointerMoved(edge));
        assert!(canvas.diagram.drag.is_some());
        let at_edge = canvas.diagram.offset;
        canvas.mouse(Event::PointerGone);
        assert_eq!(canvas.diagram.offset, at_edge);
        assert!(canvas.diagram.drag.is_none());
        assert!(!canvas.diagram.primary_edit);
        Ok(())
    }
}
