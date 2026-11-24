use crate::{player::Preferences, theme::Palette};
use font8x8::{BASIC_FONTS, UnicodeFonts};
use omarchy_snake::{BOARD_SIZE, Cell, Direction, Game, Phase};

const TOP: i32 = 36;
const BOTTOM: i32 = 60;
const SIDE: i32 = 12;
const GLYPH: i32 = 8;

struct Board {
    x: i32,
    y: i32,
    cell: i32,
}

impl Board {
    fn fit(width: i32, height: i32) -> Option<Self> {
        let available = height - TOP - BOTTOM;
        let cell = (width - SIDE * 2).min(available) / BOARD_SIZE;
        (cell > 0).then(|| {
            let side = cell * BOARD_SIZE;
            Self {
                x: (width - side) / 2,
                y: TOP + (available - side) / 2,
                cell,
            }
        })
    }

    fn rect(&self) -> Rect {
        let size = self.cell * BOARD_SIZE;
        Rect::new(self.x, self.y, size, size)
    }

    fn point(&self, cell: Cell) -> (i32, i32) {
        (self.x + cell.x * self.cell, self.y + cell.y * self.cell)
    }
}

fn interpolate_axis(before: i32, after: i32, progress: f32) -> f32 {
    let half = BOARD_SIZE / 2;
    let distance = (after - before + half).rem_euclid(BOARD_SIZE) - half;
    before as f32 + distance as f32 * progress
}

#[derive(Clone, Copy)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Rect {
    fn new(left: i32, top: i32, width: i32, height: i32) -> Self {
        Self {
            left,
            top,
            right: left + width,
            bottom: top + height,
        }
    }

    fn intersect(self, other: Self) -> Self {
        Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        }
    }

    fn outset(self, amount: i32) -> Self {
        Self {
            left: self.left - amount,
            top: self.top - amount,
            right: self.right + amount,
            bottom: self.bottom + amount,
        }
    }
}

struct Canvas<'a> {
    pixels: &'a mut [u32],
    width: i32,
    bounds: Rect,
}

impl Canvas<'_> {
    fn fill(&mut self, rect: Rect, color: u32) {
        let rect = self.bounds.intersect(rect);
        if rect.left >= rect.right || rect.top >= rect.bottom {
            return;
        }
        for row in rect.top..rect.bottom {
            let offset = row as usize * self.width as usize;
            self.pixels[offset + rect.left as usize..offset + rect.right as usize].fill(color);
        }
    }

    fn ring(&mut self, rect: Rect, thickness: i32, color: u32, strength: f32) {
        let clipped = self.bounds.intersect(rect);
        let weight = (strength * 255.0).round() as u32;
        for y in clipped.top..clipped.bottom {
            for x in clipped.left..clipped.right {
                if x - rect.left >= thickness
                    && rect.right - x > thickness
                    && y - rect.top >= thickness
                    && rect.bottom - y > thickness
                {
                    continue;
                }
                let index = (y * self.width + x) as usize;
                let old = self.pixels[index];
                let channel = |shift| {
                    let before = (old >> shift) & 255_u32;
                    let after = (color >> shift) & 255_u32;
                    ((before * (255 - weight) + after * weight) / 255_u32) << shift
                };
                self.pixels[index] = channel(16) | channel(8) | channel(0);
            }
        }
    }

    fn center_text(&mut self, center: i32, top: i32, text: &str, scale: i32, color: u32) {
        let mut left = center - text_width(text, scale) / 2;
        for ch in text.chars() {
            if let Some(glyph) = BASIC_FONTS.get(ch) {
                for (row, bits) in glyph.into_iter().enumerate() {
                    for col in 0..GLYPH {
                        if bits & (1 << col) != 0 {
                            let pixel = Rect::new(
                                left + col * scale,
                                top + row as i32 * scale,
                                scale,
                                scale,
                            );
                            self.fill(pixel, color);
                        }
                    }
                }
            }
            left += GLYPH * scale;
        }
    }
}

