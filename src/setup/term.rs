//! The terminal side of `setup` (SPEC § 14).
//!
//! Raw mode, the alternate screen and mouse capture on the way in, all
//! three undone on the way out, on `Ctrl+C` and on a panic; crossterm
//! events as the screen's own inputs.

use std::io::Write as _;
use std::time::{Duration, Instant};

use ratatui::crossterm::cursor::Show;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseButton, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use super::app::{App, Input, Key, Mouse};

/// The terminal put back the way it was found, whichever way `setup` ends.
///
/// The panic hook is chained ahead of the one already installed
/// (color-eyre's), so a report prints on a restored terminal; it runs
/// before the release profile's abort, since a hook runs before unwinding
/// (or aborting) starts.
struct Guard;

impl Guard {
    fn enter() -> std::io::Result<Self> {
        enable_raw_mode()?;
        // The guard exists from here, so a failure of the next step still
        // leaves raw mode the way it was found.
        let guard = Self;
        let mut out = std::io::stdout();
        execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
        }));
        Ok(guard)
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        restore();
    }
}

/// Leave the alternate screen, drop mouse capture and raw mode, and show
/// the cursor again (`Terminal::draw` hides it, and the `Terminal` drop
/// that would show it never runs under the release profile's abort on a
/// panic). Safe to call twice: every step is idempotent, and a failure to
/// undo one is not worth a report on the way out.
fn restore() {
    let mut out = std::io::stdout();
    let _ = execute!(out, DisableMouseCapture, LeaveAlternateScreen, Show);
    let _ = disable_raw_mode();
    let _ = out.flush();
}

/// The screen's input for a crossterm event, or `None` for one it ignores
/// (a key release, focus, a mouse move).
#[must_use]
pub fn input(event: &Event) -> Option<Input> {
    match *event {
        Event::Key(k) if k.kind != KeyEventKind::Release => {
            let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
            let key = match k.code {
                KeyCode::Char('c') if ctrl => Key::CtrlC,
                KeyCode::Char(c) if ctrl => Key::Ctrl(c),
                KeyCode::Char(c) => Key::Char(c),
                KeyCode::Enter => Key::Enter,
                KeyCode::Esc => Key::Esc,
                KeyCode::Tab => Key::Tab,
                KeyCode::BackTab => Key::BackTab,
                KeyCode::Up => Key::Up,
                KeyCode::Down => Key::Down,
                KeyCode::Left => Key::Left,
                KeyCode::Right => Key::Right,
                KeyCode::Backspace => Key::Backspace,
                KeyCode::Delete => Key::Delete,
                KeyCode::Home => Key::Home,
                KeyCode::End => Key::End,
                KeyCode::PageUp => Key::PageUp,
                KeyCode::PageDown => Key::PageDown,
                _ => return None,
            };
            Some(Input::Key(key))
        }
        Event::Mouse(m) => {
            let kind = match m.kind {
                MouseEventKind::Down(MouseButton::Left) => Mouse::Click,
                MouseEventKind::ScrollUp => Mouse::Wheel(-1),
                MouseEventKind::ScrollDown => Mouse::Wheel(1),
                _ => return None,
            };
            Some(Input::Mouse { x: m.column, y: m.row, kind })
        }
        Event::Resize(w, h) => Some(Input::Resize(w, h)),
        _ => None,
    }
}

/// How often the clock moves, and the animations with it.
const TICK: Duration = Duration::from_millis(250);

/// When the loop draws and when the clock ticks: a frame after an input
/// the screen took (not after one it ignores, which a moving mouse sends
/// by the dozen), and a tick every [`TICK`] whatever else arrives, so a
/// stream of events never stalls the animations.
struct Pace {
    last_tick: Instant,
    stale: bool,
}

impl Pace {
    const fn new(now: Instant) -> Self {
        Self { last_tick: now, stale: true }
    }

    /// How long to wait for an event before the next tick is due.
    fn wait(&self, now: Instant) -> Duration {
        TICK.saturating_sub(now.saturating_duration_since(self.last_tick))
    }

