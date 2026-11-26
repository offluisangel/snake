mod audio;
mod player;
mod presentation;
mod theme;

use audio::{Audio, AudioEvent, Cue};
use omarchy_snake::{Cell, Difficulty, Direction, Game, Phase};
use player::Player;
use std::{
    error::Error,
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use theme::Theme;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    platform::wayland::{EventLoopBuilderExtWayland, WindowAttributesExtWayland},
    window::{Window, WindowId},
};

const THEME_INTERVAL: Duration = Duration::from_millis(250);

const SIMULATION_WAKEUP: Duration = Duration::from_millis(50);

#[derive(Clone, Copy)]
enum Input {
    Turn(Direction),
    Play,
    SelectDifficulty(Difficulty),
    Mute,
    Focus(bool),
}

fn key_input(key: &Key) -> Option<Input> {
    let direction = match key {
        Key::Named(NamedKey::ArrowUp) => Direction::Up,
        Key::Named(NamedKey::ArrowDown) => Direction::Down,
        Key::Named(NamedKey::ArrowLeft) => Direction::Left,
        Key::Named(NamedKey::ArrowRight) => Direction::Right,
        Key::Named(NamedKey::Space) => return Some(Input::Play),
        Key::Character(key) if key == "1" => {
            return Some(Input::SelectDifficulty(Difficulty::Easy));
        }
        Key::Character(key) if key == "2" => {
            return Some(Input::SelectDifficulty(Difficulty::Normal));
        }
        Key::Character(key) if key == "3" => {
            return Some(Input::SelectDifficulty(Difficulty::Hard));
        }
        Key::Character(key) if key.eq_ignore_ascii_case("m") => return Some(Input::Mute),
        Key::Character(key) if key.eq_ignore_ascii_case("w") => Direction::Up,
        Key::Character(key) if key.eq_ignore_ascii_case("s") => Direction::Down,
        Key::Character(key) if key.eq_ignore_ascii_case("a") => Direction::Left,
        Key::Character(key) if key.eq_ignore_ascii_case("d") => Direction::Right,
        _ => return None,
    };
    Some(Input::Turn(direction))
}

struct FrozenMotion {
    previous: Vec<Cell>,
    progress: f32,
}

const IMPACT_PULSE: Duration = Duration::from_millis(160);

struct Session {
    game: Game,
    impact_at: Option<Instant>,
    updated: Instant,
    focused: bool,
    frozen: Option<FrozenMotion>,
    player: Player,
    sound: Box<dyn FnMut(AudioEvent)>,
}

impl Session {
    fn new(
        mut game: Game,
        now: Instant,
        player: Player,
        sound: impl FnMut(AudioEvent) + 'static,
    ) -> Self {
        game.select_difficulty(player.preferences().last_difficulty);
        Self {
            game,
            impact_at: None,
            updated: now,
            focused: false,
            frozen: None,
            player,
            sound: Box::new(sound),
        }
    }

    fn advance(&mut self, now: Instant) {
        let score = self.game.score();
        let phase = self.game.phase();
        self.game.advance(now.duration_since(self.updated));
        self.updated = now;
        let next_score = self.game.score();
        if next_score > score {
            self.play_cue(Cue::Eat);
            if let Err(error) = self.player.record(self.game.difficulty(), next_score) {
                eprintln!("Omarchy Snake: could not save player state: {error}");
            }
        }
        if phase == Phase::Running {
            match self.game.phase() {
                Phase::Lost => {
                    self.impact_at = Some(now);
                    self.play_cue(Cue::Lose);
                }
                Phase::Won => self.play_cue(Cue::Win),
                _ => {}
            }
        }

        if matches!(self.game.phase(), Phase::Lost | Phase::Won)
            || self.game.previous_body()[0] != self.game.body()[0]
        {
            self.frozen = None;
        }
    }

    fn feedback(&self) -> presentation::Feedback {
        let strength = |at: Option<Instant>, duration: Duration| {
            at.map_or(0.0, |at| {
                let elapsed = self.updated.duration_since(at);
                (1.0 - elapsed.as_secs_f32() / duration.as_secs_f32()).max(0.0)
            })
        };

        const CONTACT: f32 = 0.15;
        let meal = ((1.0 - self.game.motion_progress()) / (1.0 - CONTACT)).clamp(0.0, 1.0);
        presentation::Feedback {
            impact: self
                .game
                .impact()
                .map(|cell| (cell, strength(self.impact_at, IMPACT_PULSE))),
            meal: self
                .game
                .last_meal()
                .filter(|_| {
                    self.game.phase() == Phase::Running
                        && self.game.previous_body()[0] != self.game.body()[0]
                        && meal > 0.0
                })
                .map(|cell| (cell, meal)),
        }
    }

