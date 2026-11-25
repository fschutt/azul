//! Writing the XObjects of a resource scope, forms in particular.
//!
//! A page and a form are written by the same functions: both are a list of [`Op`]s that name
//! fonts, XObjects, graphics states and shadings in a [`PdfResources`] scope. The page's scope is
//! the document's resources, shared by all pages; a form's scope is its own. Writing a form means
//! writing its scope (the fonts it uses, subset to its glyphs, its nested XObjects, its graphics
//! states and shadings) as the form's `/Resources`, and its ops as the content stream.

use std::collections::BTreeMap;

use lopdf::{
    Dictionary as LoDictionary,
    Object::{self, Array, Dictionary, Integer, Name, Real, String as LoString},
    Stream as LoStream,
};

use super::{
    extgstate_dict, prepare_fonts_for_streams, shading_dict, translate_operations,
    used_builtin_fonts, write_font_dict, PdfSaveOptions, RuntimeFontInfo,
};
use crate::{
    form_walk::{inheriting_streams, MAX_FORM_DEPTH},
    xobject::{build_dict, DictItem, FormXObject},
    FontId, Op, PdfWarnMsg, Rect, XObject, XObjectId,
};

/// What the content that names the XObjects of a scope needs to know about that scope.
pub(crate) struct SaveScope<'a> {
    /// The fonts of the scope as they are written (glyph ids, subsetting), to encode text.
    pub font_infos: &'a BTreeMap<FontId, RuntimeFontInfo>,
    /// The XObjects of the scope, to size an image that is drawn with `Do`.
    pub xobjects: &'a BTreeMap<XObjectId, XObject>,
    /// How many forms enclose the content, so that a model nested too deeply is not written.
    pub depth: usize,
}

/// Writes every XObject of a scope as an object of `doc` and returns the `/XObject` dictionary.
pub(crate) fn write_xobjects(
    doc: &mut lopdf::Document,
    xobjects: &BTreeMap<XObjectId, XObject>,
    scope: &SaveScope<'_>,
    opts: &PdfSaveOptions,
    warnings: &mut Vec<PdfWarnMsg>,
) -> LoDictionary {
    let mut dict = LoDictionary::new();
    for (id, xobject) in xobjects.iter() {
        let object_id = match xobject {
            XObject::Image(_image) => {
                #[cfg(feature = "images")]
                {
                    let stream = crate::image::image_to_stream(
                        _image.clone(),
                        doc,
                        opts.image_optimization.as_ref(),
                    );
                    doc.add_object(stream)
                }
                #[cfg(not(feature = "images"))]
                {
                    // Raw pixels with FlateDecode need no image codec.
                    let stream = crate::image_types::raw_image_to_basic_stream(_image, doc);
                    doc.add_object(stream)
                }
            }
            XObject::Form(form) => {
                if scope.depth >= MAX_FORM_DEPTH {
                    warnings.push(PdfWarnMsg::warning(
                        0,
                        0,
                        format!(
                            "form {} is nested deeper than {MAX_FORM_DEPTH} forms and is not \
                             written",
                            id.0
                        ),
                    ));
                    continue;
                }
                let stream = write_form(form, doc, scope, opts, warnings);
                doc.add_object(stream)
            }
            XObject::External(external_xobject) => {
                // Streams nested in the dictionary (functions, ICC profiles, soft masks, ...)
                // become indirect objects, as PDF requires (ISO 32000-1, 7.3.8.1).
                let stream = external_xobject
                    .stream
                    .into_lopdf_with_indirect_streams(doc);
                doc.add_object(stream)
            }
        };
        dict.set(id.0.clone(), Object::Reference(object_id));
    }
    dict
}

