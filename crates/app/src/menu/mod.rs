//! The game's menu, over a film of the game (`menu_gfx`'s footage): a title screen (the logo and
//! its tagline), the planets one at a time (Mars 2036, two planets still to come), then Mars's
//! circuits as tiles in series. Two layouts share the same screens: wide (computer, landscape)
//! drawn in a 1280 × 720 design space, tall (phone, portrait) in 390 × 844; egui's zoom maps that
//! space to the window. The background is drawn by `menu_gfx`; this module says what it shows.
//!
//! The menu keeps its own state and tells the app what to do through [`Request`]s, and what to
//! play through [`Cue`]s.

pub(crate) mod catalog;
pub(crate) mod paint;
mod screens;

use egui::{Id, Rect, Response, Sense};

use crate::input::Nav;
use crate::menu_gfx::SkyScene;
use crate::ui_sound::{Ambience, Cue};
use catalog::{PLANETS, SLOTS, Series, TrackInfo};
pub use paint::fonts;

/// What the menu asks of the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// Build this map now (the loading screen is up).
    Build(usize),
    /// The loading is over: start the race on this map.
    Start(usize),
    ToggleMute,
}

/// What the menu shows of the game, each frame.
pub struct MenuInput<'a> {
    /// Best time per map (ticks), for the current profile.
    pub bests: &'a [Option<u32>],
    pub muted: bool,
    /// Seconds into the background footage's loop, once it plays.
    pub video_time: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Title,
    Planets,
    Solo,
}

impl Screen {
    fn order(self) -> i32 {
        self as i32
    }
}

struct Wipe {
    start: f64,
    dir: f32,
    to: Screen,
    swapped: bool,
}

struct Loading {
    start: f64,
    /// Index in the catalogue.
    track: usize,
    built: bool,
    ready: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shake {
    Cta,
    Tile(usize),
}

/// The two layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Wide,
    Tall,
}

impl Layout {
    /// Portrait windows take the phone layout.
    pub fn for_size(width: f32, height: f32) -> Self {
        if width < height * 0.8 { Layout::Tall } else { Layout::Wide }
    }

    /// egui zoom that fits the layout's design size into a window of this logical size.
    pub fn zoom(width: f32, height: f32) -> f32 {
        let z = match Self::for_size(width, height) {
            Layout::Wide => (width / 1280.0).min(height / 720.0),
            Layout::Tall => (width / 390.0).min(height / 844.0),
        };
        z.clamp(0.5, 4.0)
    }

    /// Circuit tiles per row.
    fn columns(self) -> usize {
        match self {
            Layout::Wide => 3,
            Layout::Tall => 2,
        }
    }
}

/// Cuts of the background footage, seconds into its loop (`tools/video/menu_montage.sh`): the
/// title screen flashes on each.
const CUTS: [f32; 8] = [0.0, 2.6, 4.8, 7.2, 9.4, 11.6, 13.8, 16.0];

pub struct Menu {
    /// The menu is on screen (otherwise the race is).
    pub active: bool,
    screen: Screen,
    entered: f64,
    /// Set when the menu opens from a race: the screen's time starts on the next frame.
    reopened: bool,
    tracks: Vec<TrackInfo>,
    // Title.
    /// The footage's cut on screen, and when the last one flashed.
    cut: Option<usize>,
    flash: f64,
    // Planets.
    planet: usize,
    /// Horizontal drag in progress on the planet screen, points.
    drag: Option<f32>,
    /// The info block's change of planet: shown index, previous index, start, direction.
    info: (usize, usize, f64, f32),
    rot: f32,
    last: f64,
    // Solo.
    series: Series,
    sel: usize,
    sel_at: f64,
    /// Keyboard and pad: the series switch has the focus instead of the tiles.
    on_series: bool,
    /// Tall layout: the circuit sheet, opened (true) or closing (false) since a time.
    sheet: Option<(bool, f64)>,
    // Transitions and feedback.
    wipe: Option<Wipe>,
    loading: Option<Loading>,
    outro: Option<(f64, usize)>,
    toast: Option<(String, f64)>,
    shake: Option<(Shake, f64)>,
    nav: Vec<Nav>,
    cues: Vec<Cue>,
    requests: Vec<Request>,
    hover_last: Option<Id>,
    hover_now: Option<Id>,
    parallax: f32,
    sky: SkyScene,
    /// Self-test: menu moves to make at given seconds (`MARS_MENU_NAV=1.5:right,3:confirm`).
    script: Vec<(f64, Nav)>,
}

