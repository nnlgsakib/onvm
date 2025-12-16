# Watermark Remover

A WASM-based watermark removal program that can automatically detect and remove watermarks from images using various image processing techniques.

## Features

### Detection Methods
- **Auto-Detect**: Automatically finds watermark regions using edge detection and morphological operations
- **Manual Region**: Specify exact watermark coordinates
- **Brightness Threshold**: Detect watermarks based on brightness levels
- **Color-Based**: Detect watermarks by target color matching

### Removal Methods
- **Inpaint**: Texture-aware reconstruction using surrounding pixels
- **Blur**: Gaussian blur to smooth out watermarked areas
- **Median Filter**: Median filtering for noise reduction
- **Clone**: Clone pixels from surrounding areas
- **Content-Aware**: Advanced content-aware fill algorithm

## Input Format

```json
{
  "image_data": "base64-encoded-image-data",
  "operation": {
    "type": "auto_detect" | "manual_region" | "brightness_threshold" | "color_based",
    "x": 100,           // For manual_region
    "y": 50,            // For manual_region
    "width": 200,       // For manual_region
    "height": 80,       // For manual_region
    "threshold": 200,   // For brightness_threshold
    "target_color": [255, 255, 255],  // For color_based
    "tolerance": 30     // For color_based
  },
  "parameters": {
    "method": "inpaint" | "blur" | "median_filter" | "clone" | "content_aware",
    "feather_edges": true,
    "blend_mode": "normal" | "multiply" | "screen" | "overlay"
  }
}
```

## Output Format

```json
{
  "success": true,
  "message": "Watermark removed successfully",
  "processed_image": "base64-encoded-processed-image",
  "metadata": {
    "original_size": [800, 600],
    "watermark_detected": true,
    "watermark_region": [100, 50, 200, 80],
    "processing_time_ms": 1234,
    "method_used": "Inpaint"
  }
}
```

## Building

```bash
# Build for WASM target
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown --release

# The compiled WASM file will be at:
# target/wasm32-unknown-unknown/release/watermark_remover.wasm
```

## Usage Examples

### Auto-Detect with Inpainting
```json
{
  "image_data": "base64-encoded-image-data-here",
  "operation": {
    "type": "auto_detect"
  },
  "parameters": {
    "method": "inpaint",
    "feather_edges": true
  }
}
```

### Manual Region with Content-Aware Fill
```json
{
  "image_data": "base64-encoded-image-data-here",
  "operation": {
    "type": "manual_region",
    "x": 100,
    "y": 50,
    "width": 200,
    "height": 80
  },
  "parameters": {
    "method": "content_aware",
    "feather_edges": true
  }
}
```

### Color-Based Detection
```json
{
  "image_data": "base64-encoded-image-data-here",
  "operation": {
    "type": "color_based",
    "target_color": [255, 255, 255],
    "tolerance": 30
  },
  "parameters": {
    "method": "blur"
  }
}
```

## Technical Details

### Algorithms Used

1. **Edge Detection**: Sobel operator for gradient-based edge detection
2. **Morphological Operations**: Dilation and erosion for noise reduction
3. **Flood Fill**: Connected component analysis for region detection
4. **Content-Aware Fill**: Patch-based texture synthesis
5. **Inpainting**: Adaptive sampling from surrounding regions

### Supported Image Formats
- JPEG
- PNG
- BMP
- (Decoded automatically)

### Performance Considerations
- Images larger than 2MB may take significant processing time
- Content-aware fill is computationally intensive but provides best results
- Auto-detection works best for watermarks with clear contrast

## Limitations

- Works best on watermarks with reasonable contrast from background
- Complex semi-transparent watermarks may require manual region specification
- Performance scales with image size and watermark complexity
- Very small watermarks (<10x10 pixels) may not be detected reliably

## Dependencies

- `image`: Core image processing
- `imageproc`: Advanced image processing algorithms
- `serde`: JSON serialization
- `base64`: Base64 encoding/decoding
- `once_cell`: Global state management