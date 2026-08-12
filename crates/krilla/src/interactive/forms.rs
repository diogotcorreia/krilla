//! TODO

use std::{
    collections::{BTreeSet, HashMap},
    iter::Peekable,
};

use pdf_writer::{types::FieldFlags, writers::Form, Finish, Ref, TextStr};

use crate::{
    annotation::{DualStateAppearance, SimpleAppearance, WidgetAnnotation},
    chunk_container::ChunkContainer,
    geom::Rect,
    resource::ResourceDictionaryBuilder,
    serialize::SerializeContext,
    stream::Stream,
};

pub(crate) struct AcroForm {
    field_refs: HashMap<String, Ref>,
    annotations: HashMap<String, Vec<Ref>>,
    fields: BTreeSet<FieldKind>,
    rd_builder: ResourceDictionaryBuilder,
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
        self.fields.insert(field);
    }

    pub(crate) fn serialize(
        &self,
        sc: &mut SerializeContext,
        chunk_container: &mut ChunkContainer,
        root_ref: Ref,
    ) {
        let mut chunk = sc.new_chunk();
        let mut form = chunk.indirect(root_ref).start::<Form>();

        let fields = self.serialize_field_tree(
            sc,
            chunk_container,
            None,
            &[],
            &mut self.fields.iter().peekable(),
        );

        form.fields(fields);
        form.finish();

        chunk_container.non_stream.forms = Some((root_ref, chunk));
    }

    /// Turn a flat set of fields into a tree structure, as required by
    /// the PDF specification. If a field name contains the dot (`.`)
    /// separator, the intermediate fields are created automatically.
    /// This function returns the references to the children of a given
    /// level of the tree (if `parent` is empty, the returned refs are the
    /// top-level fields, to be added to the AcroForm).
    /// It is a logic error for a provided field to also be an intermediate
    /// field (e.g., if foo.bar exists, foo cannot exist as well), and it might
    /// result in an invalid PDF.
    fn serialize_field_tree<'a>(
        &self,
        sc: &mut SerializeContext,
        chunk_container: &mut ChunkContainer,
        parent_ref: Option<Ref>,
        parent: &[&'a str],
        iter: &mut Peekable<impl Iterator<Item = &'a FieldKind>>,
    ) -> Vec<Ref> {
        let mut result = Vec::new();
        loop {
            let Some(field) = iter.peek() else {
                return result;
            };

            let path: Vec<_> = field.get_name().split('.').collect();
            if parent.len() >= path.len() || parent != &path[..parent.len()] {
                return result;
            }

            if parent != &path[..(path.len() - 1)] {
                let ref_ = sc.new_ref();
                let children = self.serialize_field_tree(
                    sc,
                    chunk_container,
                    Some(ref_),
                    &path[..(parent.len() + 1)],
                    iter,
                );
                let mut field = chunk_container.non_stream.fields.form_field(ref_);
                if let Some(parent_ref) = parent_ref {
                    field.parent(parent_ref);
                }
                field
                    .children(children)
                    .partial_name(TextStr(&path[parent.len()]));
                result.push(ref_);
            } else {
                let field = iter.next().unwrap(); // peeked before, so we know it exists
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
                result.push(ref_);
            }
        }
    }
}

impl Default for AcroForm {
    fn default() -> Self {
        Self {
            field_refs: Default::default(),
            annotations: Default::default(),
            fields: Default::default(),
            rd_builder: ResourceDictionaryBuilder::new(),
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
            self.name.clone(),
            rect,
            "Off".to_string(),
            off_appearance,
            value,
            on_appearance,
        )
    }
}

#[allow(missing_docs)]
impl FormField<kind::Text> {
    pub fn text(name: String) -> Self {
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

    pub fn new_widget(&self, rect: Rect, appearance: Stream) -> WidgetAnnotation<SimpleAppearance> {
        WidgetAnnotation::simple(self.name.clone(), rect, appearance)
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

        field
            .partial_name(TextStr(self.name.rsplit('.').next().unwrap_or(&self.name)))
            .field_flags(self.flags);

        self.kind.serialize_field(&mut field);

        if let Some(children) = annotations {
            field.children(children.iter().copied());
        }
    }
}

#[allow(missing_docs)]
#[derive(Debug, Clone)]
pub enum FieldKind {
    PushButton(FormField<kind::PushButton>),
    Checkbox(FormField<kind::Checkbox>),
    Radio(FormField<kind::Radio>),
    Text(FormField<kind::Text>),
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
            Self::Text(f) => f.serialize_field(sc, chunk_container, root_ref, annotations),
        }
    }

    fn get_name(&self) -> &String {
        match self {
            Self::PushButton(f) => &f.name,
            Self::Checkbox(f) => &f.name,
            Self::Radio(f) => &f.name,
            Self::Text(f) => &f.name,
        }
    }
}

impl Ord for FieldKind {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.get_name().cmp(other.get_name())
    }
}

impl PartialOrd for FieldKind {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for FieldKind {
    fn eq(&self, other: &Self) -> bool {
        self.get_name() == other.get_name()
    }
}

impl Eq for FieldKind {}

pub(crate) trait SerializableField {
    fn serialize_field<'a>(&self, field: &mut pdf_writer::writers::Field<'a>);
}

mod kind {
    use pdf_writer::{types::CheckBoxState, Name, TextStr};

    use crate::forms::variable_text::VariableAppearance;

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

    #[allow(missing_docs)]
    #[derive(Debug, Clone, Default)]
    pub struct Text {
        pub(super) appearance: Option<VariableAppearance>,
        pub(super) value: Option<String>,
        pub(super) default_value: Option<String>,
    }

    impl SerializableField for Text {
        fn serialize_field<'a>(&self, field: &mut pdf_writer::writers::Field<'a>) {
            field.field_type(pdf_writer::types::FieldType::Text);
            if let Some(value) = &self.value {
                field.text_value(TextStr(value));
            }
            if let Some(value) = &self.default_value {
                field.text_default_value(TextStr(value));
            }
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

impl From<FormField<kind::Text>> for FieldKind {
    fn from(value: FormField<kind::Text>) -> Self {
        Self::Text(value)
    }
}

mod variable_text {
    use pdf_writer::Buf;

    use crate::{
        chunk_container::ChunkContainer, serialize::SerializeContext, text::Font, util::NameExt,
    };

    #[derive(Debug, Clone)]
    pub struct VariableAppearance {
        pub font: Font,
        pub font_size: f32,
    }

    impl VariableAppearance {
        pub(super) fn serialize(
            &self,
            sc: &mut SerializeContext,
            chunk_container: &mut ChunkContainer,
        ) -> Buf {
            // TODO: somehow register the font
            let font_name: String = todo!();

            let mut content = sc.new_content();
            content.set_font(font_name.to_pdf_name(), self.font_size);

            content.finish()
        }
    }
}
