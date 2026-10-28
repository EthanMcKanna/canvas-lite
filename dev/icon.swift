// Draws the app icon (1024×1024 PNG) on the macOS icon grid.
// Usage: swift dev/icon.swift out.png
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let size = 1024.0
let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "icon.png"
let cs = CGColorSpace(name: CGColorSpace.sRGB)!
let ctx = CGContext(data: nil, width: Int(size), height: Int(size), bitsPerComponent: 8, bytesPerRow: 0,
                    space: cs, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!

func rgb(_ v: Double, _ a: Double = 1) -> CGColor { CGColor(colorSpace: cs, components: [v, v, v, a])! }

// macOS tile: 824pt rounded rect centred, ~22% corner radius, with a soft drop shadow.
let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
let tilePath = CGPath(roundedRect: tile, cornerWidth: 185, cornerHeight: 185, transform: nil)
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: rgb(0, 0.45))
ctx.addPath(tilePath)
ctx.setFillColor(rgb(0.06))
ctx.fillPath()
ctx.restoreGState()

// Graphite gradient, lighter at the top.
ctx.saveGState()
ctx.addPath(tilePath)
ctx.clip()
let grad = CGGradient(colorsSpace: cs, colors: [rgb(0.165), rgb(0.035)] as CFArray, locations: [0, 1])!
ctx.drawLinearGradient(grad, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 100), options: [])
// Faint radial glow behind the mark.
let glow = CGGradient(colorsSpace: cs, colors: [rgb(1, 0.07), rgb(1, 0)] as CFArray, locations: [0, 1])!
ctx.drawRadialGradient(glow, startCenter: CGPoint(x: 512, y: 560), startRadius: 0, endCenter: CGPoint(x: 512, y: 560), endRadius: 420, options: [])
ctx.restoreGState()

// Hairline edge: brighter along the top, like light catching a bevel.
ctx.saveGState()
ctx.addPath(tilePath)
ctx.clip()
ctx.addPath(CGPath(roundedRect: tile.insetBy(dx: 1.5, dy: 1.5), cornerWidth: 184, cornerHeight: 184, transform: nil))
ctx.setLineWidth(3)
ctx.replacePathWithStrokedPath()
ctx.clip()
let edge = CGGradient(colorsSpace: cs, colors: [rgb(1, 0.22), rgb(1, 0.04)] as CFArray, locations: [0, 1])!
ctx.drawLinearGradient(edge, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 100), options: [])
ctx.restoreGState()

// The mark: a page with a folded top-right corner (same geometry as the in-app logo).
let s = 380.0
let page = CGRect(x: 512 - s / 2, y: 512 - s / 2 - 6, width: s, height: s)
let f = s * 0.32
let body = CGMutablePath()
body.move(to: CGPoint(x: page.minX, y: page.maxY))
body.addLine(to: CGPoint(x: page.maxX - f, y: page.maxY))
body.addLine(to: CGPoint(x: page.maxX, y: page.maxY - f))
body.addLine(to: CGPoint(x: page.maxX, y: page.minY))
body.addLine(to: CGPoint(x: page.minX, y: page.minY))
body.closeSubpath()
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -10), blur: 30, color: rgb(0, 0.55))
ctx.addPath(body)
ctx.setFillColor(rgb(0.97))
ctx.fillPath()
ctx.restoreGState()

let fold = CGMutablePath()
fold.move(to: CGPoint(x: page.maxX - f, y: page.maxY))
fold.addLine(to: CGPoint(x: page.maxX - f, y: page.maxY - f))
fold.addLine(to: CGPoint(x: page.maxX, y: page.maxY - f))
fold.closeSubpath()
ctx.addPath(fold)
ctx.setFillColor(rgb(0.55))
ctx.fillPath()

// Three to-do rows, like the Upcoming list: the first one done.
let ink = rgb(0.10)
let r = 19.0
for (i, lineLen) in [150.0, 205.0, 120.0].enumerated() {
    let y = page.maxY - 150 - Double(i) * 78
    let c = CGPoint(x: page.minX + 70, y: y)
    if i == 0 {
        ctx.setFillColor(ink)
        ctx.fillEllipse(in: CGRect(x: c.x - r, y: c.y - r, width: r * 2, height: r * 2))
        ctx.setStrokeColor(rgb(0.97))
        ctx.setLineWidth(7)
        ctx.setLineCap(.round)
        ctx.setLineJoin(.round)
        ctx.move(to: CGPoint(x: c.x - 9, y: c.y + 0.5))
        ctx.addLine(to: CGPoint(x: c.x - 2.5, y: c.y - 7))
        ctx.addLine(to: CGPoint(x: c.x + 10, y: c.y + 8))
        ctx.strokePath()
    } else {
        ctx.setStrokeColor(ink)
        ctx.setLineWidth(7)
        ctx.strokeEllipse(in: CGRect(x: c.x - r + 3.5, y: c.y - r + 3.5, width: (r - 3.5) * 2, height: (r - 3.5) * 2))
    }
    ctx.setFillColor(i == 0 ? rgb(0.10, 0.35) : ink)
    let bar = CGRect(x: c.x + 42, y: y - 7, width: lineLen, height: 14)
    ctx.addPath(CGPath(roundedRect: bar, cornerWidth: 7, cornerHeight: 7, transform: nil))
    ctx.fillPath()
}

let img = ctx.makeImage()!
let dest = CGImageDestinationCreateWithURL(URL(fileURLWithPath: out) as CFURL, UTType.png.identifier as CFString, 1, nil)!
CGImageDestinationAddImage(dest, img, nil)
CGImageDestinationFinalize(dest)
