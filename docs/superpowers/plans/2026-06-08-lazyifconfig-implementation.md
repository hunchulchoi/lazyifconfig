# lazyifconfig Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the remaining components of the `lazyifconfig` macOS TUI application, specifically command execution, UI rendering, selection state helpers, and terminal lifecycle loop.

**Architecture:** 
- The command layer executes macOS `ifconfig` synchronously via `std::process::Command`.
- The App state is extended with interface navigation helpers.
- The UI layer uses `ratatui` layouts and widgets to construct a split interface view, detail view, event stream, and status bar.
- The main entry point manages terminal raw mode, alternates screens, and drives a Tokio-based event loop.

**Tech Stack:** Rust, Ratatui, Crossterm, Tokio.

---

## User Review Required

> [!IMPORTANT]
> The command execution layer runs the system `ifconfig` command directly. This requires the application to be executed on a macOS system where `ifconfig` exists and is accessible on the system PATH.

---

## Open Questions

- No major open questions. The MVP scope is clearly defined in the [design specification](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/docs/superpowers/specs/2026-06-08-lazyifconfig-design.md).

---

## Proposed Changes

### Command Layer

#### [MODIFY] [command.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/command.rs)
- Implement `run_ifconfig` using `std::process::Command` to invoke `/sbin/ifconfig` (or `ifconfig` on PATH) and collect output.

---

### App State Layer

#### [MODIFY] [app.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/app.rs)
- Implement selection navigation helper methods: `select_next()` and `select_previous()`.

---

### UI Layer

#### [MODIFY] [ui.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/ui.rs)
- Implement the `draw` function using `ratatui` layout constraints and widgets.

---

### Main Entry Point

#### [MODIFY] [main.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/main.rs)
- Initialize/restore terminal raw mode, switch to alternate screen, handle key events (`q`, `r`, `j`/`k`/arrows), and run the tokio interval tick update loop.

---

## Tasks

### Task 1: Command Layer implementation

**Files:**
- Modify: `src/command.rs`