fn text_width(text: &str, scale: i32) -> i32 {
    text.chars().count() as i32 * GLYPH * scale
}

fn draw_head(
    canvas: &mut Canvas<'_>,
    segment: Rect,
    bounds: Rect,
    direction: Direction,
    color: u32,
    eye_color: u32,
) {
    let size = segment.right - segment.left;
    if size < 5 {
        canvas.fill(segment.intersect(bounds), color);
        return;
    }
    let mut paint = |forward, across, depth, span, color| {
        let part = match direction {
            Direction::Right => {
                Rect::new(segment.left + forward, segment.top + across, depth, span)
            }
            Direction::Left => Rect::new(
                segment.right - forward - depth,
                segment.top + across,
                depth,
                span,
            ),
            Direction::Up => Rect::new(
                segment.left + across,
                segment.bottom - forward - depth,
                span,
                depth,
            ),
            Direction::Down => Rect::new(segment.left + across, segment.top + forward, span, depth),
        };
        canvas.fill(part.intersect(bounds), color);
    };
    let bevel = (size / 8).max(1);
    let nose = (size / 4).max(2);
    let tip = nose / 2;
    paint(0, 0, size - nose, size, color);
    paint(size - nose, bevel, nose - tip, size - 2 * bevel, color);
    paint(size - tip, 2 * bevel, tip, size - 4 * bevel, color);

    let eye = (size / 8).max(1);
    for center in [size / 3, size * 2 / 3] {
        paint(size - nose - eye, center - eye / 2, eye, eye, eye_color);
    }
}

fn draw_board(
    canvas: &mut Canvas<'_>,
    board: &Board,
    game: &Game,
    previous: &[Cell],
    progress: f32,
    palette: Palette,
    meal: Option<(Cell, f32)>,
) {
    let bounds = board.rect();
    canvas.fill(bounds.outset(1), palette.shade(42));
    canvas.fill(bounds, palette.shade(5));
    let dot_size = (board.cell / 18).max(1);
    let dot_offset = (board.cell - dot_size) / 2;
    let dot_color = palette.shade(17);
    for row in 0..BOARD_SIZE {
        for col in 0..BOARD_SIZE {
            let (x, y) = board.point(Cell { x: col, y: row });
            canvas.fill(
                Rect::new(x + dot_offset, y + dot_offset, dot_size, dot_size),
                dot_color,
            );
        }
    }
    if let Some(food) = game.food() {
        let (x, y) = board.point(food);
        canvas.fill(Rect::new(x, y, board.cell, board.cell), palette.food);
    }

    if let Some((cell, scale)) = meal {
        let (x, y) = board.point(cell);
        let side = (board.cell as f32 * scale).round() as i32;
        let offset = (board.cell - side) / 2;
        // Draw behind the incoming head so it swallows the shrinking square.
        canvas.fill(Rect::new(x + offset, y + offset, side, side), palette.food);
    }

    let inset = (board.cell / 8).max(i32::from(board.cell >= 3));
    let segment_size = board.cell - 2 * inset;
    let period = board.cell * BOARD_SIZE;
    let head_color = palette.head_mark();
    for (index, &after) in game.body().iter().enumerate().rev() {
        let before = previous.get(index).copied().unwrap_or(after);
        let origin = |before, after| {
            let offset = interpolate_axis(before, after, progress) * board.cell as f32;
            (offset.round() as i32 + inset).rem_euclid(period)
        };
        let (x, y) = (origin(before.x, after.x), origin(before.y, after.y));
        for copy_y in [y, y - period] {
            for copy_x in [x, x - period] {
                let segment = Rect::new(
                    board.x + copy_x,
                    board.y + copy_y,
                    segment_size,
                    segment_size,
                );
                if index == 0 {
                    draw_head(
                        canvas,
                        segment,
                        bounds,
                        game.direction(),
                        palette.snake,
                        head_color,
                    );
                } else {
                    canvas.fill(segment.intersect(bounds), palette.snake);
                }
            }
        }
    }
}

