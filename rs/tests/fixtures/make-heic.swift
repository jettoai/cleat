// Regenerates wide.heic and tall.heic: synthetic gradients, no real photo.
// Both are 640x360 stored pixels in Display P3 with GPS metadata; tall.heic carries EXIF
// Orientation 6, so it displays as 360x640 (portrait). Run: swift make-heic.swift <output dir>
import Foundation
import ImageIO
import CoreGraphics

let out = URL(fileURLWithPath: CommandLine.arguments[1])
let (w, h) = (640, 360)
let space = CGColorSpace(name: CGColorSpace.displayP3)!
let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0, space: space,
                    bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)!
let grad = CGGradient(colorsSpace: space, colors: [CGColor(red: 1, green: 0.2, blue: 0.1, alpha: 1),
                                                   CGColor(red: 0.1, green: 0.3, blue: 1, alpha: 1)] as CFArray,
                      locations: [0, 1])!
ctx.drawLinearGradient(grad, start: .zero, end: CGPoint(x: w, y: h), options: [])
let image = ctx.makeImage()!

for (name, orientation) in [("wide", 1), ("tall", 6)] {
    let dest = CGImageDestinationCreateWithURL(out.appendingPathComponent("\(name).heic") as CFURL, "public.heic" as CFString, 1, nil)!
    let props: [CFString: Any] = [
        kCGImagePropertyOrientation: orientation,
        kCGImagePropertyGPSDictionary: [kCGImagePropertyGPSLatitude: 25.03, kCGImagePropertyGPSLatitudeRef: "N",
                                        kCGImagePropertyGPSLongitude: 121.56, kCGImagePropertyGPSLongitudeRef: "E"],
        kCGImageDestinationLossyCompressionQuality: 0.5,
    ]
    CGImageDestinationAddImage(dest, image, props as CFDictionary)
    precondition(CGImageDestinationFinalize(dest))
}
