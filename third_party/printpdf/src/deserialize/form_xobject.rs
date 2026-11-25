//! Form XObjects read from a PDF.
//!
//! A form XObject (ISO 32000-1, 8.10) is a content stream with a `/BBox`, a `/Matrix` and
//! `/Resources` of its own. Pages that macOS (Quartz), Word or a print driver write put the whole
//! page into one form: the page's content stream is `q ... /Fm1 Do Q` and the text is in the
//! form. [`parse_form`] reads such a form into an [`XObject::Form`]: the operators of its content
//! stream become [`Op`]s, and its `/Resources` are read by the functions that read the resources
//! of a page, into a scope of its own. A name in the form never clashes with a name on the page.
//!
//! A form is modelled only when writing it again from the model loses nothing. Otherwise it stays
//! an [`XObject::External`], byte for byte, and an info message says why. That is the case when
//!
//! - its dictionary or its `/Resources` has an entry that is not in ISO 32000-1,
//! - its content stream cannot be decoded or split into operators, uses an operator that has no
//!   [`Op`], or sets a pattern or a special color space,
//! - it selects a font that has no font program, for example a Type 3 font,
//! - or it has no `/BBox`.
//!
//! What the model has no field for but ISO 32000-1 names (`/ColorSpace`, `/Pattern`,
//! `/Properties`, `/ProcSet`, `/Group` attributes, `/OC`, `/Metadata`, ...) is kept in
//! [`FormXObject::extra`] and written back.
//!
//! Each form object is read once, however many names or pages refer to it. Reading stops at
//! [`MAX_FORM_DEPTH`] nested forms, at a form that is its own ancestor, and when a document has
//! more than [`MAX_FORMS`] forms or more than [`MAX_FORM_CONTENT_BYTES`] bytes of form content:
//! the form that is not read stays as it is.

use std::{collections::BTreeMap, sync::Arc};

use lopdf::{
    Dictionary as LopdfDictionary, Document as LopdfDocument, Object, ObjectId, Stream as LopdfStream,
};

use super::{
    convert_operations, decode_content, decode_limited, dictitem_from_lopdf_resolved, font_resources,
    get_dict_or_resolve_ref, parse_rect, parse_xobjects_internal, parsefont, read_extgstates,
    read_shadings, warn_if_budget_spent, Decoded, FontTextDecodeKind, InlineBudget,
    ParsedOrBuiltinFont, PdfWarnMsg, PendingXObject, MAX_STREAM_BYTES,
};
use crate::{
    form_walk::MAX_FORM_DEPTH,
    matrix::CurTransMat,
    ops::PdfFontHandle,
    xobject::{FormType, FormXObject, GroupXObject, GroupXObjectType},
    DictItem, ExtendedGraphicsState, ExtendedGraphicsStateId, ExtendedGraphicsStateMap, FontId, Op,
    PdfFontMap, PdfLayerMap, PdfResources, Shading, ShadingId, ShadingMap, XObjectId, XObjectMap,
};

/// Most forms that are read from one document; the others stay as they are.
const MAX_FORMS: usize = 100_000;

/// Most bytes of decoded content that the forms of one document are read from.
const MAX_FORM_CONTENT_BYTES: usize = 256 * 1024 * 1024;

/// The entries of a form's dictionary that are read, or kept in [`FormXObject::extra`].
const FORM_KEYS: &[&[u8]] = &[
    b"Type",
    b"Subtype",
    b"FormType",
    b"BBox",
    b"Matrix",
    b"Resources",
    b"Group",
    b"Ref",
    b"Metadata",
    b"PieceInfo",
    b"LastModified",
    b"StructParent",
    b"StructParents",
    b"OPI",
    b"OC",
    b"Name",
    b"Length",
    b"Filter",
    b"DecodeParms",
    b"Measure",
    b"PtData",
    b"AF",
];

/// The entries of a form's dictionary that are kept as they are, in [`FormXObject::extra`].
const EXTRA_KEYS: &[&str] = &[
    "Ref",
    "Metadata",
    "PieceInfo",
    "LastModified",
    "OPI",
    "OC",
    "Measure",
    "PtData",
    "AF",
];

