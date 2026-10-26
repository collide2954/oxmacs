pub mod audio;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ox_lang::build::{make_map, make_vector};
use ox_lang::heap::{Heap, Hold};
use ox_lang::value::{Value, print, to_float};
use ox_lang::{Error, Isolate};
use oxmacs_core::command::ScopeId;
use oxmacs_core::editor::Editor;
use oxmacs_core::plugin::PluginBuffer;
use oxmacs_core::text::BufferId;
use rodeo::atlas::Atlas;
use rodeo::clock::Clock;
use rodeo::color::Color;
use rodeo::draw::{DrawList, Outline, Shape, Sprite, raster};
use rodeo::rect::Rect;
use rodeo::texture::{TextureId, Textures};
use rodeo::vec::Vec2;

use crate::ide::log;

const MODULE: &str = "rodeo";
const NAME: &str = "*game*";
const SIZE: (u32, u32) = (320, 180);

pub const DOCS: &str = include_str!("games.ox");

#[derive(Debug, Clone, PartialEq)]
pub struct Caption {
    pub text: String,
    pub at: (f32, f32),
    pub size: f32,
    pub color: [u8; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Screen {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<[u8]>,
    pub generation: u64,
    pub smooth: bool,
    pub captions: Vec<Caption>,
    pub status: Option<String>,
}

impl Screen {
    fn blank((width, height): (u32, u32)) -> Self {
        let count = usize::try_from(u64::from(width) * u64::from(height) * 4).unwrap_or(0);
        Self {
            width,
            height,
            pixels: vec![0; count].into(),
            generation: 0,
            smooth: false,
            captions: Vec::new(),
            status: Some("Loading".to_owned()),
        }
    }
}

#[derive(Debug)]
pub struct GameBuffer {
    scope: ScopeId,
    pub screen: Screen,
}

impl PluginBuffer for GameBuffer {
    fn scope(&self) -> ScopeId {
        self.scope
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Loading,
    Running,
    Failed,
}

#[derive(Debug, Clone, Copy)]
struct Hooks {
    start: Hold,
    tick: Hold,
    draw: Hold,
}

#[derive(Debug)]
struct Game {
    buffer: BufferId,
    entry: PathBuf,
    folder: PathBuf,
    size: (u32, u32),
    smooth: bool,
    rate: u32,
    phase: Phase,
    hooks: Option<Hooks>,
    clock: Clock,
    last: Option<Instant>,
    held: BTreeSet<String>,
    pressed: BTreeSet<String>,
    released: BTreeSet<String>,
    actions: BTreeMap<String, Vec<String>>,
    list: DrawList,
    captions: Vec<Caption>,
    textures: Textures,
    reloads: Vec<PathBuf>,
    restart: bool,
    dirty: bool,
    generation: u64,
    error: Option<String>,
    alpha: f64,
}

#[derive(Debug, Default)]
struct Games {
    game: Option<Game>,
    released: Vec<Hold>,
}

fn game(editor: &Editor) -> Option<&Game> {
    editor.plugin_state::<Games>()?.game.as_ref()
}

fn game_mut(editor: &mut Editor) -> Option<&mut Game> {
    editor.plugin_state_mut::<Games>()?.game.as_mut()
}

fn playing(editor: &mut Editor) -> Result<&mut Game, Error> {
    game_mut(editor)
        .ok_or_else(|| Error::new("No game is running; play-game or M-x rodeo-play starts one"))
}

pub fn screen(editor: &Editor, buffer: BufferId) -> Option<&Screen> {
    editor
        .plugin_buffer::<GameBuffer>(buffer)
        .map(|(_, view)| &view.screen)
}

pub fn active(editor: &Editor) -> bool {
    game(editor).is_some()
}

pub fn wants_keys(editor: &Editor) -> bool {
    let Some(game) = game(editor) else {
        return false;
    };
    game.phase == Phase::Running
        && !game.clock.paused()
        && editor.buffer().id() == game.buffer
        && !editor.keys_pending()
        && editor.minibuffer().is_none()
        && editor.question().is_none()
}

pub fn key(editor: &mut Editor, name: &str, down: bool) {
    let Some(game) = game_mut(editor) else {
        return;
    };
    if down {
        if game.held.insert(name.to_owned()) {
            game.pressed.insert(name.to_owned());
        }
    } else if game.held.remove(name) {
        game.released.insert(name.to_owned());
    }
}

pub fn watch(editor: &mut Editor) {
    editor.on_saved(saved);
}

fn saved(editor: &mut Editor, id: BufferId) {
    let Some(path) = editor
        .buffer_by_id(id)
        .and_then(|buffer| buffer.file())
        .map(|file| file.path.clone())
    else {
        return;
    };
    reload(editor, &path);
}

pub fn reload(editor: &mut Editor, path: &Path) {
    let Some(game) = game_mut(editor) else {
        return;
    };
    if path.extension().is_some_and(|extension| extension == "ox") && path.starts_with(&game.folder)
    {
        game.reloads.push(path.to_owned());
    }
}

fn shown(name: &str) -> String {
    if name.contains("game") {
        name.to_owned()
    } else {
        format!("rodeo/{name}")
    }
}

fn wrong(heap: &Heap, name: &str, what: &str, value: Value) -> Error {
    Error::new(format!(
        "{} expects {what}, got {}",
        shown(name),
        print::brief(heap, value)
    ))
}

fn number(heap: &Heap, name: &str, value: Value) -> Result<f64, Error> {
    match value {
        Value::Int(number) => Ok(to_float(number)),
        Value::Float(number) => Ok(number),
        _ => Err(wrong(heap, name, "a number", value)),
    }
}

fn narrow(value: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "game sizes and colors fit in an f32"
    )]
    let narrowed = value as f32;
    narrowed
}

fn numbers(
    heap: &Heap,
    name: &str,
    what: &str,
    value: Value,
    count: usize,
) -> Result<Vec<f64>, Error> {
    let items = heap
        .vector_of(value)
        .map(|vector| vector.items(heap))
        .filter(|items| items.len() == count)
        .ok_or_else(|| wrong(heap, name, what, value))?;
    items
        .into_iter()
        .map(|item| number(heap, name, item))
        .collect()
}

fn point(heap: &Heap, name: &str, value: Value) -> Result<Vec2, Error> {
    let found = numbers(heap, name, "a point as [x y]", value, 2)?;
    Ok(Vec2::new(found[0], found[1]))
}

fn rect(heap: &Heap, name: &str, value: Value) -> Result<Rect, Error> {
    let found = numbers(heap, name, "a rect as [x y w h]", value, 4)?;
    Ok(Rect::new(found[0], found[1], found[2], found[3]))
}

fn byte(value: f64) -> u8 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to a byte first"
    )]
    let byte = value.clamp(0.0, 255.0).round() as u8;
    byte
}

