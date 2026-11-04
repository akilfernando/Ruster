//! CPU-authored schematic geometry. All rectangles are emitted into a Vulkan render pass.
use ruster_core::{Config, State};

#[derive(Clone, Copy)]
pub struct Rect {
    pub bounds: [f32; 4],
    pub color: [f32; 4],
}

#[derive(Clone, Copy)]
pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

const INK: [f32; 4] = [0.80, 0.88, 0.96, 1.0];
const MUTED: [f32; 4] = [0.37, 0.51, 0.65, 1.0];
const BLUE: [f32; 4] = [0.17, 0.57, 0.91, 1.0];
const GREEN: [f32; 4] = [0.21, 0.78, 0.57, 1.0];
const AMBER: [f32; 4] = [0.99, 0.62, 0.23, 1.0];

fn rect(out: &mut Vec<Rect>, bounds: [f32; 4], color: [f32; 4]) {
    out.push(Rect { bounds, color });
}

fn text(out: &mut Vec<Rect>, x: f32, y: f32, size: f32, text: &str, color: [f32; 4]) {
    for (index, character) in text.chars().enumerate() {
        for (row, bits) in glyph(character).iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) != 0 {
                    rect(
                        out,
                        [
                            x + (index * 6 + column) as f32 * size,
                            y + row as f32 * size,
                            size,
                            size,
                        ],
                        color,
                    );
                }
            }
        }
    }
}

pub fn scene(state: State, config: Config, playing: bool) -> Vec<Rect> {
    let mut out = Vec::with_capacity(3000);
    text(&mut out, -445.0, -265.0, 4.0, "RUSTER / LOGISTICS", INK);
    text(
        &mut out,
        -445.0,
        -218.0,
        2.0,
        "AUTHORITATIVE INTEGER SIMULATION / VULKAN VIEW",
        MUTED,
    );
    for (x, label, color) in [
        (-440.0, "SOURCE", BLUE),
        (-95.0, "TRANSPORT", AMBER),
        (220.0, "DESTINATION", GREEN),
    ] {
        rect(
            &mut out,
            [x, -105.0, 220.0, 230.0],
            [0.055, 0.09, 0.14, 1.0],
        );
        rect(&mut out, [x, -105.0, 220.0, 5.0], color);
        text(&mut out, x + 15.0, -80.0, 2.0, label, color);
    }
    rect(&mut out, [-220.0, 0.0, 125.0, 5.0], MUTED);
    rect(&mut out, [125.0, 0.0, 95.0, 5.0], MUTED);
    // A bounded moving marker communicates the discrete transport phase, not extra simulation state.
    let packet = (state.tick % 5) as f32;
    rect(&mut out, [-205.0 + packet * 18.0, -7.0, 14.0, 19.0], AMBER);
    text(&mut out, -425.0, -25.0, 2.0, "STOCK", MUTED);
    text(&mut out, -425.0, 0.0, 2.0, &state.stock.to_string(), INK);
    text(&mut out, -80.0, -25.0, 2.0, "PER TICK", MUTED);
    text(
        &mut out,
        -80.0,
        0.0,
        2.0,
        &config.transfer_per_tick.to_string(),
        INK,
    );
    text(&mut out, 235.0, -25.0, 2.0, "DELIVERED", MUTED);
    text(&mut out, 235.0, 0.0, 2.0, &state.delivered.to_string(), INK);
    // Logarithmic bars stay legible across the full u64 range. Exact counters remain visible.
    for (x, value, color) in [(-425.0, state.stock, BLUE), (235.0, state.delivered, GREEN)] {
        rect(&mut out, [x, 73.0, 190.0, 22.0], [0.10, 0.16, 0.23, 1.0]);
        let width = ((value as f64 + 1.0).log2() / 12.0).min(1.0) as f32 * 190.0;
        if width > 0.0 {
            rect(&mut out, [x, 73.0, width, 22.0], color);
        }
    }
    text(
        &mut out,
        -440.0,
        155.0,
        2.0,
        &format!(
            "TICK {}  PRODUCED {}  RATE {}",
            state.tick, state.produced, config.production_per_tick
        ),
        INK,
    );
    text(
        &mut out,
        -440.0,
        190.0,
        2.0,
        if playing { "PLAYING" } else { "PAUSED" },
        if playing { GREEN } else { AMBER },
    );
    text(
        &mut out,
        -440.0,
        235.0,
        1.5,
        "SPACE PLAY/PAUSE  N STEP  R RESET  HOME CAMERA",
        MUTED,
    );
    text(
        &mut out,
        -440.0,
        258.0,
        1.5,
        "ARROWS PAN  WHEEL ZOOM  ESC QUIT",
        MUTED,
    );
    out
}

// Original compact 5x7 bitmap glyphs; no font asset or external font license dependency.
fn glyph(c: char) -> [u8; 7] {
    match c {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 14],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [31, 4, 4, 4, 4, 4, 31],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        _ => [0; 7],
    }
}
