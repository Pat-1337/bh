use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::{Color::Black, ResetColor, SetBackgroundColor},
    terminal::{
        self, BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate,
        EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use std::{
    env,
    fmt::Write as FmtWrite,
    io::{self, Write},
    time::{Duration, Instant},
};

const ELEV_DEG: f64 = 7.3;
const CELL_AR: f64 = 0.50;
const BASE_ZOOM: f64 = 20.0;
const SPHERE_R: f64 = 0.285;
const DISK_INNER: f64 = 0.375;
const DISK_OUTER: f64 = 1.16;
const ARC_HEIGHT_BASE: f64 = 0.28;

const RAMP: &[u8] = b"#S%?*+;:,.";
const RESET: &str = "\x1b[0m";
const BLACK_BG_RESET: &str = "\x1b[0m\x1b[48;2;0;0;0m";

#[derive(Clone, Copy)]
struct Opts {
    view_scale: f64,
    clean: bool,
    black: bool,
}

/// Restores the terminal on drop, so errors and panics don't leave it in raw mode.
struct TermGuard;

impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen, ResetColor);
        let _ = terminal::disable_raw_mode();
    }
}

fn hash(x: f64, y: f64) -> f64 {
    let n = (x * 127.1 + y * 311.7).sin() * 43758.5453;
    n - n.floor()
}

fn sm(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

fn vnoise(x: f64, y: f64) -> f64 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (sm(x - ix), sm(y - iy));
    let (a, b, c, d) = (
        hash(ix, iy),
        hash(ix + 1.0, iy),
        hash(ix, iy + 1.0),
        hash(ix + 1.0, iy + 1.0),
    );
    a * (1.0 - fx) * (1.0 - fy) + b * fx * (1.0 - fy) + c * (1.0 - fx) * fy + d * fx * fy
}

fn fbm(x: f64, y: f64, oct: u32) -> f64 {
    let (mut v, mut a, mut px, mut py) = (0.0, 0.5, x, y);
    for _ in 0..oct {
        v += a * vnoise(px, py);
        a *= 0.5;
        px *= 2.0;
        py *= 2.0;
    }
    v
}

fn disk_brightness(dx: f64, dz: f64, rd: f64, t: f64) -> f64 {
    let tr = ((rd - DISK_INNER) / (DISK_OUTER - DISK_INNER)).clamp(0.0, 1.0);
    let radial = (1.0 - tr).powf(0.55);
    let doppler = ((dz.atan2(dx) - t * 1.3).cos() * 0.28 + 0.72).clamp(0.20, 1.0);
    let turb = fbm(dx * 5.5 + t * 0.35, dz.abs() * 1.8 + t * 0.10, 3) * 0.28 + 0.72;
    (radial * doppler * turb).clamp(0.0, 1.0)
}

fn sample(nx: f64, ny: f64, t: f64, inv_sin_e: f64, opts: Opts) -> (f64, f64, bool) {
    let r2d = (nx * nx + ny * ny).sqrt();
    let dz = ny * inv_sin_e;
    let rd = (nx * nx + dz * dz).sqrt();
    let in_disk = (DISK_INNER..=DISK_OUTER).contains(&rd);

    if ny >= 0.0 && in_disk {
        let b = disk_brightness(nx, dz, rd, t);
        return (b, 0.55 + (1.0 - b) * 0.38, false);
    }
    if r2d < SPHERE_R {
        return (0.0, 0.0, true);
    }

    let edge = r2d - SPHERE_R;

    if edge < 0.085 {
        let g = (1.0 - edge / 0.085).powf(1.5) * 0.95;
        let amod = (ny.atan2(nx) * 2.0 + t * 0.5).cos().abs() * 0.11 + 0.89;
        let turb = fbm(nx * 12.0 + t * 0.8, ny * 12.0, 2) * 0.13 + 0.87;
        return (g * amod * turb, 0.22, false);
    }

    if ny < 0.0 && !in_disk && edge < ARC_HEIGHT_BASE {
        let t_arc = edge / ARC_HEIGHT_BASE;
        let side_fade = nx.abs().atan2(-ny).cos().powf(0.35).max(0.0);
        let rd_l = DISK_INNER + t_arc * (DISK_OUTER - DISK_INNER);
        let nx_c = nx.abs().min(rd_l * 0.9999) * nx.signum();
        let dz_l = (rd_l * rd_l - nx_c * nx_c).max(0.0).sqrt();
        let b = (disk_brightness(nx_c, dz_l, rd_l, t) * 2.5 * (1.0 - t_arc.powf(0.45)) * side_fade)
            .clamp(0.0, 1.0);
        if b > 0.012 {
            return (b, 0.55 + (1.0 - b) * 0.38, false);
        }
    }

    if ny < 0.0 && in_disk {
        let base_b = disk_brightness(nx, dz.abs(), rd, t);
        let lensing_boost = if edge < 0.14 {
            1.0 + (0.14 - edge) / 0.14 * 0.75
        } else {
            1.0
        };
        let b = (base_b * 1.48 * lensing_boost).min(1.15);
        if b > 0.018 {
            return (b, 0.35 + (1.0 - b.min(1.0)) * 0.55, false);
        }
    }

    let beyond = rd - DISK_OUTER;
    if beyond > 0.0 && beyond < 0.22 {
        let h = (1.0 - beyond / 0.22).powf(3.0) * 0.15;
        let n = fbm(nx * 3.0 + t * 0.15, ny * 3.0, 2) * 0.45 + 0.55;
        if h * n > 0.024 {
            return (h * n, 0.94, false);
        }
    }

    if !opts.clean {
        let bg_radius = 1.8 * opts.view_scale;
        let gx = (nx * 70.0 + 0.5).floor();
        let gy = (ny * 48.0 + 0.5).floor();
        let seed = hash(gx * 1.7 + 7.3, gy * 2.1 - 3.1);
        let boost = (1.0 - (r2d / bg_radius).min(1.0)).powi(2) * 0.022;
        let tw = hash(seed * 523.0 + (t * 0.75).floor(), gx - gy * 0.5);

        if seed > 0.957 - boost {
            let b = if tw > 0.50 { 0.60 } else { 0.22 };
            return (b, 0.50 + (r2d / bg_radius).min(1.0) * 0.46, false);
        } else if seed > 0.935 - boost {
            return (if tw > 0.63 { 0.18 } else { 0.07 }, 1.0, false);
        }
    }

    (0.0, 0.0, false)
}

