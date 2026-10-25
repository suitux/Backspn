import { existsSync } from 'node:fs';
import { join } from 'node:path';

export interface Clip {
  file: string;
  record: string;
  /** Pixel size of a still screenshot, so the page reserves its space. */
  size?: [number, number];
}

export interface Video extends Clip {
  poster: string;
}

// Screenshots are copies of docs/images/ in the repo (the same ones the README shows).
export const features = {
  mixing: { file: 'screens/layout-parallel.webp', size: [2000, 1275], record: 'Four decks with their waveforms in parallel lanes.' },
  stems: { file: 'screens/stems.webp', size: [1125, 725], record: 'A stem deck with Drums, Bass, Other and Vocals lanes.' },
  effects: { file: 'screens/four-decks-light.webp', size: [2000, 1189], record: 'Four decks and four FX units (light theme).' },
  samples: { file: 'screens/sample-decks.webp', size: [2000, 723], record: 'A track deck next to a sample deck with four slots.' },
  library: { file: 'screens/layout-essential.webp', size: [2000, 1275], record: 'Four Essential decks with the library below.' },
  controllers: { file: 'screens/s4-screens.webp', size: [1456, 380], record: 'The four screen layouts of the Kontrol S4 MK3.' },
} satisfies Record<string, Clip>;

export const gallery = {
  drumMachine: { file: 'screens/drum-machine.webp', size: [1026, 295], record: 'An FX unit in Drum Machine mode.' },
  kits: { file: 'screens/drum-machine-kits.webp', size: [1960, 1516], record: 'Preferences, Drum Machine page.' },
  patterns: { file: 'screens/drum-machine-patterns.webp', size: [1580, 528], record: 'One pattern with three step sizes.' },
  loopRecorder: { file: 'screens/loop-recorder.webp', size: [1220, 1156], record: 'The loop recorder above a deck.' },
  saveAsLoop: { file: 'screens/save-as-loop.webp', size: [1220, 544], record: 'Save as Loop on an active deck loop.' },
  themes: { file: 'screens/themes.webp', size: [2000, 1316], record: 'Darcula, Solar, Purple and Ocean.' },
} satisfies Record<string, Clip>;

export const videos = {
  overview: { file: 'videos/overview.mp4', poster: 'videos/overview.jpg', record: '2–3 minute tour: load tracks, mix two with sync, an effect, stems and the library.' },
  set: { file: 'videos/set.mp4', poster: 'videos/set.jpg', record: 'A real 5–10 minute four-deck mix, screen-recorded with the audio.' },
  setup: { file: 'videos/setup.mp4', poster: 'videos/setup.jpg', record: 'First launch: pick the audio output, connect a controller and import the music.' },
} satisfies Record<string, Video>;

export const hero: Clip = {
  file: 'screens/two-decks.webp',
  size: [2000, 1189],
  record: 'The whole window with two decks loaded (hero).',
};

const root = join(process.cwd(), 'public', 'media');
export const ready = (file: string) => existsSync(join(root, file));
/** A file in public/, under the site's base path. */
export const asset = (file: string) => `${import.meta.env.BASE_URL.replace(/\/$/, '')}/${file}`;
export const url = (file: string) => asset(`media/${file}`);
export const anyReady = (clips: Record<string, Clip>) => Object.values(clips).some((c) => ready(c.file));
