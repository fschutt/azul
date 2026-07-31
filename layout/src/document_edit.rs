//! Apply a [`DocumentOperation`] to a plain XML tree — the helper for apps
//! WITHOUT their own document model (Path 2; e.g. the PDF editor, whose
//! semantic-HTML model is exactly `Vec<XmlNodeChild>`).
//!
//! Azul records structural intent (`DocumentChangeset`); the app applies it
//! to ITS model and regenerates the DOM — the `StyledDom` is never mutated.
//! An app with its own model (Path 1) ignores this module entirely. An app
//! holding the XML tree calls [`apply_document_operation`], then
//! `CallbackInfo::mark_document_edit_applied(changeset.id)` (the commit
//! handshake), then returns `Update::RefreshDom`.
//!
//! Every successful apply returns the INVERSE operation — tree-shaped undo
//! for free (`UndoRedoManager` stores it; undoing re-RECORDS the inverse
//! through the same record→apply loop, it never mutates either).

use azul_core::xml::{XmlNode, XmlNodeChild};
use azul_css::AzString;

use crate::managers::changeset::{
    DocOpInsertBlock, DocOpMergeBlocks, DocOpSplitBlock, DocumentChangeset, DocumentOperation,
    EditResumePoint,
};

/// The outcome of a successful apply.
#[derive(Debug, Clone)]
pub struct AppliedEdit {
    /// Where the caret should land (passed through from the changeset —
    /// already expressed re-render-stably).
    pub resume: EditResumePoint,
    /// The operation that undoes this one. NodeId fields are advisory (they
    /// refer to the generation the ORIGINAL changeset was recorded against);
    /// the structural payload (indices via the resume point) is what the
    /// undo path re-records.
    pub inverse: DocumentOperation,
}

/// Why an apply failed. Failures leave the tree UNCHANGED.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentEditError {
    /// `host_path` did not resolve to an element in the tree.
    HostNotFound,
    /// The operation's block index does not exist under the host.
    TargetNotFound,
    /// The operation kind is recorded-only for now (Wrap/Unwrap/ReplaceRange).
    Unsupported(&'static str),
    /// An XML fragment failed to parse.
    Fragment(String),
}

/// Block-level tags whose SECOND half after a split becomes a `<p>`
/// (matching contenteditable browsers: splitting a heading yields a
/// paragraph).
const HEADING_TAGS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6"];

/// Apply a structural changeset to the XML tree the app holds.
///
/// * `tree` — the document (as produced by `xml::parse_xml_string` or built
///   by the app).
/// * `host_path` — child-index path from the tree root to the editing-host
///   ELEMENT (`[]` = the tree's single root element; text children count in
///   the indexing, same as the resume point's `block_path`).
/// * `changeset` — as delivered by `CallbackInfo::get_document_edit_clone`.
///
/// Index resolution uses the changeset's OWN resume point (recorded by the
/// same engine that computes it, so the two cannot drift): a split targets
/// `resume.block_path[0] - 1`, a merge keeps `resume.block_path[0]`.
///
/// # Errors
///
/// Returns a [`DocumentEditError`] (tree unchanged) on unresolvable paths,
/// missing targets, unparseable fragments, or record-only operation kinds.
pub fn apply_document_operation(
    tree: &mut Vec<XmlNodeChild>,
    host_path: &[u32],
    changeset: &DocumentChangeset,
) -> Result<AppliedEdit, DocumentEditError> {
    let host = resolve_host(tree, host_path).ok_or(DocumentEditError::HostNotFound)?;
    let resume_index = changeset
        .resume
        .block_path
        .as_ref()
        .first()
        .copied()
        .unwrap_or(0);

    let inverse = match &changeset.operation {
        DocumentOperation::SplitBlock(split) => {
            let block_index = resume_index.saturating_sub(1) as usize;
            apply_split(host, block_index, split)?
        }
        DocumentOperation::MergeBlocks(merge) => {
            let first_index = resume_index as usize;
            apply_merge(host, first_index, merge, changeset.resume.text_offset)?
        }
        DocumentOperation::InsertBlock(insert) => apply_insert(host, insert)?,
        DocumentOperation::DeleteBlock(_) => {
            let index = resume_index as usize;
            apply_delete(host, index, changeset)?
        }
        DocumentOperation::WrapRange(_) => {
            return Err(DocumentEditError::Unsupported("WrapRange"));
        }
        DocumentOperation::UnwrapRange(_) => {
            return Err(DocumentEditError::Unsupported("UnwrapRange"));
        }
        DocumentOperation::ReplaceRange(_) => {
            return Err(DocumentEditError::Unsupported("ReplaceRange"));
        }
    };

    Ok(AppliedEdit {
        resume: changeset.resume.clone(),
        inverse,
    })
}

