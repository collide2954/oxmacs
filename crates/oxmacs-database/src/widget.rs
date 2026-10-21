use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Paragraph as _, Text};
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::alignment::Vertical;
use iced::mouse::{self, ScrollDelta};
use iced::{
    Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme, Vector,
    keyboard,
};
use oxmacs_core::applet::Payload;
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Theme as Faces;
use oxmacs_gui::style;
use oxmacs_gui::units::{floor_to_isize, floor_to_usize, to_f32};

use crate::driver::Value;
use crate::grid::{Grid, PAD, shown};
use crate::table::{GridClick, GridScroll, GridViewport};

pub(crate) const ROW_SCALE: f32 = 1.45;
const WHEEL_ROWS: f32 = 3.0;
const ABOVE_ROWS: f32 = 2.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Geometry {
    pub(crate) origin: Point,
    pub(crate) size: Size,
    pub(crate) cell: Size,
    pub(crate) row_height: f32,
    pub(crate) gutter: f32,
    pub(crate) boxes: Vec<(usize, f32, f32)>,
    pub(crate) rows: usize,
}

fn digits(number: u64) -> usize {
    number.to_string().len()
}

impl Geometry {
    pub(crate) fn new(grid: &Grid, origin: Point, size: Size, cell: Size) -> Self {
        let row_height = (cell.height * ROW_SCALE).round();
        let gutter = to_f32(digits(grid.last_row() + 1).max(2) + PAD) * cell.width;
        let right = origin.x + size.width;
        let mut boxes = Vec::new();
        let mut x = origin.x + gutter;
        for column in grid.left..grid.columns.len() {
            if x >= right {
                break;
            }
            let width = to_f32(grid.width(column) + PAD) * cell.width;
            boxes.push((column, x, width));
            x += width;
        }
        let rows = if row_height > 0.0 {
            floor_to_usize((size.height - row_height * ABOVE_ROWS).max(0.0) / row_height)
        } else {
            0
        };
        Self {
            origin,
            size,
            cell,
            row_height,
            gutter,
            boxes,
            rows,
        }
    }

    pub(crate) fn width_in_cells(&self) -> usize {
        if self.cell.width <= 0.0 {
            return 0;
        }
        floor_to_usize((self.size.width - self.gutter).max(0.0) / self.cell.width)
    }

    fn column_at(&self, x: f32) -> Option<usize> {
        self.boxes
            .iter()
            .find(|(_, left, width)| x >= *left && x < left + width)
            .map(|(column, _, _)| *column)
    }

    pub(crate) fn hit(&self, point: Point) -> Option<(usize, usize)> {
        let y = point.y - self.origin.y - self.row_height * ABOVE_ROWS;
        if y < 0.0 || self.row_height <= 0.0 {
            return None;
        }
        let row = floor_to_usize(y / self.row_height);
        Some((row, self.column_at(point.x)?))
    }
}

pub(crate) fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

fn whole(value: f32) -> i64 {
    i64::try_from(floor_to_isize(value.round())).unwrap_or(0)
}

pub(crate) fn wheel(delta: ScrollDelta, row_height: f32) -> (i64, i64) {
    match delta {
        ScrollDelta::Lines { x, y } => (whole(-y * WHEEL_ROWS), whole(-x)),
        ScrollDelta::Pixels { x, y } => {
            let height = row_height.max(1.0);
            (whole(-y / height), whole(-x / height))
        }
    }
}

const SAMPLE: &str = "MMMMMMMMMMMMMMMMMMMM";
const HEADER_TINT: f32 = 0.07;
const ZEBRA_TINT: f32 = 0.03;
const LINE_TINT: f32 = 0.14;
const ROW_TINT: f32 = 0.08;
const SELECTION_ALPHA: f32 = 0.55;

#[derive(Debug, Default)]
struct State {
    measured: Option<(Font, Pixels)>,
    cell: Size,
    modifiers: keyboard::Modifiers,
}

#[derive(Debug)]
pub(crate) struct GridWidget<'a> {
    grid: &'a Grid,
    buffer: BufferId,
    faces: Faces,
    font: Font,
    size: Pixels,
    focused: bool,
}

