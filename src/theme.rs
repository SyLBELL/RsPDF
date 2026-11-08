//! Tema Material Design 3 untuk egui: palet warna (terang/gelap + aksen), tipografi,
//! ikon Material, dan komponen (tombol, chip, switch, kartu, FAB, progress).

use eframe::egui::{
    self, epaint::Shadow, Align2, Color32, Context, CursorIcon, FontData, FontDefinitions, FontFamily, FontId, Frame, Id,
    Margin, Pos2, Rect, Response, RichText, Rounding, Sense, Stroke, TextStyle, Ui, Vec2,
};

// ───────────────────────── palet ─────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub primary: Color32,
    pub on_primary: Color32,
    pub primary_container: Color32,
    pub on_primary_container: Color32,
    pub secondary: Color32,
    pub secondary_container: Color32,
    pub on_secondary_container: Color32,
    pub surface: Color32,
    pub on_surface: Color32,
    pub on_surface_variant: Color32,
    pub outline: Color32,
    pub outline_variant: Color32,
    pub error: Color32,
    pub on_error: Color32,
    pub error_container: Color32,
    pub on_error_container: Color32,
    pub sc_lowest: Color32,
    pub sc_low: Color32,
    pub sc: Color32,
    pub sc_high: Color32,
    pub sc_highest: Color32,
    pub inverse_surface: Color32,
    pub inverse_on_surface: Color32,
    pub inverse_primary: Color32,
    pub success: Color32,
    pub success_container: Color32,
    pub on_success_container: Color32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Accent {
    Purple,
    Blue,
    Teal,
    Green,
    Orange,
    Rose,
}

impl Accent {
    pub const ALL: [Accent; 6] = [Accent::Purple, Accent::Blue, Accent::Teal, Accent::Green, Accent::Orange, Accent::Rose];
    pub fn name(self) -> &'static str {
        match self {
            Accent::Purple => "Ungu",
            Accent::Blue => "Biru",
            Accent::Teal => "Teal",
            Accent::Green => "Hijau",
            Accent::Orange => "Jingga",
            Accent::Rose => "Mawar",
        }
    }
    /// Pergeseran hue relatif terhadap warna dasar M3 (ungu ≈ 256°).
    fn hue_shift(self) -> f32 {
        match self {
            Accent::Purple => 0.0,
            Accent::Blue => -41.0,
            Accent::Teal => -76.0,
            Accent::Green => -116.0,
            Accent::Orange => 129.0,
            Accent::Rose => 79.0,
        }
    }
    pub fn swatch(self) -> Color32 {
        rotate(hex(0x6750A4), self.hue_shift())
    }
}

fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub fn alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

fn lerp_c(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn rotate(c: Color32, deg: f32) -> Color32 {
    if deg == 0.0 {
        return c;
    }
    let (r, g, b) = (c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mx + mn) / 2.0;
    let d = mx - mn;
    let (mut h, s) = if d == 0.0 {
        (0.0, 0.0)
    } else {
        let s = d / (1.0 - (2.0 * l - 1.0).abs());
        let h = if mx == r { ((g - b) / d).rem_euclid(6.0) } else if mx == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 };
        (h * 60.0, s)
    };
    h = (h + deg).rem_euclid(360.0);
    let cc = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = cc * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - cc / 2.0;
    let (r1, g1, b1) = match (h / 60.0) as u32 {
        0 => (cc, x, 0.0),
        1 => (x, cc, 0.0),
        2 => (0.0, cc, x),
        3 => (0.0, x, cc),
        4 => (x, 0.0, cc),
        _ => (cc, 0.0, x),
    };
    Color32::from_rgb(((r1 + m) * 255.0).round() as u8, ((g1 + m) * 255.0).round() as u8, ((b1 + m) * 255.0).round() as u8)
}

