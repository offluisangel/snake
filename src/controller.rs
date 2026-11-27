use crate::ControllerInput;
use gilrs::{Axis, Button, Event, EventType, GamepadId, Gilrs};
use omarchy_snake::Direction;

#[derive(Default)]
struct Stick {
    position: (f32, f32),
    reported: Option<Direction>,
}

impl Stick {
    fn take_turn(&mut self) -> Option<Direction> {
        const ENGAGE: f32 = 0.60;
        const RELEASE: f32 = 0.30;
        const AXIS_MARGIN: f32 = 0.15;

        let (x, y) = self.position;
        let (horizontal, vertical) = (x.abs(), y.abs());
        let tilt = horizontal.max(vertical);
        if tilt <= RELEASE {
            self.reported = None;
        }
        if tilt < ENGAGE {
            return None;
        }
        // Ignore slight changes in the dominant axis while holding a diagonal.
        if self.reported.is_some() && (horizontal - vertical).abs() < AXIS_MARGIN {
            return None;
        }
        let direction = if horizontal >= vertical {
            if x > 0.0 {
                Direction::Right
            } else {
                Direction::Left
            }
        } else if y > 0.0 {
            Direction::Up
        } else {
            Direction::Down
        };
        (self.reported.replace(direction) != Some(direction)).then_some(direction)
    }

    fn dispatch_button(
        &mut self,
        input: ControllerInput,
        dpad_pressed: &mut bool,
        action: &mut impl FnMut(ControllerInput),
    ) {
        if input == ControllerInput::Play {
            // Apply earlier motion in the phase in which it arrived.
            self.flush(std::mem::take(dpad_pressed), action);
        }
        *dpad_pressed |= matches!(input, ControllerInput::Turn(_));
        action(input);
    }

    fn flush(&mut self, dpad_pressed: bool, action: &mut impl FnMut(ControllerInput)) {
        if let Some(direction) = self.take_turn()
            && !dpad_pressed
        {
            action(ControllerInput::Stick(direction));
        }
    }
}

pub struct Controller {
    gilrs: Gilrs,
    active: Option<GamepadId>,
    stick: Stick,
}

impl Controller {
    pub fn open() -> Option<Self> {
        match Gilrs::new() {
            Ok(gilrs) => {
                let active = gilrs.gamepads().next().map(|(id, _)| id);
                Some(Self {
                    gilrs,
                    active,
                    stick: Stick::default(),
                })
            }
            Err(error) => {
                eprintln!("Omarchy Snake: controller support unavailable: {error}");
                None
            }
        }
    }

    pub fn connected(&self) -> bool {
        self.active.is_some()
    }

    pub fn poll(&mut self, mut action: impl FnMut(ControllerInput)) {
        let mut dpad_pressed = false;
        while let Some(Event { id, event, .. }) = self.gilrs.next_event() {
            match event {
                EventType::Connected if self.active.is_none() => self.active = Some(id),
                EventType::Disconnected if self.active == Some(id) => {
                    self.active = self.gilrs.gamepads().next().map(|(id, _)| id);
                    self.stick = Stick::default();
                }
                EventType::ButtonPressed(button, _) if self.active == Some(id) => {
                    if let Some(input) = controller_input(button) {
                        self.stick
                            .dispatch_button(input, &mut dpad_pressed, &mut action);
                    }
                }
                EventType::AxisChanged(axis, value, _) if self.active == Some(id) => match axis {
                    Axis::LeftStickX => self.stick.position.0 = value,
                    Axis::LeftStickY => self.stick.position.1 = value,
                    _ => {}
                },
                _ => {}
            }
        }
        // Sample both axes together, and give the D-pad priority within this poll.
        self.stick.flush(dpad_pressed, &mut action);
    }
}

fn controller_input(button: Button) -> Option<ControllerInput> {
    let direction = match button {
        Button::DPadUp => Direction::Up,
        Button::DPadDown => Direction::Down,
        Button::DPadLeft => Direction::Left,
        Button::DPadRight => Direction::Right,
        Button::East => return Some(ControllerInput::Play),
        Button::West => return Some(ControllerInput::Mute),
        Button::Select => return Some(ControllerInput::Quit),
        _ => return None,
    };
    Some(ControllerInput::Turn(direction))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_stick_ignores_drift_and_chooses_one_stable_cardinal_turn() {
        let mut stick = Stick::default();
        let mut sample = |x, y| {
            stick.position = (x, y);
            stick.take_turn()
        };

        assert_eq!(sample(0.25, -0.15), None);
        assert_eq!(sample(0.8, 0.8), Some(Direction::Right));
        assert_eq!(sample(0.79, 0.82), None);
        assert_eq!(sample(0.39, 0.4), None);
        assert_eq!(sample(0.8, 0.0), None);
        assert_eq!(sample(0.45, 0.9), Some(Direction::Up));
        assert_eq!(sample(0.0, 0.25), None);
        assert_eq!(sample(0.0, 0.85), Some(Direction::Up));
        assert_eq!(sample(-0.9, 0.2), Some(Direction::Left));
        assert_eq!(sample(0.0, -0.9), Some(Direction::Down));
    }

    #[test]
    fn controller_stick_preserves_order_across_a_play_button_in_the_same_poll() {
        use crate::{Cell, Game, Input, Phase, Player, Session};
        use std::time::{Duration, Instant};

        for (axis_first, expected_head) in [(true, (9, 8)), (false, (8, 7))] {
            let now = Instant::now();
            let mut session = Session::new(Game::new(1), now, Player::load(None), |_| {});
            session.handle(Input::Focus(true), now);
            session.handle(Input::Play, now);
            session.handle(Input::Play, now);
            assert_eq!(session.game.phase(), Phase::Paused);

            let mut stick = Stick::default();
            let mut dpad_pressed = false;
            let mut dispatch = |input| {
                session.handle_controller(input, now);
            };
            if axis_first {
                stick.position = (0.0, 0.9);
            }
            stick.dispatch_button(ControllerInput::Play, &mut dpad_pressed, &mut dispatch);
            if !axis_first {
                stick.position = (0.0, 0.9);
            }
            stick.flush(dpad_pressed, &mut dispatch);
            assert_eq!(session.game.phase(), Phase::Running);
            session.advance(now + Duration::from_millis(130));
            assert_eq!(session.game.body()[0], Cell::from(expected_head));
        }
    }

    #[test]
    fn controller_maps_the_dpad_play_mute_and_quit() {
        for (button, input) in [
            (Button::DPadUp, ControllerInput::Turn(Direction::Up)),
            (Button::DPadDown, ControllerInput::Turn(Direction::Down)),
            (Button::DPadLeft, ControllerInput::Turn(Direction::Left)),
            (Button::DPadRight, ControllerInput::Turn(Direction::Right)),
            (Button::East, ControllerInput::Play),
            (Button::West, ControllerInput::Mute),
            (Button::Select, ControllerInput::Quit),
        ] {
            assert_eq!(controller_input(button), Some(input));
        }
        assert_eq!(controller_input(Button::South), None);
    }
}
