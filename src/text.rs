use std::{cell::RefCell, collections::HashMap, ops::Range, rc::Rc};

use ab_glyph::{Font, FontArc, FontVec, PxScale, ScaleFont};
use font_kit::family_name::FamilyName;
use font_kit::source::SystemSource;
use lyon::lyon_tessellation::{
    BuffersBuilder, FillOptions, FillVertex, StrokeOptions, StrokeVertex, VertexBuffers,
};
use lyon::tessellation;
use piet::Color;
use piet::{
    kurbo::{Point, Size},
    FontFamily, FontStyle, FontWeight, HitTestPoint, HitTestPosition, LineMetric, Text,
    TextAttribute, TextLayout, TextLayoutBuilder, TextStorage,
};
use wgpu_glyph::{FontId, GlyphBrush, GlyphBrushBuilder, Section};

use crate::context::WgpuRenderContext;
use crate::pipeline::GpuVertex;
use crate::text_pipeline::Instance;

#[derive(Clone)]
pub struct WgpuText {
    source: Rc<RefCell<SystemSource>>,
    fonts: Rc<RefCell<HashMap<FontFamily, (Rc<ab_glyph::FontArc>, FontId)>>>,
    glyphs: Rc<RefCell<HashMap<FontFamily, HashMap<char, Rc<(Vec<[f32; 2]>, Vec<u32>)>>>>>,
    pub(crate) glyph_brush: Rc<RefCell<GlyphBrush<wgpu::DepthStencilState>>>,
    pub(crate) scale: f64,
}

impl WgpuText {
    pub(crate) fn new(device: &wgpu::Device, scale: f64) -> Self {
        Self {
            source: Rc::new(RefCell::new(SystemSource::new())),
            fonts: Rc::new(RefCell::new(HashMap::new())),
            glyphs: Rc::new(RefCell::new(HashMap::new())),
            glyph_brush: Rc::new(RefCell::new(
                GlyphBrushBuilder::using_fonts(vec![])
                    .depth_stencil_state(wgpu::DepthStencilState {
                        format: wgpu::TextureFormat::Depth32Float,
                        depth_write_enabled: true,
                        depth_compare: wgpu::CompareFunction::GreaterEqual,
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    })
                    .build(device, wgpu::TextureFormat::Bgra8Unorm),
            )),
            scale,
        }
    }
}

#[derive(Clone)]
pub struct WgpuTextLayout {
    text: String,
    attrs: Rc<Attributes>,
    instances: Rc<RefCell<Vec<Instance>>>,
    instances_origins: Rc<RefCell<Vec<(f32, f32)>>>,
}

