//! TODO

use std::collections::HashMap;

use pdf_writer::{writers::Form, Finish, Ref, TextStr};

use crate::{
    annotation::{DualStateAppearance, SimpleAppearance, WidgetAnnotation},
    chunk_container::ChunkContainer,
    geom::Rect,
    serialize::SerializeContext,
    stream::Stream,
};

#[derive(Default)]
pub(crate) struct AcroForm {
    field_refs: HashMap<String, Ref>,
    annotations: HashMap<String, Vec<Ref>>,
    fields: Vec<FieldKind>,
}

impl AcroForm {
    pub(crate) fn register_annotation(&mut self, field_name: String, annotation_ref: Ref) {
        self.annotations
            .entry(field_name)
            .or_insert_with(|| Vec::with_capacity(1))
            .push(annotation_ref);
    }

    pub(crate) fn get_field_ref(&mut self, field_name: &str) -> Option<Ref> {
        self.field_refs.get(field_name).copied()
    }

    pub(crate) fn register_field_ref(&mut self, field_name: String, field_ref: Ref) {
        debug_assert!(!self.field_refs.contains_key(&field_name));
        self.field_refs.insert(field_name, field_ref);
    }

    pub(crate) fn register_field(&mut self, field: FieldKind) {
        self.fields.push(field);
    }

    pub(crate) fn serialize(
        &self,
        sc: &mut SerializeContext,
        chunk_container: &mut ChunkContainer,
        root_ref: Ref,
    ) {
        let mut chunk = sc.new_chunk();
        let mut form = chunk.indirect(root_ref).start::<Form>();

        let fields = self.fields.iter().map(|field| {
            let ref_ = self
                .field_refs
                .get(field.get_name())
                .copied()
                .unwrap_or_else(|| sc.new_ref());
            field.serialize_field(
                sc,
                chunk_container,
                ref_,
                self.annotations.get(field.get_name()).map(|v| v.as_slice()),
            );
            ref_
        });
        form.fields(fields);
        form.finish();

        chunk_container.non_stream.forms = Some((root_ref, chunk));
    }
}

#[allow(missing_docs)]
#[derive(Debug, Clone, Default)]
pub struct FormField<T> {
    name: String,
    kind: T,
}

#[allow(missing_docs)]
impl FormField<kind::PushButton> {
    pub fn push_button(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

    pub fn new_widget(&self, rect: Rect, appearance: Stream) -> WidgetAnnotation<SimpleAppearance> {
        WidgetAnnotation::simple(self.name.clone(), rect, appearance)
    }
}

#[allow(missing_docs)]
impl FormField<kind::Checkbox> {
    pub fn checkbox(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.kind.checked = Some(checked);
    }

    pub fn set_default_checked(&mut self, checked: bool) {
        self.kind.default_checked = Some(checked);
    }

    pub fn new_widget(
        &self,
        rect: Rect,
        off_appearance: Stream,
        on_appearance: Stream,
    ) -> WidgetAnnotation<DualStateAppearance> {
        WidgetAnnotation::dual(
            self.name.clone(),
            rect,
            "Off".to_string(),
            off_appearance,
            "Yes".to_string(),
            on_appearance,
        )
    }
}

#[allow(missing_docs)]
impl FormField<kind::Radio> {
    pub fn radio(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

    pub fn set_value(&mut self, value: String) {
        self.kind.value = Some(value);
    }

    pub fn set_default_value(&mut self, value: String) {
        self.kind.default_value = Some(value);
    }

    pub fn new_widget(
        &self,
        rect: Rect,
        value: String,
        off_appearance: Stream,
        on_appearance: Stream,
    ) -> WidgetAnnotation<DualStateAppearance> {
        WidgetAnnotation::dual(
            self.name.clone(),
            rect,
            "Off".to_string(),
            off_appearance,
            value,
            on_appearance,
        )
    }
}

impl<T: SerializableField> FormField<T> {
    fn serialize_field(
        &self,
        sc: &mut SerializeContext,
        chunk_container: &mut ChunkContainer,
        root_ref: Ref,
        annotations: Option<&[Ref]>,
    ) {
        let mut field = chunk_container.non_stream.fields.form_field(root_ref);

        field.partial_name(TextStr(&self.name));

        self.kind.serialize_field(&mut field);

        if let Some(children) = annotations {
            field.children(children.iter().copied());
        }
    }
}

#[allow(missing_docs)]
pub enum FieldKind {
    PushButton(FormField<kind::PushButton>),
    Checkbox(FormField<kind::Checkbox>),
    Radio(FormField<kind::Radio>),
}

impl FieldKind {
    fn serialize_field(
        &self,
        sc: &mut SerializeContext,
        chunk_container: &mut ChunkContainer,
        root_ref: Ref,
        annotations: Option<&[Ref]>,
    ) {
        match self {
            Self::PushButton(f) => f.serialize_field(sc, chunk_container, root_ref, annotations),
            Self::Checkbox(f) => f.serialize_field(sc, chunk_container, root_ref, annotations),
            Self::Radio(f) => f.serialize_field(sc, chunk_container, root_ref, annotations),
        }
    }

