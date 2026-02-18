use anyhow::Result;
use chrono::{DateTime, Duration, Local, Timelike};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    crossterm,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        canvas::{Canvas, Circle, Line as CanvasLine, Points},
        Block, Borders, Paragraph,
    },
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io,
    time::{self, Instant},
};
use tui_textarea::TextArea;

// --- Data Structures ---

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TimeNote {
    content: String,
    is_locked: bool,
}

const SAVE_FILE: &str = "chronos_notes.json";

fn load_notes() -> HashMap<String, TimeNote> {
    if let Ok(data) = fs::read_to_string(SAVE_FILE) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        HashMap::new()
    }
}

fn save_notes(notes: &HashMap<String, TimeNote>) {
    if let Ok(data) = serde_json::to_string_pretty(notes) {
        let _ = fs::write(SAVE_FILE, data);
    }
}

// --- App State ---

struct App<'a> {
    should_quit: bool,
    // Time State
    real_time_last_tick: DateTime<Local>,
    virtual_time: DateTime<Local>,
    time_multiplier: f64,
    // Data State
    notes: HashMap<String, TimeNote>,
    selected_minute: Option<u32>, // 0-59 for minute positions 
    // UI State
    textarea: TextArea<'a>,
    is_editing: bool,
    // Visual Effects State
    emanations: Vec<Emanation>,
}

struct Emanation {
    phase_offset: f64,
}

impl<'a> App<'a> {
    fn new() -> Self {
        let now = Local::now();
        let notes = load_notes();
        
        // Initialize simple TextArea
        let mut textarea = TextArea::default();
        textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .title("Temporal Observation Node")
                .style(Style::default().fg(Color::Rgb(212, 175, 55))), // Gold
        );

