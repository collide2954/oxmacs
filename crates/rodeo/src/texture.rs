use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::paths::absolute;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId(u32);

impl TextureId {
    pub fn index(self) -> usize {
        usize::try_from(self.0).unwrap_or(usize::MAX)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("can't load {}: {source}", path.display())]
    Load {
        path: PathBuf,
        source: image::ImageError,
    },
    #[error("a {width} by {height} texture needs {needed} bytes of pixels, got {got}")]
    Size {
        width: u32,
        height: u32,
        needed: u64,
        got: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    pub path: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<[u8]>,
    pub generation: u64,
}

#[derive(Debug, Default)]
pub struct Textures {
    textures: Vec<Texture>,
    paths: HashMap<PathBuf, TextureId>,
    generation: u64,
}

fn decode(path: &Path) -> Result<(u32, u32, Arc<[u8]>), Error> {
    let image = image::open(path)
        .map_err(|source| Error::Load {
            path: path.to_owned(),
            source,
        })?
        .into_rgba8();
    Ok((image.width(), image.height(), image.into_raw().into()))
}

impl Textures {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.textures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    fn next_generation(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }

    fn push(&mut self, texture: Texture) -> TextureId {
        let id = TextureId(u32::try_from(self.textures.len()).unwrap_or(u32::MAX));
        self.textures.push(texture);
        id
    }

    pub fn get(&self, id: TextureId) -> Option<&Texture> {
        self.textures.get(id.index())
    }

    pub fn id(&self, path: impl AsRef<Path>) -> Option<TextureId> {
        self.paths.get(&absolute(path.as_ref())).copied()
    }

    pub fn load(&mut self, path: impl AsRef<Path>) -> Result<TextureId, Error> {
        let path = absolute(path.as_ref());
        if let Some(id) = self.paths.get(&path) {
            return Ok(*id);
        }
        let (width, height, pixels) = decode(&path)?;
        let generation = self.next_generation();
        let id = self.push(Texture {
            path: Some(path.clone()),
            width,
            height,
            pixels,
            generation,
        });
        self.paths.insert(path, id);
        Ok(id)
    }

    pub fn insert(&mut self, width: u32, height: u32, pixels: Vec<u8>) -> Result<TextureId, Error> {
        let needed = u64::from(width) * u64::from(height) * 4;
        if u64::try_from(pixels.len()).ok() != Some(needed) {
            return Err(Error::Size {
                width,
                height,
                needed,
                got: pixels.len(),
            });
        }
        let generation = self.next_generation();
        Ok(self.push(Texture {
            path: None,
            width,
            height,
            pixels: pixels.into(),
            generation,
        }))
    }

    pub fn reload(&mut self, path: impl AsRef<Path>) -> Result<Option<TextureId>, Error> {
        let path = absolute(path.as_ref());
        let Some(id) = self.paths.get(&path).copied() else {
            return Ok(None);
        };
        let (width, height, pixels) = decode(&path)?;
        let generation = self.next_generation();
        let texture = &mut self.textures[id.index()];
        texture.width = width;
        texture.height = height;
        texture.pixels = pixels;
        texture.generation = generation;
        Ok(Some(id))
    }
}
