//! Custom-menu preview model, based on AssetShowcase's CUBE_UI_CORE.
//! All actions are local visual state; buttons and close intentionally do nothing.
use alloc::{format, string::String, vec, vec::Vec};
#[path = "interface_style.rs"]
mod style;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    pub progress: i32,
    pub count: i32,
    pub enabled: bool,
    pub checked: bool,
}
#[derive(Clone, Copy)]
pub struct Button {
    pub mode: usize,
    pub icon: usize,
    pub text: &'static str,
}
pub struct Definition {
    pub name: &'static str,
    pub title: &'static str,
    pub text: &'static str,
    pub theme: usize,
    pub tier: u8,
    pub buttons: &'static [Button],
    pub controls: [bool; 4],
    pub initial: State,
}
include!(concat!(env!("OUT_DIR"), "/interface_examples.rs"));
pub const PLATEAU_LOADING: usize = 3;
pub const PLATEAU_THEME: usize = 4;
pub const PLATEAU_ERROR: usize = 5;
const PAGE_COUNT: usize = 6;
const PLATEAU_PAGES: [Definition; 3] = [
    Definition { name: "plateau-loading", title: "CUSTOM PLATEAU", text: "Loading your profile...", theme: 0, tier: 1,
        buttons: &[], controls: [false; 4], initial: State { progress: 0, count: 0, enabled: false, checked: false } },
    Definition { name: "plateau-theme", title: "CUSTOM PLATEAU", text: "Choose your terrace colour once.\nOne personal plateau. Your space.", theme: 0, tier: 1,
        buttons: &[
            Button { mode: 0, icon: 0, text: "SKY" }, Button { mode: 0, icon: 0, text: "GROUND" },
            Button { mode: 0, icon: 0, text: "BLACK" }, Button { mode: 0, icon: 0, text: "WHITE" },
            Button { mode: 0, icon: 0, text: "ISLAND" }, Button { mode: 0, icon: 0, text: "CITY" },
        ], controls: [false; 4], initial: State { progress: 0, count: 0, enabled: false, checked: false } },
    Definition { name: "plateau-error", title: "CUSTOM PLATEAU", text: "Profile request did not complete.\nKey4: retry. Key5: leave.\nReload discards unsaved placements.", theme: 0, tier: 1,
        buttons: &[Button { mode: 0, icon: 0, text: "RELOAD SAVED" }], controls: [false; 4], initial: State { progress: 0, count: 0, enabled: false, checked: false } },
];
pub fn definition(page: usize) -> &'static Definition {
    if page < EXAMPLES.len() { &EXAMPLES[page] } else { &PLATEAU_PAGES[page - EXAMPLES.len()] }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Noop,
    Slider,
    Checkbox,
    Toggle,
    Decrement,
    Increment,
}
#[derive(Clone, Debug)]
pub struct Widget {
    pub rect: [i32; 4],
    pub action: Action,
    pub disabled: bool,
}
#[derive(Clone)]
pub struct Layout {
    pub width: usize,
    pub height: usize,
    pub rows: Vec<String>,
    pub positions: [i32; 4],
    pub widgets: Vec<Widget>,
}
fn printable(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '?'
            }
        })
        .collect()
}
pub fn text_rows(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let text = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "  ");
    let mut rows = vec![String::new()];
    for c in text.chars() {
        if c == '\n' || rows.last().unwrap().len() == 40 {
            if rows.len() == 12 {
                break;
            }
            rows.push(String::new());
            if c == '\n' {
                continue;
            }
        }
        rows.last_mut()
            .unwrap()
            .push(if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '?'
            });
    }
    rows
}
fn content(b: Button) -> (String, Option<usize>, i32) {
    let text = if b.mode == 1 {
        String::new()
    } else {
        printable(b.text)
    };
    let icon = (b.mode != 0).then_some(b.icon);
    let width = text.len() as i32 * 6
        + if icon.is_some() { 7 } else { 0 }
        + if icon.is_some() && !text.is_empty() {
            4
        } else {
            0
        };
    (text, icon, width)
}
pub fn layout(def: &Definition, state: State) -> Layout {
    let rows = text_rows(def.text);
    let mut widths: Vec<_> = def
        .buttons
        .iter()
        .map(|&b| (content(b).2 + 12).max(26))
        .collect();
    let n = widths.len() as i32;
    let width = 126
        .max(printable(def.title).len() as i32 * 6 + 28)
        .max(rows.iter().map(|r| r.len() as i32 * 6).max().unwrap_or(0) + 14)
        .max(widths.iter().sum::<i32>() + 6 * (n - 1) + 14);
    let mut y = if rows.is_empty() {
        24
    } else {
        24 + (rows.len() as i32 - 1) * 15 + 11 + 9
    };
    let mut positions = [-1; 4];
    if def.controls[0] {
        positions[0] = y;
        y += 23;
    }
    if def.controls[1] || def.controls[2] {
        positions[1] = y;
        y += 24;
    }
    if def.controls[3] {
        positions[2] = y;
        y += 24;
    }
    positions[3] = y + 2;
    let mut widgets = vec![Widget {
        rect: [width - 16, 3, 12, 12],
        action: Action::Noop,
        disabled: false,
    }];
    if def.controls[0] {
        widgets.push(Widget {
            rect: [7, positions[0], width - 48, 14],
            action: Action::Slider,
            disabled: false,
        });
    }
    let half = (width - 20) / 2;
    if def.controls[1] {
        widgets.push(Widget {
            rect: [7, positions[1], half, 18],
            action: Action::Checkbox,
            disabled: false,
        });
    }
    if def.controls[2] {
        widgets.push(Widget {
            rect: [width - 7 - half, positions[1], half, 18],
            action: Action::Toggle,
            disabled: false,
        });
    }
    if def.controls[3] {
        let x = (width - 80) / 2;
        for (dx, w, action, disabled) in [
            (0, 18, Action::Decrement, state.count <= -999),
            (18, 44, Action::Noop, false),
            (62, 18, Action::Increment, state.count >= 999),
        ] {
            widgets.push(Widget {
                rect: [x + dx, positions[2], w, 18],
                action,
                disabled,
            });
        }
    }
    let extra = width - 14 - 6 * (n - 1) - widths.iter().sum::<i32>();
    let mut x = 7;
    for (i, w) in widths.iter_mut().enumerate() {
        *w += extra / n + i32::from((i as i32) < extra % n);
        widgets.push(Widget {
            rect: [x, positions[3], *w, 20],
            action: Action::Noop,
            disabled: false,
        });
        x += *w + 6;
    }
    Layout {
        width: width as usize,
        height: (y + 29) as usize,
        rows,
        positions,
        widgets,
    }
}
pub struct Demo {
    pub page: usize,
    pub states: [State; PAGE_COUNT],
    pub layout: Layout,
    pub revision: u64,
    pub hover: Option<usize>,
    pub pressed: Option<usize>,
    capture: Option<usize>,
    pulse_until: u64,
    activated: Option<usize>,
}
impl Demo {
    pub fn new() -> Self {
        let states = core::array::from_fn(|i| definition(i).initial);
        Self {
            page: 0,
            layout: layout(&EXAMPLES[0], states[0]),
            states,
            revision: 1,
            hover: None,
            pressed: None,
            capture: None,
            pulse_until: 0,
            activated: None,
        }
    }
    pub fn select(&mut self, page: usize) {
        self.page = page % PAGE_COUNT;
        self.cancel();
        self.rebuild();
    }
    pub fn cancel(&mut self) {
        self.hover = None;
        self.pressed = None;
        self.capture = None;
        self.pulse_until = 0;
        self.activated = None;
        self.revision += 1;
    }
    pub fn captured(&self) -> bool {
        self.capture.is_some()
    }
    pub fn take_activation(&mut self) -> Option<usize> { self.activated.take() }
    fn rebuild(&mut self) {
        self.layout = layout(definition(self.page), self.states[self.page]);
        self.revision += 1;
    }
    pub fn tick(&mut self, now: u64) {
        if self.capture.is_none() && self.pressed.is_some() && now >= self.pulse_until {
            self.pressed = None;
            self.revision += 1;
        }
    }
    pub fn hit(&self, p: [f32; 2]) -> Option<usize> {
        self.layout.widgets.iter().position(|w| {
            let [x, y, width, height] = w.rect;
            !w.disabled
                && p[0] >= x as f32
                && p[1] >= y as f32
                && p[0] < (x + width) as f32
                && p[1] < (y + height) as f32
        })
    }
    pub fn pointer(&mut self, p: Option<[f32; 2]>, pressed: bool, down: bool, now: u64) {
        let hit = p.and_then(|p| self.hit(p));
        if self.hover != hit {
            self.hover = hit;
            self.revision += 1;
        }
        if pressed {
            self.capture = hit;
            self.pressed = hit;
            self.revision += 1;
        }
        if let Some(id) = self.capture {
            let w = &self.layout.widgets[id];
            if w.action == Action::Slider {
                if let Some(p) = p {
                    let start = w.rect[0] as f32 + 4.5;
                    let travel = w.rect[2] as f32 - 9.;
                    let value = libm::roundf((p[0] - start) / travel * 100.).clamp(0., 100.) as i32;
                    if self.states[self.page].progress != value {
                        self.states[self.page].progress = value;
                        self.rebuild();
                    }
                }
            }
            if !down {
                if hit == Some(id) {
                    self.activated = Some(id);
                    let state = &mut self.states[self.page];
                    match self.layout.widgets[id].action {
                        Action::Checkbox => state.checked = !state.checked,
                        Action::Toggle => state.enabled = !state.enabled,
                        Action::Decrement => state.count = (state.count - 1).max(-999),
                        Action::Increment => state.count = (state.count + 1).min(999),
                        Action::Noop | Action::Slider => {}
                    }
                    self.rebuild();
                }
                self.capture = None;
                self.pulse_until = now + 180;
            }
        }
    }
    pub fn raster(&self) -> Canvas {
        raster(
            definition(self.page),
            self.states[self.page],
            &self.layout,
            self.hover,
            self.pressed,
        )
    }
}
#[derive(Clone)]
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub colors: Vec<u32>,
    pub heights: Vec<u8>,
}
impl Canvas {
    fn new(w: usize, h: usize, color: u32) -> Self {
        Self {
            width: w,
            height: h,
            colors: vec![color; w * h],
            heights: vec![0; w * h],
        }
    }
    fn put(&mut self, x: i32, y: i32, z: u8, color: u32) {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            let i = y as usize * self.width + x as usize;
            if z >= self.heights[i] {
                self.colors[i] = color;
                self.heights[i] = z;
            }
        }
    }
    fn rect(&mut self, r: [i32; 4], z: u8, color: u32, clip: i32) {
        let [x, y, w, h] = r;
        for dy in 0..h {
            for dx in 0..w {
                if dx.min(w - 1 - dx) + dy.min(h - 1 - dy) >= clip {
                    self.put(x + dx, y + dy, z, color);
                }
            }
        }
    }
    fn label(&mut self, text: &str, x: i32, y: i32, z: u8, color: u32) {
        for (i, c) in printable(text).bytes().enumerate() {
            let bits = style::GLYPHS[(c - 32) as usize];
            for bit in 0..64 {
                if bits >> (63 - bit) & 1 != 0 {
                    self.put(
                        x + i as i32 * 6 + bit % 6 + i32::from(c == b'q'),
                        y + bit / 6,
                        z,
                        color,
                    );
                }
            }
        }
    }
    fn icon(&mut self, id: usize, x: i32, y: i32, z: u8, color: u32) {
        for (dy, row) in style::ICONS[id].iter().enumerate() {
            for dx in 0..7 {
                if row >> (6 - dx) & 1 != 0 {
                    self.put(x + dx, y + dy as i32, z, color);
                }
            }
        }
    }
    fn button(
        &mut self,
        r: [i32; 4],
        text: &str,
        icon: Option<usize>,
        fill: u32,
        ink: u32,
        edge: u32,
        lift: u8,
    ) {
        self.rect(r, 1, edge, 1);
        self.rect(r, 2 + lift, fill, 1);
        let [x, y, w, h] = r;
        let content = text.len() as i32 * 6
            + if icon.is_some() { 7 } else { 0 }
            + if icon.is_some() && !text.is_empty() {
                4
            } else {
                0
            };
        let left = x + (w - content) / 2;
        if let Some(icon) = icon {
            self.icon(icon, left, y + (h - 7) / 2, 3 + lift, ink);
        }
        if !text.is_empty() {
            self.label(
                text,
                left + if icon.is_some() { 11 } else { 0 },
                y + (h - 11) / 2,
                3 + lift,
                ink,
            );
        }
    }
}
fn tone(color: u32, amount: f32) -> u32 {
    (0..3)
        .map(|a| ((((color >> (a * 8)) & 255) as f32 * amount).min(255.) as u32) << (a * 8))
        .sum()
}
fn raster(
    def: &Definition,
    state: State,
    l: &Layout,
    hover: Option<usize>,
    pressed: Option<usize>,
) -> Canvas {
    let t = style::THEMES[def.theme];
    let w = l.width as i32;
    let h = l.height as i32;
    let mut c = Canvas::new(l.width, l.height, t[0]);
    c.rect([1, 1, w - 2, h - 2], 0, t[1], 1);
    c.rect([1, 1, w - 2, 16], 1, t[2], 1);
    c.label(def.title, 6, 4, 2, t[6]);
    for (i, row) in l.rows.iter().enumerate() {
        c.label(
            row,
            7,
            24 + i as i32 * 15,
            1,
            if i == 0 { t[3] } else { t[4] },
        );
    }
    let visual = |id: usize, fill: u32| {
        (
            tone(
                fill,
                if pressed == Some(id) {
                    0.78
                } else if hover == Some(id) {
                    1.15
                } else {
                    1.
                },
            ),
            u8::from(hover == Some(id) && pressed != Some(id)),
        )
    };
    let (fill, lift) = visual(0, t[10]);
    c.button(l.widgets[0].rect, "", Some(0), fill, t[6], t[0], lift);
    let mut id = 1;
    if def.controls[0] {
        let [x, y, width, _] = l.widgets[id].rect;
        let travel = width - 9;
        let offset = (travel * state.progress + 50) / 100;
        let (fill, lift) = visual(id, t[5]);
        c.rect([x + 3, y + 5, width - 6, 4], 2, t[8], 0);
        c.rect([x + 3, y + 5, offset + 3, 4], 2, fill, 0);
        c.rect([x + 3 + offset, y + 2, 3, 10], 3 + lift, t[6], 0);
        c.label(&format!("{}%", state.progress), w - 33, y + 1, 1, t[3]);
        id += 1;
    }
    if def.controls[1] {
        let r = l.widgets[id].rect;
        let [x, y, _, _] = r;
        let (fill, lift) = visual(id, if state.checked { t[5] } else { t[7] });
        let ink = if state.checked { t[6] } else { t[3] };
        c.button(r, "", None, fill, ink, t[0], lift);
        c.rect([x + 5, y + 5, 9, 9], 3 + lift, t[3], 0);
        c.rect([x + 6, y + 6, 7, 7], 3 + lift, t[1], 0);
        if state.checked {
            c.icon(1, x + 6, y + 6, 4 + lift, t[5]);
        }
        c.label("SOUND", x + 18, y + 4, 3 + lift, ink);
        id += 1;
    }
    if def.controls[2] {
        let r = l.widgets[id].rect;
        let [x, y, width, _] = r;
        let (fill, lift) = visual(id, if state.enabled { t[5] } else { t[7] });
        let ink = if state.enabled { t[6] } else { t[3] };
        c.button(r, "", None, fill, ink, t[0], lift);
        c.label(
            if state.enabled { "ON" } else { "OFF" },
            x + 5,
            y + 3,
            3 + lift,
            ink,
        );
        c.rect([x + width - 19, y + 6, 14, 5], 3 + lift, t[8], 0);
        c.rect(
            [x + width - if state.enabled { 10 } else { 19 }, y + 5, 5, 7],
            4 + lift,
            t[6],
            0,
        );
        id += 1;
    }
    if def.controls[3] {
        let x = (w - 80) / 2;
        let y = l.positions[2];
        let group = (id..id + 3)
            .find(|i| hover == Some(*i) || pressed == Some(*i))
            .unwrap_or(id);
        let (fill, lift) = visual(group, t[5]);
        c.button([x, y, 80, 18], "", None, fill, t[6], t[0], lift);
        c.icon(3, x + 5, y + 5, 3 + lift, t[6]);
        c.icon(2, x + 68, y + 5, 3 + lift, t[6]);
        let text = format!("{}", state.count);
        c.label(
            &text,
            x + (80 - text.len() as i32 * 6) / 2,
            y + 3,
            3 + lift,
            t[6],
        );
        id += 3;
    }
    for (i, &button) in def.buttons.iter().enumerate() {
        let (text, icon, _) = content(button);
        let (fill, lift) = visual(id, if i == 0 { t[5] } else { t[7] });
        c.button(
            l.widgets[id].rect,
            &text,
            icon,
            fill,
            if i == 0 { t[6] } else { t[3] },
            t[0],
            lift,
        );
        id += 1;
    }
    c
}