pub fn palette(accent: Accent, dark: bool) -> Palette {
    let s = accent.hue_shift();
    let r = |v: u32| rotate(hex(v), s);
    if dark {
        Palette {
            dark,
            primary: r(0xD0BCFF),
            on_primary: r(0x381E72),
            primary_container: r(0x4F378B),
            on_primary_container: r(0xEADDFF),
            secondary: r(0xCCC2DC),
            secondary_container: r(0x4A4458),
            on_secondary_container: r(0xE8DEF8),
            surface: r(0x141218),
            on_surface: r(0xE6E0E9),
            on_surface_variant: r(0xCAC4D0),
            outline: r(0x938F99),
            outline_variant: r(0x49454F),
            error: hex(0xF2B8B5),
            on_error: hex(0x601410),
            error_container: hex(0x8C1D18),
            on_error_container: hex(0xF9DEDC),
            sc_lowest: r(0x0F0D13),
            sc_low: r(0x1D1B20),
            sc: r(0x211F26),
            sc_high: r(0x2B2930),
            sc_highest: r(0x36343B),
            inverse_surface: r(0xE6E0E9),
            inverse_on_surface: r(0x322F35),
            inverse_primary: r(0x6750A4),
            success: hex(0x7CDA96),
            success_container: hex(0x00522A),
            on_success_container: hex(0xA8F5B8),
        }
    } else {
        Palette {
            dark,
            primary: r(0x6750A4),
            on_primary: hex(0xFFFFFF),
            primary_container: r(0xEADDFF),
            on_primary_container: r(0x21005D),
            secondary: r(0x625B71),
            secondary_container: r(0xE8DEF8),
            on_secondary_container: r(0x1D192B),
            surface: r(0xFEF7FF),
            on_surface: r(0x1D1B20),
            on_surface_variant: r(0x49454F),
            outline: r(0x79747E),
            outline_variant: r(0xCAC4D0),
            error: hex(0xB3261E),
            on_error: hex(0xFFFFFF),
            error_container: hex(0xF9DEDC),
            on_error_container: hex(0x410E0B),
            sc_lowest: hex(0xFFFFFF),
            sc_low: r(0xF7F2FA),
            sc: r(0xF3EDF7),
            sc_high: r(0xECE6F0),
            sc_highest: r(0xE6E0E9),
            inverse_surface: r(0x322F35),
            inverse_on_surface: r(0xF5EFF7),
            inverse_primary: r(0xD0BCFF),
            success: hex(0x1B6B3A),
            success_container: hex(0xB7F2C6),
            on_success_container: hex(0x00210E),
        }
    }
}

pub fn pal(ctx: &Context) -> Palette {
    ctx.data(|d| d.get_temp::<Palette>(Id::new("m3_palette"))).unwrap_or_else(|| palette(Accent::Purple, true))
}

// ───────────────────────── font & gaya global ─────────────────────────

pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("icons".into()))
}

