//! Menus the system draws, for context menus and the buttons that open a menu. Each item runs a
//! closure once the menu has closed.

use std::rc::Rc;

use gpui_kit::*;

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

/// The image in front of an item: an SF Symbol, or an image file on this device.
pub enum MenuIcon {
    Symbol(&'static str),
    File(SharedString),
}

enum Entry {
    Item {
        label: SharedString,
        enabled: bool,
        checked: bool,
        icon: Option<MenuIcon>,
        tooltip: Option<SharedString>,
        run: Run,
    },
    Note(SharedString),
    Separator,
}

#[derive(Default)]
pub struct Menu {
    entries: Vec<Entry>,
}

impl Menu {
    pub fn new() -> Self {
        Self::default()
    }

    fn push(mut self, entry: Entry) -> Self {
        self.entries.push(entry);
        self
    }

    fn entry(label: impl Into<SharedString>, enabled: bool, run: impl Fn(&mut Window, &mut App) + 'static) -> Entry {
        Entry::Item { label: label.into(), enabled, checked: false, icon: None, tooltip: None, run: Rc::new(run) }
    }

    pub fn item(self, label: impl Into<SharedString>, run: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.push(Self::entry(label, true, run))
    }

    pub fn item_if(
        self,
        enabled: bool,
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.push(Self::entry(label, enabled, run))
    }

    pub fn checked(
        self,
        checked: bool,
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        let Entry::Item { label, enabled, icon, tooltip, run, .. } = Self::entry(label, true, run) else {
            unreachable!()
        };
        self.push(Entry::Item { label, enabled, checked, icon, tooltip, run })
    }

    /// An item with an image in front, and what the pointer resting on it says.
    pub fn icon_item(
        self,
        label: impl Into<SharedString>,
        icon: MenuIcon,
        enabled: bool,
        tooltip: Option<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.push(Entry::Item {
            label: label.into(),
            enabled,
            checked: false,
            icon: Some(icon),
            tooltip,
            run: Rc::new(run),
        })
    }

    /// A line that only says something.
    pub fn note(self, label: impl Into<SharedString>) -> Self {
        self.push(Entry::Note(label.into()))
    }

    pub fn separator(self) -> Self {
        self.push(Entry::Separator)
    }

    pub fn when(self, condition: bool, build: impl FnOnce(Menu) -> Menu) -> Self {
        if condition { build(self) } else { self }
    }

    pub fn when_some<T>(self, value: Option<T>, build: impl FnOnce(Menu, T) -> Menu) -> Self {
        match value {
            Some(value) => build(self, value),
            None => self,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Opens the menu with its top left corner at `position`, in the window.
    pub fn show(self, position: Point<Pixels>, window: &mut Window, cx: &mut App) {
        self.open(position, false, window, cx);
    }

    /// Opens the menu with its top right corner at `position`.
    pub fn show_right_aligned(self, position: Point<Pixels>, window: &mut Window, cx: &mut App) {
        self.open(position, true, window, cx);
    }

    #[cfg(target_os = "macos")]
    fn open(self, position: Point<Pixels>, right_aligned: bool, window: &mut Window, cx: &mut App) {
        let Some(view) = macos::content_view(window) else { return };
        let handle = window.window_handle();
        // The system's menu loop runs once the window is free, and the item after it.
        cx.spawn(async move |cx| {
            let picked = macos::run(view, &self, position, right_aligned);
            let _ = handle.update(cx, move |_, window, cx| {
                if let Some(run) = picked {
                    run(window, cx);
                }
                window.refresh();
            });
        })
        .detach();
    }

    #[cfg(not(target_os = "macos"))]
    fn open(self, _: Point<Pixels>, _: bool, _: &mut Window, _: &mut App) {}
}

#[cfg(target_os = "macos")]
mod macos {
    use std::cell::Cell;

    use gpui_kit::*;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{AnyThread, DefinedClass, MainThreadMarker, define_class, msg_send, sel};
    use objc2_app_kit::{NSImage, NSMenu, NSMenuItem, NSView};
    use objc2_foundation::{NSPoint, NSSize, NSString};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::{Entry, Menu, MenuIcon, Run};

    struct Picked {
        tag: Cell<isize>,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "MotileMenuTarget"]
        #[ivars = Picked]
        struct Target;

        impl Target {
            #[unsafe(method(picked:))]
            fn picked(&self, sender: &NSMenuItem) {
                self.ivars().tag.set(sender.tag());
            }
        }
    );

    impl Target {
        fn new() -> Retained<Self> {
            let this = Self::alloc().set_ivars(Picked { tag: Cell::new(-1) });
            unsafe { msg_send![super(this), init] }
        }
    }

    pub fn content_view(window: &Window) -> Option<usize> {
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return None };
        Some(handle.ns_view.as_ptr() as usize)
    }

