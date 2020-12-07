 #![allow(unused_macros)]

/// Implements functions for `CallbackInfo` and `Info`,
/// to prevent duplicating the functions
#[macro_export]
macro_rules! impl_task_api {() => (
    /// Insert a timer into the list of active timers.
    /// Replaces the existing timer if called with the same TimerId.
    pub fn add_timer(&mut self, id: TimerId, timer: Timer) {
        self.timers.insert(id, timer);
    }

    /// Returns if a timer with the given ID is currently running
    pub fn has_timer(&self, timer_id: &TimerId) -> bool {
        self.get_timer(timer_id).is_some()
    }

    /// Returns a reference to an existing timer (if the `TimerId` is valid)
    pub fn get_timer(&self, timer_id: &TimerId) -> Option<&Timer> {
        self.timers.get(&timer_id)
    }

    /// Deletes a timer and returns it (if the `TimerId` is valid)
    pub fn delete_timer(&mut self, timer_id: &TimerId) -> Option<Timer> {
        self.timers.remove(timer_id)
    }

    /// Adds a (thread-safe) `Task` to the app that runs on a different thread
    pub fn add_task(&mut self, task: Task) {
        self.tasks.push(task);
    }
)}

/// Implement the `From` trait for any type.
/// Example usage:
/// ```
/// enum MyError<'a> {
///     Bar(BarError<'a>)
///     Foo(FooError<'a>)
/// }
///
/// impl_from!(BarError<'a>, Error::Bar);
/// impl_from!(BarError<'a>, Error::Bar);
///
/// ```
#[macro_export]
macro_rules! impl_from {
    // From a type with a lifetime to a type which also has a lifetime
    ($a:ident<$c:lifetime>, $b:ident::$enum_type:ident) => {
        impl<$c> From<$a<$c>> for $b<$c> {
            fn from(e: $a<$c>) -> Self {
                $b::$enum_type(e)
            }
        }
    };

    // From a type without a lifetime to a type which also does not have a lifetime
    ($a:ident, $b:ident::$enum_type:ident) => {
        impl From<$a> for $b {
            fn from(e: $a) -> Self {
                $b::$enum_type(e)
            }
        }
    };
}

/// Implement `Display` for an enum.
///
/// Example usage:
/// ```
/// enum Foo<'a> {
///     Bar(&'a str)
///     Baz(i32)
/// }
///
/// impl_display!{ Foo<'a>, {
///     Bar(s) => s,
///     Baz(i) => format!("{}", i)
/// }}
/// ```
#[macro_export]
macro_rules! impl_display {
    // For a type with a lifetime
    ($enum:ident<$lt:lifetime>, {$($variant:pat => $fmt_string:expr),+$(,)* }) => {

        impl<$lt> ::std::fmt::Display for $enum<$lt> {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                use self::$enum::*;
                match &self {
                    $(
                        $variant => write!(f, "{}", $fmt_string),
                    )+
                }
            }
        }

    };

    // For a type without a lifetime
    ($enum:ident, {$($variant:pat => $fmt_string:expr),+$(,)* }) => {

        impl ::std::fmt::Display for $enum {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                use self::$enum::*;
                match &self {
                    $(
                        $variant => write!(f, "{}", $fmt_string),
                    )+
                }
            }
        }

    };
}

