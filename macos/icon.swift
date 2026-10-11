// Draws the Annotator icon into an .iconset directory: swift icon.swift <out.iconset>
import AppKit

let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "Annotator.iconset"
try? FileManager.default.createDirectory(atPath: out, withIntermediateDirectories: true)

func draw(_ px: Int) -> Data {
    let s = CGFloat(px)
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px, bitsPerSample: 8,
                               samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let u = s / 1024  // design units on a 1024 grid

    // Background squircle with a deep indigo gradient.
    let bg = NSBezierPath(roundedRect: NSRect(x: 100*u, y: 100*u, width: 824*u, height: 824*u), xRadius: 185*u, yRadius: 185*u)
    NSGradient(starting: NSColor(calibratedRed: 0.25, green: 0.22, blue: 0.62, alpha: 1),
               ending: NSColor(calibratedRed: 0.12, green: 0.10, blue: 0.33, alpha: 1))!.draw(in: bg, angle: -90)

    // Page.
    let page = NSBezierPath(roundedRect: NSRect(x: 250*u, y: 190*u, width: 440*u, height: 600*u), xRadius: 36*u, yRadius: 36*u)
    NSColor(calibratedWhite: 0.98, alpha: 1).setFill(); page.fill()

    // Text lines, one highlighted.
    let lineColor = NSColor(calibratedWhite: 0.78, alpha: 1)
    let widths: [CGFloat] = [330, 280, 340, 220, 300, 260]
    for (i, w) in widths.enumerated() {
        let y = (700 - CGFloat(i) * 80) * u
        if i == 2 {
            NSColor(calibratedRed: 1.0, green: 0.84, blue: 0.25, alpha: 1).setFill()
            NSBezierPath(roundedRect: NSRect(x: 282*u, y: y - 22*u, width: (w + 36)*u, height: 68*u), xRadius: 14*u, yRadius: 14*u).fill()
        }
        (i == 2 ? NSColor(calibratedRed: 0.62, green: 0.45, blue: 0.05, alpha: 1) : lineColor).setFill()
        NSBezierPath(roundedRect: NSRect(x: 300*u, y: y, width: w*u, height: 24*u), xRadius: 12*u, yRadius: 12*u).fill()
    }

    // Comment bubble.
    let bubble = NSBezierPath(roundedRect: NSRect(x: 560*u, y: 300*u, width: 300*u, height: 220*u), xRadius: 60*u, yRadius: 60*u)
    let tail = NSBezierPath()
    tail.move(to: NSPoint(x: 620*u, y: 310*u)); tail.line(to: NSPoint(x: 590*u, y: 240*u)); tail.line(to: NSPoint(x: 690*u, y: 305*u)); tail.close()
    let accent = NSColor(calibratedRed: 1.0, green: 0.55, blue: 0.30, alpha: 1)
    accent.setFill(); bubble.fill(); tail.fill()
    NSColor.white.setFill()
    for i in 0..<3 {
        NSBezierPath(ovalIn: NSRect(x: (630 + CGFloat(i) * 60)*u, y: 385*u, width: 40*u, height: 40*u)).fill()
    }
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

for base in [16, 32, 128, 256, 512] {
    try! draw(base).write(to: URL(fileURLWithPath: "\(out)/icon_\(base)x\(base).png"))
    try! draw(base * 2).write(to: URL(fileURLWithPath: "\(out)/icon_\(base)x\(base)@2x.png"))
}
