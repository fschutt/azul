#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>


typedef void (*AzThreadCallbackType)(AzRefAny, AzThreadSender, AzThreadReceiver);

typedef void (*AzRefAnyDestructorType)(void*);

typedef AzTimerCallbackReturn (*AzTimerCallbackType)(AzRefAny*, AzRefAny*, AzTimerCallbackInfo);

typedef AzStyledDom (*AzLayoutCallbackType)(AzRefAny*, AzLayoutInfo);

/**
 * Spawn a new window on the screen when the app is run.
 */
void az_app_add_window(AzApp *app, AzWindowCreateOptions window);

/**
 * Creates a new AppConfig with default values
 */
AzAppConfig az_app_config_default(void);

/**
 * Destructor: Takes ownership of the `App` pointer and deletes it.
 */
void az_app_delete(AzApp *object);

/**
 * Returns a list of monitors - useful for setting the monitor that a window should spawn on.
 */
AzMonitorVec az_app_get_monitors(const AzApp *app);

/**
 * Creates a new App instance from the given `AppConfig`
 */
AzApp az_app_new(AzRefAny data, AzAppConfig config);

/**
 * Runs the application. Due to platform restrictions (specifically `WinMain` on Windows), this function never returns.
 */
void az_app_run(AzApp app,
                AzWindowCreateOptions window);

/**
 * Destructor: Takes ownership of the `CallbackDataVec` pointer and deletes it.
 */
void az_callback_data_vec_delete(AzCallbackDataVec *object);

/**
 * Spawns a new window with the given `WindowCreateOptions`.
 */
void az_callback_info_create_window(AzCallbackInfo *callbackinfo, AzWindowCreateOptions new_window);

/**
 * Returns a copy of the current windows `RawWindowHandle`.
 */
AzRawWindowHandle az_callback_info_get_current_window_handle(const AzCallbackInfo *callbackinfo);

/**
 * Returns the `LayoutPoint` of the cursor in the viewport (relative to the origin of the `Dom`). Set to `None` if the cursor is not hovering over the current node.
 */
AzOptionLayoutPoint az_callback_info_get_cursor_relative_to_node(const AzCallbackInfo *callbackinfo);

/**
 * Returns the `LayoutPoint` of the cursor in the viewport (relative to the origin of the `Dom`). Set to `None` if the cursor is not in the current window.
 */
AzOptionLayoutPoint az_callback_info_get_cursor_relative_to_viewport(const AzCallbackInfo *callbackinfo);

/**
 * Returns the next siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
 */
AzOptionDomNodeId az_callback_info_get_first_child(AzCallbackInfo *callbackinfo,
                                                   AzDomNodeId node_id);

/**
 * Returns a **reference-counted copy** of the current windows `GlContextPtr`. You can use this to render OpenGL textures.
 */
AzOptionGlContextPtr az_callback_info_get_gl_context(const AzCallbackInfo *callbackinfo);

/**
 * Returns the `DomNodeId` of the element that the callback was attached to.
 */
AzDomNodeId az_callback_info_get_hit_node(const AzCallbackInfo *callbackinfo);

/**
 * Returns a copy of the internal `KeyboardState`. Same as `self.get_window_state().keyboard_state`
 */
AzKeyboardState az_callback_info_get_keyboard_state(const AzCallbackInfo *callbackinfo);

/**
 * Returns the next siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
 */
AzOptionDomNodeId az_callback_info_get_last_child(AzCallbackInfo *callbackinfo,
                                                  AzDomNodeId node_id);

/**
 * Returns a copy of the internal `MouseState`. Same as `self.get_window_state().mouse_state`
 */
AzMouseState az_callback_info_get_mouse_state(const AzCallbackInfo *callbackinfo);

/**
 * Returns the next siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
 */
AzOptionDomNodeId az_callback_info_get_next_sibling(AzCallbackInfo *callbackinfo,
                                                    AzDomNodeId node_id);

/**
 * Returns the parent `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
 */
AzOptionDomNodeId az_callback_info_get_parent(AzCallbackInfo *callbackinfo, AzDomNodeId node_id);

/**
 * Returns the previous siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
 */
AzOptionDomNodeId az_callback_info_get_previous_sibling(AzCallbackInfo *callbackinfo,
                                                        AzDomNodeId node_id);

/**
 * Returns a copy of the current windows `WindowState`.
 */
AzWindowState az_callback_info_get_window_state(const AzCallbackInfo *callbackinfo);

/**
 * Sets a `CssProperty` on a given node to its new value. If this property change affects the layout, this will automatically trigger a relayout and redraw of the screen.
 */
void az_callback_info_set_css_property(AzCallbackInfo *callbackinfo,
                                       AzDomNodeId node_id,
                                       AzCssProperty new_property);

/**
 * Sets the new `FocusTarget` for the next frame. Note that this will emit a `On::FocusLost` and `On::FocusReceived` event, if the focused node has changed.
 */
void az_callback_info_set_focus(AzCallbackInfo *callbackinfo,
                                AzFocusTarget target);

/**
 * Sets the new `WindowState` for the next frame. The window is updated after all callbacks are run.
 */
void az_callback_info_set_window_state(AzCallbackInfo *callbackinfo, AzWindowState new_state);

/**
 * Starts a new `Thread` to the runtime. See the documentation for `Thread` for more information.
 */
void az_callback_info_start_thread(AzCallbackInfo *callbackinfo,
                                   AzThreadId id,
                                   AzRefAny thread_initialize_data,
                                   AzRefAny writeback_data,
                                   AzThreadCallbackType callback);

/**
 * Adds a new `Timer` to the runtime. See the documentation for `Timer` for more information.
 */
void az_callback_info_start_timer(AzCallbackInfo *callbackinfo, AzTimerId id, AzTimer timer);

/**
 * Stops the propagation of the current callback event type to the parent. Events are bubbled from the inside out (children first, then parents), this event stops the propagation of the event to the parent.
 */
void az_callback_info_stop_propagation(AzCallbackInfo *callbackinfo);

/**
 * Destructor: Takes ownership of the `CascadeInfoVec` pointer and deletes it.
 */
void az_cascade_info_vec_delete(AzCascadeInfoVec *object);

/**
 * Creates a new `ColorU` instance whose memory is owned by the rust allocator
 * Equivalent to the Rust `ColorU::from_str()` constructor.
 */
AzColorU az_color_u_from_str(AzString string);

/**
 * Equivalent to the Rust `ColorU::to_hash()` function.
 */
AzString az_color_u_to_hash(const AzColorU *coloru);

/**
 * Destructor: Takes ownership of the `CssDeclarationVec` pointer and deletes it.
 */
void az_css_declaration_vec_delete(AzCssDeclarationVec *object);

/**
 * Returns an empty CSS style
 */
AzCss az_css_empty(void);

/**
 * Returns a CSS style parsed from a `String`
 */
AzCss az_css_from_string(AzString s);

/**
 * Destructor: Takes ownership of the `CssPathSelectorVec` pointer and deletes it.
 */
void az_css_path_selector_vec_delete(AzCssPathSelectorVec *object);

/**
 * Clones the object
 */
AzCssPropertyCache az_css_property_cache_deep_copy(const AzCssPropertyCache *object);

/**
 * Destructor: Takes ownership of the `CssPropertyCache` pointer and deletes it.
 */
void az_css_property_cache_delete(AzCssPropertyCache *object);

/**
 * Destructor: Takes ownership of the `CssPropertyVec` pointer and deletes it.
 */
void az_css_property_vec_delete(AzCssPropertyVec *object);

/**
 * Destructor: Takes ownership of the `CssRuleBlockVec` pointer and deletes it.
 */
void az_css_rule_block_vec_delete(AzCssRuleBlockVec *object);

/**
 * Destructor: Takes ownership of the `DebugMessageVec` pointer and deletes it.
 */
void az_debug_message_vec_delete(AzDebugMessageVec *object);

/**
 * Returns the number of nodes in the DOM
 */
size_t az_dom_node_count(const AzDom *dom);

/**
 * Destructor: Takes ownership of the `DomVec` pointer and deletes it.
 */
void az_dom_vec_delete(AzDomVec *object);

/**
 * Creates a new, unique `FontId`
 */
AzFontId az_font_id_new(void);

/**
 * Destructor: Takes ownership of the `GLintVec` pointer and deletes it.
 */
void az_g_lint_vec_delete(AzGLintVec *object);

/**
 * Destructor: Takes ownership of the `GLsyncPtr` pointer and deletes it.
 */
void az_g_lsync_ptr_delete(AzGLsyncPtr *object);

/**
 * Destructor: Takes ownership of the `GLuintVec` pointer and deletes it.
 */
void az_g_luint_vec_delete(AzGLuintVec *object);

/**
 * Returns a copy of the internal `HidpiAdjustedBounds`
 */
AzHidpiAdjustedBounds az_gl_callback_info_get_bounds(const AzGlCallbackInfo *glcallbackinfo);

/**
 * Returns a copy of the internal `GlContextPtr`
 */
AzOptionGlContextPtr az_gl_callback_info_get_gl_context(const AzGlCallbackInfo *glcallbackinfo);

/**
 * Equivalent to the Rust `GlContextPtr::active_texture()` function.
 */
void az_gl_context_ptr_active_texture(const AzGlContextPtr *glcontextptr, uint32_t texture);

/**
 * Equivalent to the Rust `GlContextPtr::attach_shader()` function.
 */
void az_gl_context_ptr_attach_shader(const AzGlContextPtr *glcontextptr,
                                     uint32_t program,
                                     uint32_t shader);

/**
 * Equivalent to the Rust `GlContextPtr::begin_query()` function.
 */
void az_gl_context_ptr_begin_query(const AzGlContextPtr *glcontextptr,
                                   uint32_t target,
                                   uint32_t id);

/**
 * Equivalent to the Rust `GlContextPtr::bind_attrib_location()` function.
 */
void az_gl_context_ptr_bind_attrib_location(const AzGlContextPtr *glcontextptr,
                                            uint32_t program,
                                            uint32_t index,
                                            AzRefstr name);

/**
 * Equivalent to the Rust `GlContextPtr::bind_buffer()` function.
 */
void az_gl_context_ptr_bind_buffer(const AzGlContextPtr *glcontextptr,
                                   uint32_t target,
                                   uint32_t buffer);

/**
 * Equivalent to the Rust `GlContextPtr::bind_buffer_base()` function.
 */
void az_gl_context_ptr_bind_buffer_base(const AzGlContextPtr *glcontextptr,
                                        uint32_t target,
                                        uint32_t index,
                                        uint32_t buffer);

