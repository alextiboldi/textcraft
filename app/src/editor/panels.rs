//! The menu bar, status bar, find bar, side bar and tab bar.
use super::{title, Editor};
use crate::commands::{menu_label, CMDS, MENUS};
use crate::i18n::num;
use crate::theme::*;
use craft_ui::buttons::IconButton;
use eframe::egui::{self, FontId, Key, RichText, Sense, Stroke, Vec2};
use lucide_icons::Icon;
use std::path::{Path, PathBuf};
use textcraft_core::find_all;

impl Editor {
    pub(super) fn menu_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Panel::top("menu").frame(egui::Frame::NONE.fill(BG_TABBAR).inner_margin(egui::Margin::symmetric(8, 3))).show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                if self.logo_tex.is_none() {
                    self.logo_tex =
                        logo_rgba(64).map(|px| ctx.load_texture("logo", egui::ColorImage::from_rgba_unmultiplied([64, 64], &px), Default::default()));
                }
                if let Some(tex) = &self.logo_tex {
                    ui.add(egui::Image::new((tex.id(), Vec2::splat(18.0))));
                }
                ui.label(RichText::new("TextCraft").strong().color(FG));
                ui.add_space(8.0);
                let mut chosen = None;
                for (name, items) in MENUS {
                    ui.menu_button(tl!(name), |ui| {
                        for &i in items {
                            let (cmd, id, label, _) = CMDS[i];
                            // The short label is its own key, so shared rows ("Open…", "Save") translate it.
                            let short = tl!(menu_label(label));
                            if ui.add(egui::Button::new(short).shortcut_text(self.shortcut_text(id))).clicked() {
                                chosen = Some(cmd);
                            }
                        }
                    });
                }
                if let Some(c) = chosen {
                    self.run_command(ctx, c);
                }
            });
        });
    }

    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        let lang = match self.docs[self.active].lang().name {
            // Generic names; the others are proper names and stay as written.
            "Plain Text" => tl!("Plain Text"),
            "Shell Script" => tl!("Shell Script"),
            name => name,
        };
        egui::Panel::bottom("status").frame(egui::Frame::NONE.fill(BG_TABBAR).inner_margin(egui::Margin::symmetric(10, 4))).show(ui, |ui| {
            ui.horizontal(|ui| {
                let small = |s: String| RichText::new(s).size(12.0).color(FG_DIM);
                let toggle =
                    IconButton::new(tl!("Side bar"), Vec2::splat(18.0), &PLAIN_ICON).selected(self.show_sidebar).show(ui, |ui, rect, tint| {
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon(Icon::PanelLeft), FontId::proportional(12.0), tint);
                    });
                if toggle.on_hover_text(tl!("Toggle side bar")).clicked() {
                    self.show_sidebar = !self.show_sidebar;
                }
                ui.label(small(tf!("Line {line}, Column {column}", line = num(self.cursor_line), column = num(self.cursor_col))));
                if !self.status.is_empty() {
                    ui.label(small(format!("·  {}", self.status)));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(small(lang.to_string()));
                    ui.label(small(tf!("Spaces: {n}", n = num(4))));
                    ui.label(small("UTF-8".into()));
                });
            });
        });
    }

    pub(super) fn find_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("find").frame(egui::Frame::NONE.fill(BG_SIDE).inner_margin(egui::Margin::symmetric(10, 6))).show(ui, |ui| {
            let n = find_all(&self.docs[self.active].text, &self.find_text, self.find_case).len();
            ui.horizontal(|ui| {
                ui.toggle_value(&mut self.find_case, RichText::new(icon(Icon::CaseSensitive))).on_hover_text(tl!("Case Sensitive"));
                let r = ui.add(egui::TextEdit::singleline(&mut self.find_text).hint_text(tl!("Find")).desired_width(320.0));
                if self.find_focus {
                    r.request_focus();
                    self.find_focus = false;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    let back = ui.input(|i| i.modifiers.shift);
                    self.find_step(!back);
                    r.request_focus();
                }
                if ui.button(format!("{} {}", icon(Icon::ArrowDown), tl!("Find"))).clicked() {
                    self.find_step(true);
                }
                if ui.button(format!("{} {}", icon(Icon::ArrowUp), tl!("Previous"))).clicked() {
                    self.find_step(false);
                }
                ui.label(RichText::new(if self.find_text.is_empty() { String::new() } else { trn!(n, "{n} match", "{n} matches") }).color(FG_DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, &framed_icon(ui), tl!("Close find bar"), Icon::X, 14.0).on_hover_text(tl!("Close")).clicked() {
                        self.find_open = false;
                    }
                    ui.toggle_value(&mut self.replace_open, icon(Icon::Replace)).on_hover_text(tl!("Show replace"));
                });
            });
            if self.replace_open {
                ui.horizontal(|ui| {
                    ui.add_space(30.0);
                    ui.add(egui::TextEdit::singleline(&mut self.replace_text).hint_text(tl!("Replace")).desired_width(320.0));
                    if ui.button(format!("{} {}", icon(Icon::Replace), tl!("Replace"))).clicked() {
                        self.replace_one();
                    }
                    if ui.button(format!("{} {}", icon(Icon::ReplaceAll), tl!("Replace All"))).clicked() {
                        self.replace_all();
                    }
                });
            }
        });
    }

    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui) {
        let mut open: Option<PathBuf> = None;
        let mut switch_to = None;
        let mut close = None;
        egui::Panel::left("side").default_size(220.0).frame(egui::Frame::NONE.fill(BG_SIDE).inner_margin(egui::Margin::symmetric(8, 8))).show(
            ui,
            |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.label(RichText::new(tl!("OPEN FILES")).size(11.0).color(FG_DIM).strong());
                    for (i, d) in self.docs.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let mark = if d.dirty() { icon(Icon::Circle) } else { icon(Icon::X) };
                            if ui.add(egui::Button::new(RichText::new(mark).size(11.0).color(FG_DIM)).frame(false)).clicked() {
                                close = Some(i);
                            }
                            let col = if i == self.active { FG } else { FG_DIM };
                            if ui.add(egui::Label::new(RichText::new(title(d)).color(col)).sense(Sense::click())).clicked() {
                                switch_to = Some(i);
                            }
                        });
                    }
                    if let Some(folder) = &self.folder {
                        ui.add_space(10.0);
                        ui.label(RichText::new(tl!("FOLDERS")).size(11.0).color(FG_DIM).strong());
                        let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| folder.display().to_string());
                        let active_path = self.docs[self.active].path.clone();
                        dir_tree(ui, folder, &name, true, &active_path, &mut open);
                    }
                });
            },
        );
        if let Some(i) = switch_to {
            self.active = i;
        }
        if let Some(i) = close {
            self.close_tab(i);
        }
        if let Some(p) = open {
            self.open_path(p);
        }
    }

    pub(super) fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let mut switch_to = None;
        let mut close = None;
        egui::Frame::NONE.fill(BG_TABBAR).inner_margin(egui::Margin { left: 4, right: 4, top: 4, bottom: 0 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::horizontal().id_salt("tabs").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (i, d) in self.docs.iter().enumerate() {
                        let active = i == self.active;
                        let fill = if active { BG } else { BG_SIDE };
                        let frame = egui::Frame::NONE
                            .fill(fill)
                            .corner_radius(egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 })
                            .inner_margin(egui::Margin::symmetric(12, 6));
                        let r = frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(icon(Icon::FileText)).size(12.0).color(if active { ACCENT } else { FG_DIM }));
                                ui.label(RichText::new(title(d)).color(if active { FG } else { FG_DIM }));
                                let (mark, label) =
                                    if d.dirty() { (Icon::Circle, tl!("Close tab, unsaved changes")) } else { (Icon::X, tl!("Close tab")) };
                                if icon_button(ui, &PLAIN_ICON, label, mark, 11.0).on_hover_text(tl!("Close")).clicked() {
                                    close = Some(i);
                                }
                            });
                        });
                        let title = title(d);
                        let tab =
                            craft_ui::tabs::Tab::new(ui.scope_id().with(("tab", i)), &title, active).show_at(ui, r.response.rect, |ui, resp| {
                                if active {
                                    ui.painter().hline(resp.rect.x_range(), resp.rect.top(), Stroke::new(2.0, ACCENT));
                                }
                            });
                        if tab.clicked() {
                            switch_to = Some(i);
                        }
                    }
                    if icon_button(ui, &PLAIN_ICON, tl!("New file"), Icon::Plus, 14.0).on_hover_text(tl!("New file")).clicked() {
                        self.new_file();
                    }
                });
            });
        });
        if let Some(i) = switch_to {
            self.active = i;
        }
        if let Some(i) = close {
            self.close_tab(i);
        }
    }
}

