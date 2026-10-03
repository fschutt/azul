//! Documents as files, in the layout the per-user S3 bucket will have:
//!
//! ```text
//! photo/<uuid>/doc.json                         the layer tree, sizes, settings
//! photo/<uuid>/layers/<layer id>/<tx>_<ty>.png  one PNG per non-empty tile
//! ```
//!
//! Written through `azul_storage::Drive` (a `LocalDrive` on the user's data
//! folder now, an `S3Drive` later - nothing else changes), from an azul
//! `Thread`, never from a callback. The tiles go first and `doc.json` last,
//! so a reader never meets a document that names a tile not written yet;
//! tiles of an older save that the new one no longer has are deleted.
//!
//! PNG encoding and decoding are passed in (azul's encoder in the app), so
//! this module is tested with a plain byte format.

use std::collections::HashSet;

use azul_storage::{Drive, ListRequest};
use serde::{Deserialize, Serialize};

use crate::raster::{
    tile::TILE, Adjustment, BlendMode, Document, IRect, Layer, LayerContent, LayerId, TileGrid,
};

/// The folder of every AzPhoto document.
pub const PREFIX: &str = "photo/";
/// The format tag in `doc.json`.
pub const FORMAT: &str = "azphoto/1";

/// `photo/<uuid>/doc.json`.
#[must_use]
pub fn doc_key(uuid: &str) -> String {
    format!("{PREFIX}{uuid}/doc.json")
}

/// `photo/<uuid>/layers/`.
#[must_use]
pub fn layers_prefix(uuid: &str) -> String {
    format!("{PREFIX}{uuid}/layers/")
}

/// `photo/<uuid>/layers/<layer>/<tx>_<ty>.png`.
#[must_use]
pub fn tile_key(uuid: &str, layer: LayerId, tx: u32, ty: u32) -> String {
    format!("{}{layer}/{tx}_{ty}.png", layers_prefix(uuid))
}

/// `doc.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocFile {
    pub format: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub tile: u32,
    pub next_id: LayerId,
    /// Bottom first.
    pub layers: Vec<LayerFile>,
}

/// One layer in `doc.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerFile {
    pub id: LayerId,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    pub blend: BlendMode,
    #[serde(flatten)]
    pub kind: LayerKindFile,
}

/// What a layer holds, in `doc.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum LayerKindFile {
    /// The tiles that have pixels (`[tx, ty]`); each is a PNG of its part of
    /// the document.
    Raster { tiles: Vec<[u32; 2]> },
    Adjustment { adjustment: Adjustment },
    Group { expanded: bool, children: Vec<LayerFile> },
}

/// The file form of a document (no pixels: those are the tile files).
#[must_use]
pub fn doc_to_file(doc: &Document, name: &str) -> DocFile {
    fn layer_file(l: &Layer) -> LayerFile {
        let kind = match &l.content {
            LayerContent::Raster(g) => LayerKindFile::Raster {
                tiles: g.non_empty_tiles().into_iter().map(|(x, y)| [x, y]).collect(),
            },
            LayerContent::Adjustment(a) => LayerKindFile::Adjustment {
                adjustment: a.clone(),
            },
            LayerContent::Group(children) => LayerKindFile::Group {
                expanded: l.expanded,
                children: children.iter().map(layer_file).collect(),
            },
        };
        LayerFile {
            id: l.id,
            name: l.name.clone(),
            visible: l.visible,
            locked: l.locked,
            opacity: l.opacity,
            blend: l.blend,
            kind,
        }
    }
    DocFile {
        format: FORMAT.to_string(),
        name: name.to_string(),
        width: doc.width,
        height: doc.height,
        tile: TILE,
        next_id: doc.next_id,
        layers: doc.layers.iter().map(layer_file).collect(),
    }
}

