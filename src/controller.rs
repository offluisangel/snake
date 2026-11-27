use crate::ControllerInput;
use gilrs::{Button, Event, EventType, GamepadId, Gilrs};
use omarchy_snake::Direction;

pub struct Controller {
    gilrs: Gilrs,
    active: Option<GamepadId>,
}

impl Controller {
    pub fn open() -> Option<Self> {
        match Gilrs::new() {
            Ok(gilrs) => {
                let active = gilrs.gamepads().next().map(|(id, _)| id);
                Some(Self { gilrs, active })
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
        while let Some(Event { id, event, .. }) = self.gilrs.next_event() {
            match event {
                EventType::Connected if self.active.is_none() => self.active = Some(id),
                EventType::Disconnected if self.active == Some(id) => {
                    self.active = self.gilrs.gamepads().next().map(|(id, _)| id);
                }
                EventType::ButtonPressed(button, _) if self.active == Some(id) => {
                    if let Some(input) = controller_input(button) {
                        action(input);
                    }
                }
                _ => {}
            }
        }
    }
}

fn controller_input(button: Button) -> Option<ControllerInput> {
    let direction = match button {
        Button::DPadUp => Direction::Up,
        Button::DPadDown => Direction::Down,
        Button::DPadLeft => Direction::Left,
        Button::DPadRight => Direction::Right,
        Button::East => return Some(ControllerInput::Play),
        Button::Select => return Some(ControllerInput::Quit),
        _ => return None,
    };
    Some(ControllerInput::Turn(direction))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_maps_the_dpad_play_and_quit() {
        for (button, input) in [
            (Button::DPadUp, ControllerInput::Turn(Direction::Up)),
            (Button::DPadDown, ControllerInput::Turn(Direction::Down)),
            (Button::DPadLeft, ControllerInput::Turn(Direction::Left)),
            (Button::DPadRight, ControllerInput::Turn(Direction::Right)),
            (Button::East, ControllerInput::Play),
            (Button::Select, ControllerInput::Quit),
        ] {
            assert_eq!(controller_input(button), Some(input));
        }
        assert_eq!(controller_input(Button::South), None);
    }
}
