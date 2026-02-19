// Incremental layout reconciliation test suite
//
// Tests are organized by category, one per file:
//
// - node_change_set:     NodeChangeSet bitflag tests (field comparisons)
// - change_accumulator:  ChangeAccumulator merge + classification tests
// - fingerprint:         NodeDataFingerprint two-tier change detection tests
// - css_scope:           CSS property → RelayoutScope classification tests
// - state_preservation:  Cursor, focus, scroll state across DOM rebuilds
// - text_reconciliation: Text content change detection + cursor adjustment
// - dom_reconciliation:  Full DOM reconciliation with change tracking

mod node_change_set;
mod change_accumulator;
mod fingerprint;
mod css_scope;
mod state_preservation;
mod text_reconciliation;
mod dom_reconciliation;