fn to_rgb(b: f64, tint: f64) -> (u8, u8, u8) {
    let b = b.clamp(0.0, 1.0);
    (
        (255.0 * b.powf(0.36)).min(255.0) as u8,
        (255.0 * b.powf(0.36) * (1.0 - tint * 0.62)).min(255.0) as u8,
        (255.0 * b.powf(0.38) * (1.0 - tint * 0.93) * 0.16).min(255.0) as u8,
    )
}

fn ramp(b: f64) -> char {
    let i = ((1.0 - b.clamp(0.0, 1.0)) * (RAMP.len() - 1) as f64).round() as usize;
    RAMP[i.min(RAMP.len() - 1)] as char
}

fn render<W: Write>(out: &mut W, time: f64, w: u16, h: u16, opts: Opts) -> io::Result<()> {
    let (cols, rows) = (w as f64, h as f64);
    let scale_x = (DISK_OUTER * 2.0 * 0.92 * opts.view_scale) / cols;
    let scale_y = scale_x / CELL_AR;
    let (cx, cy) = (cols * 0.5, rows * 0.56);
    let inv_sin_e = 1.0 / ELEV_DEG.to_radians().sin();

    let bg_reset = if opts.black { BLACK_BG_RESET } else { RESET };

    let mut buf = String::with_capacity(w as usize * 55 * h as usize);
    buf.push_str(bg_reset);

    let mut last_fg: Option<(u8, u8, u8)> = None;

    for row in 0..h {
        let _ = write!(buf, "\x1b[{};1H", row + 1);
        for col in 0..w {
            let nx = (col as f64 - cx) * scale_x;
            let ny = (row as f64 - cy) * scale_y;

            let (b, tint, void) = sample(nx, ny, time, inv_sin_e, opts);

            if void || b < 0.014 {
                if last_fg.take().is_some() {
                    buf.push_str(bg_reset);
                }
                buf.push(' ');
            } else {
                let (r, g, bl) = to_rgb(b, tint);
                if last_fg != Some((r, g, bl)) {
                    let _ = write!(buf, "\x1b[38;2;{r};{g};{bl}m");
                    last_fg = Some((r, g, bl));
                }
                buf.push(ramp(b));
            }
        }
    }

    buf.push_str(RESET);
    out.write_all(buf.as_bytes())?;
    out.flush()
}

fn main() -> io::Result<()> {
    let mut zoom = BASE_ZOOM;
    let mut clean = false;
    let mut black = false;
    let mut once = false;

    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--clean" => clean = true,
            "--black" => black = true,
            "--once" => once = true,
            "-h" | "--help" => {
                println!("usage: bh [ZOOM] [--clean] [--black] [--once]");
                println!();
                println!("  ZOOM      zoom level, 5-80 (default 20)");
                println!("  --clean   hide background stars");
                println!("  --black   force a black background");
                println!("  --once    render a single frame and exit");
                println!();
                println!("press q, esc or ctrl+c to quit");
                return Ok(());
            }
            s => match s.parse::<f64>() {
                Ok(z) => zoom = z.clamp(5.0, 80.0),
                Err(_) => eprintln!("bh: ignoring unknown argument '{s}'"),
            },
        }
    }

    let opts = Opts {
        view_scale: BASE_ZOOM / zoom,
        clean,
        black,
    };

    if once {
        let (w, h) = terminal::size().unwrap_or((80, 24));
        return render(&mut io::stdout(), 0.1, w, h, opts);
    }

    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    let _guard = TermGuard;
    execute!(stdout, EnterAlternateScreen, Hide, Clear(ClearType::All))?;

    if black {
        execute!(stdout, SetBackgroundColor(Black))?;
    }

    let frame_dt = Duration::from_secs_f64(1.0 / 24.0);
    let mut time = 0.1;

    'main: loop {
        let t0 = Instant::now();
        let (w, h) = terminal::size()?;

        execute!(stdout, BeginSynchronizedUpdate)?;
        render(&mut stdout.lock(), time, w, h, opts)?;
        execute!(stdout, EndSynchronizedUpdate)?;

        while event::poll(Duration::ZERO)? {
            if let Event::Key(k) = event::read()?
                && (matches!(k.code, KeyCode::Char('q') | KeyCode::Esc)
                    || (k.code == KeyCode::Char('c')
                        && k.modifiers.contains(KeyModifiers::CONTROL)))
            {
                break 'main;
            }
        }

        let elapsed = t0.elapsed();
        if elapsed < frame_dt {
            std::thread::sleep(frame_dt - elapsed);
        }
        time += 1.0 / 24.0;
    }

    Ok(())
}
