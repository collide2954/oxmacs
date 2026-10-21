use std::ops::Range;

use iced::advanced::graphics::geometry::{self, Frame, LineCap, Path, Stroke};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Paragraph as _, Text};
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::alignment::Vertical;
use iced::mouse::{self, ScrollDelta};
use iced::{
    Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme, Vector,
};
use oxmacs_core::applet::Payload;
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::{Highlight, NICK_COLORS, Theme as Faces};
use oxmacs_gui::style;
use oxmacs_gui::units::{floor_to_isize, floor_to_usize, to_f32};

use crate::graph::GraphRow;
use crate::log::{self, LogView};
use crate::status;

const ROW_SCALE: f32 = 1.45;
const LIST_SHARE: f32 = 0.6;
const LANE_CELLS: f32 = 2.0;
const WHEEL_ROWS: f32 = 3.0;
const SAMPLE: &str = "MMMMMMMMMMMMMMMMMMMM";
const ROW_TINT: f32 = 0.08;
const LINE_TINT: f32 = 0.14;
const PILL_ALPHA: f32 = 0.22;
const PILL_INSET: f32 = 0.12;
const PILL_PADDING: f32 = 0.35;
const NODE: f32 = 0.22;
const EDGE: f32 = 2.0;
const AUTHOR: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LogViewport {
    pub(crate) buffer: BufferId,
    pub(crate) rows: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LogScroll {
    pub(crate) buffer: BufferId,
    pub(crate) rows: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LogClick {
    pub(crate) buffer: BufferId,
    pub(crate) row: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Geometry {
    pub(crate) origin: Point,
    pub(crate) size: Size,
    pub(crate) cell: Size,
    pub(crate) row_height: f32,
    pub(crate) list_height: f32,
    pub(crate) rows: usize,
}

impl Geometry {
    pub(crate) fn new(origin: Point, size: Size, cell: Size, details: bool) -> Self {
        let row_height = (cell.height * ROW_SCALE).round();
        let list_height = if details {
            (size.height * LIST_SHARE).round()
        } else {
            size.height
        };
        let rows = if row_height > 0.0 {
            floor_to_usize(list_height / row_height)
        } else {
            0
        };
        Self {
            origin,
            size,
            cell,
            row_height,
            list_height,
            rows,
        }
    }

    pub(crate) fn hit(&self, point: Point) -> Option<usize> {
        let y = point.y - self.origin.y;
        if y < 0.0 || self.row_height <= 0.0 || point.x < self.origin.x {
            return None;
        }
        let row = floor_to_usize(y / self.row_height);
        (row < self.rows).then_some(row)
    }

    pub(crate) fn lane_width(&self) -> f32 {
        (self.cell.width * LANE_CELLS).round()
    }

    pub(crate) fn lane_x(&self, lane: usize) -> f32 {
        self.origin.x + self.cell.width / 2.0 + self.lane_width() * (to_f32(lane) + 0.5)
    }

    pub(crate) fn graph_width(&self, lanes: usize) -> f32 {
        self.cell.width + self.lane_width() * to_f32(lanes.max(1))
    }

    pub(crate) fn row_top(&self, offset: usize) -> f32 {
        self.origin.y + self.row_height * to_f32(offset)
    }
}

fn whole(value: f32) -> i64 {
    i64::try_from(floor_to_isize(value.round())).unwrap_or(0)
}

pub(crate) fn wheel(delta: ScrollDelta, row_height: f32) -> i64 {
    match delta {
        ScrollDelta::Lines { y, .. } => whole(-y * WHEEL_ROWS),
        ScrollDelta::Pixels { y, .. } => whole(-y / row_height.max(1.0)),
    }
}

pub(crate) fn lane_face(lane: usize) -> Highlight {
    Highlight::Nick(u8::try_from(lane % usize::from(NICK_COLORS)).unwrap_or(0))
}

#[derive(Debug, Default)]
struct State {
    measured: Option<(Font, Pixels)>,
    cell: Size,
}

#[derive(Debug)]
pub(crate) struct LogWidget<'a> {
    view: &'a LogView,
    buffer: BufferId,
    selected: usize,
    now: i64,
    faces: Faces,
    font: Font,
    size: Pixels,
    focused: bool,
}

impl<'a> LogWidget<'a> {
    pub(crate) fn new(
        view: &'a LogView,
        buffer: BufferId,
        selected: usize,
        faces: Faces,
        font: Font,
        size: f32,
        focused: bool,
    ) -> Self {
        Self {
            view,
            buffer,
            selected,
            now: crate::view::now(),
            faces,
            font,
            size: Pixels(size),
            focused,
        }
    }

    fn text<C>(&self, content: C, height: f32) -> Text<C, Font> {
        Text {
            content,
            bounds: Size::new(f32::INFINITY, height),
            size: self.size,
            line_height: text::LineHeight::default(),
            font: self.font,
            align_x: text::Alignment::Left,
            align_y: Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::None,
        }
    }

    fn color(&self, face: Highlight) -> Color {
        style::color(self.faces.highlight(face))
    }

    fn tint(&self, amount: f32) -> Color {
        style::color(
            self.faces
                .default
                .background
                .mix(self.faces.default.foreground, amount),
        )
    }

    fn geometry(&self, bounds: Rectangle, cell: Size) -> Geometry {
        Geometry::new(
            bounds.position(),
            bounds.size(),
            cell,
            !self.view.about.text.is_empty(),
        )
    }
}

fn fill<Renderer: renderer::Renderer>(renderer: &mut Renderer, bounds: Rectangle, color: Color) {
    renderer.fill_quad(
        Quad {
            bounds,
            ..Quad::default()
        },
        color,
    );
}

fn pill<Renderer: renderer::Renderer>(renderer: &mut Renderer, area: Rectangle, color: Color) {
    renderer.fill_quad(
        Quad {
            bounds: area,
            border: Border {
                radius: (area.height / 2.0).into(),
                ..Border::default()
            },
            ..Quad::default()
        },
        color,
    );
}

impl<Renderer> Widget<Payload, Theme, Renderer> for LogWidget<'_>
where
    Renderer: text::Renderer<Font = Font> + geometry::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        if state.measured != Some((self.font, self.size)) {
            let bounds =
                Renderer::Paragraph::with_text(self.text(SAMPLE, f32::INFINITY)).min_bounds();
            state.cell = Size::new(bounds.width / to_f32(SAMPLE.len()), bounds.height);
            state.measured = Some((self.font, self.size));
        }
        layout::Node::new(limits.resolve(Length::Fill, Length::Fill, Size::ZERO))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Payload>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        let geometry = self.geometry(bounds, state.cell);
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(point) = cursor
                    .position_in(bounds)
                    .map(|at| at + Vector::new(bounds.x, bounds.y))
                    && let Some(row) = geometry.hit(point)
                {
                    shell.publish(Payload::new(LogClick {
                        buffer: self.buffer,
                        row,
                    }));
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                shell.publish(Payload::new(LogScroll {
                    buffer: self.buffer,
                    rows: wheel(*delta, geometry.row_height),
                }));
                shell.capture_event();
            }
            _ => {}
        }
        if state.cell.width > 0.0 && geometry.rows != self.view.shown {
            shell.publish(Payload::new(LogViewport {
                buffer: self.buffer,
                rows: geometry.rows,
            }));
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let geometry = self.geometry(bounds, state.cell);
        renderer.with_layer(bounds, |renderer| self.paint(renderer, bounds, &geometry));
    }
}

