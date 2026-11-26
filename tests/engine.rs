use omarchy_snake::{Cell, Difficulty, Direction::*, Game, Phase};
use std::time::Duration;

#[test]
fn movement_wraps_on_every_edge() {
    for (head, direction, expected) in [
        ((15, 5), Right, (0, 5)),
        ((0, 5), Left, (15, 5)),
        ((5, 0), Up, (5, 15)),
        ((5, 15), Down, (5, 0)),
    ] {
        let mut game = Game::from_board(vec![Cell::from(head)], direction, Cell::from((8, 8)), 0);
        game.step();
        assert_eq!(game.body()[0], Cell::from(expected));
        assert_eq!(game.phase(), Phase::Running);
    }
}

#[test]
fn eating_grows_increases_score_and_places_food_off_body() {
    let mut game = Game::from_board(
        vec![Cell::from((5, 5)), Cell::from((4, 5))],
        Right,
        Cell::from((6, 5)),
        3,
    );
    game.step();
    assert_eq!(game.score(), 4);
    assert_eq!(game.body().len(), 3);
    assert!(!game.body().contains(&game.food().unwrap()));
    assert_eq!(game.cells_per_second(), 9);
}

#[test]
fn moving_into_body_loses_and_filling_the_board_wins() {
    let mut collision = Game::from_board(
        [(1, 1), (1, 2), (2, 2), (2, 1), (2, 0)]
            .map(Cell::from)
            .to_vec(),
        Up,
        Cell::from((3, 3)),
        0,
    );
    assert!(collision.turn(Right, false));
    collision.step();
    assert_eq!(collision.phase(), Phase::Lost);
    assert_eq!(collision.body()[0], Cell::from((1, 1)));

    let food = Cell::from((1, 0));
    let body = (0..16)
        .flat_map(|y| (0..16).map(move |x| Cell::from((x, y))))
        .filter(|cell| *cell != food)
        .collect();
    let mut full = Game::from_board(body, Right, food, 252);
    full.step();
    assert_eq!(full.body().len(), 256);
    assert_eq!(full.score(), 253);
    assert_eq!(full.food(), None);
    assert_eq!(full.phase(), Phase::Won);
}

#[test]
fn queued_turns_accept_only_two_valid_key_presses() {
    let mut game = Game::new(1);
    game.toggle_play();
    let head = game.body()[0];
    assert!(!game.turn(Left, false));
    assert!(!game.turn(Up, true));
    assert!(game.turn(Up, false));
    assert!(!game.turn(Up, false));
    assert!(!game.turn(Down, false));
    assert!(game.turn(Left, false));
    assert!(!game.turn(Down, false));
    game.step();
    assert_eq!(game.body()[0], Cell::from((head.x, head.y - 1)));
    game.step();
    assert_eq!(game.body()[0], Cell::from((head.x - 1, head.y - 1)));
}

#[test]
fn interruptions_clear_turns_and_resume_only_on_explicit_play() {
    for interrupt in [Game::pause, Game::toggle_play] {
        let mut game = Game::new(1);
        assert_eq!(game.phase(), Phase::Ready);
        game.toggle_play();
        assert!(game.turn(Up, false));
        let head = game.body()[0];
        interrupt(&mut game);
        game.advance(Duration::from_secs(1));
        assert_eq!(game.body()[0], head);
        assert_eq!(game.phase(), Phase::Paused);
        game.toggle_play();
        game.step();
        assert_eq!(game.body()[0], Cell::from((head.x + 1, head.y)));
    }
}

#[test]
fn elapsed_time_not_redraw_count_controls_movement() {
    let mut low = Game::new(1);
    let mut high = Game::new(1);
    low.toggle_play();
    high.toggle_play();
    for _ in 0..50 {
        low.advance(Duration::from_millis(10));
    }
    for _ in 0..125 {
        high.advance(Duration::from_millis(4));
    }
    assert_eq!(low.body(), high.body());
    assert_eq!(low.score(), high.score());
    assert_eq!(low.body()[0], Cell::from((12, 8)));
}

#[test]
fn first_move_uses_the_selected_difficulty_interval() {
    for (difficulty, speed) in [
        (Difficulty::Easy, 6),
        (Difficulty::Normal, 8),
        (Difficulty::Hard, 16),
    ] {
        let mut game = Game::new(1);
        let head = game.body()[0];
        game.select_difficulty(difficulty);
        game.toggle_play();
        let interval = Duration::from_secs(1) / speed;
        let nanosecond = Duration::from_nanos(1);
        game.advance(interval - nanosecond);
        assert_eq!(game.body()[0], head);
        game.advance(nanosecond);
        assert_eq!(game.body()[0], Cell::from((head.x + 1, head.y)));
    }
}

#[test]
fn speed_increases_every_four_foods_until_the_board_is_full() {
    for (score, expected) in [
        (0, 8),
        (3, 8),
        (4, 9),
        (12, 11),
        (23, 13),
        (24, 14),
        (27, 14),
        (28, 15),
        (31, 15),
        (32, 16),
        (36, 17),
        (40, 18),
        (48, 20),
        (252, 71),
    ] {
        let game = Game::from_board(vec![Cell::from((1, 1))], Right, Cell::from((2, 1)), score);
        assert_eq!(game.cells_per_second(), expected);
    }
}