fn color(heap: &Heap, name: &str, value: Value) -> Result<Color, Error> {
    let what = "a color: \"#rrggbb\", [red green blue alpha] or a name such as :white";
    if let Some(text) = heap.text(value) {
        return Color::hex(text).ok_or_else(|| wrong(heap, name, what, value));
    }
    if let Value::Keyword(word) = value {
        let named = match word.name() {
            "white" => Color::WHITE,
            "black" => Color::BLACK,
            "transparent" => Color::TRANSPARENT,
            "red" => Color::rgba8(230, 60, 60, 255),
            "green" => Color::rgba8(80, 190, 90, 255),
            "blue" => Color::rgba8(70, 110, 220, 255),
            "yellow" => Color::rgba8(240, 210, 70, 255),
            _ => return Err(wrong(heap, name, what, value)),
        };
        return Ok(named);
    }
    let parts = heap
        .vector_of(value)
        .map(|vector| vector.items(heap))
        .filter(|parts| matches!(parts.len(), 3 | 4))
        .ok_or_else(|| wrong(heap, name, what, value))?;
    let mut bytes = [255; 4];
    for (slot, part) in bytes.iter_mut().zip(parts) {
        *slot = byte(number(heap, name, part)?);
    }
    Ok(Color::rgba8(bytes[0], bytes[1], bytes[2], bytes[3]))
}

fn key_name(heap: &Heap, name: &str, value: Value) -> Result<String, Error> {
    match value {
        Value::Keyword(word) => Ok(word.name().to_owned()),
        _ => heap
            .text(value)
            .map(str::to_owned)
            .ok_or_else(|| wrong(heap, name, "a key such as :left or :space", value)),
    }
}

struct Options {
    pairs: Vec<(String, Value)>,
}

impl Options {
    fn read(heap: &Heap, name: &str, rest: &[Value], known: &[&str]) -> Result<Self, Error> {
        let mut pairs = Vec::new();
        for pair in rest.chunks(2) {
            match pair {
                [Value::Keyword(key), value] if known.contains(&key.name()) => {
                    pairs.push((key.name().to_owned(), *value));
                }
                [other, ..] => {
                    let listed: Vec<String> = known.iter().map(|key| format!(":{key}")).collect();
                    return Err(Error::new(format!(
                        "{} takes {}, not {}",
                        shown(name),
                        listed.join(", "),
                        print::brief(heap, *other)
                    )));
                }
                [] => {}
            }
        }
        Ok(Self { pairs })
    }

    fn get(&self, key: &str) -> Option<Value> {
        self.pairs
            .iter()
            .rev()
            .find(|(known, _)| known == key)
            .map(|(_, value)| *value)
            .filter(|value| *value != Value::Nil)
    }

    fn number(&self, heap: &Heap, name: &str, key: &str, default: f64) -> Result<f64, Error> {
        self.get(key)
            .map_or(Ok(default), |value| number(heap, name, value))
    }

    fn layer(&self, heap: &Heap, name: &str) -> Result<i32, Error> {
        match self.get("layer") {
            None => Ok(0),
            Some(Value::Int(layer)) => i32::try_from(layer)
                .map_err(|_| Error::new(format!("{} :layer is too far from 0", shown(name)))),
            Some(other) => Err(wrong(heap, name, ":layer as a whole number", other)),
        }
    }
}