    fn get_name(&self) -> &String {
        match self {
            Self::PushButton(f) => &f.name,
            Self::Checkbox(f) => &f.name,
            Self::Radio(f) => &f.name,
        }
    }
}

trait SerializableField {
    fn serialize_field<'a>(&self, field: &mut pdf_writer::writers::Field<'a>);
}

mod kind {
    use pdf_writer::{
        types::{CheckBoxState, FieldFlags},
        Name,
    };

    use super::SerializableField;

    #[allow(missing_docs)]
    #[derive(Debug, Clone, Default)]
    pub struct PushButton;

    impl SerializableField for PushButton {
        fn serialize_field<'a>(&self, field: &mut pdf_writer::writers::Field<'a>) {
            field
                .field_type(pdf_writer::types::FieldType::Button)
                .field_flags(FieldFlags::PUSHBUTTON);
        }
    }

    #[allow(missing_docs)]
    #[derive(Debug, Clone, Default)]
    pub struct Checkbox {
        pub(super) checked: Option<bool>,
        pub(super) default_checked: Option<bool>,
    }

    impl SerializableField for Checkbox {
        fn serialize_field<'a>(&self, field: &mut pdf_writer::writers::Field<'a>) {
            let value = if self.checked.unwrap_or(false) {
                CheckBoxState::Yes
            } else {
                CheckBoxState::Off
            };
            let default_value = if self.default_checked.unwrap_or(false) {
                CheckBoxState::Yes
            } else {
                CheckBoxState::Off
            };

            field
                .field_type(pdf_writer::types::FieldType::Button)
                .checkbox_value(value)
                .checkbox_default_value(default_value);
        }
    }

    #[allow(missing_docs)]
    #[derive(Debug, Clone, Default)]
    pub struct Radio {
        pub(super) value: Option<String>,
        pub(super) default_value: Option<String>,
    }

    impl SerializableField for Radio {
        fn serialize_field<'a>(&self, field: &mut pdf_writer::writers::Field<'a>) {
            let value = if let Some(value) = &self.value {
                value.as_bytes()
            } else {
                b"Off"
            };
            let default_value = if let Some(value) = &self.default_value {
                value.as_bytes()
            } else {
                b"Off"
            };

            field
                .field_type(pdf_writer::types::FieldType::Button)
                .radio_value(Name(value))
                .radio_default_value(Name(default_value));
        }
    }
}

impl From<FormField<kind::PushButton>> for FieldKind {
    fn from(value: FormField<kind::PushButton>) -> Self {
        Self::PushButton(value)
    }
}

impl From<FormField<kind::Checkbox>> for FieldKind {
    fn from(value: FormField<kind::Checkbox>) -> Self {
        Self::Checkbox(value)
    }
}

impl From<FormField<kind::Radio>> for FieldKind {
    fn from(value: FormField<kind::Radio>) -> Self {
        Self::Radio(value)
    }
}