pub fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    for p in [
        r"C:\Windows\Fonts\SegoeUIVariable.ttf",
        r"C:\Windows\Fonts\segoeui.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(p) {
            fonts.font_data.insert("ui".into(), FontData::from_owned(bytes));
            fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "ui".into());
            break;
        }
    }
    fonts.font_data.insert("icons".into(), FontData::from_static(include_bytes!("../assets/MaterialIcons-Regular.ttf")));
    fonts.families.insert(FontFamily::Name("icons".into()), vec!["icons".into()]);
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &Context, accent: Accent, dark: bool) {
    let p = palette(accent, dark);
    ctx.data_mut(|d| d.insert_temp(Id::new("m3_palette"), p));
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.surface;
    v.window_fill = p.sc_high;
    v.window_stroke = Stroke::NONE;
    v.window_rounding = Rounding::same(28.0);
    let sh = Shadow { offset: Vec2::new(0.0, 6.0), blur: 24.0, spread: 0.0, color: Color32::from_black_alpha(if dark { 120 } else { 50 }) };
    v.window_shadow = sh;
    v.popup_shadow = sh;
    v.menu_rounding = Rounding::same(12.0);
    v.extreme_bg_color = p.sc_lowest;
    v.faint_bg_color = p.sc;
    v.code_bg_color = p.sc_high;
    v.override_text_color = Some(p.on_surface);
    v.hyperlink_color = p.primary;
    v.error_fg_color = p.error;
    v.warn_fg_color = if dark { hex(0xFFB74D) } else { hex(0xB26A00) };
    v.selection.bg_fill = alpha(p.primary, 80);
    v.selection.stroke = Stroke::new(2.0, p.primary);
    v.striped = false;
    let r = Rounding::same(8.0);
    v.widgets.noninteractive.bg_fill = p.sc_low;
    v.widgets.noninteractive.weak_bg_fill = p.sc_low;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.outline_variant);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.on_surface);
    v.widgets.noninteractive.rounding = r;
    v.widgets.inactive.bg_fill = p.sc_highest;
    v.widgets.inactive.weak_bg_fill = p.sc_highest;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.outline);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.primary);
    v.widgets.inactive.rounding = r;
    v.widgets.hovered.bg_fill = p.sc_highest;
    v.widgets.hovered.weak_bg_fill = p.sc_highest;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.on_surface);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, p.primary);
    v.widgets.hovered.rounding = r;
    v.widgets.hovered.expansion = 0.0;
    v.widgets.active.bg_fill = p.sc_highest;
    v.widgets.active.weak_bg_fill = p.sc_highest;
    v.widgets.active.bg_stroke = Stroke::new(2.0, p.primary);
    v.widgets.active.fg_stroke = Stroke::new(2.0, p.primary);
    v.widgets.active.rounding = r;
    v.widgets.active.expansion = 0.0;
    v.widgets.open = v.widgets.inactive;
    ctx.set_visuals(v);

    let mut st = (*ctx.style()).clone();
    st.spacing.item_spacing = Vec2::new(8.0, 8.0);
    st.spacing.button_padding = Vec2::new(10.0, 6.0);
    st.spacing.interact_size.y = 34.0;
    st.spacing.slider_width = 180.0;
    st.text_styles.insert(TextStyle::Heading, FontId::proportional(24.0));
    st.text_styles.insert(TextStyle::Body, FontId::proportional(14.5));
    st.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
    st.text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
    st.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.0));
    ctx.set_style(st);
}

// ───────────────────────── teks & ikon ─────────────────────────

pub fn headline(t: &str) -> RichText {
    RichText::new(t).size(26.0).strong()
}
pub fn title(t: &str) -> RichText {
    RichText::new(t).size(17.0).strong()
}
pub fn label_small(t: &str) -> RichText {
    RichText::new(t).size(12.0)
}

pub fn paint_icon(painter: &egui::Painter, center: Pos2, ch: char, size: f32, color: Color32) {
    painter.text(center, Align2::CENTER_CENTER, ch.to_string(), icon_font(size), color);
}

pub fn icon(ui: &mut Ui, ch: char, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_icon(ui.painter(), rect.center(), ch, size, color);
}

/// Lingkaran berisi ikon (leading icon di daftar/kartu).
pub fn icon_badge(ui: &mut Ui, ch: char, diameter: f32, bg: Color32, fg: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(diameter), Sense::hover());
    ui.painter().circle_filled(rect.center(), diameter / 2.0, bg);
    paint_icon(ui.painter(), rect.center(), ch, diameter * 0.55, fg);
}

// ───────────────────────── tombol ─────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Filled,
    Tonal,
    Outlined,
    Text,
    Danger,
}

pub fn button(ui: &mut Ui, kind: Kind, ic: Option<char>, label: &str) -> Response {
    button_ex(ui, kind, ic, label, true, false)
}

pub fn button_en(ui: &mut Ui, kind: Kind, ic: Option<char>, label: &str, enabled: bool) -> Response {
    button_ex(ui, kind, ic, label, enabled, false)
}

