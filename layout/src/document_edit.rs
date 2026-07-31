//! Apply a [`DocumentOperation`] to a [`Dom`] tree — the helper for apps
//! WITHOUT their own document model (Path 2).
//!
//! The applier operates on azul's NATIVE node tree (`azul_core::dom::Dom` —
//! what a layout callback returns, what `reconstruct_dom_subtree` hands
//! back), not on markup. Operations are STRUCTURAL: subtrees move wholesale
//! (`<b>…</b>` inside a split paragraph survives intact, a `<ul>` splits
//! between `<li>`s, a table row inserts like any other subtree); the ONLY
//! thing ever cut is a text child, at a char boundary, when a
//! [`NodePosition`] points inside it.
//!
//! Azul records structural intent (`DocumentChangeset`); the app applies it
//! to ITS model and regenerates the DOM — the `StyledDom` is never mutated.
//! An app holding a `Dom` calls [`apply_document_operation`], then
//! `CallbackInfo::mark_document_edit_applied_with_inverse(changeset.id,
//! applied.inverse)` (the commit handshake), then returns
//! `Update::RefreshDom`.
//!
//! Every successful apply returns the INVERSE operation — tree-shaped undo
//! for free (undoing re-RECORDS the inverse through the same
//! record→apply→ack loop; it never mutates either).
//!
//! **Fragment semantics**: `content: Dom` payloads are DocumentFragment-like
//! — the fragment's ROOT is ignored, its CHILDREN are the inserted nodes.
//! This closes the inverse algebra for multi-child operations
//! (`RemoveChildren [s, e)` ⇄ `InsertChildren` of the removed fragment).

use azul_core::dom::{Dom, NodeType};

use crate::managers::changeset::{
    DocOpInsertChildren, DocOpMergeNodes, DocOpRemoveChildren, DocOpReplaceChildren,
    DocOpSplitNode, DocumentChangeset, DocumentOperation, NodePosition,
};

/// The outcome of a successful apply.
#[derive(Debug, Clone)]
pub struct AppliedEdit {
    /// Where the caret/anchor should land (passed through from the changeset
    /// — already expressed re-render-stably).
    pub resume: crate::managers::changeset::EditResumePoint,
    /// The operation that undoes this one. `DomNodeId` fields are advisory
    /// (they refer to the generation the ORIGINAL changeset was recorded
    /// against); the structural payload (positions, ranges, fragments) is
    /// what the undo path re-records.
    pub inverse: DocumentOperation,
}

/// Why an apply failed. Failures leave the tree UNCHANGED.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentEditError {
    /// `host_path` did not resolve to a node in the tree.
    HostNotFound,
    /// An index/range in the operation does not exist under the host.
    TargetNotFound,
    /// The operation kind is recorded-only for now (Wrap/UnwrapRange — the
    /// text-specific pair).
    Unsupported(&'static str),
}

/// Wrap subtrees in a fragment `Dom` (root ignored by the applier).
#[must_use]
pub fn fragment(children: Vec<Dom>) -> Dom {
    let mut f = Dom::create_div();
    for c in children {
        f.add_child(c);
    }
    f
}

