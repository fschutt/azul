//! How a consumer of a page's [`Op`](crate::Op)s walks into the forms the page draws.
//!
//! Text extraction, text boxes and the SVG preview read the ops of a page from first to last. An
//! `Op::UseXobject` of a form sends them into the ops of that form, in the scope of the form's own
//! resources, and back. A document can be built so that this walk never ends or cannot be
//! finished: a form can draw itself (through a form without resources of its own, which names the
//! document's resources), and ten forms that each draw the next one ten times are ten billion
//! visits. [`FormWalk`] is what each consumer keeps for one page to stop that: forms nested deeper
//! than [`MAX_FORM_DEPTH`], a form that is being drawn already, more than [`MAX_FORM_VISITS`]
//! forms or [`MAX_FORM_OPS`] ops inside forms on one page are skipped.

use crate::{xobject::FormXObject, Op, PdfResources, XObject, XObjectId};

/// The deepest nesting of forms that is read from a PDF, written and walked: a form drawn by a
/// form drawn by a form and so on.
pub(crate) const MAX_FORM_DEPTH: usize = 16;

/// Most forms a consumer walks into for one page.
pub(crate) const MAX_FORM_VISITS: usize = 10_000;

/// Most ops a consumer walks through inside forms for one page.
pub(crate) const MAX_FORM_OPS: usize = 2_000_000;

/// Why a form is not walked into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Skip {
    /// The form is being drawn already: it draws itself.
    Cycle,
    /// More than [`MAX_FORM_DEPTH`] forms are nested.
    Depth,
    /// The page has drawn [`MAX_FORM_VISITS`] forms already.
    Visits,
    /// The forms of the page have [`MAX_FORM_OPS`] ops already.
    Ops,
}

impl Skip {
    /// A sentence fragment that says why, for a warning.
    pub(crate) fn describe(self) -> String {
        match self {
            Skip::Cycle => "it draws itself".to_string(),
            Skip::Depth => format!("forms are nested deeper than {MAX_FORM_DEPTH}"),
            Skip::Visits => format!("the page draws more than {MAX_FORM_VISITS} forms"),
            Skip::Ops => format!("the forms of the page have more than {MAX_FORM_OPS} ops"),
        }
    }

    fn index(self) -> usize {
        match self {
            Skip::Cycle => 0,
            Skip::Depth => 1,
            Skip::Visits => 2,
            Skip::Ops => 3,
        }
    }
}

/// The forms one consumer is walking through for one page, and what it has spent.
#[derive(Debug, Default)]
pub(crate) struct FormWalk {
    /// Addresses of the forms being drawn, the outermost first. A form is identified by where it
    /// is in the document: the same form drawn in a form it is part of is a cycle.
    stack: Vec<usize>,
    visits: usize,
    ops: usize,
    reported: [bool; 4],
}

impl FormWalk {
    pub(crate) fn new() -> Self {
        FormWalk::default()
    }

    /// Starts drawing `form`. `Err` says why the form is skipped; nothing is changed then, and
    /// [`FormWalk::leave`] must not be called.
    pub(crate) fn enter(&mut self, form: &FormXObject) -> Result<(), Skip> {
        let address = form as *const FormXObject as usize;
        if self.stack.contains(&address) {
            return Err(Skip::Cycle);
        }
        if self.stack.len() >= MAX_FORM_DEPTH {
            return Err(Skip::Depth);
        }
        if self.visits >= MAX_FORM_VISITS {
            return Err(Skip::Visits);
        }
        let ops = self.ops.saturating_add(form.ops.len());
        if ops > MAX_FORM_OPS {
            return Err(Skip::Ops);
        }
        self.visits = self.visits.saturating_add(1);
        self.ops = ops;
        self.stack.push(address);
        Ok(())
    }

    /// Ends the form that [`FormWalk::enter`] started.
    pub(crate) fn leave(&mut self) {
        self.stack.pop();
    }

    /// `true` the first time `skip` is reported, so that a consumer with a list of warnings
    /// writes one message per reason and not one per skipped form.
    pub(crate) fn first_report(&mut self, skip: Skip) -> bool {
        match self.reported.get_mut(skip.index()) {
            Some(seen) if !*seen => {
                *seen = true;
                true
            }
            _ => false,
        }
    }
}

/// The form that `Op::UseXobject` of `id` draws in `scope`, if it names one.
pub(crate) fn form_in<'a>(scope: &'a PdfResources, id: &XObjectId) -> Option<&'a FormXObject> {
    match scope.xobjects.map.get(id) {
        Some(XObject::Form(form)) => Some(form),
        _ => None,
    }
}

/// The scope the ops of `form` name their resources in when the form is drawn in `outer`: its own
/// resources, or without any those of the content that draws it.
pub(crate) fn scope_of<'a>(form: &'a FormXObject, outer: &'a PdfResources) -> &'a PdfResources {
    form.resources.as_ref().unwrap_or(outer)
}

/// The ops of the forms in `scope` that have no resources of their own. They name the fonts of
/// the scope, so the glyphs they use count towards its subsets.
pub(crate) fn inheriting_streams(scope: &PdfResources) -> Vec<&[Op]> {
    scope
        .xobjects
        .map
        .values()
        .filter_map(|xobject| match xobject {
            XObject::Form(form) if form.resources.is_none() => Some(form.ops.as_slice()),
            _ => None,
        })
        .collect()
}
