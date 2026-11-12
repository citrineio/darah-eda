use crate::{
    diagram::{layout, placements, port_anchors, port_point, subsystem_rect, wire_path},
    interfaces::Edge,
    model::{Project, Result},
    spec::{self, Data, Target},
    storage,
};
use eframe::egui::{Pos2, Rect};
use std::{fmt::Write as _, fs, path::Path, process::Command};

#[derive(Clone, Copy)]
pub enum Content { Diagram, Requirements }

#[derive(Clone, Copy)]
pub enum Format { Svg, Png, Pdf, Csv }

impl Format {
    pub const VISUAL: [Self; 3] = [Self::Pdf, Self::Png, Self::Svg];

    pub fn extension(self) -> &'static str {
        match self { Self::Svg => "svg", Self::Png => "png", Self::Pdf => "pdf", Self::Csv => "csv" }
    }

    pub fn label(self) -> &'static str {
        match self { Self::Svg => "SVG vector image", Self::Png => "PNG image", Self::Pdf => "PDF", Self::Csv => "CSV table" }
    }
}

pub fn write(project: &Project, content: Content, format: Format, path: &Path) -> Result<()> {
    if matches!(format, Format::Csv) {
        return storage::export(path, &requirements_csv(&requirements(project)));
    }
    let pages = match content {
        Content::Diagram => vec![diagram(project)],
        Content::Requirements if matches!(format, Format::Pdf) => {
            let lines = requirement_lines(project);
            let count = lines.len().div_ceil(42);
            lines.chunks(42).enumerate().map(|(i, lines)| requirement_svg(project, lines, Some((i + 1, count)))).collect()
        }
        Content::Requirements => vec![requirement_svg(project, &requirement_lines(project), None)],
    };
    match format {
        Format::Svg => storage::export(path, &pages[0]),
        Format::Pdf | Format::Png => convert(&pages, format, path),
        Format::Csv => unreachable!(),
    }
}

fn convert(pages: &[String], format: Format, output: &Path) -> Result<()> {
    let directory = tempfile::tempdir().map_err(|e| format!("Cannot prepare export: {e}"))?;
    let mut command = Command::new("rsvg-convert");
    command.arg(format!("--format={}", format.extension()));
    if matches!(format, Format::Png) {
        command.args(["--zoom=2", "--width=10000", "--height=10000", "--keep-aspect-ratio"]);
    }
    for (index, page) in pages.iter().enumerate() {
        let file = directory.path().join(format!("{index}.svg"));
        fs::write(&file, page).map_err(|e| format!("Cannot prepare export page: {e}"))?;
        command.arg(file);
    }
    let rendered = command.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "PDF and PNG export needs rsvg-convert. Install librsvg2-bin on Debian or Ubuntu.".into()
        } else { format!("Cannot start the export renderer: {e}") }
    })?;
    if !rendered.status.success() || rendered.stdout.is_empty() {
        return Err(format!("{} export failed: {}", format.label(), String::from_utf8_lossy(&rendered.stderr).trim()));
    }
    storage::export_bytes(output, &rendered.stdout)
}

fn xml(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            '\u{FFFE}' | '\u{FFFF}' => escaped.push('\u{FFFD}'),
            ch if ch.is_control() && ch != '\t' && ch != '\n' && ch != '\r' => escaped.push('\u{FFFD}'),
            ch => escaped.push(ch),
        }
    }
    escaped
}

fn svg(x: f32, y: f32, width: f32, height: f32) -> String {
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="{x:.0} {y:.0} {width:.0} {height:.0}">
<rect x="{x:.0}" y="{y:.0}" width="{width:.0}" height="{height:.0}" fill="white"/>
<g font-family="sans-serif" fill="#172638">"##)
}