/// Apply a structural changeset to the `Dom` the app holds.
///
/// * `root` — the app's document tree (e.g. from its own builder or
///   `reconstruct_dom_subtree`).
/// * `host_path` — child-index path from `root` to the node whose CHILD LIST
///   the operation edits (`[]` = `root` itself). For Split/Merge this is the
///   PARENT of the split/merged nodes.
/// * `changeset` — as delivered by `CallbackInfo::get_document_edit_clone`.
///
/// Index resolution for Split/Merge uses the changeset's OWN resume point
/// (recorded by the same engine that computes it, so the two cannot drift):
/// a split targets `resume.node_path.last() - 1` (the resume names the NEW
/// second node), a merge keeps `resume.node_path.last()`.
///
/// # Errors
///
/// Returns a [`DocumentEditError`] (tree unchanged) on unresolvable paths,
/// missing targets, or record-only operation kinds.
pub fn apply_document_operation(
    root: &mut Dom,
    host_path: &[u32],
    changeset: &DocumentChangeset,
) -> Result<AppliedEdit, DocumentEditError> {
    let host = resolve_path_mut(root, host_path).ok_or(DocumentEditError::HostNotFound)?;
    let resume_index = changeset
        .resume
        .node_path
        .as_ref()
        .last()
        .copied()
        .unwrap_or(0);

    let inverse = match &changeset.operation {
        DocumentOperation::SplitNode(split) => {
            let node_index = resume_index.saturating_sub(1) as usize;
            apply_split(host, node_index, split)?
        }
        DocumentOperation::MergeNodes(merge) => {
            let first_index = resume_index as usize;
            apply_merge(host, first_index, merge)?
        }
        DocumentOperation::InsertChildren(insert) => apply_insert(host, insert)?,
        DocumentOperation::RemoveChildren(remove) => apply_remove(host, remove)?,
        DocumentOperation::ReplaceChildren(replace) => apply_replace(host, replace)?,
        DocumentOperation::WrapRange(_) => {
            return Err(DocumentEditError::Unsupported("WrapRange"));
        }
        DocumentOperation::UnwrapRange(_) => {
            return Err(DocumentEditError::Unsupported("UnwrapRange"));
        }
    };

    // Direct `children` mutation desyncs `estimated_total_children` (the
    // CompactDom conversion asserts on it); re-sync the WHOLE tree — counts
    // bubble up through every ancestor of the edited node.
    root.fixup_children_estimated();

    Ok(AppliedEdit {
        resume: changeset.resume.clone(),
        inverse,
    })
}

/// Walk a child-index path down the tree.
fn resolve_path_mut<'a>(root: &'a mut Dom, path: &[u32]) -> Option<&'a mut Dom> {
    let mut node = root;
    for &idx in path {
        node = node.children.as_mut().get_mut(idx as usize)?;
    }
    Some(node)
}

/// Take a node's children out as a plain Vec (write back with `.into()`).
fn take_children(node: &mut Dom) -> Vec<Dom> {
    core::mem::take(&mut node.children).into_library_owned_vec()
}

/// Split the text of a text-node `Dom` at `byte` (char-boundary clamped),
/// truncating the node to the head and returning the tail as a new node.
fn split_text_dom(node: &mut Dom, byte: usize) -> Dom {
    let (head, tail) = match node.root.get_node_type() {
        NodeType::Text(t) => {
            let s = t.as_str();
            let cut = byte.min(s.len());
            let cut = (0..=cut)
                .rev()
                .find(|&c| s.is_char_boundary(c))
                .unwrap_or(0);
            (s[..cut].to_string(), s[cut..].to_string())
        }
        _ => return Dom::create_text(""),
    };
    *node = Dom::create_text(head);
    Dom::create_text(tail)
}

/// Split `host.children[node_index]` at the structural position: children
/// BEFORE the position stay, children AFTER move to a new sibling of the
/// SAME node shape (the `NodeData` is cloned — a `<ul>` splits into two
/// `<ul>`s, an `<h1>` into two `<h1>`s; tag conversion is an editing policy
/// for the RECORDER, not the tree algebra). A text child AT the position is
/// cut at its byte. Inverse: the merge at the same seam.
fn apply_split(
    host: &mut Dom,
    node_index: usize,
    split: &DocOpSplitNode,
) -> Result<DocumentOperation, DocumentEditError> {
    let mut host_children = take_children(host);
    if node_index >= host_children.len() {
        host.children = host_children.into();
        return Err(DocumentEditError::TargetNotFound);
    }

    let node = &mut host_children[node_index];
    let mut node_children = take_children(node);
    let child_index = (split.at.child_index as usize).min(node_children.len());

    let mut second_children: Vec<Dom>;
    match split.at.text_byte.into_option() {
        Some(byte)
            if child_index < node_children.len()
                && matches!(
                    node_children[child_index].root.get_node_type(),
                    NodeType::Text(_)
                ) =>
        {
            // Cut the boundary TEXT child; everything after it moves.
            let tail_text = split_text_dom(&mut node_children[child_index], byte as usize);
            second_children = vec![tail_text];
            second_children.extend(node_children.drain(child_index + 1..));
        }
        _ => {
            // Pure structural boundary: children[child_index..] move wholesale.
            second_children = node_children.drain(child_index..).collect();
        }
    }
    node.children = node_children.into();

    // The second node clones the first's SHAPE (same NodeData: type, classes,
    // attributes) and takes the moved children.
    let mut second = Dom {
        root: node.root.clone(),
        children: Vec::<Dom>::new().into(),
        css: Vec::new().into(),
        estimated_total_children: 0,
    };
    for c in second_children {
        second.add_child(c);
    }
    host_children.insert(node_index + 1, second);
    host.children = host_children.into();

    Ok(DocumentOperation::MergeNodes(DocOpMergeNodes {
        first: split.node,
        second: split.node,
        join: split.at,
    }))
}

