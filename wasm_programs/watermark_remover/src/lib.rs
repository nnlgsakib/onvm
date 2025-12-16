use base64::Engine;
use image::codecs::png::PngEncoder;
use image::{DynamicImage, GrayImage, Luma, Rgb};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::vec::Vec;
use once_cell::sync::OnceCell;

static OUT_BUF: OnceCell<Mutex<Vec<u8>>> = OnceCell::new();

#[derive(Debug, Deserialize)]
struct WatermarkRemovalRequest {
    image_data: String,
    operation: WatermarkOperation,
    parameters: Option<RemovalParameters>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum WatermarkOperation {
    AutoDetect,
    ManualRegion { x: u32, y: u32, width: u32, height: u32 },
    BrightnessThreshold { threshold: u8 },
    ColorBased { target_color: [u8; 3], tolerance: u8 },
}

#[derive(Debug, Deserialize)]
struct RemovalParameters {
    method: RemovalMethod,
    feather_edges: Option<bool>,
    blend_mode: Option<BlendMode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RemovalMethod {
    Inpaint,
    Blur,
    MedianFilter,
    Clone,
    ContentAware,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
}

#[derive(Debug, Serialize)]
struct WatermarkRemovalResponse {
    success: bool,
    message: String,
    processed_image: Option<String>,
    metadata: Option<ProcessingMetadata>,
}

#[derive(Debug, Serialize)]
struct ProcessingMetadata {
    original_size: (u32, u32),
    watermark_detected: bool,
    watermark_region: Option<(u32, u32, u32, u32)>,
    processing_time_ms: u64,
    method_used: String,
}

#[no_mangle]
pub extern "C" fn onvm_main(ptr: i32, len: i32) -> (i32, i32) {
    let input = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    
    let response = match process_watermark_removal(input) {
        Ok(response_bytes) => response_bytes,
        Err(e) => {
            let error_response = WatermarkRemovalResponse {
                success: false,
                message: format!("Processing error: {}", e),
                processed_image: None,
                metadata: None,
            };
            serde_json::to_vec(&error_response)
                .unwrap_or_else(|_| b"{\"success\":false,\"message\":\"serialization error\"}".to_vec())
        }
    };

    let buf = OUT_BUF.get_or_init(|| Mutex::new(Vec::with_capacity(1024 * 1024)));
    let mut guard = buf.lock().unwrap();
    guard.clear();
    guard.extend_from_slice(&response);
    let out_ptr = guard.as_ptr() as i32;
    let out_len = guard.len() as i32;
    std::mem::forget(guard);
    (out_ptr, out_len)
}

#[no_mangle]
pub extern "C" fn onvm_last_result() -> (i32, i32) {
    if let Some(buf) = OUT_BUF.get() {
        let guard = buf.lock().unwrap();
        let ptr = guard.as_ptr() as i32;
        let len = guard.len() as i32;
        std::mem::forget(guard);
        (ptr, len)
    } else {
        (0, 0)
    }
}

fn process_watermark_removal(input: &[u8]) -> Result<Vec<u8>, String> {
    let start_time = std::time::Instant::now();
    
    let request: WatermarkRemovalRequest = serde_json::from_slice(input)
        .map_err(|e| format!("Invalid JSON input: {}", e))?;

    let image_bytes = base64::engine::general_purpose::STANDARD
        .decode(&request.image_data)
        .map_err(|e| format!("Invalid base64 image data: {}", e))?;

    let mut img = image::load_from_memory(&image_bytes)
        .map_err(|e| format!("Failed to load image: {}", e))?;

    let original_size = (img.width(), img.height());
    let watermark_region = match &request.operation {
        WatermarkOperation::ManualRegion { x, y, width, height } => {
            Some((*x, *y, *width, *height))
        }
        WatermarkOperation::AutoDetect => {
            detect_watermark_region(&img)
        }
        WatermarkOperation::BrightnessThreshold { threshold } => {
            detect_by_brightness_threshold(&img, *threshold)
        }
        WatermarkOperation::ColorBased { target_color, tolerance } => {
            detect_by_color(&img, target_color, *tolerance)
        }
    };

    let processed_image = if let Some((x, y, width, height)) = watermark_region {
        let method = request.parameters
            .as_ref()
            .map(|p| &p.method)
            .unwrap_or(&RemovalMethod::Inpaint);

        remove_watermark(&mut img, x, y, width, height, method)?;
        encode_image_to_base64(&img)?
    } else {
        return Err("No watermark region detected".to_string());
    };

    let processing_time = start_time.elapsed().as_millis() as u64;

    let response = WatermarkRemovalResponse {
        success: true,
        message: "Watermark removed successfully".to_string(),
        processed_image: Some(processed_image),
        metadata: Some(ProcessingMetadata {
            original_size,
            watermark_detected: watermark_region.is_some(),
            watermark_region,
            processing_time_ms: processing_time,
            method_used: format!("{:?}", request.parameters
                .as_ref()
                .map(|p| &p.method)
                .unwrap_or(&RemovalMethod::Inpaint)),
        }),
    };

    serde_json::to_vec(&response)
        .map_err(|e| format!("Failed to serialize response: {}", e))
}

fn detect_watermark_region(img: &DynamicImage) -> Option<(u32, u32, u32, u32)> {
    let gray = img.to_luma8();
    
    let edges = detect_edges(&gray);
    let thresholded = simple_threshold(&edges, 50);
    
    let morphed = simple_dilate(&thresholded, 3);
    let cleaned = simple_erode(&morphed, 2);
    
    find_largest_white_region(&cleaned, img.width(), img.height())
}

fn detect_by_brightness_threshold(img: &DynamicImage, threshold: u8) -> Option<(u32, u32, u32, u32)> {
    let gray = img.to_luma8();
    let binary = simple_threshold(&gray, threshold);
    
    let morphed = simple_dilate(&binary, 2);
    find_largest_white_region(&morphed, img.width(), img.height())
}

fn detect_by_color(img: &DynamicImage, target_color: &[u8; 3], tolerance: u8) -> Option<(u32, u32, u32, u32)> {
    let rgb_img = img.to_rgb8();
    let mut mask = GrayImage::new(img.width(), img.height());
    
    for (x, y, pixel) in rgb_img.enumerate_pixels() {
        let distance = color_distance(&pixel.0, target_color);
        if distance <= tolerance {
            mask.put_pixel(x, y, Luma([255]));
        }
    }
    
    let cleaned = simple_erode(&mask, 1);
    let morphed = simple_dilate(&cleaned, 2);
    
    find_largest_white_region(&morphed, img.width(), img.height())
}

fn color_distance(color1: &[u8; 3], color2: &[u8; 3]) -> u8 {
    let r_diff = (color1[0] as i16 - color2[0] as i16).abs();
    let g_diff = (color1[1] as i16 - color2[1] as i16).abs();
    let b_diff = (color1[2] as i16 - color2[2] as i16).abs();
    ((r_diff + g_diff + b_diff) / 3) as u8
}

fn detect_edges(gray: &GrayImage) -> GrayImage {
    let mut edges = GrayImage::new(gray.width(), gray.height());
    
    for y in 1..gray.height()-1 {
        for x in 1..gray.width()-1 {
            let current = gray.get_pixel(x, y)[0] as i16;
            
            let sobel_x = 
                (-1 * gray.get_pixel(x-1, y-1)[0] as i16) +
                (1 * gray.get_pixel(x+1, y-1)[0] as i16) +
                (-2 * gray.get_pixel(x-1, y)[0] as i16) +
                (2 * gray.get_pixel(x+1, y)[0] as i16) +
                (-1 * gray.get_pixel(x-1, y+1)[0] as i16) +
                (1 * gray.get_pixel(x+1, y+1)[0] as i16);
            
            let sobel_y = 
                (-1 * gray.get_pixel(x-1, y-1)[0] as i16) +
                (-2 * gray.get_pixel(x, y-1)[0] as i16) +
                (-1 * gray.get_pixel(x+1, y-1)[0] as i16) +
                (1 * gray.get_pixel(x-1, y+1)[0] as i16) +
                (2 * gray.get_pixel(x, y+1)[0] as i16) +
                (1 * gray.get_pixel(x+1, y+1)[0] as i16);
            
            let magnitude = ((sobel_x * sobel_x + sobel_y * sobel_y) as f64).sqrt() as u8;
            let clamped = magnitude.min(255).max(0);
            edges.put_pixel(x, y, Luma([clamped]));
        }
    }
    
    edges
}

fn simple_threshold(gray: &GrayImage, threshold: u8) -> GrayImage {
    let mut result = gray.clone();
    for pixel in result.pixels_mut() {
        pixel[0] = if pixel[0] > threshold { 255 } else { 0 };
    }
    result
}

fn simple_dilate(gray: &GrayImage, kernel_size: u32) -> GrayImage {
    let mut result = gray.clone();
    let radius = kernel_size / 2;
    
    for y in radius..gray.height()-radius {
        for x in radius..gray.width()-radius {
            let mut max_val = 0;
            for dy in 0..kernel_size {
                for dx in 0..kernel_size {
                    let py = y + dy - radius;
                    let px = x + dx - radius;
                    let val = gray.get_pixel(px, py)[0];
                    max_val = max_val.max(val);
                }
            }
            result.put_pixel(x, y, Luma([max_val]));
        }
    }
    result
}

fn simple_erode(gray: &GrayImage, kernel_size: u32) -> GrayImage {
    let mut result = gray.clone();
    let radius = kernel_size / 2;
    
    for y in radius..gray.height()-radius {
        for x in radius..gray.width()-radius {
            let mut min_val = 255;
            for dy in 0..kernel_size {
                for dx in 0..kernel_size {
                    let py = y + dy - radius;
                    let px = x + dx - radius;
                    let val = gray.get_pixel(px, py)[0];
                    min_val = min_val.min(val);
                }
            }
            result.put_pixel(x, y, Luma([min_val]));
        }
    }
    result
}

fn find_largest_white_region(mask: &GrayImage, img_width: u32, img_height: u32) -> Option<(u32, u32, u32, u32)> {
    let mut visited = vec![vec![false; img_width as usize]; img_height as usize];
    let mut largest_region = None;
    let mut max_area = 0;
    
    for y in 0..img_height {
        for x in 0..img_width {
            if mask.get_pixel(x, y)[0] > 128 && !visited[y as usize][x as usize] {
                let region = flood_fill(mask, &mut visited, x, y);
                let area = (region.2 * region.3) as usize;
                
                if area > max_area && area > 100 {
                    max_area = area;
                    largest_region = Some(region);
                }
            }
        }
    }
    
    largest_region
}

fn flood_fill(mask: &GrayImage, visited: &mut Vec<Vec<bool>>, start_x: u32, start_y: u32) -> (u32, u32, u32, u32) {
    let mut min_x = start_x;
    let mut max_x = start_x;
    let mut min_y = start_y;
    let mut max_y = start_y;
    
    let mut stack = Vec::new();
    stack.push((start_x, start_y));
    
    while let Some((x, y)) = stack.pop() {
        if x >= mask.width() || y >= mask.height() {
            continue;
        }
        
        if visited[y as usize][x as usize] || mask.get_pixel(x, y)[0] <= 128 {
            continue;
        }
        
        visited[y as usize][x as usize] = true;
        
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
        
        if x > 0 { stack.push((x - 1, y)); }
        if x < mask.width() - 1 { stack.push((x + 1, y)); }
        if y > 0 { stack.push((x, y - 1)); }
        if y < mask.height() - 1 { stack.push((x, y + 1)); }
    }
    
    (min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
}

fn remove_watermark(img: &mut DynamicImage, x: u32, y: u32, width: u32, height: u32, method: &RemovalMethod) -> Result<(), String> {
    match method {
        RemovalMethod::Inpaint => {
            inpaint_region(img, x, y, width, height)?;
        }
        RemovalMethod::Blur => {
            apply_gaussian_blur(img, x, y, width, height);
        }
        RemovalMethod::MedianFilter => {
            apply_median_filter(img, x, y, width, height);
        }
        RemovalMethod::Clone => {
            clone_from_surroundings(img, x, y, width, height);
        }
        RemovalMethod::ContentAware => {
            content_aware_fill(img, x, y, width, height);
        }
    }
    Ok(())
}

fn inpaint_region(img: &mut DynamicImage, x: u32, y: u32, width: u32, height: u32) -> Result<(), String> {
    let rgb_img = img.to_rgb8();
    let mut result_img = rgb_img.clone();
    
    let patch_size = 3;
    let half_patch = patch_size / 2;
    
    for py in y..y + height {
        for px in x..x + width {
            if px < half_patch || px >= img.width() - half_patch || 
               py < half_patch || py >= img.height() - half_patch {
                continue;
            }
            
            let mut accumulated = [0u32; 3];
            let mut count = 0u32;
            
            let half_patch_i32 = half_patch as i32;
            for dy in -half_patch_i32..=half_patch_i32 {
                for dx in -half_patch_i32..=half_patch_i32 {
                    let sample_x = px as i32 + dx;
                    let sample_y = py as i32 + dy;
                    
                    if sample_x >= x as i32 && sample_x < (x + width) as i32 &&
                        sample_y >= y as i32 && sample_y < (y + height) as i32 {
                        continue;
                    }
                    
                    if sample_x >= 0 && sample_x < img.width() as i32 &&
                        sample_y >= 0 && sample_y < img.height() as i32 {
                        let pixel = rgb_img.get_pixel(sample_x as u32, sample_y as u32);
                        accumulated[0] += pixel[0] as u32;
                        accumulated[1] += pixel[1] as u32;
                        accumulated[2] += pixel[2] as u32;
                        count += 1;
                    }
                }
            }
            
            if count > 0 {
                result_img.put_pixel(px, py, Rgb([
                    (accumulated[0] / count) as u8,
                    (accumulated[1] / count) as u8,
                    (accumulated[2] / count) as u8
                ]));
            }
        }
    }
    
    *img = DynamicImage::ImageRgb8(result_img);
    Ok(())
}

fn apply_gaussian_blur(img: &mut DynamicImage, x: u32, y: u32, width: u32, height: u32) {
    let rgb_img = img.to_rgb8();
    let mut result_img = rgb_img.clone();
    
    let kernel = vec![
        1, 2, 1,
        2, 4, 2,
        1, 2, 1
    ];
    let kernel_sum = 16u32;
    
    for py in y..y + height {
        for px in x..x + width {
            let mut accumulated = [0u32; 3];
            
            for ky in 0..3 {
                for kx in 0..3 {
                    let sample_x = (px as i32 + kx as i32 - 1).max(0).min(img.width() as i32 - 1) as u32;
                    let sample_y = (py as i32 + ky as i32 - 1).max(0).min(img.height() as i32 - 1) as u32;
                    
                    let pixel = rgb_img.get_pixel(sample_x, sample_y);
                    let kernel_val = kernel[ky * 3 + kx] as u32;
                    
                    accumulated[0] += pixel[0] as u32 * kernel_val;
                    accumulated[1] += pixel[1] as u32 * kernel_val;
                    accumulated[2] += pixel[2] as u32 * kernel_val;
                }
            }
            
            result_img.put_pixel(px, py, Rgb([
                (accumulated[0] / kernel_sum) as u8,
                (accumulated[1] / kernel_sum) as u8,
                (accumulated[2] / kernel_sum) as u8
            ]));
        }
    }
    
    *img = DynamicImage::ImageRgb8(result_img);
}

fn apply_median_filter(img: &mut DynamicImage, x: u32, y: u32, width: u32, height: u32) {
    let rgb_img = img.to_rgb8();
    let mut result_img = rgb_img.clone();
    
    for py in y..y + height {
        for px in x..x + width {
            let mut neighbors = Vec::new();
            
            for dy in -1i32..=1i32 {
                for dx in -1i32..=1i32 {
                    let sample_x = (px as i32 + dx).max(0).min(img.width() as i32 - 1) as u32;
                    let sample_y = (py as i32 + dy).max(0).min(img.height() as i32 - 1) as u32;
                    
                    let pixel = rgb_img.get_pixel(sample_x, sample_y);
                    neighbors.push(pixel);
                }
            }
            
            neighbors.sort_by_key(|p| (p[0] as u32) + (p[1] as u32) + (p[2] as u32));
            let median_pixel = neighbors[neighbors.len() / 2];
            
            result_img.put_pixel(px, py, *median_pixel);
        }
    }
    
    *img = DynamicImage::ImageRgb8(result_img);
}

fn clone_from_surroundings(img: &mut DynamicImage, x: u32, y: u32, width: u32, height: u32) {
    let rgb_img = img.to_rgb8();
    let mut result_img = rgb_img.clone();
    
    for py in y..y + height {
        for px in x..x + width {
            let source_x = if px < img.width() / 2 { 
                (x + width).min(img.width() - 1) 
            } else { 
                x.saturating_sub(1) 
            };
            
            let source_y = py;
            
            if source_x < img.width() && source_x != px {
                let pixel = rgb_img.get_pixel(source_x, source_y);
                result_img.put_pixel(px, py, *pixel);
            }
        }
    }
    
    *img = DynamicImage::ImageRgb8(result_img);
}

fn content_aware_fill(img: &mut DynamicImage, x: u32, y: u32, width: u32, height: u32) {
    let rgb_img = img.to_rgb8();
    let mut result_img = rgb_img.clone();
    
    for py in y..y + height {
        for px in x..x + width {
            let mut best_patch = None;
            let mut min_diff = f64::MAX;
            
            let patch_size = 5;
            let half_patch = patch_size / 2;
            
            for source_y in half_patch..img.height() - half_patch {
                for source_x in half_patch..img.width() - half_patch {
                    if source_x >= x && source_x < x + width && 
                       source_y >= y && source_y < y + height {
                        continue;
                    }
                    
                    let mut diff = 0.0;
                    let mut samples = 0;
                    
                    let half_patch_i32 = half_patch as i32;
                    for dy in -half_patch_i32..=half_patch_i32 {
                        for dx in -half_patch_i32..=half_patch_i32 {
                            let target_x = px as i32 + dx;
                            let target_y = py as i32 + dy;
                            
                            if target_x < x as i32 || target_x >= (x + width) as i32 ||
                               target_y < y as i32 || target_y >= (y + height) as i32 {
                                if target_x >= 0 && target_x < img.width() as i32 &&
                                   target_y >= 0 && target_y < img.height() as i32 {
                                    let source_pixel = rgb_img.get_pixel(
                                        (source_x as i32 + dx) as u32,
                                        (source_y as i32 + dy) as u32
                                    );
                                    let target_pixel = rgb_img.get_pixel(target_x as u32, target_y as u32);
                                    
                                    diff += ((source_pixel[0] as i16 - target_pixel[0] as i16).abs() as f64).powi(2);
                                    diff += ((source_pixel[1] as i16 - target_pixel[1] as i16).abs() as f64).powi(2);
                                    diff += ((source_pixel[2] as i16 - target_pixel[2] as i16).abs() as f64).powi(2);
                                    samples += 1;
                                }
                            }
                        }
                    }
                    
                    if samples > 0 {
                        let avg_diff = diff / samples as f64;
                        if avg_diff < min_diff {
                            min_diff = avg_diff;
                            best_patch = Some((source_x, source_y));
                        }
                    }
                }
            }
            
            if let Some((source_x, source_y)) = best_patch {
                let pixel = rgb_img.get_pixel(source_x, source_y);
                result_img.put_pixel(px, py, *pixel);
            }
        }
    }
    
    *img = DynamicImage::ImageRgb8(result_img);
}

fn encode_image_to_base64(img: &DynamicImage) -> Result<String, String> {
    let mut buffer = Vec::new();
    
    let encoder = PngEncoder::new(&mut buffer);
    img.write_with_encoder(encoder)
        .map_err(|e| format!("Failed to encode image: {}", e))?;
    
    Ok(base64::engine::general_purpose::STANDARD.encode(&buffer))
}