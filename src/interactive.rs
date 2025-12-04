use std::collections::{BTreeMap, HashSet};

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::Book;

pub(crate) enum Outcome {
    Picked(usize),
    Unpicked(u32),
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Select {
    Choice(usize),
    Picked(u32),
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
    expanded: HashSet<usize>,
}

impl Pick {
    pub(crate) fn pick(
        &mut self,
        title: &str,
        books: &[&Book<'_>],
        picked: &BTreeMap<u32, (&Book<'_>, Vec<&Book<'_>>)>,
    ) -> Result<Outcome> {
        self.expanded.clear();
        self.scroll_x = 0;

        let last_choice = Select::Choice(books.len().saturating_sub(1));

        self.select = match self.select {
            Select::Choice(n) => Select::Choice(n.min(books.len().saturating_sub(1))),
            Select::Picked(n) => match picked.contains_key(&n) {
                true => Select::Picked(n),
                false => last_choice,
            },
        };

        let mut terminal = ratatui::init();

        let outcome = loop {
            terminal.draw(|f| self.draw(title, books, picked, f))?;
            let e = event::read()?;

            match e {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.select = match self.select {
                            Select::Choice(n) => Select::Choice(n.saturating_sub(1)),
                            Select::Picked(n) => match picked.range(..n).next_back() {
                                Some((n, _)) => Select::Picked(*n),
                                _ => last_choice,
                            },
                        };
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.select = match self.select {
                            Select::Choice(n) if n + 1 >= books.len() => {
                                match picked.first_key_value() {
                                    Some((n, _)) => Select::Picked(*n),
                                    _ => last_choice,
                                }
                            }
                            Select::Choice(n) => Select::Choice(n + 1),
                            Select::Picked(n) => match picked.range(n..).nth(1) {
                                Some((n, _)) => Select::Picked(*n),
                                _ => Select::Picked(n),
                            },
                        };
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        self.scroll_x = self.scroll_x.saturating_sub(4);
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        self.scroll_x = self.scroll_x.saturating_add(4);
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
                            Select::Choice(n) => Outcome::Picked(n),
                            Select::Picked(n) => Outcome::Unpicked(n),
                        };
                    }
                    KeyCode::Esc | KeyCode::Char('q') => break Outcome::Quit,
                    _ => {}
                },
                _ => {}
            }
        };

        ratatui::restore();
        Ok(outcome)
    }

    fn draw(
        &mut self,
        title: &str,
        books: &[&Book<'_>],
        picked: &BTreeMap<u32, (&Book<'_>, Vec<&Book<'_>>)>,
        frame: &mut Frame,
    ) {
        let area = frame.area();

        let start_y = area.y;

        // Render the title
        let title_line = Line::from(Span::styled(title, Style::default().fg(Color::Cyan).bold()));

        frame.render_widget(
            Paragraph::new(title_line).scroll((0, self.scroll_x)),
            ratatui::layout::Rect {
                x: area.x,
                y: start_y,
                width: area.width,
                height: 1,
            },
        );

        // Render the list of choices
        let mut current_y = start_y + 1;

        for (index, book) in books.iter().enumerate() {
            let (prefix, style) = if self.select == Select::Choice(index) {
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

            frame.render_widget(
                Paragraph::new(line).scroll((0, self.scroll_x)),
                ratatui::layout::Rect {
                    x: area.x,
                    y: current_y,
                    width: area.width,
                    height: 1,
                },
            );
            current_y += 1;

            if self.expanded.contains(&index) {
                if let Some(book) = books.get(index) {
                    let path_line = Line::from(Span::styled(
                        format!("    {}", book.dir.display()),
                        Style::default().fg(Color::DarkGray),
                    ));

                    frame.render_widget(
                        Paragraph::new(path_line).scroll((0, self.scroll_x)),
                        ratatui::layout::Rect {
                            x: area.x,
                            y: current_y,
                            width: area.width,
                            height: 1,
                        },
                    );
                    current_y += 1;
                }
            }
        }

        // Render the list of already picked choices
        if !picked.is_empty() {
            current_y += 1; // Add a blank line

            let picked_title = Line::from(Span::styled(
                "Already picked:",
                Style::default().fg(Color::DarkGray).bold(),
            ));

            frame.render_widget(
                Paragraph::new(picked_title),
                ratatui::layout::Rect {
                    x: area.x,
                    y: current_y,
                    width: area.width,
                    height: 1,
                },
            );
            current_y += 1;

            for (i, (n, (book, _))) in picked.iter().enumerate() {
                let (prefix, style) = if self.select == Select::Picked(*n) {
                    (
                        "* ",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    ("  ", Style::default().fg(Color::DarkGray))
                };

                let line = Line::from(Span::styled(
                    format!(
                        "{}{}: {} ({} pages, {} bytes)",
                        prefix,
                        n,
                        book.name,
                        book.pages.len(),
                        book.bytes()
                    ),
                    style,
                ));

                frame.render_widget(
                    Paragraph::new(line).scroll((0, self.scroll_x)),
                    ratatui::layout::Rect {
                        x: area.x,
                        y: current_y + i as u16,
                        width: area.width,
                        height: 1,
                    },
                );
            }
        }
    }
}