impl Menu {
    pub fn new(maps: &[track::Map]) -> Self {
        Self {
            active: true,
            screen: Screen::Title,
            entered: 0.0,
            reopened: false,
            tracks: catalog::tracks(maps),
            cut: None,
            flash: -10.0,
            planet: 0,
            drag: None,
            info: (0, 0, -10.0, 1.0),
            rot: 0.0,
            last: 0.0,
            series: Series::Easy,
            sel: 0,
            sel_at: 0.0,
            on_series: false,
            sheet: None,
            wipe: None,
            loading: None,
            outro: None,
            toast: None,
            shake: None,
            nav: Vec::new(),
            cues: Vec::new(),
            requests: Vec::new(),
            hover_last: None,
            hover_now: None,
            parallax: 0.0,
            sky: SkyScene::default(),
            script: script_from_env(),
        }
    }

    /// Whether the menu draws this frame (it also covers the start of the race while its loading
    /// screen fades out).
    pub fn shows(&self) -> bool {
        self.active || self.outro.is_some()
    }

    /// The screen's background loop.
    pub fn ambience(&self) -> Ambience {
        if !self.active {
            return Ambience::Off;
        }
        match self.screen {
            Screen::Title => Ambience::Title,
            Screen::Planets => Ambience::Space,
            Screen::Solo => Ambience::Base,
        }
    }

    pub fn push_nav(&mut self, nav: Nav) {
        self.nav.push(nav);
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }

    /// Opens on a given screen (`title`, `planets`, `solo`), for checks.
    pub fn open_on(&mut self, screen: &str) {
        self.active = true;
        self.screen = match screen {
            "planets" => Screen::Planets,
            "solo" | "circuits" => Screen::Solo,
            _ => Screen::Title,
        };
    }

    /// Back from a race: the circuits, on the circuit just driven.
    pub fn open_from_race(&mut self, map: usize) {
        self.active = true;
        self.screen = Screen::Solo;
        self.reopened = true;
        self.wipe = None;
        self.loading = None;
        self.outro = None;
        self.sheet = None;
        self.on_series = false;
        self.series = Series::Easy;
        self.sel = self.tracks.iter().position(|t| t.map == map).unwrap_or(0);
        self.cues.push(Cue::Back);
    }

    /// The circuit in slot `i` of the current series, if built.
    fn slot(&self, i: usize) -> Option<&TrackInfo> {
        match self.series {
            Series::Easy => self.tracks.get(i).filter(|_| i < SLOTS),
            Series::Hard => None,
        }
    }

    fn go(&mut self, to: Screen, now: f64) {
        if self.wipe.is_some() || to == self.screen {
            return;
        }
        let dir = if to.order() > self.screen.order() { 1.0 } else { -1.0 };
        self.cues.push(Cue::Wipe(dir));
        self.sheet = None;
        self.wipe = Some(Wipe { start: now, dir, to, swapped: false });
    }

    fn back(&mut self, now: f64) {
        let to = match self.screen {
            Screen::Solo => Screen::Planets,
            Screen::Planets => Screen::Title,
            Screen::Title => return,
        };
        self.cues.push(Cue::Back);
        self.go(to, now);
    }