/// A document from its file and its tiles: `tile(layer, tx, ty)` answers the
/// RGBA8 rows of that tile's part of the document.
pub fn file_to_doc(
    file: &DocFile,
    tile: &mut dyn FnMut(LayerId, u32, u32) -> Result<Vec<u8>, String>,
) -> Result<Document, String> {
    if file.format != FORMAT {
        return Err(format!("not an AzPhoto document ({})", file.format));
    }
    if file.tile != TILE {
        return Err(format!("tiles of {} px are not supported", file.tile));
    }
    fn layer_of(
        f: &LayerFile,
        w: u32,
        h: u32,
        tile: &mut dyn FnMut(LayerId, u32, u32) -> Result<Vec<u8>, String>,
    ) -> Result<Layer, String> {
        let mut layer = match &f.kind {
            LayerKindFile::Raster { tiles } => {
                let mut grid = TileGrid::new(w, h);
                for [tx, ty] in tiles {
                    if *tx >= grid.cols() || *ty >= grid.rows() {
                        return Err(format!("layer {} names tile {tx}_{ty} outside the image", f.id));
                    }
                    let rect = grid.tile_rect(*tx, *ty);
                    let rgba = tile(f.id, *tx, *ty)?;
                    if rgba.len() != (rect.w * rect.h * 4) as usize {
                        return Err(format!("tile {tx}_{ty} of layer {} has the wrong size", f.id));
                    }
                    grid.write_rect(rect, &rgba);
                }
                Layer::raster(f.id, f.name.clone(), grid)
            }
            LayerKindFile::Adjustment { adjustment } => Layer::adjustment(f.id, adjustment.clone()),
            LayerKindFile::Group { expanded, children } => {
                let mut list = Vec::with_capacity(children.len());
                for c in children {
                    list.push(layer_of(c, w, h, tile)?);
                }
                let mut g = Layer::group(f.id, f.name.clone(), list);
                g.expanded = *expanded;
                g
            }
        };
        layer.name = f.name.clone();
        layer.visible = f.visible;
        layer.locked = f.locked;
        layer.opacity = f.opacity.clamp(0.0, 1.0);
        layer.blend = f.blend;
        Ok(layer)
    }
    let mut doc = Document::new(file.width, file.height);
    for f in &file.layers {
        doc.layers.push(layer_of(f, file.width, file.height, tile)?);
    }
    let max_id = crate::raster::layer::all_ids(&doc.layers).into_iter().max().unwrap_or(0);
    doc.next_id = file.next_id.max(max_id + 1);
    Ok(doc)
}

/// Encode RGBA8 rows of `w` x `h` as PNG bytes.
pub type EncodeFn<'a> = &'a dyn Fn(u32, u32, &[u8]) -> Result<Vec<u8>, String>;
/// Decode PNG bytes to (`w`, `h`, RGBA8 rows).
pub type DecodeFn<'a> = &'a dyn Fn(&[u8]) -> Result<(u32, u32, Vec<u8>), String>;

/// What a save wrote.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Saved {
    pub tiles: usize,
    pub bytes: u64,
    pub deleted: usize,
}

