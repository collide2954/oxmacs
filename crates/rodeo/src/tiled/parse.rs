use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use roxmltree::{Document, Node};

use super::{
    Error, Image, Layer, LayerKind, Map, Properties, Property, TileInfo, TileLayer, Tileset, data,
};
use crate::animation::{Animation, Frame};
use crate::color::Color;
use crate::vec::Vec2;

pub(super) fn read(path: &Path) -> Result<String, Error> {
    std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })
}

fn document<'t>(text: &'t str, path: &Path) -> Result<Document<'t>, Error> {
    Document::parse(text).map_err(|source| Error::Xml {
        path: path.to_owned(),
        source,
    })
}

pub(super) fn elements<'a, 'i>(
    node: Node<'a, 'i>,
    tag: &'static str,
) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children().filter(move |child| child.has_tag_name(tag))
}

fn child<'a, 'i>(node: Node<'a, 'i>, tag: &'static str) -> Option<Node<'a, 'i>> {
    elements(node, tag).next()
}

fn class_of(node: Node<'_, '_>) -> String {
    node.attribute("class")
        .or_else(|| node.attribute("type"))
        .unwrap_or_default()
        .to_owned()
}

fn text_of(node: Node<'_, '_>, name: &str) -> String {
    node.attribute(name).unwrap_or_default().to_owned()
}

fn flag(node: Node<'_, '_>, name: &str, default: bool) -> bool {
    node.attribute(name)
        .map_or(default, |value| value == "1" || value == "true")
}

fn whole(count: u64) -> usize {
    usize::try_from(count).unwrap_or(usize::MAX)
}

fn area(width: u32, height: u32) -> usize {
    whole(u64::from(width) * u64::from(height))
}

pub(super) fn tiled_color(text: &str) -> Option<Color> {
    let digits = text.strip_prefix('#').unwrap_or(text);
    let byte = |at: usize| u8::from_str_radix(digits.get(at..at + 2)?, 16).ok();
    match digits.len() {
        6 => Some(Color::rgba8(byte(0)?, byte(2)?, byte(4)?, 255)),
        8 => Some(Color::rgba8(byte(2)?, byte(4)?, byte(6)?, byte(0)?)),
        _ => None,
    }
}

struct Context {
    path: PathBuf,
    folder: PathBuf,
}

