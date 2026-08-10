//! PDF annotations, allowing you to add extra "content" to specific pages.
//!
//! PDF has the concept of annotations, which allow you to associate certain regions of
//! a page with an "annotation". The PDF reference defines many different actions, however,
//! krilla does not and never will expose all of them. As of right now, the only annotations
//! that are supported are "link annotations", which allow you associate a certain region of
//! the page with a link.

use core::f32;

use pdf_writer::types::AnnotationFlags;
use pdf_writer::{Finish, Name, Ref, TextStr};

use crate::chunk_container::ChunkContainer;
use crate::color::Color;
use crate::configure::{PdfVersion, ValidationError};
use crate::error::KrillaResult;
use crate::geom::{Quadrilateral, Rect};
use crate::interactive::action::Action;
use crate::interactive::destination::Destination;
use crate::page::page_root_transform;
use crate::serialize::SerializeContext;
use crate::stream::Stream;
use crate::surface::Location;
use crate::xobject::XObject;

/// An annotation.
pub struct Annotation {
    pub(crate) annotation_type: AnnotationType,
    pub(crate) alt: Option<String>,
    pub(crate) struct_parent: Option<i32>,
    pub(crate) location: Option<Location>,
}

impl Annotation {
    /// Create a new link annotation with some alt text.
    ///
    /// Note that the alt text might be required in some cases, for example
    /// when exporting to PDF/UA.
    pub fn new_link(annotation: LinkAnnotation, alt_text: Option<String>) -> Self {
        Self {
            annotation_type: AnnotationType::Link(annotation),
            alt: alt_text,
            struct_parent: None,
            location: None,
        }
    }

    /// Sets the location of the annotation.
    pub fn with_location(mut self, location: Option<Location>) -> Self {
        self.location = location;
        self
    }
}

impl From<LinkAnnotation> for Annotation {
    fn from(value: LinkAnnotation) -> Self {
        Self {
            annotation_type: AnnotationType::Link(value),
            alt: None,
            struct_parent: None,
            location: None,
        }
    }
}

impl From<WidgetAnnotationKind> for Annotation {
    fn from(value: WidgetAnnotationKind) -> Self {
        Self {
            annotation_type: AnnotationType::Widget(value),
            alt: None,
            struct_parent: None,
            location: None,
        }
    }
}

impl Annotation {
    pub(crate) fn serialize(
        self,
        sc: &mut SerializeContext,
        chunk_container: &mut ChunkContainer,
        root_ref: Ref,
        page_height: f32,
    ) -> KrillaResult<()> {
        let mut annotation =
            self.annotation_type
                .serialize_type(sc, chunk_container, root_ref, page_height)?;

        if let Some(struct_parent) = self.struct_parent {
            annotation.struct_parent(struct_parent);
        }

        if let Some(alt_text) = &self.alt {
            annotation.contents(TextStr(alt_text));
        }

        if self.alt.as_ref().is_none_or(String::is_empty) {
            sc.register_validation_error(ValidationError::MissingAnnotationAltText(self.location));
        }

        annotation.finish();

        Ok(())
    }
}

/// A type of annotation.
pub enum AnnotationType {
    /// A link annotation.
    Link(LinkAnnotation),
    /// A widget annotation.
    Widget(WidgetAnnotationKind),
}

impl AnnotationType {
    fn serialize_type<'a>(
        self,
        sc: &mut SerializeContext,
        chunk_container: &'a mut ChunkContainer,
        root_ref: Ref,
        page_height: f32,
    ) -> KrillaResult<pdf_writer::writers::Annotation<'a>> {
        match self {
            AnnotationType::Link(l) => l.serialize_type(sc, chunk_container, root_ref, page_height),
            AnnotationType::Widget(w) => {
                w.serialize_type(sc, chunk_container, root_ref, page_height)
            }
        }
    }
}

/// An annotation target.
pub enum Target {
    /// A destination within the document.
    Destination(Destination),
    /// An action to be performed.
    Action(Action),
}

/// Border of a link annotation.
pub struct LinkBorder {
    pub(crate) width: f32,
    pub(crate) color: Color,
}

impl LinkBorder {
    /// Create a new link annotation border.
    ///
    /// `width`: The width of the border in pt.
    /// `color`: The color of the border.
    pub fn new(width: f32, color: Color) -> Self {
        Self { width, color }
    }
}

/// A link annotation.
pub struct LinkAnnotation {
    pub(crate) rect: Rect,
    pub(crate) quad_points: Option<Vec<Quadrilateral>>,
    pub(crate) target: Target,
    pub(crate) border: Option<LinkBorder>,
}