/// The resource categories of ISO 32000-1, 7.8.3.
const RESOURCE_KEYS: &[&[u8]] = &[
    b"Font",
    b"XObject",
    b"ExtGState",
    b"Shading",
    b"ColorSpace",
    b"Pattern",
    b"Properties",
    b"ProcSet",
];

/// The resource categories that have no typed map, which are kept as they are.
const RAW_CATEGORIES: &[&str] = &["ColorSpace", "Pattern", "Properties", "ProcSet"];

/// A form that is read, whose nested XObjects may still wait to be decoded.
#[derive(Clone)]
pub(crate) struct PendingForm {
    /// The form, with the resources of its scope except its XObjects.
    pub(crate) form: FormXObject,
    /// The XObjects of the form's scope.
    pub(crate) xobjects: BTreeMap<XObjectId, PendingXObject>,
}

/// What reading the XObjects of one document keeps from one XObject to the next.
pub(crate) struct XObjectCtx<'a> {
    /// What is left of the budget for the references that are copied into preserved XObjects.
    pub(super) budget: InlineBudget,
    /// How the text of the document's fonts decodes: the scope of a form without `/Resources`.
    font_decode: &'a BTreeMap<String, FontTextDecodeKind>,
    /// The forms that are read already, by object. An `Err` is the reason a form is not modelled.
    /// A form that is used again is shared, not copied.
    cache: BTreeMap<ObjectId, Result<Arc<PendingForm>, String>>,
    /// The forms being read, outermost first.
    stack: Vec<ObjectId>,
    forms_left: usize,
    bytes_left: usize,
    reported_budget: bool,
}

impl<'a> XObjectCtx<'a> {
    pub(crate) fn new(font_decode: &'a BTreeMap<String, FontTextDecodeKind>) -> Self {
        XObjectCtx {
            budget: InlineBudget::new(),
            font_decode,
            cache: BTreeMap::new(),
            stack: Vec::new(),
            forms_left: MAX_FORMS,
            bytes_left: MAX_FORM_CONTENT_BYTES,
            reported_budget: false,
        }
    }
}

/// Reads the form XObject `stream`, the entry `value` of an `/XObject` dictionary, under the name
/// `name`. `None` when the form stays as it is (see the module documentation).
pub(super) fn parse_form(
    doc: &LopdfDocument,
    value: &Object,
    stream: &LopdfStream,
    name: &XObjectId,
    page: usize,
    ctx: &mut XObjectCtx<'_>,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Option<Arc<PendingForm>> {
    let id = match value {
        Object::Reference(id) => Some(*id),
        _ => None,
    };
    if let Some(id) = id {
        if let Some(done) = ctx.cache.get(&id) {
            return done.clone().ok();
        }
        if ctx.stack.contains(&id) {
            warnings.push(PdfWarnMsg::info(
                page,
                0,
                format!(
                    "form XObject {} has itself in its resources; that one is kept as it is",
                    name.0
                ),
            ));
            return None;
        }
    }
    if ctx.stack.len() >= MAX_FORM_DEPTH {
        warnings.push(PdfWarnMsg::info(
            page,
            0,
            format!(
                "form XObject {} is nested in more than {MAX_FORM_DEPTH} forms and is kept as it is",
                name.0
            ),
        ));
        return None;
    }
    if ctx.forms_left == 0 || ctx.bytes_left == 0 {
        if !ctx.reported_budget {
            ctx.reported_budget = true;
            warnings.push(PdfWarnMsg::warning(
                page,
                0,
                format!(
                    "the document has more than {MAX_FORMS} forms or {MAX_FORM_CONTENT_BYTES} \
                     bytes of form content; the forms from {} on are kept as they are",
                    name.0
                ),
            ));
        }
        return None;
    }
    ctx.forms_left = ctx.forms_left.saturating_sub(1);

    ctx.stack.extend(id);
    let result = read_form(doc, stream, page, ctx, warnings).map(Arc::new);
    if id.is_some() {
        ctx.stack.pop();
    }
    if let Some(id) = id {
        ctx.cache.insert(id, result.clone());
    }
    match result {
        Ok(form) => {
            warn_if_budget_spent(&mut ctx.budget, name, page, warnings);
            Some(form)
        }
        Err(reason) => {
            warnings.push(PdfWarnMsg::info(
                page,
                0,
                format!("form XObject {} is kept as it is: {reason}", name.0),
            ));
            None
        }
    }
}

