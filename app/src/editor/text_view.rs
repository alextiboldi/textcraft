//! The text area with its line numbers, and the minimap.
use super::{editor_id, title, Editor};
use crate::theme::{tok_color, BG, BG_LINE, C_MATCH, FG, FG_DIM};
use eframe::egui::{self, text::LayoutJob, Color32, FontId, Pos2, Rect, Sense, Stroke, TextFormat, Vec2};
use egui::text::{CCursor, CCursorRange};
use textcraft_core::{find_all, tokenize};

impl Editor {
    pub(super) fn editor_area(&mut self, ui: &mut egui::Ui) {
        let full = ui.available_rect_before_wrap();
        let mini_w = if self.show_minimap { 110.0 } else { 0.0 };
        let edit_rect = Rect::from_min_max(full.min, Pos2::new(full.max.x - mini_w, full.max.y));
        let mini_rect = Rect::from_min_max(Pos2::new(full.max.x - mini_w, full.min.y), full.max);

        let font = FontId::monospace(self.font_size);
        let lang = self.docs[self.active].lang();
        let (find_text, find_case, find_open) = (self.find_text.clone(), self.find_case, self.find_open);
        let wrap = self.word_wrap;
        let doc_id = self.docs[self.active].id;
        let edit_id = editor_id(doc_id);
        // A newly shown document (new, opened, switched to, or left after a close) takes the
        // keyboard once, so typing goes to it; after that, focus stays where the user puts it.
        if self.editor_doc != Some(doc_id) {
            self.editor_doc = Some(doc_id);
            ui.ctx().memory_mut(|m| m.request_focus(edit_id));
        }

        // apply a pending cursor move (go to line, find)
        if let Some((s, e)) = self.pending_select.take() {
            if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), edit_id) {
                st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(s), CCursor::new(e))));
                st.store(ui.ctx(), edit_id);
            }
            ui.ctx().memory_mut(|m| m.request_focus(edit_id));
        }

        // Named after its tab, so a screen reader says which document the keyboard is in.
        let name = title(&self.docs[self.active]);
        let doc = &mut self.docs[self.active];
        let line_count = doc.text.matches('\n').count() + 1;
        let cur_line = self.cursor_line;
        let font_g = font.clone();

        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let text = buf.as_str();
            let mut job = LayoutJob::default();
            let matches = if find_open { find_all(text, &find_text, find_case) } else { vec![] };
            for (s, e, t) in tokenize(text, &lang) {
                // split a token where search matches start/end so they get a background
                let mut cuts = vec![s, e];
                for &(ms, me) in &matches {
                    if ms > s && ms < e {
                        cuts.push(ms);
                    }
                    if me > s && me < e {
                        cuts.push(me);
                    }
                }
                cuts.sort_unstable();
                for w in cuts.windows(2) {
                    let hit = matches.iter().any(|&(ms, me)| w[0] >= ms && w[1] <= me);
                    let fmt = TextFormat {
                        font_id: font_g.clone(),
                        color: tok_color(t),
                        background: if hit { C_MATCH } else { Color32::TRANSPARENT },
                        ..Default::default()
                    };
                    job.append(&text[w[0]..w[1]], 0.0, fmt);
                }
            }
            job.wrap.max_width = if wrap { wrap_width } else { f32::INFINITY };
            ui.painter().layout_job(job)
        };

        let mut out_cursor = None;
        let mut scroll = egui::ScrollArea::both().id_salt(("scroll", doc_id)).auto_shrink([false, false]);
        if let Some(y) = self.scroll_to.take() {
            scroll = scroll.vertical_scroll_offset(y);
        }
        let gutter_w = (line_count.to_string().len().max(3) as f32) * self.font_size * 0.62 + 20.0;
        let mut row_h = self.row_height;

        let sa = ui.scope_builder(egui::UiBuilder::new().max_rect(edit_rect), |ui| {
            scroll.show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let (gutter_rect, _) = ui.allocate_exact_size(Vec2::new(gutter_w, 0.0), Sense::hover());
                    // reserve a slot behind the text for the current-line highlight
                    let hl_slot = ui.painter().add(egui::Shape::Noop);
                    let out = egui::TextEdit::multiline(&mut doc.text)
                        .id(edit_id)
                        .font(font.clone())
                        .code_editor()
                        .frame(egui::Frame::NONE)
                        .lock_focus(true)
                        .desired_width(if wrap { ui.available_width() } else { f32::INFINITY })
                        .desired_rows(40)
                        .layouter(&mut layouter)
                        .show(ui);
                    out_cursor = out.cursor_range;
                    let _ = out.response.response.clone().accessible_name(name);
                    // line numbers + current line highlight, drawn row by row from the galley
                    let painter = ui.painter();
                    let mut line_no = 1;
                    let mut new_line = true;
                    for row in &out.galley.rows {
                        let y = out.galley_pos.y + row.min_y();
                        row_h = row.height();
                        if new_line {
                            let col = if line_no == cur_line { FG } else { FG_DIM };
                            if line_no == cur_line {
                                let r = Rect::from_min_size(
                                    Pos2::new(gutter_rect.min.x, y),
                                    Vec2::new(ui.clip_rect().max.x - gutter_rect.min.x, row.height()),
                                );
                                painter.set(hl_slot, egui::Shape::rect_filled(r, 0.0, BG_LINE));
                            }
                            painter.text(Pos2::new(gutter_rect.max.x - 12.0, y), egui::Align2::RIGHT_TOP, line_no.to_string(), font.clone(), col);
                        }
                        new_line = row.ends_with_newline;
                        if new_line {
                            line_no += 1;
                        }
                    }
                });
            })
        });
        self.row_height = row_h;
        let sa = sa.inner;

        if let Some(r) = out_cursor {
            let idx = r.primary.index.0;
            let text = &self.docs[self.active].text;
            let before: String = text.chars().take(idx).collect();
            self.cursor_line = before.matches('\n').count() + 1;
            self.cursor_col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        }

        if self.show_minimap {
            self.minimap(ui, mini_rect, &sa);
        }
    }

    pub(super) fn minimap(&mut self, ui: &mut egui::Ui, rect: Rect, sa: &egui::scroll_area::ScrollAreaOutput<()>) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, BG);
        painter.vline(rect.left(), rect.y_range(), Stroke::new(1.0, BG_LINE));
        let doc = &self.docs[self.active];
        let text = &doc.text;
        let lines = text.matches('\n').count() + 1;
        let lh = 2.0_f32; // pixels per line in the minimap
        let cw = 1.1_f32;
        let total_h = lines as f32 * lh;
        // scroll the minimap itself when the file is taller than the panel
        let content_h = sa.content_size.y.max(1.0);
        let frac = sa.state.clamped_offset().y / (content_h - sa.inner_rect.height()).max(1.0);
        let mini_off = (total_h - rect.height()).max(0.0) * frac.clamp(0.0, 1.0);
        let x0 = rect.left() + 8.0;
        let mut line = 0usize;
        let mut col = 0usize;
        // ponytail: paints every token each frame; cache per text version if big files lag.
        for (s, e, t) in tokenize(text, &doc.lang()) {
            for seg in text[s..e].split_inclusive('\n') {
                let word = seg.trim_end_matches('\n');
                let y = rect.top() + line as f32 * lh - mini_off;
                let lead = word.len() - word.trim_start().len();
                let visible = word.trim();
                if !visible.is_empty() && y > rect.top() - lh && y < rect.bottom() {
                    let x = x0 + (col + lead) as f32 * cw;
                    let w = (visible.chars().count() as f32 * cw).min(rect.right() - x - 4.0);
                    if w > 0.0 {
                        painter.rect_filled(Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, lh * 0.75)), 0.0, tok_color(t).gamma_multiply(0.7));
                    }
                }
                col += word.chars().count();
                if seg.ends_with('\n') {
                    line += 1;
                    col = 0;
                }
            }
        }
        // visible-area box
        let vis_top = sa.state.clamped_offset().y / content_h * total_h;
        let vis_h = (sa.inner_rect.height() / content_h * total_h).max(8.0);
        let vr = Rect::from_min_size(Pos2::new(rect.left() + 1.0, rect.top() + vis_top - mini_off), Vec2::new(rect.width() - 1.0, vis_h));
        painter.rect_filled(vr, 0.0, Color32::from_white_alpha(14));

        let resp = ui.interact(rect, ui.scope_id().with("minimap"), Sense::click_and_drag());
        if let Some(p) = resp.interact_pointer_pos() {
            if resp.clicked() || resp.dragged() {
                let line_at = ((p.y - rect.top() + mini_off) / lh).max(0.0);
                let y = line_at / total_h * content_h - sa.inner_rect.height() / 2.0;
                self.scroll_to = Some(y.max(0.0));
            }
        }
    }
}
