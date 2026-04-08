//! Terminal-aware status icons.
//!
//! Wraps `console::Emoji` so the binary prints unicode glyphs on
//! UTF-8-capable terminals and ASCII fallbacks elsewhere (legacy
//! Windows cmd.exe, dumb terminals, piped output). Centralized so the
//! same icons render consistently across every command and so future
//! style changes touch one file.

use console::Emoji;

/// "Done" / success marker. Renders `✓` on supporting terminals,
/// `[OK]` elsewhere.
pub static OK: Emoji<'static, 'static> = Emoji("✓", "[OK]");

/// Warning marker. Renders `⚠` on supporting terminals, `[!]` elsewhere.
pub static WARN: Emoji<'static, 'static> = Emoji("⚠", "[!]");

/// Hint / nudge marker. Renders `💡` on supporting terminals, `[hint]`
/// elsewhere.
pub static HINT: Emoji<'static, 'static> = Emoji("💡", "[hint]");