fn size_of(heap: &Heap, name: &str, value: Value) -> Result<(u32, u32), Error> {
    let found = numbers(heap, name, "a size as [width height]", value, 2)?;
    let side = |value: f64| {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "checked to be a small positive size"
        )]
        let side = value as u32;
        (1.0..=4096.0).contains(&value).then_some(side)
    };
    side(found[0])
        .zip(side(found[1]))
        .ok_or_else(|| Error::new(format!("{} takes sizes from 1 to 4096 pixels", shown(name))))
}

fn play(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let path = heap
        .text(arguments[0])
        .map(PathBuf::from)
        .ok_or_else(|| wrong(heap, "play-game", "the path of an Ox file", arguments[0]))?;
    let options = Options::read(
        heap,
        "play-game",
        &arguments[1..],
        &["size", "smooth", "rate"],
    )?;
    let size = match options.get("size") {
        Some(value) => size_of(heap, "play-game", value)?,
        None => SIZE,
    };
    let rate = match options.get("rate") {
        Some(Value::Int(rate @ 1..=240)) => u32::try_from(rate).unwrap_or(60),
        Some(other) => return Err(wrong(heap, "play-game", ":rate from 1 to 240", other)),
        None => 60,
    };
    let smooth = options.get("smooth").is_some_and(Value::truthy);
    let entry = std::path::absolute(&path).unwrap_or(path);
    let folder = entry.parent().map(Path::to_owned).unwrap_or_default();
    let mut games = editor.take_plugin_state::<Games>().unwrap_or_default();
    let reused = games.game.take().map(|old| {
        games.released.extend(
            old.hooks
                .into_iter()
                .flat_map(|hooks| [hooks.start, hooks.tick, hooks.draw]),
        );
        old.buffer
    });
    let buffer = reused
        .filter(|buffer| editor.buffer_by_id(*buffer).is_some())
        .unwrap_or_else(|| {
            let scope = editor.add_scope(NAME);
            editor.add_plugin_buffer_with(NAME, |buffer| {
                buffer.set_read_only(true);
                GameBuffer {
                    scope,
                    screen: Screen::blank(size),
                }
            })
        });
    games.game = Some(Game {
        buffer,
        entry,
        folder,
        size,
        smooth,
        rate,
        phase: Phase::Loading,
        hooks: None,
        clock: Clock::new(rate),
        last: None,
        held: BTreeSet::new(),
        pressed: BTreeSet::new(),
        released: BTreeSet::new(),
        actions: BTreeMap::new(),
        list: DrawList::new(),
        captions: Vec::new(),
        textures: Textures::new(),
        reloads: Vec::new(),
        restart: false,
        dirty: true,
        generation: 0,
        error: None,
        alpha: 0.0,
    });
    editor.set_plugin_state(games);
    crate::api::buffers::reveal(editor, buffer, true, true);
    Ok(Value::Nil)
}

fn held(game: &Game, name: &str, keys: &BTreeSet<String>) -> bool {
    keys.contains(name)
        || game
            .actions
            .get(name)
            .is_some_and(|bound| bound.iter().any(|key| keys.contains(key)))
}

fn input(isolate: &mut Isolate, name: &'static str, pick: fn(&Game) -> &BTreeSet<String>) {
    isolate.define_host_native_in::<Editor>(
        MODULE,
        name,
        1,
        Some(1),
        move |heap, editor, arguments| {
            let key = key_name(heap, name, arguments[0])?;
            let game = playing(editor)?;
            Ok(Value::Bool(held(game, &key, pick(game))))
        },
    );
}

fn actions(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let entries = heap.entries(arguments[0]).ok_or_else(|| {
        wrong(
            heap,
            "actions",
            "a map of actions to vectors of keys",
            arguments[0],
        )
    })?;
    let mut bound = BTreeMap::new();
    for (action, keys) in entries {
        let action = key_name(heap, "actions", action)?;
        let keys = heap
            .elements(keys)
            .ok_or_else(|| wrong(heap, "actions", "a vector of keys", keys))?
            .into_iter()
            .map(|key| key_name(heap, "actions", key))
            .collect::<Result<Vec<_>, _>>()?;
        bound.insert(action, keys);
    }
    playing(editor)?.actions = bound;
    Ok(Value::Nil)
}

fn shape(
    heap: &mut Heap,
    editor: &mut Editor,
    arguments: &[Value],
    name: &str,
) -> Result<Value, Error> {
    let (shape, paint, rest, known): (Shape, Value, &[Value], &[&str]) = match name {
        "rect" => (
            Shape::Rect {
                rect: rect(heap, name, arguments[0])?,
                stroke: None,
            },
            arguments[1],
            &arguments[2..],
            &["stroke", "layer"],
        ),
        "circle" => (
            Shape::Circle {
                center: point(heap, name, arguments[0])?,
                radius: number(heap, name, arguments[1])?,
                stroke: None,
            },
            arguments[2],
            &arguments[3..],
            &["stroke", "layer"],
        ),
        _ => (
            Shape::Line {
                from: point(heap, name, arguments[0])?,
                to: point(heap, name, arguments[1])?,
                width: 1.0,
            },
            arguments[2],
            &arguments[3..],
            &["width", "layer"],
        ),
    };
    let options = Options::read(heap, name, rest, known)?;
    let stroke = options
        .get("stroke")
        .map(|value| number(heap, name, value))
        .transpose()?;
    let shape = match shape {
        Shape::Rect { rect, .. } => Shape::Rect { rect, stroke },
        Shape::Circle { center, radius, .. } => Shape::Circle {
            center,
            radius,
            stroke,
        },
        Shape::Line { from, to, .. } => Shape::Line {
            from,
            to,
            width: options.number(heap, name, "width", 1.0)?,
        },
        other @ Shape::Polygon { .. } => other,
    };
    let paint = color(heap, name, paint)?;
    let layer = options.layer(heap, name)?;
    playing(editor)?.list.shape(shape, paint, layer);
    Ok(Value::Nil)
}

