use crate::imageproc::definitions::Image;
use crate::imageproc::drawing::Canvas;
use image::{GenericImage, Pixel};
use std::mem::{swap, transmute};

/// Iterates over the coordinates in a line segment using
/// [Bresenham's line drawing algorithm](https://en.wikipedia.org/wiki/Bresenham%27s_line_algorithm).
pub struct BresenhamLineIter {
    dx: f32,
    dy: f32,
    x: i32,
    y: i32,
    error: f32,
    end_x: i32,
    is_steep: bool,
    y_step: i32,
}

impl BresenhamLineIter {
    /// Creates a [`BresenhamLineIter`] which will iterate over the integer coordinates
    /// between `start` and `end`.
    pub fn new(start: (f32, f32), end: (f32, f32)) -> BresenhamLineIter {
        let (mut x0, mut y0) = (start.0, start.1);
        let (mut x1, mut y1) = (end.0, end.1);

        let is_steep = (y1 - y0).abs() > (x1 - x0).abs();
        if is_steep {
            swap(&mut x0, &mut y0);
            swap(&mut x1, &mut y1);
        }

        if x0 > x1 {
            swap(&mut x0, &mut x1);
            swap(&mut y0, &mut y1);
        }

        let dx = x1 - x0;

        BresenhamLineIter {
            dx,
            dy: (y1 - y0).abs(),
            x: x0 as i32,
            y: y0 as i32,
            error: dx / 2f32,
            end_x: x1 as i32,
            is_steep,
            y_step: if y0 < y1 { 1 } else { -1 },
        }
    }
}

impl Iterator for BresenhamLineIter {
    type Item = (i32, i32);

    fn next(&mut self) -> Option<(i32, i32)> {
        if self.x > self.end_x {
            None
        } else {
            let ret = if self.is_steep {
                (self.y, self.x)
            } else {
                (self.x, self.y)
            };

            self.x += 1;
            self.error -= self.dy;
            if self.error < 0f32 {
                self.y += self.y_step;
                self.error += self.dx;
            }

            Some(ret)
        }
    }
}

fn in_bounds<I: GenericImage>((x, y): (i32, i32), image: &I) -> bool {
    x >= 0 && x < image.width() as i32 && y >= 0 && y < image.height() as i32
}

fn clamp_point<I: GenericImage>(p: (f32, f32), image: &I) -> (f32, f32) {
    let x = p.0.clamp(0.0, (image.width() - 1) as f32);
    let y = p.1.clamp(0.0, (image.height() - 1) as f32);
    (x, y)
}

/// Iterates over the image pixels in a line segment using
/// [Bresenham's line drawing algorithm](https://en.wikipedia.org/wiki/Bresenham%27s_line_algorithm).
pub struct BresenhamLinePixelIter<'a, P: Pixel> {
    iter: BresenhamLineIter,
    image: &'a Image<P>,
}

impl<P: Pixel> BresenhamLinePixelIter<'_, P> {
    /// Creates a [`BresenhamLinePixelIter`] which will iterate over
    /// the image pixels with coordinates between `start` and `end`.
    pub fn new(
        image: &Image<P>,
        start: (f32, f32),
        end: (f32, f32),
    ) -> BresenhamLinePixelIter<'_, P> {
        assert!(
            image.width() >= 1 && image.height() >= 1,
            "BresenhamLinePixelIter does not support empty images"
        );
        let iter = BresenhamLineIter::new(clamp_point(start, image), clamp_point(end, image));
        BresenhamLinePixelIter { iter, image }
    }
}

impl<'a, P: Pixel> Iterator for BresenhamLinePixelIter<'a, P> {
    type Item = &'a P;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter
            .find(|&p| in_bounds(p, self.image))
            .map(|(x, y)| self.image.get_pixel(x as u32, y as u32))
    }
}

/// Iterates over the image pixels in a line segment using
/// [Bresenham's line drawing algorithm](https://en.wikipedia.org/wiki/Bresenham%27s_line_algorithm).
pub struct BresenhamLinePixelIterMut<'a, P: Pixel> {
    iter: BresenhamLineIter,
    image: &'a mut Image<P>,
}

impl<P: Pixel> BresenhamLinePixelIterMut<'_, P> {
    /// Creates a [`BresenhamLinePixelIterMut`] which will iterate over
    /// the image pixels with coordinates between `start` and `end`.
    pub fn new(
        image: &mut Image<P>,
        start: (f32, f32),
        end: (f32, f32),
    ) -> BresenhamLinePixelIterMut<'_, P> {
        assert!(
            image.width() >= 1 && image.height() >= 1,
            "BresenhamLinePixelIterMut does not support empty images"
        );
        // The next two assertions are for https://github.com/image-rs/imageproc/issues/281
        assert!(P::CHANNEL_COUNT > 0);
        assert!(
            image.width() < i32::MAX as u32 && image.height() < i32::MAX as u32,
            "Image dimensions are too large"
        );
        let iter = BresenhamLineIter::new(clamp_point(start, image), clamp_point(end, image));
        BresenhamLinePixelIterMut { iter, image }
    }
}

impl<'a, P: Pixel> Iterator for BresenhamLinePixelIterMut<'a, P> {
    type Item = &'a mut P;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter
            .find(|&p| in_bounds(p, self.image))
            .map(|(x, y)| self.image.get_pixel_mut(x as u32, y as u32))
            .map(|p| unsafe { transmute(p) })
    }
}