#[macro_export]
macro_rules! impl_image_api {($struct_field:ident) => (

    /// See [`AppResources::get_loaded_font_ids`]
    ///
    /// [`AppResources::get_loaded_font_ids`]: ../app_resources/struct.AppResources.html#method.get_loaded_font_ids
    pub fn get_loaded_font_ids(&self) -> Vec<FontId> {
        self.$struct_field.get_loaded_font_ids()
    }

    /// See [`AppResources::get_loaded_image_ids`]
    ///
    /// [`AppResources::get_loaded_image_ids`]: ../app_resources/struct.AppResources.html#method.get_loaded_image_ids
    pub fn get_loaded_image_ids(&self) -> Vec<ImageId> {
        self.$struct_field.get_loaded_image_ids()
    }

    /// See [`AppResources::get_loaded_css_image_ids`]
    ///
    /// [`AppResources::get_loaded_css_image_ids`]: ../app_resources/struct.AppResources.html#method.get_loaded_css_image_ids
    pub fn get_loaded_css_image_ids(&self) -> Vec<CssImageId> {
        self.$struct_field.get_loaded_css_image_ids()
    }

    /// See [`AppResources::get_loaded_css_font_ids`]
    ///
    /// [`AppResources::get_loaded_css_font_ids`]: ../app_resources/struct.AppResources.html#method.get_loaded_css_font_ids
    pub fn get_loaded_css_font_ids(&self) -> Vec<CssImageId> {
        self.$struct_field.get_loaded_css_font_ids()
    }

    /// See [`AppResources::get_loaded_text_ids`]
    ///
    /// [`AppResources::get_loaded_text_ids`]: ../app_resources/struct.AppResources.html#method.get_loaded_text_ids
    pub fn get_loaded_text_ids(&self) -> Vec<TextId> {
        self.$struct_field.get_loaded_text_ids()
    }

    // -- ImageId cache

    /// See [`AppResources::add_image`]
    ///
    /// [`AppResources::add_image`]: ../app_resources/struct.AppResources.html#method.add_image
    pub fn add_image_source(&mut self, image_id: ImageId, image_source: ImageSource) {
        self.$struct_field.add_image_source(image_id, image_source)
    }

    /// See [`AppResources::has_image`]
    ///
    /// [`AppResources::has_image`]: ../app_resources/struct.AppResources.html#method.has_image
    pub fn has_image_source(&self, image_id: &ImageId) -> bool {
        self.$struct_field.has_image_source(image_id)
    }

    /// Given an `ImageId`, returns the bytes for that image or `None`, if the `ImageId` is invalid.
    ///
    /// See [`AppResources::get_image_bytes`]
    ///
    /// [`AppResources::get_image_bytes`]: ../app_resources/struct.AppResources.html#method.get_image_bytes
    pub fn get_image_info(&self, pipeline_id: &PipelineId, image_id: &ImageId) -> Option<&ImageInfo> {
        self.$struct_field.get_image_info(pipeline_id, image_id)
    }

    /// See [`AppResources::delete_image`]
    ///
    /// [`AppResources::delete_image`]: ../app_resources/struct.AppResources.html#method.delete_image
    pub fn delete_image_source(&mut self, image_id: &ImageId) {
        self.$struct_field.delete_image_source(image_id)
    }

    /// See [`AppResources::add_css_image_id`]
    ///
    /// [`AppResources::add_css_image_id`]: ../app_resources/struct.AppResources.html#method.add_css_image_id
    pub fn add_css_image_id<S: Into<String>>(&mut self, css_id: S) -> ImageId {
        self.$struct_field.add_css_image_id(css_id)
    }

    /// See [`AppResources::has_css_image_id`]
    ///
    /// [`AppResources::has_css_image_id`]: ../app_resources/struct.AppResources.html#method.has_css_image_id
    pub fn has_css_image_id(&self, css_id: &str) -> bool {
        self.$struct_field.has_css_image_id(css_id)
    }

    /// See [`AppResources::get_css_image_id`]
    ///
    /// [`AppResources::get_css_image_id`]: ../app_resources/struct.AppResources.html#method.get_css_image_id
    pub fn get_css_image_id(&self, css_id: &str) -> Option<&ImageId> {
        self.$struct_field.get_css_image_id(css_id)
    }

    /// See [`AppResources::delete_css_image_id`]
    ///
    /// [`AppResources::delete_css_image_id`]: ../app_resources/struct.AppResources.html#method.delete_css_image_id
    pub fn delete_css_image_id(&mut self, css_id: &str) -> Option<ImageId> {
        self.$struct_field.delete_css_image_id(css_id)
    }

    /// See [`AppResources::add_css_font_id`]
    ///
    /// [`AppResources::add_css_font_id`]: ../app_resources/struct.AppResources.html#method.add_css_font_id
    pub fn add_css_font_id<S: Into<String>>(&mut self, css_id: S) -> FontId {
        self.$struct_field.add_css_font_id(css_id)
    }

    /// See [`AppResources::has_css_font_id`]
    ///
    /// [`AppResources::has_css_font_id`]: ../app_resources/struct.AppResources.html#method.has_css_font_id
    pub fn has_css_font_id(&self, css_id: &str) -> bool {
        self.$struct_field.has_css_font_id(css_id)
    }

    /// See [`AppResources::get_css_font_id`]
    ///
    /// [`AppResources::get_css_font_id`]: ../app_resources/struct.AppResources.html#method.get_css_font_id
    pub fn get_css_font_id(&self, css_id: &str) -> Option<&FontId> {
        self.$struct_field.get_css_font_id(css_id)
    }

    /// See [`AppResources::delete_css_font_id`]
    ///
    /// [`AppResources::delete_css_font_id`]: ../app_resources/struct.AppResources.html#method.delete_css_font_id
    pub fn delete_css_font_id(&mut self, css_id: &str) -> Option<FontId> {
        self.$struct_field.delete_css_font_id(css_id)
    }

)}