/// Write the document: every non-empty tile, then `doc.json`; then drop the
/// tiles an older save left that this one does not have.
pub fn save(drive: &dyn Drive, uuid: &str, name: &str, doc: &Document, encode: EncodeFn<'_>) -> Result<Saved, String> {
    let mut saved = Saved::default();
    let mut written: HashSet<String> = HashSet::new();
    fn walk(
        list: &[Layer],
        f: &mut dyn FnMut(&Layer, &TileGrid) -> Result<(), String>,
    ) -> Result<(), String> {
        for l in list {
            match &l.content {
                LayerContent::Raster(g) => f(l, g)?,
                LayerContent::Group(children) => walk(children, f)?,
                LayerContent::Adjustment(_) => {}
            }
        }
        Ok(())
    }
    walk(&doc.layers, &mut |l, grid| {
        for (tx, ty) in grid.non_empty_tiles() {
            let rect: IRect = grid.tile_rect(tx, ty);
            let png = encode(rect.w as u32, rect.h as u32, &grid.read_rect(rect))?;
            let key = tile_key(uuid, l.id, tx, ty);
            drive.put(&key, &png).map_err(|e| e.to_string())?;
            saved.tiles += 1;
            saved.bytes += png.len() as u64;
            written.insert(key);
        }
        Ok(())
    })?;
    let json = serde_json::to_vec_pretty(&doc_to_file(doc, name)).map_err(|e| e.to_string())?;
    drive.put(&doc_key(uuid), &json).map_err(|e| e.to_string())?;
    saved.bytes += json.len() as u64;
    // Tiles of an older save that this one no longer has.
    let mut request = ListRequest::recursive(&layers_prefix(uuid));
    loop {
        let page = drive.list(&request).map_err(|e| e.to_string())?;
        for object in &page.objects {
            if !written.contains(&object.key) {
                drive.delete(&object.key).map_err(|e| e.to_string())?;
                saved.deleted += 1;
            }
        }
        match page.next {
            Some(token) => request = request.with_continuation(token),
            None => break,
        }
    }
    Ok(saved)
}

/// Read the document `uuid`: its name and its layers.
pub fn load(drive: &dyn Drive, uuid: &str, decode: DecodeFn<'_>) -> Result<(String, Document), String> {
    let json = drive.get(&doc_key(uuid)).map_err(|e| e.to_string())?;
    let file: DocFile = serde_json::from_slice(&json).map_err(|e| format!("doc.json: {e}"))?;
    let doc = file_to_doc(&file, &mut |layer, tx, ty| {
        let bytes = drive
            .get(&tile_key(uuid, layer, tx, ty))
            .map_err(|e| e.to_string())?;
        decode(&bytes).map(|(_, _, rgba)| rgba)
    })?;
    Ok((file.name, doc))
}

/// One saved document, for the start screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocEntry {
    pub uuid: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Seconds since 1970 of `doc.json`, when the drive knows.
    pub modified: Option<u64>,
}