/// Merge `host.children[first_index + 1]` into `host.children[first_index]`:
/// the second node's children are appended WHOLESALE; two text nodes meeting
/// at the seam coalesce iff the join position carries a byte (the recorder
/// says the seam is text|text). Inverse: the split at the seam.
fn apply_merge(
    host: &mut Dom,
    first_index: usize,
    merge: &DocOpMergeNodes,
) -> Result<DocumentOperation, DocumentEditError> {
    let mut host_children = take_children(host);
    if first_index + 1 >= host_children.len() {
        host.children = host_children.into();
        return Err(DocumentEditError::TargetNotFound);
    }

    let second = host_children.remove(first_index + 1);
    let first = &mut host_children[first_index];
    let mut first_children = take_children(first);
    let second_children = second.children.into_library_owned_vec();

    let mut iter = second_children.into_iter();
    if merge.join.text_byte.into_option().is_some() {
        // The recorder marked the seam text|text: coalesce the two nodes so
        // a later split at the join byte round-trips.
        if let Some(second_first) = iter.next() {
            let coalesced = match (
                first_children.last().map(|n| n.root.get_node_type()),
                second_first.root.get_node_type(),
            ) {
                (Some(NodeType::Text(a)), NodeType::Text(b)) => {
                    Some(format!("{}{}", a.as_str(), b.as_str()))
                }
                _ => None,
            };
            match coalesced {
                Some(joined) => {
                    *first_children.last_mut().unwrap() = Dom::create_text(joined);
                }
                None => first_children.push(second_first),
            }
        }
    }
    first_children.extend(iter);
    first.children = first_children.into();
    host.children = host_children.into();

    Ok(DocumentOperation::SplitNode(DocOpSplitNode {
        node: merge.first,
        at: merge.join,
    }))
}

/// Insert the fragment's children under `host` at `insert.index`.
/// Inverse: remove of exactly that range.
fn apply_insert(
    host: &mut Dom,
    insert: &DocOpInsertChildren,
) -> Result<DocumentOperation, DocumentEditError> {
    let mut host_children = take_children(host);
    let index = (insert.index as usize).min(host_children.len());
    let new_children = insert.content.children.as_ref().to_vec();
    let count = new_children.len();
    for (offset, child) in new_children.into_iter().enumerate() {
        host_children.insert(index + offset, child);
    }
    host.children = host_children.into();
    Ok(DocumentOperation::RemoveChildren(DocOpRemoveChildren {
        parent: insert.parent,
        start: index as u32,
        end: (index + count) as u32,
    }))
}

/// Remove `host.children[start..end)`. Inverse: insert of the removed
/// fragment at `start`.
fn apply_remove(
    host: &mut Dom,
    remove: &DocOpRemoveChildren,
) -> Result<DocumentOperation, DocumentEditError> {
    let mut host_children = take_children(host);
    let start = remove.start as usize;
    let end = remove.end as usize;
    if start > end || end > host_children.len() {
        host.children = host_children.into();
        return Err(DocumentEditError::TargetNotFound);
    }
    let removed: Vec<Dom> = host_children.drain(start..end).collect();
    host.children = host_children.into();
    Ok(DocumentOperation::InsertChildren(DocOpInsertChildren {
        parent: remove.parent,
        index: remove.start,
        content: fragment(removed),
    }))
}

