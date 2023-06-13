//! This example is from:
//! https://blog.logrocket.com/rust-and-tui-building-a-command-line-interface-in-rust/

use chrono::{DateTime, Utc};
use crossterm::event::{self, Event as CEvent, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use std::{error, fs, io, sync::mpsc, thread, time};
use thiserror::Error;
use tui::backend::{Backend, CrosstermBackend};
use tui::layout::{Alignment, Constraint, Direction, Layout};
use tui::style::{Color, Modifier, Style};
use tui::text::{Span, Spans};
use tui::widgets::{
  Block, BorderType, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, Tabs,
};
use tui::{Frame, Terminal};

const DB_PATH: &str = "./data/db.json";

#[derive(Serialize, Deserialize, Clone)]
struct Pet {
  id: usize,
  name: String,
  category: String,
  age: usize,
  created_at: DateTime<Utc>,
}

#[derive(Error, Debug)]
pub enum DbError {
  #[error("error reading the DB file: {0}")]
  ReadDbError(#[from] io::Error),
  #[error("error parsing the DB file: {0}")]
  ParseDBError(#[from] serde_json::Error),
}

enum Event<I> {
  Input(I),
  Tick,
}

#[derive(Copy, Clone, Debug)]
enum MenuItem {
  Home,
  Pets,
}

impl From<MenuItem> for usize {
  fn from(input: MenuItem) -> Self {
    match input {
      MenuItem::Home => 0,
      MenuItem::Pets => 1,
    }
  }
}

fn main() -> Result<(), Box<dyn error::Error>> {
  enable_raw_mode().expect("can run in raw mode");

  // setup event loop
  let (tx, rx) = mpsc::channel();
  let tick_rate = time::Duration::from_millis(200);
  thread::spawn(move || {
    let mut last_tick = Instant::now();
    loop {
      let timeout = tick_rate
        .checked_sub(last_tick.elapsed())
        .unwrap_or_else(|| time::Duration::from_secs(0));

      if event::poll(timeout).expect("poll works") {
        if let CEvent::Key(key) = event::read().expect("can read events") {
          tx.send(Event::Input(key)).expect("can send events");
        }
      }

      if last_tick.elapsed() >= tick_rate && tx.send(Event::Tick).is_ok() {
        last_tick = Instant::now();
      }
    }
  });

  // setup rendering loop
  let stdout = io::stdout();
  let backend = CrosstermBackend::new(stdout);
  let mut terminal = Terminal::new(backend)?;
  terminal.clear()?;
  terminal.hide_cursor()?;
  let mut active_menu_item = MenuItem::Home;
  let mut pet_list_state = ListState::default();
  pet_list_state.select(Some(0));
  loop {
    terminal.draw(|f| {
      ui(f, active_menu_item, &mut pet_list_state);
    })?;

    match rx.recv()? {
      Event::Input(event) => match event.code {
        KeyCode::Char('q') => {
          terminal.clear()?;
          disable_raw_mode()?;
          terminal.show_cursor()?;
          break;
        }
        KeyCode::Char('h') => active_menu_item = MenuItem::Home,
        KeyCode::Char('p') => active_menu_item = MenuItem::Pets,
        KeyCode::Char('a') => {
          add_random_pet_to_db().expect("can add new pet");
        }
        KeyCode::Char('d') => remove_pet_at_index(&mut pet_list_state).expect("can remove pet"),
        KeyCode::Down => {
          if let Some(selected) = pet_list_state.selected() {
            let amount_pets = read_db().expect("can fetch pet list").len();
            if selected >= amount_pets - 1 {
              pet_list_state.select(Some(0));
            } else {
              pet_list_state.select(Some(selected + 1));
            }
          }
        }
        KeyCode::Up => {
          if let Some(selected) = pet_list_state.selected() {
            let amount_pets = read_db().expect("can fetch pet list").len();
            if selected > 0 {
              pet_list_state.select(Some(selected - 1));
            } else {
              pet_list_state.select(Some(amount_pets - 1));
            }
          }
        }
        _ => (),
      },
      Event::Tick => (),
    };
  }
  Ok(())
}

fn ui<T: Backend>(f: &mut Frame<T>, active_menu_item: MenuItem, pet_list_state: &mut ListState) {
  let size = f.size();
  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .margin(1)
    .constraints([
      Constraint::Length(3), // menu
      Constraint::Min(2),    // content
      Constraint::Length(3), // footer
    ])
    .split(size);

  let menu_titles = vec!["Home", "Pets", "Add", "Delete", "Quit"];
  let menu = menu_titles
    .iter()
    .map(|t| {
      let (first, rest) = t.split_at(1);
      Spans::from(vec![
        Span::styled(
          first,
          Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::UNDERLINED),
        ),
        Span::styled(rest, Style::default().fg(Color::White)),
      ])
    })
    .collect();
  let tabs = Tabs::new(menu)
    .select(active_menu_item.into())
    .block(Block::default().title("Menu").borders(Borders::ALL))
    .style(Style::default().fg(Color::White))
    .highlight_style(Style::default().fg(Color::Yellow))
    .divider(Span::raw("|"));
  f.render_widget(tabs, chunks[0]);

  match active_menu_item {
    MenuItem::Home => f.render_widget(render_home(), chunks[1]),
    MenuItem::Pets => {
      let pets_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(chunks[1]);
      let (left, right) = render_pets(pet_list_state);
      f.render_stateful_widget(left, pets_chunks[0], pet_list_state);
      f.render_widget(right, pets_chunks[1]);
    }
  }

  let copyright = Paragraph::new("pet-CLI 2020 - all rights reserved")
    .style(Style::default().fg(Color::LightCyan))
    .alignment(Alignment::Center)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White))
        .title("Copyright")
        .border_type(BorderType::Plain),
    );
  f.render_widget(copyright, chunks[2]);
}

