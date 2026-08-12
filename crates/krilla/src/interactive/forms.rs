//! TODO

use pdf_writer::{types::FieldFlags, writers::Form, Finish, Ref, TextStr};

use crate::{
    annotation::{DualStateAppearance, SimpleAppearance, WidgetAnnotation},
    chunk_container::ChunkContainer,
    geom::Rect,
    serialize::SerializeContext,
    stream::Stream,
    tagging::AnnotationIdentifier,
};

#[derive(Default)]
pub(crate) struct AcroForm {
    pub(crate) field_tree: FieldTree,
}

impl AcroForm {
    pub(crate) fn serialize(
        &self,
        sc: &mut SerializeContext,
        chunk_container: &mut ChunkContainer,
        root_ref: Ref,
    ) {
        let mut chunk = sc.new_chunk();
        let mut form = chunk.indirect(root_ref).start::<Form>();

        let fields = self
            .field_tree
            .fields
            .iter()
            .map(|node| node.serialize_node(sc, chunk_container));

        form.fields(fields);
        form.finish();

        chunk_container.non_stream.forms = Some((root_ref, chunk));
    }
}

#[allow(missing_docs)]
#[derive(Default)]
pub struct FieldTree {
    pub fields: Vec<Node>,
}

#[allow(missing_docs)]
pub struct FieldGroup {
    pub name: String,
    pub fields: Vec<Node>,
}

impl FieldGroup {
    fn serialize_group(
        &self,
        sc: &mut SerializeContext,
        chunk_container: &mut ChunkContainer,
    ) -> Ref {
        let ref_ = sc.new_ref();
        let children: Vec<_> = self
            .fields
            .iter()
            .map(|node| node.serialize_node(sc, chunk_container))
            .collect();

        let mut field = chunk_container.non_stream.fields.form_field(ref_);
        field.partial_name(TextStr(&self.name)).children(children);

        ref_
    }
}

#[allow(missing_docs)]
pub enum Node {
    Group(FieldGroup),
    Leaf(FieldKind),
}

impl Node {
    fn serialize_node(
        &self,
        sc: &mut SerializeContext,
        chunk_container: &mut ChunkContainer,
    ) -> Ref {
        match self {
            Self::Group(field_group) => field_group.serialize_group(sc, chunk_container),
            Self::Leaf(field_kind) => field_kind.serialize_field(sc, chunk_container),
        }
    }
}

#[allow(missing_docs)]
#[derive(Debug, Clone, Default)]
pub struct FormField<T> {
    name: String,
    alt_name: Option<String>,
    mapping_name: Option<String>,
    flags: FieldFlags,
    pub(crate) identifier: Option<Ref>,
    pub(crate) annotations: Vec<AnnotationIdentifier>,
    kind: T,
}

#[allow(missing_docs)]
impl<T> FormField<T> {
    pub fn set_alt_name(&mut self, alt_name: String) {
        self.alt_name = Some(alt_name);
    }

    pub fn set_mapping_name(&mut self, mapping_name: String) {
        self.mapping_name = Some(mapping_name);
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        self.flags.set(FieldFlags::READ_ONLY, read_only);
    }

    pub fn set_required(&mut self, required: bool) {
        self.flags.set(FieldFlags::REQUIRED, required);
    }

    pub fn set_export(&mut self, export: bool) {
        self.flags.set(FieldFlags::NO_EXPORT, !export);
    }
}

#[allow(missing_docs)]
impl FormField<kind::PushButton> {
    pub fn push_button(name: String) -> Self {
        Self {
            name,
            flags: FieldFlags::PUSHBUTTON,
            ..Default::default()
        }
    }

    pub fn new_widget(&self, rect: Rect, appearance: Stream) -> WidgetAnnotation<SimpleAppearance> {
        WidgetAnnotation::simple(rect, appearance)
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
            flags: FieldFlags::RADIO,
            ..Default::default()
        }
    }

    pub fn set_value(&mut self, value: String) {
        self.kind.value = Some(value);
    }

    pub fn set_default_value(&mut self, value: String) {
        self.kind.default_value = Some(value);
    }

    pub fn set_allow_toggling_off(&mut self, allow_off: bool) {
        self.flags.set(FieldFlags::NO_TOGGLE_TO_OFF, !allow_off);
    }

    pub fn set_radios_in_unison(&mut self, in_unison: bool) {
        self.flags.set(FieldFlags::RADIOS_IN_UNISON, in_unison);
    }

    pub fn new_widget(
        &self,
        rect: Rect,
        value: String,
        off_appearance: Stream,
        on_appearance: Stream,
    ) -> WidgetAnnotation<DualStateAppearance> {
        WidgetAnnotation::dual(
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
    ) -> Ref {
        let root_ref = self.identifier.unwrap_or_else(|| sc.new_ref());
        let mut field = chunk_container.non_stream.fields.form_field(root_ref);

        field
            .partial_name(TextStr(self.name.rsplit('.').next().unwrap_or(&self.name)))
            .field_flags(self.flags);

        self.kind.serialize_field(&mut field);

        if !self.annotations.is_empty() {
            let annotations = self.annotations.iter().map(|identifier| {
                let page_annotations = sc.page_infos()[identifier.page_index].annotations();
                page_annotations[identifier.annot_index].0
            });
            field.children(annotations);
        }

        root_ref
    }
}

#[allow(missing_docs)]
#[derive(Debug, Clone)]
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
    ) -> Ref {
        match self {
            Self::PushButton(f) => f.serialize_field(sc, chunk_container),
            Self::Checkbox(f) => f.serialize_field(sc, chunk_container),
            Self::Radio(f) => f.serialize_field(sc, chunk_container),
        }
    }
}

pub(crate) trait SerializableField {
    fn serialize_field<'a>(&self, field: &mut pdf_writer::writers::Field<'a>);
}

mod kind {
    use pdf_writer::{types::CheckBoxState, Name};

    use super::SerializableField;

    #[allow(missing_docs)]
    #[derive(Debug, Clone, Default)]
    pub struct PushButton;

    impl SerializableField for PushButton {
        fn serialize_field<'a>(&self, field: &mut pdf_writer::writers::Field<'a>) {
            field.field_type(pdf_writer::types::FieldType::Button);
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

impl<T> From<FormField<T>> for Node
where
    FormField<T>: Into<FieldKind>,
{
    fn from(value: FormField<T>) -> Self {
        Self::Leaf(value.into())
    }
}
