//! The debug panel (egui, shown with Tab): profiles, records, telemetry and the live tuning; and
//! the frame counter. The race HUD itself is hud.rs.

use std::time::Instant;

use egui::{Align2, Color32, RichText, vec2};

use crate::game::Game;
use crate::race::format_time;

pub struct Fps {
    frames: u32,
    since: Instant,
    pub fps: f32,
    pub frame_ms: f32,
}

impl Fps {
    pub fn new() -> Self {
        Self { frames: 0, since: Instant::now(), fps: 0.0, frame_ms: 0.0 }
    }

    pub fn frame(&mut self) {
        self.frames += 1;
        let t = self.since.elapsed().as_secs_f32();
        if t >= 0.5 {
            self.fps = self.frames as f32 / t;
            self.frame_ms = 1000.0 * t / self.frames as f32;
            self.frames = 0;
            self.since = Instant::now();
        }
    }
}

/// The debug panel (Tab): profiles, records, telemetry and the live tuning.
pub fn panel(ctx: &egui::Context, game: &mut Game, t: physics::Telemetry) {
    let mut select = None;
    let mut select_map = None;
    let mut toggle = None;
    let mut changed = false;
    let mut reset = false;
    let mut clear = false;
    let mut restart = false;
    egui::Window::new("Profils et réglages")
        .anchor(Align2::RIGHT_TOP, vec2(-12.0, 12.0))
        .default_width(380.0)
        .default_height(ctx.content_rect().height() - 40.0)
        .resizable(true)
        .vscroll(true)
        .show(ctx, |ui| {
            let map_key = game.map_key();
            ui.horizontal_wrapped(|ui| {
                ui.label("Map :");
                for (i, m) in game.maps.iter().enumerate() {
                    if ui.selectable_label(i == game.map_index, &m.name).clicked() {
                        select_map = Some(i);
                    }
                }
            });
            ui.separator();
            egui::Grid::new("profiles").striped(true).num_columns(5).show(ui, |ui| {
                ui.label("");
                ui.label("Profil");
                ui.label("Record");
                ui.label("Essais");
                ui.label("");
                ui.end_row();
                for (i, p) in game.session.profiles.iter().enumerate() {
                    ui.label(RichText::new(format!("{}", i + 1)).monospace());
                    let mut name = RichText::new(&p.params.name);
                    if p.eliminated {
                        name = name.strikethrough().color(Color32::from_gray(120));
                    }
                    if ui.selectable_label(i == game.session.current, name).on_hover_text(&p.params.description).clicked() {
                        select = Some(i);
                    }
                    let best = match (p.best(&map_key), p.current_best(&map_key)) {
                        (Some(_), Some(b)) => RichText::new(format_time(b.ticks)).monospace(),
                        (Some(b), None) => RichText::new(format_time(b.ticks)).monospace().color(Color32::from_gray(120)),
                        _ => RichText::new("-").monospace(),
                    };
                    ui.label(best).on_hover_text("En gris : record fait avec d'anciens réglages");
                    ui.label(RichText::new(format!("{}", p.runs)).monospace());
                    if ui.small_button(if p.eliminated { "Remettre" } else { "Éliminer" }).clicked() {
                        toggle = Some(i);
                    }
                    ui.end_row();
                }
            });
            let p = game.session.profile();
            ui.label(RichText::new(&p.params.description).italics());
            ui.horizontal(|ui| {
                restart |= ui.button("Recommencer").clicked();
                reset |= ui.button("Réglages d'origine").clicked();
                clear |= ui.button("Effacer le record").clicked();
            });

            ui.separator();
            ui.heading("Télémétrie");
            egui::Grid::new("telemetry").num_columns(2).show(ui, |ui| {
                let row = |ui: &mut egui::Ui, k: &str, v: String| {
                    ui.label(k);
                    ui.label(RichText::new(v).monospace());
                    ui.end_row();
                };
                row(ui, "Vitesse", format!("{:6.1} km/h", t.speed_kmh));
                row(ui, "Vitesse avant", format!("{:6.1} m/s", t.forward_speed));
                row(ui, "Angle de glisse", format!("{:6.1}°", t.slip_angle_deg));
                row(ui, "Accélération latérale", format!("{:6.2} g", t.lateral_g));
                row(ui, "Usage du grip", format!("{:5.0} %", t.grip_usage * 100.0));
                row(ui, "Angle de dérive", format!("{:6.1}°", t.drift_angle_deg));
                row(ui, "Lacet", format!("{:6.2} rad/s", t.yaw_rate));
                row(ui, "En l'air / glisse", format!("{} / {}", if t.airborne { "oui" } else { "non" }, if t.sliding { "oui" } else { "non" }));
                row(ui, "Contact mur", (if t.wall_contact { "oui" } else { "non" }).to_string());
            });
            let names = ["AvG", "AvD", "ArG", "ArD"];
            for (w, name) in game.run.car.state.wheels.iter().zip(names) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(name).monospace());
                    let surface = match w.surface {
                        Some(track::Surface::Road) => "route",
                        Some(track::Surface::Dirt) => "terre",
                        Some(track::Surface::Ground) => "sol",
                        Some(track::Surface::Wall) => "mur",
                        None => "-",
                    };
                    ui.label(RichText::new(format!("{:5}", if w.contact { surface } else { "air" })).monospace());
                    ui.add(egui::ProgressBar::new(w.mark.clamp(0.0, 1.0)).desired_width(120.0).text(format!("trace {:.0}% · flou {:.0}%", w.mark * 100.0, w.smear * 100.0)));
                });
            }

            ui.separator();
            ui.heading("Réglages");
            ui.label(RichText::new("S'appliquent en direct. Enregistrés dans tuning/session.json.").small());
            let profile = game.session.profile_mut();
            let tunables = profile.params.tunables();
            let mut groups: Vec<&'static str> = Vec::new();
            for t in &tunables {
                if !groups.contains(&t.group) {
                    groups.push(t.group);
                }
            }
            let mut tunables = tunables;
            for group in groups {
                egui::CollapsingHeader::new(group).default_open(false).show(ui, |ui| {
                    for t in tunables.iter_mut().filter(|t| t.group == group) {
                        let (min, max) = (t.min, t.max);
                        let resp = ui.add(egui::Slider::new(&mut *t.value, min..=max).text(t.name));
                        changed |= resp.changed();
                    }
                });
            }
        });

    if let Some(i) = toggle {
        game.toggle_eliminated(i);
    }
    if changed {
        game.params_changed();
    }
    if reset {
        game.reset_params();
    }
    if clear {
        game.clear_best();
    }
    if restart {
        game.restart();
    }
    if let Some(i) = select {
        game.select_profile(i);
    }
    if let Some(i) = select_map {
        game.select_map(i);
    }
}