fn image_id(heap: &Heap, value: Value) -> Result<usize, Error> {
    let id = match value {
        Value::Int(id) => Some(id),
        _ => heap
            .map_of(value)
            .and_then(|map| map.get(heap, Value::keyword("id")))
            .and_then(|id| match id {
                Value::Int(id) => Some(id),
                _ => None,
            }),
    };
    id.and_then(|id| usize::try_from(id).ok())
        .ok_or_else(|| wrong(heap, "sprite", "an image from rodeo/image", value))
}

fn image(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let relative = heap
        .text(arguments[0])
        .map(PathBuf::from)
        .ok_or_else(|| wrong(heap, "image", "a path", arguments[0]))?;
    let game = playing(editor)?;
    let path = game.folder.join(relative);
    let id = game
        .textures
        .load(&path)
        .map_err(|error| Error::new(error.to_string()))?;
    let texture = game
        .textures
        .get(id)
        .ok_or_else(|| Error::new("The image went missing"))?;
    let (width, height) = (texture.width, texture.height);
    make_map(
        heap,
        &[
            Value::keyword("id"),
            Value::Int(i64::try_from(id.index()).unwrap_or(i64::MAX)),
            Value::keyword("width"),
            Value::Int(i64::from(width)),
            Value::keyword("height"),
            Value::Int(i64::from(height)),
        ],
    )
}

fn texture_of(game: &Game, index: usize) -> Option<TextureId> {
    game.textures.id_at(index)
}

fn cells(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let index = image_id(heap, arguments[0])?;
    let width = number(heap, "cells", arguments[1])?;
    let height = number(heap, "cells", arguments[2])?;
    let game = playing(editor)?;
    let id = texture_of(game, index)
        .ok_or_else(|| Error::new("rodeo/cells needs an image from rodeo/image"))?;
    let texture = game
        .textures
        .get(id)
        .ok_or_else(|| Error::new("The image went missing"))?;
    let whole = |value: f64| u32::try_from(rounded(value)).unwrap_or(1).max(1);
    let atlas = Atlas::grid(
        id,
        texture.width,
        texture.height,
        whole(width),
        whole(height),
        0,
        0,
    );
    let mut regions = Vec::with_capacity(atlas.regions.len());
    for region in atlas.regions {
        let rect = region.rect;
        regions.push(make_vector(
            heap,
            &[
                Value::Float(rect.x),
                Value::Float(rect.y),
                Value::Float(rect.w),
                Value::Float(rect.h),
            ],
        )?);
    }
    make_vector(heap, &regions)
}

fn rounded(value: f64) -> i64 {
    #[expect(clippy::cast_possible_truncation, reason = "cell sizes are small")]
    let rounded = value.round().clamp(0.0, 1e9) as i64;
    rounded
}

fn sprite(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = "sprite";
    let index = image_id(heap, arguments[0])?;
    let at = point(heap, name, arguments[1])?;
    let options = Options::read(
        heap,
        name,
        &arguments[2..],
        &[
            "region", "rotation", "scale", "origin", "flip-x", "flip-y", "tint", "flash",
            "outline", "layer",
        ],
    )?;
    let region = options
        .get("region")
        .map(|value| rect(heap, name, value))
        .transpose()?;
    let scale = match options.get("scale") {
        None => Vec2::new(1.0, 1.0),
        Some(value) if heap.vector_of(value).is_some() => point(heap, name, value)?,
        Some(value) => {
            let factor = number(heap, name, value)?;
            Vec2::new(factor, factor)
        }
    };
    let origin = options
        .get("origin")
        .map(|value| point(heap, name, value))
        .transpose()?;
    let tint = options
        .get("tint")
        .map(|value| color(heap, name, value))
        .transpose()?;
    let outline = options
        .get("outline")
        .map(|value| color(heap, name, value))
        .transpose()?;
    let rotation = options.number(heap, name, "rotation", 0.0)?;
    let flash = options.number(heap, name, "flash", 0.0)?;
    let layer = options.layer(heap, name)?;
    let flip_x = options.get("flip-x").is_some_and(Value::truthy);
    let flip_y = options.get("flip-y").is_some_and(Value::truthy);
    let game = playing(editor)?;
    let id = texture_of(game, index)
        .ok_or_else(|| wrong(heap, name, "an image from rodeo/image", arguments[0]))?;
    let texture = game
        .textures
        .get(id)
        .ok_or_else(|| Error::new("The image went missing"))?;
    let whole = Rect::new(
        0.0,
        0.0,
        f64::from(texture.width),
        f64::from(texture.height),
    );
    game.list.sprite(Sprite {
        position: at,
        rotation,
        scale,
        origin: origin.unwrap_or(Vec2::ZERO),
        flip_x,
        flip_y,
        tint: tint.unwrap_or(Color::WHITE),
        flash: narrow(flash),
        outline: outline.map(|color| Outline { color, width: 1.0 }),
        layer,
        ..Sprite::new(id, region.unwrap_or(whole))
    });
    Ok(Value::Nil)
}