impl LinkAnnotation {
    /// Create a new link annotation.
    ///
    /// `rect`: The bounding box of the link annotation that it should cover on the page.
    /// `target`: The target of the link annotation.
    pub fn new(rect: Rect, target: Target) -> Self {
        Self {
            rect,
            quad_points: None,
            target,
            border: None,
        }
    }

    /// Create a new link annotation.
    ///
    /// `target`: The target of the link annotation.
    /// `quad_points`: An array of quadrilaterals that define where the link
    /// annotation should be activated. This is useful if you for example have
    /// a link annotation that is broken to one or multiple lines.
    pub fn new_with_quad_points(quad_points: Vec<Quadrilateral>, target: Target) -> Self {
        assert!(!quad_points.is_empty());

        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        for point in quad_points.iter().flat_map(|q| q.0) {
            min_x = min_x.min(point.x);
            min_y = min_y.min(point.y);
            max_x = max_x.max(point.x);
            max_y = max_y.max(point.y);
        }

        // Expand the bounding box by a little. There is a bug in adobe acrobat
        // that sometimes prevents the quadpoints from being used if the quad
        // points lie exactly on the bounding rectangle.
        const EPSILON: f32 = 0.001;
        let rect = Rect::from_ltrb(
            min_x - EPSILON,
            min_y - EPSILON,
            max_x + EPSILON,
            max_y + EPSILON,
        )
        .unwrap();

        Self {
            rect,
            quad_points: Some(quad_points),
            target,
            border: None,
        }
    }

    /// Set a border for this link annotation. The border will be visible on
    /// screen but not when printed, unless when exporting with PDF/A standard.
    pub fn with_border(self, border: LinkBorder) -> Self {
        Self {
            border: Some(border),
            ..self
        }
    }

    fn serialize_type<'a>(
        self,
        sc: &mut SerializeContext,
        chunk_container: &'a mut ChunkContainer,
        root_ref: Ref,
        page_height: f32,
    ) -> KrillaResult<pdf_writer::writers::Annotation<'a>> {
        let chunk = &mut chunk_container.non_stream.annotations;
        let mut annotation = chunk
            .indirect(root_ref)
            .start::<pdf_writer::writers::Annotation>();

        annotation.subtype(pdf_writer::types::AnnotationType::Link);

        let actual_rect = self
            .rect
            .transform(page_root_transform(page_height))
            .unwrap();
        annotation.rect(actual_rect.to_pdf_rect());
        annotation.border(
            0.0,
            0.0,
            self.border.as_ref().map_or(0.0, |x| x.width),
            None,
        );

        if let Some(border) = &self.border {
            match border.color.to_regular() {
                crate::color::RegularColor::Rgb(rgb) => {
                    let [r, g, b] = rgb.to_pdf_color();
                    annotation.color_rgb(r, g, b);
                }
                crate::color::RegularColor::Cmyk(cmyk) => {
                    let [c, m, y, k] = cmyk.to_pdf_color();
                    annotation.color_cmyk(c, m, y, k);
                }
                crate::color::RegularColor::Luma(gray) => {
                    annotation.color_gray(gray.to_pdf_color());
                }
            }
        }

        if sc.serialize_settings().pdf_version() >= PdfVersion::Pdf16 {
            self.quad_points.as_ref().map(|p| {
                annotation.quad_points(p.iter().flat_map(|q| q.0).flat_map(|p| {
                    let mut p = p.to_tsp();
                    page_root_transform(page_height).to_tsp().map_point(&mut p);
                    [p.x, p.y]
                }))
            });
        }

        match &self.target {
            Target::Destination(destination) => {
                destination.serialize(sc, annotation.insert(Name(b"Dest")))?
            }
            Target::Action(action) => action.serialize(sc, annotation.action())?,
        }

        // Only set the print flag when really necessary (only PDF/A). Don't
        // set it by default, so annotations with color borders will be shown
        // on a screen but not printed.
        // TODO: No need to write the print flag even if it is `None`,
        // only for PDF/A.
        if self.border.is_none()
            || sc
                .serialize_settings()
                .configuration
                .validators()
                .requires_annotation_flags()
        {
            annotation.flags(AnnotationFlags::PRINT);
        }

        Ok(annotation)
    }
}

/// A widget annotation.
#[allow(missing_docs)]
pub struct WidgetAnnotation<T> {
    field_name: String,
    rect: Rect,
    appearance: T,
    action: Option<Action>,
}