/// The form of `stream`, or why it cannot be modelled.
fn read_form(
    doc: &LopdfDocument,
    stream: &LopdfStream,
    page: usize,
    ctx: &mut XObjectCtx<'_>,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Result<PendingForm, String> {
    let dict = &stream.dict;
    if let Some(key) = dict
        .iter()
        .map(|(key, _)| key)
        .find(|key| !FORM_KEYS.contains(&key.as_slice()))
    {
        return Err(format!(
            "its dictionary has the entry /{}, which is not modelled",
            String::from_utf8_lossy(key)
        ));
    }

    let bbox = dict
        .get(b"BBox")
        .ok()
        .and_then(|bbox| parse_rect(deref(doc, bbox)).ok())
        .ok_or_else(|| "it has no usable /BBox".to_string())?;
    let matrix = match dict.get(b"Matrix") {
        Err(_) => None,
        Ok(matrix) => Some(
            parse_matrix(deref(doc, matrix))
                .ok_or_else(|| "its /Matrix is not six numbers".to_string())?,
        ),
    };

    let limit = ctx.bytes_left.min(MAX_STREAM_BYTES);
    let content = match decode_limited(stream, limit) {
        Decoded::Data(data) => data,
        Decoded::Undecodable => return Err("its content stream cannot be decoded".to_string()),
        Decoded::TooLarge => return Err("its content stream is too large".to_string()),
    };
    ctx.bytes_left = ctx.bytes_left.saturating_sub(content.len());
    let operations = decode_content(&content)?;

    // The resources the content names its fonts in: the form's own, or the document's.
    let mut scope = match dict.get(b"Resources") {
        Err(_) => None,
        Ok(resources) => {
            let Some(resources) = get_dict_or_resolve_ref(
                "form /Resources",
                doc,
                resources,
                warnings,
                Some(page),
            ) else {
                return Err("its /Resources is not a dictionary".to_string());
            };
            Some(read_scope(doc, resources, page, ctx, warnings)?)
        }
    };
    let font_decode = match &scope {
        Some(scope) => scope.font_decode(),
        None => ctx.font_decode.clone(),
    };
    let ops = convert_operations(page, &operations, &BTreeMap::new(), &font_decode, warnings)?;
    let fonts = match scope.as_mut() {
        Some(scope) => font_resources(std::mem::take(&mut scope.fonts).into_iter()),
        None => BTreeMap::new(),
    };
    check_content(&ops, &operations, scope.as_ref().map(|_| &fonts))?;

    let mut extra = BTreeMap::new();
    let mut group = None;
    if let Ok(object) = dict.get(b"Group") {
        let item = inline(doc, object, &mut ctx.budget);
        if is_plain_transparency_group(&item) {
            group = Some(GroupXObject {
                group_type: GroupXObjectType::TransparencyGroup,
            });
        } else {
            extra.insert("Group".to_string(), item);
        }
    }
    for key in EXTRA_KEYS {
        if let Ok(object) = dict.get(key.as_bytes()) {
            extra.insert((*key).to_string(), inline(doc, object, &mut ctx.budget));
        }
    }
    let integer = |key: &[u8]| {
        dict.get(key)
            .ok()
            .and_then(|object| deref(doc, object).as_i64().ok())
    };
    let form_name = match dict.get(b"Name").ok().map(|object| deref(doc, object)) {
        Some(Object::String(bytes, _)) | Some(Object::Name(bytes)) => {
            Some(String::from_utf8_lossy(bytes).into_owned())
        }
        _ => None,
    };

    let (resources, xobjects) = match scope {
        None => (None, BTreeMap::new()),
        Some(scope) => {
            if !scope.raw.is_empty() {
                extra.insert("Resources".to_string(), DictItem::Dict { map: scope.raw });
            }
            let resources = PdfResources {
                fonts: PdfFontMap { map: fonts },
                xobjects: XObjectMap::default(),
                extgstates: ExtendedGraphicsStateMap {
                    map: scope.extgstates,
                },
                shadings: ShadingMap { map: scope.shadings },
                layers: PdfLayerMap::default(),
            };
            (Some(resources), scope.xobjects)
        }
    };

    Ok(PendingForm {
        form: FormXObject {
            form_type: FormType::Type1,
            size: None,
            bbox,
            matrix,
            ops,
            resources,
            group,
            ref_dict: None,
            metadata: None,
            piece_info: None,
            last_modified: None,
            struct_parent: integer(b"StructParent"),
            struct_parents: integer(b"StructParents"),
            opi: None,
            oc: None,
            name: form_name,
            extra,
        },
        xobjects,
    })
}