    /// The inputs to feed after a wait that ended at `now` with `read`
    /// (the screen's input for the event read, if any): that input, then a
    /// tick when one is due.
    fn after(&mut self, now: Instant, read: Option<Input>) -> Vec<Input> {
        let mut inputs: Vec<Input> = read.into_iter().collect();
        if now.saturating_duration_since(self.last_tick) >= TICK {
            self.last_tick = now;
            inputs.push(Input::Tick);
        }
        self.stale |= !inputs.is_empty();
        inputs
    }

    /// Whether a frame is due, which it no longer is once asked.
    const fn take_draw(&mut self) -> bool {
        let stale = self.stale;
        self.stale = false;
        stale
    }
}

/// Run the screen until it asks to quit: draw when something changed,
/// wait for an event (or the next tick, so the clock-driven animations
/// move), feed it in.
///
/// # Errors
/// A terminal that cannot be put into raw mode or drawn to.
pub fn run(app: &mut App) -> std::io::Result<()> {
    let guard = Guard::enter()?;
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;
    let size = terminal.size()?;
    app.input(Input::Resize(size.width, size.height));
    let mut pace = Pace::new(Instant::now());
    while !app.done() {
        if pace.take_draw() {
            terminal.draw(|frame| app.draw(frame))?;
        }
        let read =
            if event::poll(pace.wait(Instant::now()))? { input(&event::read()?) } else { None };
        for i in pace.after(Instant::now(), read) {
            app.input(i);
        }
    }
    drop(guard);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyEvent, MouseEvent};

    #[test]
    fn crossterm_events_become_inputs_and_releases_are_ignored() {
        let key = |code| Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
        assert_eq!(input(&key(KeyCode::Char('q'))), Some(Input::Key(Key::Char('q'))));
        assert_eq!(input(&key(KeyCode::BackTab)), Some(Input::Key(Key::BackTab)));
        assert_eq!(
            input(&Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))),
            Some(Input::Key(Key::CtrlC))
        );
        assert_eq!(
            input(&Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))),
            Some(Input::Key(Key::Ctrl('s')))
        );
        let mut release = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        release.kind = KeyEventKind::Release;
        assert_eq!(input(&Event::Key(release)), None);
        assert_eq!(input(&key(KeyCode::F(1))), None);
        let mouse = |kind| {
            Event::Mouse(MouseEvent { kind, column: 3, row: 4, modifiers: KeyModifiers::NONE })
        };
        assert_eq!(
            input(&mouse(MouseEventKind::Down(MouseButton::Left))),
            Some(Input::Mouse { x: 3, y: 4, kind: Mouse::Click })
        );
        assert_eq!(
            input(&mouse(MouseEventKind::ScrollDown)),
            Some(Input::Mouse { x: 3, y: 4, kind: Mouse::Wheel(1) })
        );
        assert_eq!(input(&mouse(MouseEventKind::Moved)), None);
        assert_eq!(input(&Event::Resize(80, 24)), Some(Input::Resize(80, 24)));
        assert_eq!(input(&Event::FocusGained), None);
    }

    /// app-24: an event the screen ignores (a mouse move) draws nothing,
    /// and a stream of events never holds the clock back.
    #[test]
    fn ignored_events_draw_nothing_and_the_clock_ticks_through_a_stream() {
        let start = Instant::now();
        let at = |ms: u64| start.checked_add(Duration::from_millis(ms)).unwrap();
        let mut pace = Pace::new(start);
        assert!(pace.take_draw(), "the first frame");
        assert!(!pace.take_draw());
        assert_eq!(pace.after(at(10), None), Vec::new());
        assert!(!pace.take_draw(), "a mouse move is not worth a frame");
        let click = Input::Mouse { x: 1, y: 1, kind: Mouse::Click };
        assert_eq!(pace.after(at(20), Some(click)), vec![click]);
        assert!(pace.take_draw());
        assert_eq!(pace.wait(at(100)), Duration::from_millis(150));
        // Events every 50 ms: the tick still comes on time.
        let ticks = (1..=10_u64)
            .flat_map(|i| pace.after(at(i * 50), None))
            .filter(|i| *i == Input::Tick)
            .count();
        assert_eq!(ticks, 2, "at 250 and 500 ms");
        assert!(pace.take_draw(), "a tick moves the preview");
        assert_eq!(pace.wait(at(10_000)), Duration::ZERO);
    }
}