        Self {
            should_quit: false,
            real_time_last_tick: now,
            virtual_time: now,
            time_multiplier: 1.0,
            notes,
            selected_minute: None,
            textarea,
            is_editing: false,
            emanations: vec![
                Emanation { phase_offset: 0.0 },
                Emanation { phase_offset: 4.33 }, // 13 / 3
                Emanation { phase_offset: 8.66 }, 
            ],
        }
    }

    fn on_tick(&mut self) {
        let now = Local::now();
        let delta = now.signed_duration_since(self.real_time_last_tick);
        let delta_micros = delta.num_microseconds().unwrap_or(0);
        
        // Apply time dilation
        let virtual_delta = Duration::microseconds((delta_micros as f64 * self.time_multiplier) as i64);
        self.virtual_time = self.virtual_time + virtual_delta;
        self.real_time_last_tick = now;
    }
    
    fn get_breathing_scale(&self, phase_offset: f64) -> f64 {
         let total_secs = self.virtual_time.timestamp() as f64 + self.virtual_time.nanosecond() as f64 / 1_000_000_000.0;
         let t = (total_secs + phase_offset) % 13.0; // 4 + 1 + 8
         
         if t < 4.0 {
             t / 4.0 // Inhale (expand)
         } else if t < 5.0 {
             1.0 // Hold
         } else {
             1.0 - (t - 5.0) / 8.0 // Exhale (contract)
         }
    }

    fn draw_shader_layer(&self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        // "Software Shader" implementation
        // We iterate over every cell in the area and calculate its color based on distance fields.
        
        let center_x = area.left() as f64 + area.width as f64 / 2.0;
        let center_y = area.top() as f64 + area.height as f64 / 2.0;
        
        // Precompute time-based values
        let t = self.virtual_time;
        let sub_second = t.nanosecond() as f64 / 1_000_000_000.0;
        let second_val = t.second() as f64 + sub_second;
        let minute_val = t.minute() as f64 + second_val / 60.0;
        
        // Spirit Dot Position (Minute Hand Tip)
        let minute_angle = (90.0 - minute_val * 6.0).to_radians();

        let _scale_x = 1.0;
        let _scale_y = 0.45; // Refined Y scale
        
        let spirit_r = 95.0; 
        let clock_radius_screen_y = (area.height as f64 * 0.45).min(area.width as f64 * 0.22); 
        let clock_radius_screen_x = clock_radius_screen_y * 2.1; // Correct for cell aspect (2.1 taller)
        
        // Reverse Rotation for Lotus Ring
        let total_secs = t.timestamp() as f64 + t.nanosecond() as f64 / 1_000_000_000.0;
        let ring_rotation = -total_secs * 0.1; 
        
        let spirit_screen_x = center_x + spirit_r / 100.0 * clock_radius_screen_x * minute_angle.cos();
        let spirit_screen_y = center_y - spirit_r / 100.0 * clock_radius_screen_y * minute_angle.sin();
        
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let dx = x as f64 - center_x;
                let dy = (y as f64 - center_y) * 2.1; // Align with Canvas's implicit aspect
                let dist_sq = dx*dx + dy*dy;
                let dist = dist_sq.sqrt();
                let angle = dy.atan2(dx);
                
                // Color Accumulator
                let mut r = 0.0;
                let mut g = 0.0;
                let mut b = 0.0;
                
                // 1. Background Void
                let vign = (1.0 - dist / (area.width as f64)).max(0.0).powf(2.0);
                r += 10.0 * vign;
                g += 10.0 * vign;
                b += 15.0 * vign;

                // 2. Breathing Emanations
                for emanation in &self.emanations {
                    let scale = self.get_breathing_scale(emanation.phase_offset);
                    let screen_r = scale * clock_radius_screen_x * 1.5; // Expand beyond ring
                    let d_ring = (dist - screen_r).abs();
                    
                    let thickness = 4.0;
                    if d_ring < thickness {
                        let intensity = (1.0 - d_ring / thickness) * scale * 0.5;
                        // Gold Ripple
                        r += 212.0 * intensity;
                        g += 175.0 * intensity;
                        b += 55.0 * intensity;
                    }
                }
                
                // 3. Spirit Dot Glow (Radiant Golden)
                let dx_s = x as f64 - spirit_screen_x;
                let dy_s = (y as f64 - spirit_screen_y) * 2.0;
                let dist_s = (dx_s*dx_s + dy_s*dy_s).sqrt();
                
                let glow_radius = 12.0;
                if dist_s < glow_radius {
                     let glow = (1.0 - dist_s / glow_radius).powf(3.0);
                     r += 255.0 * glow;   // Gold R
                     g += 215.0 * glow; // Gold G
                     b += 0.0 * glow;   // Gold B
                }
                
                // 4. Aether Ring (Lotus Petals)
                // r = r_base + aura * |sin(k * alpha)|
                let num_petals = 8.0;
                let local_angle = angle + ring_rotation;
                let petal_factor = (num_petals * local_angle).sin().abs();
                let ring_r_base = clock_radius_screen_x;
                let ring_r_target = ring_r_base + 6.0 * petal_factor;
                
                let ring_dist = (dist - ring_r_target).abs();
                if ring_dist < 2.5 {
                     // Breathing light intensity
                     let breathing_light = 0.7 + 0.3 * (total_secs * 0.5).sin().abs();
                     let ring_int = (1.0 - ring_dist / 2.5) * breathing_light;
                     r += 255.0 * ring_int;
                     g += 215.0 * ring_int;
                     b += 50.0 * ring_int;
                }
                
                let (fr, fg, fb) = (r.min(255.0) as u8, g.min(255.0) as u8, b.min(255.0) as u8);
                if fr > 15 || fg > 15 || fb > 15 {
                     let cell = &mut buf[(x, y)];
                     cell.set_bg(Color::Rgb(fr, fg, fb));
                }
            }
        }
    }

    fn get_date_key(&self, minute_offset: u32) -> String {
        // Simple key generation: YYYY-MM-DD-HH-mm
        // Logic: map the selected minute to the current hour context
        
        let t = self.virtual_time;
        format!("{}-{:02}-{:02}", t.format("%Y-%m-%d"), t.hour(), minute_offset)
    }
}