/// Replace `host.children[start..end)` with the fragment's children.
/// Inverse: the replace that puts the old range back.
fn apply_replace(
    host: &mut Dom,
    replace: &DocOpReplaceChildren,
) -> Result<DocumentOperation, DocumentEditError> {
    let mut host_children = take_children(host);
    let start = replace.start as usize;
    let end = replace.end as usize;
    if start > end || end > host_children.len() {
        host.children = host_children.into();
        return Err(DocumentEditError::TargetNotFound);
    }
    let new_children = replace.content.children.as_ref().to_vec();
    let count = new_children.len();
    let removed: Vec<Dom> = host_children.splice(start..end, new_children).collect();
    host.children = host_children.into();
    Ok(DocumentOperation::ReplaceChildren(DocOpReplaceChildren {
        parent: replace.parent,
        start: replace.start,
        end: (start + count) as u32,
        content: fragment(removed),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managers::changeset::EditResumePoint;
    use azul_core::dom::{DomId, DomNodeId};
    use azul_core::styled_dom::NodeHierarchyItemId;
    use azul_core::task::{Instant, SystemTick};

    fn any_node() -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: 0 },
            node: NodeHierarchyItemId::from_crate_internal(None),
        }
    }

    fn resume(node_index: u32, position: NodePosition) -> EditResumePoint {
        EditResumePoint {
            anchor_key: 1,
            node_path: vec![node_index].into(),
            position,
        }
    }

    fn changeset(op: DocumentOperation, r: EditResumePoint) -> DocumentChangeset {
        DocumentChangeset::new(any_node(), op, r, Instant::Tick(SystemTick::new(0)))
    }

    fn p(text: &str) -> Dom {
        let mut p = Dom::create_p();
        p.add_child(Dom::create_text(text));
        p
    }

    fn el(tag: &str) -> Dom {
        Dom::create_node(azul_core::xml::tag_to_node_type(tag))
    }

    fn li(text: &str) -> Dom {
        let mut li = el("li");
        li.add_child(Dom::create_text(text));
        li
    }

    fn collect_text(node: &Dom, out: &mut String) {
        if let NodeType::Text(t) = node.root.get_node_type() {
            out.push_str(t.as_str());
        }
        for c in node.children.as_ref() {
            collect_text(c, out);
        }
    }

    /// Flattened text of each direct child of the host (assertion helper —
    /// the OPERATIONS never flatten anything).
    fn texts(host: &Dom) -> Vec<String> {
        host.children
            .as_ref()
            .iter()
            .map(|c| {
                let mut t = String::new();
                collect_text(c, &mut t);
                t
            })
            .collect()
    }

    #[test]
    fn split_p_mid_text_at_start_and_at_end() {
        for (byte, first, second) in [
            (5, "hello", " world"),
            (0, "", "hello world"),
            (11, "hello world", ""),
        ] {
            let mut host = Dom::create_div();
            host.add_child(p("hello world"));
            let cs = changeset(
                DocumentOperation::SplitNode(DocOpSplitNode {
                    node: any_node(),
                    at: NodePosition::in_text_child(0, byte),
                }),
                resume(1, NodePosition::before_child(0)),
            );
            let applied = apply_document_operation(&mut host, &[], &cs).expect("split");
            assert_eq!(
                texts(&host),
                vec![first.to_string(), second.to_string()],
                "byte {byte}"
            );
            assert!(matches!(applied.inverse, DocumentOperation::MergeNodes(_)));
        }
    }

    #[test]
    fn split_preserves_nested_element_subtrees_wholesale() {
        // <p>["ab", <b>bold</b>, "cd"]</p> split at the BOUNDARY before <b>:
        // the <b> subtree must move to the second half INTACT — nothing is
        // flattened, re-parsed, or byte-walked.
        let mut host = Dom::create_div();
        let mut para = Dom::create_p();
        para.add_child(Dom::create_text("ab"));
        let mut b = el("b");
        b.add_child(Dom::create_text("bold"));
        para.add_child(b);
        para.add_child(Dom::create_text("cd"));
        host.add_child(para);

        let cs = changeset(
            DocumentOperation::SplitNode(DocOpSplitNode {
                node: any_node(),
                at: NodePosition::before_child(1), // between "ab" and <b>
            }),
            resume(1, NodePosition::before_child(0)),
        );
        apply_document_operation(&mut host, &[], &cs).expect("split");

        assert_eq!(texts(&host), vec!["ab".to_string(), "boldcd".to_string()]);
        // The second half's first child is the <b> ELEMENT with its own text
        // child — subtree preserved.
        let second = &host.children.as_ref()[1];
        let b2 = &second.children.as_ref()[0];
        assert_eq!(b2.children.as_ref().len(), 1);
        let mut t = String::new();
        collect_text(b2, &mut t);
        assert_eq!(t, "bold");
    }

    #[test]
    fn split_ul_between_list_items_is_pure_structure() {
        // A <ul> with 3 <li> splits between items 1 and 2 — no text involved,
        // both halves keep the SAME node shape (ul → ul, never a tag swap).
        let mut host = Dom::create_div();
        let mut ul = el("ul");
        ul.add_child(li("one"));
        ul.add_child(li("two"));
        ul.add_child(li("three"));
        host.add_child(ul);

        let cs = changeset(
            DocumentOperation::SplitNode(DocOpSplitNode {
                node: any_node(),
                at: NodePosition::before_child(1),
            }),
            resume(1, NodePosition::before_child(0)),
        );
        apply_document_operation(&mut host, &[], &cs).expect("split ul");

        let kids = host.children.as_ref();
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].children.as_ref().len(), 1, "first ul keeps [one]");
        assert_eq!(
            kids[1].children.as_ref().len(),
            2,
            "second ul takes [two, three]"
        );
        assert_eq!(
            core::mem::discriminant(kids[0].root.get_node_type()),
            core::mem::discriminant(kids[1].root.get_node_type()),
            "the second node clones the first's shape"
        );
    }

    #[test]
    fn split_never_cuts_inside_a_multibyte_char() {
        let mut host = Dom::create_div();
        host.add_child(p("aä!")); // ä = bytes 1..3
        let cs = changeset(
            DocumentOperation::SplitNode(DocOpSplitNode {
                node: any_node(),
                at: NodePosition::in_text_child(0, 2), // INSIDE ä
            }),
            resume(1, NodePosition::before_child(0)),
        );
        apply_document_operation(&mut host, &[], &cs).expect("split");
        assert_eq!(texts(&host), vec!["a".to_string(), "ä!".to_string()]);
    }

    #[test]
    fn merge_appends_wholesale_and_coalesces_text_only_at_a_text_seam() {
        let mut host = Dom::create_div();
        host.add_child(p("hello"));
        host.add_child(p(" world"));
        let cs = changeset(
            DocumentOperation::MergeNodes(DocOpMergeNodes {
                first: any_node(),
                second: any_node(),
                join: NodePosition::in_text_child(0, 5),
            }),
            resume(0, NodePosition::in_text_child(0, 5)),
        );
        let applied = apply_document_operation(&mut host, &[], &cs).expect("merge");
        assert_eq!(texts(&host), vec!["hello world".to_string()]);
        assert_eq!(
            host.children.as_ref()[0].children.as_ref().len(),
            1,
            "text seam coalesced into ONE text child"
        );
        match applied.inverse {
            DocumentOperation::SplitNode(s) => {
                assert_eq!(s.at, NodePosition::in_text_child(0, 5));
            }
            other => panic!("inverse must be the split at the seam, got {other:?}"),
        }

        // A pure structural merge (ul + ul) coalesces NOTHING.
        let mut host = Dom::create_div();
        let mut ul1 = el("ul");
        ul1.add_child(li("one"));
        let mut ul2 = el("ul");
        ul2.add_child(li("two"));
        host.add_child(ul1);
        host.add_child(ul2);
        let cs = changeset(
            DocumentOperation::MergeNodes(DocOpMergeNodes {
                first: any_node(),
                second: any_node(),
                join: NodePosition::before_child(1),
            }),
            resume(0, NodePosition::before_child(1)),
        );
        apply_document_operation(&mut host, &[], &cs).expect("merge uls");
        assert_eq!(host.children.as_ref().len(), 1);
        assert_eq!(host.children.as_ref()[0].children.as_ref().len(), 2);
    }

    #[test]
    fn split_then_inverse_merge_is_identity() {
        let mut original = Dom::create_div();
        original.add_child(p("hello world"));
        original.add_child(p("tail"));
        let mut host = original.clone();

        let split_cs = changeset(
            DocumentOperation::SplitNode(DocOpSplitNode {
                node: any_node(),
                at: NodePosition::in_text_child(0, 5),
            }),
            resume(1, NodePosition::before_child(0)),
        );
        let applied = apply_document_operation(&mut host, &[], &split_cs).expect("split");

        let merge_cs = changeset(
            applied.inverse,
            resume(0, NodePosition::in_text_child(0, 5)),
        );
        apply_document_operation(&mut host, &[], &merge_cs).expect("inverse merge");

        assert_eq!(
            texts(&host),
            texts(&original),
            "inverse-of-apply restores the tree"
        );
    }

    #[test]
    fn insert_remove_replace_close_their_inverse_algebra() {
        // insertChild: a fragment of TWO subtrees (a p and a whole ul) lands
        // at index 1; the inverse removes exactly that range.
        let mut host = Dom::create_div();
        host.add_child(p("one"));
        host.add_child(p("four"));

        let mut ul = el("ul");
        ul.add_child(li("x"));
        let cs = changeset(
            DocumentOperation::InsertChildren(DocOpInsertChildren {
                parent: any_node(),
                index: 1,
                content: fragment(vec![p("two"), ul]),
            }),
            resume(1, NodePosition::before_child(1)),
        );
        let inserted = apply_document_operation(&mut host, &[], &cs).expect("insert");
        assert_eq!(
            texts(&host),
            ["one", "two", "x", "four"].map(String::from).to_vec()
        );
        let DocumentOperation::RemoveChildren(ref rm) = inserted.inverse else {
            panic!("insert inverse must be a remove");
        };
        assert_eq!((rm.start, rm.end), (1, 3));

        // removeChild: applying the inverse removes both; ITS inverse
        // re-inserts the same fragment.
        let rm_cs = changeset(
            inserted.inverse.clone(),
            resume(1, NodePosition::before_child(1)),
        );
        let removed = apply_document_operation(&mut host, &[], &rm_cs).expect("remove");
        assert_eq!(texts(&host), vec!["one".to_string(), "four".to_string()]);
        let DocumentOperation::InsertChildren(ref ins) = removed.inverse else {
            panic!("remove inverse must be an insert");
        };
        assert_eq!(ins.index, 1);
        assert_eq!(
            ins.content.children.as_ref().len(),
            2,
            "removed fragment captured"
        );

        // replaceChild: swap [0..1) for two nodes; inverse restores.
        let mut host2 = Dom::create_div();
        host2.add_child(p("old"));
        let rep_cs = changeset(
            DocumentOperation::ReplaceChildren(DocOpReplaceChildren {
                parent: any_node(),
                start: 0,
                end: 1,
                content: fragment(vec![p("new1"), p("new2")]),
            }),
            resume(0, NodePosition::before_child(0)),
        );
        let replaced = apply_document_operation(&mut host2, &[], &rep_cs).expect("replace");
        assert_eq!(texts(&host2), vec!["new1".to_string(), "new2".to_string()]);
        let inv_cs = changeset(replaced.inverse, resume(0, NodePosition::before_child(0)));
        apply_document_operation(&mut host2, &[], &inv_cs).expect("inverse replace");
        assert_eq!(texts(&host2), vec!["old".to_string()]);
    }

    #[test]
    fn failures_leave_the_tree_unchanged() {
        let mut original = Dom::create_div();
        original.add_child(p("only"));
        let mut host = original.clone();

        let cs = changeset(
            DocumentOperation::MergeNodes(DocOpMergeNodes {
                first: any_node(),
                second: any_node(),
                join: NodePosition::before_child(1),
            }),
            resume(0, NodePosition::before_child(1)),
        );
        assert_eq!(
            apply_document_operation(&mut host, &[], &cs).unwrap_err(),
            DocumentEditError::TargetNotFound
        );
        assert_eq!(texts(&host), texts(&original));

        let cs2 = changeset(
            DocumentOperation::RemoveChildren(DocOpRemoveChildren {
                parent: any_node(),
                start: 0,
                end: 1,
            }),
            resume(0, NodePosition::before_child(0)),
        );
        assert_eq!(
            apply_document_operation(&mut host, &[7, 7], &cs2).unwrap_err(),
            DocumentEditError::HostNotFound
        );
    }
}