impl WgpuTextLayout {
    pub fn new(text: String) -> Self {
        Self {
            text,
            attrs: Rc::new(Attributes::default()),
            instances: Rc::new(RefCell::new(Vec::new())),
            instances_origins: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn set_attrs(&mut self, attrs: Attributes) {
        self.attrs = Rc::new(attrs);
    }

    pub fn rebuild(&self, ctx: &mut WgpuRenderContext) {
        let mut instances = self.instances.borrow_mut();
        instances.clear();
        let mut instances_origins = self.instances_origins.borrow_mut();
        instances_origins.clear();

        let mut x = 0.0;
        let mut y = 0.0;
        for (index, c) in self.text.chars().enumerate() {
            let font_family = self.attrs.font(index);
            let font_size = self.attrs.size(index) as f32;
            let color = self.attrs.color(index);
            let color = color.as_rgba();
            let color = [
                color.0 as f32,
                color.1 as f32,
                color.2 as f32,
                color.3 as f32,
            ];
            if let Ok(glyph_pos) = ctx.renderer.text_pipeline.cache.get_glyph_pos(
                c,
                font_family,
                font_size,
                &ctx.renderer.device,
                &mut ctx.renderer.staging_belt,
                &mut ctx.encoder.as_mut().unwrap(),
            ) {
                let instance = Instance {
                    origin: [x, y, 0.0],
                    size: [
                        glyph_pos.rect.width() as f32,
                        glyph_pos.rect.height() as f32,
                    ],
                    tex_left_top: [
                        glyph_pos.cache_rect.x0 as f32,
                        glyph_pos.cache_rect.y0 as f32,
                    ],
                    tex_right_bottom: [
                        glyph_pos.cache_rect.x1 as f32,
                        glyph_pos.cache_rect.y1 as f32,
                    ],
                    color,
                };
                instances.push(instance);
                instances_origins.push((x, y));
                x += glyph_pos.rect.width() as f32;
            }
        }
    }

    pub(crate) fn draw_text(&self, ctx: &mut WgpuRenderContext, pos: Point, z: f32) {
        let mut instances = self.instances.borrow_mut();
        let instances_origins = self.instances_origins.borrow();
        for (i, instance) in instances.iter_mut().enumerate() {
            let (x, y) = instances_origins[i];
            instance.origin[0] = x + pos.x as f32;
            instance.origin[1] = y + pos.y as f32;
            instance.origin[2] = z;
        }
        ctx.renderer.text_pipeline.queue(&instances);
    }
}

pub struct WgpuTextLayoutBuilder {
    text: String,
    attrs: Attributes,
}

impl WgpuTextLayoutBuilder {
    pub(crate) fn new(text: impl TextStorage) -> Self {
        Self {
            text: text.as_str().to_string(),
            attrs: Default::default(),
        }
    }

    fn add(&mut self, attr: TextAttribute, range: Range<usize>) {
        self.attrs.add(range, attr);
    }

    pub fn build_with_ctx(self, ctx: &mut WgpuRenderContext) -> WgpuTextLayout {
        let mut text_layout = WgpuTextLayout::new(self.text);
        text_layout.set_attrs(self.attrs);
        text_layout.rebuild(ctx);
        text_layout
    }
}

impl Text for WgpuText {
    type TextLayoutBuilder = WgpuTextLayoutBuilder;
    type TextLayout = WgpuTextLayout;

    fn font_family(&mut self, family_name: &str) -> Option<FontFamily> {
        todo!()
    }

    fn load_font(&mut self, data: &[u8]) -> Result<piet::FontFamily, piet::Error> {
        todo!()
    }

    fn new_text_layout(&mut self, text: impl piet::TextStorage) -> Self::TextLayoutBuilder {
        Self::TextLayoutBuilder::new(text)
    }
}

impl TextLayoutBuilder for WgpuTextLayoutBuilder {
    type Out = WgpuTextLayout;

    fn max_width(self, width: f64) -> Self {
        self
    }

    fn alignment(self, alignment: piet::TextAlignment) -> Self {
        self
    }

    fn default_attribute(mut self, attribute: impl Into<piet::TextAttribute>) -> Self {
        let attribute = attribute.into();
        self.attrs.defaults.set(attribute);
        self
    }

    fn range_attribute(
        mut self,
        range: impl std::ops::RangeBounds<usize>,
        attribute: impl Into<piet::TextAttribute>,
    ) -> Self {
        let range = piet::util::resolve_range(range, self.text.len());
        let attribute = attribute.into();
        self.add(attribute, range);
        self
    }

    fn build(self) -> Result<Self::Out, piet::Error> {
        let mut text_layout = WgpuTextLayout::new(self.text);
        text_layout.set_attrs(self.attrs);
        Ok(text_layout)
    }
}

impl TextLayout for WgpuTextLayout {
    fn size(&self) -> Size {
        if self.instances.borrow().len() == 0 {
            Size::ZERO
        } else {
            let instances = self.instances.borrow();
            let instance_origins = self.instances_origins.borrow();
            let last_instance = &instances[instances.len() - 1];
            let last_instance_origins = &instance_origins[instance_origins.len() - 1];
            let width = last_instance_origins.0 + last_instance.size[0];
            let height = last_instance_origins.1 + last_instance.size[1];
            Size::new(width as f64, height as f64)
        }
    }

    fn trailing_whitespace_width(&self) -> f64 {
        0.0
    }

    fn image_bounds(&self) -> piet::kurbo::Rect {
        Size::ZERO.to_rect()
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn line_text(&self, line_number: usize) -> Option<&str> {
        Some(&self.text)
    }

    fn line_metric(&self, line_number: usize) -> Option<LineMetric> {
        Some(LineMetric::default())
    }

    fn line_count(&self) -> usize {
        0
    }

    fn hit_test_point(&self, point: Point) -> HitTestPoint {
        HitTestPoint::default()
    }

    fn hit_test_text_position(&self, idx: usize) -> HitTestPosition {
        HitTestPosition::default()
    }
}

#[derive(Default)]
struct Attributes {
    defaults: piet::util::LayoutDefaults,
    color: Vec<Span<Color>>,
    font: Vec<Span<FontFamily>>,
    size: Vec<Span<f64>>,
    weight: Option<Span<FontWeight>>,
    style: Option<Span<FontStyle>>,
}

/// during construction, `Span`s represent font attributes that have been applied
/// to ranges of the text; these are combined into coretext font objects as the
/// layout is built.
struct Span<T> {
    payload: T,
    range: Range<usize>,
}

impl<T> Span<T> {
    fn new(payload: T, range: Range<usize>) -> Self {
        Span { payload, range }
    }

    fn range_end(&self) -> usize {
        self.range.end
    }
}

impl Attributes {
    fn add(&mut self, range: Range<usize>, attr: TextAttribute) {
        match attr {
            TextAttribute::TextColor(color) => self.color.push(Span::new(color, range)),
            _ => {}
        }
    }

    fn color(&self, index: usize) -> &Color {
        for r in &self.color {
            if r.range.contains(&index) {
                return &r.payload;
            }
        }
        &self.defaults.fg_color
    }

    fn size(&self, index: usize) -> f64 {
        for r in &self.size {
            if r.range.contains(&index) {
                return r.payload;
            }
        }
        self.defaults.font_size
    }

    fn weight(&self) -> FontWeight {
        self.weight
            .as_ref()
            .map(|w| w.payload)
            .unwrap_or(self.defaults.weight)
    }

    fn italic(&self) -> bool {
        matches!(
            self.style
                .as_ref()
                .map(|t| t.payload)
                .unwrap_or(self.defaults.style),
            FontStyle::Italic
        )
    }

    fn font(&self, index: usize) -> &FontFamily {
        for r in &self.font {
            if r.range.contains(&index) {
                return &r.payload;
            }
        }
        &self.defaults.font
    }
}