- [ ] **Step 1: Write the failing test**
  
  Add the unit test block at the end of [src/command.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/command.rs):
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn test_run_ifconfig_success() {
          let result = run_ifconfig();
          assert!(result.is_ok());
          let output = result.unwrap();
          assert!(output.contains("lo0") || output.contains("en0"));
      }
  }
  ```

- [ ] **Step 2: Run test to verify it fails**
  
  Run: `cargo test --lib command::tests::test_run_ifconfig_success`
  Expected: Failure due to returning "not implemented" error.

- [ ] **Step 3: Write minimal implementation**
  
  Modify `run_ifconfig` in [src/command.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/command.rs) to:
  ```rust
  pub fn run_ifconfig() -> Result<String, String> {
      use std::process::Command;
      let output = Command::new("ifconfig")
          .output()
          .map_err(|e| e.to_string())?;

      if output.status.success() {
          String::from_utf8(output.stdout).map_err(|e| e.to_string())
      } else {
          Err(String::from_utf8_lossy(&output.stderr).to_string())
      }
  }
  ```

- [ ] **Step 4: Run test to verify it passes**
  
  Run: `cargo test --lib command::tests::test_run_ifconfig_success`
  Expected: PASS

- [ ] **Step 5: Commit**
  
  Run:
  ```bash
  git add src/command.rs
  git commit -m "feat: implement run_ifconfig using std::process::Command"
  ```

---

### Task 2: App Selection Navigation Helpers

**Files:**
- Modify: `src/app.rs`
- Modify: `tests/app_state.rs`

- [ ] **Step 1: Write the failing test**
  
  Add the following test case at the end of [tests/app_state.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/tests/app_state.rs):
  ```rust
  #[test]
  fn test_app_navigation() {
      let mut app = App::default();
      app.replace_snapshot(NetworkSnapshot {
          interfaces: vec![
              interface_with_stats("lo0", None, None),
              interface_with_stats("en0", None, None),
              interface_with_stats("utun0", None, None),
          ],
          captured_at_secs: 10,
      });
      assert_eq!(app.selected_index, 0);

      app.select_next();
      assert_eq!(app.selected_index, 1);

      app.select_next();
      assert_eq!(app.selected_index, 2);

      app.select_next();
      assert_eq!(app.selected_index, 0);

      app.select_previous();
      assert_eq!(app.selected_index, 2);
  }
  ```

- [ ] **Step 2: Run test to verify it fails**
  
  Run: `cargo test --test app_state test_app_navigation`
  Expected: Compile error due to missing `select_next` and `select_previous` methods.

- [ ] **Step 3: Write minimal implementation**
  
  Add these methods to the `impl App` block in [src/app.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/app.rs):
  ```rust
  impl App {
      pub fn select_next(&mut self) {
          if let Some(snapshot) = &self.current_snapshot {
              let len = snapshot.interfaces.len();
              if len > 0 {
                  self.selected_index = (self.selected_index + 1) % len;
              }
          }
      }

      pub fn select_previous(&mut self) {
          if let Some(snapshot) = &self.current_snapshot {
              let len = snapshot.interfaces.len();
              if len > 0 {
                  if self.selected_index == 0 {
                      self.selected_index = len - 1;
                  } else {
                      self.selected_index -= 1;
                  }
              }
          }
      }
  }
  ```

- [ ] **Step 4: Run test to verify it passes**
  
  Run: `cargo test --test app_state test_app_navigation`
  Expected: PASS

- [ ] **Step 5: Commit**
  
  Run:
  ```bash
  git add src/app.rs tests/app_state.rs
  git commit -m "feat: implement selection navigation in App"
  ```

---

### Task 3: UI layout and widget rendering

**Files:**
- Modify: `src/ui.rs`

- [ ] **Step 1: Write the failing test**
  
  Add the test module at the end of [src/ui.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/ui.rs):
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use ratatui::{backend::TestBackend, Terminal};

      #[test]
      fn test_ui_draw_no_panic() {
          let app = App::default();
          let backend = TestBackend::new(80, 24);
          let mut terminal = Terminal::new(backend).unwrap();
          terminal.draw(|f| draw(f, &app)).unwrap();
          let buffer = terminal.backend().buffer();
          let mut has_title = false;
          for cell in buffer.content() {
              if cell.symbol() == "│" || cell.symbol() == "─" {
                  has_title = true;
                  break;
              }
          }
          assert!(has_title);
      }
  }
  ```

- [ ] **Step 2: Run test to verify it fails**
  
  Run: `cargo test --lib ui::tests::test_ui_draw_no_panic`
  Expected: Compile error because `draw` function is not defined.