/// Draws a line segment on an image.
///
/// Draws as much of the line segment between start and end as lies inside the image bounds.
///
/// Uses [Bresenham's line drawing algorithm](https://en.wikipedia.org/wiki/Bresenham%27s_line_algorithm).
#[must_use = "the function does not modify the original image"]
pub fn draw_line_segment<I>(
    image: &I,
    start: (f32, f32),
    end: (f32, f32),
    color: I::Pixel,
) -> Image<I::Pixel>
where
    I: GenericImage,
{
    let mut out = Image::new(image.width(), image.height());
    out.copy_from(image, 0, 0).unwrap();
    draw_line_segment_mut(&mut out, start, end, color);
    out
}

pub fn draw_line_segment_mut<C>(canvas: &mut C, start: (f32, f32), end: (f32, f32), color: C::Pixel)
where
    C: Canvas,
{
    let (width, height) = canvas.dimensions();
    let in_bounds = |x, y| x >= 0 && x < width as i32 && y >= 0 && y < height as i32;

    let line_iterator = BresenhamLineIter::new(start, end);

    for point in line_iterator {
        let x = point.0;
        let y = point.1;

        if in_bounds(x, y) {
            canvas.draw_pixel(x as u32, y as u32, color);
        }
    }
}

/// Draws an antialised line segment on an image.
///
/// Draws as much of the line segment between `start` and `end` as lies inside the image bounds.
///
/// The parameters of blend are (line color, original color, line weight).
/// Consider using [`interpolate()`](crate::pixelops::interpolate) for blend.
///
/// Uses [Xu's line drawing algorithm](https://en.wikipedia.org/wiki/Xiaolin_Wu%27s_line_algorithm).
#[must_use = "the function does not modify the original image"]
pub fn draw_antialiased_line_segment<I, B>(
    image: &I,
    start: (i32, i32),
    end: (i32, i32),
    color: I::Pixel,
    blend: B,
) -> Image<I::Pixel>
where
    I: GenericImage,

    B: Fn(I::Pixel, I::Pixel, f32) -> I::Pixel,
{
    let mut out = Image::new(image.width(), image.height());
    out.copy_from(image, 0, 0).unwrap();
    draw_antialiased_line_segment_mut(&mut out, start, end, color, blend);
    out
}

pub fn draw_antialiased_line_segment_mut<I, B>(
    image: &mut I,
    start: (i32, i32),
    end: (i32, i32),
    color: I::Pixel,
    blend: B,
) where
    I: GenericImage,

    B: Fn(I::Pixel, I::Pixel, f32) -> I::Pixel,
{
    let (mut x0, mut y0) = (start.0, start.1);
    let (mut x1, mut y1) = (end.0, end.1);

    let is_steep = (y1 - y0).abs() > (x1 - x0).abs();

    if is_steep {
        if y0 > y1 {
            swap(&mut x0, &mut x1);
            swap(&mut y0, &mut y1);
        }
        let plotter = Plotter {
            image,
            transform: |x, y| (y, x),
            blend,
        };
        plot_wu_line(plotter, (y0, x0), (y1, x1), color);
    } else {
        if x0 > x1 {
            swap(&mut x0, &mut x1);
            swap(&mut y0, &mut y1);
        }
        let plotter = Plotter {
            image,
            transform: |x, y| (x, y),
            blend,
        };
        plot_wu_line(plotter, (x0, y0), (x1, y1), color);
    };
}

fn plot_wu_line<I, T, B>(
    mut plotter: Plotter<'_, I, T, B>,
    start: (i32, i32),
    end: (i32, i32),
    color: I::Pixel,
) where
    I: GenericImage,

    T: Fn(i32, i32) -> (i32, i32),
    B: Fn(I::Pixel, I::Pixel, f32) -> I::Pixel,
{
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let gradient = dy as f32 / dx as f32;
    let mut fy = start.1 as f32;

    for x in start.0..(end.0 + 1) {
        plotter.plot(x, fy as i32, color, 1.0 - fy.fract());
        plotter.plot(x, fy as i32 + 1, color, fy.fract());
        fy += gradient;
    }
}

struct Plotter<'a, I, T, B>
where
    I: GenericImage,

    T: Fn(i32, i32) -> (i32, i32),
    B: Fn(I::Pixel, I::Pixel, f32) -> I::Pixel,
{
    image: &'a mut I,
    transform: T,
    blend: B,
}

impl<I, T, B> Plotter<'_, I, T, B>
where
    I: GenericImage,

    T: Fn(i32, i32) -> (i32, i32),
    B: Fn(I::Pixel, I::Pixel, f32) -> I::Pixel,
{
    fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && x < self.image.width() as i32 && y >= 0 && y < self.image.height() as i32
    }

    pub fn plot(&mut self, x: i32, y: i32, line_color: I::Pixel, line_weight: f32) {
        let (x_trans, y_trans) = (self.transform)(x, y);
        if self.in_bounds(x_trans, y_trans) {
            let original = self.image.get_pixel(x_trans as u32, y_trans as u32);
            let blended = (self.blend)(line_color, original, line_weight);
            self.image
                .put_pixel(x_trans as u32, y_trans as u32, blended);
        }
    }
}