#[allow(missing_docs)]
impl<T> WidgetAnnotation<T> {
    pub fn set_action(&mut self, action: Action) {
        self.action = Some(action);
    }
}

#[allow(missing_docs)]
impl WidgetAnnotation<SimpleAppearance> {
    pub(crate) fn simple(field_name: String, rect: Rect, appearance: Stream) -> Self {
        Self {
            field_name,
            rect,
            appearance: SimpleAppearance {
                normal: appearance,
                rollover: None,
                down: None,
            },
            action: None,
        }
    }

    pub fn set_rollover_appearance(&mut self, appearance: Stream) {
        self.appearance.rollover = Some(appearance);
    }
    pub fn set_down_appearance(&mut self, appearance: Stream) {
        self.appearance.down = Some(appearance);
    }
}

#[allow(missing_docs)]
impl WidgetAnnotation<DualStateAppearance> {
    pub(crate) fn dual(
        field_name: String,
        rect: Rect,
        off_state: String,
        off_appearance: Stream,
        on_state: String,
        on_appearance: Stream,
    ) -> Self {
        Self {
            field_name,
            rect,
            appearance: DualStateAppearance {
                off_state,
                off_appearance: SimpleAppearance {
                    normal: off_appearance,
                    rollover: None,
                    down: None,
                },
                on_state,
                on_appearance: SimpleAppearance {
                    normal: on_appearance,
                    rollover: None,
                    down: None,
                },
            },
            action: None,
        }
    }

    pub fn set_off_rollover_appearance(&mut self, appearance: Stream) {
        self.appearance.off_appearance.rollover = Some(appearance);
    }
    pub fn set_on_rollover_appearance(&mut self, appearance: Stream) {
        self.appearance.on_appearance.rollover = Some(appearance);
    }
    pub fn set_off_down_appearance(&mut self, appearance: Stream) {
        self.appearance.off_appearance.down = Some(appearance);
    }
    pub fn set_on_down_appearance(&mut self, appearance: Stream) {
        self.appearance.on_appearance.down = Some(appearance);
    }
}

impl<T: SerializeAppearance> WidgetAnnotation<T> {
    fn serialize_type<'a>(
        self,
        sc: &mut SerializeContext,
        chunk_container: &'a mut ChunkContainer,
        root_ref: Ref,
        page_height: f32,
    ) -> KrillaResult<pdf_writer::writers::Annotation<'a>> {
        let appearance_refs = self.appearance.register_refs(sc, chunk_container);

        let chunk = &mut chunk_container.non_stream.annotations;
        let mut annotation = chunk
            .indirect(root_ref)
            .start::<pdf_writer::writers::Annotation>();
        annotation.subtype(pdf_writer::types::AnnotationType::Widget);

        let actual_rect = self
            .rect
            .transform(page_root_transform(page_height))
            .unwrap();
        annotation.rect(actual_rect.to_pdf_rect());

        if let Some(action) = &self.action {
            action.serialize(sc, annotation.additional_actions().annot_mouse_press())?;
        }

        annotation.flags(AnnotationFlags::PRINT);

        T::serialize_appearance(&mut annotation, appearance_refs);

        let parent_ref = sc
            .global_objects
            .forms
            .get_field_ref(&self.field_name)
            .unwrap_or_else(|| {
                let ref_ = sc.new_ref();
                sc.global_objects
                    .forms
                    .register_field_ref(self.field_name.clone(), ref_);
                ref_
            });
        annotation.parent(parent_ref);

        sc.global_objects
            .forms
            .register_annotation(self.field_name, root_ref);

        Ok(annotation)
    }
}

#[allow(missing_docs)]
pub enum WidgetAnnotationKind {
    Simple(WidgetAnnotation<SimpleAppearance>),
    DualState(WidgetAnnotation<DualStateAppearance>),
}

impl WidgetAnnotationKind {
    fn serialize_type<'a>(
        self,
        sc: &mut SerializeContext,
        chunk_container: &'a mut ChunkContainer,
        root_ref: Ref,
        page_height: f32,
    ) -> KrillaResult<pdf_writer::writers::Annotation<'a>> {
        match self {
            WidgetAnnotationKind::Simple(a) => {
                a.serialize_type(sc, chunk_container, root_ref, page_height)
            }
            WidgetAnnotationKind::DualState(a) => {
                a.serialize_type(sc, chunk_container, root_ref, page_height)
            }
        }
    }
}

pub(crate) trait SerializeAppearance {
    // TODO: refactor, this is the only way I could find to make the borrow checker happy
    type RefsHolder;