#[macro_export]
macro_rules! impl_font_api {($struct_field:ident) => (

    /// See [`AppResources::add_font`]
    ///
    /// [`AppResources::add_font`]: ../app_resources/struct.AppResources.html#method.add_font
    pub fn add_font_source(&mut self, font_id: FontId, font_source: FontSource) {
        self.$struct_field.add_font_source(font_id, font_source)
    }

    /// See [`AppResources::has_font`]
    ///
    /// [`AppResources::has_font`]: ../app_resources/struct.AppResources.html#method.has_font
    pub fn has_font_source(&self, font_id: &FontId) -> bool {
        self.$struct_field.has_font_source(font_id)
    }

    /// See [`AppResources::delete_font`]
    ///
    /// [`AppResources::delete_font`]: ../app_resources/struct.AppResources.html#method.delete_font
    pub fn delete_font_source(&mut self, font_id: &FontId) {
        self.$struct_field.delete_font_source(font_id)
    }

    pub fn get_loaded_font(&self, pipeline_id: &PipelineId, font_id: &ImmediateFontId) -> Option<&LoadedFont> {
        self.$struct_field.get_loaded_font(pipeline_id, font_id)
    }
)}

#[macro_export]
macro_rules! impl_text_api {($struct_field:ident) => (

    /// Adds a string to the internal text cache, but only store it as a string,
    /// without caching the layout of the string.
    ///
    /// See [`AppResources::add_text`].
    ///
    /// [`AppResources::add_text`]: ../app_resources/struct.AppResources.html#method.add_text
    pub fn add_text(&mut self, text: &str) -> TextId {
        self.$struct_field.add_text(text)
    }

    /// Removes a string from both the string cache and the layouted text cache
    ///
    /// See [`AppResources::delete_text`].
    ///
    /// [`AppResources::delete_text`]: ../app_resources/struct.AppResources.html#method.delete_text
    pub fn delete_text(&mut self, id: TextId) {
        self.$struct_field.delete_text(id)
    }

    /// Empties the entire internal text cache, invalidating all `TextId`s.
    /// If the given TextId is used after this call, the text will not render in the UI.
    /// Use with care.
    ///
    /// See [`AppResources::clear_all_texts`].
    ///
    /// [`AppResources::clear_all_texts`]: ../app_resources/struct.AppResources.html#method.clear_all_texts
    pub fn clear_all_texts(&mut self) {
        self.$struct_field.clear_all_texts()
    }

)}

#[macro_export]
macro_rules! impl_timer_api {($struct_field:ident) => (

    /// See [`AppState::add_timer`]
    ///
    /// [`AppState::add_timer`]: ../app_state/struct.AppState.html#method.add_timer
    pub fn add_timer(&mut self, timer_id: TimerId, timer: Timer) {
        self.$struct_field.add_timer(timer_id, timer)
    }

    /// See [`AppState::has_timer`]
    ///
    /// [`AppState::has_timer`]: ../app_state/struct.AppState.html#method.has_timer
    pub fn has_timer(&self, timer_id: &TimerId) -> bool {
        self.$struct_field.has_timer(timer_id)
    }

    /// See [`AppState::get_timer`]
    ///
    /// [`AppState::get_timer`]: ../app_state/struct.AppState.html#method.get_timer
    pub fn get_timer(&self, timer_id: &TimerId) -> Option<Timer> {
        self.$struct_field.get_timer(timer_id)
    }

    /// See [`AppState::delete_timer`]
    ///
    /// [`AppState::delete_timer`]: ../app_state/struct.AppState.html#method.delete_timer
    pub fn delete_timer(&mut self, timer_id: &TimerId) -> Option<Timer> {
        self.$struct_field.delete_timer(timer_id)
    }

)}

