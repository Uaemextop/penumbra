/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use human_bytes::human_bytes;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Alignment, Frame};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Row, Table};

use crate::app::AppCtx;
use crate::components::{Stars, ThemedWidgetMut, ThemedWidgetRef};

use super::DevicePage;
use super::events::DeviceStatus;

/// All rendering methods for [`DevicePage`].
impl DevicePage {
    /// Renders the background (stars :D)
    pub(super) fn render_background(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &mut AppCtx,
    ) {
        self.stars.render(area, frame.buffer_mut(), &ctx.theme);
        self.stars.tick();
    }

    /// Renders the whole layout
    pub(super) fn render_layout(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: &mut AppCtx) {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(0),    // Content
                Constraint::Length(5), // Progress
                Constraint::Length(1), // Footer
            ])
            .margin(1)
            .split(area);

        let centered = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0)])
            .split(vertical[1]);

        self.render_header(frame, vertical[0], ctx);
        self.render_content(frame, centered[0], ctx);
        self.render_progress(frame, vertical[2], ctx);
        self.render_footer(frame, vertical[3], ctx);
    }

    /// Header banner
    fn render_header(&self, frame: &mut Frame<'_>, area: Rect, ctx: &mut AppCtx) {
        let status = match &self.device_state.status {
            DeviceStatus::Disconnected => {
                Span::styled("  Disconnected ", Style::default().fg(ctx.theme.muted))
            }
            DeviceStatus::Connecting => {
                Span::styled("  Connecting… ", Style::default().fg(ctx.theme.warning))
            }
            DeviceStatus::Connected => {
                Span::styled("  Connected ", Style::default().fg(ctx.theme.success))
            }
        };

        let header = Paragraph::new(Line::from(vec![
            Span::styled(" Antumbra ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" | "),
            status,
            Span::raw(" | "),
            Span::styled(
                self.status_message.as_deref().unwrap_or(" "),
                Style::default().fg(ctx.theme.info),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .style(Style::default().fg(ctx.theme.accent)),
        )
        .alignment(Alignment::Left);

        frame.render_widget(header, area);
    }

    /// Menu + Device Info
    fn render_content(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: &mut AppCtx) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Length(2),
                Constraint::Percentage(70),
            ])
            .split(area);

        self.render_menu(frame, chunks[0], ctx);
        self.render_device_info(frame, chunks[2], ctx);
    }

    /// Action menu
    fn render_menu(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: &mut AppCtx) {
        let block = Block::default()
            .title(" ACTIONS ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ctx.theme.text));

        frame.render_widget(block.clone(), area);
        self.menu.render(block.inner(area), frame.buffer_mut(), &ctx.theme);
    }

    /// Device info card
    fn render_device_info(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: &mut AppCtx) {
        let block = Block::default()
            .title(" DEVICE INFO ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ctx.theme.text));

        frame.render_widget(block.clone(), area);
        let inner = block.inner(area);

        if !self.device_state.is_connected() {
            self.render_disconnected(frame, inner, ctx);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(8), Constraint::Length(1), Constraint::Min(0)])
            .split(inner);

        self.render_device_table(frame, chunks[0], ctx);
        self.partition_list.render(chunks[2], frame.buffer_mut(), &ctx.theme);
    }

    /// Disconnected message
    fn render_disconnected(&self, frame: &mut Frame<'_>, area: Rect, ctx: &mut AppCtx) {
        let message = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                " Waiting for device connection…",
                Style::default().fg(ctx.theme.warning).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                " (Plug device in BOOTROM or Preloader mode)",
                Style::default().fg(ctx.theme.muted),
            )),
        ])
        .alignment(Alignment::Center);

        frame.render_widget(message, area);
    }

    /// Device configuration table
    fn render_device_table(&self, frame: &mut Frame<'_>, area: Rect, ctx: &mut AppCtx) {
        let Some(devinfo) = &self.devinfo else { return };

        let hw_code = format!("0x{:X}", devinfo.hw_code);

        let sbc = if devinfo.target_config & 0x1 != 0 { "Yes" } else { "No" };
        let sla = if devinfo.target_config & 0x2 != 0 { "Yes" } else { "No" };
        let daa = if devinfo.target_config & 0x4 != 0 { "Yes" } else { "No" };

        let rows = vec![
            Row::new(vec!["HW Code", hw_code.as_str()]),
            Row::new(vec!["Secure Boot (SBC)", sbc]),
            Row::new(vec!["Serial Link Auth (SLA)", sla]),
            Row::new(vec!["Download Agent Auth (DAA)", daa]),
        ];

        let table = Table::new(rows, [Constraint::Percentage(45), Constraint::Percentage(55)])
            .block(Block::default().borders(Borders::BOTTOM))
            .column_spacing(1)
            .style(Style::default().fg(ctx.theme.text));

        frame.render_widget(table, area);
    }

    /// Progress bar
    fn render_progress(&self, frame: &mut Frame<'_>, area: Rect, ctx: &mut AppCtx) {
        let block = Block::default()
            .title(" PROGRESS ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .style(Style::default().fg(ctx.theme.accent));

        frame.render_widget(block.clone(), area);

        let inner = block.inner(area);

        // Give the progress bar exactly 3 rows
        let bar_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3)])
            .split(inner)[0];

        self.progress_bar.render_ref(bar_area, frame.buffer_mut(), &ctx.theme);
    }

    /// Footer help text
    fn render_footer(&self, frame: &mut Frame<'_>, area: Rect, ctx: &mut AppCtx) {
        let footer = Paragraph::new("[↑↓] Navigate   [Enter] Select   [Esc] Back")
            .alignment(Alignment::Center)
            .style(Style::default().fg(ctx.theme.foreground));

        frame.render_widget(footer, area);
    }
}