    pub fn run(view: usize, menu: &Menu, position: Point<Pixels>, right_aligned: bool) -> Option<Run> {
        let main = MainThreadMarker::new()?;
        // The window outlives the menu, which closes before this returns.
        let view: &NSView = unsafe { &*(view as *const NSView) };
        let target = Target::new();
        let mut runs = Vec::new();
        let menu = build(menu, &target, main, &mut runs);
        let x = f32::from(position.x) as f64 - if right_aligned { menu.size().width } else { 0. };
        let location = NSPoint::new(x, view.bounds().size.height - f32::from(position.y) as f64);
        menu.popUpMenuPositioningItem_atLocation_inView(None, location, Some(view));
        let tag = target.ivars().tag.get();
        usize::try_from(tag).ok().and_then(|tag| runs.get(tag).cloned())
    }

    fn build(menu: &Menu, target: &Target, main: MainThreadMarker, runs: &mut Vec<Run>) -> Retained<NSMenu> {
        let built = NSMenu::new(main);
        built.setAutoenablesItems(false);
        for entry in &menu.entries {
            match entry {
                Entry::Separator => built.addItem(&NSMenuItem::separatorItem(main)),
                Entry::Note(label) => {
                    let item = NSMenuItem::new(main);
                    item.setTitle(&NSString::from_str(label));
                    item.setEnabled(false);
                    built.addItem(&item);
                }
                Entry::Item { label, enabled, checked, icon, tooltip, run } => {
                    let item = NSMenuItem::new(main);
                    item.setTitle(&NSString::from_str(label));
                    item.setEnabled(*enabled);
                    if *checked {
                        item.setState(1);
                    }
                    if let Some(image) = icon.as_ref().and_then(image) {
                        item.setImage(Some(&image));
                        show_image(&item);
                    }
                    if let Some(tooltip) = tooltip {
                        item.setToolTip(Some(&NSString::from_str(tooltip)));
                    }
                    if *enabled {
                        unsafe {
                            item.setTag(runs.len() as isize);
                            item.setTarget(Some(target as &AnyObject));
                            item.setAction(Some(sel!(picked:)));
                        }
                        runs.push(run.clone());
                    }
                    built.addItem(&item);
                }
            }
        }
        built
    }

    /// macOS 27 hides the images of menu items unless they ask to be seen.
    fn show_image(item: &NSMenuItem) {
        let setter = sel!(setPreferredImageVisibility:);
        let responds: bool = unsafe { msg_send![item, respondsToSelector: setter] };
        if responds {
            let visible: isize = 1;
            let _: () = unsafe { msg_send![item, setPreferredImageVisibility: visible] };
        }
    }

    fn image(icon: &MenuIcon) -> Option<Retained<NSImage>> {
        match icon {
            MenuIcon::Symbol(name) => {
                NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), None)
            }
            MenuIcon::File(path) => {
                let image = NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(path))?;
                image.setSize(NSSize::new(16., 16.));
                Some(image)
            }
        }
    }
}

/// Where a button that opens a menu was last drawn, so the menu opens under it.
#[derive(Clone, Default)]
pub struct Anchor(Rc<std::cell::Cell<Bounds<Pixels>>>);

impl Anchor {
    /// An element that fills its parent and keeps the parent's bounds here.
    pub fn track(&self) -> impl IntoElement {
        let cell = self.0.clone();
        canvas(move |bounds, _, _| cell.set(bounds), |_, _, _, _| {}).absolute().size_full()
    }

    /// Under the element and centred on it, where a popover `width` wide opens.
    pub fn popover_origin(&self, width: f32) -> Point<Pixels> {
        let bounds = self.0.get();
        point(bounds.center().x - px(width / 2.), bounds.origin.y + bounds.size.height + px(6.))
    }

    /// Under the element, where a pull-down menu opens.
    pub fn below(&self) -> Point<Pixels> {
        let bounds = self.0.get();
        point(bounds.origin.x, bounds.origin.y + bounds.size.height + px(4.))
    }

    /// Under the element's right edge, where a menu that ends with it opens.
    pub fn below_right(&self, gap: f32) -> Point<Pixels> {
        let bounds = self.0.get();
        point(bounds.origin.x + bounds.size.width, bounds.origin.y + bounds.size.height + px(gap))
    }
}