/// The resources of a form, read from its `/Resources` dictionary.
struct FormScope {
    fonts: BTreeMap<FontId, ParsedOrBuiltinFont>,
    xobjects: BTreeMap<XObjectId, PendingXObject>,
    extgstates: BTreeMap<ExtendedGraphicsStateId, ExtendedGraphicsState>,
    shadings: BTreeMap<ShadingId, Shading>,
    /// What the typed maps cannot hold: the categories without one, and the graphics states and
    /// shadings that would lose something.
    raw: BTreeMap<String, DictItem>,
}

impl FormScope {
    /// How the text of each font resource name decodes.
    fn font_decode(&self) -> BTreeMap<String, FontTextDecodeKind> {
        self.fonts
            .iter()
            .map(|(id, font)| (id.0.clone(), font.decode_kind()))
            .collect()
    }
}

/// Reads the resources of one `/Resources` dictionary into a scope: the same functions that read
/// the resources of the pages, applied to this dictionary alone.
fn read_scope(
    doc: &LopdfDocument,
    resources: &LopdfDictionary,
    page: usize,
    ctx: &mut XObjectCtx<'_>,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Result<FormScope, String> {
    if let Some(key) = resources
        .iter()
        .map(|(key, _)| key)
        .find(|key| !RESOURCE_KEYS.contains(&key.as_slice()))
    {
        return Err(format!(
            "its /Resources has the entry /{}, which is not modelled",
            String::from_utf8_lossy(key)
        ));
    }

    let fonts =
        parsefont::parse_fonts_enhanced(doc, resources, warnings, Some(page), &BTreeMap::new());
    let mut xobjects = BTreeMap::new();
    parse_xobjects_internal(doc, resources, warnings, Some(page), &mut xobjects, ctx);
    let extgstates = read_extgstates(doc, resources, page, warnings).unwrap_or_default();
    let mut shadings = BTreeMap::new();
    read_shadings(doc, resources, page, &mut shadings, warnings);

    let mut raw = BTreeMap::new();
    for category in RAW_CATEGORIES {
        if let Ok(object) = resources.get(category.as_bytes()) {
            raw.insert((*category).to_string(), inline(doc, object, &mut ctx.budget));
        }
    }
    keep_lossy_extgstates(doc, resources, &extgstates, &mut ctx.budget, &mut raw);
    keep_unmodelled_shadings(doc, resources, &shadings, &mut ctx.budget, &mut raw);

    Ok(FormScope {
        fonts,
        xobjects,
        extgstates,
        shadings,
        raw,
    })
}

/// Checks that the form is written again as it was read: no operator without an [`Op`], no color
/// that is a pattern, no font that has no font program. `fonts` is `None` for a form that uses the
/// fonts of the document.
fn check_content(
    ops: &[Op],
    operations: &[lopdf::content::Operation],
    fonts: Option<&BTreeMap<FontId, crate::font::PdfFont>>,
) -> Result<(), String> {
    if ops.iter().any(|op| matches!(op, Op::Unknown { .. })) {
        return Err("its content has an operator that no Op models".to_string());
    }
    let pattern_color = operations.iter().any(|operation| {
        matches!(operation.operator.as_str(), "sc" | "scn" | "SC" | "SCN")
            && operation
                .operands
                .iter()
                .any(|operand| matches!(operand, Object::Name(_)))
    });
    if pattern_color {
        return Err("its content sets a pattern or special color".to_string());
    }
    if let Some(fonts) = fonts {
        for op in ops {
            if let Op::SetFont {
                font: PdfFontHandle::External(id),
                ..
            } = op
            {
                if !fonts.contains_key(id) {
                    return Err(format!("it selects the font {}, which has no font program", id.0));
                }
            }
        }
    }
    Ok(())
}

