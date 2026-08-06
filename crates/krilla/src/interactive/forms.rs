//! TODO

use std::collections::HashMap;

use pdf_writer::{types::FieldFlags, writers::Form, Finish, Ref, TextStr};

use crate::{chunk_container::ChunkContainer, serialize::SerializeContext};

#[derive(Default)]
pub(crate) struct AcroForm {
    annotations: HashMap<String, Vec<Ref>>,
    fields: Vec<PushButtonField>,
}

impl AcroForm {
    pub(crate) fn register_annotation(&mut self, field_name: String, annotation_ref: Ref) {
        self.annotations
            .entry(field_name)
            .or_insert_with(|| Vec::with_capacity(1))
            .push(annotation_ref);
    }

    pub(crate) fn register_field(&mut self, field: PushButtonField) {
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
            let ref_ = sc.new_ref();
            field.serialize_field(
                chunk_container,
                ref_,
                self.annotations.get(&field.name).map(|v| v.as_slice()),
            );
            ref_
        });
        form.fields(fields);
        form.finish();

        chunk_container.non_stream.forms = Some((root_ref, chunk));
    }
}

#[allow(missing_docs)]
pub struct PushButtonField {
    pub name: String,
    // pub annotation: WidgetAnnotation,
}

impl PushButtonField {
    fn serialize_field(
        &self,
        chunk_container: &mut ChunkContainer,
        root_ref: Ref,
        annotations: Option<&[Ref]>,
    ) {
        let mut field = chunk_container.non_stream.fields.form_field(root_ref);

        field
            .field_type(pdf_writer::types::FieldType::Button)
            .field_flags(FieldFlags::PUSHBUTTON)
            .partial_name(TextStr(&self.name));

        if let Some(children) = annotations {
            field.children(children.iter().copied());
        }
    }
}