impl LogWidget<'_> {
    fn visible(&self, geometry: &Geometry) -> Range<usize> {
        let start = self.view.top.min(self.view.entries.len());
        start..(start + geometry.rows + 1).min(self.view.entries.len())
    }

    fn paint<Renderer>(&self, renderer: &mut Renderer, bounds: Rectangle, geometry: &Geometry)
    where
        Renderer: text::Renderer<Font = Font> + geometry::Renderer,
    {
        fill(
            renderer,
            bounds,
            style::color(self.faces.default.background),
        );
        let shown = self.visible(geometry);
        let lanes = shown
            .clone()
            .filter_map(|index| self.view.graph.get(index))
            .map(GraphRow::width)
            .max()
            .unwrap_or(1);
        let list = Rectangle::new(
            bounds.position(),
            Size::new(bounds.width, geometry.list_height),
        );
        renderer.with_layer(list, |renderer| {
            for (offset, index) in shown.clone().enumerate() {
                self.band(renderer, geometry, offset, index, bounds.width);
            }
            self.graph(renderer, geometry, shown.clone(), bounds.size());
            for (offset, index) in shown.clone().enumerate() {
                self.columns(renderer, geometry, offset, index, lanes, bounds);
            }
            if self.view.entries.is_empty() {
                renderer.fill_text(
                    self.text("No commits".to_owned(), geometry.row_height),
                    Point::new(
                        bounds.x + geometry.cell.width,
                        bounds.y + (geometry.row_height - geometry.cell.height) / 2.0,
                    ),
                    self.color(Highlight::Comment),
                    bounds,
                );
            }
        });
        if !self.view.about.text.is_empty() {
            self.details(renderer, geometry, bounds);
        }
    }

    fn band<Renderer: renderer::Renderer>(
        &self,
        renderer: &mut Renderer,
        geometry: &Geometry,
        offset: usize,
        index: usize,
        width: f32,
    ) {
        if index != self.selected {
            return;
        }
        let faces = &self.faces;
        let amount = if self.focused {
            ROW_TINT * 2.0
        } else {
            ROW_TINT
        };
        fill(
            renderer,
            Rectangle::new(
                Point::new(geometry.origin.x, geometry.row_top(offset)),
                Size::new(width, geometry.row_height),
            ),
            style::color(faces.default.background.mix(faces.link, amount)),
        );
    }

    fn graph<Renderer>(
        &self,
        renderer: &mut Renderer,
        geometry: &Geometry,
        shown: Range<usize>,
        size: Size,
    ) where
        Renderer: geometry::Renderer,
    {
        let origin = Vector::new(geometry.origin.x, geometry.origin.y);
        let mut frame = Frame::new(renderer, size);
        let local = Geometry {
            origin: Point::ORIGIN,
            ..geometry.clone()
        };
        for (offset, index) in shown.enumerate() {
            let Some(row) = self.view.graph.get(index) else {
                continue;
            };
            self.edges(&mut frame, &local, offset, row);
        }
        renderer.with_translation(origin, |renderer| {
            renderer.draw_geometry(frame.into_geometry());
        });
    }

    fn edges<Renderer: geometry::Renderer>(
        &self,
        frame: &mut Frame<Renderer>,
        geometry: &Geometry,
        offset: usize,
        row: &GraphRow,
    ) {
        let top = geometry.row_top(offset);
        let bottom = top + geometry.row_height;
        let middle = top + geometry.row_height / 2.0;
        let node = Point::new(geometry.lane_x(row.lane), middle);
        let stroke = |lane: usize| {
            Stroke::default()
                .with_color(self.color(lane_face(lane)))
                .with_width(EDGE)
                .with_line_cap(LineCap::Round)
        };
        for lane in &row.through {
            let x = geometry.lane_x(*lane);
            frame.stroke(
                &Path::line(Point::new(x, top), Point::new(x, bottom)),
                stroke(*lane),
            );
        }
        for lane in &row.above {
            let from = Point::new(geometry.lane_x(*lane), top);
            frame.stroke(&curve(from, node), stroke(*lane));
        }
        for lane in &row.below {
            let to = Point::new(geometry.lane_x(*lane), bottom);
            frame.stroke(&curve(node, to), stroke(*lane));
        }
        let radius = (geometry.lane_width().min(geometry.row_height) * NODE).max(2.0);
        frame.fill(
            &Path::circle(node, radius + EDGE / 2.0),
            style::color(self.faces.default.background),
        );
        frame.fill(&Path::circle(node, radius), self.color(lane_face(row.lane)));
    }

    fn columns<Renderer>(
        &self,
        renderer: &mut Renderer,
        geometry: &Geometry,
        offset: usize,
        index: usize,
        lanes: usize,
        bounds: Rectangle,
    ) where
        Renderer: text::Renderer<Font = Font>,
    {
        let Some(entry) = self.view.entries.get(index) else {
            return;
        };
        let cell = geometry.cell;
        let top = geometry.row_top(offset);
        let inset = (geometry.row_height - cell.height) / 2.0;
        let y = top + inset;
        let ago = status::ago(entry.time, self.now);
        let author = status::shorten(&entry.author, AUTHOR);
        let right = bounds.x + bounds.width - cell.width;
        let ago_x = right - to_f32(ago.chars().count()) * cell.width;
        let author_x = right - to_f32(AUTHOR + 12) * cell.width;
        let mut x = bounds.x + geometry.graph_width(lanes);
        let row = Rectangle::new(
            Point::new(x, top),
            Size::new(author_x - x, geometry.row_height),
        );
        for label in self.view.labels.get(&entry.id).into_iter().flatten() {
            let width = to_f32(label.name.chars().count()) * cell.width;
            let color = self.color(log::face(label.kind));
            let pad = cell.width * PILL_PADDING;
            let height = geometry.row_height * (1.0 - 2.0 * PILL_INSET);
            pill(
                renderer,
                Rectangle::new(
                    Point::new(x - pad, top + geometry.row_height * PILL_INSET),
                    Size::new(width + 2.0 * pad, height),
                ),
                color.scale_alpha(PILL_ALPHA),
            );
            renderer.fill_text(
                self.text(label.name.clone(), geometry.row_height),
                Point::new(x, y),
                color,
                row,
            );
            x += width + cell.width * 1.5;
        }
        renderer.fill_text(
            self.text(entry.summary.clone(), geometry.row_height),
            Point::new(x, y),
            style::color(self.faces.default.foreground),
            row,
        );
        renderer.fill_text(
            self.text(author, geometry.row_height),
            Point::new(author_x.max(row.x), y),
            self.color(status::author_color(&entry.author)),
            bounds,
        );
        renderer.fill_text(
            self.text(ago, geometry.row_height),
            Point::new(ago_x, y),
            self.color(Highlight::Comment),
            bounds,
        );
    }

    fn details<Renderer>(&self, renderer: &mut Renderer, geometry: &Geometry, bounds: Rectangle)
    where
        Renderer: text::Renderer<Font = Font>,
    {
        let cell = geometry.cell;
        let top = bounds.y + geometry.list_height;
        fill(
            renderer,
            Rectangle::new(Point::new(bounds.x, top), Size::new(bounds.width, 1.0)),
            self.tint(LINE_TINT),
        );
        let area = Rectangle::new(
            Point::new(bounds.x, top + 1.0),
            Size::new(
                bounds.width,
                (bounds.height - geometry.list_height - 1.0).max(0.0),
            ),
        );
        let left = area.x + cell.width;
        let about = &self.view.about;
        renderer.with_layer(area, |renderer| {
            for (number, text) in about.text.split('\n').enumerate() {
                let y = area.y + cell.height * (0.5 + to_f32(number));
                if y > area.y + area.height {
                    break;
                }
                for (range, face) in about.pills.get(number).into_iter().flatten() {
                    let pad = cell.width * PILL_PADDING;
                    pill(
                        renderer,
                        Rectangle::new(
                            Point::new(left + to_f32(range.start) * cell.width - pad, y),
                            Size::new(to_f32(range.len()) * cell.width + 2.0 * pad, cell.height),
                        ),
                        self.color(*face).scale_alpha(PILL_ALPHA),
                    );
                }
                let spans = about
                    .styles
                    .get(number)
                    .map(|style| style.spans.as_slice())
                    .unwrap_or_default();
                self.line(renderer, text, spans, Point::new(left, y), cell, area);
            }
        });
    }

    fn line<Renderer>(
        &self,
        renderer: &mut Renderer,
        text: &str,
        spans: &[(Range<usize>, Highlight)],
        at: Point,
        cell: Size,
        clip: Rectangle,
    ) where
        Renderer: text::Renderer<Font = Font>,
    {
        for (range, face) in segments(text.chars().count(), spans) {
            let piece: String = text.chars().skip(range.start).take(range.len()).collect();
            let color = face.map_or_else(
                || style::color(self.faces.default.foreground),
                |face| self.color(face),
            );
            renderer.fill_text(
                self.text(piece, cell.height),
                Point::new(at.x + to_f32(range.start) * cell.width, at.y),
                color,
                clip,
            );
        }
    }
}
pub(crate) fn segments(
    width: usize,
    spans: &[(Range<usize>, Highlight)],
) -> Vec<(Range<usize>, Option<Highlight>)> {
    let mut found = Vec::new();
    let mut at = 0;
    for (range, face) in spans {
        let start = range.start.min(width);
        let end = range.end.min(width);
        if start > at {
            found.push((at..start, None));
        }
        if end > start.max(at) {
            found.push((start.max(at)..end, Some(*face)));
        }
        at = at.max(end);
    }
    if at < width {
        found.push((at..width, None));
    }
    found
}

