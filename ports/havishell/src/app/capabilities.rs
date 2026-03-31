use super::*;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SelectionCapabilities {
    pub(super) has_selection: bool,
    pub(super) is_editable_context: bool,
    pub(super) has_clipboard_text: bool,
    pub(super) document_has_selectable_text: bool,
    pub(super) can_copy: bool,
    pub(super) can_cut: bool,
    pub(super) can_paste: bool,
    pub(super) can_select_all: bool,
}

impl SelectionCapabilities {
    pub(super) fn from_inputs(
        has_selection: bool,
        is_editable_context: bool,
        has_clipboard_text: bool,
        document_has_selectable_text: bool,
    ) -> Self {
        let can_copy = has_selection;
        let can_cut = has_selection && is_editable_context;
        let can_paste = is_editable_context && has_clipboard_text;
        let can_select_all = is_editable_context || document_has_selectable_text;
        Self {
            has_selection,
            is_editable_context,
            has_clipboard_text,
            document_has_selectable_text,
            can_copy,
            can_cut,
            can_paste,
            can_select_all,
        }
    }

    pub(super) fn summary(self) -> String {
        format!(
            "sel={} editable={} clip={} doc_selectable={} copy={} cut={} paste={} select_all={}",
            self.has_selection,
            self.is_editable_context,
            self.has_clipboard_text,
            self.document_has_selectable_text,
            self.can_copy,
            self.can_cut,
            self.can_paste,
            self.can_select_all
        )
    }
}

impl App {
    pub(super) fn selection_capabilities_for_active_tab(
        &self,
        selection: &libhavi::layout::DocumentSelectionSnapshot,
        is_editable_context: bool,
    ) -> SelectionCapabilities {
        let has_selection = !selection.rects.is_empty() || !selection.text.is_empty();
        let has_clipboard_text = self
            .clipboard_state
            .as_ref()
            .is_some_and(|state| state.has_clipboard_text());
        let document_has_selectable_text = !self.tabs.is_empty();
        SelectionCapabilities::from_inputs(
            has_selection,
            is_editable_context,
            has_clipboard_text,
            document_has_selectable_text,
        )
    }
}
