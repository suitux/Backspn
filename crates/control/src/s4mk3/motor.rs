use std::f32::consts::PI;

const RATE: f32 = 500.0;
const STEPS: f32 = 2880.0;
const TICK_S: f32 = 1e-8;
const BLOCK: usize = 28;

pub const JOG_REPORT: u8 = 0x03;
pub const TORQUE_REPORT: u8 = 0x31;
pub const COMMAND_REPORT: u8 = 0x30;
const COMMAND_LEN: usize = 27;

const DAMPING_MIN: f32 = -0.09;
const DAMPING_MAX: f32 = 1.1;
pub const DEFAULT_TENSION: u8 = 50;

pub fn damping_of(tension: u8) -> f32 {
    DAMPING_MIN + f32::from(tension) / 255.0 * (DAMPING_MAX - DAMPING_MIN)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Off,
    Jog,
    Turntable,
}

pub const RPM_33: f32 = 100.0 / 3.0;
pub const RPM_45: f32 = 45.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Side {
    pub mode: Mode,
    pub playing: bool,
    pub speed_rpm: f32,
    pub motor_off: u32,
}

impl Default for Side {
    fn default() -> Self {
        Self {
            mode: Mode::Off,
            playing: false,
            speed_rpm: RPM_33,
            motor_off: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub sides: [Side; 2],
    pub tension: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sides: [Side::default(); 2],
            tension: DEFAULT_TENSION,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct JogInput {
    pub mode: u8,
    pub touched: bool,
    pub timer: u32,
    pub angle: u16,
}

pub fn parse(report: &[u8]) -> Option<[JogInput; 2]> {
    let data = report.get(1..)?;
    let jog = |i: usize| {
        let b = data.get(i * BLOCK..(i + 1) * BLOCK)?;
        Some(JogInput {
            mode: b[3],
            touched: b[4] & 1 != 0,
            timer: u32::from_le_bytes([b[7], b[8], b[9], b[10]]),
            angle: u16::from_le_bytes([b[15], b[16]]),
        })
    };
    Some([jog(0)?, jog(1)?])
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Kinematics {
    velocity: f32,
    acceleration: f32,
    touched: bool,
}

fn wrap(mut x: f32, lo: f32) -> f32 {
    while x >= lo + 1.0 {
        x -= 1.0;
    }
    while x < lo {
        x += 1.0;
    }
    x
}

#[derive(Debug, Clone, Copy, Default)]
struct Tracker {
    last: Option<(u32, f32)>,
    motion: Kinematics,
}

impl Tracker {
    fn update(&mut self, input: JogInput) -> Kinematics {
        let angle = wrap(f32::from(input.angle) / STEPS, 0.0);
        let previous = self.motion;
        let (velocity, acceleration) = match self.last {
            Some((timer, before)) => {
                let dt = input.timer.wrapping_sub(timer) as f32 * TICK_S;
                if dt > 0.0 {
                    let v = wrap(angle - before, -0.5) / dt;
                    (v, (v - previous.velocity) / dt)
                } else {
                    (previous.velocity, 0.0)
                }
            }
            None => (0.0, 0.0),
        };
        self.last = Some((input.timer, angle));
        self.motion = Kinematics {
            velocity,
            acceleration,
            touched: input.touched,
        };
        self.motion
    }
}

#[derive(Debug, Clone, Copy)]
struct LowPass {
    a: [f32; 3],
    ic: [f32; 2],
}

impl LowPass {
    fn new(cutoff: f32) -> Self {
        let w = PI * cutoff / RATE;
        let g = w + w.powi(3) / 3.0 + 2.0 * w.powi(5) / 15.0;
        let k = std::f32::consts::SQRT_2;
        let a1 = 1.0 / (1.0 + g * (g + k));
        Self {
            a: [a1, g * a1, g * g * a1],
            ic: [0.0; 2],
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let [a1, a2, a3] = self.a;
        let [ic1, ic2] = self.ic;
        let v3 = x - ic2;
        let v1 = a1 * ic1 + a2 * v3;
        let v2 = ic2 + a2 * ic1 + a3 * v3;
        self.ic = [2.0 * v1 - ic1, 2.0 * v2 - ic2];
        v2
    }
}

const SMOOTHING: f32 = 0.02;
const INERTIA: f32 = 0.08;
const INERTIA_LIMIT: f32 = 0.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Spin {
    #[default]
    None,
    Back,
    Forward,
}

#[derive(Debug, Clone, Copy)]
struct JogWheel {
    velocity: LowPass,
    acceleration: LowPass,
    damping: f32,
    damping_target: f32,
    inertia: f32,
    spin: Spin,
    was_touched: bool,
}

impl JogWheel {
    fn new() -> Self {
        Self {
            velocity: LowPass::new(40.0),
            acceleration: LowPass::new(10.0),
            damping: 0.0,
            damping_target: 0.0,
            inertia: 0.0,
            spin: Spin::None,
            was_touched: false,
        }
    }

    fn torque(&mut self, k: Kinematics, playing: bool) -> f32 {
        let released = self.was_touched && !k.touched;
        self.was_touched = k.touched;
        if released && k.velocity < -2.5 {
            self.spin = Spin::Back;
        } else if released && k.velocity > 1.0 {
            self.spin = Spin::Forward;
        } else {
            let back_over = k.velocity > -0.15 || (k.touched && k.acceleration > 4.5);
            let forward_over = k.velocity < 0.001 || k.touched;
            if back_over && forward_over {
                self.spin = Spin::None;
            }
        }
        let velocity = self.velocity.process(k.velocity);
        let _ = playing;
        let base = 0.0;
        self.damping += (self.damping_target - self.damping) * SMOOTHING;
        let damping = (-self.damping * velocity).clamp(-1.0, 1.0);
        let acceleration = self.acceleration.process(k.acceleration);
        self.inertia += (INERTIA - self.inertia) * SMOOTHING;
        let inertia = (-self.inertia * acceleration).clamp(-INERTIA_LIMIT, INERTIA_LIMIT);
        let torque = if self.spin == Spin::Back {
            inertia
        } else {
            damping + base
        };
        torque.clamp(-1.0, 1.0)
    }
}

const KP: f32 = 2.7;
const KI: f32 = 50.0;
const I_LIMIT: f32 = 0.05;
const KD: f32 = 0.007;
const KF: f32 = 0.086_396;
const FRICTION: f32 = 0.026_215;
const HELD_LIMIT: f32 = 0.2;

#[derive(Debug, Clone, Copy)]
struct Pid {
    kp: f32,
    kd: f32,
    limit: f32,
    previous: f32,
    integral: f32,
}

impl Pid {
    const fn new(limit: f32) -> Self {
        Self {
            kp: KP,
            kd: KD,
            limit,
            previous: 0.0,
            integral: 0.0,
        }
    }

    fn process(&mut self, target: f32, speed: f32) -> f32 {
        let dt = 1.0 / RATE;
        let e = target - speed;
        self.integral =
            (self.integral + KI * dt * 0.5 * (e + self.previous)).clamp(-I_LIMIT, I_LIMIT);
        let mut u = self.kp * e + self.integral + self.kd * (e - self.previous) / dt;
        if target != 0.0 {
            u += KF * target + FRICTION.copysign(target);
        }
        self.previous = e;
        u.clamp(-self.limit, self.limit)
    }
}

const RUN_DOWN_DECAY: f32 = 0.0003;
const DRAG_FASTER: f32 = 0.025;
const DRAG_SLOWER: f32 = 0.01;
const DRAG_DEAD_BAND: f32 = 0.06;
const RUN_DOWN_END: f32 = 0.03;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PlatterState {
    #[default]
    Driven,
    Backspin,
    RunDown,
}

#[derive(Debug, Clone, Copy)]
struct Platter {
    playing: bool,
    target: f32,
    velocity: LowPass,
    acceleration: LowPass,
    free: Pid,
    held: Pid,
    inertia: f32,
    virtual_speed: f32,
    follow: Pid,
    state: PlatterState,
    was_touched: bool,
}

impl Platter {
    fn new() -> Self {
        Self {
            playing: false,
            target: RPM_33 / 60.0,
            velocity: LowPass::new(40.0),
            acceleration: LowPass::new(10.0),
            free: Pid::new(1.0),
            held: Pid::new(HELD_LIMIT),
            inertia: 0.0,
            virtual_speed: 0.0,
            follow: Pid::new(1.0),
            state: PlatterState::Driven,
            was_touched: false,
        }
    }

    fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        self.state = PlatterState::Driven;
    }

    fn motor_off(&mut self) {
        self.state = PlatterState::RunDown;
        self.virtual_speed = self.target;
    }

    fn torque(&mut self, k: Kinematics) -> f32 {
        let released = self.was_touched && !k.touched;
        self.was_touched = k.touched;
        let velocity = self.velocity.process(k.velocity);
        let target = if self.playing { self.target } else { 0.0 };
        let free = self.free.process(target, velocity);
        let held = self.held.process(target, velocity);
        let base = match (self.playing, k.touched) {
            (true, false) => free,
            (true, true) => held,
            (false, _) => 0.0,
        };
        let acceleration = self.acceleration.process(k.acceleration);
        self.inertia += (INERTIA - self.inertia) * SMOOTHING;
        let inertia = (-self.inertia * acceleration).clamp(-INERTIA_LIMIT, INERTIA_LIMIT);
        let run_down = self.run_down(velocity);
        if released && k.velocity < -2.5 {
            self.state = PlatterState::Backspin;
        }
        let torque = match self.state {
            PlatterState::Driven => base,
            PlatterState::Backspin => {
                if k.velocity > -0.15 || (k.touched && k.acceleration > 4.5) {
                    self.state = PlatterState::Driven;
                }
                inertia
            }
            PlatterState::RunDown => {
                if k.touched || k.velocity < -0.002 || self.virtual_speed < RUN_DOWN_END {
                    self.state = PlatterState::Driven;
                }
                run_down
            }
        };
        torque.clamp(-1.0, 1.0)
    }

    fn run_down(&mut self, velocity: f32) -> f32 {
        let w = self.virtual_speed;
        let a = (2.0 * w.abs()).min(1.0);
        let scale = (a + 0.3).min(1.0);
        self.follow.kp = KP * scale;
        self.follow.kd = KD * scale;
        let mut w = w * (1.0 - RUN_DOWN_DECAY);
        w *= 1.0 - 8.0 * RUN_DOWN_DECAY * (1.0 - (12.0 * a).min(1.0));
        let e = velocity - w;
        if e.abs() > DRAG_DEAD_BAND {
            w += e * if e > 0.0 { DRAG_FASTER } else { DRAG_SLOWER };
        }
        self.virtual_speed = w;
        self.follow.process(w, velocity).clamp(-1.0, 1.0)
    }
}

#[derive(Debug, Clone)]
pub struct Motors {
    settings: Settings,
    engaged: bool,
    trackers: [Tracker; 2],
    wheels: [JogWheel; 2],
    platters: [Platter; 2],
}

impl Default for Motors {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            engaged: false,
            trackers: [Tracker::default(); 2],
            wheels: [JogWheel::new(); 2],
            platters: [Platter::new(); 2],
        }
    }
}

impl Motors {
    pub fn set(&mut self, settings: Settings) {
        let target = damping_of(settings.tension);
        for ((wheel, platter), (old, new)) in self
            .wheels
            .iter_mut()
            .zip(&mut self.platters)
            .zip(self.settings.sides.iter().zip(settings.sides))
        {
            wheel.damping_target = target;
            platter.target = new.speed_rpm / 60.0;
            if old.playing != new.playing {
                wheel.spin = Spin::None;
                platter.set_playing(new.playing);
            }
            if new.motor_off != old.motor_off {
                platter.motor_off();
            }
        }
        self.engaged |= settings.sides.iter().any(|s| s.mode != Mode::Off);
        self.settings = settings;
    }

    pub fn release(&self) -> Option<[u8; 11]> {
        self.engaged.then(|| torque_report([0.0; 2]))
    }

    fn engaged(&self) -> bool {
        self.engaged
    }

    pub fn step(&mut self, report: &[u8]) -> Vec<Vec<u8>> {
        let Some(inputs) = parse(report).filter(|_| self.engaged()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (i, input) in inputs.iter().enumerate() {
            if input.mode == 0 {
                out.push(command(i as u8));
            }
        }
        let mut torques = [0.0; 2];
        for (i, input) in inputs.iter().enumerate() {
            let motion = self.trackers[i].update(*input);
            let side = self.settings.sides[i];
            let jog = self.wheels[i].torque(motion, side.playing);
            let platter = self.platters[i].torque(motion);
            torques[i] = match side.mode {
                Mode::Off => 0.0,
                Mode::Jog => jog,
                Mode::Turntable => platter,
            };
        }
        if inputs.iter().all(|i| i.mode <= 3) {
            out.push(torque_report(torques).to_vec());
        }
        out
    }
}

pub fn command(jog: u8) -> Vec<u8> {
    let mut r = vec![0u8; COMMAND_LEN + 1];
    r[0] = COMMAND_REPORT;
    r[1] = jog;
    r[2] = 1;
    r[3] = 3;
    r
}

pub fn torque_report(torques: [f32; 2]) -> [u8; 11] {
    let mut r = [0u8; 11];
    r[0] = TORQUE_REPORT;
    for (i, t) in torques.into_iter().enumerate() {
        let t = t.clamp(-1.0, 1.0);
        let at = 1 + i * 5;
        let direction: i16 = if t > 0.0 { 288 } else { -288 };
        let magnitude = (t.abs() * 60000.0) as u16;
        r[at] = 1;
        r[at + 1..at + 3].copy_from_slice(&direction.to_le_bytes());
        r[at + 3..at + 5].copy_from_slice(&magnitude.to_le_bytes());
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(jogs: [JogInput; 2]) -> Vec<u8> {
        let mut r = vec![0u8; 1 + 2 * BLOCK];
        r[0] = 0x03;
        for (i, j) in jogs.iter().enumerate() {
            let b = 1 + i * BLOCK;
            r[b + 3] = j.mode;
            r[b + 4] = u8::from(j.touched);
            r[b + 7..b + 11].copy_from_slice(&j.timer.to_le_bytes());
            r[b + 15..b + 17].copy_from_slice(&j.angle.to_le_bytes());
        }
        r
    }

    #[test]
    fn tension_maps_to_damping() {
        assert!((damping_of(0) + 0.09).abs() < 1e-6);
        assert!((damping_of(255) - 1.1).abs() < 1e-6);
        assert!((damping_of(50) - 0.1433).abs() < 1e-3);
    }

    #[test]
    fn torque_report_layout() {
        let r = torque_report([0.5, -0.25]);
        assert_eq!(r[0], 0x31);
        assert_eq!(r[1], 1);
        assert_eq!(i16::from_le_bytes([r[2], r[3]]), 288);
        assert_eq!(u16::from_le_bytes([r[4], r[5]]), 30000);
        assert_eq!(r[6], 1);
        assert_eq!(i16::from_le_bytes([r[7], r[8]]), -288);
        assert_eq!(u16::from_le_bytes([r[9], r[10]]), 15000);
    }

    #[test]
    fn velocity_is_turns_per_second_across_the_wrap() {
        let mut t = Tracker::default();
        let at = |timer: u32, angle: u16| JogInput {
            mode: 3,
            timer,
            angle,
            ..JogInput::default()
        };
        t.update(at(0, 2870));
        let k = t.update(at(200_000, 10));
        assert!((k.velocity - 20.0 / 2880.0 / 0.002).abs() < 1e-3, "{k:?}");
    }

    #[test]
    fn nothing_is_sent_before_a_wheel_gets_a_mode() {
        let mut m = Motors::default();
        assert_eq!(
            m.step(&report([JogInput::default(); 2])),
            Vec::<Vec<u8>>::new()
        );
        assert!(m.release().is_none());
    }

    #[test]
    fn an_uncontrolled_wheel_gets_the_command_and_no_torque_report() {
        let mut m = Motors::default();
        let mut s = Settings::default();
        s.sides[0].mode = Mode::Jog;
        m.set(s);
        let out = m.step(&report([JogInput::default(); 2]));
        assert_eq!(out.len(), 3, "two commands and the torques");
        assert_eq!(&out[0][..4], &[0x30, 0, 1, 3]);
        assert_eq!(&out[1][..4], &[0x30, 1, 1, 3]);
        let busy = JogInput {
            mode: 9,
            ..JogInput::default()
        };
        assert_eq!(
            m.step(&report([busy; 2])),
            Vec::<Vec<u8>>::new(),
            "mode above 3"
        );
    }

    fn steady(tension: u8, rps: f32, mode: Mode) -> f32 {
        let mut m = Motors::default();
        m.set(Settings {
            sides: [
                Side {
                    mode,
                    ..Side::default()
                },
                Side {
                    mode: Mode::Jog,
                    ..Side::default()
                },
            ],
            tension,
        });
        let mut torque = 0.0;
        for n in 0..500u32 {
            let angle = (n as f32 * rps * 0.002 * STEPS) as u32 % 2880;
            let jog = JogInput {
                mode: 3,
                touched: true,
                timer: n * 200_000,
                angle: angle as u16,
            };
            let out = m.step(&report([jog, JogInput { mode: 3, ..jog }]));
            let r = out.last().unwrap();
            let sign = if i16::from_le_bytes([r[2], r[3]]) > 0 {
                1.0
            } else {
                -1.0
            };
            torque = sign * f32::from(u16::from_le_bytes([r[4], r[5]])) / 60000.0;
        }
        torque
    }

    #[test]
    fn tension_brakes_a_turning_wheel() {
        let t = steady(DEFAULT_TENSION, 0.5, Mode::Jog);
        assert!((t + damping_of(DEFAULT_TENSION) * 0.5).abs() < 0.02, "{t}");
        assert!(steady(255, 0.5, Mode::Jog) < t, "more tension, more brake");
        assert!(steady(0, 0.5, Mode::Jog) > 0.0, "below zero it pushes");
        assert!(steady(255, 0.5, Mode::Off).abs() < 1e-6, "off: free wheel");
    }

    #[test]
    fn a_backspin_is_carried_by_inertia() {
        let mut w = JogWheel::new();
        w.damping_target = damping_of(DEFAULT_TENSION);
        let touched = Kinematics {
            velocity: -4.0,
            acceleration: 0.0,
            touched: true,
        };
        w.torque(touched, true);
        let slowing = Kinematics {
            velocity: -3.5,
            acceleration: 20.0,
            touched: false,
        };
        w.torque(slowing, true);
        assert_eq!(w.spin, Spin::Back);
        let mut torque = 0.0;
        for _ in 0..100 {
            torque = w.torque(slowing, true);
        }
        assert!(
            torque < 0.0,
            "pushes backwards against the slowdown: {torque}"
        );
        assert!(torque >= -INERTIA_LIMIT);
        let stopped = Kinematics {
            velocity: -0.05,
            ..slowing
        };
        w.torque(stopped, true);
        assert_eq!(w.spin, Spin::None);
    }

    fn platter_at(p: &mut Platter, rps: f32, touched: bool) -> f32 {
        let k = Kinematics {
            velocity: rps,
            acceleration: 0.0,
            touched,
        };
        (0..500).fold(0.0, |_, _| p.torque(k))
    }

    #[test]
    fn a_playing_platter_is_driven_to_record_speed() {
        let mut p = Platter::new();
        p.set_playing(true);
        let target = RPM_33 / 60.0;
        assert!(platter_at(&mut p, 0.0, false) > 0.9, "spins up hard");
        let mut p = Platter::new();
        p.set_playing(true);
        assert!(platter_at(&mut p, target * 1.2, false) < 0.0, "brakes");
        let mut p = Platter::new();
        p.set_playing(true);
        let t = platter_at(&mut p, target, false);
        let feed_forward = KF * target + FRICTION;
        assert!(
            (feed_forward..=feed_forward + I_LIMIT + 1e-6).contains(&t),
            "{t}"
        );
    }

    #[test]
    fn a_held_platter_is_pushed_gently_and_a_stopped_deck_leaves_it_free() {
        let mut p = Platter::new();
        p.set_playing(true);
        let t = platter_at(&mut p, 0.0, true);
        assert!((t - HELD_LIMIT).abs() < 1e-6, "{t}");
        let mut p = Platter::new();
        assert!(platter_at(&mut p, 0.4, false).abs() < 1e-6);
        assert!(
            (steady(DEFAULT_TENSION, 0.5, Mode::Turntable)).abs() < 1e-6,
            "no torque in turntable mode with the deck stopped"
        );
    }

    #[test]
    fn a_backspin_on_the_platter_is_carried_by_inertia() {
        let mut p = Platter::new();
        p.set_playing(true);
        platter_at(&mut p, -3.0, true);
        let slowing = Kinematics {
            velocity: -3.0,
            acceleration: 20.0,
            touched: false,
        };
        p.torque(slowing);
        assert_eq!(p.state, PlatterState::Backspin);
        let t = (0..100).fold(0.0, |_, _| p.torque(slowing));
        assert!((-INERTIA_LIMIT..0.0).contains(&t), "{t}");
        p.torque(Kinematics {
            velocity: 0.0,
            ..slowing
        });
        assert_eq!(p.state, PlatterState::Driven);
    }

    #[test]
    fn motor_off_runs_the_platter_down_over_seconds() {
        let mut p = Platter::new();
        p.set_playing(true);
        let target = RPM_33 / 60.0;
        platter_at(&mut p, target, false);
        p.set_playing(false);
        p.motor_off();
        let mut steps = 0u32;
        while p.state == PlatterState::RunDown {
            let w = p.virtual_speed;
            p.torque(Kinematics {
                velocity: w,
                acceleration: 0.0,
                touched: false,
            });
            steps += 1;
            assert!(steps < 30 * 500, "never ends");
        }
        let seconds = steps as f32 / RATE;
        assert!((10.0..25.0).contains(&seconds), "{seconds} s");
        p.motor_off();
        p.torque(Kinematics {
            velocity: target,
            acceleration: 0.0,
            touched: true,
        });
        assert_eq!(p.state, PlatterState::Driven);
    }
}