pub fn button_sm(ui: &mut Ui, kind: Kind, ic: Option<char>, label: &str) -> Response {
    button_ex(ui, kind, ic, label, true, true)
}

pub fn button_ex(ui: &mut Ui, kind: Kind, ic: Option<char>, label: &str, enabled: bool, compact: bool) -> Response {
    let p = pal(ui.ctx());
    let (mut bg, mut fg, mut stroke) = match kind {
        Kind::Filled => (p.primary, p.on_primary, None),
        Kind::Tonal => (p.secondary_container, p.on_secondary_container, None),
        Kind::Outlined => (Color32::TRANSPARENT, p.primary, Some(p.outline)),
        Kind::Text => (Color32::TRANSPARENT, p.primary, None),
        Kind::Danger => (p.error_container, p.on_error_container, None),
    };
    if !enabled {
        if bg != Color32::TRANSPARENT {
            bg = alpha(p.on_surface, 31);
        }
        fg = alpha(p.on_surface, 97);
        stroke = stroke.map(|_| alpha(p.on_surface, 31));
    }
    let (h, fs, pad_l, pad_r, isz) = if compact { (32.0, 13.0, if ic.is_some() { 12.0 } else { 14.0 }, 14.0, 16.0) } else { (40.0, 14.0, if ic.is_some() { 16.0 } else { 24.0 }, 24.0, 18.0) };
    let font = FontId::proportional(fs);
    let tw = if label.is_empty() { 0.0 } else { ui.painter().layout_no_wrap(label.to_string(), font.clone(), fg).size().x };
    let icon_w = if ic.is_some() { isz + if label.is_empty() { 0.0 } else { 8.0 } } else { 0.0 };
    let size = Vec2::new(pad_l + icon_w + tw + pad_r, h);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let rd = Rounding::same(h / 2.0);
    let painter = ui.painter();
    if bg != Color32::TRANSPARENT {
        painter.rect_filled(rect, rd, bg);
    }
    if let Some(s) = stroke {
        painter.rect_stroke(rect, rd, Stroke::new(1.0, s));
    }
    let layer = if !enabled { 0 } else if resp.is_pointer_button_down_on() { 30 } else if resp.hovered() { 20 } else { 0 };
    if layer > 0 {
        painter.rect_filled(rect, rd, alpha(fg, layer));
    }
    if let Some(c) = ic {
        paint_icon(painter, Pos2::new(rect.left() + pad_l + isz / 2.0, rect.center().y), c, isz, fg);
    }
    if !label.is_empty() {
        painter.text(Pos2::new(rect.left() + pad_l + icon_w, rect.center().y), Align2::LEFT_CENTER, label, font, fg);
    }
    resp
}

/// Extended FAB (aksi utama).
pub fn fab(ui: &mut Ui, ic: char, label: &str, enabled: bool) -> Response {
    let p = pal(ui.ctx());
    let (bg, fg) = if enabled { (p.primary_container, p.on_primary_container) } else { (alpha(p.on_surface, 31), alpha(p.on_surface, 97)) };
    let font = FontId::proportional(16.0);
    let tw = ui.painter().layout_no_wrap(label.to_string(), font.clone(), fg).size().x;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(16.0 + 24.0 + 12.0 + tw + 20.0, 56.0), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let painter = ui.painter();
    if enabled {
        painter.add(Shadow { offset: Vec2::new(0.0, 2.0), blur: 10.0, spread: 0.0, color: Color32::from_black_alpha(55) }.as_shape(rect, Rounding::same(16.0)));
    }
    painter.rect_filled(rect, Rounding::same(16.0), bg);
    let layer = if !enabled { 0 } else if resp.is_pointer_button_down_on() { 30 } else if resp.hovered() { 20 } else { 0 };
    if layer > 0 {
        painter.rect_filled(rect, Rounding::same(16.0), alpha(fg, layer));
    }
    paint_icon(painter, Pos2::new(rect.left() + 16.0 + 12.0, rect.center().y), ic, 24.0, fg);
    painter.text(Pos2::new(rect.left() + 16.0 + 24.0 + 12.0, rect.center().y), Align2::LEFT_CENTER, label, font, fg);
    resp
}