fn main() -> Result<()> {
    // Setup Terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create App
    let mut app = App::new();
    let tick_rate = time::Duration::from_millis(16); // ~60 FPS for smooth abstract visuals
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| time::Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if app.is_editing {
                        match key.code {
                            KeyCode::Esc => {
                                app.is_editing = false;
                                // Save note logic
                                if let Some(m) = app.selected_minute {
                                    let key = app.get_date_key(m);
                                    let content = app.textarea.lines().join("\n");
                                    app.notes.insert(key, TimeNote { content, is_locked: false });
                                    save_notes(&app.notes); // Persist immediately
                                }
                            }
                            _ => {
                                let ratatui_key = ratatui::crossterm::event::KeyEvent::from(key);
                                app.textarea.input(ratatui_key);
                            }
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Char('Q') => app.should_quit = true,
                            KeyCode::Char('+') => app.time_multiplier += 0.1,
                            KeyCode::Char('-') => app.time_multiplier = (app.time_multiplier - 0.1).max(0.0),
                            KeyCode::Right => {
                                let new_m = app.selected_minute.map(|m| (m + 1) % 60).unwrap_or(0);
                                app.selected_minute = Some(new_m);
                            }
                            KeyCode::Left => {
                                let new_m = app.selected_minute.map(|m| if m == 0 { 59 } else { m - 1 }).unwrap_or(0);
                                app.selected_minute = Some(new_m);
                            }
                            KeyCode::Up => {
                                let new_m = app.selected_minute.map(|m| (m + 5) % 60).unwrap_or(0);
                                app.selected_minute = Some(new_m);
                            }
                            KeyCode::Down => {
                                let new_m = app.selected_minute.map(|m| if m < 5 { m + 55 } else { m - 5 }).unwrap_or(0);
                                app.selected_minute = Some(new_m);
                            }
                            KeyCode::Enter => {
                                if let Some(m) = app.selected_minute {
                                    app.is_editing = true;
                                    // Load existing note if any
                                    let key = app.get_date_key(m);
                                    if let Some(note) = app.notes.get(&key) {
                                        app.textarea = TextArea::from(note.content.lines());
                                    } else {
                                        app.textarea = TextArea::default();
                                    }
                                    
                                    let title = format!("Temporal Observation Node: Minute {:02}", m);
                                    app.textarea.set_block(
                                        Block::default()
                                            .borders(Borders::ALL)
                                            .title(title)
                                            .style(Style::default().fg(Color::Rgb(212, 175, 55))),
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = Instant::now();
        }

        if app.should_quit {
            break;
        }
    }

    // Restore Terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3), // Title
                Constraint::Min(10),   // Main Canvas
                Constraint::Length(3), // Footer / Experience
            ]
            .as_ref(),
        )
        .split(f.area());

    // --- Header ---
    let title_style = Style::default().fg(Color::Rgb(212, 175, 55)).add_modifier(Modifier::BOLD);
    let title = Paragraph::new("* CHRONOS PLANTACERIUM *\nAETERNUM PRECISION ARCHIVE")
        .style(title_style)
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default().borders(Borders::NONE)); // Clean look
    f.render_widget(title, chunks[0]);

    // --- Main Clock Canvas ---
    let canvas_area = chunks[1];
    
    // 1. Draw "Software Shader" Background Layer
    app.draw_shader_layer(canvas_area, f.buffer_mut());
    
    // 2. Draw Vector Overlays via Canvas
    // The canvas coordinate system will be x: [-200, 200], y: [-200, 200] roughly.
    // X is horizontal, Y is vertical (Ratatui Y grows downwards usually, but Canvas might be Cartesian).
    // Ratatui Canvas: Y grows UPWARDS (Cartesian).
    
    let canvas = Canvas::default()
        .block(Block::default().borders(Borders::NONE)) // No border, just void
        .x_bounds([-120.0, 120.0])
        .y_bounds([-120.0, 120.0])
        .marker(ratatui::symbols::Marker::Dot)
        .paint(|ctx| {
            // Colors
            let gold = Color::Rgb(212, 175, 55);
            let gold_dim = Color::Rgb(100, 80, 20);
            let active_hand = Color::Rgb(252, 246, 186); // Light Gold
            
            // 1. Breathing Emanations (Ripples)
            for emanation in &app.emanations {
                let scale = app.get_breathing_scale(emanation.phase_offset);
                let radius = scale * 100.0 * 1.5;
                ctx.draw(&Circle {
                    x: 0.0,
                    y: 0.0,
                    radius,
                    color: gold_dim,
                });
            }

            // 2. Precision Minute Indicators (60 ticks)
            for i in 0..60 {
                let angle_deg = 90.0 - (i as f64 * 6.0);
                let rad = angle_deg.to_radians();
                let r_inner = 98.0;
                let r_outer = 100.0;
                
                let is_selected = app.selected_minute == Some(i as u32);
                let is_hour = i % 5 == 0;
                let color = if is_selected { Color::White } else if is_hour { gold } else { gold_dim };
                
                let x = r_outer * rad.cos();
                let y = r_outer * rad.sin();

                ctx.draw(&CanvasLine {
                    x1: r_inner * rad.cos(),
                    y1: r_inner * rad.sin(),
                    x2: x,
                    y2: y,
                    color,
                });

                if is_selected {
                    ctx.draw(&Circle {
                        x, y, radius: 4.0, color: Color::White
                    });
                }
            }

            // 3. Decorative Lotus Petals (12 petals)
            let total_secs = app.virtual_time.timestamp() as f64 + app.virtual_time.nanosecond() as f64 / 1_000_000_000.0;
            let ring_rotation = -total_secs * 0.1;
            let breathing_light = 0.7 + 0.3 * (total_secs * 0.5).sin().abs();
            let petal_color = Color::Rgb(
                (212.0 * breathing_light) as u8,
                (175.0 * breathing_light) as u8,
                (55.0 * breathing_light) as u8,
            );

            for i in 0..12 {
                let angle_deg = 90.0 - (i as f64 * 30.0) + (ring_rotation.to_degrees());
                let rad = angle_deg.to_radians();
                let side_offset = 6.0_f64.to_radians();

                let r_base = 102.0;
                let r_apex = 118.0;

                let x_apex = r_apex * rad.cos();
                let y_apex = r_apex * rad.sin();
                let x_l = r_base * (rad - side_offset).cos();
                let y_l = r_base * (rad - side_offset).sin();
                let x_r = r_base * (rad + side_offset).cos();
                let y_r = r_base * (rad + side_offset).sin();

                ctx.draw(&CanvasLine { x1: x_l, y1: y_l, x2: x_apex, y2: y_apex, color: petal_color });
                ctx.draw(&CanvasLine { x1: x_r, y1: y_r, x2: x_apex, y2: y_apex, color: petal_color });
            }

            // 4. Hour Markers (Selected minute takes precedence for selection highlight)
            for i in 0..12 {
                let is_quadrant = i % 3 == 0;
                let draw_angle = 90.0 - (i as f64 * 30.0);
                let rad = draw_angle.to_radians();
                
                let r_marker = 90.0;
                let x = r_marker * rad.cos();
                let y = r_marker * rad.sin();
                
                // Base Marker Color
                let color = if is_quadrant {
                    gold
                } else {
                    gold_dim
                };
                
                // Draw Markers
                if is_quadrant {
                    // Larger Cross for quadrants
                    ctx.draw(&CanvasLine {
                        x1: x - 2.0, y1: y, x2: x + 2.0, y2: y, color
                    });
                    ctx.draw(&CanvasLine {
                        x1: x, y1: y - 2.0, x2: x, y2: y + 2.0, color
                    });
                } else {
                    ctx.draw(&Points {
                        coords: &[(x, y)],
                        color,
                    });
                }
            }

            // 4. Time Calculation
            let t = app.virtual_time;
            let sub_second = t.nanosecond() as f64 / 1_000_000_000.0;
            let second_val = t.second() as f64 + sub_second;
            let minute_val = t.minute() as f64 + second_val / 60.0;
            let hour_val = (t.hour() % 12) as f64 + minute_val / 60.0;

            // 5. Hands
            // Second Hand (Thin)
            {
                let angle_deg = 90.0 - (second_val * 6.0);
                let rad = angle_deg.to_radians();
                ctx.draw(&CanvasLine {
                    x1: 0.0, y1: 0.0,
                    x2: 95.0 * rad.cos(),
                    y2: 95.0 * rad.sin(),
                    color: Color::Rgb(180, 50, 50), 
                });
            }
            // Minute Hand (Bold)
            {
                let angle_deg = 90.0 - (minute_val * 6.0);
                let rad = angle_deg.to_radians();
                // Draw a double line or offset lines for "boldness"
                ctx.draw(&CanvasLine {
                    x1: 0.0, y1: 0.0, x2: 85.0 * rad.cos(), y2: 85.0 * rad.sin(),
                    color: active_hand,
                });
            }
            // Hour Hand (Short & Thick)
            {
                let angle_deg = 90.0 - (hour_val * 30.0);
                let rad = angle_deg.to_radians();
                ctx.draw(&CanvasLine {
                    x1: 0.0, y1: 0.0, x2: 60.0 * rad.cos(), y2: 60.0 * rad.sin(),
                    color: gold,
                });
            }
            
            // 6. Center Hub
            ctx.draw(&Circle {
                x: 0.0, y: 0.0, radius: 3.0, color: gold
            });
            ctx.draw(&Circle {
                x: 0.0, y: 0.0, radius: 1.0, color: Color::White
            });

        });
    f.render_widget(canvas, canvas_area);

    // --- Footer: Experience ---
    let experience_seconds = app.virtual_time.num_seconds_from_midnight();
    let stats_text = vec![
        Line::from(vec![
            Span::raw("SPEED: "),
            Span::styled(format!("{:.1}x", app.time_multiplier), Style::default().fg(if app.time_multiplier > 1.0 { Color::Red } else { Color::Green })),
            Span::raw(" | "),
            Span::styled("EXPERIENCE UNITS: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}", experience_seconds), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::raw("CONTROLS: Arrow Keys (Select/Jump) | Enter (Edit) | +/- (Time) | Q (Quit)"),
        ]),
    ];
    let footer = Paragraph::new(stats_text)
        .alignment(ratatui::layout::Alignment::Left)
        .block(Block::default().borders(Borders::TOP));
    f.render_widget(footer, chunks[2]);

    // --- Modal: Note Editor ---
    if app.is_editing {
        let area = centered_rect(70, 60, f.area());
        f.render_widget(ratatui::widgets::Clear, area);
        
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(" TEMPORAL OBSERVATION VAULT ", Style::default().fg(Color::Rgb(212, 175, 55)).add_modifier(Modifier::BOLD)))
            .title_bottom(Line::from(" [ESC] TO LOCK NODE (SAVE INTERFACE) ").alignment(ratatui::layout::Alignment::Right));
            
        app.textarea.set_block(block);
        f.render_widget(&app.textarea, area);
    }
}

// Helper for centering the modal
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
