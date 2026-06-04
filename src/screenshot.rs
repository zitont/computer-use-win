use base64::{engine::general_purpose::STANDARD, Engine};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// 初始化 DPI 感知 (Per-Monitor V2),应在程序启动时调用一次
pub fn init_dpi_awareness() {
    unsafe {
        // Per-Monitor V2 确保截图获取真实像素坐标,不受 DPI 缩放影响
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

/// 截取全屏 (支持多显示器),返回 (PNG base64 编码, 宽度, 高度)
pub fn capture_screen() -> Result<(String, u32, u32)> {
    unsafe {
        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            return Err(E_FAIL.into());
        }

        // 使用虚拟屏幕尺寸,覆盖所有显示器
        let screen_width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let screen_height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let origin_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let origin_y = GetSystemMetrics(SM_YVIRTUALSCREEN);

        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        let hbitmap = CreateCompatibleBitmap(hdc_screen, screen_width, screen_height);
        let h_old = SelectObject(hdc_mem, hbitmap.into());

        let result = BitBlt(
            hdc_mem, 0, 0, screen_width, screen_height,
            Some(hdc_screen), origin_x, origin_y, SRCCOPY,
        );

        if result.is_err() {
            SelectObject(hdc_mem, h_old);
            let _ = DeleteObject(hbitmap.into());
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(None, hdc_screen);
            return Err(E_FAIL.into());
        }

        let png_data = encode_png(hdc_mem, hbitmap, screen_width, screen_height)?;

        SelectObject(hdc_mem, h_old);
        let _ = DeleteObject(hbitmap.into());
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(None, hdc_screen);

        let b64 = STANDARD.encode(&png_data);
        Ok((b64, screen_width as u32, screen_height as u32))
    }
}

/// 从 GDI 位图直接编码 PNG (24-bit RGB, Sub 滤波 + zlib 压缩)
fn encode_png(hdc: HDC, hbitmap: HBITMAP, width: i32, height: i32) -> Result<Vec<u8>> {
    unsafe {
        // 获取 32 位 BGRA 像素
        let mut bmi = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0 as u32,
            ..Default::default()
        };

        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let result = GetDIBits(
            hdc,
            hbitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi as *mut BITMAPINFOHEADER as *mut BITMAPINFO,
            DIB_RGB_COLORS,
        );

        if result == 0 {
            return Err(E_FAIL.into());
        }

        let row_bytes = (width * 3) as usize;

        // 构建 PNG 原始扫描数据: 每行 = 滤波类型字节 + RGB 像素
        // 使用 Sub 滤波 (type=1): 当前字节减去左侧字节的差值,提高相邻像素的压缩率
        let mut raw = Vec::with_capacity((row_bytes + 1) * height as usize);

        for y in 0..height as usize {
            // 从 BGRA 提取本行 RGB
            let mut current_row = Vec::with_capacity(row_bytes);
            for x in 0..width as usize {
                let src = (y * width as usize + x) * 4;
                current_row.push(pixels[src + 2]); // R
                current_row.push(pixels[src + 1]); // G
                current_row.push(pixels[src]);     // B
            }

            // Sub 滤波: filtered[x] = raw[x] - raw[x - bpp], bpp=3 (RGB)
            raw.push(1); // 滤波类型 = Sub
            for x in 0..row_bytes {
                let left = if x >= 3 { current_row[x - 3] } else { 0 };
                raw.push(current_row[x].wrapping_sub(left));
            }
        }

        // zlib 压缩
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(&raw).map_err(|_| Error::from(E_FAIL))?;
        let compressed = encoder.finish().map_err(|_| Error::from(E_FAIL))?;

        // 组装 PNG
        let mut png = Vec::new();
        png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&(width as u32).to_be_bytes());
        ihdr.extend_from_slice(&(height as u32).to_be_bytes());
        ihdr.push(8);  // 位深度
        ihdr.push(2);  // 颜色类型: RGB
        ihdr.push(0);  // 压缩方法
        ihdr.push(0);  // 滤波方法
        ihdr.push(0);  // 隔行扫描
        write_png_chunk(&mut png, b"IHDR", &ihdr);
        write_png_chunk(&mut png, b"IDAT", &compressed);
        write_png_chunk(&mut png, b"IEND", &[]);

        Ok(png)
    }
}

fn write_png_chunk(png: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    png.extend_from_slice(chunk_type);
    png.extend_from_slice(data);
    let crc = Crc32::compute(&[chunk_type.as_slice(), data].concat());
    png.extend_from_slice(&crc.to_be_bytes());
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
