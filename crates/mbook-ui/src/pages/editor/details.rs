//! The Details section: a form over the metadata of the book.
//!
//! A field is sent to the book when it loses the focus and after a pause in
//! typing; an error of the book is shown under its field.

use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, div, px,
};

use super::EditorPage;
use super::session::{Control, MetaField, MetaValues};
use crate::shared::ui::status_line;

/// The inputs of the form.
pub(super) struct Fields {
    pub(super) title: Entity<InputState>,
    pub(super) authors: Entity<TextareaState>,
    pub(super) language: Entity<InputState>,
    pub(super) series_name: Entity<InputState>,
    pub(super) series_index: Entity<InputState>,
    pub(super) publisher: Entity<InputState>,
    pub(super) date: Entity<InputState>,
    pub(super) annotation: Entity<TextareaState>,
}

impl Fields {
    /// The inputs, empty until the book is open.
    pub(super) fn new(window: &mut Window, cx: &mut Context<'_, EditorPage>) -> Self {
        let line = |cx: &mut Context<'_, EditorPage>, window: &mut Window| {
            cx.new(|cx| InputState::new(window, cx))
        };
        Self {
            title: line(cx, window),
            authors: cx.new(|cx| TextareaState::new(window, cx).rows(3)),
            language: line(cx, window),
            series_name: line(cx, window),
            series_index: line(cx, window),
            publisher: line(cx, window),
            date: line(cx, window),
            annotation: cx.new(|cx| TextareaState::new(window, cx).rows(6)),
        }
    }

    /// Makes the page hear every input.
    pub(super) fn subscribe(
        &self,
        window: &mut Window,
        cx: &mut Context<'_, EditorPage>,
    ) -> Vec<Subscription> {
        let mut subscriptions = Vec::new();
        for (field, state) in [
            (MetaField::Title, &self.title),
            (MetaField::Language, &self.language),
            (MetaField::Series, &self.series_name),
            (MetaField::Series, &self.series_index),
            (MetaField::Publisher, &self.publisher),
            (MetaField::Date, &self.date),
        ] {
            subscriptions.push(cx.subscribe_in(
                state,
                window,
                move |this, _, event: &InputEvent, window, cx| {
                    this.on_field_event(field, event, window, cx);
                },
            ));
        }
        for (field, state) in [
            (MetaField::Authors, &self.authors),
            (MetaField::Annotation, &self.annotation),
        ] {
            subscriptions.push(cx.subscribe_in(
                state,
                window,
                move |this, _, event: &InputEvent, window, cx| {
                    this.on_field_event(field, event, window, cx);
                },
            ));
        }
        subscriptions
    }
}

impl EditorPage {
    /// Puts the metadata of the book into the fields.
    pub(super) fn fill_fields(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(meta) = self.session.meta() else {
            return;
        };
        let values = MetaValues::of(meta);
        let fields = &self.fields;
        let set = |state: &Entity<InputState>,
                   value: &str,
                   window: &mut Window,
                   cx: &mut Context<'_, Self>| {
            state.update(cx, |state, cx| state.set_value(value, window, cx));
        };
        set(&fields.title, &values.title, window, cx);
        set(&fields.language, &values.language, window, cx);
        set(&fields.series_name, &values.series_name, window, cx);
        set(&fields.series_index, &values.series_index, window, cx);
        set(&fields.publisher, &values.publisher, window, cx);
        set(&fields.date, &values.date, window, cx);
        fields.authors.update(cx, |state, cx| {
            state.set_value(values.authors.as_str(), window, cx);
        });
        fields.annotation.update(cx, |state, cx| {
            state.set_value(values.annotation.as_str(), window, cx);
        });
    }

    /// What the fields hold now.
    pub(super) fn meta_values(&self, cx: &App) -> MetaValues {
        let fields = &self.fields;
        let line = |state: &Entity<InputState>| state.read(cx).value().to_string();
        MetaValues {
            title: line(&fields.title),
            authors: fields.authors.read(cx).value().to_string(),
            language: line(&fields.language),
            series_name: line(&fields.series_name),
            series_index: line(&fields.series_index),
            publisher: line(&fields.publisher),
            date: line(&fields.date),
            annotation: fields.annotation.read(cx).value().to_string(),
        }
    }

    /// The error of a field, drawn under it.
    fn field_error(&self, field: MetaField, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
        let message = self.session.error_text(&Control::Meta(field))?;
        let id: SharedString = format!("error-{field:?}").to_lowercase().into();
        Some(
            status_line(id, message)
                .text_color(cx.theme().danger)
                .text_sm(),
        )
    }

    /// The Details section.
    pub(super) fn render_details(&self, cx: &Context<'_, Self>) -> AnyElement {
        let fields = &self.fields;
        let label =
            |text: &'static str| div().text_sm().font_weight(FontWeight::MEDIUM).child(text);
        let group = |label: gpui_kit::Div, body: gpui_kit::AnyElement| {
            v_flex().gap_1().child(label).child(body)
        };
        v_flex()
            .id("details")
            .role(Role::Form)
            .aria_label("Details")
            .gap_4()
            .p_4()
            .max_w(px(640.0))
            .w_full()
            .child(group(
                label("Title"),
                Input::new(&fields.title)
                    .aria_label("Title")
                    .into_any_element(),
            ))
            .children(self.field_error(MetaField::Title, cx))
            .child(group(
                label("Authors (one per line)"),
                Textarea::new(&fields.authors)
                    .aria_label("Authors (one per line)")
                    .into_any_element(),
            ))
            .children(self.field_error(MetaField::Authors, cx))
            .child(group(
                label("Language (a code such as en or ru)"),
                Input::new(&fields.language)
                    .aria_label("Language (a code such as en or ru)")
                    .into_any_element(),
            ))
            .children(self.field_error(MetaField::Language, cx))
            .child(group(
                label("Series"),
                Input::new(&fields.series_name)
                    .aria_label("Series")
                    .into_any_element(),
            ))
            .child(group(
                label("Number in the series"),
                Input::new(&fields.series_index)
                    .aria_label("Number in the series")
                    .into_any_element(),
            ))
            .children(self.field_error(MetaField::Series, cx))
            .child(group(
                label("Publisher"),
                Input::new(&fields.publisher)
                    .aria_label("Publisher")
                    .into_any_element(),
            ))
            .children(self.field_error(MetaField::Publisher, cx))
            .child(group(
                label("Date"),
                Input::new(&fields.date)
                    .aria_label("Date")
                    .into_any_element(),
            ))
            .children(self.field_error(MetaField::Date, cx))
            .child(group(
                label("Annotation"),
                Textarea::new(&fields.annotation)
                    .aria_label("Annotation")
                    .into_any_element(),
            ))
            .children(self.field_error(MetaField::Annotation, cx))
            .into_any_element()
    }
}