/// Walk `host_path` down the tree to the editing-host element.
fn resolve_host<'a>(tree: &'a mut Vec<XmlNodeChild>, host_path: &[u32]) -> Option<&'a mut XmlNode> {
    // The root level: pick the path's first index, or the single root element.
    let (first, rest) = match host_path.split_first() {
        Some((f, r)) => (*f as usize, r),
        None => {
            // No path: the host is the tree's ONLY element child.
            let mut elements = tree.iter_mut().filter_map(|c| match c {
                XmlNodeChild::Element(e) => Some(e),
                XmlNodeChild::Text(_) => None,
            });
            let host = elements.next()?;
            if elements.next().is_some() {
                return None; // ambiguous — require a path
            }
            return Some(host);
        }
    };

    let mut node = match tree.get_mut(first)? {
        XmlNodeChild::Element(e) => e,
        XmlNodeChild::Text(_) => return None,
    };
    for &idx in rest {
        node = match node.children.as_mut().get_mut(idx as usize)? {
            XmlNodeChild::Element(e) => e,
            XmlNodeChild::Text(_) => return None,
        };
    }
    Some(node)
}

/// Split the block at `block_index` under `host` at the recorded byte offset;
/// the second half becomes a sibling at `block_index + 1` (same tag; headings
/// yield `<p>`). Caret → offset 0 of the new block (already in the resume).
fn apply_split(
    host: &mut XmlNode,
    block_index: usize,
    split: &DocOpSplitBlock,
) -> Result<DocumentOperation, DocumentEditError> {
    let mut children: Vec<XmlNodeChild> =
        core::mem::take(&mut host.children).into_library_owned_vec();
    let restore = |host: &mut XmlNode, v: Vec<XmlNodeChild>| host.children = v.into();
    let Some(XmlNodeChild::Element(block)) = children.get_mut(block_index) else {
        restore(host, children);
        return Err(DocumentEditError::TargetNotFound);
    };

    let at_byte = split.at.cluster_id.start_byte_in_run as usize;

    // Partition the block's children at the byte offset, splitting the text
    // node the offset lands in. v1 counts bytes across DIRECT text children
    // (matching the caret model for plain blocks); nested inline elements
    // travel wholesale to whichever side their start falls on.
    let mut consumed = 0_usize;
    let mut first_half: Vec<XmlNodeChild> = Vec::new();
    let mut second_half: Vec<XmlNodeChild> = Vec::new();
    for child in block.children.as_ref().iter().cloned() {
        match child {
            XmlNodeChild::Text(t) => {
                let s = t.as_str();
                let start = consumed;
                let end = consumed + s.len();
                if end <= at_byte {
                    first_half.push(XmlNodeChild::Text(t));
                } else if start >= at_byte {
                    second_half.push(XmlNodeChild::Text(t));
                } else {
                    let cut = (at_byte - start).min(s.len());
                    // Clamp to a char boundary (never panic on multi-byte).
                    let cut = (0..=cut)
                        .rev()
                        .find(|&c| s.is_char_boundary(c))
                        .unwrap_or(0);
                    first_half.push(XmlNodeChild::Text(s[..cut].into()));
                    second_half.push(XmlNodeChild::Text(s[cut..].into()));
                }
                consumed = end;
            }
            other @ XmlNodeChild::Element(_) => {
                if consumed < at_byte {
                    first_half.push(other);
                } else {
                    second_half.push(other);
                }
            }
        }
    }

    let original_tag = block.node_type.as_str().to_string();
    let second_tag: AzString = if HEADING_TAGS.contains(&original_tag.as_str()) {
        "p".into()
    } else {
        original_tag.clone().into()
    };

    block.children = first_half.into();
    let second = XmlNode {
        node_type: second_tag.into(),
        attributes: Default::default(),
        children: second_half.into(),
    };
    children.insert(block_index + 1, XmlNodeChild::Element(second));
    restore(host, children);

    Ok(DocumentOperation::MergeBlocks(DocOpMergeBlocks {
        first: split.block,
        second: split.block,
        join_cursor: split.at,
    }))
}

