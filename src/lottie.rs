//! Discord's own stickers (Wumpus waving and the rest) are Lottie
//! animations: vector shapes and keyframes in JSON, which the CDN does not
//! turn into pictures. velato reads them and works out each frame, and
//! vello_cpu, which egui already brings, draws it into a picture.

use egui::ColorImage;
use velato::model::{ImageAsset, fixed};
use velato::{Composition, RenderSink, Renderer};
use vello_cpu::kurbo::{self, Affine, Shape};
use vello_cpu::peniko::{self, BlendMode};
use vello_cpu::{Pixmap, RenderContext, Resources};

/// The parsed animation from the sticker's JSON, `None` if velato cannot
/// read it.
pub fn parse(json: &[u8]) -> Option<Composition> {
    Composition::from_slice(json)
        .inspect_err(|error| log::warn!("lottie: {error}"))
        .ok()
}

/// Its frames per second, at most `max_fps`.
pub fn fps(animation: &Composition, max_fps: f64) -> f64 {
    let rate = animation.frame_rate;
    if rate.is_finite() && rate > 0.0 {
        rate.min(max_fps)
    } else {
        max_fps
    }
}

/// Draws each frame `size` pixels square, at `fps`, up to `limit` frames,
/// handing them to `emit` until it returns false.
pub fn render(
    animation: &Composition,
    size: u16,
    fps: f64,
    limit: usize,
    mut emit: impl FnMut(ColorImage) -> bool,
) {
    let (width, height) = (animation.width.max(1) as f64, animation.height.max(1) as f64);
    let scale = f64::from(size) / width.max(height);
    // Centred in the square, as Discord shows a sticker.
    let transform = Affine::translate((
        (f64::from(size) - width * scale) / 2.0,
        (f64::from(size) - height * scale) / 2.0,
    )) * Affine::scale(scale);
    let rate = if animation.frame_rate > 0.0 {
        animation.frame_rate
    } else {
        fps
    };
    let seconds = (animation.frames.end - animation.frames.start).max(0.0) / rate;
    let count = ((seconds * fps).round() as usize).clamp(1, limit);
    let mut renderer = Renderer::new();
    let mut resources = Resources::new();
    let mut context = RenderContext::new(size, size);
    for i in 0..count {
        let frame = animation.frames.start + i as f64 / fps * rate;
        context.reset();
        renderer.append(animation, frame, transform, 1.0, &mut Sink(&mut context));
        context.flush();
        let mut pixmap = Pixmap::new(size, size);
        context.render(&mut pixmap, &mut resources);
        let image = ColorImage::from_rgba_premultiplied(
            [usize::from(size), usize::from(size)],
            pixmap.data_as_u8_slice(),
        );
        if !emit(image) {
            return;
        }
    }
}

/// velato's drawing calls, on a vello_cpu context.
struct Sink<'a>(&'a mut RenderContext);

impl Sink<'_> {
    fn path(shape: &impl Shape) -> kurbo::BezPath {
        shape.to_path(0.1)
    }
}

impl RenderSink for Sink<'_> {
    fn push_layer(
        &mut self,
        blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        shape: &impl Shape,
    ) {
        self.0.set_transform(transform);
        self.0.push_layer(
            Some(&Self::path(shape)),
            Some(blend.into()),
            Some(alpha),
            None,
            None,
        );
    }

    fn push_clip_layer(&mut self, transform: Affine, shape: &impl Shape) {
        self.0.set_transform(transform);
        self.0.push_clip_layer(&Self::path(shape));
    }

    fn pop_layer(&mut self) {
        self.0.pop_layer();
    }

    fn draw(
        &mut self,
        stroke: Option<&fixed::Stroke>,
        transform: Affine,
        brush: &fixed::Brush,
        shape: &impl Shape,
    ) {
        let paint: vello_cpu::PaintType = match brush {
            peniko::Brush::Solid(color) => peniko::Brush::Solid(*color),
            peniko::Brush::Gradient(gradient) => peniko::Brush::Gradient(gradient.clone()),
            // Stickers carry no pictures of their own.
            peniko::Brush::Image(_) => return,
        };
        self.0.set_transform(transform);
        self.0.set_paint(paint);
        let path = Self::path(shape);
        match stroke {
            Some(stroke) => {
                self.0.set_stroke(stroke.clone());
                self.0.stroke_path(&path);
            }
            None => {
                self.0.set_fill_rule(peniko::Fill::NonZero);
                self.0.fill_path(&path);
            }
        }
    }

    fn draw_image(&mut self, _: &ImageAsset, _: Affine, _: f64) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square that slides across, drawn into frames that show it.
    #[test]
    fn a_small_animation_draws() {
        let json = br#"{"v":"5.7.0","fr":30,"ip":0,"op":30,"w":100,"h":100,"layers":[
            {"ty":4,"ind":1,"ip":0,"op":30,"st":0,"ks":{
                "o":{"a":0,"k":100},"r":{"a":0,"k":0},
                "p":{"a":1,"k":[{"t":0,"s":[25,50,0],"i":{"x":[0.5],"y":[0.5]},"o":{"x":[0.5],"y":[0.5]}},{"t":30,"s":[75,50,0]}]},
                "a":{"a":0,"k":[0,0,0]},"s":{"a":0,"k":[100,100,100]}},
            "shapes":[{"ty":"gr","it":[
                {"ty":"rc","d":1,"s":{"a":0,"k":[40,40]},"p":{"a":0,"k":[0,0]},"r":{"a":0,"k":0}},
                {"ty":"fl","c":{"a":0,"k":[0.345,0.396,0.949,1]},"o":{"a":0,"k":100},"r":1},
                {"ty":"tr","p":{"a":0,"k":[0,0]},"a":{"a":0,"k":[0,0]},"s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0},"o":{"a":0,"k":100}}
            ]}]}]}"#;
        let animation = parse(json).expect("parses");
        let mut frames = Vec::new();
        render(&animation, 64, fps(&animation, 30.0), 300, |image| {
            frames.push(image);
            true
        });
        assert_eq!(frames.len(), 30);
        let painted = |image: &ColorImage| image.pixels.iter().filter(|p| p.a() > 0).count();
        assert!(painted(&frames[0]) > 100, "the square shows");
        // It moved: the first frame's left edge is covered, the last's not.
        let at = |image: &ColorImage, x: usize| image.pixels[32 * 64 + x].a();
        assert!(at(&frames[0], 4) > 0);
        assert_eq!(at(&frames[29], 4), 0);
    }
}
