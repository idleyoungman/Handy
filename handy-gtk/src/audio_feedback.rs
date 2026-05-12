use std::io::Cursor;

use crate::app_context::AppContext;
use crate::config::SoundTheme;

pub enum SoundType {
    Start,
    Stop,
}

static DEFAULT_START: &[u8] = include_bytes!("../sounds/pop_start.wav");
static DEFAULT_STOP: &[u8] = include_bytes!("../sounds/pop_stop.wav");

/// Plays the appropriate feedback sound for `sound_type` in a background thread.
/// Does nothing if `audio_feedback` is disabled in settings.
pub fn play_feedback_sound(ctx: &AppContext, sound_type: SoundType) {
    let settings = ctx.settings();
    if !settings.audio_feedback {
        return;
    }
    let volume = settings.audio_feedback_volume;
    let theme = settings.sound_theme;

    std::thread::spawn(move || {
        if let Err(e) = do_play(sound_type, theme, volume) {
            tracing::warn!("Audio feedback failed: {e}");
        }
    });
}

fn do_play(
    sound_type: SoundType,
    theme: SoundTheme,
    volume: f32,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let data: Vec<u8> = match theme {
        SoundTheme::Default => match sound_type {
            SoundType::Start => DEFAULT_START.to_vec(),
            SoundType::Stop => DEFAULT_STOP.to_vec(),
        },
        SoundTheme::Custom => {
            let filename = match sound_type {
                SoundType::Start => "custom_start.wav",
                SoundType::Stop => "custom_stop.wav",
            };
            let path = dirs::data_dir()
                .map(|d| d.join("handy").join("sounds").join(filename))
                .ok_or("Could not determine data directory")?;
            if !path.exists() {
                tracing::warn!(
                    "Custom sound not found: {}, falling back to default",
                    path.display()
                );
                match sound_type {
                    SoundType::Start => DEFAULT_START.to_vec(),
                    SoundType::Stop => DEFAULT_STOP.to_vec(),
                }
            } else {
                std::fs::read(&path)?
            }
        }
    };

    play_bytes(data, volume)
}

fn play_bytes(data: Vec<u8>, volume: f32) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rodio::{Decoder, Player};

    let handle = rodio::DeviceSinkBuilder::open_default_sink()
        .map_err(|e| format!("Audio output unavailable: {e}"))?;
    let player = Player::connect_new(handle.mixer());
    let cursor = Cursor::new(data);
    let decoder = Decoder::try_from(cursor).map_err(|e| format!("Audio decode error: {e}"))?;
    player.append(decoder);
    player.set_volume(volume);
    player.sleep_until_end();
    Ok(())
}
