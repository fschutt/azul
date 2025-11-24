        // Check if this child has border/padding that prevents margin collapsing
        let child_has_top_blocker = has_margin_collapse_blocker(&child_node.box_props, writing_mode, true);
        let child_has_bottom_blocker = has_margin_collapse_blocker(&child_node.box_props, writing_mode, false);
        
        // Debug margin collapse checking removed for performance
