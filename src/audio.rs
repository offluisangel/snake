use std::{
    io::{self, Write},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cue {
    Eat,
    Lose,
    Win,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioEvent {
    Play(Cue),
    Stop,
}

pub struct Audio {
    queue: Option<SyncSender<(u64, Cue)>>,
    generation: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}

impl Audio {
    pub fn new() -> io::Result<Self> {
        let (queue, pending) = mpsc::sync_channel(4);
        let generation = Arc::new(AtomicU64::new(0));
        let current = generation.clone();
        let worker = thread::Builder::new()
            .name("snake-audio".into())
            .spawn(move || {
                while let Ok((generation, cue)) = pending.recv() {
                    let cancelled = || current.load(Ordering::Acquire) != generation;
                    if !cancelled()
                        && let Err(error) = play(cue, cancelled)
                    {
                        eprintln!("Omarchy Snake: audio unavailable: {error}");
                        break;
                    }
                }
            })?;
        Ok(Self {
            queue: Some(queue),
            generation,
            worker: Some(worker),
        })
    }

    pub fn handle(&self, event: AudioEvent) {
        match event {
            AudioEvent::Play(cue) => {
                if let Some(queue) = &self.queue {
                    let generation = self.generation.load(Ordering::Acquire);

                    let _ = queue.try_send((generation, cue));
                }
            }
            AudioEvent::Stop => {
                self.generation.fetch_add(1, Ordering::AcqRel);
            }
        }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        self.handle(AudioEvent::Stop);
        drop(self.queue.take());

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn play(cue: Cue, cancelled: impl Fn() -> bool) -> io::Result<()> {
    let mut child = Command::new("pw-play")
        .args(["--media-role", "Game", "--latency=20ms", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let result = stream(&mut child, cue, &cancelled);
    if result.is_err() || cancelled() {
        let _ = child.kill();
        child.wait()?;
    }
    if cancelled() { Ok(()) } else { result }
}

fn stream(child: &mut Child, cue: Cue, cancelled: impl Fn() -> bool) -> io::Result<()> {
    let wav = wave(cue);

    child.stdin.take().unwrap().write_all(wav)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while !cancelled() {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(io::Error::other(format!("pw-play exited with {status}")))
            };
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "pw-play timed out"));
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

fn wave(cue: Cue) -> &'static [u8] {
    match cue {
        Cue::Eat => include_bytes!("sounds/eat.wav"),
        Cue::Lose => include_bytes!("sounds/lose.wav"),
        Cue::Win => include_bytes!("sounds/win.wav"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues_match_the_selected_dry_percussion_preview() {
        for (cue, frames, expected) in [
            (Cue::Eat, 704, 0xe41a_17e2_8ab6_a562),
            (Cue::Lose, 1600, 0xcf29_68a2_fa42_f6a0),
            (Cue::Win, 1920, 0x8b69_766f_4224_55fc),
        ] {
            let wav = wave(cue);
            let pcm = &wav[44..];
            assert_eq!(pcm.len(), frames * 2, "{cue:?}");
            let fingerprint = pcm.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &sample| {
                (hash ^ u64::from(sample)).wrapping_mul(0x0000_0100_0000_01b3)
            });
            assert_eq!(fingerprint, expected, "{cue:?}");
        }
    }

    #[test]
    fn cues_are_distinct_short_mono_pcm_waves() {
        let waves = [Cue::Eat, Cue::Lose, Cue::Win].map(wave);
        for (index, wav) in waves.iter().enumerate() {
            assert_eq!(&wav[..4], b"RIFF");
            assert_eq!(&wav[8..16], b"WAVEfmt ");
            assert_eq!(&wav[20..24], &[1, 0, 1, 0]); // PCM, mono
            assert_eq!(&wav[34..40], &[16, 0, b'd', b'a', b't', b'a']);
            assert_eq!(
                u32::from_le_bytes(wav[4..8].try_into().unwrap()) as usize,
                wav.len() - 8
            );
            assert_eq!(
                u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize,
                wav.len() - 44
            );
            assert!(wav[44..].iter().any(|&byte| byte != 0));
            assert!(wav.len() <= 4096);
            assert!(!waves[..index].contains(wav));
        }
    }
}
