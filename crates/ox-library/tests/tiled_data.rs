use std::io::Write;
use std::path::Path;

use base64::prelude::*;
use flate2::Compression;
use flate2::write::{GzEncoder, ZlibEncoder};
use ox_library::tiled::{LayerKind, Map, Tile, TileLayer};

const RAW: [u32; 4] = [1, 0, 2, 0x4000_0003];

fn map(width: u32, height: u32, infinite: bool, data: &str) -> String {
    format!(
        r#"<map orientation="orthogonal" width="{width}" height="{height}" tilewidth="8" tileheight="8" infinite="{}">
 <layer id="1" name="ground" width="{width}" height="{height}">{data}</layer>
</map>"#,
        u8::from(infinite)
    )
}

fn layer(source: &str) -> Result<TileLayer, String> {
    let map = Map::parse(source, Path::new("data.tmx")).map_err(|error| error.to_string())?;
    match &map.layers[0].kind {
        LayerKind::Tiles(tiles) => Ok(tiles.clone()),
        other => Err(format!("not tiles: {other:?}")),
    }
}

fn bytes() -> Vec<u8> {
    RAW.iter().flat_map(|tile| tile.to_le_bytes()).collect()
}

fn encoded(compression: &str) -> String {
    let packed = match compression {
        "zlib" => {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&bytes()).unwrap();
            encoder.finish().unwrap()
        }
        "gzip" => {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&bytes()).unwrap();
            encoder.finish().unwrap()
        }
        _ => bytes(),
    };
    let attribute = if compression.is_empty() {
        String::new()
    } else {
        format!(r#" compression="{compression}""#)
    };
    format!(
        "<data encoding=\"base64\"{attribute}>\n   {}\n  </data>",
        BASE64_STANDARD.encode(packed)
    )
}

#[test]
fn every_encoding_reads_the_same_tiles() {
    let xml = r#"<data><tile gid="1"/><tile/><tile gid="2"/><tile gid="1073741827"/></data>"#;
    for data in [
        xml.to_owned(),
        encoded(""),
        encoded("zlib"),
        encoded("gzip"),
    ] {
        let tiles = layer(&map(2, 2, false, &data)).unwrap();
        assert_eq!(tiles.data, RAW, "{data}");
    }
    let tiles = layer(&map(2, 2, false, &encoded("zlib"))).unwrap();
    assert_eq!(
        tiles.get(1, 1),
        Some(Tile {
            gid: 3,
            flip_horizontal: false,
            flip_vertical: true,
            flip_diagonal: false,
        })
    );
}

#[test]
fn unreadable_data_explains_itself() {
    assert_eq!(
        layer(&map(
            2,
            2,
            false,
            r#"<data encoding="base64" compression="zstd">AAAA</data>"#
        )),
        Err(
            "data.tmx: layer \"ground\" is compressed with zstd, which rodeo can't read; save the map with zlib, gzip or no compression"
                .to_owned()
        )
    );
    assert_eq!(
        layer(&map(2, 2, false, r#"<data encoding="csv">1,2,3</data>"#)),
        Err("data.tmx: layer \"ground\" has 3 tiles but should have 4".to_owned())
    );
    assert_eq!(
        layer(&map(2, 2, false, r#"<data encoding="base64">!!</data>"#))
            .map_err(|error| error.starts_with("data.tmx: layer \"ground\" has base64 data that")),
        Err(true)
    );
}

#[test]
fn infinite_maps_join_their_chunks() {
    let data = r#"<data encoding="csv">
   <chunk x="-2" y="0" width="2" height="1">1,2</chunk>
   <chunk x="0" y="1" width="2" height="1">3,0</chunk>
  </data>"#;
    let tiles = layer(&map(4, 2, true, data)).unwrap();
    assert_eq!([tiles.x, tiles.y], [-2, 0]);
    assert_eq!([tiles.width, tiles.height], [4, 2]);
    assert_eq!(tiles.get(-2, 0), Some(Tile::new(1)));
    assert_eq!(tiles.get(-1, 0), Some(Tile::new(2)));
    assert_eq!(tiles.get(0, 1), Some(Tile::new(3)));
    assert_eq!(tiles.get(0, 0), None);
    assert_eq!(tiles.get(1, 1), None);
}
