use crate::model::{BlockPosition, Id};
use eframe::egui::{pos2, vec2, Pos2};

pub fn path(id: Id, from: Pos2, to: Pos2, power: bool, bend: Option<BlockPosition>) -> Vec<Pos2> {
    if let Some(bend) = bend {
        let bend = pos2(bend.x as f32, bend.y as f32);
        if power {
            let a = from + vec2(0.0, -12.0);
            let b = to + vec2(0.0, 12.0);
            return vec![from, a, pos2(bend.x, a.y), bend, pos2(b.x, bend.y), b, to];
        }
        let a = from + vec2(12.0, 0.0);
        let b = to + vec2(-12.0, 0.0);
        return vec![from, a, pos2(a.x, bend.y), bend, pos2(bend.x, b.y), b, to];
    }
    if power {
        if from.y > to.y {
            let lane = (from.y - 34.0).max(to.y + 10.0).min(from.y - 4.0);
            vec![from, pos2(from.x, lane), pos2(to.x, lane), to]
        } else {
            let lane = from.x.max(to.x) + 110.0;
            vec![from, pos2(from.x, from.y - 12.0), pos2(lane, from.y - 12.0),
                pos2(lane, to.y + 12.0), pos2(to.x, to.y + 12.0), to]
        }
    } else if to.x > from.x + 12.0 {
        let mid = (from.x + to.x) * 0.5;
        vec![from, pos2(mid, from.y), pos2(mid, to.y), to]
    } else {
        let lane = from.y.min(to.y) - 21.0 - (id % 3) as f32 * 8.0;
        vec![from, pos2(from.x + 14.0, from.y), pos2(from.x + 14.0, lane),
            pos2(to.x - 14.0, lane), pos2(to.x - 14.0, to.y), to]
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Handle { Corner(usize), Segment(usize) }

fn horizontal(a: Pos2, b: Pos2) -> bool { a.y == b.y }

pub fn clean(points: impl IntoIterator<Item = Pos2>) -> Vec<Pos2> {
    let mut route = Vec::<Pos2>::new();
    for point in points {
        while route.len() >= 2 {
            let a = route[route.len() - 2];
            let b = route[route.len() - 1];
            if (a.x == b.x && b.x == point.x) || (a.y == b.y && b.y == point.y) {
                route.pop();
            } else { break; }
        }
        if route.last() != Some(&point) { route.push(point); }
    }
    route
}

fn from_position(point: BlockPosition) -> Pos2 { pos2(point.x as f32, point.y as f32) }

pub fn stored_path(route: &[BlockPosition], from: Pos2, to: Pos2) -> Vec<Pos2> {
    let points: Vec<_> = route.iter().copied().map(from_position).collect();
    let first = points[0];
    let last = points[points.len() - 1];
    let delta = from - first;
    if delta == to - last { return clean(points.into_iter().map(|point| point + delta)); }
    let mut joined = vec![from];
    let length = points.len();
    if length == 2 {
        if horizontal(first, last) {
            joined.extend([pos2(from.x, first.y), pos2(to.x, last.y)]);
        } else {
            joined.extend([pos2(first.x, from.y), pos2(last.x, to.y)]);
        }
    } else {
        let next = points[1];
        let previous = points[length - 2];
        joined.push(if horizontal(first, next) { pos2(next.x, from.y) } else { pos2(from.x, next.y) });
        joined.extend_from_slice(&points[1..length - 1]);
        joined.push(if horizontal(previous, last) { pos2(previous.x, to.y) } else { pos2(to.x, previous.y) });
    }
    joined.push(to);
    clean(joined)
}

pub fn position(path: &[Pos2], handle: Handle) -> Pos2 {
    match handle {
        Handle::Corner(index) => path[index],
        Handle::Segment(index) => path[index].lerp(path[index + 1], 0.5),
    }
}

pub fn hit(path: &[Pos2], point: Pos2, tolerance: f32) -> Option<Handle> {
    let corner = path.iter().enumerate().skip(1).take(path.len().saturating_sub(2))
        .map(|(i, corner)| (i, corner.distance(point)))
        .filter(|(_, distance)| *distance <= tolerance)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((i, _)) = corner { return Some(Handle::Corner(i)); }
    path.windows(2).enumerate().map(|(i, line)| (i, segment_distance(line[0], line[1], point)))
        .filter(|(_, distance)| *distance <= tolerance)
        .min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| Handle::Segment(i))
}

pub fn horizontal_handle(path: &[Pos2], handle: Handle) -> Option<bool> {
    match handle {
        Handle::Corner(_) => None,
        Handle::Segment(i) => Some(horizontal(path[i], path[i + 1])),
    }
}

pub fn drag(path: &[Pos2], handle: Handle, target: Pos2) -> Vec<Pos2> {
    let mut points = path.to_vec();
    let mut shift = |index: usize, parallel: bool| {
        if parallel { points[index].y = target.y; } else { points[index].x = target.x; }
    };
    match handle {
        Handle::Segment(i) => {
            let parallel = horizontal(path[i], path[i + 1]);
            shift(i, parallel);
            shift(i + 1, parallel);
        }
        Handle::Corner(i) => {
            shift(i - 1, horizontal(path[i - 1], path[i]));
            shift(i + 1, horizontal(path[i], path[i + 1]));
            points[i] = target;
        }
    }
    // Original anchors stay fixed. A moved terminal segment gets a short orthogonal leg.
    clean(std::iter::once(path[0]).chain(points).chain(path.last().copied()))
}

fn segment_distance(a: Pos2, b: Pos2, point: Pos2) -> f32 {
    let delta = b - a;
    let length_squared = delta.length_sq();
    let t = if length_squared == 0.0 { 0.0 }
        else { ((point - a).dot(delta) / length_squared).clamp(0.0, 1.0) };
    point.distance(a + delta * t)
}

pub fn distance(points: &[Pos2], point: Pos2) -> f32 {
    points.windows(2).map(|line| segment_distance(line[0], line[1], point)).fold(f32::INFINITY, f32::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_shift_perpendicular_and_merge_with_their_neighbors() {
        let path = [pos2(0.0, 0.0), pos2(20.0, 0.0), pos2(20.0, 40.0), pos2(80.0, 40.0)];
        let shifted = drag(&path, Handle::Segment(1), pos2(40.0, 17.0));
        assert_eq!(shifted, [path[0], pos2(40.0, 0.0), pos2(40.0, 40.0), path[3]]);
        let merged = drag(&path, Handle::Segment(1), pos2(0.0, 17.0));
        assert_eq!(merged, [path[0], pos2(0.0, 40.0), path[3]]);
        let terminal = drag(&path, Handle::Segment(0), pos2(10.0, 20.0));
        assert_eq!(terminal, [path[0], pos2(0.0, 20.0), pos2(20.0, 20.0), path[2], path[3]]);
    }

    #[test]
    fn dragging_corners_keeps_the_route_orthogonal_and_anchored() {
        let path = [pos2(0.0, 0.0), pos2(20.0, 0.0), pos2(20.0, 40.0), pos2(80.0, 40.0)];
        let result = drag(&path, Handle::Corner(1), pos2(40.0, 20.0));
        assert_eq!(result, [path[0], pos2(0.0, 20.0), pos2(40.0, 20.0), pos2(40.0, 40.0), path[3]]);
        let result = drag(&path, Handle::Corner(2), pos2(40.0, 20.0));
        assert_eq!(result, [path[0], pos2(40.0, 0.0), pos2(40.0, 20.0), pos2(80.0, 20.0), path[3]]);
    }

    #[test]
    fn stored_routes_follow_moved_endpoints() {
        let path = [BlockPosition { x: 0, y: 0 }, BlockPosition { x: 40, y: 0 },
            BlockPosition { x: 40, y: 20 }, BlockPosition { x: 80, y: 20 }];
        let from = pos2(10.0, 10.0);
        let to = pos2(90.0, 30.0);
        assert_eq!(stored_path(&path, from, to), [from, pos2(50.0, 10.0), pos2(50.0, 30.0), to]);
        let to = pos2(85.0, 45.0);
        let joined = stored_path(&path, from, to);
        assert_eq!(joined.first(), Some(&from));
        assert_eq!(joined.last(), Some(&to));
        assert!(joined.windows(2).all(|p| p[0].x == p[1].x || p[0].y == p[1].y));
    }

    #[test]
    fn manual_route_is_orthogonal_and_passes_through_the_handle() {
        let from = pos2(12.0, 5.0);
        let to = pos2(100.0, 45.0);
        let bend = BlockPosition { x: 25, y: -50 };
        for power in [false, true] {
            let points = path(1, from, to, power, Some(bend));
            assert_eq!(points.first(), Some(&from));
            assert_eq!(points.last(), Some(&to));
            assert!(points.windows(2).all(|p| p[0].x == p[1].x || p[0].y == p[1].y));
            assert_eq!(distance(&points, pos2(25.0, -50.0)), 0.0);
        }
    }
}
