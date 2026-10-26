//! A flamegraph, laid out and rendered by hand.
//!
//! The standard shape: one row per depth, a frame's width its inclusive time, its
//! children packed at its left edge on the row below, heaviest first so the wide frames
//! are the ones worth looking at. What is unusual is only where the numbers come from —
//! each frame's width is a measurement, and a container that did not account for all of
//! its own time shows the remainder as an `(unattributed)` frame rather than hiding it
//! in a rescale.

/// What a frame is, which decides how it is coloured.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The panel's own root: the whole of what was measured.
    Root,
    /// A unit: one benchmark program, or one section of the paired workload.
    Unit,
    /// The `execute` container, or another frame whose width is measured rather than sampled.
    Measured,
    /// The instruction loop: measured time, with the mix as counts in its tooltip.
    Loop,
    /// Time the sampler charged to one native call.
    Native,
    /// Measured time no child accounted for.
    Gap,
}

/// One frame: a name, its inclusive nanoseconds, and its children.
pub struct Node {
    pub name: String,
    /// What the tooltip says beyond the name and the numbers.
    pub note: String,
    pub kind: Kind,
    /// Inclusive time: the width of the frame's own rectangle.
    pub ns: u64,
    /// Heaviest first, which is the order the layout draws them in.
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(kind: Kind, name: impl Into<String>, ns: u64, note: impl Into<String>) -> Node {
        Node { name: name.into(), note: note.into(), kind, ns, children: Vec::new() }
    }

    pub fn with(mut self, children: Vec<Node>) -> Node {
        self.children = children;
        self
    }

    /// The depth of the deepest frame under this one.
    pub fn depth(&self) -> usize {
        1 + self.children.iter().map(Node::depth).max().unwrap_or(0)
    }

    /// The time this frame spent itself, which is what it did not pass on to a child.
    fn self_ns(&self) -> u64 {
        let children: u64 = self.children.iter().map(|child| child.ns).sum();
        self.ns.saturating_sub(children)
    }
}

/// A frame placed on the graph, in nanoseconds.
pub struct Rect {
    pub x: u64,
    pub width: u64,
    pub depth: usize,
    pub name: String,
    pub note: String,
    pub kind: Kind,
    /// The frame's share of the whole panel, which decides how hot it is drawn.
    pub share: f64,
    /// The frame's time as a fraction of its parent, which is what the tooltip leads with.
    pub parent_share: f64,
}

/// Places a tree: every frame at its own depth, children at the parent's left edge.
///
/// A container's children can add up to more than it measured — the sampler is an
/// estimate — and a graph cannot show that, so the children are scaled down to fit and
/// the frame says so. A container whose children add up to less keeps its own
/// `(unattributed)` remainder, which is the measurement gap made visible.
pub fn layout(root: &Node) -> (Vec<Rect>, usize, u64) {
    let mut rects = Vec::new();
    let total = root.ns.max(1);
    place(root, 0, 0, total, total, 1.0, 1.0, &mut rects);
    (rects, root.depth(), root.ns)
}

#[allow(clippy::too_many_arguments)]
fn place(node: &Node, depth: usize, x: u64, width: u64, total: u64, share: f64, parent_share: f64, rects: &mut Vec<Rect>) {
    rects.push(Rect { x, width, depth, name: node.name.clone(), note: node.note.clone(), kind: node.kind, share, parent_share });
    if node.children.is_empty() {
        return;
    }
    let requested: u64 = node.children.iter().map(|child| child.ns).sum();
    // Only a container that over-counted needs scaling: the graph's rule is that a
    // parent is never narrower than what it holds.
    let scale = if requested > width { width as f64 / requested as f64 } else { 1.0 };
    let mut at = x;
    for child in &node.children {
        let child_width = ((child.ns as f64 * scale) as u64).max(1);
        let child_share = child_width as f64 / total as f64;
        let child_parent_share = child_width as f64 / width.max(1) as f64;
        place(child, depth + 1, at, child_width, total, child_share, child_parent_share, rects);
        at += child_width;
    }
    if scale == 1.0 {
        // What the children left over is this frame's own time.
        let used: u64 = node.children.iter().map(|child| child.ns).sum();
        if let Some(gap) = width.checked_sub(used).filter(|gap| *gap > width / 100) {
            rects.push(Rect {
                x: at,
                width: gap,
                depth: depth + 1,
                name: "(unattributed)".to_string(),
                note: format!("measured execute time no child accounted for: {} of {}", millis(gap), millis(width)),
                kind: Kind::Gap,
                share: gap as f64 / total as f64,
                parent_share: gap as f64 / width.max(1) as f64,
            });
        }
    }
}