/// The object `object` refers to, one level deep.
fn deref<'a>(doc: &'a LopdfDocument, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(object),
        other => other,
    }
}

/// `[a b c d e f]` as a matrix.
fn parse_matrix(object: &Object) -> Option<CurTransMat> {
    let Object::Array(entries) = object else {
        return None;
    };
    let mut matrix = [0.0f32; 6];
    if entries.len() != matrix.len() {
        return None;
    }
    for (slot, entry) in matrix.iter_mut().zip(entries) {
        *slot = match entry {
            Object::Integer(i) => *i as f32,
            Object::Real(r) => *r,
            _ => return None,
        };
    }
    Some(CurTransMat::Raw(matrix))
}

/// A copy of `object` with the references in it resolved, within the budget of the document.
fn inline(doc: &LopdfDocument, object: &Object, budget: &mut InlineBudget) -> DictItem {
    dictitem_from_lopdf_resolved(doc, object, 0, &mut Vec::new(), budget)
}

/// Whether `group` says no more than that the form is a transparency group.
fn is_plain_transparency_group(group: &DictItem) -> bool {
    let DictItem::Dict { map } = group else {
        return false;
    };
    map.keys().all(|key| key == "Type" || key == "S")
        && matches!(map.get("S"), Some(DictItem::Name(name)) if name.as_slice() == b"Transparency")
}

/// The dictionary of the resource category `key`.
fn category_dict<'a>(
    doc: &'a LopdfDocument,
    resources: &'a LopdfDictionary,
    key: &[u8],
) -> Option<&'a LopdfDictionary> {
    let object = resources.get(key).ok()?;
    get_dict_or_resolve_ref("form resources", doc, object, &mut Vec::new(), None)
}

/// Keeps the graphics states that the typed map cannot write again without loss (an entry the
/// model has no field for, such as `/SMask`) as they are.
fn keep_lossy_extgstates(
    doc: &LopdfDocument,
    resources: &LopdfDictionary,
    typed: &BTreeMap<ExtendedGraphicsStateId, ExtendedGraphicsState>,
    budget: &mut InlineBudget,
    raw: &mut BTreeMap<String, DictItem>,
) {
    let Some(category) = category_dict(doc, resources, b"ExtGState") else {
        return;
    };
    let mut kept = BTreeMap::new();
    for (name, state) in typed {
        let Ok(entry) = category.get(name.0.as_bytes()) else {
            continue;
        };
        let Some(source) =
            get_dict_or_resolve_ref("form graphics state", doc, entry, &mut Vec::new(), None)
        else {
            continue;
        };
        let written = crate::graphics::extgstate_to_dict(state);
        if source
            .iter()
            .any(|(key, _)| key.as_slice() != b"Type" && !written.has(key))
        {
            kept.insert(name.0.clone(), inline(doc, entry, budget));
        }
    }
    if !kept.is_empty() {
        raw.insert("ExtGState".to_string(), DictItem::Dict { map: kept });
    }
}

/// Keeps the shadings that the typed map cannot hold, or cannot write again without loss, as they
/// are.
fn keep_unmodelled_shadings(
    doc: &LopdfDocument,
    resources: &LopdfDictionary,
    typed: &BTreeMap<ShadingId, Shading>,
    budget: &mut InlineBudget,
    raw: &mut BTreeMap<String, DictItem>,
) {
    let Some(category) = category_dict(doc, resources, b"Shading") else {
        return;
    };
    let mut kept = BTreeMap::new();
    for (key, entry) in category.iter() {
        let name = String::from_utf8_lossy(key).into_owned();
        let lossy = match typed.get(&ShadingId(name.clone())) {
            None => true,
            Some(shading) => {
                let written = shading.to_dict();
                get_dict_or_resolve_ref("form shading", doc, entry, &mut Vec::new(), None)
                    .is_none_or(|source| source.iter().any(|(key, _)| !written.has(key)))
            }
        };
        if lossy {
            kept.insert(name, inline(doc, entry, budget));
        }
    }
    if !kept.is_empty() {
        raw.insert("Shading".to_string(), DictItem::Dict { map: kept });
    }
}