/**
 * Equivalent to the Rust `GlContextPtr::bind_buffer_range()` function.
 */
void az_gl_context_ptr_bind_buffer_range(const AzGlContextPtr *glcontextptr,
                                         uint32_t target,
                                         uint32_t index,
                                         uint32_t buffer,
                                         ptrdiff_t offset,
                                         ptrdiff_t size);

/**
 * Equivalent to the Rust `GlContextPtr::bind_frag_data_location_indexed()` function.
 */
void az_gl_context_ptr_bind_frag_data_location_indexed(const AzGlContextPtr *glcontextptr,
                                                       uint32_t program,
                                                       uint32_t color_number,
                                                       uint32_t index,
                                                       AzRefstr name);

/**
 * Equivalent to the Rust `GlContextPtr::bind_framebuffer()` function.
 */
void az_gl_context_ptr_bind_framebuffer(const AzGlContextPtr *glcontextptr,
                                        uint32_t target,
                                        uint32_t framebuffer);

/**
 * Equivalent to the Rust `GlContextPtr::bind_renderbuffer()` function.
 */
void az_gl_context_ptr_bind_renderbuffer(const AzGlContextPtr *glcontextptr,
                                         uint32_t target,
                                         uint32_t renderbuffer);

/**
 * Equivalent to the Rust `GlContextPtr::bind_texture()` function.
 */
void az_gl_context_ptr_bind_texture(const AzGlContextPtr *glcontextptr,
                                    uint32_t target,
                                    uint32_t texture);

/**
 * Equivalent to the Rust `GlContextPtr::bind_vertex_array()` function.
 */
void az_gl_context_ptr_bind_vertex_array(const AzGlContextPtr *glcontextptr, uint32_t vao);

/**
 * Equivalent to the Rust `GlContextPtr::bind_vertex_array_apple()` function.
 */
void az_gl_context_ptr_bind_vertex_array_apple(const AzGlContextPtr *glcontextptr, uint32_t vao);

/**
 * Equivalent to the Rust `GlContextPtr::blend_barrier_khr()` function.
 */
void az_gl_context_ptr_blend_barrier_khr(const AzGlContextPtr *glcontextptr);

/**
 * Equivalent to the Rust `GlContextPtr::blend_color()` function.
 */
void az_gl_context_ptr_blend_color(const AzGlContextPtr *glcontextptr,
                                   float r,
                                   float g,
                                   float b,
                                   float a);

/**
 * Equivalent to the Rust `GlContextPtr::blend_equation()` function.
 */
void az_gl_context_ptr_blend_equation(const AzGlContextPtr *glcontextptr, uint32_t mode);

/**
 * Equivalent to the Rust `GlContextPtr::blend_equation_separate()` function.
 */
void az_gl_context_ptr_blend_equation_separate(const AzGlContextPtr *glcontextptr,
                                               uint32_t mode_rgb,
                                               uint32_t mode_alpha);

/**
 * Equivalent to the Rust `GlContextPtr::blend_func()` function.
 */
void az_gl_context_ptr_blend_func(const AzGlContextPtr *glcontextptr,
                                  uint32_t sfactor,
                                  uint32_t dfactor);

/**
 * Equivalent to the Rust `GlContextPtr::blend_func_separate()` function.
 */
void az_gl_context_ptr_blend_func_separate(const AzGlContextPtr *glcontextptr,
                                           uint32_t src_rgb,
                                           uint32_t dest_rgb,
                                           uint32_t src_alpha,
                                           uint32_t dest_alpha);

/**
 * Equivalent to the Rust `GlContextPtr::blit_framebuffer()` function.
 */
void az_gl_context_ptr_blit_framebuffer(const AzGlContextPtr *glcontextptr,
                                        int32_t src_x0,
                                        int32_t src_y0,
                                        int32_t src_x1,
                                        int32_t src_y1,
                                        int32_t dst_x0,
                                        int32_t dst_y0,
                                        int32_t dst_x1,
                                        int32_t dst_y1,
                                        uint32_t mask,
                                        uint32_t filter);

/**
 * Equivalent to the Rust `GlContextPtr::buffer_data_untyped()` function.
 */
void az_gl_context_ptr_buffer_data_untyped(const AzGlContextPtr *glcontextptr,
                                           uint32_t target,
                                           ptrdiff_t size,
                                           const void *data,
                                           uint32_t usage);

/**
 * Equivalent to the Rust `GlContextPtr::buffer_storage()` function.
 */
void az_gl_context_ptr_buffer_storage(const AzGlContextPtr *glcontextptr,
                                      uint32_t target,
                                      ptrdiff_t size,
                                      const void *data,
                                      uint32_t flags);

/**
 * Equivalent to the Rust `GlContextPtr::buffer_sub_data_untyped()` function.
 */
void az_gl_context_ptr_buffer_sub_data_untyped(const AzGlContextPtr *glcontextptr,
                                               uint32_t target,
                                               ptrdiff_t offset,
                                               ptrdiff_t size,
                                               const void *data);

/**
 * Equivalent to the Rust `GlContextPtr::check_frame_buffer_status()` function.
 */
uint32_t az_gl_context_ptr_check_frame_buffer_status(const AzGlContextPtr *glcontextptr,
                                                     uint32_t target);

/**
 * Equivalent to the Rust `GlContextPtr::clear()` function.
 */
void az_gl_context_ptr_clear(const AzGlContextPtr *glcontextptr, uint32_t buffer_mask);

/**
 * Equivalent to the Rust `GlContextPtr::clear_color()` function.
 */
void az_gl_context_ptr_clear_color(const AzGlContextPtr *glcontextptr,
                                   float r,
                                   float g,
                                   float b,
                                   float a);

/**
 * Equivalent to the Rust `GlContextPtr::clear_depth()` function.
 */
void az_gl_context_ptr_clear_depth(const AzGlContextPtr *glcontextptr, double depth);

/**
 * Equivalent to the Rust `GlContextPtr::clear_stencil()` function.
 */
void az_gl_context_ptr_clear_stencil(const AzGlContextPtr *glcontextptr, int32_t s);

/**
 * Equivalent to the Rust `GlContextPtr::client_wait_sync()` function.
 */
uint32_t az_gl_context_ptr_client_wait_sync(const AzGlContextPtr *glcontextptr,
                                            AzGLsyncPtr sync,
                                            uint32_t flags,
                                            uint64_t timeout);

/**
 * Equivalent to the Rust `GlContextPtr::color_mask()` function.
 */
void az_gl_context_ptr_color_mask(const AzGlContextPtr *glcontextptr,
                                  bool r,
                                  bool g,
                                  bool b,
                                  bool a);

/**
 * Equivalent to the Rust `GlContextPtr::compile_shader()` function.
 */
void az_gl_context_ptr_compile_shader(const AzGlContextPtr *glcontextptr, uint32_t shader);

/**
 * Equivalent to the Rust `GlContextPtr::compressed_tex_image_2d()` function.
 */
void az_gl_context_ptr_compressed_tex_image_2d(const AzGlContextPtr *glcontextptr,
                                               uint32_t target,
                                               int32_t level,
                                               uint32_t internal_format,
                                               int32_t width,
                                               int32_t height,
                                               int32_t border,
                                               AzU8VecRef data);

/**
 * Equivalent to the Rust `GlContextPtr::compressed_tex_sub_image_2d()` function.
 */
void az_gl_context_ptr_compressed_tex_sub_image_2d(const AzGlContextPtr *glcontextptr,
                                                   uint32_t target,
                                                   int32_t level,
                                                   int32_t xoffset,
                                                   int32_t yoffset,
                                                   int32_t width,
                                                   int32_t height,
                                                   uint32_t format,
                                                   AzU8VecRef data);

/**
 * Equivalent to the Rust `GlContextPtr::copy_image_sub_data()` function.
 */
void az_gl_context_ptr_copy_image_sub_data(const AzGlContextPtr *glcontextptr,
                                           uint32_t src_name,
                                           uint32_t src_target,
                                           int32_t src_level,
                                           int32_t src_x,
                                           int32_t src_y,
                                           int32_t src_z,
                                           uint32_t dst_name,
                                           uint32_t dst_target,
                                           int32_t dst_level,
                                           int32_t dst_x,
                                           int32_t dst_y,
                                           int32_t dst_z,
                                           int32_t src_width,
                                           int32_t src_height,
                                           int32_t src_depth);

/**
 * Equivalent to the Rust `GlContextPtr::copy_sub_texture_3d_angle()` function.
 */
void az_gl_context_ptr_copy_sub_texture_3d_angle(const AzGlContextPtr *glcontextptr,
                                                 uint32_t source_id,
                                                 int32_t source_level,
                                                 uint32_t dest_target,
                                                 uint32_t dest_id,
                                                 int32_t dest_level,
                                                 int32_t x_offset,
                                                 int32_t y_offset,
                                                 int32_t z_offset,
                                                 int32_t x,
                                                 int32_t y,
                                                 int32_t z,
                                                 int32_t width,
                                                 int32_t height,
                                                 int32_t depth,
                                                 uint8_t unpack_flip_y,
                                                 uint8_t unpack_premultiply_alpha,
                                                 uint8_t unpack_unmultiply_alpha);

/**
 * Equivalent to the Rust `GlContextPtr::copy_sub_texture_chromium()` function.
 */
void az_gl_context_ptr_copy_sub_texture_chromium(const AzGlContextPtr *glcontextptr,
                                                 uint32_t source_id,
                                                 int32_t source_level,
                                                 uint32_t dest_target,
                                                 uint32_t dest_id,
                                                 int32_t dest_level,
                                                 int32_t x_offset,
                                                 int32_t y_offset,
                                                 int32_t x,
                                                 int32_t y,
                                                 int32_t width,
                                                 int32_t height,
                                                 uint8_t unpack_flip_y,
                                                 uint8_t unpack_premultiply_alpha,
                                                 uint8_t unpack_unmultiply_alpha);

/**
 * Equivalent to the Rust `GlContextPtr::copy_tex_image_2d()` function.
 */
void az_gl_context_ptr_copy_tex_image_2d(const AzGlContextPtr *glcontextptr,
                                         uint32_t target,
                                         int32_t level,
                                         uint32_t internal_format,
                                         int32_t x,
                                         int32_t y,
                                         int32_t width,
                                         int32_t height,
                                         int32_t border);

/**
 * Equivalent to the Rust `GlContextPtr::copy_tex_sub_image_2d()` function.
 */
void az_gl_context_ptr_copy_tex_sub_image_2d(const AzGlContextPtr *glcontextptr,
                                             uint32_t target,
                                             int32_t level,
                                             int32_t xoffset,
                                             int32_t yoffset,
                                             int32_t x,
                                             int32_t y,
                                             int32_t width,
                                             int32_t height);