/// Standard BMP inputs for TRUEOS's retained-image API. Four texels per UI cell
/// preserve the editor's pixel edges and supply a local bevel normal.
pub fn textures(canvas: &Canvas) -> (Vec<u8>, Vec<u8>) {
    let width = canvas.width * 4;
    let height = canvas.height * 4;
    let mut colors = Vec::with_capacity(width * height * 3);
    let mut normals = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            let cx = x / 4;
            let cy = y / 4;
            let i = cy * canvas.width + cx;
            let color = canvas.colors[i];
            colors.extend_from_slice(&[(color >> 16) as u8, (color >> 8) as u8, color as u8]);
            let mut nx = if x % 4 == 0 {
                -0.65
            } else if x % 4 == 3 {
                0.65
            } else {
                0.
            };
            let mut ny = if y % 4 == 0 {
                -0.65
            } else if y % 4 == 3 {
                0.65
            } else {
                0.
            };
            let z = canvas.heights[i] as f32;
            if x % 4 == 0 && cx > 0 {
                nx -= (z - canvas.heights[i - 1] as f32).max(0.) * 0.35;
            }
            if x % 4 == 3 && cx + 1 < canvas.width {
                nx += (z - canvas.heights[i + 1] as f32).max(0.) * 0.35;
            }
            if y % 4 == 0 && cy > 0 {
                ny -= (z - canvas.heights[i - canvas.width] as f32).max(0.) * 0.35;
            }
            if y % 4 == 3 && cy + 1 < canvas.height {
                ny += (z - canvas.heights[i + canvas.width] as f32).max(0.) * 0.35;
            }
            let length = libm::sqrtf(nx * nx + ny * ny + 1.);
            normals
                .extend([nx, ny, 1.].map(|v| libm::roundf((v / length * 0.5 + 0.5) * 255.) as u8));
        }
    }
    (bmp(width, height, &colors), bmp(width, height, &normals))
}
pub fn bmp(width: usize, height: usize, rgb: &[u8]) -> Vec<u8> {
    assert_eq!(rgb.len(), width * height * 3);
    let size = 54 + width * height * 4;
    let mut bytes = vec![0; 54];
    bytes[..2].copy_from_slice(b"BM");
    bytes[2..6].copy_from_slice(&(size as u32).to_le_bytes());
    bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&(width as i32).to_le_bytes());
    bytes[22..26].copy_from_slice(&(height as i32).to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&32u16.to_le_bytes());
    bytes[34..38].copy_from_slice(&((width * height * 4) as u32).to_le_bytes());
    for row in rgb.chunks_exact(width * 3).rev() {
        for pixel in row.chunks_exact(3) {
            bytes.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    fn center(w: &Widget) -> [f32; 2] {
        [
            w.rect[0] as f32 + w.rect[2] as f32 / 2.,
            w.rect[1] as f32 + w.rect[3] as f32 / 2.,
        ]
    }
    fn click(d: &mut Demo, id: usize) {
        let p = center(&d.layout.widgets[id]);
        d.pointer(Some(p), true, true, 10);
        d.pointer(Some(p), false, false, 20);
    }
    #[test]
    fn close_and_actions_only_animate() {
        let mut d = Demo::new();
        let initial = d.states;
        for i in 0..d.layout.widgets.len() {
            click(&mut d, i);
            assert_eq!(d.page, 0);
            assert_eq!(d.states, initial);
            assert_eq!(d.pressed, Some(i));
            d.tick(201);
            assert_eq!(d.pressed, None);
        }
    }
    #[test]
    fn controls_change_preview_values_and_slider_keeps_drag_capture_outside() {
        let mut d = Demo::new();
        d.select(2);
        for action in [
            Action::Checkbox,
            Action::Toggle,
            Action::Increment,
            Action::Decrement,
        ] {
            let id = d
                .layout
                .widgets
                .iter()
                .position(|w| w.action == action)
                .unwrap();
            click(&mut d, id);
        }
        assert!(d.states[2].checked);
        assert!(!d.states[2].enabled);
        assert_eq!(d.states[2].count, 0);
        let id = d
            .layout
            .widgets
            .iter()
            .position(|w| w.action == Action::Slider)
            .unwrap();
        d.pointer(Some(center(&d.layout.widgets[id])), true, true, 0);
        d.pointer(Some([10000., -100.]), false, true, 1);
        assert_eq!(d.states[2].progress, 100);
        d.pointer(Some([-1000., -100.]), false, false, 2);
        assert_eq!(d.states[2].progress, 0);
        assert_eq!(d.capture, None);
        d.select(0);
        d.select(2);
        assert!(d.states[2].checked);
    }
    #[test]
    fn cancelling_drag_and_releasing_elsewhere_do_not_activate_controls() {
        let mut d = Demo::new();
        d.select(2);
        let before = d.states;
        let id = d
            .layout
            .widgets
            .iter()
            .position(|w| w.action == Action::Checkbox)
            .unwrap();
        d.pointer(Some(center(&d.layout.widgets[id])), true, true, 0);
        d.pointer(None, false, false, 1);
        assert_eq!(d.states, before);
        d.pointer(Some(center(&d.layout.widgets[id])), true, true, 0);
        d.cancel();
        d.pointer(Some(center(&d.layout.widgets[id])), false, false, 1);
        assert_eq!(d.states, before);
    }
    #[test]
    fn counter_bounds_and_text_limits_match_editor() {
        let mut d = Demo::new();
        d.select(2);
        d.states[2].count = 999;
        d.rebuild();
        let id = d
            .layout
            .widgets
            .iter()
            .position(|w| w.action == Action::Increment)
            .unwrap();
        assert!(d.layout.widgets[id].disabled);
        assert_ne!(d.hit(center(&d.layout.widgets[id])), Some(id));
        assert_eq!(text_rows("a\r\nb\tc"), vec!["a", "b  c"]);
        assert_eq!(text_rows(&"x".repeat(1000)).len(), 12);
        assert!(text_rows(&"x".repeat(1000)).iter().all(|r| r.len() == 40));
        assert_eq!(text_rows("ö"), vec!["?"]);
    }
    #[test]
    fn hover_changes_pixels_and_normal_surface_without_actions() {
        let mut d = Demo::new();
        let rest = d.raster();
        let initial = d.states;
        d.pointer(Some(center(&d.layout.widgets[0])), false, false, 0);
        let hovered = d.raster();
        assert_ne!(rest.colors, hovered.colors);
        assert_ne!(rest.heights, hovered.heights);
        assert_eq!(initial, d.states);
        let (base, normal) = textures(&hovered);
        assert_eq!(&base[..2], b"BM");
        assert_eq!(&normal[..2], b"BM");
        assert_eq!(base.len(), 54 + hovered.width * hovered.height * 64);
    }
}