/// Every document under `photo/`, newest first.
pub fn list(drive: &dyn Drive) -> Result<Vec<DocEntry>, String> {
    let mut out = Vec::new();
    let mut request = ListRequest::folder(PREFIX);
    loop {
        let page = drive.list(&request).map_err(|e| e.to_string())?;
        for folder in &page.folders {
            let uuid = folder.trim_start_matches(PREFIX).trim_end_matches('/').to_string();
            let Ok(info) = drive.head(&doc_key(&uuid)) else {
                continue;
            };
            let Ok(json) = drive.get(&doc_key(&uuid)) else {
                continue;
            };
            let Ok(file) = serde_json::from_slice::<DocFile>(&json) else {
                continue;
            };
            out.push(DocEntry {
                uuid,
                name: file.name,
                width: file.width,
                height: file.height,
                modified: info.modified,
            });
        }
        match page.next {
            Some(token) => request = request.with_continuation(token),
            None => break,
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::{RasterEngine, TileEngine};
    use azul_storage::LocalDrive;

    /// A stand-in for PNG: width, height (4 bytes each, LE), then the rows.
    fn fake_encode(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
        let mut out = w.to_le_bytes().to_vec();
        out.extend_from_slice(&h.to_le_bytes());
        out.extend_from_slice(rgba);
        Ok(out)
    }

    fn fake_decode(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
        let w = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let h = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        Ok((w, h, bytes[8..].to_vec()))
    }

    fn temp_drive(name: &str) -> (LocalDrive, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("azphoto-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        (LocalDrive::new(&dir), dir)
    }

    fn sample_doc() -> Document {
        let mut e = TileEngine::new(Document::with_background(300, 260, [250, 250, 250, 255]));
        e.apply(crate::raster::Op::NewLayer { name: "Ink".into() }).unwrap();
        e.apply(crate::raster::Op::DrawShape {
            shape: crate::raster::Shape::Rect(IRect::new(270, 10, 10, 10)),
            color: [200, 0, 0, 255],
        })
        .unwrap();
        e.apply(crate::raster::Op::NewAdjustment(Adjustment::HueSaturation {
            hue: 30.0,
            saturation: 0.2,
            lightness: 0.0,
        }))
        .unwrap();
        e.document().clone()
    }

    #[test]
    fn the_keys_follow_the_bucket_layout() {
        assert_eq!(doc_key("u1"), "photo/u1/doc.json");
        assert_eq!(tile_key("u1", 7, 2, 3), "photo/u1/layers/7/2_3.png");
    }

    #[test]
    fn a_saved_document_loads_back_pixel_for_pixel() {
        let (drive, dir) = temp_drive("roundtrip");
        let doc = sample_doc();
        let saved = save(&drive, "u1", "Harbour", &doc, &fake_encode).unwrap();
        assert_eq!(saved.tiles, 4 + 1, "four background tiles and the one ink tile");
        let (name, back) = load(&drive, "u1", &fake_decode).unwrap();
        assert_eq!(name, "Harbour");
        assert_eq!((back.width, back.height), (300, 260));
        assert_eq!(back.layers.len(), 3);
        assert_eq!(back.layers[2].name, "Hue/Saturation");
        assert!(matches!(back.layers[2].content, LayerContent::Adjustment(Adjustment::HueSaturation { .. })));
        for (a, b) in doc.layers.iter().zip(&back.layers) {
            assert_eq!((a.id, a.visible, a.opacity, a.blend), (b.id, b.visible, b.opacity, b.blend));
            if let (Some(g), Some(h)) = (a.grid(), b.grid()) {
                assert_eq!(g.to_rgba(), h.to_rgba(), "layer {}", a.name);
            }
        }
        assert!(back.next_id >= doc.next_id);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn saving_again_drops_the_tiles_the_document_no_longer_has() {
        let (drive, dir) = temp_drive("resave");
        let mut doc = sample_doc();
        save(&drive, "u2", "A", &doc, &fake_encode).unwrap();
        let ink = doc.layers[1].id;
        doc.layers.remove(1);
        let saved = save(&drive, "u2", "A", &doc, &fake_encode).unwrap();
        assert_eq!(saved.deleted, 1, "the ink tile is gone");
        assert!(drive.head(&tile_key("u2", ink, 1, 0)).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_document_list_names_every_saved_document() {
        let (drive, dir) = temp_drive("list");
        save(&drive, "a", "First", &sample_doc(), &fake_encode).unwrap();
        save(&drive, "b", "Second", &sample_doc(), &fake_encode).unwrap();
        let mut names: Vec<String> = list(&drive).unwrap().into_iter().map(|e| e.name).collect();
        names.sort();
        assert_eq!(names, vec!["First", "Second"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_export_lands_in_the_documents_exports_folder_and_a_resave_keeps_it() {
        let (drive, dir) = temp_drive("export");
        let doc = sample_doc();
        save(&drive, "u3", "A", &doc, &fake_encode).unwrap();
        let key = export(&drive, "u3", "Harbour.png", b"PNGDATA").unwrap();
        assert_eq!(key, "photo/u3/exports/Harbour.png", "into the data tree, beside the document");
        assert_eq!(drive.get(&key).unwrap(), b"PNGDATA".to_vec());
        save(&drive, "u3", "A", &doc, &fake_encode).unwrap();
        assert!(drive.head(&key).is_ok(), "saving the document again keeps its exports");
        let names: Vec<String> = list(&drive).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["A".to_string()], "an exports folder is not a document");
        assert_eq!(export_key("u3", "a/b:c.png"), "photo/u3/exports/a_b_c.png", "one key segment");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_foreign_file_is_refused() {
        let mut file = doc_to_file(&sample_doc(), "x");
        file.format = "something/9".into();
        assert!(file_to_doc(&file, &mut |_, _, _| Ok(Vec::new())).is_err());
    }
}