/// The stream of a form: its dictionary, its `/Resources` and the content stream of its ops.
///
/// `parent` is the scope the form is drawn in. A form without resources of its own writes its
/// ops against it.
fn write_form(
    form: &FormXObject,
    doc: &mut lopdf::Document,
    parent: &SaveScope<'_>,
    opts: &PdfSaveOptions,
    warnings: &mut Vec<PdfWarnMsg>,
) -> LoStream {
    let mut dict = form_dictionary(form, doc);

    // One text object per line instead of per glyph run, as for the page (see text_merge.rs).
    let merged;
    let ops: &[Op] = if opts.optimize {
        merged = crate::text_merge::merge_text_runs(&form.ops);
        &merged
    } else {
        &form.ops
    };

    let content = match &form.resources {
        Some(local) => {
            // The glyphs the form uses decide which part of each font is written.
            let mut streams: Vec<&[Op]> = vec![form.ops.as_slice()];
            streams.extend(inheriting_streams(local));
            let (font_infos, subset_fonts) =
                prepare_fonts_for_streams(&local.fonts.map, &streams, opts.subset_fonts, warnings);
            let scope = SaveScope {
                font_infos: &font_infos,
                xobjects: &local.xobjects.map,
                depth: parent.depth.saturating_add(1),
            };
            let xobject_dict = write_xobjects(doc, &local.xobjects.map, &scope, opts, warnings);
            let font_dict = write_font_dict(doc, &subset_fonts, used_builtin_fonts(&streams), warnings);

            let mut resources = LoDictionary::new();
            for (category, entries) in [
                ("Font", font_dict),
                ("XObject", xobject_dict),
                ("ExtGState", extgstate_dict(local)),
                ("Shading", shading_dict(local)),
            ] {
                if !entries.is_empty() {
                    resources.set(category, Dictionary(entries));
                }
            }
            merge_extra_resources(&mut resources, form.extra.get("Resources"), doc);
            dict.set("Resources", Dictionary(resources));

            translate_operations(ops, &font_infos, &local.xobjects.map, opts.secure, warnings)
        }
        None => {
            let mut resources = LoDictionary::new();
            merge_extra_resources(&mut resources, form.extra.get("Resources"), doc);
            if !resources.is_empty() {
                dict.set("Resources", Dictionary(resources));
            }
            translate_operations(ops, parent.font_infos, parent.xobjects, opts.secure, warnings)
        }
    };

    // What the model has no field for is written as it was read.
    for (key, item) in form.extra.iter().filter(|(key, _)| key.as_str() != "Resources") {
        dict.set(key.clone(), item.to_lopdf_with_indirect_streams(doc));
    }

    LoStream::new(dict, content).with_compression(true)
}

/// The dictionary of a form without `/Resources`.
fn form_dictionary(f: &FormXObject, doc: &mut lopdf::Document) -> LoDictionary {
    let mut dict = LoDictionary::from_iter(vec![
        ("Type", Name("XObject".into())),
        ("Subtype", Name("Form".into())),
        ("FormType", Integer(f.form_type.number())),
        ("BBox", bbox_array(&f.bbox)),
    ]);

    if let Some(matrix) = f.matrix.as_ref() {
        dict.set(
            "Matrix",
            Array(matrix.as_array().into_iter().map(Real).collect()),
        );
    }

    if let Some(g) = f.group.as_ref() {
        let group_dict = LoDictionary::from_iter(vec![
            ("Type", Name("Group".into())),
            ("S", Name(g.group_type.get_id().into())),
        ]);

        dict.set("Group", Dictionary(group_dict));
    }

    if let Some(r) = f.ref_dict.as_ref() {
        dict.set("Ref", build_dict(r));
    }

    if let Some(r) = f.metadata.as_ref() {
        dict.set("Metadata", doc.add_object(build_dict(r)));
    }

    if let Some(r) = f.piece_info.as_ref() {
        dict.set("PieceInfo", doc.add_object(build_dict(r)));
    }

    if let Some(r) = f.last_modified.as_ref() {
        dict.set(
            "LastModified",
            LoString(
                crate::utils::to_pdf_time_stamp_metadata(r).into_bytes(),
                lopdf::StringFormat::Literal,
            ),
        );
    }

    if let Some(r) = f.opi.as_ref() {
        dict.set("OPI", build_dict(r));
    }

    if let Some(r) = f.oc.as_ref() {
        dict.set("OC", build_dict(r));
    }

    if let Some(r) = f.name.as_ref() {
        dict.set(
            "Name",
            LoString(r.clone().into(), lopdf::StringFormat::Literal),
        );
    }

    if let Some(sp) = &f.struct_parents {
        dict.set("StructParents", Integer(*sp));
    } else if let Some(sp) = &f.struct_parent {
        dict.set("StructParent", Integer(*sp));
    }

    dict
}

/// `[x0 y0 x1 y1]`, the corners of the rectangle.
fn bbox_array(rect: &Rect) -> Object {
    Array(vec![
        Real(rect.x.0),
        Real(rect.y.0),
        Real(rect.x.0 + rect.width.0),
        Real(rect.y.0 + rect.height.0),
    ])
}

/// Adds the resources that the model keeps as they were read (`extra`, under `Resources`) to the
/// dictionary built from the typed maps. An entry of the same name replaces the typed one.
fn merge_extra_resources(
    resources: &mut LoDictionary,
    extra: Option<&DictItem>,
    doc: &mut lopdf::Document,
) {
    let Some(DictItem::Dict { map }) = extra else {
        return;
    };
    for (category, item) in map.iter() {
        let object = item.to_lopdf_with_indirect_streams(doc);
        if let Dictionary(add) = &object {
            if let Ok(Dictionary(have)) = resources.get_mut(category.as_bytes()) {
                for (key, value) in add.iter() {
                    have.set(key.clone(), value.clone());
                }
                continue;
            }
        }
        resources.set(category.clone(), object);
    }
}
