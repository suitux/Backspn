use backspn_dsp::DelayLine;

const MAX_S: f64 = 0.2;
const DIFFUSION: f32 = 0.5;

#[derive(Debug, Clone, Copy, Default)]
pub struct Shape {
    pub main: [f32; 2],
    pub gain: [f32; 2],
    pub diffusers: [[f32; 2]; 3],
}

#[derive(Debug)]
pub struct Tank {
    main: DelayLine,
    diffusers: [DelayLine; 3],
}

impl Tank {
    pub fn new(rate: f64) -> Self {
        let len = (MAX_S * rate) as usize;
        Self {
            main: DelayLine::new(len),
            diffusers: std::array::from_fn(|_| DelayLine::new(len)),
        }
    }

    pub fn tick(&mut self, x: [f32; 2], shape: &Shape) -> [f32; 2] {
        let mut d = x;
        for (line, len) in self.diffusers.iter_mut().zip(shape.diffusers) {
            let delayed = [line.read(len[0])[0], line.read(len[1])[1]];
            let v = [d[0] + DIFFUSION * delayed[0], d[1] + DIFFUSION * delayed[1]];
            line.push(v);
            d = [delayed[0] - DIFFUSION * v[0], delayed[1] - DIFFUSION * v[1]];
        }
        let out = [
            self.main.read(shape.main[0])[0],
            self.main.read(shape.main[1])[1],
        ];
        let back = [out[0] * shape.gain[0], out[1] * shape.gain[1]];
        let mix = std::f32::consts::FRAC_1_SQRT_2;
        self.main.push([
            d[0] + mix * (back[0] + back[1]),
            d[1] + mix * (back[0] - back[1]),
        ]);
        out
    }

    pub fn clear(&mut self) {
        self.main.clear();
        for d in &mut self.diffusers {
            d.clear();
        }
    }
}
