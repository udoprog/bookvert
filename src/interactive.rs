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

use crate::{Book, State};

pub(crate) enum Action {
    Picked(usize),
    Unpick(usize),
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Select {
    Choice(usize),
    Picked(usize),
}

impl Default for Select {
    #[inline]
    fn default() -> Self {
        Select::Choice(0)
    }
}

#[derive(Default)]
pub(crate) struct Pick {
    select: Select,
    scroll_x: u16,
    list_state: ListState,
    expanded: HashSet<usize>,
}

impl Pick {
    pub(crate) fn pick(
        &mut self,
        title: &str,
        books: &[&Book<'_>],
        state: &State<'_, '_>,
    ) -> Result<Action> {
        self.expanded.clear();
        self.scroll_x = 0;
        self.list_state = ListState::default();

        let last_choice = Select::Choice(books.len().saturating_sub(1));

        self.select = match self.select {
            Select::Choice(n) => Select::Choice(n.min(books.len().saturating_sub(1))),
            Select::Picked(..) if state.picked.is_empty() => last_choice,
            Select::Picked(n) => Select::Picked(n.min(state.picked.len().saturating_sub(1))),
        };

        let mut terminal = ratatui::init();

        let outcome = loop {
            terminal.draw(|f| self.draw(title, books, state, f))?;
            let e = event::read()?;

            match e {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.select = match self.select {
                            Select::Choice(n) => Select::Choice(n.saturating_sub(1)),
                            Select::Picked(0) => last_choice,
                            Select::Picked(n) => Select::Picked(n.saturating_sub(1)),
                        };
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.select = match self.select {
                            Select::Choice(n) if n + 1 >= books.len() => Select::Picked(0),
                            Select::Choice(n) => Select::Choice(n + 1),
                            Select::Picked(n) => Select::Picked(
                                n.saturating_add(1)
                                    .min(state.picked.len().saturating_sub(1)),
                            ),
                        };
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        self.scroll_x = self.scroll_x.saturating_sub(4);
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        self.scroll_x = self.scroll_x.saturating_add(4);
                    }
                    KeyCode::Char('O') => {
                        if self.expanded.len() == books.len() {
                            self.expanded.clear();
                        } else {
                            self.expanded.extend(0..books.len());
                        }
                    }
                    KeyCode::Char('o' | ' ') => {
                        if let Select::Choice(index) = self.select {
                            if self.expanded.contains(&index) {
                                self.expanded.remove(&index);
                            } else {
                                self.expanded.insert(index);
                            }
                        }
                    }
                    KeyCode::Enter => {
                        break match self.select {
                            Select::Choice(n) => Action::Picked(n),
                            Select::Picked(n) => Action::Unpick(n),
                        };
                    }
                    KeyCode::Esc | KeyCode::Char('q') => break Action::Quit,
                    _ => {}
                },
                _ => {}
            }
        };

        ratatui::restore();
        Ok(outcome)
    }

    fn draw(&mut self, title: &str, books: &[&Book<'_>], state: &State<'_, '_>, frame: &mut Frame) {
        let mut items = Vec::new();
        let mut selected = None;

        for (i, book) in books.iter().enumerate() {
            let is_selected = self.select == Select::Choice(i);

            if is_selected {
                selected = Some(items.len());
            }

            let (prefix, style) = if self.select == Select::Choice(i) {
                (
                    "* ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ("  ", Style::default())
            };

            let line = Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(
                    format!(
                        "{} ({} pages, {} bytes)",
                        book.name,
                        book.pages.len(),
                        book.bytes()
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

        if !state.picked.is_empty() {
            items.push(ListItem::new(Line::from(Span::styled(
                "Picked:",
                Style::default().fg(Color::DarkGray).bold(),
            ))));

            for (i, (c, n)) in state.picked.iter().enumerate() {
                let Some((catalog, book)) = state
                    .catalogs
                    .get(*c)
                    .and_then(|c| Some((c, c.books.get(*n)?)))
                else {
                    continue;
                };

                let is_selected = self.select == Select::Picked(i);

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
                } else {
                    ("  ", Style::default())
                };

                let line = Line::from(Span::styled(
                    format!(
                        "{prefix}{number}: {} ({} pages, {} bytes)",
                        book.name,
                        book.pages.len(),
                        book.bytes(),
                        number = catalog.number,
                    ),
                    style,
                ));
                items.push(ListItem::new(line));
            }
        }

        self.list_state.select(selected);

        let mut scrollbar_state = ScrollbarState::new(items.len())
            .position(self.list_state.selected().unwrap_or_default());

        let area = frame.area();
        let layout = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);

        let title_line = Line::from(Span::styled(title, Style::default().fg(Color::Cyan).bold()));
        frame.render_widget(
            Paragraph::new(title_line).scroll((0, self.scroll_x)),
            layout[0],
        );

        let list = List::new(items);
        frame.render_stateful_widget(list, layout[1], &mut self.list_state);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(scrollbar, layout[1], &mut scrollbar_state);
    }
}
