use crate::error::{XCapError, XCapResult};
use image::RgbaImage;
use std::env;
use xcb::x::ScreenBuf;
use xcb::{
    Connection,
    x::{Drawable, GetImage, ImageFormat, ImageOrder, Window},
};

fn get_pixel8_rgba(
    bytes: &[u8],
    x: u32,
    y: u32,
    width: u32,
    bits_per_pixel: u32,
    bit_order: ImageOrder,
) -> (u8, u8, u8, u8) {
    let index = ((y * width + x) * bits_per_pixel / 8) as usize;

    let pixel = if bit_order == ImageOrder::LsbFirst {
        bytes[index]
    } else {
        bytes[index] & (7 << 4) | (bytes[index] >> 4)
    };

    let r = (pixel >> 6) as f32 / 3.0 * 255.0;
    let g = ((pixel >> 2) & 7) as f32 / 7.0 * 255.0;
    let b = (pixel & 3) as f32 / 3.0 * 255.0;

    (r as u8, g as u8, b as u8, 255)
}

fn get_pixel16_rgba(
    bytes: &[u8],
    x: u32,
    y: u32,
    width: u32,
    bits_per_pixel: u32,
    bit_order: ImageOrder,
) -> (u8, u8, u8, u8) {
    let index = ((y * width + x) * bits_per_pixel / 8) as usize;

    let pixel = if bit_order == ImageOrder::LsbFirst {
        bytes[index] as u16 | ((bytes[index + 1] as u16) << 8)
    } else {
        ((bytes[index] as u16) << 8) | bytes[index + 1] as u16
    };

    let r = (pixel >> 11) as f32 / 31.0 * 255.0;
    let g = ((pixel >> 5) & 63) as f32 / 63.0 * 255.0;
    let b = (pixel & 31) as f32 / 31.0 * 255.0;

    (r as u8, g as u8, b as u8, 255)
}

fn get_pixel24_32_rgba(
    bytes: &[u8],
    x: u32,
    y: u32,
    width: u32,
    bits_per_pixel: u32,
    bit_order: ImageOrder,
) -> (u8, u8, u8, u8) {
    let index = ((y * width + x) * bits_per_pixel / 8) as usize;

    if bit_order == ImageOrder::LsbFirst {
        (bytes[index + 2], bytes[index + 1], bytes[index], 255)
    } else {
        (bytes[index], bytes[index + 1], bytes[index + 2], 255)
    }
}

pub fn xorg_capture(
    window: Window,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> XCapResult<RgbaImage> {
    let (conn, _) = Connection::connect(None)?;

    let setup = conn.get_setup();

    let get_image_cookie = conn.send_request(&GetImage {
        format: ImageFormat::ZPixmap,
        drawable: Drawable::Window(window),
        x: x as i16,
        y: y as i16,
        width: width as u16,
        height: height as u16,
        plane_mask: u32::MAX,
    });

    let get_image_reply = conn.wait_for_reply(get_image_cookie)?;
    let bytes = get_image_reply.data();
    let depth = get_image_reply.depth();

    let pixmap_format = setup
        .pixmap_formats()
        .iter()
        .find(|item| item.depth() == depth)
        .ok_or(XCapError::new("Not found pixmap format"))?;

    let bits_per_pixel = pixmap_format.bits_per_pixel() as u32;
    let bit_order = setup.bitmap_format_bit_order();

    let get_pixel_rgba = match depth {
        8 => get_pixel8_rgba,
        16 => get_pixel16_rgba,
        24 => get_pixel24_32_rgba,
        32 => get_pixel24_32_rgba,
        _ => return Err(XCapError::new(format!("Unsupported {depth} depth"))),
    };

    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let index = ((y * width + x) * 4) as usize;
            let (r, g, b, a) = get_pixel_rgba(bytes, x, y, width, bits_per_pixel, bit_order);

            rgba[index] = r;
            rgba[index + 1] = g;
            rgba[index + 2] = b;
            rgba[index + 3] = a;
        }
    }

    RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| XCapError::new("RgbaImage::from_raw failed"))
}

pub fn get_xcb_connection_and_index() -> XCapResult<(Connection, i32)> {
    let display = env::var("DISPLAY").unwrap_or_else(|_| "DISPLAY:1".to_string());
    let (conn, idx) = Connection::connect(Some(display.as_str()))
        .or_else(|_| Connection::connect(None))
        .map_err(|e| XCapError::new(e.to_string()))?;
    Ok((conn, idx))
}

pub fn get_current_screen_buf() -> XCapResult<ScreenBuf> {
    let (conn, index) = get_xcb_connection_and_index()?;

    let setup = conn.get_setup();

    let screen = setup
        .roots()
        .nth(index as usize)
        .ok_or_else(|| XCapError::new("Not found screen"))?;

    Ok(screen.to_owned())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_full_screen_capture() {
        let screen_buf = get_current_screen_buf().unwrap();
        println!("{:?}", screen_buf);
        let result = xorg_capture(
            screen_buf.root(),
            0,
            0,
            screen_buf.width_in_pixels() as u32,
            screen_buf.height_in_pixels() as u32,
        );
        let image = result.unwrap();
        image.save("test_full_screen_capture.png").unwrap();
    }
}