/**
 * Equivalent to the Rust `GlContextPtr::copy_tex_sub_image_3d()` function.
 */
void az_gl_context_ptr_copy_tex_sub_image_3d(const AzGlContextPtr *glcontextptr,
                                             uint32_t target,
                                             int32_t level,
                                             int32_t xoffset,
                                             int32_t yoffset,
                                             int32_t zoffset,
                                             int32_t x,
                                             int32_t y,
                                             int32_t width,
                                             int32_t height);

/**
 * Equivalent to the Rust `GlContextPtr::copy_texture_3d_angle()` function.
 */
void az_gl_context_ptr_copy_texture_3d_angle(const AzGlContextPtr *glcontextptr,
                                             uint32_t source_id,
                                             int32_t source_level,
                                             uint32_t dest_target,
                                             uint32_t dest_id,
                                             int32_t dest_level,
                                             int32_t internal_format,
                                             uint32_t dest_type,
                                             uint8_t unpack_flip_y,
                                             uint8_t unpack_premultiply_alpha,
                                             uint8_t unpack_unmultiply_alpha);

/**
 * Equivalent to the Rust `GlContextPtr::copy_texture_chromium()` function.
 */
void az_gl_context_ptr_copy_texture_chromium(const AzGlContextPtr *glcontextptr,
                                             uint32_t source_id,
                                             int32_t source_level,
                                             uint32_t dest_target,
                                             uint32_t dest_id,
                                             int32_t dest_level,
                                             int32_t internal_format,
                                             uint32_t dest_type,
                                             uint8_t unpack_flip_y,
                                             uint8_t unpack_premultiply_alpha,
                                             uint8_t unpack_unmultiply_alpha);

/**
 * Equivalent to the Rust `GlContextPtr::create_program()` function.
 */
uint32_t az_gl_context_ptr_create_program(const AzGlContextPtr *glcontextptr);

/**
 * Equivalent to the Rust `GlContextPtr::create_shader()` function.
 */
uint32_t az_gl_context_ptr_create_shader(const AzGlContextPtr *glcontextptr, uint32_t shader_type);

/**
 * Equivalent to the Rust `GlContextPtr::cull_face()` function.
 */
void az_gl_context_ptr_cull_face(const AzGlContextPtr *glcontextptr, uint32_t mode);

/**
 * Equivalent to the Rust `GlContextPtr::debug_message_insert_khr()` function.
 */
void az_gl_context_ptr_debug_message_insert_khr(const AzGlContextPtr *glcontextptr,
                                                uint32_t source,
                                                uint32_t type_,
                                                uint32_t id,
                                                uint32_t severity,
                                                AzRefstr message);

/**
 * Clones the object
 */
AzGlContextPtr az_gl_context_ptr_deep_copy(const AzGlContextPtr *object);

/**
 * Destructor: Takes ownership of the `GlContextPtr` pointer and deletes it.
 */
void az_gl_context_ptr_delete(AzGlContextPtr *object);

/**
 * Equivalent to the Rust `GlContextPtr::delete_buffers()` function.
 */
void az_gl_context_ptr_delete_buffers(const AzGlContextPtr *glcontextptr, AzGLuintVecRef buffers);

/**
 * Equivalent to the Rust `GlContextPtr::delete_fences_apple()` function.
 */
void az_gl_context_ptr_delete_fences_apple(const AzGlContextPtr *glcontextptr,
                                           AzGLuintVecRef fences);

/**
 * Equivalent to the Rust `GlContextPtr::delete_framebuffers()` function.
 */
void az_gl_context_ptr_delete_framebuffers(const AzGlContextPtr *glcontextptr,
                                           AzGLuintVecRef framebuffers);

/**
 * Equivalent to the Rust `GlContextPtr::delete_program()` function.
 */
void az_gl_context_ptr_delete_program(const AzGlContextPtr *glcontextptr, uint32_t program);

/**
 * Equivalent to the Rust `GlContextPtr::delete_queries()` function.
 */
void az_gl_context_ptr_delete_queries(const AzGlContextPtr *glcontextptr, AzGLuintVecRef queries);

/**
 * Equivalent to the Rust `GlContextPtr::delete_renderbuffers()` function.
 */
void az_gl_context_ptr_delete_renderbuffers(const AzGlContextPtr *glcontextptr,
                                            AzGLuintVecRef renderbuffers);

/**
 * Equivalent to the Rust `GlContextPtr::delete_shader()` function.
 */
void az_gl_context_ptr_delete_shader(const AzGlContextPtr *glcontextptr, uint32_t shader);

/**
 * Equivalent to the Rust `GlContextPtr::delete_sync()` function.
 */
void az_gl_context_ptr_delete_sync(const AzGlContextPtr *glcontextptr, AzGLsyncPtr sync);

/**
 * Equivalent to the Rust `GlContextPtr::delete_textures()` function.
 */
void az_gl_context_ptr_delete_textures(const AzGlContextPtr *glcontextptr, AzGLuintVecRef textures);

/**
 * Equivalent to the Rust `GlContextPtr::delete_vertex_arrays()` function.
 */
void az_gl_context_ptr_delete_vertex_arrays(const AzGlContextPtr *glcontextptr,
                                            AzGLuintVecRef vertex_arrays);

/**
 * Equivalent to the Rust `GlContextPtr::delete_vertex_arrays_apple()` function.
 */
void az_gl_context_ptr_delete_vertex_arrays_apple(const AzGlContextPtr *glcontextptr,
                                                  AzGLuintVecRef vertex_arrays);

/**
 * Equivalent to the Rust `GlContextPtr::depth_func()` function.
 */
void az_gl_context_ptr_depth_func(const AzGlContextPtr *glcontextptr, uint32_t func);

/**
 * Equivalent to the Rust `GlContextPtr::depth_mask()` function.
 */
void az_gl_context_ptr_depth_mask(const AzGlContextPtr *glcontextptr, bool flag);

/**
 * Equivalent to the Rust `GlContextPtr::depth_range()` function.
 */
void az_gl_context_ptr_depth_range(const AzGlContextPtr *glcontextptr, double near, double far);

/**
 * Equivalent to the Rust `GlContextPtr::detach_shader()` function.
 */
void az_gl_context_ptr_detach_shader(const AzGlContextPtr *glcontextptr,
                                     uint32_t program,
                                     uint32_t shader);

/**
 * Equivalent to the Rust `GlContextPtr::disable()` function.
 */
void az_gl_context_ptr_disable(const AzGlContextPtr *glcontextptr, uint32_t cap);

/**
 * Equivalent to the Rust `GlContextPtr::disable_vertex_attrib_array()` function.
 */
void az_gl_context_ptr_disable_vertex_attrib_array(const AzGlContextPtr *glcontextptr,
                                                   uint32_t index);

/**
 * Equivalent to the Rust `GlContextPtr::draw_arrays()` function.
 */
void az_gl_context_ptr_draw_arrays(const AzGlContextPtr *glcontextptr,
                                   uint32_t mode,
                                   int32_t first,
                                   int32_t count);

/**
 * Equivalent to the Rust `GlContextPtr::draw_arrays_instanced()` function.
 */
void az_gl_context_ptr_draw_arrays_instanced(const AzGlContextPtr *glcontextptr,
                                             uint32_t mode,
                                             int32_t first,
                                             int32_t count,
                                             int32_t primcount);

/**
 * Equivalent to the Rust `GlContextPtr::draw_buffers()` function.
 */
void az_gl_context_ptr_draw_buffers(const AzGlContextPtr *glcontextptr, AzGLenumVecRef bufs);

/**
 * Equivalent to the Rust `GlContextPtr::draw_elements()` function.
 */
void az_gl_context_ptr_draw_elements(const AzGlContextPtr *glcontextptr,
                                     uint32_t mode,
                                     int32_t count,
                                     uint32_t element_type,
                                     uint32_t indices_offset);

/**
 * Equivalent to the Rust `GlContextPtr::draw_elements_instanced()` function.
 */
void az_gl_context_ptr_draw_elements_instanced(const AzGlContextPtr *glcontextptr,
                                               uint32_t mode,
                                               int32_t count,
                                               uint32_t element_type,
                                               uint32_t indices_offset,
                                               int32_t primcount);

/**
 * Equivalent to the Rust `GlContextPtr::egl_image_target_renderbuffer_storage_oes()` function.
 */
void az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes(const AzGlContextPtr *glcontextptr,
                                                                 uint32_t target,
                                                                 const void *image);

/**
 * Equivalent to the Rust `GlContextPtr::egl_image_target_texture2d_oes()` function.
 */
void az_gl_context_ptr_egl_image_target_texture2d_oes(const AzGlContextPtr *glcontextptr,
                                                      uint32_t target,
                                                      const void *image);

/**
 * Equivalent to the Rust `GlContextPtr::enable()` function.
 */
void az_gl_context_ptr_enable(const AzGlContextPtr *glcontextptr, uint32_t cap);

/**
 * Equivalent to the Rust `GlContextPtr::enable_vertex_attrib_array()` function.
 */
void az_gl_context_ptr_enable_vertex_attrib_array(const AzGlContextPtr *glcontextptr,
                                                  uint32_t index);

/**
 * Equivalent to the Rust `GlContextPtr::end_query()` function.
 */
void az_gl_context_ptr_end_query(const AzGlContextPtr *glcontextptr, uint32_t target);

/**
 * Equivalent to the Rust `GlContextPtr::fence_sync()` function.
 */
AzGLsyncPtr az_gl_context_ptr_fence_sync(const AzGlContextPtr *glcontextptr,
                                         uint32_t condition,
                                         uint32_t flags);

/**
 * Equivalent to the Rust `GlContextPtr::finish()` function.
 */
void az_gl_context_ptr_finish(const AzGlContextPtr *glcontextptr);

/**
 * Equivalent to the Rust `GlContextPtr::finish_fence_apple()` function.
 */
void az_gl_context_ptr_finish_fence_apple(const AzGlContextPtr *glcontextptr, uint32_t fence);

/**
 * Equivalent to the Rust `GlContextPtr::finish_object_apple()` function.
 */
void az_gl_context_ptr_finish_object_apple(const AzGlContextPtr *glcontextptr,
                                           uint32_t object,
                                           uint32_t name);

/**
 * Equivalent to the Rust `GlContextPtr::flush()` function.
 */
void az_gl_context_ptr_flush(const AzGlContextPtr *glcontextptr);

/**
 * Equivalent to the Rust `GlContextPtr::flush_mapped_buffer_range()` function.
 */