impl Context {
    fn new(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            folder: path.parent().map(Path::to_owned).unwrap_or_default(),
        }
    }

    fn invalid(&self, message: impl Into<String>) -> Error {
        Error::Invalid {
            path: self.path.clone(),
            message: message.into(),
        }
    }

    fn resolve(&self, relative: &str) -> PathBuf {
        self.folder.join(relative)
    }

    fn parsed<T: FromStr>(&self, node: Node<'_, '_>, name: &str, text: &str) -> Result<T, Error> {
        text.trim().parse().map_err(|_| {
            self.invalid(format!(
                "<{}> has {name}=\"{text}\", which isn't a valid value",
                node.tag_name().name()
            ))
        })
    }

    fn number<T: FromStr>(&self, node: Node<'_, '_>, name: &str, default: T) -> Result<T, Error> {
        node.attribute(name)
            .map_or(Ok(default), |text| self.parsed(node, name, text))
    }

    fn required<T: FromStr>(&self, node: Node<'_, '_>, name: &str) -> Result<T, Error> {
        let text = node
            .attribute(name)
            .ok_or_else(|| self.invalid(format!("<{}> needs a {name}", node.tag_name().name())))?;
        self.parsed(node, name, text)
    }

    fn color(&self, node: Node<'_, '_>, name: &str) -> Result<Option<Color>, Error> {
        node.attribute(name)
            .map(|text| {
                tiled_color(text).ok_or_else(|| {
                    self.invalid(format!(
                        "<{}> has {name}=\"{text}\", which isn't a color",
                        node.tag_name().name()
                    ))
                })
            })
            .transpose()
    }

    fn properties(&self, node: Node<'_, '_>) -> Result<Properties, Error> {
        let mut found = Properties::new();
        let Some(list) = child(node, "properties") else {
            return Ok(found);
        };
        for property in elements(list, "property") {
            let name: String = self.required(property, "name")?;
            let value = property
                .attribute("value")
                .or_else(|| property.text())
                .unwrap_or_default();
            let parsed = match property.attribute("type").unwrap_or("string") {
                "string" => Property::String(value.to_owned()),
                "int" => Property::Int(self.parsed(property, "value", value)?),
                "float" => Property::Float(self.parsed(property, "value", value)?),
                "bool" => Property::Bool(value == "true"),
                "color" => Property::Color(tiled_color(value).unwrap_or(Color::TRANSPARENT)),
                "file" => Property::File(self.resolve(value)),
                "object" => Property::Object(self.parsed(property, "value", value)?),
                "class" => Property::Class(
                    text_of(property, "propertytype"),
                    self.properties(property)?,
                ),
                other => {
                    return Err(self.invalid(format!(
                        "property \"{name}\" has type {other}, which rodeo doesn't know"
                    )));
                }
            };
            found.insert(name, parsed);
        }
        Ok(found)
    }

    fn image(&self, node: Node<'_, '_>) -> Result<Option<Image>, Error> {
        child(node, "image")
            .map(|image| {
                Ok(Image {
                    source: self.resolve(&self.required::<String>(image, "source")?),
                    width: self.number(image, "width", 0)?,
                    height: self.number(image, "height", 0)?,
                })
            })
            .transpose()
    }

    fn animation(&self, node: Node<'_, '_>) -> Result<Option<Animation>, Error> {
        child(node, "animation")
            .map(|animation| {
                let frames = elements(animation, "frame")
                    .map(|frame| {
                        let tile: u32 = self.required(frame, "tileid")?;
                        let duration: u32 = self.required(frame, "duration")?;
                        Ok(Frame {
                            region: usize::try_from(tile).unwrap_or(usize::MAX),
                            seconds: f64::from(duration) / 1000.0,
                        })
                    })
                    .collect::<Result<_, Error>>()?;
                Ok(Animation::new(frames))
            })
            .transpose()
    }

    fn tileset(&self, node: Node<'_, '_>) -> Result<Tileset, Error> {
        let first_gid = self.required(node, "firstgid")?;
        let Some(source) = node.attribute("source") else {
            return self.tileset_body(node, first_gid);
        };
        let path = self.resolve(source);
        let text = read(&path)?;
        let external = Context::new(&path);
        let document = document(&text, &path)?;
        let root = document.root_element();
        if !root.has_tag_name("tileset") {
            return Err(external.invalid("isn't a Tiled tileset"));
        }
        let mut tileset = external.tileset_body(root, first_gid)?;
        tileset.source = Some(path);
        Ok(tileset)
    }

    fn tileset_body(&self, node: Node<'_, '_>, first_gid: u32) -> Result<Tileset, Error> {
        let mut tiles = BTreeMap::new();
        for tile in elements(node, "tile") {
            let id = self.required(tile, "id")?;
            tiles.insert(
                id,
                TileInfo {
                    class: class_of(tile),
                    properties: self.properties(tile)?,
                    image: self.image(tile)?,
                    animation: self.animation(tile)?,
                    collision: Vec::new(),
                },
            );
        }
        let offset = match child(node, "tileoffset") {
            Some(offset) => Vec2::new(
                self.number(offset, "x", 0.0)?,
                self.number(offset, "y", 0.0)?,
            ),
            None => Vec2::ZERO,
        };
        Ok(Tileset {
            first_gid,
            name: text_of(node, "name"),
            class: class_of(node),
            source: None,
            tile_width: self.required(node, "tilewidth")?,
            tile_height: self.required(node, "tileheight")?,
            spacing: self.number(node, "spacing", 0)?,
            margin: self.number(node, "margin", 0)?,
            tile_count: self.number(node, "tilecount", 0)?,
            columns: self.number(node, "columns", 0)?,
            offset,
            image: self.image(node)?,
            properties: self.properties(node)?,
            tiles,
        })
    }

    fn layers(&self, parent: Node<'_, '_>) -> Result<Vec<Layer>, Error> {
        parent
            .children()
            .filter_map(|node| self.layer(node).transpose())
            .collect()
    }

    fn layer(&self, node: Node<'_, '_>) -> Result<Option<Layer>, Error> {
        let name = text_of(node, "name");
        let kind = match node.tag_name().name() {
            "layer" => LayerKind::Tiles(self.tile_layer(node, &name)?),
            _ => return Ok(None),
        };
        Ok(Some(Layer {
            id: self.number(node, "id", 0)?,
            class: class_of(node),
            visible: flag(node, "visible", true),
            opacity: self.number(node, "opacity", 1.0)?,
            offset: Vec2::new(
                self.number(node, "offsetx", 0.0)?,
                self.number(node, "offsety", 0.0)?,
            ),
            parallax: Vec2::new(
                self.number(node, "parallaxx", 1.0)?,
                self.number(node, "parallaxy", 1.0)?,
            ),
            tint: self.color(node, "tintcolor")?,
            properties: self.properties(node)?,
            kind,
            name,
        }))
    }

    fn tile_layer(&self, node: Node<'_, '_>, name: &str) -> Result<TileLayer, Error> {
        let width: u32 = self.required(node, "width")?;
        let height: u32 = self.required(node, "height")?;
        let failed = |message: String| self.invalid(format!("layer \"{name}\" {message}"));
        let Some(data) = child(node, "data") else {
            return Ok(TileLayer {
                x: 0,
                y: 0,
                width,
                height,
                data: vec![0; area(width, height)],
            });
        };
        let encoding = data.attribute("encoding");
        let compression = data.attribute("compression");
        let chunks: Vec<_> = elements(data, "chunk").collect();
        if chunks.is_empty() {
            return Ok(TileLayer {
                x: 0,
                y: 0,
                width,
                height,
                data: data::decode(data, encoding, compression, area(width, height))
                    .map_err(failed)?,
            });
        }
        let mut pieces = Vec::new();
        for chunk in chunks {
            let x: i64 = self.required(chunk, "x")?;
            let y: i64 = self.required(chunk, "y")?;
            let columns: u32 = self.required(chunk, "width")?;
            let rows: u32 = self.required(chunk, "height")?;
            let tiles =
                data::decode(chunk, encoding, compression, area(columns, rows)).map_err(failed)?;
            pieces.push((x, y, columns, rows, tiles));
        }
        let left = pieces.iter().map(|piece| piece.0).min().unwrap_or(0);
        let top = pieces.iter().map(|piece| piece.1).min().unwrap_or(0);
        let right = pieces
            .iter()
            .map(|piece| piece.0 + i64::from(piece.2))
            .max()
            .unwrap_or(0);
        let bottom = pieces
            .iter()
            .map(|piece| piece.1 + i64::from(piece.3))
            .max()
            .unwrap_or(0);
        let too_big = || failed("spreads its chunks too far apart".to_owned());
        let width = u32::try_from(right - left).map_err(|_| too_big())?;
        let height = u32::try_from(bottom - top).map_err(|_| too_big())?;
        let mut joined = vec![0; area(width, height)];
        for (x, y, columns, rows, tiles) in pieces {
            let across = u64::try_from(x - left).unwrap_or(0);
            let down = u64::try_from(y - top).unwrap_or(0);
            let length = area(columns, 1);
            for row in 0..rows {
                let from = area(row, columns);
                let to = whole((down + u64::from(row)) * u64::from(width) + across);
                joined[to..to + length].copy_from_slice(&tiles[from..from + length]);
            }
        }
        Ok(TileLayer {
            x: left,
            y: top,
            width,
            height,
            data: joined,
        })
    }
}

pub(super) fn map(text: &str, path: &Path) -> Result<Map, Error> {
    let document = document(text, path)?;
    let root = document.root_element();
    let context = Context::new(path);
    if !root.has_tag_name("map") {
        return Err(context.invalid("isn't a Tiled map"));
    }
    let orientation = root.attribute("orientation").unwrap_or("orthogonal");
    if orientation != "orthogonal" {
        return Err(context.invalid(format!(
            "uses {orientation} tiles, but rodeo loads only orthogonal maps"
        )));
    }
    let tilesets = elements(root, "tileset")
        .map(|node| context.tileset(node))
        .collect::<Result<_, _>>()?;
    Ok(Map {
        path: path.to_owned(),
        width: context.required(root, "width")?,
        height: context.required(root, "height")?,
        tile_width: context.required(root, "tilewidth")?,
        tile_height: context.required(root, "tileheight")?,
        infinite: flag(root, "infinite", false),
        background: context.color(root, "backgroundcolor")?,
        class: class_of(root),
        properties: context.properties(root)?,
        layers: context.layers(root)?,
        tilesets,
    })
}
