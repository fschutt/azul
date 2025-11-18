        // Check if this child has border/padding that prevents margin collapsing
        let child_has_top_blocker = has_margin_collapse_blocker(&child_node.box_props, writing_mode, true);
        let child_has_bottom_blocker = has_margin_collapse_blocker(&child_node.box_props, writing_mode, false);
        
        if std::env::var("DEBUG_MARGIN_COLLAPSE").is_ok() {
            println!("[MARGIN_COLLAPSE] Child {:?} blockers: top={}, bottom={}", 
                     child_dom_id, child_has_top_blocker, child_has_bottom_blocker);
            if child_has_top_blocker {
                println!("[MARGIN_COLLAPSE]   Top blocker details: border={:?}, padding={:?}", 
                         child_node.box_props.border.main_start(writing_mode),
                         child_node.box_props.padding.main_start(writing_mode));
            }
        }