void az_gl_context_ptr_flush_mapped_buffer_range(const AzGlContextPtr *glcontextptr,
                                                 uint32_t target,
                                                 ptrdiff_t offset,
                                                 ptrdiff_t length);

/**
 * Equivalent to the Rust `GlContextPtr::framebuffer_renderbuffer()` function.
 */
void az_gl_context_ptr_framebuffer_renderbuffer(const AzGlContextPtr *glcontextptr,
                                                uint32_t target,
                                                uint32_t attachment,
                                                uint32_t renderbuffertarget,
                                                uint32_t renderbuffer);

/**
 * Equivalent to the Rust `GlContextPtr::framebuffer_texture_2d()` function.
 */
void az_gl_context_ptr_framebuffer_texture_2d(const AzGlContextPtr *glcontextptr,
                                              uint32_t target,
                                              uint32_t attachment,
                                              uint32_t textarget,
                                              uint32_t texture,
                                              int32_t level);

/**
 * Equivalent to the Rust `GlContextPtr::framebuffer_texture_layer()` function.
 */
void az_gl_context_ptr_framebuffer_texture_layer(const AzGlContextPtr *glcontextptr,
                                                 uint32_t target,
                                                 uint32_t attachment,
                                                 uint32_t texture,
                                                 int32_t level,
                                                 int32_t layer);

/**
 * Equivalent to the Rust `GlContextPtr::front_face()` function.
 */
void az_gl_context_ptr_front_face(const AzGlContextPtr *glcontextptr, uint32_t mode);

/**
 * Equivalent to the Rust `GlContextPtr::gen_buffers()` function.
 */
AzGLuintVec az_gl_context_ptr_gen_buffers(const AzGlContextPtr *glcontextptr, int32_t n);

/**
 * Equivalent to the Rust `GlContextPtr::gen_fences_apple()` function.
 */
AzGLuintVec az_gl_context_ptr_gen_fences_apple(const AzGlContextPtr *glcontextptr, int32_t n);

/**
 * Equivalent to the Rust `GlContextPtr::gen_framebuffers()` function.
 */
AzGLuintVec az_gl_context_ptr_gen_framebuffers(const AzGlContextPtr *glcontextptr, int32_t n);

/**
 * Equivalent to the Rust `GlContextPtr::gen_queries()` function.
 */
AzGLuintVec az_gl_context_ptr_gen_queries(const AzGlContextPtr *glcontextptr, int32_t n);

/**
 * Equivalent to the Rust `GlContextPtr::gen_renderbuffers()` function.
 */
AzGLuintVec az_gl_context_ptr_gen_renderbuffers(const AzGlContextPtr *glcontextptr, int32_t n);

/**
 * Equivalent to the Rust `GlContextPtr::gen_textures()` function.
 */
AzGLuintVec az_gl_context_ptr_gen_textures(const AzGlContextPtr *glcontextptr, int32_t n);

/**
 * Equivalent to the Rust `GlContextPtr::gen_vertex_arrays()` function.
 */
AzGLuintVec az_gl_context_ptr_gen_vertex_arrays(const AzGlContextPtr *glcontextptr, int32_t n);

/**
 * Equivalent to the Rust `GlContextPtr::gen_vertex_arrays_apple()` function.
 */
AzGLuintVec az_gl_context_ptr_gen_vertex_arrays_apple(const AzGlContextPtr *glcontextptr,
                                                      int32_t n);

/**
 * Equivalent to the Rust `GlContextPtr::generate_mipmap()` function.
 */
void az_gl_context_ptr_generate_mipmap(const AzGlContextPtr *glcontextptr, uint32_t target);

/**
 * Equivalent to the Rust `GlContextPtr::get_active_attrib()` function.
 */
AzGetActiveAttribReturn az_gl_context_ptr_get_active_attrib(const AzGlContextPtr *glcontextptr,
                                                            uint32_t program,
                                                            uint32_t index);

/**
 * Equivalent to the Rust `GlContextPtr::get_active_uniform()` function.
 */
AzGetActiveUniformReturn az_gl_context_ptr_get_active_uniform(const AzGlContextPtr *glcontextptr,
                                                              uint32_t program,
                                                              uint32_t index);

/**
 * Equivalent to the Rust `GlContextPtr::get_active_uniform_block_i()` function.
 */
int32_t az_gl_context_ptr_get_active_uniform_block_i(const AzGlContextPtr *glcontextptr,
                                                     uint32_t program,
                                                     uint32_t index,
                                                     uint32_t pname);

/**
 * Equivalent to the Rust `GlContextPtr::get_active_uniform_block_iv()` function.
 */
AzGLintVec az_gl_context_ptr_get_active_uniform_block_iv(const AzGlContextPtr *glcontextptr,
                                                         uint32_t program,
                                                         uint32_t index,
                                                         uint32_t pname);

/**
 * Equivalent to the Rust `GlContextPtr::get_active_uniform_block_name()` function.
 */
AzString az_gl_context_ptr_get_active_uniform_block_name(const AzGlContextPtr *glcontextptr,
                                                         uint32_t program,
                                                         uint32_t index);

/**
 * Equivalent to the Rust `GlContextPtr::get_active_uniforms_iv()` function.
 */
AzGLintVec az_gl_context_ptr_get_active_uniforms_iv(const AzGlContextPtr *glcontextptr,
                                                    uint32_t program,
                                                    AzGLuintVec indices,
                                                    uint32_t pname);

/**
 * Equivalent to the Rust `GlContextPtr::get_attrib_location()` function.
 */
int32_t az_gl_context_ptr_get_attrib_location(const AzGlContextPtr *glcontextptr,
                                              uint32_t program,
                                              AzRefstr name);

/**
 * Equivalent to the Rust `GlContextPtr::get_boolean_v()` function.
 */
void az_gl_context_ptr_get_boolean_v(const AzGlContextPtr *glcontextptr,
                                     uint32_t name,
                                     AzGLbooleanVecRefMut result);

/**
 * Equivalent to the Rust `GlContextPtr::get_buffer_parameter_iv()` function.
 */
int32_t az_gl_context_ptr_get_buffer_parameter_iv(const AzGlContextPtr *glcontextptr,
                                                  uint32_t target,
                                                  uint32_t pname);

/**
 * Equivalent to the Rust `GlContextPtr::get_debug_messages()` function.
 */
AzDebugMessageVec az_gl_context_ptr_get_debug_messages(const AzGlContextPtr *glcontextptr);

/**
 * Equivalent to the Rust `GlContextPtr::get_error()` function.
 */
uint32_t az_gl_context_ptr_get_error(const AzGlContextPtr *glcontextptr);

/**
 * Equivalent to the Rust `GlContextPtr::get_float_v()` function.
 */
void az_gl_context_ptr_get_float_v(const AzGlContextPtr *glcontextptr,
                                   uint32_t name,
                                   AzGLfloatVecRefMut result);

/**
 * Equivalent to the Rust `GlContextPtr::get_frag_data_index()` function.
 */
int32_t az_gl_context_ptr_get_frag_data_index(const AzGlContextPtr *glcontextptr,
                                              uint32_t program,
                                              AzRefstr name);

/**
 * Equivalent to the Rust `GlContextPtr::get_frag_data_location()` function.
 */
int32_t az_gl_context_ptr_get_frag_data_location(const AzGlContextPtr *glcontextptr,
                                                 uint32_t program,
                                                 AzRefstr name);

/**
 * Equivalent to the Rust `GlContextPtr::get_framebuffer_attachment_parameter_iv()` function.
 */
int32_t az_gl_context_ptr_get_framebuffer_attachment_parameter_iv(const AzGlContextPtr *glcontextptr,
                                                                  uint32_t target,
                                                                  uint32_t attachment,
                                                                  uint32_t pname);

/**
 * Equivalent to the Rust `GlContextPtr::get_integer_64iv()` function.
 */
void az_gl_context_ptr_get_integer_64iv(const AzGlContextPtr *glcontextptr,
                                        uint32_t name,
                                        uint32_t index,
                                        AzGLint64VecRefMut result);

/**
 * Equivalent to the Rust `GlContextPtr::get_integer_64v()` function.
 */
void az_gl_context_ptr_get_integer_64v(const AzGlContextPtr *glcontextptr,
                                       uint32_t name,
                                       AzGLint64VecRefMut result);

/**
 * Equivalent to the Rust `GlContextPtr::get_integer_iv()` function.
 */
void az_gl_context_ptr_get_integer_iv(const AzGlContextPtr *glcontextptr,
                                      uint32_t name,
                                      uint32_t index,
                                      AzGLintVecRefMut result);

/**
 * Equivalent to the Rust `GlContextPtr::get_integer_v()` function.
 */
void az_gl_context_ptr_get_integer_v(const AzGlContextPtr *glcontextptr,
                                     uint32_t name,
                                     AzGLintVecRefMut result);

/**
 * Equivalent to the Rust `GlContextPtr::get_program_binary()` function.
 */
AzGetProgramBinaryReturn az_gl_context_ptr_get_program_binary(const AzGlContextPtr *glcontextptr,
                                                              uint32_t program);

/**
 * Equivalent to the Rust `GlContextPtr::get_program_info_log()` function.
 */
AzString az_gl_context_ptr_get_program_info_log(const AzGlContextPtr *glcontextptr,
                                                uint32_t program);

/**
 * Equivalent to the Rust `GlContextPtr::get_program_iv()` function.
 */
void az_gl_context_ptr_get_program_iv(const AzGlContextPtr *glcontextptr,
                                      uint32_t program,
                                      uint32_t pname,
                                      AzGLintVecRefMut result);

/**
 * Equivalent to the Rust `GlContextPtr::get_query_object_i64v()` function.
 */
int64_t az_gl_context_ptr_get_query_object_i64v(const AzGlContextPtr *glcontextptr,
                                                uint32_t id,
                                                uint32_t pname);

/**
 * Equivalent to the Rust `GlContextPtr::get_query_object_iv()` function.
 */
int32_t az_gl_context_ptr_get_query_object_iv(const AzGlContextPtr *glcontextptr,
                                              uint32_t id,
                                              uint32_t pname);

/**
 * Equivalent to the Rust `GlContextPtr::get_query_object_ui64v()` function.
 */
uint64_t az_gl_context_ptr_get_query_object_ui64v(const AzGlContextPtr *glcontextptr,
                                                  uint32_t id,
                                                  uint32_t pname);

/**
 * Equivalent to the Rust `GlContextPtr::get_query_object_uiv()` function.
 */
uint32_t az_gl_context_ptr_get_query_object_uiv(const AzGlContextPtr *glcontextptr,
                                                uint32_t id,
                                                uint32_t pname);