fn curve(from: Point, to: Point) -> Path {
    if (from.x - to.x).abs() < f32::EPSILON {
        return Path::line(from, to);
    }
    let middle = f32::midpoint(from.y, to.y);
    Path::new(|builder| {
        builder.move_to(from);
        builder.bezier_curve_to(Point::new(from.x, middle), Point::new(to.x, middle), to);
    })
}

impl<'a> From<LogWidget<'a>> for Element<'a, Payload> {
    fn from(widget: LogWidget<'a>) -> Self {
        Element::new(widget)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use iced::Font;
    use iced::time::Instant;
    use oxmacs_core::editor::Editor;
    use oxmacs_gui::applet::{Context, GuiApplet};

    use super::*;
    use crate::Git;
    use crate::repo::fixtures::{git, repository};
    use crate::testing::{installed, press};

    const CELL: Size = Size {
        width: 10.0,
        height: 20.0,
    };

    #[test]
    fn the_list_takes_the_top_of_the_window_above_the_details() {
        let geometry = Geometry::new(Point::new(5.0, 7.0), Size::new(500.0, 300.0), CELL, true);

        assert_eq!(geometry.row_height, 29.0);
        assert_eq!(geometry.list_height, 180.0);
        assert_eq!(geometry.rows, 6);
        let whole = Geometry::new(Point::ORIGIN, Size::new(500.0, 300.0), CELL, false);
        assert_eq!(whole.rows, 10);
    }

    #[test]
    fn points_hit_the_row_under_them() {
        let geometry = Geometry::new(Point::ORIGIN, Size::new(500.0, 300.0), CELL, true);

        assert_eq!(geometry.hit(Point::new(50.0, 10.0)), Some(0));
        assert_eq!(geometry.hit(Point::new(50.0, 30.0)), Some(1));
        assert_eq!(geometry.hit(Point::new(50.0, 200.0)), None);
        assert_eq!(geometry.hit(Point::new(50.0, -1.0)), None);
    }

    #[test]
    fn lanes_sit_two_cells_apart() {
        let geometry = Geometry::new(Point::new(5.0, 0.0), Size::new(500.0, 300.0), CELL, false);

        assert_eq!(geometry.lane_x(0), 20.0);
        assert_eq!(geometry.lane_x(1), 40.0);
        assert_eq!(geometry.graph_width(2), 50.0);
    }

    #[test]
    fn the_wheel_scrolls_three_rows_a_notch() {
        assert_eq!(wheel(ScrollDelta::Lines { x: 0.0, y: -1.0 }, 29.0), 3);
        assert_eq!(wheel(ScrollDelta::Pixels { x: 0.0, y: 58.0 }, 29.0), -2);
    }

    #[test]
    fn spans_split_a_line_into_colored_pieces() {
        assert_eq!(
            segments(10, &[(2..4, Highlight::Comment)]),
            [
                (0..2, None),
                (2..4, Some(Highlight::Comment)),
                (4..10, None)
            ]
        );
        assert_eq!(segments(3, &[]), [(0..3, None)]);
    }

    #[test]
    fn lanes_cycle_through_the_theme_colors() {
        assert_eq!(lane_face(0), Highlight::Nick(0));
        assert_eq!(lane_face(9), Highlight::Nick(1));
    }

    #[test]
    fn the_log_draws_its_own_window() {
        let dir = repository();
        fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();
        git(dir.path(), &["commit", "-q", "-am", "Second"]);
        let mut editor = installed(dir.path());
        let context = Context {
            faces: *editor.theme(),
            palette: Theme::Light.palette(),
            font: Font::MONOSPACE,
            size: 14.0,
            now: Instant::now(),
            animations: true,
        };
        let drawn = |editor: &Editor| {
            Git.body(editor, editor.pane(), editor.selected_window(), &context)
                .is_some()
        };

        press(&mut editor, "C-x g");
        assert!(!drawn(&editor));
        press(&mut editor, "l l");
        assert!(drawn(&editor));
    }
}
