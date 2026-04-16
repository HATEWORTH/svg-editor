//! SVG path `d` attribute parser, builder, and node point extraction.
//! Converts all commands to absolute coordinates for uniform editing.

use egui::Pos2;

#[derive(Clone, Debug)]
pub enum PathCmd {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32), // x1 y1 x2 y2 x y
    Quad(f32, f32, f32, f32),            // x1 y1 x y
    Arc(f32, f32, f32, bool, bool, f32, f32), // rx ry rot large sweep x y
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointKind {
    Anchor,
    ControlIn,
    ControlOut,
}

#[derive(Clone, Debug)]
pub struct NodePoint {
    pub pos: Pos2,
    pub kind: PointKind,
    pub cmd_idx: usize,
    pub field: usize,
}

// ─── Parser ─────────────────────────────────────────

pub fn parse(d: &str) -> Vec<PathCmd> {
    let tokens = tokenize(d);
    let mut cmds = Vec::new();
    let mut cx: f32 = 0.0;
    let mut cy: f32 = 0.0;
    let mut sx: f32 = 0.0;
    let mut sy: f32 = 0.0;
    let mut i = 0;

    while i < tokens.len() {
        match tokens[i] {
            Token::Cmd(ch) => {
                let abs = ch.is_ascii_uppercase();
                let cmd = ch.to_ascii_uppercase();
                i += 1;

                match cmd {
                    'M' => {
                        let mut first = true;
                        while i + 1 < tokens.len() && tokens[i].is_num() {
                            let (x, y) = coords(abs, cx, cy, tokens[i].num(), tokens[i + 1].num());
                            if first {
                                cmds.push(PathCmd::Move(x, y));
                                sx = x;
                                sy = y;
                                first = false;
                            } else {
                                cmds.push(PathCmd::Line(x, y));
                            }
                            cx = x;
                            cy = y;
                            i += 2;
                        }
                    }
                    'L' => {
                        while i + 1 < tokens.len() && tokens[i].is_num() {
                            let (x, y) = coords(abs, cx, cy, tokens[i].num(), tokens[i + 1].num());
                            cmds.push(PathCmd::Line(x, y));
                            cx = x;
                            cy = y;
                            i += 2;
                        }
                    }
                    'H' => {
                        while i < tokens.len() && tokens[i].is_num() {
                            let x = if abs { tokens[i].num() } else { cx + tokens[i].num() };
                            cmds.push(PathCmd::Line(x, cy));
                            cx = x;
                            i += 1;
                        }
                    }
                    'V' => {
                        while i < tokens.len() && tokens[i].is_num() {
                            let y = if abs { tokens[i].num() } else { cy + tokens[i].num() };
                            cmds.push(PathCmd::Line(cx, y));
                            cy = y;
                            i += 1;
                        }
                    }
                    'C' => {
                        while i + 5 < tokens.len() && tokens[i].is_num() {
                            let (x1, y1) = coords(abs, cx, cy, tokens[i].num(), tokens[i + 1].num());
                            let (x2, y2) = coords(abs, cx, cy, tokens[i + 2].num(), tokens[i + 3].num());
                            let (x, y) = coords(abs, cx, cy, tokens[i + 4].num(), tokens[i + 5].num());
                            cmds.push(PathCmd::Cubic(x1, y1, x2, y2, x, y));
                            cx = x;
                            cy = y;
                            i += 6;
                        }
                    }
                    'S' => {
                        while i + 3 < tokens.len() && tokens[i].is_num() {
                            let (x1, y1) = match cmds.last() {
                                Some(PathCmd::Cubic(_, _, cx2, cy2, ex, ey)) => (2.0 * ex - cx2, 2.0 * ey - cy2),
                                _ => (cx, cy),
                            };
                            let (x2, y2) = coords(abs, cx, cy, tokens[i].num(), tokens[i + 1].num());
                            let (x, y) = coords(abs, cx, cy, tokens[i + 2].num(), tokens[i + 3].num());
                            cmds.push(PathCmd::Cubic(x1, y1, x2, y2, x, y));
                            cx = x;
                            cy = y;
                            i += 4;
                        }
                    }
                    'Q' => {
                        while i + 3 < tokens.len() && tokens[i].is_num() {
                            let (x1, y1) = coords(abs, cx, cy, tokens[i].num(), tokens[i + 1].num());
                            let (x, y) = coords(abs, cx, cy, tokens[i + 2].num(), tokens[i + 3].num());
                            cmds.push(PathCmd::Quad(x1, y1, x, y));
                            cx = x;
                            cy = y;
                            i += 4;
                        }
                    }
                    'T' => {
                        while i + 1 < tokens.len() && tokens[i].is_num() {
                            let (x1, y1) = match cmds.last() {
                                Some(PathCmd::Quad(qx, qy, _, _)) => (2.0 * cx - qx, 2.0 * cy - qy),
                                _ => (cx, cy),
                            };
                            let (x, y) = coords(abs, cx, cy, tokens[i].num(), tokens[i + 1].num());
                            cmds.push(PathCmd::Quad(x1, y1, x, y));
                            cx = x;
                            cy = y;
                            i += 2;
                        }
                    }
                    'A' => {
                        while i + 6 < tokens.len() && tokens[i].is_num() {
                            let rx = tokens[i].num();
                            let ry = tokens[i + 1].num();
                            let rot = tokens[i + 2].num();
                            let large = tokens[i + 3].num() != 0.0;
                            let sweep = tokens[i + 4].num() != 0.0;
                            let (x, y) = coords(abs, cx, cy, tokens[i + 5].num(), tokens[i + 6].num());
                            cmds.push(PathCmd::Arc(rx, ry, rot, large, sweep, x, y));
                            cx = x;
                            cy = y;
                            i += 7;
                        }
                    }
                    'Z' => {
                        cmds.push(PathCmd::Close);
                        cx = sx;
                        cy = sy;
                    }
                    _ => {}
                }
            }
            Token::Num(_) => {
                i += 1;
            }
        }
    }

    cmds
}