    fn register_refs(
        self,
        sc: &mut SerializeContext,
        chunk_container: &mut ChunkContainer,
    ) -> Self::RefsHolder;
    fn serialize_appearance(
        annotation: &mut pdf_writer::writers::Annotation<'_>,
        refs: Self::RefsHolder,
    );

    fn register_appearance_entry(
        sc: &mut SerializeContext,
        chunk_container: &mut ChunkContainer,
        appearance: Stream,
    ) -> Ref {
        let xobject = XObject::new(appearance, false, false, None);
        sc.register_cacheable(chunk_container, xobject)
    }
}

#[allow(missing_docs)]
pub struct SimpleAppearance {
    normal: Stream,
    rollover: Option<Stream>,
    down: Option<Stream>,
}

#[allow(missing_docs)]
pub struct DualStateAppearance {
    off_state: String,
    off_appearance: SimpleAppearance,
    on_state: String,
    on_appearance: SimpleAppearance,
}

impl From<WidgetAnnotation<SimpleAppearance>> for WidgetAnnotationKind {
    fn from(value: WidgetAnnotation<SimpleAppearance>) -> Self {
        Self::Simple(value)
    }
}

impl From<WidgetAnnotation<DualStateAppearance>> for WidgetAnnotationKind {
    fn from(value: WidgetAnnotation<DualStateAppearance>) -> Self {
        Self::DualState(value)
    }
}

impl<T> From<WidgetAnnotation<T>> for Annotation
where
    WidgetAnnotation<T>: Into<WidgetAnnotationKind>,
{
    fn from(value: WidgetAnnotation<T>) -> Self {
        Into::<WidgetAnnotationKind>::into(value).into()
    }
}

impl SerializeAppearance for SimpleAppearance {
    type RefsHolder = (Ref, Option<Ref>, Option<Ref>);

    fn register_refs(
        self,
        sc: &mut SerializeContext,
        chunk_container: &mut ChunkContainer,
    ) -> Self::RefsHolder {
        let normal_ref = Self::register_appearance_entry(sc, chunk_container, self.normal);
        let rollover_ref = self
            .rollover
            .map(|stream| Self::register_appearance_entry(sc, chunk_container, stream));
        let down_ref = self
            .down
            .map(|stream| Self::register_appearance_entry(sc, chunk_container, stream));

        (normal_ref, rollover_ref, down_ref)
    }

    fn serialize_appearance(
        annotation: &mut pdf_writer::writers::Annotation<'_>,
        refs: Self::RefsHolder,
    ) {
        let mut appearance = annotation.appearance();
        appearance.normal().stream(refs.0);
        if let Some(ref_) = refs.1 {
            appearance.rollover().stream(ref_);
        }
        if let Some(ref_) = refs.2 {
            appearance.alternate().stream(ref_);
        }
    }
}

impl SerializeAppearance for DualStateAppearance {
    type RefsHolder = (
        (String, Ref, Option<Ref>, Option<Ref>), // off
        (String, Ref, Option<Ref>, Option<Ref>), // on
    );

    fn register_refs(
        self,
        sc: &mut SerializeContext,
        chunk_container: &mut ChunkContainer,
    ) -> Self::RefsHolder {
        let off_refs = self.off_appearance.register_refs(sc, chunk_container);
        let on_refs = self.on_appearance.register_refs(sc, chunk_container);

        (
            (self.off_state, off_refs.0, off_refs.1, off_refs.2),
            (self.on_state, on_refs.0, on_refs.1, on_refs.2),
        )
    }

    fn serialize_appearance(
        annotation: &mut pdf_writer::writers::Annotation<'_>,
        refs: Self::RefsHolder,
    ) {
        let (off, on) = refs;
        let off_name = Name(off.0.as_bytes());
        let on_name = Name(on.0.as_bytes());

        // TODO: this should come from checkbox/radio group
        annotation.appearance_state(off_name);

        let mut appearance = annotation.appearance();
        appearance
            .normal()
            .streams()
            .pair(off_name, off.1)
            .pair(on_name, on.1);
        if off.2.is_some() || on.2.is_some() {
            let mut dict = appearance.rollover().streams();

            if let Some(ref_) = off.2 {
                dict.pair(off_name, ref_);
            }
            if let Some(ref_) = on.2 {
                dict.pair(on_name, ref_);
            }
        }
        if off.3.is_some() || on.3.is_some() {
            let mut dict = appearance.alternate().streams();

            if let Some(ref_) = off.3 {
                dict.pair(off_name, ref_);
            }
            if let Some(ref_) = on.3 {
                dict.pair(on_name, ref_);
            }
        }
    }
}
