// Optional macOS-only fixture regeneration. Original 8x8 test pixels, no third-party assets.
import AppKit
let image = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 8, pixelsHigh: 8, bitsPerSample: 8, samplesPerPixel: 3, hasAlpha: false, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 24, bitsPerPixel: 24)!
for y in 0..<8 { for x in 0..<8 { let p = y * 24 + x * 3; image.bitmapData![p] = UInt8(x * 32); image.bitmapData![p + 1] = UInt8(y * 32); image.bitmapData![p + 2] = 128 } }
let data = image.representation(using: .jpeg, properties: [.compressionFactor: 0.85])!
try data.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