    fn start(&mut self, now: f64) {
        if self.screen == Screen::Title && self.wipe.is_none() {
            self.cues.push(Cue::Boot);
            self.go(Screen::Planets, now);
        }
    }

    fn toast(&mut self, text: &str, now: f64) {
        self.toast = Some((text.to_string(), now));
    }

    fn deny(&mut self, what: Shake, text: &str, now: f64) {
        self.cues.push(Cue::Deny);
        self.shake = Some((what, now));
        self.toast(text, now);
    }

    fn set_planet(&mut self, n: isize, now: f64) {
        if n < 0 || n as usize >= PLANETS.len() || n as usize == self.planet {
            return;
        }
        let n = n as usize;
        let dir = if n > self.planet { 1.0 } else { -1.0 };
        self.info = (n, self.planet, now, dir);
        self.planet = n;
        self.cues.push(Cue::Swipe(dir));
        if !PLANETS[n].open {
            self.cues.push(Cue::Static);
        }
    }

    fn choose_planet(&mut self, now: f64) {
        if PLANETS[self.planet].open {
            self.cues.push(Cue::Confirm);
            self.go(Screen::Solo, now);
        } else {
            self.deny(Shake::Cta, "Cette planète arrive bientôt.", now);
        }
    }

    fn set_sel(&mut self, i: usize, now: f64, sound: bool) {
        if i < SLOTS && i != self.sel {
            self.sel = i;
            self.sel_at = now;
            if sound {
                self.cues.push(Cue::Select);
            }
        }
    }

    fn set_series(&mut self, s: Series, now: f64) {
        if s != self.series {
            self.series = s;
            self.sel = 0;
            self.sel_at = now;
            self.sheet = None;
            self.cues.push(Cue::Tab);
        }
    }