fn coords(abs: bool, cx: f32, cy: f32, x: f32, y: f32) -> (f32, f32) {
    if abs { (x, y) } else { (cx + x, cy + y) }
}

// ─── Builder ────────────────────────────────────────

pub fn build(cmds: &[PathCmd]) -> String {
    let mut s = String::new();
    for cmd in cmds {
        if !s.is_empty() {
            s.push(' ');
        }
        match cmd {
            PathCmd::Move(x, y) => write_cmd(&mut s, 'M', &[*x, *y]),
            PathCmd::Line(x, y) => write_cmd(&mut s, 'L', &[*x, *y]),
            PathCmd::Cubic(x1, y1, x2, y2, x, y) => {
                write_cmd(&mut s, 'C', &[*x1, *y1, *x2, *y2, *x, *y])
            }
            PathCmd::Quad(x1, y1, x, y) => write_cmd(&mut s, 'Q', &[*x1, *y1, *x, *y]),
            PathCmd::Arc(rx, ry, rot, large, sweep, x, y) => {
                s.push('A');
                s.push_str(&format!(
                    "{},{} {} {} {} {},{}",
                    fmt(*rx),
                    fmt(*ry),
                    fmt(*rot),
                    *large as u8,
                    *sweep as u8,
                    fmt(*x),
                    fmt(*y)
                ));
            }
            PathCmd::Close => s.push('Z'),
        }
    }
    s
}

fn write_cmd(s: &mut String, cmd: char, vals: &[f32]) {
    s.push(cmd);
    for (i, v) in vals.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&fmt(*v));
    }
}

fn fmt(v: f32) -> String {
    if (v - v.round()).abs() < 0.005 {
        format!("{}", v.round() as i32)
    } else {
        format!("{:.2}", v)
    }
}

// ─── Point extraction ───────────────────────────────