impl<'a> GridWidget<'a> {
    pub(crate) fn new(
        grid: &'a Grid,
        buffer: BufferId,
        faces: Faces,
        font: Font,
        size: f32,
        focused: bool,
    ) -> Self {
        Self {
            grid,
            buffer,
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

    fn tint(&self, amount: f32) -> Color {
        style::color(
            self.faces
                .default
                .background
                .mix(self.faces.default.foreground, amount),
        )
    }

    fn cell_face(&self, value: &Value) -> (String, oxmacs_core::theme::Color, bool) {
        let faces = &self.faces;
        match value {
            Value::Null => (shown(value), faces.shadow, false),
            Value::Integer(_) | Value::Real(_) => (shown(value), faces.syntax.constant, true),
            Value::Blob(_) => (shown(value), faces.syntax.comment, false),
            Value::Text(_) => (shown(value), faces.default.foreground, false),
        }
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

fn outline<Renderer: renderer::Renderer>(renderer: &mut Renderer, bounds: Rectangle, color: Color) {
    renderer.fill_quad(
        Quad {
            bounds,
            border: Border {
                color,
                width: 1.5,
                radius: 2.0.into(),
            },
            ..Quad::default()
        },
        Color::TRANSPARENT,
    );
}

impl<Renderer> Widget<Payload, Theme, Renderer> for GridWidget<'_>
where
    Renderer: text::Renderer<Font = Font>,
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
        let geometry = Geometry::new(self.grid, bounds.position(), bounds.size(), state.cell);
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(point) = cursor
                    .position_in(bounds)
                    .map(|at| at + Vector::new(bounds.x, bounds.y))
                    && let Some((offset, column)) = geometry.hit(point)
                {
                    let row = self.grid.top + u64::try_from(offset).unwrap_or(0);
                    if row <= self.grid.last_row() {
                        shell.publish(Payload::new(GridClick {
                            buffer: self.buffer,
                            row,
                            column,
                            extend: state.modifiers.shift(),
                        }));
                    }
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let (rows, columns) = wheel(*delta, geometry.row_height);
                let (rows, columns) = if state.modifiers.shift() {
                    (0, rows.signum() + columns)
                } else {
                    (rows, columns)
                };
                shell.publish(Payload::new(GridScroll {
                    buffer: self.buffer,
                    rows,
                    columns,
                }));
                shell.capture_event();
            }
            _ => {}
        }
        let rows = geometry.rows;
        let width = geometry.width_in_cells();
        if state.cell.width > 0.0 && (rows, width) != (self.grid.shown_rows, self.grid.shown_width)
        {
            shell.publish(Payload::new(GridViewport {
                buffer: self.buffer,
                rows,
                width,
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
        let geometry = Geometry::new(self.grid, bounds.position(), bounds.size(), state.cell);
        renderer.with_layer(bounds, |renderer| self.paint(renderer, bounds, &geometry));
    }
}

struct Frame<'g> {
    bounds: Rectangle,
    geometry: &'g Geometry,
    inset: f32,
}

impl Frame<'_> {
    fn row_top(&self, offset: usize) -> f32 {
        self.bounds.y + self.geometry.row_height * (to_f32(offset) + ABOVE_ROWS)
    }

    fn header_top(&self) -> f32 {
        self.bounds.y + self.geometry.row_height
    }
}

impl GridWidget<'_> {
    fn paint<Renderer>(&self, renderer: &mut Renderer, bounds: Rectangle, geometry: &Geometry)
    where
        Renderer: text::Renderer<Font = Font>,
    {
        let frame = Frame {
            bounds,
            geometry,
            inset: (geometry.row_height - geometry.cell.height) / 2.0,
        };
        self.background(renderer, &frame);
        let grid = self.grid;
        let shown_rows: Vec<u64> = (0..=geometry.rows)
            .map(|offset| grid.top + u64::try_from(offset).unwrap_or(0))
            .take_while(|row| *row <= grid.last_row() && grid.total != Some(0))
            .collect();
        for (offset, row) in shown_rows.iter().enumerate() {
            self.row(renderer, &frame, offset, *row);
        }
        self.headers(renderer, &frame);
        self.lines(renderer, &frame, shown_rows.len());
        self.strip(renderer, &frame);
        self.notice(renderer, &frame);
    }

    fn background<Renderer: renderer::Renderer>(&self, renderer: &mut Renderer, frame: &Frame<'_>) {
        let Frame {
            bounds, geometry, ..
        } = *frame;
        let row_height = geometry.row_height;
        let body = frame.row_top(0);
        fill(
            renderer,
            bounds,
            style::color(self.faces.default.background),
        );
        fill(
            renderer,
            Rectangle::new(
                Point::new(bounds.x, frame.header_top()),
                Size::new(bounds.width, row_height),
            ),
            self.tint(HEADER_TINT),
        );
        fill(
            renderer,
            Rectangle::new(
                Point::new(bounds.x, body),
                Size::new(geometry.gutter, bounds.y + bounds.height - body),
            ),
            self.tint(HEADER_TINT),
        );
    }

    fn selected(&self, row: u64, column: usize) -> bool {
        let grid = self.grid;
        let Some(anchor) = grid.anchor else {
            return false;
        };
        let rows = anchor.row.min(grid.cursor.row)..=anchor.row.max(grid.cursor.row);
        let columns = anchor.column.min(grid.cursor.column)..=anchor.column.max(grid.cursor.column);
        rows.contains(&row) && columns.contains(&column)
    }

    fn row<Renderer>(&self, renderer: &mut Renderer, frame: &Frame<'_>, offset: usize, row: u64)
    where
        Renderer: text::Renderer<Font = Font>,
    {
        let Frame {
            bounds,
            geometry,
            inset,
        } = *frame;
        let faces = &self.faces;
        let current = row == self.grid.cursor.row;
        let y = frame.row_top(offset);
        let band = Rectangle::new(
            Point::new(bounds.x + geometry.gutter, y),
            Size::new(bounds.width - geometry.gutter, geometry.row_height),
        );
        if current {
            fill(
                renderer,
                band,
                style::color(faces.default.background.mix(faces.link, ROW_TINT)),
            );
        } else if row % 2 == 1 {
            fill(renderer, band, self.tint(ZEBRA_TINT));
        }
        let number = (row + 1).to_string();
        let left = bounds.x + geometry.gutter - to_f32(number.len() + 1) * geometry.cell.width;
        renderer.fill_text(
            self.text(number, geometry.row_height),
            Point::new(left, y + inset),
            style::color(if current {
                faces.default.foreground
            } else {
                faces.shadow
            }),
            bounds,
        );
        for (column, x, width) in &geometry.boxes {
            let area = Rectangle::new(Point::new(*x, y), Size::new(*width, geometry.row_height));
            self.cell(renderer, frame, area, row, *column);
        }
    }

    fn cell<Renderer>(
        &self,
        renderer: &mut Renderer,
        frame: &Frame<'_>,
        area: Rectangle,
        row: u64,
        column: usize,
    ) where
        Renderer: text::Renderer<Font = Font>,
    {
        let faces = &self.faces;
        let cell = frame.geometry.cell;
        if self.selected(row, column) {
            fill(
                renderer,
                area,
                style::color(faces.region).scale_alpha(SELECTION_ALPHA),
            );
        }
        if row == self.grid.cursor.row && column == self.grid.cursor.column {
            if self.focused {
                fill(renderer, area, style::color(faces.region));
            }
            outline(renderer, area, style::color(faces.cursor));
        }
        let Some(value) = self.grid.value(row, column) else {
            return;
        };
        let (shown, color, right) = self.cell_face(value);
        let shown = clip(&shown, self.grid.width(column));
        let text_width = to_f32(shown.chars().count()) * cell.width;
        let start = if right {
            area.x + area.width - cell.width - text_width
        } else {
            area.x + cell.width
        };
        renderer.fill_text(
            self.text(shown, area.height),
            Point::new(start, area.y + frame.inset),
            style::color(color),
            area,
        );
    }

    fn headers<Renderer>(&self, renderer: &mut Renderer, frame: &Frame<'_>)
    where
        Renderer: text::Renderer<Font = Font>,
    {
        let Frame {
            geometry, inset, ..
        } = *frame;
        let top = frame.header_top();
        for (column, x, width) in &geometry.boxes {
            let area = Rectangle::new(Point::new(*x, top), Size::new(*width, geometry.row_height));
            renderer.fill_text(
                self.text(self.grid.heading(*column), geometry.row_height),
                Point::new(x + geometry.cell.width, top + inset),
                style::color(self.faces.heading),
                area,
            );
        }
    }

    fn strip<Renderer>(&self, renderer: &mut Renderer, frame: &Frame<'_>)
    where
        Renderer: text::Renderer<Font = Font>,
    {
        let Frame {
            bounds,
            geometry,
            inset,
        } = *frame;
        let name = &self.grid.query.table;
        let x = bounds.x + geometry.cell.width;
        let y = bounds.y + inset;
        renderer.fill_text(
            self.text(name.clone(), geometry.row_height),
            Point::new(x, y),
            style::color(self.faces.heading),
            bounds,
        );
        renderer.fill_text(
            self.text(format!(" · {}", self.grid.counted()), geometry.row_height),
            Point::new(x + to_f32(name.chars().count()) * geometry.cell.width, y),
            style::color(self.faces.shadow),
            bounds,
        );
    }

    fn lines<Renderer: renderer::Renderer>(
        &self,
        renderer: &mut Renderer,
        frame: &Frame<'_>,
        rows: usize,
    ) {
        let Frame {
            bounds, geometry, ..
        } = *frame;
        let line = self.tint(LINE_TINT);
        let top = frame.header_top();
        let bottom = frame.row_top(rows);
        for (_, x, width) in &geometry.boxes {
            fill(
                renderer,
                Rectangle::new(
                    Point::new(x + width - 1.0, top),
                    Size::new(1.0, bottom - top),
                ),
                line,
            );
        }
        for y in [top, frame.row_top(0)] {
            fill(
                renderer,
                Rectangle::new(Point::new(bounds.x, y - 1.0), Size::new(bounds.width, 1.0)),
                line,
            );
        }
        fill(
            renderer,
            Rectangle::new(
                Point::new(bounds.x + geometry.gutter - 1.0, top),
                Size::new(1.0, bounds.y + bounds.height - top),
            ),
            line,
        );
    }

    fn notice<Renderer>(&self, renderer: &mut Renderer, frame: &Frame<'_>)
    where
        Renderer: text::Renderer<Font = Font>,
    {
        let grid = self.grid;
        let faces = &self.faces;
        let notice = if let Some(error) = &grid.error {
            Some((error.clone(), faces.error))
        } else if grid.total == Some(0) {
            Some(("No rows".to_owned(), faces.shadow))
        } else if grid.columns.is_empty() {
            Some(("Loading…".to_owned(), faces.shadow))
        } else {
            None
        };
        let Some((message, color)) = notice else {
            return;
        };
        let Frame {
            bounds,
            geometry,
            inset,
        } = *frame;
        renderer.fill_text(
            self.text(message, geometry.row_height),
            Point::new(
                bounds.x + geometry.gutter + geometry.cell.width,
                frame.row_top(0) + inset,
            ),
            style::color(color),
            bounds,
        );
    }
}
impl<'a> From<GridWidget<'a>> for Element<'a, Payload> {
    fn from(widget: GridWidget<'a>) -> Self {
        Element::new(widget)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::{Page, Query, Value};
    use crate::service::DbId;

    fn grid() -> Grid {
        let mut grid = Grid::new(
            DbId(1),
            Query {
                table: "t".to_owned(),
                ..Query::default()
            },
        );
        grid.insert(
            0,
            Page {
                columns: vec!["id".to_owned(), "name".to_owned(), "notes".to_owned()],
                rows: vec![vec![
                    Value::Integer(1),
                    Value::Text("x".repeat(10)),
                    Value::Text("y".repeat(20)),
                ]],
                rowids: vec![Some(1)],
            },
        );
        grid.total = Some(120);
        grid
    }

    const CELL: Size = Size {
        width: 10.0,
        height: 20.0,
    };

    #[test]
    fn columns_are_laid_out_after_the_row_numbers() {
        let grid = grid();

        let geometry = Geometry::new(&grid, Point::new(5.0, 7.0), Size::new(500.0, 300.0), CELL);

        assert_eq!(geometry.row_height, 29.0);
        assert_eq!(geometry.gutter, 50.0);
        assert_eq!(
            geometry.boxes,
            [(0, 55.0, 60.0), (1, 115.0, 120.0), (2, 235.0, 220.0)]
        );
        assert_eq!(geometry.rows, 8);
        assert_eq!(geometry.width_in_cells(), 45);
    }

    #[test]
    fn points_hit_the_row_and_column_under_them() {
        let grid = grid();
        let geometry = Geometry::new(&grid, Point::ORIGIN, Size::new(500.0, 300.0), CELL);

        assert_eq!(geometry.hit(Point::new(60.0, 59.0)), Some((0, 0)));
        assert_eq!(geometry.hit(Point::new(130.0, 88.0)), Some((1, 1)));
        assert_eq!(geometry.hit(Point::new(300.0, 129.0)), Some((2, 2)));
        assert_eq!(geometry.hit(Point::new(20.0, 129.0)), None);
        assert_eq!(geometry.hit(Point::new(60.0, 10.0)), None);
        assert_eq!(geometry.hit(Point::new(60.0, 40.0)), None);
        assert_eq!(geometry.hit(Point::new(480.0, 129.0)), None);
    }

    #[test]
    fn scrolled_columns_start_at_the_left_one() {
        let mut grid = grid();
        grid.left = 1;

        let geometry = Geometry::new(&grid, Point::ORIGIN, Size::new(500.0, 300.0), CELL);

        assert_eq!(geometry.boxes[0], (1, 50.0, 120.0));
    }

    #[test]
    fn long_text_is_cut_with_an_ellipsis() {
        assert_eq!(clip("short", 8), "short");
        assert_eq!(clip("exactly8", 8), "exactly8");
        assert_eq!(clip("much too long", 8), "much to…");
        assert_eq!(clip("", 0), "");
    }

    #[test]
    fn the_wheel_scrolls_rows_and_shift_scrolls_columns() {
        assert_eq!(wheel(ScrollDelta::Lines { x: 0.0, y: -1.0 }, 29.0), (3, 0));
        assert_eq!(wheel(ScrollDelta::Lines { x: 0.0, y: 2.0 }, 29.0), (-6, 0));
        assert_eq!(
            wheel(ScrollDelta::Pixels { x: 0.0, y: -58.0 }, 29.0),
            (2, 0)
        );
        assert_eq!(wheel(ScrollDelta::Lines { x: -1.0, y: 0.0 }, 29.0), (0, 1));
    }
}