pub fn icon_button(ui: &mut Ui, ch: char, tip: &str) -> Response {
    icon_button_ex(ui, ch, tip, true, 40.0, None)
}

pub fn icon_button_sm(ui: &mut Ui, ch: char, tip: &str, enabled: bool) -> Response {
    icon_button_ex(ui, ch, tip, enabled, 32.0, None)
}

pub fn icon_button_ex(ui: &mut Ui, ch: char, tip: &str, enabled: bool, size: f32, color: Option<Color32>) -> Response {
    let p = pal(ui.ctx());
    let fg = if enabled { color.unwrap_or(p.on_surface_variant) } else { alpha(p.on_surface, 97) };
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let painter = ui.painter();
    let layer = if !enabled { 0 } else if resp.is_pointer_button_down_on() { 30 } else if resp.hovered() { 20 } else { 0 };
    if layer > 0 {
        painter.circle_filled(rect.center(), size / 2.0, alpha(fg, layer));
    }
    paint_icon(painter, rect.center(), ch, size * 0.6, fg);
    if tip.is_empty() { resp } else { resp.on_hover_text(tip) }
}

// ───────────────────────── chip, switch ─────────────────────────

/// Filter/choice chip: pilih satu nilai (pengganti radio button).
pub fn choice<T: PartialEq>(ui: &mut Ui, cur: &mut T, val: T, label: &str) -> Response {
    let p = pal(ui.ctx());
    let sel = *cur == val;
    let (fg, bg) = if sel { (p.on_secondary_container, p.secondary_container) } else { (p.on_surface_variant, Color32::TRANSPARENT) };
    let font = FontId::proportional(13.5);
    let tw = ui.painter().layout_no_wrap(label.to_string(), font.clone(), fg).size().x;
    let lead = if sel { 18.0 + 8.0 } else { 0.0 };
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(12.0 + lead + tw + 16.0, 32.0), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let rd = Rounding::same(8.0);
    let painter = ui.painter();
    painter.rect_filled(rect, rd, bg);
    if !sel {
        painter.rect_stroke(rect, rd, Stroke::new(1.0, p.outline));
    }
    let layer = if resp.is_pointer_button_down_on() { 30 } else if resp.hovered() { 20 } else { 0 };
    if layer > 0 {
        painter.rect_filled(rect, rd, alpha(fg, layer));
    }
    if sel {
        paint_icon(painter, Pos2::new(rect.left() + 12.0 + 9.0, rect.center().y), crate::icons::CHECK, 18.0, fg);
    }
    painter.text(Pos2::new(rect.left() + 12.0 + lead, rect.center().y), Align2::LEFT_CENTER, label, font, fg);
    if resp.clicked() && !sel {
        *cur = val;
        resp.mark_changed();
    }
    resp
}

