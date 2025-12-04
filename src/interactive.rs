use std::collections::HashSet;

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::State;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum View {
    #[default]
    Catalogs,
    Books(usize),
}

#[derive(Default)]
pub(crate) struct App {
    view: View,
    catalog_index: usize,
    book_index: usize,
    scroll_x: u16,
    list_state: ListState,
    expanded: HashSet<usize>,
}

impl App {
    pub(crate) fn run(&mut self, state: &mut State<'_, '_>) -> Result<bool> {
        self.expanded.clear();
        self.scroll_x = 0;
        self.list_state = ListState::default();
        self.view = View::Catalogs;
        self.catalog_index = self
            .catalog_index
            .min(state.catalogs.len().saturating_sub(1));

        let mut terminal = ratatui::init();

        let outcome = loop {
            terminal.draw(|f| self.draw(state, f))?;
            let e = event::read()?;

            match e {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Up | KeyCode::Char('k') => match self.view {
                        View::Catalogs => {
                            self.catalog_index = self.catalog_index.saturating_sub(1);
                        }
                        View::Books(_) => {
                            self.book_index = self.book_index.saturating_sub(1);
                        }
                    },
                    KeyCode::Down | KeyCode::Char('j') => match self.view {
                        View::Catalogs => {
                            self.catalog_index = self
                                .catalog_index
                                .saturating_add(1)
                                .min(state.catalogs.len().saturating_sub(1));
                        }
                        View::Books(cat_idx) => {
                            if let Some(catalog) = state.catalogs.get(cat_idx) {
                                self.book_index = self
                                    .book_index
                                    .saturating_add(1)
                                    .min(catalog.books.len().saturating_sub(1));
                            }
                        }
                    },
                    KeyCode::Left | KeyCode::Char('h') => {
                        if let View::Books(_) = self.view {
                            self.view = View::Catalogs;
                            self.book_index = 0;
                            self.expanded.clear();
                        } else {
                            self.scroll_x = self.scroll_x.saturating_sub(4);
                        }
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        if let View::Catalogs = self.view {
                            self.view = View::Books(self.catalog_index);
                            self.book_index =
                                state.picked.get(&self.catalog_index).copied().unwrap_or(0);
                        } else {
                            self.scroll_x = self.scroll_x.saturating_add(4);
                        }
                    }
                    KeyCode::Char('O') => {
                        if let View::Books(cat_idx) = self.view
                            && let Some(catalog) = state.catalogs.get(cat_idx)
                        {
                            if self.expanded.len() == catalog.books.len() {
                                self.expanded.clear();
                            } else {
                                self.expanded.extend(0..catalog.books.len());
                            }
                        }
                    }
                    KeyCode::Char('o' | ' ') => match self.view {
                        View::Catalogs => {
                            self.view = View::Books(self.catalog_index);
                            self.book_index =
                                state.picked.get(&self.catalog_index).copied().unwrap_or(0);
                        }
                        View::Books(_) => {
                            if self.expanded.contains(&self.book_index) {
                                self.expanded.remove(&self.book_index);
                            } else {
                                self.expanded.insert(self.book_index);
                            }
                        }
                    },
                    KeyCode::Enter => match self.view {
                        View::Catalogs => {
                            self.view = View::Books(self.catalog_index);
                            self.book_index =
                                state.picked.get(&self.catalog_index).copied().unwrap_or(0);
                        }
                        View::Books(cat_idx) => {
                            state.picked.insert(cat_idx, self.book_index);
                            if state.next_unpicked().is_none() {
                                break true;
                            }
                            self.view = View::Catalogs;
                            self.expanded.clear();
                        }
                    },
                    KeyCode::Esc | KeyCode::Char('q') => {
                        if let View::Books(_) = self.view {
                            self.view = View::Catalogs;
                            self.book_index = 0;
                            self.expanded.clear();
                        } else {
                            break false;
                        }
                    }
                    KeyCode::Char('x') => {
                        if let View::Catalogs = self.view
                            && !state.picked.is_empty()
                        {
                            break true;
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        };

        ratatui::restore();
        Ok(outcome)
    }

    fn draw(&mut self, state: &State<'_, '_>, frame: &mut Frame) {
        match self.view {
            View::Catalogs => self.draw_catalogs(state, frame),
            View::Books(cat_idx) => self.draw_books(state, cat_idx, frame),
        }
    }

    fn draw_catalogs(&mut self, state: &State<'_, '_>, frame: &mut Frame) {
        let mut items = Vec::new();
        let mut selected = None;

        for (i, catalog) in state.catalogs.iter().enumerate() {
            let is_selected = i == self.catalog_index;
            let is_picked = state.picked.contains_key(&i);

            if is_selected {
                selected = Some(items.len());
            }

            let base_color = if is_picked { Color::Green } else { Color::Red };

            let (prefix, style) = if is_selected {
                (
                    "* ",
                    Style::default().fg(base_color).add_modifier(Modifier::BOLD),
                )
            } else {
                ("  ", Style::default().fg(base_color))
            };

            let picked_info = if let Some(&book_idx) = state.picked.get(&i) {
                if let Some(book) = catalog.books.get(book_idx) {
                    format!(" {}", book.name)
                } else {
                    String::new()
                }
            } else {
                " (not selected)".to_string()
            };

            let line = Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("{:03}", catalog.number), style),
                Span::styled(picked_info, style),
                Span::styled(
                    format!(" ({} options)", catalog.books.len()),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            items.push(ListItem::new(line));
        }

        self.list_state.select(selected);

        let mut scrollbar_state = ScrollbarState::new(items.len())
            .position(self.list_state.selected().unwrap_or_default());

        let area = frame.area();
        let layout = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

        let line = Line::from(vec![
            Span::styled("Catalogs", Style::default().fg(Color::Cyan).bold()),
            Span::styled(
                " (Enter/o/→ to select, Esc/q to quit)",
                Style::default().fg(Color::Cyan),
            ),
        ]);
        frame.render_widget(Paragraph::new(line).scroll((0, self.scroll_x)), layout[0]);

        let list = List::new(items);
        frame.render_stateful_widget(list, layout[1], &mut self.list_state);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(scrollbar, layout[1], &mut scrollbar_state);

        let picked_count = state.picked.len();
        let total_count = state.catalogs.len();
        let footer_style = if picked_count > 0 {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let footer = Line::from(vec![Span::styled(
            format!("[x] Execute with {picked_count}/{total_count} selected"),
            footer_style,
        )]);
        frame.render_widget(Paragraph::new(footer), layout[2]);
    }

    fn draw_books(&mut self, state: &State<'_, '_>, cat_idx: usize, frame: &mut Frame) {
        let Some(catalog) = state.catalogs.get(cat_idx) else {
            return;
        };

        let mut items = Vec::new();
        let mut selected = None;
        let current_pick = state.picked.get(&cat_idx).copied();

        for (i, book) in catalog.books.iter().enumerate() {
            let is_selected = i == self.book_index;
            let is_picked = current_pick == Some(i);

            if is_selected {
                selected = Some(items.len());
            }

            let (prefix, style) = if is_selected {
                (
                    "* ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else if is_picked {
                ("  ", Style::default().fg(Color::Green))
            } else {
                ("  ", Style::default())
            };

            let picked_marker = if is_picked { " ✓" } else { "" };

            let line = Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(
                    format!(
                        "{} ({} pages, {} bytes){}",
                        book.name,
                        book.pages.len(),
                        book.bytes(),
                        picked_marker,
                    ),
                    style,
                ),
            ]);

            items.push(ListItem::new(line));

            if self.expanded.contains(&i) {
                let path_line = Line::from(Span::styled(
                    format!("    {}", book.dir.display()),
                    Style::default().fg(Color::DarkGray),
                ));
                items.push(ListItem::new(path_line));
            }
        }

        self.list_state.select(selected);

        let mut scrollbar_state = ScrollbarState::new(items.len())
            .position(self.list_state.selected().unwrap_or_default());

        let area = frame.area();
        let layout = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);

        let line = format!("Catalog {:03} - Select book", catalog.number);
        let line = Line::from(vec![
            Span::styled(line, Style::default().fg(Color::Cyan).bold()),
            Span::styled(
                " (Enter to pick, Esc/q/← to go back, o to show path, O to show path for all)",
                Style::default().fg(Color::Cyan),
            ),
        ]);
        frame.render_widget(Paragraph::new(line).scroll((0, self.scroll_x)), layout[0]);

        let list = List::new(items);
        frame.render_stateful_widget(list, layout[1], &mut self.list_state);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(scrollbar, layout[1], &mut scrollbar_state);
    }
}