/// Merge the block at `first_index + 1` into the one at `first_index`
/// (inline markup preserved — children are appended, adjacent text nodes
/// coalesced). Caret → the join point (`join_offset`, already in the resume).
fn apply_merge(
    host: &mut XmlNode,
    first_index: usize,
    merge: &DocOpMergeBlocks,
    join_offset: u32,
) -> Result<DocumentOperation, DocumentEditError> {
    let mut children: Vec<XmlNodeChild> =
        core::mem::take(&mut host.children).into_library_owned_vec();
    let fail = |host: &mut XmlNode, v: Vec<XmlNodeChild>| {
        host.children = v.into();
        Err(DocumentEditError::TargetNotFound)
    };
    if first_index + 1 >= children.len() {
        return fail(host, children);
    }
    if !matches!(children.get(first_index + 1), Some(XmlNodeChild::Element(_)))
        || !matches!(children.get(first_index), Some(XmlNodeChild::Element(_)))
    {
        return fail(host, children);
    }
    let XmlNodeChild::Element(second) = children.remove(first_index + 1) else {
        unreachable!("checked above");
    };
    let Some(XmlNodeChild::Element(first)) = children.get_mut(first_index) else {
        unreachable!("checked above");
    };

    let mut first_children: Vec<XmlNodeChild> =
        core::mem::take(&mut first.children).into_library_owned_vec();
    for child in second.children.as_ref().iter().cloned() {
        match (first_children.last_mut(), &child) {
            // Coalesce adjacent text so a later split round-trips.
            (Some(XmlNodeChild::Text(a)), XmlNodeChild::Text(b)) => {
                let joined = format!("{}{}", a.as_str(), b.as_str());
                *a = joined.into();
            }
            _ => first_children.push(child),
        }
    }
    first.children = first_children.into();
    host.children = children.into();

    let mut inverse_cursor = merge.join_cursor;
    inverse_cursor.cluster_id.start_byte_in_run = join_offset;
    Ok(DocumentOperation::SplitBlock(DocOpSplitBlock {
        block: merge.first,
        at: inverse_cursor,
    }))
}

/// Insert the blocks parsed from `insert.xml_fragment` at `insert.index`.
fn apply_insert(
    host: &mut XmlNode,
    insert: &DocOpInsertBlock,
) -> Result<DocumentOperation, DocumentEditError> {
    let fragment = crate::xml::parse_xml_string(insert.xml_fragment.as_str())
        .map_err(|e| DocumentEditError::Fragment(format!("{e:?}")))?;
    let mut children: Vec<XmlNodeChild> =
        core::mem::take(&mut host.children).into_library_owned_vec();
    let index = (insert.index as usize).min(children.len());
    for (offset, child) in fragment.into_iter().enumerate() {
        children.insert(index + offset, child);
    }
    host.children = children.into();
    Ok(DocumentOperation::DeleteBlock(
        crate::managers::changeset::DocOpDeleteBlock {
            block: insert.parent,
        },
    ))
}

/// Delete the block at `index`; the inverse re-inserts its serialization.
fn apply_delete(
    host: &mut XmlNode,
    index: usize,
    changeset: &DocumentChangeset,
) -> Result<DocumentOperation, DocumentEditError> {
    let mut children: Vec<XmlNodeChild> =
        core::mem::take(&mut host.children).into_library_owned_vec();
    if index >= children.len() {
        host.children = children.into();
        return Err(DocumentEditError::TargetNotFound);
    }
    let removed = children.remove(index);
    host.children = children.into();
    let fragment = serialize_xml_child(&removed);
    Ok(DocumentOperation::InsertBlock(DocOpInsertBlock {
        parent: changeset.target,
        index: index as u32,
        xml_fragment: fragment.into(),
    }))
}

