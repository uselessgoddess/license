use crate::prelude::*;

pub fn format_date(date: DateTime) -> String {
  date.format("%d.%m.%Y %H:%M").to_string()
}

pub fn format_duration(duration: TimeDelta) -> String {
  format!(
    "{}d {}h {}m",
    duration.num_days(),
    duration.num_hours() % 24,
    duration.num_minutes() % 60
  )
}

/// Maximum message length for Telegram Bot API (4096 characters).
/// We use a slightly smaller limit to account for potential HTML entity expansion.
const TELEGRAM_MAX_MESSAGE_LENGTH: usize = 4000;

/// Splits a long message into chunks that fit within Telegram's message limit.
/// Attempts to split at newline boundaries to preserve formatting.
pub fn chunk_message(text: &str, max_len: usize) -> Vec<String> {
  let max_len =
    if max_len == 0 { TELEGRAM_MAX_MESSAGE_LENGTH } else { max_len };

  if text.len() <= max_len {
    return vec![text.to_string()];
  }

  let mut chunks = Vec::new();
  let mut current = String::new();

  for line in text.lines() {
    // If adding this line would exceed the limit
    if !current.is_empty() && current.len() + line.len() + 1 > max_len {
      chunks.push(current);
      current = String::new();
    }

    // If a single line is longer than max_len, we need to split it
    if line.len() > max_len {
      // First, push any existing content
      if !current.is_empty() {
        chunks.push(current);
        current = String::new();
      }
      // Split the long line
      let mut remaining = line;
      while remaining.len() > max_len {
        chunks.push(remaining[..max_len].to_string());
        remaining = &remaining[max_len..];
      }
      if !remaining.is_empty() {
        current = remaining.to_string();
      }
    } else {
      if !current.is_empty() {
        current.push('\n');
      }
      current.push_str(line);
    }
  }

  if !current.is_empty() {
    chunks.push(current);
  }

  chunks
}
