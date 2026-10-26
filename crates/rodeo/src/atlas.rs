use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde::de::{self, MapAccess, SeqAccess, Visitor};

use crate::texture::{self, TextureId, Textures};
use ox_library::animation::{Animation, Direction, Frame};
use ox_library::geometry::Rect;
use ox_library::geometry::Vec2;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the sprite sheet isn't valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the tag {name} covers frames {from} to {to}, but the sheet has {frames} frames")]
    Tag {
        name: String,
        from: usize,
        to: usize,
        frames: usize,
    },
    #[error("can't read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{} doesn't name its image", path.display())]
    NoImage { path: PathBuf },
    #[error(transparent)]
    Texture(#[from] texture::Error),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    pub rect: Rect,
    pub offset: Vec2,
    pub size: Vec2,
    pub seconds: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    pub name: String,
    pub from: usize,
    pub to: usize,
    pub direction: Direction,
    pub repeat: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Atlas {
    pub texture: TextureId,
    pub regions: Vec<Region>,
    pub names: BTreeMap<String, usize>,
    pub tags: Vec<Tag>,
}

#[derive(Deserialize)]
struct Bounds {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Deserialize)]
struct Size {
    w: f64,
    h: f64,
}

#[derive(Deserialize)]
struct SheetFrame {
    #[serde(default)]
    filename: Option<String>,
    frame: Bounds,
    #[serde(default, rename = "spriteSourceSize")]
    trimmed: Option<Bounds>,
    #[serde(default, rename = "sourceSize")]
    source: Option<Size>,
    #[serde(default)]
    duration: Option<f64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Repeat {
    Text(String),
    Number(u32),
}

#[derive(Deserialize)]
struct SheetTag {
    name: String,
    from: usize,
    to: usize,
    #[serde(default)]
    direction: Option<String>,
    #[serde(default)]
    repeat: Option<Repeat>,
}

#[derive(Default, Deserialize)]
struct Meta {
    #[serde(default)]
    image: Option<String>,
    #[serde(default, rename = "frameTags")]
    tags: Vec<SheetTag>,
}

struct Frames(Vec<(String, SheetFrame)>);

impl<'de> Deserialize<'de> for Frames {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Ordered;

        impl<'de> Visitor<'de> for Ordered {
            type Value = Frames;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("frames as an object or an array")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Frames, A::Error> {
                let mut frames = Vec::new();
                while let Some(entry) = map.next_entry::<String, SheetFrame>()? {
                    frames.push(entry);
                }
                Ok(Frames(frames))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Frames, A::Error> {
                let mut frames = Vec::new();
                while let Some(frame) = seq.next_element::<SheetFrame>()? {
                    let name = frame
                        .filename
                        .clone()
                        .unwrap_or_else(|| frames.len().to_string());
                    frames.push((name, frame));
                }
                Ok(Frames(frames))
            }
        }

        deserializer.deserialize_any(Ordered)
    }
}

#[derive(Deserialize)]
struct Sheet {
    frames: Frames,
    #[serde(default)]
    meta: Meta,
}

fn direction(name: Option<&str>) -> Direction {
    match name {
        Some("reverse") => Direction::Reverse,
        Some("pingpong") => Direction::PingPong,
        Some("pingpong_reverse") => Direction::PingPongReverse,
        _ => Direction::Forward,
    }
}

impl Atlas {
    pub fn grid(
        texture: TextureId,
        width: u32,
        height: u32,
        cell_width: u32,
        cell_height: u32,
        margin: u32,
        spacing: u32,
    ) -> Self {
        let fit = |length: u32, cell: u32| {
            (length + spacing).saturating_sub(margin * 2) / (cell + spacing).max(1)
        };
        let columns = fit(width, cell_width);
        let rows = fit(height, cell_height);
        let place = |index: u32, cell: u32| f64::from(margin + index * (cell + spacing));
        let size = Vec2::new(f64::from(cell_width), f64::from(cell_height));
        let regions = (0..rows)
            .flat_map(|row| (0..columns).map(move |column| (column, row)))
            .map(|(column, row)| Region {
                rect: Rect::at(
                    Vec2::new(place(column, cell_width), place(row, cell_height)),
                    size,
                ),
                offset: Vec2::ZERO,
                size,
                seconds: 0.0,
            })
            .collect();
        Self {
            texture,
            regions,
            names: BTreeMap::new(),
            tags: Vec::new(),
        }
    }

    fn sheet(json: &str) -> Result<Sheet, Error> {
        Ok(serde_json::from_str(json)?)
    }

    fn from_sheet(texture: TextureId, sheet: Sheet) -> Result<Self, Error> {
        let mut names = BTreeMap::new();
        let mut regions = Vec::with_capacity(sheet.frames.0.len());
        for (index, (name, frame)) in sheet.frames.0.into_iter().enumerate() {
            let rect = Rect::new(frame.frame.x, frame.frame.y, frame.frame.w, frame.frame.h);
            let offset = frame
                .trimmed
                .as_ref()
                .map_or(Vec2::ZERO, |trimmed| Vec2::new(trimmed.x, trimmed.y));
            let size = frame
                .source
                .as_ref()
                .map_or(rect.size(), |source| Vec2::new(source.w, source.h));
            regions.push(Region {
                rect,
                offset,
                size,
                seconds: frame.duration.unwrap_or(100.0) / 1000.0,
            });
            names.entry(name).or_insert(index);
        }
        let tags = sheet
            .meta
            .tags
            .into_iter()
            .map(|tag| {
                if tag.from > tag.to || tag.to >= regions.len() {
                    return Err(Error::Tag {
                        name: tag.name,
                        from: tag.from,
                        to: tag.to,
                        frames: regions.len(),
                    });
                }
                let repeat = match tag.repeat {
                    Some(Repeat::Number(times)) => Some(times),
                    Some(Repeat::Text(text)) => text.trim().parse().ok(),
                    None => None,
                }
                .filter(|times| *times > 0);
                Ok(Tag {
                    direction: direction(tag.direction.as_deref()),
                    name: tag.name,
                    from: tag.from,
                    to: tag.to,
                    repeat,
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            texture,
            regions,
            names,
            tags,
        })
    }

    pub fn aseprite(texture: TextureId, json: &str) -> Result<Self, Error> {
        Self::from_sheet(texture, Self::sheet(json)?)
    }

    pub fn load_aseprite(textures: &mut Textures, path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let json = std::fs::read_to_string(path).map_err(|source| Error::Read {
            path: path.to_owned(),
            source,
        })?;
        let sheet = Self::sheet(&json)?;
        let image = sheet.meta.image.as_deref().ok_or_else(|| Error::NoImage {
            path: path.to_owned(),
        })?;
        let folder = path.parent().unwrap_or_else(|| Path::new(""));
        let texture = textures.load(folder.join(image))?;
        Self::from_sheet(texture, sheet)
    }

    pub fn region(&self, index: usize) -> Option<&Region> {
        self.regions.get(index)
    }

    pub fn named(&self, name: &str) -> Option<usize> {
        self.names.get(name).copied()
    }

    pub fn tag(&self, name: &str) -> Option<&Tag> {
        self.tags.iter().find(|tag| tag.name == name)
    }

    pub fn animation(&self, name: &str) -> Option<Animation> {
        let tag = self.tag(name)?;
        let frames = (tag.from..=tag.to)
            .map(|region| Frame {
                region,
                seconds: self.regions[region].seconds,
            })
            .collect();
        Some(Animation {
            frames,
            direction: tag.direction,
            repeat: tag.repeat,
        })
    }
}