    fn play_cue(&mut self, cue: Cue) {
        if !self.player.preferences().muted {
            (self.sound)(AudioEvent::Play(cue));
        }
    }

    fn motion(&self) -> (&[Cell], f32) {
        let progress = self.game.motion_progress();
        match &self.frozen {
            Some(frozen) => {
                let resumed = if self.game.phase() == Phase::Running {
                    progress
                } else {
                    0.0
                };
                (
                    &frozen.previous,
                    frozen.progress + (1.0 - frozen.progress) * resumed,
                )
            }
            None => (self.game.previous_body(), progress),
        }
    }

    fn freeze_motion(&mut self) {
        let (previous, progress) = self.motion();
        self.frozen = Some(FrozenMotion {
            previous: previous.to_vec(),
            progress,
        });
    }

    fn handle(&mut self, input: Input, now: Instant) {
        self.advance(now);
        match input {
            Input::Focus(focused) => {
                self.focused = focused;
                if !focused && self.game.phase() == Phase::Running {
                    self.freeze_motion();
                    self.game.pause();
                }
            }
            Input::Mute if self.focused => {
                if !self.player.preferences().muted {
                    (self.sound)(AudioEvent::Stop);
                }
                if let Err(error) = self.player.toggle_mute() {
                    eprintln!("Omarchy Snake: could not save player state: {error}");
                }
            }
            Input::Turn(direction) if self.focused => {
                self.game.turn(direction, false);
            }
            Input::SelectDifficulty(difficulty) if self.focused => {
                self.game.select_difficulty(difficulty);
            }
            Input::Play if self.focused => {
                let phase = self.game.phase();
                if phase == Phase::Running {
                    self.freeze_motion();
                }
                self.game.toggle_play();
                if phase.can_select_difficulty() {
                    if let Err(error) = self.player.remember_difficulty(self.game.difficulty()) {
                        eprintln!("Omarchy Snake: could not save player state: {error}");
                    }
                }
            }
            _ => {}
        }
    }
}

struct Display {
    window: Arc<Window>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
}

impl Display {
    fn new(event_loop: &ActiveEventLoop) -> Result<Self, Box<dyn Error>> {
        let attributes = Window::default_attributes()
            .with_title("Omarchy Snake")
            .with_name("omarchy-snake", "omarchy-snake")
            .with_inner_size(LogicalSize::new(600, 640))
            .with_min_inner_size(LogicalSize::new(224, 252));
        let window = Arc::new(event_loop.create_window(attributes)?);
        let context = softbuffer::Context::new(window.clone())?;
        let surface = softbuffer::Surface::new(&context, window.clone())?;
        Ok(Self { window, surface })
    }

    fn redraw(
        &mut self,
        session: &Session,
        theme: &Theme,
    ) -> Result<(), softbuffer::SoftBufferError> {
        let size = self.window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            // A subsequent nonzero resize requests another frame.
            return Ok(());
        };
        self.surface.resize(width, height)?;
        let mut buffer = self.surface.buffer_mut()?;
        let feedback = session.feedback();
        presentation::render(
            &mut buffer,
            (size.width as i32, size.height as i32),
            &session.game,
            session.motion(),
            theme.palette,
            session.player.preferences(),
            feedback,
        );
        self.window.pre_present_notify();
        buffer.present()?;
        if session.game.phase() == Phase::Running || feedback.active() {
            self.window.request_redraw();
        }
        Ok(())
    }
}

struct App {
    session: Session,
    theme: Theme,
    theme_deadline: Instant,
    display: Option<Display>,
    error: Option<Box<dyn Error>>,
}