/// A duration in milliseconds, with the digits a graph frame can carry.
pub fn millis(ns: u64) -> String {
    format!("{:.3} ms", ns as f64 / 1_000_000.0)
}

/// A duration in nanoseconds, for frames too small to read as milliseconds.
pub fn nanos(ns: u64) -> String {
    if ns < 1_000 {
        format!("{ns} ns")
    } else if ns < 1_000_000 {
        format!("{:.2} µs", ns as f64 / 1_000.0)
    } else {
        millis(ns)
    }
}

/// One flamegraph: a title, a subtitle, and the tree it draws.
pub struct Panel {
    pub title: String,
    pub subtitle: String,
    pub root: Node,
}

/// The graph's box, in pixels. Wide enough for the labels a wide frame carries, and
/// short enough that a deep tree still reads at a glance.
const WIDTH: f64 = 1500.0;
const PAD: f64 = 10.0;
const ROW_H: f64 = 17.0;
const FONT: f64 = 11.5;
/// Roughly the width of one character at [`FONT`] in the monospace stack below, which is
/// what the label fitter measures with.
const CHAR_W: f64 = 6.35;

/// Renders the panels and the notes into one self-contained SVG.
pub fn render(panels: &[Panel], notes: &[String]) -> String {
    let mut body = String::new();
    let mut y = 0.0;

    y += 34.0;
    body.push_str(&text(PAD, y, 22.0, "#f8fafc", 600.0, "Tiger: where the time goes"));
    y += 22.0;
    body.push_str(&text(
        PAD,
        y,
        FONT + 1.0,
        "#94a3b8",
        400.0,
        "Every frame's width is a measured time. Release build, in process, one machine — the numbers are this host's, not a specification.",
    ));
    y += 26.0;

    for panel in panels {
        let (rects, depth, total) = layout(&panel.root);
        let plot_w = WIDTH - 2.0 * PAD;
        let scale = plot_w / total.max(1) as f64;
        body.push_str(&text(PAD, y, FONT + 2.5, "#e2e8f0", 600.0, &panel.title));
        y += 19.0;
        body.push_str(&text(PAD, y, FONT, "#64748b", 400.0, &panel.subtitle));
        y += 16.0;
        for row in 0..depth {
            let top = y + row as f64 * ROW_H;
            for rect in rects.iter().filter(|rect| rect.depth == row) {
                let x = PAD + rect.x as f64 * scale;
                let width = rect.width as f64 * scale;
                if width < 0.7 {
                    continue;
                }
                let (fill, text_fill) = colour(rect);
                body.push_str(&format!(
                    "<rect x=\"{x:.2}\" y=\"{top:.2}\" width=\"{width:.2}\" height=\"{h:.2}\" fill=\"{fill}\"><title>{tip}</title></rect>",
                    h = ROW_H - 1.0,
                    tip = escape(&tooltip(rect)),
                ));
                let room = (width - 8.0) / CHAR_W;
                if room >= 4.0 {
                    let label = fit(&rect.name, room as usize);
                    let combined = format!("{label}  {}", nanos(rect.width));
                    let shown = if combined.chars().count() <= room as usize { combined } else { label };
                    body.push_str(&text(x + 3.0, top + ROW_H - 4.5, FONT, text_fill, 400.0, &shown));
                }
            }
        }
        y += depth as f64 * ROW_H + 4.0;
        body.push_str(&format!(
            "<line x1=\"{PAD}\" y1=\"{y:.2}\" x2=\"{:.2}\" y2=\"{y:.2}\" stroke=\"#1f2937\" stroke-width=\"1\"/>",
            WIDTH - PAD
        ));
        y += 14.0;
        body.push_str(&text(PAD, y, FONT, "#64748b", 400.0, &format!("whole panel: {}", millis(total))));
        y += 28.0;
    }

    body.push_str(&text(PAD, y, FONT + 1.5, "#e2e8f0", 600.0, "Reading it"));
    y += 18.0;
    for note in notes {
        for line in wrap(note, ((WIDTH - 2.0 * PAD) / CHAR_W) as usize) {
            body.push_str(&text(PAD, y, FONT, "#94a3b8", 400.0, &line));
            y += 15.0;
        }
    }
    y += 8.0;
    for (kind, label) in [
        (Kind::Unit, "a unit: one program, or one workload section"),
        (Kind::Measured, "measured: the floor, parse + compile, execute"),
        (Kind::Native, "measured: a native call — a built-in, or a string method"),
        (Kind::Loop, "measured: the instruction loop, whose mix of shapes is a count"),
        (Kind::Gap, "measured time no child accounted for"),
    ] {
        let sample = Rect { x: 0, width: 10, depth: 0, name: String::new(), note: String::new(), kind, share: 0.3, parent_share: 0.3 };
        let (fill, _) = colour(&sample);
        body.push_str(&format!("<rect x=\"{PAD}\" y=\"{:.2}\" width=\"12\" height=\"12\" fill=\"{fill}\"/>", y - 10.0));
        body.push_str(&text(PAD + 18.0, y, FONT, "#94a3b8", 400.0, label));
        y += 16.0;
    }
    y += 10.0;

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{WIDTH}\" height=\"{h:.0}\" viewBox=\"0 0 {WIDTH} {h:.0}\" font-family=\"ui-monospace, SFMono-Regular, Menlo, Consolas, monospace\">\n<rect width=\"{WIDTH}\" height=\"{h:.0}\" fill=\"#0b1120\"/>\n{body}\n</svg>\n",
        h = y
    )
}

