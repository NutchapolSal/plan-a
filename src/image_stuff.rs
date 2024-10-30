use image::{ImageBuffer, RgbaImage, GrayImage, Luma};
pub use image_new::RgbaImage as RgbaImageNew;



/// convert from image v0.25.2 struct to image v0.24.9 struct
pub fn downgrade_image(image_25: RgbaImageNew) -> RgbaImage {
    ImageBuffer::from_vec(image_25.width(), image_25.height(), image_25.into_raw()).unwrap()
}

pub fn convert_luma_f32_to_u8(image: ImageBuffer<Luma<f32>, Vec<f32>>, max_value: f32) -> GrayImage {
    // Create a new image with u8 pixels
    let (width, height) = image.dimensions();

    // Map each pixel from f32 to u8, scaling and clamping as necessary
    ImageBuffer::from_fn(width, height, |x, y| {
        let Luma([pixel_value]) = image.get_pixel(x, y);
        // Scale f32 to u8 (assuming f32 values are in 0.0..1.0 range)
        let scaled_value = ((pixel_value / max_value) * 255.0) as u8;
        Luma([scaled_value])
    })
}