/**
 * Equivalent to the Rust `GlContextPtr::get_renderbuffer_parameter_iv()` function.
 */
int32_t az_gl_context_ptr_get_renderbuffer_parameter_iv(const AzGlContextPtr *glcontextptr,
                                                        uint32_t target,
                                                        uint32_t pname);

/**
 * Equivalent to the Rust `GlContextPtr::get_shader_info_log()` function.
 */
AzString az_gl_context_ptr_get_shader_info_log(const AzGlContextPtr *glcontextptr, uint32_t shader);

/**
 * Equivalent to the Rust `GlContextPtr::get_shader_iv()` function.
 */
void az_gl_context_ptr_get_shader_iv(const AzGlContextPtr *glcontextptr,
                                     uint32_t shader,
                                     uint32_t pname,
                                     AzGLintVecRefMut result);

/**
 * Equivalent to the Rust `GlContextPtr::get_shader_precision_format()` function.
 */
AzGlShaderPrecisionFormatReturn az_gl_context_ptr_get_shader_precision_format(const AzGlContextPtr *glcontextptr,
                                                                              uint32_t shader_type,
                                                                              uint32_t precision_type);

/**
 * Equivalent to the Rust `GlContextPtr::get_string()` function.
 */
AzString az_gl_context_ptr_get_string(const AzGlContextPtr *glcontextptr, uint32_t which);

/**
 * Equivalent to the Rust `GlContextPtr::get_string_i()` function.
 */
AzString az_gl_context_ptr_get_string_i(const AzGlContextPtr *glcontextptr,
                                        uint32_t which,
                                        uint32_t index);

/**
 * Equivalent to the Rust `GlContextPtr::get_tex_image_into_buffer()` function.
 */
void az_gl_context_ptr_get_tex_image_into_buffer(const AzGlContextPtr *glcontextptr,
                                                 uint32_t target,
                                                 int32_t level,
                                                 uint32_t format,
                                                 uint32_t ty,
                                                 AzU8VecRefMut output);

/**
 * Equivalent to the Rust `GlContextPtr::get_tex_parameter_fv()` function.
 */
float az_gl_context_ptr_get_tex_parameter_fv(const AzGlContextPtr *glcontextptr,
                                             uint32_t target,
                                             uint32_t name);

/**
 * Equivalent to the Rust `GlContextPtr::get_tex_parameter_iv()` function.
 */
int32_t az_gl_context_ptr_get_tex_parameter_iv(const AzGlContextPtr *glcontextptr,
                                               uint32_t target,
                                               uint32_t name);

/**
 * Equivalent to the Rust `GlContextPtr::get_type()` function.
 */
AzGlType az_gl_context_ptr_get_type(const AzGlContextPtr *glcontextptr);

/**
 * Equivalent to the Rust `GlContextPtr::get_uniform_block_index()` function.
 */
uint32_t az_gl_context_ptr_get_uniform_block_index(const AzGlContextPtr *glcontextptr,
                                                   uint32_t program,
                                                   AzRefstr name);

/**
 * Equivalent to the Rust `GlContextPtr::get_uniform_fv()` function.
 */
void az_gl_context_ptr_get_uniform_fv(const AzGlContextPtr *glcontextptr,
                                      uint32_t program,
                                      int32_t location,
                                      AzGLfloatVecRefMut result);

/**
 * Equivalent to the Rust `GlContextPtr::get_uniform_indices()` function.
 */
AzGLuintVec az_gl_context_ptr_get_uniform_indices(const AzGlContextPtr *glcontextptr,
                                                  uint32_t program,
                                                  AzRefstrVecRef names);

/**
 * Equivalent to the Rust `GlContextPtr::get_uniform_iv()` function.
 */
void az_gl_context_ptr_get_uniform_iv(const AzGlContextPtr *glcontextptr,
                                      uint32_t program,
                                      int32_t location,
                                      AzGLintVecRefMut result);

/**
 * Equivalent to the Rust `GlContextPtr::get_uniform_location()` function.
 */
int32_t az_gl_context_ptr_get_uniform_location(const AzGlContextPtr *glcontextptr,
                                               uint32_t program,
                                               AzRefstr name);

/**
 * Equivalent to the Rust `GlContextPtr::get_vertex_attrib_fv()` function.
 */
void az_gl_context_ptr_get_vertex_attrib_fv(const AzGlContextPtr *glcontextptr,
                                            uint32_t index,
                                            uint32_t pname,
                                            AzGLfloatVecRefMut result);

/**
 * Equivalent to the Rust `GlContextPtr::get_vertex_attrib_iv()` function.
 */
void az_gl_context_ptr_get_vertex_attrib_iv(const AzGlContextPtr *glcontextptr,
                                            uint32_t index,
                                            uint32_t pname,
                                            AzGLintVecRefMut result);

/**
 * Equivalent to the Rust `GlContextPtr::get_vertex_attrib_pointer_v()` function.
 */
ptrdiff_t az_gl_context_ptr_get_vertex_attrib_pointer_v(const AzGlContextPtr *glcontextptr,
                                                        uint32_t index,
                                                        uint32_t pname);

/**
 * Equivalent to the Rust `GlContextPtr::hint()` function.
 */
void az_gl_context_ptr_hint(const AzGlContextPtr *glcontextptr,
                            uint32_t param_name,
                            uint32_t param_val);

/**
 * Equivalent to the Rust `GlContextPtr::insert_event_marker_ext()` function.
 */
void az_gl_context_ptr_insert_event_marker_ext(const AzGlContextPtr *glcontextptr,
                                               AzRefstr message);

/**
 * Equivalent to the Rust `GlContextPtr::invalidate_framebuffer()` function.
 */
void az_gl_context_ptr_invalidate_framebuffer(const AzGlContextPtr *glcontextptr,
                                              uint32_t target,
                                              AzGLenumVecRef attachments);

/**
 * Equivalent to the Rust `GlContextPtr::invalidate_sub_framebuffer()` function.
 */
void az_gl_context_ptr_invalidate_sub_framebuffer(const AzGlContextPtr *glcontextptr,
                                                  uint32_t target,
                                                  AzGLenumVecRef attachments,
                                                  int32_t xoffset,
                                                  int32_t yoffset,
                                                  int32_t width,
                                                  int32_t height);

/**
 * Equivalent to the Rust `GlContextPtr::is_enabled()` function.
 */
uint8_t az_gl_context_ptr_is_enabled(const AzGlContextPtr *glcontextptr, uint32_t cap);

/**
 * Equivalent to the Rust `GlContextPtr::is_framebuffer()` function.
 */
uint8_t az_gl_context_ptr_is_framebuffer(const AzGlContextPtr *glcontextptr, uint32_t framebuffer);

/**
 * Equivalent to the Rust `GlContextPtr::is_renderbuffer()` function.
 */
uint8_t az_gl_context_ptr_is_renderbuffer(const AzGlContextPtr *glcontextptr,
                                          uint32_t renderbuffer);

/**
 * Equivalent to the Rust `GlContextPtr::is_shader()` function.
 */
uint8_t az_gl_context_ptr_is_shader(const AzGlContextPtr *glcontextptr, uint32_t shader);

/**
 * Equivalent to the Rust `GlContextPtr::is_texture()` function.
 */
uint8_t az_gl_context_ptr_is_texture(const AzGlContextPtr *glcontextptr, uint32_t texture);

/**
 * Equivalent to the Rust `GlContextPtr::line_width()` function.
 */
void az_gl_context_ptr_line_width(const AzGlContextPtr *glcontextptr, float width);

/**
 * Equivalent to the Rust `GlContextPtr::link_program()` function.
 */
void az_gl_context_ptr_link_program(const AzGlContextPtr *glcontextptr, uint32_t program);

/**
 * Equivalent to the Rust `GlContextPtr::map_buffer()` function.
 */
void *az_gl_context_ptr_map_buffer(const AzGlContextPtr *glcontextptr,
                                   uint32_t target,
                                   uint32_t access);

/**
 * Equivalent to the Rust `GlContextPtr::map_buffer_range()` function.
 */
void *az_gl_context_ptr_map_buffer_range(const AzGlContextPtr *glcontextptr,
                                         uint32_t target,
                                         ptrdiff_t offset,
                                         ptrdiff_t length,
                                         uint32_t access);

/**
 * Equivalent to the Rust `GlContextPtr::pixel_store_i()` function.
 */
void az_gl_context_ptr_pixel_store_i(const AzGlContextPtr *glcontextptr,
                                     uint32_t name,
                                     int32_t param);

/**
 * Equivalent to the Rust `GlContextPtr::polygon_offset()` function.
 */
void az_gl_context_ptr_polygon_offset(const AzGlContextPtr *glcontextptr,
                                      float factor,
                                      float units);

/**
 * Equivalent to the Rust `GlContextPtr::pop_debug_group_khr()` function.
 */
void az_gl_context_ptr_pop_debug_group_khr(const AzGlContextPtr *glcontextptr);

/**
 * Equivalent to the Rust `GlContextPtr::pop_group_marker_ext()` function.
 */
void az_gl_context_ptr_pop_group_marker_ext(const AzGlContextPtr *glcontextptr);

/**
 * Equivalent to the Rust `GlContextPtr::program_binary()` function.
 */
void az_gl_context_ptr_program_binary(const AzGlContextPtr *glcontextptr,
                                      uint32_t program,
                                      uint32_t format,
                                      AzU8VecRef binary);

/**
 * Equivalent to the Rust `GlContextPtr::program_parameter_i()` function.
 */
void az_gl_context_ptr_program_parameter_i(const AzGlContextPtr *glcontextptr,
                                           uint32_t program,
                                           uint32_t pname,
                                           int32_t value);

/**
 * Equivalent to the Rust `GlContextPtr::provoking_vertex_angle()` function.
 */
void az_gl_context_ptr_provoking_vertex_angle(const AzGlContextPtr *glcontextptr, uint32_t mode);

/**
 * Equivalent to the Rust `GlContextPtr::push_debug_group_khr()` function.
 */
void az_gl_context_ptr_push_debug_group_khr(const AzGlContextPtr *glcontextptr,
                                            uint32_t source,
                                            uint32_t id,
                                            AzRefstr message);

/**
 * Equivalent to the Rust `GlContextPtr::push_group_marker_ext()` function.
 */
void az_gl_context_ptr_push_group_marker_ext(const AzGlContextPtr *glcontextptr, AzRefstr message);

/**
 * Equivalent to the Rust `GlContextPtr::query_counter()` function.
 */
void az_gl_context_ptr_query_counter(const AzGlContextPtr *glcontextptr,
                                     uint32_t id,
                                     uint32_t target);