fn dir_tree(ui: &mut egui::Ui, dir: &Path, name: &str, open_default: bool, active: &Option<PathBuf>, open: &mut Option<PathBuf>) {
    egui::CollapsingHeader::new(RichText::new(format!("{} {}", icon(Icon::Folder), name)).color(FG)).id_salt(dir).default_open(open_default).show(
        ui,
        |ui| {
            let Ok(rd) = std::fs::read_dir(dir) else { return };
            let mut entries: Vec<_> = rd.flatten().filter(|e| !e.file_name().to_string_lossy().starts_with('.')).collect();
            entries.sort_by_key(|e| (!e.path().is_dir(), e.file_name().to_ascii_lowercase()));
            // ponytail: re-reads the folder every frame while open; cache if it gets slow on network drives.
            for e in entries {
                let p = e.path();
                let n = e.file_name().to_string_lossy().into_owned();
                if p.is_dir() {
                    dir_tree(ui, &p, &n, false, active, open);
                } else {
                    let is_active = active.as_ref() == Some(&p);
                    let txt = RichText::new(format!("{} {}", icon(Icon::File), n)).color(if is_active { ACCENT } else { FG_DIM });
                    if ui.add(egui::Label::new(txt).sense(Sense::click())).clicked() {
                        *open = Some(p);
                    }
                }
            }
        },
    );
}