fn text(svg: &mut String, x: f32, y: f32, size: u32, align: &str, value: &str) {
    let _ = writeln!(svg, r#"<text x="{x:.1}" y="{y:.1}" font-size="{size}" text-anchor="{align}">{}</text>"#, xml(value));
}

fn diagram(project: &Project) -> String {
    let cards = layout(project);
    let anchors = port_anchors(&cards);
    let routes: Vec<_> = project.bindings.iter().filter_map(|wire| wire_path(project, &anchors, wire)).collect();
    let mut bounds = cards.iter().map(|(_, card, _)| *card).chain(project.subsystems.iter().map(subsystem_rect))
        .fold(Rect::NOTHING, |bounds, card| bounds.union(card));
    for point in routes.iter().flat_map(|(path, _)| path) {
        bounds = bounds.union(Rect::from_min_max(*point, *point));
    }
    if cards.is_empty() && project.subsystems.is_empty() {
        bounds = Rect::from_min_max(Pos2::ZERO, eframe::egui::pos2(420.0, 120.0));
    }
    let x = bounds.left() - 54.0;
    let y = bounds.top() - 86.0;
    let width = (bounds.width() + 108.0).max(520.0).ceil();
    let height = (bounds.height() + 140.0).ceil();
    let mut out = svg(x, y, width, height);
    text(&mut out, x + 22.0, y + 33.0, 24, "start", &short(&project.title, ((width - 44.0) / 14.0) as usize));
    text(&mut out, x + 22.0, y + 54.0, 13, "start", "System block diagram");
    for scope in &project.subsystems {
        let rect = subsystem_rect(scope);
        let _ = writeln!(out, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="4" fill="none" stroke="rgb(42,118,106)" stroke-dasharray="8 5"/>"#,
            rect.left(), rect.top(), rect.width(), rect.height());
        text(&mut out, rect.left() + 8.0, rect.top() + 17.0, 13, "start", &scope.name);
    }
    for (path, power) in &routes {
        let coordinates = path.iter().map(|point| format!("{:.1},{:.1}", point.x, point.y)).collect::<Vec<_>>().join(" ");
        let color = if *power { "#167452" } else { "#255caa" };
        let _ = writeln!(out, r#"<polyline points="{coordinates}" fill="none" stroke="{color}" stroke-width="2.2" stroke-linejoin="round"/>"#);
    }
    for stage in project.interfaces.protections.iter().filter(|stage| stage.shunt) {
        if let (Some((_, card, _)), Some(target)) = (cards.iter().find(|(block, _, _)| block.id == stage.block), anchors.get(&stage.entry)) {
            let center = card.center();
            let _ = writeln!(out, r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="rgb(143,93,30)" stroke-dasharray="5 4"/>"#,
                center.x, center.y, target.x, target.y);
        }
    }
    for (block, card, ports) in cards {
        let _ = writeln!(out, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="6" fill="rgb(241,246,250)" stroke="rgb(58,79,100)"/>"#,
            card.left(), card.top(), card.width(), card.height());
        text(&mut out, card.left() + 12.0, card.top() + 19.0, 10, "start", if block.power_converter { "Power converter" } else { block.role.label() });
        text(&mut out, card.left() + 12.0, card.top() + 38.0, 16, "start",
            &short(&block.name, ((card.width() - 24.0) / 9.0) as usize));
        let positions = placements(&ports);
        for port in ports {
            let position = positions[&port.id];
            let point = port_point(card, position);
            let color = if port.kind.power() { "#167452" } else { "#255caa" };
            let _ = writeln!(out, r#"<circle cx="{}" cy="{}" r="4" fill="{color}"/>"#, point.x, point.y);
            let siblings = positions.values().filter(|other| other.edge == position.edge).count();
            let (dx, dy, align, limit) = match position.edge {
                Edge::Left => (9.0, 4.0, "start", ((card.width() / 2.0 - 15.0) / 7.0) as usize),
                Edge::Right => (-9.0, 4.0, "end", ((card.width() / 2.0 - 15.0) / 7.0) as usize),
                Edge::Top => (0.0, -9.0, "middle", if siblings > 1 { 4 } else { 24 }),
                Edge::Bottom => (0.0, 17.0, "middle", if siblings > 1 { 4 } else { 24 }),
            };
            text(&mut out, point.x + dx, point.y + dy, 11, align, &short(&port.name, limit));
        }
    }
    if project.blocks.is_empty() { text(&mut out, x + 22.0, y + 108.0, 14, "start", "No blocks in this project."); }
    out.push_str("</g></svg>\n");
    out
}

fn short(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let label: String = chars.by_ref().take(max.saturating_sub(1)).collect();
    match (chars.next(), chars.next()) {
        (Some(_), Some(_)) => format!("{label}…"),
        (Some(last), None) => format!("{label}{last}"),
        _ => label,
    }
}

const FIELDS: [&str; 8] = ["ID", "Requirement", "Criterion", "Source", "Allocation", "Design result", "Verification method", "Evidence"];
type Requirement = [String; 8];

fn requirements(project: &Project) -> Vec<Requirement> {
    let mut rows: Vec<_> = project.spec.iter().filter_map(|entry| {
        let Data::Requirement { title, source, target, rule, method, evidence } = &entry.data else { return None };
        Some((entry.reference, [
            spec::requirement_reference(entry.reference), title.clone(), spec::criterion(project, rule),
            spec::pending(source).into(), spec::target_name(project, *target), spec::check(project, *target, rule),
            spec::pending(method).into(), spec::pending(evidence).into(),
        ]))
    }).collect();
    for goal in &project.goals {
        let mut blocks = goal.responsible.clone();
        if let Some(owner) = goal.owner.filter(|owner| !blocks.contains(owner)) { blocks.insert(0, owner); }
        let allocation = blocks.into_iter().map(|id| spec::target_name(project, Some(Target::Block(id)))).collect::<Vec<_>>().join(", ");
        rows.push((goal.reference, [
            spec::requirement_reference(goal.reference), goal.description.clone(), "System goal".into(),
            "—".into(), spec::pending(&allocation).into(), "Manual verification".into(),
            spec::pending(&goal.method).into(), spec::pending(&goal.note).into(),
        ]));
    }
    rows.sort_by_key(|(number, _)| *number);
    rows.into_iter().map(|(_, row)| row).collect()
}

fn requirements_csv(rows: &[Requirement]) -> String {
    let mut out = FIELDS.join(",");
    out.push_str("\r\n");
    for row in rows { csv_row(&mut out, row.iter().map(String::as_str)); }
    out
}

fn csv_row<'a>(out: &mut String, values: impl IntoIterator<Item = &'a str>) {
    for (i, value) in values.into_iter().enumerate() {
        if i > 0 { out.push(','); }
        out.push('"');
        if value.trim_start().starts_with(['=', '+', '-', '@']) || value.starts_with(['\t', '\r', '\n']) { out.push('\''); }
        out.push_str(&value.replace('"', "\"\""));
        out.push('"');
    }
    out.push_str("\r\n");
}

fn wrap(value: &str, width: usize) -> Vec<String> {
    wrap_weighted(value, width, |_| 1)
}

fn wrap_weighted(value: &str, width: usize, weight: impl Fn(char) -> usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    let space = weight(' ');
    for word in value.split_whitespace() {
        let word_width = word.chars().map(&weight).sum::<usize>();
        if !line.is_empty() && used + word_width + space > width {
            lines.push(std::mem::take(&mut line));
            used = 0;
        }
        if !line.is_empty() {
            line.push(' ');
            used += space;
        }
        for ch in word.chars() {
            let next = weight(ch);
            if !line.is_empty() && used + next > width {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            line.push(ch);
            used += next;
        }
    }
    if !line.is_empty() { lines.push(line); }
    lines
}

fn requirement_lines(project: &Project) -> Vec<(bool, String)> {
    let mut lines = Vec::new();
    for row in requirements(project) {
        let title = format!("{}  {}", row[0], row[1]);
        lines.extend(wrap(&title, 64).into_iter().map(|line| (true, line)));
        for (field, value) in FIELDS[2..].iter().zip(&row[2..]) {
            lines.extend(wrap(&format!("{field}: {value}"), 82).into_iter().map(|line| (false, line)));
        }
        lines.push((false, String::new()));
    }
    if lines.is_empty() { lines.push((false, "No requirements in this project.".into())); }
    lines
}

fn requirement_svg(project: &Project, lines: &[(bool, String)], page: Option<(usize, usize)>) -> String {
    text_report_svg(&project.title, "Requirements and verification", lines, page)
}

fn text_report_svg(title: &str, subtitle: &str, lines: &[(bool, String)], page: Option<(usize, usize)>) -> String {
    let lines = lines.iter().map(|(heading, line)| (*heading, line.clone(), None)).collect::<Vec<_>>();
    colored_text_report_svg(title, subtitle, &lines, page)
}

fn colored_text_report_svg(title: &str, subtitle: &str,
    lines: &[(bool, String, Option<crate::review::Status>)], page: Option<(usize, usize)>) -> String
{
    let height = if page.is_some() { 920.0 } else { 112.0 + lines.len() as f32 * 19.0 };
    let mut out = svg(0.0, 0.0, 760.0, height);
    text(&mut out, 32.0, 34.0, 23, "start", &short(title, 49));
    text(&mut out, 32.0, 61.0, 16, "start", subtitle);
    let _ = writeln!(out, r#"<path d="M32 75 H728" stroke="rgb(170,189,205)"/>"#);
    for (index, (heading, line, status)) in lines.iter().enumerate() {
        let baseline = 101.0 + index as f32 * 19.0;
        if let Some(colors) = status.map(verdict_colors) {
            let _ = writeln!(out, r#"<rect x="32" y="{}" width="696" height="19" fill="{}"/>
<rect x="32" y="{}" width="3" height="19" fill="{}"/>"#,
                baseline - 14.0, colors.background, baseline - 14.0, colors.accent);
        }
        text(&mut out, if *heading && status.is_none() { 32.0 } else { 44.0 }, baseline,
            if *heading { 15 } else { 12 }, "start", line);
    }
    if let Some((index, total)) = page {
        text(&mut out, 728.0, height - 16.0, 11, "end", &format!("{index} / {total}"));
    }
    out.push_str("</g></svg>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagram_exports_labels_and_uses_current_geometry() -> Result<()> {
        let mut project = Project::monitor()?;
        project.title = "Design & review <draft>".into();
        let block = project.blocks[0].id;
        project.resize_block(block, None, Some(crate::model::BlockSize { width: 375, height: 150 }))?;
        let rendered = diagram(&project);
        assert!(rendered.contains("Design &amp; review &lt;draft&gt;"));
        assert!(rendered.contains("width=\"375\""));
        assert!(rendered.contains("<polyline "));
        Ok(())
    }

    #[test]
    fn requirements_export_carries_stable_ids_and_csv_escaping() -> Result<()> {
        let mut project = Project::monitor()?;
        project.goals[0].description = "Verify \"active\", then standby".into();
        let csv = requirements_csv(&requirements(&project));
        assert!(csv.contains("\"REQ-001\",\"Verify \"\"active\"\", then standby\""));
        let visual = requirement_svg(&project, &requirement_lines(&project), Some((1, 2)));
        assert!(visual.contains("REQ-001"));
        assert!(visual.contains("1 / 2"));
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub enum ReviewFormat { Markdown, Pdf, Html, Csv }

impl ReviewFormat {
    pub const ALL: [Self; 4] = [Self::Pdf, Self::Html, Self::Csv, Self::Markdown];

    pub fn extension(self) -> &'static str {
        match self { Self::Markdown => "md", Self::Pdf => "pdf", Self::Html => "html", Self::Csv => "csv" }
    }

    pub fn label(self) -> &'static str {
        match self { Self::Markdown => "Markdown", Self::Pdf => "PDF", Self::Html => "HTML", Self::Csv => "CSV check register" }
    }
}

pub fn write_review(project: &Project, mode: Option<&crate::model::Mode>, findings: &[crate::review::Finding],
    scope: Option<crate::model::Id>, format: ReviewFormat, path: &Path) -> Result<()>
{
    let name = scope.map_or_else(|| project.title.clone(), |id| project.subsystem_path(id));
    if matches!(format, ReviewFormat::Csv) {
        let mut out = String::from("Scope,Operating mode,Status,Check,Details,Evidence\r\n");
        let condition = mode.map_or(project.condition.as_str(), |mode| mode.name.as_str());
        for finding in findings.iter().filter(|finding| project.finding_in_scope(finding, scope)) {
            csv_row(&mut out, [&*name, condition, finding.status.label(), &finding.title, &finding.detail, &finding.evidence.join("\n")]);
        }
        if let Some(scope) = scope {
            for line in crate::review::boundary_lines(project, scope, mode, false, false) {
                csv_row(&mut out, [&*name, condition, "Interface", "Boundary crossing", &line, ""]);
            }
        }
        return storage::export(path, &out);
    }
    let report = crate::review::markdown_for_scope(project, mode, findings, scope)?;
    match format {
        ReviewFormat::Markdown => storage::export(path, &report),
        ReviewFormat::Html => storage::export(path, &review_html(&name, &report)),
        ReviewFormat::Pdf => {
            convert(&review_pdf_pages(&name, &report), Format::Pdf, path)
        }
        ReviewFormat::Csv => unreachable!(),
    }
}

fn plain_review_line(value: &str) -> String {
    value.replace("**", "").replace('`', "")
        .replace("&lt;", "<").replace("&gt;", ">").replace("&#124;", "|")
        .replace("&#42;", "*").replace("&#95;", "_").replace("&#96;", "`")
        .replace("&#91;", "[").replace("&#93;", "]").replace("&amp;", "&")
}

// These annotations follow the report's generated headings and table columns.
// Words in project names, evidence, or arbitrary prose never select a color.
fn review_status(value: &str) -> Option<crate::review::Status> {
    use crate::review::Status;
    match value.split(" · ").next()?.trim() {
        "Satisfied within scope" | "Design pass" => Some(Status::Pass),
        "Failed" | "Design fail" => Some(Status::Fail),
        "Unresolved" | "Open" => Some(Status::Open),
        "Manual verification" | "Captured" => Some(Status::Manual),
        "Warning" => Some(Status::Warning),
        _ => None,
    }
}

fn review_lines(markdown: &str) -> Vec<(&str, Option<crate::review::Status>)> {
    let mut section = "";
    let mut in_table = false;
    let mut status_column = None;
    markdown.lines().map(|line| {
        let mut status = None;
        if line.starts_with('|') && line.ends_with('|') {
            let cells = line.trim_matches('|').split('|').map(str::trim).collect::<Vec<_>>();
            if !in_table {
                status_column = cells.iter().position(|cell| matches!(*cell, "Status" | "Design result"));
                in_table = true;
            } else {
                status = status_column.and_then(|index| cells.get(index)).and_then(|cell| review_status(cell));
            }
        } else {
            in_table = false;
            status_column = None;
            if let Some(title) = line.strip_prefix("## ") { section = title; }
            if let Some(title) = line.strip_prefix("### ") {
                status = match section {
                    "Review" => review_status(title),
                    "Design checks and evidence" => title.rsplit_once(" · ").and_then(|(_, status)| review_status(status)),
                    _ => None,
                };
            }
            if section == "Power budgets by operating mode" && line.starts_with("- ") {
                status = line.strip_prefix("- ").and_then(review_status);
            }
        }
        (line, status)
    }).collect()
}

fn review_pdf_pages(title: &str, markdown: &str) -> Vec<String> {
    let mut lines = vec![(false, "Status colors: green = satisfied; red = failed; yellow = unresolved / warning / manual.".into(), None)];
    lines.extend(review_lines(markdown).into_iter().skip(1).flat_map(|(line, status)| {
        let heading = line.starts_with('#');
        let value = plain_review_line(line.trim_start_matches('#').trim());
        let parts = wrap(&value, if heading { 68 } else { 88 });
        if parts.is_empty() { vec![(false, String::new(), None)] }
        else { parts.into_iter().map(|line| (heading, line, status)).collect() }
    }));
    let count = lines.len().div_ceil(42);
    lines.chunks(42).enumerate().map(|(index, lines)|
        colored_text_report_svg(title, "Design review", lines, Some((index + 1, count)))).collect()
}

#[cfg(test)]
mod design_review_color_tests {
    use super::*;
    use crate::review::Status;

    #[test]
    fn all_design_verdicts_share_power_review_colors_in_html_and_pdf() {
        let mut markdown = String::from("# Demo · Design review\n\n## Review\n");
        for status in [Status::Pass, Status::Fail, Status::Open, Status::Manual, Status::Warning] {
            writeln!(markdown, "### {} · Check <A&B>\n\nEvidence remains readable.\n", status.label()).unwrap();
        }
        let html = review_html("Demo <A&B>", &markdown);
        let pages = review_pdf_pages("Demo <A&B>", &markdown);
        assert_eq!(html.matches("<h3 class=\"verdict\"").count(), 5);
        assert!(html.contains("Check &lt;A&amp;B&gt;"));
        for status in [Status::Pass, Status::Fail, Status::Open, Status::Manual, Status::Warning] {
            let colors = verdict_colors(status);
            assert!(html.contains(colors.accent));
            assert!(html.contains(status.label()));
            assert!(pages.iter().any(|page| page.contains(colors.background) && page.contains(colors.accent)));
            assert!(pages.iter().any(|page| page.contains(status.label())));
        }
    }

    #[test]
    fn scoped_checks_and_status_columns_are_colored_without_guessing_from_names() {
        let markdown = "# Demo\n## Blocks\n- Failed · board name\n\n## Design checks and evidence\n### Board · Failed\nDetails\n\n## Rail comparison\n| Rail | Mode | Status | Budget |\n| --- | --- | --- | --- |\n| Failed | Warning | Satisfied within scope | Within capacity |\n\n## Requirements\n| Requirement | Design result |\n| --- | --- |\n| A | Design fail · Outside declared range |\n| B | Open · Input missing |\n| C | Manual verification · Review needed |\n\n## Interface characteristics\n### Failed · port name\n";
        let lines = review_lines(markdown);
        assert_eq!(lines.iter().filter(|(_, status)| status.is_some()).count(), 5);
        assert_eq!(lines.iter().find(|(line, _)| line.starts_with("| Failed |")).unwrap().1, Some(Status::Pass));
        assert!(lines.iter().filter(|(line, _)| line.contains("name")).all(|(_, status)| status.is_none()));
        let html = review_html("Demo", markdown);
        assert_eq!(html.matches("<tr class=\"verdict\"").count(), 4);
        assert!(html.contains("<h3>Failed · port name</h3>"));
        let budgets = review_lines("## Power budgets by operating mode\n- Satisfied within scope · Current budget · Failed · Within capacity\n");
        assert_eq!(budgets[1].1, Some(Status::Pass));
    }

    #[test]
    fn long_status_rows_keep_their_color_across_pdf_pages() {
        let markdown = format!("# Demo\n## Rail comparison\n| Rail | Mode | Status | Budget |\n| --- | --- | --- | --- |\n| Rail | Run | Failed | {} |\n", "Long evidence statement. ".repeat(300));
        let pages = review_pdf_pages("Demo", &markdown);
        assert!(pages.len() > 1);
        for page in &pages {
            assert!(page.contains(verdict_colors(Status::Fail).accent));
            assert!(page.ends_with("</svg>\n"));
        }
        let plain = requirement_svg(&Project::new(), &[(false, "Failed is a requirement title".into())], None);
        assert!(!plain.contains(verdict_colors(Status::Fail).accent));
    }

    #[test]
    fn generated_whole_design_and_subsystem_exports_preserve_status_and_plain_registers() -> Result<()> {
        use crate::model::{BlockPosition, BlockSize, Role};
        use crate::review::{Finding, Subject};
        let mut project = Project::new();
        let block = project.add_block(Role::Processing, "Controller <A&B>".into(), false)?;
        let scope = project.add_subsystem("Controller subsystem".into(), None,
            BlockPosition { x: 0, y: 0 }, BlockSize { width: 300, height: 200 }, &[block])?;
        let findings = [Status::Pass, Status::Fail, Status::Open, Status::Manual, Status::Warning]
            .into_iter().map(|status| Finding {
                subject: Subject::Block(block), status, title: "Check <A&B>".into(),
                detail: "Declared values and evidence".into(), evidence: vec!["Lab note".into()],
                ports: vec![], blocks: vec![block],
            }).collect::<Vec<_>>();
        let directory = tempfile::tempdir().unwrap();
        for scope in [None, Some(scope)] {
            let html_path = directory.path().join(format!("review-{scope:?}.html"));
            write_review(&project, None, &findings, scope, ReviewFormat::Html, &html_path)?;
            let html = fs::read_to_string(html_path).unwrap();
            assert_eq!(html.matches("<h3 class=\"verdict\"").count(), 5);
            let report = crate::review::markdown_for_scope(&project, None, &findings, scope)?;
            assert_eq!(review_lines(&report).iter().filter(|(_, status)| status.is_some()).count(), 5);
            for format in [ReviewFormat::Markdown, ReviewFormat::Csv] {
                let path = directory.path().join(format!("review-{scope:?}.{}", format.extension()));
                write_review(&project, None, &findings, scope, format, &path)?;
                let plain = fs::read_to_string(path).unwrap();
                assert!(!plain.contains("--verdict"));
                for finding in &findings { assert!(plain.contains(finding.status.label())); }
            }
        }
        Ok(())
    }
}

fn review_html(title: &str, markdown: &str) -> String {
    let mut out = String::from(r#"<!doctype html><html lang="en"><meta charset="utf-8">
<style>body{font:15px system-ui;color:#172638;max-width:1100px;padding:2rem;margin:auto}h1,h2,h3{color:#1e605a}
table{border-collapse:collapse;width:100%;margin:1em 0}td,th{border:1px solid #cbd9e4;padding:.5em;text-align:left;overflow-wrap:anywhere}
th{background:#eff5f9}p{white-space:pre-wrap;overflow-wrap:anywhere;margin:.45em 0}
.verdict{background:var(--verdict-bg);print-color-adjust:exact;-webkit-print-color-adjust:exact}
h3.verdict{border-left:4px solid var(--verdict-accent);padding:.5em;color:#172638}
tr.verdict td:first-child,p.verdict{border-left:4px solid var(--verdict-accent)}p.verdict{padding:.5em}
@media print{body{padding:0}h2,h3{break-after:avoid}tr{break-inside:avoid}}</style>"#);
    let _ = writeln!(out, "<title>{}</title><body>", xml(title));
    out.push_str("<p>Status colors: green = satisfied; red = failed; yellow = unresolved / warning / manual.</p>\n");
    let mut table = false;
    for (line, status) in review_lines(markdown) {
        let decoration = status.map(verdict_colors).map_or_else(String::new, |colors|
            format!(r#" class="verdict" style="--verdict-bg:{};--verdict-accent:{}""#, colors.background, colors.accent));
        if line.starts_with('|') && line.ends_with('|') {
            let cells: Vec<_> = line.trim_matches('|').split('|').map(str::trim).collect();
            if cells.iter().all(|cell| cell.chars().all(|ch| matches!(ch, '-' | ':' | ' '))) { continue; }
            let tag = if table { "td" } else { "th" };
            if !table { out.push_str("<table>"); table = true; }
            let _ = write!(out, "<tr{decoration}>");
            for cell in cells { let _ = write!(out, "<{tag}>{}</{tag}>", xml(&plain_review_line(cell))); }
            out.push_str("</tr>\n");
        } else {
            if table { out.push_str("</table>\n"); table = false; }
            if line.is_empty() { continue; }
            let level = line.bytes().take_while(|&byte| byte == b'#').count();
            if (1..=6).contains(&level) && line.as_bytes().get(level) == Some(&b' ') {
                let _ = writeln!(out, "<h{level}{decoration}>{}</h{level}>", xml(&plain_review_line(&line[level + 1..])));
            } else {
                let _ = writeln!(out, "<p{decoration}>{}</p>", xml(&plain_review_line(line)));
            }
        }
    }
    if table { out.push_str("</table>\n"); }
    out.push_str("</body></html>\n");
    out
}

#[derive(Clone, Copy)]
pub enum PowerReportFormat { Pdf, Html, Csv }

impl PowerReportFormat {
    pub const ALL: [Self; 3] = [Self::Pdf, Self::Html, Self::Csv];

    pub fn extension(self) -> &'static str {
        match self { Self::Pdf => "pdf", Self::Html => "html", Self::Csv => "csv" }
    }

    pub fn label(self) -> &'static str {
        match self { Self::Pdf => "PDF", Self::Html => "HTML", Self::Csv => "CSV calculation register" }
    }
}

pub fn write_power_review(project: &Project, mode: Option<&crate::model::Mode>, scope: Option<crate::model::Id>,
    format: PowerReportFormat, path: &Path) -> Result<()>
{
    let report = crate::power_report::Report::build(project, mode, scope)?;
    match format {
        PowerReportFormat::Pdf => convert(&power_pdf_pages(&report), Format::Pdf, path),
        PowerReportFormat::Html => storage::export(path, &power_html(&report)),
        PowerReportFormat::Csv => storage::export(path, &power_csv(&report)),
    }
}

#[derive(Clone, Copy)]
struct VerdictColors {
    background: &'static str,
    accent: &'static str,
}

fn verdict_colors(status: crate::review::Status) -> VerdictColors {
    use crate::review::Status;
    match status {
        Status::Pass => VerdictColors { background: "#e8f5eb", accent: "#228246" },
        Status::Fail => VerdictColors { background: "#fcebec", accent: "#b82f3b" },
        Status::Open | Status::Manual | Status::Warning =>
            VerdictColors { background: "#fff5d6", accent: "#aa7600" },
    }
}

fn power_html(report: &crate::power_report::Report) -> String {
    let mut out = String::from(r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<style>
body{font:15px system-ui;color:#172638;max-width:1140px;padding:2rem;margin:auto}
header{border-bottom:2px solid #12685b;padding-bottom:1rem}h1,h2{color:#12685b}
table{border-collapse:collapse;width:100%;margin:1em 0 2em}td,th{border-bottom:1px solid #cbd9e4;padding:.55em;text-align:left;overflow-wrap:anywhere;vertical-align:top}
th{background:#e9f2f4}p{overflow-wrap:anywhere}td{white-space:pre-wrap}
tr.verdict{background:var(--verdict-bg);print-color-adjust:exact;-webkit-print-color-adjust:exact}
tr.verdict td:first-child{border-left:4px solid var(--verdict-accent)}
@media print{body{padding:0}h2{break-after:avoid}tr{break-inside:avoid}thead{display:table-header-group}}
</style>"#);
    let _ = writeln!(out, "<title>Power Review · {}</title></head><body>", xml(&report.title));
    let _ = writeln!(out, "<header><small>DARAH / ENGINEERING REVIEW</small><h1>Power Review</h1><p>{} / {} / {}</p><p>Project revision SHA-256 · {}</p></header><p>{}</p>",
        xml(&report.title), xml(&report.scope), xml(&report.mode), xml(&report.revision), crate::power_report::BASIS);
    for section in &report.sections {
        if section.rows.is_empty() && section.title != "Rail budgets" { continue; }
        let _ = writeln!(out, "<section><h2>{}</h2>", section.title);
        if section.rows.is_empty() {
            out.push_str("<p>No supply rails in this scope.</p></section>\n");
            continue;
        }
        out.push_str("<table><thead><tr>");
        for column in section.columns { let _ = write!(out, "<th scope=\"col\">{}</th>", xml(column)); }
        out.push_str("</tr></thead><tbody>\n");
        for row in &section.rows {
            if let Some(colors) = row.status.map(verdict_colors) {
                let _ = write!(out, r#"<tr class="verdict" style="--verdict-bg:{};--verdict-accent:{}">"#,
                    colors.background, colors.accent);
            } else {
                out.push_str("<tr>");
            }
            for value in &row.cells { let _ = write!(out, "<td>{}</td>", xml(value)); }
            out.push_str("</tr>\n");
        }
        out.push_str("</tbody></table></section>\n");
    }
    out.push_str("</body></html>\n");
    out
}

fn power_csv(report: &crate::power_report::Report) -> String {
    let mut out = String::new();
    csv_row(&mut out, ["Project", "Scope", "Mode", "Revision SHA-256", "Section", "Record",
        "Field 1", "Field 2", "Field 3", "Field 4", "Field 5"]);
    let mut append = |section: &str, record: &str, fields: &[&str]| {
        let metadata = [report.title.as_str(), report.scope.as_str(), report.mode.as_str(), report.revision.as_str(), section, record];
        csv_row(&mut out, metadata.into_iter().chain(fields.iter().copied()).chain(std::iter::repeat("")).take(11));
    };
    append("Overview", "Basis", &[crate::power_report::BASIS]);
    for section in &report.sections {
        if section.rows.is_empty() && section.title != "Rail budgets" { continue; }
        append(section.title, "Columns", section.columns);
        for row in &section.rows {
            append(section.title, "Data", &row.cells.iter().map(String::as_str).collect::<Vec<_>>());
        }
    }
    out
}

const POWER_PAGE_WIDTH: f32 = 760.0;
const POWER_PAGE_HEIGHT: f32 = 1075.0;
const POWER_BOTTOM: f32 = 1010.0;

fn power_page(report: &crate::power_report::Report, first: bool) -> (String, f32) {
    let mut page = svg(0.0, 0.0, POWER_PAGE_WIDTH, POWER_PAGE_HEIGHT);
    page.push_str(r##"<rect x="40" y="62" width="680" height="2" fill="#12685b"/>"##);
    text(&mut page, 40.0, 48.0, 12, "start", "DARAH / ENGINEERING REVIEW");
    let mut y = 87.0;
    if first {
        text(&mut page, 40.0, 109.0, 34, "start", "Power Review");
        y = 136.0;
    }
    for value in [&report.title, &format!("{} / {}", report.scope, report.mode)] {
        let lines = power_wrap(value, 680.0, 13);
        for line in lines.iter().take(2) {
            text(&mut page, 40.0, y, 13, "start", line);
            y += 17.0;
        }
        if lines.len() > 2 {
            text(&mut page, 40.0, y, 11, "start", "… Full name in the HTML and CSV exports");
            y += 17.0;
        }
    }
    text(&mut page, 40.0, y + 3.0, 10, "start", &format!("Project revision SHA-256 · {}", report.revision));
    (page, y + 26.0)
}

fn power_table_header(page: &mut String, y: &mut f32, title: &str, columns: &[&str], widths: &[f32], continued: bool) {
    let heading = if continued { format!("{title} (continued)") } else { title.to_owned() };
    text(page, 40.0, *y + 20.0, 18, "start", &heading);
    *y += 28.0;
    let headers: Vec<_> = columns.iter().zip(widths).map(|(column, width)| power_cell(column, *width)).collect();
    let height = headers.iter().map(Vec::len).max().unwrap_or(1) as f32 * 15.0 + 10.0;
    power_row(page, *y, height, widths, &headers, 0, usize::MAX, "#dfeef0", None);
    *y += height;
}

fn power_cell(value: &str, width: f32) -> Vec<String> {
    power_wrap(value, width - 16.0, 12)
}

fn power_wrap(value: &str, width: f32, size: usize) -> Vec<String> {
    // Conservative glyph widths for the PDF renderer's proportional sans-serif face.
    wrap_weighted(value, (width * 100.0) as usize, |ch| size * match ch {
        ' ' | 'i' | 'l' | 'I' | '.' | ',' | ':' | '|' => 42,
        'm' | 'w' | 'M' | 'W' | '@' => 100,
        'A'..='Z' => 82,
        'a'..='z' | '0'..='9' => 68,
        '·' | 'µ' | '±' | '×' | '→' | '≥' | '−' => 85,
        ch if ch.is_ascii() => 72,
        _ => 110,
    })
}

fn power_row(page: &mut String, y: f32, height: f32, widths: &[f32], cells: &[Vec<String>],
    offset: usize, count: usize, color: &str, verdict: Option<VerdictColors>)
{
    let background = verdict.map_or(color, |verdict| verdict.background);
    let _ = writeln!(page, r#"<rect x="40" y="{y}" width="680" height="{height}" fill="{background}"/>"#);
    if let Some(verdict) = verdict {
        let _ = writeln!(page, r#"<rect x="40" y="{y}" width="4" height="{height}" fill="{}"/>"#, verdict.accent);
    }
    let mut x = 48.0;
    for (width, lines) in widths.iter().zip(cells) {
        for (index, line) in lines.iter().skip(offset).take(count).enumerate() {
            text(page, x, y + 15.0 + index as f32 * 15.0, 12, "start", line);
        }
        x += width;
    }
}

fn power_pdf_pages(report: &crate::power_report::Report) -> Vec<String> {
    let (mut page, mut y) = power_page(report, true);
    let mut pages = Vec::new();
    for line in power_wrap(crate::power_report::BASIS, 680.0, 11) {
        text(&mut page, 40.0, y + 10.0, 11, "start", &line);
        y += 15.0;
    }
    y += 12.0;
    for section in &report.sections {
        if section.rows.is_empty() {
            if section.title == "Rail budgets" {
                text(&mut page, 40.0, y + 20.0, 14, "start", "No supply rails in this scope.");
                y += 36.0;
            }
            continue;
        }
        let widths: &[f32] = match section.title {
            "Rail budgets" => &[170.0, 135.0, 148.0, 112.0, 115.0],
            "Inputs and provenance" => &[200.0, 185.0, 295.0],
            "Calculations" => &[113.0, 125.0, 442.0],
            _ => match section.columns.len() {
                1 => &[680.0],
                2 => &[220.0, 460.0],
                _ => &[230.0, 230.0, 220.0],
            },
        };
        if y + 125.0 > POWER_BOTTOM {
            let (next, top) = power_page(report, false);
            pages.push(std::mem::replace(&mut page, next));
            y = top;
        }
        power_table_header(&mut page, &mut y, section.title, section.columns, widths, false);
        for (index, row) in section.rows.iter().enumerate() {
            let cells: Vec<_> = row.cells.iter().zip(widths).map(|(value, width)| power_cell(value, *width)).collect();
            let line_count = cells.iter().map(Vec::len).max().unwrap_or(0).max(1);
            let color = if index % 2 == 0 { "#f1f6f9" } else { "#ffffff" };
            let verdict = row.status.map(verdict_colors);
            let mut offset = 0;
            while offset < line_count {
                let available = ((POWER_BOTTOM - y - 10.0) / 15.0).floor() as usize;
                if available == 0 || (offset == 0 && available < line_count && available < 4) {
                    let (next, top) = power_page(report, false);
                    pages.push(std::mem::replace(&mut page, next));
                    y = top;
                    power_table_header(&mut page, &mut y, section.title, section.columns, widths, true);
                    continue;
                }
                let count = available.min(line_count - offset);
                let height = count as f32 * 15.0 + 10.0;
                power_row(&mut page, y, height, widths, &cells, offset, count, color, verdict);
                y += height;
                offset += count;
            }
        }
        y += 14.0;
    }
    pages.push(page);
    let count = pages.len();
    for (index, page) in pages.iter_mut().enumerate() {
        page.push_str(r##"<rect x="40" y="1026" width="680" height="1" fill="#cbd9e4"/>"##);
        text(page, 40.0, 1047.0, 10, "start", "DARAH / POWER REVIEW");
        text(page, 720.0, 1047.0, 10, "end", &format!("{} / {count}", index + 1));
        page.push_str("</g></svg>");
    }
    pages
}

#[cfg(test)]
mod power_report_tests {
    use super::*;
    use crate::{
        interface_ui::{self, NewBlock, ProtectionType},
        model::{BlockPosition, BlockSize, Port, PortKind, Role},
        power::Kind,
        rail::VoltagePreset,
    };

    #[test]
    fn detailed_power_review_matches_live_model_and_preserves_mode_and_scope() -> Result<()> {
        let mut project = Project::new();
        project.title = "Pump <controller>".into();
        let source_block = project.add_block(Role::Power, "DC source".into(), false)?;
        let protection = interface_ui::create_block(&mut project, "Series protection",
            NewBlock::Protection(ProtectionType::SeriesPower, None))?;
        let buck = project.add_power_converter("Buck regulator".into(), Kind::Buck)?;
        let mcu = project.add_block(Role::Processing, "MCU".into(), false)?;
        let mut source = Port::new(source_block, PortKind::Rail);
        let preset = VoltagePreset { nominal_mv: 5000, tolerance_bp: 500 };
        let (min, max) = preset.bounds();
        (source.min_uv, source.max_uv, source.current_ua, source.voltage_preset) =
            (Some(min), Some(max), Some(500_000), Some(preset));
        source.note = "Bench supply label".into();
        let source = project.set_port(source)?;
        let (entry, protected) = (project.interfaces.protections[0].entry, project.interfaces.protections[0].exit.unwrap());
        let mut channel = project.conversions[0].clone();
        let (input, output) = (channel.input, channel.output);
        let mut accepted = project.port(input).unwrap().clone();
        (accepted.min_uv, accepted.max_uv) = (Some(min), Some(max));
        project.set_port(accepted)?;
        let mut regulated = project.port(output).unwrap().clone();
        let preset = VoltagePreset { nominal_mv: 3300, tolerance_bp: 500 };
        let (min, max) = preset.bounds();
        (regulated.min_uv, regulated.max_uv, regulated.current_ua, regulated.voltage_preset) =
            (Some(min), Some(max), Some(1_000_000), Some(preset));
        project.set_port(regulated)?;
        channel.efficiency_bp = Some(9000);
        project.set_conversion(channel.clone())?;
        let mut load = Port::new(mcu, PortKind::Load);
        (load.min_uv, load.max_uv, load.current_ua) = (Some(min), Some(max), Some(400_000));
        let load = project.set_port(load)?;
        project.connect(source, entry)?;
        project.connect(protected, input)?;
        project.connect(output, load)?;
        project.sync_protection_rails();
        let run = project.add_mode("Pump active".into())?;
        project.set_mode_current(run, load, Some(Some(760_000)))?;
        let scope = project.add_subsystem("Controller".into(), None,
            BlockPosition { x: 0, y: 0 }, BlockSize { width: 375, height: 250 }, &[protection, buck, mcu])?;
        let report = crate::power_report::Report::build(&project, project.mode(run), Some(scope))?;
        let budgets = &report.sections[0].rows;
        assert_eq!(budgets.len(), 2);
        assert_eq!(&budgets[0].cells[1..], &["0.616 A / 0.5 A", "2.78667 W / 2.5 W", "−0.116 A", "Failed"]);
        assert_eq!(&budgets[1].cells[1..], &["0.76 A / 1 A", "2.508 W / 3.3 W", "+0.24 A", "Satisfied within scope"]);

        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("review.html");
        write_power_review(&project, project.mode(run), Some(scope), PowerReportFormat::Html, &file)?;
        let html = fs::read_to_string(file).unwrap();
        for fragment in ["Pump &lt;controller&gt;", "Controller / Pump active", "Bench supply label",
            "Operating-mode current override", "Inherited through series protection", "Power boundary interfaces",
            "0.76 A × 3.465 V / (4.75 V × 0.9) = 0.616 A", "0.557334 A"] {
            assert!(html.contains(fragment), "{fragment}");
        }
        assert!(power_csv(&report).contains("\"Calculations\",\"Data\""));
        let pages = power_pdf_pages(&report);
        for status in [crate::review::Status::Pass, crate::review::Status::Fail] {
            let colors = verdict_colors(status);
            assert!(html.contains(colors.accent));
            assert!(pages.iter().any(|page| page.contains(colors.accent)));
            assert!(!power_csv(&report).contains(colors.accent));
        }
        assert!(pages.iter().any(|page| page.contains("0.557334")));
        assert!(pages.iter().all(|page| page.ends_with("</svg>")));

        channel.efficiency_bp = None;
        project.set_conversion(channel)?;
        let incomplete = crate::power_report::Report::build(&project, project.mode(run), Some(scope))?;
        let html = power_html(&incomplete);
        assert!(html.contains("Ideal lower bound"));
        assert!(html.contains("Converter efficiency is undeclared."));
        assert!(html.contains("Unresolved"));
        let colors = verdict_colors(crate::review::Status::Open);
        assert!(html.contains(colors.accent));
        assert!(power_pdf_pages(&incomplete).iter().any(|page| page.contains(colors.accent)));
        Ok(())
    }
}
