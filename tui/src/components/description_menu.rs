/*
    SPDX-License-Identifier:  AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::components::ThemedWidgetMut;
use crate::themes::Theme;

#[derive(Clone)]
pub struct DescriptionMenuItem {
    pub icon: char,
    pub label: String,
    pub description: String,
}

pub struct DescriptionMenu {
    pub items: Vec<DescriptionMenuItem>,
    pub selected: usize,
    scroll_offset: usize,
    max_visible: usize,
}

impl DescriptionMenu {
    pub fn new(items: Vec<DescriptionMenuItem>) -> Self {
        Self { items, selected: 0, scroll_offset: 0, max_visible: 8 }
    }

    pub fn next(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.selected + 1 >= self.items.len() {
            self.selected = 0;
        } else {
            self.selected += 1;
        }
        self.adjust_scroll();
    }

    pub fn previous(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.items.len() - 1;
        } else {
            self.selected -= 1;
        }
        self.adjust_scroll();
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    fn adjust_scroll(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + self.max_visible {
            self.scroll_offset = self.selected + 1 - self.max_visible;
        }
    }

    pub fn set_max_visible(&mut self, max: usize) {
        self.max_visible = max.max(1).min(self.items.len());
        self.adjust_scroll();
    }

    fn wrap_text(s: &str, max_width: usize) -> Vec<String> {
        let words = s.split_whitespace().peekable();
        let mut lines = Vec::new();
        let mut current = String::new();
        for word in words {
            if current.len() + word.len() + 1 > max_width {
                if !current.is_empty() {
                    lines.push(current.clone());
                }
                current.clear();
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        if !current.is_empty() {
            lines.push(current);
        }
        lines
    }
}

impl ThemedWidgetMut for DescriptionMenu {
    fn render(&mut self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        const Y_SPACING: u16 = 1;
        const MENU_WIDTH: u16 = 24;
        const DESC_PAD: u16 = 4;
        const DESC_MIN_WIDTH: u16 = 18;
        const DESC_MAX_WIDTH: u16 = 36;

        let avail = (area.height.saturating_sub(2)) as usize;
        self.set_max_visible(avail);

        let desc_width =
            (area.width.saturating_sub(MENU_WIDTH + DESC_PAD)).clamp(DESC_MIN_WIDTH, DESC_MAX_WIDTH);

        let n_shown = self.max_visible.min(self.items.len());
        let menu_height = n_shown as u16 * Y_SPACING;
        let start_y = area.y + (area.height.saturating_sub(menu_height)).saturating_div(2);

        let base_x = area.x + area.width / 2 - ((MENU_WIDTH + DESC_PAD + desc_width) / 2);
        let desc_x = base_x + MENU_WIDTH + DESC_PAD;

        let items = &self.items;
        let win_start = self.scroll_offset;
        let win_end = (win_start + self.max_visible).min(items.len());

        let mut selected_desc: Option<String> = None;
        for (visible_idx, i) in (win_start..win_end).enumerate() {
            let item = &items[i];
            let is_selected = i == self.selected;
            let y = start_y + visible_idx as u16 * Y_SPACING;

            if is_selected {
                selected_desc = Some(item.description.clone());
            }

            let style = if is_selected {
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let text = format!("{}  {}", item.icon, item.label);
            buf.set_string(base_x, y, &text, style);
        }

        // Only appear when there's more to scroll btw
        if self.scroll_offset > 0 {
            buf.set_string(
                base_x,
                start_y.saturating_sub(1),
                "↑",
                Style::default().fg(theme.muted),
            );
        }
        if win_end < items.len() {
            let y = start_y + self.max_visible as u16 * Y_SPACING;
            if y < area.y + area.height {
                buf.set_string(base_x, y, "↓", Style::default().fg(theme.muted));
            }
        }

        if let Some(desc) = selected_desc {
            let sel_y = start_y;
            let max_box_height = area.height.saturating_sub(2).min(6);
            let lines = Self::wrap_text(&desc, (desc_width - 4) as usize);
            let content_lines = lines.iter().take((max_box_height as usize).saturating_sub(2));
            let box_height = 2 + content_lines.len() as u16;

            let mut box_y = sel_y;
            if box_y + box_height > area.y + area.height {
                box_y = area.y + area.height - box_height;
            }
            if box_y < area.y {
                box_y = area.y;
            }

            let accent_top = format!("╭{}╮", "─".repeat(desc_width.saturating_sub(2) as usize));
            buf.set_string(desc_x, box_y, &accent_top, Style::default().fg(theme.foreground));
            for (j, line) in lines.iter().enumerate() {
                let y = box_y + 1 + j as u16;
                if (j as u16) + 1 >= max_box_height - 1 || y >= area.y + area.height {
                    continue;
                }

                let line_width = desc_width.saturating_sub(4) as usize;
                let padded_text = format!("{:<width$}", line, width = line_width);

                let line_spans = Line::from(vec![
                    Span::styled("│ ", Style::default().fg(theme.foreground)),
                    Span::styled(
                        padded_text,
                        Style::default()
                            .fg(theme.info)
                            .add_modifier(Modifier::ITALIC | Modifier::BOLD),
                    ),
                    Span::styled(" │", Style::default().fg(theme.foreground)),
                ]);

                buf.set_line(desc_x, y, &line_spans, desc_width);
            }

            let accent_bottom = format!("╰{}╯", "─".repeat(desc_width.saturating_sub(2) as usize));
            buf.set_string(
                desc_x,
                box_y + box_height - 1,
                &accent_bottom,
                Style::default().fg(theme.foreground),
            );
        }
    }
}
