use crate::imageproc::definitions::Image;
use crate::imageproc::drawing::line::draw_line_segment_mut;
use crate::imageproc::drawing::Canvas;
use image::GenericImage;

/// Draws a cubic Bézier curve on an image.
///
/// Draws as much of the curve as lies within image bounds.
#[must_use = "the function does not modify the original image"]
pub fn draw_cubic_bezier_curve<I>(
    image: &I,
    start: (f32, f32),
    end: (f32, f32),
    control_a: (f32, f32),
    control_b: (f32, f32),
    color: I::Pixel,
) -> Image<I::Pixel>
where
    I: GenericImage,
{
    let mut out = Image::new(image.width(), image.height());
    out.copy_from(image, 0, 0).unwrap();
    draw_cubic_bezier_curve_mut(&mut out, start, end, control_a, control_b, color);
    out
}

pub fn draw_cubic_bezier_curve_mut<C>(
    canvas: &mut C,
    start: (f32, f32),
    end: (f32, f32),
    control_a: (f32, f32),
    control_b: (f32, f32),
    color: C::Pixel,
) where
    C: Canvas,
{
    // Bezier Curve function from: https://pomax.github.io/bezierinfo/#control
    let cubic_bezier_curve = |t: f32| {
        let t2 = t * t;
        let t3 = t2 * t;
        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let mt3 = mt2 * mt;
        let x = (start.0 * mt3)
            + (3.0 * control_a.0 * mt2 * t)
            + (3.0 * control_b.0 * mt * t2)
            + (end.0 * t3);
        let y = (start.1 * mt3)
            + (3.0 * control_a.1 * mt2 * t)
            + (3.0 * control_b.1 * mt * t2)
            + (end.1 * t3);
        (x.round(), y.round()) // round to nearest pixel, to avoid ugly line artifacts
    };

    let distance = |point_a: (f32, f32), point_b: (f32, f32)| {
        ((point_a.0 - point_b.0).powi(2) + (point_a.1 - point_b.1).powi(2)).sqrt()
    };

    // Approximate curve's length by adding distance between control points.
    let curve_length_bound: f32 =
        distance(start, control_a) + distance(control_a, control_b) + distance(control_b, end);

    // Use hyperbola function to give shorter curves a bias in number of line segments.
    let num_segments: i32 = ((curve_length_bound.powi(2) + 800.0).sqrt() / 8.0) as i32;

    // Sample points along the curve and connect them with line segments.
    let t_interval = 1f32 / (num_segments as f32);
    let mut t1 = 0f32;
    for i in 0..num_segments {
        let t2 = (i as f32 + 1.0) * t_interval;
        draw_line_segment_mut(
            canvas,
            cubic_bezier_curve(t1),
            cubic_bezier_curve(t2),
            color,
        );
        t1 = t2;
    }
}