/**
 * Equivalent to the Rust `GlContextPtr::read_buffer()` function.
 */
void az_gl_context_ptr_read_buffer(const AzGlContextPtr *glcontextptr, uint32_t mode);

/**
 * Equivalent to the Rust `GlContextPtr::read_pixels()` function.
 */
AzU8Vec az_gl_context_ptr_read_pixels(const AzGlContextPtr *glcontextptr,
                                      int32_t x,
                                      int32_t y,
                                      int32_t width,
                                      int32_t height,
                                      uint32_t format,
                                      uint32_t pixel_type);

/**
 * Equivalent to the Rust `GlContextPtr::read_pixels_into_buffer()` function.
 */
void az_gl_context_ptr_read_pixels_into_buffer(const AzGlContextPtr *glcontextptr,
                                               int32_t x,
                                               int32_t y,
                                               int32_t width,
                                               int32_t height,
                                               uint32_t format,
                                               uint32_t pixel_type,
                                               AzU8VecRefMut dst_buffer);

/**
 * Equivalent to the Rust `GlContextPtr::read_pixels_into_pbo()` function.
 */
void az_gl_context_ptr_read_pixels_into_pbo(const AzGlContextPtr *glcontextptr,
                                            int32_t x,
                                            int32_t y,
                                            int32_t width,
                                            int32_t height,
                                            uint32_t format,
                                            uint32_t pixel_type);

/**
 * Equivalent to the Rust `GlContextPtr::renderbuffer_storage()` function.
 */
void az_gl_context_ptr_renderbuffer_storage(const AzGlContextPtr *glcontextptr,
                                            uint32_t target,
                                            uint32_t internalformat,
                                            int32_t width,
                                            int32_t height);

/**
 * Equivalent to the Rust `GlContextPtr::sample_coverage()` function.
 */
void az_gl_context_ptr_sample_coverage(const AzGlContextPtr *glcontextptr,
                                       float value,
                                       bool invert);

/**
 * Equivalent to the Rust `GlContextPtr::scissor()` function.
 */
void az_gl_context_ptr_scissor(const AzGlContextPtr *glcontextptr,
                               int32_t x,
                               int32_t y,
                               int32_t width,
                               int32_t height);

/**
 * Equivalent to the Rust `GlContextPtr::set_fence_apple()` function.
 */
void az_gl_context_ptr_set_fence_apple(const AzGlContextPtr *glcontextptr, uint32_t fence);

/**
 * Equivalent to the Rust `GlContextPtr::shader_source()` function.
 */
void az_gl_context_ptr_shader_source(const AzGlContextPtr *glcontextptr,
                                     uint32_t shader,
                                     AzStringVec strings);

/**
 * Equivalent to the Rust `GlContextPtr::stencil_func()` function.
 */
void az_gl_context_ptr_stencil_func(const AzGlContextPtr *glcontextptr,
                                    uint32_t func,
                                    int32_t ref_,
                                    uint32_t mask);

/**
 * Equivalent to the Rust `GlContextPtr::stencil_func_separate()` function.
 */
void az_gl_context_ptr_stencil_func_separate(const AzGlContextPtr *glcontextptr,
                                             uint32_t face,
                                             uint32_t func,
                                             int32_t ref_,
                                             uint32_t mask);

/**
 * Equivalent to the Rust `GlContextPtr::stencil_mask()` function.
 */
void az_gl_context_ptr_stencil_mask(const AzGlContextPtr *glcontextptr, uint32_t mask);

/**
 * Equivalent to the Rust `GlContextPtr::stencil_mask_separate()` function.
 */
void az_gl_context_ptr_stencil_mask_separate(const AzGlContextPtr *glcontextptr,
                                             uint32_t face,
                                             uint32_t mask);

/**
 * Equivalent to the Rust `GlContextPtr::stencil_op()` function.
 */
void az_gl_context_ptr_stencil_op(const AzGlContextPtr *glcontextptr,
                                  uint32_t sfail,
                                  uint32_t dpfail,
                                  uint32_t dppass);

/**
 * Equivalent to the Rust `GlContextPtr::stencil_op_separate()` function.
 */
void az_gl_context_ptr_stencil_op_separate(const AzGlContextPtr *glcontextptr,
                                           uint32_t face,
                                           uint32_t sfail,
                                           uint32_t dpfail,
                                           uint32_t dppass);

/**
 * Equivalent to the Rust `GlContextPtr::test_fence_apple()` function.
 */
void az_gl_context_ptr_test_fence_apple(const AzGlContextPtr *glcontextptr, uint32_t fence);

/**
 * Equivalent to the Rust `GlContextPtr::test_object_apple()` function.
 */
uint8_t az_gl_context_ptr_test_object_apple(const AzGlContextPtr *glcontextptr,
                                            uint32_t object,
                                            uint32_t name);

/**
 * Equivalent to the Rust `GlContextPtr::tex_buffer()` function.
 */
void az_gl_context_ptr_tex_buffer(const AzGlContextPtr *glcontextptr,
                                  uint32_t target,
                                  uint32_t internal_format,
                                  uint32_t buffer);

/**
 * Equivalent to the Rust `GlContextPtr::tex_image_2d()` function.
 */
void az_gl_context_ptr_tex_image_2d(const AzGlContextPtr *glcontextptr,
                                    uint32_t target,
                                    int32_t level,
                                    int32_t internal_format,
                                    int32_t width,
                                    int32_t height,
                                    int32_t border,
                                    uint32_t format,
                                    uint32_t ty,
                                    AzOptionU8VecRef opt_data);

/**
 * Equivalent to the Rust `GlContextPtr::tex_image_3d()` function.
 */
void az_gl_context_ptr_tex_image_3d(const AzGlContextPtr *glcontextptr,
                                    uint32_t target,
                                    int32_t level,
                                    int32_t internal_format,
                                    int32_t width,
                                    int32_t height,
                                    int32_t depth,
                                    int32_t border,
                                    uint32_t format,
                                    uint32_t ty,
                                    AzOptionU8VecRef opt_data);

/**
 * Equivalent to the Rust `GlContextPtr::tex_parameter_f()` function.
 */
void az_gl_context_ptr_tex_parameter_f(const AzGlContextPtr *glcontextptr,
                                       uint32_t target,
                                       uint32_t pname,
                                       float param);

/**
 * Equivalent to the Rust `GlContextPtr::tex_parameter_i()` function.
 */
void az_gl_context_ptr_tex_parameter_i(const AzGlContextPtr *glcontextptr,
                                       uint32_t target,
                                       uint32_t pname,
                                       int32_t param);

/**
 * Equivalent to the Rust `GlContextPtr::tex_storage_2d()` function.
 */
void az_gl_context_ptr_tex_storage_2d(const AzGlContextPtr *glcontextptr,
                                      uint32_t target,
                                      int32_t levels,
                                      uint32_t internal_format,
                                      int32_t width,
                                      int32_t height);

/**
 * Equivalent to the Rust `GlContextPtr::tex_storage_3d()` function.
 */
void az_gl_context_ptr_tex_storage_3d(const AzGlContextPtr *glcontextptr,
                                      uint32_t target,
                                      int32_t levels,
                                      uint32_t internal_format,
                                      int32_t width,
                                      int32_t height,
                                      int32_t depth);

/**
 * Equivalent to the Rust `GlContextPtr::tex_sub_image_2d()` function.
 */
void az_gl_context_ptr_tex_sub_image_2d(const AzGlContextPtr *glcontextptr,
                                        uint32_t target,
                                        int32_t level,
                                        int32_t xoffset,
                                        int32_t yoffset,
                                        int32_t width,
                                        int32_t height,
                                        uint32_t format,
                                        uint32_t ty,
                                        AzU8VecRef data);

/**
 * Equivalent to the Rust `GlContextPtr::tex_sub_image_2d_pbo()` function.
 */
void az_gl_context_ptr_tex_sub_image_2d_pbo(const AzGlContextPtr *glcontextptr,
                                            uint32_t target,
                                            int32_t level,
                                            int32_t xoffset,
                                            int32_t yoffset,
                                            int32_t width,
                                            int32_t height,
                                            uint32_t format,
                                            uint32_t ty,
                                            size_t offset);

/**
 * Equivalent to the Rust `GlContextPtr::tex_sub_image_3d()` function.
 */
void az_gl_context_ptr_tex_sub_image_3d(const AzGlContextPtr *glcontextptr,
                                        uint32_t target,
                                        int32_t level,
                                        int32_t xoffset,
                                        int32_t yoffset,
                                        int32_t zoffset,
                                        int32_t width,
                                        int32_t height,
                                        int32_t depth,
                                        uint32_t format,
                                        uint32_t ty,
                                        AzU8VecRef data);

/**
 * Equivalent to the Rust `GlContextPtr::tex_sub_image_3d_pbo()` function.
 */
void az_gl_context_ptr_tex_sub_image_3d_pbo(const AzGlContextPtr *glcontextptr,
                                            uint32_t target,
                                            int32_t level,
                                            int32_t xoffset,
                                            int32_t yoffset,
                                            int32_t zoffset,
                                            int32_t width,
                                            int32_t height,
                                            int32_t depth,
                                            uint32_t format,
                                            uint32_t ty,
                                            size_t offset);

/**
 * Equivalent to the Rust `GlContextPtr::texture_range_apple()` function.
 */
void az_gl_context_ptr_texture_range_apple(const AzGlContextPtr *glcontextptr,
                                           uint32_t target,
                                           AzU8VecRef data);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_1f()` function.
 */
void az_gl_context_ptr_uniform_1f(const AzGlContextPtr *glcontextptr, int32_t location, float v0);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_1fv()` function.
 */
void az_gl_context_ptr_uniform_1fv(const AzGlContextPtr *glcontextptr,
                                   int32_t location,
                                   AzF32VecRef values);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_1i()` function.
 */
void az_gl_context_ptr_uniform_1i(const AzGlContextPtr *glcontextptr, int32_t location, int32_t v0);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_1iv()` function.
 */
void az_gl_context_ptr_uniform_1iv(const AzGlContextPtr *glcontextptr,
                                   int32_t location,
                                   AzI32VecRef values);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_1ui()` function.
 */
void az_gl_context_ptr_uniform_1ui(const AzGlContextPtr *glcontextptr,
                                   int32_t location,
                                   uint32_t v0);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_2f()` function.
 */
void az_gl_context_ptr_uniform_2f(const AzGlContextPtr *glcontextptr,
                                  int32_t location,
                                  float v0,
                                  float v1);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_2fv()` function.
 */
void az_gl_context_ptr_uniform_2fv(const AzGlContextPtr *glcontextptr,
                                   int32_t location,
                                   AzF32VecRef values);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_2i()` function.
 */
