//! Building blocks: a closure target for controls, the accent switch, and the label, icon, row,
//! card and section helpers that stand in for SwiftUI's grouped Form.

use std::cell::Cell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly, Message};
use objc2_app_kit::{
    NSBezierPath, NSBox, NSBoxType, NSButton, NSColor, NSEvent, NSFont, NSFontWeightRegular,
    NSFontWeightSemibold, NSImage, NSImageSymbolConfiguration, NSImageView, NSLayoutAttribute,
    NSLayoutConstraintOrientation, NSStackView, NSStackViewDistribution, NSTextField, NSTitlePosition,
    NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_foundation::{NSEdgeInsets, NSPoint, NSRect, NSSize, NSString};

pub struct ActionIvars {
    f: Box<dyn Fn(&AnyObject)>,
}

define_class!(
    /// A control's target: runs a closure. AppKit holds targets weakly; `Ctx` keeps them alive.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CleatRsSettingsAction"]
    #[ivars = ActionIvars]
    pub struct Action;

    impl Action {
        #[unsafe(method(fire:))]
        fn fire(&self, sender: &AnyObject) {
            (self.ivars().f)(sender);
        }
    }
);

impl Action {
    pub fn new(mtm: MainThreadMarker, f: impl Fn(&AnyObject) + 'static) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ActionIvars { f: Box::new(f) });
        // SAFETY: NSObject's init.
        unsafe { msg_send![super(this), init] }
    }
}

/// Collects every target made while building a page so they live as long as their controls.
pub struct Ctx {
    pub mtm: MainThreadMarker,
    pub targets: Vec<Retained<Action>>,
}

impl Ctx {
    pub fn act(&mut self, f: impl Fn(&AnyObject) + 'static) -> Retained<Action> {
        let a = Action::new(self.mtm, f);
        self.targets.push(a.clone());
        a
    }
}

pub struct SwitchIvars {
    on: Cell<bool>,
    enabled: Cell<bool>,
    f: Box<dyn Fn(bool)>,
}

define_class!(
    /// Swift `AccentSwitchStyle`: always the accent colour when on, even with the app behind.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "CleatRsAccentSwitch"]
    #[ivars = SwitchIvars]
    pub struct AccentSwitch;

    impl AccentSwitch {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let i = self.ivars();
            let alpha = if i.enabled.get() { 1.0 } else { 0.5 };
            let track = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(32.0, 18.0));
            let fill = if i.on.get() {
                NSColor::controlAccentColor()
            } else {
                NSColor::secondaryLabelColor().colorWithAlphaComponent(0.3)
            };
            fill.colorWithAlphaComponent(fill.alphaComponent() * alpha).setFill();
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(track, 9.0, 9.0).fill();
            let x = if i.on.get() { 16.0 } else { 2.0 };
            NSColor::whiteColor().colorWithAlphaComponent(alpha).setFill();
            NSBezierPath::bezierPathWithOvalInRect(NSRect::new(NSPoint::new(x, 2.0), NSSize::new(14.0, 14.0))).fill();
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            let i = self.ivars();
            if !i.enabled.get() {
                return;
            }
            i.on.set(!i.on.get());
            self.setNeedsDisplay(true);
            (i.f)(i.on.get());
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }
    }
);

impl AccentSwitch {
    pub fn new(mtm: MainThreadMarker, on: bool, enabled: bool, f: impl Fn(bool) + 'static) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(SwitchIvars { on: Cell::new(on), enabled: Cell::new(enabled), f: Box::new(f) });
        // SAFETY: NSView's designated initialiser.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] };
        size(&view, 32.0, 18.0);
        view
    }

    pub fn set_enabled(&self, on: bool) {
        self.ivars().enabled.set(on);
        self.setNeedsDisplay(true);
    }
}

define_class!(
    /// A document view whose origin is the top left, so content starts under the title bar.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "CleatRsFlippedView"]
    pub struct FlippedView;

    impl FlippedView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

impl FlippedView {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // SAFETY: NSView's designated initialiser.
        unsafe { msg_send![Self::alloc(mtm), initWithFrame: NSRect::ZERO] }
    }
}

/// Any view, as a plain `NSView`.
pub fn to_view<T: AsRef<NSView>>(x: &T) -> Retained<NSView> {
    x.as_ref().retain()
}

pub fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