pub fn extract_points(cmds: &[PathCmd]) -> Vec<NodePoint> {
    let mut points = Vec::new();

    for (i, cmd) in cmds.iter().enumerate() {
        match cmd {
            PathCmd::Move(x, y) | PathCmd::Line(x, y) => {
                points.push(NodePoint {
                    pos: Pos2::new(*x, *y),
                    kind: PointKind::Anchor,
                    cmd_idx: i,
                    field: 0,
                });
            }
            PathCmd::Cubic(x1, y1, x2, y2, x, y) => {
                points.push(NodePoint {
                    pos: Pos2::new(*x1, *y1),
                    kind: PointKind::ControlOut,
                    cmd_idx: i,
                    field: 0,
                });
                points.push(NodePoint {
                    pos: Pos2::new(*x2, *y2),
                    kind: PointKind::ControlIn,
                    cmd_idx: i,
                    field: 1,
                });
                points.push(NodePoint {
                    pos: Pos2::new(*x, *y),
                    kind: PointKind::Anchor,
                    cmd_idx: i,
                    field: 2,
                });
            }
            PathCmd::Quad(x1, y1, x, y) => {
                points.push(NodePoint {
                    pos: Pos2::new(*x1, *y1),
                    kind: PointKind::ControlOut,
                    cmd_idx: i,
                    field: 0,
                });
                points.push(NodePoint {
                    pos: Pos2::new(*x, *y),
                    kind: PointKind::Anchor,
                    cmd_idx: i,
                    field: 1,
                });
            }
            PathCmd::Arc(_, _, _, _, _, x, y) => {
                points.push(NodePoint {
                    pos: Pos2::new(*x, *y),
                    kind: PointKind::Anchor,
                    cmd_idx: i,
                    field: 0,
                });
            }
            PathCmd::Close => {}
        }
    }

    points
}

/// Get the endpoint of a command (where the pen ends up).
pub fn cmd_endpoint(cmd: &PathCmd) -> Option<Pos2> {
    match cmd {
        PathCmd::Move(x, y) | PathCmd::Line(x, y) => Some(Pos2::new(*x, *y)),
        PathCmd::Cubic(_, _, _, _, x, y) => Some(Pos2::new(*x, *y)),
        PathCmd::Quad(_, _, x, y) => Some(Pos2::new(*x, *y)),
        PathCmd::Arc(_, _, _, _, _, x, y) => Some(Pos2::new(*x, *y)),
        PathCmd::Close => None,
    }
}

pub fn update_point(cmds: &mut [PathCmd], cmd_idx: usize, field: usize, nx: f32, ny: f32) {
    if cmd_idx >= cmds.len() {
        return;
    }
    match &mut cmds[cmd_idx] {
        PathCmd::Move(x, y) | PathCmd::Line(x, y) if field == 0 => {
            *x = nx;
            *y = ny;
        }
        PathCmd::Cubic(x1, y1, x2, y2, x, y) => match field {
            0 => {
                *x1 = nx;
                *y1 = ny;
            }
            1 => {
                *x2 = nx;
                *y2 = ny;
            }
            2 => {
                *x = nx;
                *y = ny;
            }
            _ => {}
        },
        PathCmd::Quad(x1, y1, x, y) => match field {
            0 => {
                *x1 = nx;
                *y1 = ny;
            }
            1 => {
                *x = nx;
                *y = ny;
            }
            _ => {}
        },
        PathCmd::Arc(_, _, _, _, _, x, y) if field == 0 => {
            *x = nx;
            *y = ny;
        }
        _ => {}
    }
}

// ─── Tokenizer ──────────────────────────────────────

#[derive(Debug)]
enum Token {
    Cmd(char),
    Num(f32),
}

impl Token {
    fn is_num(&self) -> bool {
        matches!(self, Token::Num(_))
    }
    fn num(&self) -> f32 {
        if let Token::Num(n) = self {
            *n
        } else {
            0.0
        }
    }
}

fn tokenize(d: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let bytes = d.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        let c = bytes[i];

        if c.is_ascii_alphabetic() && c != b'e' && c != b'E' {
            tokens.push(Token::Cmd(c as char));
            i += 1;
        } else if c == b'-' || c == b'+' || c == b'.' || c.is_ascii_digit() {
            let start = i;
            if c == b'-' || c == b'+' {
                i += 1;
            }
            while i < len && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < len && bytes[i] == b'.' {
                i += 1;
                while i < len && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < len && (bytes[i] == b'e' || bytes[i] == b'E') {
                i += 1;
                if i < len && (bytes[i] == b'-' || bytes[i] == b'+') {
                    i += 1;
                }
                while i < len && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if let Ok(n) = d[start..i].parse::<f32>() {
                tokens.push(Token::Num(n));
            }
        } else {
            i += 1;
        }
    }

    tokens
}
