#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;

use azul::{prelude::*, widgets::button::Button};
use std::sync::atomic::{AtomicUsize, Ordering};

macro_rules! CSS_PATH {() => { concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/game_of_life/game_of_life.css")};}

const CSS: &str = include_str!(CSS_PATH!());
const INITIAL_UNIVERSE_WIDTH: usize = 75;
const INITIAL_UNIVERSE_HEIGHT: usize = 75;

static RAND_SEED: AtomicUsize = AtomicUsize::new(2100);

/// Simple rand() function (32-bit)
fn rand_xorshift() -> usize {
    let mut x = RAND_SEED.fetch_add(21, Ordering::SeqCst);
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x
}

#[derive(Debug, Clone, PartialEq)]
enum Cell {
    Dead,
    Alive,
}

impl Cell {
    pub fn is_alive(&self) -> bool { *self == Cell::Alive }
}

#[derive(Debug, PartialEq, Clone)]
struct Universe {
    board: Board,
    game_is_running: bool,
}

#[derive(Debug, PartialEq, Clone)]
struct Board {
    vertical_cells: usize,
    horizontal_cells: usize,
    cells: Vec<Vec<Cell>>,
}

impl Layout for Universe {

    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<Self> {

        let (dead_cells, alive_cells) = count_dead_and_alive_cells(&self.board.cells);

        let header = Dom::div()
            .with_id("header")
            .with_child(Dom::label("Game of Life").with_id("title"))
            .with_child(Dom::label(format!("{} Alive Cells", alive_cells)).with_id("alive_count"))
            .with_child(Dom::label(format!("{} Dead Cells", dead_cells)).with_id("dead_count"))
            .with_child(
                Button::with_label(if !self.game_is_running { "Start" } else { "Restart" }).dom()
                .with_id("start_btn")
                .with_callback(On::MouseUp, Callback(start_stop_game))
            );

        Dom::new(NodeType::Div)
            .with_child(header)
            .with_child(self.board.dom())
    }
}

/// Returns the number of (dead, alive) cells
fn count_dead_and_alive_cells(cells: &[Vec<Cell>]) -> (usize, usize) {
    let total_cells: usize = cells.iter().map(|row| row.len()).sum();
    let alive_cells = cells.iter().map(|row| row.iter().filter(|c| c.is_alive()).count()).sum();
    let dead_cells = total_cells - alive_cells;
    (dead_cells, alive_cells)
}

impl Board {

    pub fn empty(board_width: usize, board_height: usize) -> Self {
        Self {
            cells: vec![vec![Cell::Dead; board_width]; board_height],
            vertical_cells: board_height,
            horizontal_cells: board_width,
        }
    }

    pub fn new_random(board_width: usize, board_height: usize) -> Self {

        let cells = (0..board_height).map(|_| {
            (0..board_width)
            // Initial cell has 1 in 4 chance of being alive
            .map(|_| rand_xorshift() % 4 == 0)
            .map(|alive| if alive { Cell::Alive } else { Cell::Dead })
            .collect()
        }).collect();

        Self {
            cells,
            vertical_cells: board_height,
            horizontal_cells: board_width,
        }
    }

    /// Render the board in a table-like grid structure
    pub fn dom<T: Layout>(&self) -> Dom<T> {
        self.cells.iter().map(|row| {
            row.iter().map(|c|
                NodeData::div()
                .with_classes(vec![match c {
                    Cell::Alive => "alive_cell".into(),
                    Cell::Dead => "dead_cell".into(),
                }])
            ).collect::<Dom<T>>()
            .with_class("row")
        }).collect()
    }
}

// Update the cell state
fn tick(state: &mut Universe, _: &mut AppResources) -> (UpdateScreen, TerminateTimer) {
    state.board = next_iteration(&state.board);
    (Redraw, TerminateTimer::Continue)
}

fn next_iteration(input: &Board) -> Board {

    let mut new_board = input.clone();

    for (row_idx, row) in new_board.cells.iter_mut().enumerate() {

        let upper_r = if row_idx == 0 { input.vertical_cells - 1 } else { row_idx - 1 };
        let lower_r = if row_idx == input.vertical_cells - 1 { 0 } else { row_idx + 1 };

        for (cell_idx, cell) in row.iter_mut().enumerate() {

            // Select all neighbours of the current cell (the 8 cells surrounding the current cell)
            let left_c = if cell_idx == 0 { input.horizontal_cells - 1 } else { cell_idx - 1 };
            let right_c = if cell_idx == input.horizontal_cells - 1 { 0 } else { cell_idx + 1 };

            let neighbors = [
                &input.cells[upper_r][left_c],
                &input.cells[upper_r][cell_idx],
                &input.cells[upper_r][right_c],
                &input.cells[row_idx][left_c],
                &input.cells[row_idx][right_c],
                &input.cells[lower_r][left_c],
                &input.cells[lower_r][cell_idx],
                &input.cells[lower_r][right_c]
            ];

            let alive_neighbors = neighbors.iter().filter(|c| c.is_alive()).count();
            let is_cell_alive = match cell {
                Cell::Alive => !(alive_neighbors < 2 || alive_neighbors > 3),
                Cell::Dead => alive_neighbors == 3,
            };

            *cell = if is_cell_alive { Cell::Alive } else { Cell::Dead };
        }
    }

    new_board
}

/// Callback that starts the main
fn start_stop_game(app_state: &mut AppState<Universe>, _: &mut CallbackInfo<Universe>) -> UpdateScreen {

    use std::time::Duration;

    if let Some(timer) = {
        let state = &mut app_state.data;
        state.board = Board::new_random(INITIAL_UNIVERSE_WIDTH, INITIAL_UNIVERSE_HEIGHT);

        if state.game_is_running {
            None
        } else {
            let timer = Timer::new(tick).with_interval(Duration::from_millis(200));

            state.game_is_running = true;
            Some(timer)
        }
    }{
        app_state.add_timer(TimerId::new(), timer);
    }

    Redraw
}

fn main() {

    let mut app = App::new(Universe {
        board: Board::empty(INITIAL_UNIVERSE_WIDTH, INITIAL_UNIVERSE_HEIGHT),
        game_is_running: false,
    }, AppConfig::default()).unwrap();

    let mut window_options = WindowCreateOptions::default();
    window_options.state.title = "Game of Life".into();

    let css = css::override_native(CSS).unwrap();
    let window = app.create_window(window_options, css).unwrap();
    app.run(window).unwrap();
}