pub fn semibold() -> f64 {
    // SAFETY: an AppKit constant.
    unsafe { NSFontWeightSemibold }
}

pub fn regular() -> f64 {
    // SAFETY: an AppKit constant.
    unsafe { NSFontWeightRegular }
}

pub fn font(size: f64, w: f64) -> Retained<NSFont> {
    NSFont::systemFontOfSize_weight(size, w)
}

pub fn label(mtm: MainThreadMarker, text: &str, size_pt: f64, w: f64, color: &NSColor) -> Retained<NSTextField> {
    let l = NSTextField::labelWithString(&ns(text), mtm);
    l.setFont(Some(&font(size_pt, w)));
    l.setTextColor(Some(color));
    l
}

pub fn body(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    label(mtm, text, 13.0, regular(), &NSColor::labelColor())
}

pub fn secondary(mtm: MainThreadMarker, text: &str, size_pt: f64) -> Retained<NSTextField> {
    label(mtm, text, size_pt, regular(), &NSColor::secondaryLabelColor())
}

/// A label that wraps to whatever width its container gives it.
pub fn wrapping(mtm: MainThreadMarker, text: &str, size_pt: f64, color: &NSColor) -> Retained<NSTextField> {
    let l = NSTextField::wrappingLabelWithString(&ns(text), mtm);
    l.setFont(Some(&font(size_pt, regular())));
    l.setTextColor(Some(color));
    l.setContentCompressionResistancePriority_forOrientation(250.0, NSLayoutConstraintOrientation::Horizontal);
    l
}

pub fn symbol_image(name: &str, point: f64) -> Option<Retained<NSImage>> {
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(&ns(name), None)
        .or_else(|| NSImage::imageWithSystemSymbolName_accessibilityDescription(&ns("speaker.wave.2"), None))?;
    let config = NSImageSymbolConfiguration::configurationWithPointSize_weight(point, regular())
        .configurationByApplyingConfiguration(&NSImageSymbolConfiguration::configurationPreferringMonochrome());
    image.imageWithSymbolConfiguration(&config)
}

pub fn symbol_exists(name: &str) -> bool {
    NSImage::imageWithSystemSymbolName_accessibilityDescription(&ns(name), None).is_some()
}

/// Swift `SettingsIcon`: a monochrome regular symbol in a `size` square.
pub fn icon(mtm: MainThreadMarker, name: &str, size_pt: f64, color: &NSColor) -> Retained<NSView> {
    let point = size_pt * if size_pt <= 24.0 { 0.75 } else { 0.7 };
    let view = NSImageView::new(mtm);
    if let Some(img) = symbol_image(name, point) {
        view.setImage(Some(&img));
    }
    view.setContentTintColor(Some(color));
    size(&view, size_pt, size_pt);
    to_view(&view)
}

pub fn size(view: &NSView, w: f64, h: f64) {
    view.setTranslatesAutoresizingMaskIntoConstraints(false);
    view.widthAnchor().constraintEqualToConstant(w).setActive(true);
    view.heightAnchor().constraintEqualToConstant(h).setActive(true);
}

pub fn width(view: &NSView, w: f64) {
    view.setTranslatesAutoresizingMaskIntoConstraints(false);
    view.widthAnchor().constraintEqualToConstant(w).setActive(true);
}

pub fn stack(mtm: MainThreadMarker, vertical: bool, spacing: f64, views: &[&NSView]) -> Retained<NSStackView> {
    let s = NSStackView::new(mtm);
    s.setOrientation(if vertical {
        NSUserInterfaceLayoutOrientation::Vertical
    } else {
        NSUserInterfaceLayoutOrientation::Horizontal
    });
    s.setAlignment(if vertical { NSLayoutAttribute::Leading } else { NSLayoutAttribute::CenterY });
    s.setSpacing(spacing);
    s.setDistribution(NSStackViewDistribution::Fill);
    for v in views {
        s.addArrangedSubview(v);
    }
    s.setTranslatesAutoresizingMaskIntoConstraints(false);
    s
}

/// A flexible gap in a horizontal stack.
pub fn spacer(mtm: MainThreadMarker) -> Retained<NSView> {
    let v = NSView::new(mtm);
    v.setTranslatesAutoresizingMaskIntoConstraints(false);
    v.setContentHuggingPriority_forOrientation(1.0, NSLayoutConstraintOrientation::Horizontal);
    v.setContentCompressionResistancePriority_forOrientation(1.0, NSLayoutConstraintOrientation::Horizontal);
    v
}