fn overlay(
    canvas: &mut Canvas<'_>,
    center: (i32, i32),
    scale: i32,
    lines: &[&str],
    palette: Palette,
) {
    let (x, y) = center;
    let line = 12 * scale;
    let text_height = lines.len() as i32 * line - (line - GLYPH * scale);
    let text_width = lines
        .iter()
        .map(|text| text_width(text, scale))
        .max()
        .unwrap_or(0);
    let top = y - text_height / 2;
    let background = Rect::new(x - text_width / 2, top, text_width, text_height).outset(6 * scale);
    canvas.fill(background.outset(1), palette.shade(42));
    canvas.fill(background, palette.background);
    for (row, text) in lines.iter().enumerate() {
        canvas.center_text(x, top + row as i32 * line, text, scale, palette.text);
    }
}

#[derive(Clone, Copy, Default)]
pub struct Feedback {
    pub impact: Option<(Cell, f32)>,
    pub meal: Option<(Cell, f32)>,
}

impl Feedback {
    pub fn active(self) -> bool {
        self.meal.is_some() || self.impact.is_some_and(|(_, strength)| strength > 0.0)
    }

    fn draw_impact(self, canvas: &mut Canvas<'_>, board: &Board, palette: Palette) {
        if let Some((cell, strength)) = self.impact {
            let (x, y) = board.point(cell);
            let rect = Rect::new(x, y, board.cell, board.cell);
            let line = (board.cell / 14).max(1);
            canvas.ring(rect, line * 2, palette.background, 1.0);
            canvas.ring(rect, line, palette.text, 1.0);
            if strength > 0.0 {
                canvas.ring(rect.outset(-line), line * 2, palette.food, strength);
            }
        }
    }
}