void az_gl_context_ptr_uniform_2i(const AzGlContextPtr *glcontextptr,
                                  int32_t location,
                                  int32_t v0,
                                  int32_t v1);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_2iv()` function.
 */
void az_gl_context_ptr_uniform_2iv(const AzGlContextPtr *glcontextptr,
                                   int32_t location,
                                   AzI32VecRef values);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_2ui()` function.
 */
void az_gl_context_ptr_uniform_2ui(const AzGlContextPtr *glcontextptr,
                                   int32_t location,
                                   uint32_t v0,
                                   uint32_t v1);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_3f()` function.
 */
void az_gl_context_ptr_uniform_3f(const AzGlContextPtr *glcontextptr,
                                  int32_t location,
                                  float v0,
                                  float v1,
                                  float v2);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_3fv()` function.
 */
void az_gl_context_ptr_uniform_3fv(const AzGlContextPtr *glcontextptr,
                                   int32_t location,
                                   AzF32VecRef values);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_3i()` function.
 */
void az_gl_context_ptr_uniform_3i(const AzGlContextPtr *glcontextptr,
                                  int32_t location,
                                  int32_t v0,
                                  int32_t v1,
                                  int32_t v2);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_3iv()` function.
 */
void az_gl_context_ptr_uniform_3iv(const AzGlContextPtr *glcontextptr,
                                   int32_t location,
                                   AzI32VecRef values);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_3ui()` function.
 */
void az_gl_context_ptr_uniform_3ui(const AzGlContextPtr *glcontextptr,
                                   int32_t location,
                                   uint32_t v0,
                                   uint32_t v1,
                                   uint32_t v2);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_4f()` function.
 */
void az_gl_context_ptr_uniform_4f(const AzGlContextPtr *glcontextptr,
                                  int32_t location,
                                  float x,
                                  float y,
                                  float z,
                                  float w);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_4fv()` function.
 */
void az_gl_context_ptr_uniform_4fv(const AzGlContextPtr *glcontextptr,
                                   int32_t location,
                                   AzF32VecRef values);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_4i()` function.
 */
void az_gl_context_ptr_uniform_4i(const AzGlContextPtr *glcontextptr,
                                  int32_t location,
                                  int32_t x,
                                  int32_t y,
                                  int32_t z,
                                  int32_t w);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_4iv()` function.
 */
void az_gl_context_ptr_uniform_4iv(const AzGlContextPtr *glcontextptr,
                                   int32_t location,
                                   AzI32VecRef values);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_4ui()` function.
 */
void az_gl_context_ptr_uniform_4ui(const AzGlContextPtr *glcontextptr,
                                   int32_t location,
                                   uint32_t x,
                                   uint32_t y,
                                   uint32_t z,
                                   uint32_t w);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_block_binding()` function.
 */
void az_gl_context_ptr_uniform_block_binding(const AzGlContextPtr *glcontextptr,
                                             uint32_t program,
                                             uint32_t uniform_block_index,
                                             uint32_t uniform_block_binding);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_matrix_2fv()` function.
 */