/// Pins `child` (already in `parent`'s stack) to the parent's width, less `inset` each side.
pub fn fill(parent: &NSView, child: &NSView, inset: f64) {
    child.setTranslatesAutoresizingMaskIntoConstraints(false);
    child.leadingAnchor().constraintEqualToAnchor_constant(&parent.leadingAnchor(), inset).setActive(true);
    child.trailingAnchor().constraintEqualToAnchor_constant(&parent.trailingAnchor(), -inset).setActive(true);
}

/// One plain Form row: content padded 10 all round, as tall as its content.
pub fn plain_row(mtm: MainThreadMarker, views: &[&NSView]) -> Retained<NSStackView> {
    let s = stack(mtm, false, 8.0, views);
    s.setEdgeInsets(NSEdgeInsets { top: 10.0, left: 10.0, bottom: 10.0, right: 10.0 });
    s
}

/// Swift `.settingsRow()`: a plain row whose content is at least 24 tall.
pub fn row(mtm: MainThreadMarker, views: &[&NSView]) -> Retained<NSStackView> {
    let s = plain_row(mtm, views);
    s.heightAnchor().constraintGreaterThanOrEqualToConstant(44.0).setActive(true);
    s
}

/// A Form row holding a column; `vpad` is the row's 10 plus any `.padding(.vertical)` inside it.
pub fn padded_column(mtm: MainThreadMarker, spacing: f64, vpad: f64, views: &[&NSView]) -> Retained<NSStackView> {
    let s = stack(mtm, true, spacing, views);
    s.setEdgeInsets(NSEdgeInsets { top: vpad, left: 10.0, bottom: vpad, right: 10.0 });
    for v in views {
        fill(&s, v, 10.0);
    }
    s
}

fn rounded_box(mtm: MainThreadMarker, radius: f64, fill_color: &NSColor, border: Option<&NSColor>) -> Retained<NSBox> {
    let b = NSBox::new(mtm);
    b.setBoxType(NSBoxType::Custom);
    b.setTitlePosition(NSTitlePosition::NoTitle);
    b.setCornerRadius(radius);
    b.setFillColor(fill_color);
    match border {
        Some(c) => {
            b.setBorderColor(c);
            b.setBorderWidth(0.5);
        }
        None => b.setBorderWidth(0.0),
    }
    b.setContentViewMargins(NSSize::new(0.0, 0.0));
    b.setTranslatesAutoresizingMaskIntoConstraints(false);
    b
}

/// The grouped Form card: rows stacked with hairlines between them.
pub fn card(mtm: MainThreadMarker, rows: &[Retained<NSView>]) -> Retained<NSView> {
    let inner = stack(mtm, true, 0.0, &[]);
    for (i, r) in rows.iter().enumerate() {
        if i > 0 {
            let line = NSBox::new(mtm);
            line.setBoxType(NSBoxType::Separator);
            inner.addArrangedSubview(&line);
            fill(&inner, &line, 10.0);
        }
        inner.addArrangedSubview(r);
        fill(&inner, r, 0.0);
    }
    let b = rounded_box(mtm, 12.0, &NSColor::quaternarySystemFillColor(), None);
    b.setContentView(Some(&inner));
    inner.topAnchor().constraintEqualToAnchor(&b.topAnchor()).setActive(true);
    inner.bottomAnchor().constraintEqualToAnchor(&b.bottomAnchor()).setActive(true);
    fill(&b, &inner, 0.0);
    to_view(&b)
}

/// Header, card, footer: one Form Section.
pub fn section(mtm: MainThreadMarker, header: Option<&str>, rows: &[Retained<NSView>], footer: Option<&str>) -> Retained<NSView> {
    let s = stack(mtm, true, 10.0, &[]);
    if let Some(h) = header {
        let l = label(mtm, h, 13.0, semibold(), &NSColor::labelColor());
        s.addArrangedSubview(&l);
        fill(&s, &l, 10.0);
    }
    let c = card(mtm, rows);
    s.addArrangedSubview(&c);
    fill(&s, &c, 0.0);
    if let Some(f) = footer {
        let l = wrapping(mtm, f, 10.0, &NSColor::secondaryLabelColor());
        s.addArrangedSubview(&l);
        fill(&s, &l, 10.0);
    }
    to_view(&s)
}