- [ ] **Step 3: Write minimal implementation**
  
  Implement the `draw` function in [src/ui.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/ui.rs):
  ```rust
  use ratatui::{
      layout::{Constraint, Direction, Layout},
      style::{Color, Modifier, Style},
      widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
      Frame,
  };
  use crate::app::App;
  use crate::model::InterfaceStatus;

  pub fn render_title() -> &'static str {
      "lazyifconfig"
  }

  pub fn draw(frame: &mut Frame, app: &App) {
      let chunks = Layout::default()
          .direction(Direction::Vertical)
          .constraints([
              Constraint::Min(3),
              Constraint::Length(5),
              Constraint::Length(1),
          ])
          .split(frame.area());

      let top_chunks = Layout::default()
          .direction(Direction::Horizontal)
          .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
          .split(chunks[0]);

      // 1. Left Pane: Interfaces list
      let interfaces_block = Block::default()
          .borders(Borders::ALL)
          .title(" Interfaces ");
      
      let mut list_items = Vec::new();
      if let Some(snapshot) = &app.current_snapshot {
          for (idx, interface) in snapshot.interfaces.iter().enumerate() {
              let status_indicator = match interface.status {
                  InterfaceStatus::Up => "●",
                  InterfaceStatus::Down => "○",
              };
              let ip = interface.ipv4.first().map(|addr| addr.value.as_str()).unwrap_or("no IP");
              let style = if idx == app.selected_index {
                  Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
              } else {
                  Style::default()
              };
              list_items.push(ListItem::new(format!("{} {} ({})", status_indicator, interface.name, ip)).style(style));
          }
      }
      let interfaces_list = List::new(list_items).block(interfaces_block);
      frame.render_widget(interfaces_list, top_chunks[0]);

      // 2. Right Pane: Interface Details
      let details_block = Block::default()
          .borders(Borders::ALL)
          .title(" Interface Details ");
      
      let mut details_text = String::new();
      if let Some(snapshot) = &app.current_snapshot {
          if let Some(interface) = snapshot.interfaces.get(app.selected_index) {
              details_text.push_str(&format!("Name: {}\n", interface.name));
              details_text.push_str(&format!("Type: {:?}\n", interface.interface_type));
              details_text.push_str(&format!("Status: {}\n", match interface.status {
                  InterfaceStatus::Up => "Active / Up",
                  InterfaceStatus::Down => "Inactive / Down",
              }));
              details_text.push_str(&format!("MAC Address: {}\n", interface.mac_address.as_deref().unwrap_or("N/A")));
              details_text.push_str(&format!("MTU: {}\n", interface.mtu.map(|m| m.to_string()).unwrap_or_else(|| "N/A".to_string())));
              
              details_text.push_str("\nIPv4 Addresses:\n");
              for addr in &interface.ipv4 {
                  details_text.push_str(&format!("  - {}\n", addr.value));
              }
              details_text.push_str("IPv6 Addresses:\n");
              for addr in &interface.ipv6 {
                  details_text.push_str(&format!("  - {}\n", addr.value));
              }

              details_text.push_str("\nTraffic Statistics:\n");
              if let Some(stats) = &interface.stats {
                  details_text.push_str(&format!("  RX Packets: {}\n", stats.rx_packets));
                  details_text.push_str(&format!("  TX Packets: {}\n", stats.tx_packets));
                  details_text.push_str(&format!("  RX Bytes:   {}\n", stats.rx_bytes));
                  details_text.push_str(&format!("  TX Bytes:   {}\n", stats.tx_bytes));
                  if let Some((rx_rate, tx_rate)) = app.selected_rates() {
                      details_text.push_str(&format!("  RX Rate:    {} B/s\n", rx_rate));
                      details_text.push_str(&format!("  TX Rate:    {} B/s\n", tx_rate));
                  } else {
                      details_text.push_str("  RX Rate:    0 B/s (calculating...)\n");
                      details_text.push_str("  TX Rate:    0 B/s (calculating...)\n");
                  }
              } else {
                  details_text.push_str("  No stats available\n");
              }
          } else {
              details_text.push_str("No interface selected\n");
          }
      } else {
          details_text.push_str("No data collected yet. Press 'r' to refresh.\n");
      }

      let details_p = Paragraph::new(details_text).block(details_block).wrap(Wrap { trim: true });
      frame.render_widget(details_p, top_chunks[1]);

      // 3. Event Panel
      let event_block = Block::default()
          .borders(Borders::ALL)
          .title(" Recent Events ");
      let mut event_items = Vec::new();
      for event in app.recent_events.iter().rev().take(10) {
          event_items.push(ListItem::new(format!("[{}] {}", event.captured_at_secs, event.message)));
      }
      let event_list = List::new(event_items).block(event_block);
      frame.render_widget(event_list, chunks[1]);

      // 4. Status Bar
      let status_p = Paragraph::new(" q: Quit | r: Manual Refresh | j/k or Arrow Keys: Navigation ")
          .style(Style::default().bg(Color::Blue).fg(Color::White));
      frame.render_widget(status_p, chunks[2]);
  }
  ```

- [ ] **Step 4: Run test to verify it passes**
  
  Run: `cargo test --lib ui::tests::test_ui_draw_no_panic`
  Expected: PASS

- [ ] **Step 5: Commit**
  
  Run:
  ```bash
  git add src/ui.rs
  git commit -m "feat: implement UI drawing and layout layout blocks"
  ```

---