fn render_home<'a>() -> Paragraph<'a> {
  let home = Paragraph::new(vec![
    Spans::from(""),
    Spans::from("Welcome"),
    Spans::from(""),
    Spans::from("to"),
    Spans::from(""),
    Spans::from(Span::styled(
      "pet-CLI",
      Style::default().fg(Color::LightBlue),
    )),
    Spans::from(""),
    Spans::from("Press 'p' to access pets,\n'a' to add random new pets\nand 'd' to delete the currently selected pet.")
  ])
    .alignment(Alignment::Center)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White))
        .title("Home")
        .border_type(BorderType::Plain),
    );
  home
}

fn read_db() -> Result<Vec<Pet>, DbError> {
  let db_content = fs::read_to_string(DB_PATH)?;
  let parsed: Vec<Pet> = serde_json::from_str(&db_content)?;
  Ok(parsed)
}

fn render_pets<'a>(pet_list_state: &ListState) -> (List<'a>, Table<'a>) {
  let pets = Block::default()
    .borders(Borders::ALL)
    .style(Style::default().fg(Color::White))
    .title("Pets")
    .border_type(BorderType::Plain);
  let pet_list = read_db().expect("can fetch pet list");
  let items: Vec<_> = pet_list
    .iter()
    .map(|p| ListItem::new(Spans::from(p.name.clone())))
    .collect();
  let selected_pet = pet_list
    .get(
      pet_list_state
        .selected()
        .expect("there is always a selected pet"),
    )
    .expect("exists")
    .clone();
  let list = List::new(items).block(pets).highlight_style(
    Style::default()
      .bg(Color::Yellow)
      .fg(Color::Black)
      .add_modifier(Modifier::BOLD),
  );
  let pet_detail = Table::new(vec![Row::new(vec![
    Cell::from(Span::raw(selected_pet.id.to_string())),
    Cell::from(Span::raw(selected_pet.name)),
    Cell::from(Span::raw(selected_pet.category)),
    Cell::from(Span::raw(selected_pet.age.to_string())),
    Cell::from(Span::raw(selected_pet.created_at.to_string())),
  ])])
  .header(Row::new(vec![
    Cell::from(Span::styled(
      "ID",
      Style::default().add_modifier(Modifier::BOLD),
    )),
    Cell::from(Span::styled(
      "Name",
      Style::default().add_modifier(Modifier::BOLD),
    )),
    Cell::from(Span::styled(
      "Category",
      Style::default().add_modifier(Modifier::BOLD),
    )),
    Cell::from(Span::styled(
      "Age",
      Style::default().add_modifier(Modifier::BOLD),
    )),
    Cell::from(Span::styled(
      "Created At",
      Style::default().add_modifier(Modifier::BOLD),
    )),
  ]))
  .block(
    Block::default()
      .borders(Borders::ALL)
      .style(Style::default().fg(Color::White))
      .title("Detail")
      .border_type(BorderType::Plain),
  )
  .widths(&[
    Constraint::Percentage(5),
    Constraint::Percentage(20),
    Constraint::Percentage(20),
    Constraint::Percentage(5),
    Constraint::Percentage(20),
  ]);
  (list, pet_detail)
}

fn add_random_pet_to_db() -> Result<Vec<Pet>, DbError> {
  use rand::distributions::Alphanumeric;
  use rand::Rng;
  let mut rng = rand::thread_rng();
  let db_content = fs::read_to_string(DB_PATH)?;
  let mut parsed: Vec<Pet> = serde_json::from_str(&db_content)?;
  let cat_dog = match rng.gen_range(0..=1) {
    0 => "cats",
    _ => "dogs",
  };
  let random_pet = Pet {
    id: rng.gen_range(0..999999),
    name: (&mut rng)
      .sample_iter(&Alphanumeric)
      .take(10)
      .map(char::from)
      .collect(),
    category: cat_dog.to_owned(),
    age: rng.gen_range(1..15),
    created_at: Utc::now(),
  };

  parsed.push(random_pet);
  fs::write(DB_PATH, serde_json::to_vec(&parsed)?)?;
  Ok(parsed)
}

fn remove_pet_at_index(pet_list_state: &mut ListState) -> Result<(), DbError> {
  if let Some(selected) = pet_list_state.selected() {
    let db_content = fs::read_to_string(DB_PATH)?;
    let mut parsed: Vec<Pet> = serde_json::from_str(&db_content)?;
    parsed.remove(selected);
    fs::write(DB_PATH, serde_json::to_vec(&parsed)?)?;
    pet_list_state.select(Some(selected - 1));
  }
  Ok(())
}