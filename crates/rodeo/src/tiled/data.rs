use std::io::Read;

use base64::prelude::*;
use flate2::read::{GzDecoder, ZlibDecoder};
use roxmltree::Node;

use super::parse::elements;

fn inflated(packed: Vec<u8>, compression: Option<&str>) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let unpacked = match compression {
        None | Some("") => return Ok(packed),
        Some("zlib") => ZlibDecoder::new(packed.as_slice()).read_to_end(&mut bytes),
        Some("gzip") => GzDecoder::new(packed.as_slice()).read_to_end(&mut bytes),
        Some(other) => {
            return Err(format!(
                "is compressed with {other}, which rodeo can't read; save the map with zlib, gzip or no compression"
            ));
        }
    };
    unpacked.map_err(|error| {
        format!(
            "has {} data that won't unpack: {error}",
            compression.unwrap_or_default()
        )
    })?;
    Ok(bytes)
}

fn base64(node: Node<'_, '_>, compression: Option<&str>) -> Result<Vec<u32>, String> {
    let text: String = node
        .text()
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect();
    let packed = BASE64_STANDARD
        .decode(text)
        .map_err(|error| format!("has base64 data that won't decode: {error}"))?;
    let bytes = inflated(packed, compression)?;
    if bytes.len() % 4 != 0 {
        return Err(format!(
            "has {} bytes of tile data, which isn't a whole number of tiles",
            bytes.len()
        ));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|tile| u32::from_le_bytes([tile[0], tile[1], tile[2], tile[3]]))
        .collect())
}

pub(super) fn decode(
    node: Node<'_, '_>,
    encoding: Option<&str>,
    compression: Option<&str>,
    count: usize,
) -> Result<Vec<u32>, String> {
    let tiles = match encoding {
        None => elements(node, "tile")
            .map(|tile| {
                tile.attribute("gid").map_or(Ok(0), |gid| {
                    gid.parse()
                        .map_err(|_| format!("has gid=\"{gid}\", which isn't a tile"))
                })
            })
            .collect::<Result<Vec<u32>, String>>()?,
        Some("csv") => node
            .text()
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|field| !field.is_empty())
            .map(|field| {
                field
                    .parse()
                    .map_err(|_| format!("has \"{field}\" in its CSV data, which isn't a tile"))
            })
            .collect::<Result<Vec<u32>, String>>()?,
        Some("base64") => base64(node, compression)?,
        Some(other) => return Err(format!("uses {other} encoding, which rodeo can't read")),
    };
    if tiles.len() != count {
        return Err(format!("has {} tiles but should have {count}", tiles.len()));
    }
    Ok(tiles)
}