    fn locked_text(&self) -> &'static str {
        match self.series {
            Series::Hard => "Termine la série Facile pour débloquer la série Dur.",
            Series::Easy => "Ce circuit est encore en construction.",
        }
    }

    fn open_sheet(&mut self, i: usize, now: f64) {
        self.set_sel(i, now, false);
        if self.slot(i).is_none() {
            let text = self.locked_text();
            self.deny(Shake::Tile(i), text, now);
            return;
        }
        self.cues.push(Cue::Confirm);
        self.cues.push(Cue::SheetOpen);
        self.sheet = Some((true, now));
    }

    fn close_sheet(&mut self, now: f64) {
        if matches!(self.sheet, Some((true, _))) {
            self.cues.push(Cue::SheetClose);
            self.sheet = Some((false, now));
        }
    }

    fn launch(&mut self, now: f64) {
        if self.loading.is_some() {
            return;
        }
        if self.slot(self.sel).is_none() {
            let text = self.locked_text();
            self.deny(Shake::Tile(self.sel), text, now);
            return;
        }
        self.cues.push(Cue::Launch);
        self.loading = Some(Loading { start: now, track: self.sel, built: false, ready: false });
    }

    /// Keyboard and pad on the circuits: the arrows move through the tiles, up from the first
    /// row reaches the series switch (left and right change the series there, down comes back).
    fn solo_nav(&mut self, nav: Nav, layout: Layout, now: f64) {
        let sheet_open = matches!(self.sheet, Some((true, _)));
        if sheet_open {
            match nav {
                Nav::Confirm => self.launch(now),
                Nav::Back => self.close_sheet(now),
                _ => {}
            }
            return;
        }
        if self.on_series {
            match nav {
                Nav::Left => self.set_series(Series::Easy, now),
                Nav::Right => self.set_series(Series::Hard, now),
                Nav::Down | Nav::Confirm => {
                    self.on_series = false;
                    self.cues.push(Cue::Select);
                }
                Nav::Back => self.back(now),
                _ => {}
            }
            return;
        }
        let cols = layout.columns();
        let (row, col) = (self.sel / cols, self.sel % cols);
        match nav {
            Nav::Left if col > 0 => self.set_sel(self.sel - 1, now, true),
            Nav::Right if col + 1 < cols && self.sel + 1 < SLOTS => self.set_sel(self.sel + 1, now, true),
            Nav::Up if row == 0 => {
                self.on_series = true;
                self.cues.push(Cue::Select);
            }
            Nav::Up => self.set_sel(self.sel - cols, now, true),
            Nav::Down if self.sel + cols < SLOTS => self.set_sel(self.sel + cols, now, true),
            Nav::Confirm => match layout {
                Layout::Tall => self.open_sheet(self.sel, now),
                Layout::Wide => self.launch(now),
            },
            Nav::Back => self.back(now),
            _ => {}
        }
    }

    fn handle_nav(&mut self, nav: Nav, layout: Layout, now: f64) {
        if self.loading.is_some() || self.wipe.is_some() {
            return;
        }
        match self.screen {
            Screen::Title => self.start(now),
            Screen::Planets => match nav {
                Nav::Left => self.set_planet(self.planet as isize - 1, now),
                Nav::Right => self.set_planet(self.planet as isize + 1, now),
                Nav::Confirm => self.choose_planet(now),
                Nav::Back => self.back(now),
                _ => {}
            },
            Screen::Solo => self.solo_nav(nav, layout, now),
        }
    }

    /// Timers: the screen change half-way through the wipe, the loading steps.
    fn advance(&mut self, now: f64) {
        if let Some(w) = &mut self.wipe {
            let t = now - w.start;
            if t >= 0.36 && !w.swapped {
                w.swapped = true;
                self.screen = w.to;
                self.entered = now;
                self.sel_at = now;
                self.on_series = false;
            }
            if t >= 0.72 {
                self.wipe = None;
            }
        }
        if let Some(l) = &mut self.loading {
            let t = now - l.start;
            let map = self.tracks[l.track].map;
            if t >= 0.3 && !l.built {
                l.built = true;
                self.requests.push(Request::Build(map));
            }
            if t >= 2.0 && !l.ready {
                l.ready = true;
                self.cues.push(Cue::Go);
            }
            if t >= 3.1 {
                self.requests.push(Request::Start(map));
                self.outro = Some((now, l.track));
                self.loading = None;
                self.active = false;
                self.sheet = None;
            }
        }
        if self.outro.is_some_and(|(t0, _)| now - t0 > 0.4) {
            self.outro = None;
        }
        if self.toast.as_ref().is_some_and(|(_, t)| now - t > 2.7) {
            self.toast = None;
        }
    }

    /// Fade and rise of the `i`-th element of a screen as it comes in.
    fn enter(&self, i: usize, now: f64) -> (f32, f32) {
        let t = (now - self.entered) as f32 - 0.09 - i as f32 * 0.06;
        let e = paint::ease_out(t / 0.5);
        (e, (1.0 - e) * 16.0)
    }

    /// Horizontal shake of an element denied, in points.
    fn shake_x(&self, what: Shake, now: f64) -> f32 {
        let Some((w, t0)) = self.shake else { return 0.0 };
        if w != what {
            return 0.0;
        }
        let t = ((now - t0) / 0.36) as f32;
        if t >= 1.0 {
            return 0.0;
        }
        let keys = [0.0, -7.0, 6.0, -4.0, 2.0, 0.0];
        let x = t * 5.0;
        let i = (x as usize).min(4);
        let k = x - i as f32;
        keys[i] + (keys[i + 1] - keys[i]) * k
    }

    /// A clickable area; a new hover plays a tick (unless `select` handles hovering itself).
    fn hit(&mut self, ui: &mut egui::Ui, rect: Rect, id: Id, tick: bool) -> Response {
        let r = ui.interact(rect, id, Sense::click());
        if r.hovered() {
            self.hover_now = Some(id);
            if tick && self.hover_last != Some(id) {
                self.cues.push(Cue::Tick);
            }
        }
        r
    }

    /// Draws the menu (or, while the race starts, its loading screen fading out) and returns
    /// the background to render under it, in physical pixels.
    pub fn ui(&mut self, ui: &mut egui::Ui, input: MenuInput) -> &SkyScene {
        let now = ui.input(|i| i.time);
        let screen_rect = ui.ctx().content_rect();
        let layout = Layout::for_size(screen_rect.width(), screen_rect.height());
        let dt = (now - self.last).clamp(0.0, 0.1) as f32;
        self.last = now;
        if self.reopened {
            self.reopened = false;
            self.entered = now;
            self.sel_at = now;
        }
        self.sky = SkyScene { time: now as f32, ..Default::default() };
        self.hover_now = None;
        while self.script.first().is_some_and(|(t, _)| now >= *t) {
            let (_, n) = self.script.remove(0);
            self.nav.push(n);
        }
        for n in std::mem::take(&mut self.nav) {
            self.handle_nav(n, layout, now);
        }
        self.advance(now);
        if !self.active {
            if let Some((t0, track)) = self.outro {
                let a = 1.0 - ((now - t0) as f32 / 0.4).clamp(0.0, 1.0);
                self.loading_overlay(ui, screen_rect, layout, track, 3.1, a, now);
            }
            return &self.sky;
        }
        self.rot -= 0.07 * dt;
        let target = self.screen.order() as f32 * 1.6 + self.planet as f32 * 0.8;
        self.parallax += (target - self.parallax) * (dt * 3.0).min(1.0);
        self.sky.parallax = self.parallax;
        // The footage's cuts.
        if let Some(t) = input.video_time {
            let cut = CUTS.iter().rposition(|&c| t >= c).unwrap_or(0);
            if self.cut.is_some_and(|c| c != cut) {
                self.flash = now;
            }
            self.cut = Some(cut);
        }

        let covered = self.loading.as_ref().is_some_and(|l| now - l.start > 0.3);
        if !covered {
            match self.screen {
                Screen::Title => self.title(ui, screen_rect, layout, now),
                Screen::Planets => self.planets(ui, screen_rect, layout, now, input.muted),
                Screen::Solo => self.solo(ui, screen_rect, layout, now, input.muted, input.bests),
            }
        }
        self.toast_ui(ui, screen_rect, layout, now);
        if let Some(l) = &self.loading {
            let (t, track) = ((now - l.start) as f32, l.track);
            let a = (t / 0.25).clamp(0.0, 1.0);
            self.loading_overlay(ui, screen_rect, layout, track, t, a, now);
        }
        if let Some(w) = &self.wipe {
            let t = ((now - w.start) / 0.72) as f32;
            let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("menu wipe")));
            paint::wipe(&p, screen_rect, t, w.dir);
        }
        self.hover_last = self.hover_now;

        // Points to physical pixels for the GPU.
        let ppp = ui.ctx().pixels_per_point();
        for p in &mut self.sky.planets {
            p.center *= ppp;
            p.radius *= ppp;
            for c in &mut p.clip {
                *c *= ppp;
            }
        }
        for g in &mut self.sky.glow {
            *g *= ppp;
        }
        &self.sky
    }
}

fn script_from_env() -> Vec<(f64, Nav)> {
    let Ok(s) = std::env::var("MARS_MENU_NAV") else { return Vec::new() };
    s.split(',')
        .filter_map(|item| {
            let (t, n) = item.trim().split_once(':')?;
            let nav = match n.trim().to_lowercase().as_str() {
                "left" => Nav::Left,
                "right" => Nav::Right,
                "up" => Nav::Up,
                "down" => Nav::Down,
                "confirm" => Nav::Confirm,
                "back" => Nav::Back,
                _ => Nav::Any,
            };
            Some((t.trim().parse().ok()?, nav))
        })
        .collect()
}
