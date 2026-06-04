use base64::{engine::general_purpose::STANDARD, Engine};
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// 截取全屏，返回 (PNG base64 编码, 宽度, 高度)
pub fn capture_screen() -> Result<(String, u32, u32)> {
    unsafe {
        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            return Err(E_FAIL.into());
        }

        let screen_width = GetSystemMetrics(SM_CXSCREEN);
        let screen_height = GetSystemMetrics(SM_CYSCREEN);

        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        let hbitmap = CreateCompatibleBitmap(hdc_screen, screen_width, screen_height);
        let h_old = SelectObject(hdc_mem, hbitmap.into());

        let result = BitBlt(
            hdc_mem, 0, 0, screen_width, screen_height,
            Some(hdc_screen), 0, 0, SRCCOPY,
        );

        if result.is_err() {
            SelectObject(hdc_mem, h_old);
            let _ = DeleteObject(hbitmap.into());
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(None, hdc_screen);
            return Err(E_FAIL.into());
        }

        let png_data = bmp_to_png(hdc_mem, screen_width, screen_height)?;

        SelectObject(hdc_mem, h_old);
        let _ = DeleteObject(hbitmap.into());
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(None, hdc_screen);

        let b64 = STANDARD.encode(&png_data);
        Ok((b64, screen_width as u32, screen_height as u32))
    }
}

/// 将内存 DC 中的位图转换为简易 PNG
fn bmp_to_png(hdc: HDC, width: i32, height: i32) -> Result<Vec<u8>> {
    unsafe {
        let mut bmi = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0 as u32,
            ..Default::default()
        };

        let mut pixel_data = vec![0u8; (width * height * 4) as usize];
        let result = GetDIBits(
            hdc,
            HBITMAP(std::ptr::null_mut()),
            0,
            height as u32,
            Some(pixel_data.as_mut_ptr() as *mut _),
            &mut bmi as *mut BITMAPINFOHEADER as *mut BITMAPINFO,
            DIB_RGB_COLORS,
        );

        if result == 0 {
            return Err(E_FAIL.into());
        }

        // 构造 BMP 文件 (24bpp)
        let row_size = (width * 3 + 3) & !3;
        let image_size = row_size * height;
        let file_size = 54 + image_size;
        let mut bmp_file = Vec::with_capacity(file_size as usize);

        // BMP 文件头
        bmp_file.extend_from_slice(b"BM");
        bmp_file.extend_from_slice(&(file_size as u32).to_le_bytes());
        bmp_file.extend_from_slice(&0u16.to_le_bytes());
        bmp_file.extend_from_slice(&0u16.to_le_bytes());
        bmp_file.extend_from_slice(&54u32.to_le_bytes());

        // BITMAPINFOHEADER
        bmp_file.extend_from_slice(&40u32.to_le_bytes());
        bmp_file.extend_from_slice(&(width as u32).to_le_bytes());
        bmp_file.extend_from_slice(&(height as u32).to_le_bytes());
        bmp_file.extend_from_slice(&1u16.to_le_bytes());
        bmp_file.extend_from_slice(&24u16.to_le_bytes());
        bmp_file.extend_from_slice(&0u32.to_le_bytes());
        bmp_file.extend_from_slice(&(image_size as u32).to_le_bytes());
        bmp_file.extend_from_slice(&2835u32.to_le_bytes());
        bmp_file.extend_from_slice(&2835u32.to_le_bytes());
        bmp_file.extend_from_slice(&0u32.to_le_bytes());
        bmp_file.extend_from_slice(&0u32.to_le_bytes());

        // BGRA -> BGR
        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                if idx + 2 < pixel_data.len() {
                    bmp_file.push(pixel_data[idx]);
                    bmp_file.push(pixel_data[idx + 1]);
                    bmp_file.push(pixel_data[idx + 2]);
                }
            }
            let padding = row_size - (width * 3);
            for _ in 0..padding {
                bmp_file.push(0);
            }
        }

        Ok(bmp_to_png_simple(&bmp_file))
    }
}

fn bmp_to_png_simple(bmp: &[u8]) -> Vec<u8> {
    if bmp.len() < 54 {
        return vec![];
    }

    let width = u32::from_le_bytes([bmp[18], bmp[19], bmp[20], bmp[21]]);
    let height = u32::from_le_bytes([bmp[22], bmp[23], bmp[24], bmp[25]]);
    let pixel_offset = u32::from_le_bytes([bmp[10], bmp[11], bmp[12], bmp[13]]) as usize;

    if pixel_offset >= bmp.len() || width == 0 || height == 0 {
        return vec![];
    }

    let mut rgb_data = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let idx = pixel_offset + ((y * width + x) * 3) as usize;
            if idx + 2 < bmp.len() {
                rgb_data.push(bmp[idx + 2]);
                rgb_data.push(bmp[idx + 1]);
                rgb_data.push(bmp[idx]);
            }
        }
    }

    let raw_len = (width * 3 + 1) * height;
    let mut raw_data = Vec::with_capacity(raw_len as usize);
    for y in 0..height {
        raw_data.push(0);
        let row_start = (y * width * 3) as usize;
        let row_end = row_start + (width * 3) as usize;
        if row_end <= rgb_data.len() {
            raw_data.extend_from_slice(&rgb_data[row_start..row_end]);
        }
    }

    let mut png = Vec::new();
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8);
    ihdr.push(2);
    ihdr.push(0);
    ihdr.push(0);
    ihdr.push(0);
    write_png_chunk(&mut png, b"IHDR", &ihdr);

    let compressed = deflate_compress(&raw_data);
    write_png_chunk(&mut png, b"IDAT", &compressed);
    write_png_chunk(&mut png, b"IEND", &[]);

    png
}

fn write_png_chunk(png: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    let length = (data.len() as u32).to_be_bytes();
    png.extend_from_slice(&length);
    png.extend_from_slice(chunk_type);
    png.extend_from_slice(data);
    let crc = Crc32::compute(chunk_type).to_be_bytes();
    png.extend_from_slice(&crc);
}

struct Crc32;

impl Crc32 {
    fn compute(data: &[u8]) -> u32 {
        let mut value: u32 = 0xFFFFFFFF;
        for &byte in data {
            value ^= byte as u32;
            for _ in 0..8 {
                if value & 1 != 0 {
                    value = (value >> 1) ^ 0xEDB88320;
                } else {
                    value >>= 1;
                }
            }
        }
        value ^ 0xFFFFFFFF
    }
}

fn deflate_compress(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    result.push(0x78);
    result.push(0x01);

    let max_block = 65535;
    let mut offset = 0;
    while offset < data.len() {
        let remaining = data.len() - offset;
        let block_size = remaining.min(max_block);
        let is_last = offset + block_size >= data.len();

        let bfinal: u8 = if is_last { 1 } else { 0 };
        result.push(bfinal);
        result.push((block_size & 0xFF) as u8);
        result.push(((block_size >> 8) & 0xFF) as u8);
        result.push((!block_size & 0xFF) as u8);
        result.push(((!block_size >> 8) & 0xFF) as u8);
        result.extend_from_slice(&data[offset..offset + block_size]);
        offset += block_size;
    }

    let adler = adler32(data);
    result.extend_from_slice(&adler.to_be_bytes());
    result
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}