void az_gl_context_ptr_uniform_matrix_2fv(const AzGlContextPtr *glcontextptr,
                                          int32_t location,
                                          bool transpose,
                                          AzF32VecRef value);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_matrix_3fv()` function.
 */
void az_gl_context_ptr_uniform_matrix_3fv(const AzGlContextPtr *glcontextptr,
                                          int32_t location,
                                          bool transpose,
                                          AzF32VecRef value);

/**
 * Equivalent to the Rust `GlContextPtr::uniform_matrix_4fv()` function.
 */
void az_gl_context_ptr_uniform_matrix_4fv(const AzGlContextPtr *glcontextptr,
                                          int32_t location,
                                          bool transpose,
                                          AzF32VecRef value);

/**
 * Equivalent to the Rust `GlContextPtr::unmap_buffer()` function.
 */
uint8_t az_gl_context_ptr_unmap_buffer(const AzGlContextPtr *glcontextptr, uint32_t target);

/**
 * Equivalent to the Rust `GlContextPtr::use_program()` function.
 */
void az_gl_context_ptr_use_program(const AzGlContextPtr *glcontextptr, uint32_t program);

/**
 * Equivalent to the Rust `GlContextPtr::validate_program()` function.
 */
void az_gl_context_ptr_validate_program(const AzGlContextPtr *glcontextptr, uint32_t program);

/**
 * Equivalent to the Rust `GlContextPtr::vertex_attrib_4f()` function.
 */
void az_gl_context_ptr_vertex_attrib_4f(const AzGlContextPtr *glcontextptr,
                                        uint32_t index,
                                        float x,
                                        float y,
                                        float z,
                                        float w);

/**
 * Equivalent to the Rust `GlContextPtr::vertex_attrib_divisor()` function.
 */
void az_gl_context_ptr_vertex_attrib_divisor(const AzGlContextPtr *glcontextptr,
                                             uint32_t index,
                                             uint32_t divisor);

/**
 * Equivalent to the Rust `GlContextPtr::vertex_attrib_i_pointer()` function.
 */
void az_gl_context_ptr_vertex_attrib_i_pointer(const AzGlContextPtr *glcontextptr,
                                               uint32_t index,
                                               int32_t size,
                                               uint32_t type_,
                                               int32_t stride,
                                               uint32_t offset);

/**
 * Equivalent to the Rust `GlContextPtr::vertex_attrib_pointer()` function.
 */
void az_gl_context_ptr_vertex_attrib_pointer(const AzGlContextPtr *glcontextptr,
                                             uint32_t index,
                                             int32_t size,
                                             uint32_t type_,
                                             bool normalized,
                                             int32_t stride,
                                             uint32_t offset);

/**
 * Equivalent to the Rust `GlContextPtr::vertex_attrib_pointer_f32()` function.
 */
void az_gl_context_ptr_vertex_attrib_pointer_f32(const AzGlContextPtr *glcontextptr,
                                                 uint32_t index,
                                                 int32_t size,
                                                 bool normalized,
                                                 int32_t stride,
                                                 uint32_t offset);

/**
 * Equivalent to the Rust `GlContextPtr::viewport()` function.
 */
void az_gl_context_ptr_viewport(const AzGlContextPtr *glcontextptr,
                                int32_t x,
                                int32_t y,
                                int32_t width,
                                int32_t height);

/**
 * Equivalent to the Rust `GlContextPtr::wait_sync()` function.
 */
void az_gl_context_ptr_wait_sync(const AzGlContextPtr *glcontextptr,
                                 AzGLsyncPtr sync,
                                 uint32_t flags,
                                 uint64_t timeout);

/**
 * Returns the hidpi factor of the bounds
 */
float az_hidpi_adjusted_bounds_get_hidpi_factor(const AzHidpiAdjustedBounds *hidpiadjustedbounds);

/**
 * Returns the size of the bounds in logical units
 */
AzLogicalSize az_hidpi_adjusted_bounds_get_logical_size(const AzHidpiAdjustedBounds *hidpiadjustedbounds);

/**
 * Returns the size of the bounds in physical units
 */
AzPhysicalSizeU32 az_hidpi_adjusted_bounds_get_physical_size(const AzHidpiAdjustedBounds *hidpiadjustedbounds);

/**
 * Returns a copy of the internal `HidpiAdjustedBounds`
 */
AzHidpiAdjustedBounds az_i_frame_callback_info_get_bounds(const AzIFrameCallbackInfo *iframecallbackinfo);

/**
 * Destructor: Takes ownership of the `IdOrClassVec` pointer and deletes it.
 */
void az_id_or_class_vec_delete(AzIdOrClassVec *object);

/**
 * Creates a new, unique `ImageId`
 */
AzImageId az_image_id_new(void);

/**
 * Clones the object
 */
AzInstantPtr az_instant_ptr_deep_copy(const AzInstantPtr *object);

/**
 * Destructor: Takes ownership of the `InstantPtr` pointer and deletes it.
 */
void az_instant_ptr_delete(AzInstantPtr *object);

/**
 * Equivalent to the Rust `LayoutInfo::window_height_larger_than()` function.
 */
bool az_layout_info_window_height_larger_than(AzLayoutInfo *layoutinfo, float width);

/**
 * Equivalent to the Rust `LayoutInfo::window_height_smaller_than()` function.
 */
bool az_layout_info_window_height_smaller_than(AzLayoutInfo *layoutinfo, float width);

/**
 * Equivalent to the Rust `LayoutInfo::window_width_larger_than()` function.
 */
bool az_layout_info_window_width_larger_than(AzLayoutInfo *layoutinfo, float width);

/**
 * Equivalent to the Rust `LayoutInfo::window_width_smaller_than()` function.
 */
bool az_layout_info_window_width_smaller_than(AzLayoutInfo *layoutinfo, float width);

/**
 * Destructor: Takes ownership of the `LinearColorStopVec` pointer and deletes it.
 */
void az_linear_color_stop_vec_delete(AzLinearColorStopVec *object);

/**
 * Clones the object
 */
AzMonitorHandle az_monitor_handle_deep_copy(const AzMonitorHandle *object);

/**
 * Destructor: Takes ownership of the `MonitorHandle` pointer and deletes it.
 */
void az_monitor_handle_delete(AzMonitorHandle *object);

/**
 * Destructor: Takes ownership of the `MonitorVec` pointer and deletes it.
 */
void az_monitor_vec_delete(AzMonitorVec *object);

/**
 * Destructor: Takes ownership of the `NodeDataInlineCssPropertyVec` pointer and deletes it.
 */
void az_node_data_inline_css_property_vec_delete(AzNodeDataInlineCssPropertyVec *object);

/**
 * Destructor: Takes ownership of the `NodeDataVec` pointer and deletes it.
 */
void az_node_data_vec_delete(AzNodeDataVec *object);

/**
 * Destructor: Takes ownership of the `NodeIdVec` pointer and deletes it.
 */
void az_node_id_vec_delete(AzNodeIdVec *object);

/**
 * Destructor: Takes ownership of the `NodeVec` pointer and deletes it.
 */
void az_node_vec_delete(AzNodeVec *object);

/**
 * Converts the `On` shorthand into a `EventFilter`
 */
AzEventFilter az_on_into_event_filter(AzOn on);

/**
 * Destructor: Takes ownership of the `ParentWithNodeDepthVec` pointer and deletes it.
 */
void az_parent_with_node_depth_vec_delete(AzParentWithNodeDepthVec *object);

/**
 * Destructor: Takes ownership of the `RadialColorStopVec` pointer and deletes it.
 */
void az_radial_color_stop_vec_delete(AzRadialColorStopVec *object);

/**
 * Creates a new `RawImage` by loading the decoded bytes
 */
AzRawImage az_raw_image_new(AzU8Vec decoded_pixels,
                            size_t width,
                            size_t height,
                            AzRawImageFormat data_format);

/**
 * Equivalent to the Rust `RefAny::clone()` function.
 */
AzRefAny az_ref_any_clone(AzRefAny *refany);

/**
 * Destructor: Takes ownership of the `RefAny` pointer and deletes it.
 */
void az_ref_any_delete(AzRefAny *object);

/**
 * Equivalent to the Rust `RefAny::get_type_name()` function.
 */
AzString az_ref_any_get_type_name(const AzRefAny *refany);

/**
 * Equivalent to the Rust `RefAny::is_type()` function.
 */
bool az_ref_any_is_type(const AzRefAny *refany, uint64_t type_id);

/**
 * Creates a new `RefAny` instance whose memory is owned by the rust allocator
 * Equivalent to the Rust `RefAny::new_c()` constructor.
 */
AzRefAny az_ref_any_new_c(const void *ptr,
                          size_t len,
                          uint64_t type_id,
                          AzString type_name,
                          AzRefAnyDestructorType destructor);

/**
 * Equivalent to the Rust `RefCount::can_be_shared()` function.
 */
bool az_ref_count_can_be_shared(const AzRefCount *refcount);

/**
 * Equivalent to the Rust `RefCount::can_be_shared_mut()` function.
 */
bool az_ref_count_can_be_shared_mut(const AzRefCount *refcount);

/**
 * Equivalent to the Rust `RefCount::decrease_ref()` function.
 */
void az_ref_count_decrease_ref(AzRefCount *refcount);

/**
 * Equivalent to the Rust `RefCount::decrease_refmut()` function.
 */
void az_ref_count_decrease_refmut(AzRefCount *refcount);

/**
 * Clones the object
 */
AzRefCount az_ref_count_deep_copy(const AzRefCount *object);

/**
 * Destructor: Takes ownership of the `RefCount` pointer and deletes it.
 */
void az_ref_count_delete(AzRefCount *object);

/**
 * Equivalent to the Rust `RefCount::increase_ref()` function.
 */
void az_ref_count_increase_ref(AzRefCount *refcount);

/**
 * Equivalent to the Rust `RefCount::increase_refmut()` function.
 */
void az_ref_count_increase_refmut(AzRefCount *refcount);

/**
 * Destructor: Takes ownership of the `ScanCodeVec` pointer and deletes it.
 */
void az_scan_code_vec_delete(AzScanCodeVec *object);

/**
 * Destructor: Takes ownership of the `StringPairVec` pointer and deletes it.
 */
void az_string_pair_vec_delete(AzStringPairVec *object);

/**
 * Destructor: Takes ownership of the `StringVec` pointer and deletes it.
 */
void az_string_vec_delete(AzStringVec *object);

/**
 * Destructor: Takes ownership of the `StyleBackgroundContentVec` pointer and deletes it.
 */
void az_style_background_content_vec_delete(AzStyleBackgroundContentVec *object);

/**
 * Destructor: Takes ownership of the `StyleBackgroundPositionVec` pointer and deletes it.
 */
void az_style_background_position_vec_delete(AzStyleBackgroundPositionVec *object);

/**
 * Destructor: Takes ownership of the `StyleBackgroundRepeatVec` pointer and deletes it.
 */
void az_style_background_repeat_vec_delete(AzStyleBackgroundRepeatVec *object);

/**
 * Destructor: Takes ownership of the `StyleBackgroundSizeVec` pointer and deletes it.
 */
void az_style_background_size_vec_delete(AzStyleBackgroundSizeVec *object);

/**
 * Destructor: Takes ownership of the `StyleTransformVec` pointer and deletes it.
 */
void az_style_transform_vec_delete(AzStyleTransformVec *object);

/**
 * Appends an already styled list of DOM nodes to the current `dom.root` - complexity `O(count(dom.dom_nodes))`
 */
void az_styled_dom_append(AzStyledDom *styleddom,
                          AzStyledDom dom);

/**
 * Styles a `Dom` with the given `Css`, returning the `StyledDom` - complexity `O(count(dom_nodes) * count(css_blocks))`: make sure that the `Dom` and the `Css` are as small as possible, use inline CSS if the performance isn't good enough
 */
AzStyledDom az_styled_dom_new(AzDom dom,
                              AzCss css);

/**
 * Returns the number of nodes in the styled DOM
 */
size_t az_styled_dom_node_count(const AzStyledDom *styleddom);

/**
 * Destructor: Takes ownership of the `StyledNodeVec` pointer and deletes it.
 */
void az_styled_node_vec_delete(AzStyledNodeVec *object);

/**
 * Destructor: Takes ownership of the `StylesheetVec` pointer and deletes it.
 */
void az_stylesheet_vec_delete(AzStylesheetVec *object);

/**
 * Clones the object
 */
AzSvg az_svg_deep_copy(const AzSvg *object);

/**
 * Destructor: Takes ownership of the `Svg` pointer and deletes it.
 */
void az_svg_delete(AzSvg *object);

/**
 * Destructor: Takes ownership of the `SvgMultiPolygonVec` pointer and deletes it.
 */
void az_svg_multi_polygon_vec_delete(AzSvgMultiPolygonVec *object);

/**
 * Creates a new `Svg` instance whose memory is owned by the rust allocator
 * Equivalent to the Rust `Svg::parse()` constructor.
 */
AzResultSvgSvgParseError az_svg_parse(AzU8VecRef svg_bytes, AzSvgParseOptions parse_options);

/**
 * Creates a new `SvgParseOptions` instance whose memory is owned by the rust allocator
 * Equivalent to the Rust `SvgParseOptions::default()` constructor.
 */
AzSvgParseOptions az_svg_parse_options_default(void);

/**
 * Destructor: Takes ownership of the `SvgPathElementVec` pointer and deletes it.
 */
void az_svg_path_element_vec_delete(AzSvgPathElementVec *object);

/**
 * Destructor: Takes ownership of the `SvgPathVec` pointer and deletes it.
 */
void az_svg_path_vec_delete(AzSvgPathVec *object);

/**
 * Creates a new `SvgRenderOptions` instance whose memory is owned by the rust allocator
 * Equivalent to the Rust `SvgRenderOptions::default()` constructor.
 */
AzSvgRenderOptions az_svg_render_options_default(void);

/**
 * Destructor: Takes ownership of the `SvgVertexVec` pointer and deletes it.
 */
void az_svg_vertex_vec_delete(AzSvgVertexVec *object);

/**
 * Clones the object
 */
AzSvgXmlNode az_svg_xml_node_deep_copy(const AzSvgXmlNode *object);

/**
 * Destructor: Takes ownership of the `SvgXmlNode` pointer and deletes it.
 */
void az_svg_xml_node_delete(AzSvgXmlNode *object);

/**
 * Destructor: Takes ownership of the `TagIdsToNodeIdsMappingVec` pointer and deletes it.
 */
void az_tag_ids_to_node_ids_mapping_vec_delete(AzTagIdsToNodeIdsMappingVec *object);

/**
 * Destructor: Takes ownership of the `Texture` pointer and deletes it.
 */
void az_texture_delete(AzTexture *object);

/**
 * Default texture flags (not opaque, not a video texture)
 */
AzTextureFlags az_texture_flags_default(void);

/**
 * Destructor: Takes ownership of the `ThreadReceiver` pointer and deletes it.
 */
void az_thread_receiver_delete(AzThreadReceiver *object);

/**
 * Equivalent to the Rust `ThreadReceiver::receive()` function.
 */
AzOptionThreadSendMsg az_thread_receiver_receive(AzThreadReceiver *threadreceiver);

/**
 * Destructor: Takes ownership of the `ThreadSender` pointer and deletes it.
 */
void az_thread_sender_delete(AzThreadSender *object);

/**
 * Equivalent to the Rust `ThreadSender::send()` function.
 */
bool az_thread_sender_send(AzThreadSender *threadsender, AzThreadReceiveMsg msg);

/**
 * Creates a new `TimerId` instance whose memory is owned by the rust allocator
 * Equivalent to the Rust `TimerId::unique()` constructor.
 */
AzTimerId az_timer_id_unique(void);

/**
 * Creates a new `Timer` instance whose memory is owned by the rust allocator
 * Equivalent to the Rust `Timer::new()` constructor.
 */
AzTimer az_timer_new(AzRefAny timer_data,
                     AzTimerCallbackType callback,
                     AzGetSystemTimeFn get_system_time_fn);

/**
 * Equivalent to the Rust `Timer::with_delay()` function.
 */
AzTimer az_timer_with_delay(AzTimer timer, AzDuration delay);

/**
 * Equivalent to the Rust `Timer::with_interval()` function.
 */
AzTimer az_timer_with_interval(AzTimer timer, AzDuration interval);

/**
 * Equivalent to the Rust `Timer::with_timeout()` function.
 */
AzTimer az_timer_with_timeout(AzTimer timer, AzDuration timeout);

/**
 * Destructor: Takes ownership of the `U32Vec` pointer and deletes it.
 */
void az_u32_vec_delete(AzU32Vec *object);

/**
 * Destructor: Takes ownership of the `U8Vec` pointer and deletes it.
 */
void az_u8_vec_delete(AzU8Vec *object);

/**
 * Destructor: Takes ownership of the `VertexAttributeVec` pointer and deletes it.
 */
void az_vertex_attribute_vec_delete(AzVertexAttributeVec *object);

/**
 * Destructor: Takes ownership of the `VideoModeVec` pointer and deletes it.
 */
void az_video_mode_vec_delete(AzVideoModeVec *object);

/**
 * Destructor: Takes ownership of the `VirtualKeyCodeVec` pointer and deletes it.
 */
void az_virtual_key_code_vec_delete(AzVirtualKeyCodeVec *object);

/**
 * Creates a new window configuration with a custom layout callback
 */
AzWindowCreateOptions az_window_create_options_new(AzLayoutCallbackType layout_callback);

/**
 * Creates a default WindowState with an empty layout callback - useful only if you use the Rust `WindowState { .. WindowState::default() }` intialization syntax.
 */
AzWindowState az_window_state_default(void);

/**
 * Creates a new WindowState with default settings and a custom layout callback
 */
AzWindowState az_window_state_new(AzLayoutCallbackType layout_callback);

/**
 * Destructor: Takes ownership of the `XWindowTypeVec` pointer and deletes it.
 */
void az_x_window_type_vec_delete(AzXWindowTypeVec *object);