impl App {
    fn redraw(&self) {
        if let Some(display) = &self.display {
            display.window.request_redraw();
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: Box<dyn Error>) {
        self.error = Some(error);
        event_loop.exit();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.display.is_some() {
            return;
        }
        match Display::new(event_loop) {
            Ok(display) => {
                self.session
                    .handle(Input::Focus(display.window.has_focus()), Instant::now());
                display.window.request_redraw();
                self.display = Some(display);
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(display) = &mut self.display else {
            return;
        };
        if display.window.id() != id {
            return;
        }
        let now = Instant::now();
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(focused) => {
                self.session.handle(Input::Focus(focused), now);
                display.window.request_redraw();
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } if event.state == ElementState::Pressed && !event.repeat && self.session.focused => {
                if matches!(&event.logical_key, Key::Character(key) if key.eq_ignore_ascii_case("q"))
                {
                    event_loop.exit();
                } else if let Some(input) = key_input(&event.logical_key) {
                    self.session.handle(input, now);
                    display.window.request_redraw();
                }
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                display.window.request_redraw()
            }
            WindowEvent::RedrawRequested => {
                self.session.advance(now);
                if let Err(error) = display.redraw(&self.session, &self.theme) {
                    self.fail(event_loop, error.into());
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let phase = self.session.game.phase();
        self.session.advance(now);
        if phase != self.session.game.phase() {
            self.redraw();
        }
        if now >= self.theme_deadline {
            self.theme_deadline = now + THEME_INTERVAL;
            if self.theme.reload() {
                self.redraw();
            }
        }
        let deadline = if self.session.game.phase() == Phase::Running {
            self.theme_deadline.min(now + SIMULATION_WAKEUP)
        } else {
            self.theme_deadline
        };
        event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::builder().with_wayland().build()?;
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let state_path = player::user_state_path();
    if state_path.is_none() {
        eprintln!("Omarchy Snake: player state unavailable: no absolute XDG_STATE_HOME or HOME");
    }
    let player = Player::load(state_path);
    let audio = Audio::new()
        .map_err(|error| eprintln!("Omarchy Snake: audio unavailable: {error}"))
        .ok();
    let now = Instant::now();
    let mut app = App {
        session: Session::new(Game::new(seed), now, player, move |event| {
            if let Some(audio) = &audio {
                audio.handle(event);
            }
        }),
        theme: Theme::new(theme::active_path()),
        theme_deadline: now + THEME_INTERVAL,
        display: None,
        error: None,
    };
    event_loop.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use omarchy_snake::Cell;

    fn observed_session(
        game: Game,
        start: Instant,
        player: Player,
    ) -> (Session, std::rc::Rc<std::cell::RefCell<Vec<AudioEvent>>>) {
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let observed = events.clone();
        let session = Session::new(game, start, player, move |event| {
            observed.borrow_mut().push(event);
        });
        (session, events)
    }

    #[test]
    fn number_keys_select_the_starting_speed_without_starting() {
        for (number, speed) in [("1", 6), ("2", 8), ("3", 10)] {
            let now = Instant::now();
            let (mut session, _) = observed_session(Game::new(1), now, Player::load(None));
            session.handle(Input::Focus(true), now);
            session.handle(key_input(&Key::Character(number.into())).unwrap(), now);
            assert_eq!(session.game.phase(), Phase::Ready);
            session.handle(key_input(&Key::Named(NamedKey::Space)).unwrap(), now);
            assert_eq!(session.game.phase(), Phase::Running);
            assert_eq!(session.game.cells_per_second(), speed);
        }
    }

    #[test]
    fn reopening_uses_the_last_started_difficulty_not_an_unplayed_selection() {
        let scratch = player::tests::Scratch::new();
        let path = scratch.state_path();
        let open = |now| observed_session(Game::new(1), now, Player::load(Some(path.clone()))).0;
        let now = Instant::now();
        let mut session = open(now);
        assert_eq!(session.game.selected_difficulty(), Difficulty::Normal);
        session.handle(Input::Focus(true), now);
        session.handle(Input::SelectDifficulty(Difficulty::Easy), now);
        assert_eq!(open(now).game.selected_difficulty(), Difficulty::Normal);
        session.handle(Input::Play, now);
        session.handle(Input::SelectDifficulty(Difficulty::Hard), now);
        session.handle(Input::Play, now);
        assert_eq!(session.game.phase(), Phase::Paused);
        session.handle(Input::SelectDifficulty(Difficulty::Hard), now);
        session.handle(Input::Play, now);
        assert_eq!(session.game.phase(), Phase::Running);
        assert_eq!(session.game.difficulty(), Difficulty::Easy);
        assert_eq!(session.game.selected_difficulty(), Difficulty::Easy);
        drop(session);

        let mut reopened = open(now);
        assert_eq!(reopened.game.selected_difficulty(), Difficulty::Easy);
        reopened.handle(Input::Focus(true), now);
        reopened.handle(Input::Play, now);
        assert_eq!(reopened.game.difficulty(), Difficulty::Easy);
    }

    #[test]
    fn eating_updates_the_saved_record_and_plays_only_on_the_score_change() {
        let scratch = player::tests::Scratch::new();
        let path = scratch.state_path();
        let start = Instant::now();
        let game = Game::from_board(vec![Cell::from((0, 0))], Direction::Right, (1, 0).into(), 3);
        let (mut session, events) = observed_session(game, start, Player::load(Some(path.clone())));
        session.advance(start + Duration::from_millis(100));
        assert!(events.borrow().is_empty());
        assert_eq!(session.game.score(), 3);
        session.advance(start + Duration::from_millis(170));
        session.advance(start + Duration::from_millis(180));
        assert_eq!(*events.borrow(), [AudioEvent::Play(Cue::Eat)]);
        assert_eq!(session.game.score(), 4);
        assert_eq!(Player::load(Some(path)).preferences().best, 4);
    }

    fn losing_game() -> Game {
        Game::from_board(
            [(1, 0), (1, 1), (2, 1), (2, 0), (3, 0)]
                .map(Cell::from)
                .to_vec(),
            Direction::Right,
            (5, 5).into(),
            0,
        )
    }

    #[test]
    fn mute_key_persists_and_suppresses_audio_without_suppressing_the_result() {
        let scratch = player::tests::Scratch::new();
        let path = scratch.state_path();
        let start = Instant::now();
        let (mut session, events) =
            observed_session(losing_game(), start, Player::load(Some(path.clone())));
        let mute = key_input(&Key::Character("M".into())).unwrap();
        assert!(matches!(
            key_input(&Key::Character("m".into())),
            Some(Input::Mute)
        ));
        session.handle(mute, start);
        assert!(!session.player.preferences().muted);
        assert!(events.borrow().is_empty());
        session.handle(Input::Focus(true), start);
        session.handle(mute, start);
        assert_eq!(*events.borrow(), [AudioEvent::Stop]);
        session.advance(start + Duration::from_millis(170));
        assert_eq!(session.game.phase(), Phase::Lost);
        assert_eq!(*events.borrow(), [AudioEvent::Stop]);
        drop(session);

        let muted_player = Player::load(Some(path.clone()));
        assert!(muted_player.preferences().muted);
        let eating = Game::from_board(vec![(0, 0).into()], Direction::Right, (1, 0).into(), 0);
        let (mut reopened, reopened_events) = observed_session(eating, start, muted_player);
        reopened.handle(Input::Focus(true), start);
        reopened.advance(start + Duration::from_millis(170));
        assert_eq!(reopened.game.score(), 1);
        assert!(reopened_events.borrow().is_empty());
        reopened.handle(mute, start + Duration::from_millis(180));
        let saved = Player::load(Some(path)).preferences();
        assert!(!saved.muted);
        assert_eq!(saved.best, 1);
        assert!(reopened_events.borrow().is_empty());
    }

    #[test]
    fn unmuted_loss_and_win_sound_once_and_winning_also_counts_as_eating() {
        let start = Instant::now();
        let missing = Cell::from((1, 0));
        let size = omarchy_snake::BOARD_SIZE;
        let winning = Game::from_board(
            (0..size)
                .flat_map(|y| (0..size).map(move |x| Cell { x, y }))
                .filter(|&cell| cell != missing)
                .collect(),
            Direction::Right,
            missing,
            0,
        );
        for (game, phase, expected) in [
            (
                losing_game(),
                Phase::Lost,
                vec![AudioEvent::Play(Cue::Lose)],
            ),
            (
                winning,
                Phase::Won,
                vec![AudioEvent::Play(Cue::Eat), AudioEvent::Play(Cue::Win)],
            ),
        ] {
            let (mut session, events) = observed_session(game, start, Player::load(None));
            session.handle(Input::Focus(true), start);
            session.advance(start + Duration::from_millis(170));
            session.advance(start + Duration::from_millis(180));
            assert_eq!(session.game.phase(), phase);
            assert_eq!(*events.borrow(), expected);
            let now = start + Duration::from_millis(180);
            let score = session.game.score();
            session.handle(key_input(&Key::Character("3".into())).unwrap(), now);
            assert_eq!(session.game.phase(), phase);
            assert_eq!(session.game.score(), score);
            session.handle(Input::Play, now);
            assert_eq!(session.game.phase(), Phase::Running);
            assert_eq!(session.game.score(), 0);
            assert_eq!(session.game.cells_per_second(), 10);
            assert_eq!(*events.borrow(), expected);
        }
    }

    #[test]
    fn impact_pulses_once_and_leaves_a_mark_until_immediate_retry() {
        let start = Instant::now();
        let (mut session, _) = observed_session(losing_game(), start, Player::load(None));
        session.handle(Input::Focus(true), start);
        session.advance(start + Duration::from_millis(125));
        let hit = Cell::from((2, 0));
        assert_eq!(session.game.body()[0], Cell::from((1, 0)));
        assert_eq!(session.feedback().impact, Some((hit, 1.0)));
        session.advance(start + Duration::from_millis(285));
        assert_eq!(session.feedback().impact, Some((hit, 0.0)));
        assert!(!session.feedback().active());

        for delay in [125, 285] {
            let (mut session, _) = observed_session(losing_game(), start, Player::load(None));
            session.handle(Input::Focus(true), start);
            session.advance(start + Duration::from_millis(125));
            session.handle(Input::Play, start + Duration::from_millis(delay));
            assert_eq!(session.game.phase(), Phase::Running);
            assert_eq!(session.feedback().impact, None);
        }
    }

    #[test]
    fn meal_shrinks_during_contact_and_clears_on_the_next_step() {
        let start = Instant::now();
        let eaten = Cell::from((1, 0));
        let game = Game::from_board(vec![(0, 0).into()], Direction::Right, eaten, 0);
        let (mut session, _) = observed_session(game, start, Player::load(None));
        assert_eq!(session.feedback().meal, None);
        session.advance(start + Duration::from_millis(125));
        let next_food = session.game.food().unwrap();
        assert_ne!(next_food, eaten);
        assert_eq!(session.feedback().meal, Some((eaten, 1.0)));
        session.advance(start + Duration::from_millis(140));
        assert_eq!(session.feedback().meal, Some((eaten, 1.0)));
        session.advance(start + Duration::from_millis(200));
        let (cell, scale) = session.feedback().meal.unwrap();
        assert_eq!(cell, eaten);
        assert!(scale > 0.0 && scale < 0.5);
        session.advance(start + Duration::from_millis(250));
        assert_eq!(session.feedback().meal, None);
        assert_eq!(session.game.food(), Some(next_food));
        assert_eq!(session.game.phase(), Phase::Running);
    }

    #[test]
    fn question_mark_is_not_a_game_command() {
        assert!(key_input(&Key::Character("?".into())).is_none());
    }

    #[test]
    fn interruptions_freeze_the_rendered_motion_and_require_focused_manual_resume() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        for interrupt in [Input::Play, Input::Focus(false)] {
            let (mut session, events) = observed_session(Game::new(1), start, Player::load(None));
            session.handle(Input::Focus(true), start);
            session.handle(Input::Play, start);
            session.advance(at(200));
            let head = session.game.body()[0];
            assert_eq!(head, Cell::from((9, 8)));
            let (previous, progress) = session.motion();
            let previous_head = previous[0];
            assert!(progress > 0.0 && progress < 1.0);
            session.handle(Input::Turn(Direction::Up), at(200));
            session.handle(interrupt, at(200));
            session.advance(at(1000));
            assert_eq!(session.game.phase(), Phase::Paused);
            assert_eq!(session.game.body()[0], head);
            assert_eq!(session.motion().0[0], previous_head);
            assert_eq!(session.motion().1, progress);
            if matches!(interrupt, Input::Focus(false)) {
                session.handle(Input::Play, at(1000));
                assert_eq!(session.game.phase(), Phase::Paused);
                session.handle(Input::Focus(true), at(1000));
            }
            session.advance(at(1100));
            assert_eq!(session.game.phase(), Phase::Paused);
            session.handle(Input::Play, at(1100));
            assert_eq!(session.game.phase(), Phase::Running);
            assert_eq!(session.motion().0[0], previous_head);
            assert_eq!(session.motion().1, progress);
            session.advance(at(1300));
            assert_eq!(session.game.body()[0], Cell::from((10, 8)));
            assert_eq!(session.motion().0[0], head);
            assert!(events.borrow().is_empty());
        }
    }
}
