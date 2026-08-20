//! PDF forms, allowing you to add interactive widgets to the document,
//! such as text fields, checkboxes, radio buttons, and more.
//!
//! There is only a single form in the entire document, but it can have
//! multiple fields. These fields are organized in a tree structure.
//! Each field can be displayed on the page via one or more annotations.

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
    pub(crate) field_tree: Option<FieldTree>,
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

        if let Some(field_tree) = &self.field_tree {
            let fields = field_tree
                .fields
                .iter()
                .map(|node| node.serialize_node(sc, chunk_container));

            form.fields(fields);
        }

        form.finish();

        chunk_container.non_stream.forms = Some((root_ref, chunk));
    }
}

/// A field tree.
#[derive(Default)]
pub struct FieldTree {
    /// The children of the field tree.
    pub fields: Vec<Node>,
}

/// A field group.
pub struct FieldGroup {
    /// The name of the field group.
    /// The group name must not contain any dot character (`.`).
    pub name: String,
    /// The children of the field group.
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

/// A node in a field tree.
pub enum Node {
    /// A group node.
    Group(FieldGroup),
    /// A leaf node.
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

/// A form field.
///
/// Fields can be created via [`FormField::push_button`],
/// [`FormField::checkbox`], and [`FormField::radio`].
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

impl<T> FormField<T> {
    /// Set the alternative name of the field.
    /// This is used to refer to this field in the user interface,
    /// as well as for accessibility purposes.
    pub fn set_alt_name(&mut self, alt_name: String) {
        self.alt_name = Some(alt_name);
    }

    /// Set the mapping name of the field.
    /// This is used during submission/export.
    pub fn set_mapping_name(&mut self, mapping_name: String) {
        self.mapping_name = Some(mapping_name);
    }

    /// Set whether the field is read-only. Default: `false`.
    pub fn set_read_only(&mut self, read_only: bool) {
        self.flags.set(FieldFlags::READ_ONLY, read_only);
    }

    /// Set whether the field is required. Default: `false`.
    pub fn set_required(&mut self, required: bool) {
        self.flags.set(FieldFlags::REQUIRED, required);
    }

    /// Set whether the field will be exported during submission. Default: `true`.
    pub fn set_export(&mut self, export: bool) {
        self.flags.set(FieldFlags::NO_EXPORT, !export);
    }
}

impl FormField<kind::PushButton> {
    /// Create a push button field.
    /// The field name must not contain any dot character (`.`).
    pub fn push_button(name: String) -> Self {
        Self {
            name,
            flags: FieldFlags::PUSHBUTTON,
            ..Default::default()
        }
    }

    /// Create a widget annotation for the push button field.
    ///
    /// - `rect`: The bounding box of the widget annotation that it should cover on the page.
    /// - `appearance`: The appearance of the widget annotation.
    pub fn new_widget(&self, rect: Rect, appearance: Stream) -> WidgetAnnotation<SimpleAppearance> {
        WidgetAnnotation::simple(rect, appearance)
    }
}

impl FormField<kind::Checkbox> {
    /// Create a checkbox field.
    /// The field name must not contain any dot character (`.`).
    pub fn checkbox(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

    /// Set whether this checkbox is checked.
    pub fn set_checked(&mut self, checked: bool) {
        self.kind.checked = Some(checked);
    }

    /// Set whether this checkbox is checked by default.
    pub fn set_default_checked(&mut self, checked: bool) {
        self.kind.default_checked = Some(checked);
    }

    /// Create a widget annotation for the checkbox field.
    ///
    /// - `rect`: The bounding box of the widget annotation that it should cover on the page.
    /// - `off_appearance`: The appearance of the widget annotation when the checkbox is unchecked.
    /// - `on_appearance`: The appearance of the widget annotation when the checkbox is checked.
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

impl FormField<kind::Radio> {
    /// Create a radio group field.
    /// The field name must not contain any dot character (`.`).
    pub fn radio(name: String) -> Self {
        Self {
            name,
            flags: FieldFlags::RADIO,
            ..Default::default()
        }
    }

    /// Set the value of the radio group.
    /// It should correspond to a value of one of the annotations.
    // TODO: take Option<String> instead?
    pub fn set_value(&mut self, value: String) {
        self.kind.value = Some(value);
    }

    /// Set the default value of the radio group.
    /// It should correspond to a value of one of the annotations.
    // TODO: take Option<String> instead?
    pub fn set_default_value(&mut self, value: String) {
        self.kind.default_value = Some(value);
    }

    /// Set whether to allow unselecting all buttons of this radio group.
    /// Default: true
    pub fn set_allow_toggling_off(&mut self, allow_off: bool) {
        self.flags.set(FieldFlags::NO_TOGGLE_TO_OFF, !allow_off);
    }

    /// Set whether to toggle on all buttons with the same value simultaneously.
    /// Default: false
    pub fn set_radios_in_unison(&mut self, in_unison: bool) {
        self.flags.set(FieldFlags::RADIOS_IN_UNISON, in_unison);
    }

    /// Create a widget annotation for the radio group field.
    ///
    /// - `rect`: The bounding box of the widget annotation that it should cover on the page.
    /// - `value`: The value this widget annotation represents in the radio group.
    /// - `off_appearance`: The appearance of the widget annotation when the radio button is off.
    /// - `on_appearance`: The appearance of the widget annotation when the radio button is on.
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

#[allow(private_bounds)]
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

/// A type-agnostic field.
#[derive(Debug, Clone)]
pub enum FieldKind {
    /// A push button field.
    PushButton(FormField<kind::PushButton>),
    /// A checkbox field.
    Checkbox(FormField<kind::Checkbox>),
    /// A radio group field.
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

/// Field kind structs.
pub mod kind {
    use pdf_writer::{types::CheckBoxState, Name};

    use super::SerializableField;

    /// A push button.
    /// Create a field of this type via [`FormField::push_button`](super::FormField::push_button).
    #[derive(Debug, Clone, Default)]
    pub struct PushButton;

    impl SerializableField for PushButton {
        fn serialize_field<'a>(&self, field: &mut pdf_writer::writers::Field<'a>) {
            field.field_type(pdf_writer::types::FieldType::Button);
        }
    }

    /// A checkbox.
    /// Create a field of this type via [`FormField::checkbox`](super::FormField::checkbox).
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

    /// A radio group.
    /// Create a field of this type via [`FormField::radio`](super::FormField::radio).
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

impl From<FieldGroup> for Node {
    fn from(value: FieldGroup) -> Self {
        Self::Group(value)
    }
}