/// What a frame's tooltip says: what it is, how long it took, and what share that is.
fn tooltip(rect: &Rect) -> String {
    format!(
        "{}\n{}\ntime: {} ({} ns)\nshare of the panel: {:.2}%\nshare of its parent: {:.2}%",
        rect.name,
        rect.note,
        nanos(rect.width),
        rect.width,
        rect.share * 100.0,
        rect.parent_share * 100.0
    )
}

/// A frame's fill and the colour of the text on it.
///
/// The palette is the flamegraph one: warm for the graph itself, with a frame's own
/// weight deciding how hot it is drawn, so the heaviest thing in a panel is what the eye
/// lands on. Cool blue is what was measured rather than sampled, green is native time,
/// and the remainder is grey.
fn colour(rect: &Rect) -> (String, &'static str) {
    let (hue, sat, light) = match rect.kind {
        Kind::Root => (12.0, 45.0, 22.0),
        // A unit is amber, and one container's own frame is a near-neutral so that the
        // shapes inside it — the actual weak points — are what carries the colour.
        Kind::Unit => (38.0, 88.0, 60.0 - 16.0 * rect.share.min(1.0)),
        Kind::Measured => {
            if rect.name.starts_with("parse") {
                (203.0, 62.0, 56.0 - 8.0 * rect.share.min(1.0))
            } else if rect.name.starts_with("floor") {
                (220.0, 10.0, 56.0 - 8.0 * rect.share.min(1.0))
            } else {
                (30.0, 12.0, 68.0 - 8.0 * rect.share.min(1.0))
            }
        }
        // The loop is the one frame whose inside is not a measurement: drawn as a neutral, so
        // the eye does not read its width as an instruction's cost.
        Kind::Loop => (215.0, 40.0, 44.0 - 8.0 * rect.parent_share.min(1.0)),
        Kind::Native => (152.0, 55.0, 70.0 - 32.0 * rect.parent_share.min(1.0)),
        Kind::Gap => (220.0, 8.0, 32.0),
    };
    let text_fill = if light > 56.0 { "#0b1120" } else { "#f8fafc" };
    (format!("hsl({hue:.0}, {sat:.0}%, {light:.1}%)"), text_fill)
}

/// One line of text, positioned by its baseline.
fn text(x: f64, y: f64, size: f64, fill: &str, weight: f64, content: &str) -> String {
    format!(
        "<text x=\"{x:.2}\" y=\"{y:.2}\" font-size=\"{size:.1}\" fill=\"{fill}\" font-weight=\"{weight:.0}\">{}</text>",
        escape(content)
    )
}

/// Truncates a label to the room a frame has, keeping as much of it as fits.
fn fit(label: &str, room: usize) -> String {
    if label.chars().count() <= room {
        return label.to_string();
    }
    let taken: String = label.chars().take(room.saturating_sub(1)).collect();
    format!("{taken}…")
}

/// Breaks a line at spaces, so a note fits the graph's width.
fn wrap(line: &str, room: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in line.split(' ') {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > room {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Escapes the characters that would end an SVG text node or an attribute.
fn escape(content: &str) -> String {
    content.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The folded stacks a graph of this shape would be loaded from, one line per frame.
///
/// Every frame is emitted with the time it did not pass on to a child, so a tool that
/// reads folded input sees the same graph this module draws.
pub fn folded(root: &Node) -> String {
    let mut out = String::new();
    fold(root, &mut Vec::new(), &mut out);
    out
}

fn fold(node: &Node, path: &mut Vec<String>, out: &mut String) {
    path.push(node.name.clone());
    if node.self_ns() > 0 || node.children.is_empty() {
        out.push_str(&format!("{} {}\n", path.join(";"), node.self_ns()));
    }
    for child in &node.children {
        fold(child, path, out);
    }
    path.pop();
}
