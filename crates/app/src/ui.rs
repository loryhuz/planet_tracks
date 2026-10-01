//! HUD and the tuning panel (egui). Every changing number uses a monospace font.

use std::time::Instant;

use egui::{Align2, Color32, FontId, RichText, vec2};

use crate::camera::MODES;
use crate::game::Game;
use crate::race::{COUNTDOWN_TICKS, format_time};

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

fn hud_frame() -> egui::Frame {
    egui::Frame::new().fill(Color32::from_black_alpha(110)).corner_radius(6.0).inner_margin(8.0)
}

fn mono(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::monospace(size)).color(Color32::WHITE)
}

pub fn draw(ui: &mut egui::Ui, game: &mut Game, fps: &Fps) {
    let ctx = ui.ctx().clone();
    let telemetry = game.telemetry();

    // Top left: FPS (always), profile, camera.
    egui::Area::new(egui::Id::new("fps")).anchor(Align2::LEFT_TOP, vec2(12.0, 10.0)).interactable(false).show(&ctx, |ui| {
        hud_frame().show(ui, |ui| {
            ui.add(egui::Label::new(mono(format!("{:>4.0} FPS {:>5.1} ms", fps.fps, fps.frame_ms), 15.0)).extend());
            let p = game.session.profile();
            ui.label(RichText::new(format!("{} · Profil {} · {}", game.map_name(), game.session.current + 1, p.params.name)).color(Color32::WHITE).size(14.0));
            ui.label(RichText::new(format!("Caméra : {}", MODES[game.camera.mode])).color(Color32::from_gray(200)).size(12.0));
        });
    });

    // Top centre: race time.
    let run = &game.run;
    let time = run.finished.unwrap_or(run.tick);
    egui::Area::new(egui::Id::new("timer")).anchor(Align2::CENTER_TOP, vec2(0.0, 12.0)).interactable(false).show(&ctx, |ui| {
        hud_frame().show(ui, |ui| {
            ui.label(mono(format_time(time), 34.0));
            let n = game.track.checkpoints.len();
            if n > 0 {
                ui.label(mono(format!("CP {}/{}", run.splits.len(), n), 14.0));
            }
        });
    });

    // Checkpoint / finish popup.
    if let Some(p) = &game.popup {
        if Instant::now() < p.until {
            egui::Area::new(egui::Id::new("popup")).anchor(Align2::CENTER_TOP, vec2(0.0, 110.0)).interactable(false).show(&ctx, |ui| {
                hud_frame().show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(&p.title).color(Color32::WHITE).size(18.0));
                        ui.label(mono(&p.time, 28.0));
                        if let Some(d) = p.delta {
                            let color = if d <= 0 { Color32::from_rgb(90, 160, 255) } else { Color32::from_rgb(255, 90, 80) };
                            ui.label(mono(Game::delta_text(d), 20.0).color(color));
                        }
                        if game.run.finished.is_some() {
                            ui.label(RichText::new("Entrée ou Retour arrière pour recommencer").color(Color32::from_gray(210)).size(13.0));
                        }
                    });
                });
            });
        }
    }

    // Countdown.
    if run.countdown > 0 || (run.tick < 60 && run.finished.is_none()) {
        let text = if run.countdown > 0 { format!("{}", 1 + run.countdown * 3 / COUNTDOWN_TICKS) } else { "GO".into() };
        egui::Area::new(egui::Id::new("countdown")).anchor(Align2::CENTER_CENTER, vec2(0.0, -60.0)).interactable(false).show(&ctx, |ui| {
            ui.add(egui::Label::new(mono(text, 72.0)).extend());
        });
    }

    // Bottom centre: speed.
    egui::Area::new(egui::Id::new("speed")).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -18.0)).interactable(false).show(&ctx, |ui| {
        hud_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(mono(format!("{:>3.0}", telemetry.speed_kmh), 40.0));
                ui.label(RichText::new("km/h").color(Color32::from_gray(210)).size(14.0));
            });
        });
    });

    // Bottom left: keys.
    egui::Area::new(egui::Id::new("help")).anchor(Align2::LEFT_BOTTOM, vec2(12.0, -10.0)).interactable(false).show(&ctx, |ui| {
        hud_frame().show(ui, |ui| {
            let c = Color32::from_gray(215);
            ui.label(RichText::new("Haut/W : gaz · Bas/S : frein · Gauche/Droite ou A/D : tourner · Entrée : dernier CP · Retour arrière : recommencer").color(c).size(12.0));
            ui.label(RichText::new("1-8 : profil · PgUp/PgDn : profil suivant · X : éliminer · Tab : réglages · C : caméra · F : plein écran · M : son · N : map suivante · T : textures").color(c).size(12.0));
            if let Some(name) = &game.controls.gamepad_name {
                ui.label(RichText::new(format!("Manette : {name} (RT gaz, LT frein, B dernier CP, Y recommencer, LB/RB profil)")).color(c).size(12.0));
            }
        });
    });

    if game.panel_open {
        panel(&ctx, game, telemetry);
    }
}

fn panel(ctx: &egui::Context, game: &mut Game, t: physics::Telemetry) {
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
            let mut textures = game.session.textures;
            if ui
                .checkbox(&mut textures, "Textures des surfaces (T)")
                .on_hover_text("Décoché : l'ancien rendu procédural, pour comparer")
                .changed()
            {
                game.session.toggle_textures();
            }
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
