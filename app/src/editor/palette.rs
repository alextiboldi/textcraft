//! The command palette: commands, files and line numbers.
use super::{Editor, Overlay};
use crate::commands::{Cmd, CMDS};
use crate::i18n::num;
use crate::theme::{ACCENT, BG_LINE, BG_SIDE, FG, FG_DIM};
use eframe::egui::{self, Color32, Key, Pos2, RichText, Sense};
use std::path::PathBuf;
use textcraft_core::fuzzy;

impl Editor {
    pub(super) fn palette(&mut self, ctx: &egui::Context) {
        // items: (label, detail, action)
        enum Act {
            Cmd(Cmd),
            File(PathBuf),
            Line(usize),
        }
        let q = self.palette_query.clone();
        let mut items: Vec<(String, String, Act)> = vec![];
        if let Some(rest) = q.strip_prefix('>') {
            let mut scored: Vec<_> = CMDS
                .iter()
                .filter_map(|&(c, id, l, _)| {
                    let l = tid!(id, l);
                    fuzzy(l, rest.trim()).map(|s| (s, c, id, l))
                })
                .collect();
            scored.sort_by_key(|x| x.0);
            items = scored.into_iter().map(|(_, c, id, l)| (l.to_string(), self.shortcut_text(id), Act::Cmd(c))).collect();
        } else if let Some(rest) = q.strip_prefix(':') {
            let n: usize = rest.trim().parse().unwrap_or(0);
            let total = self.docs[self.active].text.matches('\n').count() + 1;
            if n > 0 {
                items.push((tf!("Go to line {n}", n = num(n.min(total))), String::new(), Act::Line(n.min(total))));
            } else {
                items.push((tf!("Type a line number between {first} and {last}", first = num(1), last = num(total)), String::new(), Act::Line(0)));
            }
        } else {
            let root = self.folder.clone().unwrap_or_default();
            let mut scored: Vec<_> = self
                .file_index
                .iter()
                .filter_map(|p| {
                    let rel = p.strip_prefix(&root).unwrap_or(p).to_string_lossy().into_owned();
                    fuzzy(&rel, q.trim()).map(|s| (s, rel, p.clone()))
                })
                .collect();
            scored.sort_by_key(|x| x.0);
            items = scored
                .into_iter()
                .take(200)
                .map(|(_, rel, p)| {
                    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    (name, rel, Act::File(p))
                })
                .collect();
            if self.folder.is_none() {
                items.push((tl!("Open a folder to search its files").into(), String::new(), Act::Cmd(Cmd::OpenFolder)));
            }
        }
        self.palette_sel = self.palette_sel.min(items.len().saturating_sub(1));
        if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
            self.palette_sel = (self.palette_sel + 1).min(items.len().saturating_sub(1));
        }
        if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
            self.palette_sel = self.palette_sel.saturating_sub(1);
        }

        let mut run: Option<usize> = None;
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::unique("palette"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(screen.center().x - 260.0, screen.top() + 60.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).fill(BG_SIDE).inner_margin(8).show(ui, |ui| {
                    ui.set_width(520.0);
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.palette_query)
                            .hint_text(tl!("Search files, > for commands, : for line"))
                            .desired_width(f32::INFINITY),
                    );
                    r.request_focus();
                    if r.changed() {
                        self.palette_sel = 0;
                    }
                    if ui.input(|i| i.key_pressed(Key::Enter)) && !items.is_empty() {
                        run = Some(self.palette_sel);
                    }
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                        for (i, (label, detail, _)) in items.iter().enumerate() {
                            let sel = i == self.palette_sel;
                            let fr = egui::Frame::NONE
                                .fill(if sel { BG_LINE } else { Color32::TRANSPARENT })
                                .inner_margin(egui::Margin::symmetric(8, 4))
                                .corner_radius(4);
                            let resp = fr
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(label).color(if sel { ACCENT } else { FG }));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(RichText::new(detail).size(11.0).color(FG_DIM));
                                        });
                                    });
                                })
                                .response;
                            if sel {
                                resp.scroll_to_me(None);
                            }
                            if ui.interact(resp.rect, ui.scope_id().with(("pi", i)), Sense::click()).clicked() {
                                run = Some(i);
                            }
                        }
                    });
                });
            });
        if let Some(i) = run {
            let act = items.swap_remove(i).2;
            self.overlay = Overlay::None;
            match act {
                Act::Cmd(c) => self.run_command(ctx, c),
                Act::File(p) => self.open_path(p),
                Act::Line(n) if n > 0 => self.goto_line(n),
                Act::Line(_) => {}
            }
        }
    }
}