fn text(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let shown = print::display(heap, arguments[0]);
    let at = point(heap, "text", arguments[1])?;
    let options = Options::read(heap, "text", &arguments[2..], &["color", "size", "layer"])?;
    let paint = options
        .get("color")
        .map(|value| color(heap, "text", value))
        .transpose()?
        .unwrap_or(Color::WHITE);
    let size = options.number(heap, "text", "size", 8.0)?;
    playing(editor)?.captions.push(Caption {
        text: shown,
        at: (narrow(at.x), narrow(at.y)),
        size: narrow(size),
        color: paint.to_rgba8(),
    });
    Ok(Value::Nil)
}

fn control(isolate: &mut Isolate, name: &'static str, act: fn(&mut Game)) {
    isolate.define_host_native::<Editor>(name, 0, Some(0), move |_, editor, _| {
        act(playing(editor)?);
        Ok(Value::Nil)
    });
}

fn measure(isolate: &mut Isolate, name: &'static str, read: fn(&Game) -> Value) {
    isolate.define_host_native_in::<Editor>(MODULE, name, 0, Some(0), move |_, editor, _| {
        Ok(read(playing(editor)?))
    });
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("play-game", 1, None, play);
    isolate.define_host_native_in::<Editor>(
        MODULE,
        "screen",
        2,
        None,
        |heap, editor, arguments| {
            let width = number(heap, "screen", arguments[0])?;
            let height = number(heap, "screen", arguments[1])?;
            let options = Options::read(heap, "screen", &arguments[2..], &["smooth"])?;
            let pair = make_vector(heap, &[Value::Float(width), Value::Float(height)])?;
            let size = size_of(heap, "screen", pair)?;
            let game = playing(editor)?;
            game.size = size;
            game.smooth = options.get("smooth").is_some_and(Value::truthy);
            game.dirty = true;
            Ok(Value::Nil)
        },
    );
    isolate.define_host_native_in::<Editor>(MODULE, "size", 0, Some(0), |heap, editor, _| {
        let (width, height) = playing(editor)?.size;
        make_vector(
            heap,
            &[Value::Int(i64::from(width)), Value::Int(i64::from(height))],
        )
    });
    control(isolate, "pause-game", |game| {
        game.clock.pause();
        game.dirty = true;
    });
    control(isolate, "resume-game", |game| {
        game.clock.resume();
        if game.phase == Phase::Failed {
            game.phase = Phase::Running;
            game.error = None;
        }
        game.last = None;
        game.dirty = true;
    });
    control(isolate, "step-game", |game| {
        game.clock.pause();
        game.clock.step_once();
    });
    control(isolate, "restart-game", |game| game.restart = true);
    isolate.define_host_native::<Editor>("stop-game", 0, Some(0), |_, editor, _| {
        stop(editor);
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>("game-speed", 1, Some(1), |heap, editor, arguments| {
        let factor = number(heap, "game-speed", arguments[0])?;
        playing(editor)?.clock.set_scale(factor);
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>("game-playing?", 0, Some(0), |_, editor, _| {
        Ok(Value::Bool(game(editor).is_some_and(|game| {
            game.phase == Phase::Running && !game.clock.paused()
        })))
    });
    install_time_and_input(isolate);
    install_drawing(isolate);
    isolate.add_native_docs(DOCS);
}

fn install_time_and_input(isolate: &mut Isolate) {
    measure(isolate, "time", |game| Value::Float(game.clock.seconds()));
    measure(isolate, "ticks", |game| {
        Value::Int(i64::try_from(game.clock.ticks()).unwrap_or(i64::MAX))
    });
    measure(isolate, "dt", |game| {
        Value::Float(game.clock.step().as_secs_f64())
    });
    measure(isolate, "alpha", |game| Value::Float(game.alpha));
    input(isolate, "held?", |game| &game.held);
    input(isolate, "pressed?", |game| &game.pressed);
    input(isolate, "released?", |game| &game.released);
    isolate.define_host_native_in::<Editor>(MODULE, "actions", 1, Some(1), actions);
}

fn install_drawing(isolate: &mut Isolate) {
    isolate.define_host_native_in::<Editor>(
        MODULE,
        "clear",
        1,
        Some(1),
        |heap, editor, arguments| {
            let paint = color(heap, "clear", arguments[0])?;
            playing(editor)?.list.background = paint;
            Ok(Value::Nil)
        },
    );
    isolate.define_host_native_in::<Editor>(
        MODULE,
        "fade",
        2,
        Some(2),
        |heap, editor, arguments| {
            let paint = color(heap, "fade", arguments[0])?;
            let amount = number(heap, "fade", arguments[1])?;
            playing(editor)?.list.fade = Some(Color {
                a: narrow(amount.clamp(0.0, 1.0)),
                ..paint
            });
            Ok(Value::Nil)
        },
    );
    isolate.define_host_native_in::<Editor>(MODULE, "rect", 2, None, |heap, editor, arguments| {
        shape(heap, editor, arguments, "rect")
    });
    isolate.define_host_native_in::<Editor>(
        MODULE,
        "circle",
        3,
        None,
        |heap, editor, arguments| shape(heap, editor, arguments, "circle"),
    );
    isolate.define_host_native_in::<Editor>(MODULE, "line", 3, None, |heap, editor, arguments| {
        shape(heap, editor, arguments, "line")
    });
    isolate.define_host_native_in::<Editor>(MODULE, "text", 2, None, text);
    isolate.define_host_native_in::<Editor>(MODULE, "image", 1, Some(1), image);
    isolate.define_host_native_in::<Editor>(MODULE, "cells", 3, Some(3), cells);
    isolate.define_host_native_in::<Editor>(MODULE, "sprite", 2, None, sprite);
}

fn stop(editor: &mut Editor) {
    let Some(mut games) = editor.take_plugin_state::<Games>() else {
        return;
    };
    if let Some(game) = games.game.take() {
        games.released.extend(
            game.hooks
                .into_iter()
                .flat_map(|hooks| [hooks.start, hooks.tick, hooks.draw]),
        );
        editor.set_plugin_state(games);
        editor.close_buffer(game.buffer);
    } else {
        editor.set_plugin_state(games);
    }
}

fn release(isolate: &mut Isolate, editor: &mut Editor) {
    let Some(games) = editor.plugin_state_mut::<Games>() else {
        return;
    };
    for hold in std::mem::take(&mut games.released) {
        isolate.release(hold);
    }
}

fn status(game: &Game) -> Option<String> {
    match (game.phase, &game.error) {
        (Phase::Failed, Some(error)) => Some(format!("Paused: {error}")),
        (Phase::Loading, _) => Some("Loading".to_owned()),
        _ if game.clock.paused() => Some("Paused".to_owned()),
        _ => None,
    }
}

fn present(editor: &mut Editor, render: bool) {
    let Some(game) = game_mut(editor) else {
        return;
    };
    let (width, height) = game.size;
    let frame = render.then(|| raster::render(&mut game.list, &game.textures, width, height));
    if render {
        game.generation += 1;
    }
    let (buffer, generation, smooth, state) =
        (game.buffer, game.generation, game.smooth, status(game));
    let captions = if render {
        Some(std::mem::take(&mut game.captions))
    } else {
        None
    };
    let Some((_, view)) = editor.plugin_buffer_mut::<GameBuffer>(buffer) else {
        return;
    };
    if let Some(frame) = frame {
        view.screen.width = frame.width;
        view.screen.height = frame.height;
        view.screen.pixels = frame.pixels.into();
    }
    if let Some(captions) = captions {
        view.screen.captions = captions;
    }
    view.screen.generation = generation;
    view.screen.smooth = smooth;
    view.screen.status = state;
}

fn fail(editor: &mut Editor, error: &Error, origin: Option<(&str, &str)>) {
    let Some(game) = game_mut(editor) else {
        return;
    };
    game.phase = Phase::Failed;
    game.error = Some(error.message.clone());
    log::report(editor, error, origin);
    editor.show_message(format!(
        "rodeo: {}; the game is paused until you fix it and resume",
        error.message
    ));
    present(editor, false);
}

fn load(isolate: &mut Isolate, editor: &mut Editor, path: &Path) -> bool {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            let error = Error::new(format!("can't read {}: {error}", path.display()));
            fail(editor, &error, None);
            return false;
        }
    };
    let name = path.display().to_string();
    match isolate.within(crate::PATIENT, |isolate| {
        isolate.eval_named_with(editor, &name, &source)
    }) {
        Ok(_) => true,
        Err(error) => {
            fail(editor, &error, Some((&name, &source)));
            false
        }
    }
}

fn hooks(isolate: &mut Isolate, editor: &mut Editor) -> Result<Hooks, Error> {
    let mut make = |source: &str| -> Result<Hold, Error> {
        let function = isolate.eval_with(editor, source)?;
        Ok(isolate.hold(function))
    };
    Ok(Hooks {
        start: make("(fn [] (start))")?,
        tick: make("(fn [] (tick))")?,
        draw: make("(fn [] (draw))")?,
    })
}

fn call(isolate: &mut Isolate, editor: &mut Editor, hold: Hold, name: &str) -> bool {
    let Some(function) = isolate.held(hold) else {
        return true;
    };
    match isolate.within(crate::QUICK, |isolate| {
        isolate.call_with(editor, function, &[])
    }) {
        Ok(_) => true,
        Err(error) if error.message == format!("{name} is not defined") => true,
        Err(error) => {
            fail(editor, &error, None);
            false
        }
    }
}

fn start(isolate: &mut Isolate, editor: &mut Editor) {
    let Some(game) = game_mut(editor) else {
        return;
    };
    let (entry, folder) = (game.entry.clone(), game.folder.clone());
    game.phase = Phase::Running;
    game.error = None;
    isolate.add_root(folder);
    if !load(isolate, editor, &entry) {
        return;
    }
    let made = match hooks(isolate, editor) {
        Ok(made) => made,
        Err(error) => {
            fail(editor, &error, None);
            return;
        }
    };
    let Some(game) = game_mut(editor) else {
        return;
    };
    let rate = game.rate;
    if let Some(old) = game.hooks.replace(made) {
        let released = [old.start, old.tick, old.draw];
        if let Some(games) = editor.plugin_state_mut::<Games>() {
            games.released.extend(released);
        }
    }
    if let Some(game) = game_mut(editor) {
        game.clock = Clock::new(rate);
        game.last = None;
        game.dirty = true;
    }
    call(isolate, editor, made.start, "start");
}

fn reload_files(isolate: &mut Isolate, editor: &mut Editor) {
    let Some(game) = game_mut(editor) else {
        return;
    };
    let waiting = std::mem::take(&mut game.reloads);
    let mut loaded = false;
    for path in waiting {
        if !load(isolate, editor, &path) {
            return;
        }
        loaded = true;
    }
    if loaded && let Some(game) = game_mut(editor) {
        if game.phase == Phase::Failed {
            game.phase = Phase::Running;
            game.error = None;
            game.last = None;
        }
        game.dirty = true;
        editor.show_message("rodeo: reloaded the game");
    }
}

pub fn frame(isolate: &mut Isolate, editor: &mut Editor, now: Instant) {
    release(isolate, editor);
    let Some(game) = game_mut(editor) else {
        return;
    };
    let buffer = game.buffer;
    if editor.buffer_by_id(buffer).is_none() {
        stop(editor);
        release(isolate, editor);
        return;
    }
    let Some(game) = game_mut(editor) else {
        return;
    };
    if game.phase == Phase::Loading || std::mem::take(&mut game.restart) {
        game.phase = Phase::Loading;
        start(isolate, editor);
    }
    reload_files(isolate, editor);
    let Some(game) = game_mut(editor) else {
        return;
    };
    if game.phase == Phase::Failed {
        game.last = Some(now);
        return;
    }
    let elapsed = game
        .last
        .map_or(Duration::ZERO, |last| now.saturating_duration_since(last));
    game.last = Some(now);
    let ticks = game.clock.advance(elapsed);
    let dirty = std::mem::take(&mut game.dirty);
    let Some(hooks) = game.hooks else {
        return;
    };
    for _ in 0..ticks {
        if !call(isolate, editor, hooks.tick, "tick") {
            return;
        }
        if let Some(game) = game_mut(editor) {
            game.pressed.clear();
            game.released.clear();
        }
    }
    if ticks == 0 && !dirty {
        return;
    }
    let Some(game) = game_mut(editor) else {
        return;
    };
    game.list.clear();
    game.list.background = Color::BLACK;
    game.captions.clear();
    game.alpha = game.clock.alpha();
    if !call(isolate, editor, hooks.draw, "draw") {
        return;
    }
    present(editor, true);
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use oxmacs_core::editor::Editor;

    use super::{GameBuffer, screen, wants_keys};
    use crate::Ox;

    fn game_file(source: &str) -> (tempfile::TempDir, String) {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("game.ox");
        std::fs::write(&path, source).unwrap();
        let shown = path.display().to_string().replace('\\', "/");
        (folder, shown)
    }

    fn pixel(editor: &Editor, x: u32, y: u32) -> [u8; 4] {
        let id = editor.buffer_named("*game*").unwrap();
        let screen = screen(editor, id).unwrap();
        let at = usize::try_from((y * screen.width + x) * 4).unwrap();
        screen.pixels[at..at + 4].try_into().unwrap()
    }

    const COUNTER: &str = r##"(module (import (rodeo)))
(defonce ticks (atom 0))
(def (tick) (swap! ticks inc))
(def (draw)
  (rodeo/clear "#102030")
  (rodeo/rect [0 0 2 2] "#ff0000"))
"##;

    fn playing(source: &str) -> (Editor, Ox, Instant, tempfile::TempDir) {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        let (folder, path) = game_file(source);
        ox.eval(
            &mut editor,
            &format!("(module (import (rodeo))) (play-game \"{path}\" :size [4 4])"),
        )
        .unwrap();
        let start = Instant::now();
        ox.games_frame(&mut editor, start);
        (editor, ox, start, folder)
    }

    fn sixtieths(count: u32) -> Duration {
        Duration::from_nanos(16_666_667 * u64::from(count))
    }

    #[test]
    fn playing_a_game_shows_its_frames_beside_the_code() {
        let (mut editor, mut ox, start, _folder) = playing(COUNTER);
        let id = editor.buffer_named("*game*").unwrap();
        assert!(editor.plugin_buffer::<GameBuffer>(id).is_some());
        assert_eq!(editor.buffer().id(), id);
        assert_eq!(pixel(&editor, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&editor, 3, 3), [16, 32, 48, 255]);
        ox.games_frame(&mut editor, start + sixtieths(3));
        assert_eq!(ox.eval(&mut editor, "@ticks").unwrap(), "3");
        assert_eq!(screen(&editor, id).unwrap().status, None);
    }

    #[test]
    fn redefining_draw_changes_the_running_game() {
        let (mut editor, mut ox, start, _folder) = playing(COUNTER);
        ox.eval(&mut editor, r##"(def (draw) (rodeo/clear "#00ff00"))"##)
            .unwrap();
        ox.games_frame(&mut editor, start + sixtieths(1));
        assert_eq!(pixel(&editor, 0, 0), [0, 255, 0, 255]);
    }

    #[test]
    fn keys_reach_the_game_as_held_pressed_and_released() {
        let (mut editor, mut ox, start, _folder) = playing(
            r"(module (import (rodeo)))
(defonce seen (atom []))
(rodeo/actions {jump: [:space :up]})
(def (tick)
  (swap! seen conj [(rodeo/held? :left) (rodeo/pressed? :left) (rodeo/released? :left) (rodeo/held? :jump)]))
",
        );
        assert!(wants_keys(&editor));
        super::key(&mut editor, "left", true);
        super::key(&mut editor, "up", true);
        ox.games_frame(&mut editor, start + sixtieths(2));
        super::key(&mut editor, "left", false);
        ox.games_frame(&mut editor, start + sixtieths(3));
        assert_eq!(
            ox.eval(&mut editor, "@seen").unwrap(),
            "[[true true false true] [true false false true] [false false true true]]"
        );
    }

    #[test]
    fn errors_pause_the_game_until_it_is_fixed_and_resumed() {
        let (mut editor, mut ox, start, _folder) = playing(
            r"(module (import (rodeo)))
(def (tick) (/ 1 0))
",
        );
        ox.games_frame(&mut editor, start + sixtieths(1));
        let id = editor.buffer_named("*game*").unwrap();
        assert_eq!(
            screen(&editor, id).unwrap().status.as_deref(),
            Some("Paused: Division by zero")
        );
        assert!(!wants_keys(&editor));
        ox.eval(
            &mut editor,
            "(defonce fixed (atom 0)) (def (tick) (swap! fixed inc))",
        )
        .unwrap();
        ox.eval(&mut editor, "(resume-game)").unwrap();
        ox.games_frame(&mut editor, start + sixtieths(2));
        ox.games_frame(&mut editor, start + sixtieths(4));
        assert_eq!(ox.eval(&mut editor, "@fixed").unwrap(), "2");
        assert_eq!(screen(&editor, id).unwrap().status, None);
    }

    #[test]
    fn saving_a_game_file_reloads_it_and_keeps_defonce_state() {
        let (mut editor, mut ox, start, folder) = playing(COUNTER);
        ox.games_frame(&mut editor, start + sixtieths(2));
        let path = folder.path().join("game.ox");
        std::fs::write(
            &path,
            COUNTER.replace("(swap! ticks inc)", "(swap! ticks (fn [n] (+ n 10)))"),
        )
        .unwrap();
        super::reload(&mut editor, &std::path::absolute(&path).unwrap());
        ox.games_frame(&mut editor, start + sixtieths(3));
        assert_eq!(ox.eval(&mut editor, "@ticks").unwrap(), "12");
    }

    #[test]
    fn sprites_draw_images_from_the_game_folder() {
        let folder = tempfile::tempdir().unwrap();
        let picture = image::RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                image::Rgba([0, 0, 255, 255])
            } else {
                image::Rgba([255, 255, 0, 255])
            }
        });
        picture.save(folder.path().join("hero.png")).unwrap();
        let path = folder.path().join("game.ox");
        std::fs::write(
            &path,
            r#"(module (import (rodeo)))
(def hero (rodeo/image "hero.png"))
(def (draw)
  (rodeo/sprite hero [1 1])
  (rodeo/sprite hero [0 3] :region (first (rodeo/cells hero 1 1)) :scale 2)
  (rodeo/text (str hero.width "x" hero.height) [0 0]))
"#,
        )
        .unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        let shown = path.display().to_string().replace('\\', "/");
        ox.eval(
            &mut editor,
            &format!("(module (import (rodeo))) (play-game \"{shown}\" :size [4 5])"),
        )
        .unwrap();
        ox.games_frame(&mut editor, Instant::now());
        assert_eq!(pixel(&editor, 1, 1), [0, 0, 255, 255]);
        assert_eq!(pixel(&editor, 2, 1), [255, 255, 0, 255]);
        assert_eq!(pixel(&editor, 1, 4), [0, 0, 255, 255]);
        let id = editor.buffer_named("*game*").unwrap();
        assert_eq!(screen(&editor, id).unwrap().captions[0].text, "2x1");
    }

    #[test]
    fn stopping_closes_the_window() {
        let (mut editor, mut ox, _, _folder) = playing(COUNTER);
        ox.eval(&mut editor, "(stop-game)").unwrap();
        assert!(editor.buffer_named("*game*").is_none());
        assert!(!super::active(&editor));
    }
}
