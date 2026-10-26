use roxmltree::Node;

use super::parse::elements;

pub(super) fn decode(node: Node<'_, '_>, count: usize) -> Result<Vec<u32>, String> {
    let tiles = match node.attribute("encoding") {
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
        Some(other) => return Err(format!("uses {other} encoding, which rodeo can't read")),
    };
    if tiles.len() != count {
        return Err(format!("has {} tiles but should have {count}", tiles.len()));
    }
    Ok(tiles)
}
