use makepad_widgets::makepad_platform::event::PopupDismissedEvent;
use makepad_widgets::*;

use super::App;

/// Context menu dimensions.
const MENU_WIDTH: f64 = 220.0;
const ITEM_HEIGHT: f64 = 28.0;
const SEPARATOR_HEIGHT: f64 = 9.0;
const MENU_PADDING: f64 = 8.0; // top + bottom (4 each side)

#[derive(Clone, Debug)]
pub(super) enum ContextMenuEntryKind {
    Action(libhavi::ContextMenuAction),
    Separator,
}

#[derive(Clone, Debug)]
pub(super) struct ContextMenuEntry {
    pub(super) widget_id: LiveId,
    pub(super) label: String,
    pub(super) enabled: bool,
    pub(super) kind: ContextMenuEntryKind,
}

static CONTEXT_ITEM_ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_context_item_live_id() -> LiveId {
    LiveId(CONTEXT_ITEM_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

impl App {
    fn ensure_context_templates(&mut self, cx: &mut Cx) {
        if !self.context_item_template_source.is_zero()
            && !self.context_separator_template_source.is_zero()
        {
            return;
        }

        let menu_ref = self.ui.view(cx, ids!(context_menu));
        let (mut item_source, mut separator_source) = (None, None);

        {
            if let Some(menu) = menu_ref.borrow_mut() {
                for (id, child) in menu.children.iter() {
                    if *id == live_id!(context_item_template) {
                        if let Some(view) = child.borrow_mut::<View>() {
                            item_source = Some(view.source.clone());
                        }
                    } else if *id == live_id!(context_separator_template) {
                        if let Some(view) = child.borrow_mut::<View>() {
                            separator_source = Some(view.source.clone());
                        }
                    }
                }
            };
        }

        if let Some(source) = item_source {
            self.context_item_template_source = source;
        }
        if let Some(source) = separator_source {
            self.context_separator_template_source = source;
        }

        {
            if let Some(mut menu) = menu_ref.borrow_mut() {
                menu.children.retain(|(id, _)| {
                    *id != live_id!(context_item_template)
                        && *id != live_id!(context_separator_template)
                });
            };
        }
    }

    fn push_context_separator(&mut self) {
        if self
            .context_menu_entries
            .last()
            .is_some_and(|entry| matches!(entry.kind, ContextMenuEntryKind::Separator))
        {
            return;
        }
        self.context_menu_entries.push(ContextMenuEntry {
            widget_id: next_context_item_live_id(),
            label: String::new(),
            enabled: false,
            kind: ContextMenuEntryKind::Separator,
        });
    }

    fn push_context_action(&mut self, label: String, enabled: bool, kind: ContextMenuEntryKind) {
        self.context_menu_entries.push(ContextMenuEntry {
            widget_id: next_context_item_live_id(),
            label,
            enabled,
            kind,
        });
    }

    fn rebuild_context_menu_entries(&mut self, cx: &mut Cx) {
        self.ensure_context_templates(cx);

        self.context_menu_entries.clear();

        let selection_snapshot = self
            .tabs
            .get(self.active_tab_idx)
            .map(|tab| libhavi::layout::shared_document_selection_for(tab.webview_id).snapshot())
            .unwrap_or_default();
        let editable_context = self.last_context_menu_flags.is_some_and(|flags| {
            flags.contains(libhavi::ContextMenuElementInformationFlags::EditableText)
        });
        let capabilities =
            self.selection_capabilities_for_active_tab(&selection_snapshot, editable_context);

        let menu_items = self
            .active_context_menu
            .as_ref()
            .map(|menu| menu.items().to_vec())
            .unwrap_or_default();
        for item in menu_items {
            match item {
                libhavi::ContextMenuItem::Item {
                    label,
                    action,
                    enabled,
                } => {
                    let enabled = match action {
                        libhavi::ContextMenuAction::Copy => capabilities.can_copy,
                        libhavi::ContextMenuAction::Cut => capabilities.can_cut,
                        libhavi::ContextMenuAction::Paste => capabilities.can_paste,
                        libhavi::ContextMenuAction::SelectAll => capabilities.can_select_all,
                        _ => enabled,
                    };
                    self.push_context_action(label, enabled, ContextMenuEntryKind::Action(action));
                },
                libhavi::ContextMenuItem::Separator => {
                    self.push_context_separator();
                },
            }
        }
        while self
            .context_menu_entries
            .last()
            .is_some_and(|entry| matches!(entry.kind, ContextMenuEntryKind::Separator))
        {
            self.context_menu_entries.pop();
        }

        if self.context_menu_entries.is_empty() {
            self.push_context_action(
                "No actions".to_string(),
                false,
                ContextMenuEntryKind::Action(libhavi::ContextMenuAction::Copy),
            );
        }

        let item_template = self.context_item_template_source.clone();
        let separator_template = self.context_separator_template_source.clone();
        let mut children: Vec<(LiveId, WidgetRef)> =
            Vec::with_capacity(self.context_menu_entries.len());

        for entry in &self.context_menu_entries {
            let widget = match entry.kind {
                ContextMenuEntryKind::Separator => cx.with_vm(|vm| {
                    let template_val: ScriptValue = separator_template.as_object().into();
                    WidgetRef::script_from_value(vm, template_val)
                }),
                _ => cx.with_vm(|vm| {
                    let template_val: ScriptValue = item_template.as_object().into();
                    WidgetRef::script_from_value(vm, template_val)
                }),
            };

            if !matches!(entry.kind, ContextMenuEntryKind::Separator) {
                widget
                    .button(cx, ids!(context_item_button))
                    .set_text(cx, &entry.label);
                widget
                    .button(cx, ids!(context_item_button))
                    .set_enabled(cx, entry.enabled);
            }

            children.push((entry.widget_id, widget));
        }

        if let Some(mut menu) = self.ui.view(cx, ids!(context_menu)).borrow_mut() {
            menu.children.clear();
            menu.children.extend(children);
        }
    }

    fn context_menu_height(&self) -> f64 {
        let body_height = self
            .context_menu_entries
            .iter()
            .map(|entry| {
                if matches!(entry.kind, ContextMenuEntryKind::Separator) {
                    SEPARATOR_HEIGHT
                } else {
                    ITEM_HEIGHT
                }
            })
            .sum::<f64>();
        MENU_PADDING + body_height
    }

    /// Show the context menu at the right-click position as a popup window.
    pub(super) fn show_context_menu(&mut self, cx: &mut Cx) {
        self.rebuild_context_menu_entries(cx);

        if let Some(tab) = self.tabs.get(self.active_tab_idx) {
            let selection = libhavi::layout::shared_document_selection_for(tab.webview_id).snapshot();
            let editable = self.last_context_menu_flags.is_some_and(|flags| {
                flags.contains(libhavi::ContextMenuElementInformationFlags::EditableText)
            });
            let capabilities = self.selection_capabilities_for_active_tab(&selection, editable);
            let visible_actions: Vec<&str> = self
                .context_menu_entries
                .iter()
                .filter_map(|entry| {
                    if matches!(entry.kind, ContextMenuEntryKind::Separator) {
                        None
                    } else {
                        Some(entry.label.as_str())
                    }
                })
                .collect();
            ::log::trace!(
                "[havishell] context menu rev={} rects={} caps={} actions={:?}",
                selection.revision,
                selection.rects.len(),
                capabilities.summary(),
                visible_actions
            );
        }

        // Take old handles but keep them alive until after the new popup is
        // allocated. This prevents the pool allocator from reusing the freed
        // slot and bumping its generation before the deferred CloseWindow op
        // (which still references the old generation) is processed.
        let _old_pass = self.context_popup_pass.take();
        let mut old_window = self.context_popup_window.take();

        let menu_height = self.context_menu_height();

        // Position in parent-client coordinates (from FingerDown abs).
        let parent_window_id = CxWindowPool::id_zero();
        let position = dvec2(self.context_menu_pos.x, self.context_menu_pos.y);
        let size = dvec2(MENU_WIDTH, menu_height);

        let window = WindowHandle::new_popup(cx, parent_window_id, position, size);
        let pass = DrawPass::new(cx);
        pass.set_window_clear_color(cx, vec4(1.0, 1.0, 1.0, 1.0));
        window.set_pass(cx, &pass);

        self.context_popup_window = Some(window);
        self.context_popup_pass = Some(pass);

        // Now safe to close the old popup — new one has a different pool slot.
        if let Some(ref mut w) = old_window {
            w.close(cx);
        }

        // The context_menu View stays invisible in the main window tree.
        // It is drawn only into the popup pass by draw_context_menu_popup().
        cx.redraw_all();
    }

    pub(super) fn hide_context_menu(&mut self, cx: &mut Cx) {
        self.active_context_menu.take();
        self.context_menu_entries.clear();
        self.last_context_menu_flags = None;
        self.close_context_popup(cx);
        cx.redraw_all();
    }

    /// Close the popup window and clean up state.
    fn close_context_popup(&mut self, cx: &mut Cx) {
        self.ui.view(cx, ids!(context_menu)).set_visible(cx, false);
        if let Some(mut window) = self.context_popup_window.take() {
            window.close(cx);
        }
        self.context_popup_pass = None;
    }

    /// Handle PopupDismissed event from the framework.
    ///
    /// The framework sends this as a notification — the popup is still open.
    /// We must explicitly close it.
    pub(super) fn handle_popup_dismissed(&mut self, cx: &mut Cx, event: &PopupDismissedEvent) {
        if let Some(ref window) = self.context_popup_window {
            if window.window_id() == event.window_id {
                self.hide_context_menu(cx);
            }
        }
    }

    /// Draw context menu contents into the popup pass. Called during draw events.
    ///
    /// The context_menu View stays invisible in the main window tree to avoid
    /// rendering it twice (once in the overlay, once in the popup). We
    /// temporarily set it visible here so `draw_all` produces output, then
    /// restore invisibility before the main window pass draws.
    pub(super) fn draw_context_menu_popup(&mut self, cx: &mut Cx2d) {
        let Some(ref pass) = self.context_popup_pass else {
            return;
        };

        let draw_list = self
            .context_popup_draw_list
            .get_or_insert_with(|| DrawList2d::new(cx));

        cx.begin_pass(pass, None);
        draw_list.begin_always(cx);

        let size = cx.current_pass_size();
        cx.begin_root_turtle(size, Layout::flow_down());

        let menu = self.ui.view(cx, ids!(context_menu));
        menu.set_visible(cx, true);
        menu.draw_all(cx, &mut Scope::empty());
        menu.set_visible(cx, false);

        cx.end_pass_sized_turtle();
        draw_list.end(cx);
        cx.end_pass(pass);
    }

    pub(super) fn handle_context_menu_actions(
        &mut self,
        cx: &mut Cx,
        actions: &Actions,
    ) {
        let clicked_entry_id = {
            let mut clicked = None;
            if let Some(menu) = self.ui.view(cx, ids!(context_menu)).borrow_mut() {
                for (child_id, child_widget) in menu.children.iter() {
                    if child_widget
                        .button(cx, ids!(context_item_button))
                        .clicked(actions)
                    {
                        clicked = Some(*child_id);
                        break;
                    }
                }
            }
            clicked
        };

        let Some(clicked_entry_id) = clicked_entry_id else {
            return;
        };

        let Some(entry) = self
            .context_menu_entries
            .iter()
            .find(|entry| entry.widget_id == clicked_entry_id)
            .cloned()
        else {
            return;
        };

        if !entry.enabled {
            return;
        }

        match entry.kind {
            ContextMenuEntryKind::Action(action) => {
                self.select_context_menu_action(cx, action);
            },
            ContextMenuEntryKind::Separator => {},
        }
    }

    /// Select a context menu action and close the menu.
    pub(super) fn select_context_menu_action(
        &mut self,
        cx: &mut Cx,
        action: libhavi::ContextMenuAction,
    ) {
        if let Some(menu) = self.active_context_menu.take() {
            menu.select(action);
        }
        self.context_menu_entries.clear();
        self.last_context_menu_flags = None;
        self.close_context_popup(cx);
        cx.redraw_all();
    }
}