/// Implements functions for `CallbackInfo` and `Info`,
/// to prevent duplicating the functions
macro_rules! impl_callback_info_api {() => (

    pub fn window_state(&self) -> &FullWindowState {
        self.current_window_state
    }

    pub fn window_state_mut(&mut self) -> &mut WindowState {
        self.modifiable_window_state
    }

    pub fn get_keyboard_state(&self) -> &KeyboardState {
        self.window_state().get_keyboard_state()
    }

    pub fn get_mouse_state(&self) -> &MouseState {
        self.window_state().get_mouse_state()
    }

    /// Returns the bounds (width / height / position / margins / border) for any given NodeId,
    /// useful for calculating scroll positions / offsets
    pub fn get_bounds(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<&PositionedRectangle> {
        self.layout_result.get(&dom_id)?.rects.get(*node_id)
    }

    /// If the node is a text node, return the text of the node
    pub fn get_words(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<&Words> {
        self.layout_result.get(&dom_id)?.word_cache.get(&node_id)
    }

    /// If the node is a text node, return the shaped glyphs (on a per-word basis, unpositioned)
    pub fn get_shaped_words(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<&ShapedWords> {
        self.layout_result.get(&dom_id).as_ref().and_then(|lr| lr.shaped_words.get(&node_id).as_ref().map(|sw| &sw.0))
    }

    /// If the node is a text node, return the shaped glyphs (on a per-word basis, unpositioned)
    pub fn get_word_positions(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<&WordPositions> {
        self.layout_result.get(&dom_id).as_ref().and_then(|lr| lr.positioned_word_cache.get(&node_id).as_ref().map(|sw| &sw.0))
    }

    pub fn get_layouted_glyphs(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<&LayoutedGlyphs> {
        self.layout_result.get(&dom_id)?.layouted_glyph_cache.get(&node_id)
    }

    /// Returns information about the current scroll position of a node, such as the
    /// size of the scroll frame, the position of the scroll in the parent (how far the node has been scrolled),
    /// as well as the size of the parent node (so that things like "scroll to left edge", etc. are easy to calculate).
    pub fn get_current_scroll_position(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<ScrollPosition> {
        self.current_scroll_states.get(&dom_id)?.get(node_id).cloned()
    }

    /// For any node ID, returns what the position in its parent it is, plus the parent itself.
    /// Returns `None` on the root ID (because the root has no parent, therefore it's the 1st item)
    ///
    /// Note: Index is 0-based (first item has the index of 0)
    pub fn get_index_in_parent(&self, node_id: &(DomId, NodeId)) -> Option<(usize, (DomId, NodeId))> {
        let node_layout = &self.ui_state[&node_id.0].dom.arena.node_hierarchy;

        if node_id.1.index() > node_layout.len() {
            return None; // node_id out of range
        }

        let parent_node = self.get_parent_node_id(node_id)?;
        Some((node_layout.get_index_in_parent(node_id.1), parent_node))
    }

    // Functions that are may be called from the user callback
    // - the `CallbackInfo` contains a `&mut UiState`, which can be
    // used to query DOM information when the callbacks are run

    /// Returns the hierarchy of the given node ID
    pub fn get_node(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<&Node> {
        self.ui_state[dom_id].dom.arena.node_hierarchy.internal.get(node_id.index())
    }

    /// Returns the parent of the given `NodeId` or None if the target is the root node.
    pub fn get_parent_node_id(&self, node_id: &(DomId, NodeId)) -> Option<(DomId, NodeId)> {
        let new_node_id = self.get_node(node_id)?.parent?;
        Some((node_id.0.clone(), new_node_id))
    }

    /// Returns the node hierarchy (DOM tree order)
    pub fn get_node_hierarchy(&self) -> &NodeHierarchy {
        &self.ui_state[&self.hit_dom_node.0].dom.arena.node_hierarchy
    }

    /// Returns the node content of a specific node
    pub fn get_node_content(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<&NodeData> {
        self.ui_state[dom_id].dom.arena.node_data.internal.get(node_id.index())
    }

    /// Returns the index of the target NodeId (the target that received the event)
    /// in the targets parent or None if the target is the root node
    pub fn target_index_in_parent(&self) -> Option<usize> {
        let (index, _) = self.get_index_in_parent(&self.hit_dom_node)?;
        Some(index)
    }

    /// Returns the parent of the current target or None if the target is the root node.
    pub fn target_parent_node_id(&self) -> Option<(DomId, NodeId)> {
        self.get_parent_node_id(&self.hit_dom_node)
    }

    /// Checks whether the target of the CallbackInfo has a certain node type
    pub fn target_is_node_type(&self, node_type: NodeType) -> bool {
        if let Some(self_node) = self.get_node_content(&self.hit_dom_node) {
            self_node.is_node_type(node_type)
        } else {
            false
        }
    }

    /// Checks whether the target of the CallbackInfo has a certain ID
    pub fn target_has_id(&self, id: &str) -> bool {
        if let Some(self_node) = self.get_node_content(&self.hit_dom_node) {
            self_node.has_id(id)
        } else {
            false
        }
    }

    /// Checks whether the target of the CallbackInfo has a certain class
    pub fn target_has_class(&self, class: &str) -> bool {
        if let Some(self_node) = self.get_node_content(&self.hit_dom_node) {
            self_node.has_class(class)
        } else {
            false
        }
    }

    /// Traverses up the hierarchy, checks whether any parent has a certain ID,
    /// the returns that parent
    pub fn any_parent_has_id(&self, id: &str) -> Option<(DomId, NodeId)> {
        self.parent_nodes().find(|parent_id| {
            if let Some(self_node) = self.get_node_content(parent_id) {
                self_node.has_id(id)
            } else {
                false
            }
        })
    }

    /// Traverses up the hierarchy, checks whether any parent has a certain class
    pub fn any_parent_has_class(&self, class: &str) -> Option<(DomId, NodeId)> {
        self.parent_nodes().find(|parent_id| {
            if let Some(self_node) = self.get_node_content(parent_id) {
                self_node.has_class(class)
            } else {
                false
            }
        })
    }

    /// Scrolls a node to a certain position
    pub fn scroll_node(&mut self, (dom_id, node_id): &(DomId, NodeId), scroll_location: LayoutPoint) {
        self.nodes_scrolled_in_callback
            .entry(dom_id.clone())
            .or_insert_with(|| BTreeMap::default())
            .insert(*node_id, scroll_location);
    }

    /// Scrolls a node to a certain position
    pub fn scroll_target(&mut self, scroll_location: LayoutPoint) {
        let target = self.hit_dom_node.clone(); // borrowing issue
        self.scroll_node(&target, scroll_location);
    }

    /// Set the focus_target to a certain div by parsing a string.
    /// Note that the parsing of the string can fail, therefore the Result
    #[cfg(feature = "css_parser")]
    pub fn set_focus_from_css<'c>(&mut self, input: &'c str) -> Result<(), CssPathParseError<'c>> {
        use azul_css_parser::parse_css_path;
        let path = parse_css_path(input)?;
        *self.focus_target = Some(FocusTarget::Path((self.hit_dom_node.0.clone(), path)));
        Ok(())
    }

    /// Creates an iterator that starts at the current DOM node and continouusly
    /// returns the parent `(DomId, NodeId)`, until the iterator gets to the root DOM node.
    pub fn parent_nodes<'c>(&'c self) -> ParentNodesIterator<'c> {
        ParentNodesIterator {
            ui_state: &self.ui_state,
            current_item: self.hit_dom_node.clone(),
        }
    }

    /// Sets the focus_target by using an already-parsed `CssPath`.
    pub fn set_focus_from_path(&mut self, path: CssPath) {
        *self.focus_target = Some(FocusTarget::Path((self.hit_dom_node.0.clone(), path)))
    }

    /// Set the focus_target of the window to a specific div using a `NodeId`.
    ///
    /// Note that this ID will be dependent on the position in the DOM and therefore
    /// the next frames UI must be the exact same as the current one, otherwise
    /// the focus_target will be cleared or shifted (depending on apps setting).
    pub fn set_focus_from_node_id(&mut self, id: (DomId, NodeId)) {
        *self.focus_target = Some(FocusTarget::Id(id));
    }

    /// Clears the focus_target for the next frame.
    pub fn clear_focus(&mut self) {
        *self.focus_target = Some(FocusTarget::NoFocus);
    }
)}