/// Swift `Keycap`: tinted rounded box, monospaced digits.
pub fn keycap(mtm: MainThreadMarker, text: &str, w: f64) -> (Retained<NSView>, Retained<NSTextField>) {
    let l = NSTextField::labelWithString(&ns(text), mtm);
    l.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(12.0, regular())));
    let b = rounded_box(mtm, 5.0, &NSColor::quaternaryLabelColor().colorWithAlphaComponent(0.6 * 0.25), Some(&NSColor::separatorColor()));
    let inner = stack(mtm, false, 0.0, &[&l]);
    inner.setEdgeInsets(NSEdgeInsets { top: 1.0, left: 6.0, bottom: 1.0, right: 6.0 });
    b.setContentView(Some(&inner));
    pin(&b, &inner);
    hug(&b);
    let holder = stack(mtm, false, 0.0, &[&spacer(mtm), &b]);
    width(&holder, w);
    (to_view(&holder), l)
}

/// "使用中" / "已排除" capsules.
pub fn tag(mtm: MainThreadMarker, text: &str, accent: bool) -> Retained<NSView> {
    let color = if accent { NSColor::controlAccentColor() } else { NSColor::secondaryLabelColor() };
    let fill_color = if accent {
        NSColor::controlAccentColor().colorWithAlphaComponent(0.14)
    } else {
        NSColor::quaternaryLabelColor()
    };
    let l = label(mtm, text, 10.0, semibold(), &color);
    let b = rounded_box(mtm, 8.0, &fill_color, None);
    let inner = stack(mtm, false, 0.0, &[&l]);
    inner.setEdgeInsets(NSEdgeInsets { top: 2.0, left: 6.0, bottom: 2.0, right: 6.0 });
    b.setContentView(Some(&inner));
    pin(&b, &inner);
    hug(&b);
    to_view(&b)
}

/// The box takes its content's size.
fn pin(b: &NSBox, inner: &NSView) {
    inner.topAnchor().constraintEqualToAnchor(&b.topAnchor()).setActive(true);
    inner.bottomAnchor().constraintEqualToAnchor(&b.bottomAnchor()).setActive(true);
    fill(b, inner, 0.0);
}

/// Keeps a view at its natural width inside a stack with a spacer.
pub fn hug(view: &NSView) {
    view.setContentHuggingPriority_forOrientation(750.0, NSLayoutConstraintOrientation::Horizontal);
    view.setContentCompressionResistancePriority_forOrientation(750.0, NSLayoutConstraintOrientation::Horizontal);
}

/// A borderless button showing a symbol and optional words, firing `action`.
pub fn plain_button(
    mtm: MainThreadMarker,
    symbol: &str,
    title: &str,
    title_size: f64,
    title_color: &NSColor,
    action: &Action,
) -> Retained<NSButton> {
    // SAFETY: target and selector match `Action::fire:`.
    let b = unsafe { NSButton::buttonWithTitle_target_action(&ns(title), Some(action), Some(sel!(fire:)), mtm) };
    b.setBordered(false);
    if let Some(img) = symbol_image(symbol, title_size) {
        b.setImage(Some(&img));
        b.setImagePosition(if title.is_empty() {
            objc2_app_kit::NSCellImagePosition::ImageOnly
        } else {
            objc2_app_kit::NSCellImagePosition::ImageLeading
        });
    }
    b.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
    b.setFont(Some(&font(title_size, regular())));
    if !title.is_empty() {
        let attrs = attributed(title, title_size, title_color);
        b.setAttributedTitle(&attrs);
    }
    b
}

pub fn attributed(text: &str, size_pt: f64, color: &NSColor) -> Retained<objc2_foundation::NSAttributedString> {
    use objc2_app_kit::{NSFontAttributeName, NSForegroundColorAttributeName};
    use objc2::AllocAnyThread;
    use objc2_foundation::{NSAttributedString, NSDictionary};
    let f = font(size_pt, regular());
    // SAFETY: AppKit attribute-name constants; values are the documented types.
    unsafe {
        let keys = [NSFontAttributeName, NSForegroundColorAttributeName];
        let values: [&AnyObject; 2] = [&f, color];
        let dict = NSDictionary::from_slices(&keys, &values);
        NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), &ns(text), Some(&dict))
    }
}