### Task 4: Terminal lifecycle and main event loop

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Write the failing test**
  
  Add a unit test checking that `tick_update` performs correctly at the end of [src/main.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/main.rs):
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn test_tick_update() {
          let mut app = App::default();
          let res = tick_update(&mut app);
          assert!(res.is_ok());
          assert!(app.current_snapshot.is_some());
      }
  }
  ```

- [ ] **Step 2: Run test to verify it fails**
  
  Run: `cargo test --bin lazyifconfig tests::test_tick_update`
  Expected: Compile error due to `tick_update` not being defined.

- [ ] **Step 3: Write minimal implementation**
  
  Modify [src/main.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/main.rs) to:
  ```rust
  use std::io;
  use std::time::{Duration, SystemTime, UNIX_EPOCH};
  use crossterm::{
      event::{self, Event, KeyCode},
      execute,
      terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
  };
  use ratatui::{backend::CrosstermBackend, Terminal};
  use lazyifconfig::app::App;
  use lazyifconfig::command::run_ifconfig;
  use lazyifconfig::collector::interface::parse_interfaces;
  use lazyifconfig::collector::stats::merge_stats;
  use lazyifconfig::model::NetworkSnapshot;

  pub fn tick_update(app: &mut App) -> Result<(), String> {
      let raw_out = run_ifconfig()?;
      let parsed = parse_interfaces(&raw_out);
      let merged = merge_stats(&raw_out, parsed);
      
      let now = SystemTime::now()
          .duration_since(UNIX_EPOCH)
          .map(|d| d.as_secs())
          .unwrap_or(0);

      app.replace_snapshot(NetworkSnapshot {
          interfaces: merged,
          captured_at_secs: now,
      });
      Ok(())
  }

  #[tokio::main]
  async fn main() -> Result<(), Box<dyn std::error::Error>> {
      enable_raw_mode()?;
      let mut stdout = io::stdout();
      execute!(stdout, EnterAlternateScreen)?;
      let backend = CrosstermBackend::new(stdout);
      let mut terminal = Terminal::new(backend)?;

      let mut app = App::default();
      let _ = tick_update(&mut app);

      let mut last_tick = std::time::Instant::now();
      let tick_rate = Duration::from_secs(2);

      loop {
          terminal.draw(|f| lazyifconfig::ui::draw(f, &app))?;

          let timeout = tick_rate
              .checked_sub(last_tick.elapsed())
              .unwrap_or(Duration::from_secs(0));

          if event::poll(timeout)? {
              if let Event::Key(key) = event::read()? {
                  match key.code {
                      KeyCode::Char('q') => break,
                      KeyCode::Char('r') => {
                          let _ = tick_update(&mut app);
                          last_tick = std::time::Instant::now();
                      }
                      KeyCode::Char('j') | KeyCode::Down => {
                          app.select_next();
                      }
                      KeyCode::Char('k') | KeyCode::Up => {
                          app.select_previous();
                      }
                      _ => {}
                  }
              }
          }

          if last_tick.elapsed() >= tick_rate {
              let _ = tick_update(&mut app);
              last_tick = std::time::Instant::now();
          }
      }

      disable_raw_mode()?;
      execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
      terminal.show_cursor()?;

      Ok(())
  }
  ```

- [ ] **Step 4: Run test to verify it passes**
  
  Run: `cargo test --bin lazyifconfig tests::test_tick_update`
  Expected: PASS

- [ ] **Step 5: Commit**
  
  Run:
  ```bash
  git add src/main.rs
  git commit -m "feat: complete main TUI application event loop"
  ```

---

## Verification Plan

### Automated Tests
Run the entire suite of tests to verify both new and old components compile and pass:
`cargo test`

### Manual Verification
1. Run `cargo run` on a macOS host.
2. Confirm the interfaces list renders on the left pane.
3. Use Arrow Keys or `j`/`k` to navigate and confirm detail view updates.
4. Unplug/replug internet or toggle a VPN connection, and verify new interface events appear in the bottom pane.
5. Press `q` to safely exit raw mode back to your standard terminal session.
