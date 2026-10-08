//! The Jetto wordmark (logo + logotype lock-up) for the "by Jetto" attribution, drawn as Tally
//! draws it: even-odd fill, in a system colour resolved for the appearance it is drawn in. Path data is jetto `assets/logo-with-logotext.svg` (viewBox
//! 4547 x 1024), the same string as Tally's `ProviderMarks.jettoWordmark`, inlined so there is no
//! bundle resource to miss.

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2::AnyThread;
use objc2_app_kit::{NSColor, NSCompositingOperation, NSImage, NSRectFillUsingOperation};
use objc2_foundation::{NSData, NSRect, NSSize, NSString};

/// Tally's About row (`SettingsView.aboutRows`): 53 x 12 pt in the primary colour.
pub const ABOUT: (f64, f64) = (53.0, 12.0);
/// Tally's popover footer (`PopoverFooterView.jettoCredit`): 40 x 9 pt in the secondary colour.
pub const FOOTER: (f64, f64) = (40.0, 9.0);

const PATH: &str = "M3161 962.636C2787.24 962.636 2797.05 628.544 2797.05 628.544V78.9463H2979.03V274.822H3156.91V456.794H2979.03V585.197C2979.03 815.423 3161 791.296 3161 791.296V962.636Z M3679.53 962.636C3305.77 962.636 3315.58 628.544 3315.58 628.544V78.9463H3497.56V274.822H3679.53V456.794H3497.56V585.197C3497.56 815.423 3679.53 791.296 3679.53 791.296V962.636Z M1475.38 963.046C1305.67 963.046 1167.86 825.238 1167.86 655.533V631.407H1350.65V651.035C1350.65 719.735 1406.68 775.758 1475.38 775.758C1544.08 775.758 1600.1 719.735 1600.1 651.035V87.1255H1782.9V655.533C1782.9 825.238 1645.09 963.046 1475.38 963.046Z M4178.43 901.707C3975.6 901.707 3810.39 736.501 3810.39 533.673C3810.39 330.846 3975.6 165.639 4178.43 165.639C4381.26 165.639 4546.47 330.846 4546.47 533.673C4546.47 736.501 4381.26 901.707 4178.43 901.707ZM4178.43 354.972C4079.88 354.972 3999.73 435.122 3999.73 533.673C3999.73 632.225 4079.88 712.374 4178.43 712.374C4276.98 712.374 4357.14 632.225 4357.14 533.673C4357.14 435.122 4276.98 354.972 4178.43 354.972Z M866.851 227.316L785.095 186.44C799.923 137.549 816.354 80.2419 829.58 33.7553C837.595 5.70304 809.541 -11.1283 788.301 8.50827C750.228 43.774 702.538 89.0583 664.866 126.328L494.542 41.7702C475.305 32.1523 452.862 32.1523 433.626 41.7702L60.916 227.316C23.645 246.151 0 283.821 0 325.9V732.657C0 774.335 23.645 812.406 60.916 830.84L141.469 870.915L684.503 214.893L358.683 978.716L433.626 1016.79C452.862 1026.4 475.305 1026.4 494.542 1016.79L867.251 831.241C904.923 812.406 928.167 774.736 928.167 733.058V325.9C927.767 283.821 904.522 246.151 866.851 227.316Z M2116.59 612.187H2648.61V530.402C2648.61 334.117 2488.72 174.227 2292.43 174.227C2096.14 174.227 1936.25 334.117 1936.25 530.402V543.897C1936.25 747.951 2102.27 913.566 2305.92 913.566C2305.92 913.566 2485.44 921.745 2588.09 828.51L2536.15 705.014C2469.9 734.456 2401.61 749.587 2332.5 749.587C2141.53 749.587 2108.82 623.637 2108.41 622.411L2105.95 612.596L2116.59 612.187ZM2117.4 459.249C2150.12 386.46 2223.32 339.842 2303.88 339.842C2384.44 339.842 2457.64 386.869 2490.35 459.249L2495.26 470.29H2112.09L2117.4 459.249Z";

/// The wordmark at `size`, as an image that resolves `color` each time it is drawn, so it follows
/// light and dark wherever it sits (an image view, a text attachment). None if AppKit cannot
/// read SVG (before macOS 14, below this app's minimum).
pub fn wordmark(size: (f64, f64), color: fn() -> Retained<NSColor>) -> Option<Retained<NSImage>> {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="4547" height="1024" viewBox="0 0 4547 1024"><path fill-rule="evenodd" fill="black" d="{PATH}"/></svg>"#
    );
    let data = NSData::with_bytes(svg.as_bytes());
    let shape = NSImage::initWithData(NSImage::alloc(), &data)?;
    let draw = RcBlock::new(move |rect: NSRect| -> Bool {
        shape.drawInRect(rect);
        color().set();
        // SourceIn: the shape's coverage times the colour, its own alpha included (secondary is translucent).
        NSRectFillUsingOperation(rect, NSCompositingOperation::SourceIn);
        Bool::YES
    });
    let image = NSImage::imageWithSize_flipped_drawingHandler(NSSize::new(size.0, size.1), false, &draw);
    image.setAccessibilityDescription(Some(&NSString::from_str("Jetto")));
    Some(image)
}