pub fn render(
    pixels: &mut [u32],
    (width, height): (i32, i32),
    game: &Game,
    (previous, progress): (&[Cell], f32),
    palette: Palette,
    preferences: Preferences,
    feedback: Feedback,
) {
    pixels.fill(palette.background);
    let mut canvas = Canvas {
        pixels,
        width,
        bounds: Rect::new(0, 0, width, height),
    };
    let scale = if width >= 408 && height >= 420 { 2 } else { 1 };
    let phase = game.phase();
    if let Some(board) = Board::fit(width, height) {
        draw_board(
            &mut canvas,
            &board,
            game,
            previous,
            progress,
            palette,
            feedback.meal,
        );
        feedback.draw_impact(&mut canvas, &board, palette);
        let side = board.cell * BOARD_SIZE;
        // Keep the result card away from the impact marker.
        let result_y = if feedback
            .impact
            .is_some_and(|(cell, _)| cell.y >= BOARD_SIZE / 2)
        {
            board.y + side / 4
        } else {
            board.y + side * 3 / 4
        };
        let center = (width / 2, result_y);
        if let Some(lines) = match phase {
            Phase::Ready => Some(["Ready", "Space to start"]),
            Phase::Paused => Some(["Paused", "Space to resume"]),
            Phase::Lost => Some(["Game Over", "Space to retry"]),
            Phase::Running | Phase::Won => None,
        } {
            overlay(&mut canvas, center, scale, &lines, palette);
        }
    }
    let glyph_height = GLYPH * scale;
    let heading = match phase {
        Phase::Won => format!("You Win!  {} pts", game.score()),
        _ => format!("Score {:03}  Best {:03}", game.score(), preferences.best),
    };
    canvas.center_text(
        width / 2,
        (TOP - glyph_height) / 2,
        &heading,
        scale,
        palette.text,
    );
    let turns = "Arrows/WASD   Q Quit";
    let action = match phase {
        Phase::Ready => "Space Start",
        Phase::Running => "Space Pause",
        Phase::Paused => "Space Resume",
        Phase::Lost => "Space Retry",
        Phase::Won => "Space Play Again",
    };
    let sound = if preferences.muted {
        "M Sound Off"
    } else {
        "M Sound On"
    };
    let gap = 4;
    let start = height - BOTTOM + (BOTTOM - 3 * glyph_height - 2 * gap) / 2;
    let footer = if phase == Phase::Won {
        [action, "Q Quit", sound]
    } else {
        [turns, action, sound]
    };
    for (row, text) in footer.into_iter().enumerate() {
        canvas.center_text(
            width / 2,
            start + row as i32 * (glyph_height + gap),
            text,
            scale,
            palette.text,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_resize_keeps_square_cells_and_centers_the_fixed_grid() {
        for (width, height) in [(960, 500), (410, 950), (414, 422)] {
            let board = Board::fit(width, height).unwrap();
            let size = board.cell * BOARD_SIZE;
            assert_eq!((width - size) / 2, board.x);
            assert!(board.y >= TOP && board.y + size <= height - BOTTOM);
            assert!(size <= width - SIDE * 2);
        }
        assert!(Board::fit(20, 20).is_none());
    }

    #[test]
    fn head_has_a_beveled_front_and_two_eyes_in_every_direction() {
        let (width, height) = (440, 460);
        let board = Board::fit(width, height).unwrap();
        let palette = Palette::default();
        let head = Cell::from((6, 6));
        let (x, y) = board.point(head);
        let inset = (board.cell / 8).max(1);
        let size = board.cell - 2 * inset;
        for direction in [
            Direction::Right,
            Direction::Left,
            Direction::Up,
            Direction::Down,
        ] {
            let game = Game::from_board(vec![head], direction, (9, 9).into(), 0);
            let mut pixels = vec![0; (width * height) as usize];
            render(
                &mut pixels,
                (width, height),
                &game,
                (game.previous_body(), 1.0),
                palette,
                Preferences::default(),
                Feedback::default(),
            );
            let pixel = |forward, across| {
                let (dx, dy) = match direction {
                    Direction::Right => (forward, across),
                    Direction::Left => (size - 1 - forward, across),
                    Direction::Up => (across, size - 1 - forward),
                    Direction::Down => (across, forward),
                };
                pixels[((y + inset + dy) * width + x + inset + dx) as usize]
            };
            assert_eq!(pixel(0, 0), palette.snake, "{direction:?}: rear corner");
            assert_ne!(
                pixel(size - 1, 0),
                palette.snake,
                "{direction:?}: front corner"
            );
            assert_eq!(
                pixel(size - 1, size / 2),
                palette.snake,
                "{direction:?}: front center"
            );
            for across in [size / 3, size * 2 / 3] {
                assert!(
                    (size / 2..size - 1)
                        .any(|forward| pixel(forward, across) == palette.head_mark()),
                    "{direction:?}: missing eye"
                );
            }
            assert!(
                (0..size).all(|forward| pixel(forward, size / 2) != palette.head_mark()),
                "{direction:?}: eyes should be separated"
            );
        }
    }

    #[test]
    fn rendered_food_fills_its_cell_and_wrapped_head_stays_inside_board_edges() {
        let (width, height) = (440, 460);
        let board = Board::fit(width, height).unwrap();
        let mut game = Game::from_board(
            vec![Cell::from((15, 5))],
            Direction::Right,
            Cell::from((8, 12)),
            0,
        );
        game.step();
        let palette = Palette::default();
        let mut pixels = vec![0; (width * height) as usize];
        render(
            &mut pixels,
            (width, height),
            &game,
            (game.previous_body(), 0.5),
            palette,
            Preferences::default(),
            Feedback::default(),
        );
        let pixel = |x, y| pixels[(y * width + x) as usize];
        let food_x = board.x + 8 * board.cell;
        let food_y = board.y + 12 * board.cell;
        for y in food_y..food_y + board.cell {
            for x in food_x..food_x + board.cell {
                assert_eq!(pixel(x, y), palette.food);
            }
        }
        let right = board.x + BOARD_SIZE * board.cell;
        let head_y = board.y + 5 * board.cell;
        assert_ne!(pixel(board.x, head_y), palette.snake);
        assert_eq!(pixel(board.x, head_y + board.cell / 4), palette.snake);
        assert_eq!(pixel(right - 1, head_y + board.cell / 4), palette.snake);
        assert_ne!(pixel(right, head_y + board.cell / 4), palette.snake);
        assert_ne!(
            pixel(board.x + 10 * board.cell, head_y + board.cell / 4),
            palette.snake
        );
        assert!((board.x..board.x + board.cell / 2).any(|x| {
            (head_y..head_y + board.cell).any(|y| pixel(x, y) == palette.head_mark())
        }));
    }

    #[test]
    fn victory_keeps_the_full_board_visible_and_shows_the_score_and_replay() {
        let mut game = Game::from_board(
            (0..BOARD_SIZE)
                .flat_map(|y| (0..BOARD_SIZE).map(move |x| (x, y)))
                .skip(1)
                .map(Cell::from)
                .collect(),
            Direction::Left,
            (0, 0).into(),
            252,
        );
        game.step();
        assert_eq!(game.phase(), Phase::Won);
        assert_eq!(game.score(), 253);
        assert_eq!(game.body().len(), (BOARD_SIZE * BOARD_SIZE) as usize);
        let palette = Palette::default();
        for (width, height, scale) in [(224, 252, 1), (600, 640, 2)] {
            let mut pixels = vec![0; (width * height) as usize];
            let motion = (game.previous_body(), 1.0);
            render(
                &mut pixels,
                (width, height),
                &game,
                motion,
                palette,
                Preferences::default(),
                Feedback::default(),
            );
            let mut expected = vec![palette.background; pixels.len()];
            let mut canvas = Canvas {
                pixels: &mut expected,
                width,
                bounds: Rect::new(0, 0, width, height),
            };
            let board = Board::fit(width, height).unwrap();
            draw_board(
                &mut canvas,
                &board,
                &game,
                motion.0,
                motion.1,
                palette,
                None,
            );
            let glyph = GLYPH * scale;
            let header = (TOP - glyph) / 2;
            let footer = height - BOTTOM + (BOTTOM - 3 * glyph - 8) / 2;
            canvas.center_text(width / 2, header, "You Win!  253 pts", scale, palette.text);
            for (row, label) in ["Space Play Again", "Q Quit", "M Sound On"]
                .into_iter()
                .enumerate()
            {
                let y = footer + row as i32 * (glyph + 4);
                canvas.center_text(width / 2, y, label, scale, palette.text);
            }
            for y in board.rect().top..board.rect().bottom {
                let start = (y * width + board.rect().left) as usize;
                let end = (y * width + board.rect().right) as usize;
                assert_eq!(&pixels[start..end], &expected[start..end], "board row {y}");
            }
            let rows = (header * width) as usize..((header + glyph) * width) as usize;
            assert_eq!(&pixels[rows.clone()], &expected[rows], "victory heading");
            let rows = (footer * width) as usize..pixels.len();
            let mismatch = pixels[rows.clone()]
                .iter()
                .zip(&expected[rows])
                .position(|(actual, expected)| actual != expected);
            assert_eq!(mismatch, None, "victory footer pixel offset");
        }
    }

    #[test]
    fn compact_window_shows_best_score_and_text_mute_state_alongside_the_result() {
        let (width, height) = (224, 252);
        let palette = Palette::default();
        let mut game = Game::from_board(
            [(1, 0), (1, 1), (2, 1), (2, 0), (3, 0)]
                .map(Cell::from)
                .to_vec(),
            Direction::Right,
            (5, 5).into(),
            7,
        );
        game.step();
        assert_eq!(game.phase(), Phase::Lost);
        let full = Rect::new(0, 0, width, height);
        for (muted, label) in [(false, "M Sound On"), (true, "M Sound Off")] {
            let mut pixels = vec![0; (width * height) as usize];
            render(
                &mut pixels,
                (width, height),
                &game,
                (game.previous_body(), 1.0),
                palette,
                Preferences { best: 42, muted },
                Feedback::default(),
            );
            let mut expected = vec![palette.background; pixels.len()];
            let center = width / 2;
            let board = Board::fit(width, height).unwrap();
            let result_y = board.y + board.cell * BOARD_SIZE * 3 / 4 - 10;
            let top_y = (TOP - GLYPH) / 2;
            let footer_y = height - BOTTOM + (BOTTOM - 3 * GLYPH - 8) / 2;
            let mut canvas = Canvas {
                pixels: &mut expected,
                width,
                bounds: full,
            };
            for (y, text) in [
                (top_y, "Score 007  Best 042"),
                (footer_y + 2 * (GLYPH + 4), label),
            ] {
                canvas.center_text(center, y, text, 1, palette.text);
            }
            canvas.center_text(center, result_y, "Game Over", 1, palette.text);
            let rows = |y: i32| (y * width) as usize..((y + GLYPH) * width) as usize;
            assert_eq!(pixels[rows(top_y)], expected[rows(top_y)]);
            let status_y = footer_y + 2 * (GLYPH + 4);
            assert_eq!(pixels[rows(status_y)], expected[rows(status_y)]);
            let left = center - text_width("Game Over", 1) / 2;
            for y in result_y..result_y + GLYPH {
                let start = (y * width + left) as usize;
                let end = start + text_width("Game Over", 1) as usize;
                assert_eq!(pixels[start..end], expected[start..end]);
            }
        }
    }

    #[test]
    fn pulses_draw_in_the_event_cell_without_covering_the_new_food() {
        let (width, height) = (440, 460);
        let board = Board::fit(width, height).unwrap();
        let palette = Palette::default();
        let game = Game::from_board(vec![(2, 12).into()], Direction::Right, (3, 12).into(), 1);
        let event = Cell::from((2, 12));
        let (x, y) = board.point(event);
        let (food_x, food_y) = board.point(game.food().unwrap());
        let frames = [
            Feedback::default(),
            Feedback {
                meal: Some((event, 1.0)),
                impact: None,
            },
            Feedback {
                meal: None,
                impact: Some((event, 1.0)),
            },
            Feedback {
                meal: None,
                impact: Some((event, 0.0)),
            },
            Feedback {
                meal: Some((event, 0.5)),
                impact: None,
            },
        ]
        .map(|feedback| {
            let mut pixels = vec![0; (width * height) as usize];
            render(
                &mut pixels,
                (width, height),
                &game,
                (&[Cell::from((1, 12))], 0.0),
                palette,
                Preferences::default(),
                feedback,
            );
            pixels
        });
        let sample = |frame: usize, x, y| frames[frame][(y * width + x) as usize];
        assert_ne!(sample(0, x, y), sample(1, x, y));
        assert_eq!(sample(1, x, y), palette.food);
        let middle = board.cell / 2;
        assert_eq!(sample(1, x + middle, y + middle), palette.food);
        assert_eq!(sample(4, x + middle, y + middle), palette.food);
        assert_eq!(sample(4, x, y), sample(0, x, y));
        assert_eq!(sample(2, x, y), palette.text);
        assert_eq!(sample(3, x, y), palette.text);
        assert_ne!(sample(2, x + 1, y + 1), sample(3, x + 1, y + 1));
        for frame in 0..frames.len() {
            assert_eq!(sample(frame, food_x, food_y), palette.food);
            assert_eq!(sample(frame, x - 1, y), sample(0, x - 1, y));
        }
    }

    #[test]
    fn wrapped_interpolation_crosses_the_nearest_edge() {
        assert_eq!(interpolate_axis(15, 0, 0.5), 15.5);
        assert_eq!(interpolate_axis(0, 15, 0.5), -0.5);
        assert_eq!(interpolate_axis(4, 5, 0.5), 4.5);
        assert_eq!(interpolate_axis(4, 4, 0.5), 4.0);
    }
}
