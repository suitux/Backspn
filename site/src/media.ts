import { existsSync } from 'node:fs';
import { join } from 'node:path';

export interface Clip {
  file: string;
  record: string;
}

export interface Video extends Clip {
  poster: string;
}

export const features = {
  mixing: { file: 'features/mixing.gif', record: 'Two decks playing: SYNC on deck B jumps to the master tempo; move the tempo with keylock on and hear the pitch stay the same.' },
  stems: { file: 'features/stems.gif', record: 'Deck with stems: mute the vocals, filter the bass and bring them back.' },
  effects: { file: 'features/effects.gif', record: 'FX unit: pick Beat Chopper, raise dry/wet and change the beat size; then Back Spin.' },
  samples: { file: 'features/samples.gif', record: 'Sample deck and drum machine: launch two samples in time and switch on pattern steps.' },
  library: { file: 'features/library.gif', record: 'Library: search, sort by BPM, watch a new track being analysed and load it into a deck.' },
  controllers: { file: 'features/controllers.gif', record: 'Connected controller: move a fader and a jog and see it on screen; MIDI learn a button.' },
} satisfies Record<string, Clip>;

export const videos = {
  overview: { file: 'videos/overview.mp4', poster: 'videos/overview.jpg', record: '2–3 minute tour: load tracks, mix two with sync, an effect, stems and the library.' },
  set: { file: 'videos/set.mp4', poster: 'videos/set.jpg', record: 'A real 5–10 minute four-deck mix, screen-recorded with the audio.' },
  setup: { file: 'videos/setup.mp4', poster: 'videos/setup.jpg', record: 'First launch: pick the audio output, connect a controller and import the music.' },
} satisfies Record<string, Video>;

export const hero: Video = {
  file: 'videos/hero.mp4',
  poster: 'videos/hero.jpg',
  record: 'Silent 10–20 s loop of the whole window while two decks play (hero).',
};

const root = join(process.cwd(), 'public', 'media');
export const ready = (file: string) => existsSync(join(root, file));
export const url = (file: string) => `/media/${file}`;