/// Switch M3 (52×32) dengan label di kanan.
pub fn switch(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let p = pal(ui.ctx());
    let font = FontId::proportional(14.5);
    let tw = if label.is_empty() { 0.0 } else { ui.painter().layout_no_wrap(label.to_string(), font.clone(), p.on_surface).size().x };
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(52.0 + if label.is_empty() { 0.0 } else { 12.0 + tw }, 32.0), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.15);
    let track = Rect::from_min_size(rect.min, Vec2::new(52.0, 32.0));
    let painter = ui.painter();
    painter.rect_filled(track, Rounding::same(16.0), lerp_c(p.sc_highest, p.primary, t));
    painter.rect_stroke(track, Rounding::same(16.0), Stroke::new(2.0, alpha(p.outline, (255.0 * (1.0 - t)) as u8)));
    let pressed = resp.is_pointer_button_down_on();
    let radius = if pressed { 14.0 } else { 8.0 + 4.0 * t };
    let cx = track.left() + 16.0 + 20.0 * t;
    if resp.hovered() {
        painter.circle_filled(Pos2::new(cx, track.center().y), 20.0, alpha(lerp_c(p.on_surface, p.primary, t), 20));
    }
    painter.circle_filled(Pos2::new(cx, track.center().y), radius, lerp_c(p.outline, p.on_primary, t));
    if t > 0.5 {
        paint_icon(painter, Pos2::new(cx, track.center().y), crate::icons::CHECK, 14.0, alpha(p.primary, (255.0 * t) as u8));
    }
    if !label.is_empty() {
        painter.text(Pos2::new(track.right() + 12.0, track.center().y), Align2::LEFT_CENTER, label, font, p.on_surface);
    }
    resp
}

// ───────────────────────── kartu & layout ─────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CardKind {
    Filled,
    Outlined,
    Tonal,
    Highest,
    Success,
}

pub fn card<R>(ui: &mut Ui, kind: CardKind, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = pal(ui.ctx());
    let (fill, stroke) = match kind {
        CardKind::Filled => (p.sc_low, Stroke::NONE),
        CardKind::Outlined => (p.surface, Stroke::new(1.0, p.outline_variant)),
        CardKind::Tonal => (p.secondary_container, Stroke::NONE),
        CardKind::Highest => (p.sc_highest, Stroke::NONE),
        CardKind::Success => (p.success_container, Stroke::NONE),
    };
    Frame::none()
        .fill(fill)
        .stroke(stroke)
        .rounding(Rounding::same(16.0))
        .inner_margin(Margin::same(20.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Judul langkah bernomor (1 · Pilih file).
pub fn step_header(ui: &mut Ui, n: u32, text: &str) {
    let p = pal(ui.ctx());
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 13.0, p.primary);
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, n.to_string(), FontId::proportional(13.0), p.on_primary);
        ui.label(title(text));
    });
}

/// Progress linear indeterminate (animasi).
pub fn progress_indeterminate(ui: &mut Ui, width: f32) {
    let p = pal(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 4.0), Sense::hover());
    let t = ui.input(|i| i.time) as f32;
    let head = (t * 0.8).fract() * 1.5;
    let tail = (head - 0.45).max(0.0);
    let head = head.min(1.0);
    let painter = ui.painter();
    painter.rect_filled(rect, Rounding::same(2.0), p.secondary_container);
    if head > tail {
        let bar = Rect::from_min_max(Pos2::new(rect.left() + rect.width() * tail, rect.top()), Pos2::new(rect.left() + rect.width() * head, rect.bottom()));
        painter.rect_filled(bar, Rounding::same(2.0), p.primary);
    }
    ui.ctx().request_repaint();
}

/// Kotak informasi berwarna (banner).
pub fn banner(ui: &mut Ui, ic: char, text: &str, kind: BannerKind) {
    let p = pal(ui.ctx());
    let (bg, fg) = match kind {
        BannerKind::Info => (p.secondary_container, p.on_secondary_container),
        BannerKind::Warn => (if p.dark { hex(0x4A3B12) } else { hex(0xFFEFC2) }, if p.dark { hex(0xFFE08A) } else { hex(0x4A3500) }),
        BannerKind::Error => (p.error_container, p.on_error_container),
        BannerKind::Success => (p.success_container, p.on_success_container),
    };
    Frame::none().fill(bg).rounding(Rounding::same(12.0)).inner_margin(Margin::symmetric(14.0, 10.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            icon(ui, ic, 20.0, fg);
            ui.add(egui::Label::new(RichText::new(text).color(fg)).wrap());
        });
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BannerKind {
    Info,
    Warn,
    Error,
    Success,
}