/// Minimal XML serializer for the delete-inverse fragment (elements,
/// attributes, escaped text).
fn serialize_xml_child(child: &XmlNodeChild) -> String {
    fn escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
    fn node(n: &XmlNode, out: &mut String) {
        let tag = n.node_type.as_str();
        out.push('<');
        out.push_str(tag);
        for attr in n.attributes.as_ref() {
            out.push(' ');
            out.push_str(attr.key.as_str());
            out.push_str("=\"");
            out.push_str(&escape(attr.value.as_str()));
            out.push('"');
        }
        out.push('>');
        for c in n.children.as_ref() {
            match c {
                XmlNodeChild::Text(t) => out.push_str(&escape(t.as_str())),
                XmlNodeChild::Element(e) => node(e, out),
            }
        }
        out.push_str("</");
        out.push_str(tag);
        out.push('>');
    }
    let mut out = String::new();
    match child {
        XmlNodeChild::Text(t) => out.push_str(&escape(t.as_str())),
        XmlNodeChild::Element(e) => node(e, &mut out),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul_core::dom::{DomId, DomNodeId};
    use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
    use azul_core::styled_dom::NodeHierarchyItemId;
    use azul_core::task::{Instant, SystemTick};

    fn any_node() -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: 0 },
            node: NodeHierarchyItemId::from_crate_internal(None),
        }
    }

    fn cursor(byte: u32) -> TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: byte,
            },
            affinity: CursorAffinity::Leading,
        }
    }

    fn resume(block_index: u32, offset: u32) -> EditResumePoint {
        EditResumePoint {
            contenteditable_key: 1,
            block_path: vec![block_index].into(),
            text_offset: offset,
        }
    }

    fn changeset(op: DocumentOperation, r: EditResumePoint) -> DocumentChangeset {
        DocumentChangeset::new(
            any_node(),
            op,
            r,
            Instant::Tick(SystemTick::new(0)),
        )
    }

    fn host_tree(blocks: &[(&str, &str)]) -> Vec<XmlNodeChild> {
        let children: Vec<XmlNodeChild> = blocks
            .iter()
            .map(|(tag, text)| {
                XmlNodeChild::Element(XmlNode {
                    node_type: (*tag).into(),
                    attributes: Default::default(),
                    children: vec![XmlNodeChild::Text((*text).into())].into(),
                })
            })
            .collect();
        vec![XmlNodeChild::Element(XmlNode {
            node_type: "div".into(),
            attributes: Default::default(),
            children: children.into(),
        })]
    }

    fn block_texts(tree: &[XmlNodeChild]) -> Vec<(String, String)> {
        let XmlNodeChild::Element(host) = &tree[0] else {
            panic!("host")
        };
        host.children
            .as_ref()
            .iter()
            .map(|c| match c {
                XmlNodeChild::Element(e) => {
                    (e.node_type.as_str().to_string(), e.get_text_content())
                }
                XmlNodeChild::Text(t) => ("#text".to_string(), t.as_str().to_string()),
            })
            .collect()
    }

    #[test]
    fn split_mid_at_start_and_at_end() {
        for (at, expect_first, expect_second) in
            [(5, "hello", " world"), (0, "", "hello world"), (11, "hello world", "")]
        {
            let mut tree = host_tree(&[("p", "hello world")]);
            let cs = changeset(
                DocumentOperation::SplitBlock(DocOpSplitBlock {
                    block: any_node(),
                    at: cursor(at),
                }),
                resume(1, 0), // post-edit path: the NEW second block at index 1
            );
            let applied = apply_document_operation(&mut tree, &[], &cs).expect("split");
            assert_eq!(
                block_texts(&tree),
                vec![
                    ("p".to_string(), expect_first.to_string()),
                    ("p".to_string(), expect_second.to_string())
                ],
                "split at {at}"
            );
            assert!(matches!(applied.inverse, DocumentOperation::MergeBlocks(_)));
        }
    }

    #[test]
    fn split_h1_second_half_becomes_p() {
        let mut tree = host_tree(&[("h1", "Title text")]);
        let cs = changeset(
            DocumentOperation::SplitBlock(DocOpSplitBlock {
                block: any_node(),
                at: cursor(5),
            }),
            resume(1, 0),
        );
        apply_document_operation(&mut tree, &[], &cs).expect("split");
        assert_eq!(
            block_texts(&tree),
            vec![
                ("h1".to_string(), "Title".to_string()),
                ("p".to_string(), " text".to_string())
            ],
            "splitting a heading yields a paragraph second half (browser convention)"
        );
    }

    #[test]
    fn split_never_cuts_inside_a_multibyte_char() {
        let mut tree = host_tree(&[("p", "aä!")]); // ä = 2 bytes at offset 1..3
        let cs = changeset(
            DocumentOperation::SplitBlock(DocOpSplitBlock {
                block: any_node(),
                at: cursor(2), // INSIDE ä
            }),
            resume(1, 0),
        );
        apply_document_operation(&mut tree, &[], &cs).expect("split");
        let texts = block_texts(&tree);
        assert_eq!(texts[0].1, "a", "clamped to the previous char boundary");
        assert_eq!(texts[1].1, "ä!");
    }

    #[test]
    fn merge_preserves_and_coalesces_text() {
        let mut tree = host_tree(&[("p", "hello"), ("p", " world")]);
        let cs = changeset(
            DocumentOperation::MergeBlocks(DocOpMergeBlocks {
                first: any_node(),
                second: any_node(),
                join_cursor: cursor(0),
            }),
            resume(0, 5), // post-edit: caret at byte 5 of surviving block 0
        );
        let applied = apply_document_operation(&mut tree, &[], &cs).expect("merge");
        assert_eq!(
            block_texts(&tree),
            vec![("p".to_string(), "hello world".to_string())]
        );
        // Inverse is the split at the join point.
        match applied.inverse {
            DocumentOperation::SplitBlock(s) => {
                assert_eq!(s.at.cluster_id.start_byte_in_run, 5);
            }
            other => panic!("inverse must be a split, got {other:?}"),
        }
    }

    #[test]
    fn split_then_inverse_merge_is_identity() {
        let original = host_tree(&[("p", "hello world"), ("p", "tail")]);
        let mut tree = original.clone();

        let split_cs = changeset(
            DocumentOperation::SplitBlock(DocOpSplitBlock {
                block: any_node(),
                at: cursor(5),
            }),
            resume(1, 0),
        );
        let applied = apply_document_operation(&mut tree, &[], &split_cs).expect("split");

        // Undo: re-record the inverse THROUGH the same apply loop.
        let merge_cs = changeset(applied.inverse, resume(0, 5));
        apply_document_operation(&mut tree, &[], &merge_cs).expect("inverse merge");

        assert_eq!(
            block_texts(&tree),
            block_texts(&original),
            "inverse-of-apply restores the tree"
        );
    }

    #[test]
    fn insert_and_delete_round_trip() {
        let mut tree = host_tree(&[("p", "one"), ("p", "three")]);
        let cs = changeset(
            DocumentOperation::InsertBlock(DocOpInsertBlock {
                parent: any_node(),
                index: 1,
                xml_fragment: "<p>two</p>".into(),
            }),
            resume(1, 0),
        );
        let applied = apply_document_operation(&mut tree, &[], &cs).expect("insert");
        assert_eq!(block_texts(&tree).len(), 3);
        assert_eq!(block_texts(&tree)[1].1, "two");
        assert!(matches!(applied.inverse, DocumentOperation::DeleteBlock(_)));

        // Delete it again via the inverse; ITS inverse re-inserts the fragment.
        let del_cs = changeset(applied.inverse, resume(1, 0));
        let deleted = apply_document_operation(&mut tree, &[], &del_cs).expect("delete");
        assert_eq!(block_texts(&tree).len(), 2);
        match deleted.inverse {
            DocumentOperation::InsertBlock(i) => {
                assert_eq!(i.xml_fragment.as_str(), "<p>two</p>");
                assert_eq!(i.index, 1);
            }
            other => panic!("delete inverse must re-insert, got {other:?}"),
        }
    }

    #[test]
    fn failures_leave_the_tree_unchanged() {
        let original = host_tree(&[("p", "only")]);
        let mut tree = original.clone();

        // Merge with no second block: TargetNotFound.
        let cs = changeset(
            DocumentOperation::MergeBlocks(DocOpMergeBlocks {
                first: any_node(),
                second: any_node(),
                join_cursor: cursor(0),
            }),
            resume(0, 4),
        );
        assert_eq!(
            apply_document_operation(&mut tree, &[], &cs).unwrap_err(),
            DocumentEditError::TargetNotFound
        );
        assert_eq!(block_texts(&tree), block_texts(&original));

        // Bad host path.
        let cs2 = changeset(
            DocumentOperation::DeleteBlock(crate::managers::changeset::DocOpDeleteBlock {
                block: any_node(),
            }),
            resume(0, 0),
        );
        assert_eq!(
            apply_document_operation(&mut tree, &[7, 7], &cs2).unwrap_err(),
            DocumentEditError::HostNotFound
        );
    }
}
