use std::path::Path;

use image::{Rgba, RgbaImage};
use ox_rodeo::texture::Textures;

fn save(path: &Path, pixels: &[[u8; 4]]) {
    let width = u32::try_from(pixels.len()).unwrap();
    let image = RgbaImage::from_fn(width, 1, |x, _| Rgba(pixels[usize::try_from(x).unwrap()]));
    image.save(path).unwrap();
}

#[test]
fn textures_load_once_per_path() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("hero.png");
    save(&path, &[[255, 0, 0, 255], [0, 0, 0, 0]]);
    let mut textures = Textures::new();
    let hero = textures.load(&path).unwrap();
    let texture = textures.get(hero).unwrap();
    assert_eq!([texture.width, texture.height], [2, 1]);
    assert_eq!(&*texture.pixels, &[255, 0, 0, 255, 0, 0, 0, 0]);
    let generation = texture.generation;
    let again = folder.path().join("sub").join("..").join("hero.png");
    assert_eq!(textures.load(&again).unwrap(), hero);
    assert_eq!(textures.get(hero).unwrap().generation, generation);
    assert_eq!(textures.id(&path), Some(hero));
    assert_eq!(textures.len(), 1);
}

#[test]
fn reloading_swaps_pixels_and_bumps_the_generation() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("coin.png");
    save(&path, &[[1, 2, 3, 4]]);
    let mut textures = Textures::new();
    let coin = textures.load(&path).unwrap();
    let before = textures.get(coin).unwrap().generation;
    save(&path, &[[9, 9, 9, 9], [8, 8, 8, 8]]);
    assert_eq!(textures.reload(&path).unwrap(), Some(coin));
    let texture = textures.get(coin).unwrap();
    assert!(texture.generation > before);
    assert_eq!(texture.width, 2);
    assert_eq!(&*texture.pixels, &[9, 9, 9, 9, 8, 8, 8, 8]);
    assert_eq!(
        textures.reload(folder.path().join("other.png")).unwrap(),
        None
    );
}

#[test]
fn textures_come_from_memory_and_report_bad_files() {
    let mut textures = Textures::new();
    let white = textures.insert(1, 1, vec![255; 4]).unwrap();
    assert_eq!(textures.get(white).unwrap().path, None);
    assert_eq!(
        textures.insert(2, 2, vec![0; 4]).unwrap_err().to_string(),
        "a 2 by 2 texture needs 16 bytes of pixels, got 4"
    );
    let folder = tempfile::tempdir().unwrap();
    let missing = folder.path().join("missing.png");
    let error = textures.load(&missing).unwrap_err().to_string();
    assert!(
        error.starts_with(&format!("can't load {}", missing.display())),
        "{error}"
    );
}
